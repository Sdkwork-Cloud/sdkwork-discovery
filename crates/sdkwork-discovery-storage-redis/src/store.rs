use std::sync::atomic::{AtomicBool, Ordering};

use redis::aio::MultiplexedConnection;
use redis::AsyncCommands;
use sdkwork_discovery_config::StorageTransportConfig;
use sdkwork_discovery_contract::DiscoveryResult;
use sdkwork_discovery_storage_memory::MemoryDiscoveryStore;
use tokio::sync::Mutex;

use crate::connection::{RedisConnectionOptions, DISCOVERY_REDIS_KEY_PREFIX};

pub fn map_redis_error(error: redis::RedisError) -> sdkwork_discovery_contract::DiscoveryError {
    // Redis driver errors are runtime failures (connection loss, AUTH failure,
    // pool exhaustion), not configuration defects. Map them to `Unavailable` so
    // the RPC layer surfaces UNAVAILABLE instead of FAILED_PRECONDITION and
    // returns a fixed sanitized message; the original driver message (which may
    // contain host/port and AUTH details) stays in server logs only.
    sdkwork_discovery_contract::DiscoveryError::Unavailable(format!("redis error: {error}"))
}

#[derive(Debug)]
pub struct RedisDiscoveryStore {
    client: Option<redis::Client>,
    state_key: String,
    safe_summary: String,
    pub(crate) memory: Mutex<MemoryDiscoveryStore>,
    hydrated: AtomicBool,
    persist_enabled: bool,
}

impl RedisDiscoveryStore {
    pub fn new_lazy(transport: &StorageTransportConfig) -> DiscoveryResult<Self> {
        let options = RedisConnectionOptions::from_transport(transport)?;
        let redis_url = options.redis_url()?;
        let client = redis::Client::open(redis_url).map_err(map_redis_error)?;
        Ok(Self {
            client: Some(client),
            state_key: options.state_key(),
            safe_summary: options.safe_summary(),
            memory: Mutex::new(MemoryDiscoveryStore::new()),
            hydrated: AtomicBool::new(false),
            persist_enabled: true,
        })
    }

    pub fn new_in_memory_delegate() -> Self {
        Self {
            client: None,
            state_key: format!("{DISCOVERY_REDIS_KEY_PREFIX}:test-delegate"),
            safe_summary: "redis delegate=memory (no persistence)".to_string(),
            memory: Mutex::new(MemoryDiscoveryStore::new()),
            hydrated: AtomicBool::new(true),
            persist_enabled: false,
        }
    }

    pub fn safe_summary(&self) -> &str {
        &self.safe_summary
    }

    pub(crate) async fn hydrate_once(&self) -> DiscoveryResult<()> {
        if self.hydrated.load(Ordering::Acquire) {
            return Ok(());
        }
        let mut connection = self.connection().await?;
        let bytes: Option<Vec<u8>> = connection
            .get(&self.state_key)
            .await
            .map_err(map_redis_error)?;
        if let Some(bytes) = bytes {
            let restored = MemoryDiscoveryStore::from_snapshot_bytes(&bytes)?;
            let mut memory = self.memory.lock().await;
            *memory = restored;
        }
        self.hydrated.store(true, Ordering::Release);
        Ok(())
    }

    pub(crate) async fn persist_state(&self) -> DiscoveryResult<()> {
        if !self.persist_enabled {
            return Ok(());
        }
        let bytes = {
            let memory = self.memory.lock().await;
            memory.to_snapshot_bytes()?
        };
        let mut connection = self.connection().await?;
        connection
            .set::<_, _, ()>(&self.state_key, bytes)
            .await
            .map_err(map_redis_error)?;
        Ok(())
    }

    async fn connection(&self) -> DiscoveryResult<MultiplexedConnection> {
        let client = self.client.as_ref().ok_or_else(|| {
            sdkwork_discovery_contract::DiscoveryError::InvalidConfig(
                "redis connection is disabled for in-memory delegate".to_string(),
            )
        })?;
        client
            .get_multiplexed_async_connection()
            .await
            .map_err(map_redis_error)
    }
}
