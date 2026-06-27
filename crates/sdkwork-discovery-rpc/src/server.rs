use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use sdkwork_discovery_contract::{DiscoveryError, DiscoveryResult};
use sdkwork_discovery_rpc_proto::generated::FILE_DESCRIPTOR_SET;
use sdkwork_discovery_rpc_proto::sdkwork::discovery::backend::v3::discovery_admin_service_server::DiscoveryAdminServiceServer;
use sdkwork_discovery_rpc_proto::sdkwork::discovery::internal::v1::discovery_config_service_server::DiscoveryConfigServiceServer;
use sdkwork_discovery_rpc_proto::sdkwork::discovery::internal::v1::discovery_watch_service_server::DiscoveryWatchServiceServer;
use sdkwork_discovery_rpc_proto::sdkwork::discovery::internal::v1::registry_service_server::RegistryServiceServer;
use sdkwork_discovery_storage_contract::{ConfigStore, RegistryStore, WatchEventStore};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{oneshot, Semaphore};
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::{Certificate, Identity, Server, ServerTlsConfig};
use tonic_health::pb::health_server::HealthServer;
use tracing::{error, info, warn};

type DiscoveryHealthServer = HealthServer<tonic_health::server::HealthService>;

fn discovery_health_pair() -> (tonic_health::server::HealthReporter, DiscoveryHealthServer) {
    let reporter = tonic_health::server::HealthReporter::new();
    let service = tonic_health::server::HealthService::from_health_reporter(reporter.clone());
    let server = DiscoveryHealthServer::new(service);
    (reporter, server)
}

use crate::health::{
    spawn_health_sync, DiscoveryHealthState, DiscoveryHealthStatus, DEFAULT_HEALTH_SYNC_INTERVAL,
};
use crate::metrics::set_health_status_value;
use crate::services::DiscoveryWatchServiceConfig;
use crate::{
    DiscoveryAdminRpcService, DiscoveryConfigRpcService, DiscoveryRpcRuntime,
    DiscoveryWatchRpcService, RegistryRpcService,
};

