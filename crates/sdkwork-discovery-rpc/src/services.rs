use sdkwork_discovery_contract::{
    CallerContext, DiscoveryError, DiscoveryEvent, DiscoveryEventKind, RetrieveInstanceQuery,
    WatchEventsQuery,
};
use sdkwork_discovery_rpc_proto::sdkwork::discovery::backend::v3 as backend_proto;
use sdkwork_discovery_rpc_proto::sdkwork::discovery::common::v1 as common_proto;
use sdkwork_discovery_rpc_proto::sdkwork::discovery::internal::v1 as internal_proto;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, OwnedSemaphorePermit, Semaphore};
use tokio::time::MissedTickBehavior;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use tracing::{debug, info, instrument, warn};

use crate::actor::DiscoveryRpcRuntime;
use crate::codec;
use crate::context::{
    caller_from_metadata, caller_from_metadata_with_required_idempotency,
    config_reader_from_metadata, idempotency_from_metadata, registry_reader_from_metadata,
    request_id_from_metadata, trace_id_from_metadata,
};
use crate::error::{
    attach_rpc_correlation_metadata, grpc_status_code_for_discovery_error,
    map_discovery_error_to_rpc_status, map_discovery_error_to_status,
};
use crate::metrics::{
    decrement_active_streams, increment_active_streams, record_auth_failure, record_cancellation,
    RpcMetrics, RpcMetricsGuard,
};
use crate::watch::{event_matches_config_watch, event_matches_service_watch, WatchEventSubscriber};

fn record_guard_rpc_error(metrics: &mut RpcMetricsGuard, error: &DiscoveryError) {
    metrics.record_error(
        grpc_status_code_for_discovery_error(error),
        error.kind_string(),
    );
}

fn map_guard_rpc_err(
    metrics: &mut RpcMetricsGuard,
    error: DiscoveryError,
    map_rpc_err: impl Fn(DiscoveryError) -> Status,
) -> Status {
    record_guard_rpc_error(metrics, &error);
    map_rpc_err(error)
}

#[derive(Clone)]
pub struct RegistryRpcService<S> {
    runtime: DiscoveryRpcRuntime<S>,
}

#[derive(Clone)]
pub struct DiscoveryConfigRpcService<S> {
    runtime: DiscoveryRpcRuntime<S>,
    config: DiscoveryWatchServiceConfig,
    limiter: Arc<Semaphore>,
}

#[derive(Clone)]
pub struct DiscoveryAdminRpcService<S> {
    runtime: DiscoveryRpcRuntime<S>,
}

#[derive(Clone)]
pub struct DiscoveryWatchRpcService<S> {
    runtime: DiscoveryRpcRuntime<S>,
    config: DiscoveryWatchServiceConfig,
    limiter: Arc<Semaphore>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DiscoveryWatchServiceConfig {
    pub enabled: bool,
    pub max_streams: u32,
    pub event_buffer_size: usize,
    pub heartbeat_interval_ms: u64,
    pub durable_poll_interval_ms: u64,
    pub durable_replay_batch_size: usize,
}

impl Default for DiscoveryWatchServiceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_streams: 10_000,
            event_buffer_size: 1_024,
            heartbeat_interval_ms: 15_000,
            durable_poll_interval_ms: 1_000,
            durable_replay_batch_size: 1_000,
        }
    }
}

impl<S> RegistryRpcService<S> {
    pub fn new(runtime: DiscoveryRpcRuntime<S>) -> Self {
        Self { runtime }
    }
}

impl<S> DiscoveryConfigRpcService<S> {
    pub fn new(runtime: DiscoveryRpcRuntime<S>) -> Self {
        Self::with_watch_config(runtime, DiscoveryWatchServiceConfig::default())
    }

    pub(crate) fn with_watch_config(
        runtime: DiscoveryRpcRuntime<S>,
        config: DiscoveryWatchServiceConfig,
    ) -> Self {
        Self::with_watch_limiter(runtime, config, None)
    }

    pub(crate) fn with_watch_limiter(
        runtime: DiscoveryRpcRuntime<S>,
        config: DiscoveryWatchServiceConfig,
        limiter: Option<Arc<Semaphore>>,
    ) -> Self {
        let limiter =
            limiter.unwrap_or_else(|| Arc::new(Semaphore::new(config.max_streams as usize)));
        Self {
            runtime,
            limiter,
            config,
        }
    }

    fn acquire_stream_permit(&self) -> Result<OwnedSemaphorePermit, Status> {
        self.limiter
            .clone()
            .try_acquire_owned()
            .map_err(|_| Status::resource_exhausted("watch stream limit exceeded"))
    }
}

impl<S> DiscoveryAdminRpcService<S> {
    pub fn new(runtime: DiscoveryRpcRuntime<S>) -> Self {
        Self { runtime }
    }
}

impl<S> DiscoveryWatchRpcService<S> {
    pub fn new(runtime: DiscoveryRpcRuntime<S>) -> Self {
        Self::with_config(runtime, DiscoveryWatchServiceConfig::default())
    }

    pub(crate) fn with_config(
        runtime: DiscoveryRpcRuntime<S>,
        config: DiscoveryWatchServiceConfig,
    ) -> Self {
        Self::with_limiter(runtime, config, None)
    }

    pub(crate) fn with_limiter(
        runtime: DiscoveryRpcRuntime<S>,
        config: DiscoveryWatchServiceConfig,
        limiter: Option<Arc<Semaphore>>,
    ) -> Self {
        let limiter =
            limiter.unwrap_or_else(|| Arc::new(Semaphore::new(config.max_streams as usize)));
        Self {
            runtime,
            limiter,
            config,
        }
    }

    fn acquire_stream_permit(&self) -> Result<OwnedSemaphorePermit, Status> {
        self.limiter
            .clone()
            .try_acquire_owned()
            .map_err(|_| Status::resource_exhausted("watch stream limit exceeded"))
    }
}

