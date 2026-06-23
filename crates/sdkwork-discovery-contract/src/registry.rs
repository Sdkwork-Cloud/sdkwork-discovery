use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceStatus {
    Serving,
    Degraded,
    NotServing,
}

impl InstanceStatus {
    pub fn is_discoverable(&self) -> bool {
        matches!(self, Self::Serving | Self::Degraded)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterInstanceCommand {
    pub namespace: String,
    pub environment: String,
    pub service_name: String,
    pub instance_id: String,
    pub endpoint: String,
    pub protocol: String,
    pub version: String,
    pub region: String,
    pub zone: String,
    pub weight: u32,
    pub priority: u32,
    pub status: InstanceStatus,
    pub metadata: HashMap<String, String>,
    pub lease_ttl_seconds: u64,
    pub now_ms: u64,
    pub expected_revision: Option<u64>,
    pub persistent: bool,
    pub health_check: Option<crate::health_check::HealthCheckConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterInstanceResult {
    pub lease_id: String,
    pub namespace: String,
    pub environment: String,
    pub service_name: String,
    pub instance_id: String,
    pub revision: u64,
    pub expires_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchRegisterResult {
    pub results: Vec<RegisterInstanceResult>,
    pub errors: Vec<BatchOperationError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchOperationError {
    pub index: usize,
    pub error_code: String,
    pub error_message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportInstanceStatusCommand {
    pub namespace: String,
    pub environment: String,
    pub service_name: String,
    pub instance_id: String,
    pub status: InstanceStatus,
    pub now_ms: u64,
    pub expected_revision: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportInstanceStatusResult {
    pub revision: u64,
    pub status: InstanceStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenewLeaseCommand {
    pub lease_id: String,
    pub lease_ttl_seconds: u64,
    pub now_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenewLeaseResult {
    pub lease_id: String,
    pub namespace: String,
    pub environment: String,
    pub service_name: String,
    pub instance_id: String,
    pub revision: u64,
    pub expires_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeregisterInstanceResult {
    pub namespace: String,
    pub environment: String,
    pub service_name: String,
    pub instance_id: String,
    pub revision: u64,
    pub deregistered: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DiscoverInstancesQuery {
    pub namespace: String,
    pub environment: String,
    pub service_name: String,
    pub healthy_only: bool,
    pub protocol: Option<String>,
    pub label_filters: Vec<LabelFilter>,
    pub sort_by: Option<DiscoverSortBy>,
    pub page_size: u32,
    pub page_token: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoverSortBy {
    InstanceId,
    Priority,
    Weight,
    WeightedRandom,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelFilter {
    pub key: String,
    pub op: LabelFilterOp,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LabelFilterOp {
    Eq,
    NotEq,
    In,
    Exists,
}

impl LabelFilter {
    pub fn matches(&self, metadata: &HashMap<String, String>) -> bool {
        match &self.op {
            LabelFilterOp::Eq => metadata.get(&self.key) == Some(&self.value),
            LabelFilterOp::NotEq => metadata.get(&self.key) != Some(&self.value),
            LabelFilterOp::In => {
                let values: Vec<&str> = self.value.split(',').collect();
                metadata
                    .get(&self.key)
                    .is_some_and(|v| values.contains(&v.as_str()))
            }
            LabelFilterOp::Exists => metadata.contains_key(&self.key),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetrieveInstanceQuery {
    pub namespace: String,
    pub environment: String,
    pub service_name: String,
    pub instance_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoverInstancesResult {
    pub revision: u64,
    pub instances: Vec<ServiceInstance>,
    pub next_page_token: Option<String>,
}

/// Applies label filters and sort order after storage retrieval so all backends behave consistently.
pub fn finalize_discover_instances(
    mut instances: Vec<ServiceInstance>,
    query: &DiscoverInstancesQuery,
    revision: u64,
) -> DiscoverInstancesResult {
    instances.retain(|instance| {
        query
            .label_filters
            .iter()
            .all(|filter| filter.matches(&instance.metadata))
    });

    match query
        .sort_by
        .as_ref()
        .copied()
        .unwrap_or(DiscoverSortBy::InstanceId)
    {
        DiscoverSortBy::InstanceId => {
            instances.sort_by(|a, b| a.instance_id.cmp(&b.instance_id));
        }
        DiscoverSortBy::Priority => {
            instances.sort_by(|a, b| {
                a.priority
                    .cmp(&b.priority)
                    .then_with(|| b.weight.cmp(&a.weight))
                    .then_with(|| a.instance_id.cmp(&b.instance_id))
            });
        }
        DiscoverSortBy::Weight => {
            instances.sort_by(|a, b| {
                b.weight
                    .cmp(&a.weight)
                    .then_with(|| a.instance_id.cmp(&b.instance_id))
            });
        }
        DiscoverSortBy::WeightedRandom => {
            instances.sort_by(|a, b| a.priority.cmp(&b.priority));
            weighted_shuffle_discover_instances(&mut instances);
            let page_size = crate::pagination::normalize_page_size(query.page_size);
            if instances.len() > page_size as usize {
                instances.truncate(page_size as usize);
            }
            return DiscoverInstancesResult {
                revision,
                instances,
                next_page_token: None,
            };
        }
    }

    let (instances, next_page_token) = crate::pagination::paginate_sorted_keys(
        instances,
        query.page_size,
        query.page_token.as_deref(),
        |instance| instance.instance_id.clone(),
    );

    DiscoverInstancesResult {
        revision,
        instances,
        next_page_token,
    }
}

fn weighted_shuffle_discover_instances(instances: &mut Vec<ServiceInstance>) {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let total_weight: u64 = instances.iter().map(|i| i.weight as u64).sum();
    if total_weight == 0 {
        return;
    }

    let mut result = Vec::with_capacity(instances.len());
    let mut remaining = std::mem::take(instances);

    while !remaining.is_empty() {
        let current_weight: u64 = remaining.iter().map(|i| i.weight as u64).sum();
        if current_weight == 0 {
            result.append(&mut remaining);
            break;
        }

        let mut hasher = DefaultHasher::new();
        remaining.len().hash(&mut hasher);
        result.len().hash(&mut hasher);
        let target = hasher.finish() % current_weight;

        let mut cumulative = 0u64;
        let mut selected_idx = 0;
        for (idx, instance) in remaining.iter().enumerate() {
            cumulative += instance.weight as u64;
            if cumulative > target {
                selected_idx = idx;
                break;
            }
        }

        result.push(remaining.remove(selected_idx));
    }

    *instances = result;
}

pub fn finalize_list_services(
    mut services: Vec<ServiceSummary>,
    revision: u64,
    query: &ListServicesQuery,
) -> ListServicesResult {
    services.sort_by(|left, right| left.service_name.cmp(&right.service_name));
    let (services, next_page_token) = crate::pagination::paginate_sorted_keys(
        services,
        query.page_size,
        query.page_token.as_deref(),
        |summary| summary.service_name.clone(),
    );

    ListServicesResult {
        revision,
        services,
        next_page_token,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ListServicesQuery {
    pub namespace: String,
    pub environment: String,
    pub page_size: u32,
    pub page_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListServicesResult {
    pub revision: u64,
    pub services: Vec<ServiceSummary>,
    pub next_page_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceSummary {
    pub namespace: String,
    pub environment: String,
    pub service_name: String,
    pub active_instance_count: usize,
    pub latest_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ServiceInstance {
    pub namespace: String,
    pub environment: String,
    pub service_name: String,
    pub instance_id: String,
    pub endpoint: String,
    pub protocol: String,
    pub version: String,
    pub region: String,
    pub zone: String,
    pub weight: u32,
    pub priority: u32,
    pub status: InstanceStatus,
    pub metadata: HashMap<String, String>,
    pub lease_id: String,
    pub expires_at_ms: u64,
    pub revision: u64,
    pub health_check: Option<crate::health_check::HealthCheckConfig>,
    pub health_check_state: crate::health_check::HealthCheckRuntimeState,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigFormat {
    Text,
    Json,
    Toml,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConfigScope {
    Namespace,
    Application {
        application: String,
    },
    Service {
        application: String,
        service_name: String,
    },
}

impl ConfigScope {
    pub fn specificity(&self) -> u8 {
        match self {
            Self::Namespace => 0,
            Self::Application { .. } => 1,
            Self::Service { .. } => 2,
        }
    }

    pub fn applies_to(&self, application: &str, service_name: &str) -> bool {
        match self {
            Self::Namespace => true,
            Self::Application {
                application: scope_application,
            } => scope_application == application,
            Self::Service {
                application: scope_application,
                service_name: scope_service,
            } => scope_application == application && scope_service == service_name,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdempotencyContext {
    pub operation_id: String,
    pub key: String,
    pub request_hash: String,
}

impl IdempotencyContext {
    pub fn new(
        operation_id: impl Into<String>,
        key: impl Into<String>,
        request_hash: impl Into<String>,
    ) -> Self {
        Self {
            operation_id: operation_id.into(),
            key: key.into(),
            request_hash: request_hash.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateConfigDraftCommand {
    pub namespace: String,
    pub environment: String,
    pub group: String,
    pub key: String,
    pub format: ConfigFormat,
    pub value: String,
    pub scope: ConfigScope,
    pub created_by: String,
    pub idempotency: Option<IdempotencyContext>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ConfigDraft {
    pub draft_id: String,
    pub namespace: String,
    pub environment: String,
    pub group: String,
    pub key: String,
    pub format: ConfigFormat,
    pub value: String,
    pub scope: ConfigScope,
    pub created_by: String,
    pub content_hash: String,
    pub published: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishConfigCommand {
    pub draft_id: String,
    pub published_by: String,
    pub now_ms: u64,
    pub idempotency: Option<IdempotencyContext>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackConfigCommand {
    pub source_release_id: String,
    pub rolled_back_by: String,
    pub now_ms: u64,
    pub idempotency: Option<IdempotencyContext>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ConfigRelease {
    pub release_id: String,
    pub draft_id: String,
    pub namespace: String,
    pub environment: String,
    pub group: String,
    pub key: String,
    pub format: ConfigFormat,
    pub value: String,
    pub scope: ConfigScope,
    pub content_hash: String,
    pub published_by: String,
    pub published_at_ms: u64,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetrieveEffectiveConfigQuery {
    pub namespace: String,
    pub environment: String,
    pub application: String,
    pub service_name: String,
    pub group: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveConfig {
    pub revision: u64,
    pub values: BTreeMap<String, EffectiveConfigValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveConfigValue {
    pub value: String,
    pub format: ConfigFormat,
    pub source_release_id: String,
    pub source_specificity: u8,
    pub source_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryEventKind {
    InstanceRegistered,
    InstanceUpdated,
    InstanceStatusReported,
    InstanceRenewed,
    InstanceDeregistered,
    ConfigPublished,
    ConfigRolledBack,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DiscoveryEvent {
    pub revision: u64,
    pub namespace: String,
    pub environment: String,
    pub kind: DiscoveryEventKind,
    pub resource_id: String,
    pub service_name: Option<String>,
    pub config_group: Option<String>,
    pub config_key: Option<String>,
    pub config_application: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchEventsQuery {
    pub namespace: String,
    pub environment: String,
    pub from_revision: u64,
    pub service_name: Option<String>,
    pub config_group: Option<String>,
    pub config_application: Option<String>,
    pub max_events: usize,
}

impl WatchEventsQuery {
    pub fn matches_event(&self, event: &DiscoveryEvent) -> bool {
        event.namespace == self.namespace
            && event.environment == self.environment
            && event.revision > self.from_revision
            && self.service_name.as_ref().is_none_or(|service_name| {
                event
                    .service_name
                    .as_deref()
                    .is_none_or(|event_service| event_service == service_name)
            })
            && self.config_group.as_ref().is_none_or(|config_group| {
                event.config_group.as_deref() == Some(config_group.as_str())
            })
            && self.config_application.as_ref().is_none_or(|application| {
                event.config_application.as_deref() == Some(application.as_str())
            })
    }
}

#[cfg(test)]
mod watch_query_tests {
    use super::{DiscoveryEvent, DiscoveryEventKind, WatchEventsQuery};

    #[test]
    fn application_scoped_watch_rejects_cross_application_events() {
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

        assert!(!query.matches_event(&event));
    }

    #[test]
    fn application_scoped_watch_rejects_events_without_application() {
        let event = DiscoveryEvent {
            revision: 1,
            namespace: "sdkwork".to_string(),
            environment: "development".to_string(),
            kind: DiscoveryEventKind::ConfigPublished,
            resource_id: "release-1".to_string(),
            service_name: None,
            config_group: Some("runtime".to_string()),
            config_key: Some("log.level".to_string()),
            config_application: None,
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

        assert!(!query.matches_event(&event));
    }
}
