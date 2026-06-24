# REVIEW-20260623: Release Gate v0.1.0

Status: approved
Date: 2026-06-23
Gate: release
Version: 0.1.0
Runtime target: server
Deployment profiles: cloud, standalone
Specs: QUALITY_GATE_SPEC.md, RELEASE_SPEC.md, CODE_REVIEW_SPEC.md

## Scope

Production release candidate for `sdkwork-discovery` server/container delivery: Service Registry, Config Registry, watch, PostgreSQL durable storage, security hardening, observability, packaging, and operator documentation.

## Release Gate Evidence

| Requirement | Evidence |
| --- | --- |
| Manifest validation | `sdkwork.app.config.json`, `sdkwork.workflow.json`, `specs/component.spec.json` |
| Deployment profile / runtime target | `cloud` + `standalone`; runtime `SERVER` / `CONTAINER` in app manifest |
| Version and changelog | `Cargo.toml` workspace `0.1.0`, [CHANGELOG.md](../../changelogs/CHANGELOG.md), [RELEASE-v0.1.0.md](../../releases/RELEASE-v0.1.0.md) |
| Build verification | `pnpm run verify` (fmt, clippy, workspace tests, topology, proto, package contract, docs) |
| Standards crate | `cargo test -p sdkwork-discovery-standards` (9 tests) |
| Package artifact | `pnpm run package:server` → `dist/server/sdkwork-discovery-0.1.0-*-server.tar.gz` |
| Archive validation | `pnpm run package:server:validate` (checksums, INSTALL, runbooks, release evidence) |
| CI | `.github/workflows/verify.yml` — `verify` + `package-smoke` jobs with sibling dependency checkout |
| Migration readiness | `database/migrations/{postgres,sqlite}/`; [migration rollback runbook](../../runbooks/RUNBOOK-database-migration-rollback.md) |
| Rollout plan | [production deployment runbook](../../runbooks/RUNBOOK-production-server-deployment.md) |
| Signing / SBOM | Not required (`sdkwork.workflow.json` → `signingRequired: false`, `sbomRequired: false`) |
| Architecture | [ADR-20260609-rust-grpc-control-plane.md](../../architecture/decisions/ADR-20260609-rust-grpc-control-plane.md) |

## Security And Privacy Impact

- Production policy enforces TLS/mTLS, service tokens, no unsigned local context, no inline secrets
- Negative tests: bootstrap rejects etcd/consul, missing TLS files, invalid tokens (see `services/sdkwork-discovery-service-host/tests/bootstrap.rs`)

## Residual Risk (Accepted)

| Item | Mitigation |
| --- | --- |
| OTLP export not wired | Prometheus + RPC correlation headers shipped; tracked in roadmap |
| etcd/Consul adapters | Fail-fast bootstrap; documented in ADR and PRD |
| Process-local watch fanout | Durable replay + reconnect semantics documented for integrators |
| `publish.status: DRAFT` | App store listing pending operator sign-off; server package delivery unaffected |

## Verification Commands (Recorded Outcomes)

```bash
pnpm run verify          # pass
pnpm run package:server:validate  # pass
node ../sdkwork-specs/tools/check-repository-docs-standard.mjs --root .  # pass
node ../sdkwork-specs/tools/check-database-framework-standard.mjs --root .  # pass
```

## Approval

Release gate satisfied for **server runtime target** artifacts at version **0.1.0**. Operator approval required before changing `sdkwork.app.config.json` publish status from `DRAFT`.

## Linked Evidence

- [RELEASE-v0.1.0.md](../../releases/RELEASE-v0.1.0.md)
- [CHANGELOG.md](../../changelogs/CHANGELOG.md)
- [PRD.md](../../product/prd/PRD.md)
