use std::collections::BTreeMap;

use sdkwork_discovery_contract::{
    BatchRegisterResult, ConfigFormat, ConfigRelease, ConfigScope, CreateConfigDraftCommand,
    DiscoverInstancesQuery, DiscoverInstancesResult, DiscoveryError, DiscoveryEvent,
    DiscoveryEventKind, DiscoveryResult, EffectiveConfig, IdempotencyContext, InstanceStatus,
    ListServicesQuery, ListServicesResult, PublishConfigCommand, RegisterInstanceCommand,
    RegisterInstanceResult, RenewLeaseCommand, RenewLeaseResult, ReportInstanceStatusCommand,
    ReportInstanceStatusResult, RetrieveEffectiveConfigQuery, RetrieveInstanceQuery,
    RollbackConfigCommand, ServiceInstance, ServiceSummary,
};
use sdkwork_discovery_rpc_proto::sdkwork::discovery::backend::v3 as backend_proto;
use sdkwork_discovery_rpc_proto::sdkwork::discovery::common::v1 as common_proto;
use sdkwork_discovery_rpc_proto::sdkwork::discovery::internal::v1 as internal_proto;

pub fn register_instance_command(
    request: internal_proto::RegisterInstanceRequest,
    now_ms: u64,
) -> DiscoveryResult<RegisterInstanceCommand> {
    validate_identity_field("namespace", &request.namespace)?;
    validate_identity_field("environment", &request.environment)?;
    validate_identity_field("service_name", &request.service_name)?;
    validate_identity_field("instance_id", &request.instance_id)?;
    validate_endpoint_field("endpoint", &request.endpoint)?;
    validate_short_text_field("protocol", &request.protocol)?;
    validate_short_text_field("version", &request.version)?;
    validate_short_text_field("region", &request.region)?;
    validate_short_text_field("zone", &request.zone)?;
    validate_metadata(&request.metadata)?;
    if request.lease_ttl_seconds == 0 {
        return Err(DiscoveryError::InvalidArgument(
            "lease_ttl_seconds must be greater than zero".to_string(),
        ));
    }

    Ok(RegisterInstanceCommand {
        namespace: request.namespace,
        environment: request.environment,
        service_name: request.service_name,
        instance_id: request.instance_id,
        endpoint: request.endpoint,
        protocol: request.protocol,
        version: request.version,
        region: request.region,
        zone: request.zone,
        weight: request.weight,
        priority: request.priority,
        status: instance_status_from_proto(request.status)?,
        metadata: request.metadata,
        lease_ttl_seconds: request.lease_ttl_seconds,
        now_ms,
        expected_revision: request.expected_revision,
        persistent: request.persistent,
        health_check: request
            .health_check
            .as_ref()
            .map(health_check_from_proto)
            .transpose()?,
    })
}

pub fn register_instance_response(
    result: RegisterInstanceResult,
    request_id: String,
    trace_id: String,
) -> internal_proto::RegisterInstanceResponse {
    internal_proto::RegisterInstanceResponse {
        lease_id: result.lease_id,
        expires_at: Some(timestamp_from_millis(result.expires_at_ms)),
        metadata: Some(response_metadata(result.revision, request_id, trace_id)),
    }
}

pub fn batch_register_instances_commands(
    request: internal_proto::BatchRegisterInstancesRequest,
    now_ms: u64,
) -> DiscoveryResult<Vec<RegisterInstanceCommand>> {
    request
        .instances
        .into_iter()
        .map(|instance| register_instance_command(instance, now_ms))
        .collect()
}

pub fn batch_register_instances_response(
    result: BatchRegisterResult,
    request_id: String,
    trace_id: String,
) -> internal_proto::BatchRegisterInstancesResponse {
    let revision = result
        .results
        .last()
        .map(|register_result| register_result.revision)
        .unwrap_or(0);

    internal_proto::BatchRegisterInstancesResponse {
        results: result
            .results
            .into_iter()
            .map(|register_result| internal_proto::RegisterInstanceResponse {
                lease_id: register_result.lease_id,
                expires_at: Some(timestamp_from_millis(register_result.expires_at_ms)),
                metadata: None,
            })
            .collect(),
        errors: result
            .errors
            .into_iter()
            .map(|error| common_proto::BatchOperationError {
                index: error.index as u32,
                error_code: error.error_code,
                error_message: error.error_message,
            })
            .collect(),
        metadata: Some(response_metadata(revision, request_id, trace_id)),
    }
}

pub fn renew_lease_command(
    request: internal_proto::RenewLeaseRequest,
    now_ms: u64,
) -> DiscoveryResult<RenewLeaseCommand> {
    validate_identity_field("lease_id", &request.lease_id)?;
    if request.lease_ttl_seconds == 0 {
        return Err(DiscoveryError::InvalidArgument(
            "lease_ttl_seconds must be greater than zero".to_string(),
        ));
    }

    Ok(RenewLeaseCommand {
        lease_id: request.lease_id,
        lease_ttl_seconds: request.lease_ttl_seconds,
        now_ms,
    })
}

