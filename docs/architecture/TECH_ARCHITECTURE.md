# SDKWork Discovery Technical Architecture

Status: draft
Owner: SDKWork maintainers
Updated: 2026-06-23
Specs: ARCHITECTURE_DECISION_SPEC.md, DOCUMENTATION_SPEC.md, DISCOVERY_SPEC.md, RUST_RPC_SPEC.md

## 1. Architecture Overview

SDKWork separates RPC control plane from RPC data plane:

```text
sdkwork-specs (L0)
  DISCOVERY_SPEC.md + RPC_FRAMEWORK_SPEC.md
       -> narrows
sdkwork-discovery (L1 control plane)
  Registry + Config + Watch over gRPC
       -> resolves
sdkwork-rpc-framework client resolver (L1 invocation runtime)
       -> calls
domain RPC servers (L2 data plane)
```

`sdkwork-discovery` exposes gRPC services defined under `proto/sdkwork.discovery.*`. Callers integrate through `sdkwork-discovery-rpc-sdk` (Rust and TypeScript). RPC adapters in `sdkwork-discovery-rpc` remain thin: metadata/context mapping, validation, dispatch, response mapping, error mapping, metrics, and tracing headers.

Domain logic lives in `sdkwork-discovery-core` over async storage ports defined in `sdkwork-discovery-storage-contract`.

## 2. Technology Choices

| Layer | Choice | Rationale |
| --- | --- | --- |
| Language | Rust workspace | Aligns with SDKWork server/control-plane stack and tonic gRPC ecosystem |
| RPC transport | tonic + prost | Standard SDKWork Rust RPC stack; proto generated at build time |
| Configuration | Typed TOML + validated env overlay | `CONFIG_SPEC.md` / `ENVIRONMENT_SPEC.md` compliance |
| Durable storage | PostgreSQL (production), SQLite (local), Redis (optional) | Shared migration canon under `database/migrations/` via sdkwork-database |
| Observability | Prometheus metrics (release builds), structured RPC errors with trace/request IDs | `OBSERVABILITY_SPEC.md`; OTLP export deferred |
| Packaging | `scripts/package-server.mjs` tar archive with manifest and runbooks | Commercial server/container delivery |

## 3. System Boundaries And Modules

| Crate / service | Responsibility |
| --- | --- |
| `sdkwork-discovery-contract` | Domain types, identifiers, errors, pagination, query objects |
| `sdkwork-discovery-config` | Runtime config load, normalization, production policy validation |
| `sdkwork-discovery-storage-contract` | `RegistryStore`, `ConfigStore`, `RevisionStore`, `WatchEventStore` ports |
| `sdkwork-discovery-storage-*` | Provider adapters (memory, postgres, sqlite, redis) |
| `sdkwork-discovery-core` | Application services, lease expiry, permission policy |
| `sdkwork-discovery-rpc` | gRPC service implementations and RPC boundary |
| `sdkwork-discovery-health-checker` | Health probe helpers for registered instances |
| `sdkwork-discovery-service-host` | Process bootstrap, storage composition, server lifecycle, shutdown |
| `proto/` | Source-of-truth protobuf contracts |
| `sdks/sdkwork-discovery-rpc-sdk/` | Generated SDK family plus handwritten deadline/idempotency/pagination/tracing helpers |

Registry identity: `namespace + environment + service_name + instance_id`. Config effective resolution merges scopes from broad to narrow with immutable publish/rollback semantics.

## 4. Directory And Package Layout

```text
crates/           Domain, config, storage, RPC adapter libraries
services/         Runnable service host binary
proto/            RPC contracts (buf-managed)
sdks/             RPC SDK manifest and generated/handwritten SDK artifacts
database/         Canonical SQL migrations and seeds
etc/              Example and production TOML templates
configs/topology/ Hosting and ingress topology profiles
docs/             PRD, architecture, runbooks, guides
scripts/          Verify, package, dev bootstrap
specs/            Component and topology contracts
```

