# SDKWork Discovery Technical Architecture

Status: active
Owner: SDKWork maintainers
Updated: 2026-06-26
Specs: ARCHITECTURE_DECISION_SPEC.md, DOCUMENTATION_SPEC.md, DISCOVERY_SPEC.md, RPC_SPEC.md, HEALTH_CHECK_SPEC.md

## Document Map

- [TECH-2026-06-09-sdkwork-discovery-control-plane-design.md](TECH-2026-06-09-sdkwork-discovery-control-plane-design.md)
- [TECH-2026-06-09-sdkwork-discovery-control-plane.md](TECH-2026-06-09-sdkwork-discovery-control-plane.md)
- [TECH-topology-standard.md](TECH-topology-standard.md)

## 1. Architecture Overview

SDKWork Discovery is a dual-plane gRPC control plane for service registry, config registry, and audit eventing. The repository ships a `sdkwork-discovery-service-host` binary that owns the gRPC transports, durable storage adapters, and health/operations surfaces.

Architecture detail lives in the linked TECH shards below.

## 2. Technology Choices

- **gRPC transport**: tonic + prost, generated from `sdkwork-discovery-rpc-proto`.
- **Durable storage**: Postgres via `sdkwork-database-sqlx` and `sdkwork-database-config`; Redis cache adapter; SQLite for local development.
- **Memory cache**: in-process `sdkwork-discovery-storage-memory` for unit tests and ephemeral profiles.
- **Health probes**: mounted through `sdkwork-web-bootstrap` `service_router`, exposed as `/healthz`, `/readyz`, `/livez`, and `/metrics`.
- **SDK family**: the generated RPC SDK workspace `sdkwork-discovery-rpc-sdk` ships Rust and TypeScript bindings under `sdks/sdkwork-discovery-rpc-sdk/`.

## 3. System Boundaries And Modules

- `crates/sdkwork-discovery-rpc-proto`: protoc-generated types, owned by this repo.
- `crates/sdkwork-discovery-rpc`: gRPC adapter (server, services, health, resilience, watch). Depends on proto and contract only.
- `crates/sdkwork-discovery-storage-*`: storage adapters behind the contract ports.
- `services/sdkwork-discovery-service-host`: bootstrap, runtime, HTTP probes, wiring.

## 4. Directory And Package Layout

- `crates/`: library crates (proto, rpc, config, contract, storage adapters).
- `services/`: runnable binaries.
- `sdks/sdkwork-discovery-rpc-sdk/`: SDK language workspaces (Rust + TypeScript) that consume the RPC SDK family.
- `xtask/`: workspace verification tools (standards, packaging).
- `database/migrations/`: durable Postgres migrations.

## 5. API, SDK, And Data Ownership

- API authority: `sdkwork-specs/API_SPEC.md`, `sdkwork-specs/DISCOVERY_SPEC.md`, `sdkwork-specs/RPC_SPEC.md`.
- SDK family authority: `sdks/sdkwork-discovery-rpc-sdk/` exposes `sdkwork-discovery-rpc-sdk-rust` and `sdkwork-discovery-rpc-sdk-typescript` bindings documented under `sdks/sdkwork-discovery-rpc-sdk/README.md`.
- Data ownership: Postgres schema owned by `sdkwork-discovery-storage-postgres`; migrations live under `database/migrations/postgres/`.

## 6. Security, Privacy, And Observability

Security, privacy, and observability requirements follow `sdkwork-specs/SECURITY_SPEC.md`, `sdkwork-specs/PRIVACY_SPEC.md`, and `sdkwork-specs/OBSERVABILITY_SPEC.md`. Operationally the service host exposes `/healthz`, `/readyz`, `/livez`, and `/metrics` per `sdkwork-specs/HEALTH_CHECK_SPEC.md`, and emits Prometheus metrics through the `sdkwork-web-bootstrap` recorder.

## 7. Deployment And Runtime Topology

Deployment and runtime topology follow `sdkwork-specs/DEPLOYMENT_SPEC.md` and the repository runbooks under `docs/runbooks/`. The production profile uses `etc/discovery.production.example.toml` and the `package-server.mjs` packaging flow.

## 8. Architecture Decision Index

- ADR records live under `docs/architecture/decisions/` and link back to `ARCHITECTURE_DECISION_SPEC.md`.

## 9. Verification

- `cargo test --workspace` covers unit, contract, and smoke tests.
- `cargo fmt --check` and `cargo clippy --workspace --all-targets` gate style and lint.
- `pnpm verify:docs` and `pnpm test:docs-canon` gate Canon documentation contracts.
- `xtask/sdkwork-discovery-standards` enforces module boundaries and production ops artifacts.
