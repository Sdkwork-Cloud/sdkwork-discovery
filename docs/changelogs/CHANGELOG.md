# Changelog

All notable changes to `sdkwork-discovery` are documented in this file.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and aligns with `RELEASE_SPEC.md` and `DOCUMENTATION_SPEC.md`.

## [0.1.0] - 2026-06-23

### Added

- Rust gRPC control plane for Service Registry and Config Registry (`DISCOVERY_SPEC.md`)
- Contract crates, core services, storage adapters (memory, PostgreSQL, SQLite, Redis)
- RPC surface: Registry, Config, Watch, Admin, and gRPC health
- Pagination (`PageRequest` / `PageResponse`) across proto, contract, storage, RPC, and SDK helpers
- Canonical SQL migrations under `database/migrations/{postgres,sqlite}/`
- Typed runtime configuration with safe `SDKWORK_DISCOVERY_*` environment overlay
- TLS/mTLS, HMAC service-token auth, dual-token metadata policy
- W3C `traceparent` parsing, generated `x-request-id`, RPC error correlation metadata
- Prometheus metrics (`prometheus` feature in release packages) and graceful SIGTERM shutdown
- Server release packaging (`scripts/package-server.mjs`) with INSTALL guide, runbooks, and install manifest
- Topology-aware local dev via `@sdkwork/app-topology`
- Documentation canon: PRD, TECH_ARCHITECTURE, ADR, operator/developer/integrator guides, runbooks
- Repository verification chain (`pnpm run verify`) and GitHub CI (`verify` + `package-smoke` jobs)

### Changed

- Service host crate renamed from forbidden `sdkwork-discovery-product` to `sdkwork-discovery-service-host`
- PostgreSQL/SQLite storage bootstraps through `sdkwork-database` instead of raw sqlx pools
- RPC adapter records metrics on validation/auth errors and deadline/cancellation outcomes

### Security

- Production config rejects memory/sqlite providers, unsigned local context, inline secrets, and missing TLS
- Secret material uses `*_file` references only; operational summaries redact secret contents

### Known Limitations

- etcd and Consul providers fail fast at bootstrap until adapters land
- OTLP trace export deferred (structured logs, metrics, and RPC correlation headers shipped)
- Process-local watch fanout; horizontal scale uses sticky streams or reconnect with last revision
- App store publish status remains `DRAFT` pending operator sign-off

### Verification

```bash
pnpm run verify
pnpm run release:validate
```

Release evidence: [RELEASE-v0.1.0.md](../releases/RELEASE-v0.1.0.md)

[0.1.0]: ../releases/RELEASE-v0.1.0.md
