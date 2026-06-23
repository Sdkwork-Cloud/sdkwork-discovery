ALTER TABLE discovery_service_instance
    DROP COLUMN IF EXISTS health_check_state_json;

ALTER TABLE discovery_service_instance
    DROP COLUMN IF EXISTS health_check_json;