#[tonic::async_trait]
impl<S> internal_proto::registry_service_server::RegistryService for RegistryRpcService<S>
where
    S: Send + Sync + 'static,
{
    #[instrument(
        skip(self, request),
        fields(
            package = "sdkwork.discovery.internal.v1",
            service = "RegistryService",
            method = "RegisterInstance",
            operation_id = "discovery.registry.instances.register",
            api_surface = "rpc"
        )
    )]
    async fn register_instance(
        &self,
        request: Request<internal_proto::RegisterInstanceRequest>,
    ) -> Result<Response<internal_proto::RegisterInstanceResponse>, Status> {
        let request_id =
            request_id_from_metadata(request.metadata()).map_err(map_discovery_error_to_status)?;
        let trace_id = trace_id_from_metadata(request.metadata())
            .map_err(|error| map_discovery_error_to_rpc_status(error, &request_id, ""))?;
        let map_rpc_err = |error: sdkwork_discovery_contract::DiscoveryError| {
            map_discovery_error_to_rpc_status(error, &request_id, &trace_id)
        };
        let mut metrics = RpcMetricsGuard::new(
            "sdkwork.discovery.internal.v1",
            "RegistryService",
            "RegisterInstance",
            "discovery.registry.instances.register",
        );
        let caller = caller_from_metadata(request.metadata(), self.runtime.context_policy())
            .map_err(|e| {
                record_auth_failure(
                    "sdkwork.discovery.internal.v1",
                    "RegistryService",
                    "RegisterInstance",
                );
                map_guard_rpc_err(&mut metrics, e, &map_rpc_err)
            })?;
        let command = codec::register_instance_command(request.into_inner(), codec::now_millis())
            .map_err(|error| map_guard_rpc_err(&mut metrics, error, &map_rpc_err))?;
        debug!(
            request_id = %request_id,
            trace_id = %trace_id,
            subject_id = %caller.subject_id,
            package = "sdkwork.discovery.internal.v1",
            service = "RegistryService",
            method = "RegisterInstance",
            operation_id = "discovery.registry.instances.register",
            "registering instance"
        );
        match self.runtime.register_instance(caller, command).await {
            Ok(result) => {
                info!(
                    request_id = %request_id,
                    trace_id = %trace_id,
                    package = "sdkwork.discovery.internal.v1",
                    service = "RegistryService",
                    method = "RegisterInstance",
                    operation_id = "discovery.registry.instances.register",
                    lease_id = %result.lease_id,
                    status = "OK",
                    "instance registered"
                );
                metrics.record_success("OK");
                Ok(Response::new(codec::register_instance_response(
                    result, request_id, trace_id,
                )))
            }
            Err(error) => {
                warn!(
                    request_id = %request_id,
                    trace_id = %trace_id,
                    package = "sdkwork.discovery.internal.v1",
                    service = "RegistryService",
                    method = "RegisterInstance",
                    operation_id = "discovery.registry.instances.register",
                    error = %error,
                    status = grpc_status_code_for_discovery_error(&error),
                    "instance registration failed"
                );
                metrics.record_error(
                    grpc_status_code_for_discovery_error(&error),
                    error.kind_string(),
                );
                Err(map_rpc_err(error))
            }
        }
    }

    #[instrument(
        skip(self, request),
        fields(
            package = "sdkwork.discovery.internal.v1",
            service = "RegistryService",
            method = "BatchRegisterInstances",
            operation_id = "discovery.registry.instances.batch_register",
            api_surface = "rpc"
        )
    )]
    async fn batch_register_instances(
        &self,
        request: Request<internal_proto::BatchRegisterInstancesRequest>,
    ) -> Result<Response<internal_proto::BatchRegisterInstancesResponse>, Status> {
        let request_id =
            request_id_from_metadata(request.metadata()).map_err(map_discovery_error_to_status)?;
        let trace_id = trace_id_from_metadata(request.metadata())
            .map_err(|error| map_discovery_error_to_rpc_status(error, &request_id, ""))?;
        let map_rpc_err = |error: sdkwork_discovery_contract::DiscoveryError| {
            map_discovery_error_to_rpc_status(error, &request_id, &trace_id)
        };
        let mut metrics = RpcMetricsGuard::new(
            "sdkwork.discovery.internal.v1",
            "RegistryService",
            "BatchRegisterInstances",
            "discovery.registry.instances.batch_register",
        );
        let caller = caller_from_metadata(request.metadata(), self.runtime.context_policy())
            .map_err(|e| {
                record_auth_failure(
                    "sdkwork.discovery.internal.v1",
                    "RegistryService",
                    "BatchRegisterInstances",
                );
                map_guard_rpc_err(&mut metrics, e, &map_rpc_err)
            })?;
        let commands =
            codec::batch_register_instances_commands(request.into_inner(), codec::now_millis())
                .map_err(|error| map_guard_rpc_err(&mut metrics, error, &map_rpc_err))?;
        match self
            .runtime
            .batch_register_instances(caller, commands)
            .await
        {
            Ok(result) => {
                metrics.record_success("OK");
                Ok(Response::new(codec::batch_register_instances_response(
                    result, request_id, trace_id,
                )))
            }
            Err(error) => {
                metrics.record_error(
                    grpc_status_code_for_discovery_error(&error),
                    error.kind_string(),
                );
                Err(map_rpc_err(error))
            }
        }
    }

    #[instrument(
        skip(self, request),
        fields(
            package = "sdkwork.discovery.internal.v1",
            service = "RegistryService",
            method = "RenewLease",
            operation_id = "discovery.registry.leases.renew",
            api_surface = "rpc"
        )
    )]
    async fn renew_lease(
        &self,
        request: Request<internal_proto::RenewLeaseRequest>,
    ) -> Result<Response<internal_proto::RenewLeaseResponse>, Status> {
        let request_id =
            request_id_from_metadata(request.metadata()).map_err(map_discovery_error_to_status)?;
        let trace_id = trace_id_from_metadata(request.metadata())
            .map_err(|error| map_discovery_error_to_rpc_status(error, &request_id, ""))?;
        let map_rpc_err = |error: sdkwork_discovery_contract::DiscoveryError| {
            map_discovery_error_to_rpc_status(error, &request_id, &trace_id)
        };
        let mut metrics = RpcMetricsGuard::new(
            "sdkwork.discovery.internal.v1",
            "RegistryService",
            "RenewLease",
            "discovery.registry.leases.renew",
        );
        let caller = caller_from_metadata(request.metadata(), self.runtime.context_policy())
            .map_err(|e| {
                record_auth_failure(
                    "sdkwork.discovery.internal.v1",
                    "RegistryService",
                    "RenewLease",
                );
                map_guard_rpc_err(&mut metrics, e, &map_rpc_err)
            })?;
        let command = codec::renew_lease_command(request.into_inner(), codec::now_millis())
            .map_err(|error| map_guard_rpc_err(&mut metrics, error, &map_rpc_err))?;
        debug!(
            request_id = %request_id,
            trace_id = %trace_id,
            subject_id = %caller.subject_id,
            package = "sdkwork.discovery.internal.v1",
            service = "RegistryService",
            method = "RenewLease",
            operation_id = "discovery.registry.leases.renew",
            "renewing lease"
        );
        match self.runtime.renew_lease(caller, command).await {
            Ok(result) => {
                info!(
                    request_id = %request_id,
                    trace_id = %trace_id,
                    package = "sdkwork.discovery.internal.v1",
                    service = "RegistryService",
                    method = "RenewLease",
                    operation_id = "discovery.registry.leases.renew",
                    lease_id = %result.lease_id,
                    status = "OK",
                    "lease renewed"
                );
                metrics.record_success("OK");
                Ok(Response::new(codec::renew_lease_response(
                    result, request_id, trace_id,
                )))
            }
            Err(error) => {
                warn!(
                    request_id = %request_id,
                    trace_id = %trace_id,
                    package = "sdkwork.discovery.internal.v1",
                    service = "RegistryService",
                    method = "RenewLease",
                    operation_id = "discovery.registry.leases.renew",
                    error = %error,
                    status = grpc_status_code_for_discovery_error(&error),
                    "lease renewal failed"
                );
                metrics.record_error(
                    grpc_status_code_for_discovery_error(&error),
                    error.kind_string(),
                );
                Err(map_rpc_err(error))
            }
        }
    }

    #[instrument(
        skip(self, request),
        fields(
            package = "sdkwork.discovery.internal.v1",
            service = "RegistryService",
            method = "DeregisterInstance",
            operation_id = "discovery.registry.instances.deregister",
            api_surface = "rpc"
        )
    )]
    async fn deregister_instance(
        &self,
        request: Request<internal_proto::DeregisterInstanceRequest>,
    ) -> Result<Response<internal_proto::DeregisterInstanceResponse>, Status> {
        let request_id =
            request_id_from_metadata(request.metadata()).map_err(map_discovery_error_to_status)?;
        let trace_id = trace_id_from_metadata(request.metadata())
            .map_err(|error| map_discovery_error_to_rpc_status(error, &request_id, ""))?;
        let map_rpc_err = |error: sdkwork_discovery_contract::DiscoveryError| {
            map_discovery_error_to_rpc_status(error, &request_id, &trace_id)
        };
        let mut metrics = RpcMetricsGuard::new(
            "sdkwork.discovery.internal.v1",
            "RegistryService",
            "DeregisterInstance",
            "discovery.registry.instances.deregister",
        );
        let caller = caller_from_metadata(request.metadata(), self.runtime.context_policy())
            .map_err(|e| {
                record_auth_failure(
                    "sdkwork.discovery.internal.v1",
                    "RegistryService",
                    "DeregisterInstance",
                );
                map_guard_rpc_err(&mut metrics, e, &map_rpc_err)
            })?;
        let request =
            codec::deregister_instance_request(request.into_inner()).map_err(|error| map_guard_rpc_err(&mut metrics, error, &map_rpc_err))?;
        debug!(
            request_id = %request_id,
            trace_id = %trace_id,
            subject_id = %caller.subject_id,
            package = "sdkwork.discovery.internal.v1",
            service = "RegistryService",
            method = "DeregisterInstance",
            operation_id = "discovery.registry.instances.deregister",
            instance_id = %request.instance_id,
            "deregistering instance"
        );
        match self
            .runtime
            .deregister_instance(
                caller,
                request.namespace,
                request.environment,
                request.service_name,
                request.instance_id,
                codec::now_millis(),
            )
            .await
        {
            Ok(result) => {
                info!(
                    request_id = %request_id,
                    trace_id = %trace_id,
                    package = "sdkwork.discovery.internal.v1",
                    service = "RegistryService",
                    method = "DeregisterInstance",
                    operation_id = "discovery.registry.instances.deregister",
                    status = "OK",
                    "instance deregistered"
                );
                metrics.record_success("OK");
                Ok(Response::new(internal_proto::DeregisterInstanceResponse {
                    metadata: Some(codec::response_metadata(
                        result.revision,
                        request_id,
                        trace_id,
                    )),
                }))
            }
            Err(error) => {
                warn!(
                    request_id = %request_id,
                    trace_id = %trace_id,
                    package = "sdkwork.discovery.internal.v1",
                    service = "RegistryService",
                    method = "DeregisterInstance",
                    operation_id = "discovery.registry.instances.deregister",
                    error = %error,
                    status = grpc_status_code_for_discovery_error(&error),
                    "instance deregistration failed"
                );
                metrics.record_error(
                    grpc_status_code_for_discovery_error(&error),
                    error.kind_string(),
                );
                Err(map_rpc_err(error))
            }
        }
    }

    #[instrument(
        skip(self, request),
        fields(
            package = "sdkwork.discovery.internal.v1",
            service = "RegistryService",
            method = "ReportInstanceStatus",
            operation_id = "discovery.registry.instances.status.report",
            api_surface = "rpc"
        )
    )]
    async fn report_instance_status(
        &self,
        request: Request<internal_proto::ReportInstanceStatusRequest>,
    ) -> Result<Response<internal_proto::ReportInstanceStatusResponse>, Status> {
        let request_id =
            request_id_from_metadata(request.metadata()).map_err(map_discovery_error_to_status)?;
        let trace_id = trace_id_from_metadata(request.metadata())
            .map_err(|error| map_discovery_error_to_rpc_status(error, &request_id, ""))?;
        let map_rpc_err = |error: sdkwork_discovery_contract::DiscoveryError| {
            map_discovery_error_to_rpc_status(error, &request_id, &trace_id)
        };
        let mut metrics = RpcMetricsGuard::new(
            "sdkwork.discovery.internal.v1",
            "RegistryService",
            "ReportInstanceStatus",
            "discovery.registry.instances.status.report",
        );
        let caller = caller_from_metadata(request.metadata(), self.runtime.context_policy())
            .map_err(|e| {
                record_auth_failure(
                    "sdkwork.discovery.internal.v1",
                    "RegistryService",
                    "ReportInstanceStatus",
                );
                map_guard_rpc_err(&mut metrics, e, &map_rpc_err)
            })?;
        let command = codec::report_status_command(request.into_inner(), codec::now_millis())
            .map_err(|error| map_guard_rpc_err(&mut metrics, error, &map_rpc_err))?;
        debug!(
            request_id = %request_id,
            trace_id = %trace_id,
            subject_id = %caller.subject_id,
            package = "sdkwork.discovery.internal.v1",
            service = "RegistryService",
            method = "ReportInstanceStatus",
            operation_id = "discovery.registry.instances.status.report",
            "reporting instance status"
        );
        match self.runtime.report_instance_status(caller, command).await {
            Ok(result) => {
                info!(
                    request_id = %request_id,
                    trace_id = %trace_id,
                    package = "sdkwork.discovery.internal.v1",
                    service = "RegistryService",
                    method = "ReportInstanceStatus",
                    operation_id = "discovery.registry.instances.status.report",
                    status = "OK",
                    "instance status reported"
                );
                metrics.record_success("OK");
                Ok(Response::new(codec::report_status_response(
                    result, request_id, trace_id,
                )))
            }
            Err(error) => {
                warn!(
                    request_id = %request_id,
                    trace_id = %trace_id,
                    package = "sdkwork.discovery.internal.v1",
                    service = "RegistryService",
                    method = "ReportInstanceStatus",
                    operation_id = "discovery.registry.instances.status.report",
                    error = %error,
                    status = grpc_status_code_for_discovery_error(&error),
                    "instance status report failed"
                );
                metrics.record_error(
                    grpc_status_code_for_discovery_error(&error),
                    error.kind_string(),
                );
                Err(map_rpc_err(error))
            }
        }
    }

    #[instrument(
        skip(self, request),
        fields(
            package = "sdkwork.discovery.internal.v1",
            service = "RegistryService",
            method = "RetrieveInstance",
            operation_id = "discovery.registry.instances.retrieve",
            api_surface = "rpc"
        )
    )]
    async fn retrieve_instance(
        &self,
        request: Request<internal_proto::RetrieveInstanceRequest>,
    ) -> Result<Response<internal_proto::RetrieveInstanceResponse>, Status> {
        let request_id =
            request_id_from_metadata(request.metadata()).map_err(map_discovery_error_to_status)?;
        let trace_id = trace_id_from_metadata(request.metadata())
            .map_err(|error| map_discovery_error_to_rpc_status(error, &request_id, ""))?;
        let map_rpc_err = |error: sdkwork_discovery_contract::DiscoveryError| {
            map_discovery_error_to_rpc_status(error, &request_id, &trace_id)
        };
        let mut metrics = RpcMetricsGuard::new(
            "sdkwork.discovery.internal.v1",
            "RegistryService",
            "RetrieveInstance",
            "discovery.registry.instances.retrieve",
        );
        let caller =
            registry_reader_from_metadata(request.metadata(), self.runtime.context_policy())
                .map_err(|e| {
                    record_auth_failure(
                        "sdkwork.discovery.internal.v1",
                        "RegistryService",
                        "RetrieveInstance",
                    );
                    map_guard_rpc_err(&mut metrics, e, &map_rpc_err)
                })?;
        let query = codec::retrieve_instance_query(request.into_inner()).map_err(|error| map_guard_rpc_err(&mut metrics, error, &map_rpc_err))?;
        debug!(
            request_id = %request_id,
            trace_id = %trace_id,
            subject_id = %caller.subject_id,
            package = "sdkwork.discovery.internal.v1",
            service = "RegistryService",
            method = "RetrieveInstance",
            operation_id = "discovery.registry.instances.retrieve",
            "retrieving instance"
        );
        match self
            .runtime
            .retrieve_instance(caller, query, codec::now_millis())
            .await
        {
            Ok(Some(instance)) => {
                info!(
                    request_id = %request_id,
                    trace_id = %trace_id,
                    package = "sdkwork.discovery.internal.v1",
                    service = "RegistryService",
                    method = "RetrieveInstance",
                    operation_id = "discovery.registry.instances.retrieve",
                    instance_id = %instance.instance_id,
                    status = "OK",
                    "instance retrieved"
                );
                metrics.record_success("OK");
                Ok(Response::new(codec::retrieve_instance_response(
                    instance, request_id, trace_id,
                )))
            }
            Ok(None) => {
                warn!(
                    request_id = %request_id,
                    trace_id = %trace_id,
                    package = "sdkwork.discovery.internal.v1",
                    service = "RegistryService",
                    method = "RetrieveInstance",
                    operation_id = "discovery.registry.instances.retrieve",
                    status = "NOT_FOUND",
                    "service instance not found"
                );
                metrics.record_error("NOT_FOUND", "not_found");
                Err(map_rpc_err(DiscoveryError::NotFound(
                    "service instance not found".to_string(),
                )))
            }
            Err(error) => {
                warn!(
                    request_id = %request_id,
                    trace_id = %trace_id,
                    package = "sdkwork.discovery.internal.v1",
                    service = "RegistryService",
                    method = "RetrieveInstance",
                    operation_id = "discovery.registry.instances.retrieve",
                    error = %error,
                    status = grpc_status_code_for_discovery_error(&error),
                    "instance retrieval failed"
                );
                metrics.record_error(
                    grpc_status_code_for_discovery_error(&error),
                    error.kind_string(),
                );
                Err(map_rpc_err(error))
            }
        }
    }

    #[instrument(
        skip(self, request),
        fields(
            package = "sdkwork.discovery.internal.v1",
            service = "RegistryService",
            method = "DiscoverInstances",
            operation_id = "discovery.registry.instances.discover",
            api_surface = "rpc"
        )
    )]
    async fn discover_instances(
        &self,
        request: Request<internal_proto::DiscoverInstancesRequest>,
    ) -> Result<Response<internal_proto::DiscoverInstancesResponse>, Status> {
        let request_id =
            request_id_from_metadata(request.metadata()).map_err(map_discovery_error_to_status)?;
        let trace_id = trace_id_from_metadata(request.metadata())
            .map_err(|error| map_discovery_error_to_rpc_status(error, &request_id, ""))?;
        let map_rpc_err = |error: sdkwork_discovery_contract::DiscoveryError| {
            map_discovery_error_to_rpc_status(error, &request_id, &trace_id)
        };
        let mut metrics = RpcMetricsGuard::new(
            "sdkwork.discovery.internal.v1",
            "RegistryService",
            "DiscoverInstances",
            "discovery.registry.instances.discover",
        );
        let caller = caller_from_metadata(request.metadata(), self.runtime.context_policy())
            .map_err(|e| {
                record_auth_failure(
                    "sdkwork.discovery.internal.v1",
                    "RegistryService",
                    "DiscoverInstances",
                );
                map_guard_rpc_err(&mut metrics, e, &map_rpc_err)
            })?;
        let query = codec::discover_instances_query(request.into_inner()).map_err(|error| map_guard_rpc_err(&mut metrics, error, &map_rpc_err))?;
        debug!(
            request_id = %request_id,
            trace_id = %trace_id,
            subject_id = %caller.subject_id,
            package = "sdkwork.discovery.internal.v1",
            service = "RegistryService",
            method = "DiscoverInstances",
            operation_id = "discovery.registry.instances.discover",
            "discovering instances"
        );
        match self
            .runtime
            .discover_instances(caller, query, codec::now_millis())
            .await
        {
            Ok(result) => {
                info!(
                    request_id = %request_id,
                    trace_id = %trace_id,
                    package = "sdkwork.discovery.internal.v1",
                    service = "RegistryService",
                    method = "DiscoverInstances",
                    operation_id = "discovery.registry.instances.discover",
                    instance_count = result.instances.len(),
                    status = "OK",
                    "instances discovered"
                );
                metrics.record_success("OK");
                Ok(Response::new(codec::discover_instances_response(
                    result, request_id, trace_id,
                )))
            }
            Err(error) => {
                warn!(
                    request_id = %request_id,
                    trace_id = %trace_id,
                    package = "sdkwork.discovery.internal.v1",
                    service = "RegistryService",
                    method = "DiscoverInstances",
                    operation_id = "discovery.registry.instances.discover",
                    error = %error,
                    status = grpc_status_code_for_discovery_error(&error),
                    "instance discovery failed"
                );
                metrics.record_error(
                    grpc_status_code_for_discovery_error(&error),
                    error.kind_string(),
                );
                Err(map_rpc_err(error))
            }
        }
    }
}

