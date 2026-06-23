use async_trait::async_trait;
use sdkwork_discovery_contract::{
    BatchOperationError, BatchRegisterResult, DeregisterInstanceResult, DiscoverInstancesQuery,
    DiscoverInstancesResult, DiscoveryError, DiscoveryEventKind, DiscoveryResult,
    ListServicesQuery, ListServicesResult, RegisterInstanceCommand, RegisterInstanceResult,
    RenewLeaseCommand, RenewLeaseResult, ReportInstanceStatusCommand, ReportInstanceStatusResult,
    RetrieveInstanceQuery, ServiceInstance, ServiceSummary,
};
use sdkwork_discovery_storage_contract::RegistryStore;
use sqlx::{Postgres, Row, Transaction};

use crate::codec::{
    bind_priority, bind_weight, encode_event_kind, encode_instance_status,
    health_check_state_to_json, health_check_to_json, metadata_to_json, new_prefixed_id, new_uuid,
    service_instance_from_row, sqlx_error,
};
use crate::sql;
use crate::store::PostgresDiscoveryStore;
use crate::validation::{i64_to_u64, to_i64, usize_to_i64, validate_non_empty};

#[async_trait]
impl RegistryStore for PostgresDiscoveryStore {
    async fn current_revision(&self) -> DiscoveryResult<u64> {
        let row = sqlx::query(sql::SELECT_CURRENT_REVISION)
            .fetch_one(self.pool())
            .await
            .map_err(sqlx_error)?;
        let revision: i64 = row.try_get("current_revision").map_err(sqlx_error)?;
        Ok(revision as u64)
    }

    async fn register_instance(
        &mut self,
        command: RegisterInstanceCommand,
    ) -> DiscoveryResult<RegisterInstanceResult> {
        validate_register_command(&command)?;

        let expires_at_ms = lease_expires_at_ms(
            command.now_ms,
            command.lease_ttl_seconds,
            command.persistent,
        )?;
        let expires_at_ms_i64 = to_i64("expires_at_ms", expires_at_ms)?;
        let metadata_json = metadata_to_json(&command.metadata)?;
        let health_check_json = health_check_to_json(&command.health_check)?;
        let health_check_state_json = health_check_state_to_json(
            &sdkwork_discovery_contract::HealthCheckRuntimeState::default(),
        )?;
        let status = encode_instance_status(&command.status);
        let weight = bind_weight(command.weight)?;
        let priority = bind_priority(command.priority)?;

        let mut transaction = self.pool().begin().await.map_err(sqlx_error)?;
        let existing_row = sqlx::query(sql::SELECT_EXISTING_INSTANCE_LEASE)
            .bind(&command.namespace)
            .bind(&command.environment)
            .bind(&command.service_name)
            .bind(&command.instance_id)
            .bind(to_i64("now_ms", command.now_ms)?)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(sqlx_error)?;

        if let Some(expected_revision) = command.expected_revision {
            if let Some(row) = &existing_row {
                let current_revision =
                    i64_to_u64("revision", row.try_get("revision").map_err(sqlx_error)?)?;
                if current_revision != expected_revision {
                    return Err(DiscoveryError::Conflict(format!(
                        "expected revision {expected_revision} but found {current_revision}"
                    )));
                }
            }
        }

        let existing_lease: Option<String> = existing_row
            .as_ref()
            .map(|row| row.try_get("lease_id"))
            .transpose()
            .map_err(sqlx_error)?;
        let was_update = existing_lease.is_some();
        let lease_id = existing_lease.unwrap_or_else(|| new_prefixed_id("lease"));
        let revision =
            next_revision(&mut transaction, &command.namespace, &command.environment).await?;
        let revision_i64 = to_i64("revision", revision)?;

        let row = sqlx::query(sql::REGISTER_INSTANCE)
            .bind(new_uuid())
            .bind(&command.namespace)
            .bind(&command.environment)
            .bind(&command.service_name)
            .bind(&command.instance_id)
            .bind(&command.endpoint)
            .bind(&command.protocol)
            .bind(&command.version)
            .bind(&command.region)
            .bind(&command.zone)
            .bind(weight)
            .bind(priority)
            .bind(status)
            .bind(&metadata_json)
            .bind(&lease_id)
            .bind(expires_at_ms_i64)
            .bind(revision_i64)
            .bind(&health_check_json)
            .bind(&health_check_state_json)
            .fetch_one(&mut *transaction)
            .await
            .map_err(sqlx_error)?;

        insert_event(
            &mut transaction,
            revision,
            &command.namespace,
            &command.environment,
            if was_update {
                DiscoveryEventKind::InstanceUpdated
            } else {
                DiscoveryEventKind::InstanceRegistered
            },
            &command.instance_id,
            None,
        )
        .await?;
        transaction.commit().await.map_err(sqlx_error)?;

        Ok(RegisterInstanceResult {
            lease_id: row.try_get("lease_id").map_err(sqlx_error)?,
            namespace: row.try_get("namespace").map_err(sqlx_error)?,
            environment: row.try_get("environment").map_err(sqlx_error)?,
            service_name: row.try_get("service_name").map_err(sqlx_error)?,
            instance_id: row.try_get("instance_id").map_err(sqlx_error)?,
            expires_at_ms: i64_to_u64(
                "expires_at_ms",
                row.try_get("expires_at_ms").map_err(sqlx_error)?,
            )?,
            revision: i64_to_u64("revision", row.try_get("revision").map_err(sqlx_error)?)?,
        })
    }

