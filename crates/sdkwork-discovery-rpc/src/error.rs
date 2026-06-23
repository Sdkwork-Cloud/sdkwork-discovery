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
    let status = match error {
        DiscoveryError::Unauthenticated(message) => {
            warn!(error = %message, "unauthenticated request");
            Status::unauthenticated(message)
        }
        DiscoveryError::InvalidConfig(message) => {
            warn!(error = %message, "invalid config");
            Status::failed_precondition(message)
        }
        DiscoveryError::InvalidArgument(message) => {
            warn!(error = %message, "invalid argument");
            Status::invalid_argument(message)
        }
        DiscoveryError::NotFound(message) => {
            warn!(error = %message, "resource not found");
            Status::not_found(message)
        }
        DiscoveryError::AlreadyPublished(message) => {
            warn!(error = %message, "already published");
            Status::already_exists(message)
        }
        DiscoveryError::PermissionDenied(message) => {
            warn!(error = %message, "permission denied");
            Status::permission_denied(message)
        }
        DiscoveryError::PolicyViolation(message) => {
            warn!(error = %message, "policy violation");
            Status::permission_denied(message)
        }
        DiscoveryError::Conflict(message) => {
            warn!(error = %message, "conflict");
            Status::aborted(message)
        }
        DiscoveryError::Unavailable(message) => {
            warn!(error = %message, "service unavailable");
            Status::unavailable(message)
        }
        DiscoveryError::ResourceExhausted(message) => {
            warn!(error = %message, "resource exhausted");
            Status::resource_exhausted(message)
        }
    };

    attach_correlation_metadata(status, request_id, trace_id)
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
        map_discovery_error_to_rpc_status,
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
}
