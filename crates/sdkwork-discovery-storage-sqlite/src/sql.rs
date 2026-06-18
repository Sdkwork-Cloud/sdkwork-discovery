pub const NEXT_REVISION: &str = r#"
INSERT INTO discovery_revision_counter (
    uuid,
    namespace,
    environment,
    current_revision
) VALUES (?, ?, ?, 1)
ON CONFLICT (namespace, environment)
DO UPDATE SET
    current_revision = current_revision + 1,
    updated_at = CURRENT_TIMESTAMP,
    version = version + 1
RETURNING current_revision
"#;

pub const SELECT_EXISTING_INSTANCE_LEASE: &str = r#"
SELECT lease_id, revision
FROM discovery_service_instance
WHERE namespace = ?
  AND environment = ?
  AND service_name = ?
  AND instance_id = ?
  AND deleted_at IS NULL
  AND expires_at_ms > ?
"#;

pub const REGISTER_INSTANCE: &str = r#"
INSERT INTO discovery_service_instance (
    uuid,
    namespace,
    environment,
    service_name,
    instance_id,
    endpoint,
    protocol,
    service_version,
    region,
    zone,
    weight,
    priority,
    status,
    metadata_json,
    lease_id,
    expires_at_ms,
    revision,
    health_check_json,
    health_check_state_json
) VALUES (
    ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
    ?, ?, ?, ?, ?, ?, ?, ?, ?
)
ON CONFLICT (namespace, environment, service_name, instance_id)
DO UPDATE SET
    endpoint = excluded.endpoint,
    protocol = excluded.protocol,
    service_version = excluded.service_version,
    region = excluded.region,
    zone = excluded.zone,
    weight = excluded.weight,
    priority = excluded.priority,
    status = excluded.status,
    metadata_json = excluded.metadata_json,
    lease_id = excluded.lease_id,
    expires_at_ms = excluded.expires_at_ms,
    revision = excluded.revision,
    health_check_json = excluded.health_check_json,
    health_check_state_json = excluded.health_check_state_json,
    updated_at = CURRENT_TIMESTAMP,
    version = discovery_service_instance.version + 1,
    deleted_at = NULL
RETURNING namespace, environment, service_name, instance_id, lease_id, expires_at_ms, revision
"#;

pub const RENEW_LEASE: &str = r#"
UPDATE discovery_service_instance
SET expires_at_ms = ?,
    revision = ?,
    updated_at = CURRENT_TIMESTAMP,
    version = version + 1
WHERE lease_id = ?
  AND deleted_at IS NULL
  AND expires_at_ms > ?
RETURNING namespace, environment, service_name, instance_id, lease_id, expires_at_ms, revision
"#;

pub const REPORT_INSTANCE_STATUS: &str = r#"
UPDATE discovery_service_instance
SET status = ?,
    revision = ?,
    updated_at = CURRENT_TIMESTAMP,
    version = version + 1
WHERE namespace = ?
  AND environment = ?
  AND service_name = ?
  AND instance_id = ?
  AND deleted_at IS NULL
  AND expires_at_ms > ?
RETURNING status, revision
"#;

pub const SELECT_ACTIVE_INSTANCE_FOR_STATUS: &str = r#"
SELECT revision
FROM discovery_service_instance
WHERE namespace = ?
  AND environment = ?
  AND service_name = ?
  AND instance_id = ?
  AND deleted_at IS NULL
  AND expires_at_ms > ?
"#;

pub const DEREGISTER_INSTANCE: &str = r#"
UPDATE discovery_service_instance
SET deleted_at = CURRENT_TIMESTAMP,
    revision = ?,
    updated_at = CURRENT_TIMESTAMP,
    version = version + 1
WHERE namespace = ?
  AND environment = ?
  AND service_name = ?
  AND instance_id = ?
  AND deleted_at IS NULL
  AND expires_at_ms > ?
RETURNING namespace, environment, service_name, instance_id, revision
"#;

pub const SELECT_ACTIVE_INSTANCE_FOR_DEREGISTER: &str = r#"
SELECT 1
FROM discovery_service_instance
WHERE namespace = ?
  AND environment = ?
  AND service_name = ?
  AND instance_id = ?
  AND deleted_at IS NULL
  AND expires_at_ms > ?
"#;

pub const SELECT_EXPIRED_INSTANCES: &str = r#"
SELECT namespace, environment, service_name, instance_id
FROM discovery_service_instance
WHERE deleted_at IS NULL
  AND expires_at_ms <= ?
ORDER BY namespace ASC, environment ASC, service_name ASC, instance_id ASC
LIMIT ?
"#;

pub const EXPIRE_INSTANCE: &str = r#"
UPDATE discovery_service_instance
SET deleted_at = CURRENT_TIMESTAMP,
    revision = ?,
    updated_at = CURRENT_TIMESTAMP,
    version = version + 1
