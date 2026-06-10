use std::collections::BTreeMap;

use async_trait::async_trait;
use sdkwork_discovery_contract::{
    ConfigDraft, ConfigRelease, ConfigScope, CreateConfigDraftCommand, DiscoveryError,
    DiscoveryEventKind, DiscoveryResult, EffectiveConfig, EffectiveConfigValue, IdempotencyContext,
    PublishConfigCommand, RetrieveEffectiveConfigQuery, RollbackConfigCommand,
};
use sdkwork_discovery_storage_contract::ConfigStore;
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, Sqlite, Transaction};

use crate::codec::{
    draft_from_row, encode_config_format, encode_config_scope, new_prefixed_id, new_uuid,
    release_from_row, sqlx_error,
};
use crate::hash::content_hash;
use crate::registry::{insert_event, next_revision};
use crate::sql;
use crate::store::SqliteDiscoveryStore;
use crate::validation::{to_i64, validate_non_empty};

#[async_trait]
impl ConfigStore for SqliteDiscoveryStore {
    async fn create_config_draft(
        &mut self,
        command: CreateConfigDraftCommand,
    ) -> DiscoveryResult<ConfigDraft> {
        validate_config_draft_command(&command)?;
        if let Some(draft) = replay_config_draft(self.pool(), command.idempotency.as_ref()).await? {
            return Ok(draft);
        }

        let draft_id = new_prefixed_id("draft");
        let content_hash = content_hash(&command.value);
        let (scope_kind, scope_application, scope_service_name) =
            encode_config_scope(&command.scope);

        sqlx::query(sql::INSERT_CONFIG_DRAFT)
            .bind(new_uuid())
            .bind(&draft_id)
            .bind(&command.namespace)
            .bind(&command.environment)
            .bind(&command.group)
            .bind(&command.key)
            .bind(encode_config_format(&command.format))
            .bind(&command.value)
            .bind(scope_kind)
            .bind(scope_application)
            .bind(scope_service_name)
            .bind(&command.created_by)
            .bind(&content_hash)
            .execute(self.pool())
            .await
            .map_err(sqlx_error)?;
        record_idempotency(
            self.pool(),
            command.idempotency.as_ref(),
            "config_draft",
            &draft_id,
        )
        .await?;

        Ok(ConfigDraft {
            draft_id,
            namespace: command.namespace,
            environment: command.environment,
            group: command.group,
            key: command.key,
            format: command.format,
            value: command.value,
            scope: command.scope,
            created_by: command.created_by,
            content_hash,
            published: false,
        })
    }

