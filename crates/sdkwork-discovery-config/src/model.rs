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
    pub resilience: ResilienceConfig,
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

    pub fn is_primary_capable_for_provider(self, provider: StorageProvider) -> bool {
        match provider {
            StorageProvider::Memory | StorageProvider::Postgres | StorageProvider::Sqlite => {
                self == Self::Primary
            }
            StorageProvider::Redis | StorageProvider::Etcd | StorageProvider::Consul => true,
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
    #[serde(default)]
    pub health_check_scan_interval_ms: u64,
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
    #[serde(default = "default_event_gc_interval_ms")]
    pub event_gc_interval_ms: u64,
    #[serde(default = "default_event_gc_retention_count")]
    pub event_gc_retention_count: u64,
    #[serde(default = "default_event_gc_batch_size")]
    pub event_gc_batch_size: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
pub struct ResilienceConfig {
    #[serde(default)]
    pub circuit_breaker: ResilienceCircuitBreakerConfig,
    #[serde(default)]
    pub rate_limit: ResilienceRateLimitConfig,
    #[serde(default)]
    pub degradation: ResilienceDegradationConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ResilienceCircuitBreakerConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_circuit_breaker_failure_threshold")]
    pub failure_threshold: u32,
    #[serde(default = "default_circuit_breaker_recovery_timeout_ms")]
    pub recovery_timeout_ms: u64,
    #[serde(default = "default_circuit_breaker_half_open_max_requests")]
    pub half_open_max_requests: u32,
}

impl Default for ResilienceCircuitBreakerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            failure_threshold: default_circuit_breaker_failure_threshold(),
            recovery_timeout_ms: default_circuit_breaker_recovery_timeout_ms(),
            half_open_max_requests: default_circuit_breaker_half_open_max_requests(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ResilienceRateLimitConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_rate_limit_requests_per_second")]
    pub requests_per_second: u64,
    #[serde(default = "default_rate_limit_burst_capacity")]
    pub burst_capacity: u64,
}

impl Default for ResilienceRateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            requests_per_second: default_rate_limit_requests_per_second(),
            burst_capacity: default_rate_limit_burst_capacity(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ResilienceDegradationConfig {
    #[serde(default)]
    pub read_only_on_storage_failure: bool,
    #[serde(default = "default_degradation_stale_read_max_age_ms")]
    pub stale_read_max_age_ms: u64,
}

impl Default for ResilienceDegradationConfig {
    fn default() -> Self {
        Self {
            read_only_on_storage_failure: false,
            stale_read_max_age_ms: default_degradation_stale_read_max_age_ms(),
        }
    }
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
        self.validate_storage_roles()?;
        self.validate_resilience()?;

        if self.runtime.environment == RuntimeEnvironment::Production {
            if self.security.allow_unsigned_local_context {
                return Err(DiscoveryError::InvalidConfig(
                    "production security config must disable unsigned local context".to_string(),
                ));
            }

            require_file_reference(
                "service-token HMAC secret file",
                self.security.service_token.hmac_secret_file.as_deref(),
            )?;

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

    fn validate_resilience(&self) -> DiscoveryResult<()> {
        if self.resilience.circuit_breaker.enabled
            && self.resilience.circuit_breaker.failure_threshold == 0
        {
            return Err(DiscoveryError::InvalidConfig(
                "resilience circuit breaker failure threshold must be greater than zero when enabled"
                    .to_string(),
            ));
        }

        if self.resilience.circuit_breaker.enabled
            && self.resilience.circuit_breaker.recovery_timeout_ms == 0
        {
            return Err(DiscoveryError::InvalidConfig(
                "resilience circuit breaker recovery timeout must be greater than zero when enabled"
                    .to_string(),
            ));
        }

        if self.resilience.circuit_breaker.enabled
            && self.resilience.circuit_breaker.half_open_max_requests == 0
        {
            return Err(DiscoveryError::InvalidConfig(
                "resilience circuit breaker half-open max requests must be greater than zero when enabled"
                    .to_string(),
            ));
        }

        if self.resilience.rate_limit.enabled {
            if self.resilience.rate_limit.requests_per_second == 0 {
                return Err(DiscoveryError::InvalidConfig(
                    "resilience rate limit requests per second must be greater than zero when enabled"
                        .to_string(),
                ));
            }

            if self.resilience.rate_limit.burst_capacity == 0 {
                return Err(DiscoveryError::InvalidConfig(
                    "resilience rate limit burst capacity must be greater than zero when enabled"
                        .to_string(),
                ));
            }
        }

        if self.resilience.degradation.read_only_on_storage_failure
            && self.resilience.degradation.stale_read_max_age_ms == 0
        {
            return Err(DiscoveryError::InvalidConfig(
                "resilience degradation stale read max age must be greater than zero when read-only degradation is enabled"
                    .to_string(),
            ));
        }

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

    fn validate_storage_roles(&self) -> DiscoveryResult<()> {
        let provider = self.storage.provider.as_str();

        if !self
            .storage
            .registry_role
            .is_primary_capable_for_provider(self.storage.provider)
        {
            return Err(DiscoveryError::InvalidConfig(format!(
                "storage provider {provider} requires registry_role = primary"
            )));
        }

        if self.config_registry.enabled
            && !self
                .storage
                .config_role
                .is_primary_capable_for_provider(self.storage.provider)
        {
            return Err(DiscoveryError::InvalidConfig(format!(
                "enabled config registry with storage provider {provider} requires config_role = primary"
            )));
        }

        if self.watch.enabled
            && !self
                .storage
                .watch_role
                .is_primary_capable_for_provider(self.storage.provider)
        {
            return Err(DiscoveryError::InvalidConfig(format!(
                "enabled watch with storage provider {provider} requires watch_role = primary"
            )));
        }

        Ok(())
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

fn default_event_gc_interval_ms() -> u64 {
    60_000
}

fn default_event_gc_retention_count() -> u64 {
    10_000
}

fn default_event_gc_batch_size() -> usize {
    1_000
}

fn default_circuit_breaker_failure_threshold() -> u32 {
    5
}

fn default_circuit_breaker_recovery_timeout_ms() -> u64 {
    30_000
}

fn default_circuit_breaker_half_open_max_requests() -> u32 {
    3
}

fn default_rate_limit_requests_per_second() -> u64 {
    1_000
}

fn default_rate_limit_burst_capacity() -> u64 {
    1_000
}

fn default_degradation_stale_read_max_age_ms() -> u64 {
    60_000
}
