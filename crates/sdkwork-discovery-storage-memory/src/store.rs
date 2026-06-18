use std::collections::HashMap;

use sdkwork_discovery_contract::{ConfigDraft, ConfigRelease, DiscoveryEvent, ServiceInstance};

#[derive(Debug, Default)]
pub struct MemoryDiscoveryStore {
    pub(crate) revision: u64,
    pub(crate) next_sequence: u64,
    pub(crate) instances: HashMap<InstanceKey, ServiceInstance>,
    pub(crate) lease_index: HashMap<String, InstanceKey>,
    pub(crate) drafts: HashMap<String, ConfigDraft>,
    pub(crate) releases: Vec<ConfigRelease>,
    pub(crate) events: Vec<DiscoveryEvent>,
    pub(crate) idempotency: HashMap<IdempotencyKey, IdempotencyRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct InstanceKey {
    pub(crate) namespace: String,
    pub(crate) environment: String,
    pub(crate) service_name: String,
    pub(crate) instance_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct IdempotencyKey {
    pub(crate) operation_id: String,
    pub(crate) key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IdempotencyRecord {
    pub(crate) request_hash: String,
    pub(crate) resource_id: String,
}

impl MemoryDiscoveryStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn current_revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn next_revision(&mut self) -> u64 {
        self.revision += 1;
        self.revision
    }

    pub(crate) fn next_id(&mut self, prefix: &str) -> String {
        self.next_sequence += 1;
        format!("{prefix}-{}", self.next_sequence)
    }
}