pub fn renew_lease_response(
    result: RenewLeaseResult,
    request_id: String,
    trace_id: String,
) -> internal_proto::RenewLeaseResponse {
    internal_proto::RenewLeaseResponse {
        lease_id: result.lease_id,
        expires_at: Some(timestamp_from_millis(result.expires_at_ms)),
        metadata: Some(response_metadata(result.revision, request_id, trace_id)),
    }
}

pub fn report_status_command(
    request: internal_proto::ReportInstanceStatusRequest,
    now_ms: u64,
) -> DiscoveryResult<ReportInstanceStatusCommand> {
    validate_identity_field("namespace", &request.namespace)?;
    validate_identity_field("environment", &request.environment)?;
    validate_identity_field("service_name", &request.service_name)?;
    validate_identity_field("instance_id", &request.instance_id)?;

    Ok(ReportInstanceStatusCommand {
        namespace: request.namespace,
        environment: request.environment,
        service_name: request.service_name,
        instance_id: request.instance_id,
        status: instance_status_from_proto(request.status)?,
        now_ms,
        expected_revision: request.expected_revision,
    })
}

pub fn deregister_instance_request(
    request: internal_proto::DeregisterInstanceRequest,
) -> DiscoveryResult<internal_proto::DeregisterInstanceRequest> {
    validate_identity_field("namespace", &request.namespace)?;
    validate_identity_field("environment", &request.environment)?;
    validate_identity_field("service_name", &request.service_name)?;
    validate_identity_field("instance_id", &request.instance_id)?;
    Ok(request)
}

pub fn report_status_response(
    result: ReportInstanceStatusResult,
    request_id: String,
    trace_id: String,
) -> internal_proto::ReportInstanceStatusResponse {
    internal_proto::ReportInstanceStatusResponse {
        metadata: Some(response_metadata(result.revision, request_id, trace_id)),
    }
}

pub fn discover_instances_query(
    request: internal_proto::DiscoverInstancesRequest,
) -> DiscoveryResult<DiscoverInstancesQuery> {
    validate_identity_field("namespace", &request.namespace)?;
    validate_identity_field("environment", &request.environment)?;
    validate_identity_field("service_name", &request.service_name)?;
    validate_optional_short_text_field("protocol", &request.protocol)?;

    let label_filters = request
        .label_filters
        .into_iter()
        .map(|filter| {
            let op = match filter.op {
                1 => sdkwork_discovery_contract::LabelFilterOp::Eq,
                2 => sdkwork_discovery_contract::LabelFilterOp::NotEq,
                3 => sdkwork_discovery_contract::LabelFilterOp::In,
                4 => sdkwork_discovery_contract::LabelFilterOp::Exists,
                _ => sdkwork_discovery_contract::LabelFilterOp::Eq,
            };
            sdkwork_discovery_contract::LabelFilter {
                key: filter.key,
                op,
                value: filter.value,
            }
        })
        .collect::<Vec<_>>();
    validate_label_filters(&label_filters)?;

    let sort_by = match request.sort_by {
        1 => Some(sdkwork_discovery_contract::DiscoverSortBy::InstanceId),
        2 => Some(sdkwork_discovery_contract::DiscoverSortBy::Priority),
        3 => Some(sdkwork_discovery_contract::DiscoverSortBy::Weight),
        4 => Some(sdkwork_discovery_contract::DiscoverSortBy::WeightedRandom),
        _ => None,
    };

    if let Some(page) = request.page.as_ref() {
        if !page.page_token.is_empty()
            && matches!(
                sort_by,
                Some(sdkwork_discovery_contract::DiscoverSortBy::WeightedRandom)
            )
        {
            return Err(DiscoveryError::InvalidArgument(
                "page_token is not supported with weighted-random discovery sorting".to_string(),
            ));
        }
    }

    let page = request.page.unwrap_or_default();

    Ok(DiscoverInstancesQuery {
        namespace: request.namespace,
        environment: request.environment,
        service_name: request.service_name,
        healthy_only: request.healthy_only,
        protocol: empty_string_as_none(request.protocol),
        label_filters,
        sort_by,
        page_size: page.page_size,
        page_token: sdkwork_discovery_contract::normalize_page_token(
            if page.page_token.is_empty() {
                None
            } else {
                Some(page.page_token)
            },
        ),
    })
}

pub fn retrieve_instance_query(
    request: internal_proto::RetrieveInstanceRequest,
) -> DiscoveryResult<RetrieveInstanceQuery> {
    validate_identity_field("namespace", &request.namespace)?;
    validate_identity_field("environment", &request.environment)?;
    validate_identity_field("service_name", &request.service_name)?;
    validate_identity_field("instance_id", &request.instance_id)?;

    Ok(RetrieveInstanceQuery {
        namespace: request.namespace,
        environment: request.environment,
        service_name: request.service_name,
        instance_id: request.instance_id,
    })
}

pub fn discover_instances_response(
    result: DiscoverInstancesResult,
    request_id: String,
    trace_id: String,
) -> internal_proto::DiscoverInstancesResponse {
    internal_proto::DiscoverInstancesResponse {
        instances: result
            .instances
            .into_iter()
            .map(service_instance_to_proto)
            .collect(),
        metadata: Some(response_metadata(result.revision, request_id, trace_id)),
        page: Some(common_proto::PageResponse {
            next_page_token: result.next_page_token.unwrap_or_default(),
        }),
    }
}

