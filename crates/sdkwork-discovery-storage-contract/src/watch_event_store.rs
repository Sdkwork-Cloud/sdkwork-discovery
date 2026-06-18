use async_trait::async_trait;
use sdkwork_discovery_contract::{DiscoveryEvent, DiscoveryResult, WatchEventsQuery};

#[async_trait]
pub trait WatchEventStore {
    async fn watch_events(&self, query: WatchEventsQuery) -> DiscoveryResult<Vec<DiscoveryEvent>>;

    async fn gc_watch_events(
        &mut self,
        before_revision: u64,
        max_deletes: usize,
    ) -> DiscoveryResult<usize>;

    async fn compact_watch_events(
        &mut self,
        namespace: &str,
        environment: &str,
        max_events_per_resource: usize,
    ) -> DiscoveryResult<usize>;
}
