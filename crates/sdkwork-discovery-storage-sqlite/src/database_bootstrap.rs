use std::str::FromStr;
use std::time::Duration;

use sdkwork_database_config::{
    DatabaseConfig, DatabaseEngine, DeploymentMode, SqliteJournalMode, SqliteSynchronous,
};
use sdkwork_database_sqlx::{create_pool_from_config, DatabasePool, PoolError};
use sdkwork_discovery_contract::{DiscoveryError, DiscoveryResult};
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode as SqlxJournalMode, SqlitePoolOptions,
    SqliteSynchronous as SqlxSynchronous,
};
use sqlx::{Pool, Sqlite};

use crate::codec::sqlx_error;

pub const DISCOVERY_DATABASE_SERVICE_NAME: &str = "DISCOVERY";

pub fn sqlite_database_config(file: &str, max_connections: u32) -> DatabaseConfig {
    let max_connections = normalized_max_connections(file, max_connections);
    DatabaseConfig {
        engine: DatabaseEngine::Sqlite,
        url: sqlite_database_url(file),
        mode: DeploymentMode::Standalone,
        table_prefix: String::new(),
        max_connections,
        min_connections: 1,
        sqlite: sdkwork_database_config::SqliteConfig {
            create_if_missing: file != ":memory:",
            journal_mode: SqliteJournalMode::Wal,
            ..Default::default()
        },
        ..Default::default()
    }
}

pub async fn connect_sqlite_pool(config: DatabaseConfig) -> DiscoveryResult<DatabasePool> {
    create_pool_from_config(config)
        .await
        .map_err(map_pool_error)
}

pub fn lazy_sqlite_pool(config: &DatabaseConfig) -> DiscoveryResult<Pool<Sqlite>> {
    let sqlite_config = &config.sqlite;
    let connect_options = SqliteConnectOptions::from_str(&config.url)
        .map_err(|error| DiscoveryError::InvalidConfig(format!("sqlite url: {error}")))?
        .create_if_missing(sqlite_config.create_if_missing)
        .journal_mode(map_journal_mode(sqlite_config.journal_mode))
        .busy_timeout(Duration::from_secs(sqlite_config.busy_timeout_secs))
        .foreign_keys(sqlite_config.foreign_keys)
        .synchronous(map_synchronous(sqlite_config.synchronous));

    Ok(SqlitePoolOptions::new()
        .max_connections(config.max_connections)
        .connect_lazy_with(connect_options))
}

pub fn map_pool_error(error: PoolError) -> DiscoveryError {
    match error {
        PoolError::PoolCreation(err) | PoolError::Connection(err) => sqlx_error(err),
        other => DiscoveryError::InvalidConfig(other.to_string()),
    }
}

fn sqlite_database_url(file: &str) -> String {
    if file == ":memory:" {
        "sqlite::memory:".to_string()
    } else {
        format!("sqlite:{}?mode=rwc", file.replace('\\', "/"))
    }
}

pub fn normalized_max_connections(file: &str, max_connections: u32) -> u32 {
    if file == ":memory:" {
        1
    } else {
        max_connections.max(1)
    }
}

fn map_journal_mode(mode: SqliteJournalMode) -> SqlxJournalMode {
    match mode {
        SqliteJournalMode::Delete => SqlxJournalMode::Delete,
        SqliteJournalMode::Truncate => SqlxJournalMode::Truncate,
        SqliteJournalMode::Persist => SqlxJournalMode::Persist,
        SqliteJournalMode::Memory => SqlxJournalMode::Memory,
        SqliteJournalMode::Wal => SqlxJournalMode::Wal,
        SqliteJournalMode::Off => SqlxJournalMode::Off,
    }
}

fn map_synchronous(mode: SqliteSynchronous) -> SqlxSynchronous {
    match mode {
        SqliteSynchronous::Off => SqlxSynchronous::Off,
        SqliteSynchronous::Normal => SqlxSynchronous::Normal,
        SqliteSynchronous::Full => SqlxSynchronous::Full,
        SqliteSynchronous::Extra => SqlxSynchronous::Extra,
    }
}
