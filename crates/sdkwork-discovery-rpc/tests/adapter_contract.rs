use async_trait::async_trait;
use sdkwork_discovery_contract::DiscoveryError;
use sdkwork_discovery_contract::{
    BatchRegisterResult, ConfigDraft, ConfigRelease, CreateConfigDraftCommand,
    DeregisterInstanceResult, DiscoverInstancesQuery, DiscoverInstancesResult, DiscoveryEvent,
    DiscoveryResult, EffectiveConfig, InstanceStatus, ListServicesQuery, ListServicesResult,
    PublishConfigCommand, RegisterInstanceCommand, RegisterInstanceResult, RenewLeaseCommand,
    RenewLeaseResult, ReportInstanceStatusCommand, ReportInstanceStatusResult,
    RetrieveEffectiveConfigQuery, RetrieveInstanceQuery, RollbackConfigCommand, ServiceInstance,
    WatchEventsQuery,
};
use sdkwork_discovery_core::{ConfigPolicy, DiscoveryControlPlane, RegistryPolicy};
use sdkwork_discovery_rpc::{
    discovery_rpc_service_manifest, map_discovery_error_to_status, DiscoveryAdminRpcService,
    DiscoveryConfigRpcService, DiscoveryRpcRuntime, DiscoveryRpcRuntimeConfig,
    DiscoveryRpcServiceTokenVerifierConfig, DiscoveryWatchRpcService, RegistryRpcService,
    RuntimeResilienceConfig,
};
use sdkwork_discovery_rpc_proto::sdkwork::discovery::backend::v3::discovery_admin_service_server::DiscoveryAdminService;
use sdkwork_discovery_rpc_proto::sdkwork::discovery::backend::v3::{
    CreateConfigDraftRequest, ListServicesRequest, PublishConfigRequest, RollbackConfigRequest,
};
use sdkwork_discovery_rpc_proto::sdkwork::discovery::common::v1 as common_proto;
use sdkwork_discovery_rpc_proto::sdkwork::discovery::common::v1::InstanceStatus as ProtoInstanceStatus;
use sdkwork_discovery_rpc_proto::sdkwork::discovery::common::v1::{
    ConfigFormat as ProtoConfigFormat, ConfigScopeType as ProtoConfigScopeType,
};
use sdkwork_discovery_rpc_proto::sdkwork::discovery::internal::v1::discovery_config_service_server::DiscoveryConfigService;
use sdkwork_discovery_rpc_proto::sdkwork::discovery::internal::v1::discovery_watch_service_server::DiscoveryWatchService;
use sdkwork_discovery_rpc_proto::sdkwork::discovery::internal::v1::registry_service_server::RegistryService;
use sdkwork_discovery_rpc_proto::sdkwork::discovery::internal::v1::DeregisterInstanceRequest;
use sdkwork_discovery_rpc_proto::sdkwork::discovery::internal::v1::DiscoverInstancesRequest;
use sdkwork_discovery_rpc_proto::sdkwork::discovery::internal::v1::RegisterInstanceRequest;
use sdkwork_discovery_rpc_proto::sdkwork::discovery::internal::v1::ReportInstanceStatusRequest;
use sdkwork_discovery_rpc_proto::sdkwork::discovery::internal::v1::RetrieveEffectiveConfigRequest;
use sdkwork_discovery_rpc_proto::sdkwork::discovery::internal::v1::RetrieveInstanceRequest;
use sdkwork_discovery_rpc_proto::sdkwork::discovery::internal::v1::WatchConfigRequest;
use sdkwork_discovery_rpc_proto::sdkwork::discovery::internal::v1::WatchServiceRequest;
use sdkwork_discovery_storage_contract::{ConfigStore, RegistryStore, WatchEventStore};
use sdkwork_discovery_storage_memory::MemoryDiscoveryStore;
use serde_json::Value;
use tokio::time::{timeout, Duration};
use tokio_stream::StreamExt;
use tonic::{Code, Request};

#[test]
fn service_manifest_declares_standard_discovery_methods() {
    let manifest = discovery_rpc_service_manifest();
    let methods = manifest
        .methods
        .iter()
        .map(|method| method.operation_id)
        .collect::<Vec<_>>();

    assert!(methods.contains(&"discovery.registry.instances.register"));
    assert!(methods.contains(&"discovery.registry.instances.batch_register"));
    assert!(methods.contains(&"discovery.registry.instances.retrieve"));
    assert!(methods.contains(&"discovery.registry.instances.discover"));
    assert!(methods.contains(&"discovery.config.effective.retrieve"));
    assert!(methods.contains(&"discovery.config.drafts.create"));
    assert!(methods.contains(&"discovery.registry.services.list"));
    assert_eq!(manifest.methods.len(), 14);
    for method in &manifest.methods {
        assert!(
            matches!(method.surface, "internal" | "backend"),
            "manifest method {}.{}.{} must declare a standard surface",
            method.package,
            method.service,
            method.method
        );
        assert_eq!(method.owner, "sdkwork-platform");
        assert_eq!(method.compatibility, "stable");
    }
}

#[test]
fn service_manifest_matches_rpc_sdk_manifest_metadata() {
    let sdk_manifest =
        include_str!("../../../sdks/sdkwork-discovery-rpc-sdk/sdkwork-discovery-rpc.manifest.json");
    let sdk_manifest: Value = serde_json::from_str(sdk_manifest).unwrap();
    let manifest = discovery_rpc_service_manifest();
    let proto_roots = sdk_manifest["protoRoots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|root| root.as_str().unwrap())
        .collect::<Vec<_>>();

    assert_eq!(
        u64::from(manifest.schema_version),
        sdk_manifest["schemaVersion"].as_u64().unwrap()
    );
    assert_eq!(manifest.kind, sdk_manifest["kind"].as_str().unwrap());
    assert_eq!(manifest.domain, sdk_manifest["domain"].as_str().unwrap());
    assert_eq!(
        manifest.capability,
        sdk_manifest["capability"].as_str().unwrap()
    );
    assert_eq!(
        manifest.sdk_family,
        sdk_manifest["sdkFamily"].as_str().unwrap()
    );
    assert_eq!(manifest.proto_roots, proto_roots);
}

#[test]
fn service_manifest_matches_rpc_sdk_manifest_methods() {
    let sdk_manifest =
        include_str!("../../../sdks/sdkwork-discovery-rpc-sdk/sdkwork-discovery-rpc.manifest.json");
    let sdk_manifest: Value = serde_json::from_str(sdk_manifest).unwrap();
    let expected_methods = sdk_manifest_methods(&sdk_manifest);
    let actual_methods = discovery_rpc_service_manifest()
        .methods
        .iter()
        .map(|method| {
            vec![
                method.package,
                method.service,
                method.surface,
                method.method,
                method.operation_id,
                method.auth,
                method.idempotency,
                method.streaming,
                method.owner,
                method.compatibility,
            ]
        })
        .collect::<Vec<_>>();

    assert_eq!(actual_methods, expected_methods);
}

#[test]
fn discovery_errors_map_to_standard_grpc_status_codes() {
    assert_eq!(
        map_discovery_error_to_status(DiscoveryError::InvalidArgument("bad".to_string())).code(),
        Code::InvalidArgument
    );
    assert_eq!(
        map_discovery_error_to_status(DiscoveryError::Unauthenticated("missing".to_string()))
            .code(),
        Code::Unauthenticated
    );
    assert_eq!(
        map_discovery_error_to_status(DiscoveryError::PermissionDenied("no".to_string())).code(),
        Code::PermissionDenied
    );
    assert_eq!(
        map_discovery_error_to_status(DiscoveryError::NotFound("missing".to_string())).code(),
        Code::NotFound
    );
}

#[tokio::test]
async fn registry_service_rejects_missing_registry_write_permission() {
    let service = registry_service();
    let mut request = Request::new(register_request());
    add_auth_metadata(&mut request);
    request
        .metadata_mut()
        .insert("x-sdkwork-subject-id", "service-1".parse().unwrap());

    let status = service.register_instance(request).await.unwrap_err();

    assert_eq!(status.code(), Code::PermissionDenied);
}

#[tokio::test]
async fn registry_service_rejects_missing_dual_token_metadata_before_permissions() {
    let service = registry_service();
    let mut request = Request::new(register_request());
    request
        .metadata_mut()
        .insert("x-sdkwork-subject-id", "service-1".parse().unwrap());
    request
        .metadata_mut()
        .insert("x-sdkwork-registry-permissions", "write".parse().unwrap());

    let status = service.register_instance(request).await.unwrap_err();

    assert_eq!(status.code(), Code::Unauthenticated);
    assert!(status.message().contains("authorization"));
    assert!(status.message().contains("access-token"));
}

#[tokio::test]
async fn registry_service_rejects_missing_subject_identity_before_permissions() {
    let service = registry_service();
    let mut request = Request::new(register_request());
    add_auth_metadata(&mut request);
    request
        .metadata_mut()
        .insert("x-sdkwork-registry-permissions", "write".parse().unwrap());

    let status = service.register_instance(request).await.unwrap_err();

    assert_eq!(status.code(), Code::Unauthenticated);
    assert!(status.message().contains("x-sdkwork-subject-id"));
}

#[tokio::test]
async fn registry_service_rejects_unsigned_local_context_when_policy_disallows_it() {
    let service = RegistryRpcService::new(runtime_without_unsigned_local_context());
    let mut request = Request::new(register_request());
    add_registry_write_metadata(&mut request);

    let status = service.register_instance(request).await.unwrap_err();

    assert_eq!(status.code(), Code::Unauthenticated);
    assert!(status.message().contains("unsigned local context"));
}

#[tokio::test]
async fn registry_service_accepts_verified_service_token_when_unsigned_context_is_disabled() {
    let service = RegistryRpcService::new(runtime_with_service_token_verifier());
    let mut request = Request::new(register_request());
    add_verified_service_token_metadata(&mut request);

    let response = service
        .register_instance(request)
        .await
        .unwrap()
        .into_inner();

    assert_eq!(response.lease_id, "lease-1");
    assert!(response.expires_at.is_some());
}

#[tokio::test]
async fn registry_service_rejects_service_token_access_token_hash_mismatch() {
    let service = RegistryRpcService::new(runtime_with_service_token_verifier());
    let mut request = Request::new(register_request());
    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {VERIFIED_SERVICE_TOKEN}").parse().unwrap(),
    );
    request
        .metadata_mut()
        .insert("access-token", "wrong-access-token".parse().unwrap());

    let status = service.register_instance(request).await.unwrap_err();

    assert_eq!(status.code(), Code::Unauthenticated);
    assert!(status.message().contains("access-token"));
}

