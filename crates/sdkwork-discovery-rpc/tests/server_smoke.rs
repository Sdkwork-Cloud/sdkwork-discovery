use sdkwork_discovery_contract::{InstanceStatus, RegisterInstanceCommand};
use sdkwork_discovery_core::{ConfigPolicy, DiscoveryControlPlane, RegistryPolicy};
use sdkwork_discovery_rpc::{
    DiscoveryRpcRuntime, DiscoveryRpcRuntimeConfig, DiscoveryRpcServerConfig,
    DiscoveryRpcServerHandle, DiscoveryRpcServices, DiscoveryRpcTlsIdentity,
    RuntimeResilienceConfig,
};
use sdkwork_discovery_rpc_proto::sdkwork::discovery::backend::v3::discovery_admin_service_client::DiscoveryAdminServiceClient;
use sdkwork_discovery_rpc_proto::sdkwork::discovery::backend::v3::{
    CreateConfigDraftRequest, ListServicesRequest, PublishConfigRequest,
};
use sdkwork_discovery_rpc_proto::sdkwork::discovery::common::v1 as common_proto;
use sdkwork_discovery_rpc_proto::sdkwork::discovery::common::v1::{
    ConfigFormat as ProtoConfigFormat, ConfigScopeType as ProtoConfigScopeType,
    InstanceStatus as ProtoInstanceStatus,
};
use sdkwork_discovery_rpc_proto::sdkwork::discovery::internal::v1::discovery_config_service_client::DiscoveryConfigServiceClient;
use sdkwork_discovery_rpc_proto::sdkwork::discovery::internal::v1::discovery_watch_service_client::DiscoveryWatchServiceClient;
use sdkwork_discovery_rpc_proto::sdkwork::discovery::internal::v1::registry_service_client::RegistryServiceClient;
use sdkwork_discovery_rpc_proto::sdkwork::discovery::internal::v1::RegisterInstanceRequest;
use sdkwork_discovery_rpc_proto::sdkwork::discovery::internal::v1::RetrieveEffectiveConfigRequest;
use sdkwork_discovery_rpc_proto::sdkwork::discovery::internal::v1::RetrieveInstanceRequest;
use sdkwork_discovery_rpc_proto::sdkwork::discovery::internal::v1::WatchConfigRequest;
use sdkwork_discovery_rpc_proto::sdkwork::discovery::internal::v1::WatchServiceRequest;
use sdkwork_discovery_storage_contract::RegistryStore;
use sdkwork_discovery_storage_memory::MemoryDiscoveryStore;
use tokio::net::TcpListener;
use tokio::time::{timeout, Duration};
use tonic::transport::Endpoint;
use tonic::Request;

#[tokio::test]
async fn generated_client_can_call_registry_service_on_local_server() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let runtime = runtime();
    let services = DiscoveryRpcServices::new(runtime);
    let server = DiscoveryRpcServerHandle::serve_with_listener(
        DiscoveryRpcServerConfig {
            bind_addr: addr.to_string(),
            enable_health: true,
            enable_reflection: true,
            default_deadline_ms: 5_000,
            watch_enabled: true,
            watch_max_streams: 10_000,
            watch_event_buffer_size: 1_024,
            watch_heartbeat_interval_ms: 15_000,
            watch_durable_poll_interval_ms: 1_000,
            watch_durable_replay_batch_size: 1_000,
            require_tls: false,
            tls_identity: None,
            client_ca_certificate_pem: None,
        },
        services,
        listener,
    )
    .await
    .unwrap();

    let channel = Endpoint::from_shared(format!("http://{addr}"))
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = RegistryServiceClient::new(channel);
    let mut request = Request::new(register_request());
    add_registry_write_metadata(&mut request);

    let response = client
        .register_instance(request)
        .await
        .unwrap()
        .into_inner();
    server.shutdown().await;

    assert_eq!(response.lease_id, "lease-1");
    assert!(response.metadata.unwrap().revision >= 1);
}

