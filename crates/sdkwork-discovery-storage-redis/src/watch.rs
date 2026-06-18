use async_trait::async_trait;
use sdkwork_discovery_contract::{DiscoveryEvent, DiscoveryResult, WatchEventsQuery};
use sdkwork_discovery_storage_contract::WatchEventStore;

use crate::store::RedisDiscoveryStore;

#[async_trait]
impl WatchEventStore for RedisDiscoveryStore {
    async fn watch_events(&self, query: WatchEventsQuery) -> DiscoveryResult<Vec<DiscoveryEvent>> {
        self.hydrate_once().await?;
        self.memory.lock().await.watch_events(query).await
    }

    async fn gc_watch_events(
        &mut self,
        before_revision: u64,
        max_deletes: usize,
    ) -> DiscoveryResult<usize> {
        self.hydrate_once().await?;
        let removed = self
            .memory
            .lock()
            .await
            .gc_watch_events(before_revision, max_deletes)
            .await?;
        self.persist_state().await?;
        Ok(removed)
    }

    async fn compact_watch_events(
        &mut self,
        namespace: &str,
        environment: &str,
        max_events_per_resource: usize,
    ) -> DiscoveryResult<usize> {
        self.hydrate_once().await?;
        let removed = self
            .memory
            .lock()
            .await
            .compact_watch_events(namespace, environment, max_events_per_resource)
            .await?;
        self.persist_state().await?;
        Ok(removed)
    }
}
