//! SDKWork Discovery application service crate.

mod control_plane;
mod permissions;
mod policy;

pub use control_plane::DiscoveryControlPlane;
pub use policy::{ConfigPolicy, RegistryPolicy};
