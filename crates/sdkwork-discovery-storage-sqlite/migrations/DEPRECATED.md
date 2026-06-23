# Deprecated

Canonical database lifecycle assets live in the application-root `database/migrations/` directory.

Storage crates load schema SQL from `database/migrations/{postgres,sqlite}/` via `migration.rs`.
Do not add new SQL files in this crate-local directory.