Rust `src/lib.rs` files are module assembly boundaries only; business logic stays in focused modules per `RUST_CODE_SPEC.md`.

## 5. API, SDK, And Data Ownership

RPC packages:

```text
sdkwork.discovery.common.v1
sdkwork.discovery.internal.v1
sdkwork.discovery.backend.v3
```

Services: `RegistryService`, `DiscoveryConfigService`, `DiscoveryWatchService`, `DiscoveryAdminService`, `grpc.health.v1.Health`.

Every RPC method is listed in `sdks/sdkwork-discovery-rpc-sdk/sdkwork-discovery-rpc.manifest.json`. Generated SDK output is never hand-edited; contract changes flow proto → regenerate → verify.

Database schema ownership lives in `database/migrations/{postgres,sqlite}/`. Crate-local migration folders are deprecated.

Events emitted at the platform boundary: `discovery.registry.changed`, `discovery.config.changed` (revision-ordered watch streams).

## 6. Security, Privacy, And Observability

**Authentication**

- Production requires TLS or mTLS and rejects `allow_unsigned_local_context`.
- Service-token mode: HMAC-signed bearer token plus bound `access-token`; permissions encoded in token claims.
- Development loopback mode may allow unsigned local context with explicit metadata when policy enables it.

**Secrets**

- Password and key material use `*_file` references only; inline secrets rejected in config registry and runtime config.
- Operational summaries redact secret paths and contents.

**Observability**

- RPC errors attach `x-trace-id` and `x-request-id` (W3C `traceparent` when present).
- `RpcMetricsGuard` records latency, errors, and deadline/cancellation outcomes.
- Release packages build with `prometheus` feature; scrape bind via `SDKWORK_DISCOVERY_METRICS_BIND`.

## 7. Deployment And Runtime Topology

The service host supports dual ingress surfaces per `APP_RUNTIME_TOPOLOGY_SPEC.md`:

- **application.public-ingress** — application-facing gRPC
- **operations.control-ingress** — backend/operator gRPC

When binds differ, two tonic servers run; when equal, a combined server is used.

Production profile expectations:

- PostgreSQL storage with migrations applied before traffic
- Non-loopback bind addresses
- Reflection disabled or governed at deployment layer
- Graceful shutdown on SIGTERM and Ctrl+C
- Topology env files under `configs/topology/*.production.env`

Watch streams replay durable events from storage starting at `from_revision`, then receive live process-local fanout. Horizontal scale uses sticky streams or client reconnect with last revision until distributed fanout adapters exist.

Storage providers without implemented adapters (`etcd`, `consul`) fail fast at bootstrap with documented configuration shape.

## 8. Architecture Decision Index

Formal ADRs are recorded under `docs/architecture/decisions/` using `ARCHITECTURE_DECISION_SPEC.md` filename patterns.

| ADR | Topic |
| --- | --- |
| [ADR-20260609-rust-grpc-control-plane.md](decisions/ADR-20260609-rust-grpc-control-plane.md) | Rust gRPC control plane crate boundaries and storage profile |

Additional design evidence:

- [2026-06-09-sdkwork-discovery-control-plane-design.md](../superpowers/specs/2026-06-09-sdkwork-discovery-control-plane-design.md)
- `DISCOVERY_SPEC.md` — platform-standard registry/config/watch semantics (external canon)

## 9. Verification

Repository gates (also listed in `specs/component.spec.json`):

```bash
pnpm run verify
pnpm run package:server:validate
node ../sdkwork-specs/tools/check-repository-docs-standard.mjs --root .
node ../sdkwork-specs/tools/check-database-framework-standard.mjs --root .
```

Focused Rust tests cover runtime config policy, registry lifecycle, config publish/rollback, permission enforcement, RPC metadata/tracing, and module boundary standards via `xtask/sdkwork-discovery-standards`.
