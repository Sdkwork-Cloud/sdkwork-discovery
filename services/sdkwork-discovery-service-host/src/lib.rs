//! SDKWork Discovery service host bootstrap crate.

mod bootstrap;
mod runtime;

pub use bootstrap::DiscoveryServiceHostBootstrap;
pub use bootstrap::DiscoveryServiceHostGrpcServer;
pub use runtime::DiscoveryServiceHostRuntime;
pub use runtime::DiscoveryRuntimeOptions;
