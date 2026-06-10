//! SDKWork Discovery storage contract crate.

mod config_store;
mod registry_store;
mod watch_event_store;

pub use config_store::ConfigStore;
pub use registry_store::RegistryStore;
pub use watch_event_store::WatchEventStore;