#[tokio::test]
async fn generated_client_can_retrieve_registered_instance_on_local_server() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let runtime = runtime();
    let services = DiscoveryRpcServices::new(runtime);
    let server = DiscoveryRpcServerHandle::serve_with_listener(
        DiscoveryRpcServerConfig {
            bind_addr: addr.to_string(),
            enable_health: true,
            enable_reflection: true,
            default_deadline_ms: 5_000,
            watch_enabled: true,
            watch_max_streams: 10_000,
            watch_event_buffer_size: 1_024,
            watch_heartbeat_interval_ms: 15_000,
            watch_durable_poll_interval_ms: 1_000,
            watch_durable_replay_batch_size: 1_000,
            require_tls: false,
            tls_identity: None,
            client_ca_certificate_pem: None,
        },
        services,
        listener,
    )
    .await
    .unwrap();

    let channel = Endpoint::from_shared(format!("http://{addr}"))
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = RegistryServiceClient::new(channel);
    let mut register = Request::new(register_request());
    add_registry_write_metadata(&mut register);
    client.register_instance(register).await.unwrap();

    let mut retrieve = Request::new(retrieve_instance_request());
    add_registry_read_metadata(&mut retrieve);
    let response = client
        .retrieve_instance(retrieve)
        .await
        .unwrap()
        .into_inner();
    server.shutdown().await;

    let instance = response
        .instance
        .expect("retrieve instance response must include registered instance");
    assert_eq!(instance.namespace, "sdkwork");
    assert_eq!(instance.environment, "development");
    assert_eq!(instance.service_name, "sdkwork-drive-product");
    assert_eq!(instance.instance_id, "drive-1");
    assert_eq!(instance.endpoint, "grpc://127.0.0.1:50051");
    assert_eq!(instance.status, ProtoInstanceStatus::Serving as i32);
    assert!(response.metadata.unwrap().revision >= 1);
}

#[tokio::test]
async fn generated_clients_can_publish_and_retrieve_effective_config_on_local_server() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let runtime = runtime();
    let services = DiscoveryRpcServices::new(runtime);
    let server = DiscoveryRpcServerHandle::serve_with_listener(
        DiscoveryRpcServerConfig {
            bind_addr: addr.to_string(),
            enable_health: true,
            enable_reflection: true,
            default_deadline_ms: 5_000,
            watch_enabled: true,
            watch_max_streams: 10_000,
            watch_event_buffer_size: 1_024,
            watch_heartbeat_interval_ms: 15_000,
            watch_durable_poll_interval_ms: 1_000,
            watch_durable_replay_batch_size: 1_000,
            require_tls: false,
            tls_identity: None,
            client_ca_certificate_pem: None,
        },
        services,
        listener,
    )
    .await
    .unwrap();

    let channel = Endpoint::from_shared(format!("http://{addr}"))
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut admin = DiscoveryAdminServiceClient::new(channel.clone());
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
    add_config_write_metadata(&mut create, "draft-smoke-1", "sha256:draft-smoke-1");
    let draft = admin
        .create_config_draft(create)
        .await
        .unwrap()
        .into_inner();

    let mut publish = Request::new(PublishConfigRequest {
        draft_id: draft.draft_id,
    });
    add_config_write_metadata(&mut publish, "publish-smoke-1", "sha256:publish-smoke-1");
    let published = admin.publish_config(publish).await.unwrap().into_inner();

    let mut config = DiscoveryConfigServiceClient::new(channel);
    let mut retrieve = Request::new(RetrieveEffectiveConfigRequest {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        application: "sdkwork-drive".to_string(),
        service_name: "sdkwork-drive-product".to_string(),
        group: "runtime".to_string(),
    });
    add_config_read_metadata(&mut retrieve);
    let effective = config
        .retrieve_effective_config(retrieve)
        .await
        .unwrap()
        .into_inner();
    server.shutdown().await;

    assert_eq!(published.metadata.unwrap().revision, 1);
    assert_eq!(effective.values.len(), 1);
    assert_eq!(effective.values[0].key, "log.level");
    assert_eq!(effective.values[0].value, "debug");
    assert_eq!(effective.values[0].source_revision, 1);
}

#[tokio::test]
async fn server_rejects_tls_required_without_server_identity_before_spawning() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let runtime = runtime();
    let services = DiscoveryRpcServices::new(runtime);

    let result = DiscoveryRpcServerHandle::serve_with_listener(
        DiscoveryRpcServerConfig {
            bind_addr: addr.to_string(),
            enable_health: true,
            enable_reflection: false,
            default_deadline_ms: 5_000,
            watch_enabled: true,
            watch_max_streams: 10_000,
            watch_event_buffer_size: 1_024,
            watch_heartbeat_interval_ms: 15_000,
            watch_durable_poll_interval_ms: 1_000,
            watch_durable_replay_batch_size: 1_000,
            require_tls: true,
            tls_identity: None,
            client_ca_certificate_pem: None,
        },
        services,
        listener,
    )
    .await;
    let error = match result {
        Ok(server) => {
            server.shutdown().await;
            panic!("TLS-required server must reject missing identity")
        }
        Err(error) => error,
    };

    assert!(error.to_string().contains("TLS"));
    assert!(error.to_string().contains("identity"));
}

