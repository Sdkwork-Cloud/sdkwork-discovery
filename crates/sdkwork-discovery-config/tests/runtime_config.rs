use sdkwork_discovery_config::{
    DiscoveryRuntimeConfig, RuntimeEnvironment, RuntimeTarget, StorageCredentialSource,
    StorageProvider, StorageRole,
};
use std::collections::BTreeMap;

const DEV_SECURITY_BLOCK: &str = r#"[security]
auth_mode = "service-token"
tls_enabled = false
mtls_enabled = false
allow_unsigned_local_context = true"#;

const PRODUCTION_SIGNED_SECURITY_BLOCK: &str = r#"[security]
auth_mode = "service-token"
tls_enabled = true
mtls_enabled = true
allow_unsigned_local_context = false
server_tls_cert_file = "/run/secrets/sdkwork/discovery/server.crt"
server_tls_key_file = "/run/secrets/sdkwork/discovery/server.key"
client_ca_cert_file = "/run/secrets/sdkwork/discovery/client-ca.crt"

[security.service_token]
hmac_secret_file = "/run/secrets/sdkwork/discovery/service-token-hmac.secret"
issuer = "sdkwork-discovery"
audience = "sdkwork-discovery-rpc"
max_token_ttl_seconds = 3600"#;

const PRODUCTION_RESILIENCE_BLOCK: &str = r#"
[resilience]

[resilience.rate_limit]
enabled = true
requests_per_second = 1000
burst_capacity = 2000

[resilience.circuit_breaker]
enabled = true
failure_threshold = 5
recovery_timeout_ms = 5000

[resilience.degradation]
read_only_on_storage_failure = true"#;

fn minimal_config(profile: &str, environment: Option<&str>) -> String {
    let environment_line = environment
        .map(|value| format!("environment = \"{value}\"\n"))
        .unwrap_or_default();

    format!(
        r#"
[runtime]
{environment_line}config_profile = "{profile}"
deployment_mode = "server"
runtime_target = "server"

[server]
grpc_bind_host = "127.0.0.1"
grpc_port = 19090
admin_grpc_port = 19091
enable_health = true
enable_reflection = true
default_deadline_ms = 5000

[security]
auth_mode = "service-token"
tls_enabled = false
mtls_enabled = false
allow_unsigned_local_context = true

[storage]
provider = "memory"

[registry]
default_lease_ttl_seconds = 30
min_lease_ttl_seconds = 5
max_lease_ttl_seconds = 300
expiry_scan_interval_ms = 1000
expiry_scan_batch_size = 1000

[config_registry]
enabled = true
max_config_body_bytes = 262144
require_publish_for_reads = true
allow_secret_values = false
allow_secret_refs = true

[watch]
enabled = true
max_streams = 10000
event_buffer_size = 1024
heartbeat_interval_ms = 15000
durable_poll_interval_ms = 1000
durable_replay_batch_size = 1000
"#
    )
}

#[test]
fn dev_profile_normalizes_to_development_environment() {
    let config = DiscoveryRuntimeConfig::from_toml_str(&minimal_config("dev", None)).unwrap();

    assert_eq!(config.runtime.environment, RuntimeEnvironment::Development);
    assert_eq!(config.runtime.config_profile.as_deref(), Some("dev"));
    assert_eq!(config.storage.provider, StorageProvider::Memory);
    assert_eq!(config.storage.registry_role, StorageRole::Primary);
    assert_eq!(config.storage.config_role, StorageRole::Primary);
    assert_eq!(config.storage.watch_role, StorageRole::Primary);
    assert!(config.config_registry.enabled);
}

#[test]
fn production_rejects_loopback_bind_and_secret_literals() {
    let mut toml = minimal_config("prod", Some("production"));
    toml = toml.replace("allow_secret_values = false", "allow_secret_values = true");
    toml = toml.replace(
        "allow_unsigned_local_context = true",
        "allow_unsigned_local_context = false",
    );
    toml.push_str(
        r#"

[security.service_token]
hmac_secret_file = "/run/secrets/sdkwork/discovery/service-token-hmac.secret"
issuer = "sdkwork-discovery"
audience = "sdkwork-discovery-rpc"
max_token_ttl_seconds = 3600
"#,
    );

    let error = DiscoveryRuntimeConfig::from_toml_str(&toml).unwrap_err();

    assert!(error.to_string().contains("production"));
    assert!(error.to_string().contains("secret"));
}

#[test]
fn rejects_unknown_security_auth_mode() {
    let toml = minimal_config("dev", None).replace(
        r#"auth_mode = "service-token""#,
        r#"auth_mode = "anonymous""#,
    );

    let error = DiscoveryRuntimeConfig::from_toml_str(&toml).unwrap_err();
    let message = error.to_string().to_lowercase();

    assert!(message.contains("auth"));
    assert!(message.contains("service-token"));
}

#[test]
fn non_development_runtime_rejects_unsigned_local_context() {
    let toml = minimal_config("staging", Some("staging"));

    let error = DiscoveryRuntimeConfig::from_toml_str(&toml).unwrap_err();
    let message = error.to_string().to_lowercase();

    assert!(message.contains("unsigned"));
    assert!(message.contains("local"));
}

#[test]
fn development_runtime_rejects_unsigned_context_on_non_loopback_bind() {
    let toml = minimal_config("dev", None).replace(
        r#"grpc_bind_host = "127.0.0.1""#,
        r#"grpc_bind_host = "0.0.0.0""#,
    );

    let error = DiscoveryRuntimeConfig::from_toml_str(&toml).unwrap_err();
    let message = error.to_string().to_lowercase();

    assert!(message.contains("unsigned"));
    assert!(message.contains("loopback"));
}

#[test]
fn env_overlay_can_disable_unsigned_local_context_for_non_local_runtime() {
    let env = BTreeMap::from([
        (
            "SDKWORK_DISCOVERY_RPC_ALLOW_UNSIGNED_LOCAL_CONTEXT".to_string(),
            "false".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_RPC_AUTH_MODE".to_string(),
            "service-token".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_RPC_SERVICE_TOKEN_HMAC_SECRET_FILE".to_string(),
            "/run/secrets/sdkwork/discovery/service-token-hmac.secret".to_string(),
        ),
    ]);

    let config = DiscoveryRuntimeConfig::from_toml_str_with_env(
        &minimal_config("staging", Some("staging")),
        &env,
    )
    .unwrap();

    assert!(!config.security.allow_unsigned_local_context);
}

