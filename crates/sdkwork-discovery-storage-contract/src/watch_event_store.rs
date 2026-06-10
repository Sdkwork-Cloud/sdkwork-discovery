use async_trait::async_trait;
use sdkwork_discovery_contract::{DiscoveryEvent, DiscoveryResult, WatchEventsQuery};

#[async_trait]
pub trait WatchEventStore {
    async fn watch_events(&self, query: WatchEventsQuery) -> DiscoveryResult<Vec<DiscoveryEvent>>;
}
