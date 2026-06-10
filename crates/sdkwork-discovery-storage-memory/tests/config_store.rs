use sdkwork_discovery_contract::{
    ConfigFormat, ConfigScope, CreateConfigDraftCommand, DiscoveryError, IdempotencyContext,
    PublishConfigCommand, RetrieveEffectiveConfigQuery, RollbackConfigCommand,
};
use sdkwork_discovery_storage_contract::ConfigStore;
use sdkwork_discovery_storage_memory::MemoryDiscoveryStore;

fn draft(key: &str, value: &str, scope: ConfigScope) -> CreateConfigDraftCommand {
    CreateConfigDraftCommand {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        group: "runtime".to_string(),
        key: key.to_string(),
        format: ConfigFormat::Text,
        value: value.to_string(),
        scope,
        created_by: "operator-1".to_string(),
        idempotency: None,
    }
}

fn draft_with_idempotency(
    key: &str,
    value: &str,
    idempotency: IdempotencyContext,
) -> CreateConfigDraftCommand {
    CreateConfigDraftCommand {
        idempotency: Some(idempotency),
        ..draft(key, value, ConfigScope::Namespace)
    }
}

#[tokio::test]
async fn publish_creates_immutable_releases_and_effective_config_prefers_narrow_scope() {
    let mut store = MemoryDiscoveryStore::new();

    let broad = store
        .create_config_draft(draft("log.level", "info", ConfigScope::Namespace))
        .await
        .unwrap();
    let narrow = store
        .create_config_draft(draft(
            "log.level",
            "debug",
            ConfigScope::Service {
                application: "sdkwork-drive".to_string(),
                service_name: "sdkwork-drive-product".to_string(),
            },
        ))
        .await
        .unwrap();

    let first_release = store
        .publish_config(PublishConfigCommand {
            draft_id: broad.draft_id.clone(),
            published_by: "operator-1".to_string(),
            now_ms: 1_000,
            idempotency: None,
        })
        .await
        .unwrap();
    let second_release = store
        .publish_config(PublishConfigCommand {
            draft_id: narrow.draft_id.clone(),
            published_by: "operator-1".to_string(),
            now_ms: 2_000,
            idempotency: None,
        })
        .await
        .unwrap();

    assert_eq!(first_release.revision, 1);
    assert_eq!(second_release.revision, 2);

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

    assert_eq!(effective.revision, 2);
    assert_eq!(effective.values.get("log.level").unwrap().value, "debug");
    assert_eq!(
        effective.values.get("log.level").unwrap().source_release_id,
        second_release.release_id
    );
}

#[tokio::test]
async fn retrieve_effective_config_rejects_blank_required_filters() {
    let store = MemoryDiscoveryStore::new();

    assert_invalid_argument_contains(
        store
            .retrieve_effective_config(RetrieveEffectiveConfigQuery {
                namespace: " ".to_string(),
                ..effective_query()
            })
            .await
            .unwrap_err(),
        "namespace",
    );
    assert_invalid_argument_contains(
        store
            .retrieve_effective_config(RetrieveEffectiveConfigQuery {
                environment: " ".to_string(),
                ..effective_query()
            })
            .await
            .unwrap_err(),
        "environment",
    );
    assert_invalid_argument_contains(
        store
            .retrieve_effective_config(RetrieveEffectiveConfigQuery {
                application: " ".to_string(),
                ..effective_query()
            })
            .await
            .unwrap_err(),
        "application",
    );
    assert_invalid_argument_contains(
        store
            .retrieve_effective_config(RetrieveEffectiveConfigQuery {
                service_name: " ".to_string(),
                ..effective_query()
            })
            .await
            .unwrap_err(),
        "service_name",
    );
    assert_invalid_argument_contains(
        store
            .retrieve_effective_config(RetrieveEffectiveConfigQuery {
                group: " ".to_string(),
                ..effective_query()
            })
            .await
            .unwrap_err(),
        "group",
    );
}

#[tokio::test]
async fn create_config_draft_rejects_blank_audit_and_scope_fields() {
    let mut store = MemoryDiscoveryStore::new();

    assert_invalid_argument_contains(
        store
            .create_config_draft(CreateConfigDraftCommand {
                created_by: " ".to_string(),
                ..draft("log.level", "debug", ConfigScope::Namespace)
            })
            .await
            .unwrap_err(),
        "created_by",
    );
    assert_invalid_argument_contains(
        store
            .create_config_draft(draft(
                "log.level",
                "debug",
                ConfigScope::Application {
                    application: " ".to_string(),
                },
            ))
            .await
            .unwrap_err(),
        "application",
    );
    assert_invalid_argument_contains(
        store
            .create_config_draft(draft(
                "log.level",
                "debug",
                ConfigScope::Service {
                    application: " ".to_string(),
                    service_name: "sdkwork-drive-product".to_string(),
                },
            ))
            .await
            .unwrap_err(),
        "application",
    );
    assert_invalid_argument_contains(
        store
            .create_config_draft(draft(
                "log.level",
                "debug",
                ConfigScope::Service {
                    application: "sdkwork-drive".to_string(),
                    service_name: " ".to_string(),
                },
            ))
            .await
            .unwrap_err(),
        "service_name",
    );
}

