use std::fmt::{Debug, Formatter};
use std::time::{SystemTime, UNIX_EPOCH};

use sdkwork_discovery_contract::{
    CallerContext, ConfigPermission, DiscoveryError, DiscoveryResult, RegistryPermission,
};
use sdkwork_utils_rust::{base64url_decode, sha256_hash, verify_hmac_sha256_base64url};
use serde::Deserialize;

const TOKEN_PREFIX: &str = "sdkwork-discovery-v1";
const TOKEN_HEADER_TYPE: &str = "sdkwork.discovery.service-token.v1";
const TOKEN_ALGORITHM: &str = "HS256";
const MIN_HMAC_SECRET_BYTES: usize = 32;

#[derive(Clone, PartialEq, Eq)]
pub struct DiscoveryRpcServiceTokenVerifierConfig {
    pub hmac_secret: Vec<u8>,
    pub issuer: String,
    pub audience: String,
    pub max_token_ttl_seconds: u64,
}

impl Debug for DiscoveryRpcServiceTokenVerifierConfig {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DiscoveryRpcServiceTokenVerifierConfig")
            .field("hmac_secret", &"<redacted>")
            .field("issuer", &self.issuer)
            .field("audience", &self.audience)
            .field("max_token_ttl_seconds", &self.max_token_ttl_seconds)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ServiceTokenVerifier {
    config: DiscoveryRpcServiceTokenVerifierConfig,
}

impl ServiceTokenVerifier {
    pub fn new(config: DiscoveryRpcServiceTokenVerifierConfig) -> Self {
        Self { config }
    }

    pub fn verify(
        &self,
        service_token: &str,
        access_token: &str,
    ) -> DiscoveryResult<CallerContext> {
        self.validate_config()?;

        let token = ParsedServiceToken::parse(service_token)?;
        self.verify_signature(&token)?;
        let header: ServiceTokenHeader = decode_json_part("service token header", token.header)?;
        let claims: ServiceTokenClaims = decode_json_part("service token claims", token.claims)?;

        validate_header(&header)?;
        self.validate_claims(&claims, access_token)?;
        caller_from_claims(claims)
    }

    fn validate_config(&self) -> DiscoveryResult<()> {
        if self.config.hmac_secret.len() < MIN_HMAC_SECRET_BYTES {
            return Err(DiscoveryError::InvalidConfig(format!(
                "service-token HMAC secret must contain at least {MIN_HMAC_SECRET_BYTES} bytes"
            )));
        }

        if self.config.issuer.trim().is_empty() {
            return Err(DiscoveryError::InvalidConfig(
                "service-token issuer must not be empty".to_string(),
            ));
        }

        if self.config.audience.trim().is_empty() {
            return Err(DiscoveryError::InvalidConfig(
                "service-token audience must not be empty".to_string(),
            ));
        }

        if self.config.max_token_ttl_seconds == 0 {
            return Err(DiscoveryError::InvalidConfig(
                "service-token max ttl must be greater than zero".to_string(),
            ));
        }

        Ok(())
    }

    fn verify_signature(&self, token: &ParsedServiceToken<'_>) -> DiscoveryResult<()> {
        let signature = base64url_decode(token.signature).ok_or_else(|| {
            DiscoveryError::Unauthenticated("service-token signature is not base64url".to_string())
        })?;

        if !verify_hmac_sha256_base64url(
            token.signing_input.as_bytes(),
            &self.config.hmac_secret,
            &signature,
        ) {
            return Err(DiscoveryError::Unauthenticated(
                "service-token signature is invalid".to_string(),
            ));
        }

        Ok(())
    }

    fn validate_claims(
        &self,
        claims: &ServiceTokenClaims,
        access_token: &str,
    ) -> DiscoveryResult<()> {
        if claims.issuer != self.config.issuer {
            return Err(DiscoveryError::Unauthenticated(
                "service-token issuer is invalid".to_string(),
            ));
        }

        if claims.audience != self.config.audience {
            return Err(DiscoveryError::Unauthenticated(
                "service-token audience is invalid".to_string(),
            ));
        }

        if claims.subject_id.trim().is_empty() {
            return Err(DiscoveryError::Unauthenticated(
                "service-token subject must not be empty".to_string(),
            ));
        }

        // Service-token callers MUST carry a tenant_id claim. Without this
        // requirement, a token without tenant_id bypasses
        // `require_namespace_tenant_access` entirely (the helper returns Ok when
        // `caller.tenant_id` is None), allowing cross-tenant namespace access.
        // The unsigned-local-context path (development-only) is unaffected
        // because it never enters this verifier.
        if claims
            .tenant_id
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .is_empty()
        {
            return Err(DiscoveryError::Unauthenticated(
                "service-token must carry a tenant_id claim".to_string(),
            ));
        }

        if claims.expires_at_ms() <= current_time_millis()? {
            return Err(DiscoveryError::Unauthenticated(
                "service-token is expired".to_string(),
            ));
        }

        if claims.expires_at_ms() <= claims.issued_at_ms() {
            return Err(DiscoveryError::Unauthenticated(
                "service-token expiry must be after issue time".to_string(),
            ));
        }

        let ttl_ms = claims.expires_at_ms() - claims.issued_at_ms();
        let max_ttl_ms = self
            .config
            .max_token_ttl_seconds
            .checked_mul(1_000)
            .ok_or_else(|| {
                DiscoveryError::InvalidConfig(
                    "service-token max ttl overflows milliseconds".to_string(),
                )
            })?;

        if ttl_ms > max_ttl_ms {
            return Err(DiscoveryError::Unauthenticated(
                "service-token ttl exceeds configured maximum".to_string(),
            ));
        }

        if claims.access_token_sha256 != sha256_hash(access_token.trim().as_bytes()) {
            return Err(DiscoveryError::Unauthenticated(
                "access-token is not bound to service-token".to_string(),
            ));
        }

        Ok(())
    }
}

#[derive(Debug)]
struct ParsedServiceToken<'a> {
    header: &'a str,
    claims: &'a str,
    signature: &'a str,
    signing_input: String,
}

