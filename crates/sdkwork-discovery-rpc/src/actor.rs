use sdkwork_discovery_contract::{
    CallerContext, ConfigDraft, ConfigRelease, CreateConfigDraftCommand, DeregisterInstanceResult,
    DiscoverInstancesQuery, DiscoverInstancesResult, DiscoveryError, DiscoveryEvent,
    DiscoveryResult, EffectiveConfig, ListServicesQuery, ListServicesResult, PublishConfigCommand,
    RegisterInstanceCommand, RegisterInstanceResult, RenewLeaseCommand, RenewLeaseResult,
    ReportInstanceStatusCommand, ReportInstanceStatusResult, RetrieveEffectiveConfigQuery,
    RetrieveInstanceQuery, RollbackConfigCommand, ServiceInstance, WatchEventsQuery,
};
use sdkwork_discovery_core::{
    ComponentChangeEmitter, DiscoveryControlPlane, TracingComponentChangeEmitter,
};
use sdkwork_discovery_storage_contract::{ConfigStore, RegistryStore, WatchEventStore};
use tokio::sync::{mpsc, oneshot};
use tokio::time::{Duration, Instant, MissedTickBehavior};

use crate::context::RpcContextPolicy;
use crate::degradation::OperationType;
use crate::health_probes::run_health_checks;
use crate::resilience::{RuntimeResilience, RuntimeResilienceConfig};
use crate::service_token::{DiscoveryRpcServiceTokenVerifierConfig, ServiceTokenVerifier};
use crate::watch::{events_for_revision, WatchEventPublisher, WatchEventSubscriber};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryRpcRuntimeConfig {
    pub registry_expiry_scan_interval_ms: u64,
    pub registry_expiry_scan_batch_size: usize,
    pub allow_unsigned_local_context: bool,
    pub service_token_verifier: Option<DiscoveryRpcServiceTokenVerifierConfig>,
    pub event_gc_interval_ms: u64,
    pub event_gc_retention_count: u64,
    pub event_gc_batch_size: usize,
    pub resilience: RuntimeResilienceConfig,
    pub health_check_scan_interval_ms: u64,
}

impl Default for DiscoveryRpcRuntimeConfig {
    fn default() -> Self {
        Self {
            registry_expiry_scan_interval_ms: 0,
            registry_expiry_scan_batch_size: 1_000,
            allow_unsigned_local_context: false,
            service_token_verifier: None,
            event_gc_interval_ms: 60_000,
            event_gc_retention_count: 10_000,
            event_gc_batch_size: 1_000,
            resilience: RuntimeResilienceConfig::default(),
            health_check_scan_interval_ms: 0,
        }
    }
}

pub struct DiscoveryRpcRuntime<S> {
    sender: mpsc::Sender<RuntimeCommand>,
    watch_events: WatchEventPublisher,
    context_policy: RpcContextPolicy,
    _store_marker: std::marker::PhantomData<S>,
}

impl<S> Clone for DiscoveryRpcRuntime<S> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            watch_events: self.watch_events.clone(),
            context_policy: self.context_policy.clone(),
            _store_marker: std::marker::PhantomData,
        }
    }
}