    async fn batch_register_instances(
        &mut self,
        commands: Vec<RegisterInstanceCommand>,
    ) -> DiscoveryResult<BatchRegisterResult> {
        let mut results = Vec::with_capacity(commands.len());
        let mut errors = Vec::new();

        for (index, command) in commands.into_iter().enumerate() {
            match self.register_instance(command).await {
                Ok(result) => results.push(result),
                Err(e) => errors.push(BatchOperationError {
                    index,
                    error_code: e.kind_string().to_string(),
                    error_message: e.to_string(),
                }),
            }
        }

        Ok(BatchRegisterResult { results, errors })
    }

    async fn renew_lease(
        &mut self,
        command: RenewLeaseCommand,
    ) -> DiscoveryResult<RenewLeaseResult> {
        validate_non_empty("lease_id", &command.lease_id)?;
        if command.lease_ttl_seconds == 0 {
            return Err(DiscoveryError::InvalidArgument(
                "lease ttl must be greater than zero".to_string(),
            ));
        }

        let expires_at_ms = command
            .now_ms
            .checked_add(
                command
                    .lease_ttl_seconds
                    .checked_mul(1_000)
                    .ok_or_else(|| {
                        DiscoveryError::InvalidArgument(
                            "lease ttl overflows milliseconds".to_string(),
                        )
                    })?,
            )
            .ok_or_else(|| {
                DiscoveryError::InvalidArgument("lease expiration overflows".to_string())
            })?;
        let expires_at_ms_i64 = to_i64("expires_at_ms", expires_at_ms)?;

        let mut transaction = self.pool().begin().await.map_err(sqlx_error)?;
        let lease_scope = sqlx::query(
            "SELECT namespace, environment FROM discovery_service_instance \
             WHERE lease_id = $1 AND deleted_at IS NULL AND expires_at_ms > $2",
        )
        .bind(&command.lease_id)
        .bind(to_i64("now_ms", command.now_ms)?)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(sqlx_error)?
        .ok_or_else(|| DiscoveryError::NotFound("lease not found".to_string()))?;
        let namespace: String = lease_scope.try_get("namespace").map_err(sqlx_error)?;
        let environment: String = lease_scope.try_get("environment").map_err(sqlx_error)?;
        let revision = next_revision(&mut transaction, &namespace, &environment).await?;
        let revision_i64 = to_i64("revision", revision)?;

        let row = sqlx::query(sql::RENEW_LEASE)
            .bind(&command.lease_id)
            .bind(expires_at_ms_i64)
            .bind(revision_i64)
            .bind(to_i64("now_ms", command.now_ms)?)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(sqlx_error)?
            .ok_or_else(|| DiscoveryError::NotFound("lease not found".to_string()))?;
        let resource_id: String = row.try_get("instance_id").map_err(sqlx_error)?;
        insert_event(
            &mut transaction,
            revision,
            &namespace,
            &environment,
            DiscoveryEventKind::InstanceRenewed,
            &resource_id,
            None,
        )
        .await?;
        transaction.commit().await.map_err(sqlx_error)?;

        Ok(RenewLeaseResult {
            lease_id: row.try_get("lease_id").map_err(sqlx_error)?,
            namespace: row.try_get("namespace").map_err(sqlx_error)?,
            environment: row.try_get("environment").map_err(sqlx_error)?,
            service_name: row.try_get("service_name").map_err(sqlx_error)?,
            instance_id: row.try_get("instance_id").map_err(sqlx_error)?,
            expires_at_ms: i64_to_u64(
                "expires_at_ms",
                row.try_get("expires_at_ms").map_err(sqlx_error)?,
            )?,
            revision: i64_to_u64("revision", row.try_get("revision").map_err(sqlx_error)?)?,
        })
    }

