use std::collections::HashMap;

use sdkwork_discovery_contract::DiscoveryResult;
use serde::{Deserialize, Serialize};

use crate::store::{IdempotencyKey, IdempotencyRecord, InstanceKey, MemoryDiscoveryStore};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryDiscoverySnapshot {
    revision: u64,
    next_sequence: u64,
    instances: HashMap<InstanceKey, sdkwork_discovery_contract::ServiceInstance>,
    lease_index: HashMap<String, InstanceKey>,
    drafts: HashMap<String, sdkwork_discovery_contract::ConfigDraft>,
    releases: Vec<sdkwork_discovery_contract::ConfigRelease>,
    events: Vec<sdkwork_discovery_contract::DiscoveryEvent>,
    idempotency: HashMap<IdempotencyKey, IdempotencyRecord>,
}

impl MemoryDiscoveryStore {
    pub fn to_snapshot(&self) -> MemoryDiscoverySnapshot {
        MemoryDiscoverySnapshot {
            revision: self.revision,
            next_sequence: self.next_sequence,
            instances: self.instances.clone(),
            lease_index: self.lease_index.clone(),
            drafts: self.drafts.clone(),
            releases: self.releases.clone(),
            events: self.events.clone(),
            idempotency: self.idempotency.clone(),
        }
    }

    pub fn from_snapshot(snapshot: MemoryDiscoverySnapshot) -> Self {
        Self {
            revision: snapshot.revision,
            next_sequence: snapshot.next_sequence,
            instances: snapshot.instances,
            lease_index: snapshot.lease_index,
            drafts: snapshot.drafts,
            releases: snapshot.releases,
            events: snapshot.events,
            idempotency: snapshot.idempotency,
        }
    }

    pub fn to_snapshot_bytes(&self) -> DiscoveryResult<Vec<u8>> {
        serde_json::to_vec(&self.to_snapshot()).map_err(|error| {
            sdkwork_discovery_contract::DiscoveryError::InvalidConfig(format!(
                "memory snapshot encode failed: {error}"
            ))
        })
    }

    pub fn from_snapshot_bytes(bytes: &[u8]) -> DiscoveryResult<Self> {
        let snapshot: MemoryDiscoverySnapshot = serde_json::from_slice(bytes).map_err(|error| {
            sdkwork_discovery_contract::DiscoveryError::InvalidConfig(format!(
                "memory snapshot decode failed: {error}"
            ))
        })?;
        Ok(Self::from_snapshot(snapshot))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_bytes_round_trip_preserves_revision() {
        let store = MemoryDiscoveryStore::new();
        let bytes = store.to_snapshot_bytes().unwrap();
        let restored = MemoryDiscoveryStore::from_snapshot_bytes(&bytes).unwrap();
        assert_eq!(store.current_revision(), restored.current_revision());
    }
}