#[test]
fn service_token_secret_file_is_required_when_unsigned_context_is_disabled() {
    let toml = minimal_config("staging", Some("staging")).replace(
        "allow_unsigned_local_context = true",
        "allow_unsigned_local_context = false",
    );

    let error = DiscoveryRuntimeConfig::from_toml_str(&toml).unwrap_err();
    let message = error.to_string();

    assert!(message.contains("service-token"));
    assert!(message.contains("HMAC secret file"));
}

#[test]
fn env_overlay_can_configure_service_token_verifier_fields() {
    let env = BTreeMap::from([
        (
            "SDKWORK_DISCOVERY_RPC_ALLOW_UNSIGNED_LOCAL_CONTEXT".to_string(),
            "false".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_RPC_SERVICE_TOKEN_HMAC_SECRET_FILE".to_string(),
            "/run/secrets/sdkwork/discovery/service-token-hmac.secret".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_RPC_SERVICE_TOKEN_ISSUER".to_string(),
            "sdkwork-discovery".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_RPC_SERVICE_TOKEN_AUDIENCE".to_string(),
            "sdkwork-discovery-rpc".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_RPC_SERVICE_TOKEN_MAX_TTL_SECONDS".to_string(),
            "3600".to_string(),
        ),
    ]);

    let config = DiscoveryRuntimeConfig::from_toml_str_with_env(
        &minimal_config("staging", Some("staging")),
        &env,
    )
    .unwrap();

    assert_eq!(
        config.security.service_token.hmac_secret_file.as_deref(),
        Some("/run/secrets/sdkwork/discovery/service-token-hmac.secret")
    );
    assert_eq!(config.security.service_token.issuer, "sdkwork-discovery");
    assert_eq!(
        config.security.service_token.audience,
        "sdkwork-discovery-rpc"
    );
    assert_eq!(config.security.service_token.max_token_ttl_seconds, 3600);
}

#[test]
fn rejects_invalid_lease_ttl_bounds() {
    let toml = minimal_config("dev", None)
        .replace("min_lease_ttl_seconds = 5", "min_lease_ttl_seconds = 60");

    let error = DiscoveryRuntimeConfig::from_toml_str(&toml).unwrap_err();

    assert!(error.to_string().contains("lease"));
}

#[test]
fn enabled_watch_rejects_zero_runtime_governance_values() {
    for (field, replacement, expected) in [
        ("max_streams", "max_streams = 0", "max streams"),
        (
            "event_buffer_size",
            "event_buffer_size = 0",
            "event buffer size",
        ),
        (
            "heartbeat_interval_ms",
            "heartbeat_interval_ms = 0",
            "heartbeat interval",
        ),
        (
            "durable_poll_interval_ms",
            "durable_poll_interval_ms = 0",
            "durable poll interval",
        ),
        (
            "durable_replay_batch_size",
            "durable_replay_batch_size = 0",
            "durable replay batch size",
        ),
    ] {
        let source = match field {
            "max_streams" => "max_streams = 10000",
            "event_buffer_size" => "event_buffer_size = 1024",
            "heartbeat_interval_ms" => "heartbeat_interval_ms = 15000",
            "durable_poll_interval_ms" => "durable_poll_interval_ms = 1000",
            "durable_replay_batch_size" => "durable_replay_batch_size = 1000",
            _ => unreachable!(),
        };
        let toml = minimal_config("dev", None).replace(source, replacement);

        let error = DiscoveryRuntimeConfig::from_toml_str(&toml).unwrap_err();

        assert!(
            error.to_string().contains(expected),
            "{field} error should mention {expected}: {error}"
        );
    }
}

#[test]
fn env_overlay_can_override_storage_and_port_without_mutating_unrelated_config() {
    let toml = minimal_config("dev", None).replace(
        r#"[storage]
provider = "memory""#,
        r#"[storage]
provider = "memory"

[storage.postgres]
host = "127.0.0.1"
port = 5432
database = "sdkwork_ai_dev"
schema = "sdkwork_ai_dev"
username = "sdkwork_ai_dev"
password_file = "/run/secrets/sdkwork/discovery/postgres-password"
tls_enabled = false
connect_timeout_ms = 3000
max_connections = 16"#,
    );
    let mut env = BTreeMap::new();
    env.insert(
        "SDKWORK_DISCOVERY_STORAGE_PROVIDER".to_string(),
        "postgres".to_string(),
    );
    env.insert(
        "SDKWORK_DISCOVERY_APPLICATION_PUBLIC_INGRESS_BIND".to_string(),
        "127.0.0.1:19190".to_string(),
    );

    let config = DiscoveryRuntimeConfig::from_toml_str_with_env(&toml, &env).unwrap();

    assert_eq!(config.storage.provider, StorageProvider::Postgres);
    assert_eq!(config.server.grpc_port, 19190);
    assert_eq!(config.server.admin_grpc_port, 19091);
}

#[test]
fn env_overlay_maps_topology_surface_bind_keys() {
    let toml = minimal_config("dev", Some("development"));
    let env = BTreeMap::from([
        (
            "SDKWORK_DISCOVERY_SERVICE_LAYOUT".to_string(),
            "unified-process".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_PROFILE_ID".to_string(),
            "self-hosted.unified-process.development".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_APPLICATION_PUBLIC_INGRESS_BIND".to_string(),
            "127.0.0.1:19190".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_APPLICATION_PUBLIC_GRPC_URL".to_string(),
            "grpc://127.0.0.1:19190".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_OPERATIONS_CONTROL_INGRESS_BIND".to_string(),
            "127.0.0.1:19192".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_OPERATIONS_CONTROL_GRPC_URL".to_string(),
            "grpc://127.0.0.1:19192".to_string(),
        ),
    ]);

    let config = DiscoveryRuntimeConfig::from_toml_str_with_env(&toml, &env).unwrap();

    assert_eq!(config.server.grpc_bind_host, "127.0.0.1");
    assert_eq!(config.server.grpc_port, 19190);
    assert_eq!(config.server.admin_grpc_port, 19192);
}

