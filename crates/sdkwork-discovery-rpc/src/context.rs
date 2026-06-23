use sdkwork_discovery_contract::{
    CallerContext, ConfigPermission, DiscoveryError, DiscoveryResult, IdempotencyContext,
    RegistryPermission,
};
use tonic::metadata::MetadataMap;

use crate::service_token::ServiceTokenVerifier;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct RpcContextPolicy {
    pub allow_unsigned_local_context: bool,
    pub service_token_verifier: Option<ServiceTokenVerifier>,
}

pub fn caller_from_metadata(
    metadata: &MetadataMap,
    policy: RpcContextPolicy,
) -> DiscoveryResult<CallerContext> {
    if policy.allow_unsigned_local_context && has_unsigned_context_metadata(metadata) {
        if policy.service_token_verifier.is_some() {
            return Err(DiscoveryError::Unauthenticated(
                "unsigned context metadata is not accepted with verified service-token authentication"
                    .to_string(),
            ));
        }
        return caller_from_validated_metadata(metadata);
    }

    let authenticated = authenticated_metadata(metadata)?;
    caller_from_authenticated_metadata(metadata, policy, authenticated)
}

pub fn caller_from_metadata_with_required_idempotency(
    metadata: &MetadataMap,
    policy: RpcContextPolicy,
) -> DiscoveryResult<CallerContext> {
    if policy.allow_unsigned_local_context && has_unsigned_context_metadata(metadata) {
        if policy.service_token_verifier.is_some() {
            return Err(DiscoveryError::Unauthenticated(
                "unsigned context metadata is not accepted with verified service-token authentication"
                    .to_string(),
            ));
        }
        validate_required_idempotency_metadata(metadata)?;
        return caller_from_validated_metadata(metadata);
    }

    let authenticated = authenticated_metadata(metadata)?;
    validate_required_idempotency_metadata(metadata)?;
    caller_from_authenticated_metadata(metadata, policy, authenticated)
}

pub fn registry_reader_from_metadata(
    metadata: &MetadataMap,
    policy: RpcContextPolicy,
) -> DiscoveryResult<CallerContext> {
    let caller = caller_from_metadata(metadata, policy)?;
    if caller.has_registry_permission(RegistryPermission::Read) {
        Ok(caller)
    } else {
        Err(DiscoveryError::PermissionDenied(format!(
            "subject {} lacks registry permission",
            caller.subject_id
        )))
    }
}

pub fn config_reader_from_metadata(
    metadata: &MetadataMap,
    policy: RpcContextPolicy,
) -> DiscoveryResult<CallerContext> {
    let caller = caller_from_metadata(metadata, policy)?;
    if caller.has_config_permission(ConfigPermission::Read) {
        Ok(caller)
    } else {
        Err(DiscoveryError::PermissionDenied(format!(
            "subject {} lacks config permission",
            caller.subject_id
        )))
    }
}

fn validate_unsigned_local_context_policy(policy: RpcContextPolicy) -> DiscoveryResult<()> {
    if policy.allow_unsigned_local_context {
        return Ok(());
    }

    Err(DiscoveryError::Unauthenticated(
        "unsigned local context is disabled; a verified service-token context resolver is required"
            .to_string(),
    ))
}

fn caller_from_authenticated_metadata(
    metadata: &MetadataMap,
    policy: RpcContextPolicy,
    authenticated: AuthenticatedMetadata,
) -> DiscoveryResult<CallerContext> {
    let has_unsigned = has_unsigned_context_metadata(metadata);

    if let Some(verifier) = policy.service_token_verifier.as_ref() {
        if has_unsigned {
            return Err(DiscoveryError::Unauthenticated(
                "unsigned context metadata is not accepted with verified service-token authentication"
                    .to_string(),
            ));
        }

        return verifier.verify(&authenticated.bearer_token, &authenticated.access_token);
    }

    if !policy.allow_unsigned_local_context {
        return Err(DiscoveryError::Unauthenticated(
            "unsigned local context is disabled; a verified service-token context resolver is required"
                .to_string(),
        ));
    }

    validate_unsigned_local_context_policy(policy)?;
    caller_from_validated_metadata(metadata)
}

