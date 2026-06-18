use sdkwork_discovery_contract::{
    CallerContext, ConfigFormat, ConfigPermission, ConfigScope, CreateConfigDraftCommand,
    DiscoverInstancesQuery, InstanceStatus, ListServicesQuery, PublishConfigCommand,
    RegisterInstanceCommand, RegistryPermission, RenewLeaseCommand, ReportInstanceStatusCommand,
    RetrieveEffectiveConfigQuery, RetrieveInstanceQuery, RollbackConfigCommand, WatchEventsQuery,
};
use sdkwork_discovery_core::{ConfigPolicy, DiscoveryControlPlane, RegistryPolicy};
use sdkwork_discovery_storage_memory::MemoryDiscoveryStore;

fn control_plane() -> DiscoveryControlPlane<MemoryDiscoveryStore> {
    DiscoveryControlPlane::new(
        MemoryDiscoveryStore::new(),
        ConfigPolicy {
            enabled: true,
            require_publish_for_reads: true,
            allow_secret_values: false,
            allow_secret_refs: true,
            max_config_body_bytes: 1024,
        },
        RegistryPolicy::default(),
    )
}

fn draft(value: &str) -> CreateConfigDraftCommand {
    CreateConfigDraftCommand {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        group: "runtime".to_string(),
        key: "database.password".to_string(),
        format: ConfigFormat::Text,
        value: value.to_string(),
        scope: ConfigScope::Namespace,
        created_by: "operator-1".to_string(),
        idempotency: None,
    }
}

fn draft_with_format(value: &str, format: ConfigFormat) -> CreateConfigDraftCommand {
    CreateConfigDraftCommand {
        format,
        ..draft(value)
    }
}

#[tokio::test]
async fn config_publish_requires_publish_permission() {
    let mut control = control_plane();
    let read_only = CallerContext::new("operator-1").with_config_permission(ConfigPermission::Read);

    let error = control
        .create_config_draft(&read_only, draft("plain-secret"))
        .await
        .unwrap_err();

    assert!(error.to_string().contains("permission"));
}

#[tokio::test]
async fn config_policy_rejects_literal_secret_values() {
    let mut control = control_plane();
    let publisher =
        CallerContext::new("operator-1").with_config_permission(ConfigPermission::Publish);

    let error = control
        .create_config_draft(&publisher, draft("password = \"plain-secret\""))
        .await
        .unwrap_err();

    assert!(error.to_string().contains("secret"));
}

#[tokio::test]
async fn config_policy_rejects_invalid_json_and_toml_bodies() {
    let mut control = control_plane();
    let publisher =
        CallerContext::new("operator-1").with_config_permission(ConfigPermission::Publish);

    let json_error = control
        .create_config_draft(
            &publisher,
            draft_with_format("{\"log.level\":", ConfigFormat::Json),
        )
        .await
        .unwrap_err();
    let toml_error = control
        .create_config_draft(
            &publisher,
            draft_with_format("log.level =", ConfigFormat::Toml),
        )
        .await
        .unwrap_err();

    assert!(json_error.to_string().contains("JSON"));
    assert!(toml_error.to_string().contains("TOML"));
}

