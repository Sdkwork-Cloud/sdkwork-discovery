use sdkwork_discovery_config::{StorageCredentialSource, StorageTransportConfig};
use sdkwork_discovery_contract::{DiscoveryError, DiscoveryResult};

pub const DISCOVERY_REDIS_KEY_PREFIX: &str = "sdkwork:discovery:v1";

#[derive(Debug, Clone)]
pub struct RedisConnectionOptions {
    host: String,
    port: u16,
    database: u8,
    username: Option<String>,
    password: Option<String>,
    tls_enabled: bool,
    connect_timeout_ms: u64,
    max_connections: u32,
}

impl RedisConnectionOptions {
    pub fn from_transport(
        transport: &StorageTransportConfig,
        password: Option<&str>,
    ) -> DiscoveryResult<Self> {
        if let (StorageCredentialSource::None, Some(_)) = (&transport.credential_source, password) {
            return Err(DiscoveryError::InvalidConfig(
                "redis password material requires a configured password_file".to_string(),
            ));
        }

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
            password: password.map(ToOwned::to_owned),
            tls_enabled: transport.tls_enabled,
            connect_timeout_ms: transport.connect_timeout_ms,
            max_connections: transport.max_connections.max(1),
        })
    }

    pub fn redis_url(&self) -> String {
        let authority = match self.username.as_deref() {
            Some(username) if !username.is_empty() => {
                let password = self
                    .password
                    .as_deref()
                    .filter(|value| !value.is_empty())
                    .map(|value| format!(":{value}"))
                    .unwrap_or_default();
                format!("{username}{password}@{}:{}", self.host, self.port)
            }
            _ => match self.password.as_deref() {
                Some(password) if !password.is_empty() => {
                    format!(":{}@{}:{}", password, self.host, self.port)
                }
                _ => format!("{}:{}", self.host, self.port),
            },
        };
        let scheme = if self.tls_enabled { "rediss" } else { "redis" };
        format!("{scheme}://{authority}/{db}", db = self.database)
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
