use async_trait::async_trait;
use sdkwork_discovery_contract::{
    ConfigDraft, ConfigRelease, CreateConfigDraftCommand, DiscoveryResult, EffectiveConfig,
    PublishConfigCommand, RetrieveEffectiveConfigQuery, RollbackConfigCommand,
};
use sdkwork_discovery_storage_contract::ConfigStore;

use crate::store::RedisDiscoveryStore;

#[async_trait]
impl ConfigStore for RedisDiscoveryStore {
    async fn create_config_draft(
        &mut self,
        command: CreateConfigDraftCommand,
    ) -> DiscoveryResult<ConfigDraft> {
        self.hydrate_once().await?;
        let result = self
            .memory
            .lock()
            .await
            .create_config_draft(command)
            .await?;
        self.persist_state().await?;
        Ok(result)
    }

    async fn publish_config(
        &mut self,
        command: PublishConfigCommand,
    ) -> DiscoveryResult<ConfigRelease> {
        self.hydrate_once().await?;
        let result = self.memory.lock().await.publish_config(command).await?;
        self.persist_state().await?;
        Ok(result)
    }

    async fn rollback_config(
        &mut self,
        command: RollbackConfigCommand,
    ) -> DiscoveryResult<ConfigRelease> {
        self.hydrate_once().await?;
        let result = self.memory.lock().await.rollback_config(command).await?;
        self.persist_state().await?;
        Ok(result)
    }

    async fn retrieve_effective_config(
        &self,
        query: RetrieveEffectiveConfigQuery,
    ) -> DiscoveryResult<EffectiveConfig> {
        self.hydrate_once().await?;
        self.memory
            .lock()
            .await
            .retrieve_effective_config(query)
            .await
    }
}
