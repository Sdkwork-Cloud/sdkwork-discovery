# RUNBOOK: Database Migration Rollback

Status: active  
Owner: SDKWork Discovery platform  
Application: sdkwork-discovery  
Updated: 2026-06-23  
Specs: DATABASE_SPEC.md, DOCUMENTATION_SPEC.md

## Purpose

Recover from a failed or incompatible Discovery database migration without silent schema drift.

## Authority

Canonical migration SQL:

- `database/migrations/postgres/`
- `tests/fixtures/database/sqlite/migrations/`

Apply and inspect through sdkwork-database CLI from the application root:

```bash
pnpm run db:status
pnpm run db:drift
pnpm run db:drift:check
```

Production config **must not** use `apply_initial_schema = true`. Migrations are applied by pipeline or operator command before traffic.

## Rollback procedure

1. Stop all Discovery server processes writing to the target database.
2. Identify the last known-good migration revision with `pnpm run db:status`.
3. Execute the paired `*.down.sql` for the failed migration using your approved database rollback procedure.
4. Re-run `pnpm run db:drift:check` and confirm no drift against `database/contract/`.
5. Start Discovery with the application version that matches the rolled-back schema.
6. Run registry/config smoke tests before restoring production traffic.

## Escalation

- Do not hand-edit crate-local SQL under `crates/sdkwork-discovery-storage-*/migrations/`; those paths are deprecated.
- Cross-version rollback requires a coordinated release note and consumer compatibility review.