enum RuntimeCommand {
    RegisterInstance {
        caller: CallerContext,
        command: RegisterInstanceCommand,
        response: oneshot::Sender<DiscoveryResult<RegisterInstanceResult>>,
    },
    BatchRegisterInstances {
        caller: CallerContext,
        commands: Vec<RegisterInstanceCommand>,
        response: oneshot::Sender<DiscoveryResult<sdkwork_discovery_contract::BatchRegisterResult>>,
    },
    RenewLease {
        caller: CallerContext,
        command: RenewLeaseCommand,
        response: oneshot::Sender<DiscoveryResult<RenewLeaseResult>>,
    },
    DeregisterInstance {
        caller: CallerContext,
        namespace: String,
        environment: String,
        service_name: String,
        instance_id: String,
        now_ms: u64,
        response:
            oneshot::Sender<DiscoveryResult<sdkwork_discovery_contract::DeregisterInstanceResult>>,
    },
    ExpireInstances {
        now_ms: u64,
        max_instances: usize,
        response: oneshot::Sender<DiscoveryResult<Vec<DeregisterInstanceResult>>>,
    },
    ReportInstanceStatus {
        caller: CallerContext,
        command: ReportInstanceStatusCommand,
        response: oneshot::Sender<DiscoveryResult<ReportInstanceStatusResult>>,
    },
    DiscoverInstances {
        caller: CallerContext,
        query: DiscoverInstancesQuery,
        now_ms: u64,
        response: oneshot::Sender<DiscoveryResult<DiscoverInstancesResult>>,
    },
    RetrieveInstance {
        caller: CallerContext,
        query: RetrieveInstanceQuery,
        now_ms: u64,
        response: oneshot::Sender<DiscoveryResult<Option<ServiceInstance>>>,
    },
    ListServices {
        caller: CallerContext,
        query: ListServicesQuery,
        now_ms: u64,
        response: oneshot::Sender<DiscoveryResult<ListServicesResult>>,
    },
    CreateConfigDraft {
        caller: CallerContext,
        command: CreateConfigDraftCommand,
        response: oneshot::Sender<DiscoveryResult<ConfigDraft>>,
    },
    PublishConfig {
        caller: CallerContext,
        command: PublishConfigCommand,
        response: oneshot::Sender<DiscoveryResult<ConfigRelease>>,
    },
    RollbackConfig {
        caller: CallerContext,
        command: RollbackConfigCommand,
        response: oneshot::Sender<DiscoveryResult<ConfigRelease>>,
    },
    RetrieveEffectiveConfig {
        caller: CallerContext,
        query: RetrieveEffectiveConfigQuery,
        response: oneshot::Sender<DiscoveryResult<EffectiveConfig>>,
    },
    WatchRegistryEvents {
        caller: CallerContext,
        query: WatchEventsQuery,
        response: oneshot::Sender<DiscoveryResult<Vec<DiscoveryEvent>>>,
    },
    WatchConfigEvents {
        caller: CallerContext,
        query: WatchEventsQuery,
        response: oneshot::Sender<DiscoveryResult<Vec<DiscoveryEvent>>>,
    },
}

impl<S> DiscoveryRpcRuntime<S>
where
    S: ConfigStore + RegistryStore + WatchEventStore + Send + Sync + 'static,
{
    pub fn new(control_plane: DiscoveryControlPlane<S>) -> Self {
        Self::with_config(control_plane, DiscoveryRpcRuntimeConfig::default())
    }

    pub fn with_config(
        mut control_plane: DiscoveryControlPlane<S>,
        config: DiscoveryRpcRuntimeConfig,
    ) -> Self {
        let (sender, mut receiver) = mpsc::channel(128);
        let watch_events = WatchEventPublisher::new(1_024);
        let publisher = watch_events.clone();
        let context_policy = RpcContextPolicy {
            allow_unsigned_local_context: config.allow_unsigned_local_context,
            service_token_verifier: config
                .service_token_verifier
                .clone()
                .map(ServiceTokenVerifier::new),
        };

        tokio::spawn(async move {
            run_actor(&mut control_plane, &publisher, &mut receiver, config).await;
        });

        Self {
            sender,
            watch_events,
            context_policy,
            _store_marker: std::marker::PhantomData,
        }
    }
}

impl<S> DiscoveryRpcRuntime<S> {
    pub async fn register_instance(
        &self,
        caller: CallerContext,
        command: RegisterInstanceCommand,
    ) -> DiscoveryResult<RegisterInstanceResult> {
        let (response, receiver) = oneshot::channel();
        self.send(RuntimeCommand::RegisterInstance {
            caller,
            command,
            response,
        })
        .await?;
        receive(receiver).await
    }

    pub async fn batch_register_instances(
        &self,
        caller: CallerContext,
        commands: Vec<RegisterInstanceCommand>,
    ) -> DiscoveryResult<sdkwork_discovery_contract::BatchRegisterResult> {
        let (response, receiver) = oneshot::channel();
        self.send(RuntimeCommand::BatchRegisterInstances {
            caller,
            commands,
            response,
        })
        .await?;
        receive(receiver).await
    }

    pub async fn renew_lease(
        &self,
        caller: CallerContext,
        command: RenewLeaseCommand,
    ) -> DiscoveryResult<RenewLeaseResult> {
        let (response, receiver) = oneshot::channel();
        self.send(RuntimeCommand::RenewLease {
            caller,
            command,
            response,
        })
        .await?;
        receive(receiver).await
    }

