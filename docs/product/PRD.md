# SDKWork Discovery PRD

Status: draft
Owner: SDKWork maintainers
Application: sdkwork-discovery
Updated: 2026-06-23
Specs: REQUIREMENTS_SPEC.md, DOCUMENTATION_SPEC.md, DISCOVERY_SPEC.md

## 1. Background And Problem

SDKWork deployments need a single platform-standard control plane for dynamic RPC resolution and versioned runtime configuration. Ad hoc Redis service lists, environment-only peer tables, and parallel registry products create drift, security gaps, and migration debt across tenants and environments.

`sdkwork-discovery` is the canonical implementation of `DISCOVERY_SPEC.md`. Business RPC data-plane services register instances and read effective configuration through generated RPC SDKs and the SDKWork RPC framework resolver instead of hard-coded endpoints.

## 2. Target Users

| Persona | Need |
| --- | --- |
| Platform operator | Deploy, scale, migrate, and observe the discovery control plane in production |
| Backend service owner | Register instances, renew leases, publish configuration releases |
| SDKWork integrator | Consume `sdkwork-discovery-rpc-sdk` from Rust or TypeScript services |
| Platform architect | Align topology, security, and storage profiles with SDKWork standards |

Browser, H5, mini program, and most mobile UI clients are out of scope; they continue to use generated HTTP SDKs unless a governed gRPC-Web ADR exists.

## 3. Goals And Non-Goals

### Goals

- Provide Service Registry and Config Registry capabilities under one gRPC infrastructure service.
- Enforce contract-first RPC under `proto/` and `sdks/sdkwork-discovery-rpc-sdk/`.
- Support durable PostgreSQL production storage with revision-ordered watch semantics.
- Ship production-ready runtime configuration, security (TLS/mTLS, service tokens), metrics, graceful shutdown, packaging, and operator runbooks.
- Pass repository verification (`pnpm run verify`) and component spec gates.

### Non-Goals

- Browser UI for registry administration in the first platform slice.
- Compatibility shims for Nacos, Apollo, Eureka, Consul KV, or etcd APIs.
- Plaintext secret storage in config releases.
- Active health probing beyond lease TTL and reported instance status in the first slice.

## 4. Scope

In scope for this product:

- Instance registration, lease renewal, deregistration, discovery queries, and watch streams.
- Config draft validation, immutable publish, rollback, effective resolution, and watch streams.
- Typed runtime configuration with safe `SDKWORK_DISCOVERY_*` environment overlay.
- Storage providers: `memory`, `postgres`, `sqlite`, `redis` (durable); `etcd` and `consul` fail fast until adapters land.
- Runnable service host (`services/sdkwork-discovery-service-host`), release packaging, and CI verification.

Out of scope:

- End-user workflows and app-store consumer UX beyond server/container delivery metadata.
- Domain business RPC services (they are discovery clients, not this product).

## 5. User Scenarios

1. **Service bootstrap** — A Rust RPC server registers `namespace + environment + service_name + instance_id`, renews its lease on an interval, and deregisters on shutdown.
2. **Caller resolution** — An internal orchestration service discovers healthy gRPC instances through the RPC framework resolver backed by discovery watch/cache updates.
3. **Config release** — An operator validates a draft, publishes an immutable release, and downstream services read effective merged configuration at `namespace/environment → application → service_name` precedence.
4. **Production deploy** — An operator applies `database/migrations/postgres/`, starts the server with TLS and service-token auth, scrapes Prometheus metrics, and follows runbooks for migration rollback if needed.
5. **Local development** — A developer runs `pnpm discovery:dev` with topology profiles and optional unsigned local caller context on loopback only.

## 6. Success Metrics

| Metric | Target |
| --- | --- |
| Verification | `pnpm run verify` green on main; CI workflow mirrors component spec commands |
| Contract parity | RPC manifest covers all proto services; pagination and tracing helpers in Rust/TS SDKs |
| Production safety | Production config rejects memory/sqlite, unsigned local context, inline secrets, and missing TLS |
| Operability | Release package includes production template, INSTALL guidance, and bundled runbooks |
| Documentation | Canon PRD and technical architecture registered in `docs/INDEX.yaml`; docs standard check passes |

## 7. Phases

| Phase | Status | Deliverable |
| --- | --- | --- |
| Core contracts and storage | Complete | Contract crates, postgres/sqlite/memory/redis adapters, migrations under `database/migrations/` |
| RPC surface and SDK | Complete | Registry, config, watch, admin services; Rust/TS SDK helpers |
| Production hardening | Complete | TLS, service tokens, metrics, graceful shutdown, packaging, runbooks |
| Distributed adapters | Planned | etcd/Consul durable adapters after shared contract tests |
| OTLP tracing export | Planned | SHOULD per OBSERVABILITY_SPEC; metrics and structured errors shipped first |

## 8. Linked Requirements

- Design evidence: [2026-06-09-sdkwork-discovery-control-plane-design.md](../superpowers/specs/2026-06-09-sdkwork-discovery-control-plane-design.md)
- Operator runbooks: [production deployment](../runbooks/RUNBOOK-production-server-deployment.md), [database migration rollback](../runbooks/RUNBOOK-database-migration-rollback.md)
- Component contract: `specs/component.spec.json`
- Application identity: `sdkwork.app.config.json`

Detailed requirement IDs live under `docs/product/requirements/` as they are formalized.

## 9. Open Questions

- Timing and ADR for gRPC-Web discovery access from browser clients.
- Distributed watch fanout strategy for multi-writer etcd/Consul deployments.
- App store publish status transition from `DRAFT` to production listing after operator sign-off.
