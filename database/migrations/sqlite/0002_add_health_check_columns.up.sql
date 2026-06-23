ALTER TABLE discovery_service_instance
    ADD COLUMN health_check_json TEXT;

ALTER TABLE discovery_service_instance
    ADD COLUMN health_check_state_json TEXT NOT NULL DEFAULT '{}';
