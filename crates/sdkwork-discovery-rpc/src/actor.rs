use sdkwork_discovery_contract::{
    CallerContext, ConfigDraft, ConfigRelease, CreateConfigDraftCommand, DeregisterInstanceResult,
    DiscoverInstancesQuery, DiscoverInstancesResult, DiscoveryError, DiscoveryEvent,
    DiscoveryResult, EffectiveConfig, ListServicesQuery, ListServicesResult, PublishConfigCommand,
    RegisterInstanceCommand, RegisterInstanceResult, RenewLeaseCommand, RenewLeaseResult,
    ReportInstanceStatusCommand, ReportInstanceStatusResult, RetrieveEffectiveConfigQuery,
    RetrieveInstanceQuery, RollbackConfigCommand, ServiceInstance, WatchEventsQuery,
};
use sdkwork_discovery_core::DiscoveryControlPlane;
use sdkwork_discovery_storage_contract::{ConfigStore, RegistryStore, WatchEventStore};
use tokio::sync::{mpsc, oneshot};
use tokio::time::{Duration, Instant, MissedTickBehavior};

use crate::context::RpcContextPolicy;
use crate::service_token::{DiscoveryRpcServiceTokenVerifierConfig, ServiceTokenVerifier};
use crate::watch::{events_for_revision, WatchEventPublisher, WatchEventSubscriber};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryRpcRuntimeConfig {
    pub registry_expiry_scan_interval_ms: u64,
    pub registry_expiry_scan_batch_size: usize,
    pub allow_unsigned_local_context: bool,
    pub service_token_verifier: Option<DiscoveryRpcServiceTokenVerifierConfig>,
}

impl Default for DiscoveryRpcRuntimeConfig {
    fn default() -> Self {
        Self {
            registry_expiry_scan_interval_ms: 0,
            registry_expiry_scan_batch_size: 1_000,
            allow_unsigned_local_context: true,
            service_token_verifier: None,
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
    if config.registry_expiry_scan_interval_ms == 0 {
        while let Some(command) = receiver.recv().await {
            dispatch_command(control_plane, publisher, command).await;
        }
        return;
    }

    let scan_interval = Duration::from_millis(config.registry_expiry_scan_interval_ms);
    let mut interval = tokio::time::interval_at(Instant::now() + scan_interval, scan_interval);
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            command = receiver.recv() => {
                let Some(command) = command else {
                    return;
                };
                dispatch_command(control_plane, publisher, command).await;
            }
            _ = interval.tick() => {
                run_expiry_scan(
                    control_plane,
                    publisher,
                    crate::codec::now_millis(),
                    config.registry_expiry_scan_batch_size,
                ).await;
            }
        }
    }
}

async fn dispatch_command<S>(
    control_plane: &mut DiscoveryControlPlane<S>,
    publisher: &WatchEventPublisher,
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
            let event_scope = (command.namespace.clone(), command.environment.clone());
            let result = control_plane.register_instance(&caller, command).await;
            publish_scoped_revision_events(control_plane.store(), publisher, event_scope, &result)
                .await;
            let _ = response.send(result);
        }
        RuntimeCommand::RenewLease {
            caller,
            command,
            response,
        } => {
            let result = control_plane.renew_lease(&caller, command).await;
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
            publish_revision_events_from_deregister(control_plane.store(), publisher, &result)
                .await;
            let _ = response.send(result);
        }
        RuntimeCommand::ExpireInstances {
            now_ms,
            max_instances,
            response,
        } => {
            let result = control_plane.expire_instances(now_ms, max_instances).await;
            publish_revision_events_from_deregisters(control_plane.store(), publisher, &result)
                .await;
            let _ = response.send(result);
        }
        RuntimeCommand::ReportInstanceStatus {
            caller,
            command,
            response,
        } => {
            let event_scope = (command.namespace.clone(), command.environment.clone());
            let result = control_plane.report_instance_status(&caller, command).await;
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
            let _ = response.send(
                control_plane
                    .discover_instances(&caller, query, now_ms)
                    .await,
            );
        }
        RuntimeCommand::RetrieveInstance {
            caller,
            query,
            now_ms,
            response,
        } => {
            let _ = response.send(
                control_plane
                    .retrieve_instance(&caller, query, now_ms)
                    .await,
            );
        }
        RuntimeCommand::ListServices {
            caller,
            query,
            now_ms,
            response,
        } => {
            let _ = response.send(control_plane.list_services(&caller, query, now_ms).await);
        }
        RuntimeCommand::CreateConfigDraft {
            caller,
            command,
            response,
        } => {
            let _ = response.send(control_plane.create_config_draft(&caller, command).await);
        }
        RuntimeCommand::PublishConfig {
            caller,
            command,
            response,
        } => {
            let result = control_plane.publish_config(&caller, command).await;
            publish_revision_events_from_config_release(control_plane.store(), publisher, &result)
                .await;
            let _ = response.send(result);
        }
        RuntimeCommand::RollbackConfig {
            caller,
            command,
            response,
        } => {
            let result = control_plane.rollback_config(&caller, command).await;
            publish_revision_events_from_config_release(control_plane.store(), publisher, &result)
                .await;
            let _ = response.send(result);
        }
        RuntimeCommand::RetrieveEffectiveConfig {
            caller,
            query,
            response,
        } => {
            let _ = response.send(
                control_plane
                    .retrieve_effective_config(&caller, query)
                    .await,
            );
        }
        RuntimeCommand::WatchRegistryEvents {
            caller,
            query,
            response,
        } => {
            let _ = response.send(control_plane.watch_registry_events(&caller, query).await);
        }
        RuntimeCommand::WatchConfigEvents {
            caller,
            query,
            response,
        } => {
            let _ = response.send(control_plane.watch_config_events(&caller, query).await);
        }
    }
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