fn caller_from_validated_metadata(metadata: &MetadataMap) -> DiscoveryResult<CallerContext> {
    let subject_id = optional_metadata_value(metadata, "x-sdkwork-subject-id")?
        .map(|value| value.trim().to_string())
        .unwrap_or_default();

    if subject_id.is_empty() {
        return Err(DiscoveryError::Unauthenticated(
            "missing required RPC metadata: x-sdkwork-subject-id".to_string(),
        ));
    }

    let mut caller = CallerContext::new(subject_id);

    if let Some(tenant_id) = optional_metadata_value(metadata, "x-sdkwork-tenant-id")? {
        caller = caller.with_tenant_id(tenant_id);
    }

    if let Some(organization_id) = optional_metadata_value(metadata, "x-sdkwork-organization-id")? {
        caller = caller.with_organization_id(organization_id);
    }

    for permission in split_permissions(
        optional_metadata_value(metadata, "x-sdkwork-registry-permissions")?.as_deref(),
    ) {
        caller = caller.with_registry_permission(parse_registry_permission(permission)?);
    }

    for permission in split_permissions(
        optional_metadata_value(metadata, "x-sdkwork-config-permissions")?.as_deref(),
    ) {
        caller = caller.with_config_permission(parse_config_permission(permission)?);
    }

    Ok(caller)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AuthenticatedMetadata {
    bearer_token: String,
    access_token: String,
}

fn authenticated_metadata(metadata: &MetadataMap) -> DiscoveryResult<AuthenticatedMetadata> {
    let authorization = optional_metadata_value(metadata, "authorization")?;
    let access_token = optional_metadata_value(metadata, "access-token")?;
    let mut missing = Vec::new();

    let bearer_token = match authorization.as_deref().map(str::trim) {
        Some(value) => match bearer_token(value) {
            Some(token) => Some(token.to_string()),
            None => {
                return Err(DiscoveryError::Unauthenticated(
                    "authorization metadata must use Bearer token format".to_string(),
                ));
            }
        },
        None => {
            missing.push("authorization");
            None
        }
    };

    let access_token = access_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    if access_token.is_none() {
        missing.push("access-token");
    }

    if !missing.is_empty() {
        return Err(DiscoveryError::Unauthenticated(format!(
            "missing required RPC metadata: {}",
            missing.join(", ")
        )));
    }

    Ok(AuthenticatedMetadata {
        bearer_token: bearer_token.expect("bearer token is present when metadata is valid"),
        access_token: access_token.expect("access token is present when metadata is valid"),
    })
}

fn validate_required_idempotency_metadata(metadata: &MetadataMap) -> DiscoveryResult<()> {
    let idempotency_key = optional_metadata_value(metadata, "idempotency-key")?;
    let request_hash = optional_metadata_value(metadata, "x-request-hash")?;
    let mut missing = Vec::new();

    if idempotency_key
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        missing.push("idempotency-key");
    }

    if request_hash
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        missing.push("x-request-hash");
    }

    if missing.is_empty() {
        return Ok(());
    }

    Err(DiscoveryError::InvalidArgument(format!(
        "missing required RPC idempotency metadata: {}",
        missing.join(", ")
    )))
}

fn bearer_token(value: &str) -> Option<&str> {
    let token = value.strip_prefix("Bearer ")?;
    let token = token.trim();
    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

pub fn request_id_from_metadata(metadata: &MetadataMap) -> DiscoveryResult<String> {
    if let Some(request_id) = optional_metadata_value(metadata, "x-request-id")? {
        let trimmed = request_id.trim();
        if !trimmed.is_empty() {
            validate_request_id(trimmed)?;
            return Ok(trimmed.to_string());
        }
    }

    Ok(generate_request_id())
}

fn validate_request_id(value: &str) -> DiscoveryResult<()> {
    const MAX_REQUEST_ID_LEN: usize = 128;

    if value.len() > MAX_REQUEST_ID_LEN {
        return Err(DiscoveryError::InvalidArgument(format!(
            "x-request-id must be at most {MAX_REQUEST_ID_LEN} characters"
        )));
    }

    Ok(())
}

pub fn trace_id_from_metadata(metadata: &MetadataMap) -> DiscoveryResult<String> {
    let traceparent = optional_metadata_value(metadata, "traceparent")?.unwrap_or_default();
    Ok(parse_trace_id_from_traceparent(&traceparent).unwrap_or_default())
}

pub fn parse_trace_id_from_traceparent(traceparent: &str) -> Option<String> {
    let trimmed = traceparent.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut parts = trimmed.split('-');
    let _version = parts.next()?;
    let trace_id = parts.next()?;
    if trace_id.len() != 32 || !trace_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }

    Some(trace_id.to_ascii_lowercase())
}

pub fn idempotency_from_metadata(
    metadata: &MetadataMap,
    operation_id: &'static str,
) -> DiscoveryResult<IdempotencyContext> {
    let key = optional_metadata_value(metadata, "idempotency-key")?
        .map(|value| value.trim().to_string())
        .unwrap_or_default();
    let request_hash = optional_metadata_value(metadata, "x-request-hash")?
        .map(|value| value.trim().to_string())
        .unwrap_or_default();
    Ok(IdempotencyContext::new(operation_id, key, request_hash))
}

fn optional_metadata_value(
    metadata: &MetadataMap,
    key: &'static str,
) -> DiscoveryResult<Option<String>> {
    metadata
        .get(key)
        .map(|value| {
            value.to_str().map(str::to_string).map_err(|_| {
                DiscoveryError::InvalidArgument(format!("metadata {key} must be visible ASCII"))
            })
        })
        .transpose()
}

fn has_unsigned_context_metadata(metadata: &MetadataMap) -> bool {
    metadata.get("x-sdkwork-subject-id").is_some()
        || metadata.get("x-sdkwork-registry-permissions").is_some()
        || metadata.get("x-sdkwork-config-permissions").is_some()
}

