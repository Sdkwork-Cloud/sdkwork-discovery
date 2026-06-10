use async_trait::async_trait;
use sdkwork_discovery_contract::{DiscoveryEvent, DiscoveryResult, WatchEventsQuery};
use sdkwork_discovery_storage_contract::WatchEventStore;

use crate::store::MemoryDiscoveryStore;
use crate::validation::validate_non_empty;

#[async_trait]
impl WatchEventStore for MemoryDiscoveryStore {
    async fn watch_events(&self, query: WatchEventsQuery) -> DiscoveryResult<Vec<DiscoveryEvent>> {
        validate_watch_query(&query)?;

        Ok(self
            .events
            .iter()
            .filter(|event| query.matches_event(event))
            .take(query.max_events)
            .cloned()
            .collect())
    }
}

fn validate_watch_query(query: &WatchEventsQuery) -> DiscoveryResult<()> {
    validate_non_empty("namespace", &query.namespace)?;
    validate_non_empty("environment", &query.environment)?;
    validate_optional_filter("service_name", query.service_name.as_deref())?;
    validate_optional_filter("config_group", query.config_group.as_deref())?;
    validate_optional_filter("config_application", query.config_application.as_deref())?;
    if query.max_events == 0 {
        return Err(sdkwork_discovery_contract::DiscoveryError::InvalidArgument(
            "max_events must be greater than zero".to_string(),
        ));
    }
    Ok(())
}

fn validate_optional_filter(field: &str, value: Option<&str>) -> DiscoveryResult<()> {
    if let Some(value) = value {
        validate_non_empty(field, value)?;
    }
    Ok(())
}
