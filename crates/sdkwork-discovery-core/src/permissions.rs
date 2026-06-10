use sdkwork_discovery_contract::{
    CallerContext, ConfigPermission, DiscoveryError, DiscoveryResult, RegistryPermission,
};

pub(crate) fn require_registry_permission(
    caller: &CallerContext,
    permission: RegistryPermission,
) -> DiscoveryResult<()> {
    if caller.has_registry_permission(permission) {
        Ok(())
    } else {
        Err(DiscoveryError::PermissionDenied(format!(
            "subject {} lacks registry permission",
            caller.subject_id
        )))
    }
}

pub(crate) fn require_config_permission(
    caller: &CallerContext,
    permission: ConfigPermission,
) -> DiscoveryResult<()> {
    if caller.has_config_permission(permission) {
        Ok(())
    } else {
        Err(DiscoveryError::PermissionDenied(format!(
            "subject {} lacks config permission",
            caller.subject_id
        )))
    }
}