#[tokio::test]
async fn registry_service_rejects_unsigned_context_headers_with_verified_service_token() {
    let service = RegistryRpcService::new(runtime_with_service_token_verifier());
    let mut request = Request::new(register_request());
    add_verified_service_token_metadata(&mut request);
    request
        .metadata_mut()
        .insert("x-sdkwork-subject-id", "service-1".parse().unwrap());
    request
        .metadata_mut()
        .insert("x-sdkwork-registry-permissions", "write".parse().unwrap());

    let status = service.register_instance(request).await.unwrap_err();

    assert_eq!(status.code(), Code::Unauthenticated);
    assert!(status.message().contains("unsigned context metadata"));
}

#[tokio::test]
async fn admin_write_rejects_missing_required_idempotency_metadata_before_permissions() {
    let service = DiscoveryAdminRpcService::new(runtime());
    let mut request = Request::new(CreateConfigDraftRequest {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        group: "runtime".to_string(),
        key: "log.level".to_string(),
        format: ProtoConfigFormat::Text as i32,
        value: "debug".to_string(),
        scope_type: ProtoConfigScopeType::Service as i32,
        application: "sdkwork-drive".to_string(),
        service_name: "sdkwork-drive-product".to_string(),
    });
    add_auth_metadata(&mut request);
    request
        .metadata_mut()
        .insert("x-sdkwork-subject-id", "operator-1".parse().unwrap());
    request
        .metadata_mut()
        .insert("x-sdkwork-config-permissions", "publish".parse().unwrap());

    let status = service.create_config_draft(request).await.unwrap_err();

    assert_eq!(status.code(), Code::InvalidArgument);
    assert!(status.message().contains("idempotency-key"));
    assert!(status.message().contains("x-request-hash"));
}

#[tokio::test]
async fn admin_config_publish_replays_same_idempotency_key_without_second_release() {
    let runtime = runtime();
    let admin = DiscoveryAdminRpcService::new(runtime);

    let mut create = Request::new(CreateConfigDraftRequest {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        group: "runtime".to_string(),
        key: "log.level".to_string(),
        format: ProtoConfigFormat::Text as i32,
        value: "debug".to_string(),
        scope_type: ProtoConfigScopeType::Service as i32,
        application: "sdkwork-drive".to_string(),
        service_name: "sdkwork-drive-product".to_string(),
    });
    add_config_write_metadata(&mut create, "draft-key-replay", "sha256:draft-request-hash");
    let draft = admin
        .create_config_draft(create)
        .await
        .unwrap()
        .into_inner();

    let mut first_publish = Request::new(PublishConfigRequest {
        draft_id: draft.draft_id.clone(),
    });
    add_config_write_metadata(
        &mut first_publish,
        "publish-key-replay",
        "sha256:publish-request-hash",
    );
    let first_release = admin
        .publish_config(first_publish)
        .await
        .unwrap()
        .into_inner();

    let mut replayed_publish = Request::new(PublishConfigRequest {
        draft_id: draft.draft_id,
    });
    add_config_write_metadata(
        &mut replayed_publish,
        "publish-key-replay",
        "sha256:publish-request-hash",
    );
    let replayed_release = admin
        .publish_config(replayed_publish)
        .await
        .unwrap()
        .into_inner();

    assert_eq!(replayed_release.release_id, first_release.release_id);
    assert_eq!(
        replayed_release.metadata.unwrap().revision,
        first_release.metadata.unwrap().revision
    );
}

#[tokio::test]
async fn admin_config_rollback_uses_authenticated_caller_as_audit_actor() {
    let runtime = runtime();
    let admin = DiscoveryAdminRpcService::new(runtime);

    let mut create = Request::new(create_service_config_draft_request(
        "sdkwork-drive-product",
        "debug",
    ));
    add_config_write_metadata(
        &mut create,
        "draft-rollback-audit",
        "sha256:draft-rollback-audit",
    );
    let draft = admin
        .create_config_draft(create)
        .await
        .unwrap()
        .into_inner();

    let mut publish = Request::new(PublishConfigRequest {
        draft_id: draft.draft_id,
    });
    add_config_write_metadata(
        &mut publish,
        "publish-rollback-audit",
        "sha256:publish-rollback-audit",
    );
    let published = admin.publish_config(publish).await.unwrap().into_inner();

    let mut rollback = Request::new(RollbackConfigRequest {
        source_release_id: published.release_id,
    });
    add_config_rollback_metadata(&mut rollback, "rollback-audit", "sha256:rollback-audit");

    let rollback = admin.rollback_config(rollback).await.unwrap().into_inner();

    assert_eq!(rollback.release.unwrap().published_by, "operator-1");
}

#[tokio::test]
async fn registry_service_registers_instance_from_service_identity_metadata() {
    let service = registry_service();
    let mut request = Request::new(register_request());
    add_registry_write_metadata(&mut request);
    request
        .metadata_mut()
        .insert("x-request-id", "req-1".parse().unwrap());

    let response = service
        .register_instance(request)
        .await
        .unwrap()
        .into_inner();

    assert_eq!(response.lease_id, "lease-1");
    let request_id = response.metadata.unwrap().request_id;
    assert!(
        request_id.starts_with("req_"),
        "expected server-generated request id, got {request_id}"
    );
    assert!(response.expires_at.is_some());
}

#[tokio::test]
async fn registry_service_generates_request_id_when_metadata_is_missing() {
    let service = registry_service();
    let mut request = Request::new(register_request());
    add_registry_write_metadata(&mut request);

    let response = service
        .register_instance(request)
        .await
        .unwrap()
        .into_inner();

    let request_id = response.metadata.unwrap().request_id;
    assert!(
        request_id.starts_with("req_"),
        "expected generated request id, got {request_id}"
    );
    assert!(request_id.len() > "req_".len());
}

#[tokio::test]
async fn registry_service_retrieve_instance_returns_registered_instance_by_identity() {
    let service = registry_service();
    let mut register = Request::new(register_request());
    add_registry_write_metadata(&mut register);
    service.register_instance(register).await.unwrap();

    let mut retrieve = Request::new(retrieve_instance_request());
    add_registry_read_metadata(&mut retrieve);
    retrieve
        .metadata_mut()
        .insert("x-request-id", "req-retrieve-1".parse().unwrap());

    let response = service
        .retrieve_instance(retrieve)
        .await
        .unwrap()
        .into_inner();

    let instance = response
        .instance
        .expect("retrieve instance response must include instance");
    assert_eq!(instance.namespace, "sdkwork");
    assert_eq!(instance.environment, "development");
    assert_eq!(instance.service_name, "sdkwork-drive-product");
    assert_eq!(instance.instance_id, "drive-1");
    assert_eq!(instance.endpoint, "grpc://127.0.0.1:50051");
    assert_eq!(instance.protocol, "grpc");
    assert_eq!(instance.version, "0.1.0");
    assert_eq!(instance.region, "local");
    assert_eq!(instance.zone, "local-a");
    assert_eq!(instance.weight, 100);
    assert_eq!(instance.priority, 0);
    assert_eq!(instance.status, ProtoInstanceStatus::Serving as i32);
    let metadata = response.metadata.unwrap();
    assert!(
        metadata.request_id.starts_with("req_"),
        "expected server-generated request id, got {}",
        metadata.request_id
    );
    assert_eq!(metadata.revision, instance.revision);
}

