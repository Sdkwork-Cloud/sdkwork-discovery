pub const NEXT_REVISION: &str = r#"
INSERT INTO discovery_revision_counter (
    uuid,
    namespace,
    environment,
    current_revision
) VALUES ($1, $2, $3, 1)
ON CONFLICT (namespace, environment)
DO UPDATE SET
    current_revision = current_revision + 1,
    updated_at = now(),
    version = discovery_revision_counter.version + 1
RETURNING current_revision
"#;

pub const SELECT_EXISTING_INSTANCE_LEASE: &str = r#"
SELECT lease_id, revision
FROM discovery_service_instance
WHERE namespace = $1
  AND environment = $2
  AND service_name = $3
  AND instance_id = $4
  AND deleted_at IS NULL
  AND expires_at_ms > $5
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
    $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
    $11, $12, $13, $14::jsonb, $15, $16, $17, $18::jsonb, $19::jsonb
)
ON CONFLICT ON CONSTRAINT uk_discovery_service_instance_identity
DO UPDATE SET
    endpoint = EXCLUDED.endpoint,
    protocol = EXCLUDED.protocol,
    service_version = EXCLUDED.service_version,
    region = EXCLUDED.region,
    zone = EXCLUDED.zone,
    weight = EXCLUDED.weight,
    priority = EXCLUDED.priority,
    status = EXCLUDED.status,
    metadata_json = EXCLUDED.metadata_json,
    lease_id = EXCLUDED.lease_id,
    expires_at_ms = EXCLUDED.expires_at_ms,
    revision = EXCLUDED.revision,
    health_check_json = EXCLUDED.health_check_json,
    health_check_state_json = EXCLUDED.health_check_state_json,
    updated_at = now(),
    version = discovery_service_instance.version + 1,
    deleted_at = NULL
RETURNING namespace, environment, service_name, instance_id, lease_id, expires_at_ms, revision
"#;

pub const RENEW_LEASE: &str = r#"
UPDATE discovery_service_instance
SET expires_at_ms = $2,
    revision = $3,
    updated_at = now(),
    version = version + 1
WHERE lease_id = $1
  AND deleted_at IS NULL
  AND expires_at_ms > $4
RETURNING namespace, environment, service_name, instance_id, lease_id, expires_at_ms, revision
"#;

pub const REPORT_INSTANCE_STATUS: &str = r#"
UPDATE discovery_service_instance
SET status = $5,
    revision = $6,
    updated_at = now(),
    version = version + 1
WHERE namespace = $1
  AND environment = $2
  AND service_name = $3
  AND instance_id = $4
  AND deleted_at IS NULL
  AND expires_at_ms > $7
RETURNING status, revision
"#;

pub const SELECT_ACTIVE_INSTANCE_FOR_STATUS: &str = r#"
SELECT revision
FROM discovery_service_instance
WHERE namespace = $1
  AND environment = $2
  AND service_name = $3
  AND instance_id = $4
  AND deleted_at IS NULL
  AND expires_at_ms > $5
"#;

pub const DEREGISTER_INSTANCE: &str = r#"
UPDATE discovery_service_instance
SET deleted_at = now(),
    revision = $5,
    updated_at = now(),
    version = version + 1
WHERE namespace = $1
  AND environment = $2
  AND service_name = $3
  AND instance_id = $4
  AND deleted_at IS NULL
  AND expires_at_ms > $6
RETURNING namespace, environment, service_name, instance_id, revision
"#;

pub const SELECT_ACTIVE_INSTANCE_FOR_DEREGISTER: &str = r#"
SELECT 1
FROM discovery_service_instance
WHERE namespace = $1
  AND environment = $2
  AND service_name = $3
  AND instance_id = $4
  AND deleted_at IS NULL
  AND expires_at_ms > $5
"#;

pub const SELECT_EXPIRED_INSTANCES: &str = r#"
SELECT namespace, environment, service_name, instance_id
FROM discovery_service_instance
WHERE deleted_at IS NULL
  AND expires_at_ms <= $1
ORDER BY namespace ASC, environment ASC, service_name ASC, instance_id ASC
LIMIT $2
"#;

pub const EXPIRE_INSTANCE: &str = r#"
UPDATE discovery_service_instance
SET deleted_at = now(),
    revision = $5,
    updated_at = now(),
    version = version + 1
