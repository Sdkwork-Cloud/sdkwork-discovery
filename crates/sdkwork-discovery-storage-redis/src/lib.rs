//! SDKWork Discovery Redis storage adapter crate.

mod config;
mod connection;
mod registry;
mod store;
mod watch;

pub use connection::{RedisConnectionOptions, DISCOVERY_REDIS_KEY_PREFIX};
pub use store::{map_redis_error, RedisDiscoveryStore};
