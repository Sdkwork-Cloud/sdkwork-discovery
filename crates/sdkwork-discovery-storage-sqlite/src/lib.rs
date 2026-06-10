//! SDKWork Discovery SQLite storage adapter crate.

mod codec;
mod config;
mod hash;
pub mod migration;
mod registry;
pub mod sql;
pub mod store;
mod validation;
mod watch;

pub use store::SqliteDiscoveryStore;
