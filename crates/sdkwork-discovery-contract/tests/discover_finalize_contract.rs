use sdkwork_discovery_contract::{
    finalize_discover_instances, finalize_list_services, DiscoverInstancesQuery, DiscoverSortBy,
    InstanceStatus, LabelFilter, LabelFilterOp, ListServicesQuery, ServiceInstance, ServiceSummary,
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
        page_size: 0,
        page_token: None,
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

#[test]
fn finalize_discover_instances_paginates_sorted_results() {
    let query = DiscoverInstancesQuery {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        service_name: "demo".to_string(),
        healthy_only: true,
        protocol: None,
        label_filters: vec![],
        sort_by: Some(DiscoverSortBy::Weight),
        page_size: 2,
        page_token: None,
    };

    let first = finalize_discover_instances(
        vec![
            instance("a", "primary", 10),
            instance("b", "primary", 50),
            instance("c", "primary", 100),
        ],
        &query,
        3,
    );
    assert_eq!(first.instances.len(), 2);
    assert_eq!(first.instances[0].instance_id, "c");
    assert_eq!(first.next_page_token.as_deref(), Some("b"));

    let second = finalize_discover_instances(
        vec![
            instance("a", "primary", 10),
            instance("b", "primary", 50),
            instance("c", "primary", 100),
        ],
        &DiscoverInstancesQuery {
            page_token: first.next_page_token.clone(),
            ..query
        },
        3,
    );
    assert_eq!(second.instances.len(), 1);
    assert_eq!(second.instances[0].instance_id, "a");
    assert_eq!(second.next_page_token, None);
}

#[test]
fn finalize_list_services_paginates_sorted_service_names() {
    let query = ListServicesQuery {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        page_size: 2,
        page_token: None,
    };
    let summaries = vec![
        ServiceSummary {
            namespace: "sdkwork".to_string(),
            environment: "development".to_string(),
            service_name: "alpha".to_string(),
            active_instance_count: 1,
            latest_revision: 1,
        },
        ServiceSummary {
            namespace: "sdkwork".to_string(),
            environment: "development".to_string(),
            service_name: "beta".to_string(),
            active_instance_count: 2,
            latest_revision: 2,
        },
        ServiceSummary {
            namespace: "sdkwork".to_string(),
            environment: "development".to_string(),
            service_name: "gamma".to_string(),
            active_instance_count: 1,
            latest_revision: 3,
        },
    ];

    let first = finalize_list_services(summaries.clone(), 9, &query);
    assert_eq!(first.services.len(), 2);
    assert_eq!(first.services[0].service_name, "alpha");
    assert_eq!(first.next_page_token.as_deref(), Some("beta"));

    let second = finalize_list_services(
        summaries,
        9,
        &ListServicesQuery {
            page_token: first.next_page_token.clone(),
            ..query
        },
    );
    assert_eq!(second.services.len(), 1);
    assert_eq!(second.services[0].service_name, "gamma");
    assert_eq!(second.next_page_token, None);
}
