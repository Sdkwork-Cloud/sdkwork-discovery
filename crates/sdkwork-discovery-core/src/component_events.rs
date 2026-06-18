//! Component-level change events declared in `specs/component.spec.json`.

pub const REGISTRY_CHANGED: &str = "discovery.registry.changed";
pub const CONFIG_CHANGED: &str = "discovery.config.changed";

/// Emits SDKWork component integration events for registry and config mutations.
pub trait ComponentChangeEmitter: Send + Sync {
    fn emit_registry_changed(&self, namespace: &str, environment: &str, revision: u64);
    fn emit_config_changed(&self, namespace: &str, environment: &str, revision: u64);
}

/// Default emitter that records component events through structured tracing.
#[derive(Debug, Default, Clone, Copy)]
pub struct TracingComponentChangeEmitter;

impl ComponentChangeEmitter for TracingComponentChangeEmitter {
    fn emit_registry_changed(&self, namespace: &str, environment: &str, revision: u64) {
        tracing::info!(
            event = REGISTRY_CHANGED,
            namespace = %namespace,
            environment = %environment,
            revision = revision,
            "discovery registry changed"
        );
    }

    fn emit_config_changed(&self, namespace: &str, environment: &str, revision: u64) {
        tracing::info!(
            event = CONFIG_CHANGED,
            namespace = %namespace,
            environment = %environment,
            revision = revision,
            "discovery config changed"
        );
    }
}