#[tokio::test]
async fn registry_service_retrieve_instance_returns_not_found_for_missing_identity() {
    let service = registry_service();
    let mut retrieve = Request::new(RetrieveInstanceRequest {
        instance_id: "missing-drive".to_string(),
        ..retrieve_instance_request()
    });
    add_registry_read_metadata(&mut retrieve);

    let status = service.retrieve_instance(retrieve).await.unwrap_err();

    assert_eq!(status.code(), Code::NotFound);
    assert!(status.message().contains("service instance"));
}

#[tokio::test]
async fn registry_service_retrieve_instance_returns_not_found_for_expired_identity() {
    let mut store = MemoryDiscoveryStore::new();
    store
        .register_instance(RegisterInstanceCommand {
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
            now_ms: 0,
            expected_revision: None,
            persistent: false,
            health_check: None,
        })
        .await
        .unwrap();
    let service = RegistryRpcService::new(runtime_with_store(store));
    let mut retrieve = Request::new(retrieve_instance_request());
    add_registry_read_metadata(&mut retrieve);

    let status = service.retrieve_instance(retrieve).await.unwrap_err();

    assert_eq!(status.code(), Code::NotFound);
    assert!(status.message().contains("service instance"));
}

#[tokio::test]
async fn registry_service_retrieve_instance_requires_registry_read_permission() {
    let service = registry_service();
    let mut retrieve = Request::new(retrieve_instance_request());
    add_auth_metadata(&mut retrieve);
    retrieve
        .metadata_mut()
        .insert("x-sdkwork-subject-id", "service-1".parse().unwrap());
    retrieve
        .metadata_mut()
        .insert("x-sdkwork-config-permissions", "read".parse().unwrap());

    let status = service.retrieve_instance(retrieve).await.unwrap_err();

    assert_eq!(status.code(), Code::PermissionDenied);
}

#[tokio::test]
async fn registry_service_retrieve_instance_rejects_blank_required_fields_before_storage_dispatch()
{
    let service = RegistryRpcService::new(fail_runtime());

    for (field, request) in invalid_retrieve_instance_requests() {
        let mut request = Request::new(request);
        add_registry_read_metadata(&mut request);

        let status = service.retrieve_instance(request).await.unwrap_err();

        assert_eq!(status.code(), Code::InvalidArgument, "{field}");
        assert!(
            status.message().contains(field),
            "expected {field} in {}, got {}",
            status.message(),
            status.message()
        );
    }
}

#[tokio::test]
async fn registry_service_register_rejects_blank_required_fields_before_storage_dispatch() {
    let service = RegistryRpcService::new(fail_runtime());

    for (field, request) in invalid_register_requests() {
        let mut request = Request::new(request);
        add_registry_write_metadata(&mut request);

        let status = service.register_instance(request).await.unwrap_err();

        assert_eq!(status.code(), Code::InvalidArgument, "{field}");
        assert!(
            status.message().contains(field),
            "expected {field} in {}, got {}",
            status.message(),
            status.message()
        );
    }
}

#[tokio::test]
async fn registry_service_renew_lease_rejects_blank_lease_id_before_storage_dispatch() {
    let service = RegistryRpcService::new(fail_runtime());
    let mut request = Request::new(
        sdkwork_discovery_rpc_proto::sdkwork::discovery::internal::v1::RenewLeaseRequest {
            lease_id: " ".to_string(),
            lease_ttl_seconds: 30,
        },
    );
    add_registry_write_metadata(&mut request);

    let status = service.renew_lease(request).await.unwrap_err();

    assert_eq!(status.code(), Code::InvalidArgument);
    assert!(status.message().contains("lease_id"));
}

#[tokio::test]
async fn registry_service_renew_lease_returns_not_found_for_expired_lease() {
    let mut store = MemoryDiscoveryStore::new();
    let registered = store
        .register_instance(RegisterInstanceCommand {
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
            now_ms: 0,
            expected_revision: None,
            persistent: false,
            health_check: None,
        })
        .await
        .unwrap();
    let service = RegistryRpcService::new(runtime_with_store(store));
    let mut request = Request::new(
        sdkwork_discovery_rpc_proto::sdkwork::discovery::internal::v1::RenewLeaseRequest {
            lease_id: registered.lease_id,
            lease_ttl_seconds: 30,
        },
    );
    add_registry_write_metadata(&mut request);

    let status = service.renew_lease(request).await.unwrap_err();

    assert_eq!(status.code(), Code::NotFound);
    assert!(status.message().contains("lease"));
}

#[tokio::test]
async fn registry_service_report_status_returns_not_found_for_expired_instance() {
    let mut store = MemoryDiscoveryStore::new();
    store
        .register_instance(RegisterInstanceCommand {
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
            now_ms: 0,
            expected_revision: None,
            persistent: false,
            health_check: None,
        })
        .await
        .unwrap();
    let service = RegistryRpcService::new(runtime_with_store(store));
    let mut request = Request::new(report_status_request());
    add_registry_write_metadata(&mut request);

    let status = service.report_instance_status(request).await.unwrap_err();

    assert_eq!(status.code(), Code::NotFound);
    assert!(status.message().contains("instance"));
}

#[tokio::test]
async fn registry_service_discover_instances_rejects_blank_protocol_filter_before_storage_dispatch()
{
    let service = RegistryRpcService::new(fail_runtime());
    let mut request = Request::new(DiscoverInstancesRequest {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        service_name: "sdkwork-drive-product".to_string(),
        healthy_only: true,
        protocol: " ".to_string(),
        label_filters: vec![],
        sort_by: 0,
    });
    add_registry_read_metadata(&mut request);

    let status = service.discover_instances(request).await.unwrap_err();

    assert_eq!(status.code(), Code::InvalidArgument);
    assert!(status.message().contains("protocol"));
}

#[tokio::test]
async fn registry_service_report_status_rejects_blank_required_fields_before_storage_dispatch() {
    let service = RegistryRpcService::new(fail_runtime());

    for (field, request) in invalid_report_status_requests() {
        let mut request = Request::new(request);
        add_registry_write_metadata(&mut request);

        let status = service.report_instance_status(request).await.unwrap_err();

        assert_eq!(status.code(), Code::InvalidArgument, "{field}");
        assert!(
            status.message().contains(field),
            "expected {field} in {}, got {}",
            status.message(),
            status.message()
        );
    }
}

#[tokio::test]
async fn registry_service_deregister_rejects_blank_required_fields_before_storage_dispatch() {
    let service = RegistryRpcService::new(fail_runtime());

    for (field, request) in invalid_deregister_requests() {
        let mut request = Request::new(request);
        add_registry_write_metadata(&mut request);

        let status = service.deregister_instance(request).await.unwrap_err();

        assert_eq!(status.code(), Code::InvalidArgument, "{field}");
        assert!(
            status.message().contains(field),
            "expected {field} in {}, got {}",
            status.message(),
            status.message()
        );
    }
}

#[tokio::test]
async fn registry_service_deregister_expired_instance_is_idempotent_noop() {
    let mut store = MemoryDiscoveryStore::new();
    store
        .register_instance(RegisterInstanceCommand {
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
            now_ms: 0,
            expected_revision: None,
            persistent: false,
            health_check: None,
        })
        .await
        .unwrap();
    let service = RegistryRpcService::new(runtime_with_store(store));
    let mut request = Request::new(deregister_request());
    add_registry_write_metadata(&mut request);

    let response = service
        .deregister_instance(request)
        .await
        .unwrap()
        .into_inner();

    assert_eq!(response.metadata.unwrap().revision, 0);
}

#[tokio::test]
async fn admin_create_config_draft_rejects_blank_required_fields_before_storage_dispatch() {
    let service = DiscoveryAdminRpcService::new(fail_runtime());

    for (field, request) in invalid_create_config_draft_requests() {
        let mut request = Request::new(request);
        add_config_write_metadata(&mut request, "draft-invalid-field", "sha256:draft-invalid");

        let status = service.create_config_draft(request).await.unwrap_err();

        assert_eq!(status.code(), Code::InvalidArgument, "{field}");
        assert!(
            status.message().contains(field),
            "expected {field} in {}, got {}",
            status.message(),
            status.message()
        );
    }
}