    async fn publish_config(
        &mut self,
        command: PublishConfigCommand,
    ) -> DiscoveryResult<ConfigRelease> {
        validate_publish_config_command(&command)?;

        let mut transaction = self.pool().begin().await.map_err(sqlx_error)?;
        if let Some(release) =
            replay_config_release_in_transaction(&mut transaction, command.idempotency.as_ref())
                .await?
        {
            transaction.commit().await.map_err(sqlx_error)?;
            return Ok(release);
        }
        let draft_row = sqlx::query(sql::SELECT_CONFIG_DRAFT_FOR_PUBLISH)
            .bind(&command.draft_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(sqlx_error)?
            .ok_or_else(|| DiscoveryError::NotFound("config draft not found".to_string()))?;
        let draft = draft_from_row(&draft_row)?;
        if draft.published {
            return Err(DiscoveryError::AlreadyPublished(
                "config draft is already published".to_string(),
            ));
        }

        let revision =
            next_revision(&mut transaction, &draft.namespace, &draft.environment).await?;
        let release_id = new_prefixed_id("release");
        sqlx::query(sql::INSERT_CONFIG_RELEASE_FROM_DRAFT)
            .bind(new_uuid())
            .bind(&release_id)
            .bind(&command.published_by)
            .bind(to_i64("published_at_ms", command.now_ms)?)
            .bind(to_i64("revision", revision)?)
            .bind(&command.draft_id)
            .execute(&mut *transaction)
            .await
            .map_err(sqlx_error)?;
        sqlx::query(sql::MARK_DRAFT_PUBLISHED)
            .bind(&command.draft_id)
            .execute(&mut *transaction)
            .await
            .map_err(sqlx_error)?;
        insert_event(
            &mut transaction,
            revision,
            &draft.namespace,
            &draft.environment,
            DiscoveryEventKind::ConfigPublished,
            &release_id,
            config_application_from_scope(&draft.scope),
        )
        .await?;
        record_idempotency_in_transaction(
            &mut transaction,
            command.idempotency.as_ref(),
            "config_release",
            &release_id,
        )
        .await?;
        transaction.commit().await.map_err(sqlx_error)?;

        Ok(ConfigRelease {
            release_id,
            draft_id: draft.draft_id,
            namespace: draft.namespace,
            environment: draft.environment,
            group: draft.group,
            key: draft.key,
            format: draft.format,
            value: draft.value,
            scope: draft.scope,
            content_hash: draft.content_hash,
            published_by: command.published_by,
            published_at_ms: command.now_ms,
            revision,
        })
    }

    async fn rollback_config(
        &mut self,
        command: RollbackConfigCommand,
    ) -> DiscoveryResult<ConfigRelease> {
        validate_rollback_config_command(&command)?;

        let mut transaction = self.pool().begin().await.map_err(sqlx_error)?;
        if let Some(release) =
            replay_config_release_in_transaction(&mut transaction, command.idempotency.as_ref())
                .await?
        {
            transaction.commit().await.map_err(sqlx_error)?;
            return Ok(release);
        }
        let source_row = sqlx::query(sql::SELECT_CONFIG_RELEASE_FOR_ROLLBACK)
            .bind(&command.source_release_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(sqlx_error)?
            .ok_or_else(|| DiscoveryError::NotFound("config release not found".to_string()))?;
        let source = release_from_row(&source_row)?;
        let revision =
            next_revision(&mut transaction, &source.namespace, &source.environment).await?;
        let release_id = new_prefixed_id("release");
        let (scope_kind, scope_application, scope_service_name) =
            encode_config_scope(&source.scope);

        sqlx::query(sql::INSERT_CONFIG_RELEASE_FROM_RELEASE)
            .bind(new_uuid())
            .bind(&release_id)
            .bind(&source.draft_id)
            .bind(&source.namespace)
            .bind(&source.environment)
            .bind(&source.group)
            .bind(&source.key)
            .bind(encode_config_format(&source.format))
            .bind(&source.value)
            .bind(scope_kind)
            .bind(scope_application)
            .bind(scope_service_name)
            .bind(&source.content_hash)
            .bind(&command.rolled_back_by)
            .bind(to_i64("published_at_ms", command.now_ms)?)
            .bind(to_i64("revision", revision)?)
            .execute(&mut *transaction)
            .await
            .map_err(sqlx_error)?;
        insert_event(
            &mut transaction,
            revision,
            &source.namespace,
            &source.environment,
            DiscoveryEventKind::ConfigRolledBack,
            &release_id,
            config_application_from_scope(&source.scope),
        )
        .await?;
        record_idempotency_in_transaction(
            &mut transaction,
            command.idempotency.as_ref(),
            "config_release",
            &release_id,
        )
        .await?;
        transaction.commit().await.map_err(sqlx_error)?;

        Ok(ConfigRelease {
            release_id,
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
        })
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

        let rows = sqlx::query(sql::SELECT_EFFECTIVE_RELEASES)
            .bind(&query.namespace)
            .bind(&query.environment)
            .bind(&query.group)
            .bind(&query.application)
            .bind(&query.application)
            .bind(&query.service_name)
            .fetch_all(self.pool())
            .await
            .map_err(sqlx_error)?;

        let mut values = BTreeMap::new();
        let mut revision = 0;

        for row in rows {
            let release = release_from_row(&row)?;
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
                        value: release.value,
                        format: release.format,
                        source_release_id: release.release_id,
                        source_specificity: specificity,
                        source_revision: release.revision,
                    },
                );
            }
        }

        Ok(EffectiveConfig { revision, values })
    }
}

async fn replay_config_draft(
    pool: &sqlx::SqlitePool,
    idempotency: Option<&IdempotencyContext>,
) -> DiscoveryResult<Option<ConfigDraft>> {
    let Some(record) = select_idempotency_record(pool, idempotency).await? else {
        return Ok(None);
    };
    validate_idempotency_record(&record, "config_draft", idempotency)?;
    let row = sqlx::query(sql::SELECT_CONFIG_DRAFT_FOR_PUBLISH)
        .bind(&record.resource_id)
        .fetch_optional(pool)
        .await
        .map_err(sqlx_error)?
        .ok_or_else(|| {
            DiscoveryError::InvalidConfig("idempotency draft target is missing".to_string())
        })?;
    Ok(Some(draft_from_row(&row)?))
}