    async fn report_instance_status(
        &mut self,
        command: ReportInstanceStatusCommand,
    ) -> DiscoveryResult<ReportInstanceStatusResult> {
        validate_non_empty("namespace", &command.namespace)?;
        validate_non_empty("environment", &command.environment)?;
        validate_non_empty("service_name", &command.service_name)?;
        validate_non_empty("instance_id", &command.instance_id)?;

        let status = encode_instance_status(&command.status);
        let mut transaction = self.pool().begin().await.map_err(sqlx_error)?;
        let current_row = sqlx::query(sql::SELECT_ACTIVE_INSTANCE_FOR_STATUS)
            .bind(&command.namespace)
            .bind(&command.environment)
            .bind(&command.service_name)
            .bind(&command.instance_id)
            .bind(to_i64("now_ms", command.now_ms)?)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(sqlx_error)?
            .ok_or_else(|| DiscoveryError::NotFound("instance not found".to_string()))?;

        if let Some(expected_revision) = command.expected_revision {
            let current_revision = i64_to_u64(
                "revision",
                current_row.try_get("revision").map_err(sqlx_error)?,
            )?;
            if current_revision != expected_revision {
                return Err(DiscoveryError::Conflict(format!(
                    "expected revision {expected_revision} but found {current_revision}"
                )));
            }
        }

        let revision =
            next_revision(&mut transaction, &command.namespace, &command.environment).await?;
        let row = sqlx::query(sql::REPORT_INSTANCE_STATUS)
            .bind(&command.namespace)
            .bind(&command.environment)
            .bind(&command.service_name)
            .bind(&command.instance_id)
            .bind(status)
            .bind(to_i64("revision", revision)?)
            .bind(to_i64("now_ms", command.now_ms)?)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(sqlx_error)?
            .ok_or_else(|| DiscoveryError::NotFound("instance not found".to_string()))?;
        insert_event(
            &mut transaction,
            revision,
            &command.namespace,
            &command.environment,
            DiscoveryEventKind::InstanceStatusReported,
            &command.instance_id,
            None,
        )
        .await?;
        transaction.commit().await.map_err(sqlx_error)?;

        Ok(ReportInstanceStatusResult {
            revision: i64_to_u64("revision", row.try_get("revision").map_err(sqlx_error)?)?,
            status: command.status,
        })
    }

    async fn deregister_instance(
        &mut self,
        namespace: &str,
        environment: &str,
        service_name: &str,
        instance_id: &str,
        now_ms: u64,
    ) -> DiscoveryResult<DeregisterInstanceResult> {
        validate_non_empty("namespace", namespace)?;
        validate_non_empty("environment", environment)?;
        validate_non_empty("service_name", service_name)?;
        validate_non_empty("instance_id", instance_id)?;

        let mut transaction = self.pool().begin().await.map_err(sqlx_error)?;
        let active = sqlx::query(sql::SELECT_ACTIVE_INSTANCE_FOR_DEREGISTER)
            .bind(namespace)
            .bind(environment)
            .bind(service_name)
            .bind(instance_id)
            .bind(to_i64("now_ms", now_ms)?)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(sqlx_error)?;
        if active.is_none() {
            transaction.commit().await.map_err(sqlx_error)?;
            return Ok(DeregisterInstanceResult {
                namespace: namespace.to_string(),
                environment: environment.to_string(),
                service_name: service_name.to_string(),
                instance_id: instance_id.to_string(),
                revision: 0,
                deregistered: false,
            });
        }
        let revision = next_revision(&mut transaction, namespace, environment).await?;
        let row = sqlx::query(sql::DEREGISTER_INSTANCE)
            .bind(namespace)
            .bind(environment)
            .bind(service_name)
            .bind(instance_id)
            .bind(to_i64("revision", revision)?)
            .bind(to_i64("now_ms", now_ms)?)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(sqlx_error)?;
        let result = if let Some(row) = row {
            insert_event(
                &mut transaction,
                revision,
                namespace,
                environment,
                DiscoveryEventKind::InstanceDeregistered,
                instance_id,
                None,
            )
            .await?;
            DeregisterInstanceResult {
                namespace: row.try_get("namespace").map_err(sqlx_error)?,
                environment: row.try_get("environment").map_err(sqlx_error)?,
                service_name: row.try_get("service_name").map_err(sqlx_error)?,
                instance_id: row.try_get("instance_id").map_err(sqlx_error)?,
                revision: i64_to_u64("revision", row.try_get("revision").map_err(sqlx_error)?)?,
                deregistered: true,
            }
        } else {
            DeregisterInstanceResult {
                namespace: namespace.to_string(),
                environment: environment.to_string(),
                service_name: service_name.to_string(),
                instance_id: instance_id.to_string(),
                revision: 0,
                deregistered: false,
            }
        };
        transaction.commit().await.map_err(sqlx_error)?;

        Ok(result)
    }

