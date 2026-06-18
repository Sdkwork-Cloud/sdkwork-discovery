use std::collections::HashMap;

use sdkwork_discovery_contract::{
    ConfigDraft, ConfigFormat, ConfigRelease, ConfigScope, DiscoveryError, DiscoveryEvent,
    DiscoveryEventKind, DiscoveryResult, ServiceInstance,
};
use sqlx::postgres::PgRow;
use sqlx::Row;

use crate::validation::{i64_to_u64, u32_to_i32};

pub(crate) fn new_uuid() -> String {
    uuid::Uuid::now_v7().to_string()
}

pub(crate) fn new_prefixed_id(prefix: &str) -> String {
    format!("{prefix}-{}", uuid::Uuid::now_v7())
}

pub(crate) fn encode_instance_status(
    status: &sdkwork_discovery_contract::InstanceStatus,
) -> &'static str {
    match status {
        sdkwork_discovery_contract::InstanceStatus::Serving => "serving",
        sdkwork_discovery_contract::InstanceStatus::Degraded => "degraded",
        sdkwork_discovery_contract::InstanceStatus::NotServing => "not_serving",
    }
}

pub(crate) fn decode_instance_status(
    status: &str,
) -> DiscoveryResult<sdkwork_discovery_contract::InstanceStatus> {
    match status {
        "serving" => Ok(sdkwork_discovery_contract::InstanceStatus::Serving),
        "degraded" => Ok(sdkwork_discovery_contract::InstanceStatus::Degraded),
        "not_serving" => Ok(sdkwork_discovery_contract::InstanceStatus::NotServing),
        _ => Err(DiscoveryError::InvalidConfig(format!(
            "postgres stored unknown instance status: {status}"
        ))),
    }
}

pub(crate) fn encode_config_format(format: &ConfigFormat) -> &'static str {
    match format {
        ConfigFormat::Text => "text",
        ConfigFormat::Json => "json",
        ConfigFormat::Toml => "toml",
    }
}

pub(crate) fn decode_config_format(format: &str) -> DiscoveryResult<ConfigFormat> {
    match format {
        "text" => Ok(ConfigFormat::Text),
        "json" => Ok(ConfigFormat::Json),
        "toml" => Ok(ConfigFormat::Toml),
        _ => Err(DiscoveryError::InvalidConfig(format!(
            "postgres stored unknown config format: {format}"
        ))),
    }
}

pub(crate) fn encode_config_scope(
    scope: &ConfigScope,
) -> (&'static str, Option<&str>, Option<&str>) {
    match scope {
        ConfigScope::Namespace => ("namespace", None, None),
        ConfigScope::Application { application } => ("application", Some(application), None),
        ConfigScope::Service {
            application,
            service_name,
        } => ("service", Some(application), Some(service_name)),
    }
}

pub(crate) fn decode_config_scope(
    kind: &str,
    application: Option<String>,
    service_name: Option<String>,
) -> DiscoveryResult<ConfigScope> {
    match kind {
        "namespace" => Ok(ConfigScope::Namespace),
        "application" => Ok(ConfigScope::Application {
            application: application.ok_or_else(|| {
                DiscoveryError::InvalidConfig(
                    "postgres application config scope is missing application".to_string(),
                )
            })?,
        }),
        "service" => Ok(ConfigScope::Service {
            application: application.ok_or_else(|| {
                DiscoveryError::InvalidConfig(
                    "postgres service config scope is missing application".to_string(),
                )
            })?,
            service_name: service_name.ok_or_else(|| {
                DiscoveryError::InvalidConfig(
                    "postgres service config scope is missing service_name".to_string(),
                )
            })?,
        }),
        _ => Err(DiscoveryError::InvalidConfig(format!(
            "postgres stored unknown config scope: {kind}"
        ))),
    }
}

pub(crate) fn encode_event_kind(kind: &DiscoveryEventKind) -> &'static str {
    match kind {
        DiscoveryEventKind::InstanceRegistered => "instance_registered",
        DiscoveryEventKind::InstanceUpdated => "instance_updated",
        DiscoveryEventKind::InstanceStatusReported => "instance_status_reported",
        DiscoveryEventKind::InstanceRenewed => "instance_renewed",
        DiscoveryEventKind::InstanceDeregistered => "instance_deregistered",
        DiscoveryEventKind::ConfigPublished => "config_published",
        DiscoveryEventKind::ConfigRolledBack => "config_rolled_back",
    }
}