#[tonic::async_trait]
impl<S> internal_proto::discovery_config_service_server::DiscoveryConfigService
    for DiscoveryConfigRpcService<S>
where
    S: Send + Sync + 'static,
{
    type WatchConfigStream = ReceiverStream<Result<internal_proto::WatchConfigResponse, Status>>;

    #[instrument(
        skip(self, request),
        fields(
            package = "sdkwork.discovery.internal.v1",
            service = "DiscoveryConfigService",
            method = "RetrieveEffectiveConfig",
            operation_id = "discovery.config.effective.retrieve",
            api_surface = "rpc"
        )
    )]
    async fn retrieve_effective_config(
        &self,
        request: Request<internal_proto::RetrieveEffectiveConfigRequest>,
    ) -> Result<Response<internal_proto::RetrieveEffectiveConfigResponse>, Status> {
        let request_id =
            request_id_from_metadata(request.metadata()).map_err(map_discovery_error_to_status)?;
        let trace_id = trace_id_from_metadata(request.metadata())
            .map_err(|error| map_discovery_error_to_rpc_status(error, &request_id, ""))?;
        let map_rpc_err = |error: sdkwork_discovery_contract::DiscoveryError| {
            map_discovery_error_to_rpc_status(error, &request_id, &trace_id)
        };
        let mut metrics = RpcMetricsGuard::new(
            "sdkwork.discovery.internal.v1",
            "DiscoveryConfigService",
            "RetrieveEffectiveConfig",
            "discovery.config.effective.retrieve",
        );
        let caller = caller_from_metadata(request.metadata(), self.runtime.context_policy())
            .map_err(|e| {
                record_auth_failure(
                    "sdkwork.discovery.internal.v1",
                    "DiscoveryConfigService",
                    "RetrieveEffectiveConfig",
                );
                map_guard_rpc_err(&mut metrics, e, &map_rpc_err)
            })?;
        let query =
            codec::retrieve_effective_config_query(request.into_inner()).map_err(|error| map_guard_rpc_err(&mut metrics, error, &map_rpc_err))?;
        debug!(
            request_id = %request_id,
            trace_id = %trace_id,
            subject_id = %caller.subject_id,
            package = "sdkwork.discovery.internal.v1",
            service = "DiscoveryConfigService",
            method = "RetrieveEffectiveConfig",
            operation_id = "discovery.config.effective.retrieve",
            "retrieving effective config"
        );
        match self.runtime.retrieve_effective_config(caller, query).await {
            Ok(result) => {
                info!(
                    request_id = %request_id,
                    trace_id = %trace_id,
                    package = "sdkwork.discovery.internal.v1",
                    service = "DiscoveryConfigService",
                    method = "RetrieveEffectiveConfig",
                    operation_id = "discovery.config.effective.retrieve",
                    status = "OK",
                    "effective config retrieved"
                );
                metrics.record_success("OK");
                Ok(Response::new(codec::retrieve_effective_config_response(
                    result, request_id, trace_id,
                )))
            }
            Err(error) => {
                warn!(
                    request_id = %request_id,
                    trace_id = %trace_id,
                    package = "sdkwork.discovery.internal.v1",
                    service = "DiscoveryConfigService",
                    method = "RetrieveEffectiveConfig",
                    operation_id = "discovery.config.effective.retrieve",
                    error = %error,
                    status = grpc_status_code_for_discovery_error(&error),
                    "effective config retrieval failed"
                );
                metrics.record_error(
                    grpc_status_code_for_discovery_error(&error),
                    error.kind_string(),
                );
                Err(map_rpc_err(error))
            }
        }
    }

    #[instrument(
        skip(self, request),
        fields(
            package = "sdkwork.discovery.internal.v1",
            service = "DiscoveryConfigService",
            method = "WatchConfig",
            operation_id = "discovery.config.releases.watch",
            api_surface = "rpc"
        )
    )]
    async fn watch_config(
        &self,
        request: Request<internal_proto::WatchConfigRequest>,
    ) -> Result<Response<Self::WatchConfigStream>, Status> {
        let request_id =
            request_id_from_metadata(request.metadata()).map_err(map_discovery_error_to_status)?;
        let trace_id = trace_id_from_metadata(request.metadata())
            .map_err(|error| map_discovery_error_to_rpc_status(error, &request_id, ""))?;
        if !self.config.enabled {
            RpcMetrics::new(
                "sdkwork.discovery.internal.v1",
                "DiscoveryConfigService",
                "WatchConfig",
                "discovery.config.releases.watch",
            )
            .record_error("UNIMPLEMENTED", "config_watch_disabled");
            return Err(attach_rpc_correlation_metadata(
                Status::unimplemented("discovery config watch is disabled"),
                &request_id,
                &trace_id,
            ));
        }
        let map_rpc_err = |error: sdkwork_discovery_contract::DiscoveryError| {
            map_discovery_error_to_rpc_status(error, &request_id, &trace_id)
        };
        let mut metrics = RpcMetricsGuard::new(
            "sdkwork.discovery.internal.v1",
            "DiscoveryConfigService",
            "WatchConfig",
            "discovery.config.releases.watch",
        );
        let caller = config_reader_from_metadata(request.metadata(), self.runtime.context_policy())
            .map_err(|e| {
                record_auth_failure(
                    "sdkwork.discovery.internal.v1",
                    "DiscoveryConfigService",
                    "WatchConfig",
                );
                map_guard_rpc_err(&mut metrics, e, &map_rpc_err)
            })?;
        let request = request.into_inner();
        codec::validate_required_field("namespace", &request.namespace).map_err(|error| map_guard_rpc_err(&mut metrics, error, &map_rpc_err))?;
        codec::validate_required_field("environment", &request.environment).map_err(|error| map_guard_rpc_err(&mut metrics, error, &map_rpc_err))?;
        codec::validate_required_field("application", &request.application).map_err(|error| map_guard_rpc_err(&mut metrics, error, &map_rpc_err))?;
        codec::validate_required_field("service_name", &request.service_name)
            .map_err(|error| map_guard_rpc_err(&mut metrics, error, &map_rpc_err))?;
        codec::validate_required_field("group", &request.group).map_err(|error| map_guard_rpc_err(&mut metrics, error, &map_rpc_err))?;
        let permit = self.acquire_stream_permit()?;
        let query = sdkwork_discovery_contract::WatchEventsQuery {
            namespace: request.namespace,
            environment: request.environment,
            from_revision: request.from_revision,
            service_name: Some(request.service_name),
            config_group: Some(request.group),
            config_application: Some(request.application),
            max_events: self.config.durable_replay_batch_size,
        };
        let live_events = self.runtime.subscribe_watch_events();
        let events = self
            .runtime
            .watch_config_events(caller.clone(), query.clone())
            .await
            .map_err(|error| map_guard_rpc_err(&mut metrics, error, &map_rpc_err))?;
        let (sender, receiver) = mpsc::channel(self.config.event_buffer_size);
        let heartbeat_interval = Duration::from_millis(self.config.heartbeat_interval_ms);
        let durable_poll_interval = Duration::from_millis(self.config.durable_poll_interval_ms);
        debug!(
            request_id = %request_id,
            trace_id = %trace_id,
            subject_id = %caller.subject_id,
            package = "sdkwork.discovery.internal.v1",
            service = "DiscoveryConfigService",
            method = "WatchConfig",
            operation_id = "discovery.config.releases.watch",
            "starting config watch stream"
        );
        metrics.record_success("OK");
        increment_active_streams("config_watch");
        ConfigWatchStreamTask {
            permit,
            runtime: self.runtime.clone(),
            caller,
            query,
            events,
            live_events,
            sender,
            request_id,
            trace_id,
            heartbeat_interval,
            durable_poll_interval,
        }
        .spawn();
        Ok(Response::new(ReceiverStream::new(receiver)))
    }
}

