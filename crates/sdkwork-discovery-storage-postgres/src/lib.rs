//! SDKWork Discovery PostgreSQL storage crate.

mod bootstrap;
mod codec;
mod config;
mod database_bootstrap;
mod hash;
pub mod migration;
pub mod options;
mod registry;
pub mod sql;
pub mod store;
mod validation;
mod watch;

#[rustfmt::skip]
pub use bootstrap::{bootstrap_discovery_database, bootstrap_discovery_database_from_env, connect_and_bootstrap_discovery_database_from_env, connect_discovery_database_pool_from_env, DiscoveryDatabaseHost, DiscoveryDatabasePool};
pub use options::PostgresConnectionOptions;
pub use store::PostgresDiscoveryStore;
