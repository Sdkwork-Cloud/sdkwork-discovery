use std::collections::BTreeMap;

use sdkwork_discovery_contract::{
    DiscoveryError, DiscoveryResult, RuntimeDeploymentMode, RuntimeEnvironment, RuntimeTarget,
};
use serde::Deserialize;

use crate::env_overlay::{apply_env_overlay, validate_env_keys};
use crate::model::{
    ConfigRegistryConfig, DiscoveryRuntimeConfig, RegistryConfig, RuntimeConfig, SecurityAuthMode,
    SecurityConfig, ServerConfig, ServiceTokenConfig, StorageConfig, StorageCredentialSource,
    StorageFileConfig, StorageProvider, StorageRole, StorageTransportConfig, WatchConfig,
};

impl DiscoveryRuntimeConfig {
    pub fn from_toml_str(input: &str) -> DiscoveryResult<Self> {
        let config = Self::from_toml_str_unvalidated(input)?;
        config.validate()?;
        Ok(config)
    }

    pub fn from_toml_str_with_env(
        input: &str,
        env: &BTreeMap<String, String>,
    ) -> DiscoveryResult<Self> {
        validate_env_keys(env)?;
        let mut config = Self::from_toml_str_unvalidated(input)?;
        apply_env_overlay(&mut config, env)?;
        config.validate()?;
        Ok(config)
    }

    fn from_toml_str_unvalidated(input: &str) -> DiscoveryResult<Self> {
        let raw: RawDiscoveryRuntimeConfig = toml::from_str(input)
            .map_err(|error| DiscoveryError::InvalidConfig(error.to_string()))?;
        raw.normalize()
    }
}

#[derive(Debug, Deserialize)]
struct RawDiscoveryRuntimeConfig {
    runtime: RawRuntimeConfig,
    server: ServerConfig,
    security: RawSecurityConfig,
    storage: RawStorageConfig,
    registry: RegistryConfig,
    config_registry: ConfigRegistryConfig,
    watch: WatchConfig,
}

#[derive(Debug, Deserialize)]
struct RawRuntimeConfig {
    environment: Option<String>,
    config_profile: Option<String>,
    deployment_mode: String,
    runtime_target: String,
}

