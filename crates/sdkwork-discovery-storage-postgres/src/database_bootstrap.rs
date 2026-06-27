use std::str::FromStr;
use std::time::Duration;

use sdkwork_database_config::{
    DatabaseConfig, DatabaseEngine, DeploymentMode, PgSslMode as ConfigPgSslMode,
};
use sdkwork_database_sqlx::{create_pool_from_config, DatabasePool, PoolError};
use sdkwork_discovery_contract::{DiscoveryError, DiscoveryResult};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgSslMode};
use sqlx::{Pool, Postgres};

use crate::codec::sqlx_error;
use crate::options::PostgresConnectionOptions;

pub const DISCOVERY_DATABASE_SERVICE_NAME: &str = "DISCOVERY";

pub fn postgres_database_config(
    options: &PostgresConnectionOptions,
) -> DiscoveryResult<DatabaseConfig> {
    let ssl_mode = if options.tls_enabled() {
        ConfigPgSslMode::Require
    } else {
        ConfigPgSslMode::Disable
    };
    let postgres = sdkwork_database_config::PostgresConfig {
        ssl_mode,
        application_name: Some(format!("{DISCOVERY_DATABASE_SERVICE_NAME}-discovery")),
        ..Default::default()
    };

    Ok(DatabaseConfig {
        engine: DatabaseEngine::Postgres,
        url: options.database_url()?,
        mode: DeploymentMode::Standalone,
        table_prefix: String::new(),
        max_connections: options.max_connections(),
        min_connections: 1,
        acquire_timeout_secs: options.connect_timeout_ms().div_ceil(1000).max(1),
        postgres,
        ..Default::default()
    })
}

pub async fn connect_postgres_pool(config: DatabaseConfig) -> DiscoveryResult<DatabasePool> {
    create_pool_from_config(config)
        .await
        .map_err(map_pool_error)
}

pub fn lazy_postgres_pool(options: &PostgresConnectionOptions) -> DiscoveryResult<Pool<Postgres>> {
    let config = postgres_database_config(options)?;
    let mut connect_options = PgConnectOptions::from_str(&config.url)
        .map_err(|error| DiscoveryError::InvalidConfig(format!("postgres url: {error}")))?
        .ssl_mode(if options.tls_enabled() {
            PgSslMode::Require
        } else {
            PgSslMode::Disable
        });

    if let Some(schema) = options.schema() {
        connect_options = connect_options.options([("search_path", schema)]);
    }

    Ok(PgPoolOptions::new()
        .max_connections(config.max_connections)
        .acquire_timeout(Duration::from_millis(options.connect_timeout_ms()))
        .connect_lazy_with(connect_options))
}

pub fn map_pool_error(error: PoolError) -> DiscoveryError {
    match error {
        PoolError::PoolCreation(err) | PoolError::Connection(err) => sqlx_error(err),
        other => DiscoveryError::InvalidConfig(other.to_string()),
    }
}
