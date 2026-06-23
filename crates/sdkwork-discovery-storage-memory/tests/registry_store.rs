use sdkwork_discovery_contract::{
    DiscoverInstancesQuery, DiscoveryError, InstanceStatus, ListServicesQuery,
    RegisterInstanceCommand, RenewLeaseCommand, ReportInstanceStatusCommand, RetrieveInstanceQuery,
    WatchEventsQuery,
};
use sdkwork_discovery_storage_contract::{RegistryStore, WatchEventStore};
use sdkwork_discovery_storage_memory::MemoryDiscoveryStore;

fn register_command(endpoint: &str, now_ms: u64, ttl_seconds: u64) -> RegisterInstanceCommand {
    RegisterInstanceCommand {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        service_name: "sdkwork-drive-product".to_string(),
        instance_id: "drive-1".to_string(),
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
        lease_ttl_seconds: ttl_seconds,
        now_ms,
        expected_revision: None,
        persistent: false,
        health_check: None,
    }
}

fn discovery_query() -> DiscoverInstancesQuery {
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
    }
}

fn retrieve_query(instance_id: &str) -> RetrieveInstanceQuery {
    RetrieveInstanceQuery {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        service_name: "sdkwork-drive-product".to_string(),
        instance_id: instance_id.to_string(),
    }
}

#[tokio::test]
async fn register_is_upsert_and_discovery_excludes_expired_instances() {
    let mut store = MemoryDiscoveryStore::new();

    let first = store
        .register_instance(register_command("grpc://127.0.0.1:50051", 1_000, 30))
        .await
        .unwrap();
    let second = store
        .register_instance(register_command("grpc://127.0.0.1:50052", 2_000, 30))
        .await
        .unwrap();

    assert_eq!(first.revision, 1);
    assert_eq!(second.revision, 2);
    assert_eq!(first.lease_id, second.lease_id);

    let active = store
        .discover_instances(discovery_query(), 2_500)
        .await
        .unwrap();
    assert_eq!(active.revision, 2);
    assert_eq!(active.instances.len(), 1);
    assert_eq!(active.instances[0].endpoint, "grpc://127.0.0.1:50052");

    let expired = store
        .discover_instances(discovery_query(), 33_000)
        .await
        .unwrap();
    assert!(expired.instances.is_empty());
}

#[tokio::test]
async fn retrieve_instance_returns_current_registered_instance_by_identity() {
    let mut store = MemoryDiscoveryStore::new();
    store
        .register_instance(register_command("grpc://127.0.0.1:50051", 1_000, 30))
        .await
        .unwrap();

    let instance = store
        .retrieve_instance(retrieve_query("drive-1"), 2_500)
        .await
        .unwrap()
        .expect("registered instance should be retrievable by identity");
    let missing = store
        .retrieve_instance(retrieve_query("missing-instance"), 2_500)
        .await
        .unwrap();

    assert_eq!(instance.namespace, "sdkwork");
    assert_eq!(instance.environment, "development");
    assert_eq!(instance.service_name, "sdkwork-drive-product");
    assert_eq!(instance.instance_id, "drive-1");
    assert_eq!(instance.endpoint, "grpc://127.0.0.1:50051");
    assert_eq!(instance.lease_id, "lease-1");
    assert!(missing.is_none());
}

#[tokio::test]
async fn retrieve_instance_excludes_expired_registered_instance_by_identity() {
    let mut store = MemoryDiscoveryStore::new();
    store
        .register_instance(register_command("grpc://127.0.0.1:50051", 1_000, 30))
        .await
        .unwrap();

    let active = store
        .retrieve_instance(retrieve_query("drive-1"), 2_500)
        .await
        .unwrap();
    let expired = store
        .retrieve_instance(retrieve_query("drive-1"), 31_001)
        .await
        .unwrap();

    assert!(active.is_some());
    assert!(expired.is_none());
}

