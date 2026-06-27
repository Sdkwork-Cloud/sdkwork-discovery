use sdkwork_discovery_contract::DiscoveryError;
use tonic::Status;
use tracing::warn;

pub fn map_discovery_error_to_status(error: DiscoveryError) -> Status {
    map_discovery_error_to_rpc_status(error, "", "")
}

pub fn map_discovery_error_to_rpc_status(
    error: DiscoveryError,
    request_id: &str,
    trace_id: &str,
) -> Status {
    // Log the original (unsanitized) message for service-side observability, then
    // return a sanitized external message to the gRPC client. Storage layer errors
    // (sqlx/redis) embed driver diagnostics that may leak SQL text, table names,
    // connection URLs, or filesystem paths; those MUST NOT reach callers.
    let status = match error {
        DiscoveryError::Unauthenticated(ref message) => {
            warn!(error = %message, "unauthenticated request");
            Status::unauthenticated(sanitize_external_message(&error))
        }
        DiscoveryError::InvalidConfig(ref message) => {
            warn!(error = %message, "invalid config");
            Status::failed_precondition(sanitize_external_message(&error))
        }
        DiscoveryError::InvalidArgument(ref message) => {
            warn!(error = %message, "invalid argument");
            Status::invalid_argument(sanitize_external_message(&error))
        }
        DiscoveryError::NotFound(ref message) => {
            warn!(error = %message, "resource not found");
            Status::not_found(sanitize_external_message(&error))
        }
        DiscoveryError::AlreadyPublished(ref message) => {
            warn!(error = %message, "already published");
            Status::already_exists(sanitize_external_message(&error))
        }
        DiscoveryError::PermissionDenied(ref message) => {
            warn!(error = %message, "permission denied");
            Status::permission_denied(sanitize_external_message(&error))
        }
        DiscoveryError::PolicyViolation(ref message) => {
            warn!(error = %message, "policy violation");
            Status::permission_denied(sanitize_external_message(&error))
        }
        DiscoveryError::Conflict(ref message) => {
            warn!(error = %message, "conflict");
            Status::aborted(sanitize_external_message(&error))
        }
        DiscoveryError::Unavailable(ref message) => {
            warn!(error = %message, "service unavailable");
            Status::unavailable(sanitize_external_message(&error))
        }
        DiscoveryError::ResourceExhausted(ref message) => {
            warn!(error = %message, "resource exhausted");
            Status::resource_exhausted(sanitize_external_message(&error))
        }
    };

    attach_correlation_metadata(status, request_id, trace_id)
}

/// Produce a safe external message for a [`DiscoveryError`].
///
/// Caller-facing messages are classified into tiers:
/// 1. **Caller-input errors** (`InvalidArgument`, `NotFound`, `AlreadyPublished`,
///    `Conflict`): preserve the developer-facing diagnostic wording. These
///    messages are constructed with safe field-name wording in the codec layer
///    and do not carry runtime subject identifiers or secrets.
/// 2. **AuthN/AuthZ errors** (`Unauthenticated`, `PermissionDenied`,
///    `PolicyViolation`): preserve the developer-facing diagnostic wording
///    (e.g. "missing authorization header", "missing x-sdkwork-subject-id
///    metadata"). These messages identify which header or permission is missing
///    so callers can correct the request; they do not carry runtime subject
///    identifiers, tenant ids, or secrets.
/// 3. **Configuration / policy gate errors** (`InvalidConfig`): preserve the
///    message. After the storage-layer error redirect (sqlx/redis errors now
///    map to `Unavailable`), `InvalidConfig` is only used for startup-time
///    configuration defects and runtime policy gates (e.g. "config registry is
///    disabled"), all of which are safe developer-facing diagnostics.
/// 4. **Runtime infrastructure failures** (`Unavailable`,
///    `ResourceExhausted`): return a fixed string. Storage drivers (sqlx/redis)
///    embed SQL text, table names, connection URLs, or filesystem paths in
///    these variants, which would leak infrastructure topology if echoed
///    verbatim. The original driver message is preserved in server-side logs by
///    the `warn!` call in the mapper.
fn sanitize_external_message(error: &DiscoveryError) -> String {
    match error {
        DiscoveryError::InvalidArgument(message)
        | DiscoveryError::NotFound(message)
        | DiscoveryError::AlreadyPublished(message)
        | DiscoveryError::Conflict(message)
        | DiscoveryError::Unauthenticated(message)
        | DiscoveryError::PermissionDenied(message)
        | DiscoveryError::PolicyViolation(message)
        | DiscoveryError::InvalidConfig(message) => message.clone(),
        DiscoveryError::Unavailable(_) => "service unavailable".to_string(),
        DiscoveryError::ResourceExhausted(_) => "resource exhausted".to_string(),
    }
}

