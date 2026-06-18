# Adoption Guide

This guide migrates an existing SDKWork application from ad-hoc gateway/dev scripts to `@sdkwork/app-topology`.

## Phase 1 — Declare the contract

1. Run `init-app` from the framework repo CLI.
2. Replace generic template keys with app-specific env keys (Drive uses `VITE_DRIVE_PC_*`).
3. Move hardcoded URLs from scripts into the four profile env files.
4. Add `docs/topology-standard.md` command matrix.

## Phase 2 — Add the library adapter

Create `scripts/lib/<app>-topology.mjs`:

```javascript
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createTopologyRuntime, loadTopologySpec } from '@sdkwork/app-topology';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const spec = loadTopologySpec(path.join(repoRoot, 'specs/topology.spec.json'));
export const appTopology = createTopologyRuntime(spec, repoRoot);
```

Re-export only what local scripts need.

## Phase 3 — Rename scripts and commands

| Old pattern | New pattern |
| --- | --- |
| `run-<app>-pc-dev.mjs` | `<app>-dev.mjs` |
| `run-<app>-pc-build.mjs` | `<app>-build.mjs` |
| `--gateway-mode` | `--topology` |
| `pnpm dev` | `pnpm <app>:dev` |
| `pnpm desktop:build` | `pnpm <app>:build` |

Delete legacy aliases instead of keeping compatibility shims.

## Phase 4 — Runtime config

1. Add `topology` to runtime config model.
2. Read client topology env key from the spec (`envKeys.clientTopology`).
3. Align default public URLs with cloud production profile env.

## Phase 5 — CI and packaging

1. Move gateway package targets into `specs/topology.spec.json`.
2. Update `sdkwork.workflow.json` profiles to `standalone` and `cloud-config`.
3. Replace cloud gateway binary packaging with config bundle scripts in the app repo.

## Phase 6 — Verification

Application repo:

```bash
pnpm <app>:dev --help
pnpm gateway:matrix
cargo test / node test for script contract checks
```

Framework repo:

```bash
node ../sdkwork-app-topology/scripts/sdkwork-topology.mjs validate --root . --spec specs/topology.spec.json
```

## Reference

See `../sdkwork-drive` after migration:

- `specs/drive-topology.spec.json`
- `scripts/lib/drive-topology.mjs`
- `scripts/drive-dev.mjs`
- `docs/drive-topology-standard.md`
