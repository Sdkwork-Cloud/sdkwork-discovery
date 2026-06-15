use sdkwork_discovery_contract::DiscoveryError;
use tonic::Status;
use tracing::warn;

pub fn map_discovery_error_to_status(error: DiscoveryError) -> Status {
    match error {
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
    }
}