    pub async fn deregister_instance(
        &self,
        caller: CallerContext,
        namespace: String,
        environment: String,
        service_name: String,
        instance_id: String,
        now_ms: u64,
    ) -> DiscoveryResult<sdkwork_discovery_contract::DeregisterInstanceResult> {
        let (response, receiver) = oneshot::channel();
        self.send(RuntimeCommand::DeregisterInstance {
            caller,
            namespace,
            environment,
            service_name,
            instance_id,
            now_ms,
            response,
        })
        .await?;
        receive(receiver).await
    }

    pub async fn expire_instances(
        &self,
        now_ms: u64,
        max_instances: usize,
    ) -> DiscoveryResult<Vec<DeregisterInstanceResult>> {
        let (response, receiver) = oneshot::channel();
        self.send(RuntimeCommand::ExpireInstances {
            now_ms,
            max_instances,
            response,
        })
        .await?;
        receive(receiver).await
    }

    pub async fn report_instance_status(
        &self,
        caller: CallerContext,
        command: ReportInstanceStatusCommand,
    ) -> DiscoveryResult<ReportInstanceStatusResult> {
        let (response, receiver) = oneshot::channel();
        self.send(RuntimeCommand::ReportInstanceStatus {
            caller,
            command,
            response,
        })
        .await?;
        receive(receiver).await
    }

    pub async fn discover_instances(
        &self,
        caller: CallerContext,
        query: DiscoverInstancesQuery,
        now_ms: u64,
    ) -> DiscoveryResult<DiscoverInstancesResult> {
        let (response, receiver) = oneshot::channel();
        self.send(RuntimeCommand::DiscoverInstances {
            caller,
            query,
            now_ms,
            response,
        })
        .await?;
        receive(receiver).await
    }

    pub async fn retrieve_instance(
        &self,
        caller: CallerContext,
        query: RetrieveInstanceQuery,
        now_ms: u64,
    ) -> DiscoveryResult<Option<ServiceInstance>> {
        let (response, receiver) = oneshot::channel();
        self.send(RuntimeCommand::RetrieveInstance {
            caller,
            query,
            now_ms,
            response,
        })
        .await?;
        receive(receiver).await
    }

    pub async fn list_services(
        &self,
        caller: CallerContext,
        query: ListServicesQuery,
        now_ms: u64,
    ) -> DiscoveryResult<ListServicesResult> {
        let (response, receiver) = oneshot::channel();
        self.send(RuntimeCommand::ListServices {
            caller,
            query,
            now_ms,
            response,
        })
        .await?;
        receive(receiver).await
    }

    pub async fn create_config_draft(
        &self,
        caller: CallerContext,
        command: CreateConfigDraftCommand,
    ) -> DiscoveryResult<ConfigDraft> {
        let (response, receiver) = oneshot::channel();
        self.send(RuntimeCommand::CreateConfigDraft {
            caller,
            command,
            response,
        })
        .await?;
        receive(receiver).await
    }

    pub async fn publish_config(
        &self,
        caller: CallerContext,
        command: PublishConfigCommand,
    ) -> DiscoveryResult<ConfigRelease> {
        let (response, receiver) = oneshot::channel();
        self.send(RuntimeCommand::PublishConfig {
            caller,
            command,
            response,
        })
        .await?;
        receive(receiver).await
    }

    pub async fn rollback_config(
        &self,
        caller: CallerContext,
        command: RollbackConfigCommand,
    ) -> DiscoveryResult<ConfigRelease> {
        let (response, receiver) = oneshot::channel();
        self.send(RuntimeCommand::RollbackConfig {
            caller,
            command,
            response,
        })
        .await?;
        receive(receiver).await
    }

    pub async fn retrieve_effective_config(
        &self,
        caller: CallerContext,
        query: RetrieveEffectiveConfigQuery,
    ) -> DiscoveryResult<EffectiveConfig> {
        let (response, receiver) = oneshot::channel();
        self.send(RuntimeCommand::RetrieveEffectiveConfig {
            caller,
            query,
            response,
        })
        .await?;
        receive(receiver).await
    }

    pub async fn watch_registry_events(
        &self,
        caller: CallerContext,
        query: WatchEventsQuery,
    ) -> DiscoveryResult<Vec<DiscoveryEvent>> {
        let (response, receiver) = oneshot::channel();
        self.send(RuntimeCommand::WatchRegistryEvents {
            caller,
            query,
            response,
        })
        .await?;
        receive(receiver).await
    }