#[tokio::test]
async fn admin_publish_config_rejects_blank_draft_id_before_storage_dispatch() {
    let service = DiscoveryAdminRpcService::new(fail_runtime());
    let mut request = Request::new(PublishConfigRequest {
        draft_id: " ".to_string(),
    });
    add_config_write_metadata(
        &mut request,
        "publish-invalid-field",
        "sha256:publish-invalid",
    );

    let status = service.publish_config(request).await.unwrap_err();

    assert_eq!(status.code(), Code::InvalidArgument);
    assert!(status.message().contains("draft_id"));
}

#[tokio::test]
async fn admin_rollback_config_rejects_blank_required_fields_before_storage_dispatch() {
    let service = DiscoveryAdminRpcService::new(fail_runtime());

    for (field, request) in invalid_rollback_config_requests() {
        let mut request = Request::new(request);
        add_config_rollback_metadata(
            &mut request,
            "rollback-invalid-field",
            "sha256:rollback-invalid",
        );

        let status = service.rollback_config(request).await.unwrap_err();

        assert_eq!(status.code(), Code::InvalidArgument, "{field}");
        assert!(
            status.message().contains(field),
            "expected {field} in {}, got {}",
            status.message(),
            status.message()
        );
    }
}

#[tokio::test]
async fn admin_and_config_services_publish_and_read_effective_config() {
    let runtime = runtime();
    let admin = DiscoveryAdminRpcService::new(runtime.clone());
    let config = DiscoveryConfigRpcService::new(runtime);

    let mut create = Request::new(CreateConfigDraftRequest {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        group: "runtime".to_string(),
        key: "log.level".to_string(),
        format: ProtoConfigFormat::Text as i32,
        value: "debug".to_string(),
        scope_type: ProtoConfigScopeType::Service as i32,
        application: "sdkwork-drive".to_string(),
        service_name: "sdkwork-drive-product".to_string(),
    });
    add_config_publish_metadata(&mut create);
    let draft = admin
        .create_config_draft(create)
        .await
        .unwrap()
        .into_inner();

    let mut publish = Request::new(PublishConfigRequest {
        draft_id: draft.draft_id,
    });
    add_config_publish_metadata(&mut publish);
    let release = admin.publish_config(publish).await.unwrap().into_inner();

    let mut retrieve = Request::new(RetrieveEffectiveConfigRequest {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        application: "sdkwork-drive".to_string(),
        service_name: "sdkwork-drive-product".to_string(),
        group: "runtime".to_string(),
    });
    retrieve
        .metadata_mut()
        .insert("x-sdkwork-subject-id", "service-1".parse().unwrap());
    retrieve
        .metadata_mut()
        .insert("x-sdkwork-config-permissions", "read".parse().unwrap());
    add_auth_metadata(&mut retrieve);

    let effective = config
        .retrieve_effective_config(retrieve)
        .await
        .unwrap()
        .into_inner();

    assert_eq!(release.metadata.unwrap().revision, 1);
    assert_eq!(effective.values.len(), 1);
    assert_eq!(effective.values[0].key, "log.level");
    assert_eq!(effective.values[0].value, "debug");
}

#[tokio::test]
async fn registry_service_discover_instances_rejects_blank_namespace_filter() {
    let status =
        discover_instances_status_for_filter(" ", "development", "sdkwork-drive-product").await;

    assert_eq!(status.code(), Code::InvalidArgument);
    assert!(status.message().contains("namespace"));
}

#[tokio::test]
async fn registry_service_discover_instances_rejects_blank_environment_filter() {
    let status =
        discover_instances_status_for_filter("sdkwork", " ", "sdkwork-drive-product").await;

    assert_eq!(status.code(), Code::InvalidArgument);
    assert!(status.message().contains("environment"));
}

#[tokio::test]
async fn registry_service_discover_instances_rejects_blank_service_name_filter() {
    let status = discover_instances_status_for_filter("sdkwork", "development", " ").await;

    assert_eq!(status.code(), Code::InvalidArgument);
    assert!(status.message().contains("service_name"));
}

#[tokio::test]
async fn registry_service_retrieve_instance_rejects_blank_namespace_filter() {
    let status =
        retrieve_instance_status_for_filter(" ", "development", "sdkwork-drive-product", "drive-1")
            .await;

    assert_eq!(status.code(), Code::InvalidArgument);
    assert!(status.message().contains("namespace"));
}

#[tokio::test]
async fn registry_service_retrieve_instance_rejects_blank_environment_filter() {
    let status =
        retrieve_instance_status_for_filter("sdkwork", " ", "sdkwork-drive-product", "drive-1")
            .await;

    assert_eq!(status.code(), Code::InvalidArgument);
    assert!(status.message().contains("environment"));
}

#[tokio::test]
async fn registry_service_retrieve_instance_rejects_blank_service_name_filter() {
    let status =
        retrieve_instance_status_for_filter("sdkwork", "development", " ", "drive-1").await;

    assert_eq!(status.code(), Code::InvalidArgument);
    assert!(status.message().contains("service_name"));
}

#[tokio::test]
async fn registry_service_retrieve_instance_rejects_blank_instance_id_filter() {
    let status =
        retrieve_instance_status_for_filter("sdkwork", "development", "sdkwork-drive-product", " ")
            .await;

    assert_eq!(status.code(), Code::InvalidArgument);
    assert!(status.message().contains("instance_id"));
}

#[tokio::test]
async fn discovery_config_service_retrieve_effective_config_rejects_blank_namespace_filter() {
    let status = retrieve_effective_config_status_for_filter(
        " ",
        "development",
        "sdkwork-drive",
        "sdkwork-drive-product",
        "runtime",
    )
    .await;

    assert_eq!(status.code(), Code::InvalidArgument);
    assert!(status.message().contains("namespace"));
}

#[tokio::test]
async fn discovery_config_service_retrieve_effective_config_rejects_blank_environment_filter() {
    let status = retrieve_effective_config_status_for_filter(
        "sdkwork",
        " ",
        "sdkwork-drive",
        "sdkwork-drive-product",
        "runtime",
    )
    .await;

    assert_eq!(status.code(), Code::InvalidArgument);
    assert!(status.message().contains("environment"));
}

#[tokio::test]
async fn discovery_config_service_retrieve_effective_config_rejects_blank_application_filter() {
    let status = retrieve_effective_config_status_for_filter(
        "sdkwork",
        "development",
        " ",
        "sdkwork-drive-product",
        "runtime",
    )
    .await;

    assert_eq!(status.code(), Code::InvalidArgument);
    assert!(status.message().contains("application"));
}

#[tokio::test]
async fn discovery_config_service_retrieve_effective_config_rejects_blank_service_name_filter() {
    let status = retrieve_effective_config_status_for_filter(
        "sdkwork",
        "development",
        "sdkwork-drive",
        " ",
        "runtime",
    )
    .await;

    assert_eq!(status.code(), Code::InvalidArgument);
    assert!(status.message().contains("service_name"));
}

#[tokio::test]
async fn discovery_config_service_retrieve_effective_config_rejects_blank_group_filter() {
    let status = retrieve_effective_config_status_for_filter(
        "sdkwork",
        "development",
        "sdkwork-drive",
        "sdkwork-drive-product",
        " ",
    )
    .await;

    assert_eq!(status.code(), Code::InvalidArgument);
    assert!(status.message().contains("group"));
}

#[tokio::test]
async fn admin_list_services_rejects_blank_namespace_filter_before_storage_dispatch() {
    let status = list_services_status_for_filter_with_fail_store(" ", "development").await;

    assert_eq!(status.code(), Code::InvalidArgument);
    assert!(status.message().contains("namespace"));
}

#[tokio::test]
async fn admin_list_services_rejects_blank_environment_filter_before_storage_dispatch() {
    let status = list_services_status_for_filter_with_fail_store("sdkwork", " ").await;

    assert_eq!(status.code(), Code::InvalidArgument);
    assert!(status.message().contains("environment"));
}

