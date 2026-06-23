use sdkwork_discovery_contract::{
    ConfigFormat, ConfigScope, CreateConfigDraftCommand, DiscoverInstancesQuery, DiscoveryError,
    DiscoveryEventKind, IdempotencyContext, InstanceStatus, ListServicesQuery,
    PublishConfigCommand, RegisterInstanceCommand, RenewLeaseCommand, ReportInstanceStatusCommand,
    RetrieveEffectiveConfigQuery, RetrieveInstanceQuery, WatchEventsQuery,
};
use sdkwork_discovery_storage_contract::{ConfigStore, RegistryStore, WatchEventStore};
use sdkwork_discovery_storage_sqlite::SqliteDiscoveryStore;

async fn store() -> SqliteDiscoveryStore {
    let store = SqliteDiscoveryStore::new_in_memory().await.unwrap();
    store.apply_initial_schema().await.unwrap();
    store
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

#[tokio::test]
async fn sqlite_registry_persists_instances_and_filters_expired_rows() {
    let mut store = store().await;

    let first = store
        .register_instance(register_command("drive-1", "grpc://127.0.0.1:50051", 1_000))
        .await
        .unwrap();
    let second = store
        .register_instance(register_command("drive-1", "grpc://127.0.0.1:50052", 2_000))
        .await
        .unwrap();

    assert_eq!(first.revision, 1);
    assert_eq!(second.revision, 2);
    assert_eq!(first.lease_id, second.lease_id);

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

    assert_eq!(active.revision, 2);
    assert_eq!(active.instances.len(), 1);
    assert_eq!(active.instances[0].endpoint, "grpc://127.0.0.1:50052");

    let expired = store
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
            33_000,
        )
        .await
        .unwrap();

    assert!(expired.instances.is_empty());
}

#[tokio::test]
async fn sqlite_retrieve_instance_reads_current_registered_instance_by_identity() {
    let mut store = store().await;

    let registered = store
        .register_instance(register_command("drive-1", "grpc://127.0.0.1:50051", 1_000))
        .await
        .unwrap();

    let instance = store
        .retrieve_instance(
            RetrieveInstanceQuery {
                namespace: "sdkwork".to_string(),
                environment: "development".to_string(),
                service_name: "sdkwork-drive-product".to_string(),
                instance_id: "drive-1".to_string(),
            },
            2_500,
        )
        .await
        .unwrap()
        .expect("registered instance should be retrievable by identity");
    let missing = store
        .retrieve_instance(
            RetrieveInstanceQuery {
                namespace: "sdkwork".to_string(),
                environment: "development".to_string(),
                service_name: "sdkwork-drive-product".to_string(),
                instance_id: "missing-instance".to_string(),
            },
            2_500,
        )
        .await
        .unwrap();

    assert_eq!(instance.instance_id, "drive-1");
    assert_eq!(instance.endpoint, "grpc://127.0.0.1:50051");
    assert_eq!(instance.lease_id, registered.lease_id);
    assert!(missing.is_none());
}

#[tokio::test]
async fn sqlite_retrieve_instance_excludes_expired_registered_instance_by_identity() {
    let mut store = store().await;

    store
        .register_instance(register_command("drive-1", "grpc://127.0.0.1:50051", 1_000))
        .await
        .unwrap();

    let active = store
        .retrieve_instance(
            RetrieveInstanceQuery {
                namespace: "sdkwork".to_string(),
                environment: "development".to_string(),
                service_name: "sdkwork-drive-product".to_string(),
                instance_id: "drive-1".to_string(),
            },
            2_500,
        )
        .await
        .unwrap();
    let expired = store
        .retrieve_instance(
            RetrieveInstanceQuery {
                namespace: "sdkwork".to_string(),
                environment: "development".to_string(),
                service_name: "sdkwork-drive-product".to_string(),
                instance_id: "drive-1".to_string(),
            },
            31_001,
        )
        .await
        .unwrap();

    assert!(active.is_some());
    assert!(expired.is_none());
}