#[tokio::test]
async fn publish_config_rejects_blank_required_fields() {
    let mut store = MemoryDiscoveryStore::new();
    let created = store
        .create_config_draft(draft("feature.enabled", "true", ConfigScope::Namespace))
        .await
        .unwrap();

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
                draft_id: created.draft_id,
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
async fn config_writes_reject_blank_idempotency_fields() {
    let mut store = MemoryDiscoveryStore::new();

    for (field, idempotency) in invalid_idempotency_cases() {
        assert_invalid_argument_contains(
            store
                .create_config_draft(CreateConfigDraftCommand {
                    idempotency: Some(idempotency),
                    ..draft("log.level", "debug", ConfigScope::Namespace)
                })
                .await
                .unwrap_err(),
            field,
        );
    }

    let publish_draft = store
        .create_config_draft(draft("feature.enabled", "true", ConfigScope::Namespace))
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
        .create_config_draft(draft("feature.mode", "stable", ConfigScope::Namespace))
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
                .rollback_config(RollbackConfigCommand {
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
async fn config_writes_replay_same_idempotency_key_and_reject_hash_conflicts() {
    let mut store = MemoryDiscoveryStore::new();

    let first_draft = store
        .create_config_draft(draft_with_idempotency(
            "log.level",
            "debug",
            IdempotencyContext::new(
                "discovery.config.drafts.create",
                "draft-key-1",
                "sha256:draft-hash-1",
            ),
        ))
        .await
        .unwrap();
    let replayed_draft = store
        .create_config_draft(draft_with_idempotency(
            "log.level",
            "debug",
            IdempotencyContext::new(
                "discovery.config.drafts.create",
                "draft-key-1",
                "sha256:draft-hash-1",
            ),
        ))
        .await
        .unwrap();

    assert_eq!(replayed_draft.draft_id, first_draft.draft_id);
    assert_eq!(replayed_draft.content_hash, first_draft.content_hash);

    let conflict = store
        .create_config_draft(draft_with_idempotency(
            "log.level",
            "trace",
            IdempotencyContext::new(
                "discovery.config.drafts.create",
                "draft-key-1",
                "sha256:different-draft-hash",
            ),
        ))
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
async fn publishing_the_same_draft_twice_is_rejected() {
    let mut store = MemoryDiscoveryStore::new();
    let created = store
        .create_config_draft(draft("feature.enabled", "true", ConfigScope::Namespace))
        .await
        .unwrap();

    store
        .publish_config(PublishConfigCommand {
            draft_id: created.draft_id.clone(),
            published_by: "operator-1".to_string(),
            now_ms: 1_000,
            idempotency: None,
        })
        .await
        .unwrap();

    let error = store
        .publish_config(PublishConfigCommand {
            draft_id: created.draft_id,
            published_by: "operator-1".to_string(),
            now_ms: 2_000,
            idempotency: None,
        })
        .await
        .unwrap_err();

    assert!(error.to_string().contains("already published"));
}

#[tokio::test]
async fn effective_config_uses_latest_release_when_scope_specificity_matches() {
    let mut store = MemoryDiscoveryStore::new();
    let first = store
        .create_config_draft(draft("log.level", "info", ConfigScope::Namespace))
        .await
        .unwrap();
    let second = store
        .create_config_draft(draft("log.level", "warn", ConfigScope::Namespace))
        .await
        .unwrap();

    store
        .publish_config(PublishConfigCommand {
            draft_id: first.draft_id,
            published_by: "operator-1".to_string(),
            now_ms: 1_000,
            idempotency: None,
        })
        .await
        .unwrap();
    let second_release = store
        .publish_config(PublishConfigCommand {
            draft_id: second.draft_id,
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
    assert_eq!(value.value, "warn");
    assert_eq!(value.source_release_id, second_release.release_id);
    assert_eq!(value.source_revision, second_release.revision);
}

#[tokio::test]
async fn rollback_republishes_a_historical_release_as_latest_effective_config() {
    let mut store = MemoryDiscoveryStore::new();
    let first = store
        .create_config_draft(draft("log.level", "info", ConfigScope::Namespace))
        .await
        .unwrap();
    let second = store
        .create_config_draft(draft("log.level", "warn", ConfigScope::Namespace))
        .await
        .unwrap();

    let first_release = store
        .publish_config(PublishConfigCommand {
            draft_id: first.draft_id,
            published_by: "operator-1".to_string(),
            now_ms: 1_000,
            idempotency: None,
        })
        .await
        .unwrap();
    store
        .publish_config(PublishConfigCommand {
            draft_id: second.draft_id,
            published_by: "operator-1".to_string(),
            now_ms: 2_000,
            idempotency: None,
        })
        .await
        .unwrap();

    let rollback = store
        .rollback_config(RollbackConfigCommand {
            source_release_id: first_release.release_id.clone(),
            rolled_back_by: "operator-2".to_string(),
            now_ms: 3_000,
            idempotency: None,
        })
        .await
        .unwrap();

    assert_eq!(rollback.revision, 3);
    assert_eq!(rollback.value, "info");
    assert_eq!(rollback.draft_id, first_release.draft_id);
    assert_ne!(rollback.release_id, first_release.release_id);

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
    assert_eq!(value.value, "info");
    assert_eq!(value.source_release_id, rollback.release_id);
}

fn effective_query() -> RetrieveEffectiveConfigQuery {
    RetrieveEffectiveConfigQuery {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        application: "sdkwork-drive".to_string(),
        service_name: "sdkwork-drive-product".to_string(),
        group: "runtime".to_string(),
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