#[tokio::test]
async fn config_policy_accepts_valid_json_and_toml_bodies() {
    let mut control = control_plane();
    let publisher =
        CallerContext::new("operator-1").with_config_permission(ConfigPermission::Publish);

    let json = control
        .create_config_draft(
            &publisher,
            draft_with_format(r#"{"log_level":"info"}"#, ConfigFormat::Json),
        )
        .await
        .unwrap();
    let toml = control
        .create_config_draft(
            &publisher,
            draft_with_format("log_level = \"info\"", ConfigFormat::Toml),
        )
        .await
        .unwrap();

    assert_eq!(json.format, ConfigFormat::Json);
    assert_eq!(toml.format, ConfigFormat::Toml);
}

#[tokio::test]
async fn publisher_can_create_and_publish_secret_reference() {
    let mut control = control_plane();
    let publisher =
        CallerContext::new("operator-1").with_config_permission(ConfigPermission::Publish);

    let created = control
        .create_config_draft(
            &publisher,
            draft("secret_ref:sdkwork://secrets/discovery/postgres-password"),
        )
        .await
        .unwrap();
    let release = control
        .publish_config(
            &publisher,
            PublishConfigCommand {
                draft_id: created.draft_id,
                published_by: "operator-1".to_string(),
                now_ms: 1_000,
                idempotency: None,
            },
        )
        .await
        .unwrap();

    assert_eq!(release.revision, 1);
}

#[tokio::test]
async fn config_reader_can_retrieve_effective_config_but_not_rollback() {
    let mut control = control_plane();
    let publisher =
        CallerContext::new("operator-1").with_config_permission(ConfigPermission::Publish);
    let reader = CallerContext::new("service-1").with_config_permission(ConfigPermission::Read);
    let created = control
        .create_config_draft(&publisher, draft("log.level=info"))
        .await
        .unwrap();
    let release = control
        .publish_config(
            &publisher,
            PublishConfigCommand {
                draft_id: created.draft_id,
                published_by: "operator-1".to_string(),
                now_ms: 1_000,
                idempotency: None,
            },
        )
        .await
        .unwrap();

    let effective = control
        .retrieve_effective_config(
            &reader,
            RetrieveEffectiveConfigQuery {
                namespace: "sdkwork".to_string(),
                environment: "development".to_string(),
                application: "sdkwork-drive".to_string(),
                service_name: "sdkwork-drive-product".to_string(),
                group: "runtime".to_string(),
            },
        )
        .await
        .unwrap();
    let rollback_error = control
        .rollback_config(
            &reader,
            RollbackConfigCommand {
                source_release_id: release.release_id,
                rolled_back_by: "operator-2".to_string(),
                now_ms: 2_000,
                idempotency: None,
            },
        )
        .await
        .unwrap_err();

    assert_eq!(
        effective.values.get("database.password").unwrap().value,
        "log.level=info"
    );
    assert!(rollback_error.to_string().contains("permission"));
}

#[tokio::test]
async fn config_policy_requires_published_config_for_reads_by_default() {
    let control = control_plane();
    let reader = CallerContext::new("service-1").with_config_permission(ConfigPermission::Read);

    let error = control
        .retrieve_effective_config(
            &reader,
            RetrieveEffectiveConfigQuery {
                namespace: "sdkwork".to_string(),
                environment: "development".to_string(),
                application: "sdkwork-drive".to_string(),
                service_name: "sdkwork-drive-product".to_string(),
                group: "runtime".to_string(),
            },
        )
        .await
        .unwrap_err();

    assert!(error.to_string().contains("published config"));
}

#[tokio::test]
async fn config_policy_can_allow_empty_effective_config_for_reads() {
    let control = DiscoveryControlPlane::new(
        MemoryDiscoveryStore::new(),
        ConfigPolicy {
            enabled: true,
            require_publish_for_reads: false,
            allow_secret_values: false,
            allow_secret_refs: true,
            max_config_body_bytes: 1024,
        },
        RegistryPolicy::default(),
    );
    let reader = CallerContext::new("service-1").with_config_permission(ConfigPermission::Read);

    let effective = control
        .retrieve_effective_config(
            &reader,
            RetrieveEffectiveConfigQuery {
                namespace: "sdkwork".to_string(),
                environment: "development".to_string(),
                application: "sdkwork-drive".to_string(),
                service_name: "sdkwork-drive-product".to_string(),
                group: "runtime".to_string(),
            },
        )
        .await
        .unwrap();

    assert_eq!(effective.revision, 0);
    assert!(effective.values.is_empty());
}

#[tokio::test]
async fn disabled_config_registry_rejects_config_reads_and_writes() {
    let mut control = DiscoveryControlPlane::new(
        MemoryDiscoveryStore::new(),
        ConfigPolicy {
            enabled: false,
            require_publish_for_reads: false,
            allow_secret_values: false,
            allow_secret_refs: true,
            max_config_body_bytes: 1024,
        },
        RegistryPolicy::default(),
    );
    let publisher =
        CallerContext::new("operator-1").with_config_permission(ConfigPermission::Publish);
    let reader = CallerContext::new("service-1").with_config_permission(ConfigPermission::Read);

    let create_error = control
        .create_config_draft(&publisher, draft("log.level=info"))
        .await
        .unwrap_err();
    let read_error = control
        .retrieve_effective_config(
            &reader,
            RetrieveEffectiveConfigQuery {
                namespace: "sdkwork".to_string(),
                environment: "development".to_string(),
                application: "sdkwork-drive".to_string(),
                service_name: "sdkwork-drive-product".to_string(),
                group: "runtime".to_string(),
            },
        )
        .await
        .unwrap_err();

    assert!(create_error.to_string().contains("config registry"));
    assert!(create_error.to_string().contains("disabled"));
    assert!(read_error.to_string().contains("config registry"));
    assert!(read_error.to_string().contains("disabled"));
}

#[tokio::test]
async fn disabled_config_registry_rejects_config_watch_events() {
    let control = DiscoveryControlPlane::new(
        MemoryDiscoveryStore::new(),
        ConfigPolicy {
            enabled: false,
            require_publish_for_reads: false,
            allow_secret_values: false,
            allow_secret_refs: true,
            max_config_body_bytes: 1024,
        },
        RegistryPolicy::default(),
    );
    let reader = CallerContext::new("service-1").with_config_permission(ConfigPermission::Read);

    let error = control
        .watch_config_events(
            &reader,
            WatchEventsQuery {
                namespace: "sdkwork".to_string(),
                environment: "development".to_string(),
                from_revision: 0,
                service_name: Some("sdkwork-drive-product".to_string()),
                config_group: Some("runtime".to_string()),
                config_application: Some("sdkwork-drive".to_string()),
                max_events: 1_024,
            },
        )
        .await
        .unwrap_err();

    assert!(error.to_string().contains("config registry"));
    assert!(error.to_string().contains("disabled"));
}

#[tokio::test]
async fn config_rollback_requires_rollback_permission() {
    let mut control = control_plane();
    let publisher =
        CallerContext::new("operator-1").with_config_permission(ConfigPermission::Publish);
    let rollbacker =
        CallerContext::new("operator-2").with_config_permission(ConfigPermission::Rollback);
    let created = control
        .create_config_draft(&publisher, draft("feature.enabled=true"))
        .await
        .unwrap();
    let release = control
        .publish_config(
            &publisher,
            PublishConfigCommand {
                draft_id: created.draft_id,
                published_by: "operator-1".to_string(),
                now_ms: 1_000,
                idempotency: None,
            },
        )
        .await
        .unwrap();

    let rollback = control
        .rollback_config(
            &rollbacker,
            RollbackConfigCommand {
                source_release_id: release.release_id,
                rolled_back_by: "operator-2".to_string(),
                now_ms: 2_000,
                idempotency: None,
            },
        )
        .await
        .unwrap();

    assert_eq!(rollback.value, "feature.enabled=true");
    assert_eq!(rollback.revision, 2);
}

#[tokio::test]
async fn registry_registration_requires_write_permission() {
    let mut control = control_plane();
    let read_only =
        CallerContext::new("service-1").with_registry_permission(RegistryPermission::Read);

    let error = control
        .register_instance(
            &read_only,
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
                expected_revision: None,
                persistent: false,
                health_check: None,
            },
        )
        .await
        .unwrap_err();

    assert!(error.to_string().contains("permission"));
}