#[test]
fn env_overlay_rejects_retired_grpc_bind_env_keys() {
    let toml = minimal_config("dev", Some("development"));
    for (key, value) in [
        ("SDKWORK_DISCOVERY_GRPC_BIND_HOST", "127.0.0.1"),
        ("SDKWORK_DISCOVERY_GRPC_PORT", "19190"),
        ("SDKWORK_DISCOVERY_ADMIN_GRPC_PORT", "19191"),
    ] {
        let env = BTreeMap::from([(key.to_string(), value.to_string())]);
        let error = DiscoveryRuntimeConfig::from_toml_str_with_env(&toml, &env).unwrap_err();
        assert!(
            error.to_string().contains("unsupported discovery env key"),
            "{key} should be rejected: {error}"
        );
    }
}

#[test]
fn env_overlay_rejects_retired_provider_database_fields() {
    for key in [
        ["SDKWORK", "DISCOVERY", "DATABASE", "NAME"].join("_"),
        ["SDKWORK", "DISCOVERY", "STORAGE", "POSTGRES", "DATABASE"].join("_"),
        ["SDKWORK", "DISCOVERY", "STORAGE", "SQLITE", "FILE"].join("_"),
        ["SDKWORK", "CLAW", "DATABASE", "NAME"].join("_"),
    ] {
        let env = BTreeMap::from([(key.to_string(), "retired".to_string())]);
        let error =
            DiscoveryRuntimeConfig::from_toml_str_with_env(&minimal_config("dev", None), &env)
                .unwrap_err();

        assert!(error.to_string().contains(&key));
        assert!(error.to_string().contains("SDKWORK_DATABASE_*"));
    }
}

#[test]
fn env_overlay_can_build_redis_storage_from_structured_fields() {
    let env = BTreeMap::from([
        (
            "SDKWORK_DISCOVERY_STORAGE_PROVIDER".to_string(),
            "redis".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_STORAGE_REDIS_HOST".to_string(),
            "redis.internal".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_STORAGE_REDIS_PORT".to_string(),
            "6379".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_STORAGE_REDIS_DATABASE".to_string(),
            "0".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_STORAGE_REDIS_USERNAME".to_string(),
            "sdkwork_discovery".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_STORAGE_REDIS_PASSWORD_FILE".to_string(),
            "/run/secrets/sdkwork/discovery/redis-password".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_STORAGE_REDIS_TLS_ENABLED".to_string(),
            "true".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_STORAGE_REDIS_CONNECT_TIMEOUT_MS".to_string(),
            "2000".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_STORAGE_REDIS_MAX_CONNECTIONS".to_string(),
            "32".to_string(),
        ),
    ]);

    let config =
        DiscoveryRuntimeConfig::from_toml_str_with_env(&minimal_config("dev", None), &env).unwrap();

    let redis = config.storage.redis.as_ref().unwrap();
    assert_eq!(config.storage.provider, StorageProvider::Redis);
    assert_eq!(redis.host, "redis.internal");
    assert_eq!(redis.port, 6379);
    assert_eq!(redis.database.as_deref(), Some("0"));
    assert_eq!(redis.username.as_deref(), Some("sdkwork_discovery"));
    assert_eq!(
        redis.credential_source,
        StorageCredentialSource::PasswordFile(
            "/run/secrets/sdkwork/discovery/redis-password".to_string()
        )
    );
    assert!(redis.tls_enabled);
    assert_eq!(redis.connect_timeout_ms, 2000);
    assert_eq!(redis.max_connections, 32);
}

#[test]
fn env_overlay_rejects_schema_for_non_postgres_storage_provider() {
    let env = BTreeMap::from([
        (
            "SDKWORK_DISCOVERY_STORAGE_PROVIDER".to_string(),
            "redis".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_STORAGE_REDIS_HOST".to_string(),
            "redis.internal".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_STORAGE_REDIS_PORT".to_string(),
            "6379".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_STORAGE_REDIS_SCHEMA".to_string(),
            "public".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_STORAGE_REDIS_CONNECT_TIMEOUT_MS".to_string(),
            "2000".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_STORAGE_REDIS_MAX_CONNECTIONS".to_string(),
            "32".to_string(),
        ),
    ]);

    let error = DiscoveryRuntimeConfig::from_toml_str_with_env(&minimal_config("dev", None), &env)
        .unwrap_err();
    let message = error.to_string().to_lowercase();

    assert!(message.contains("schema"));
    assert!(message.contains("postgres"));
}

#[test]
fn storage_rejects_blank_postgres_schema() {
    let toml = minimal_config("dev", None).replace(
        r#"[storage]
provider = "memory""#,
        r#"[storage]
provider = "postgres"

[storage.postgres]
host = "127.0.0.1"
port = 5432
database = "sdkwork_ai_dev"
schema = " "
username = "sdkwork_ai_dev"
tls_enabled = false
connect_timeout_ms = 3000
max_connections = 16"#,
    );

    let error = DiscoveryRuntimeConfig::from_toml_str(&toml).unwrap_err();
    let message = error.to_string().to_lowercase();

    assert!(message.contains("postgres"));
    assert!(message.contains("schema"));
}

#[test]
fn storage_rejects_schema_for_non_postgres_transport_provider() {
    let toml = minimal_config("dev", None).replace(
        r#"[storage]
provider = "memory""#,
        r#"[storage]
provider = "redis"

[storage.redis]
host = "redis.internal"
port = 6379
schema = "public"
tls_enabled = false
connect_timeout_ms = 2000
max_connections = 32"#,
    );

    let error = DiscoveryRuntimeConfig::from_toml_str(&toml).unwrap_err();
    let message = error.to_string().to_lowercase();

    assert!(message.contains("redis"));
    assert!(message.contains("schema"));
    assert!(message.contains("postgres"));
}

