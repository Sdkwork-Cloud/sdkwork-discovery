use sdkwork_discovery_contract::{ConfigFormat, DiscoveryError, DiscoveryResult, EffectiveConfig};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigPolicy {
    pub enabled: bool,
    pub require_publish_for_reads: bool,
    pub allow_secret_values: bool,
    pub allow_secret_refs: bool,
    pub max_config_body_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryPolicy {
    pub default_lease_ttl_seconds: u64,
    pub min_lease_ttl_seconds: u64,
    pub max_lease_ttl_seconds: u64,
}

impl Default for RegistryPolicy {
    fn default() -> Self {
        Self {
            default_lease_ttl_seconds: 30,
            min_lease_ttl_seconds: 1,
            max_lease_ttl_seconds: 300,
        }
    }
}

pub(crate) fn require_config_registry_enabled(policy: &ConfigPolicy) -> DiscoveryResult<()> {
    if !policy.enabled {
        return Err(DiscoveryError::InvalidConfig(
            "config registry is disabled".to_string(),
        ));
    }

    Ok(())
}

pub(crate) fn validate_config_policy(
    policy: &ConfigPolicy,
    format: &ConfigFormat,
    value: &str,
) -> DiscoveryResult<()> {
    if value.len() > policy.max_config_body_bytes {
        return Err(DiscoveryError::PolicyViolation(
            "config body exceeds configured maximum size".to_string(),
        ));
    }

    validate_config_format(format, value)?;

    if is_secret_reference(value) {
        if policy.allow_secret_refs {
            return Ok(());
        }
        return Err(DiscoveryError::PolicyViolation(
            "secret references are disabled by policy".to_string(),
        ));
    }

    if !policy.allow_secret_values && looks_like_literal_secret(value) {
        return Err(DiscoveryError::PolicyViolation(
            "literal secret values are disabled by policy".to_string(),
        ));
    }

    Ok(())
}

pub(crate) fn validate_effective_config_read_policy(
    policy: &ConfigPolicy,
    effective: &EffectiveConfig,
) -> DiscoveryResult<()> {
    if policy.require_publish_for_reads && effective.values.is_empty() {
        return Err(DiscoveryError::NotFound(
            "published config not found for requested scope".to_string(),
        ));
    }

    Ok(())
}

pub(crate) fn validate_registry_lease_ttl(
    policy: &RegistryPolicy,
    lease_ttl_seconds: u64,
) -> DiscoveryResult<()> {
    if policy.min_lease_ttl_seconds > policy.default_lease_ttl_seconds
        || policy.default_lease_ttl_seconds > policy.max_lease_ttl_seconds
    {
        return Err(DiscoveryError::InvalidConfig(
            "lease ttl bounds must satisfy min <= default <= max".to_string(),
        ));
    }

    if lease_ttl_seconds < policy.min_lease_ttl_seconds
        || lease_ttl_seconds > policy.max_lease_ttl_seconds
    {
        return Err(DiscoveryError::PolicyViolation(format!(
            "lease ttl must be between {} and {} seconds",
            policy.min_lease_ttl_seconds, policy.max_lease_ttl_seconds
        )));
    }

    Ok(())
}

fn validate_config_format(format: &ConfigFormat, value: &str) -> DiscoveryResult<()> {
    match format {
        ConfigFormat::Text => Ok(()),
        ConfigFormat::Json => serde_json::from_str::<serde_json::Value>(value)
            .map(|_| ())
            .map_err(|error| {
                DiscoveryError::InvalidArgument(format!(
                    "config value must be valid JSON for JSON format: {error}"
                ))
            }),
        ConfigFormat::Toml => value.parse::<toml::Value>().map(|_| ()).map_err(|error| {
            DiscoveryError::InvalidArgument(format!(
                "config value must be valid TOML for TOML format: {error}"
            ))
        }),
    }
}

fn is_secret_reference(value: &str) -> bool {
    value.trim_start().starts_with("secret_ref:")
}

fn looks_like_literal_secret(value: &str) -> bool {
    let normalized = value.to_ascii_lowercase();
    normalized.contains("password")
        || normalized.contains("secret")
        || normalized.contains("access_token")
        || normalized.contains("auth_token")
        || normalized.contains("api_key")
}