#[tokio::test]
async fn watch_config_filters_service_scoped_events_by_application_and_service() {
    let runtime = runtime();
    let admin = DiscoveryAdminRpcService::new(runtime.clone());
    let watch = DiscoveryConfigRpcService::new(runtime);

    let mut product_create = Request::new(create_service_config_draft_request(
        "sdkwork-drive-product",
        "debug",
    ));
    add_config_write_metadata(
        &mut product_create,
        "draft-watch-product",
        "sha256:draft-product",
    );
    let product_draft = admin
        .create_config_draft(product_create)
        .await
        .unwrap()
        .into_inner();

    let mut worker_create = Request::new(create_service_config_draft_request(
        "sdkwork-drive-worker",
        "trace",
    ));
    add_config_write_metadata(
        &mut worker_create,
        "draft-watch-worker",
        "sha256:draft-worker",
    );
    let worker_draft = admin
        .create_config_draft(worker_create)
        .await
        .unwrap()
        .into_inner();

    let mut product_publish = Request::new(PublishConfigRequest {
        draft_id: product_draft.draft_id,
    });
    add_config_write_metadata(
        &mut product_publish,
        "publish-watch-product",
        "sha256:publish-product",
    );
    admin.publish_config(product_publish).await.unwrap();

    let mut worker_publish = Request::new(PublishConfigRequest {
        draft_id: worker_draft.draft_id,
    });
    add_config_write_metadata(
        &mut worker_publish,
        "publish-watch-worker",
        "sha256:publish-worker",
    );
    admin.publish_config(worker_publish).await.unwrap();

    let mut request = Request::new(WatchConfigRequest {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        application: "sdkwork-drive".to_string(),
        service_name: "sdkwork-drive-product".to_string(),
        group: "runtime".to_string(),
        from_revision: 0,
    });
    add_config_read_metadata(&mut request);

    let mut stream = watch.watch_config(request).await.unwrap().into_inner();
    let event = timeout(Duration::from_secs(1), stream.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        event.event_type,
        common_proto::WatchEventType::ConfigPublished as i32
    );
    assert_eq!(event.group, "runtime");
    assert_eq!(event.key, "log.level");
    assert_eq!(event.metadata.unwrap().revision, 1);

    assert!(timeout(Duration::from_millis(100), stream.next())
        .await
        .is_err());
}