#[test]
fn env_overlay_can_build_postgres_storage_from_canonical_database_fields() {
    let env = BTreeMap::from([
        (
            "SDKWORK_DATABASE_ENGINE".to_string(),
            "postgresql".to_string(),
        ),
        (
            "SDKWORK_DATABASE_HOST".to_string(),
            "postgres.internal".to_string(),
        ),
        ("SDKWORK_DATABASE_PORT".to_string(), "5432".to_string()),
        (
            "SDKWORK_DATABASE_NAME".to_string(),
            "sdkwork_ai_dev".to_string(),
        ),
        (
            "SDKWORK_DATABASE_SCHEMA".to_string(),
            "sdkwork_ai_dev".to_string(),
        ),
        (
            "SDKWORK_DATABASE_USERNAME".to_string(),
            "sdkwork_ai_dev".to_string(),
        ),
        (
            "SDKWORK_DATABASE_PASSWORD_FILE".to_string(),
            "/run/secrets/sdkwork/discovery/postgres-password".to_string(),
        ),
        (
            "SDKWORK_DATABASE_SSL_MODE".to_string(),
            "require".to_string(),
        ),
        (
            "SDKWORK_DATABASE_ACQUIRE_TIMEOUT".to_string(),
            "3".to_string(),
        ),
        (
            "SDKWORK_DATABASE_MAX_CONNECTIONS".to_string(),
            "16".to_string(),
        ),
    ]);

    let config =
        DiscoveryRuntimeConfig::from_toml_str_with_env(&minimal_config("dev", None), &env).unwrap();

    let postgres = config.storage.postgres.as_ref().unwrap();
    assert_eq!(config.storage.provider, StorageProvider::Postgres);
    assert_eq!(postgres.host, "postgres.internal");
    assert_eq!(postgres.port, 5432);
    assert_eq!(postgres.database.as_deref(), Some("sdkwork_ai_dev"));
    assert_eq!(postgres.schema.as_deref(), Some("sdkwork_ai_dev"));
    assert_eq!(postgres.username.as_deref(), Some("sdkwork_ai_dev"));
    assert_eq!(
        postgres.credential_source,
        StorageCredentialSource::PasswordFile(
            "/run/secrets/sdkwork/discovery/postgres-password".to_string()
        )
    );
    assert!(postgres.tls_enabled);
    assert_eq!(postgres.connect_timeout_ms, 3000);
    assert_eq!(postgres.max_connections, 16);
}

#[test]
fn env_overlay_can_build_sqlite_storage_from_canonical_database_fields() {
    let env = BTreeMap::from([
        ("SDKWORK_DATABASE_ENGINE".to_string(), "sqlite".to_string()),
        (
            "SDKWORK_DATABASE_FILE".to_string(),
            "target/dev/discovery/discovery.sqlite".to_string(),
        ),
        (
            "SDKWORK_DATABASE_MAX_CONNECTIONS".to_string(),
            "1".to_string(),
        ),
    ]);

    let config =
        DiscoveryRuntimeConfig::from_toml_str_with_env(&minimal_config("dev", None), &env).unwrap();

    let sqlite = config.storage.sqlite.as_ref().unwrap();
    assert_eq!(config.storage.provider, StorageProvider::Sqlite);
    assert_eq!(sqlite.file, "target/dev/discovery/discovery.sqlite");
    assert_eq!(sqlite.max_connections, 1);
}

#[test]
fn env_overlay_can_build_sqlite_database_fields_from_storage_provider_selection() {
    let env = BTreeMap::from([
        (
            "SDKWORK_DISCOVERY_STORAGE_PROVIDER".to_string(),
            "sqlite".to_string(),
        ),
        (
            "SDKWORK_DATABASE_FILE".to_string(),
            "target/dev/discovery/discovery.sqlite".to_string(),
        ),
        (
            "SDKWORK_DATABASE_MAX_CONNECTIONS".to_string(),
            "1".to_string(),
        ),
    ]);

    let config =
        DiscoveryRuntimeConfig::from_toml_str_with_env(&minimal_config("dev", None), &env).unwrap();

    let sqlite = config.storage.sqlite.as_ref().unwrap();
    assert_eq!(config.storage.provider, StorageProvider::Sqlite);
    assert_eq!(sqlite.file, "target/dev/discovery/discovery.sqlite");
    assert_eq!(sqlite.max_connections, 1);
}

#[test]
fn env_overlay_rejects_direct_database_password_values() {
    let env = BTreeMap::from([
        (
            "SDKWORK_DATABASE_ENGINE".to_string(),
            "postgresql".to_string(),
        ),
        (
            "SDKWORK_DATABASE_PASSWORD".to_string(),
            "plain-password".to_string(),
        ),
    ]);

    let error = DiscoveryRuntimeConfig::from_toml_str_with_env(&minimal_config("dev", None), &env)
        .unwrap_err();
    let message = error.to_string().to_lowercase();

    assert!(message.contains("credential"));
    assert!(message.contains("password"));
}

#[test]
fn env_overlay_rejects_conflicting_storage_provider_and_database_engine() {
    let env = BTreeMap::from([
        (
            "SDKWORK_DISCOVERY_STORAGE_PROVIDER".to_string(),
            "sqlite".to_string(),
        ),
        (
            "SDKWORK_DATABASE_ENGINE".to_string(),
            "postgresql".to_string(),
        ),
        (
            "SDKWORK_DATABASE_HOST".to_string(),
            "postgres.internal".to_string(),
        ),
        ("SDKWORK_DATABASE_PORT".to_string(), "5432".to_string()),
        (
            "SDKWORK_DATABASE_NAME".to_string(),
            "sdkwork_ai_dev".to_string(),
        ),
        (
            "SDKWORK_DATABASE_ACQUIRE_TIMEOUT".to_string(),
            "3".to_string(),
        ),
        (
            "SDKWORK_DATABASE_MAX_CONNECTIONS".to_string(),
            "16".to_string(),
        ),
        (
            "SDKWORK_DATABASE_FILE".to_string(),
            "target/dev/discovery/discovery.sqlite".to_string(),
        ),
    ]);

    let error = DiscoveryRuntimeConfig::from_toml_str_with_env(&minimal_config("dev", None), &env)
        .unwrap_err();
    let message = error.to_string().to_lowercase();

    assert!(message.contains("storage"));
    assert!(message.contains("database"));
    assert!(message.contains("conflict"));
}

#[test]
fn env_overlay_rejects_database_fields_without_selected_database_provider() {
    let env = BTreeMap::from([
        (
            "SDKWORK_DATABASE_HOST".to_string(),
            "postgres.internal".to_string(),
        ),
        ("SDKWORK_DATABASE_PORT".to_string(), "5432".to_string()),
    ]);

    let error = DiscoveryRuntimeConfig::from_toml_str_with_env(&minimal_config("dev", None), &env)
        .unwrap_err();
    let message = error.to_string().to_lowercase();

    assert!(message.contains("database"));
    assert!(message.contains("engine"));
}