#[tokio::test]
async fn server_rejects_invalid_tls_identity_before_spawning() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let runtime = runtime();
    let services = DiscoveryRpcServices::new(runtime);

    let result = DiscoveryRpcServerHandle::serve_with_listener(
        DiscoveryRpcServerConfig {
            bind_addr: addr.to_string(),
            enable_health: true,
            enable_reflection: false,
            default_deadline_ms: 5_000,
            watch_enabled: true,
            watch_max_streams: 10_000,
            watch_event_buffer_size: 1_024,
            watch_heartbeat_interval_ms: 15_000,
            watch_durable_poll_interval_ms: 1_000,
            watch_durable_replay_batch_size: 1_000,
            require_tls: true,
            tls_identity: Some(DiscoveryRpcTlsIdentity {
                certificate_pem: b"not a certificate".to_vec(),
                private_key_pem: b"not a private key".to_vec(),
            }),
            client_ca_certificate_pem: None,
        },
        services,
        listener,
    )
    .await;
    let error = match result {
        Ok(server) => {
            server.shutdown().await;
            panic!("TLS-required server must reject invalid identity PEM")
        }
        Err(error) => error,
    };

    assert!(error.to_string().contains("TLS"));
}

#[tokio::test]
async fn health_service_is_not_registered_when_disabled() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let runtime = runtime();
    let services = DiscoveryRpcServices::new(runtime);
    let server = DiscoveryRpcServerHandle::serve_with_listener(
        DiscoveryRpcServerConfig {
            bind_addr: addr.to_string(),
            enable_health: false,
            enable_reflection: false,
            default_deadline_ms: 5_000,
            watch_enabled: true,
            watch_max_streams: 10_000,
            watch_event_buffer_size: 1_024,
            watch_heartbeat_interval_ms: 15_000,
            watch_durable_poll_interval_ms: 1_000,
            watch_durable_replay_batch_size: 1_000,
            require_tls: false,
            tls_identity: None,
            client_ca_certificate_pem: None,
        },
        services,
        listener,
    )
    .await
    .unwrap();

    let channel = Endpoint::from_shared(format!("http://{addr}"))
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = tonic_health::pb::health_client::HealthClient::new(channel);
    let status = client
        .check(tonic_health::pb::HealthCheckRequest {
            service: String::new(),
        })
        .await
        .unwrap_err();
    server.shutdown().await;

    assert_eq!(status.code(), tonic::Code::Unimplemented);
}

#[tokio::test]
async fn health_service_reports_registered_rpc_services_as_serving() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let runtime = runtime();
    let services = DiscoveryRpcServices::new(runtime);
    let server = DiscoveryRpcServerHandle::serve_with_listener(
        DiscoveryRpcServerConfig {
            bind_addr: addr.to_string(),
            enable_health: true,
            enable_reflection: false,
            default_deadline_ms: 5_000,
            watch_enabled: true,
            watch_max_streams: 10_000,
            watch_event_buffer_size: 1_024,
            watch_heartbeat_interval_ms: 15_000,
            watch_durable_poll_interval_ms: 1_000,
            watch_durable_replay_batch_size: 1_000,
            require_tls: false,
            tls_identity: None,
            client_ca_certificate_pem: None,
        },
        services,
        listener,
    )
    .await
    .unwrap();

    let channel = Endpoint::from_shared(format!("http://{addr}"))
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = tonic_health::pb::health_client::HealthClient::new(channel);
    for service_name in [
        "",
        "sdkwork.discovery.internal.v1.RegistryService",
        "sdkwork.discovery.internal.v1.DiscoveryConfigService",
        "sdkwork.discovery.internal.v1.DiscoveryWatchService",
        "sdkwork.discovery.backend.v3.DiscoveryAdminService",
    ] {
        let response = client
            .check(tonic_health::pb::HealthCheckRequest {
                service: service_name.to_string(),
            })
            .await
            .unwrap()
            .into_inner();
        assert_eq!(
            response.status,
            tonic_health::pb::health_check_response::ServingStatus::Serving as i32,
            "{service_name} health status must be SERVING"
        );
    }
    server.shutdown().await;
}

