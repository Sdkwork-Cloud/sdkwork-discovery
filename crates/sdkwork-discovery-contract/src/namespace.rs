use std::collections::HashMap;

use async_trait::async_trait;

use crate::error::DiscoveryResult;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NamespaceConfig {
    pub namespace: String,
    pub max_instances: Option<usize>,
    pub max_services: Option<usize>,
    pub max_config_releases: Option<usize>,
    pub allowed_writers: Vec<String>,
    pub allowed_readers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamespaceQuotaStatus {
    pub namespace: String,
    pub current_instances: usize,
    pub max_instances: Option<usize>,
    pub current_services: usize,
    pub max_services: Option<usize>,
}

#[async_trait]
pub trait NamespaceStore {
    async fn create_namespace(&mut self, config: NamespaceConfig) -> DiscoveryResult<()>;
    async fn get_namespace(&self, namespace: &str) -> DiscoveryResult<Option<NamespaceConfig>>;
    async fn update_namespace(&mut self, config: NamespaceConfig) -> DiscoveryResult<()>;
    async fn delete_namespace(&mut self, namespace: &str) -> DiscoveryResult<bool>;
    async fn list_namespaces(&self) -> DiscoveryResult<Vec<NamespaceConfig>>;
    async fn check_instance_quota(&self, namespace: &str) -> DiscoveryResult<bool>;
    async fn check_service_quota(&self, namespace: &str) -> DiscoveryResult<bool>;
}

pub struct MemoryNamespaceStore {
    namespaces: HashMap<String, NamespaceConfig>,
}

impl Default for MemoryNamespaceStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryNamespaceStore {
    pub fn new() -> Self {
        Self {
            namespaces: HashMap::new(),
        }
    }
}

#[async_trait]
impl NamespaceStore for MemoryNamespaceStore {
    async fn create_namespace(&mut self, config: NamespaceConfig) -> DiscoveryResult<()> {
        self.namespaces.insert(config.namespace.clone(), config);
        Ok(())
    }

    async fn get_namespace(&self, namespace: &str) -> DiscoveryResult<Option<NamespaceConfig>> {
        Ok(self.namespaces.get(namespace).cloned())
    }

    async fn update_namespace(&mut self, config: NamespaceConfig) -> DiscoveryResult<()> {
        self.namespaces.insert(config.namespace.clone(), config);
        Ok(())
    }

    async fn delete_namespace(&mut self, namespace: &str) -> DiscoveryResult<bool> {
        Ok(self.namespaces.remove(namespace).is_some())
    }

    async fn list_namespaces(&self) -> DiscoveryResult<Vec<NamespaceConfig>> {
        Ok(self.namespaces.values().cloned().collect())
    }

    async fn check_instance_quota(&self, _namespace: &str) -> DiscoveryResult<bool> {
        // Quota checking would require access to instance count
        Ok(true)
    }

    async fn check_service_quota(&self, _namespace: &str) -> DiscoveryResult<bool> {
        // Quota checking would require access to service count
        Ok(true)
    }
}
