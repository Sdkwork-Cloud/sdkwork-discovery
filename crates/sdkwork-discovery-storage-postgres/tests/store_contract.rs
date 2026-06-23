use sdkwork_discovery_config::{StorageCredentialSource, StorageTransportConfig};
use sdkwork_discovery_contract::{
    ConfigFormat, ConfigScope, CreateConfigDraftCommand, DiscoverInstancesQuery, DiscoveryError,
    IdempotencyContext, InstanceStatus, PublishConfigCommand, RegisterInstanceCommand,
    RetrieveInstanceQuery, RollbackConfigCommand, WatchEventsQuery,
};
use sdkwork_discovery_storage_contract::{ConfigStore, RegistryStore, WatchEventStore};
use sdkwork_discovery_storage_postgres::PostgresDiscoveryStore;

fn transport() -> StorageTransportConfig {
    StorageTransportConfig {
        host: "127.0.0.1".to_string(),
        port: 5432,
        database: Some("sdkwork_discovery".to_string()),
        schema: None,
        username: Some("sdkwork_discovery".to_string()),
        credential_source: StorageCredentialSource::None,
        tls_enabled: false,
        connect_timeout_ms: 3000,
        max_connections: 8,
    }
}

#[test]
fn lazy_store_can_be_constructed_without_opening_network_connection() {
    let store = PostgresDiscoveryStore::new_lazy(&transport(), None).unwrap();

    assert_eq!(
        store.safe_summary(),
        "postgres host=127.0.0.1 port=5432 database=sdkwork_discovery schema=<none> username=sdkwork_discovery tls=false max_connections=8"
    );
}

#[test]
fn store_exposes_initial_schema_for_deployment_bootstrap() {
    let store = PostgresDiscoveryStore::new_lazy(&transport(), None).unwrap();

    assert!(store
        .initial_schema_sql()
        .contains("CREATE TABLE IF NOT EXISTS discovery_service_instance"));
}

#[tokio::test]
async fn postgres_discover_instances_rejects_blank_required_filters_before_querying() {
    let store = PostgresDiscoveryStore::new_lazy(&transport(), None).unwrap();

    assert_invalid_argument_contains(
        store
            .discover_instances(
                DiscoverInstancesQuery {
                    namespace: " ".to_string(),
                    environment: "development".to_string(),
                    service_name: "sdkwork-drive-product".to_string(),
                    healthy_only: true,
                    protocol: Some("grpc".to_string()),
                    label_filters: vec![],
                    sort_by: None,
                    page_size: 0,
                    page_token: None,
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
                    namespace: "sdkwork".to_string(),
                    environment: " ".to_string(),
                    service_name: "sdkwork-drive-product".to_string(),
                    healthy_only: true,
                    protocol: Some("grpc".to_string()),
                    label_filters: vec![],
                    sort_by: None,
                    page_size: 0,
                    page_token: None,
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
                    namespace: "sdkwork".to_string(),
                    environment: "development".to_string(),
                    service_name: " ".to_string(),
                    healthy_only: true,
                    protocol: Some("grpc".to_string()),
                    label_filters: vec![],
                    sort_by: None,
                    page_size: 0,
                    page_token: None,
                },
                1_000,
            )
            .await
            .unwrap_err(),
        "service_name",
    );
}