pub fn retrieve_instance_response(
    instance: ServiceInstance,
    request_id: String,
    trace_id: String,
) -> internal_proto::RetrieveInstanceResponse {
    let revision = instance.revision;
    internal_proto::RetrieveInstanceResponse {
        instance: Some(service_instance_to_proto(instance)),
        metadata: Some(response_metadata(revision, request_id, trace_id)),
    }
}

pub fn retrieve_effective_config_query(
    request: internal_proto::RetrieveEffectiveConfigRequest,
) -> DiscoveryResult<RetrieveEffectiveConfigQuery> {
    validate_identity_field("namespace", &request.namespace)?;
    validate_identity_field("environment", &request.environment)?;
    validate_identity_field("application", &request.application)?;
    validate_identity_field("service_name", &request.service_name)?;
    validate_identity_field("group", &request.group)?;

    Ok(RetrieveEffectiveConfigQuery {
        namespace: request.namespace,
        environment: request.environment,
        application: request.application,
        service_name: request.service_name,
        group: request.group,
    })
}

pub fn retrieve_effective_config_response(
    result: EffectiveConfig,
    request_id: String,
    trace_id: String,
) -> internal_proto::RetrieveEffectiveConfigResponse {
    internal_proto::RetrieveEffectiveConfigResponse {
        values: effective_values_to_proto(result.values),
        metadata: Some(response_metadata(result.revision, request_id, trace_id)),
    }
}

pub fn create_config_draft_command(
    request: backend_proto::CreateConfigDraftRequest,
    created_by: String,
    idempotency: IdempotencyContext,
) -> DiscoveryResult<CreateConfigDraftCommand> {
    validate_identity_field("namespace", &request.namespace)?;
    validate_identity_field("environment", &request.environment)?;
    validate_identity_field("group", &request.group)?;
    validate_identity_field("key", &request.key)?;

    Ok(CreateConfigDraftCommand {
        namespace: request.namespace,
        environment: request.environment,
        group: request.group,
        key: request.key,
        format: config_format_from_proto(request.format)?,
        value: request.value,
        scope: config_scope_from_proto(
            request.scope_type,
            request.application,
            request.service_name,
        )?,
        created_by,
        idempotency: Some(idempotency),
    })
}

pub fn create_config_draft_response(
    draft_id: String,
    content_hash: String,
    request_id: String,
    trace_id: String,
) -> backend_proto::CreateConfigDraftResponse {
    backend_proto::CreateConfigDraftResponse {
        draft_id,
        content_hash,
        metadata: Some(response_metadata(0, request_id, trace_id)),
    }
}

pub fn publish_config_command(
    request: backend_proto::PublishConfigRequest,
    published_by: String,
    now_ms: u64,
    idempotency: IdempotencyContext,
) -> DiscoveryResult<PublishConfigCommand> {
    validate_identity_field("draft_id", &request.draft_id)?;

    Ok(PublishConfigCommand {
        draft_id: request.draft_id,
        published_by,
        now_ms,
        idempotency: Some(idempotency),
    })
}

pub fn publish_config_response(
    release: ConfigRelease,
    request_id: String,
    trace_id: String,
) -> backend_proto::PublishConfigResponse {
    backend_proto::PublishConfigResponse {
        release_id: release.release_id,
        metadata: Some(response_metadata(release.revision, request_id, trace_id)),
    }
}

pub fn rollback_config_command(
    request: backend_proto::RollbackConfigRequest,
    rolled_back_by: String,
    now_ms: u64,
    idempotency: IdempotencyContext,
) -> DiscoveryResult<RollbackConfigCommand> {
    validate_identity_field("source_release_id", &request.source_release_id)?;

    Ok(RollbackConfigCommand {
        source_release_id: request.source_release_id,
        rolled_back_by,
        now_ms,
        idempotency: Some(idempotency),
    })
}

pub fn rollback_config_response(
    release: ConfigRelease,
    request_id: String,
    trace_id: String,
) -> backend_proto::RollbackConfigResponse {
    let revision = release.revision;
    backend_proto::RollbackConfigResponse {
        release: Some(config_release_to_proto(release)),
        metadata: Some(response_metadata(revision, request_id, trace_id)),
    }
}

pub fn list_services_query(
    request: backend_proto::ListServicesRequest,
) -> DiscoveryResult<ListServicesQuery> {
    validate_identity_field("namespace", &request.namespace)?;
    validate_identity_field("environment", &request.environment)?;

    Ok(ListServicesQuery {
        namespace: request.namespace,
        environment: request.environment,
        page_size: request
            .page
            .as_ref()
            .map(|page| page.page_size)
            .unwrap_or(0),
        page_token: sdkwork_discovery_contract::normalize_page_token(request.page.and_then(
            |page| {
                if page.page_token.is_empty() {
                    None
                } else {
                    Some(page.page_token)
                }
            },
        )),
    })
}

