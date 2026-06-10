use sdkwork_discovery_contract::DiscoveryError;
use tonic::Status;

pub fn map_discovery_error_to_status(error: DiscoveryError) -> Status {
    match error {
        DiscoveryError::Unauthenticated(message) => Status::unauthenticated(message),
        DiscoveryError::InvalidConfig(message) => Status::failed_precondition(message),
        DiscoveryError::InvalidArgument(message) => Status::invalid_argument(message),
        DiscoveryError::NotFound(message) => Status::not_found(message),
        DiscoveryError::AlreadyPublished(message) => Status::already_exists(message),
        DiscoveryError::PermissionDenied(message) => Status::permission_denied(message),
        DiscoveryError::PolicyViolation(message) => Status::permission_denied(message),
    }
}
