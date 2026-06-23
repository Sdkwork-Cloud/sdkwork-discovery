use sdkwork_discovery_contract::{DiscoverInstancesQuery, InstanceStatus, RegisterInstanceCommand};
use sdkwork_discovery_storage_contract::RegistryStore;
use sdkwork_discovery_storage_redis::RedisDiscoveryStore;

fn register_command(instance_id: &str, endpoint: &str, now_ms: u64) -> RegisterInstanceCommand {
    RegisterInstanceCommand {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        service_name: "sdkwork-drive-product".to_string(),
        instance_id: instance_id.to_string(),
        endpoint: endpoint.to_string(),
        protocol: "grpc".to_string(),
        version: "0.1.0".to_string(),
        region: "local".to_string(),
        zone: "local-a".to_string(),
        weight: 100,
        priority: 0,
        status: InstanceStatus::Serving,
        metadata: [("role".to_string(), "primary".to_string())]
            .into_iter()
            .collect(),
        lease_ttl_seconds: 30,
        now_ms,
        expected_revision: None,
        persistent: false,
        health_check: None,
    }
}

#[tokio::test]
async fn redis_delegate_registers_and_discovers_instances_without_external_server() {
    let mut store = RedisDiscoveryStore::new_in_memory_delegate();

    store
        .register_instance(register_command("drive-1", "grpc://127.0.0.1:50051", 1_000))
        .await
        .unwrap();

    let active = store
        .discover_instances(
            DiscoverInstancesQuery {
                namespace: "sdkwork".to_string(),
                environment: "development".to_string(),
                service_name: "sdkwork-drive-product".to_string(),
                healthy_only: true,
                protocol: Some("grpc".to_string()),
                label_filters: vec![],
                sort_by: None,
                page_size: 0,
                page_token: None,
            },
            2_500,
        )
        .await
        .unwrap();

    assert_eq!(active.instances.len(), 1);
    assert_eq!(active.instances[0].endpoint, "grpc://127.0.0.1:50051");
}
