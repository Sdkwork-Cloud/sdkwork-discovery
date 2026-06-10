use sdkwork_discovery_contract::{
    DiscoverInstancesQuery, InstanceStatus, ListServicesQuery, RegisterInstanceCommand,
};
use sdkwork_discovery_storage_contract::RegistryStore;
use sdkwork_discovery_storage_memory::MemoryDiscoveryStore;

fn command() -> RegisterInstanceCommand {
    RegisterInstanceCommand {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        service_name: "sdkwork-drive-product".to_string(),
        instance_id: "drive-1".to_string(),
        endpoint: "grpc://127.0.0.1:50051".to_string(),
        protocol: "grpc".to_string(),
        version: "0.1.0".to_string(),
        region: "local".to_string(),
        zone: "local-a".to_string(),
        weight: 100,
        priority: 0,
        status: InstanceStatus::Serving,
        metadata: Default::default(),
        lease_ttl_seconds: 30,
        now_ms: 1_000,
    }
}

#[tokio::test]
async fn memory_registry_store_contract_is_async() {
    let mut store = MemoryDiscoveryStore::new();

    let registered = store.register_instance(command()).await.unwrap();
    let discovered = store
        .discover_instances(
            DiscoverInstancesQuery {
                namespace: "sdkwork".to_string(),
                environment: "development".to_string(),
                service_name: "sdkwork-drive-product".to_string(),
                healthy_only: true,
                protocol: Some("grpc".to_string()),
            },
            2_000,
        )
        .await
        .unwrap();
    let services = store
        .list_services(
            ListServicesQuery {
                namespace: "sdkwork".to_string(),
                environment: "development".to_string(),
            },
            2_000,
        )
        .await
        .unwrap();

    assert_eq!(registered.revision, 1);
    assert_eq!(discovered.instances.len(), 1);
    assert_eq!(services.services.len(), 1);
}
