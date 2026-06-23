# DISCOVERY Database Module

Canonical lifecycle assets for `sdkwork-discovery` per `DATABASE_FRAMEWORK_SPEC.md`.

- moduleId: `discovery`
- serviceCode: `DISCOVERY`
- tablePrefix: `discovery_`

## Commands

```bash
pnpm run db:materialize:contract
pnpm run db:validate
pnpm run db:migrate
pnpm run db:status
```

## Migrations

Authoritative migration SQL lives under:

- `database/migrations/postgres/`
- `database/migrations/sqlite/`

Crate-local `crates/sdkwork-discovery-storage-*/migrations/` paths are deprecated. Storage crates load SQL through `include_str!` from `database/migrations/`.

Legacy baselines are preserved under `database/ddl/baseline/` for drift review only.

## Runtime bootstrap

- `sdkwork-discovery-database-host` integrates `sdkwork-database-lifecycle` for init/migrate when enabled by deployment policy.
- `apply_initial_schema = true` is allowed only outside production. Production deployments must run migrations through the database CLI or pipeline before serving traffic.