#[tokio::test]
async fn renew_extends_lease_and_deregister_is_idempotent() {
    let mut store = MemoryDiscoveryStore::new();

    let registered = store
        .register_instance(register_command("grpc://127.0.0.1:50051", 1_000, 10))
        .await
        .unwrap();
    let renewed = store
        .renew_lease(RenewLeaseCommand {
            lease_id: registered.lease_id.clone(),
            lease_ttl_seconds: 20,
            now_ms: 5_000,
        })
        .await
        .unwrap();

    assert_eq!(renewed.expires_at_ms, 25_000);

    let active = store
        .discover_instances(discovery_query(), 16_000)
        .await
        .unwrap();
    assert_eq!(active.instances.len(), 1);

    store
        .deregister_instance(
            "sdkwork",
            "development",
            "sdkwork-drive-product",
            "drive-1",
            16_000,
        )
        .await
        .unwrap();
    store
        .deregister_instance(
            "sdkwork",
            "development",
            "sdkwork-drive-product",
            "drive-1",
            16_000,
        )
        .await
        .unwrap();

    let after = store
        .discover_instances(discovery_query(), 16_500)
        .await
        .unwrap();
    assert!(after.instances.is_empty());
}

#[tokio::test]
async fn deregister_instance_ignores_expired_instance_without_advancing_revision() {
    let mut store = MemoryDiscoveryStore::new();
    store
        .register_instance(register_command("grpc://127.0.0.1:50051", 1_000, 1))
        .await
        .unwrap();

    let expired_deregister = store
        .deregister_instance(
            "sdkwork",
            "development",
            "sdkwork-drive-product",
            "drive-1",
            2_001,
        )
        .await
        .unwrap();

    assert!(!expired_deregister.deregistered);
    assert_eq!(expired_deregister.revision, 0);

    let replacement = store
        .register_instance(register_command("grpc://127.0.0.1:50052", 2_500, 30))
        .await
        .unwrap();

    assert_eq!(replacement.revision, 2);
    assert_eq!(replacement.lease_id, "lease-2");
}

#[tokio::test]
async fn expire_instances_soft_deletes_expired_instances_and_emits_watch_events() {
    let mut store = MemoryDiscoveryStore::new();
    store
        .register_instance(register_command("grpc://127.0.0.1:50051", 1_000, 1))
        .await
        .unwrap();
    let mut still_active = register_command("grpc://127.0.0.1:50052", 1_000, 30);
    still_active.instance_id = "drive-2".to_string();
    store.register_instance(still_active).await.unwrap();

    let expired = store.expire_instances(2_001, 1_000).await.unwrap();
    let discovered = store
        .discover_instances(discovery_query(), 2_001)
        .await
        .unwrap();
    let events = store
        .watch_events(WatchEventsQuery {
            namespace: "sdkwork".to_string(),
            environment: "development".to_string(),
            from_revision: 2,
            service_name: Some("sdkwork-drive-product".to_string()),
            config_group: None,
            config_application: None,
            max_events: 1_024,
        })
        .await
        .unwrap();

    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].instance_id, "drive-1");
    assert_eq!(expired[0].revision, 3);
    assert!(expired[0].deregistered);
    assert_eq!(discovered.instances.len(), 1);
    assert_eq!(discovered.instances[0].instance_id, "drive-2");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].resource_id, "drive-1");
    assert_eq!(
        events[0].kind,
        sdkwork_discovery_contract::DiscoveryEventKind::InstanceDeregistered
    );
}

#[tokio::test]
async fn expire_instances_respects_batch_limit() {
    let mut store = MemoryDiscoveryStore::new();

    for instance_id in ["drive-1", "drive-2", "drive-3"] {
        let mut command = register_command("grpc://127.0.0.1:50051", 1_000, 1);
        command.instance_id = instance_id.to_string();
        store.register_instance(command).await.unwrap();
    }

    let first_batch = store.expire_instances(2_001, 2).await.unwrap();
    let second_batch = store.expire_instances(2_001, 2).await.unwrap();

    assert_eq!(first_batch.len(), 2);
    assert_eq!(first_batch[0].instance_id, "drive-1");
    assert_eq!(first_batch[1].instance_id, "drive-2");
    assert_eq!(second_batch.len(), 1);
    assert_eq!(second_batch[0].instance_id, "drive-3");
}

