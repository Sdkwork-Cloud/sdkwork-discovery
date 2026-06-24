# Database Migrations

Schema migration evidence for `sdkwork-discovery` durable storage.

## Canonical SQL

- PostgreSQL: `database/migrations/postgres/`
- SQLite: `database/migrations/sqlite/`

Apply via sdkwork-database CLI from repository root:

```bash
pnpm run db:validate
pnpm run db:migrate
pnpm run db:status
```

Deprecated crate-local migration folders are documented in `database/README.md`.

## Operational Procedures

- Forward migrate before production traffic
- Rollback: [RUNBOOK-database-migration-rollback.md](../runbooks/RUNBOOK-database-migration-rollback.md)

Formal migration plan documents (`MIG-YYYY-NNNN-*.md`) are added here when governed schema changes require tracked migration evidence beyond SQL files.

Spec: `DATABASE_SPEC.md`, `MIGRATION_SPEC.md`
