# sdkwork-discovery-rpc-sdk

SDKWork RPC SDK family for the Discovery control plane.

## Source Contracts

- Proto source root: `../../proto`
- RPC manifest: `sdkwork-discovery-rpc.manifest.json`

Generated protobuf output must not be hand-edited. Missing RPC client behavior is fixed by updating proto source and this manifest, then regenerating through the SDKWork RPC generation workflow.

## Supported First Slice

- Service instance registration, renewal, deregistration, status report, exact instance retrieval, and discovery.
- Effective config retrieval through `DiscoveryConfigService`.
- Config watch streams through `DiscoveryConfigService.WatchConfig`.
- Service registry watch streams through `DiscoveryWatchService.WatchService`.
- Backend config draft, publish, and service listing operations.

## Client Requirements

Generated or composed clients must support:

- endpoint configuration
- TLS/mTLS configuration with server CA trust and client certificate identity when required by deployment policy
- service identity or backend operator metadata provider
- standard SDKWork dual-token metadata providers:
  - `authorization: Bearer <auth-token>`
  - `access-token: <access-token>`
- authenticated caller context metadata:
  - `x-sdkwork-subject-id: <service-or-operator-id>`
  - `x-sdkwork-registry-permissions: read,write` for registry operations when authorized by the host identity layer
  - `x-sdkwork-config-permissions: read,publish,rollback` for config operations when authorized by the host identity layer
- request id and trace metadata:
  - `x-request-id: <request-id>` when supplied by a trusted gateway or runtime
  - `traceparent: <w3c-trace-context>` when distributed tracing is enabled
- required write idempotency metadata:
  - `idempotency-key: <stable-operation-key>`
  - `x-request-hash: <canonical-request-hash>`
- default deadline
- idempotency-aware retry policy

Protected Discovery RPC methods reject requests that do not include `authorization`,
`access-token`, and a non-empty `x-sdkwork-subject-id` metadata value. The metadata provider may
resolve service-identity or backend-operator tokens and subject identity from the host runtime, IAM
integration, or deployment identity layer, but examples must never hard-code live token values.
If `x-request-id` is missing or blank, the server returns a generated request id in response
metadata so successful unary and stream responses remain traceable.

Backend config write methods declared with `idempotency: "required"` in the RPC manifest reject
requests that do not include both `idempotency-key` and `x-request-hash`. Replayed requests with
the same operation id, idempotency key, and request hash return the original draft or release
result; the same idempotency key with a different request hash is rejected.

## Watch Semantics

`DiscoveryWatchService.WatchService` and `DiscoveryConfigService.WatchConfig` first stream revision-ordered events already persisted in the Discovery watch store from the supplied `from_revision`, then continue with live events published by the connected server process. Clients should keep the latest received revision and reconnect from that revision after cancellation, deadline expiry, transient transport errors, or server failover.

`DiscoveryWatchService.WatchService` requires `x-sdkwork-registry-permissions: read`. `DiscoveryConfigService.WatchConfig` requires `x-sdkwork-config-permissions: read`. Both watch methods reject missing or insufficient read permission before opening a stream or dispatching to storage.

`DiscoveryWatchService.WatchService` registry mutation events include a `ServiceInstance` payload so clients can update local discovery caches from the stream. Registered, updated, renewed, and status events are enriched from current registry state. Deregistered events, or events whose current row is no longer available, return an identity tombstone with namespace, environment, service name, instance id, event revision, and `INSTANCE_STATUS_NOT_SERVING`; endpoint, protocol, version, region, zone, and lease id are empty. This slice does not persist event-time instance snapshots, so strict historical snapshot replay requires a future storage schema review.

Idle Watch streams receive `heartbeat` events at the server-configured interval. Heartbeats are transport keepalives and carry the last delivered revision; they do not represent registry or config mutations. Clients should ignore them for cache mutation and still retain the revision for reconnect.

Servers may cap concurrent Watch streams. Clients that receive gRPC `ResourceExhausted` should back off, retry on another allowed stream endpoint, or fall back to polling plus later reconnect from the last observed revision.

Live fanout is process-local in this slice. Durable replay remains the correctness boundary, so horizontally scaled deployments should use sticky stream routing or reconnect-based catch-up until a provider-specific distributed fanout adapter is introduced.

## mTLS

Production service-to-service deployments should use mTLS. Server deployments configure certificate, key, and optional client CA files through private runtime config or `SDKWORK_DISCOVERY_RPC_*` process env keys. SDK examples must use metadata providers and TLS identity providers; do not hard-code tokens or PEM contents in generated examples.
