use std::path::Path;
use std::sync::OnceLock;

use sdkwork_database_config::DatabaseConfig;
use sdkwork_discovery_contract::{DiscoveryError, DiscoveryResult};
use sqlx::{Pool, Sqlite};

use crate::database_bootstrap::{
    connect_sqlite_pool, lazy_sqlite_pool, sqlite_database_config, DISCOVERY_DATABASE_SERVICE_NAME,
};
use crate::{codec::sqlx_error, migration};

#[derive(Debug)]
pub struct SqliteDiscoveryStore {
    database_config: DatabaseConfig,
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

        let database_config = sqlite_database_config(file, max_connections);
        Ok(Self {
            safe_summary: safe_summary(file, database_config.max_connections),
            database_config,
            pool: OnceLock::new(),
        })
    }

    pub async fn new_in_memory() -> DiscoveryResult<Self> {
        Self::from_database_config(sqlite_database_config(":memory:", 1)).await
    }

    pub async fn new_file(path: impl AsRef<Path>, max_connections: u32) -> DiscoveryResult<Self> {
        let path = path.as_ref();
        if path.as_os_str().is_empty() {
            return Err(DiscoveryError::InvalidConfig(
                "sqlite storage file must not be empty".to_string(),
            ));
        }

        let file = path.to_string_lossy();
        Self::from_database_config(sqlite_database_config(&file, max_connections)).await
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
            lazy_sqlite_pool(&self.database_config).expect("sqlite lazy pool config must be valid")
        })
    }

    async fn from_database_config(database_config: DatabaseConfig) -> DiscoveryResult<Self> {
        let file = database_config
            .url
            .strip_prefix("sqlite:")
            .unwrap_or(&database_config.url)
            .split('?')
            .next()
            .unwrap_or(":memory:");
        let safe_summary = safe_summary(
            if file == ":memory:" || database_config.url.contains("mode=memory") {
                ":memory:"
            } else {
                file
            },
            database_config.max_connections,
        );
        let database_pool = connect_sqlite_pool(database_config.clone()).await?;
        let pool = database_pool.as_sqlite().ok_or_else(|| {
            DiscoveryError::InvalidConfig("sdkwork-database pool is not sqlite-backed".to_string())
        })?;

        Ok(Self {
            database_config,
            pool: OnceLock::from(pool.clone()),
            safe_summary,
        })
    }
}

fn safe_summary(file: &str, max_connections: u32) -> String {
    format!(
        "sqlite file={file} max_connections={max_connections} service={DISCOVERY_DATABASE_SERVICE_NAME}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database_bootstrap::DISCOVERY_DATABASE_SERVICE_NAME;

    #[test]
    fn lazy_pool_uses_sdkwork_database_config() {
        assert_eq!(DISCOVERY_DATABASE_SERVICE_NAME, "DISCOVERY");
        let store = SqliteDiscoveryStore::new_lazy(":memory:", 4).unwrap();
        assert_eq!(
            store.database_config.engine,
            sdkwork_database_config::DatabaseEngine::Sqlite
        );
        assert_eq!(store.database_config.max_connections, 1);
        assert!(store.database_config.url.contains("memory"));
    }
}
