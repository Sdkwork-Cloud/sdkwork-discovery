//! SDKWork Discovery Rust gRPC adapter crate.

mod actor;
pub mod circuit_breaker;
mod codec;
mod context;
pub mod degradation;
mod error;
mod health_probes;
mod manifest;
pub mod metrics;
pub mod rate_limiter;
pub mod resilience;
pub mod server;
mod service_token;
mod services;
mod stale_read_cache;
mod watch;

pub use actor::DiscoveryRpcRuntime;
pub use actor::DiscoveryRpcRuntimeConfig;
pub use circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, CircuitState};
pub use degradation::{DegradationConfig, DegradationState, OperationType};
pub use error::map_discovery_error_to_status;
pub use manifest::discovery_rpc_service_manifest;
pub use manifest::DiscoveryRpcMethod;
pub use manifest::DiscoveryRpcServiceManifest;
#[rustfmt::skip]
pub use metrics::{decrement_active_streams, describe_metrics, increment_active_streams, RpcMetrics};
pub use rate_limiter::{RateLimitConfig, TokenBucketRateLimiter};
pub use resilience::{RuntimeResilience, RuntimeResilienceConfig};
pub use server::DiscoveryRpcServerConfig;
pub use server::DiscoveryRpcServerHandle;
pub use server::DiscoveryRpcServices;
pub use server::DiscoveryRpcTlsIdentity;
pub use service_token::DiscoveryRpcServiceTokenVerifierConfig;
pub use services::DiscoveryAdminRpcService;
pub use services::DiscoveryConfigRpcService;
pub use services::DiscoveryWatchRpcService;
pub use services::RegistryRpcService;
