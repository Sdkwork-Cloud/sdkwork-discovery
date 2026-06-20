# DISCOVERY Database Module

Canonical lifecycle assets for `sdkwork-discovery` per `DATABASE_FRAMEWORK_SPEC.md`.

- moduleId: `discovery`
- serviceCode: `DISCOVERY`
- tablePrefix: `discovery_`

## Commands

```bash
pnpm run db:materialize:contract
pnpm run db:validate
```

Legacy SQL: `crates/sdkwork-discovery-storage-postgres/migrations/*.sql` → `database/ddl/baseline/postgres/0001_discovery_legacy_baseline.sql`

Runtime bootstrap: `sdkwork-discovery-database-host` via `PostgresDiscoveryStore::apply_initial_schema()` when `apply_initial_schema = true`.
