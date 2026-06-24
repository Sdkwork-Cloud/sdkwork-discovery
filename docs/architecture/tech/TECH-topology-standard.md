> Migrated from `docs/topology-standard.md` on 2026-06-24.
> Owner: SDKWork maintainers

Archetype: `application-http-gateway` (`specs/topology.spec.json`, `schemaVersion: 2`).

Platform standard: `../sdkwork-specs/APP_RUNTIME_TOPOLOGY_ADOPTION.md`

Discovery is a unified-process gRPC control plane. Clients connect to **application.public-ingress** for registry and config RPC. Operator/admin RPC terminates on **operations.control-ingress**.

## Default dev profile

`standalone.unified-process.development` — start the service host with:

```bash
pnpm discovery:dev
```

Cloud development profile:

```bash
pnpm discovery:dev:cloud
```

## Surfaces

| Surface id | Plane | Service |
| --- | --- | --- |
| `application.public-ingress` | application | `sdkwork-discovery-service-host` (registry/config gRPC) |
| `operations.control-ingress` | operations | `sdkwork-discovery-service-host` (admin gRPC) |

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

