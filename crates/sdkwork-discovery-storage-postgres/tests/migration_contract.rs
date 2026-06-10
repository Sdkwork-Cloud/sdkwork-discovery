use sdkwork_discovery_storage_postgres::migration;

#[test]
fn migration_contains_standard_discovery_tables_and_constraints() {
    let sql = migration::INITIAL_SCHEMA_SQL;

    assert!(sql.contains("CREATE TABLE IF NOT EXISTS discovery_service_instance"));
    assert!(sql.contains("CREATE TABLE IF NOT EXISTS discovery_config_draft"));
    assert!(sql.contains("CREATE TABLE IF NOT EXISTS discovery_config_release"));
    assert!(sql.contains("CREATE TABLE IF NOT EXISTS discovery_idempotency_record"));
    assert!(sql.contains("CREATE TABLE IF NOT EXISTS discovery_watch_event"));
    assert!(sql.contains("config_application VARCHAR(256)"));
    assert!(sql.contains("CREATE TABLE IF NOT EXISTS discovery_revision_counter"));
    assert!(sql.contains("uk_discovery_service_instance_identity"));
    assert!(sql.contains("uk_discovery_config_draft_id"));
    assert!(sql.contains("uk_discovery_config_release_id"));
    assert!(sql.contains("uk_discovery_idempotency_record_identity"));
    assert!(sql.contains("idx_discovery_idempotency_record_resource"));
    assert!(sql.contains("idx_discovery_service_instance_expiry"));
    assert!(sql.contains("idx_discovery_watch_event_scope_revision"));
}

#[test]
fn migration_uses_sdkwork_standard_audit_and_lifecycle_columns() {
    let sql = migration::INITIAL_SCHEMA_SQL;

    for required_column in [
        "id BIGSERIAL",
        "uuid VARCHAR(64) NOT NULL",
        "created_at TIMESTAMPTZ NOT NULL",
        "updated_at TIMESTAMPTZ NOT NULL",
        "version BIGINT NOT NULL DEFAULT 0",
        "deleted_at TIMESTAMPTZ",
    ] {
        assert!(
            sql.contains(required_column),
            "missing standard column: {required_column}"
        );
    }
}
