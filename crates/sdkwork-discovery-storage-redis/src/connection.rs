use sdkwork_discovery_config::{StorageCredentialSource, StorageTransportConfig};
use sdkwork_discovery_contract::{DiscoveryError, DiscoveryResult};

pub const DISCOVERY_REDIS_KEY_PREFIX: &str = "sdkwork:discovery:v1";

#[derive(Debug, Clone)]
pub struct RedisConnectionOptions {
    host: String,
    port: u16,
    database: u8,
    username: Option<String>,
    credential_source: StorageCredentialSource,
    tls_enabled: bool,
    connect_timeout_ms: u64,
    max_connections: u32,
}

impl RedisConnectionOptions {
    pub fn from_transport(transport: &StorageTransportConfig) -> DiscoveryResult<Self> {
        let database = transport
            .database
            .as_deref()
            .unwrap_or("0")
            .trim()
            .parse::<u8>()
            .map_err(|_| {
                DiscoveryError::InvalidConfig(
                    "redis storage database index must be a number between 0 and 255".to_string(),
                )
            })?;

        Ok(Self {
            host: transport.host.clone(),
            port: transport.port,
            database,
            username: transport.username.clone(),
            credential_source: transport.credential_source.clone(),
            tls_enabled: transport.tls_enabled,
            connect_timeout_ms: transport.connect_timeout_ms,
            max_connections: transport.max_connections.max(1),
        })
    }

    fn resolve_password(&self) -> DiscoveryResult<Option<String>> {
        match &self.credential_source {
            StorageCredentialSource::None => Ok(None),
            StorageCredentialSource::PasswordFile(path) => {
                let path = path.trim();
                if path.is_empty() {
                    return Err(DiscoveryError::InvalidConfig(
                        "storage password_file path must not be empty".to_string(),
                    ));
                }
                let mut password = std::fs::read(path).map_err(|error| {
                    DiscoveryError::InvalidConfig(format!(
                        "storage password_file could not be read: {error}"
                    ))
                })?;
                while password.last().is_some_and(u8::is_ascii_whitespace) {
                    password.pop();
                }
                if password.is_empty() {
                    return Err(DiscoveryError::InvalidConfig(
                        "storage password_file must not be empty".to_string(),
                    ));
                }
                Ok(Some(String::from_utf8(password).map_err(|error| {
                    DiscoveryError::InvalidConfig(format!(
                        "storage password_file must be valid UTF-8: {error}"
                    ))
                })?))
            }
        }
    }

    pub fn redis_url(&self) -> DiscoveryResult<String> {
        let password = self.resolve_password()?;
        let authority = match self.username.as_deref() {
            Some(username) if !username.is_empty() => {
                let password = password
                    .as_deref()
                    .filter(|value| !value.is_empty())
                    .map(|value| format!(":{value}"))
                    .unwrap_or_default();
                format!("{username}{password}@{}:{}", self.host, self.port)
            }
            _ => match password.as_deref() {
                Some(password) if !password.is_empty() => {
                    format!(":{}@{}:{}", password, self.host, self.port)
                }
                _ => format!("{}:{}", self.host, self.port),
            },
        };
        let scheme = if self.tls_enabled { "rediss" } else { "redis" };
        Ok(format!("{scheme}://{authority}/{db}", db = self.database))
    }

    pub fn safe_summary(&self) -> String {
        format!(
            "redis host={} port={} database={} username={} tls={} connect_timeout_ms={} max_connections={}",
            self.host,
            self.port,
            self.database,
            self.username.as_deref().unwrap_or("<none>"),
            self.tls_enabled,
            self.connect_timeout_ms,
            self.max_connections
        )
    }

    pub fn state_key(&self) -> String {
        format!("{DISCOVERY_REDIS_KEY_PREFIX}:durable-state")
    }
}
