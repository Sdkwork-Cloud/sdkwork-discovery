//! API assembly bootstrap for sdkwork-discovery.

use sdkwork_web_bootstrap::{ApiAssemblyContribution, HttpRouteManifest, WebModule};
use axum::Router;

pub type ApiAssembly = ApiAssemblyContribution;

pub fn assemble_api_router() -> ApiAssembly {
    ApiAssemblyContribution::from_manifest(
        "sdkwork-discovery",
        "SDKWork Discovery API",
        Router::new(),
        HttpRouteManifest::from_owned_routes(Vec::new()),
        Vec::new(),
        std::sync::Arc::new(sdkwork_web_bootstrap::AlwaysReady),
    )
    .unwrap_or_else(|error| panic!("sdkwork-discovery API assembly failed: {error}"))
}

/// Canonical Web Module definition for this application
/// (API_ASSEMBLY_SPEC §4.1.1): the complete HTTP surface — every route,
/// manifest, and OpenAPI document of this owner — as one installable module.
pub fn web_module() -> Result<WebModule, String> {
    Ok(WebModule::from_contribution(assemble_api_router()))
}