#[test]
fn env_overlay_rejects_postgres_database_fields_for_sqlite_provider() {
    let env = BTreeMap::from([
        ("SDKWORK_DATABASE_ENGINE".to_string(), "sqlite".to_string()),
        (
            "SDKWORK_DATABASE_HOST".to_string(),
            "postgres.internal".to_string(),
        ),
        (
            "SDKWORK_DATABASE_FILE".to_string(),
            "target/dev/discovery/discovery.sqlite".to_string(),
        ),
        (
            "SDKWORK_DATABASE_MAX_CONNECTIONS".to_string(),
            "1".to_string(),
        ),
    ]);

    let error = DiscoveryRuntimeConfig::from_toml_str_with_env(&minimal_config("dev", None), &env)
        .unwrap_err();
    let message = error.to_string().to_lowercase();

    assert!(message.contains("database"));
    assert!(message.contains("sqlite"));
    assert!(message.contains("postgres"));
}

#[test]
fn env_overlay_rejects_sqlite_database_file_for_postgres_provider() {
    let env = BTreeMap::from([
        (
            "SDKWORK_DATABASE_ENGINE".to_string(),
            "postgresql".to_string(),
        ),
        (
            "SDKWORK_DATABASE_HOST".to_string(),
            "postgres.internal".to_string(),
        ),
        ("SDKWORK_DATABASE_PORT".to_string(), "5432".to_string()),
        (
            "SDKWORK_DATABASE_NAME".to_string(),
            "sdkwork_ai_dev".to_string(),
        ),
        (
            "SDKWORK_DATABASE_ACQUIRE_TIMEOUT".to_string(),
            "3".to_string(),
        ),
        (
            "SDKWORK_DATABASE_MAX_CONNECTIONS".to_string(),
            "16".to_string(),
        ),
        (
            "SDKWORK_DATABASE_FILE".to_string(),
            "target/dev/discovery/discovery.sqlite".to_string(),
        ),
    ]);

    let error = DiscoveryRuntimeConfig::from_toml_str_with_env(&minimal_config("dev", None), &env)
        .unwrap_err();
    let message = error.to_string().to_lowercase();

    assert!(message.contains("database"));
    assert!(message.contains("postgres"));
    assert!(message.contains("sqlite"));
}

#[test]
fn env_overlay_can_override_standard_runtime_and_server_fields() {
    let env = BTreeMap::from([
        (
            "SDKWORK_DISCOVERY_ENVIRONMENT".to_string(),
            "test".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_CONFIG_PROFILE".to_string(),
            "test".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_RUNTIME_TARGET".to_string(),
            "test-runner".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_RPC_DEFAULT_DEADLINE_MS".to_string(),
            "2500".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_WATCH_ENABLED".to_string(),
            "false".to_string(),
        ),
    ]);

    let config =
        DiscoveryRuntimeConfig::from_toml_str_with_env(&minimal_config("dev", None), &env).unwrap();

    assert_eq!(config.runtime.environment, RuntimeEnvironment::Test);
    assert_eq!(config.runtime.config_profile.as_deref(), Some("test"));
    assert_eq!(config.runtime.runtime_target, RuntimeTarget::TestRunner);
    assert_eq!(config.server.default_deadline_ms, 2500);
    assert!(!config.watch.enabled);
}

#[test]
fn env_overlay_can_override_watch_runtime_governance_fields() {
    let env = BTreeMap::from([
        (
            "SDKWORK_DISCOVERY_WATCH_MAX_STREAMS".to_string(),
            "64".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_WATCH_EVENT_BUFFER_SIZE".to_string(),
            "128".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_WATCH_HEARTBEAT_INTERVAL_MS".to_string(),
            "250".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_WATCH_DURABLE_POLL_INTERVAL_MS".to_string(),
            "750".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_WATCH_DURABLE_REPLAY_BATCH_SIZE".to_string(),
            "256".to_string(),
        ),
    ]);

    let config =
        DiscoveryRuntimeConfig::from_toml_str_with_env(&minimal_config("dev", None), &env).unwrap();

    assert_eq!(config.watch.max_streams, 64);
    assert_eq!(config.watch.event_buffer_size, 128);
    assert_eq!(config.watch.heartbeat_interval_ms, 250);
    assert_eq!(config.watch.durable_poll_interval_ms, 750);
    assert_eq!(config.watch.durable_replay_batch_size, 256);
}

#[test]
fn env_overlay_can_override_registry_runtime_governance_fields() {
    let env = BTreeMap::from([
        (
            "SDKWORK_DISCOVERY_REGISTRY_DEFAULT_LEASE_TTL_SECONDS".to_string(),
            "45".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_REGISTRY_MIN_LEASE_TTL_SECONDS".to_string(),
            "10".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_REGISTRY_MAX_LEASE_TTL_SECONDS".to_string(),
            "600".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_REGISTRY_EXPIRY_SCAN_INTERVAL_MS".to_string(),
            "1500".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_REGISTRY_EXPIRY_SCAN_BATCH_SIZE".to_string(),
            "512".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_REGISTRY_HEALTH_CHECK_SCAN_INTERVAL_MS".to_string(),
            "5000".to_string(),
        ),
    ]);

    let config =
        DiscoveryRuntimeConfig::from_toml_str_with_env(&minimal_config("dev", None), &env).unwrap();

    assert_eq!(config.registry.default_lease_ttl_seconds, 45);
    assert_eq!(config.registry.min_lease_ttl_seconds, 10);
    assert_eq!(config.registry.max_lease_ttl_seconds, 600);
    assert_eq!(config.registry.expiry_scan_interval_ms, 1500);
    assert_eq!(config.registry.expiry_scan_batch_size, 512);
    assert_eq!(config.registry.health_check_scan_interval_ms, 5000);
}

#[test]
fn env_overlay_can_override_watch_event_gc_fields() {
    let env = BTreeMap::from([
        (
            "SDKWORK_DISCOVERY_WATCH_EVENT_GC_INTERVAL_MS".to_string(),
            "30000".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_WATCH_EVENT_GC_RETENTION_COUNT".to_string(),
            "5000".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_WATCH_EVENT_GC_BATCH_SIZE".to_string(),
            "256".to_string(),
        ),
    ]);

    let config =
        DiscoveryRuntimeConfig::from_toml_str_with_env(&minimal_config("dev", None), &env).unwrap();

    assert_eq!(config.watch.event_gc_interval_ms, 30_000);
    assert_eq!(config.watch.event_gc_retention_count, 5_000);
    assert_eq!(config.watch.event_gc_batch_size, 256);
}