pub(crate) fn decode_event_kind(kind: &str) -> DiscoveryResult<DiscoveryEventKind> {
    match kind {
        "instance_registered" => Ok(DiscoveryEventKind::InstanceRegistered),
        "instance_updated" => Ok(DiscoveryEventKind::InstanceUpdated),
        "instance_status_reported" => Ok(DiscoveryEventKind::InstanceStatusReported),
        "instance_renewed" => Ok(DiscoveryEventKind::InstanceRenewed),
        "instance_deregistered" => Ok(DiscoveryEventKind::InstanceDeregistered),
        "config_published" => Ok(DiscoveryEventKind::ConfigPublished),
        "config_rolled_back" => Ok(DiscoveryEventKind::ConfigRolledBack),
        _ => Err(DiscoveryError::InvalidConfig(format!(
            "postgres stored unknown event kind: {kind}"
        ))),
    }
}

pub(crate) fn metadata_to_json(metadata: &HashMap<String, String>) -> DiscoveryResult<String> {
    serde_json::to_string(metadata)
        .map_err(|error| DiscoveryError::InvalidArgument(format!("invalid metadata: {error}")))
}

pub(crate) fn metadata_from_json(input: &str) -> DiscoveryResult<HashMap<String, String>> {
    serde_json::from_str(input).map_err(|error| {
        DiscoveryError::InvalidConfig(format!("postgres stored invalid metadata json: {error}"))
    })
}

pub(crate) fn health_check_to_json(
    health_check: &Option<sdkwork_discovery_contract::HealthCheckConfig>,
) -> DiscoveryResult<Option<String>> {
    match health_check {
        None => Ok(None),
        Some(config) => serde_json::to_string(config).map(Some).map_err(|error| {
            DiscoveryError::InvalidArgument(format!("invalid health_check config: {error}"))
        }),
    }
}

pub(crate) fn health_check_state_to_json(
    state: &sdkwork_discovery_contract::HealthCheckRuntimeState,
) -> DiscoveryResult<String> {
    serde_json::to_string(state).map_err(|error| {
        DiscoveryError::InvalidArgument(format!("invalid health_check_state: {error}"))
    })
}

pub(crate) fn service_instance_from_row(row: &PgRow) -> DiscoveryResult<ServiceInstance> {
    let status: String = row.try_get("status").map_err(sqlx_error)?;
    let metadata_json: String = row.try_get("metadata_json_text").map_err(sqlx_error)?;
    let expires_at_ms: i64 = row.try_get("expires_at_ms").map_err(sqlx_error)?;
    let revision: i64 = row.try_get("revision").map_err(sqlx_error)?;
    let weight: i32 = row.try_get("weight").map_err(sqlx_error)?;
    let priority: i32 = row.try_get("priority").map_err(sqlx_error)?;

    Ok(ServiceInstance {
        namespace: row.try_get("namespace").map_err(sqlx_error)?,
        environment: row.try_get("environment").map_err(sqlx_error)?,
        service_name: row.try_get("service_name").map_err(sqlx_error)?,
        instance_id: row.try_get("instance_id").map_err(sqlx_error)?,
        endpoint: row.try_get("endpoint").map_err(sqlx_error)?,
        protocol: row.try_get("protocol").map_err(sqlx_error)?,
        version: row.try_get("service_version").map_err(sqlx_error)?,
        region: row.try_get("region").map_err(sqlx_error)?,
        zone: row.try_get("zone").map_err(sqlx_error)?,
        weight: u32::try_from(weight).map_err(|_| {
            DiscoveryError::InvalidConfig("postgres returned negative weight".to_string())
        })?,
        priority: u32::try_from(priority).map_err(|_| {
            DiscoveryError::InvalidConfig("postgres returned negative priority".to_string())
        })?,
        status: decode_instance_status(&status)?,
        metadata: metadata_from_json(&metadata_json)?,
        lease_id: row.try_get("lease_id").map_err(sqlx_error)?,
        expires_at_ms: i64_to_u64("expires_at_ms", expires_at_ms)?,
        revision: i64_to_u64("revision", revision)?,
        health_check: health_check_from_json_text(
            row.try_get::<Option<String>, _>("health_check_json_text")
                .ok()
                .flatten()
                .as_deref(),
        )?,
        health_check_state: health_check_state_from_json_text(
            row.try_get::<Option<String>, _>("health_check_state_json_text")
                .ok()
                .flatten()
                .as_deref(),
        )?,
    })
}

fn health_check_from_json_text(
    input: Option<&str>,
) -> DiscoveryResult<Option<sdkwork_discovery_contract::HealthCheckConfig>> {
    let Some(input) = input.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    serde_json::from_str(input).map_err(|error| {
        DiscoveryError::InvalidConfig(format!(
            "postgres stored invalid health_check json: {error}"
        ))
    })
}

