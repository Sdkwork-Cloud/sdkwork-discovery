# ADR-20260609: Rust gRPC Control Plane for SDKWork Discovery

Status: accepted
Date: 2026-06-09
Updated: 2026-06-23
Specs: ARCHITECTURE_DECISION_SPEC.md, DISCOVERY_SPEC.md, RUST_RPC_SPEC.md

## Context

SDKWork needs a platform-standard service registry and versioned runtime configuration control plane. Parallel ad hoc discovery mechanisms create security, migration, and operational drift across deployments.

## Decision

Implement `sdkwork-discovery` as a Rust workspace with:

- Contract-first gRPC under `proto/` and `sdks/sdkwork-discovery-rpc-sdk/`
- Thin RPC adapters in `sdkwork-discovery-rpc` and domain logic in `sdkwork-discovery-core`
- Async storage ports with PostgreSQL as the production durable backend
- Runnable host at `services/sdkwork-discovery-service-host`
- Dual ingress surfaces (application public + operations control) per APP_RUNTIME_TOPOLOGY_SPEC.md

## Consequences

**Positive**

- Aligns with SDKWork RPC framework resolver integration
- Enables typed runtime config, production policy validation, and generated multi-language SDKs
- Supports revision-ordered watch with durable replay before live fanout

**Negative**

- etcd/Consul adapters deferred; configuration shapes exist but bootstrap fails fast until implemented
- Horizontal watch fanout is process-local until distributed adapters land
- No browser admin UI in the first platform slice

## Alternatives Considered

- Third-party registry compatibility (Nacos, Consul KV, etcd APIs): rejected to avoid permanent compatibility debt
- HTTP-only discovery API: rejected; internal orchestration uses gRPC per RPC_SPEC.md

## Evidence

- Design: [2026-06-09-sdkwork-discovery-control-plane-design.md](../../superpowers/specs/2026-06-09-sdkwork-discovery-control-plane-design.md)
- Canon architecture: [TECH_ARCHITECTURE.md](../TECH_ARCHITECTURE.md)

## Verification

```bash
pnpm run verify
cargo test -p sdkwork-discovery-standards
```