pub fn attach_rpc_correlation_metadata(status: Status, request_id: &str, trace_id: &str) -> Status {
    attach_correlation_metadata(status, request_id, trace_id)
}

pub fn grpc_status_code_for_discovery_error(error: &DiscoveryError) -> &'static str {
    match error {
        DiscoveryError::Unauthenticated(_) => "UNAUTHENTICATED",
        DiscoveryError::InvalidConfig(_) => "FAILED_PRECONDITION",
        DiscoveryError::InvalidArgument(_) => "INVALID_ARGUMENT",
        DiscoveryError::NotFound(_) => "NOT_FOUND",
        DiscoveryError::AlreadyPublished(_) => "ALREADY_EXISTS",
        DiscoveryError::PermissionDenied(_) => "PERMISSION_DENIED",
        DiscoveryError::PolicyViolation(_) => "PERMISSION_DENIED",
        DiscoveryError::Conflict(_) => "ABORTED",
        DiscoveryError::Unavailable(_) => "UNAVAILABLE",
        DiscoveryError::ResourceExhausted(_) => "RESOURCE_EXHAUSTED",
    }
}

fn attach_correlation_metadata(mut status: Status, request_id: &str, trace_id: &str) -> Status {
    let metadata = status.metadata_mut();
    if !trace_id.is_empty() {
        if let Ok(value) = trace_id.parse() {
            metadata.insert("x-trace-id", value);
        }
    }
    if !request_id.is_empty() {
        if let Ok(value) = request_id.parse() {
            metadata.insert("x-request-id", value);
        }
    }
    status
}

#[cfg(test)]
mod tests {
    use super::{
        attach_correlation_metadata, grpc_status_code_for_discovery_error,
        map_discovery_error_to_rpc_status, sanitize_external_message,
    };
    use sdkwork_discovery_contract::DiscoveryError;
    use tonic::Code;

    #[test]
    fn rpc_status_includes_correlation_metadata() {
        let status = map_discovery_error_to_rpc_status(
            DiscoveryError::NotFound("missing".to_string()),
            "req_test",
            "0af7651916cd43dd8448eb211c80319c",
        );

        assert_eq!(status.code(), Code::NotFound);
        assert_eq!(
            status
                .metadata()
                .get("x-trace-id")
                .unwrap()
                .to_str()
                .unwrap(),
            "0af7651916cd43dd8448eb211c80319c"
        );
        assert_eq!(
            status
                .metadata()
                .get("x-request-id")
                .unwrap()
                .to_str()
                .unwrap(),
            "req_test"
        );
    }

    #[test]
    fn grpc_status_code_matches_mapped_rpc_status() {
        let error = DiscoveryError::InvalidArgument("bad".to_string());
        let status = map_discovery_error_to_rpc_status(error.clone(), "req-1", "trace-1");

        assert_eq!(status.code(), Code::InvalidArgument);
        assert_eq!(
            grpc_status_code_for_discovery_error(&error),
            "INVALID_ARGUMENT"
        );
    }

    #[test]
    fn attach_correlation_metadata_skips_empty_values() {
        let status = attach_correlation_metadata(tonic::Status::invalid_argument("bad"), "", "");
        assert!(status.metadata().get("x-trace-id").is_none());
        assert!(status.metadata().get("x-request-id").is_none());
    }