    pub async fn watch_config_events(
        &self,
        caller: CallerContext,
        query: WatchEventsQuery,
    ) -> DiscoveryResult<Vec<DiscoveryEvent>> {
        let (response, receiver) = oneshot::channel();
        self.send(RuntimeCommand::WatchConfigEvents {
            caller,
            query,
            response,
        })
        .await?;
        receive(receiver).await
    }

    pub fn subscribe_watch_events(&self) -> WatchEventSubscriber {
        self.watch_events.subscribe()
    }

    pub(crate) fn context_policy(&self) -> RpcContextPolicy {
        self.context_policy.clone()
    }

    async fn send(&self, command: RuntimeCommand) -> DiscoveryResult<()> {
        self.sender.send(command).await.map_err(|_| {
            DiscoveryError::InvalidConfig("discovery rpc runtime is not available".to_string())
        })
    }
}

async fn receive<T>(receiver: oneshot::Receiver<DiscoveryResult<T>>) -> DiscoveryResult<T> {
    receiver.await.map_err(|_| {
        DiscoveryError::InvalidConfig("discovery rpc runtime response was dropped".to_string())
    })?
}

async fn run_actor<S>(
    control_plane: &mut DiscoveryControlPlane<S>,
    publisher: &WatchEventPublisher,
    receiver: &mut mpsc::Receiver<RuntimeCommand>,
    config: DiscoveryRpcRuntimeConfig,
) where
    S: ConfigStore + RegistryStore + WatchEventStore,
{
    let has_expiry_scan = config.registry_expiry_scan_interval_ms > 0;
    let has_gc = config.event_gc_interval_ms > 0;
    let has_health_scan = config.health_check_scan_interval_ms > 0;
    let mut resilience = RuntimeResilience::new(config.resilience.clone());

    if !has_expiry_scan && !has_gc && !has_health_scan {
        while let Some(command) = receiver.recv().await {
            dispatch_command(control_plane, publisher, &mut resilience, command).await;
        }
        return;
    }

    let scan_interval = Duration::from_millis(config.registry_expiry_scan_interval_ms.max(1));
    let mut scan_timer = tokio::time::interval_at(Instant::now() + scan_interval, scan_interval);
    scan_timer.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let gc_interval = Duration::from_millis(config.event_gc_interval_ms.max(1));
    let mut gc_timer = tokio::time::interval_at(Instant::now() + gc_interval, gc_interval);
    gc_timer.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let health_interval = Duration::from_millis(config.health_check_scan_interval_ms.max(1));
    let mut health_timer =
        tokio::time::interval_at(Instant::now() + health_interval, health_interval);
    health_timer.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            command = receiver.recv() => {
                let Some(command) = command else {
                    return;
                };
                dispatch_command(control_plane, publisher, &mut resilience, command).await;
            }
            _ = scan_timer.tick(), if has_expiry_scan => {
                run_expiry_scan(
                    control_plane,
                    publisher,
                    crate::codec::now_millis(),
                    config.registry_expiry_scan_batch_size,
                ).await;
            }
            _ = gc_timer.tick(), if has_gc => {
                run_event_gc(
                    control_plane,
                    config.event_gc_retention_count,
                    config.event_gc_batch_size,
                ).await;
            }
            _ = health_timer.tick(), if has_health_scan => {
                run_health_checks(control_plane, crate::codec::now_millis()).await;
            }
        }
    }
}