#[tonic::async_trait]
impl<S> backend_proto::discovery_admin_service_server::DiscoveryAdminService
    for DiscoveryAdminRpcService<S>
where
    S: Send + Sync + 'static,
{
    #[instrument(
        skip(self, request),
        fields(
            package = "sdkwork.discovery.backend.v3",
            service = "DiscoveryAdminService",
            method = "CreateConfigDraft",
            operation_id = "discovery.config.drafts.create",
            api_surface = "rpc"
        )
    )]
    async fn create_config_draft(
        &self,
        request: Request<backend_proto::CreateConfigDraftRequest>,
    ) -> Result<Response<backend_proto::CreateConfigDraftResponse>, Status> {
        let request_id =
            request_id_from_metadata(request.metadata()).map_err(map_discovery_error_to_status)?;
        let trace_id = trace_id_from_metadata(request.metadata())
            .map_err(|error| map_discovery_error_to_rpc_status(error, &request_id, ""))?;
        let map_rpc_err = |error: sdkwork_discovery_contract::DiscoveryError| {
            map_discovery_error_to_rpc_status(error, &request_id, &trace_id)
        };
        let mut metrics = RpcMetricsGuard::new(
            "sdkwork.discovery.backend.v3",
            "DiscoveryAdminService",
            "CreateConfigDraft",
            "discovery.config.drafts.create",
        );
        let idempotency =
            idempotency_from_metadata(request.metadata(), "discovery.config.drafts.create")
                .map_err(|error| map_guard_rpc_err(&mut metrics, error, &map_rpc_err))?;
        let caller = caller_from_metadata_with_required_idempotency(
            request.metadata(),
            self.runtime.context_policy(),
        )
        .map_err(|e| {
            record_auth_failure(
                "sdkwork.discovery.backend.v3",
                "DiscoveryAdminService",
                "CreateConfigDraft",
            );
            map_guard_rpc_err(&mut metrics, e, &map_rpc_err)
        })?;
        let created_by = caller.subject_id.clone();
        let command =
            codec::create_config_draft_command(request.into_inner(), created_by, idempotency)
                .map_err(|error| map_guard_rpc_err(&mut metrics, error, &map_rpc_err))?;
        debug!(
            request_id = %request_id,
            trace_id = %trace_id,
            subject_id = %caller.subject_id,
            package = "sdkwork.discovery.backend.v3",
            service = "DiscoveryAdminService",
            method = "CreateConfigDraft",
            operation_id = "discovery.config.drafts.create",
            "creating config draft"
        );
        match self.runtime.create_config_draft(caller, command).await {
            Ok(draft) => {
                info!(
                    request_id = %request_id,
                    trace_id = %trace_id,
                    package = "sdkwork.discovery.backend.v3",
                    service = "DiscoveryAdminService",
                    method = "CreateConfigDraft",
                    operation_id = "discovery.config.drafts.create",
                    draft_id = %draft.draft_id,
                    status = "OK",
                    "config draft created"
                );
                metrics.record_success("OK");
                Ok(Response::new(codec::create_config_draft_response(
                    draft.draft_id,
                    draft.content_hash,
                    request_id,
                    trace_id,
                )))
            }
            Err(error) => {
                warn!(
                    request_id = %request_id,
                    trace_id = %trace_id,
                    package = "sdkwork.discovery.backend.v3",
                    service = "DiscoveryAdminService",
                    method = "CreateConfigDraft",
                    operation_id = "discovery.config.drafts.create",
                    error = %error,
                    status = grpc_status_code_for_discovery_error(&error),
                    "config draft creation failed"
                );
                metrics.record_error(
                    grpc_status_code_for_discovery_error(&error),
                    error.kind_string(),
                );
                Err(map_rpc_err(error))
            }
        }
    }

    #[instrument(
        skip(self, request),
        fields(
            package = "sdkwork.discovery.backend.v3",
            service = "DiscoveryAdminService",
            method = "PublishConfig",
            operation_id = "discovery.config.releases.publish",
            api_surface = "rpc"
        )
    )]
    async fn publish_config(
        &self,
        request: Request<backend_proto::PublishConfigRequest>,
    ) -> Result<Response<backend_proto::PublishConfigResponse>, Status> {
        let request_id =
            request_id_from_metadata(request.metadata()).map_err(map_discovery_error_to_status)?;
        let trace_id = trace_id_from_metadata(request.metadata())
            .map_err(|error| map_discovery_error_to_rpc_status(error, &request_id, ""))?;
        let map_rpc_err = |error: sdkwork_discovery_contract::DiscoveryError| {
            map_discovery_error_to_rpc_status(error, &request_id, &trace_id)
        };
        let mut metrics = RpcMetricsGuard::new(
            "sdkwork.discovery.backend.v3",
            "DiscoveryAdminService",
            "PublishConfig",
            "discovery.config.releases.publish",
        );
        let idempotency =
            idempotency_from_metadata(request.metadata(), "discovery.config.releases.publish")
                .map_err(|error| map_guard_rpc_err(&mut metrics, error, &map_rpc_err))?;
        let caller = caller_from_metadata_with_required_idempotency(
            request.metadata(),
            self.runtime.context_policy(),
        )
        .map_err(|e| {
            record_auth_failure(
                "sdkwork.discovery.backend.v3",
                "DiscoveryAdminService",
                "PublishConfig",
            );
            map_guard_rpc_err(&mut metrics, e, &map_rpc_err)
        })?;
        let published_by = caller.subject_id.clone();
        let command = codec::publish_config_command(
            request.into_inner(),
            published_by,
            codec::now_millis(),
            idempotency,
        )
        .map_err(|error| map_guard_rpc_err(&mut metrics, error, &map_rpc_err))?;
        debug!(
            request_id = %request_id,
            trace_id = %trace_id,
            subject_id = %caller.subject_id,
            package = "sdkwork.discovery.backend.v3",
            service = "DiscoveryAdminService",
            method = "PublishConfig",
            operation_id = "discovery.config.releases.publish",
            "publishing config"
        );
        match self.runtime.publish_config(caller, command).await {
            Ok(release) => {
                info!(
                    request_id = %request_id,
                    trace_id = %trace_id,
                    package = "sdkwork.discovery.backend.v3",
                    service = "DiscoveryAdminService",
                    method = "PublishConfig",
                    operation_id = "discovery.config.releases.publish",
                    release_id = %release.release_id,
                    status = "OK",
                    "config published"
                );
                metrics.record_success("OK");
                Ok(Response::new(codec::publish_config_response(
                    release, request_id, trace_id,
                )))
            }
            Err(error) => {
                warn!(
                    request_id = %request_id,
                    trace_id = %trace_id,
                    package = "sdkwork.discovery.backend.v3",
                    service = "DiscoveryAdminService",
                    method = "PublishConfig",
                    operation_id = "discovery.config.releases.publish",
                    error = %error,
                    status = grpc_status_code_for_discovery_error(&error),
                    "config publish failed"
                );
                metrics.record_error(
                    grpc_status_code_for_discovery_error(&error),
                    error.kind_string(),
                );
                Err(map_rpc_err(error))
            }
        }
    }

    #[instrument(
        skip(self, request),
        fields(
            package = "sdkwork.discovery.backend.v3",
            service = "DiscoveryAdminService",
            method = "RollbackConfig",
            operation_id = "discovery.config.releases.rollback",
            api_surface = "rpc"
        )
    )]
    async fn rollback_config(
        &self,
        request: Request<backend_proto::RollbackConfigRequest>,
    ) -> Result<Response<backend_proto::RollbackConfigResponse>, Status> {
        let request_id =
            request_id_from_metadata(request.metadata()).map_err(map_discovery_error_to_status)?;
        let trace_id = trace_id_from_metadata(request.metadata())
            .map_err(|error| map_discovery_error_to_rpc_status(error, &request_id, ""))?;
        let map_rpc_err = |error: sdkwork_discovery_contract::DiscoveryError| {
            map_discovery_error_to_rpc_status(error, &request_id, &trace_id)
        };
        let mut metrics = RpcMetricsGuard::new(
            "sdkwork.discovery.backend.v3",
            "DiscoveryAdminService",
            "RollbackConfig",
            "discovery.config.releases.rollback",
        );
        let idempotency =
            idempotency_from_metadata(request.metadata(), "discovery.config.releases.rollback")
                .map_err(|error| map_guard_rpc_err(&mut metrics, error, &map_rpc_err))?;
        let caller = caller_from_metadata_with_required_idempotency(
            request.metadata(),
            self.runtime.context_policy(),
        )
        .map_err(|e| {
            record_auth_failure(
                "sdkwork.discovery.backend.v3",
                "DiscoveryAdminService",
                "RollbackConfig",
            );
            map_guard_rpc_err(&mut metrics, e, &map_rpc_err)
        })?;
        let rolled_back_by = caller.subject_id.clone();
        let command = codec::rollback_config_command(
            request.into_inner(),
            rolled_back_by,
            codec::now_millis(),
            idempotency,
        )
        .map_err(|error| map_guard_rpc_err(&mut metrics, error, &map_rpc_err))?;
        debug!(
            request_id = %request_id,
            trace_id = %trace_id,
            subject_id = %caller.subject_id,
            package = "sdkwork.discovery.backend.v3",
            service = "DiscoveryAdminService",
            method = "RollbackConfig",
            operation_id = "discovery.config.releases.rollback",
            "rolling back config"
        );
        match self.runtime.rollback_config(caller, command).await {
            Ok(release) => {
                info!(
                    request_id = %request_id,
                    trace_id = %trace_id,
                    package = "sdkwork.discovery.backend.v3",
                    service = "DiscoveryAdminService",
                    method = "RollbackConfig",
                    operation_id = "discovery.config.releases.rollback",
                    release_id = %release.release_id,
                    status = "OK",
                    "config rolled back"
                );
                metrics.record_success("OK");
                Ok(Response::new(codec::rollback_config_response(
                    release, request_id, trace_id,
                )))
            }
            Err(error) => {
                warn!(
                    request_id = %request_id,
                    trace_id = %trace_id,
                    package = "sdkwork.discovery.backend.v3",
                    service = "DiscoveryAdminService",
                    method = "RollbackConfig",
                    operation_id = "discovery.config.releases.rollback",
                    error = %error,
                    status = grpc_status_code_for_discovery_error(&error),
                    "config rollback failed"
                );
                metrics.record_error(
                    grpc_status_code_for_discovery_error(&error),
                    error.kind_string(),
                );
                Err(map_rpc_err(error))
            }
        }
    }

    #[instrument(
        skip(self, request),
        fields(
            package = "sdkwork.discovery.backend.v3",
            service = "DiscoveryAdminService",
            method = "ListServices",
            operation_id = "discovery.registry.services.list",
            api_surface = "rpc"
        )
    )]
    async fn list_services(
        &self,
        request: Request<backend_proto::ListServicesRequest>,
    ) -> Result<Response<backend_proto::ListServicesResponse>, Status> {
        let request_id =
            request_id_from_metadata(request.metadata()).map_err(map_discovery_error_to_status)?;
        let trace_id = trace_id_from_metadata(request.metadata())
            .map_err(|error| map_discovery_error_to_rpc_status(error, &request_id, ""))?;
        let map_rpc_err = |error: sdkwork_discovery_contract::DiscoveryError| {
            map_discovery_error_to_rpc_status(error, &request_id, &trace_id)
        };
        let mut metrics = RpcMetricsGuard::new(
            "sdkwork.discovery.backend.v3",
            "DiscoveryAdminService",
            "ListServices",
            "discovery.registry.services.list",
        );
        let caller = caller_from_metadata(request.metadata(), self.runtime.context_policy())
            .map_err(|e| {
                record_auth_failure(
                    "sdkwork.discovery.backend.v3",
                    "DiscoveryAdminService",
                    "ListServices",
                );
                map_guard_rpc_err(&mut metrics, e, &map_rpc_err)
            })?;
        let query = codec::list_services_query(request.into_inner()).map_err(|error| map_guard_rpc_err(&mut metrics, error, &map_rpc_err))?;
        debug!(
            request_id = %request_id,
            trace_id = %trace_id,
            subject_id = %caller.subject_id,
            package = "sdkwork.discovery.backend.v3",
            service = "DiscoveryAdminService",
            method = "ListServices",
            operation_id = "discovery.registry.services.list",
            "listing services"
        );
        match self
            .runtime
            .list_services(caller, query, codec::now_millis())
            .await
        {
            Ok(result) => {
                info!(
                    request_id = %request_id,
                    trace_id = %trace_id,
                    package = "sdkwork.discovery.backend.v3",
                    service = "DiscoveryAdminService",
                    method = "ListServices",
                    operation_id = "discovery.registry.services.list",
                    service_count = result.services.len(),
                    status = "OK",
                    "services listed"
                );
                metrics.record_success("OK");
                Ok(Response::new(codec::list_services_response(
                    result, request_id, trace_id,
                )))
            }
            Err(error) => {
                warn!(
                    request_id = %request_id,
                    trace_id = %trace_id,
                    package = "sdkwork.discovery.backend.v3",
                    service = "DiscoveryAdminService",
                    method = "ListServices",
                    operation_id = "discovery.registry.services.list",
                    error = %error,
                    status = grpc_status_code_for_discovery_error(&error),
                    "service listing failed"
                );
                metrics.record_error(
                    grpc_status_code_for_discovery_error(&error),
                    error.kind_string(),
                );
                Err(map_rpc_err(error))
            }
        }
    }
}

