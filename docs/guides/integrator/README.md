# Integrator Guide

How downstream services consume SDKWork Discovery through generated RPC SDKs.

## SDK Family

- Manifest: `sdks/sdkwork-discovery-rpc-sdk/sdkwork-discovery-rpc.manifest.json`
- Rust workspace: `sdks/sdkwork-discovery-rpc-sdk/sdkwork-discovery-rpc-sdk-rust`
- TypeScript package: `sdks/sdkwork-discovery-rpc-sdk/sdkwork-discovery-rpc-sdk-typescript`
- Proto source: `proto/`

Read the [SDK README](../../../sdks/sdkwork-discovery-rpc-sdk/README.md) for metadata, watch semantics, and idempotency requirements.

## Integration Model

```text
Your RPC server process
  -> register / renew / deregister via RegistryService
  -> read effective config via DiscoveryConfigService
  -> subscribe via DiscoveryWatchService / WatchConfig

Your RPC client (via sdkwork-rpc-framework resolver)
  -> discover healthy gRPC instances
  -> apply config releases from watch or polling
```

Browser, H5, and most mobile UI clients must not call discovery directly unless a governed gRPC-Web ADR exists.

## Authentication

Production deployments require TLS or mTLS and signed service tokens. Each protected RPC call needs:

- `authorization: Bearer <sdkwork-discovery-v1 token>`
- `access-token: <bound access token>`
- `x-sdkwork-subject-id: <service-or-operator-id>`
- Permission metadata (`x-sdkwork-registry-permissions`, `x-sdkwork-config-permissions`) as required by the method

Local loopback development may enable `allow_unsigned_local_context` only when runtime policy explicitly allows it.

## Client Metadata

| Header | When |
| --- | --- |
| `x-request-id` | Supply from gateway when available; server generates `req_*` when missing |
| `traceparent` | W3C trace context when distributed tracing is enabled |
| `idempotency-key` + `x-request-hash` | Required for manifest-declared idempotent config writes |

Use SDK helpers for deadline, pagination, idempotency, and traceparent construction instead of raw string assembly.

## Registry Identity

```text
namespace + environment + service_name + instance_id
```

Register with endpoint (`grpcs://...`), protocol `grpc`, version, lease TTL, and structured metadata (`rpc_surface`, `sdk_family`, `domain`).

## Watch And Reconnect

1. Open watch from last known `from_revision`
2. Apply durable replay events, then live events
3. On disconnect or `ResourceExhausted`, backoff and reconnect from last revision
4. Treat heartbeat events as keepalives only

## Production Checklist

- [ ] PostgreSQL migrations applied (`database/migrations/postgres/`)
- [ ] TLS/mTLS configured via secret files, not inline PEM
- [ ] Service-token HMAC secret file ≥ 32 bytes
- [ ] `allow_unsigned_local_context = false`
- [ ] Reflection disabled or access-controlled at deployment layer
- [ ] Metrics scrape bind configured (`SDKWORK_DISCOVERY_METRICS_BIND`)

Operator procedures: [production deployment runbook](../../runbooks/RUNBOOK-production-server-deployment.md).

## Verification For Integrators

```bash
pnpm run verify:sdk-rust
pnpm run verify:sdk-typescript
pnpm run test:sdk-typescript-helpers
```

## Related Specs

`DISCOVERY_SPEC.md`, `RPC_SDK_WORKSPACE_SPEC.md`, `SECURITY_SPEC.md`, `RPC_RESILIENCE_SPEC.md`
