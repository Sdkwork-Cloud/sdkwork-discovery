use sdkwork_discovery_contract::{
    ConfigFormat, ConfigScope, CreateConfigDraftCommand, DiscoveryError, DiscoveryEventKind,
    RegisterInstanceCommand, WatchEventsQuery,
};
use sdkwork_discovery_storage_contract::{ConfigStore, RegistryStore, WatchEventStore};
use sdkwork_discovery_storage_memory::MemoryDiscoveryStore;

fn register_command(instance_id: &str, now_ms: u64) -> RegisterInstanceCommand {
    RegisterInstanceCommand {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        service_name: "sdkwork-drive-product".to_string(),
        instance_id: instance_id.to_string(),
        endpoint: format!("grpc://127.0.0.1:{}", 50_000 + now_ms),
        protocol: "grpc".to_string(),
        version: "0.1.0".to_string(),
        region: "local".to_string(),
        zone: "local-a".to_string(),
        weight: 100,
        priority: 0,
        status: sdkwork_discovery_contract::InstanceStatus::Serving,
        metadata: Default::default(),
        lease_ttl_seconds: 30,
        now_ms,
    }
}

#[tokio::test]
async fn registry_and_config_changes_are_exposed_as_revision_ordered_watch_events() {
    let mut store = MemoryDiscoveryStore::new();

    store
        .register_instance(register_command("drive-1", 1_000))
        .await
        .unwrap();
    let draft = store
        .create_config_draft(CreateConfigDraftCommand {
            namespace: "sdkwork".to_string(),
            environment: "development".to_string(),
            group: "runtime".to_string(),
            key: "log.level".to_string(),
            format: ConfigFormat::Text,
            value: "debug".to_string(),
            scope: ConfigScope::Namespace,
            created_by: "operator-1".to_string(),
            idempotency: None,
        })
        .await
        .unwrap();
    store
        .publish_config(sdkwork_discovery_contract::PublishConfigCommand {
            draft_id: draft.draft_id,
            published_by: "operator-1".to_string(),
            now_ms: 2_000,
            idempotency: None,
        })
        .await
        .unwrap();
    store
        .deregister_instance(
            "sdkwork",
            "development",
            "sdkwork-drive-product",
            "drive-1",
            2_000,
        )
        .await
        .unwrap();

    let events = store
        .watch_events(WatchEventsQuery {
            namespace: "sdkwork".to_string(),
            environment: "development".to_string(),
            from_revision: 0,
            service_name: None,
            config_group: None,
            config_application: None,
            max_events: 1_024,
        })
        .await
        .unwrap();

    assert_eq!(events.len(), 3);
    assert_eq!(events[0].revision, 1);
    assert_eq!(events[0].kind, DiscoveryEventKind::InstanceRegistered);
    assert_eq!(events[1].revision, 2);
    assert_eq!(events[1].kind, DiscoveryEventKind::ConfigPublished);
    assert_eq!(events[2].revision, 3);
    assert_eq!(events[2].kind, DiscoveryEventKind::InstanceDeregistered);
}

#[tokio::test]
async fn application_scoped_config_events_include_application_scope() {
    let mut store = MemoryDiscoveryStore::new();

    let draft = store
        .create_config_draft(CreateConfigDraftCommand {
            namespace: "sdkwork".to_string(),
            environment: "development".to_string(),
            group: "runtime".to_string(),
            key: "log.level".to_string(),
            format: ConfigFormat::Text,
            value: "debug".to_string(),
            scope: ConfigScope::Application {
                application: "sdkwork-drive".to_string(),
            },
            created_by: "operator-1".to_string(),
            idempotency: None,
        })
        .await
        .unwrap();
    store
        .publish_config(sdkwork_discovery_contract::PublishConfigCommand {
            draft_id: draft.draft_id,
            published_by: "operator-1".to_string(),
            now_ms: 2_000,
            idempotency: None,
        })
        .await
        .unwrap();

    let events = store
        .watch_events(WatchEventsQuery {
            namespace: "sdkwork".to_string(),
            environment: "development".to_string(),
            from_revision: 0,
            service_name: None,
            config_group: None,
            config_application: None,
            max_events: 1_024,
        })
        .await
        .unwrap();

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, DiscoveryEventKind::ConfigPublished);
    assert_eq!(
        events[0].config_application.as_deref(),
        Some("sdkwork-drive")
    );
}

#[tokio::test]
async fn watch_events_respects_max_events() {
    let mut store = MemoryDiscoveryStore::new();

    store
        .register_instance(register_command("drive-1", 1_000))
        .await
        .unwrap();
    store
        .register_instance(register_command("drive-2", 2_000))
        .await
        .unwrap();
    store
        .register_instance(register_command("drive-3", 3_000))
        .await
        .unwrap();

    let events = store
        .watch_events(WatchEventsQuery {
            namespace: "sdkwork".to_string(),
            environment: "development".to_string(),
            from_revision: 0,
            service_name: None,
            config_group: None,
            config_application: None,
            max_events: 2,
        })
        .await
        .unwrap();

    assert_eq!(events.len(), 2);
    assert_eq!(events[0].resource_id, "drive-1");
    assert_eq!(events[1].resource_id, "drive-2");
}

#[tokio::test]
async fn watch_events_rejects_zero_max_events() {
    let store = MemoryDiscoveryStore::new();

    assert_invalid_argument_contains(
        store
            .watch_events(WatchEventsQuery {
                namespace: "sdkwork".to_string(),
                environment: "development".to_string(),
                from_revision: 0,
                service_name: None,
                config_group: None,
                config_application: None,
                max_events: 0,
            })
            .await
            .unwrap_err(),
        "max_events",
    );
}

#[tokio::test]
async fn watch_events_rejects_blank_required_and_optional_filters() {
    let store = MemoryDiscoveryStore::new();

    assert_invalid_argument_contains(
        store
            .watch_events(WatchEventsQuery {
                namespace: " ".to_string(),
                environment: "development".to_string(),
                from_revision: 0,
                service_name: None,
                config_group: None,
                config_application: None,
                max_events: 1_024,
            })
            .await
            .unwrap_err(),
        "namespace",
    );
    assert_invalid_argument_contains(
        store
            .watch_events(WatchEventsQuery {
                namespace: "sdkwork".to_string(),
                environment: " ".to_string(),
                from_revision: 0,
                service_name: None,
                config_group: None,
                config_application: None,
                max_events: 1_024,
            })
            .await
            .unwrap_err(),
        "environment",
    );
    assert_invalid_argument_contains(
        store
            .watch_events(WatchEventsQuery {
                namespace: "sdkwork".to_string(),
                environment: "development".to_string(),
                from_revision: 0,
                service_name: Some(" ".to_string()),
                config_group: None,
                config_application: None,
                max_events: 1_024,
            })
            .await
            .unwrap_err(),
        "service_name",
    );
    assert_invalid_argument_contains(
        store
            .watch_events(WatchEventsQuery {
                namespace: "sdkwork".to_string(),
                environment: "development".to_string(),
                from_revision: 0,
                service_name: None,
                config_group: Some(" ".to_string()),
                config_application: None,
                max_events: 1_024,
            })
            .await
            .unwrap_err(),
        "config_group",
    );
    assert_invalid_argument_contains(
        store
            .watch_events(WatchEventsQuery {
                namespace: "sdkwork".to_string(),
                environment: "development".to_string(),
                from_revision: 0,
                service_name: None,
                config_group: None,
                config_application: Some(" ".to_string()),
                max_events: 1_024,
            })
            .await
            .unwrap_err(),
        "config_application",
    );
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