const SERVER_SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_millis(500);
const OVERALL_HEALTH_SERVICE: &str = "";
const REGISTRY_SERVICE_NAME: &str = "sdkwork.discovery.internal.v1.RegistryService";
const DISCOVERY_CONFIG_SERVICE_NAME: &str = "sdkwork.discovery.internal.v1.DiscoveryConfigService";
const DISCOVERY_WATCH_SERVICE_NAME: &str = "sdkwork.discovery.internal.v1.DiscoveryWatchService";
const DISCOVERY_ADMIN_SERVICE_NAME: &str = "sdkwork.discovery.backend.v3.DiscoveryAdminService";
const FULL_RPC_HEALTH_SERVICES: &[&str] = &[
    OVERALL_HEALTH_SERVICE,
    REGISTRY_SERVICE_NAME,
    DISCOVERY_CONFIG_SERVICE_NAME,
    DISCOVERY_WATCH_SERVICE_NAME,
    DISCOVERY_ADMIN_SERVICE_NAME,
];
const FULL_RPC_HEALTH_SERVICES_WITHOUT_WATCH: &[&str] = &[
    OVERALL_HEALTH_SERVICE,
    REGISTRY_SERVICE_NAME,
    DISCOVERY_CONFIG_SERVICE_NAME,
    DISCOVERY_ADMIN_SERVICE_NAME,
];
const INTERNAL_RPC_HEALTH_SERVICES: &[&str] = &[
    OVERALL_HEALTH_SERVICE,
    REGISTRY_SERVICE_NAME,
    DISCOVERY_CONFIG_SERVICE_NAME,
    DISCOVERY_WATCH_SERVICE_NAME,
];
const INTERNAL_RPC_HEALTH_SERVICES_WITHOUT_WATCH: &[&str] = &[
    OVERALL_HEALTH_SERVICE,
    REGISTRY_SERVICE_NAME,
    DISCOVERY_CONFIG_SERVICE_NAME,
];
const BACKEND_RPC_HEALTH_SERVICES: &[&str] =
    &[OVERALL_HEALTH_SERVICE, DISCOVERY_ADMIN_SERVICE_NAME];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryRpcTlsIdentity {
    pub certificate_pem: Vec<u8>,
    pub private_key_pem: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryRpcServerConfig {
    pub bind_addr: String,
    pub enable_health: bool,
    pub enable_reflection: bool,
    pub default_deadline_ms: u64,
    pub watch_enabled: bool,
    pub watch_max_streams: u32,
    pub watch_event_buffer_size: usize,
    pub watch_heartbeat_interval_ms: u64,
    pub watch_durable_poll_interval_ms: u64,
    pub watch_durable_replay_batch_size: usize,
    pub require_tls: bool,
    pub tls_identity: Option<DiscoveryRpcTlsIdentity>,
    pub client_ca_certificate_pem: Option<Vec<u8>>,
}

impl DiscoveryRpcServerConfig {
    pub fn local_development(bind_addr: impl Into<String>) -> Self {
        Self {
            bind_addr: bind_addr.into(),
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
        }
    }
}

pub struct DiscoveryRpcServices<S> {
    runtime: DiscoveryRpcRuntime<S>,
}

pub struct DiscoveryRpcServerHandle {
    shutdown: Option<oneshot::Sender<()>>,
    join_handle: tokio::task::JoinHandle<DiscoveryResult<()>>,
    health_reporter: Option<tonic_health::server::HealthReporter>,
    health_service_names: Vec<String>,
    health_sync_handle: Option<tokio::task::JoinHandle<()>>,
    health_state: Option<DiscoveryHealthState>,
}

impl DiscoveryRpcServerHandle {
    /// Returns the shared runtime health state, when gRPC health reporting is
    /// enabled. HTTP readiness probes in the service host read this cell to
    /// derive `/readyz` without touching the resilience internals.
    pub fn health_state(&self) -> Option<DiscoveryHealthState> {
        self.health_state.clone()
    }
}

impl<S> DiscoveryRpcServices<S> {
    pub fn new(runtime: DiscoveryRpcRuntime<S>) -> Self {
        Self { runtime }
    }
}

impl DiscoveryRpcServerHandle {
    pub async fn serve<S>(
        config: DiscoveryRpcServerConfig,
        services: DiscoveryRpcServices<S>,
    ) -> DiscoveryResult<Self>
    where
        S: ConfigStore + RegistryStore + WatchEventStore + Send + Sync + 'static,
    {
        let bind_addr = parse_bind_addr(&config.bind_addr)?;
        let listener = TcpListener::bind(bind_addr).await.map_err(|error| {
            DiscoveryError::InvalidConfig(format!(
                "failed to bind discovery gRPC listener {bind_addr}: {error}"
            ))
        })?;
        Self::serve_with_listener(config, services, listener).await
    }

    pub async fn serve_with_listener<S>(
        config: DiscoveryRpcServerConfig,
        services: DiscoveryRpcServices<S>,
        listener: TcpListener,
    ) -> DiscoveryResult<Self>
    where
        S: ConfigStore + RegistryStore + WatchEventStore + Send + Sync + 'static,
    {
        validate_server_config(&config)?;
        validate_transport_config(&config)?;

        let health_names = full_rpc_health_service_names(config.watch_enabled);
        start_rpc_server(
            config,
            services,
            listener,
            health_names,
            serve_with_incoming,
        )
        .await
    }

    pub async fn serve_internal<S>(
        config: DiscoveryRpcServerConfig,
        services: DiscoveryRpcServices<S>,
    ) -> DiscoveryResult<Self>
    where
        S: ConfigStore + RegistryStore + WatchEventStore + Send + Sync + 'static,
    {
        let bind_addr = parse_bind_addr(&config.bind_addr)?;
        let listener = TcpListener::bind(bind_addr).await.map_err(|error| {
            DiscoveryError::InvalidConfig(format!(
                "failed to bind discovery internal gRPC listener {bind_addr}: {error}"
            ))
        })?;
        Self::serve_internal_with_listener(config, services, listener).await
    }

    pub async fn serve_internal_with_listener<S>(
        config: DiscoveryRpcServerConfig,
        services: DiscoveryRpcServices<S>,
        listener: TcpListener,
    ) -> DiscoveryResult<Self>
    where
        S: ConfigStore + RegistryStore + WatchEventStore + Send + Sync + 'static,
    {
        validate_server_config(&config)?;
        validate_transport_config(&config)?;

        let health_names = internal_rpc_health_service_names(config.watch_enabled);
        start_rpc_server(
            config,
            services,
            listener,
            health_names,
            serve_internal_with_incoming,
        )
        .await
    }

    pub async fn serve_backend<S>(
        config: DiscoveryRpcServerConfig,
        services: DiscoveryRpcServices<S>,
    ) -> DiscoveryResult<Self>
    where
        S: ConfigStore + RegistryStore + WatchEventStore + Send + Sync + 'static,
    {
        let bind_addr = parse_bind_addr(&config.bind_addr)?;
        let listener = TcpListener::bind(bind_addr).await.map_err(|error| {
            DiscoveryError::InvalidConfig(format!(
                "failed to bind discovery backend gRPC listener {bind_addr}: {error}"
            ))
        })?;
        Self::serve_backend_with_listener(config, services, listener).await
    }

    pub async fn serve_backend_with_listener<S>(
        config: DiscoveryRpcServerConfig,
        services: DiscoveryRpcServices<S>,
        listener: TcpListener,
    ) -> DiscoveryResult<Self>
    where
        S: ConfigStore + RegistryStore + WatchEventStore + Send + Sync + 'static,
    {
        validate_server_config(&config)?;
        validate_transport_config(&config)?;

        start_rpc_server(
            config,
            services,
            listener,
            BACKEND_RPC_HEALTH_SERVICES,
            serve_backend_with_incoming,
        )
        .await
    }

    pub async fn shutdown(mut self) {
        info!("shutting down gRPC server");
        if let Some(handle) = self.health_sync_handle.take() {
            handle.abort();
            let _ = handle.await;
        }
        if let Some(reporter) = &self.health_reporter {
            set_health_services_not_serving(reporter, &self.health_service_names).await;
        }
        set_health_status_value(DiscoveryHealthStatus::NotServing.as_gauge_value());
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if tokio::time::timeout(SERVER_SHUTDOWN_GRACE_PERIOD, &mut self.join_handle)
            .await
            .is_err()
        {
            warn!("gRPC server shutdown timed out, aborting");
            self.join_handle.abort();
            let _ = self.join_handle.await;
        }
        info!("gRPC server shutdown complete");
    }
}

async fn serve_with_incoming<S>(
    config: DiscoveryRpcServerConfig,
    services: DiscoveryRpcServices<S>,
    incoming: TcpListenerStream,
    shutdown: oneshot::Receiver<()>,
    health_service: Option<DiscoveryHealthServer>,
) -> DiscoveryResult<()>
where
    S: ConfigStore + RegistryStore + WatchEventStore + Send + Sync + 'static,
{
    let watch_config = watch_service_config(&config);
    let watch_limiter = Arc::new(Semaphore::new(watch_config.max_streams as usize));
    let registry = RegistryServiceServer::new(RegistryRpcService::new(services.runtime.clone()));
    let config_service =
        DiscoveryConfigServiceServer::new(DiscoveryConfigRpcService::with_watch_limiter(
            services.runtime.clone(),
            watch_config.clone(),
            Some(watch_limiter.clone()),
        ));
    let watch = DiscoveryWatchServiceServer::new(DiscoveryWatchRpcService::with_limiter(
        services.runtime.clone(),
        watch_config,
        Some(watch_limiter),
    ));
    let admin = DiscoveryAdminServiceServer::new(DiscoveryAdminRpcService::new(services.runtime));
    let shutdown_signal = async {
        let _ = shutdown.await;
    };

    if config.enable_reflection {
        let reflection = tonic_reflection::server::Builder::configure()
            .register_encoded_file_descriptor_set(FILE_DESCRIPTOR_SET)
            .build_v1()
            .map_err(|error| {
                DiscoveryError::InvalidConfig(format!(
                    "failed to build discovery gRPC reflection service: {error}"
                ))
            })?;

        let mut router = base_server(&config)?.add_service(reflection);
        if let Some(health_service) = health_service {
            router = router.add_service(health_service);
        }
        if config.watch_enabled {
            router
                .add_service(registry)
                .add_service(config_service)
                .add_service(watch)
                .add_service(admin)
                .serve_with_incoming_shutdown(incoming, shutdown_signal)
                .await
                .map_err(server_error)?;
        } else {
            router
                .add_service(registry)
                .add_service(config_service)
                .add_service(admin)
                .serve_with_incoming_shutdown(incoming, shutdown_signal)
                .await
                .map_err(server_error)?;
        }
    } else {
        let mut server = base_server(&config)?;
        if let Some(health_service) = health_service {
            if config.watch_enabled {
                server
                    .add_service(health_service)
                    .add_service(registry)
                    .add_service(config_service)
                    .add_service(watch)
                    .add_service(admin)
                    .serve_with_incoming_shutdown(incoming, shutdown_signal)
                    .await
                    .map_err(server_error)?;
            } else {
                server
                    .add_service(health_service)
                    .add_service(registry)
                    .add_service(config_service)
                    .add_service(admin)
                    .serve_with_incoming_shutdown(incoming, shutdown_signal)
                    .await
                    .map_err(server_error)?;
            }
        } else if config.watch_enabled {
            server
                .add_service(registry)
                .add_service(config_service)
                .add_service(watch)
                .add_service(admin)
                .serve_with_incoming_shutdown(incoming, shutdown_signal)
                .await
                .map_err(server_error)?;
        } else {
            server
                .add_service(registry)
                .add_service(config_service)
                .add_service(admin)
                .serve_with_incoming_shutdown(incoming, shutdown_signal)
                .await
                .map_err(server_error)?;
        }
    }

    Ok(())
}

async fn serve_internal_with_incoming<S>(
    config: DiscoveryRpcServerConfig,
    services: DiscoveryRpcServices<S>,
    incoming: TcpListenerStream,
    shutdown: oneshot::Receiver<()>,
    health_service: Option<DiscoveryHealthServer>,
) -> DiscoveryResult<()>
where
    S: ConfigStore + RegistryStore + WatchEventStore + Send + Sync + 'static,
{
    let watch_config = watch_service_config(&config);
    let watch_limiter = Arc::new(Semaphore::new(watch_config.max_streams as usize));
    let registry = RegistryServiceServer::new(RegistryRpcService::new(services.runtime.clone()));
    let config_service =
        DiscoveryConfigServiceServer::new(DiscoveryConfigRpcService::with_watch_limiter(
            services.runtime.clone(),
            watch_config.clone(),
            Some(watch_limiter.clone()),
        ));
    let watch = DiscoveryWatchServiceServer::new(DiscoveryWatchRpcService::with_limiter(
        services.runtime.clone(),
        watch_config,
        Some(watch_limiter),
    ));
    let shutdown_signal = async {
        let _ = shutdown.await;
    };

    if config.enable_reflection {
        let reflection = tonic_reflection::server::Builder::configure()
            .register_encoded_file_descriptor_set(FILE_DESCRIPTOR_SET)
            .build_v1()
            .map_err(|error| {
                DiscoveryError::InvalidConfig(format!(
                    "failed to build discovery internal gRPC reflection service: {error}"
                ))
            })?;

        let mut router = base_server(&config)?.add_service(reflection);
        if let Some(health_service) = health_service {
            router = router.add_service(health_service);
        }
        if config.watch_enabled {
            router
                .add_service(registry)
                .add_service(config_service)
                .add_service(watch)
                .serve_with_incoming_shutdown(incoming, shutdown_signal)
                .await
                .map_err(server_error)?;
        } else {
            router
                .add_service(registry)
                .add_service(config_service)
                .serve_with_incoming_shutdown(incoming, shutdown_signal)
                .await
                .map_err(server_error)?;
        }
    } else {
        let mut server = base_server(&config)?;
        if let Some(health_service) = health_service {
            if config.watch_enabled {
                server
                    .add_service(health_service)
                    .add_service(registry)
                    .add_service(config_service)
                    .add_service(watch)
                    .serve_with_incoming_shutdown(incoming, shutdown_signal)
                    .await
                    .map_err(server_error)?;
            } else {
                server
                    .add_service(health_service)
                    .add_service(registry)
                    .add_service(config_service)
                    .serve_with_incoming_shutdown(incoming, shutdown_signal)
                    .await
                    .map_err(server_error)?;
            }
        } else if config.watch_enabled {
            server
                .add_service(registry)
                .add_service(config_service)
                .add_service(watch)
                .serve_with_incoming_shutdown(incoming, shutdown_signal)
                .await
                .map_err(server_error)?;
        } else {
            server
                .add_service(registry)
                .add_service(config_service)
                .serve_with_incoming_shutdown(incoming, shutdown_signal)
                .await
                .map_err(server_error)?;
        }
    }

    Ok(())
}

async fn serve_backend_with_incoming<S>(
    config: DiscoveryRpcServerConfig,
    services: DiscoveryRpcServices<S>,
    incoming: TcpListenerStream,
    shutdown: oneshot::Receiver<()>,
    health_service: Option<DiscoveryHealthServer>,
) -> DiscoveryResult<()>
where
    S: ConfigStore + RegistryStore + WatchEventStore + Send + Sync + 'static,
{
    let admin = DiscoveryAdminServiceServer::new(DiscoveryAdminRpcService::new(services.runtime));
    let shutdown_signal = async {
        let _ = shutdown.await;
    };

    if config.enable_reflection {
        let reflection = tonic_reflection::server::Builder::configure()
            .register_encoded_file_descriptor_set(FILE_DESCRIPTOR_SET)
            .build_v1()
            .map_err(|error| {
                DiscoveryError::InvalidConfig(format!(
                    "failed to build discovery backend gRPC reflection service: {error}"
                ))
            })?;

        let mut router = base_server(&config)?.add_service(reflection);
        if let Some(health_service) = health_service {
            router = router.add_service(health_service);
        }
        router
            .add_service(admin)
            .serve_with_incoming_shutdown(incoming, shutdown_signal)
            .await
            .map_err(server_error)?;
    } else {
        let mut server = base_server(&config)?;
        if let Some(health_service) = health_service {
            server
                .add_service(health_service)
                .add_service(admin)
                .serve_with_incoming_shutdown(incoming, shutdown_signal)
                .await
                .map_err(server_error)?;
        } else {
            server
                .add_service(admin)
                .serve_with_incoming_shutdown(incoming, shutdown_signal)
                .await
                .map_err(server_error)?;
        }
    }

    Ok(())
}

fn parse_bind_addr(bind_addr: &str) -> DiscoveryResult<SocketAddr> {
    bind_addr.parse().map_err(|error| {
        DiscoveryError::InvalidConfig(format!(
            "invalid discovery gRPC bind address {bind_addr}: {error}"
        ))
    })
}

fn server_error(error: tonic::transport::Error) -> DiscoveryError {
    DiscoveryError::InvalidConfig(format!("discovery gRPC server failed: {error}"))
}

fn validate_server_config(config: &DiscoveryRpcServerConfig) -> DiscoveryResult<()> {
    if config.default_deadline_ms == 0 {
        return Err(DiscoveryError::InvalidConfig(
            "gRPC default deadline must be greater than zero".to_string(),
        ));
    }

    if config.watch_enabled {
        if config.watch_max_streams == 0 {
            return Err(DiscoveryError::InvalidConfig(
                "watch max streams must be greater than zero".to_string(),
            ));
        }

        if config.watch_event_buffer_size == 0 {
            return Err(DiscoveryError::InvalidConfig(
                "watch event buffer size must be greater than zero".to_string(),
            ));
        }

        if config.watch_heartbeat_interval_ms == 0 {
            return Err(DiscoveryError::InvalidConfig(
                "watch heartbeat interval must be greater than zero".to_string(),
            ));
        }

        if config.watch_durable_poll_interval_ms == 0 {
            return Err(DiscoveryError::InvalidConfig(
                "watch durable poll interval must be greater than zero".to_string(),
            ));
        }

        if config.watch_durable_replay_batch_size == 0 {
            return Err(DiscoveryError::InvalidConfig(
                "watch durable replay batch size must be greater than zero".to_string(),
            ));
        }
    }

    if config.require_tls {
        let identity = config.tls_identity.as_ref().ok_or_else(|| {
            DiscoveryError::InvalidConfig(
                "TLS is required but server identity certificate and key are not configured"
                    .to_string(),
            )
        })?;

        if identity.certificate_pem.is_empty() || identity.private_key_pem.is_empty() {
            return Err(DiscoveryError::InvalidConfig(
                "TLS server identity certificate and key must not be empty".to_string(),
            ));
        }
    }

    Ok(())
}

fn watch_service_config(config: &DiscoveryRpcServerConfig) -> DiscoveryWatchServiceConfig {
    DiscoveryWatchServiceConfig {
        enabled: config.watch_enabled,
        max_streams: config.watch_max_streams,
        event_buffer_size: config.watch_event_buffer_size,
        heartbeat_interval_ms: config.watch_heartbeat_interval_ms,
        durable_poll_interval_ms: config.watch_durable_poll_interval_ms,
        durable_replay_batch_size: config.watch_durable_replay_batch_size,
    }
}

fn full_rpc_health_service_names(watch_enabled: bool) -> &'static [&'static str] {
    if watch_enabled {
        FULL_RPC_HEALTH_SERVICES
    } else {
        FULL_RPC_HEALTH_SERVICES_WITHOUT_WATCH
    }
}