#[tokio::test]
async fn watch_service_stream_includes_registered_instance_payload() {
    let runtime = runtime();
    let registry = RegistryRpcService::new(runtime.clone());
    let watch = DiscoveryWatchRpcService::new(runtime);

    let mut register = Request::new(register_request());
    add_registry_write_metadata(&mut register);
    registry.register_instance(register).await.unwrap();

    let mut request = Request::new(WatchServiceRequest {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        service_name: "sdkwork-drive-product".to_string(),
        from_revision: 0,
    });
    add_registry_read_metadata(&mut request);

    let mut stream = watch.watch_service(request).await.unwrap().into_inner();
    let event = timeout(Duration::from_secs(1), stream.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    assert_eq!(
        event.event_type,
        common_proto::WatchEventType::InstanceRegistered as i32
    );
    assert_eq!(event.metadata.unwrap().revision, 1);
    let instance = event
        .instance
        .expect("watch service registry events must include service instance payload");
    assert_eq!(instance.namespace, "sdkwork");
    assert_eq!(instance.environment, "development");
    assert_eq!(instance.service_name, "sdkwork-drive-product");
    assert_eq!(instance.instance_id, "drive-1");
    assert_eq!(instance.endpoint, "grpc://127.0.0.1:50051");
    assert_eq!(instance.protocol, "grpc");
    assert_eq!(instance.version, "0.1.0");
    assert_eq!(instance.region, "local");
    assert_eq!(instance.zone, "local-a");
    assert_eq!(instance.weight, 100);
    assert_eq!(instance.priority, 0);
    assert_eq!(instance.status, ProtoInstanceStatus::Serving as i32);
    assert_eq!(instance.lease_id, "lease-1");
    assert!(instance.expires_at.is_some());
}

#[tokio::test]
async fn watch_service_live_stream_includes_registered_instance_payload() {
    let runtime = runtime();
    let registry = RegistryRpcService::new(runtime.clone());
    let watch = DiscoveryWatchRpcService::new(runtime);

    let mut request = Request::new(WatchServiceRequest {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        service_name: "sdkwork-drive-product".to_string(),
        from_revision: 0,
    });
    add_registry_read_metadata(&mut request);
    let mut stream = watch.watch_service(request).await.unwrap().into_inner();

    let mut register = Request::new(register_request());
    add_registry_write_metadata(&mut register);
    registry.register_instance(register).await.unwrap();

    let event = timeout(Duration::from_secs(1), stream.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    assert_eq!(
        event.event_type,
        common_proto::WatchEventType::InstanceRegistered as i32
    );
    let instance = event
        .instance
        .expect("live watch service registry events must include service instance payload");
    assert_eq!(instance.instance_id, "drive-1");
    assert_eq!(instance.endpoint, "grpc://127.0.0.1:50051");
    assert_eq!(instance.status, ProtoInstanceStatus::Serving as i32);
}

#[tokio::test]
async fn watch_service_deregister_event_includes_identity_tombstone_payload() {
    let runtime = runtime();
    let registry = RegistryRpcService::new(runtime.clone());
    let watch = DiscoveryWatchRpcService::new(runtime);

    let mut register = Request::new(register_request());
    add_registry_write_metadata(&mut register);
    registry.register_instance(register).await.unwrap();

    let mut deregister = Request::new(deregister_request());
    add_registry_write_metadata(&mut deregister);
    registry.deregister_instance(deregister).await.unwrap();

    let mut request = Request::new(WatchServiceRequest {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        service_name: "sdkwork-drive-product".to_string(),
        from_revision: 1,
    });
    add_registry_read_metadata(&mut request);

    let mut stream = watch.watch_service(request).await.unwrap().into_inner();
    let event = timeout(Duration::from_secs(1), stream.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    assert_eq!(
        event.event_type,
        common_proto::WatchEventType::InstanceDeregistered as i32
    );
    assert_eq!(event.metadata.unwrap().revision, 2);
    let instance = event
        .instance
        .expect("deregister watch events must include removable instance identity");
    assert_eq!(instance.namespace, "sdkwork");
    assert_eq!(instance.environment, "development");
    assert_eq!(instance.service_name, "sdkwork-drive-product");
    assert_eq!(instance.instance_id, "drive-1");
    assert_eq!(instance.status, ProtoInstanceStatus::NotServing as i32);
    assert_eq!(instance.revision, 2);
}

#[tokio::test]
async fn watch_service_rejects_missing_registry_read_permission_before_storage_dispatch() {
    let watch = DiscoveryWatchRpcService::new(fail_runtime());
    let mut request = Request::new(WatchServiceRequest {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        service_name: "sdkwork-drive-product".to_string(),
        from_revision: 0,
    });
    add_auth_metadata(&mut request);
    request
        .metadata_mut()
        .insert("x-sdkwork-subject-id", "service-1".parse().unwrap());

    let status = watch.watch_service(request).await.unwrap_err();

    assert_eq!(status.code(), Code::PermissionDenied);
    assert!(status.message().contains("registry permission"));
}

#[tokio::test]
async fn watch_config_rejects_missing_config_read_permission_before_storage_dispatch() {
    let watch = DiscoveryConfigRpcService::new(fail_runtime());
    let mut request = Request::new(WatchConfigRequest {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        application: "sdkwork-drive".to_string(),
        service_name: "sdkwork-drive-product".to_string(),
        group: "runtime".to_string(),
        from_revision: 0,
    });
    add_auth_metadata(&mut request);
    request
        .metadata_mut()
        .insert("x-sdkwork-subject-id", "service-1".parse().unwrap());

    let status = watch.watch_config(request).await.unwrap_err();

    assert_eq!(status.code(), Code::PermissionDenied);
    assert!(status.message().contains("config permission"));
}

#[tokio::test]
async fn watch_service_rejects_blank_service_name_filter() {
    let status = watch_service_status_for_filter("sdkwork", "development", " ").await;

    assert_eq!(status.code(), Code::InvalidArgument);
    assert!(status.message().contains("service_name"));
}

#[tokio::test]
async fn watch_service_rejects_blank_namespace_filter() {
    let status = watch_service_status_for_filter(" ", "development", "sdkwork-drive-product").await;

    assert_eq!(status.code(), Code::InvalidArgument);
    assert!(status.message().contains("namespace"));
}

#[tokio::test]
async fn watch_service_rejects_blank_environment_filter() {
    let status = watch_service_status_for_filter("sdkwork", " ", "sdkwork-drive-product").await;

    assert_eq!(status.code(), Code::InvalidArgument);
    assert!(status.message().contains("environment"));
}

#[tokio::test]
async fn watch_config_rejects_blank_application_filter() {
    let status =
        watch_config_status_for_filter_defaults(" ", "sdkwork-drive-product", "runtime").await;

    assert_eq!(status.code(), Code::InvalidArgument);
    assert!(status.message().contains("application"));
}

#[tokio::test]
async fn watch_config_rejects_blank_service_name_filter() {
    let status = watch_config_status_for_filter_defaults("sdkwork-drive", " ", "runtime").await;

    assert_eq!(status.code(), Code::InvalidArgument);
    assert!(status.message().contains("service_name"));
}

#[tokio::test]
async fn watch_config_rejects_blank_group_filter() {
    let status =
        watch_config_status_for_filter_defaults("sdkwork-drive", "sdkwork-drive-product", " ")
            .await;

    assert_eq!(status.code(), Code::InvalidArgument);
    assert!(status.message().contains("group"));
}

#[tokio::test]
async fn watch_config_rejects_blank_namespace_filter() {
    let status = watch_config_status_for_filter(
        " ",
        "development",
        "sdkwork-drive",
        "sdkwork-drive-product",
        "runtime",
    )
    .await;

    assert_eq!(status.code(), Code::InvalidArgument);
    assert!(status.message().contains("namespace"));
}

#[tokio::test]
async fn watch_config_rejects_blank_environment_filter() {
    let status = watch_config_status_for_filter(
        "sdkwork",
        " ",
        "sdkwork-drive",
        "sdkwork-drive-product",
        "runtime",
    )
    .await;

    assert_eq!(status.code(), Code::InvalidArgument);
    assert!(status.message().contains("environment"));
}

#[tokio::test]
async fn admin_list_services_reads_registry_state_through_rpc_runtime() {
    let runtime = runtime();
    let registry = RegistryRpcService::new(runtime.clone());
    let admin = DiscoveryAdminRpcService::new(runtime);

    let mut register = Request::new(register_request());
    add_registry_write_metadata(&mut register);
    registry.register_instance(register).await.unwrap();

    let mut list = Request::new(ListServicesRequest {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
    });
    list.metadata_mut()
        .insert("x-sdkwork-subject-id", "operator-1".parse().unwrap());
    list.metadata_mut()
        .insert("x-sdkwork-registry-permissions", "read".parse().unwrap());
    add_auth_metadata(&mut list);

    let services = admin.list_services(list).await.unwrap().into_inner();

    assert_eq!(services.services.len(), 1);
    assert_eq!(services.services[0].service_name, "sdkwork-drive-product");
}

fn sdk_manifest_methods(manifest: &Value) -> Vec<Vec<&str>> {
    manifest["services"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|service| {
            service["methods"].as_array().unwrap().iter().map(|method| {
                vec![
                    service["package"].as_str().unwrap(),
                    service["service"].as_str().unwrap(),
                    service["surface"].as_str().unwrap(),
                    method["method"].as_str().unwrap(),
                    method["operationId"].as_str().unwrap(),
                    method["auth"].as_str().unwrap(),
                    method["idempotency"].as_str().unwrap(),
                    method["streaming"].as_str().unwrap(),
                    service["owner"].as_str().unwrap(),
                    method["compatibility"].as_str().unwrap(),
                ]
            })
        })
        .collect()
}

fn registry_service() -> RegistryRpcService<MemoryDiscoveryStore> {
    RegistryRpcService::new(runtime())
}

fn runtime() -> DiscoveryRpcRuntime<MemoryDiscoveryStore> {
    runtime_with_store(MemoryDiscoveryStore::new())
}

fn runtime_with_store(store: MemoryDiscoveryStore) -> DiscoveryRpcRuntime<MemoryDiscoveryStore> {
    let control_plane = DiscoveryControlPlane::new(
        store,
        ConfigPolicy {
            enabled: true,
            require_publish_for_reads: true,
            allow_secret_values: false,
            allow_secret_refs: true,
            max_config_body_bytes: 1024,
        },
        RegistryPolicy::default(),
    );
    DiscoveryRpcRuntime::with_config(
        control_plane,
        DiscoveryRpcRuntimeConfig {
            registry_expiry_scan_interval_ms: 0,
            registry_expiry_scan_batch_size: 1_000,
            allow_unsigned_local_context: true,
            service_token_verifier: None,
            event_gc_interval_ms: 0,
            event_gc_retention_count: 10_000,
            event_gc_batch_size: 1_000,
            resilience: RuntimeResilienceConfig::default(),
            health_check_scan_interval_ms: 0,
        },
    )
}

fn runtime_without_unsigned_local_context() -> DiscoveryRpcRuntime<MemoryDiscoveryStore> {
    DiscoveryRpcRuntime::with_config(
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
        ),
        DiscoveryRpcRuntimeConfig {
            registry_expiry_scan_interval_ms: 0,
            registry_expiry_scan_batch_size: 1_000,
            allow_unsigned_local_context: false,
            service_token_verifier: None,
            event_gc_interval_ms: 0,
            event_gc_retention_count: 10_000,
            event_gc_batch_size: 1_000,
            resilience: RuntimeResilienceConfig::default(),
            health_check_scan_interval_ms: 0,
        },
    )
}

fn runtime_with_service_token_verifier() -> DiscoveryRpcRuntime<MemoryDiscoveryStore> {
    DiscoveryRpcRuntime::with_config(
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
        ),
        DiscoveryRpcRuntimeConfig {
            registry_expiry_scan_interval_ms: 0,
            registry_expiry_scan_batch_size: 1_000,
            allow_unsigned_local_context: false,
            service_token_verifier: Some(DiscoveryRpcServiceTokenVerifierConfig {
                hmac_secret: SERVICE_TOKEN_SECRET.to_vec(),
                issuer: "sdkwork-discovery".to_string(),
                audience: "sdkwork-discovery-rpc".to_string(),
                max_token_ttl_seconds: 200 * 365 * 24 * 60 * 60,
            }),
            event_gc_interval_ms: 0,
            event_gc_retention_count: 10_000,
            event_gc_batch_size: 1_000,
            resilience: RuntimeResilienceConfig::default(),
            health_check_scan_interval_ms: 0,
        },
    )
}

fn fail_runtime() -> DiscoveryRpcRuntime<FailOnReadStore> {
    let control_plane = DiscoveryControlPlane::new(
        FailOnReadStore,
        ConfigPolicy {
            enabled: true,
            require_publish_for_reads: true,
            allow_secret_values: false,
            allow_secret_refs: true,
            max_config_body_bytes: 1024,
        },
        RegistryPolicy::default(),
    );
    DiscoveryRpcRuntime::with_config(
        control_plane,
        DiscoveryRpcRuntimeConfig {
            registry_expiry_scan_interval_ms: 0,
            registry_expiry_scan_batch_size: 1_000,
            allow_unsigned_local_context: true,
            service_token_verifier: None,
            event_gc_interval_ms: 0,
            event_gc_retention_count: 10_000,
            event_gc_batch_size: 1_000,
            resilience: RuntimeResilienceConfig::default(),
            health_check_scan_interval_ms: 0,
        },
    )
}

fn add_registry_write_metadata<T>(request: &mut Request<T>) {
    add_auth_metadata(request);
    request
        .metadata_mut()
        .insert("x-sdkwork-subject-id", "service-1".parse().unwrap());
    request
        .metadata_mut()
        .insert("x-sdkwork-registry-permissions", "write".parse().unwrap());
}

fn add_registry_read_metadata<T>(request: &mut Request<T>) {
    add_auth_metadata(request);
    request
        .metadata_mut()
        .insert("x-sdkwork-subject-id", "service-1".parse().unwrap());
    request
        .metadata_mut()
        .insert("x-sdkwork-registry-permissions", "read".parse().unwrap());
}

fn add_config_publish_metadata<T>(request: &mut Request<T>) {
    add_config_write_metadata(request, "idem-config-write-1", "sha256:test-request-hash");
}

fn add_config_read_metadata<T>(request: &mut Request<T>) {
    add_auth_metadata(request);
    request
        .metadata_mut()
        .insert("x-sdkwork-subject-id", "service-1".parse().unwrap());
    request
        .metadata_mut()
        .insert("x-sdkwork-config-permissions", "read".parse().unwrap());
}

async fn watch_service_status_for_filter(
    namespace: &str,
    environment: &str,
    service_name: &str,
) -> tonic::Status {
    let watch = DiscoveryWatchRpcService::new(runtime());
    let mut request = Request::new(WatchServiceRequest {
        namespace: namespace.to_string(),
        environment: environment.to_string(),
        service_name: service_name.to_string(),
        from_revision: 0,
    });
    add_registry_read_metadata(&mut request);

    watch.watch_service(request).await.unwrap_err()
}

async fn discover_instances_status_for_filter(
    namespace: &str,
    environment: &str,
    service_name: &str,
) -> tonic::Status {
    let service = registry_service();
    let mut request = Request::new(DiscoverInstancesRequest {
        namespace: namespace.to_string(),
        environment: environment.to_string(),
        service_name: service_name.to_string(),
        healthy_only: true,
        protocol: "grpc".to_string(),
        label_filters: vec![],
        sort_by: 0,
    });
    add_registry_read_metadata(&mut request);

    service.discover_instances(request).await.unwrap_err()
}

async fn retrieve_instance_status_for_filter(
    namespace: &str,
    environment: &str,
    service_name: &str,
    instance_id: &str,
) -> tonic::Status {
    let service = registry_service();
    let mut request = Request::new(RetrieveInstanceRequest {
        namespace: namespace.to_string(),
        environment: environment.to_string(),
        service_name: service_name.to_string(),
        instance_id: instance_id.to_string(),
    });
    add_registry_read_metadata(&mut request);

    service.retrieve_instance(request).await.unwrap_err()
}

async fn retrieve_effective_config_status_for_filter(
    namespace: &str,
    environment: &str,
    application: &str,
    service_name: &str,
    group: &str,
) -> tonic::Status {
    let config = DiscoveryConfigRpcService::new(runtime());
    let mut request = Request::new(RetrieveEffectiveConfigRequest {
        namespace: namespace.to_string(),
        environment: environment.to_string(),
        application: application.to_string(),
        service_name: service_name.to_string(),
        group: group.to_string(),
    });
    add_config_read_metadata(&mut request);

    config.retrieve_effective_config(request).await.unwrap_err()
}

async fn list_services_status_for_filter_with_fail_store(
    namespace: &str,
    environment: &str,
) -> tonic::Status {
    let admin = DiscoveryAdminRpcService::new(fail_runtime());
    let mut request = Request::new(ListServicesRequest {
        namespace: namespace.to_string(),
        environment: environment.to_string(),
    });
    request
        .metadata_mut()
        .insert("x-sdkwork-subject-id", "operator-1".parse().unwrap());
    request
        .metadata_mut()
        .insert("x-sdkwork-registry-permissions", "read".parse().unwrap());
    add_auth_metadata(&mut request);

    admin.list_services(request).await.unwrap_err()
}

async fn watch_config_status_for_filter_defaults(
    application: &str,
    service_name: &str,
    group: &str,
) -> tonic::Status {
    watch_config_status_for_filter("sdkwork", "development", application, service_name, group).await
}

async fn watch_config_status_for_filter(
    namespace: &str,
    environment: &str,
    application: &str,
    service_name: &str,
    group: &str,
) -> tonic::Status {
    let watch = DiscoveryConfigRpcService::new(runtime());
    let mut request = Request::new(WatchConfigRequest {
        namespace: namespace.to_string(),
        environment: environment.to_string(),
        application: application.to_string(),
        service_name: service_name.to_string(),
        group: group.to_string(),
        from_revision: 0,
    });
    add_config_read_metadata(&mut request);

    watch.watch_config(request).await.unwrap_err()
}

fn add_config_write_metadata<T>(
    request: &mut Request<T>,
    idempotency_key: &'static str,
    request_hash: &'static str,
) {
    add_auth_metadata(request);
    request
        .metadata_mut()
        .insert("x-sdkwork-subject-id", "operator-1".parse().unwrap());
    request
        .metadata_mut()
        .insert("x-sdkwork-config-permissions", "publish".parse().unwrap());
    request
        .metadata_mut()
        .insert("idempotency-key", idempotency_key.parse().unwrap());
    request
        .metadata_mut()
        .insert("x-request-hash", request_hash.parse().unwrap());
}

fn add_config_rollback_metadata<T>(
    request: &mut Request<T>,
    idempotency_key: &'static str,
    request_hash: &'static str,
) {
    add_auth_metadata(request);
    request
        .metadata_mut()
        .insert("x-sdkwork-subject-id", "operator-1".parse().unwrap());
    request
        .metadata_mut()
        .insert("x-sdkwork-config-permissions", "rollback".parse().unwrap());
    request
        .metadata_mut()
        .insert("idempotency-key", idempotency_key.parse().unwrap());
    request
        .metadata_mut()
        .insert("x-request-hash", request_hash.parse().unwrap());
}

const SERVICE_TOKEN_SECRET: &[u8] = b"0123456789abcdef0123456789abcdef";
const VERIFIED_ACCESS_TOKEN: &str = "verified-access-token";
const VERIFIED_SERVICE_TOKEN: &str = "sdkwork-discovery-v1.eyJhbGciOiJIUzI1NiIsInR5cCI6InNka3dvcmsuZGlzY292ZXJ5LnNlcnZpY2UtdG9rZW4udjEifQ.eyJpc3MiOiJzZGt3b3JrLWRpc2NvdmVyeSIsImF1ZCI6InNka3dvcmstZGlzY292ZXJ5LXJwYyIsInN1YiI6InNlcnZpY2UtMSIsImlhdF9tcyI6MTcwMDAwMDAwMDAwMCwiZXhwX21zIjo0MTAyNDQ0ODAwMDAwLCJhY2Nlc3NfdG9rZW5fc2hhMjU2IjoiYjE2MWJlYjI5NjM5NjI1MmJmZDZlNzc1NmQyODdlNjY3OTkxMzdmMDIyMTljOGRlODNlODIxMzgyNzFiOWQyNyIsInJlZ2lzdHJ5X3Blcm1pc3Npb25zIjpbInJlYWQiLCJ3cml0ZSJdLCJjb25maWdfcGVybWlzc2lvbnMiOlsicmVhZCIsInB1Ymxpc2giXX0.NoJu0JcYJxTCP----H4bCIOAho-nybRC6X0pg6Z74fs";

fn add_verified_service_token_metadata<T>(request: &mut Request<T>) {
    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {VERIFIED_SERVICE_TOKEN}").parse().unwrap(),
    );
    request
        .metadata_mut()
        .insert("access-token", VERIFIED_ACCESS_TOKEN.parse().unwrap());
}