#[tonic::async_trait]
impl<S> internal_proto::discovery_watch_service_server::DiscoveryWatchService
    for DiscoveryWatchRpcService<S>
where
    S: Send + Sync + 'static,
{
    type WatchServiceStream = ReceiverStream<Result<internal_proto::WatchServiceResponse, Status>>;

    #[instrument(
        skip(self, request),
        fields(
            package = "sdkwork.discovery.internal.v1",
            service = "DiscoveryWatchService",
            method = "WatchService",
            operation_id = "discovery.registry.services.watch",
            api_surface = "rpc"
        )
    )]
    async fn watch_service(
        &self,
        request: Request<internal_proto::WatchServiceRequest>,
    ) -> Result<Response<Self::WatchServiceStream>, Status> {
        let request_id =
            request_id_from_metadata(request.metadata()).map_err(map_discovery_error_to_status)?;
        let trace_id = trace_id_from_metadata(request.metadata())
            .map_err(|error| map_discovery_error_to_rpc_status(error, &request_id, ""))?;
        if !self.config.enabled {
            RpcMetrics::new(
                "sdkwork.discovery.internal.v1",
                "DiscoveryWatchService",
                "WatchService",
                "discovery.registry.services.watch",
            )
            .record_error("UNIMPLEMENTED", "service_watch_disabled");
            return Err(attach_rpc_correlation_metadata(
                Status::unimplemented("discovery watch service is disabled"),
                &request_id,
                &trace_id,
            ));
        }
        let map_rpc_err = |error: sdkwork_discovery_contract::DiscoveryError| {
            map_discovery_error_to_rpc_status(error, &request_id, &trace_id)
        };
        let mut metrics = RpcMetricsGuard::new(
            "sdkwork.discovery.internal.v1",
            "DiscoveryWatchService",
            "WatchService",
            "discovery.registry.services.watch",
        );
        let caller =
            registry_reader_from_metadata(request.metadata(), self.runtime.context_policy())
                .map_err(|e| {
                    record_auth_failure(
                        "sdkwork.discovery.internal.v1",
                        "DiscoveryWatchService",
                        "WatchService",
                    );
                    map_guard_rpc_err(&mut metrics, e, &map_rpc_err)
                })?;
        let request = request.into_inner();
        codec::validate_required_field("namespace", &request.namespace).map_err(|error| map_guard_rpc_err(&mut metrics, error, &map_rpc_err))?;
        codec::validate_required_field("environment", &request.environment).map_err(|error| map_guard_rpc_err(&mut metrics, error, &map_rpc_err))?;
        codec::validate_required_field("service_name", &request.service_name)
            .map_err(|error| map_guard_rpc_err(&mut metrics, error, &map_rpc_err))?;
        let permit = self.acquire_stream_permit()?;
        let query = sdkwork_discovery_contract::WatchEventsQuery {
            namespace: request.namespace,
            environment: request.environment,
            from_revision: request.from_revision,
            service_name: Some(request.service_name),
            config_group: None,
            config_application: None,
            max_events: self.config.durable_replay_batch_size,
        };
        let live_events = self.runtime.subscribe_watch_events();
        let events = self
            .runtime
            .watch_registry_events(caller.clone(), query.clone())
            .await
            .map_err(|error| map_guard_rpc_err(&mut metrics, error, &map_rpc_err))?;
        let (sender, receiver) = mpsc::channel(self.config.event_buffer_size);
        let heartbeat_interval = Duration::from_millis(self.config.heartbeat_interval_ms);
        let durable_poll_interval = Duration::from_millis(self.config.durable_poll_interval_ms);
        debug!(
            request_id = %request_id,
            trace_id = %trace_id,
            subject_id = %caller.subject_id,
            package = "sdkwork.discovery.internal.v1",
            service = "DiscoveryWatchService",
            method = "WatchService",
            operation_id = "discovery.registry.services.watch",
            "starting service watch stream"
        );
        metrics.record_success("OK");
        increment_active_streams("service_watch");
        ServiceWatchStreamTask {
            permit,
            runtime: self.runtime.clone(),
            caller,
            query,
            events,
            live_events,
            sender,
            request_id,
            trace_id,
            heartbeat_interval,
            durable_poll_interval,
        }
        .spawn();
        Ok(Response::new(ReceiverStream::new(receiver)))
    }
}