    async fn batch_deregister_instances(
        &mut self,
        namespace: &str,
        environment: &str,
        service_name: &str,
        instance_ids: Vec<String>,
        now_ms: u64,
    ) -> DiscoveryResult<Vec<DeregisterInstanceResult>> {
        let mut results = Vec::with_capacity(instance_ids.len());
        for instance_id in instance_ids {
            let result = self
                .deregister_instance(namespace, environment, service_name, &instance_id, now_ms)
                .await?;
            results.push(result);
        }
        Ok(results)
    }

    async fn expire_instances(
        &mut self,
        now_ms: u64,
        max_instances: usize,
    ) -> DiscoveryResult<Vec<DeregisterInstanceResult>> {
        if max_instances == 0 {
            return Err(DiscoveryError::InvalidArgument(
                "max_instances must be greater than zero".to_string(),
            ));
        }

        let now_ms_i64 = to_i64("now_ms", now_ms)?;
        let mut transaction = self.pool().begin().await.map_err(sqlx_error)?;
        let expired_rows = sqlx::query(sql::SELECT_EXPIRED_INSTANCES)
            .bind(now_ms_i64)
            .bind(usize_to_i64("max_instances", max_instances)?)
            .fetch_all(&mut *transaction)
            .await
            .map_err(sqlx_error)?;

        let mut expired = Vec::with_capacity(expired_rows.len());
        for expired_row in expired_rows {
            let namespace: String = expired_row.try_get("namespace").map_err(sqlx_error)?;
            let environment: String = expired_row.try_get("environment").map_err(sqlx_error)?;
            let service_name: String = expired_row.try_get("service_name").map_err(sqlx_error)?;
            let instance_id: String = expired_row.try_get("instance_id").map_err(sqlx_error)?;
            let revision = next_revision(&mut transaction, &namespace, &environment).await?;
            let row = sqlx::query(sql::EXPIRE_INSTANCE)
                .bind(&namespace)
                .bind(&environment)
                .bind(&service_name)
                .bind(&instance_id)
                .bind(to_i64("revision", revision)?)
                .bind(now_ms_i64)
                .fetch_optional(&mut *transaction)
                .await
                .map_err(sqlx_error)?;

            if let Some(row) = row {
                insert_event(
                    &mut transaction,
                    revision,
                    &namespace,
                    &environment,
                    DiscoveryEventKind::InstanceDeregistered,
                    &instance_id,
                    None,
                )
                .await?;
                expired.push(DeregisterInstanceResult {
                    namespace: row.try_get("namespace").map_err(sqlx_error)?,
                    environment: row.try_get("environment").map_err(sqlx_error)?,
                    service_name: row.try_get("service_name").map_err(sqlx_error)?,
                    instance_id: row.try_get("instance_id").map_err(sqlx_error)?,
                    revision: i64_to_u64("revision", row.try_get("revision").map_err(sqlx_error)?)?,
                    deregistered: true,
                });
            }
        }
        transaction.commit().await.map_err(sqlx_error)?;

        Ok(expired)
    }

