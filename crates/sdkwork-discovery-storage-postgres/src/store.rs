use sdkwork_database_config::DatabaseConfig;
use sdkwork_discovery_config::StorageTransportConfig;
use sdkwork_discovery_contract::DiscoveryResult;
use sqlx::{Pool, Postgres};
use std::sync::OnceLock;

use crate::database_bootstrap::{
    connect_postgres_pool, lazy_postgres_pool, postgres_database_config,
};
use crate::migration;
use crate::options::PostgresConnectionOptions;

#[derive(Debug)]
pub struct PostgresDiscoveryStore {
    connection_options: PostgresConnectionOptions,
    database_config: DatabaseConfig,
    pool: OnceLock<Pool<Postgres>>,
}

impl PostgresDiscoveryStore {
    pub fn new_lazy(
        transport: &StorageTransportConfig,
        password: Option<&str>,
    ) -> DiscoveryResult<Self> {
        let connection_options = PostgresConnectionOptions::from_transport(transport, password)?;
        let database_config = postgres_database_config(&connection_options);

        Ok(Self {
            connection_options,
            database_config,
            pool: OnceLock::new(),
        })
    }

    pub fn safe_summary(&self) -> String {
        self.connection_options.safe_summary()
    }

    pub fn connection_options(&self) -> &PostgresConnectionOptions {
        &self.connection_options
    }

    pub fn database_config(&self) -> &DatabaseConfig {
        &self.database_config
    }

    pub fn initial_schema_sql(&self) -> &'static str {
        migration::INITIAL_SCHEMA_SQL
    }

    pub async fn apply_initial_schema(&self) -> DiscoveryResult<()> {
        let database_pool = connect_postgres_pool(self.database_config.clone()).await?;
        crate::bootstrap::bootstrap_discovery_database(database_pool)
            .await
            .map_err(sdkwork_discovery_contract::DiscoveryError::InvalidConfig)?;
        Ok(())
    }

    pub(crate) fn pool(&self) -> &Pool<Postgres> {
        self.pool.get_or_init(|| {
            lazy_postgres_pool(&self.connection_options)
                .expect("postgres lazy pool config must be valid")
        })
    }

    pub async fn connect_eager(
        transport: &StorageTransportConfig,
        password: Option<&str>,
    ) -> DiscoveryResult<Self> {
        let connection_options = PostgresConnectionOptions::from_transport(transport, password)?;
        let database_config = postgres_database_config(&connection_options);
        let database_pool = connect_postgres_pool(database_config.clone()).await?;
        let pool = database_pool.as_postgres().ok_or_else(|| {
            sdkwork_discovery_contract::DiscoveryError::InvalidConfig(
                "sdkwork-database pool is not postgres-backed".to_string(),
            )
        })?;

        Ok(Self {
            connection_options,
            database_config,
            pool: OnceLock::from(pool.clone()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdkwork_discovery_config::StorageCredentialSource;

    #[test]
    fn lazy_pool_uses_sdkwork_database_config() {
        let transport = StorageTransportConfig {
            host: "127.0.0.1".to_string(),
            port: 5432,
            database: Some("discovery".to_string()),
            schema: None,
            username: Some("discovery".to_string()),
            credential_source: StorageCredentialSource::None,
            tls_enabled: false,
            connect_timeout_ms: 5_000,
            max_connections: 8,
        };
        let store = PostgresDiscoveryStore::new_lazy(&transport, None).unwrap();
        assert_eq!(
            store.database_config.engine,
            sdkwork_database_config::DatabaseEngine::Postgres
        );
        assert_eq!(store.database_config.max_connections, 8);
    }
}
