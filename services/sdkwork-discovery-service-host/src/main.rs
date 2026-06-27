mod shutdown;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::routing::get;
use axum::Router;
use sdkwork_discovery_rpc::{describe_metrics, init_telemetry_context, RpcTelemetryContext};
use sdkwork_discovery_service_host::DiscoveryServiceHostRuntime;
use sdkwork_web_bootstrap::{
    mount_infra_routes, ReadinessCheck, ReadinessFuture, ServiceRouterConfig,
};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    let environment = std::env::var("SDKWORK_DISCOVERY_ENVIRONMENT")
        .unwrap_or_else(|_| "development".to_string());
    let service_name = "sdkwork-discovery";

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .json()
        .flatten_event(true)
        .init();

    tracing::info!(
        service = service_name,
        environment = %environment,
        "initializing sdkwork-discovery service"
    );

    describe_metrics();

    // Install the Prometheus recorder globally so that `metrics` crate facade
    // calls (counter!/gauge!/histogram!) in the RPC layer are captured. The
    // returned handle is rendered by the custom `/metrics` handler below. We
    // intentionally avoid `with_http_listener` so that metrics are exposed on
    // the same HTTP probe port as `/healthz` and `/readyz` rather than a
    // separate listener.
    #[cfg(feature = "prometheus")]
    let prometheus_handle = install_prometheus_recorder(service_name, &environment);

    #[cfg(not(feature = "prometheus"))]
    let prometheus_handle: Option<()> = None;

    match DiscoveryServiceHostRuntime::from_process_env() {
        Ok(runtime) => {
            let config = runtime.bootstrap().config();
            let deployment_profile = config.runtime.deployment_profile.as_str().to_string();
            let runtime_target = config.runtime.runtime_target.as_str().to_string();
            init_telemetry_context(RpcTelemetryContext {
                process_service: service_name.to_string(),
                environment: config.runtime.environment.as_str().to_string(),
                deployment_profile: deployment_profile.clone(),
                runtime_target: runtime_target.clone(),
            });

            if let Err(error) = runtime.bootstrap().initialize_storage().await {
                tracing::error!(
                    service = service_name,
                    environment = %environment,
                    error = %error,
                    "failed to initialize storage"
                );
                std::process::exit(1);
            }
            tracing::info!(
                service = service_name,
                environment = %environment,
                summary = %runtime.safe_summary(),
                "runtime configured"
            );
            let server = match runtime.serve_grpc().await {
                Ok(server) => server,
                Err(error) => {
                    tracing::error!(
                        service = service_name,
                        environment = %environment,
                        error = %error,
                        "failed to start gRPC server"
                    );
                    std::process::exit(1);
                }
            };
            tracing::info!(
                service = service_name,
                environment = %environment,
                "gRPC transport started"
            );

            let probe_bind = parse_probe_bind_addr(service_name, &environment);
            let health_state = server.health_state();
            let probe_handle = start_probe_server(
                probe_bind,
                health_state,
                prometheus_handle,
                service_name,
                &environment,
            );

            shutdown::wait_for_shutdown_signal().await;
            tracing::info!(
                service = service_name,
                environment = %environment,
                deployment_profile = %deployment_profile,
                runtime_target = %runtime_target,
                "shutting down"
            );

            if let Some(handle) = probe_handle {
                handle.abort();
            }
            server.shutdown().await;
        }
        Err(error) => {
            tracing::error!(
                service = service_name,
                environment = %environment,
                error = %error,
                "failed to start runtime"
            );
            std::process::exit(1);
        }
    }
}

/// Readiness probe backed by the shared discovery health state.
///
/// Returns `Ok(())` only when the runtime status is `Serving`. `Degraded`
/// (stale-read fallback) and `NotServing` (circuit breaker open without
/// fallback) both return `Err` so that `/readyz` responds `503` — signalling
/// Kubernetes/load balancers to stop routing new traffic while the gRPC
/// serving status may still be `SERVING` for `Degraded` to keep existing
/// connections draining.
struct DiscoveryHealthReadiness {
    state: sdkwork_discovery_rpc::DiscoveryHealthState,
}

impl ReadinessCheck for DiscoveryHealthReadiness {
    fn check(&self) -> ReadinessFuture<'_> {
        let ready = self.state.is_ready();
        Box::pin(async move {
            if ready {
                Ok(())
            } else {
                Err("discovery runtime is not ready".to_string())
            }
        })
    }
}