async fn dispatch_command<S>(
    control_plane: &mut DiscoveryControlPlane<S>,
    publisher: &WatchEventPublisher,
    resilience: &mut RuntimeResilience,
    command: RuntimeCommand,
) where
    S: ConfigStore + RegistryStore + WatchEventStore,
{
    match command {
        RuntimeCommand::RegisterInstance {
            caller,
            command,
            response,
        } => {
            let Some(response) = gate_command(resilience, OperationType::Write, response) else {
                return;
            };
            let event_scope = (command.namespace.clone(), command.environment.clone());
            let result = control_plane.register_instance(&caller, command).await;
            resilience.record_result(&result);
            if let Ok(ref register_result) = result {
                TracingComponentChangeEmitter.emit_registry_changed(
                    &event_scope.0,
                    &event_scope.1,
                    register_result.revision,
                );
            }
            publish_scoped_revision_events(control_plane.store(), publisher, event_scope, &result)
                .await;
            let _ = response.send(result);
        }
        RuntimeCommand::BatchRegisterInstances {
            caller,
            commands,
            response,
        } => {
            let Some(response) = gate_command(resilience, OperationType::Write, response) else {
                return;
            };
            let scopes = commands
                .iter()
                .map(|command| (command.namespace.clone(), command.environment.clone()))
                .collect::<Vec<_>>();
            let result = control_plane
                .batch_register_instances(&caller, commands)
                .await;
            resilience.record_result(&result);
            if let Ok(ref batch_result) = result {
                let emitter = TracingComponentChangeEmitter;
                for (index, register_result) in batch_result.results.iter().enumerate() {
                    if let Some((namespace, environment)) = scopes.get(index) {
                        emitter.emit_registry_changed(
                            namespace,
                            environment,
                            register_result.revision,
                        );
                    }
                }
            }
            let _ = response.send(result);
        }
        RuntimeCommand::RenewLease {
            caller,
            command,
            response,
        } => {
            let Some(response) = gate_command(resilience, OperationType::Write, response) else {
                return;
            };
            let result = control_plane.renew_lease(&caller, command).await;
            resilience.record_result(&result);
            publish_revision_events_from_renewal(control_plane.store(), publisher, &result).await;
            let _ = response.send(result);
        }
        RuntimeCommand::DeregisterInstance {
            caller,
            namespace,
            environment,
            service_name,
            instance_id,
            now_ms,
            response,
        } => {
            let Some(response) = gate_command(resilience, OperationType::Write, response) else {
                return;
            };
            let result = control_plane
                .deregister_instance(
                    &caller,
                    &namespace,
                    &environment,
                    &service_name,
                    &instance_id,
                    now_ms,
                )
                .await;
            resilience.record_result(&result);
            publish_revision_events_from_deregister(control_plane.store(), publisher, &result)
                .await;
            let _ = response.send(result);
        }
        RuntimeCommand::ExpireInstances {
            now_ms,
            max_instances,
            response,
        } => {
            let Some(response) = gate_command(resilience, OperationType::Write, response) else {
                return;
            };
            let result = control_plane.expire_instances(now_ms, max_instances).await;
            resilience.record_result(&result);
            publish_revision_events_from_deregisters(control_plane.store(), publisher, &result)
                .await;
            let _ = response.send(result);
        }
        RuntimeCommand::ReportInstanceStatus {
            caller,
            command,
            response,
        } => {
            let Some(response) = gate_command(resilience, OperationType::Write, response) else {
                return;
            };
            let event_scope = (command.namespace.clone(), command.environment.clone());
            let result = control_plane.report_instance_status(&caller, command).await;
            resilience.record_result(&result);
            publish_scoped_revision_events(control_plane.store(), publisher, event_scope, &result)
                .await;
            let _ = response.send(result);
        }
        RuntimeCommand::DiscoverInstances {
            caller,
            query,
            now_ms,
            response,
        } => {
            let Some(response) = gate_command(resilience, OperationType::Read, response) else {
                return;
            };
            let cache_key = discover_cache_key(&query);
            let result = control_plane
                .discover_instances(&caller, query, now_ms)
                .await;
            let result = resilience.resolve_discover_instances(cache_key, result);
            resilience.record_result(&result);
            let _ = response.send(result);
        }
        RuntimeCommand::RetrieveInstance {
            caller,
            query,
            now_ms,
            response,
        } => {
            let Some(response) = gate_command(resilience, OperationType::Read, response) else {
                return;
            };
            let cache_key = retrieve_instance_cache_key(&query);
            let result = control_plane
                .retrieve_instance(&caller, query, now_ms)
                .await;
            let result = resilience.resolve_retrieve_instance(cache_key, result);
            resilience.record_result(&result);
            let _ = response.send(result);
        }
        RuntimeCommand::ListServices {
            caller,
            query,
            now_ms,
            response,
        } => {
            let Some(response) = gate_command(resilience, OperationType::Read, response) else {
                return;
            };
            let cache_key = list_services_cache_key(&query);
            let result = control_plane.list_services(&caller, query, now_ms).await;
            let result = resilience.resolve_list_services(cache_key, result);
            resilience.record_result(&result);
            let _ = response.send(result);
        }
        RuntimeCommand::CreateConfigDraft {
            caller,
            command,
            response,
        } => {
            let Some(response) = gate_command(resilience, OperationType::Write, response) else {
                return;
            };
            let result = control_plane.create_config_draft(&caller, command).await;
            resilience.record_result(&result);
            let _ = response.send(result);
        }
        RuntimeCommand::PublishConfig {
            caller,
            command,
            response,
        } => {
            let Some(response) = gate_command(resilience, OperationType::Write, response) else {
                return;
            };
            let result = control_plane.publish_config(&caller, command).await;
            resilience.record_result(&result);
            if let Ok(ref release) = result {
                TracingComponentChangeEmitter.emit_config_changed(
                    &release.namespace,
                    &release.environment,
                    release.revision,
                );
            }
            publish_revision_events_from_config_release(control_plane.store(), publisher, &result)
                .await;
            let _ = response.send(result);
        }
        RuntimeCommand::RollbackConfig {
            caller,
            command,
            response,
        } => {
            let Some(response) = gate_command(resilience, OperationType::Write, response) else {
                return;
            };
            let result = control_plane.rollback_config(&caller, command).await;
            resilience.record_result(&result);
            if let Ok(ref release) = result {
                TracingComponentChangeEmitter.emit_config_changed(
                    &release.namespace,
                    &release.environment,
                    release.revision,
                );
            }
            publish_revision_events_from_config_release(control_plane.store(), publisher, &result)
                .await;
            let _ = response.send(result);
        }
        RuntimeCommand::RetrieveEffectiveConfig {
            caller,
            query,
            response,
        } => {
            let Some(response) = gate_command(resilience, OperationType::Read, response) else {
                return;
            };
            let cache_key = effective_config_cache_key(&query);
            let result = control_plane
                .retrieve_effective_config(&caller, query)
                .await;
            let result = resilience.resolve_effective_config(cache_key, result);
            resilience.record_result(&result);
            let _ = response.send(result);
        }
        RuntimeCommand::WatchRegistryEvents {
            caller,
            query,
            response,
        } => {
            let Some(response) = gate_command(resilience, OperationType::Read, response) else {
                return;
            };
            let cache_key = watch_events_cache_key(&query);
            let result = control_plane.watch_registry_events(&caller, query).await;
            let result = resilience.resolve_watch_events(cache_key, result);
            resilience.record_result(&result);
            let _ = response.send(result);
        }
        RuntimeCommand::WatchConfigEvents {
            caller,
            query,
            response,
        } => {
            let Some(response) = gate_command(resilience, OperationType::Read, response) else {
                return;
            };
            let cache_key = watch_events_cache_key(&query);
            let result = control_plane.watch_config_events(&caller, query).await;
            let result = resilience.resolve_watch_events(cache_key, result);
            resilience.record_result(&result);
            let _ = response.send(result);
        }
    }
}