fn add_auth_metadata<T>(request: &mut Request<T>) {
    request
        .metadata_mut()
        .insert("authorization", "Bearer test-auth-token".parse().unwrap());
    request
        .metadata_mut()
        .insert("access-token", "test-access-token".parse().unwrap());
}

fn register_request() -> RegisterInstanceRequest {
    RegisterInstanceRequest {
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
        status: ProtoInstanceStatus::Serving as i32,
        metadata: Default::default(),
        lease_ttl_seconds: 30,
        expected_revision: None,
        persistent: false,
        health_check: None,
    }
}

fn retrieve_instance_request() -> RetrieveInstanceRequest {
    RetrieveInstanceRequest {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        service_name: "sdkwork-drive-product".to_string(),
        instance_id: "drive-1".to_string(),
    }
}

fn invalid_register_requests() -> Vec<(&'static str, RegisterInstanceRequest)> {
    vec![
        (
            "namespace",
            RegisterInstanceRequest {
                namespace: " ".to_string(),
                ..register_request()
            },
        ),
        (
            "environment",
            RegisterInstanceRequest {
                environment: " ".to_string(),
                ..register_request()
            },
        ),
        (
            "service_name",
            RegisterInstanceRequest {
                service_name: " ".to_string(),
                ..register_request()
            },
        ),
        (
            "instance_id",
            RegisterInstanceRequest {
                instance_id: " ".to_string(),
                ..register_request()
            },
        ),
        (
            "endpoint",
            RegisterInstanceRequest {
                endpoint: " ".to_string(),
                ..register_request()
            },
        ),
        (
            "protocol",
            RegisterInstanceRequest {
                protocol: " ".to_string(),
                ..register_request()
            },
        ),
        (
            "version",
            RegisterInstanceRequest {
                version: " ".to_string(),
                ..register_request()
            },
        ),
        (
            "region",
            RegisterInstanceRequest {
                region: " ".to_string(),
                ..register_request()
            },
        ),
        (
            "zone",
            RegisterInstanceRequest {
                zone: " ".to_string(),
                ..register_request()
            },
        ),
        (
            "lease_ttl_seconds",
            RegisterInstanceRequest {
                lease_ttl_seconds: 0,
                ..register_request()
            },
        ),
    ]
}

fn invalid_retrieve_instance_requests() -> Vec<(&'static str, RetrieveInstanceRequest)> {
    vec![
        (
            "namespace",
            RetrieveInstanceRequest {
                namespace: " ".to_string(),
                ..retrieve_instance_request()
            },
        ),
        (
            "environment",
            RetrieveInstanceRequest {
                environment: " ".to_string(),
                ..retrieve_instance_request()
            },
        ),
        (
            "service_name",
            RetrieveInstanceRequest {
                service_name: " ".to_string(),
                ..retrieve_instance_request()
            },
        ),
        (
            "instance_id",
            RetrieveInstanceRequest {
                instance_id: " ".to_string(),
                ..retrieve_instance_request()
            },
        ),
    ]
}

fn report_status_request() -> ReportInstanceStatusRequest {
    ReportInstanceStatusRequest {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        service_name: "sdkwork-drive-product".to_string(),
        instance_id: "drive-1".to_string(),
        status: ProtoInstanceStatus::Serving as i32,
        expected_revision: None,
    }
}