impl<'a> ParsedServiceToken<'a> {
    fn parse(token: &'a str) -> DiscoveryResult<Self> {
        let parts = token.split('.').collect::<Vec<_>>();
        if parts.len() != 4 || parts[0] != TOKEN_PREFIX {
            return Err(DiscoveryError::Unauthenticated(
                "authorization bearer token is not an SDKWork Discovery service-token".to_string(),
            ));
        }

        if parts[1].is_empty() || parts[2].is_empty() || parts[3].is_empty() {
            return Err(DiscoveryError::Unauthenticated(
                "service-token must include header, claims, and signature".to_string(),
            ));
        }

        Ok(Self {
            header: parts[1],
            claims: parts[2],
            signature: parts[3],
            signing_input: format!("{}.{}", parts[1], parts[2]),
        })
    }
}

#[derive(Debug, Deserialize)]
struct ServiceTokenHeader {
    alg: String,
    typ: String,
}

#[derive(Debug, Deserialize)]
struct ServiceTokenClaims {
    #[serde(rename = "iss")]
    issuer: String,
    #[serde(rename = "aud")]
    audience: String,
    #[serde(rename = "sub")]
    subject_id: String,
    #[serde(default)]
    tenant_id: Option<String>,
    #[serde(default)]
    organization_id: Option<String>,
    iat_ms: u64,
    exp_ms: u64,
    access_token_sha256: String,
    #[serde(default)]
    registry_permissions: Vec<String>,
    #[serde(default)]
    config_permissions: Vec<String>,
}

impl ServiceTokenClaims {
    fn issued_at_ms(&self) -> u64 {
        self.iat_ms
    }

    fn expires_at_ms(&self) -> u64 {
        self.exp_ms
    }
}

fn validate_header(header: &ServiceTokenHeader) -> DiscoveryResult<()> {
    if header.alg != TOKEN_ALGORITHM {
        return Err(DiscoveryError::Unauthenticated(
            "service-token algorithm is not supported".to_string(),
        ));
    }

    if header.typ != TOKEN_HEADER_TYPE {
        return Err(DiscoveryError::Unauthenticated(
            "service-token type is not supported".to_string(),
        ));
    }

    Ok(())
}

fn caller_from_claims(claims: ServiceTokenClaims) -> DiscoveryResult<CallerContext> {
    let mut caller = CallerContext::new(claims.subject_id.trim().to_string());

    if let Some(tenant_id) = claims.tenant_id {
        caller = caller.with_tenant_id(tenant_id);
    }

    if let Some(organization_id) = claims.organization_id {
        caller = caller.with_organization_id(organization_id);
    }

    for permission in claims.registry_permissions {
        caller = caller.with_registry_permission(parse_registry_permission(&permission)?);
    }

    for permission in claims.config_permissions {
        caller = caller.with_config_permission(parse_config_permission(&permission)?);
    }

    Ok(caller)
}

fn parse_registry_permission(value: &str) -> DiscoveryResult<RegistryPermission> {
    match value {
        "read" => Ok(RegistryPermission::Read),
        "write" => Ok(RegistryPermission::Write),
        "admin" => Ok(RegistryPermission::Admin),
        _ => Err(DiscoveryError::Unauthenticated(format!(
            "service-token contains unknown registry permission {value}"
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
        _ => Err(DiscoveryError::Unauthenticated(format!(
            "service-token contains unknown config permission {value}"
        ))),
    }
}

fn decode_json_part<T>(label: &'static str, value: &str) -> DiscoveryResult<T>
where
    T: for<'de> Deserialize<'de>,
{
    let bytes = base64url_decode(value)
        .ok_or_else(|| DiscoveryError::Unauthenticated(format!("{label} is not base64url")))?;
    serde_json::from_slice(&bytes)
        .map_err(|_| DiscoveryError::Unauthenticated(format!("{label} is not valid JSON")))
}

fn current_time_millis() -> DiscoveryResult<u64> {
    let elapsed = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| {
        DiscoveryError::InvalidConfig("system clock is before UNIX_EPOCH".to_string())
    })?;
    Ok(elapsed.as_millis() as u64)
}
