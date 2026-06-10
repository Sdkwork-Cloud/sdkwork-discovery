use async_trait::async_trait;
use sdkwork_discovery_contract::{
    ConfigDraft, ConfigRelease, CreateConfigDraftCommand, DiscoveryResult, EffectiveConfig,
    PublishConfigCommand, RetrieveEffectiveConfigQuery, RollbackConfigCommand,
};

#[async_trait]
pub trait ConfigStore {
    async fn create_config_draft(
        &mut self,
        command: CreateConfigDraftCommand,
    ) -> DiscoveryResult<ConfigDraft>;

    async fn publish_config(
        &mut self,
        command: PublishConfigCommand,
    ) -> DiscoveryResult<ConfigRelease>;

    async fn rollback_config(
        &mut self,
        command: RollbackConfigCommand,
    ) -> DiscoveryResult<ConfigRelease>;

    async fn retrieve_effective_config(
        &self,
        query: RetrieveEffectiveConfigQuery,
    ) -> DiscoveryResult<EffectiveConfig>;
}