fn parse_probe_bind_addr(service_name: &str, environment: &str) -> SocketAddr {
    let addr = std::env::var("SDKWORK_DISCOVERY_HTTP_PROBE_BIND")
        .ok()
        .and_then(|value| value.parse::<SocketAddr>().ok())
        .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 8080)));
    tracing::info!(
        service = service_name,
        environment = %environment,
        probe_bind = %addr,
        "HTTP probe server configured"
    );
    addr
}

#[cfg(feature = "prometheus")]
fn install_prometheus_recorder(
    service_name: &str,
    environment: &str,
) -> Option<metrics_exporter_prometheus::PrometheusHandle> {
    use metrics_exporter_prometheus::PrometheusBuilder;
    match PrometheusBuilder::new().install_recorder() {
        Ok(handle) => {
            tracing::info!(
                service = service_name,
                environment = %environment,
                "Prometheus metrics recorder installed"
            );
            Some(handle)
        }
        Err(error) => {
            tracing::error!(
                service = service_name,
                environment = %environment,
                error = %error,
                "failed to install Prometheus recorder"
            );
            std::process::exit(1);
        }
    }
}

#[cfg(feature = "prometheus")]
fn start_probe_server(
    bind: SocketAddr,
    health_state: Option<sdkwork_discovery_rpc::DiscoveryHealthState>,
    prometheus_handle: Option<metrics_exporter_prometheus::PrometheusHandle>,
    service_name: &str,
    environment: &str,
) -> Option<tokio::task::JoinHandle<()>> {
    let readiness = health_state
        .map(|state| Arc::new(DiscoveryHealthReadiness { state }) as Arc<dyn ReadinessCheck>);

    let mut config = ServiceRouterConfig::default();
    if let Some(readiness) = readiness {
        config = config.with_readiness_check(readiness);
    } else {
        tracing::warn!(
            service = service_name,
            environment = %environment,
            "gRPC health reporting is disabled; /readyz will return 503"
        );
    }
    config = config.skip_metrics();

    let router = mount_infra_routes(Router::new(), config);
    let router = if let Some(handle) = prometheus_handle {
        router.route(
            "/metrics",
            get(move || {
                let handle = handle.clone();
                async move { render_prometheus_metrics(handle).await }
            }),
        )
    } else {
        router
    };

    Some(tokio::spawn(async move {
        let listener = match tokio::net::TcpListener::bind(bind).await {
            Ok(listener) => listener,
            Err(error) => {
                tracing::error!(
                    probe_bind = %bind,
                    error = %error,
                    "failed to bind HTTP probe listener"
                );
                return;
            }
        };
        tracing::info!(probe_bind = %bind, "HTTP probe server started");
        if let Err(error) = axum::serve(listener, router).await {
            tracing::error!(probe_bind = %bind, error = %error, "HTTP probe server stopped");
        }
    }))
}

#[cfg(not(feature = "prometheus"))]
fn start_probe_server(
    bind: SocketAddr,
    health_state: Option<sdkwork_discovery_rpc::DiscoveryHealthState>,
    _prometheus_handle: Option<()>,
    service_name: &str,
    environment: &str,
) -> Option<tokio::task::JoinHandle<()>> {
    let readiness = health_state
        .map(|state| Arc::new(DiscoveryHealthReadiness { state }) as Arc<dyn ReadinessCheck>);

    let mut config = ServiceRouterConfig::default();
    if let Some(readiness) = readiness {
        config = config.with_readiness_check(readiness);
    } else {
        tracing::warn!(
            service = service_name,
            environment = %environment,
            "gRPC health reporting is disabled; /readyz will return 503"
        );
    }

    let router = mount_infra_routes(Router::new(), config);

    Some(tokio::spawn(async move {
        let listener = match tokio::net::TcpListener::bind(bind).await {
            Ok(listener) => listener,
            Err(error) => {
                tracing::error!(
                    probe_bind = %bind,
                    error = %error,
                    "failed to bind HTTP probe listener"
                );
                return;
            }
        };
        tracing::info!(probe_bind = %bind, "HTTP probe server started");
        if let Err(error) = axum::serve(listener, router).await {
            tracing::error!(probe_bind = %bind, error = %error, "HTTP probe server stopped");
        }
    }))
}

#[cfg(feature = "prometheus")]
async fn render_prometheus_metrics(
    handle: metrics_exporter_prometheus::PrometheusHandle,
) -> impl axum::response::IntoResponse {
    (
        axum::http::StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        handle.render(),
    )
}
