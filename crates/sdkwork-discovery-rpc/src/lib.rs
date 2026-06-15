//! SDKWork Discovery Rust gRPC adapter crate.

mod actor;
mod codec;
mod context;
mod error;
mod manifest;
pub mod metrics;
pub mod server;
mod service_token;
mod services;
mod watch;

pub use actor::DiscoveryRpcRuntime;
pub use actor::DiscoveryRpcRuntimeConfig;
pub use error::map_discovery_error_to_status;
pub use manifest::discovery_rpc_service_manifest;
pub use manifest::DiscoveryRpcMethod;
pub use manifest::DiscoveryRpcServiceManifest;
pub use metrics::{
    decrement_active_streams, describe_metrics, increment_active_streams, RpcMetrics,
};
pub use server::DiscoveryRpcServerConfig;
pub use server::DiscoveryRpcServerHandle;
pub use server::DiscoveryRpcServices;
pub use server::DiscoveryRpcTlsIdentity;
pub use service_token::DiscoveryRpcServiceTokenVerifierConfig;
pub use services::DiscoveryAdminRpcService;
pub use services::DiscoveryConfigRpcService;
pub use services::DiscoveryWatchRpcService;
pub use services::RegistryRpcService;