#[tokio::test]
async fn registry_reader_can_discover_instances_but_not_write() {
    let mut control = control_plane();
    let writer =
        CallerContext::new("service-1").with_registry_permission(RegistryPermission::Write);
    let reader = CallerContext::new("service-2").with_registry_permission(RegistryPermission::Read);

    control
        .register_instance(
            &writer,
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
                expected_revision: None,
                persistent: false,
                health_check: None,
            },
        )
        .await
        .unwrap();

    let discovered = control
        .discover_instances(
            &reader,
            DiscoverInstancesQuery {
                namespace: "sdkwork".to_string(),
                environment: "development".to_string(),
                service_name: "sdkwork-drive-product".to_string(),
                healthy_only: true,
                protocol: Some("grpc".to_string()),
                label_filters: vec![],
                sort_by: None,
            },
            2_000,
        )
        .await
        .unwrap();

    assert_eq!(discovered.instances.len(), 1);
}

#[tokio::test]
async fn registry_reader_can_retrieve_instance_by_identity_but_config_reader_cannot() {
    let mut control = control_plane();
    let writer =
        CallerContext::new("service-1").with_registry_permission(RegistryPermission::Write);
    let registry_reader =
        CallerContext::new("service-2").with_registry_permission(RegistryPermission::Read);
    let config_reader =
        CallerContext::new("service-3").with_config_permission(ConfigPermission::Read);

    control
        .register_instance(
            &writer,
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
                expected_revision: None,
                persistent: false,
                health_check: None,
            },
        )
        .await
        .unwrap();

    let query = RetrieveInstanceQuery {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        service_name: "sdkwork-drive-product".to_string(),
        instance_id: "drive-1".to_string(),
    };
    let denied = control
        .retrieve_instance(&config_reader, query.clone(), 2_000)
        .await
        .unwrap_err();
    let instance = control
        .retrieve_instance(&registry_reader, query, 2_000)
        .await
        .unwrap()
        .expect("registered instance should be visible to registry readers");

    assert!(denied.to_string().contains("registry permission"));
    assert_eq!(instance.instance_id, "drive-1");
}