    async fn discover_instances(
        &self,
        query: DiscoverInstancesQuery,
        now_ms: u64,
    ) -> DiscoveryResult<DiscoverInstancesResult> {
        validate_non_empty("namespace", &query.namespace)?;
        validate_non_empty("environment", &query.environment)?;
        validate_non_empty("service_name", &query.service_name)?;
        validate_optional_filter("protocol", query.protocol.as_deref())?;

        let rows = sqlx::query(sql::DISCOVER_INSTANCES_PREFIX)
            .bind(&query.namespace)
            .bind(&query.environment)
            .bind(&query.service_name)
            .bind(to_i64("now_ms", now_ms)?)
            .bind(query.protocol.as_deref())
            .bind(query.healthy_only)
            .fetch_all(self.pool())
            .await
            .map_err(sqlx_error)?;

        let mut revision = 0;
        let mut instances = Vec::with_capacity(rows.len());
        for row in rows {
            let instance = service_instance_from_row(&row)?;
            revision = revision.max(instance.revision);
            instances.push(instance);
        }

        Ok(sdkwork_discovery_contract::finalize_discover_instances(
            instances, &query, revision,
        ))
    }

    async fn retrieve_instance(
        &self,
        query: RetrieveInstanceQuery,
        now_ms: u64,
    ) -> DiscoveryResult<Option<ServiceInstance>> {
        validate_non_empty("namespace", &query.namespace)?;
        validate_non_empty("environment", &query.environment)?;
        validate_non_empty("service_name", &query.service_name)?;
        validate_non_empty("instance_id", &query.instance_id)?;

        let row = sqlx::query(sql::RETRIEVE_INSTANCE)
            .bind(&query.namespace)
            .bind(&query.environment)
            .bind(&query.service_name)
            .bind(&query.instance_id)
            .bind(to_i64("now_ms", now_ms)?)
            .fetch_optional(self.pool())
            .await
            .map_err(sqlx_error)?;

        row.map(|row| service_instance_from_row(&row)).transpose()
    }

    async fn list_services(
        &self,
        query: ListServicesQuery,
        now_ms: u64,
    ) -> DiscoveryResult<ListServicesResult> {
        validate_non_empty("namespace", &query.namespace)?;
        validate_non_empty("environment", &query.environment)?;

        let rows = sqlx::query(sql::LIST_SERVICES)
            .bind(&query.namespace)
            .bind(&query.environment)
            .bind(to_i64("now_ms", now_ms)?)
            .fetch_all(self.pool())
            .await
            .map_err(sqlx_error)?;

        let mut revision = 0;
        let mut services = Vec::with_capacity(rows.len());
        for row in rows {
            let latest_revision: i64 = row.try_get("latest_revision").map_err(sqlx_error)?;
            let active_instance_count: i64 =
                row.try_get("active_instance_count").map_err(sqlx_error)?;
            let latest_revision = i64_to_u64("latest_revision", latest_revision)?;
            revision = revision.max(latest_revision);
            services.push(ServiceSummary {
                namespace: row.try_get("namespace").map_err(sqlx_error)?,
                environment: row.try_get("environment").map_err(sqlx_error)?,
                service_name: row.try_get("service_name").map_err(sqlx_error)?,
                active_instance_count: crate::validation::i64_to_usize(
                    "active_instance_count",
                    active_instance_count,
                )?,
                latest_revision,
            });
        }

        Ok(sdkwork_discovery_contract::finalize_list_services(
            services, revision, &query,
        ))
    }

    async fn list_active_instances_with_health_check(
        &self,
        now_ms: u64,
    ) -> DiscoveryResult<Vec<ServiceInstance>> {
        let rows = sqlx::query(sql::LIST_HEALTH_CHECK_INSTANCES)
            .bind(to_i64("now_ms", now_ms)?)
            .fetch_all(self.pool())
            .await
            .map_err(sqlx_error)?;
        rows.iter().map(service_instance_from_row).collect()
    }

    async fn update_health_check_state(
        &mut self,
        namespace: &str,
        environment: &str,
        service_name: &str,
        instance_id: &str,
        state: sdkwork_discovery_contract::HealthCheckRuntimeState,
    ) -> DiscoveryResult<()> {
        let state_json = health_check_state_to_json(&state)?;
        let result = sqlx::query(sql::UPDATE_HEALTH_CHECK_STATE)
            .bind(namespace)
            .bind(environment)
            .bind(service_name)
            .bind(instance_id)
            .bind(&state_json)
            .execute(self.pool())
            .await
            .map_err(sqlx_error)?;
        if result.rows_affected() == 0 {
            return Err(DiscoveryError::NotFound("instance not found".to_string()));
        }
        Ok(())
    }
}

