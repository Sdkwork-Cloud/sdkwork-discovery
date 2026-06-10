use async_trait::async_trait;
use sdkwork_discovery_contract::{
    ConfigDraft, ConfigRelease, ConfigScope, CreateConfigDraftCommand, DiscoveryError,
    DiscoveryEvent, DiscoveryEventKind, DiscoveryResult, EffectiveConfig, EffectiveConfigValue,
    IdempotencyContext, PublishConfigCommand, RetrieveEffectiveConfigQuery, RollbackConfigCommand,
};
use sdkwork_discovery_storage_contract::ConfigStore;

use crate::hash::content_hash;
use crate::store::{IdempotencyKey, IdempotencyRecord, MemoryDiscoveryStore};
use crate::validation::validate_non_empty;

#[async_trait]
impl ConfigStore for MemoryDiscoveryStore {
    async fn create_config_draft(
        &mut self,
        command: CreateConfigDraftCommand,
    ) -> DiscoveryResult<ConfigDraft> {
        validate_config_draft_command(&command)?;
        if let Some(draft) = self.replay_config_draft(&command)? {
            return Ok(draft);
        }

        let draft_id = self.next_id("draft");
        let draft = ConfigDraft {
            draft_id: draft_id.clone(),
            namespace: command.namespace,
            environment: command.environment,
            group: command.group,
            key: command.key,
            format: command.format,
            content_hash: content_hash(&command.value),
            value: command.value,
            scope: command.scope,
            created_by: command.created_by,
            published: false,
        };
        self.drafts.insert(draft_id, draft.clone());
        self.record_idempotency(command.idempotency.as_ref(), draft.draft_id.clone());
        Ok(draft)
    }

    async fn publish_config(
        &mut self,
        command: PublishConfigCommand,
    ) -> DiscoveryResult<ConfigRelease> {
        validate_publish_config_command(&command)?;
        if let Some(release) = self.replay_config_release(command.idempotency.as_ref())? {
            return Ok(release);
        }

        let draft_snapshot = {
            let draft = self
                .drafts
                .get(&command.draft_id)
                .ok_or_else(|| DiscoveryError::NotFound("config draft not found".to_string()))?;
            if draft.published {
                return Err(DiscoveryError::AlreadyPublished(
                    "config draft is already published".to_string(),
                ));
            }
            draft.clone()
        };

        let revision = self.next_revision();
        let release_id = self.next_id("release");
        let release = ConfigRelease {
            release_id: release_id.clone(),
            draft_id: draft_snapshot.draft_id.clone(),
            namespace: draft_snapshot.namespace,
            environment: draft_snapshot.environment,
            group: draft_snapshot.group,
            key: draft_snapshot.key,
            format: draft_snapshot.format,
            value: draft_snapshot.value,
            scope: draft_snapshot.scope,
            content_hash: draft_snapshot.content_hash,
            published_by: command.published_by,
            published_at_ms: command.now_ms,
            revision,
        };

        let draft = self
            .drafts
            .get_mut(&command.draft_id)
            .ok_or_else(|| DiscoveryError::NotFound("config draft not found".to_string()))?;
        draft.published = true;
        self.releases.push(release.clone());
        self.events.push(DiscoveryEvent {
            revision,
            namespace: release.namespace.clone(),
            environment: release.environment.clone(),
            kind: DiscoveryEventKind::ConfigPublished,
            resource_id: release.release_id.clone(),
            config_application: config_application_from_scope(&release.scope),
            service_name: match &release.scope {
                sdkwork_discovery_contract::ConfigScope::Service { service_name, .. } => {
                    Some(service_name.clone())
                }
                _ => None,
            },
            config_group: Some(release.group.clone()),
            config_key: Some(release.key.clone()),
        });
        self.record_idempotency(command.idempotency.as_ref(), release.release_id.clone());
        Ok(release)
    }

