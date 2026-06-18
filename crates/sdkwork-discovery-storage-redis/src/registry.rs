use async_trait::async_trait;
use sdkwork_discovery_contract::{
    BatchRegisterResult, DeregisterInstanceResult, DiscoverInstancesQuery, DiscoverInstancesResult,
    DiscoveryResult, ListServicesQuery, ListServicesResult, RegisterInstanceCommand,
    RegisterInstanceResult, RenewLeaseCommand, RenewLeaseResult, ReportInstanceStatusCommand,
    ReportInstanceStatusResult, RetrieveInstanceQuery, ServiceInstance,
};
use sdkwork_discovery_storage_contract::RegistryStore;

use crate::store::RedisDiscoveryStore;

#[async_trait]
impl RegistryStore for RedisDiscoveryStore {
    async fn current_revision(&self) -> DiscoveryResult<u64> {
        self.hydrate_once().await?;
        self.memory.lock().await.current_revision().await
    }

    async fn register_instance(
        &mut self,
        command: RegisterInstanceCommand,
    ) -> DiscoveryResult<RegisterInstanceResult> {
        self.hydrate_once().await?;
        let result = self.memory.lock().await.register_instance(command).await?;
        self.persist_state().await?;
        Ok(result)
    }

    async fn batch_register_instances(
        &mut self,
        commands: Vec<RegisterInstanceCommand>,
    ) -> DiscoveryResult<BatchRegisterResult> {
        self.hydrate_once().await?;
        let result = self
            .memory
            .lock()
            .await
            .batch_register_instances(commands)
            .await?;
        self.persist_state().await?;
        Ok(result)
    }

    async fn renew_lease(
        &mut self,
        command: RenewLeaseCommand,
    ) -> DiscoveryResult<RenewLeaseResult> {
        self.hydrate_once().await?;
        let result = self.memory.lock().await.renew_lease(command).await?;
        self.persist_state().await?;
        Ok(result)
    }

    async fn report_instance_status(
        &mut self,
        command: ReportInstanceStatusCommand,
    ) -> DiscoveryResult<ReportInstanceStatusResult> {
        self.hydrate_once().await?;
        let result = self
            .memory
            .lock()
            .await
            .report_instance_status(command)
            .await?;
        self.persist_state().await?;
        Ok(result)
    }

    async fn deregister_instance(
        &mut self,
        namespace: &str,
        environment: &str,
        service_name: &str,
        instance_id: &str,
        now_ms: u64,
    ) -> DiscoveryResult<DeregisterInstanceResult> {
        self.hydrate_once().await?;
        let result = self
            .memory
            .lock()
            .await
            .deregister_instance(namespace, environment, service_name, instance_id, now_ms)
            .await?;
        self.persist_state().await?;
        Ok(result)
    }

    async fn batch_deregister_instances(
        &mut self,
        namespace: &str,
        environment: &str,
        service_name: &str,
        instance_ids: Vec<String>,
        now_ms: u64,
    ) -> DiscoveryResult<Vec<DeregisterInstanceResult>> {
        self.hydrate_once().await?;
        let result = self
            .memory
            .lock()
            .await
            .batch_deregister_instances(namespace, environment, service_name, instance_ids, now_ms)
            .await?;
        self.persist_state().await?;
        Ok(result)
    }

    async fn expire_instances(
        &mut self,
        now_ms: u64,
        max_instances: usize,
    ) -> DiscoveryResult<Vec<DeregisterInstanceResult>> {
        self.hydrate_once().await?;
        let result = self
            .memory
            .lock()
            .await
            .expire_instances(now_ms, max_instances)
            .await?;
        self.persist_state().await?;
        Ok(result)
    }

    async fn discover_instances(
        &self,
        query: DiscoverInstancesQuery,
        now_ms: u64,
    ) -> DiscoveryResult<DiscoverInstancesResult> {
        self.hydrate_once().await?;
        self.memory
            .lock()
            .await
            .discover_instances(query, now_ms)
            .await
    }

    async fn retrieve_instance(
        &self,
        query: RetrieveInstanceQuery,
        now_ms: u64,
    ) -> DiscoveryResult<Option<ServiceInstance>> {
        self.hydrate_once().await?;
        self.memory
            .lock()
            .await
            .retrieve_instance(query, now_ms)
            .await
    }

    async fn list_services(
        &self,
        query: ListServicesQuery,
        now_ms: u64,
    ) -> DiscoveryResult<ListServicesResult> {
        self.hydrate_once().await?;
        self.memory.lock().await.list_services(query, now_ms).await
    }

    async fn list_active_instances_with_health_check(
        &self,
        now_ms: u64,
    ) -> DiscoveryResult<Vec<ServiceInstance>> {
        self.hydrate_once().await?;
        self.memory
            .lock()
            .await
            .list_active_instances_with_health_check(now_ms)
            .await
    }

    async fn update_health_check_state(
        &mut self,
        namespace: &str,
        environment: &str,
        service_name: &str,
        instance_id: &str,
        state: sdkwork_discovery_contract::HealthCheckRuntimeState,
    ) -> DiscoveryResult<()> {
        self.hydrate_once().await?;
        self.memory
            .lock()
            .await
            .update_health_check_state(namespace, environment, service_name, instance_id, state)
            .await?;
        self.persist_state().await
    }
}