#[tokio::test]
async fn health_service_flips_to_not_serving_during_shutdown() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let runtime = runtime();
    let services = DiscoveryRpcServices::new(runtime);
    let server = DiscoveryRpcServerHandle::serve_with_listener(
        DiscoveryRpcServerConfig {
            bind_addr: addr.to_string(),
            enable_health: true,
            enable_reflection: false,
            default_deadline_ms: 5_000,
            watch_enabled: true,
            watch_max_streams: 10_000,
            watch_event_buffer_size: 1_024,
            watch_heartbeat_interval_ms: 15_000,
            watch_durable_poll_interval_ms: 1_000,
            watch_durable_replay_batch_size: 1_000,
            require_tls: false,
            tls_identity: None,
            client_ca_certificate_pem: None,
        },
        services,
        listener,
    )
    .await
    .unwrap();

    let channel = Endpoint::from_shared(format!("http://{addr}"))
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = tonic_health::pb::health_client::HealthClient::new(channel);
    let response = client
        .check(tonic_health::pb::HealthCheckRequest {
            service: String::new(),
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        response.status,
        tonic_health::pb::health_check_response::ServingStatus::Serving as i32,
    );

    let shutdown_task = tokio::spawn(async move {
        server.shutdown().await;
    });

    let mut saw_not_serving = false;
    for _ in 0..20 {
        if let Ok(response) = client
            .check(tonic_health::pb::HealthCheckRequest {
                service: String::new(),
            })
            .await
        {
            if response.into_inner().status
                == tonic_health::pb::health_check_response::ServingStatus::NotServing as i32
            {
                saw_not_serving = true;
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    shutdown_task.await.unwrap();

    assert!(
        saw_not_serving,
        "health status must flip to NOT_SERVING before shutdown completes"
    );
}

#[tokio::test]
async fn internal_health_service_reports_only_internal_rpc_services() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let runtime = runtime();
    let services = DiscoveryRpcServices::new(runtime);
    let server = DiscoveryRpcServerHandle::serve_internal_with_listener(
        DiscoveryRpcServerConfig {
            bind_addr: addr.to_string(),
            enable_health: true,
            enable_reflection: false,
            default_deadline_ms: 5_000,
            watch_enabled: true,
            watch_max_streams: 10_000,
            watch_event_buffer_size: 1_024,
            watch_heartbeat_interval_ms: 15_000,
            watch_durable_poll_interval_ms: 1_000,
            watch_durable_replay_batch_size: 1_000,
            require_tls: false,
            tls_identity: None,
            client_ca_certificate_pem: None,
        },
        services,
        listener,
    )
    .await
    .unwrap();

    let channel = Endpoint::from_shared(format!("http://{addr}"))
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = tonic_health::pb::health_client::HealthClient::new(channel);
    for service_name in [
        "",
        "sdkwork.discovery.internal.v1.RegistryService",
        "sdkwork.discovery.internal.v1.DiscoveryConfigService",
        "sdkwork.discovery.internal.v1.DiscoveryWatchService",
    ] {
        let response = client
            .check(tonic_health::pb::HealthCheckRequest {
                service: service_name.to_string(),
            })
            .await
            .unwrap()
            .into_inner();
        assert_eq!(
            response.status,
            tonic_health::pb::health_check_response::ServingStatus::Serving as i32,
            "{service_name} health status must be SERVING"
        );
    }

    let status = client
        .check(tonic_health::pb::HealthCheckRequest {
            service: "sdkwork.discovery.backend.v3.DiscoveryAdminService".to_string(),
        })
        .await
        .unwrap_err();
    server.shutdown().await;

    assert_eq!(status.code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn health_service_omits_watch_service_when_watch_is_disabled() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let runtime = runtime();
    let services = DiscoveryRpcServices::new(runtime);
    let server = DiscoveryRpcServerHandle::serve_with_listener(
        DiscoveryRpcServerConfig {
            bind_addr: addr.to_string(),
            enable_health: true,
            enable_reflection: false,
            default_deadline_ms: 5_000,
            watch_enabled: false,
            watch_max_streams: 10_000,
            watch_event_buffer_size: 1_024,
            watch_heartbeat_interval_ms: 15_000,
            watch_durable_poll_interval_ms: 1_000,
            watch_durable_replay_batch_size: 1_000,
            require_tls: false,
            tls_identity: None,
            client_ca_certificate_pem: None,
        },
        services,
        listener,
    )
    .await
    .unwrap();

    let channel = Endpoint::from_shared(format!("http://{addr}"))
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = tonic_health::pb::health_client::HealthClient::new(channel);
    for service_name in [
        "",
        "sdkwork.discovery.internal.v1.RegistryService",
        "sdkwork.discovery.internal.v1.DiscoveryConfigService",
        "sdkwork.discovery.backend.v3.DiscoveryAdminService",
    ] {
        let response = client
            .check(tonic_health::pb::HealthCheckRequest {
                service: service_name.to_string(),
            })
            .await
            .unwrap()
            .into_inner();
        assert_eq!(
            response.status,
            tonic_health::pb::health_check_response::ServingStatus::Serving as i32,
            "{service_name} health status must be SERVING"
        );
    }

    let status = client
        .check(tonic_health::pb::HealthCheckRequest {
            service: "sdkwork.discovery.internal.v1.DiscoveryWatchService".to_string(),
        })
        .await
        .unwrap_err();
    server.shutdown().await;

    assert_eq!(status.code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn watch_service_streams_historical_registry_events() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let runtime = runtime();
    let services = DiscoveryRpcServices::new(runtime);
    let server = DiscoveryRpcServerHandle::serve_with_listener(
        DiscoveryRpcServerConfig {
            bind_addr: addr.to_string(),
            enable_health: true,
            enable_reflection: false,
            default_deadline_ms: 5_000,
            watch_enabled: true,
            watch_max_streams: 10_000,
            watch_event_buffer_size: 1_024,
            watch_heartbeat_interval_ms: 15_000,
            watch_durable_poll_interval_ms: 1_000,
            watch_durable_replay_batch_size: 1_000,
            require_tls: false,
            tls_identity: None,
            client_ca_certificate_pem: None,
        },
        services,
        listener,
    )
    .await
    .unwrap();

    let channel = Endpoint::from_shared(format!("http://{addr}"))
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut registry = RegistryServiceClient::new(channel.clone());
    let mut register = Request::new(register_request());
    add_registry_write_metadata(&mut register);
    registry.register_instance(register).await.unwrap();

    let mut watch = DiscoveryWatchServiceClient::new(channel);
    let mut request = Request::new(WatchServiceRequest {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        service_name: "sdkwork-drive-product".to_string(),
        from_revision: 0,
    });
    add_registry_read_metadata(&mut request);
    let mut stream = watch.watch_service(request).await.unwrap().into_inner();
    let event = stream.message().await.unwrap().unwrap();
    server.shutdown().await;

    assert_eq!(
        event.event_type,
        common_proto::WatchEventType::InstanceRegistered as i32
    );
    assert!(event.metadata.as_ref().unwrap().revision >= 1);
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
    assert_eq!(instance.revision, event.metadata.unwrap().revision);
}

#[tokio::test]
async fn watch_service_replays_history_then_streams_live_registry_events() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let runtime = runtime();
    let services = DiscoveryRpcServices::new(runtime);
    let server = DiscoveryRpcServerHandle::serve_with_listener(
        DiscoveryRpcServerConfig {
            bind_addr: addr.to_string(),
            enable_health: true,
            enable_reflection: false,
            default_deadline_ms: 5_000,
            watch_enabled: true,
            watch_max_streams: 10_000,
            watch_event_buffer_size: 1_024,
            watch_heartbeat_interval_ms: 15_000,
            watch_durable_poll_interval_ms: 1_000,
            watch_durable_replay_batch_size: 1_000,
            require_tls: false,
            tls_identity: None,
            client_ca_certificate_pem: None,
        },
        services,
        listener,
    )
    .await
    .unwrap();

    let channel = Endpoint::from_shared(format!("http://{addr}"))
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut registry = RegistryServiceClient::new(channel.clone());

    let mut historical_register = Request::new(register_request_for("drive-1"));
    add_registry_write_metadata(&mut historical_register);
    registry
        .register_instance(historical_register)
        .await
        .unwrap();

    let mut watch = DiscoveryWatchServiceClient::new(channel);
    let mut request = Request::new(WatchServiceRequest {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        service_name: "sdkwork-drive-product".to_string(),
        from_revision: 0,
    });
    add_registry_read_metadata(&mut request);
    let mut stream = watch.watch_service(request).await.unwrap().into_inner();

    let replayed = timeout(Duration::from_secs(2), stream.message())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        replayed.event_type,
        common_proto::WatchEventType::InstanceRegistered as i32
    );
    assert_eq!(replayed.metadata.as_ref().unwrap().revision, 1);
    let replayed_instance = replayed
        .instance
        .expect("historical watch replay must include service instance payload");
    assert_eq!(replayed_instance.instance_id, "drive-1");
    assert_eq!(replayed_instance.endpoint, "grpc://127.0.0.1:50051");
    assert_eq!(
        replayed_instance.status,
        ProtoInstanceStatus::Serving as i32
    );
    assert_eq!(replayed_instance.revision, 1);

    let mut live_register = Request::new(register_request_for("drive-2"));
    add_registry_write_metadata(&mut live_register);
    registry.register_instance(live_register).await.unwrap();

    let live = timeout(Duration::from_secs(2), stream.message())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    server.shutdown().await;

    assert_eq!(
        live.event_type,
        common_proto::WatchEventType::InstanceRegistered as i32
    );
    assert_eq!(live.metadata.as_ref().unwrap().revision, 2);
    let live_instance = live
        .instance
        .expect("live watch event must include service instance payload");
    assert_eq!(live_instance.instance_id, "drive-2");
    assert_eq!(live_instance.endpoint, "grpc://127.0.0.1:50051");
    assert_eq!(live_instance.status, ProtoInstanceStatus::Serving as i32);
    assert_eq!(live_instance.revision, 2);
}

#[tokio::test]
async fn runtime_expiry_scan_publishes_live_deregister_watch_event() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
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
    let runtime = DiscoveryRpcRuntime::with_config(
        DiscoveryControlPlane::new(
            store,
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
            registry_expiry_scan_interval_ms: 20,
            registry_expiry_scan_batch_size: 1_000,
            allow_unsigned_local_context: true,
            service_token_verifier: None,
            event_gc_interval_ms: 0,
            event_gc_retention_count: 10_000,
            event_gc_batch_size: 1_000,
            resilience: RuntimeResilienceConfig::default(),
            health_check_scan_interval_ms: 0,
        },
    );
    let services = DiscoveryRpcServices::new(runtime);
    let server = DiscoveryRpcServerHandle::serve_with_listener(
        DiscoveryRpcServerConfig {
            bind_addr: addr.to_string(),
            enable_health: true,
            enable_reflection: false,
            default_deadline_ms: 5_000,
            watch_enabled: true,
            watch_max_streams: 10_000,
            watch_event_buffer_size: 1_024,
            watch_heartbeat_interval_ms: 15_000,
            watch_durable_poll_interval_ms: 1_000,
            watch_durable_replay_batch_size: 1_000,
            require_tls: false,
            tls_identity: None,
            client_ca_certificate_pem: None,
        },
        services,
        listener,
    )
    .await
    .unwrap();

    let channel = Endpoint::from_shared(format!("http://{addr}"))
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut watch = DiscoveryWatchServiceClient::new(channel);
    let mut request = Request::new(WatchServiceRequest {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        service_name: "sdkwork-drive-product".to_string(),
        from_revision: 1,
    });
    add_registry_read_metadata(&mut request);
    let mut stream = watch.watch_service(request).await.unwrap().into_inner();

    let event = timeout(Duration::from_secs(2), stream.message())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    server.shutdown().await;

    assert_eq!(
        event.event_type,
        common_proto::WatchEventType::InstanceDeregistered as i32
    );
    assert_eq!(event.metadata.as_ref().unwrap().revision, 2);
    let instance = event
        .instance
        .expect("deregister watch event must include tombstone service identity");
    assert_eq!(instance.instance_id, "drive-1");
    assert_eq!(instance.service_name, "sdkwork-drive-product");
}

#[tokio::test]
async fn watch_service_rejects_streams_over_configured_limit() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let runtime = runtime();
    let services = DiscoveryRpcServices::new(runtime);
    let server = DiscoveryRpcServerHandle::serve_with_listener(
        DiscoveryRpcServerConfig {
            bind_addr: addr.to_string(),
            enable_health: true,
            enable_reflection: false,
            default_deadline_ms: 5_000,
            watch_enabled: true,
            watch_max_streams: 1,
            watch_event_buffer_size: 8,
            watch_heartbeat_interval_ms: 1_000,
            watch_durable_poll_interval_ms: 1_000,
            watch_durable_replay_batch_size: 1_000,
            require_tls: false,
            tls_identity: None,
            client_ca_certificate_pem: None,
        },
        services,
        listener,
    )
    .await
    .unwrap();

    let channel = Endpoint::from_shared(format!("http://{addr}"))
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut first_watch = DiscoveryWatchServiceClient::new(channel.clone());
    let mut first_request = Request::new(watch_service_request());
    add_registry_read_metadata(&mut first_request);
    let mut first_stream = first_watch
        .watch_service(first_request)
        .await
        .unwrap()
        .into_inner();

    let mut second_watch = DiscoveryWatchServiceClient::new(channel);
    let mut second_request = Request::new(watch_service_request());
    add_registry_read_metadata(&mut second_request);
    let status = second_watch
        .watch_service(second_request)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::ResourceExhausted);

    assert!(timeout(Duration::from_millis(100), first_stream.message())
        .await
        .is_err());

    drop(first_stream);
    server.shutdown().await;
}

