//! SDKWork Discovery product service bootstrap crate.

mod bootstrap;
mod runtime;

pub use bootstrap::DiscoveryProductBootstrap;
pub use bootstrap::DiscoveryProductGrpcServer;
pub use runtime::DiscoveryProductRuntime;
pub use runtime::DiscoveryRuntimeOptions;