fn gate_command<T>(
    resilience: &mut RuntimeResilience,
    operation: OperationType,
    response: oneshot::Sender<DiscoveryResult<T>>,
) -> Option<oneshot::Sender<DiscoveryResult<T>>> {
    match resilience.gate(operation) {
        Ok(()) => Some(response),
        Err(error) => {
            let _ = response.send(Err(error));
            None
        }
    }
}

fn discover_cache_key(query: &DiscoverInstancesQuery) -> String {
    let mut key = format!(
        "discover:{}:{}:{}:healthy={}:protocol={:?}",
        query.namespace, query.environment, query.service_name, query.healthy_only, query.protocol
    );
    for filter in &query.label_filters {
        key.push_str(&format!(
            ":lf:{}:{:?}:{}",
            filter.key, filter.op, filter.value
        ));
    }
    if let Some(sort_by) = query.sort_by {
        key.push_str(&format!(":sort:{sort_by:?}"));
    }
    key
}

fn retrieve_instance_cache_key(query: &RetrieveInstanceQuery) -> String {
    format!(
        "retrieve:{}:{}:{}:{}",
        query.namespace, query.environment, query.service_name, query.instance_id
    )
}

fn list_services_cache_key(query: &ListServicesQuery) -> String {
    format!("list:{}:{}", query.namespace, query.environment)
}

