use std::path::Path;
use std::sync::OnceLock;

use sdkwork_discovery_contract::{DiscoveryError, DiscoveryResult};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Pool, Sqlite};

use crate::{codec::sqlx_error, migration};

#[derive(Debug)]
pub struct SqliteDiscoveryStore {
    options: SqliteConnectOptions,
    max_connections: u32,
    pool: OnceLock<Pool<Sqlite>>,
    safe_summary: String,
}

impl SqliteDiscoveryStore {
    pub fn new_lazy(file: &str, max_connections: u32) -> DiscoveryResult<Self> {
        if file.trim().is_empty() {
            return Err(DiscoveryError::InvalidConfig(
                "sqlite storage file must not be empty".to_string(),
            ));
        }

        let max_connections = normalized_max_connections(file, max_connections);
        let options = sqlite_options(file);
        Ok(Self {
            options,
            max_connections,
            pool: OnceLock::new(),
            safe_summary: safe_summary(file, max_connections),
        })
    }

    pub async fn new_in_memory() -> DiscoveryResult<Self> {
        let options = sqlite_options(":memory:");
        Self::from_options(options, "sqlite file=:memory: max_connections=1", 1).await
    }

    pub async fn new_file(path: impl AsRef<Path>, max_connections: u32) -> DiscoveryResult<Self> {
        let path = path.as_ref();
        if path.as_os_str().is_empty() {
            return Err(DiscoveryError::InvalidConfig(
                "sqlite storage file must not be empty".to_string(),
            ));
        }

        let file = path.to_string_lossy();
        let max_connections = normalized_max_connections(&file, max_connections);
        let options = sqlite_options(&file);
        let safe_summary = safe_summary(&file, max_connections);
        Self::from_options(options, &safe_summary, max_connections).await
    }

    pub fn safe_summary(&self) -> &str {
        &self.safe_summary
    }

    pub fn initial_schema_sql(&self) -> &'static str {
        migration::INITIAL_SCHEMA_SQL
    }

    pub async fn apply_initial_schema(&self) -> DiscoveryResult<()> {
        sqlx::raw_sql(migration::INITIAL_SCHEMA_SQL)
            .execute(self.pool())
            .await
            .map_err(sqlx_error)?;
        Ok(())
    }

    pub(crate) fn pool(&self) -> &Pool<Sqlite> {
        self.pool.get_or_init(|| {
            SqlitePoolOptions::new()
                .max_connections(self.max_connections)
                .connect_lazy_with(self.options.clone())
        })
    }

    async fn from_options(
        options: SqliteConnectOptions,
        safe_summary: &str,
        max_connections: u32,
    ) -> DiscoveryResult<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(max_connections)
            .connect_with(options.clone())
            .await
            .map_err(sqlx_error)?;

        Ok(Self {
            options,
            max_connections,
            pool: OnceLock::from(pool),
            safe_summary: safe_summary.to_string(),
        })
    }
}

fn sqlite_options(file: &str) -> SqliteConnectOptions {
    SqliteConnectOptions::new()
        .filename(file)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
}

fn normalized_max_connections(file: &str, max_connections: u32) -> u32 {
    if file == ":memory:" {
        1
    } else {
        max_connections.max(1)
    }
}

fn safe_summary(file: &str, max_connections: u32) -> String {
    format!("sqlite file={file} max_connections={max_connections}")
}
