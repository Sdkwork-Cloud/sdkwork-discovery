//! SDKWork Discovery application service crate.

mod component_events;
mod control_plane;
mod permissions;
mod policy;
mod tenant_scope;

#[rustfmt::skip]
pub use component_events::{ComponentChangeEmitter, TracingComponentChangeEmitter, CONFIG_CHANGED, REGISTRY_CHANGED};
pub use control_plane::DiscoveryControlPlane;
pub use policy::{ConfigPolicy, RegistryPolicy};
pub use tenant_scope::require_namespace_tenant_access;
