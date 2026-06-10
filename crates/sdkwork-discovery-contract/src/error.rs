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
        }
    }
}

impl std::error::Error for DiscoveryError {}
