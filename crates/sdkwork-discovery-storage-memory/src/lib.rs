//! SDKWork Discovery deterministic in-memory storage crate.

mod config;
mod hash;
mod registry;
mod snapshot;
mod store;
mod validation;
mod watch;

pub use snapshot::MemoryDiscoverySnapshot;
pub use store::MemoryDiscoveryStore;
