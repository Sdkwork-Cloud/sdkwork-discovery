//! SDKWork Discovery SQLite storage adapter crate.

mod codec;
mod config;
mod database_bootstrap;
mod hash;
pub mod migration;
mod registry;
pub mod sql;
pub mod store;
mod validation;
mod watch;

pub use store::SqliteDiscoveryStore;
