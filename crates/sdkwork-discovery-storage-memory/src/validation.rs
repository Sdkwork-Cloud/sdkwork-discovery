use sdkwork_discovery_contract::{DiscoveryError, DiscoveryResult};

pub(crate) fn validate_non_empty(field: &str, value: &str) -> DiscoveryResult<()> {
    if value.trim().is_empty() {
        return Err(DiscoveryError::InvalidArgument(format!(
            "{field} must not be empty"
        )));
    }
    Ok(())
}
