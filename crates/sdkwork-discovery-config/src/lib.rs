//! SDKWork Discovery typed runtime configuration crate.

mod env_overlay;
mod loader;
mod model;

pub use model::ConfigRegistryConfig;
pub use model::DiscoveryRuntimeConfig;
pub use model::RegistryConfig;
pub use model::ResilienceCircuitBreakerConfig;
pub use model::ResilienceConfig;
pub use model::ResilienceDegradationConfig;
pub use model::ResilienceRateLimitConfig;
pub use model::RuntimeConfig;
pub use model::SecurityAuthMode;
pub use model::SecurityConfig;
pub use model::ServerConfig;
pub use model::ServiceTokenConfig;
pub use model::StorageConfig;
pub use model::StorageCredentialSource;
pub use model::StorageProvider;
pub use model::StorageRole;
pub use model::StorageTransportConfig;
pub use model::WatchConfig;
pub use sdkwork_discovery_contract::RuntimeDeploymentMode;
pub use sdkwork_discovery_contract::RuntimeEnvironment;
pub use sdkwork_discovery_contract::RuntimeTarget;