#[derive(Debug, Deserialize)]
struct RawSecurityConfig {
    auth_mode: String,
    tls_enabled: bool,
    mtls_enabled: bool,
    allow_unsigned_local_context: bool,
    service_token: Option<RawServiceTokenConfig>,
    server_tls_cert_file: Option<String>,
    server_tls_key_file: Option<String>,
    client_ca_cert_file: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawServiceTokenConfig {
    hmac_secret_file: Option<String>,
    issuer: Option<String>,
    audience: Option<String>,
    max_token_ttl_seconds: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct RawStorageConfig {
    provider: String,
    registry_role: Option<String>,
    config_role: Option<String>,
    watch_role: Option<String>,
    apply_initial_schema: Option<bool>,
    postgres: Option<RawStorageTransportConfig>,
    sqlite: Option<RawStorageFileConfig>,
    redis: Option<RawStorageTransportConfig>,
    etcd: Option<RawStorageTransportConfig>,
    consul: Option<RawStorageTransportConfig>,
}

#[derive(Debug, Deserialize)]
struct RawStorageTransportConfig {
    host: String,
    port: u16,
    database: Option<String>,
    schema: Option<String>,
    username: Option<String>,
    password_file: Option<String>,
    password: Option<String>,
    tls_enabled: bool,
    connect_timeout_ms: u64,
    max_connections: u32,
}

#[derive(Debug, Deserialize)]
struct RawStorageFileConfig {
    file: String,
    max_connections: u32,
}

impl RawDiscoveryRuntimeConfig {
    fn normalize(self) -> DiscoveryResult<DiscoveryRuntimeConfig> {
        let profile_environment = match self.runtime.config_profile.as_deref() {
            Some(profile) => Some(RuntimeEnvironment::from_profile(profile).ok_or_else(|| {
                DiscoveryError::InvalidConfig(format!("unknown config profile: {profile}"))
            })?),
            None => None,
        };

        let explicit_environment = match self.runtime.environment.as_deref() {
            Some(environment) => Some(RuntimeEnvironment::parse(environment).ok_or_else(|| {
                DiscoveryError::InvalidConfig(format!("unknown environment: {environment}"))
            })?),
            None => None,
        };

        let environment = match (explicit_environment, profile_environment) {
            (Some(explicit), Some(from_profile)) if explicit != from_profile => {
                return Err(DiscoveryError::InvalidConfig(
                    "environment and config profile describe different lifecycle stages"
                        .to_string(),
                ));
            }
            (Some(explicit), _) => explicit,
            (None, Some(from_profile)) => from_profile,
            (None, None) => {
                return Err(DiscoveryError::InvalidConfig(
                    "runtime environment or config profile is required".to_string(),
                ));
            }
        };

        let deployment_mode = RuntimeDeploymentMode::parse(&self.runtime.deployment_mode)
            .ok_or_else(|| {
                DiscoveryError::InvalidConfig(format!(
                    "unknown deployment mode: {}",
                    self.runtime.deployment_mode
                ))
            })?;
        let runtime_target =
            RuntimeTarget::parse(&self.runtime.runtime_target).ok_or_else(|| {
                DiscoveryError::InvalidConfig(format!(
                    "unknown runtime target: {}",
                    self.runtime.runtime_target
                ))
            })?;

        Ok(DiscoveryRuntimeConfig {
            runtime: RuntimeConfig {
                environment,
                config_profile: self.runtime.config_profile,
                deployment_mode,
                runtime_target,
            },
            server: self.server,
            security: self.security.normalize()?,
            storage: self.storage.normalize()?,
            registry: self.registry,
            config_registry: self.config_registry,
            watch: self.watch,
        })
    }
}

impl RawSecurityConfig {
    fn normalize(self) -> DiscoveryResult<SecurityConfig> {
        Ok(SecurityConfig {
            auth_mode: SecurityAuthMode::parse(&self.auth_mode).ok_or_else(|| {
                DiscoveryError::InvalidConfig(format!(
                    "unsupported RPC auth_mode {}; expected service-token",
                    self.auth_mode
                ))
            })?,
            tls_enabled: self.tls_enabled,
            mtls_enabled: self.mtls_enabled,
            allow_unsigned_local_context: self.allow_unsigned_local_context,
            service_token: normalize_service_token(self.service_token),
            server_tls_cert_file: self.server_tls_cert_file,
            server_tls_key_file: self.server_tls_key_file,
            client_ca_cert_file: self.client_ca_cert_file,
        })
    }
}

fn normalize_service_token(raw: Option<RawServiceTokenConfig>) -> ServiceTokenConfig {
    let raw = raw.unwrap_or(RawServiceTokenConfig {
        hmac_secret_file: None,
        issuer: None,
        audience: None,
        max_token_ttl_seconds: None,
    });

    ServiceTokenConfig {
        hmac_secret_file: raw.hmac_secret_file,
        issuer: raw
            .issuer
            .unwrap_or_else(|| "sdkwork-discovery".to_string()),
        audience: raw
            .audience
            .unwrap_or_else(|| "sdkwork-discovery-rpc".to_string()),
        max_token_ttl_seconds: raw.max_token_ttl_seconds.unwrap_or(3_600),
    }
}

impl RawStorageConfig {
    fn normalize(self) -> DiscoveryResult<StorageConfig> {
        Ok(StorageConfig {
            provider: parse_storage_provider(&self.provider)?,
            registry_role: parse_storage_role(self.registry_role.as_deref(), "registry_role")?,
            config_role: parse_storage_role(self.config_role.as_deref(), "config_role")?,
            watch_role: parse_storage_role(self.watch_role.as_deref(), "watch_role")?,
            apply_initial_schema: self.apply_initial_schema.unwrap_or(false),
            postgres: normalize_transport("postgres", self.postgres)?,
            sqlite: self.sqlite.map(normalize_file_storage).transpose()?,
            redis: normalize_transport("redis", self.redis)?,
            etcd: normalize_transport("etcd", self.etcd)?,
            consul: normalize_transport("consul", self.consul)?,
        })
    }
}

fn parse_storage_provider(value: &str) -> DiscoveryResult<StorageProvider> {
    StorageProvider::parse(value)
        .ok_or_else(|| DiscoveryError::InvalidConfig(format!("unknown storage provider: {value}")))
}

fn parse_storage_role(value: Option<&str>, field: &str) -> DiscoveryResult<StorageRole> {
    let value = value.unwrap_or("primary");
    StorageRole::parse(value)
        .ok_or_else(|| DiscoveryError::InvalidConfig(format!("unknown storage {field}: {value}")))
}

fn normalize_file_storage(raw: RawStorageFileConfig) -> DiscoveryResult<StorageFileConfig> {
    Ok(StorageFileConfig {
        file: raw.file,
        max_connections: raw.max_connections,
    })
}

fn normalize_transport(
    name: &str,
    raw: Option<RawStorageTransportConfig>,
) -> DiscoveryResult<Option<StorageTransportConfig>> {
    let Some(raw) = raw else {
        return Ok(None);
    };

    if raw.password.is_some() {
        return Err(DiscoveryError::InvalidConfig(format!(
            "storage provider {name} must use password_file instead of direct password values"
        )));
    }

    Ok(Some(StorageTransportConfig {
        host: raw.host,
        port: raw.port,
        database: raw.database,
        schema: raw.schema,
        username: raw.username,
        credential_source: raw
            .password_file
            .map(StorageCredentialSource::PasswordFile)
            .unwrap_or(StorageCredentialSource::None),
        tls_enabled: raw.tls_enabled,
        connect_timeout_ms: raw.connect_timeout_ms,
        max_connections: raw.max_connections,
    }))
}
