use sdkwork_discovery_rpc::describe_metrics;
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
        PrometheusBuilder::new()
            .with_http_listener(([0, 0, 0, 0], 9090))
            .install()
            .expect("failed to install Prometheus exporter");
        tracing::info!(
            service = service_name,
            environment = %environment,
            "Prometheus metrics exporter listening on :9090"
        );
    }

    match DiscoveryServiceHostRuntime::from_process_env() {
        Ok(runtime) => {
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
            if let Err(error) = tokio::signal::ctrl_c().await {
                tracing::error!(
                    service = service_name,
                    environment = %environment,
                    error = %error,
                    "failed to wait for shutdown signal"
                );
            }
            tracing::info!(
                service = service_name,
                environment = %environment,
                "shutting down"
            );
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
