use sdkwork_discovery_config::StorageTransportConfig;
use sdkwork_discovery_contract::DiscoveryResult;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};
use std::sync::OnceLock;

use crate::options::PostgresConnectionOptions;
use crate::{codec::sqlx_error, migration};

#[derive(Debug)]
pub struct PostgresDiscoveryStore {
    options: PostgresConnectionOptions,
    pool: OnceLock<Pool<Postgres>>,
}

impl PostgresDiscoveryStore {
    pub fn new_lazy(
        transport: &StorageTransportConfig,
        password: Option<&str>,
    ) -> DiscoveryResult<Self> {
        let options = PostgresConnectionOptions::from_transport(transport, password)?;

        Ok(Self {
            options,
            pool: OnceLock::new(),
        })
    }

    pub fn safe_summary(&self) -> String {
        self.options.safe_summary()
    }

    pub fn connection_options(&self) -> &PostgresConnectionOptions {
        &self.options
    }

    pub fn initial_schema_sql(&self) -> &'static str {
        migration::INITIAL_SCHEMA_SQL
    }

    pub async fn apply_initial_schema(&self) -> DiscoveryResult<()> {
        sqlx::query(migration::INITIAL_SCHEMA_SQL)
            .execute(self.pool())
            .await
            .map_err(sqlx_error)?;
        Ok(())
    }

    pub(crate) fn pool(&self) -> &Pool<Postgres> {
        self.pool.get_or_init(|| {
            PgPoolOptions::new()
                .max_connections(self.options.max_connections())
                .acquire_timeout(self.options.acquire_timeout())
                .connect_lazy_with(self.options.to_sqlx_connect_options())
        })
    }
}