pub fn list_services_response(
    result: ListServicesResult,
    request_id: String,
    trace_id: String,
) -> backend_proto::ListServicesResponse {
    backend_proto::ListServicesResponse {
        services: result
            .services
            .into_iter()
            .map(service_summary_to_proto)
            .collect(),
        metadata: Some(response_metadata(result.revision, request_id, trace_id)),
        page: Some(common_proto::PageResponse {
            next_page_token: result.next_page_token.unwrap_or_default(),
        }),
    }
}

pub fn watch_event_type(event: &DiscoveryEvent) -> i32 {
    match event.kind {
        DiscoveryEventKind::InstanceRegistered => {
            common_proto::WatchEventType::InstanceRegistered as i32
        }
        DiscoveryEventKind::InstanceUpdated => common_proto::WatchEventType::InstanceUpdated as i32,
        DiscoveryEventKind::InstanceStatusReported => {
            common_proto::WatchEventType::InstanceStatusReported as i32
        }
        DiscoveryEventKind::InstanceRenewed => common_proto::WatchEventType::InstanceRenewed as i32,
        DiscoveryEventKind::InstanceDeregistered => {
            common_proto::WatchEventType::InstanceDeregistered as i32
        }
        DiscoveryEventKind::ConfigPublished => common_proto::WatchEventType::ConfigPublished as i32,
        DiscoveryEventKind::ConfigRolledBack => {
            common_proto::WatchEventType::ConfigRolledBack as i32
        }
    }
}

pub fn response_metadata(
    revision: u64,
    request_id: String,
    trace_id: String,
) -> common_proto::ResponseMetadata {
    common_proto::ResponseMetadata {
        request_id,
        trace_id,
        server_time: Some(timestamp_from_millis(now_millis())),
        revision,
    }
}

pub fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

fn effective_values_to_proto(
    values: BTreeMap<String, sdkwork_discovery_contract::EffectiveConfigValue>,
) -> Vec<common_proto::ConfigValue> {
    values
        .into_iter()
        .map(|(key, value)| common_proto::ConfigValue {
            key,
            value: value.value,
            format: config_format_to_proto(value.format) as i32,
            source_release_id: value.source_release_id,
            source_specificity: u32::from(value.source_specificity),
            source_revision: value.source_revision,
        })
        .collect()
}

pub(crate) fn service_instance_to_proto(
    instance: ServiceInstance,
) -> common_proto::ServiceInstance {
    common_proto::ServiceInstance {
        namespace: instance.namespace,
        environment: instance.environment,
        service_name: instance.service_name,
        instance_id: instance.instance_id,
        endpoint: instance.endpoint,
        protocol: instance.protocol,
        version: instance.version,
        region: instance.region,
        zone: instance.zone,
        weight: instance.weight,
        priority: instance.priority,
        status: instance_status_to_proto(instance.status) as i32,
        metadata: instance.metadata,
        lease_id: instance.lease_id,
        expires_at: Some(timestamp_from_millis(instance.expires_at_ms)),
        revision: instance.revision,
        health_check: instance.health_check.as_ref().map(health_check_to_proto),
    }
}

fn health_check_from_proto(
    config: &common_proto::HealthCheckConfig,
) -> DiscoveryResult<sdkwork_discovery_contract::HealthCheckConfig> {
    let Some(probe) = config.probe.as_ref() else {
        return Err(DiscoveryError::InvalidArgument(
            "health_check.probe is required".to_string(),
        ));
    };
    Ok(sdkwork_discovery_contract::HealthCheckConfig {
        probe: health_check_probe_from_proto(probe)?,
        interval_ms: config.interval_ms,
        timeout_ms: config.timeout_ms,
        unhealthy_threshold: config.unhealthy_threshold,
        healthy_threshold: config.healthy_threshold,
    })
}

fn health_check_probe_from_proto(
    probe: &common_proto::HealthCheckProbe,
) -> DiscoveryResult<sdkwork_discovery_contract::HealthCheckProbe> {
    use common_proto::HealthCheckProbeKind;
    match HealthCheckProbeKind::try_from(probe.kind) {
        Ok(HealthCheckProbeKind::Tcp) => Ok(sdkwork_discovery_contract::HealthCheckProbe::Tcp),
        Ok(HealthCheckProbeKind::Http) => Ok(sdkwork_discovery_contract::HealthCheckProbe::Http {
            path: probe.http_path.clone(),
            expected_status: probe.http_expected_status as u16,
        }),
        Ok(HealthCheckProbeKind::Grpc) => Ok(sdkwork_discovery_contract::HealthCheckProbe::Grpc {
            service_name: probe.grpc_service_name.clone(),
        }),
        _ => Err(DiscoveryError::InvalidArgument(
            "health_check.probe kind is required".to_string(),
        )),
    }
}