fn internal_rpc_health_service_names(watch_enabled: bool) -> &'static [&'static str] {
    if watch_enabled {
        INTERNAL_RPC_HEALTH_SERVICES
    } else {
        INTERNAL_RPC_HEALTH_SERVICES_WITHOUT_WATCH
    }
}

async fn set_health_services_serving(
    health_reporter: &tonic_health::server::HealthReporter,
    service_names: &[&str],
) {
    for service_name in service_names {
        health_reporter
            .set_service_status(*service_name, tonic_health::ServingStatus::Serving)
            .await;
    }
}

async fn set_health_services_not_serving(
    health_reporter: &tonic_health::server::HealthReporter,
    service_names: &[String],
) {
    for service_name in service_names {
        health_reporter
            .set_service_status(service_name, tonic_health::ServingStatus::NotServing)
            .await;
    }
}

async fn start_rpc_server<S, ServeFn, ServeFut>(
    config: DiscoveryRpcServerConfig,
    services: DiscoveryRpcServices<S>,
    listener: TcpListener,
    health_names: &[&str],
    serve: ServeFn,
) -> DiscoveryResult<DiscoveryRpcServerHandle>
where
    S: ConfigStore + RegistryStore + WatchEventStore + Send + Sync + 'static,
    ServeFn: FnOnce(
            DiscoveryRpcServerConfig,
            DiscoveryRpcServices<S>,
            TcpListenerStream,
            oneshot::Receiver<()>,
            Option<DiscoveryHealthServer>,
        ) -> ServeFut
        + Send
        + 'static,
    ServeFut: std::future::Future<Output = DiscoveryResult<()>> + Send + 'static,
{
    validate_server_config(&config)?;
    validate_transport_config(&config)?;

    let health_service_names = if config.enable_health {
        health_names
            .iter()
            .map(|service_name| (*service_name).to_string())
            .collect()
    } else {
        Vec::new()
    };

    // Extract the shared health cell before `services` moves into the gRPC
    // task. The sync task and HTTP readiness probes read from this cell while
    // the RPC actor updates it after each command.
    let health_state = if config.enable_health {
        Some(services.runtime.health_state())
    } else {
        None
    };

    let (health_reporter, health_service) = if config.enable_health {
        let (reporter, service) = discovery_health_pair();
        set_health_services_serving(&reporter, health_names).await;
        (Some(reporter), Some(service))
    } else {
        (None, None)
    };

    // Bridge the resilience-derived health cell into grpc.health.v1 and the
    // `discovery_health_status` gauge. Coalesces transitions so rapid
    // resilience changes produce a single reporter update.
    let health_sync_handle = match (config.enable_health, health_reporter.as_ref()) {
        (true, Some(reporter)) => Some(spawn_health_sync(
            health_state
                .clone()
                .expect("health state present when enabled"),
            reporter.clone(),
            health_service_names.clone(),
            DEFAULT_HEALTH_SYNC_INTERVAL,
        )),
        _ => None,
    };

    let bind_addr = config.bind_addr.clone();
    let incoming = TcpListenerStream::new(listener);
    let (shutdown_sender, shutdown_receiver) = oneshot::channel();
    let join_handle = tokio::spawn(async move {
        info!(bind_addr = %bind_addr, "starting gRPC server");
        let result = serve(
            config,
            services,
            incoming,
            shutdown_receiver,
            health_service,
        )
        .await;
        match &result {
            Ok(_) => info!(bind_addr = %bind_addr, "gRPC server stopped"),
            Err(error) => error!(bind_addr = %bind_addr, error = %error, "gRPC server failed"),
        }
        result
    });

    Ok(DiscoveryRpcServerHandle {
        shutdown: Some(shutdown_sender),
        join_handle,
        health_reporter,
        health_service_names,
        health_sync_handle,
        health_state,
    })
}

