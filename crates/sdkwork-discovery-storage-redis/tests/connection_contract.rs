use sdkwork_discovery_config::{StorageCredentialSource, StorageTransportConfig};
use sdkwork_discovery_storage_redis::{RedisConnectionOptions, RedisDiscoveryStore};

#[test]
fn redis_connection_options_build_rediss_url_with_credentials() {
    let mut password_path = std::env::temp_dir();
    password_path.push("sdkwork-discovery-redis-password-file-test");
    let _ = std::fs::remove_file(&password_path);
    std::fs::write(&password_path, "s3cret\n").expect("temp password file must be writable");

    let transport = StorageTransportConfig {
        host: "redis.internal".to_string(),
        port: 6380,
        database: Some("2".to_string()),
        schema: None,
        username: Some("discovery".to_string()),
        credential_source: StorageCredentialSource::PasswordFile(
            password_path.to_string_lossy().into_owned(),
        ),
        tls_enabled: true,
        connect_timeout_ms: 2_000,
        max_connections: 16,
    };

    let options = RedisConnectionOptions::from_transport(&transport).unwrap();
    assert_eq!(
        options.redis_url().unwrap(),
        "rediss://discovery:s3cret@redis.internal:6380/2"
    );
    assert!(options.safe_summary().contains("redis host=redis.internal"));
    let _ = std::fs::remove_file(&password_path);
}

#[test]
fn redis_lazy_store_reports_transport_summary() {
    let transport = StorageTransportConfig {
        host: "127.0.0.1".to_string(),
        port: 6379,
        database: Some("0".to_string()),
        schema: None,
        username: None,
        credential_source: StorageCredentialSource::None,
        tls_enabled: false,
        connect_timeout_ms: 1_000,
        max_connections: 8,
    };
    let store = RedisDiscoveryStore::new_lazy(&transport).unwrap();
    assert!(store.safe_summary().contains("redis host=127.0.0.1"));
}