WHERE namespace = ?
  AND environment = ?
  AND service_name = ?
  AND instance_id = ?
  AND deleted_at IS NULL
  AND expires_at_ms <= ?
RETURNING namespace, environment, service_name, instance_id, revision
"#;

pub const DISCOVER_INSTANCES: &str = r#"
SELECT
    namespace,
    environment,
    service_name,
    instance_id,
    endpoint,
    protocol,
    service_version,
    region,
    zone,
    weight,
    priority,
    status,
    metadata_json AS metadata_json_text,
    lease_id,
    expires_at_ms,
    revision,
    health_check_json AS health_check_json_text,
    health_check_state_json AS health_check_state_json_text
FROM discovery_service_instance
WHERE namespace = ?
  AND environment = ?
  AND service_name = ?
  AND deleted_at IS NULL
  AND expires_at_ms > ?
  AND (? IS NULL OR protocol = ?)
  AND (? = 0 OR status IN ('serving', 'degraded'))
ORDER BY instance_id ASC
"#;

pub const LIST_HEALTH_CHECK_INSTANCES: &str = r#"
SELECT
    namespace,
    environment,
    service_name,
    instance_id,
    endpoint,
    protocol,
    service_version,
    region,
    zone,
    weight,
    priority,
    status,
    metadata_json AS metadata_json_text,
    lease_id,
    expires_at_ms,
    revision,
    health_check_json AS health_check_json_text,
    health_check_state_json AS health_check_state_json_text
FROM discovery_service_instance
WHERE deleted_at IS NULL
  AND expires_at_ms > ?
  AND health_check_json IS NOT NULL
"#;

pub const UPDATE_HEALTH_CHECK_STATE: &str = r#"
UPDATE discovery_service_instance
SET health_check_state_json = ?,
    updated_at = CURRENT_TIMESTAMP,
    version = version + 1
WHERE namespace = ?
  AND environment = ?
  AND service_name = ?
  AND instance_id = ?
  AND deleted_at IS NULL
"#;

pub const RETRIEVE_INSTANCE: &str = r#"
SELECT
    namespace,
    environment,
    service_name,
    instance_id,
    endpoint,
    protocol,
    service_version,
    region,
    zone,
    weight,
    priority,
    status,
    metadata_json AS metadata_json_text,
    lease_id,
    expires_at_ms,
    revision,
    health_check_json AS health_check_json_text,
    health_check_state_json AS health_check_state_json_text
FROM discovery_service_instance
WHERE namespace = ?
  AND environment = ?
  AND service_name = ?
  AND instance_id = ?
  AND deleted_at IS NULL
  AND expires_at_ms > ?
"#;

pub const LIST_SERVICES: &str = r#"
SELECT
    namespace,
    environment,
    service_name,
    COUNT(*) AS active_instance_count,
    MAX(revision) AS latest_revision
FROM discovery_service_instance
WHERE namespace = ?
  AND environment = ?
  AND deleted_at IS NULL
  AND expires_at_ms > ?
GROUP BY namespace, environment, service_name
ORDER BY service_name ASC
"#;

pub const INSERT_CONFIG_DRAFT: &str = r#"
INSERT INTO discovery_config_draft (
    uuid,
    draft_id,
    namespace,
    environment,
    config_group,
    config_key,
    config_format,
    config_value,
    scope_kind,
    scope_application,
    scope_service_name,
    created_by,
    content_hash,
    published
) VALUES (
    ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0
)
"#;

pub const SELECT_CONFIG_DRAFT_FOR_PUBLISH: &str = r#"
SELECT
    draft_id,
    namespace,
    environment,
    config_group,
    config_key,
    config_format,
    config_value,
    scope_kind,
    scope_application,
    scope_service_name,
    created_by,
    content_hash,
    published
FROM discovery_config_draft
WHERE draft_id = ?
  AND deleted_at IS NULL
"#;

pub const INSERT_CONFIG_RELEASE_FROM_DRAFT: &str = r#"
INSERT INTO discovery_config_release (
    uuid,
    release_id,
    draft_id,
    namespace,
    environment,
    config_group,
    config_key,
    config_format,
    config_value,
    scope_kind,
    scope_application,
    scope_service_name,
    content_hash,
    published_by,
    published_at_ms,
    revision
)
SELECT
    ?,
    ?,
    draft_id,
    namespace,
    environment,
    config_group,
    config_key,
    config_format,
    config_value,
    scope_kind,
    scope_application,
    scope_service_name,
    content_hash,
    ?,
    ?,
    ?
FROM discovery_config_draft
WHERE draft_id = ?
  AND deleted_at IS NULL
"#;

