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

    async fn gc_watch_events(
        &mut self,
        before_revision: u64,
        max_deletes: usize,
    ) -> DiscoveryResult<usize> {
        if max_deletes == 0 {
            return Err(sdkwork_discovery_contract::DiscoveryError::InvalidArgument(
                "max_deletes must be greater than zero".to_string(),
            ));
        }

        let mut removed = 0usize;
        self.events.retain(|event| {
            if event.revision <= before_revision && removed < max_deletes {
                removed += 1;
                false
            } else {
                true
            }
        });
        Ok(removed)
    }

    async fn compact_watch_events(
        &mut self,
        namespace: &str,
        environment: &str,
        max_events_per_resource: usize,
    ) -> DiscoveryResult<usize> {
        use std::collections::HashMap;

        let before_count = self.events.len();

        // Group events by resource_id
        let mut events_by_resource: HashMap<String, Vec<usize>> = HashMap::new();
        for (idx, event) in self.events.iter().enumerate() {
            if event.namespace == namespace && event.environment == environment {
                events_by_resource
                    .entry(event.resource_id.clone())
                    .or_default()
                    .push(idx);
            }
        }

        // Find indices to remove (keep only latest N per resource)
        let mut indices_to_remove = Vec::new();
        for indices in events_by_resource.values() {
            if indices.len() > max_events_per_resource {
                let remove_count = indices.len() - max_events_per_resource;
                indices_to_remove.extend_from_slice(&indices[..remove_count]);
            }
        }

        // Remove events in reverse order to preserve indices
        indices_to_remove.sort_unstable();
        indices_to_remove.reverse();
        for idx in indices_to_remove {
            self.events.remove(idx);
        }

        Ok(before_count - self.events.len())
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