fn generate_request_id() -> String {
    format!("req_{}", uuid::Uuid::now_v7().simple())
}

fn split_permissions(value: Option<&str>) -> impl Iterator<Item = &str> {
    value
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn parse_registry_permission(value: &str) -> DiscoveryResult<RegistryPermission> {
    match value {
        "read" => Ok(RegistryPermission::Read),
        "write" => Ok(RegistryPermission::Write),
        "admin" => Ok(RegistryPermission::Admin),
        _ => Err(DiscoveryError::InvalidArgument(format!(
            "unknown registry permission {value}"
        ))),
    }
}

fn parse_config_permission(value: &str) -> DiscoveryResult<ConfigPermission> {
    match value {
        "read" => Ok(ConfigPermission::Read),
        "write" => Ok(ConfigPermission::Write),
        "publish" => Ok(ConfigPermission::Publish),
        "rollback" => Ok(ConfigPermission::Rollback),
        "admin" => Ok(ConfigPermission::Admin),
        _ => Err(DiscoveryError::InvalidArgument(format!(
            "unknown config permission {value}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        caller_from_metadata, parse_trace_id_from_traceparent, request_id_from_metadata,
        validate_request_id, RpcContextPolicy,
    };
    use sdkwork_discovery_contract::{DiscoveryError, RegistryPermission};
    use tonic::metadata::MetadataMap;

    #[test]
    fn parse_trace_id_from_valid_traceparent() {
        assert_eq!(
            parse_trace_id_from_traceparent(
                "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01"
            )
            .as_deref(),
            Some("0af7651916cd43dd8448eb211c80319c")
        );
    }

    #[test]
    fn parse_trace_id_rejects_invalid_traceparent() {
        assert_eq!(parse_trace_id_from_traceparent("not-a-traceparent"), None);
        assert_eq!(parse_trace_id_from_traceparent(""), None);
    }

    #[test]
    fn request_id_uses_gateway_metadata_when_present() {
        let mut metadata = MetadataMap::new();
        metadata.insert("x-request-id", "req-gateway-1".parse().unwrap());

        assert_eq!(
            request_id_from_metadata(&metadata).unwrap(),
            "req-gateway-1"
        );
    }

    #[test]
    fn request_id_generates_server_owned_id_when_metadata_missing() {
        let metadata = MetadataMap::new();
        let request_id = request_id_from_metadata(&metadata).unwrap();

        assert!(request_id.starts_with("req_"));
        assert!(request_id.len() > "req_".len());
    }

    #[test]
    fn request_id_generates_server_owned_id_when_metadata_blank() {
        let mut metadata = MetadataMap::new();
        metadata.insert("x-request-id", "   ".parse().unwrap());

        let request_id = request_id_from_metadata(&metadata).unwrap();
        assert!(request_id.starts_with("req_"));
    }

    #[test]
    fn request_id_rejects_overlong_metadata_value() {
        let mut metadata = MetadataMap::new();
        metadata.insert("x-request-id", "x".repeat(129).parse().unwrap());

        let error = request_id_from_metadata(&metadata).unwrap_err();
        assert!(matches!(error, DiscoveryError::InvalidArgument(_)));
        assert!(error.to_string().contains("x-request-id"));
    }

    #[test]
    fn validate_request_id_accepts_bounded_gateway_values() {
        assert!(validate_request_id("req-gateway-1").is_ok());
        assert!(validate_request_id(&"x".repeat(128)).is_ok());
        assert!(validate_request_id(&"x".repeat(129)).is_err());
    }

    #[test]
    fn caller_accepts_unsigned_local_context_without_dual_tokens_when_enabled() {
        let mut metadata = MetadataMap::new();
        metadata.insert("x-sdkwork-subject-id", "service-1".parse().unwrap());
        metadata.insert("x-sdkwork-registry-permissions", "write".parse().unwrap());

        let caller = caller_from_metadata(
            &metadata,
            RpcContextPolicy {
                allow_unsigned_local_context: true,
                service_token_verifier: None,
            },
        )
        .unwrap();

        assert_eq!(caller.subject_id, "service-1");
        assert!(caller.has_registry_permission(RegistryPermission::Write));
    }

    #[test]
    fn caller_rejects_missing_dual_tokens_when_unsigned_context_disabled() {
        let mut metadata = MetadataMap::new();
        metadata.insert("x-sdkwork-subject-id", "service-1".parse().unwrap());
        metadata.insert("x-sdkwork-registry-permissions", "write".parse().unwrap());

        let error = caller_from_metadata(
            &metadata,
            RpcContextPolicy {
                allow_unsigned_local_context: false,
                service_token_verifier: None,
            },
        )
        .unwrap_err();

        assert!(matches!(error, DiscoveryError::Unauthenticated(_)));
        let message = error.to_string();
        assert!(message.contains("authorization"));
        assert!(message.contains("access-token"));
    }
}