fn health_check_to_proto(
    config: &sdkwork_discovery_contract::HealthCheckConfig,
) -> common_proto::HealthCheckConfig {
    use common_proto::HealthCheckProbeKind;
    let (kind, http_path, http_expected_status, grpc_service_name) = match &config.probe {
        sdkwork_discovery_contract::HealthCheckProbe::Tcp => (
            HealthCheckProbeKind::Tcp as i32,
            String::new(),
            0,
            String::new(),
        ),
        sdkwork_discovery_contract::HealthCheckProbe::Http {
            path,
            expected_status,
        } => (
            HealthCheckProbeKind::Http as i32,
            path.clone(),
            u32::from(*expected_status),
            String::new(),
        ),
        sdkwork_discovery_contract::HealthCheckProbe::Grpc { service_name } => (
            HealthCheckProbeKind::Grpc as i32,
            String::new(),
            0,
            service_name.clone(),
        ),
    };
    common_proto::HealthCheckConfig {
        probe: Some(common_proto::HealthCheckProbe {
            kind,
            http_path,
            http_expected_status,
            grpc_service_name,
        }),
        interval_ms: config.interval_ms,
        timeout_ms: config.timeout_ms,
        unhealthy_threshold: config.unhealthy_threshold,
        healthy_threshold: config.healthy_threshold,
    }
}

fn service_summary_to_proto(summary: ServiceSummary) -> common_proto::ServiceSummary {
    common_proto::ServiceSummary {
        namespace: summary.namespace,
        environment: summary.environment,
        service_name: summary.service_name,
        active_instance_count: summary.active_instance_count as u64,
        latest_revision: summary.latest_revision,
    }
}

fn config_release_to_proto(release: ConfigRelease) -> common_proto::ConfigRelease {
    let (scope_type, application, service_name) = config_scope_to_proto_fields(release.scope);
    common_proto::ConfigRelease {
        release_id: release.release_id,
        draft_id: release.draft_id,
        namespace: release.namespace,
        environment: release.environment,
        group: release.group,
        key: release.key,
        format: config_format_to_proto(release.format) as i32,
        value: release.value,
        scope_type: scope_type as i32,
        application,
        service_name,
        content_hash: release.content_hash,
        published_by: release.published_by,
        published_at: Some(timestamp_from_millis(release.published_at_ms)),
        revision: release.revision,
    }
}

fn instance_status_from_proto(value: i32) -> DiscoveryResult<InstanceStatus> {
    match common_proto::InstanceStatus::try_from(value).map_err(|_| {
        DiscoveryError::InvalidArgument("instance status is not recognized".to_string())
    })? {
        common_proto::InstanceStatus::Serving => Ok(InstanceStatus::Serving),
        common_proto::InstanceStatus::Degraded => Ok(InstanceStatus::Degraded),
        common_proto::InstanceStatus::NotServing => Ok(InstanceStatus::NotServing),
        common_proto::InstanceStatus::Unspecified => Err(DiscoveryError::InvalidArgument(
            "instance status must be specified".to_string(),
        )),
    }
}

fn instance_status_to_proto(status: InstanceStatus) -> common_proto::InstanceStatus {
    match status {
        InstanceStatus::Serving => common_proto::InstanceStatus::Serving,
        InstanceStatus::Degraded => common_proto::InstanceStatus::Degraded,
        InstanceStatus::NotServing => common_proto::InstanceStatus::NotServing,
    }
}

fn config_format_from_proto(value: i32) -> DiscoveryResult<ConfigFormat> {
    match common_proto::ConfigFormat::try_from(value).map_err(|_| {
        DiscoveryError::InvalidArgument("config format is not recognized".to_string())
    })? {
        common_proto::ConfigFormat::Text => Ok(ConfigFormat::Text),
        common_proto::ConfigFormat::Json => Ok(ConfigFormat::Json),
        common_proto::ConfigFormat::Toml => Ok(ConfigFormat::Toml),
        common_proto::ConfigFormat::Unspecified => Err(DiscoveryError::InvalidArgument(
            "config format must be specified".to_string(),
        )),
    }
}

fn config_format_to_proto(format: ConfigFormat) -> common_proto::ConfigFormat {
    match format {
        ConfigFormat::Text => common_proto::ConfigFormat::Text,
        ConfigFormat::Json => common_proto::ConfigFormat::Json,
        ConfigFormat::Toml => common_proto::ConfigFormat::Toml,
    }
}

fn config_scope_from_proto(
    value: i32,
    application: String,
    service_name: String,
) -> DiscoveryResult<ConfigScope> {
    match common_proto::ConfigScopeType::try_from(value).map_err(|_| {
        DiscoveryError::InvalidArgument("config scope type is not recognized".to_string())
    })? {
        common_proto::ConfigScopeType::Namespace => Ok(ConfigScope::Namespace),
        common_proto::ConfigScopeType::Application => {
            if application.trim().is_empty() {
                return Err(DiscoveryError::InvalidArgument(
                    "application config scope requires application".to_string(),
                ));
            }
            Ok(ConfigScope::Application { application })
        }
        common_proto::ConfigScopeType::Service => {
            if application.trim().is_empty() || service_name.trim().is_empty() {
                return Err(DiscoveryError::InvalidArgument(
                    "service config scope requires application and service_name".to_string(),
                ));
            }
            Ok(ConfigScope::Service {
                application,
                service_name,
            })
        }
        common_proto::ConfigScopeType::Unspecified => Err(DiscoveryError::InvalidArgument(
            "config scope type must be specified".to_string(),
        )),
    }
}

