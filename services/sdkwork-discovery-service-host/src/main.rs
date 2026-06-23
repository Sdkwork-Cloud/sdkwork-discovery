mod shutdown;

use sdkwork_discovery_rpc::{
    describe_metrics, init_telemetry_context, set_health_status, RpcTelemetryContext,
};
use sdkwork_discovery_service_host::DiscoveryServiceHostRuntime;
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

    #[cfg(feature = "prometheus")]
    {
        use metrics_exporter_prometheus::PrometheusBuilder;
        use std::net::SocketAddr;

        let metrics_bind = std::env::var("SDKWORK_DISCOVERY_METRICS_BIND")
            .ok()
            .and_then(|value| value.parse::<SocketAddr>().ok())
            .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 9090)));

        PrometheusBuilder::new()
            .with_http_listener(metrics_bind)
            .install()
            .expect("failed to install Prometheus exporter");
        tracing::info!(
            service = service_name,
            environment = %environment,
            metrics_bind = %metrics_bind,
            "Prometheus metrics exporter listening"
        );
    }

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
            set_health_status(true);

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
            shutdown::wait_for_shutdown_signal().await;
            tracing::info!(
                service = service_name,
                environment = %environment,
                deployment_profile = %deployment_profile,
                runtime_target = %runtime_target,
                "shutting down"
            );
            set_health_status(false);
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
