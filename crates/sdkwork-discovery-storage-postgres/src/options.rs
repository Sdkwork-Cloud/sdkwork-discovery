use std::fmt::{Debug, Formatter};
use std::time::Duration;

use sdkwork_discovery_config::{StorageCredentialSource, StorageTransportConfig};
use sdkwork_discovery_contract::{DiscoveryError, DiscoveryResult};
use sqlx::postgres::{PgConnectOptions, PgSslMode};

#[derive(Clone)]
pub struct PostgresConnectionOptions {
    host: String,
    port: u16,
    database: String,
    schema: Option<String>,
    username: Option<String>,
    credential_source: StorageCredentialSource,
    tls_enabled: bool,
    connect_timeout_ms: u64,
    max_connections: u32,
}

impl PostgresConnectionOptions {
    pub fn from_transport(transport: &StorageTransportConfig) -> DiscoveryResult<Self> {
        let database = transport
            .database
            .as_deref()
            .ok_or_else(|| {
                DiscoveryError::InvalidConfig(
                    "postgres storage requires a database name".to_string(),
                )
            })?
            .trim();
        if database.is_empty() {
            return Err(DiscoveryError::InvalidConfig(
                "postgres storage database name must not be empty".to_string(),
            ));
        }

        Ok(Self {
            host: transport.host.clone(),
            port: transport.port,
            database: database.to_string(),
            schema: transport.schema.clone(),
            username: transport.username.clone(),
            credential_source: transport.credential_source.clone(),
            tls_enabled: transport.tls_enabled,
            connect_timeout_ms: transport.connect_timeout_ms,
            max_connections: transport.max_connections,
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

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn database(&self) -> &str {
        &self.database
    }

    pub fn schema(&self) -> Option<&str> {
        self.schema.as_deref()
    }

    pub fn username(&self) -> Option<&str> {
        self.username.as_deref()
    }

    pub fn connect_timeout_ms(&self) -> u64 {
        self.connect_timeout_ms
    }

    pub fn max_connections(&self) -> u32 {
        self.max_connections
    }

    pub fn tls_enabled(&self) -> bool {
        self.tls_enabled
    }

    pub fn database_url(&self) -> DiscoveryResult<String> {
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
            _ => format!("{}:{}", self.host, self.port),
        };
        let ssl_mode = if self.tls_enabled {
            "require"
        } else {
            "disable"
        };
        Ok(format!(
            "postgres://{authority}/{database}?sslmode={ssl_mode}",
            database = self.database
        ))
    }

    pub fn connection_uri(&self) -> String {
        let authority = match self.username.as_deref() {
            Some(username) if !username.is_empty() => {
                format!("{username}@{}:{}", self.host, self.port)
            }
            _ => format!("{}:{}", self.host, self.port),
        };
        let ssl_mode = if self.tls_enabled {
            "require"
        } else {
            "disable"
        };
        format!(
            "postgres://{authority}/{}?sslmode={ssl_mode}",
            self.database
        )
    }

    pub fn safe_summary(&self) -> String {
        format!(
            "postgres host={} port={} database={} schema={} username={} tls={} max_connections={}",
            self.host,
            self.port,
            self.database,
            self.schema.as_deref().unwrap_or("<none>"),
            self.username.as_deref().unwrap_or("<none>"),
            self.tls_enabled,
            self.max_connections
        )
    }

    pub fn to_sqlx_connect_options(&self) -> DiscoveryResult<PgConnectOptions> {
        let mut options = PgConnectOptions::new()
            .host(&self.host)
            .port(self.port)
            .database(&self.database)
            .ssl_mode(if self.tls_enabled {
                PgSslMode::Require
            } else {
                PgSslMode::Disable
            });

        if let Some(username) = self.username.as_deref() {
            options = options.username(username);
        }

        if let Some(password) = self.resolve_password()?.as_deref() {
            options = options.password(password);
        }

        if let Some(schema) = self.schema.as_deref() {
            options = options.options([("search_path", schema)]);
        }

        Ok(options)
    }

    pub fn acquire_timeout(&self) -> Duration {
        Duration::from_millis(self.connect_timeout_ms)
    }
}

impl Debug for PostgresConnectionOptions {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PostgresConnectionOptions")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("database", &self.database)
            .field("schema", &self.schema)
            .field("username", &self.username)
            .field("credential_source", &self.credential_source)
            .field("tls_enabled", &self.tls_enabled)
            .field("connect_timeout_ms", &self.connect_timeout_ms)
            .field("max_connections", &self.max_connections)
            .finish()
    }
}
