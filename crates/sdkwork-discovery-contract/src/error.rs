use std::fmt::{Display, Formatter};

pub type DiscoveryResult<T> = Result<T, DiscoveryError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoveryError {
    Unauthenticated(String),
    InvalidConfig(String),
    InvalidArgument(String),
    NotFound(String),
    AlreadyPublished(String),
    PermissionDenied(String),
    PolicyViolation(String),
    Conflict(String),
    Unavailable(String),
    ResourceExhausted(String),
}

impl DiscoveryError {
    pub fn kind_string(&self) -> &'static str {
        match self {
            Self::Unauthenticated(_) => "unauthenticated",
            Self::InvalidConfig(_) => "invalid_config",
            Self::InvalidArgument(_) => "invalid_argument",
            Self::NotFound(_) => "not_found",
            Self::AlreadyPublished(_) => "already_published",
            Self::PermissionDenied(_) => "permission_denied",
            Self::PolicyViolation(_) => "policy_violation",
            Self::Conflict(_) => "conflict",
            Self::Unavailable(_) => "unavailable",
            Self::ResourceExhausted(_) => "resource_exhausted",
        }
    }
}

impl Display for DiscoveryError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unauthenticated(message) => write!(formatter, "unauthenticated: {message}"),
            Self::InvalidConfig(message) => write!(formatter, "invalid config: {message}"),
            Self::InvalidArgument(message) => write!(formatter, "invalid argument: {message}"),
            Self::NotFound(message) => write!(formatter, "not found: {message}"),
            Self::AlreadyPublished(message) => write!(formatter, "already published: {message}"),
            Self::PermissionDenied(message) => write!(formatter, "permission denied: {message}"),
            Self::PolicyViolation(message) => write!(formatter, "policy violation: {message}"),
            Self::Conflict(message) => write!(formatter, "conflict: {message}"),
            Self::Unavailable(message) => write!(formatter, "unavailable: {message}"),
            Self::ResourceExhausted(message) => {
                write!(formatter, "resource exhausted: {message}")
            }
        }
    }
}

impl std::error::Error for DiscoveryError {}