struct ServiceWatchStreamTask<S> {
    permit: OwnedSemaphorePermit,
    runtime: DiscoveryRpcRuntime<S>,
    caller: CallerContext,
    query: WatchEventsQuery,
    events: Vec<DiscoveryEvent>,
    live_events: WatchEventSubscriber,
    sender: mpsc::Sender<Result<internal_proto::WatchServiceResponse, Status>>,
    request_id: String,
    trace_id: String,
    heartbeat_interval: Duration,
    durable_poll_interval: Duration,
}

impl<S> ServiceWatchStreamTask<S>
where
    S: Send + Sync + 'static,
{
    fn spawn(self) {
        tokio::spawn(async move {
            let _permit = self.permit;
            let mut cursor_revision = self.query.from_revision;
            for event in self.events {
                if event.revision <= cursor_revision {
                    continue;
                }
                cursor_revision = cursor_revision.max(event.revision);
                if !event_matches_service_watch(&event, &self.query) {
                    continue;
                }
                let response = service_watch_response(
                    &self.runtime,
                    &self.caller,
                    event,
                    &self.request_id,
                    &self.trace_id,
                )
                .await;
                if finish_watch_stream(
                    "service_watch",
                    send_service_watch_response(&self.sender, response).await,
                ) {
                    return;
                }
            }

            let mut live_events = self.live_events;
            let mut heartbeat = tokio::time::interval(self.heartbeat_interval);
            let mut persisted_events = tokio::time::interval(self.durable_poll_interval);
            persisted_events.set_missed_tick_behavior(MissedTickBehavior::Skip);
            heartbeat.tick().await;
            persisted_events.tick().await;
            loop {
                tokio::select! {
                    event = live_events.recv() => {
                        match event {
                            Some(Ok(event)) if should_wake_persisted_poll(
                                &event,
                                &self.query,
                                cursor_revision,
                            ) => {
                                if drain_persisted_service_events(
                                    &self.runtime,
                                    &self.caller,
                                    &self.query,
                                    &mut cursor_revision,
                                    &self.sender,
                                    &self.request_id,
                                    &self.trace_id,
                                )
                                .await {
                                    decrement_active_streams("service_watch");
                                    return;
                                }
                            }
                            Some(Ok(_)) => {}
                            Some(Err(error)) => {
                                if finish_watch_stream(
                                    "service_watch",
                                    send_watch_item(
                                        &self.sender,
                                        Err(map_discovery_error_to_rpc_status(
                                            error,
                                            &self.request_id,
                                            &self.trace_id,
                                        )),
                                    )
                                    .await,
                                ) {
                                    return;
                                }
                            }
                            None => {
                                record_service_watch_cancellation();
                                decrement_active_streams("service_watch");
                                return;
                            }
                        }
                    }
                    _ = persisted_events.tick() => {
                        if drain_persisted_service_events(
                            &self.runtime,
                            &self.caller,
                            &self.query,
                            &mut cursor_revision,
                            &self.sender,
                            &self.request_id,
                            &self.trace_id,
                        )
                        .await {
                            decrement_active_streams("service_watch");
                            return;
                        }
                    }
                    _ = heartbeat.tick() => {
                        let response = service_watch_heartbeat(
                            cursor_revision,
                            &self.request_id,
                            &self.trace_id,
                        );
                        if finish_watch_stream(
                            "service_watch",
                            send_watch_item(&self.sender, Ok(response)).await,
                        ) {
                            return;
                        }
                    }
                }
            }
        });
    }
}