#[tokio::test]
async fn renew_lease_rejects_expired_lease() {
    let mut store = MemoryDiscoveryStore::new();

    let registered = store
        .register_instance(register_command("grpc://127.0.0.1:50051", 1_000, 1))
        .await
        .unwrap();

    assert_not_found_contains(
        store
            .renew_lease(RenewLeaseCommand {
                lease_id: registered.lease_id,
                lease_ttl_seconds: 30,
                now_ms: 2_001,
            })
            .await
            .unwrap_err(),
        "lease",
    );
}

#[tokio::test]
async fn register_instance_replaces_expired_lease_for_existing_identity() {
    let mut store = MemoryDiscoveryStore::new();

    let expired = store
        .register_instance(register_command("grpc://127.0.0.1:50051", 1_000, 1))
        .await
        .unwrap();
    let replacement = store
        .register_instance(register_command("grpc://127.0.0.1:50052", 2_001, 30))
        .await
        .unwrap();

    assert_ne!(replacement.lease_id, expired.lease_id);

    assert_not_found_contains(
        store
            .renew_lease(RenewLeaseCommand {
                lease_id: expired.lease_id,
                lease_ttl_seconds: 30,
                now_ms: 2_500,
            })
            .await
            .unwrap_err(),
        "lease",
    );
    let renewed = store
        .renew_lease(RenewLeaseCommand {
            lease_id: replacement.lease_id,
            lease_ttl_seconds: 30,
            now_ms: 2_500,
        })
        .await
        .unwrap();

    assert_eq!(renewed.expires_at_ms, 32_500);
}

#[tokio::test]
async fn renew_lease_rejects_blank_lease_id() {
    let mut store = MemoryDiscoveryStore::new();

    assert_invalid_argument_contains(
        store
            .renew_lease(RenewLeaseCommand {
                lease_id: " ".to_string(),
                lease_ttl_seconds: 20,
                now_ms: 5_000,
            })
            .await
            .unwrap_err(),
        "lease_id",
    );
}

#[tokio::test]
async fn register_instance_rejects_lease_ttl_millisecond_overflow() {
    let mut store = MemoryDiscoveryStore::new();

    assert_invalid_argument_contains(
        store
            .register_instance(register_command("grpc://127.0.0.1:50051", 1_000, u64::MAX))
            .await
            .unwrap_err(),
        "lease ttl",
    );
}

#[tokio::test]
async fn register_instance_rejects_blank_required_metadata_fields() {
    let mut store = MemoryDiscoveryStore::new();

    assert_invalid_argument_contains(
        store
            .register_instance(RegisterInstanceCommand {
                endpoint: " ".to_string(),
                ..register_command("grpc://127.0.0.1:50051", 1_000, 30)
            })
            .await
            .unwrap_err(),
        "endpoint",
    );
    assert_invalid_argument_contains(
        store
            .register_instance(RegisterInstanceCommand {
                protocol: " ".to_string(),
                ..register_command("grpc://127.0.0.1:50051", 1_000, 30)
            })
            .await
            .unwrap_err(),
        "protocol",
    );
    assert_invalid_argument_contains(
        store
            .register_instance(RegisterInstanceCommand {
                version: " ".to_string(),
                ..register_command("grpc://127.0.0.1:50051", 1_000, 30)
            })
            .await
            .unwrap_err(),
        "version",
    );
    assert_invalid_argument_contains(
        store
            .register_instance(RegisterInstanceCommand {
                region: " ".to_string(),
                ..register_command("grpc://127.0.0.1:50051", 1_000, 30)
            })
            .await
            .unwrap_err(),
        "region",
    );
    assert_invalid_argument_contains(
        store
            .register_instance(RegisterInstanceCommand {
                zone: " ".to_string(),
                ..register_command("grpc://127.0.0.1:50051", 1_000, 30)
            })
            .await
            .unwrap_err(),
        "zone",
    );
}