fn invalid_report_status_requests() -> Vec<(&'static str, ReportInstanceStatusRequest)> {
    vec![
        (
            "namespace",
            ReportInstanceStatusRequest {
                namespace: " ".to_string(),
                ..report_status_request()
            },
        ),
        (
            "environment",
            ReportInstanceStatusRequest {
                environment: " ".to_string(),
                ..report_status_request()
            },
        ),
        (
            "service_name",
            ReportInstanceStatusRequest {
                service_name: " ".to_string(),
                ..report_status_request()
            },
        ),
        (
            "instance_id",
            ReportInstanceStatusRequest {
                instance_id: " ".to_string(),
                ..report_status_request()
            },
        ),
    ]
}

fn deregister_request() -> DeregisterInstanceRequest {
    DeregisterInstanceRequest {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        service_name: "sdkwork-drive-product".to_string(),
        instance_id: "drive-1".to_string(),
    }
}

fn invalid_deregister_requests() -> Vec<(&'static str, DeregisterInstanceRequest)> {
    vec![
        (
            "namespace",
            DeregisterInstanceRequest {
                namespace: " ".to_string(),
                ..deregister_request()
            },
        ),
        (
            "environment",
            DeregisterInstanceRequest {
                environment: " ".to_string(),
                ..deregister_request()
            },
        ),
        (
            "service_name",
            DeregisterInstanceRequest {
                service_name: " ".to_string(),
                ..deregister_request()
            },
        ),
        (
            "instance_id",
            DeregisterInstanceRequest {
                instance_id: " ".to_string(),
                ..deregister_request()
            },
        ),
    ]
}

fn invalid_create_config_draft_requests() -> Vec<(&'static str, CreateConfigDraftRequest)> {
    vec![
        (
            "namespace",
            CreateConfigDraftRequest {
                namespace: " ".to_string(),
                ..create_service_config_draft_request("sdkwork-drive-product", "debug")
            },
        ),
        (
            "environment",
            CreateConfigDraftRequest {
                environment: " ".to_string(),
                ..create_service_config_draft_request("sdkwork-drive-product", "debug")
            },
        ),
        (
            "group",
            CreateConfigDraftRequest {
                group: " ".to_string(),
                ..create_service_config_draft_request("sdkwork-drive-product", "debug")
            },
        ),
        (
            "key",
            CreateConfigDraftRequest {
                key: " ".to_string(),
                ..create_service_config_draft_request("sdkwork-drive-product", "debug")
            },
        ),
    ]
}

fn invalid_rollback_config_requests() -> Vec<(&'static str, RollbackConfigRequest)> {
    vec![(
        "source_release_id",
        RollbackConfigRequest {
            source_release_id: " ".to_string(),
        },
    )]
}

#[derive(Clone, Copy)]
struct FailOnReadStore;

#[async_trait]
impl RegistryStore for FailOnReadStore {
    async fn current_revision(&self) -> DiscoveryResult<u64> {
        Err(unexpected_storage_dispatch_error("current_revision"))
    }

    async fn register_instance(
        &mut self,
        _command: RegisterInstanceCommand,
    ) -> DiscoveryResult<RegisterInstanceResult> {
        Err(unexpected_storage_dispatch_error("register_instance"))
    }

    async fn batch_register_instances(
        &mut self,
        _commands: Vec<RegisterInstanceCommand>,
    ) -> DiscoveryResult<BatchRegisterResult> {
        Err(unexpected_storage_dispatch_error(
            "batch_register_instances",
        ))
    }

    async fn renew_lease(
        &mut self,
        _command: RenewLeaseCommand,
    ) -> DiscoveryResult<RenewLeaseResult> {
        Err(unexpected_storage_dispatch_error("renew_lease"))
    }

    async fn report_instance_status(
        &mut self,
        _command: ReportInstanceStatusCommand,
    ) -> DiscoveryResult<ReportInstanceStatusResult> {
        Err(unexpected_storage_dispatch_error("report_instance_status"))
    }

    async fn deregister_instance(
        &mut self,
        _namespace: &str,
        _environment: &str,
        _service_name: &str,
        _instance_id: &str,
        _now_ms: u64,
    ) -> DiscoveryResult<DeregisterInstanceResult> {
        Err(unexpected_storage_dispatch_error("deregister_instance"))
    }

    async fn batch_deregister_instances(
        &mut self,
        _namespace: &str,
        _environment: &str,
        _service_name: &str,
        _instance_ids: Vec<String>,
        _now_ms: u64,
    ) -> DiscoveryResult<Vec<DeregisterInstanceResult>> {
        Err(unexpected_storage_dispatch_error(
            "batch_deregister_instances",
        ))
    }

    async fn expire_instances(
        &mut self,
        _now_ms: u64,
        _max_instances: usize,
    ) -> DiscoveryResult<Vec<DeregisterInstanceResult>> {
        Err(unexpected_storage_dispatch_error("expire_instances"))
    }

    async fn discover_instances(
        &self,
        _query: DiscoverInstancesQuery,
        _now_ms: u64,
    ) -> DiscoveryResult<DiscoverInstancesResult> {
        Err(unexpected_storage_dispatch_error("discover_instances"))
    }

    async fn retrieve_instance(
        &self,
        _query: RetrieveInstanceQuery,
        _now_ms: u64,
    ) -> DiscoveryResult<Option<ServiceInstance>> {
        Err(unexpected_storage_dispatch_error("retrieve_instance"))
    }

    async fn list_services(
        &self,
        _query: ListServicesQuery,
        _now_ms: u64,
    ) -> DiscoveryResult<ListServicesResult> {
        Err(unexpected_storage_dispatch_error("list_services"))
    }

    async fn list_active_instances_with_health_check(
        &self,
        _now_ms: u64,
    ) -> DiscoveryResult<Vec<ServiceInstance>> {
        Err(unexpected_storage_dispatch_error(
            "list_active_instances_with_health_check",
        ))
    }

    async fn update_health_check_state(
        &mut self,
        _namespace: &str,
        _environment: &str,
        _service_name: &str,
        _instance_id: &str,
        _state: sdkwork_discovery_contract::HealthCheckRuntimeState,
    ) -> DiscoveryResult<()> {
        Err(unexpected_storage_dispatch_error(
            "update_health_check_state",
        ))
    }
}

#[async_trait]
impl ConfigStore for FailOnReadStore {
    async fn create_config_draft(
        &mut self,
        _command: CreateConfigDraftCommand,
    ) -> DiscoveryResult<ConfigDraft> {
        Err(unexpected_storage_dispatch_error("create_config_draft"))
    }

    async fn publish_config(
        &mut self,
        _command: PublishConfigCommand,
    ) -> DiscoveryResult<ConfigRelease> {
        Err(unexpected_storage_dispatch_error("publish_config"))
    }

    async fn rollback_config(
        &mut self,
        _command: RollbackConfigCommand,
    ) -> DiscoveryResult<ConfigRelease> {
        Err(unexpected_storage_dispatch_error("rollback_config"))
    }

    async fn retrieve_effective_config(
        &self,
        _query: RetrieveEffectiveConfigQuery,
    ) -> DiscoveryResult<EffectiveConfig> {
        Err(unexpected_storage_dispatch_error(
            "retrieve_effective_config",
        ))
    }
}

#[async_trait]
impl WatchEventStore for FailOnReadStore {
    async fn watch_events(&self, _query: WatchEventsQuery) -> DiscoveryResult<Vec<DiscoveryEvent>> {
        Err(unexpected_storage_dispatch_error("watch_events"))
    }

    async fn gc_watch_events(
        &mut self,
        _before_revision: u64,
        _max_deletes: usize,
    ) -> DiscoveryResult<usize> {
        Err(unexpected_storage_dispatch_error("gc_watch_events"))
    }

    async fn compact_watch_events(
        &mut self,
        _namespace: &str,
        _environment: &str,
        _max_events_per_resource: usize,
    ) -> DiscoveryResult<usize> {
        Err(unexpected_storage_dispatch_error("compact_watch_events"))
    }
}

fn unexpected_storage_dispatch_error(operation: &str) -> DiscoveryError {
    DiscoveryError::InvalidConfig(format!("{operation} reached storage unexpectedly"))
}

fn create_service_config_draft_request(
    service_name: &str,
    value: &str,
) -> CreateConfigDraftRequest {
    CreateConfigDraftRequest {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        group: "runtime".to_string(),
        key: "log.level".to_string(),
        format: ProtoConfigFormat::Text as i32,
        value: value.to_string(),
        scope_type: ProtoConfigScopeType::Service as i32,
        application: "sdkwork-drive".to_string(),
        service_name: service_name.to_string(),
    }
}
