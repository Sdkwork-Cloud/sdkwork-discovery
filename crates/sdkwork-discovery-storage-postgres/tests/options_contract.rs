use sdkwork_discovery_config::{StorageCredentialSource, StorageTransportConfig};
use sdkwork_discovery_storage_postgres::PostgresConnectionOptions;

fn transport() -> StorageTransportConfig {
    StorageTransportConfig {
        host: "postgres.internal".to_string(),
        port: 5432,
        database: Some("sdkwork_discovery".to_string()),
        schema: Some("sdkwork_discovery_runtime".to_string()),
        username: Some("sdkwork_discovery".to_string()),
        credential_source: StorageCredentialSource::PasswordFile(
            "/run/secrets/sdkwork/discovery/postgres-password".to_string(),
        ),
        tls_enabled: true,
        connect_timeout_ms: 3000,
        max_connections: 16,
    }
}

#[test]
fn connection_options_are_derived_from_structured_transport_config() {
    let options = PostgresConnectionOptions::from_transport(&transport(), Some("s3cret")).unwrap();

    assert_eq!(options.host(), "postgres.internal");
    assert_eq!(options.port(), 5432);
    assert_eq!(options.database(), "sdkwork_discovery");
    assert_eq!(options.schema(), Some("sdkwork_discovery_runtime"));
    assert_eq!(options.username(), Some("sdkwork_discovery"));
    assert_eq!(options.max_connections(), 16);
    assert_eq!(options.connect_timeout_ms(), 3000);
    assert_eq!(
        options.safe_summary(),
        "postgres host=postgres.internal port=5432 database=sdkwork_discovery schema=sdkwork_discovery_runtime username=sdkwork_discovery tls=true max_connections=16"
    );
}

#[test]
fn connection_options_apply_configured_schema_as_search_path() {
    let options = PostgresConnectionOptions::from_transport(&transport(), None).unwrap();

    assert_eq!(
        options.to_sqlx_connect_options().get_options(),
        Some("-c search_path=sdkwork_discovery_runtime")
    );
}

#[test]
fn connection_options_never_expose_password_material() {
    let options = PostgresConnectionOptions::from_transport(&transport(), Some("s3cret")).unwrap();

    assert!(!options.connection_uri().contains("s3cret"));
    assert!(!options.safe_summary().contains("s3cret"));
    assert!(!format!("{options:?}").contains("s3cret"));
}

#[test]
fn connection_options_support_passwordless_local_development() {
    let mut transport = transport();
    transport.credential_source = StorageCredentialSource::None;

    let options = PostgresConnectionOptions::from_transport(&transport, None).unwrap();

    assert!(options
        .connection_uri()
        .starts_with("postgres://sdkwork_discovery@"));
    assert!(!options.connection_uri().contains("password"));
}

#[test]
fn connection_options_reject_password_for_passwordless_transport() {
    let mut transport = transport();
    transport.credential_source = StorageCredentialSource::None;

    let error =
        PostgresConnectionOptions::from_transport(&transport, Some("unexpected")).unwrap_err();

    assert!(error.to_string().contains("password"));
    assert!(error.to_string().contains("password_file"));
}
