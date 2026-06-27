# SDKWork Discovery PRD

Status: draft
Owner: SDKWork maintainers
Application: sdkwork-discovery
Updated: 2026-06-26
Specs: REQUIREMENTS_SPEC.md, DOCUMENTATION_SPEC.md, DISCOVERY_SPEC.md

## Document Map

- Add `PRD-<topic>.md` shards in this directory when the PRD grows beyond one reviewable screen.

## 1. Background And Problem

Distributed SDKWork applications need a single source of truth for live service topology, configuration, and security policy. Operators need to discover healthy service instances, watchers need to react to registry and config changes in near-real time, and platform teams need durable records for compliance and audit. Without a unified control plane, ad-hoc service discovery, out-of-band configuration distribution, and shadow security policy create drift, downtime, and audit gaps.

## 2. Target Users

- **Service operators** register and deregister service instances and read live topology for routing and incident response.
- **Application integrators** consume the Service Registry and Config Registry through generated RPC SDKs.
- **Platform security engineers** audit identity tombstones, dual-token authorization, and mutation events.
- **Site reliability engineers** use the health probes and metrics emitted by the service host to drive alerting and rollbacks.

## 3. Goals And Non-Goals

### Goals

- Provide a canonical **Service Registry** for registering, deregistering, and watching service instances across deployments.
- Provide a canonical **Config Registry** for drafting, publishing, and watching scoped configuration documents.
- Expose dual-plane RPC surfaces: a public-facing ingress for service operators and an operations control ingress for admin/mutation actions.
- Emit durable audit events for every registry and config mutation.

### Non-Goals

- This PRD does not authorize API paths, table names, or implementation steps. Those live in `docs/architecture/tech/TECH_ARCHITECTURE.md` and governing `sdkwork-specs`.
- This PRD does not replace per-application product PRDs for consuming applications.

## 4. Scope

In scope: service instance lifecycle, registry watch streams, configuration drafts and publications, dual-plane RPC ingress, security authorization, health probes, and observability.

Out of scope: end-user browser experiences, per-application business logic, and any capability owned by other SDKWork applications.

## 5. User Scenarios

- A service operator registers an instance and a watcher receives the new `ServiceInstance` payload within the configured watch heartbeat.
- A platform engineer drafts a configuration change, publishes it, and downstream consumers receive the effective config through `WatchConfig`.
- A security engineer reviews identity tombstones (`INSTANCE_STATUS_NOT_SERVING`) to confirm deregistration.
- An SRE probes `/healthz` and `/readyz` during rollout to gate traffic promotion.

## 6. Success Metrics

- Registry mutation events reach active watchers within the configured watch heartbeat interval.
- Health probes reflect dependency readiness within the configured sync interval.
- Dual-token authorization rejects unauthorized registry and config mutations.
- Audit events persist for every mutation.

## 7. Phases

- Phase 0: control plane skeleton, dual-plane RPC ingress, memory storage adapter.
- Phase 1: durable Postgres and Redis adapters, dual-token authorization, health probes.
- Phase 2: production hardening (mTLS, rate limiting, watch governance), SDK language bindings, runbooks.

## 8. Linked Requirements

- Engineering requirements live in `docs/product/requirements/` with `REQ-*` ids and link back to `REQUIREMENTS_SPEC.md`.

## 9. Open Questions

- Whether to expose etcd and Consul adapters in the commercial release, or stay fail-fast until implemented.
- Whether to broaden the Service Registry payload schema to carry per-instance capabilities beyond status and metadata.