fn config_scope_to_proto_fields(
    scope: ConfigScope,
) -> (common_proto::ConfigScopeType, String, String) {
    match scope {
        ConfigScope::Namespace => (
            common_proto::ConfigScopeType::Namespace,
            String::new(),
            String::new(),
        ),
        ConfigScope::Application { application } => (
            common_proto::ConfigScopeType::Application,
            application,
            String::new(),
        ),
        ConfigScope::Service {
            application,
            service_name,
        } => (
            common_proto::ConfigScopeType::Service,
            application,
            service_name,
        ),
    }
}

fn timestamp_from_millis(milliseconds: u64) -> prost_types::Timestamp {
    let seconds = milliseconds / 1_000;
    let nanos = (milliseconds % 1_000) * 1_000_000;
    prost_types::Timestamp {
        seconds: seconds.min(i64::MAX as u64) as i64,
        nanos: nanos as i32,
    }
}

fn empty_string_as_none(value: String) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}

pub(crate) fn validate_required_field(field: &str, value: &str) -> DiscoveryResult<()> {
    if value.trim().is_empty() {
        return Err(DiscoveryError::InvalidArgument(format!(
            "{field} must not be empty"
        )));
    }
    Ok(())
}

/// Maximum length for identity-style fields (namespace, environment,
/// service_name, instance_id, group, key, application, lease_id, draft_id,
/// source_release_id). Aligned with DNS subdomain / Kubernetes label limits.
const MAX_IDENTITY_FIELD_LEN: usize = 128;

/// Maximum length for endpoint URIs. Generous enough for IPv6 + scheme +
/// path, bounded to prevent abuse.
const MAX_ENDPOINT_LEN: usize = 512;

/// Maximum length for short descriptive fields (protocol, version, region,
/// zone). These carry human-readable values but must be bounded.
const MAX_SHORT_FIELD_LEN: usize = 64;

/// Maximum number of metadata entries per request. Prevents unbounded
/// memory allocation from adversarial clients.
const MAX_METADATA_ENTRIES: usize = 64;

/// Maximum length of a metadata key. Matches identity-field bound.
const MAX_METADATA_KEY_LEN: usize = 128;

/// Maximum length of a metadata value. Allows structured payloads (JSON,
/// YAML fragments) while bounding memory.
const MAX_METADATA_VALUE_LEN: usize = 4096;

/// Maximum number of label filters per discover query. Prevents
/// combinatorial explosion in storage layer.
const MAX_LABEL_FILTERS: usize = 32;

/// Returns true when the byte is allowed in identity-style fields:
/// ASCII alphanumeric, dot, underscore, or hyphen.
fn is_identity_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')
}

/// Returns true when the byte is printable ASCII (0x20..=0x7E).
fn is_printable_ascii(byte: u8) -> bool {
    (0x20..=0x7E).contains(&byte)
}

/// Returns true when the byte is a control character (0x00..=0x1F or 0x7F).
fn is_control_char(byte: u8) -> bool {
    byte < 0x20 || byte == 0x7F
}

/// Validates an identity-style field: non-empty, bounded length, restricted
/// character set `[a-zA-Z0-9._-]`. Used for namespace, environment,
/// service_name, instance_id, group, key, application, lease_id, draft_id,
/// source_release_id.
pub(crate) fn validate_identity_field(field: &str, value: &str) -> DiscoveryResult<()> {
    validate_required_field(field, value)?;
    if value.len() > MAX_IDENTITY_FIELD_LEN {
        return Err(DiscoveryError::InvalidArgument(format!(
            "{field} must be at most {MAX_IDENTITY_FIELD_LEN} characters"
        )));
    }
    if let Some(byte) = value.bytes().find(|byte| !is_identity_char(*byte)) {
        return Err(DiscoveryError::InvalidArgument(format!(
            "{field} contains an invalid character (0x{byte:02X}); allowed set is [a-zA-Z0-9._-]"
        )));
    }
    Ok(())
}

/// Validates an endpoint URI: non-empty, bounded length, printable ASCII
/// only (no control characters, no non-ASCII bytes).
fn validate_endpoint_field(field: &str, value: &str) -> DiscoveryResult<()> {
    validate_required_field(field, value)?;
    if value.len() > MAX_ENDPOINT_LEN {
        return Err(DiscoveryError::InvalidArgument(format!(
            "{field} must be at most {MAX_ENDPOINT_LEN} characters"
        )));
    }
    if let Some(byte) = value.bytes().find(|byte| !is_printable_ascii(*byte)) {
        return Err(DiscoveryError::InvalidArgument(format!(
            "{field} contains a non-printable ASCII byte (0x{byte:02X})"
        )));
    }
    Ok(())
}

