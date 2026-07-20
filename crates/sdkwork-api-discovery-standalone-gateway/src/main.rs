use sdkwork_api_discovery_assembly as api_assembly;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let bind_address = std::env::var("SDKWORK_DISCOVERY_APPLICATION_PUBLIC_INGRESS_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8080".to_owned());
    let app = api_assembly::assemble_api_router().router;
    let listener = tokio::net::TcpListener::bind(&bind_address).await?;
    eprintln!("sdkwork-api-discovery-standalone-gateway listening on {bind_address}");
    axum::serve(listener, app).await?;
    Ok(())
}