struct ConfigWatchStreamTask<S> {
    permit: OwnedSemaphorePermit,
    runtime: DiscoveryRpcRuntime<S>,
    caller: CallerContext,
    query: WatchEventsQuery,
    events: Vec<DiscoveryEvent>,
    live_events: WatchEventSubscriber,
    sender: mpsc::Sender<Result<internal_proto::WatchConfigResponse, Status>>,
    request_id: String,
    trace_id: String,
    heartbeat_interval: Duration,
    durable_poll_interval: Duration,
}

impl<S> ConfigWatchStreamTask<S>
where
    S: Send + Sync + 'static,
{
    fn spawn(self) {
        tokio::spawn(async move {
            let _permit = self.permit;
            let mut cursor_revision = self.query.from_revision;
            for event in self.events {
                if event.revision <= cursor_revision {
                    continue;
                }
                cursor_revision = cursor_revision.max(event.revision);
                if !event_matches_config_watch(&event, &self.query) {
                    continue;
                }
                let response = config_watch_response(event, &self.request_id, &self.trace_id);
                if finish_watch_stream(
                    "config_watch",
                    send_watch_item(&self.sender, Ok(response)).await,
                ) {
                    return;
                }
            }

            let mut live_events = self.live_events;
            let mut heartbeat = tokio::time::interval(self.heartbeat_interval);
            let mut persisted_events = tokio::time::interval(self.durable_poll_interval);
            persisted_events.set_missed_tick_behavior(MissedTickBehavior::Skip);
            heartbeat.tick().await;
            persisted_events.tick().await;
            loop {
                tokio::select! {
                    event = live_events.recv() => {
                        match event {
                            Some(Ok(event)) if should_wake_persisted_poll(
                                &event,
                                &self.query,
                                cursor_revision,
                            ) => {
                                if drain_persisted_config_events(
                                    &self.runtime,
                                    &self.caller,
                                    &self.query,
                                    &mut cursor_revision,
                                    &self.sender,
                                    &self.request_id,
                                    &self.trace_id,
                                )
                                .await {
                                    decrement_active_streams("config_watch");
                                    return;
                                }
                            }
                            Some(Ok(_)) => {}
                            Some(Err(error)) => {
                                if finish_watch_stream(
                                    "config_watch",
                                    send_watch_item(
                                        &self.sender,
                                        Err(map_discovery_error_to_rpc_status(
                                            error,
                                            &self.request_id,
                                            &self.trace_id,
                                        )),
                                    )
                                    .await,
                                ) {
                                    return;
                                }
                            }
                            None => {
                                record_config_watch_cancellation();
                                decrement_active_streams("config_watch");
                                return;
                            }
                        }
                    }
                    _ = persisted_events.tick() => {
                        if drain_persisted_config_events(
                            &self.runtime,
                            &self.caller,
                            &self.query,
                            &mut cursor_revision,
                            &self.sender,
                            &self.request_id,
                            &self.trace_id,
                        )
                        .await {
                            decrement_active_streams("config_watch");
                            return;
                        }
                    }
                    _ = heartbeat.tick() => {
                        let response = config_watch_heartbeat(
                            cursor_revision,
                            &self.request_id,
                            &self.trace_id,
                        );
                        if finish_watch_stream(
                            "config_watch",
                            send_watch_item(&self.sender, Ok(response)).await,
                        ) {
                            return;
                        }
                    }
                }
            }
        });
    }
}

fn watch_query_after(query: &WatchEventsQuery, revision: u64) -> WatchEventsQuery {
    let mut query = query.clone();
    query.from_revision = revision;
    query
}