#[tokio::test]
async fn registry_reader_retrieve_instance_excludes_expired_lease_by_identity() {
    let mut control = control_plane();
    let writer =
        CallerContext::new("service-1").with_registry_permission(RegistryPermission::Write);
    let reader = CallerContext::new("service-2").with_registry_permission(RegistryPermission::Read);

    control
        .register_instance(
            &writer,
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
                expected_revision: None,
                persistent: false,
                health_check: None,
            },
        )
        .await
        .unwrap();

    let query = RetrieveInstanceQuery {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        service_name: "sdkwork-drive-product".to_string(),
        instance_id: "drive-1".to_string(),
    };
    let active = control
        .retrieve_instance(&reader, query.clone(), 2_000)
        .await
        .unwrap();
    let expired = control
        .retrieve_instance(&reader, query, 31_001)
        .await
        .unwrap();

    assert!(active.is_some());
    assert!(expired.is_none());
}

#[tokio::test]
async fn watch_registry_events_requires_registry_read_permission() {
    let control = control_plane();
    let config_reader =
        CallerContext::new("service-1").with_config_permission(ConfigPermission::Read);
    let registry_reader =
        CallerContext::new("service-2").with_registry_permission(RegistryPermission::Read);
    let query = WatchEventsQuery {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        from_revision: 0,
        service_name: Some("sdkwork-drive-product".to_string()),
        config_group: None,
        config_application: None,
        max_events: 1_024,
    };

    let error = control
        .watch_registry_events(&config_reader, query.clone())
        .await
        .unwrap_err();
    let events = control
        .watch_registry_events(&registry_reader, query)
        .await
        .unwrap();

    assert!(error.to_string().contains("registry permission"));
    assert!(events.is_empty());
}

#[tokio::test]
async fn watch_config_events_requires_config_read_permission() {
    let control = control_plane();
    let registry_reader =
        CallerContext::new("service-1").with_registry_permission(RegistryPermission::Read);
    let config_reader =
        CallerContext::new("service-2").with_config_permission(ConfigPermission::Read);
    let query = WatchEventsQuery {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        from_revision: 0,
        service_name: Some("sdkwork-drive-product".to_string()),
        config_group: Some("runtime".to_string()),
        config_application: Some("sdkwork-drive".to_string()),
        max_events: 1_024,
    };

    let error = control
        .watch_config_events(&registry_reader, query.clone())
        .await
        .unwrap_err();
    let events = control
        .watch_config_events(&config_reader, query)
        .await
        .unwrap();

    assert!(error.to_string().contains("config permission"));
    assert!(events.is_empty());
}

#[tokio::test]
async fn registry_status_report_requires_write_permission() {
    let mut control = control_plane();
    let writer =
        CallerContext::new("service-1").with_registry_permission(RegistryPermission::Write);
    let reader = CallerContext::new("service-2").with_registry_permission(RegistryPermission::Read);

    control
        .register_instance(
            &writer,
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
                expected_revision: None,
                persistent: false,
                health_check: None,
            },
        )
        .await
        .unwrap();

    let error = control
        .report_instance_status(
            &reader,
            ReportInstanceStatusCommand {
                namespace: "sdkwork".to_string(),
                environment: "development".to_string(),
                service_name: "sdkwork-drive-product".to_string(),
                instance_id: "drive-1".to_string(),
                status: InstanceStatus::NotServing,
                now_ms: 2_000,
                expected_revision: None,
            },
        )
        .await
        .unwrap_err();

    assert!(error.to_string().contains("permission"));
}

#[tokio::test]
async fn registry_writer_cannot_report_status_for_expired_instance() {
    let mut control = control_plane();
    let writer =
        CallerContext::new("service-1").with_registry_permission(RegistryPermission::Write);

    control
        .register_instance(
            &writer,
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
                lease_ttl_seconds: 1,
                now_ms: 1_000,
                expected_revision: None,
                persistent: false,
                health_check: None,
            },
        )
        .await
        .unwrap();

    let error = control
        .report_instance_status(
            &writer,
            ReportInstanceStatusCommand {
                namespace: "sdkwork".to_string(),
                environment: "development".to_string(),
                service_name: "sdkwork-drive-product".to_string(),
                instance_id: "drive-1".to_string(),
                status: InstanceStatus::NotServing,
                now_ms: 2_001,
                expected_revision: None,
            },
        )
        .await
        .unwrap_err();

    assert!(error.to_string().contains("instance"));
}