#[tokio::test]
async fn postgres_retrieve_instance_rejects_blank_identity_fields_before_querying() {
    let store = PostgresDiscoveryStore::new_lazy(&transport(), None).unwrap();

    for (field, query) in [
        (
            "namespace",
            RetrieveInstanceQuery {
                namespace: " ".to_string(),
                environment: "development".to_string(),
                service_name: "sdkwork-drive-product".to_string(),
                instance_id: "drive-1".to_string(),
            },
        ),
        (
            "environment",
            RetrieveInstanceQuery {
                namespace: "sdkwork".to_string(),
                environment: " ".to_string(),
                service_name: "sdkwork-drive-product".to_string(),
                instance_id: "drive-1".to_string(),
            },
        ),
        (
            "service_name",
            RetrieveInstanceQuery {
                namespace: "sdkwork".to_string(),
                environment: "development".to_string(),
                service_name: " ".to_string(),
                instance_id: "drive-1".to_string(),
            },
        ),
        (
            "instance_id",
            RetrieveInstanceQuery {
                namespace: "sdkwork".to_string(),
                environment: "development".to_string(),
                service_name: "sdkwork-drive-product".to_string(),
                instance_id: " ".to_string(),
            },
        ),
    ] {
        assert_invalid_argument_contains(
            store.retrieve_instance(query, 1_000).await.unwrap_err(),
            field,
        );
    }
}

#[tokio::test]
async fn postgres_register_instance_rejects_blank_required_metadata_fields_before_querying() {
    let mut store = PostgresDiscoveryStore::new_lazy(&transport(), None).unwrap();

    assert_invalid_argument_contains(
        store
            .register_instance(RegisterInstanceCommand {
                endpoint: " ".to_string(),
                ..register_command("drive-1", "grpc://127.0.0.1:50051", 1_000)
            })
            .await
            .unwrap_err(),
        "endpoint",
    );
    assert_invalid_argument_contains(
        store
            .register_instance(RegisterInstanceCommand {
                protocol: " ".to_string(),
                ..register_command("drive-1", "grpc://127.0.0.1:50051", 1_000)
            })
            .await
            .unwrap_err(),
        "protocol",
    );
    assert_invalid_argument_contains(
        store
            .register_instance(RegisterInstanceCommand {
                version: " ".to_string(),
                ..register_command("drive-1", "grpc://127.0.0.1:50051", 1_000)
            })
            .await
            .unwrap_err(),
        "version",
    );
    assert_invalid_argument_contains(
        store
            .register_instance(RegisterInstanceCommand {
                region: " ".to_string(),
                ..register_command("drive-1", "grpc://127.0.0.1:50051", 1_000)
            })
            .await
            .unwrap_err(),
        "region",
    );
    assert_invalid_argument_contains(
        store
            .register_instance(RegisterInstanceCommand {
                zone: " ".to_string(),
                ..register_command("drive-1", "grpc://127.0.0.1:50051", 1_000)
            })
            .await
            .unwrap_err(),
        "zone",
    );
}

#[tokio::test]
async fn postgres_discover_instances_rejects_blank_optional_protocol_filter_before_querying() {
    let store = PostgresDiscoveryStore::new_lazy(&transport(), None).unwrap();

    assert_invalid_argument_contains(
        store
            .discover_instances(
                DiscoverInstancesQuery {
                    namespace: "sdkwork".to_string(),
                    environment: "development".to_string(),
                    service_name: "sdkwork-drive-product".to_string(),
                    healthy_only: true,
                    protocol: Some(" ".to_string()),
                    label_filters: vec![],
                    sort_by: None,
                    page_size: 0,
                    page_token: None,
                },
                1_000,
            )
            .await
            .unwrap_err(),
        "protocol",
    );
}