fn should_wake_persisted_poll(
    event: &DiscoveryEvent,
    query: &WatchEventsQuery,
    cursor_revision: u64,
) -> bool {
    event.revision > cursor_revision
        && event.namespace == query.namespace
        && event.environment == query.environment
}

async fn drain_persisted_service_events<S>(
    runtime: &DiscoveryRpcRuntime<S>,
    caller: &CallerContext,
    query: &WatchEventsQuery,
    cursor_revision: &mut u64,
    sender: &mpsc::Sender<Result<internal_proto::WatchServiceResponse, Status>>,
    request_id: &str,
    trace_id: &str,
) -> bool
where
    S: Send + Sync + 'static,
{
    match runtime
        .watch_registry_events(caller.clone(), watch_query_after(query, *cursor_revision))
        .await
    {
        Ok(events) => {
            for event in events {
                if event.revision <= *cursor_revision {
                    continue;
                }
                *cursor_revision = (*cursor_revision).max(event.revision);
                if !event_matches_service_watch(&event, query) {
                    continue;
                }
                let response =
                    service_watch_response(runtime, caller, event, request_id, trace_id).await;
                if finish_watch_stream(
                    "service_watch",
                    send_service_watch_response(sender, response).await,
                ) {
                    return true;
                }
            }
            false
        }
        Err(error) => finish_watch_stream(
            "service_watch",
            send_watch_item(
                sender,
                Err(map_discovery_error_to_rpc_status(
                    error, request_id, trace_id,
                )),
            )
            .await,
        ),
    }
}

async fn drain_persisted_config_events<S>(
    runtime: &DiscoveryRpcRuntime<S>,
    caller: &CallerContext,
    query: &WatchEventsQuery,
    cursor_revision: &mut u64,
    sender: &mpsc::Sender<Result<internal_proto::WatchConfigResponse, Status>>,
    request_id: &str,
    trace_id: &str,
) -> bool
where
    S: Send + Sync + 'static,
{
    match runtime
        .watch_config_events(caller.clone(), watch_query_after(query, *cursor_revision))
        .await
    {
        Ok(events) => {
            for event in events {
                if event.revision <= *cursor_revision {
                    continue;
                }
                *cursor_revision = (*cursor_revision).max(event.revision);
                if !event_matches_config_watch(&event, query) {
                    continue;
                }
                let response = config_watch_response(event, request_id, trace_id);
                if finish_watch_stream("config_watch", send_watch_item(sender, Ok(response)).await)
                {
                    return true;
                }
            }
            false
        }
        Err(error) => finish_watch_stream(
            "config_watch",
            send_watch_item(
                sender,
                Err(map_discovery_error_to_rpc_status(
                    error, request_id, trace_id,
                )),
            )
            .await,
        ),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WatchStreamSendOutcome {
    Continue,
    StreamEnded,
    ClientDisconnected,
}

fn record_service_watch_cancellation() {
    record_cancellation(
        "sdkwork.discovery.internal.v1",
        "DiscoveryWatchService",
        "WatchService",
    );
}

fn record_config_watch_cancellation() {
    record_cancellation(
        "sdkwork.discovery.internal.v1",
        "DiscoveryConfigService",
        "WatchConfig",
    );
}

fn finish_watch_stream(surface: &'static str, outcome: WatchStreamSendOutcome) -> bool {
    match outcome {
        WatchStreamSendOutcome::Continue => false,
        WatchStreamSendOutcome::StreamEnded => {
            decrement_active_streams(surface);
            true
        }
        WatchStreamSendOutcome::ClientDisconnected => {
            if surface == "service_watch" {
                record_service_watch_cancellation();
            } else {
                record_config_watch_cancellation();
            }
            decrement_active_streams(surface);
            true
        }
    }
}

async fn send_watch_item<T>(
    sender: &mpsc::Sender<Result<T, Status>>,
    response: Result<T, Status>,
) -> WatchStreamSendOutcome {
    let should_close_stream = response.is_err();
    match sender.send(response).await {
        Ok(()) if should_close_stream => WatchStreamSendOutcome::StreamEnded,
        Ok(()) => WatchStreamSendOutcome::Continue,
        Err(_) => WatchStreamSendOutcome::ClientDisconnected,
    }
}

async fn send_service_watch_response(
    sender: &mpsc::Sender<Result<internal_proto::WatchServiceResponse, Status>>,
    response: Result<internal_proto::WatchServiceResponse, Status>,
) -> WatchStreamSendOutcome {
    send_watch_item(sender, response).await
}

async fn service_watch_response<S>(
    runtime: &DiscoveryRpcRuntime<S>,
    caller: &CallerContext,
    event: DiscoveryEvent,
    request_id: &str,
    trace_id: &str,
) -> Result<internal_proto::WatchServiceResponse, Status>
where
    S: Send + Sync + 'static,
{
    Ok(internal_proto::WatchServiceResponse {
        event_type: codec::watch_event_type(&event),
        instance: service_watch_instance(runtime, caller, &event, request_id, trace_id).await?,
        metadata: Some(codec::response_metadata(
            event.revision,
            request_id.to_string(),
            trace_id.to_string(),
        )),
    })
}

async fn service_watch_instance<S>(
    runtime: &DiscoveryRpcRuntime<S>,
    caller: &CallerContext,
    event: &DiscoveryEvent,
    request_id: &str,
    trace_id: &str,
) -> Result<Option<common_proto::ServiceInstance>, Status>
where
    S: Send + Sync + 'static,
{
    if matches!(event.kind, DiscoveryEventKind::InstanceDeregistered) {
        return Ok(Some(service_watch_identity_tombstone(event)));
    }

    let Some(service_name) = event.service_name.clone() else {
        return Err(attach_rpc_correlation_metadata(
            Status::failed_precondition("registry watch event is missing service_name"),
            request_id,
            trace_id,
        ));
    };
    let result = runtime
        .retrieve_instance(
            caller.clone(),
            RetrieveInstanceQuery {
                namespace: event.namespace.clone(),
                environment: event.environment.clone(),
                service_name,
                instance_id: event.resource_id.clone(),
            },
            codec::now_millis(),
        )
        .await
        .map_err(|error| map_discovery_error_to_rpc_status(error, request_id, trace_id))?;

    Ok(result
        .map(codec::service_instance_to_proto)
        .or_else(|| Some(service_watch_identity_tombstone(event))))
}

fn service_watch_identity_tombstone(event: &DiscoveryEvent) -> common_proto::ServiceInstance {
    common_proto::ServiceInstance {
        namespace: event.namespace.clone(),
        environment: event.environment.clone(),
        service_name: event.service_name.clone().unwrap_or_default(),
        instance_id: event.resource_id.clone(),
        endpoint: String::new(),
        protocol: String::new(),
        version: String::new(),
        region: String::new(),
        zone: String::new(),
        weight: 0,
        priority: 0,
        status: common_proto::InstanceStatus::NotServing as i32,
        metadata: Default::default(),
        lease_id: String::new(),
        expires_at: None,
        revision: event.revision,
        health_check: None,
    }
}

fn service_watch_heartbeat(
    revision: u64,
    request_id: &str,
    trace_id: &str,
) -> internal_proto::WatchServiceResponse {
    internal_proto::WatchServiceResponse {
        event_type: common_proto::WatchEventType::Heartbeat as i32,
        instance: None,
        metadata: Some(codec::response_metadata(
            revision,
            request_id.to_string(),
            trace_id.to_string(),
        )),
    }
}

fn config_watch_response(
    event: DiscoveryEvent,
    request_id: &str,
    trace_id: &str,
) -> internal_proto::WatchConfigResponse {
    internal_proto::WatchConfigResponse {
        event_type: codec::watch_event_type(&event),
        release_id: event.resource_id,
        group: event.config_group.unwrap_or_default(),
        key: event.config_key.unwrap_or_default(),
        metadata: Some(codec::response_metadata(
            event.revision,
            request_id.to_string(),
            trace_id.to_string(),
        )),
    }
}

fn config_watch_heartbeat(
    revision: u64,
    request_id: &str,
    trace_id: &str,
) -> internal_proto::WatchConfigResponse {
    internal_proto::WatchConfigResponse {
        event_type: common_proto::WatchEventType::Heartbeat as i32,
        release_id: String::new(),
        group: String::new(),
        key: String::new(),
        metadata: Some(codec::response_metadata(
            revision,
            request_id.to_string(),
            trace_id.to_string(),
        )),
    }
}

#[allow(dead_code)]
fn _assert_response_metadata_send_sync(_: common_proto::ResponseMetadata) {}
