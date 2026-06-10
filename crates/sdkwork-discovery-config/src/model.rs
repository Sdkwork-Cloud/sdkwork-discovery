use sdkwork_discovery_contract::{
    DiscoveryError, DiscoveryResult, RuntimeDeploymentMode, RuntimeEnvironment, RuntimeTarget,
};
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryRuntimeConfig {
    pub runtime: RuntimeConfig,
    pub server: ServerConfig,
    pub security: SecurityConfig,
    pub storage: StorageConfig,
    pub registry: RegistryConfig,
    pub config_registry: ConfigRegistryConfig,
    pub watch: WatchConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub environment: RuntimeEnvironment,
    pub config_profile: Option<String>,
    pub deployment_mode: RuntimeDeploymentMode,
    pub runtime_target: RuntimeTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ServerConfig {
    pub grpc_bind_host: String,
    pub grpc_port: u16,
    pub admin_grpc_port: u16,
    pub enable_health: bool,
    pub enable_reflection: bool,
    pub default_deadline_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityAuthMode {
    ServiceToken,
}

impl SecurityAuthMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "service-token" => Some(Self::ServiceToken),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ServiceToken => "service-token",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityConfig {
    pub auth_mode: SecurityAuthMode,
    pub tls_enabled: bool,
    pub mtls_enabled: bool,
    pub allow_unsigned_local_context: bool,
    pub service_token: ServiceTokenConfig,
    pub server_tls_cert_file: Option<String>,
    pub server_tls_key_file: Option<String>,
    pub client_ca_cert_file: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceTokenConfig {
    pub hmac_secret_file: Option<String>,
    pub issuer: String,
    pub audience: String,
    pub max_token_ttl_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageConfig {
    pub provider: StorageProvider,
    pub registry_role: StorageRole,
    pub config_role: StorageRole,
    pub watch_role: StorageRole,
    pub apply_initial_schema: bool,
    pub postgres: Option<StorageTransportConfig>,
    pub sqlite: Option<StorageFileConfig>,
    pub redis: Option<StorageTransportConfig>,
    pub etcd: Option<StorageTransportConfig>,
    pub consul: Option<StorageTransportConfig>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageProvider {
    Memory,
    Postgres,
    Sqlite,
    Redis,
    Etcd,
    Consul,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageRole {
    Primary,
    Cache,
    Watch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageTransportConfig {
    pub host: String,
    pub port: u16,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub username: Option<String>,
    pub credential_source: StorageCredentialSource,
    pub tls_enabled: bool,
    pub connect_timeout_ms: u64,
    pub max_connections: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageFileConfig {
    pub file: String,
    pub max_connections: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageCredentialSource {
    None,
    PasswordFile(String),
}

impl StorageProvider {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "memory" => Some(Self::Memory),
            "postgres" => Some(Self::Postgres),
            "sqlite" => Some(Self::Sqlite),
            "redis" => Some(Self::Redis),
            "etcd" => Some(Self::Etcd),
            "consul" => Some(Self::Consul),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Postgres => "postgres",
            Self::Sqlite => "sqlite",
            Self::Redis => "redis",
            Self::Etcd => "etcd",
            Self::Consul => "consul",
        }
    }
}

impl StorageRole {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "primary" => Some(Self::Primary),
            "cache" => Some(Self::Cache),
            "watch" => Some(Self::Watch),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RegistryConfig {
    pub default_lease_ttl_seconds: u64,
    pub min_lease_ttl_seconds: u64,
    pub max_lease_ttl_seconds: u64,
    pub expiry_scan_interval_ms: u64,
    pub expiry_scan_batch_size: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ConfigRegistryConfig {
    pub enabled: bool,
    pub max_config_body_bytes: usize,
    pub require_publish_for_reads: bool,
    pub allow_secret_values: bool,
    pub allow_secret_refs: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct WatchConfig {
    pub enabled: bool,
    pub max_streams: u32,
    pub event_buffer_size: usize,
    pub heartbeat_interval_ms: u64,
    pub durable_poll_interval_ms: u64,
    pub durable_replay_batch_size: usize,
}

impl DiscoveryRuntimeConfig {
    pub(crate) fn validate(&self) -> DiscoveryResult<()> {
        if self.registry.min_lease_ttl_seconds > self.registry.default_lease_ttl_seconds
            || self.registry.default_lease_ttl_seconds > self.registry.max_lease_ttl_seconds
        {
            return Err(DiscoveryError::InvalidConfig(
                "lease ttl bounds must satisfy min <= default <= max".to_string(),
            ));
        }

        if self.registry.expiry_scan_interval_ms == 0 {
            return Err(DiscoveryError::InvalidConfig(
                "lease expiry scan interval must be greater than zero".to_string(),
            ));
        }

        if self.registry.expiry_scan_batch_size == 0 {
            return Err(DiscoveryError::InvalidConfig(
                "lease expiry scan batch size must be greater than zero".to_string(),
            ));
        }

        if self.watch.enabled {
            if self.watch.max_streams == 0 {
                return Err(DiscoveryError::InvalidConfig(
                    "watch max streams must be greater than zero".to_string(),
                ));
            }

            if self.watch.event_buffer_size == 0 {
                return Err(DiscoveryError::InvalidConfig(
                    "watch event buffer size must be greater than zero".to_string(),
                ));
            }

            if self.watch.heartbeat_interval_ms == 0 {
                return Err(DiscoveryError::InvalidConfig(
                    "watch heartbeat interval must be greater than zero".to_string(),
                ));
            }

            if self.watch.durable_poll_interval_ms == 0 {
                return Err(DiscoveryError::InvalidConfig(
                    "watch durable poll interval must be greater than zero".to_string(),
                ));
            }

            if self.watch.durable_replay_batch_size == 0 {
                return Err(DiscoveryError::InvalidConfig(
                    "watch durable replay batch size must be greater than zero".to_string(),
                ));
            }
        }

        self.validate_security()?;
        self.validate_storage()?;

        if self.runtime.environment == RuntimeEnvironment::Production {
            if self.storage.apply_initial_schema {
                return Err(DiscoveryError::InvalidConfig(
                    "production storage must not apply initial schema automatically".to_string(),
                ));
            }

            if self.config_registry.allow_secret_values {
                return Err(DiscoveryError::InvalidConfig(
                    "production config registry must not allow literal secret values".to_string(),
                ));
            }

            if self.server.grpc_bind_host == "127.0.0.1"
                || self.server.grpc_bind_host == "localhost"
            {
                return Err(DiscoveryError::InvalidConfig(
                    "production server config must not bind only to localhost".to_string(),
                ));
            }

            if !self.security.tls_enabled && !self.security.mtls_enabled {
                return Err(DiscoveryError::InvalidConfig(
                    "production server config must enable TLS or mTLS".to_string(),
                ));
            }

            if self.server.enable_reflection {
                return Err(DiscoveryError::InvalidConfig(
                    "production gRPC reflection must be disabled or access-controlled".to_string(),
                ));
            }

            if self.storage.provider == StorageProvider::Memory {
                return Err(DiscoveryError::InvalidConfig(
                    "production storage provider must not be memory".to_string(),
                ));
            }

            if self.storage.provider == StorageProvider::Sqlite {
                return Err(DiscoveryError::InvalidConfig(
                    "production storage provider must not be sqlite".to_string(),
                ));
            }
        }

        self.validate_unsigned_local_context()?;

        Ok(())
    }

    fn validate_security(&self) -> DiscoveryResult<()> {
        let tls_required = self.security.tls_enabled || self.security.mtls_enabled;

        validate_optional_file_reference(
            "TLS server certificate file",
            self.security.server_tls_cert_file.as_deref(),
        )?;
        validate_optional_file_reference(
            "TLS server private key file",
            self.security.server_tls_key_file.as_deref(),
        )?;
        validate_optional_file_reference(
            "mTLS client CA certificate file",
            self.security.client_ca_cert_file.as_deref(),
        )?;
        validate_optional_file_reference(
            "service-token HMAC secret file",
            self.security.service_token.hmac_secret_file.as_deref(),
        )?;

        if self.security.service_token.issuer.trim().is_empty() {
            return Err(DiscoveryError::InvalidConfig(
                "service-token issuer must not be empty".to_string(),
            ));
        }

        if self.security.service_token.audience.trim().is_empty() {
            return Err(DiscoveryError::InvalidConfig(
                "service-token audience must not be empty".to_string(),
            ));
        }

        if self.security.service_token.max_token_ttl_seconds == 0 {
            return Err(DiscoveryError::InvalidConfig(
                "service-token max ttl must be greater than zero".to_string(),
            ));
        }

        if !self.security.allow_unsigned_local_context {
            require_file_reference(
                "service-token HMAC secret file",
                self.security.service_token.hmac_secret_file.as_deref(),
            )?;
        }

        if tls_required {
            require_file_reference(
                "TLS server certificate file",
                self.security.server_tls_cert_file.as_deref(),
            )?;
            require_file_reference(
                "TLS server private key file",
                self.security.server_tls_key_file.as_deref(),
            )?;
        }

        if self.security.mtls_enabled {
            require_file_reference(
                "mTLS client CA certificate file",
                self.security.client_ca_cert_file.as_deref(),
            )?;
        }

        Ok(())
    }

    fn validate_unsigned_local_context(&self) -> DiscoveryResult<()> {
        if !self.security.allow_unsigned_local_context {
            return Ok(());
        }

        if !matches!(
            self.runtime.environment,
            RuntimeEnvironment::Development | RuntimeEnvironment::Test
        ) {
            return Err(DiscoveryError::InvalidConfig(
                "unsigned local context is allowed only for development or test runtimes"
                    .to_string(),
            ));
        }

        if self.runtime.deployment_mode == RuntimeDeploymentMode::Container
            || self.runtime.runtime_target == RuntimeTarget::Container
        {
            return Err(DiscoveryError::InvalidConfig(
                "unsigned local context is not allowed for container runtimes".to_string(),
            ));
        }

        if !is_loopback_bind_host(&self.server.grpc_bind_host) {
            return Err(DiscoveryError::InvalidConfig(
                "unsigned local context requires a loopback gRPC bind host".to_string(),
            ));
        }

        Ok(())
    }

    fn validate_storage(&self) -> DiscoveryResult<()> {
        match self.storage.provider {
            StorageProvider::Memory => Ok(()),
            StorageProvider::Postgres => {
                validate_transport("postgres", &self.storage.postgres, true)
            }
            StorageProvider::Sqlite => validate_file_storage("sqlite", &self.storage.sqlite),
            StorageProvider::Redis => validate_transport("redis", &self.storage.redis, false),
            StorageProvider::Etcd => validate_transport("etcd", &self.storage.etcd, false),
            StorageProvider::Consul => validate_transport("consul", &self.storage.consul, false),
        }
    }
}

fn is_loopback_bind_host(value: &str) -> bool {
    matches!(value.trim(), "127.0.0.1" | "localhost" | "::1" | "[::1]")
}

fn require_file_reference(label: &str, value: Option<&str>) -> DiscoveryResult<()> {
    if value.is_none() {
        return Err(DiscoveryError::InvalidConfig(format!(
            "{label} is required when TLS or mTLS is enabled"
        )));
    }
    Ok(())
}

fn validate_optional_file_reference(label: &str, value: Option<&str>) -> DiscoveryResult<()> {
    let Some(value) = value else {
        return Ok(());
    };

    if value.trim().is_empty() {
        return Err(DiscoveryError::InvalidConfig(format!(
            "{label} must not be empty"
        )));
    }

    if value.contains("-----BEGIN ") || value.contains('\n') || value.contains('\r') {
        return Err(DiscoveryError::InvalidConfig(format!(
            "{label} must be a file reference and must not contain inline PEM material"
        )));
    }

    Ok(())
}

fn validate_file_storage(name: &str, config: &Option<StorageFileConfig>) -> DiscoveryResult<()> {
    let config = config.as_ref().ok_or_else(|| {
        DiscoveryError::InvalidConfig(format!("storage provider {name} requires [storage.{name}]"))
    })?;

    if config.file.trim().is_empty() {
        return Err(DiscoveryError::InvalidConfig(format!(
            "storage provider {name} file must not be empty"
        )));
    }

    if config.max_connections == 0 {
        return Err(DiscoveryError::InvalidConfig(format!(
            "storage provider {name} max connections must be greater than zero"
        )));
    }

    Ok(())
}

fn validate_transport(
    name: &str,
    transport: &Option<StorageTransportConfig>,
    supports_schema: bool,
) -> DiscoveryResult<()> {
    let transport = transport.as_ref().ok_or_else(|| {
        DiscoveryError::InvalidConfig(format!("storage provider {name} requires [storage.{name}]"))
    })?;

    if transport.host.trim().is_empty() {
        return Err(DiscoveryError::InvalidConfig(format!(
            "storage provider {name} host must not be empty"
        )));
    }

    if transport.port == 0 {
        return Err(DiscoveryError::InvalidConfig(format!(
            "storage provider {name} port must be greater than zero"
        )));
    }

    if transport.connect_timeout_ms == 0 {
        return Err(DiscoveryError::InvalidConfig(format!(
            "storage provider {name} connect timeout must be greater than zero"
        )));
    }

    if transport.max_connections == 0 {
        return Err(DiscoveryError::InvalidConfig(format!(
            "storage provider {name} max connections must be greater than zero"
        )));
    }

    if let Some(schema) = transport.schema.as_deref() {
        if !supports_schema {
            return Err(DiscoveryError::InvalidConfig(format!(
                "storage provider {name} does not support schema; schema is postgres-only"
            )));
        }

        if schema.trim().is_empty() {
            return Err(DiscoveryError::InvalidConfig(format!(
                "storage provider {name} schema must not be empty"
            )));
        }
    }

    match &transport.credential_source {
        StorageCredentialSource::PasswordFile(path) if path.trim().is_empty() => {
            Err(DiscoveryError::InvalidConfig(format!(
                "storage provider {name} password_file must not be empty"
            )))
        }
        _ => Ok(()),
    }
}