#[tokio::test]
async fn watch_stream_limit_is_shared_by_service_and_config_watch_services() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let runtime = runtime();
    let services = DiscoveryRpcServices::new(runtime);
    let server = DiscoveryRpcServerHandle::serve_with_listener(
        DiscoveryRpcServerConfig {
            bind_addr: addr.to_string(),
            enable_health: true,
            enable_reflection: false,
            default_deadline_ms: 5_000,
            watch_enabled: true,
            watch_max_streams: 1,
            watch_event_buffer_size: 8,
            watch_heartbeat_interval_ms: 1_000,
            watch_durable_poll_interval_ms: 1_000,
            watch_durable_replay_batch_size: 1_000,
            require_tls: false,
            tls_identity: None,
            client_ca_certificate_pem: None,
        },
        services,
        listener,
    )
    .await
    .unwrap();

    let channel = Endpoint::from_shared(format!("http://{addr}"))
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut config_watch = DiscoveryConfigServiceClient::new(channel.clone());
    let mut config_request = Request::new(WatchConfigRequest {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        application: "sdkwork-drive".to_string(),
        service_name: "sdkwork-drive-product".to_string(),
        group: "runtime".to_string(),
        from_revision: 0,
    });
    add_config_read_metadata(&mut config_request);
    let mut config_stream = config_watch
        .watch_config(config_request)
        .await
        .unwrap()
        .into_inner();

    let mut service_watch = DiscoveryWatchServiceClient::new(channel);
    let mut service_request = Request::new(watch_service_request());
    add_registry_read_metadata(&mut service_request);
    let status = service_watch
        .watch_service(service_request)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::ResourceExhausted);

    assert!(timeout(Duration::from_millis(100), config_stream.message())
        .await
        .is_err());

    drop(config_stream);
    server.shutdown().await;
}

