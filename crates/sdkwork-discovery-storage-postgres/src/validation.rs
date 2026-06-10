use sdkwork_discovery_contract::{DiscoveryError, DiscoveryResult};

pub(crate) fn validate_non_empty(field: &str, value: &str) -> DiscoveryResult<()> {
    if value.trim().is_empty() {
        return Err(DiscoveryError::InvalidArgument(format!(
            "{field} must not be empty"
        )));
    }
    Ok(())
}

pub(crate) fn to_i64(field: &str, value: u64) -> DiscoveryResult<i64> {
    i64::try_from(value).map_err(|_| {
        DiscoveryError::InvalidArgument(format!("{field} is too large for postgres BIGINT"))
    })
}

pub(crate) fn usize_to_i64(field: &str, value: usize) -> DiscoveryResult<i64> {
    i64::try_from(value).map_err(|_| {
        DiscoveryError::InvalidArgument(format!("{field} is too large for postgres BIGINT"))
    })
}

pub(crate) fn i64_to_u64(field: &str, value: i64) -> DiscoveryResult<u64> {
    u64::try_from(value)
        .map_err(|_| DiscoveryError::InvalidConfig(format!("postgres returned negative {field}")))
}

pub(crate) fn u32_to_i32(field: &str, value: u32) -> DiscoveryResult<i32> {
    i32::try_from(value).map_err(|_| {
        DiscoveryError::InvalidArgument(format!("{field} is too large for postgres INTEGER"))
    })
}

pub(crate) fn i64_to_usize(field: &str, value: i64) -> DiscoveryResult<usize> {
    usize::try_from(value)
        .map_err(|_| DiscoveryError::InvalidConfig(format!("postgres returned invalid {field}")))
}