/// Validates a short descriptive field: non-empty, bounded length, no
/// control characters. Used for protocol, version, region, zone.
fn validate_short_text_field(field: &str, value: &str) -> DiscoveryResult<()> {
    validate_required_field(field, value)?;
    if value.len() > MAX_SHORT_FIELD_LEN {
        return Err(DiscoveryError::InvalidArgument(format!(
            "{field} must be at most {MAX_SHORT_FIELD_LEN} characters"
        )));
    }
    if let Some(byte) = value.bytes().find(|byte| is_control_char(*byte)) {
        return Err(DiscoveryError::InvalidArgument(format!(
            "{field} contains a control character (0x{byte:02X})"
        )));
    }
    Ok(())
}

/// Validates an optional short descriptive field. Empty values are accepted.
fn validate_optional_short_text_field(field: &str, value: &str) -> DiscoveryResult<()> {
    if !value.is_empty() {
        validate_short_text_field(field, value)?;
    }
    Ok(())
}

/// Validates a metadata map: entry count, key length/charset, and value
/// length are bounded to prevent resource exhaustion.
pub(crate) fn validate_metadata(
    metadata: &std::collections::HashMap<String, String>,
) -> DiscoveryResult<()> {
    if metadata.len() > MAX_METADATA_ENTRIES {
        return Err(DiscoveryError::InvalidArgument(format!(
            "metadata must contain at most {MAX_METADATA_ENTRIES} entries"
        )));
    }
    for (key, value) in metadata {
        if key.len() > MAX_METADATA_KEY_LEN {
            return Err(DiscoveryError::InvalidArgument(format!(
                "metadata key must be at most {MAX_METADATA_KEY_LEN} characters"
            )));
        }
        if let Some(byte) = key.bytes().find(|byte| !is_identity_char(*byte)) {
            return Err(DiscoveryError::InvalidArgument(format!(
                "metadata key contains an invalid character (0x{byte:02X}); allowed set is [a-zA-Z0-9._-]"
            )));
        }
        if value.len() > MAX_METADATA_VALUE_LEN {
            return Err(DiscoveryError::InvalidArgument(format!(
                "metadata value for key '{key}' must be at most {MAX_METADATA_VALUE_LEN} characters"
            )));
        }
    }
    Ok(())
}