#[tokio::test]
async fn postgres_watch_events_rejects_blank_required_and_optional_filters_before_querying() {
    let store = PostgresDiscoveryStore::new_lazy(&transport(), None).unwrap();

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

#[tokio::test]
async fn postgres_create_config_draft_rejects_blank_audit_and_scope_fields_before_querying() {
    let mut store = PostgresDiscoveryStore::new_lazy(&transport(), None).unwrap();

    assert_invalid_argument_contains(
        store
            .create_config_draft(config_draft_command(
                "log.level",
                "debug",
                ConfigScope::Namespace,
                Some(" ".to_string()),
                None,
            ))
            .await
            .unwrap_err(),
        "created_by",
    );
    assert_invalid_argument_contains(
        store
            .create_config_draft(config_draft_command(
                "log.level",
                "debug",
                ConfigScope::Application {
                    application: " ".to_string(),
                },
                None,
                None,
            ))
            .await
            .unwrap_err(),
        "application",
    );
    assert_invalid_argument_contains(
        store
            .create_config_draft(config_draft_command(
                "log.level",
                "debug",
                ConfigScope::Service {
                    application: " ".to_string(),
                    service_name: "sdkwork-drive-product".to_string(),
                },
                None,
                None,
            ))
            .await
            .unwrap_err(),
        "application",
    );
    assert_invalid_argument_contains(
        store
            .create_config_draft(config_draft_command(
                "log.level",
                "debug",
                ConfigScope::Service {
                    application: "sdkwork-drive".to_string(),
                    service_name: " ".to_string(),
                },
                None,
                None,
            ))
            .await
            .unwrap_err(),
        "service_name",
    );
}

#[tokio::test]
async fn postgres_publish_config_rejects_blank_required_fields_before_querying() {
    let mut store = PostgresDiscoveryStore::new_lazy(&transport(), None).unwrap();

    assert_invalid_argument_contains(
        store
            .publish_config(PublishConfigCommand {
                draft_id: " ".to_string(),
                published_by: "operator-1".to_string(),
                now_ms: 1_000,
                idempotency: None,
            })
            .await
            .unwrap_err(),
        "draft_id",
    );
    assert_invalid_argument_contains(
        store
            .publish_config(PublishConfigCommand {
                draft_id: "draft-1".to_string(),
                published_by: " ".to_string(),
                now_ms: 1_000,
                idempotency: None,
            })
            .await
            .unwrap_err(),
        "published_by",
    );
}

#[tokio::test]
async fn postgres_config_writes_reject_blank_idempotency_fields_before_querying() {
    let mut store = PostgresDiscoveryStore::new_lazy(&transport(), None).unwrap();

    for (field, idempotency) in invalid_idempotency_cases() {
        assert_invalid_argument_contains(
            store
                .create_config_draft(config_draft_command(
                    "log.level",
                    "debug",
                    ConfigScope::Namespace,
                    None,
                    Some(idempotency),
                ))
                .await
                .unwrap_err(),
            field,
        );
    }

    for (field, idempotency) in invalid_idempotency_cases() {
        assert_invalid_argument_contains(
            store
                .publish_config(PublishConfigCommand {
                    draft_id: "draft-1".to_string(),
                    published_by: "operator-1".to_string(),
                    now_ms: 1_000,
                    idempotency: Some(idempotency),
                })
                .await
                .unwrap_err(),
            field,
        );
    }

    for (field, idempotency) in invalid_idempotency_cases() {
        assert_invalid_argument_contains(
            store
                .rollback_config(RollbackConfigCommand {
                    source_release_id: "release-1".to_string(),
                    rolled_back_by: "operator-2".to_string(),
                    now_ms: 2_000,
                    idempotency: Some(idempotency),
                })
                .await
                .unwrap_err(),
            field,
        );
    }
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

fn config_draft_command(
    key: &str,
    value: &str,
    scope: ConfigScope,
    created_by: Option<String>,
    idempotency: Option<IdempotencyContext>,
) -> CreateConfigDraftCommand {
    CreateConfigDraftCommand {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        group: "runtime".to_string(),
        key: key.to_string(),
        format: ConfigFormat::Text,
        value: value.to_string(),
        scope,
        created_by: created_by.unwrap_or_else(|| "operator-1".to_string()),
        idempotency,
    }
}

fn invalid_idempotency_cases() -> Vec<(&'static str, IdempotencyContext)> {
    vec![
        (
            "operation_id",
            IdempotencyContext::new(" ", "idempotency-key-1", "sha256:hash-1"),
        ),
        (
            "idempotency key",
            IdempotencyContext::new("discovery.config.write", " ", "sha256:hash-1"),
        ),
        (
            "request_hash",
            IdempotencyContext::new("discovery.config.write", "idempotency-key-1", " "),
        ),
    ]
}

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