#[tokio::test]
async fn watch_service_sends_idle_heartbeat() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let runtime = runtime();
    let services = DiscoveryRpcServices::new(runtime);
    let server = DiscoveryRpcServerHandle::serve_with_listener(
        DiscoveryRpcServerConfig {
            bind_addr: addr.to_string(),
            enable_health: true,
            enable_reflection: false,
            default_deadline_ms: 5_000,
            watch_enabled: true,
            watch_max_streams: 16,
            watch_event_buffer_size: 8,
            watch_heartbeat_interval_ms: 20,
            watch_durable_poll_interval_ms: 1_000,
            watch_durable_replay_batch_size: 1_000,
            require_tls: false,
            tls_identity: None,
            client_ca_certificate_pem: None,
        },
        services,
        listener,
    )
    .await
    .unwrap();

    let channel = Endpoint::from_shared(format!("http://{addr}"))
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut watch = DiscoveryWatchServiceClient::new(channel);
    let mut request = Request::new(watch_service_request());
    add_registry_read_metadata(&mut request);
    let mut stream = watch.watch_service(request).await.unwrap().into_inner();

    let heartbeat = timeout(Duration::from_secs(1), stream.message())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    server.shutdown().await;

    assert_eq!(
        heartbeat.event_type,
        common_proto::WatchEventType::Heartbeat as i32
    );
    assert_eq!(heartbeat.metadata.unwrap().revision, 0);
}