pub const MARK_DRAFT_PUBLISHED: &str = r#"
UPDATE discovery_config_draft
SET published = 1,
    updated_at = CURRENT_TIMESTAMP,
    version = version + 1
WHERE draft_id = ?
  AND deleted_at IS NULL
"#;

pub const SELECT_CONFIG_RELEASE_FOR_ROLLBACK: &str = r#"
SELECT
    release_id,
    draft_id,
    namespace,
    environment,
    config_group,
    config_key,
    config_format,
    config_value,
    scope_kind,
    scope_application,
    scope_service_name,
    content_hash,
    published_by,
    published_at_ms,
    revision
FROM discovery_config_release
WHERE release_id = ?
  AND deleted_at IS NULL
"#;

pub const INSERT_CONFIG_RELEASE_FROM_RELEASE: &str = r#"
INSERT INTO discovery_config_release (
    uuid,
    release_id,
    draft_id,
    namespace,
    environment,
    config_group,
    config_key,
    config_format,
    config_value,
    scope_kind,
    scope_application,
    scope_service_name,
    content_hash,
    published_by,
    published_at_ms,
    revision
) VALUES (
    ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
    ?, ?, ?, ?, ?, ?
)
"#;

pub const SELECT_EFFECTIVE_RELEASES: &str = r#"
SELECT
    release_id,
    draft_id,
    namespace,
    environment,
    config_group,
    config_key,
    config_format,
    config_value,
    scope_kind,
    scope_application,
    scope_service_name,
    content_hash,
    published_by,
    published_at_ms,
    revision
FROM discovery_config_release
WHERE namespace = ?
  AND environment = ?
  AND config_group = ?
  AND deleted_at IS NULL
  AND (
      scope_kind = 'namespace'
      OR (scope_kind = 'application' AND scope_application = ?)
      OR (scope_kind = 'service' AND scope_application = ? AND scope_service_name = ?)
  )
ORDER BY revision ASC
"#;

pub const SELECT_IDEMPOTENCY_RECORD: &str = r#"
SELECT request_hash, resource_kind, resource_id
FROM discovery_idempotency_record
WHERE operation_id = ?
  AND idempotency_key = ?
  AND deleted_at IS NULL
"#;

pub const INSERT_IDEMPOTENCY_RECORD: &str = r#"
INSERT INTO discovery_idempotency_record (
    uuid,
    operation_id,
    idempotency_key,
    request_hash,
    resource_kind,
    resource_id
) VALUES (?, ?, ?, ?, ?, ?)
"#;

pub const INSERT_WATCH_EVENT: &str = r#"
INSERT INTO discovery_watch_event (
    uuid,
    revision,
    namespace,
    environment,
    event_kind,
    resource_id,
    config_application
) VALUES (?, ?, ?, ?, ?, ?, ?)
"#;

pub const SELECT_WATCH_EVENTS: &str = r#"
SELECT
    event.revision,
    event.namespace,
    event.environment,
    event.event_kind,
    event.resource_id,
    event.config_application,
    COALESCE(instance.service_name, release.scope_service_name) AS service_name,
    release.config_group,
    release.config_key
FROM discovery_watch_event event
LEFT JOIN discovery_service_instance instance
    ON event.namespace = instance.namespace
   AND event.environment = instance.environment
   AND event.resource_id = instance.instance_id
   AND event.event_kind IN (
       'instance_registered',
       'instance_updated',
       'instance_status_reported',
       'instance_renewed',
       'instance_deregistered'
   )
LEFT JOIN discovery_config_release release
    ON event.resource_id = release.release_id
   AND event.event_kind IN ('config_published', 'config_rolled_back')
WHERE event.namespace = ?
  AND event.environment = ?
  AND event.revision > ?
  AND event.deleted_at IS NULL
ORDER BY event.revision ASC
LIMIT ?
"#;

pub const GC_WATCH_EVENTS: &str = r#"
DELETE FROM discovery_watch_event
WHERE revision IN (
    SELECT revision FROM discovery_watch_event
    WHERE revision <= ?
      AND deleted_at IS NULL
    ORDER BY revision ASC
    LIMIT ?
)
"#;

pub const SELECT_CURRENT_REVISION: &str = r#"
SELECT COALESCE(MAX(current_revision), 0) AS current_revision
FROM discovery_revision_counter
"#;

pub const COMPACT_WATCH_EVENTS: &str = r#"
DELETE FROM discovery_watch_event
WHERE revision NOT IN (
    SELECT revision FROM (
        SELECT revision, ROW_NUMBER() OVER (
            PARTITION BY resource_id ORDER BY revision DESC
        ) as rn
        FROM discovery_watch_event
        WHERE namespace = ?
          AND environment = ?
          AND deleted_at IS NULL
    ) ranked
    WHERE rn <= ?
)
AND namespace = ?
AND environment = ?
AND deleted_at IS NULL
"#;
