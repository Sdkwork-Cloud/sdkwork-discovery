use sdkwork_discovery_contract::{
    DiscoveryEvent, DiscoveryEventKind, DiscoveryResult, WatchEventsQuery,
};
use sdkwork_discovery_storage_contract::WatchEventStore;
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub struct WatchEventPublisher {
    sender: broadcast::Sender<DiscoveryEvent>,
}

impl WatchEventPublisher {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity.max(1));
        Self { sender }
    }

    pub fn publish(&self, event: DiscoveryEvent) {
        let _ = self.sender.send(event);
    }

    pub fn subscribe(&self) -> WatchEventSubscriber {
        WatchEventSubscriber {
            receiver: self.sender.subscribe(),
        }
    }
}

pub struct WatchEventSubscriber {
    receiver: broadcast::Receiver<DiscoveryEvent>,
}

impl WatchEventSubscriber {
    pub async fn recv(&mut self) -> Option<DiscoveryResult<DiscoveryEvent>> {
        match self.receiver.recv().await {
            Ok(event) => Some(Ok(event)),
            Err(broadcast::error::RecvError::Lagged(skipped)) => Some(Err(
                sdkwork_discovery_contract::DiscoveryError::ResourceExhausted(format!(
                    "discovery watch stream lagged behind by {skipped} events"
                )),
            )),
            Err(broadcast::error::RecvError::Closed) => None,
        }
    }
}

pub async fn events_for_revision<S>(
    store: &S,
    namespace: &str,
    environment: &str,
    revision: u64,
) -> DiscoveryResult<Vec<DiscoveryEvent>>
where
    S: WatchEventStore,
{
    if revision == 0 {
        return Ok(Vec::new());
    }

    let mut events = store
        .watch_events(WatchEventsQuery {
            namespace: namespace.to_string(),
            environment: environment.to_string(),
            from_revision: revision - 1,
            service_name: None,
            config_group: None,
            config_application: None,
            max_events: 1,
        })
        .await?;
    events.retain(|event| event.revision == revision);
    Ok(events)
}

pub fn event_matches_service_watch(event: &DiscoveryEvent, query: &WatchEventsQuery) -> bool {
    query.matches_event(event)
        && query
            .service_name
            .as_ref()
            .is_none_or(|service_name| event.service_name.as_deref() == Some(service_name.as_str()))
        && matches!(
            event.kind,
            DiscoveryEventKind::InstanceRegistered
                | DiscoveryEventKind::InstanceUpdated
                | DiscoveryEventKind::InstanceStatusReported
                | DiscoveryEventKind::InstanceRenewed
                | DiscoveryEventKind::InstanceDeregistered
        )
}

pub fn event_matches_config_watch(event: &DiscoveryEvent, query: &WatchEventsQuery) -> bool {
    query.matches_event(event)
        && matches!(
            event.kind,
            DiscoveryEventKind::ConfigPublished | DiscoveryEventKind::ConfigRolledBack
        )
}

#[cfg(test)]
mod tests {
    use sdkwork_discovery_contract::{DiscoveryEvent, DiscoveryEventKind, WatchEventsQuery};

    use super::event_matches_config_watch;

    #[test]
    fn config_watch_filters_application_scoped_events_by_application() {
        let event = DiscoveryEvent {
            revision: 1,
            namespace: "sdkwork".to_string(),
            environment: "development".to_string(),
            kind: DiscoveryEventKind::ConfigPublished,
            resource_id: "release-1".to_string(),
            service_name: None,
            config_group: Some("runtime".to_string()),
            config_key: Some("log.level".to_string()),
            config_application: Some("sdkwork-drive".to_string()),
        };
        let query = WatchEventsQuery {
            namespace: "sdkwork".to_string(),
            environment: "development".to_string(),
            from_revision: 0,
            service_name: None,
            config_group: Some("runtime".to_string()),
            config_application: Some("sdkwork-chat".to_string()),
            max_events: 1_024,
        };

        assert!(!event_matches_config_watch(&event, &query));
    }
}
