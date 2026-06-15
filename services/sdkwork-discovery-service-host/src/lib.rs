//! SDKWork Discovery service host bootstrap crate.

mod bootstrap;
mod runtime;

pub use bootstrap::DiscoveryServiceHostBootstrap;
pub use bootstrap::DiscoveryServiceHostGrpcServer;
pub use runtime::DiscoveryRuntimeOptions;
pub use runtime::DiscoveryServiceHostRuntime;
