# scripts

Topology-aware dev orchestration:

- `discovery-dev.mjs` — load profile env, resolve orchestration from spec, start service host
- `lib/discovery-topology.mjs` — adapter over `@sdkwork/app-topology`

Commands: `pnpm discovery:dev`, `pnpm topology:validate`, `pnpm topology:matrix`.

See `docs/topology-standard.md`.