#[tokio::test]
async fn sqlite_renew_lease_rejects_expired_lease() {
    let mut store = store().await;

    let registered = store
        .register_instance(RegisterInstanceCommand {
            lease_ttl_seconds: 1,
            ..register_command("drive-1", "grpc://127.0.0.1:50051", 1_000)
        })
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
async fn sqlite_register_instance_replaces_expired_lease_for_existing_identity() {
    let mut store = store().await;

    let expired = store
        .register_instance(RegisterInstanceCommand {
            lease_ttl_seconds: 1,
            ..register_command("drive-1", "grpc://127.0.0.1:50051", 1_000)
        })
        .await
        .unwrap();
    let replacement = store
        .register_instance(register_command("drive-1", "grpc://127.0.0.1:50052", 2_001))
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
async fn sqlite_report_instance_status_rejects_expired_instance_without_advancing_revision() {
    let mut store = store().await;

    store
        .register_instance(RegisterInstanceCommand {
            lease_ttl_seconds: 1,
            ..register_command("drive-1", "grpc://127.0.0.1:50051", 1_000)
        })
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
        .register_instance(register_command("drive-1", "grpc://127.0.0.1:50052", 2_500))
        .await
        .unwrap();

    assert_eq!(replacement.revision, 2);
}

#[tokio::test]
async fn sqlite_deregister_instance_ignores_expired_instance_without_advancing_revision() {
    let mut store = store().await;

    store
        .register_instance(RegisterInstanceCommand {
            lease_ttl_seconds: 1,
            ..register_command("drive-1", "grpc://127.0.0.1:50051", 1_000)
        })
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
        .register_instance(register_command("drive-1", "grpc://127.0.0.1:50052", 2_500))
        .await
        .unwrap();

    assert_eq!(replacement.revision, 2);
}

#[tokio::test]
async fn sqlite_expire_instances_soft_deletes_expired_instances_and_emits_watch_events() {
    let mut store = store().await;

    store
        .register_instance(RegisterInstanceCommand {
            lease_ttl_seconds: 1,
            ..register_command("drive-1", "grpc://127.0.0.1:50051", 1_000)
        })
        .await
        .unwrap();
    store
        .register_instance(register_command("drive-2", "grpc://127.0.0.1:50052", 1_000))
        .await
        .unwrap();

    let expired = store.expire_instances(2_001, 1_000).await.unwrap();

    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].instance_id, "drive-1");
    assert_eq!(expired[0].revision, 3);
    assert!(expired[0].deregistered);

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
            2_001,
        )
        .await
        .unwrap();
    assert_eq!(active.instances.len(), 1);
    assert_eq!(active.instances[0].instance_id, "drive-2");

    let events = store
        .watch_events(WatchEventsQuery {
            namespace: "sdkwork".to_string(),
            environment: "development".to_string(),
            from_revision: 2,
            service_name: None,
            config_group: None,
            config_application: None,
            max_events: 1_024,
        })
        .await
        .unwrap();

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].resource_id, "drive-1");
    assert_eq!(events[0].kind, DiscoveryEventKind::InstanceDeregistered);
}

