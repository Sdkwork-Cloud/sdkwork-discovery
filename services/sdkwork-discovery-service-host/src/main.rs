use sdkwork_discovery_service_host::DiscoveryServiceHostRuntime;

#[tokio::main]
async fn main() {
    match DiscoveryServiceHostRuntime::from_process_env() {
        Ok(runtime) => {
            if let Err(error) = runtime.bootstrap().initialize_storage().await {
                eprintln!("failed to initialize sdkwork-discovery storage: {error}");
                std::process::exit(1);
            }
            println!("{}", runtime.safe_summary());
            let server = match runtime.serve_grpc().await {
                Ok(server) => server,
                Err(error) => {
                    eprintln!("failed to start sdkwork-discovery gRPC server: {error}");
                    std::process::exit(1);
                }
            };
            println!("sdkwork-discovery gRPC transport started");
            if let Err(error) = tokio::signal::ctrl_c().await {
                eprintln!("failed to wait for shutdown signal: {error}");
            }
            server.shutdown().await;
        }
        Err(error) => {
            eprintln!("failed to start sdkwork-discovery: {error}");
            std::process::exit(1);
        }
    }
}
