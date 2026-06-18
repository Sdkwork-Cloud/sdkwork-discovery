use std::collections::BTreeMap;

use async_trait::async_trait;
use sdkwork_discovery_contract::{
    finalize_discover_instances, BatchOperationError, BatchRegisterResult,
    DeregisterInstanceResult, DiscoverInstancesQuery, DiscoverInstancesResult, DiscoveryError,
    DiscoveryEvent, DiscoveryEventKind, DiscoveryResult, ListServicesQuery, ListServicesResult,
    RegisterInstanceCommand, RegisterInstanceResult, RenewLeaseCommand, RenewLeaseResult,
    ReportInstanceStatusCommand, ReportInstanceStatusResult, RetrieveInstanceQuery,
    ServiceInstance, ServiceSummary,
};
use sdkwork_discovery_storage_contract::RegistryStore;

use crate::store::{InstanceKey, MemoryDiscoveryStore};
use crate::validation::validate_non_empty;

#[async_trait]
impl RegistryStore for MemoryDiscoveryStore {
    async fn current_revision(&self) -> DiscoveryResult<u64> {
        Ok(self.revision)
    }

    async fn register_instance(
        &mut self,
        command: RegisterInstanceCommand,
    ) -> DiscoveryResult<RegisterInstanceResult> {
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

        let key = InstanceKey {
            namespace: command.namespace.clone(),
            environment: command.environment.clone(),
            service_name: command.service_name.clone(),
            instance_id: command.instance_id.clone(),
        };

        // CAS check: if expected_revision is set, verify it matches current revision
        if let Some(expected_revision) = command.expected_revision {
            if let Some(existing) = self.instances.get(&key) {
                if existing.revision != expected_revision {
                    return Err(DiscoveryError::Conflict(format!(
                        "expected revision {} but found {}",
                        expected_revision, existing.revision
                    )));
                }
            }
        }

        let existing_lease_id = self.instances.get(&key).and_then(|instance| {
            if instance.expires_at_ms > command.now_ms {
                Some(instance.lease_id.clone())
            } else {
                None
            }
        });
        let was_update = existing_lease_id.is_some();
        let lease_id = existing_lease_id.unwrap_or_else(|| self.next_id("lease"));
        let revision = self.next_revision();
        let expires_at_ms = if command.persistent {
            u64::MAX
        } else {
            lease_expires_at_ms(command.now_ms, command.lease_ttl_seconds)?
        };
        let event_namespace = command.namespace.clone();
        let event_environment = command.environment.clone();
        let event_service_name = command.service_name.clone();
        let event_resource_id = command.instance_id.clone();

        let instance = ServiceInstance {
            namespace: command.namespace,
            environment: command.environment,
            service_name: command.service_name,
            instance_id: command.instance_id,
            endpoint: command.endpoint,
            protocol: command.protocol,
            version: command.version,
            region: command.region,
            zone: command.zone,
            weight: command.weight,
            priority: command.priority,
            status: command.status,
            metadata: command.metadata,
            lease_id: lease_id.clone(),
            expires_at_ms,
            revision,
            health_check: command.health_check.clone(),
            health_check_state: sdkwork_discovery_contract::HealthCheckRuntimeState::default(),
        };
        if let Some(previous) = self.instances.get(&key) {
            if previous.lease_id != lease_id {
                self.lease_index.remove(&previous.lease_id);
            }
        }
        self.lease_index.insert(lease_id.clone(), key.clone());
        self.instances.insert(key, instance);
        self.events.push(DiscoveryEvent {
            revision,
            namespace: event_namespace.clone(),
            environment: event_environment.clone(),
            kind: if was_update {
                DiscoveryEventKind::InstanceUpdated
            } else {
                DiscoveryEventKind::InstanceRegistered
            },
            resource_id: event_resource_id.clone(),
            service_name: Some(event_service_name.clone()),
            config_group: None,
            config_key: None,
            config_application: None,
        });

        Ok(RegisterInstanceResult {
            lease_id,
            namespace: event_namespace,
            environment: event_environment,
            service_name: event_service_name,
            instance_id: event_resource_id,
            revision,
            expires_at_ms,
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
        let key = self
            .lease_index
            .get(&command.lease_id)
            .cloned()
            .ok_or_else(|| DiscoveryError::NotFound("lease not found".to_string()))?;
        let expires_at_ms = lease_expires_at_ms(command.now_ms, command.lease_ttl_seconds)?;
        let current_expires_at_ms = self
            .instances
            .get(&key)
            .ok_or_else(|| DiscoveryError::NotFound("instance not found".to_string()))?;
        if current_expires_at_ms.expires_at_ms <= command.now_ms {
            self.lease_index.remove(&command.lease_id);
            return Err(DiscoveryError::NotFound("lease not found".to_string()));
        }
        let revision = self.next_revision();
        let instance = self
            .instances
            .get_mut(&key)
            .ok_or_else(|| DiscoveryError::NotFound("instance not found".to_string()))?;
        instance.expires_at_ms = expires_at_ms;
        instance.revision = revision;
        self.events.push(DiscoveryEvent {
            revision,
            namespace: instance.namespace.clone(),
            environment: instance.environment.clone(),
            kind: DiscoveryEventKind::InstanceRenewed,
            resource_id: instance.instance_id.clone(),
            service_name: Some(instance.service_name.clone()),
            config_group: None,
            config_key: None,
            config_application: None,
        });

        Ok(RenewLeaseResult {
            lease_id: command.lease_id,
            namespace: instance.namespace.clone(),
            environment: instance.environment.clone(),
            service_name: instance.service_name.clone(),
            instance_id: instance.instance_id.clone(),
            revision,
            expires_at_ms,
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

        let key = InstanceKey {
            namespace: command.namespace,
            environment: command.environment,
            service_name: command.service_name,
            instance_id: command.instance_id,
        };
        let current_instance = self
            .instances
            .get(&key)
            .ok_or_else(|| DiscoveryError::NotFound("instance not found".to_string()))?;

        if current_instance.expires_at_ms <= command.now_ms {
            return Err(DiscoveryError::NotFound("instance not found".to_string()));
        }

        // CAS check
        if let Some(expected_revision) = command.expected_revision {
            if current_instance.revision != expected_revision {
                return Err(DiscoveryError::Conflict(format!(
                    "expected revision {} but found {}",
                    expected_revision, current_instance.revision
                )));
            }
        }

        let revision = self.next_revision();
        let instance = self
            .instances
            .get_mut(&key)
            .ok_or_else(|| DiscoveryError::NotFound("instance not found".to_string()))?;
        instance.status = command.status;
        instance.revision = revision;
        self.events.push(DiscoveryEvent {
            revision,
            namespace: instance.namespace.clone(),
            environment: instance.environment.clone(),
            kind: DiscoveryEventKind::InstanceStatusReported,
            resource_id: instance.instance_id.clone(),
            service_name: Some(instance.service_name.clone()),
            config_group: None,
            config_key: None,
            config_application: None,
        });

        Ok(ReportInstanceStatusResult {
            revision,
            status: instance.status,
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

        let key = InstanceKey {
            namespace: namespace.to_string(),
            environment: environment.to_string(),
            service_name: service_name.to_string(),
            instance_id: instance_id.to_string(),
        };
        let current_instance = self.instances.get(&key);
        if current_instance.is_none_or(|instance| instance.expires_at_ms <= now_ms) {
            return Ok(DeregisterInstanceResult {
                namespace: namespace.to_string(),
                environment: environment.to_string(),
                service_name: service_name.to_string(),
                instance_id: instance_id.to_string(),
                revision: 0,
                deregistered: false,
            });
        }

        if let Some(instance) = self.instances.remove(&key) {
            self.lease_index.remove(&instance.lease_id);
            let revision = self.next_revision();
            self.events.push(DiscoveryEvent {
                revision,
                namespace: instance.namespace.clone(),
                environment: instance.environment.clone(),
                kind: DiscoveryEventKind::InstanceDeregistered,
                resource_id: instance.instance_id.clone(),
                service_name: Some(instance.service_name.clone()),
                config_group: None,
                config_key: None,
                config_application: None,
            });
            return Ok(DeregisterInstanceResult {
                namespace: instance.namespace,
                environment: instance.environment,
                service_name: instance.service_name,
                instance_id: instance.instance_id,
                revision,
                deregistered: true,
            });
        }
        Ok(DeregisterInstanceResult {
            namespace: namespace.to_string(),
            environment: environment.to_string(),
            service_name: service_name.to_string(),
            instance_id: instance_id.to_string(),
            revision: 0,
            deregistered: false,
        })
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

        let mut expired_keys = self
            .instances
            .iter()
            .filter_map(|(key, instance)| {
                if instance.expires_at_ms <= now_ms {
                    Some(key.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        expired_keys.sort_by(|left, right| {
            (
                &left.namespace,
                &left.environment,
                &left.service_name,
                &left.instance_id,
            )
                .cmp(&(
                    &right.namespace,
                    &right.environment,
                    &right.service_name,
                    &right.instance_id,
                ))
        });
        expired_keys.truncate(max_instances);

        let mut expired = Vec::with_capacity(expired_keys.len());
        for key in expired_keys {
            let Some(instance) = self.instances.remove(&key) else {
                continue;
            };
            self.lease_index.remove(&instance.lease_id);
            let revision = self.next_revision();
            self.events.push(DiscoveryEvent {
                revision,
                namespace: instance.namespace.clone(),
                environment: instance.environment.clone(),
                kind: DiscoveryEventKind::InstanceDeregistered,
                resource_id: instance.instance_id.clone(),
                service_name: Some(instance.service_name.clone()),
                config_group: None,
                config_key: None,
                config_application: None,
            });
            expired.push(DeregisterInstanceResult {
                namespace: instance.namespace,
                environment: instance.environment,
                service_name: instance.service_name,
                instance_id: instance.instance_id,
                revision,
                deregistered: true,
            });
        }

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

        let instances = self
            .instances
            .values()
            .filter(|instance| {
                instance.namespace == query.namespace
                    && instance.environment == query.environment
                    && instance.service_name == query.service_name
                    && query
                        .protocol
                        .as_ref()
                        .is_none_or(|protocol| &instance.protocol == protocol)
                    && instance.expires_at_ms > now_ms
                    && (!query.healthy_only || instance.status.is_discoverable())
            })
            .cloned()
            .collect::<Vec<_>>();

        Ok(finalize_discover_instances(
            instances,
            &query,
            self.revision,
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

        let key = InstanceKey {
            namespace: query.namespace,
            environment: query.environment,
            service_name: query.service_name,
            instance_id: query.instance_id,
        };
        Ok(self
            .instances
            .get(&key)
            .filter(|instance| instance.expires_at_ms > now_ms)
            .cloned())
    }

    async fn list_services(
        &self,
        query: ListServicesQuery,
        now_ms: u64,
    ) -> DiscoveryResult<ListServicesResult> {
        validate_non_empty("namespace", &query.namespace)?;
        validate_non_empty("environment", &query.environment)?;

        let mut services: BTreeMap<String, ServiceSummary> = BTreeMap::new();

        for instance in self.instances.values().filter(|instance| {
            instance.namespace == query.namespace && instance.environment == query.environment
        }) {
            if instance.expires_at_ms <= now_ms {
                continue;
            }

            let summary = services
                .entry(instance.service_name.clone())
                .or_insert_with(|| ServiceSummary {
                    namespace: instance.namespace.clone(),
                    environment: instance.environment.clone(),
                    service_name: instance.service_name.clone(),
                    active_instance_count: 0,
                    latest_revision: 0,
                });
            summary.active_instance_count += 1;
            summary.latest_revision = summary.latest_revision.max(instance.revision);
        }

        Ok(ListServicesResult {
            revision: self.revision,
            services: services.into_values().collect(),
        })
    }

    async fn list_active_instances_with_health_check(
        &self,
        now_ms: u64,
    ) -> DiscoveryResult<Vec<ServiceInstance>> {
        Ok(self
            .instances
            .values()
            .filter(|instance| instance.health_check.is_some() && instance.expires_at_ms > now_ms)
            .cloned()
            .collect())
    }

    async fn update_health_check_state(
        &mut self,
        namespace: &str,
        environment: &str,
        service_name: &str,
        instance_id: &str,
        state: sdkwork_discovery_contract::HealthCheckRuntimeState,
    ) -> DiscoveryResult<()> {
        let key = InstanceKey {
            namespace: namespace.to_string(),
            environment: environment.to_string(),
            service_name: service_name.to_string(),
            instance_id: instance_id.to_string(),
        };
        let Some(instance) = self.instances.get_mut(&key) else {
            return Err(DiscoveryError::NotFound("instance not found".to_string()));
        };
        instance.health_check_state = state;
        Ok(())
    }
}

fn lease_expires_at_ms(now_ms: u64, lease_ttl_seconds: u64) -> DiscoveryResult<u64> {
    let ttl_ms = lease_ttl_seconds.checked_mul(1_000).ok_or_else(|| {
        DiscoveryError::InvalidArgument("lease ttl overflows milliseconds".to_string())
    })?;
    now_ms
        .checked_add(ttl_ms)
        .ok_or_else(|| DiscoveryError::InvalidArgument("lease expiration overflows".to_string()))
}

fn validate_optional_filter(field: &str, value: Option<&str>) -> DiscoveryResult<()> {
    if let Some(value) = value {
        validate_non_empty(field, value)?;
    }
    Ok(())
}
