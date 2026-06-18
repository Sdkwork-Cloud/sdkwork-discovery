use async_trait::async_trait;
use sdkwork_discovery_contract::{
    BatchRegisterResult, DeregisterInstanceResult, DiscoverInstancesQuery, DiscoverInstancesResult,
    DiscoveryResult, ListServicesQuery, ListServicesResult, RegisterInstanceCommand,
    RegisterInstanceResult, RenewLeaseCommand, RenewLeaseResult, ReportInstanceStatusCommand,
    ReportInstanceStatusResult, RetrieveInstanceQuery, ServiceInstance,
};

#[async_trait]
pub trait RegistryStore {
    async fn current_revision(&self) -> DiscoveryResult<u64>;

    async fn register_instance(
        &mut self,
        command: RegisterInstanceCommand,
    ) -> DiscoveryResult<RegisterInstanceResult>;

    async fn batch_register_instances(
        &mut self,
        commands: Vec<RegisterInstanceCommand>,
    ) -> DiscoveryResult<BatchRegisterResult>;

    async fn renew_lease(
        &mut self,
        command: RenewLeaseCommand,
    ) -> DiscoveryResult<RenewLeaseResult>;

    async fn report_instance_status(
        &mut self,
        command: ReportInstanceStatusCommand,
    ) -> DiscoveryResult<ReportInstanceStatusResult>;

    async fn deregister_instance(
        &mut self,
        namespace: &str,
        environment: &str,
        service_name: &str,
        instance_id: &str,
        now_ms: u64,
    ) -> DiscoveryResult<DeregisterInstanceResult>;

    async fn batch_deregister_instances(
        &mut self,
        namespace: &str,
        environment: &str,
        service_name: &str,
        instance_ids: Vec<String>,
        now_ms: u64,
    ) -> DiscoveryResult<Vec<DeregisterInstanceResult>>;

    async fn expire_instances(
        &mut self,
        now_ms: u64,
        max_instances: usize,
    ) -> DiscoveryResult<Vec<DeregisterInstanceResult>>;

    async fn discover_instances(
        &self,
        query: DiscoverInstancesQuery,
        now_ms: u64,
    ) -> DiscoveryResult<DiscoverInstancesResult>;

    async fn retrieve_instance(
        &self,
        query: RetrieveInstanceQuery,
        now_ms: u64,
    ) -> DiscoveryResult<Option<ServiceInstance>>;

    async fn list_services(
        &self,
        query: ListServicesQuery,
        now_ms: u64,
    ) -> DiscoveryResult<ListServicesResult>;

    async fn list_active_instances_with_health_check(
        &self,
        now_ms: u64,
    ) -> DiscoveryResult<Vec<ServiceInstance>>;

    async fn update_health_check_state(
        &mut self,
        namespace: &str,
        environment: &str,
        service_name: &str,
        instance_id: &str,
        state: sdkwork_discovery_contract::HealthCheckRuntimeState,
    ) -> DiscoveryResult<()>;
}