#[tokio::test]
async fn renew_lease_rejects_lease_expiration_overflow() {
    let mut store = MemoryDiscoveryStore::new();
    let registered = store
        .register_instance(register_command("grpc://127.0.0.1:50051", 1_000, 30))
        .await
        .unwrap();

    assert_invalid_argument_contains(
        store
            .renew_lease(RenewLeaseCommand {
                lease_id: registered.lease_id,
                lease_ttl_seconds: 1,
                now_ms: u64::MAX,
            })
            .await
            .unwrap_err(),
        "lease expiration",
    );
}

#[tokio::test]
async fn report_instance_status_updates_discoverability_and_revision() {
    let mut store = MemoryDiscoveryStore::new();
    store
        .register_instance(register_command("grpc://127.0.0.1:50051", 1_000, 30))
        .await
        .unwrap();

    let report = store
        .report_instance_status(ReportInstanceStatusCommand {
            namespace: "sdkwork".to_string(),
            environment: "development".to_string(),
            service_name: "sdkwork-drive-product".to_string(),
            instance_id: "drive-1".to_string(),
            status: InstanceStatus::NotServing,
            now_ms: 2_000,
            expected_revision: None,
        })
        .await
        .unwrap();

    assert_eq!(report.revision, 2);

    let discovered = store
        .discover_instances(discovery_query(), 2_000)
        .await
        .unwrap();

    assert!(discovered.instances.is_empty());
    assert_eq!(discovered.revision, 2);
}

#[tokio::test]
async fn report_instance_status_rejects_expired_instance_without_advancing_revision() {
    let mut store = MemoryDiscoveryStore::new();
    store
        .register_instance(register_command("grpc://127.0.0.1:50051", 1_000, 1))
        .await
        .unwrap();

    assert_not_found_contains(
        store
            .report_instance_status(ReportInstanceStatusCommand {
                namespace: "sdkwork".to_string(),
                environment: "development".to_string(),
                service_name: "sdkwork-drive-product".to_string(),
                instance_id: "drive-1".to_string(),
                status: InstanceStatus::NotServing,
                now_ms: 2_001,
                expected_revision: None,
            })
            .await
            .unwrap_err(),
        "instance",
    );

    let replacement = store
        .register_instance(register_command("grpc://127.0.0.1:50052", 2_500, 30))
        .await
        .unwrap();

    assert_eq!(replacement.revision, 2);
    assert_eq!(replacement.lease_id, "lease-2");
}

#[tokio::test]
async fn discover_instances_rejects_blank_required_filters() {
    let store = MemoryDiscoveryStore::new();

    assert_invalid_argument_contains(
        store
            .discover_instances(
                DiscoverInstancesQuery {
                    namespace: " ".to_string(),
                    ..discovery_query()
                },
                1_000,
            )
            .await
            .unwrap_err(),
        "namespace",
    );
    assert_invalid_argument_contains(
        store
            .discover_instances(
                DiscoverInstancesQuery {
                    environment: " ".to_string(),
                    ..discovery_query()
                },
                1_000,
            )
            .await
            .unwrap_err(),
        "environment",
    );
    assert_invalid_argument_contains(
        store
            .discover_instances(
                DiscoverInstancesQuery {
                    service_name: " ".to_string(),
                    ..discovery_query()
                },
                1_000,
            )
            .await
            .unwrap_err(),
        "service_name",
    );
}

#[tokio::test]
async fn discover_instances_rejects_blank_optional_protocol_filter() {
    let store = MemoryDiscoveryStore::new();

    assert_invalid_argument_contains(
        store
            .discover_instances(
                DiscoverInstancesQuery {
                    protocol: Some(" ".to_string()),
                    ..discovery_query()
                },
                1_000,
            )
            .await
            .unwrap_err(),
        "protocol",
    );
}

