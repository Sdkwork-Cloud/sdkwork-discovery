//! SDKWork Discovery database pool bootstrap via `sdkwork-database`.

use sdkwork_database_config::DatabaseConfig;
use sdkwork_database_sqlx::{create_pool_from_config, DatabasePool, PoolError};

pub use sdkwork_discovery_database_host::{
    bootstrap_discovery_database, bootstrap_discovery_database_from_env, DiscoveryDatabaseHost,
};

pub type DiscoveryDatabasePool = DatabasePool;

pub async fn connect_discovery_database_pool_from_env() -> Result<DiscoveryDatabasePool, PoolError>
{
    let config = DatabaseConfig::from_env("DISCOVERY")?;
    create_pool_from_config(config).await
}

pub async fn connect_and_bootstrap_discovery_database_from_env(
) -> Result<DiscoveryDatabaseHost, String> {
    let pool = connect_discovery_database_pool_from_env()
        .await
        .map_err(|error| error.to_string())?;
    bootstrap_discovery_database(pool).await
}
