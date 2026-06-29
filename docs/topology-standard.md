# SDKWork Discovery Topology

Archetype: `application-http-gateway` (`specs/topology.spec.json`, `schemaVersion: 2`).

Platform standard: `../sdkwork-specs/APP_RUNTIME_TOPOLOGY_ADOPTION.md`

Discovery is a unified-process gRPC control plane. Clients connect to **application.public-ingress** for registry and config RPC. Operator/admin RPC terminates on **operations.control-ingress**.

## Default dev profile

`standalone.unified-process.development` — start the service host with:

```bash
pnpm dev
```

Cloud development profile:

```bash
pnpm dev:cloud
```

## Surfaces

| Surface id | Plane | Service |
| --- | --- | --- |
| `application.public-ingress` | application | `sdkwork-discovery-service-host` (registry/config gRPC) |
| `operations.control-ingress` | operations | `sdkwork-discovery-service-host` (admin gRPC) |

Profile env files under `configs/topology/` declare `SDKWORK_DISCOVERY_APPLICATION_PUBLIC_*` and `SDKWORK_DISCOVERY_OPERATIONS_CONTROL_*` bind and gRPC URL keys for each deployment profile.

Loader: `scripts/lib/discovery-topology.mjs` → `@sdkwork/app-topology`.

Validate:

```bash
pnpm topology:validate
pnpm topology:matrix
pnpm test:topology-baggage
```

Dry-run orchestration:

```bash
node scripts/discovery-dev.mjs --dry-run
```