#[test]
fn env_overlay_can_override_resilience_fields() {
    let env = BTreeMap::from([
        (
            "SDKWORK_DISCOVERY_RESILIENCE_CIRCUIT_BREAKER_ENABLED".to_string(),
            "true".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_RESILIENCE_CIRCUIT_BREAKER_FAILURE_THRESHOLD".to_string(),
            "3".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_RESILIENCE_CIRCUIT_BREAKER_RECOVERY_TIMEOUT_MS".to_string(),
            "15000".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_RESILIENCE_CIRCUIT_BREAKER_HALF_OPEN_MAX_REQUESTS".to_string(),
            "2".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_RESILIENCE_RATE_LIMIT_ENABLED".to_string(),
            "true".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_RESILIENCE_RATE_LIMIT_REQUESTS_PER_SECOND".to_string(),
            "50".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_RESILIENCE_RATE_LIMIT_BURST_CAPACITY".to_string(),
            "100".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_RESILIENCE_DEGRADATION_READ_ONLY_ON_STORAGE_FAILURE".to_string(),
            "true".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_RESILIENCE_DEGRADATION_STALE_READ_MAX_AGE_MS".to_string(),
            "120000".to_string(),
        ),
    ]);

    let config =
        DiscoveryRuntimeConfig::from_toml_str_with_env(&minimal_config("dev", None), &env).unwrap();

    assert!(config.resilience.circuit_breaker.enabled);
    assert_eq!(config.resilience.circuit_breaker.failure_threshold, 3);
    assert_eq!(
        config.resilience.circuit_breaker.recovery_timeout_ms,
        15_000
    );
    assert_eq!(config.resilience.circuit_breaker.half_open_max_requests, 2);
    assert!(config.resilience.rate_limit.enabled);
    assert_eq!(config.resilience.rate_limit.requests_per_second, 50);
    assert_eq!(config.resilience.rate_limit.burst_capacity, 100);
    assert!(config.resilience.degradation.read_only_on_storage_failure);
    assert_eq!(config.resilience.degradation.stale_read_max_age_ms, 120_000);
}

#[test]
fn enabled_resilience_rate_limit_rejects_zero_burst_capacity() {
    let mut toml = minimal_config("dev", None);
    toml.push_str(
        r#"

[resilience.rate_limit]
enabled = true
requests_per_second = 100
burst_capacity = 0
"#,
    );

    let error = DiscoveryRuntimeConfig::from_toml_str(&toml).unwrap_err();
    assert!(error.to_string().to_lowercase().contains("burst capacity"));
}

#[test]
fn env_overlay_can_override_config_registry_policy_fields() {
    let env = BTreeMap::from([
        (
            "SDKWORK_DISCOVERY_CONFIG_REGISTRY_MAX_CONFIG_BODY_BYTES".to_string(),
            "4096".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_CONFIG_REGISTRY_REQUIRE_PUBLISH_FOR_READS".to_string(),
            "false".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_CONFIG_REGISTRY_ALLOW_SECRET_VALUES".to_string(),
            "false".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_CONFIG_REGISTRY_ALLOW_SECRET_REFS".to_string(),
            "false".to_string(),
        ),
    ]);

    let config =
        DiscoveryRuntimeConfig::from_toml_str_with_env(&minimal_config("dev", None), &env).unwrap();

    assert_eq!(config.config_registry.max_config_body_bytes, 4096);
    assert!(!config.config_registry.require_publish_for_reads);
    assert!(!config.config_registry.allow_secret_values);
    assert!(!config.config_registry.allow_secret_refs);
}

#[test]
fn env_overlay_rejects_profile_environment_mismatch() {
    let env = BTreeMap::from([
        (
            "SDKWORK_DISCOVERY_ENVIRONMENT".to_string(),
            "production".to_string(),
        ),
        (
            "SDKWORK_DISCOVERY_CONFIG_PROFILE".to_string(),
            "dev".to_string(),
        ),
    ]);

    let error = DiscoveryRuntimeConfig::from_toml_str_with_env(&minimal_config("dev", None), &env)
        .unwrap_err();

    assert!(error.to_string().contains("environment"));
    assert!(error.to_string().contains("profile"));
}

#[test]
fn env_overlay_rejects_runtime_credential_values() {
    let mut env = BTreeMap::new();
    env.insert(
        "SDKWORK_DISCOVERY_ACCESS_TOKEN".to_string(),
        "must-not-be-accepted".to_string(),
    );

    let error = DiscoveryRuntimeConfig::from_toml_str_with_env(&minimal_config("dev", None), &env)
        .unwrap_err();

    assert!(error.to_string().contains("credential"));
}

#[test]
fn durable_storage_rejects_non_primary_registry_role() {
    let toml = minimal_config("dev", None).replace(
        r#"[storage]
provider = "memory""#,
        r#"[storage]
provider = "postgres"
registry_role = "cache"
config_role = "primary"
watch_role = "primary"

[storage.postgres]
host = "127.0.0.1"
port = 5432
database = "sdkwork_ai_dev"
schema = "sdkwork_ai_dev"
username = "sdkwork_ai_dev"
password_file = "/run/secrets/sdkwork/discovery/postgres-password"
tls_enabled = false
connect_timeout_ms = 3000
max_connections = 16"#,
    );

    let error = DiscoveryRuntimeConfig::from_toml_str(&toml).unwrap_err();
    assert!(error.to_string().contains("registry_role"));
}