#[tokio::test]
async fn generated_client_receives_invalid_argument_for_blank_watch_filter() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let runtime = runtime();
    let services = DiscoveryRpcServices::new(runtime);
    let server = DiscoveryRpcServerHandle::serve_with_listener(
        DiscoveryRpcServerConfig {
            bind_addr: addr.to_string(),
            enable_health: true,
            enable_reflection: false,
            default_deadline_ms: 5_000,
            watch_enabled: true,
            watch_max_streams: 16,
            watch_event_buffer_size: 8,
            watch_heartbeat_interval_ms: 1_000,
            watch_durable_poll_interval_ms: 1_000,
            watch_durable_replay_batch_size: 1_000,
            require_tls: false,
            tls_identity: None,
            client_ca_certificate_pem: None,
        },
        services,
        listener,
    )
    .await
    .unwrap();

    let channel = Endpoint::from_shared(format!("http://{addr}"))
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut watch = DiscoveryWatchServiceClient::new(channel);
    let mut request = Request::new(WatchServiceRequest {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        service_name: " ".to_string(),
        from_revision: 0,
    });
    add_registry_read_metadata(&mut request);
    let status = watch.watch_service(request).await.unwrap_err();
    server.shutdown().await;

    assert_eq!(status.code(), tonic::Code::InvalidArgument);
    assert!(status.message().contains("service_name"));
}

#[tokio::test]
async fn watch_service_is_unimplemented_when_disabled() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let runtime = runtime();
    let services = DiscoveryRpcServices::new(runtime);
    let server = DiscoveryRpcServerHandle::serve_with_listener(
        DiscoveryRpcServerConfig {
            bind_addr: addr.to_string(),
            enable_health: true,
            enable_reflection: false,
            default_deadline_ms: 5_000,
            watch_enabled: false,
            watch_max_streams: 16,
            watch_event_buffer_size: 8,
            watch_heartbeat_interval_ms: 1_000,
            watch_durable_poll_interval_ms: 1_000,
            watch_durable_replay_batch_size: 1_000,
            require_tls: false,
            tls_identity: None,
            client_ca_certificate_pem: None,
        },
        services,
        listener,
    )
    .await
    .unwrap();

    let channel = Endpoint::from_shared(format!("http://{addr}"))
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut watch = DiscoveryWatchServiceClient::new(channel);
    let mut request = Request::new(watch_service_request());
    add_registry_read_metadata(&mut request);
    let status = watch.watch_service(request).await.unwrap_err();
    server.shutdown().await;

    assert_eq!(status.code(), tonic::Code::Unimplemented);
}

