ALTER TABLE discovery_service_instance
    ADD COLUMN IF NOT EXISTS health_check_json JSONB;

ALTER TABLE discovery_service_instance
    ADD COLUMN IF NOT EXISTS health_check_state_json JSONB NOT NULL DEFAULT '{}'::jsonb;