    async fn rollback_config(
        &mut self,
        command: RollbackConfigCommand,
    ) -> DiscoveryResult<ConfigRelease> {
        validate_rollback_config_command(&command)?;
        if let Some(release) = self.replay_config_release(command.idempotency.as_ref())? {
            return Ok(release);
        }

        let source = self
            .releases
            .iter()
            .find(|release| release.release_id == command.source_release_id)
            .cloned()
            .ok_or_else(|| DiscoveryError::NotFound("config release not found".to_string()))?;

        let revision = self.next_revision();
        let release_id = self.next_id("release");
        let release = ConfigRelease {
            release_id: release_id.clone(),
            draft_id: source.draft_id,
            namespace: source.namespace,
            environment: source.environment,
            group: source.group,
            key: source.key,
            format: source.format,
            value: source.value,
            scope: source.scope,
            content_hash: source.content_hash,
            published_by: command.rolled_back_by,
            published_at_ms: command.now_ms,
            revision,
        };

        self.releases.push(release.clone());
        self.events.push(DiscoveryEvent {
            revision,
            namespace: release.namespace.clone(),
            environment: release.environment.clone(),
            kind: DiscoveryEventKind::ConfigRolledBack,
            resource_id: release.release_id.clone(),
            config_application: config_application_from_scope(&release.scope),
            service_name: match &release.scope {
                sdkwork_discovery_contract::ConfigScope::Service { service_name, .. } => {
                    Some(service_name.clone())
                }
                _ => None,
            },
            config_group: Some(release.group.clone()),
            config_key: Some(release.key.clone()),
        });
        self.record_idempotency(command.idempotency.as_ref(), release.release_id.clone());
        Ok(release)
    }

    async fn retrieve_effective_config(
        &self,
        query: RetrieveEffectiveConfigQuery,
    ) -> DiscoveryResult<EffectiveConfig> {
        validate_non_empty("namespace", &query.namespace)?;
        validate_non_empty("environment", &query.environment)?;
        validate_non_empty("application", &query.application)?;
        validate_non_empty("service_name", &query.service_name)?;
        validate_non_empty("group", &query.group)?;

        let mut values = std::collections::BTreeMap::new();
        let mut revision = 0;

        for release in self.releases.iter().filter(|release| {
            release.namespace == query.namespace
                && release.environment == query.environment
                && release.group == query.group
                && release
                    .scope
                    .applies_to(&query.application, &query.service_name)
        }) {
            revision = revision.max(release.revision);
            let specificity = release.scope.specificity();
            let replace = values
                .get(&release.key)
                .map(|current: &EffectiveConfigValue| {
                    specificity > current.source_specificity
                        || (specificity == current.source_specificity
                            && release.revision >= current.source_revision)
                })
                .unwrap_or(true);

            if replace {
                values.insert(
                    release.key.clone(),
                    EffectiveConfigValue {
                        value: release.value.clone(),
                        format: release.format.clone(),
                        source_release_id: release.release_id.clone(),
                        source_specificity: specificity,
                        source_revision: release.revision,
                    },
                );
            }
        }

        Ok(EffectiveConfig { revision, values })
    }
}

impl MemoryDiscoveryStore {
    fn replay_config_draft(
        &self,
        command: &CreateConfigDraftCommand,
    ) -> DiscoveryResult<Option<ConfigDraft>> {
        let Some(idempotency) = command.idempotency.as_ref() else {
            return Ok(None);
        };
        let Some(record) = self.idempotency.get(&IdempotencyKey {
            operation_id: idempotency.operation_id.clone(),
            key: idempotency.key.clone(),
        }) else {
            return Ok(None);
        };
        validate_idempotency_hash(&record.request_hash, &idempotency.request_hash)?;
        let draft = self
            .drafts
            .get(&record.resource_id)
            .cloned()
            .ok_or_else(|| {
                DiscoveryError::InvalidConfig("idempotency draft target is missing".to_string())
            })?;
        Ok(Some(draft))
    }