    /// Caller-input errors preserve their developer-facing diagnostic wording so
    /// callers can correct the request.
    #[test]
    fn sanitize_preserves_caller_input_error_messages() {
        assert_eq!(
            sanitize_external_message(&DiscoveryError::InvalidArgument(
                "namespace must not be empty".to_string()
            )),
            "namespace must not be empty"
        );
        assert_eq!(
            sanitize_external_message(&DiscoveryError::NotFound("instance not found".to_string())),
            "instance not found"
        );
        assert_eq!(
            sanitize_external_message(&DiscoveryError::AlreadyPublished(
                "draft already published".to_string()
            )),
            "draft already published"
        );
        assert_eq!(
            sanitize_external_message(&DiscoveryError::Conflict("revision mismatch".to_string())),
            "revision mismatch"
        );
    }

    /// AuthN/AuthZ errors preserve the diagnostic wording (which header or
    /// permission is missing) without echoing runtime subject identifiers.
    #[test]
    fn sanitize_preserves_authn_authz_diagnostic_wording() {
        assert_eq!(
            sanitize_external_message(&DiscoveryError::Unauthenticated(
                "missing authorization header".to_string()
            )),
            "missing authorization header"
        );
        assert_eq!(
            sanitize_external_message(&DiscoveryError::PermissionDenied(
                "missing registry permission".to_string()
            )),
            "missing registry permission"
        );
        assert_eq!(
            sanitize_external_message(&DiscoveryError::PolicyViolation(
                "config body exceeds configured maximum size".to_string()
            )),
            "config body exceeds configured maximum size"
        );
    }

    /// Policy-gate and configuration errors preserve their message because they
    /// are safe developer-facing diagnostics (e.g. "config registry is
    /// disabled"), not storage-driver diagnostics.
    #[test]
    fn sanitize_preserves_policy_gate_and_config_error_messages() {
        assert_eq!(
            sanitize_external_message(&DiscoveryError::InvalidConfig(
                "config registry is disabled".to_string()
            )),
            "config registry is disabled"
        );
    }

    /// Storage-driver and runtime infrastructure failures MUST NOT echo the
    /// original message, which may carry SQL text, table names, connection URLs,
    /// or filesystem paths. The original message is preserved only in
    /// server-side `warn!` logs.
    #[test]
    fn sanitize_masks_runtime_infrastructure_failure_messages() {
        let postgres_message = sanitize_external_message(&DiscoveryError::Unavailable(
            "postgres storage error: connection refused to postgres://user:pass@host:5432/db"
                .to_string(),
        ));
        assert_eq!(postgres_message, "service unavailable");
        assert!(!postgres_message.contains("postgres://"));
        assert!(!postgres_message.contains("user"));
        assert!(!postgres_message.contains("pass"));

        let redis_message = sanitize_external_message(&DiscoveryError::Unavailable(
            "redis error: AUTH failed against redis://host:6379".to_string(),
        ));
        assert_eq!(redis_message, "service unavailable");
        assert!(!redis_message.contains("redis://"));
        assert!(!redis_message.contains("AUTH"));

        let exhausted_message = sanitize_external_message(&DiscoveryError::ResourceExhausted(
            "watch stream pool exhausted (max=64)".to_string(),
        ));
        assert_eq!(exhausted_message, "resource exhausted");
        assert!(!exhausted_message.contains("max=64"));
    }

    /// Storage-layer errors map to `Unavailable` so the RPC layer surfaces
    /// UNAVAILABLE (not FAILED_PRECONDITION) to callers.
    #[test]
    fn storage_errors_surface_as_unavailable_status_code() {
        let status = map_discovery_error_to_rpc_status(
            DiscoveryError::Unavailable("postgres storage error: connection refused".to_string()),
            "req-storage",
            "trace-storage",
        );
        assert_eq!(status.code(), Code::Unavailable);
        assert_eq!(status.message(), "service unavailable");
    }
}