#[tokio::test]
async fn deregister_instance_rejects_blank_required_identity_fields() {
    let mut store = MemoryDiscoveryStore::new();

    assert_invalid_argument_contains(
        store
            .deregister_instance(
                " ",
                "development",
                "sdkwork-drive-product",
                "drive-1",
                1_000,
            )
            .await
            .unwrap_err(),
        "namespace",
    );
    assert_invalid_argument_contains(
        store
            .deregister_instance("sdkwork", " ", "sdkwork-drive-product", "drive-1", 1_000)
            .await
            .unwrap_err(),
        "environment",
    );
    assert_invalid_argument_contains(
        store
            .deregister_instance("sdkwork", "development", " ", "drive-1", 1_000)
            .await
            .unwrap_err(),
        "service_name",
    );
    assert_invalid_argument_contains(
        store
            .deregister_instance(
                "sdkwork",
                "development",
                "sdkwork-drive-product",
                " ",
                1_000,
            )
            .await
            .unwrap_err(),
        "instance_id",
    );
}

#[tokio::test]
async fn list_services_groups_non_expired_instances_by_service_name() {
    let mut store = MemoryDiscoveryStore::new();
    store
        .register_instance(register_command("grpc://127.0.0.1:50051", 1_000, 30))
        .await
        .unwrap();
    let mut second = register_command("grpc://127.0.0.1:50052", 1_000, 1);
    second.service_name = "sdkwork-config-product".to_string();
    store.register_instance(second).await.unwrap();

    let services = store
        .list_services(
            ListServicesQuery {
                namespace: "sdkwork".to_string(),
                environment: "development".to_string(),
                page_size: 0,
                page_token: None,
            },
            2_500,
        )
        .await
        .unwrap();

    assert_eq!(services.revision, 2);
    assert_eq!(services.services.len(), 1);
    assert_eq!(services.services[0].service_name, "sdkwork-drive-product");
    assert_eq!(services.services[0].active_instance_count, 1);
}

#[tokio::test]
async fn list_services_paginates_sorted_service_names() {
    let mut store = MemoryDiscoveryStore::new();
    store
        .register_instance(register_command("grpc://127.0.0.1:50051", 1_000, 30))
        .await
        .unwrap();
    let mut second = register_command("grpc://127.0.0.1:50052", 1_000, 30);
    second.service_name = "sdkwork-config-product".to_string();
    store.register_instance(second).await.unwrap();
    let mut third = register_command("grpc://127.0.0.1:50053", 1_000, 30);
    third.service_name = "sdkwork-iam-product".to_string();
    store.register_instance(third).await.unwrap();

    let first = store
        .list_services(
            ListServicesQuery {
                namespace: "sdkwork".to_string(),
                environment: "development".to_string(),
                page_size: 2,
                page_token: None,
            },
            2_500,
        )
        .await
        .unwrap();

    assert_eq!(first.services.len(), 2);
    assert_eq!(first.services[0].service_name, "sdkwork-config-product");
    assert_eq!(
        first.next_page_token.as_deref(),
        Some("sdkwork-drive-product")
    );

    let second_page = store
        .list_services(
            ListServicesQuery {
                page_token: first.next_page_token.clone(),
                namespace: "sdkwork".to_string(),
                environment: "development".to_string(),
                page_size: 2,
            },
            2_500,
        )
        .await
        .unwrap();

    assert_eq!(second_page.services.len(), 1);
    assert_eq!(second_page.services[0].service_name, "sdkwork-iam-product");
    assert_eq!(second_page.next_page_token, None);
}

fn assert_invalid_argument_contains(error: DiscoveryError, field: &str) {
    match error {
        DiscoveryError::InvalidArgument(message) => assert!(
            message.contains(field),
            "expected invalid argument message to mention {field}, got {message}"
        ),
        other => panic!("expected InvalidArgument for {field}, got {other}"),
    }
}

fn assert_not_found_contains(error: DiscoveryError, field: &str) {
    match error {
        DiscoveryError::NotFound(message) => assert!(
            message.contains(field),
            "expected not found message to mention {field}, got {message}"
        ),
        other => panic!("expected NotFound for {field}, got {other}"),
    }
}