/// Validates label filters: count, key length/charset, and value length are
/// bounded to prevent resource exhaustion in the discovery query path.
fn validate_label_filters(
    filters: &[sdkwork_discovery_contract::LabelFilter],
) -> DiscoveryResult<()> {
    if filters.len() > MAX_LABEL_FILTERS {
        return Err(DiscoveryError::InvalidArgument(format!(
            "label_filters must contain at most {MAX_LABEL_FILTERS} entries"
        )));
    }
    for filter in filters {
        if filter.key.len() > MAX_IDENTITY_FIELD_LEN {
            return Err(DiscoveryError::InvalidArgument(format!(
                "label_filters key must be at most {MAX_IDENTITY_FIELD_LEN} characters"
            )));
        }
        if let Some(byte) = filter.key.bytes().find(|byte| !is_identity_char(*byte)) {
            return Err(DiscoveryError::InvalidArgument(format!(
                "label_filters key contains an invalid character (0x{byte:02X}); allowed set is [a-zA-Z0-9._-]"
            )));
        }
        if filter.value.len() > MAX_METADATA_VALUE_LEN {
            return Err(DiscoveryError::InvalidArgument(format!(
                "label_filters value must be at most {MAX_METADATA_VALUE_LEN} characters"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdkwork_discovery_contract::LabelFilterOp;
    use std::collections::HashMap;

    #[test]
    fn validate_identity_field_accepts_valid_inputs() {
        assert!(validate_identity_field("namespace", "sdkwork").is_ok());
        assert!(validate_identity_field("service_name", "sdkwork-drive-product").is_ok());
        assert!(validate_identity_field("instance_id", "drive.1_2-3").is_ok());
        assert!(validate_identity_field("key", "a").is_ok());
        // Exactly MAX_IDENTITY_FIELD_LEN characters of allowed charset.
        let max_len = "a".repeat(MAX_IDENTITY_FIELD_LEN);
        assert!(validate_identity_field("namespace", &max_len).is_ok());
    }

    #[test]
    fn validate_identity_field_rejects_empty() {
        let err = validate_identity_field("namespace", "").unwrap_err();
        assert!(matches!(err, DiscoveryError::InvalidArgument(msg) if msg.contains("namespace")));
        let err = validate_identity_field("namespace", "   ").unwrap_err();
        assert!(matches!(err, DiscoveryError::InvalidArgument(_)));
    }

    #[test]
    fn validate_identity_field_rejects_excessive_length() {
        let too_long = "a".repeat(MAX_IDENTITY_FIELD_LEN + 1);
        let err = validate_identity_field("namespace", &too_long).unwrap_err();
        assert!(matches!(err, DiscoveryError::InvalidArgument(msg) if msg.contains("128")));
    }

    #[test]
    fn validate_identity_field_rejects_invalid_charset() {
        // Space, slash, colon are not in [a-zA-Z0-9._-].
        for invalid in ["sdk work", "sdk/work", "sdk:work", "sdk@work"] {
            let err = validate_identity_field("namespace", invalid).unwrap_err();
            assert!(
                matches!(err, DiscoveryError::InvalidArgument(msg) if msg.contains("invalid character"))
            );
        }
    }

    #[test]
    fn validate_endpoint_field_accepts_valid_uris() {
        assert!(validate_endpoint_field("endpoint", "grpc://127.0.0.1:50051").is_ok());
        assert!(validate_endpoint_field("endpoint", "http://example.com:8080/path?q=1").is_ok());
        assert!(validate_endpoint_field("endpoint", "https://[::1]:443").is_ok());
        // Printable ASCII includes space (0x20).
        assert!(validate_endpoint_field("endpoint", "a b").is_ok());
    }

    #[test]
    fn validate_endpoint_field_rejects_non_printable_ascii() {
        // Tab (0x09) and newline (0x0A) are non-printable.
        assert!(validate_endpoint_field("endpoint", "grpc://\t127.0.0.1").is_err());
        assert!(validate_endpoint_field("endpoint", "grpc://\n127.0.0.1").is_err());
        // Non-ASCII byte (0x80).
        let value = String::from("grpc://") + "\u{0080}";
        assert!(validate_endpoint_field("endpoint", &value).is_err());
    }

    #[test]
    fn validate_short_text_field_accepts_valid_inputs() {
        assert!(validate_short_text_field("protocol", "grpc").is_ok());
        assert!(validate_short_text_field("version", "0.1.0").is_ok());
        assert!(validate_short_text_field("region", "us-east-1").is_ok());
        assert!(validate_short_text_field("zone", "zone-a").is_ok());
    }

    #[test]
    fn validate_short_text_field_rejects_control_chars() {
        assert!(validate_short_text_field("protocol", "gr\tpc").is_err());
        assert!(validate_short_text_field("version", "1.0\n").is_err());
    }

    #[test]
    fn validate_metadata_accepts_valid_map() {
        let mut metadata = HashMap::new();
        metadata.insert("key1".to_string(), "value1".to_string());
        metadata.insert("key_2".to_string(), "value-2".to_string());
        assert!(validate_metadata(&metadata).is_ok());
    }

    #[test]
    fn validate_metadata_rejects_excessive_entries() {
        let mut metadata = HashMap::new();
        for i in 0..(MAX_METADATA_ENTRIES + 1) {
            metadata.insert(format!("k{i}"), "v".to_string());
        }
        let err = validate_metadata(&metadata).unwrap_err();
        assert!(matches!(err, DiscoveryError::InvalidArgument(msg) if msg.contains("64")));
    }

    #[test]
    fn validate_metadata_rejects_oversized_key() {
        let mut metadata = HashMap::new();
        metadata.insert("a".repeat(MAX_METADATA_KEY_LEN + 1), "v".to_string());
        let err = validate_metadata(&metadata).unwrap_err();
        assert!(matches!(err, DiscoveryError::InvalidArgument(msg) if msg.contains("128")));
    }

    #[test]
    fn validate_metadata_rejects_invalid_key_charset() {
        let mut metadata = HashMap::new();
        metadata.insert("bad key".to_string(), "v".to_string());
        let err = validate_metadata(&metadata).unwrap_err();
        assert!(
            matches!(err, DiscoveryError::InvalidArgument(msg) if msg.contains("invalid character"))
        );
    }

    #[test]
    fn validate_metadata_rejects_oversized_value() {
        let mut metadata = HashMap::new();
        metadata.insert("k".to_string(), "a".repeat(MAX_METADATA_VALUE_LEN + 1));
        let err = validate_metadata(&metadata).unwrap_err();
        assert!(matches!(err, DiscoveryError::InvalidArgument(msg) if msg.contains("4096")));
    }

    #[test]
    fn validate_label_filters_accepts_valid_filters() {
        let filters = vec![
            sdkwork_discovery_contract::LabelFilter {
                key: "env".to_string(),
                op: LabelFilterOp::Eq,
                value: "prod".to_string(),
            },
            sdkwork_discovery_contract::LabelFilter {
                key: "tier".to_string(),
                op: LabelFilterOp::In,
                value: "1,2,3".to_string(),
            },
        ];
        assert!(validate_label_filters(&filters).is_ok());
    }

    #[test]
    fn validate_label_filters_rejects_excessive_count() {
        let filters: Vec<_> = (0..(MAX_LABEL_FILTERS + 1))
            .map(|_| sdkwork_discovery_contract::LabelFilter {
                key: "k".to_string(),
                op: LabelFilterOp::Eq,
                value: "v".to_string(),
            })
            .collect();
        let err = validate_label_filters(&filters).unwrap_err();
        assert!(matches!(err, DiscoveryError::InvalidArgument(msg) if msg.contains("32")));
    }

    #[test]
    fn validate_label_filters_rejects_invalid_key_charset() {
        let filters = vec![sdkwork_discovery_contract::LabelFilter {
            key: "bad key".to_string(),
            op: LabelFilterOp::Eq,
            value: "v".to_string(),
        }];
        let err = validate_label_filters(&filters).unwrap_err();
        assert!(
            matches!(err, DiscoveryError::InvalidArgument(msg) if msg.contains("invalid character"))
        );
    }
}