#[test]
fn postgres_storage_requires_structured_connection_and_password_file() {
    let toml = minimal_config("dev", None).replace(
        r#"[storage]
provider = "memory""#,
        r#"[storage]
provider = "postgres"
registry_role = "primary"
config_role = "primary"
watch_role = "primary"

[storage.postgres]
host = "127.0.0.1"
port = 5432
database = "sdkwork_ai_dev"
schema = "sdkwork_ai_dev"
username = "sdkwork_ai_dev"
password_file = "/run/secrets/sdkwork/discovery/postgres-password"
tls_enabled = false
connect_timeout_ms = 3000
max_connections = 16"#,
    );

    let config = DiscoveryRuntimeConfig::from_toml_str(&toml).unwrap();

    let postgres = config.storage.postgres.as_ref().unwrap();
    assert_eq!(config.storage.provider, StorageProvider::Postgres);
    assert_eq!(postgres.database.as_deref(), Some("sdkwork_ai_dev"));
    assert_eq!(
        postgres.credential_source,
        StorageCredentialSource::PasswordFile(
            "/run/secrets/sdkwork/discovery/postgres-password".to_string()
        )
    );
}

#[test]
fn sqlite_storage_uses_file_config_without_network_transport_fields() {
    let toml = minimal_config("dev", None).replace(
        r#"[storage]
provider = "memory""#,
        r#"[storage]
provider = "sqlite"
apply_initial_schema = true

[storage.sqlite]
file = "target/dev/discovery/discovery.sqlite"
max_connections = 1"#,
    );

    let config = DiscoveryRuntimeConfig::from_toml_str(&toml).unwrap();

    let sqlite = config.storage.sqlite.as_ref().unwrap();
    assert_eq!(config.storage.provider, StorageProvider::Sqlite);
    assert_eq!(sqlite.file, "target/dev/discovery/discovery.sqlite");
    assert_eq!(sqlite.max_connections, 1);
    assert!(config.storage.apply_initial_schema);
}

#[test]
fn sqlite_storage_rejects_empty_file_and_zero_connections() {
    let toml = minimal_config("dev", None).replace(
        r#"[storage]
provider = "memory""#,
        r#"[storage]
provider = "sqlite"

[storage.sqlite]
file = " "
max_connections = 0"#,
    );

    let error = DiscoveryRuntimeConfig::from_toml_str(&toml).unwrap_err();

    assert!(error.to_string().contains("sqlite"));
    assert!(error.to_string().contains("file"));
}

#[test]
fn storage_can_enable_explicit_initial_schema_application_outside_production() {
    let toml = minimal_config("dev", None).replace(
        r#"[storage]
provider = "memory""#,
        r#"[storage]
provider = "postgres"
apply_initial_schema = true

[storage.postgres]
host = "127.0.0.1"
port = 5432
database = "sdkwork_ai_dev"
schema = "sdkwork_ai_dev"
username = "sdkwork_ai_dev"
password_file = "/run/secrets/sdkwork/discovery/postgres-password"
tls_enabled = false
connect_timeout_ms = 3000
max_connections = 16"#,
    );

    let config = DiscoveryRuntimeConfig::from_toml_str(&toml).unwrap();

    assert!(config.storage.apply_initial_schema);
}

#[test]
fn production_rejects_automatic_initial_schema_application() {
    let toml = minimal_config("prod", Some("production"))
        .replace(
            r#"grpc_bind_host = "127.0.0.1""#,
            r#"grpc_bind_host = "0.0.0.0""#,
        )
        .replace(DEV_SECURITY_BLOCK, PRODUCTION_SIGNED_SECURITY_BLOCK)
        .replace(
            r#"[storage]
provider = "memory""#,
            r#"[storage]
provider = "postgres"
apply_initial_schema = true

[storage.postgres]
host = "postgres.internal"
port = 5432
database = "sdkwork_ai_prod"
schema = "sdkwork_ai_prod"
username = "sdkwork_ai_prod"
password_file = "/run/secrets/sdkwork/discovery/postgres-password"
tls_enabled = true
connect_timeout_ms = 3000
max_connections = 16"#,
        );

    let error = DiscoveryRuntimeConfig::from_toml_str(&toml).unwrap_err();

    assert!(error.to_string().contains("production"));
    assert!(error.to_string().contains("schema"));
}

#[test]
fn postgres_storage_rejects_direct_password_values() {
    let toml = minimal_config("dev", None).replace(
        r#"[storage]
provider = "memory""#,
        r#"[storage]
provider = "postgres"

[storage.postgres]
host = "127.0.0.1"
port = 5432
database = "sdkwork_ai_dev"
schema = "sdkwork_ai_dev"
username = "sdkwork_ai_dev"
password = "plain-password"
tls_enabled = false
connect_timeout_ms = 3000
max_connections = 16"#,
    );

    let error = DiscoveryRuntimeConfig::from_toml_str(&toml).unwrap_err();

    assert!(error.to_string().contains("password_file"));
}

#[test]
fn production_rejects_memory_storage_provider() {
    let mut toml = minimal_config("prod", Some("production"));
    toml = toml.replace(
        r#"grpc_bind_host = "127.0.0.1""#,
        r#"grpc_bind_host = "0.0.0.0""#,
    );
    toml = toml.replace("enable_reflection = true", "enable_reflection = false");
    toml = toml.replace(DEV_SECURITY_BLOCK, PRODUCTION_SIGNED_SECURITY_BLOCK);
    toml.push_str(PRODUCTION_RESILIENCE_BLOCK);

    let error = DiscoveryRuntimeConfig::from_toml_str(&toml).unwrap_err();

    assert!(error.to_string().contains("production"));
    assert!(error.to_string().contains("memory"));
}

#[test]
fn production_rejects_sqlite_storage_provider() {
    let mut toml = minimal_config("prod", Some("production"))
        .replace(
            r#"grpc_bind_host = "127.0.0.1""#,
            r#"grpc_bind_host = "0.0.0.0""#,
        )
        .replace("enable_reflection = true", "enable_reflection = false")
        .replace(DEV_SECURITY_BLOCK, PRODUCTION_SIGNED_SECURITY_BLOCK)
        .replace(
            r#"[storage]
provider = "memory""#,
            r#"[storage]
provider = "sqlite"

[storage.sqlite]
file = "/var/lib/sdkwork/discovery/discovery.sqlite"
max_connections = 4"#,
        );
    toml.push_str(PRODUCTION_RESILIENCE_BLOCK);

    let error = DiscoveryRuntimeConfig::from_toml_str(&toml).unwrap_err();

    assert!(error.to_string().contains("production"));
    assert!(error.to_string().contains("sqlite"));
}