fn effective_config_cache_key(query: &RetrieveEffectiveConfigQuery) -> String {
    format!(
        "config:{}:{}:{}:{}:{}",
        query.namespace, query.environment, query.application, query.service_name, query.group
    )
}

fn watch_events_cache_key(query: &WatchEventsQuery) -> String {
    format!(
        "watch:{}:{}:from={}:max={}:service={:?}:group={:?}:application={:?}",
        query.namespace,
        query.environment,
        query.from_revision,
        query.max_events,
        query.service_name,
        query.config_group,
        query.config_application,
    )
}

async fn run_expiry_scan<S>(
    control_plane: &mut DiscoveryControlPlane<S>,
    publisher: &WatchEventPublisher,
    now_ms: u64,
    max_instances: usize,
) where
    S: ConfigStore + RegistryStore + WatchEventStore,
{
    let result = control_plane.expire_instances(now_ms, max_instances).await;
    publish_revision_events_from_deregisters(control_plane.store(), publisher, &result).await;
}

async fn run_event_gc<S>(
    control_plane: &mut DiscoveryControlPlane<S>,
    retention_count: u64,
    batch_size: usize,
) where
    S: ConfigStore + RegistryStore + WatchEventStore,
{
    if let Ok(current_revision) = control_plane.store().current_revision().await {
        let before_revision = current_revision.saturating_sub(retention_count);
        let _ = control_plane
            .gc_watch_events(before_revision, batch_size)
            .await;
    }
}

async fn publish_scoped_revision_events<S, T>(
    store: &S,
    publisher: &WatchEventPublisher,
    event_scope: (String, String),
    result: &DiscoveryResult<T>,
) where
    S: WatchEventStore,
    T: HasRevision,
{
    if let Ok(result) = result {
        publish_events_for_revision(store, publisher, event_scope, result.revision()).await;
    }
}

async fn publish_revision_events_from_renewal<S>(
    store: &S,
    publisher: &WatchEventPublisher,
    result: &DiscoveryResult<RenewLeaseResult>,
) where
    S: WatchEventStore,
{
    if let Ok(result) = result {
        publish_events_for_revision(
            store,
            publisher,
            (result.namespace.clone(), result.environment.clone()),
            result.revision,
        )
        .await;
    }
}

async fn publish_revision_events_from_deregister<S>(
    store: &S,
    publisher: &WatchEventPublisher,
    result: &DiscoveryResult<sdkwork_discovery_contract::DeregisterInstanceResult>,
) where
    S: WatchEventStore,
{
    if let Ok(result) = result {
        if !result.deregistered {
            return;
        }
        publish_events_for_revision(
            store,
            publisher,
            (result.namespace.clone(), result.environment.clone()),
            result.revision,
        )
        .await;
    }
}

async fn publish_revision_events_from_deregisters<S>(
    store: &S,
    publisher: &WatchEventPublisher,
    result: &DiscoveryResult<Vec<DeregisterInstanceResult>>,
) where
    S: WatchEventStore,
{
    if let Ok(results) = result {
        for result in results {
            if !result.deregistered {
                continue;
            }
            publish_events_for_revision(
                store,
                publisher,
                (result.namespace.clone(), result.environment.clone()),
                result.revision,
            )
            .await;
        }
    }
}

async fn publish_revision_events_from_config_release<S>(
    store: &S,
    publisher: &WatchEventPublisher,
    result: &DiscoveryResult<ConfigRelease>,
) where
    S: WatchEventStore,
{
    if let Ok(release) = result {
        publish_events_for_revision(
            store,
            publisher,
            (release.namespace.clone(), release.environment.clone()),
            release.revision,
        )
        .await;
    }
}

async fn publish_events_for_revision<S>(
    store: &S,
    publisher: &WatchEventPublisher,
    event_scope: (String, String),
    revision: u64,
) where
    S: WatchEventStore,
{
    if let Ok(events) = events_for_revision(store, &event_scope.0, &event_scope.1, revision).await {
        for event in events {
            publisher.publish(event);
        }
    }
}

trait HasRevision {
    fn revision(&self) -> u64;
}

impl HasRevision for RegisterInstanceResult {
    fn revision(&self) -> u64 {
        self.revision
    }
}

impl HasRevision for ReportInstanceStatusResult {
    fn revision(&self) -> u64 {
        self.revision
    }
}
