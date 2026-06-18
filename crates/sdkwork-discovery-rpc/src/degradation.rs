#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DegradationConfig {
    pub read_only_on_storage_failure: bool,
    pub stale_read_max_age_ms: u64,
}

impl Default for DegradationConfig {
    fn default() -> Self {
        Self {
            read_only_on_storage_failure: false,
            stale_read_max_age_ms: 60_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationType {
    Read,
    Write,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DegradationState {
    Normal,
    ReadOnly,
}

impl DegradationState {
    pub fn allows(&self, op: OperationType) -> bool {
        match self {
            DegradationState::Normal => true,
            DegradationState::ReadOnly => matches!(op, OperationType::Read),
        }
    }
}