#[tokio::test]
async fn sqlite_expire_instances_respects_batch_limit() {
    let mut store = store().await;

    for instance_id in ["drive-1", "drive-2", "drive-3"] {
        store
            .register_instance(RegisterInstanceCommand {
                lease_ttl_seconds: 1,
                ..register_command(instance_id, "grpc://127.0.0.1:50051", 1_000)
            })
            .await
            .unwrap();
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
async fn sqlite_discover_instances_rejects_blank_required_filters() {
    let store = store().await;

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
async fn sqlite_register_instance_rejects_blank_required_metadata_fields() {
    let mut store = store().await;

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
async fn sqlite_discover_instances_rejects_blank_optional_protocol_filter() {
    let store = store().await;

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
async fn sqlite_config_releases_are_effective_by_scope_specificity() {
    let mut store = store().await;

    let broad = store
        .create_config_draft(CreateConfigDraftCommand {
            namespace: "sdkwork".to_string(),
            environment: "development".to_string(),
            group: "runtime".to_string(),
            key: "log.level".to_string(),
            format: ConfigFormat::Text,
            value: "info".to_string(),
            scope: ConfigScope::Namespace,
            created_by: "operator-1".to_string(),
            idempotency: None,
        })
        .await
        .unwrap();
    let narrow = store
        .create_config_draft(CreateConfigDraftCommand {
            namespace: "sdkwork".to_string(),
            environment: "development".to_string(),
            group: "runtime".to_string(),
            key: "log.level".to_string(),
            format: ConfigFormat::Text,
            value: "debug".to_string(),
            scope: ConfigScope::Service {
                application: "sdkwork-drive".to_string(),
                service_name: "sdkwork-drive-product".to_string(),
            },
            created_by: "operator-1".to_string(),
            idempotency: None,
        })
        .await
        .unwrap();

    store
        .publish_config(PublishConfigCommand {
            draft_id: broad.draft_id,
            published_by: "operator-1".to_string(),
            now_ms: 1_000,
            idempotency: None,
        })
        .await
        .unwrap();
    let release = store
        .publish_config(PublishConfigCommand {
            draft_id: narrow.draft_id,
            published_by: "operator-1".to_string(),
            now_ms: 2_000,
            idempotency: None,
        })
        .await
        .unwrap();

    let effective = store
        .retrieve_effective_config(RetrieveEffectiveConfigQuery {
            namespace: "sdkwork".to_string(),
            environment: "development".to_string(),
            application: "sdkwork-drive".to_string(),
            service_name: "sdkwork-drive-product".to_string(),
            group: "runtime".to_string(),
        })
        .await
        .unwrap();

    let value = effective.values.get("log.level").unwrap();
    assert_eq!(effective.revision, 2);
    assert_eq!(value.value, "debug");
    assert_eq!(value.source_release_id, release.release_id);
}

#[tokio::test]
async fn sqlite_config_writes_replay_same_idempotency_key() {
    let mut store = store().await;

    let first_draft = store
        .create_config_draft(CreateConfigDraftCommand {
            namespace: "sdkwork".to_string(),
            environment: "development".to_string(),
            group: "runtime".to_string(),
            key: "log.level".to_string(),
            format: ConfigFormat::Text,
            value: "debug".to_string(),
            scope: ConfigScope::Namespace,
            created_by: "operator-1".to_string(),
            idempotency: Some(IdempotencyContext::new(
                "discovery.config.drafts.create",
                "draft-key-1",
                "sha256:draft-hash-1",
            )),
        })
        .await
        .unwrap();
    let replayed_draft = store
        .create_config_draft(CreateConfigDraftCommand {
            namespace: "sdkwork".to_string(),
            environment: "development".to_string(),
            group: "runtime".to_string(),
            key: "log.level".to_string(),
            format: ConfigFormat::Text,
            value: "debug".to_string(),
            scope: ConfigScope::Namespace,
            created_by: "operator-1".to_string(),
            idempotency: Some(IdempotencyContext::new(
                "discovery.config.drafts.create",
                "draft-key-1",
                "sha256:draft-hash-1",
            )),
        })
        .await
        .unwrap();

    assert_eq!(replayed_draft.draft_id, first_draft.draft_id);

    let conflict = store
        .create_config_draft(CreateConfigDraftCommand {
            namespace: "sdkwork".to_string(),
            environment: "development".to_string(),
            group: "runtime".to_string(),
            key: "log.level".to_string(),
            format: ConfigFormat::Text,
            value: "trace".to_string(),
            scope: ConfigScope::Namespace,
            created_by: "operator-1".to_string(),
            idempotency: Some(IdempotencyContext::new(
                "discovery.config.drafts.create",
                "draft-key-1",
                "sha256:different-draft-hash",
            )),
        })
        .await
        .unwrap_err();
    assert!(conflict.to_string().contains("idempotency"));
    assert!(conflict.to_string().contains("request hash"));

    let first_release = store
        .publish_config(PublishConfigCommand {
            draft_id: first_draft.draft_id,
            published_by: "operator-1".to_string(),
            now_ms: 1_000,
            idempotency: Some(IdempotencyContext::new(
                "discovery.config.releases.publish",
                "publish-key-1",
                "sha256:publish-hash-1",
            )),
        })
        .await
        .unwrap();
    let replayed_release = store
        .publish_config(PublishConfigCommand {
            draft_id: first_release.draft_id.clone(),
            published_by: "operator-1".to_string(),
            now_ms: 2_000,
            idempotency: Some(IdempotencyContext::new(
                "discovery.config.releases.publish",
                "publish-key-1",
                "sha256:publish-hash-1",
            )),
        })
        .await
        .unwrap();

    assert_eq!(replayed_release.release_id, first_release.release_id);
    assert_eq!(replayed_release.revision, first_release.revision);
}

#[tokio::test]
async fn sqlite_create_config_draft_rejects_blank_audit_and_scope_fields() {
    let mut store = store().await;

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
async fn sqlite_config_writes_reject_blank_idempotency_fields() {
    let mut store = store().await;

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

    let publish_draft = store
        .create_config_draft(config_draft_command(
            "feature.enabled",
            "true",
            ConfigScope::Namespace,
            None,
            None,
        ))
        .await
        .unwrap();
    for (field, idempotency) in invalid_idempotency_cases() {
        assert_invalid_argument_contains(
            store
                .publish_config(PublishConfigCommand {
                    draft_id: publish_draft.draft_id.clone(),
                    published_by: "operator-1".to_string(),
                    now_ms: 1_000,
                    idempotency: Some(idempotency),
                })
                .await
                .unwrap_err(),
            field,
        );
    }

    let rollback_source_draft = store
        .create_config_draft(config_draft_command(
            "feature.mode",
            "stable",
            ConfigScope::Namespace,
            None,
            None,
        ))
        .await
        .unwrap();
    let rollback_source = store
        .publish_config(PublishConfigCommand {
            draft_id: rollback_source_draft.draft_id,
            published_by: "operator-1".to_string(),
            now_ms: 2_000,
            idempotency: None,
        })
        .await
        .unwrap();
    for (field, idempotency) in invalid_idempotency_cases() {
        assert_invalid_argument_contains(
            store
                .rollback_config(sdkwork_discovery_contract::RollbackConfigCommand {
                    source_release_id: rollback_source.release_id.clone(),
                    rolled_back_by: "operator-2".to_string(),
                    now_ms: 3_000,
                    idempotency: Some(idempotency),
                })
                .await
                .unwrap_err(),
            field,
        );
    }
}

#[tokio::test]
async fn sqlite_watch_events_replay_registry_and_config_changes() {
    let mut store = store().await;

    store
        .register_instance(register_command("drive-1", "grpc://127.0.0.1:50051", 1_000))
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
        .publish_config(PublishConfigCommand {
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
            3_000,
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
    assert_eq!(events[0].kind, DiscoveryEventKind::InstanceRegistered);
    assert_eq!(events[1].kind, DiscoveryEventKind::ConfigPublished);
    assert_eq!(events[1].config_group.as_deref(), Some("runtime"));
    assert_eq!(events[1].config_key.as_deref(), Some("log.level"));
    assert_eq!(events[2].kind, DiscoveryEventKind::InstanceDeregistered);
}

#[tokio::test]
async fn sqlite_watch_events_rejects_blank_required_and_optional_filters() {
    let store = store().await;

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
async fn sqlite_watch_events_filter_application_scoped_config_changes() {
    let mut store = store().await;

    let drive = store
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
    let chat = store
        .create_config_draft(CreateConfigDraftCommand {
            namespace: "sdkwork".to_string(),
            environment: "development".to_string(),
            group: "runtime".to_string(),
            key: "log.level".to_string(),
            format: ConfigFormat::Text,
            value: "trace".to_string(),
            scope: ConfigScope::Application {
                application: "sdkwork-chat".to_string(),
            },
            created_by: "operator-1".to_string(),
            idempotency: None,
        })
        .await
        .unwrap();

    store
        .publish_config(PublishConfigCommand {
            draft_id: drive.draft_id,
            published_by: "operator-1".to_string(),
            now_ms: 1_000,
            idempotency: None,
        })
        .await
        .unwrap();
    store
        .publish_config(PublishConfigCommand {
            draft_id: chat.draft_id,
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
            config_group: Some("runtime".to_string()),
            config_application: Some("sdkwork-drive".to_string()),
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
async fn sqlite_watch_events_respects_max_events() {
    let mut store = store().await;

    for (instance_id, now_ms) in [("drive-1", 1_000), ("drive-2", 2_000), ("drive-3", 3_000)] {
        store
            .register_instance(register_command(
                instance_id,
                "grpc://127.0.0.1:50051",
                now_ms,
            ))
            .await
            .unwrap();
    }

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
async fn sqlite_watch_events_rejects_zero_max_events() {
    let store = store().await;

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
async fn sqlite_watch_events_filter_service_scoped_config_changes() {
    let mut store = store().await;

    let product = store
        .create_config_draft(CreateConfigDraftCommand {
            namespace: "sdkwork".to_string(),
            environment: "development".to_string(),
            group: "runtime".to_string(),
            key: "log.level".to_string(),
            format: ConfigFormat::Text,
            value: "debug".to_string(),
            scope: ConfigScope::Service {
                application: "sdkwork-drive".to_string(),
                service_name: "sdkwork-drive-product".to_string(),
            },
            created_by: "operator-1".to_string(),
            idempotency: None,
        })
        .await
        .unwrap();
    let worker = store
        .create_config_draft(CreateConfigDraftCommand {
            namespace: "sdkwork".to_string(),
            environment: "development".to_string(),
            group: "runtime".to_string(),
            key: "log.level".to_string(),
            format: ConfigFormat::Text,
            value: "trace".to_string(),
            scope: ConfigScope::Service {
                application: "sdkwork-drive".to_string(),
                service_name: "sdkwork-drive-worker".to_string(),
            },
            created_by: "operator-1".to_string(),
            idempotency: None,
        })
        .await
        .unwrap();

    store
        .publish_config(PublishConfigCommand {
            draft_id: product.draft_id,
            published_by: "operator-1".to_string(),
            now_ms: 1_000,
            idempotency: None,
        })
        .await
        .unwrap();
    store
        .publish_config(PublishConfigCommand {
            draft_id: worker.draft_id,
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
            service_name: Some("sdkwork-drive-product".to_string()),
            config_group: Some("runtime".to_string()),
            config_application: Some("sdkwork-drive".to_string()),
            max_events: 1_024,
        })
        .await
        .unwrap();

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, DiscoveryEventKind::ConfigPublished);
    assert_eq!(
        events[0].service_name.as_deref(),
        Some("sdkwork-drive-product")
    );
}

#[tokio::test]
async fn sqlite_lists_non_expired_services() {
    let mut store = store().await;

    store
        .register_instance(register_command("drive-1", "grpc://127.0.0.1:50051", 1_000))
        .await
        .unwrap();

    let services = store
        .list_services(
            ListServicesQuery {
                namespace: "sdkwork".to_string(),
                environment: "development".to_string(),
                page_size: 0,
                page_token: None,
            },
            2_000,
        )
        .await
        .unwrap();

    assert_eq!(services.revision, 1);
    assert_eq!(services.services.len(), 1);
    assert_eq!(services.services[0].service_name, "sdkwork-drive-product");
}

#[tokio::test]
async fn sqlite_persistent_instances_survive_past_lease_ttl() {
    let mut store = store().await;
    let mut command = register_command("drive-persistent", "grpc://127.0.0.1:50051", 1_000);
    command.persistent = true;

    store.register_instance(command).await.unwrap();

    let discovered = store
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
            10_000_000,
        )
        .await
        .unwrap();

    assert_eq!(discovered.instances.len(), 1);
    assert_eq!(discovered.instances[0].instance_id, "drive-persistent");
}

#[tokio::test]
async fn sqlite_register_rejects_expected_revision_mismatch() {
    let mut store = store().await;

    let registered = store
        .register_instance(register_command("drive-1", "grpc://127.0.0.1:50051", 1_000))
        .await
        .unwrap();

    let mut retry = register_command("drive-1", "grpc://127.0.0.1:50052", 2_000);
    retry.expected_revision = Some(registered.revision + 99);

    let error = store.register_instance(retry).await.unwrap_err();
    assert!(matches!(error, DiscoveryError::Conflict(_)));
}

#[tokio::test]
async fn sqlite_discover_instances_applies_label_filters() {
    use sdkwork_discovery_contract::{LabelFilter, LabelFilterOp};

    let mut store = store().await;

    let mut primary = register_command("drive-primary", "grpc://127.0.0.1:50051", 1_000);
    primary
        .metadata
        .insert("role".to_string(), "primary".to_string());
    store.register_instance(primary).await.unwrap();

    let mut secondary = register_command("drive-secondary", "grpc://127.0.0.1:50052", 1_000);
    secondary
        .metadata
        .insert("role".to_string(), "secondary".to_string());
    store.register_instance(secondary).await.unwrap();

    let discovered = store
        .discover_instances(
            DiscoverInstancesQuery {
                namespace: "sdkwork".to_string(),
                environment: "development".to_string(),
                service_name: "sdkwork-drive-product".to_string(),
                healthy_only: true,
                protocol: Some("grpc".to_string()),
                label_filters: vec![LabelFilter {
                    key: "role".to_string(),
                    op: LabelFilterOp::Eq,
                    value: "primary".to_string(),
                }],
                sort_by: None,
                page_size: 0,
                page_token: None,
            },
            2_500,
        )
        .await
        .unwrap();

    assert_eq!(discovered.instances.len(), 1);
    assert_eq!(discovered.instances[0].instance_id, "drive-primary");
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