#[tokio::test]
async fn registry_reader_can_list_services() {
    let mut control = control_plane();
    let writer =
        CallerContext::new("service-1").with_registry_permission(RegistryPermission::Write);
    let reader =
        CallerContext::new("operator-1").with_registry_permission(RegistryPermission::Read);
    control
        .register_instance(
            &writer,
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
                expected_revision: None,
                persistent: false,
                health_check: None,
            },
        )
        .await
        .unwrap();

    let services = control
        .list_services(
            &reader,
            ListServicesQuery {
                namespace: "sdkwork".to_string(),
                environment: "development".to_string(),
            },
            2_000,
        )
        .await
        .unwrap();

    assert_eq!(services.services.len(), 1);
    assert_eq!(services.services[0].service_name, "sdkwork-drive-product");
}

#[tokio::test]
async fn registry_renew_and_deregister_require_write_permission() {
    let mut control = control_plane();
    let writer =
        CallerContext::new("service-1").with_registry_permission(RegistryPermission::Write);
    let reader = CallerContext::new("service-2").with_registry_permission(RegistryPermission::Read);

    let registered = control
        .register_instance(
            &writer,
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
                expected_revision: None,
                persistent: false,
                health_check: None,
            },
        )
        .await
        .unwrap();

    let renew_error = control
        .renew_lease(
            &reader,
            RenewLeaseCommand {
                lease_id: registered.lease_id.clone(),
                lease_ttl_seconds: 30,
                now_ms: 2_000,
            },
        )
        .await
        .unwrap_err();
    let deregister_error = control
        .deregister_instance(
            &reader,
            "sdkwork",
            "development",
            "sdkwork-drive-product",
            "drive-1",
            2_000,
        )
        .await
        .unwrap_err();

    assert!(renew_error.to_string().contains("permission"));
    assert!(deregister_error.to_string().contains("permission"));

    let renewed = control
        .renew_lease(
            &writer,
            RenewLeaseCommand {
                lease_id: registered.lease_id,
                lease_ttl_seconds: 60,
                now_ms: 2_000,
            },
        )
        .await
        .unwrap();
    control
        .deregister_instance(
            &writer,
            "sdkwork",
            "development",
            "sdkwork-drive-product",
            "drive-1",
            2_500,
        )
        .await
        .unwrap();

    assert_eq!(renewed.expires_at_ms, 62_000);
}

#[tokio::test]
async fn registry_writer_deregister_expired_instance_is_idempotent_noop() {
    let mut control = control_plane();
    let writer =
        CallerContext::new("service-1").with_registry_permission(RegistryPermission::Write);

    control
        .register_instance(
            &writer,
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
                lease_ttl_seconds: 1,
                now_ms: 1_000,
                expected_revision: None,
                persistent: false,
                health_check: None,
            },
        )
        .await
        .unwrap();

    let result = control
        .deregister_instance(
            &writer,
            "sdkwork",
            "development",
            "sdkwork-drive-product",
            "drive-1",
            2_001,
        )
        .await
        .unwrap();

    assert!(!result.deregistered);
    assert_eq!(result.revision, 0);
}

#[tokio::test]
async fn registry_expire_instances_is_internal_maintenance_and_emits_watch_events() {
    let mut control = control_plane();
    let writer =
        CallerContext::new("service-1").with_registry_permission(RegistryPermission::Write);
    let reader = CallerContext::new("service-2").with_registry_permission(RegistryPermission::Read);

    control
        .register_instance(
            &writer,
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
                lease_ttl_seconds: 1,
                now_ms: 1_000,
                expected_revision: None,
                persistent: false,
                health_check: None,
            },
        )
        .await
        .unwrap();
    control
        .register_instance(
            &writer,
            RegisterInstanceCommand {
                namespace: "sdkwork".to_string(),
                environment: "development".to_string(),
                service_name: "sdkwork-drive-product".to_string(),
                instance_id: "drive-2".to_string(),
                endpoint: "grpc://127.0.0.1:50052".to_string(),
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
                expected_revision: None,
                persistent: false,
                health_check: None,
            },
        )
        .await
        .unwrap();

    let expired = control.expire_instances(2_001, 1_000).await.unwrap();

    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].instance_id, "drive-1");
    assert_eq!(expired[0].revision, 3);

    let discovered = control
        .discover_instances(
            &reader,
            DiscoverInstancesQuery {
                namespace: "sdkwork".to_string(),
                environment: "development".to_string(),
                service_name: "sdkwork-drive-product".to_string(),
                healthy_only: true,
                protocol: Some("grpc".to_string()),
                label_filters: vec![],
                sort_by: None,
            },
            2_001,
        )
        .await
        .unwrap();
    let events = control
        .watch_registry_events(
            &reader,
            WatchEventsQuery {
                namespace: "sdkwork".to_string(),
                environment: "development".to_string(),
                from_revision: 2,
                service_name: None,
                config_group: None,
                config_application: None,
                max_events: 1_024,
            },
        )
        .await
        .unwrap();

    assert_eq!(discovered.instances.len(), 1);
    assert_eq!(discovered.instances[0].instance_id, "drive-2");
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].kind,
        sdkwork_discovery_contract::DiscoveryEventKind::InstanceDeregistered
    );
    assert_eq!(events[0].resource_id, "drive-1");
}

