//! SDKWork Discovery deterministic in-memory storage crate.

mod config;
mod hash;
mod registry;
mod store;
mod validation;
mod watch;

pub use store::MemoryDiscoveryStore;