fn validate_transport_config(config: &DiscoveryRpcServerConfig) -> DiscoveryResult<()> {
    let _ = base_server(config)?;
    Ok(())
}

fn base_server(config: &DiscoveryRpcServerConfig) -> DiscoveryResult<Server> {
    let server = Server::builder().timeout(Duration::from_millis(config.default_deadline_ms));

    if !config.require_tls {
        return Ok(server);
    }

    let identity = config.tls_identity.as_ref().ok_or_else(|| {
        DiscoveryError::InvalidConfig(
            "TLS is required but server identity certificate and key are not configured"
                .to_string(),
        )
    })?;
    let mut tls = ServerTlsConfig::new().identity(Identity::from_pem(
        &identity.certificate_pem,
        &identity.private_key_pem,
    ));

    if let Some(client_ca_certificate_pem) = &config.client_ca_certificate_pem {
        if client_ca_certificate_pem.is_empty() {
            return Err(DiscoveryError::InvalidConfig(
                "mTLS client CA certificate must not be empty".to_string(),
            ));
        }
        tls = tls.client_ca_root(Certificate::from_pem(client_ca_certificate_pem));
    }

    server.tls_config(tls).map_err(tls_config_error)
}

fn tls_config_error(error: tonic::transport::Error) -> DiscoveryError {
    DiscoveryError::InvalidConfig(format!(
        "failed to build discovery gRPC TLS config: {error}"
    ))
}

#[allow(dead_code)]
fn _assert_tcp_stream_send(_: TcpStream) {}
