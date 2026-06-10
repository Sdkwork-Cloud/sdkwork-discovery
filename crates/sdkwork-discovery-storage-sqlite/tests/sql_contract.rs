use sdkwork_discovery_storage_sqlite::sql;

#[test]
fn registry_sql_uses_atomic_upsert_and_revisioned_events() {
    assert!(sql::REGISTER_INSTANCE.contains("ON CONFLICT"));
    assert!(sql::SELECT_EXISTING_INSTANCE_LEASE.contains("expires_at_ms >"));
    assert!(sql::RENEW_LEASE.contains("expires_at_ms >"));
    assert!(sql::REPORT_INSTANCE_STATUS.contains("expires_at_ms >"));
    assert!(sql::DEREGISTER_INSTANCE.contains("expires_at_ms >"));
    assert!(sql::NEXT_REVISION.contains("current_revision = current_revision + 1"));
    assert!(sql::INSERT_WATCH_EVENT.contains("discovery_watch_event"));
    assert!(sql::INSERT_WATCH_EVENT.contains("config_application"));
    assert!(sql::SELECT_WATCH_EVENTS.contains("event.config_application"));
    assert!(sql::SELECT_WATCH_EVENTS.contains("release.scope_service_name"));
}

#[test]
fn query_sql_filters_soft_deleted_and_expired_records() {
    assert!(sql::DISCOVER_INSTANCES.contains("deleted_at IS NULL"));
    assert!(sql::DISCOVER_INSTANCES.contains("expires_at_ms >"));
    assert!(sql::RETRIEVE_INSTANCE.contains("instance_id = ?"));
    assert!(sql::RETRIEVE_INSTANCE.contains("deleted_at IS NULL"));
    assert!(sql::RETRIEVE_INSTANCE.contains("expires_at_ms >"));
    assert!(sql::LIST_SERVICES.contains("deleted_at IS NULL"));
    assert!(sql::LIST_SERVICES.contains("expires_at_ms >"));
}

#[test]
fn config_sql_uses_immutable_release_rows() {
    assert!(sql::INSERT_CONFIG_RELEASE_FROM_DRAFT.contains("INSERT INTO discovery_config_release"));
    assert!(sql::INSERT_CONFIG_RELEASE_FROM_DRAFT.contains("SELECT"));
    assert!(sql::MARK_DRAFT_PUBLISHED.contains("published = 1"));
    assert!(sql::SELECT_EFFECTIVE_RELEASES.contains("ORDER BY revision ASC"));
    assert!(sql::SELECT_IDEMPOTENCY_RECORD.contains("discovery_idempotency_record"));
    assert!(sql::INSERT_IDEMPOTENCY_RECORD.contains("operation_id"));
    assert!(sql::INSERT_IDEMPOTENCY_RECORD.contains("request_hash"));
}

#[test]
fn high_volume_maintenance_sql_is_bounded() {
    assert!(sql::SELECT_EXPIRED_INSTANCES.contains("LIMIT"));
    assert!(sql::SELECT_WATCH_EVENTS.contains("LIMIT"));
}