fn validate_register_command(command: &RegisterInstanceCommand) -> DiscoveryResult<()> {
    validate_non_empty("namespace", &command.namespace)?;
    validate_non_empty("environment", &command.environment)?;
    validate_non_empty("service_name", &command.service_name)?;
    validate_non_empty("instance_id", &command.instance_id)?;
    validate_non_empty("endpoint", &command.endpoint)?;
    validate_non_empty("protocol", &command.protocol)?;
    validate_non_empty("version", &command.version)?;
    validate_non_empty("region", &command.region)?;
    validate_non_empty("zone", &command.zone)?;
    if command.lease_ttl_seconds == 0 {
        return Err(DiscoveryError::InvalidArgument(
            "lease ttl must be greater than zero".to_string(),
        ));
    }
    Ok(())
}

fn validate_optional_filter(field: &str, value: Option<&str>) -> DiscoveryResult<()> {
    if let Some(value) = value {
        validate_non_empty(field, value)?;
    }
    Ok(())
}

fn lease_expires_at_ms(
    now_ms: u64,
    lease_ttl_seconds: u64,
    persistent: bool,
) -> DiscoveryResult<u64> {
    if persistent {
        return Ok(i64::MAX as u64);
    }

    let ttl_ms = lease_ttl_seconds.checked_mul(1_000).ok_or_else(|| {
        DiscoveryError::InvalidArgument("lease ttl overflows milliseconds".to_string())
    })?;
    now_ms
        .checked_add(ttl_ms)
        .ok_or_else(|| DiscoveryError::InvalidArgument("lease expiration overflows".to_string()))
}

pub(crate) async fn next_revision(
    transaction: &mut Transaction<'_, Postgres>,
    namespace: &str,
    environment: &str,
) -> DiscoveryResult<u64> {
    let row = sqlx::query(sql::NEXT_REVISION)
        .bind(new_uuid())
        .bind(namespace)
        .bind(environment)
        .fetch_one(&mut **transaction)
        .await
        .map_err(sqlx_error)?;
    i64_to_u64(
        "revision",
        row.try_get("current_revision").map_err(sqlx_error)?,
    )
}

pub(crate) async fn insert_event(
    transaction: &mut Transaction<'_, Postgres>,
    revision: u64,
    namespace: &str,
    environment: &str,
    kind: DiscoveryEventKind,
    resource_id: &str,
    config_application: Option<&str>,
) -> DiscoveryResult<()> {
    sqlx::query(sql::INSERT_WATCH_EVENT)
        .bind(new_uuid())
        .bind(to_i64("revision", revision)?)
        .bind(namespace)
        .bind(environment)
        .bind(encode_event_kind(&kind))
        .bind(resource_id)
        .bind(config_application)
        .execute(&mut **transaction)
        .await
        .map_err(sqlx_error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use sdkwork_discovery_contract::{InstanceStatus, RegisterInstanceCommand};

    use super::{lease_expires_at_ms, validate_register_command};

    fn register_command() -> RegisterInstanceCommand {
        RegisterInstanceCommand {
            namespace: "sdkwork".to_string(),
            environment: "development".to_string(),
            service_name: "sdkwork-drive-product".to_string(),
            instance_id: "drive-1".to_string(),
            endpoint: "grpc://127.0.0.1:50051".to_string(),
            protocol: "grpc".to_string(),
            version: "0.1.0".to_string(),
            region: "local".to_string(),
            zone: "local-a".to_string(),
            weight: 100,
            priority: 0,
            status: InstanceStatus::Serving,
            metadata: Default::default(),
            lease_ttl_seconds: 30,
            now_ms: 1_000,
            expected_revision: None,
            persistent: false,
            health_check: None,
        }
    }

    #[test]
    fn persistent_lease_expires_at_max_timestamp() {
        let expires = lease_expires_at_ms(1_000, 30, true).unwrap();
        assert_eq!(expires, i64::MAX as u64);
    }

    #[test]
    fn register_command_rejects_zero_lease_ttl() {
        let mut command = register_command();
        command.lease_ttl_seconds = 0;

        let error = validate_register_command(&command).unwrap_err();
        assert!(error.to_string().contains("lease ttl"));
    }
}