#[tokio::test]
async fn server_rejects_zero_watch_stream_capacity_before_spawning() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let runtime = runtime();
    let services = DiscoveryRpcServices::new(runtime);

    let result = DiscoveryRpcServerHandle::serve_with_listener(
        DiscoveryRpcServerConfig {
            bind_addr: addr.to_string(),
            enable_health: true,
            enable_reflection: false,
            default_deadline_ms: 5_000,
            watch_enabled: true,
            watch_max_streams: 0,
            watch_event_buffer_size: 8,
            watch_heartbeat_interval_ms: 1_000,
            watch_durable_poll_interval_ms: 1_000,
            watch_durable_replay_batch_size: 1_000,
            require_tls: false,
            tls_identity: None,
            client_ca_certificate_pem: None,
        },
        services,
        listener,
    )
    .await;
    let error = match result {
        Ok(server) => {
            server.shutdown().await;
            panic!("server must reject zero watch stream capacity")
        }
        Err(error) => error,
    };

    assert!(error.to_string().contains("watch"));
    assert!(error.to_string().contains("max streams"));
}

#[tokio::test]
async fn server_rejects_zero_durable_watch_poll_interval_before_spawning() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let runtime = runtime();
    let services = DiscoveryRpcServices::new(runtime);

    let result = DiscoveryRpcServerHandle::serve_with_listener(
        DiscoveryRpcServerConfig {
            bind_addr: addr.to_string(),
            enable_health: true,
            enable_reflection: false,
            default_deadline_ms: 5_000,
            watch_enabled: true,
            watch_max_streams: 16,
            watch_event_buffer_size: 8,
            watch_heartbeat_interval_ms: 1_000,
            watch_durable_poll_interval_ms: 0,
            watch_durable_replay_batch_size: 1_000,
            require_tls: false,
            tls_identity: None,
            client_ca_certificate_pem: None,
        },
        services,
        listener,
    )
    .await;
    let error = match result {
        Ok(server) => {
            server.shutdown().await;
            panic!("server must reject zero durable watch poll interval")
        }
        Err(error) => error,
    };

    assert!(error.to_string().contains("watch"));
    assert!(error.to_string().contains("durable poll interval"));
}

#[tokio::test]
async fn internal_server_does_not_expose_backend_admin_service() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let runtime = runtime();
    let services = DiscoveryRpcServices::new(runtime);
    let server = DiscoveryRpcServerHandle::serve_internal_with_listener(
        DiscoveryRpcServerConfig {
            bind_addr: addr.to_string(),
            enable_health: true,
            enable_reflection: false,
            default_deadline_ms: 5_000,
            watch_enabled: true,
            watch_max_streams: 10_000,
            watch_event_buffer_size: 1_024,
            watch_heartbeat_interval_ms: 15_000,
            watch_durable_poll_interval_ms: 1_000,
            watch_durable_replay_batch_size: 1_000,
            require_tls: false,
            tls_identity: None,
            client_ca_certificate_pem: None,
        },
        services,
        listener,
    )
    .await
    .unwrap();
    let channel = Endpoint::from_shared(format!("http://{addr}"))
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = DiscoveryAdminServiceClient::new(channel);
    let mut request = Request::new(ListServicesRequest {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        page: None,
    });
    request
        .metadata_mut()
        .insert("x-sdkwork-subject-id", "operator-1".parse().unwrap());
    request
        .metadata_mut()
        .insert("x-sdkwork-registry-permissions", "read".parse().unwrap());
    add_auth_metadata(&mut request);

    let status = client.list_services(request).await.unwrap_err();
    server.shutdown().await;

    assert_eq!(status.code(), tonic::Code::Unimplemented);
}

fn runtime() -> DiscoveryRpcRuntime<MemoryDiscoveryStore> {
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

fn add_config_read_metadata<T>(request: &mut Request<T>) {
    add_auth_metadata(request);
    request
        .metadata_mut()
        .insert("x-sdkwork-subject-id", "service-1".parse().unwrap());
    request
        .metadata_mut()
        .insert("x-sdkwork-config-permissions", "read".parse().unwrap());
}

fn add_auth_metadata<T>(request: &mut Request<T>) {
    request
        .metadata_mut()
        .insert("authorization", "Bearer test-auth-token".parse().unwrap());
    request
        .metadata_mut()
        .insert("access-token", "test-access-token".parse().unwrap());
}

fn watch_service_request() -> WatchServiceRequest {
    WatchServiceRequest {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        service_name: "sdkwork-drive-product".to_string(),
        from_revision: 0,
    }
}

fn register_request() -> RegisterInstanceRequest {
    register_request_for("drive-1")
}

fn register_request_for(instance_id: &str) -> RegisterInstanceRequest {
    RegisterInstanceRequest {
        namespace: "sdkwork".to_string(),
        environment: "development".to_string(),
        service_name: "sdkwork-drive-product".to_string(),
        instance_id: instance_id.to_string(),
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
