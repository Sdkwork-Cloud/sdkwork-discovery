use sdkwork_discovery_storage_sqlite::SqliteDiscoveryStore;

#[test]
fn lazy_store_can_be_constructed_without_tokio_context() {
    let store = SqliteDiscoveryStore::new_lazy("target/dev/discovery/discovery.sqlite", 1).unwrap();

    assert_eq!(
        store.safe_summary(),
        "sqlite file=target/dev/discovery/discovery.sqlite max_connections=1"
    );
}

#[test]
fn lazy_memory_store_uses_single_connection_without_tokio_context() {
    let store = SqliteDiscoveryStore::new_lazy(":memory:", 16).unwrap();

    assert_eq!(
        store.safe_summary(),
        "sqlite file=:memory: max_connections=1"
    );
}

#[test]
fn store_exposes_initial_schema_for_deployment_bootstrap() {
    let store = SqliteDiscoveryStore::new_lazy("target/dev/discovery/discovery.sqlite", 1).unwrap();

    assert!(store
        .initial_schema_sql()
        .contains("CREATE TABLE IF NOT EXISTS discovery_service_instance"));
}
