pub const INITIAL_SCHEMA_SQL: &str = concat!(
    include_str!("../../../database/migrations/postgres/0001_initial_discovery_schema.up.sql"),
    "\n",
    include_str!("../../../database/migrations/postgres/0002_add_health_check_columns.up.sql"),
);