    fn replay_config_release(
        &self,
        idempotency: Option<&sdkwork_discovery_contract::IdempotencyContext>,
    ) -> DiscoveryResult<Option<ConfigRelease>> {
        let Some(idempotency) = idempotency else {
            return Ok(None);
        };
        let Some(record) = self.idempotency.get(&IdempotencyKey {
            operation_id: idempotency.operation_id.clone(),
            key: idempotency.key.clone(),
        }) else {
            return Ok(None);
        };
        validate_idempotency_hash(&record.request_hash, &idempotency.request_hash)?;
        let release = self
            .releases
            .iter()
            .find(|release| release.release_id == record.resource_id)
            .cloned()
            .ok_or_else(|| {
                DiscoveryError::InvalidConfig("idempotency release target is missing".to_string())
            })?;
        Ok(Some(release))
    }

    fn record_idempotency(
        &mut self,
        idempotency: Option<&sdkwork_discovery_contract::IdempotencyContext>,
        resource_id: String,
    ) {
        let Some(idempotency) = idempotency else {
            return;
        };
        self.idempotency.insert(
            IdempotencyKey {
                operation_id: idempotency.operation_id.clone(),
                key: idempotency.key.clone(),
            },
            IdempotencyRecord {
                request_hash: idempotency.request_hash.clone(),
                resource_id,
            },
        );
    }
}

fn validate_idempotency_hash(stored: &str, provided: &str) -> DiscoveryResult<()> {
    if stored == provided {
        return Ok(());
    }
    Err(DiscoveryError::InvalidArgument(
        "idempotency request hash does not match original request".to_string(),
    ))
}

fn validate_config_draft_command(command: &CreateConfigDraftCommand) -> DiscoveryResult<()> {
    validate_non_empty("namespace", &command.namespace)?;
    validate_non_empty("environment", &command.environment)?;
    validate_non_empty("group", &command.group)?;
    validate_non_empty("key", &command.key)?;
    validate_non_empty("created_by", &command.created_by)?;
    validate_config_scope(&command.scope)?;
    validate_idempotency_context(command.idempotency.as_ref())
}

fn validate_publish_config_command(command: &PublishConfigCommand) -> DiscoveryResult<()> {
    validate_non_empty("draft_id", &command.draft_id)?;
    validate_non_empty("published_by", &command.published_by)?;
    validate_idempotency_context(command.idempotency.as_ref())
}

fn validate_rollback_config_command(command: &RollbackConfigCommand) -> DiscoveryResult<()> {
    validate_non_empty("source_release_id", &command.source_release_id)?;
    validate_non_empty("rolled_back_by", &command.rolled_back_by)?;
    validate_idempotency_context(command.idempotency.as_ref())
}

fn validate_config_scope(scope: &ConfigScope) -> DiscoveryResult<()> {
    match scope {
        ConfigScope::Namespace => Ok(()),
        ConfigScope::Application { application } => validate_non_empty("application", application),
        ConfigScope::Service {
            application,
            service_name,
        } => {
            validate_non_empty("application", application)?;
            validate_non_empty("service_name", service_name)
        }
    }
}

fn validate_idempotency_context(idempotency: Option<&IdempotencyContext>) -> DiscoveryResult<()> {
    let Some(idempotency) = idempotency else {
        return Ok(());
    };
    validate_non_empty("operation_id", &idempotency.operation_id)?;
    validate_non_empty("idempotency key", &idempotency.key)?;
    validate_non_empty("request_hash", &idempotency.request_hash)
}

fn config_application_from_scope(
    scope: &sdkwork_discovery_contract::ConfigScope,
) -> Option<String> {
    match scope {
        sdkwork_discovery_contract::ConfigScope::Namespace => None,
        sdkwork_discovery_contract::ConfigScope::Application { application }
        | sdkwork_discovery_contract::ConfigScope::Service { application, .. } => {
            Some(application.clone())
        }
    }
}
