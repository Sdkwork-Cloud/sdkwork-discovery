use sdkwork_discovery_contract::{
    finalize_discover_instances, DiscoverInstancesQuery, DiscoverSortBy, InstanceStatus,
    LabelFilter, LabelFilterOp, ServiceInstance,
};

fn instance(instance_id: &str, role: &str, weight: u32) -> ServiceInstance {
    ServiceInstance {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        service_name: "demo".to_string(),
        instance_id: instance_id.to_string(),
        endpoint: format!("grpc://127.0.0.1:{instance_id}"),
        protocol: "grpc".to_string(),
        version: "0.1.0".to_string(),
        region: "local".to_string(),
        zone: "local-a".to_string(),
        weight,
        priority: 0,
        status: InstanceStatus::Serving,
        metadata: [("role".to_string(), role.to_string())]
            .into_iter()
            .collect(),
        lease_id: format!("lease-{instance_id}"),
        expires_at_ms: u64::MAX,
        revision: 1,
        health_check: None,
        health_check_state: Default::default(),
    }
}

#[test]
fn finalize_discover_instances_applies_label_filters_and_sort_by_weight() {
    let query = DiscoverInstancesQuery {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        service_name: "demo".to_string(),
        healthy_only: true,
        protocol: None,
        label_filters: vec![LabelFilter {
            key: "role".to_string(),
            op: LabelFilterOp::Eq,
            value: "primary".to_string(),
        }],
        sort_by: Some(DiscoverSortBy::Weight),
    };

    let result = finalize_discover_instances(
        vec![
            instance("a", "primary", 10),
            instance("b", "secondary", 100),
            instance("c", "primary", 50),
        ],
        &query,
        7,
    );

    assert_eq!(result.revision, 7);
    assert_eq!(result.instances.len(), 2);
    assert_eq!(result.instances[0].instance_id, "c");
    assert_eq!(result.instances[1].instance_id, "a");
}
