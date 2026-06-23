//! SDKWork Discovery database host integration crate.

mod bootstrap;

#[rustfmt::skip]
pub use bootstrap::{bootstrap_discovery_database, bootstrap_discovery_database_from_env, DiscoveryDatabaseHost};
