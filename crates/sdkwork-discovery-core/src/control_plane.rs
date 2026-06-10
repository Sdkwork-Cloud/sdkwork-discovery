use sdkwork_discovery_contract::{
    CallerContext, ConfigDraft, ConfigPermission, ConfigRelease, CreateConfigDraftCommand,
    DeregisterInstanceResult, DiscoverInstancesQuery, DiscoverInstancesResult, DiscoveryEvent,
    DiscoveryResult, EffectiveConfig, ListServicesQuery, ListServicesResult, PublishConfigCommand,
    RegisterInstanceCommand, RegisterInstanceResult, RegistryPermission, RenewLeaseCommand,
    RenewLeaseResult, ReportInstanceStatusCommand, ReportInstanceStatusResult,
    RetrieveEffectiveConfigQuery, RetrieveInstanceQuery, RollbackConfigCommand, ServiceInstance,
    WatchEventsQuery,
};
use sdkwork_discovery_storage_contract::{ConfigStore, RegistryStore, WatchEventStore};

use crate::permissions::{require_config_permission, require_registry_permission};
use crate::policy::{
    require_config_registry_enabled, validate_config_policy, validate_effective_config_read_policy,
    validate_registry_lease_ttl, ConfigPolicy, RegistryPolicy,
};

pub struct DiscoveryControlPlane<S> {
    store: S,
    config_policy: ConfigPolicy,
    registry_policy: RegistryPolicy,
}

impl<S> DiscoveryControlPlane<S>
where
    S: ConfigStore + RegistryStore,
{
    pub fn new(store: S, config_policy: ConfigPolicy, registry_policy: RegistryPolicy) -> Self {
        Self {
            store,
            config_policy,
            registry_policy,
        }
    }

    pub fn store(&self) -> &S {
        &self.store
    }

    pub async fn create_config_draft(
        &mut self,
        caller: &CallerContext,
        command: CreateConfigDraftCommand,
    ) -> DiscoveryResult<ConfigDraft> {
        require_config_permission(caller, ConfigPermission::Publish)?;
        require_config_registry_enabled(&self.config_policy)?;
        validate_config_policy(&self.config_policy, &command.format, &command.value)?;
        self.store.create_config_draft(command).await
    }

    pub async fn publish_config(
        &mut self,
        caller: &CallerContext,
        command: PublishConfigCommand,
    ) -> DiscoveryResult<ConfigRelease> {
        require_config_permission(caller, ConfigPermission::Publish)?;
        require_config_registry_enabled(&self.config_policy)?;
        self.store.publish_config(command).await
    }

    pub async fn rollback_config(
        &mut self,
        caller: &CallerContext,
        command: RollbackConfigCommand,
    ) -> DiscoveryResult<ConfigRelease> {
        require_config_permission(caller, ConfigPermission::Rollback)?;
        require_config_registry_enabled(&self.config_policy)?;
        self.store.rollback_config(command).await
    }

    pub async fn retrieve_effective_config(
        &self,
        caller: &CallerContext,
        query: RetrieveEffectiveConfigQuery,
    ) -> DiscoveryResult<EffectiveConfig> {
        require_config_permission(caller, ConfigPermission::Read)?;
        require_config_registry_enabled(&self.config_policy)?;
        let effective = self.store.retrieve_effective_config(query).await?;
        validate_effective_config_read_policy(&self.config_policy, &effective)?;
        Ok(effective)
    }

    pub async fn register_instance(
        &mut self,
        caller: &CallerContext,
        command: RegisterInstanceCommand,
    ) -> DiscoveryResult<RegisterInstanceResult> {
        require_registry_permission(caller, RegistryPermission::Write)?;
        validate_registry_lease_ttl(&self.registry_policy, command.lease_ttl_seconds)?;
        self.store.register_instance(command).await
    }

    pub async fn report_instance_status(
        &mut self,
        caller: &CallerContext,
        command: ReportInstanceStatusCommand,
    ) -> DiscoveryResult<ReportInstanceStatusResult> {
        require_registry_permission(caller, RegistryPermission::Write)?;
        self.store.report_instance_status(command).await
    }

    pub async fn renew_lease(
        &mut self,
        caller: &CallerContext,
        command: RenewLeaseCommand,
    ) -> DiscoveryResult<RenewLeaseResult> {
        require_registry_permission(caller, RegistryPermission::Write)?;
        validate_registry_lease_ttl(&self.registry_policy, command.lease_ttl_seconds)?;
        self.store.renew_lease(command).await
    }

    pub async fn deregister_instance(
        &mut self,
        caller: &CallerContext,
        namespace: &str,
        environment: &str,
        service_name: &str,
        instance_id: &str,
        now_ms: u64,
    ) -> DiscoveryResult<DeregisterInstanceResult> {
        require_registry_permission(caller, RegistryPermission::Write)?;
        self.store
            .deregister_instance(namespace, environment, service_name, instance_id, now_ms)
            .await
    }

    pub async fn expire_instances(
        &mut self,
        now_ms: u64,
        max_instances: usize,
    ) -> DiscoveryResult<Vec<DeregisterInstanceResult>> {
        self.store.expire_instances(now_ms, max_instances).await
    }

    pub async fn discover_instances(
        &self,
        caller: &CallerContext,
        query: DiscoverInstancesQuery,
        now_ms: u64,
    ) -> DiscoveryResult<DiscoverInstancesResult> {
        require_registry_permission(caller, RegistryPermission::Read)?;
        self.store.discover_instances(query, now_ms).await
    }

    pub async fn retrieve_instance(
        &self,
        caller: &CallerContext,
        query: RetrieveInstanceQuery,
        now_ms: u64,
    ) -> DiscoveryResult<Option<ServiceInstance>> {
        require_registry_permission(caller, RegistryPermission::Read)?;
        self.store.retrieve_instance(query, now_ms).await
    }

    pub async fn list_services(
        &self,
        caller: &CallerContext,
        query: ListServicesQuery,
        now_ms: u64,
    ) -> DiscoveryResult<ListServicesResult> {
        require_registry_permission(caller, RegistryPermission::Read)?;
        self.store.list_services(query, now_ms).await
    }
}

impl<S> DiscoveryControlPlane<S>
where
    S: WatchEventStore,
{
    pub async fn watch_registry_events(
        &self,
        caller: &CallerContext,
        query: WatchEventsQuery,
    ) -> DiscoveryResult<Vec<DiscoveryEvent>> {
        require_registry_permission(caller, RegistryPermission::Read)?;
        self.store.watch_events(query).await
    }

    pub async fn watch_config_events(
        &self,
        caller: &CallerContext,
        query: WatchEventsQuery,
    ) -> DiscoveryResult<Vec<DiscoveryEvent>> {
        require_config_permission(caller, ConfigPermission::Read)?;
        require_config_registry_enabled(&self.config_policy)?;
        self.store.watch_events(query).await
    }
}