#[tokio::test]
async fn registry_writer_cannot_renew_expired_lease() {
    let mut control = control_plane();
    let writer =
        CallerContext::new("service-1").with_registry_permission(RegistryPermission::Write);

    let registered = control
        .register_instance(
            &writer,
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
                lease_ttl_seconds: 1,
                now_ms: 1_000,
                expected_revision: None,
                persistent: false,
                health_check: None,
            },
        )
        .await
        .unwrap();

    let error = control
        .renew_lease(
            &writer,
            RenewLeaseCommand {
                lease_id: registered.lease_id,
                lease_ttl_seconds: 30,
                now_ms: 2_001,
            },
        )
        .await
        .unwrap_err();

    assert!(error.to_string().contains("lease"));
}

#[tokio::test]
async fn registry_policy_rejects_register_ttl_outside_configured_bounds() {
    let mut control = DiscoveryControlPlane::new(
        MemoryDiscoveryStore::new(),
        ConfigPolicy {
            enabled: true,
            require_publish_for_reads: true,
            allow_secret_values: false,
            allow_secret_refs: true,
            max_config_body_bytes: 1024,
        },
        RegistryPolicy {
            default_lease_ttl_seconds: 30,
            min_lease_ttl_seconds: 5,
            max_lease_ttl_seconds: 300,
        },
    );
    let writer =
        CallerContext::new("service-1").with_registry_permission(RegistryPermission::Write);
    let mut command = RegisterInstanceCommand {
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
        lease_ttl_seconds: 4,
        now_ms: 1_000,
        expected_revision: None,
        persistent: false,
        health_check: None,
    };

    let below_min = control
        .register_instance(&writer, command.clone())
        .await
        .unwrap_err();
    command.lease_ttl_seconds = 301;
    let above_max = control
        .register_instance(&writer, command)
        .await
        .unwrap_err();

    assert!(below_min.to_string().contains("lease ttl"));
    assert!(above_max.to_string().contains("lease ttl"));
}

#[tokio::test]
async fn registry_policy_rejects_renew_ttl_outside_configured_bounds() {
    let mut control = DiscoveryControlPlane::new(
        MemoryDiscoveryStore::new(),
        ConfigPolicy {
            enabled: true,
            require_publish_for_reads: true,
            allow_secret_values: false,
            allow_secret_refs: true,
            max_config_body_bytes: 1024,
        },
        RegistryPolicy {
            default_lease_ttl_seconds: 30,
            min_lease_ttl_seconds: 5,
            max_lease_ttl_seconds: 300,
        },
    );
    let writer =
        CallerContext::new("service-1").with_registry_permission(RegistryPermission::Write);
    let registered = control
        .register_instance(
            &writer,
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
                expected_revision: None,
                persistent: false,
                health_check: None,
            },
        )
        .await
        .unwrap();

    let below_min = control
        .renew_lease(
            &writer,
            RenewLeaseCommand {
                lease_id: registered.lease_id.clone(),
                lease_ttl_seconds: 4,
                now_ms: 2_000,
            },
        )
        .await
        .unwrap_err();
    let above_max = control
        .renew_lease(
            &writer,
            RenewLeaseCommand {
                lease_id: registered.lease_id,
                lease_ttl_seconds: 301,
                now_ms: 2_000,
            },
        )
        .await
        .unwrap_err();

    assert!(below_min.to_string().contains("lease ttl"));
    assert!(above_max.to_string().contains("lease ttl"));
}
