use sdkwork_discovery_contract::{CallerContext, DiscoveryError, DiscoveryResult};

/// When the caller carries a tenant id, the target namespace must belong to that tenant.
pub fn require_namespace_tenant_access(
    caller: &CallerContext,
    namespace: &str,
) -> DiscoveryResult<()> {
    let Some(tenant_id) = caller.tenant_id.as_deref() else {
        return Ok(());
    };

    if namespace == tenant_id || namespace.starts_with(&format!("{tenant_id}/")) {
        return Ok(());
    }

    Err(DiscoveryError::PermissionDenied(format!(
        "subject {} is not authorized for namespace {}",
        caller.subject_id, namespace
    )))
}
