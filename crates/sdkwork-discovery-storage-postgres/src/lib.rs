//! SDKWork Discovery PostgreSQL storage crate.

mod codec;
mod config;
mod hash;
pub mod migration;
pub mod options;
mod registry;
pub mod sql;
pub mod store;
mod validation;
mod watch;

pub use options::PostgresConnectionOptions;
pub use store::PostgresDiscoveryStore;
