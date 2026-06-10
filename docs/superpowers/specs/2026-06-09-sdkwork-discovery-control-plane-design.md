# SDKWork Discovery Control Plane Design

## Objective

Build `sdkwork-discovery` as a new SDKWork-standard Rust gRPC control plane for service discovery and versioned runtime configuration. The application owns two bounded capabilities under one infrastructure service:

- Service Registry: service instance registration, lease renewal, health state, discovery, and change watch.
- Config Registry: configuration drafts, immutable releases, rollback, effective configuration resolution, and change watch.

The first implementation must be correct, contract-first, test-first, and free of compatibility debt. No browser UI is part of this scope.

## Standards

This application follows these canonical SDKWork specs:

- `../sdkwork-specs/SOUL.md`
- `../sdkwork-specs/CODE_STYLE_SPEC.md`
- `../sdkwork-specs/NAMING_SPEC.md`
- `../sdkwork-specs/RUST_CODE_SPEC.md`
- `../sdkwork-specs/RPC_SPEC.md`
- `../sdkwork-specs/RUST_RPC_SPEC.md`
- `../sdkwork-specs/RPC_SDK_WORKSPACE_SPEC.md`
- `../sdkwork-specs/CONFIG_SPEC.md`
- `../sdkwork-specs/ENVIRONMENT_SPEC.md`
- `../sdkwork-specs/DATABASE_SPEC.md`
- `../sdkwork-specs/CACHE_SPEC.md`
- `../sdkwork-specs/SECURITY_SPEC.md`
- `../sdkwork-specs/OBSERVABILITY_SPEC.md`
- `../sdkwork-specs/TEST_SPEC.md`

## Architecture

The repository is a Rust workspace modeled after `sdkwork-drive`, with focused crates:

- `sdkwork-discovery-contract`: stable domain types, identifiers, errors, query objects, and DTO-neutral contracts.
- `sdkwork-discovery-config`: typed runtime configuration loading and validation.
- `sdkwork-discovery-storage-contract`: storage ports for registry, config, revision, and event persistence.
- `sdkwork-discovery-storage-memory`: deterministic in-memory store for tests and local development.
- `sdkwork-discovery-core`: application services and policy enforcement over storage ports.
- `sdkwork-discovery-product`: runnable process bootstrap, storage composition, and tonic gRPC server lifecycle.

The RPC contract lives under `proto/` and `sdks/sdkwork-discovery-rpc-sdk/`. The Rust proto crate generates tonic/prost bindings at build time from checked-in proto source. Generated output must not be hand-edited. Core logic stays independent from generated RPC transport so RPC adapters remain thin.

## Service Registry

Registry identity is:

```text
namespace + environment + service_name + instance_id
```

Required behavior:

- Registering an instance is an upsert and returns a lease.
- Renewing a lease extends `expires_at_ms`.
- Reporting instance status updates the reported health state and emits a new revision.
- Deregistration is idempotent.
- Backend service listing aggregates active, non-expired instances by service name.
- Discovery excludes expired instances and non-serving instances by default.
- Every material change increments a monotonic revision.
- Watch events use the same revision stream as config changes.

MVP health model:

- Lease health from TTL.
- Reported health from service instance status.
- Active probing is deferred.

## Config Registry

Configuration is versioned and release-based. Clients read effective released config, never drafts.

Required behavior:

- Draft creation validates scope, format, size, and secret policy.
- Publish creates an immutable release and increments revision.
- Rollback creates a new immutable release from a selected historical release and increments revision.
- Effective config resolution merges scopes from broad to narrow:
  `namespace/environment` -> `application` -> `service_name`.
- Narrower scopes override broader scopes for the same config key.
- Config body values are stored as text with a declared format and content hash.
- Literal secret values are rejected by default; secret references are allowed.

MVP formats:

- `TEXT`
- `JSON`
- `TOML`

## Storage

Storage is async trait-based:

- `RegistryStore`
- `ConfigStore`
- `RevisionStore`
- `WatchEventStore`

MVP storage:

- `memory`: deterministic tests and local development.
- `postgres`: durable registry, config, and watch storage.
- `sqlite`: durable local/test/small single-node registry, config, and watch storage.

Redis, etcd, and Consul adapters are planned after the core contracts stabilize. They must implement the same async store contract tests before being accepted. Durable storage implementations must not hide blocking network I/O behind the storage API.

Runtime storage configuration is already provider-aware:

- `[storage].provider` is a typed selection, not a free-form string.
- `[storage].registry_role`, `[storage].config_role`, and `[storage].watch_role` describe each provider's intended responsibility.
- `[storage.postgres]`, `[storage.redis]`, `[storage.etcd]`, and `[storage.consul]` use structured host, port, optional database, optional username, TLS, timeout, and pool fields.
- `[storage.sqlite]` uses structured file and pool fields and is limited to non-production local/test/small single-node deployments.
- Password material must use `password_file`; direct password values are rejected.
- Production config rejects the `memory` and `sqlite` providers.
- Product bootstrap starts `memory`, `postgres`, and `sqlite` adapters. Providers without implemented adapters fail fast before serving traffic.

## Security

The service layer enforces:

- Service registration requires registry write authority.
- Config publish requires config publish authority.
- Config reads require config read authority.
- Secret literal publishing is rejected unless an explicit policy enables it.

Transport metadata, mTLS, service identity, and backend operator identity are part of the RPC boundary. Core services receive a typed caller context and never inspect raw headers.

## RPC Contract

RPC packages:

```text
sdkwork.discovery.common.v1
sdkwork.discovery.internal.v1
sdkwork.discovery.backend.v3
```

Services:

- `RegistryService`
- `DiscoveryConfigService`
- `DiscoveryWatchService`
- `DiscoveryAdminService`
- `grpc.health.v1.Health`

Every RPC method must be present in `sdks/sdkwork-discovery-rpc-sdk/sdkwork-discovery-rpc.manifest.json`.

## Verification

Required first-pass verification:

- `cargo fmt --all -- --check`
- `cargo test --workspace`

Focused tests must cover:

- Runtime config profile normalization and production safety validation.
- Registry upsert, lease renewal, expiration filtering, and idempotent deregistration.
- Config draft validation, publish, effective resolution, and secret policy rejection.
- Core service permission enforcement.
- RPC manifest coverage by static contract review until proto generation is wired.

## Non-Goals

- No browser UI.
- No app-facing user workflow.
- No compatibility mode for Nacos, Apollo, Redis, etcd, or Consul in the first slice.
- No full Secret Manager. The config registry may store secret references, not plaintext production secrets.
- No active health probing in the first slice.