#[test]
fn production_rejects_enabled_rpc_reflection_without_access_control() {
    let mut toml = minimal_config("prod", Some("production"))
        .replace(
            r#"grpc_bind_host = "127.0.0.1""#,
            r#"grpc_bind_host = "0.0.0.0""#,
        )
        .replace(DEV_SECURITY_BLOCK, PRODUCTION_SIGNED_SECURITY_BLOCK)
        .replace(
            r#"[storage]
provider = "memory""#,
            r#"[storage]
provider = "postgres"

[storage.postgres]
host = "postgres.internal"
port = 5432
database = "sdkwork_ai_prod"
schema = "sdkwork_ai_prod"
username = "sdkwork_ai_prod"
password_file = "/run/secrets/sdkwork/discovery/postgres-password"
tls_enabled = true
connect_timeout_ms = 3000
max_connections = 16"#,
        );
    toml.push_str(PRODUCTION_RESILIENCE_BLOCK);

    let error = DiscoveryRuntimeConfig::from_toml_str(&toml).unwrap_err();

    assert!(error.to_string().contains("production"));
    assert!(error.to_string().contains("reflection"));
}

#[test]
fn production_rejects_tls_enabled_without_server_certificate_files() {
    let toml = minimal_config("prod", Some("production"))
        .replace(
            r#"grpc_bind_host = "127.0.0.1""#,
            r#"grpc_bind_host = "0.0.0.0""#,
        )
        .replace(
            DEV_SECURITY_BLOCK,
            r#"[security]
auth_mode = "service-token"
tls_enabled = true
mtls_enabled = false
allow_unsigned_local_context = false

[security.service_token]
hmac_secret_file = "/run/secrets/sdkwork/discovery/service-token-hmac.secret"
issuer = "sdkwork-discovery"
audience = "sdkwork-discovery-rpc"
max_token_ttl_seconds = 3600"#,
        )
        .replace(
            r#"[storage]
provider = "memory""#,
            r#"[storage]
provider = "postgres"

[storage.postgres]
host = "postgres.internal"
port = 5432
database = "sdkwork_ai_prod"
schema = "sdkwork_ai_prod"
username = "sdkwork_ai_prod"
password_file = "/run/secrets/sdkwork/discovery/postgres-password"
tls_enabled = true
connect_timeout_ms = 3000
max_connections = 16"#,
        );

    let error = DiscoveryRuntimeConfig::from_toml_str(&toml).unwrap_err();

    assert!(error.to_string().contains("TLS"));
    assert!(error.to_string().contains("certificate"));
}

#[test]
fn mtls_requires_client_ca_certificate_file_reference() {
    let toml = minimal_config("dev", None)
        .replace("mtls_enabled = false", "mtls_enabled = true")
        .replace(
            "allow_unsigned_local_context = true",
            r#"allow_unsigned_local_context = false
server_tls_cert_file = "/run/secrets/sdkwork/discovery/server.crt"
server_tls_key_file = "/run/secrets/sdkwork/discovery/server.key"

[security.service_token]
hmac_secret_file = "/run/secrets/sdkwork/discovery/service-token-hmac.secret"
issuer = "sdkwork-discovery"
audience = "sdkwork-discovery-rpc"
max_token_ttl_seconds = 3600"#,
        );

    let error = DiscoveryRuntimeConfig::from_toml_str(&toml).unwrap_err();

    assert!(error.to_string().contains("mTLS"));
    assert!(error.to_string().contains("client CA"));
}

#[test]
fn tls_file_references_reject_inline_pem_material() {
    let toml = minimal_config("dev", None)
        .replace("tls_enabled = false", "tls_enabled = true")
        .replace(
            "allow_unsigned_local_context = true",
            r#"allow_unsigned_local_context = true
server_tls_cert_file = "/run/secrets/sdkwork/discovery/server.crt"
server_tls_key_file = "-----BEGIN PRIVATE KEY-----""#,
        );

    let error = DiscoveryRuntimeConfig::from_toml_str(&toml).unwrap_err();

    assert!(error.to_string().contains("inline PEM"));
}

#[test]
fn env_overlay_accepts_rpc_tls_file_reference_paths() {
    let mut env = BTreeMap::new();
    env.insert(
        "SDKWORK_DISCOVERY_RPC_TLS_ENABLED".to_string(),
        "true".to_string(),
    );
    env.insert(
        "SDKWORK_DISCOVERY_RPC_SERVER_TLS_CERT_FILE".to_string(),
        "/run/secrets/sdkwork/discovery/server.crt".to_string(),
    );
    env.insert(
        "SDKWORK_DISCOVERY_RPC_SERVER_TLS_KEY_FILE".to_string(),
        "/run/secrets/sdkwork/discovery/server.key".to_string(),
    );

    let config =
        DiscoveryRuntimeConfig::from_toml_str_with_env(&minimal_config("dev", None), &env).unwrap();

    assert!(config.security.tls_enabled);
}

#[test]
fn unsupported_storage_provider_is_rejected() {
    let toml = minimal_config("dev", None).replace(r#"provider = "memory""#, r#"provider = "s3""#);

    let error = DiscoveryRuntimeConfig::from_toml_str(&toml).unwrap_err();

    assert!(error.to_string().contains("storage provider"));
}

#[test]
fn deployment_profile_defaults_to_standalone() {
    let config = DiscoveryRuntimeConfig::from_toml_str(&minimal_config("dev", None)).unwrap();

    assert_eq!(
        config.runtime.deployment_profile,
        sdkwork_discovery_contract::RuntimeDeploymentProfile::Standalone
    );
}

#[test]
fn hosting_env_overlay_is_retired_and_rejected() {
    let mut env = BTreeMap::new();
    env.insert(
        "SDKWORK_DISCOVERY_HOSTING".to_string(),
        "cloud-hosted".to_string(),
    );

    let result = DiscoveryRuntimeConfig::from_toml_str_with_env(&minimal_config("dev", None), &env);

    assert!(
        result.is_err(),
        "SDKWORK_DISCOVERY_HOSTING must be rejected as a retired env key"
    );
    let message = result.unwrap_err().to_string().to_lowercase();
    assert!(
        message.contains("unsupported discovery env key"),
        "error should mention unsupported env key, got: {message}"
    );
}

#[test]
fn production_example_config_passes_policy_validation() {
    let toml = include_str!("../../../etc/discovery.production.example.toml");
    DiscoveryRuntimeConfig::from_toml_str(toml)
        .expect("production example config must pass policy validation");
}