WHERE namespace = $1
  AND environment = $2
  AND service_name = $3
  AND instance_id = $4
  AND deleted_at IS NULL
  AND expires_at_ms <= $6
RETURNING namespace, environment, service_name, instance_id, revision
"#;

pub const DISCOVER_INSTANCES_PREFIX: &str = r#"
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
    metadata_json::text AS metadata_json_text,
    lease_id,
    expires_at_ms,
    revision,
    health_check_json::text AS health_check_json_text,
    health_check_state_json::text AS health_check_state_json_text
FROM discovery_service_instance
WHERE namespace = $1
  AND environment = $2
  AND service_name = $3
  AND deleted_at IS NULL
  AND expires_at_ms > $4
  AND ($5::TEXT IS NULL OR protocol = $5)
  AND ($6::BOOLEAN = false OR status IN ('serving', 'degraded'))
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
    metadata_json::text AS metadata_json_text,
    lease_id,
    expires_at_ms,
    revision,
    health_check_json::text AS health_check_json_text,
    health_check_state_json::text AS health_check_state_json_text
FROM discovery_service_instance
WHERE deleted_at IS NULL
  AND expires_at_ms > $1
  AND health_check_json IS NOT NULL
"#;

pub const UPDATE_HEALTH_CHECK_STATE: &str = r#"
UPDATE discovery_service_instance
SET health_check_state_json = $5::jsonb,
    updated_at = now(),
    version = version + 1
WHERE namespace = $1
  AND environment = $2
  AND service_name = $3
  AND instance_id = $4
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
    metadata_json::text AS metadata_json_text,
    lease_id,
    expires_at_ms,
    revision,
    health_check_json::text AS health_check_json_text,
    health_check_state_json::text AS health_check_state_json_text
FROM discovery_service_instance
WHERE namespace = $1
  AND environment = $2
  AND service_name = $3
  AND instance_id = $4
  AND deleted_at IS NULL
  AND expires_at_ms > $5
"#;

pub const LIST_SERVICES: &str = r#"
SELECT
    namespace,
    environment,
    service_name,
    COUNT(*)::BIGINT AS active_instance_count,
    MAX(revision)::BIGINT AS latest_revision
FROM discovery_service_instance
WHERE namespace = $1
  AND environment = $2
  AND deleted_at IS NULL
  AND expires_at_ms > $3
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
    $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, false
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
WHERE draft_id = $1
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
    $1,
    $2,
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
    $3,
    $4,
    $5
FROM discovery_config_draft
WHERE draft_id = $6
  AND deleted_at IS NULL
"#;

pub const MARK_DRAFT_PUBLISHED: &str = r#"
UPDATE discovery_config_draft
SET published = true,
    updated_at = now(),
    version = version + 1
WHERE draft_id = $1
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
WHERE release_id = $1
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
    $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
    $11, $12, $13, $14, $15, $16
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
WHERE namespace = $1
  AND environment = $2
  AND config_group = $3
  AND deleted_at IS NULL
  AND (
      scope_kind = 'namespace'
      OR (scope_kind = 'application' AND scope_application = $4)
      OR (scope_kind = 'service' AND scope_application = $4 AND scope_service_name = $5)
  )
ORDER BY revision ASC
"#;

pub const SELECT_IDEMPOTENCY_RECORD: &str = r#"
SELECT request_hash, resource_kind, resource_id
FROM discovery_idempotency_record
WHERE operation_id = $1
  AND idempotency_key = $2
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
) VALUES ($1, $2, $3, $4, $5, $6)
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
) VALUES ($1, $2, $3, $4, $5, $6, $7)
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
WHERE event.namespace = $1
  AND event.environment = $2
  AND event.revision > $3
  AND event.deleted_at IS NULL
ORDER BY event.revision ASC
LIMIT $4
"#;

pub const GC_WATCH_EVENTS: &str = r#"
DELETE FROM discovery_watch_event
WHERE revision IN (
    SELECT revision FROM discovery_watch_event
    WHERE revision <= $1
      AND deleted_at IS NULL
    ORDER BY revision ASC
    LIMIT $2
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
        WHERE namespace = $1
          AND environment = $2
          AND deleted_at IS NULL
    ) ranked
    WHERE rn <= $3
)
AND namespace = $4
AND environment = $5
AND deleted_at IS NULL
"#;
