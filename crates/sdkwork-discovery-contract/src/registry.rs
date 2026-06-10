use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, PartialEq, Eq)]
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
pub struct ReportInstanceStatusCommand {
    pub namespace: String,
    pub environment: String,
    pub service_name: String,
    pub instance_id: String,
    pub status: InstanceStatus,
    pub now_ms: u64,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoverInstancesQuery {
    pub namespace: String,
    pub environment: String,
    pub service_name: String,
    pub healthy_only: bool,
    pub protocol: Option<String>,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListServicesQuery {
    pub namespace: String,
    pub environment: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListServicesResult {
    pub revision: u64,
    pub services: Vec<ServiceSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceSummary {
    pub namespace: String,
    pub environment: String,
    pub service_name: String,
    pub active_instance_count: usize,
    pub latest_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigFormat {
    Text,
    Json,
    Toml,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoveryEventKind {
    InstanceRegistered,
    InstanceUpdated,
    InstanceStatusReported,
    InstanceRenewed,
    InstanceDeregistered,
    ConfigPublished,
    ConfigRolledBack,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
                event
                    .config_application
                    .as_deref()
                    .is_none_or(|event_application| event_application == application)
            })
    }
}