async fn replay_config_release_in_transaction(
    transaction: &mut Transaction<'_, Sqlite>,
    idempotency: Option<&IdempotencyContext>,
) -> DiscoveryResult<Option<ConfigRelease>> {
    let Some(record) = select_idempotency_record_in_transaction(transaction, idempotency).await?
    else {
        return Ok(None);
    };
    validate_idempotency_record(&record, "config_release", idempotency)?;
    let row = sqlx::query(sql::SELECT_CONFIG_RELEASE_FOR_ROLLBACK)
        .bind(&record.resource_id)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(sqlx_error)?
        .ok_or_else(|| {
            DiscoveryError::InvalidConfig("idempotency release target is missing".to_string())
        })?;
    Ok(Some(release_from_row(&row)?))
}

async fn select_idempotency_record(
    pool: &sqlx::SqlitePool,
    idempotency: Option<&IdempotencyContext>,
) -> DiscoveryResult<Option<StoredIdempotencyRecord>> {
    let Some(idempotency) = idempotency else {
        return Ok(None);
    };
    let row = sqlx::query(sql::SELECT_IDEMPOTENCY_RECORD)
        .bind(&idempotency.operation_id)
        .bind(&idempotency.key)
        .fetch_optional(pool)
        .await
        .map_err(sqlx_error)?;
    row.map(|row| stored_idempotency_record_from_row(&row))
        .transpose()
}

async fn select_idempotency_record_in_transaction(
    transaction: &mut Transaction<'_, Sqlite>,
    idempotency: Option<&IdempotencyContext>,
) -> DiscoveryResult<Option<StoredIdempotencyRecord>> {
    let Some(idempotency) = idempotency else {
        return Ok(None);
    };
    let row = sqlx::query(sql::SELECT_IDEMPOTENCY_RECORD)
        .bind(&idempotency.operation_id)
        .bind(&idempotency.key)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(sqlx_error)?;
    row.map(|row| stored_idempotency_record_from_row(&row))
        .transpose()
}

async fn record_idempotency(
    pool: &sqlx::SqlitePool,
    idempotency: Option<&IdempotencyContext>,
    resource_kind: &str,
    resource_id: &str,
) -> DiscoveryResult<()> {
    let Some(idempotency) = idempotency else {
        return Ok(());
    };
    sqlx::query(sql::INSERT_IDEMPOTENCY_RECORD)
        .bind(new_uuid())
        .bind(&idempotency.operation_id)
        .bind(&idempotency.key)
        .bind(&idempotency.request_hash)
        .bind(resource_kind)
        .bind(resource_id)
        .execute(pool)
        .await
        .map_err(sqlx_error)?;
    Ok(())
}

async fn record_idempotency_in_transaction(
    transaction: &mut Transaction<'_, Sqlite>,
    idempotency: Option<&IdempotencyContext>,
    resource_kind: &str,
    resource_id: &str,
) -> DiscoveryResult<()> {
    let Some(idempotency) = idempotency else {
        return Ok(());
    };
    sqlx::query(sql::INSERT_IDEMPOTENCY_RECORD)
        .bind(new_uuid())
        .bind(&idempotency.operation_id)
        .bind(&idempotency.key)
        .bind(&idempotency.request_hash)
        .bind(resource_kind)
        .bind(resource_id)
        .execute(&mut **transaction)
        .await
        .map_err(sqlx_error)?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StoredIdempotencyRecord {
    request_hash: String,
    resource_kind: String,
    resource_id: String,
}

fn stored_idempotency_record_from_row(row: &SqliteRow) -> DiscoveryResult<StoredIdempotencyRecord> {
    Ok(StoredIdempotencyRecord {
        request_hash: row.try_get("request_hash").map_err(sqlx_error)?,
        resource_kind: row.try_get("resource_kind").map_err(sqlx_error)?,
        resource_id: row.try_get("resource_id").map_err(sqlx_error)?,
    })
}

fn validate_idempotency_record(
    record: &StoredIdempotencyRecord,
    expected_resource_kind: &str,
    idempotency: Option<&IdempotencyContext>,
) -> DiscoveryResult<()> {
    let idempotency = idempotency.ok_or_else(|| {
        DiscoveryError::InvalidArgument("missing idempotency context".to_string())
    })?;
    if record.request_hash != idempotency.request_hash {
        return Err(DiscoveryError::InvalidArgument(
            "idempotency request hash does not match original request".to_string(),
        ));
    }
    if record.resource_kind != expected_resource_kind {
        return Err(DiscoveryError::InvalidConfig(
            "idempotency record resource kind does not match operation".to_string(),
        ));
    }
    Ok(())
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

fn config_application_from_scope(scope: &ConfigScope) -> Option<&str> {
    match scope {
        ConfigScope::Namespace => None,
        ConfigScope::Application { application } | ConfigScope::Service { application, .. } => {
            Some(application)
        }
    }
}