fn health_check_state_from_json_text(
    input: Option<&str>,
) -> DiscoveryResult<sdkwork_discovery_contract::HealthCheckRuntimeState> {
    let Some(input) = input.filter(|value| !value.is_empty()) else {
        return Ok(sdkwork_discovery_contract::HealthCheckRuntimeState::default());
    };
    serde_json::from_str(input).map_err(|error| {
        DiscoveryError::InvalidConfig(format!(
            "postgres stored invalid health_check_state json: {error}"
        ))
    })
}

pub(crate) fn draft_from_row(row: &PgRow) -> DiscoveryResult<ConfigDraft> {
    let format: String = row.try_get("config_format").map_err(sqlx_error)?;
    let scope_kind: String = row.try_get("scope_kind").map_err(sqlx_error)?;
    let scope_application: Option<String> = row.try_get("scope_application").map_err(sqlx_error)?;
    let scope_service_name: Option<String> =
        row.try_get("scope_service_name").map_err(sqlx_error)?;

    Ok(ConfigDraft {
        draft_id: row.try_get("draft_id").map_err(sqlx_error)?,
        namespace: row.try_get("namespace").map_err(sqlx_error)?,
        environment: row.try_get("environment").map_err(sqlx_error)?,
        group: row.try_get("config_group").map_err(sqlx_error)?,
        key: row.try_get("config_key").map_err(sqlx_error)?,
        format: decode_config_format(&format)?,
        value: row.try_get("config_value").map_err(sqlx_error)?,
        scope: decode_config_scope(&scope_kind, scope_application, scope_service_name)?,
        created_by: row.try_get("created_by").map_err(sqlx_error)?,
        content_hash: row.try_get("content_hash").map_err(sqlx_error)?,
        published: row.try_get("published").map_err(sqlx_error)?,
    })
}

pub(crate) fn release_from_row(row: &PgRow) -> DiscoveryResult<ConfigRelease> {
    let format: String = row.try_get("config_format").map_err(sqlx_error)?;
    let scope_kind: String = row.try_get("scope_kind").map_err(sqlx_error)?;
    let scope_application: Option<String> = row.try_get("scope_application").map_err(sqlx_error)?;
    let scope_service_name: Option<String> =
        row.try_get("scope_service_name").map_err(sqlx_error)?;
    let published_at_ms: i64 = row.try_get("published_at_ms").map_err(sqlx_error)?;
    let revision: i64 = row.try_get("revision").map_err(sqlx_error)?;

    Ok(ConfigRelease {
        release_id: row.try_get("release_id").map_err(sqlx_error)?,
        draft_id: row.try_get("draft_id").map_err(sqlx_error)?,
        namespace: row.try_get("namespace").map_err(sqlx_error)?,
        environment: row.try_get("environment").map_err(sqlx_error)?,
        group: row.try_get("config_group").map_err(sqlx_error)?,
        key: row.try_get("config_key").map_err(sqlx_error)?,
        format: decode_config_format(&format)?,
        value: row.try_get("config_value").map_err(sqlx_error)?,
        scope: decode_config_scope(&scope_kind, scope_application, scope_service_name)?,
        content_hash: row.try_get("content_hash").map_err(sqlx_error)?,
        published_by: row.try_get("published_by").map_err(sqlx_error)?,
        published_at_ms: i64_to_u64("published_at_ms", published_at_ms)?,
        revision: i64_to_u64("revision", revision)?,
    })
}

pub(crate) fn event_from_row(row: &PgRow) -> DiscoveryResult<DiscoveryEvent> {
    let revision: i64 = row.try_get("revision").map_err(sqlx_error)?;
    let event_kind: String = row.try_get("event_kind").map_err(sqlx_error)?;

    Ok(DiscoveryEvent {
        revision: i64_to_u64("revision", revision)?,
        namespace: row.try_get("namespace").map_err(sqlx_error)?,
        environment: row.try_get("environment").map_err(sqlx_error)?,
        kind: decode_event_kind(&event_kind)?,
        resource_id: row.try_get("resource_id").map_err(sqlx_error)?,
        service_name: row.try_get("service_name").map_err(sqlx_error)?,
        config_group: row.try_get("config_group").map_err(sqlx_error)?,
        config_key: row.try_get("config_key").map_err(sqlx_error)?,
        config_application: row.try_get("config_application").map_err(sqlx_error)?,
    })
}

pub(crate) fn sqlx_error(error: sqlx::Error) -> DiscoveryError {
    DiscoveryError::InvalidConfig(format!("postgres storage error: {error}"))
}

pub(crate) fn bind_weight(value: u32) -> DiscoveryResult<i32> {
    u32_to_i32("weight", value)
}

pub(crate) fn bind_priority(value: u32) -> DiscoveryResult<i32> {
    u32_to_i32("priority", value)
}
