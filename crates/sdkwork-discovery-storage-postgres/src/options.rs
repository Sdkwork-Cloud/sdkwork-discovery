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
    password: Option<String>,
    tls_enabled: bool,
    connect_timeout_ms: u64,
    max_connections: u32,
}

impl PostgresConnectionOptions {
    pub fn from_transport(
        transport: &StorageTransportConfig,
        password: Option<&str>,
    ) -> DiscoveryResult<Self> {
        if let (StorageCredentialSource::None, Some(_)) = (&transport.credential_source, password) {
            return Err(DiscoveryError::InvalidConfig(
                "postgres password material requires a configured password_file".to_string(),
            ));
        }

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
            password: password.map(ToOwned::to_owned),
            tls_enabled: transport.tls_enabled,
            connect_timeout_ms: transport.connect_timeout_ms,
            max_connections: transport.max_connections,
        })
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

    pub fn database_url(&self) -> String {
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
            _ => format!("{}:{}", self.host, self.port),
        };
        let ssl_mode = if self.tls_enabled {
            "require"
        } else {
            "disable"
        };
        format!(
            "postgres://{authority}/{database}?sslmode={ssl_mode}",
            database = self.database
        )
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

    pub fn to_sqlx_connect_options(&self) -> PgConnectOptions {
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

        if let Some(password) = self.password.as_deref() {
            options = options.password(password);
        }

        if let Some(schema) = self.schema.as_deref() {
            options = options.options([("search_path", schema)]);
        }

        options
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
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("tls_enabled", &self.tls_enabled)
            .field("connect_timeout_ms", &self.connect_timeout_ms)
            .field("max_connections", &self.max_connections)
            .finish()
    }
}
