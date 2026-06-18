# SDKWork App Topology Framework

Cross-application standard for SDKWork deployment topology, dev profiles, gateway wiring, IAM database bootstrap, and packaging matrices.

This repository is the reusable counterpart to application-local orchestration such as `sdkwork-drive/scripts/drive-dev.mjs`. Applications stay thin: they declare a topology spec, profile env files, and app-specific process wiring. The framework owns the vocabulary, loaders, validators, and shared helpers.

Pair this framework with:

- `../sdkwork-specs/APP_RUNTIME_TOPOLOGY_SPEC.md` — platform connectivity standard (v2)
- `../sdkwork-specs/APP_RUNTIME_TOPOLOGY_NAMING.md` — **naming authority** (hosting, serviceLayout, env keys)
- `../sdkwork-specs/APP_RUNTIME_TOPOLOGY_ARCHETYPES.md` — reusable archetype catalog
- `../sdkwork-github-workflow` for CI packaging matrices and lifecycle execution
- `sdkwork-api-gateway` for cloud unified API routing
- Application-owned standalone gateway crates (for example `sdkwork-drive-standalone-gateway`)

## Why This Exists

Before extraction, each product repository duplicated:

- `topology` vs `gateway-mode` vocabulary drift
- profile env loading and IAM PostgreSQL mapping
- gateway bind/base URL resolution
- packaging target matrices maintained separately from CI
- ad-hoc script sprawl (`dev`, `tauri:dev`, `desktop:build`, ...)

`@sdkwork/app-topology` centralizes the **standard** so every SDKWork app (`sdkwork-drive`, `sdkwork-commerce`, `sdkwork-im`, ...) adopts the same model with only app-specific values in `specs/topology.spec.json`.

## Core Vocabulary

### v1 (Drive and existing adopters)

| Term | Values | Meaning |
| --- | --- | --- |
| `topology` | `standalone`, `cloud` | How the client and dev orchestrator reach APIs |
| `profile` | `development`, `production` | Which env file under `configs/topology/` is loaded |

### v2 (greenfield multi-plane apps — IM, AIoT)

See `APP_RUNTIME_TOPOLOGY_NAMING.md`. Summary:

| Term | Values | Spoken example |
| --- | --- | --- |
| `hosting` | `self-hosted`, `cloud-hosted` | "self-hosted split dev" |
| `serviceLayout` | `unified-process`, `split-services` | split-services = default IM dev |
| `environment` | `development`, `production` | |
| Profile id | `{hosting}.{serviceLayout}.{environment}` | `self-hosted.split-services.development` |

JSON Schema: `specs/topology.schema.v2.json` for `schemaVersion: 2` specs.

Defaults (v1):

- development → `standalone`
- desktop/build release → `cloud`

Do not introduce alternate names such as `gateway-mode`, `standalone-gateway profile`, or duplicated URL blocks inside scripts.

## Repository Layout

```text
sdkwork-app-topology/
  tools/topology/lib/          # importable zero-dependency Node library
  scripts/sdkwork-topology.mjs # CLI: init-app, validate, scaffold-profiles, print-matrix
  specs/topology.schema.json     # JSON Schema v1 (Drive-class)
  specs/topology.schema.v2.json  # JSON Schema v2 (multi-plane apps)
  configs/templates/           # starter profile env templates
  docs/                        # standard and adoption docs
  examples/sdkwork-drive/      # reference spec for SDKWork Drive
  tests/                       # Node test coverage
```

## Quick Start For A New Application

From the target application repository root:

```bash
node ../sdkwork-app-topology/scripts/sdkwork-topology.mjs init-app \
  --app-id sdkwork-commerce \
  --app-name "SDKWork Commerce"
```

This writes:

- `specs/topology.spec.json`
- `configs/topology/{standalone,cloud}.{development,production}.env`
- `docs/topology-standard.md` (app-local pointer)

Then add a file dependency in the application root `package.json`:

```json
{
  "dependencies": {
    "@sdkwork/app-topology": "file:../sdkwork-app-topology"
  }
}
```

Create a thin adapter (recommended path: `scripts/lib/app-topology.mjs`):

```javascript
import path from 'node:path';
import { createTopologyRuntime, loadTopologySpec } from '@sdkwork/app-topology';

const repoRoot = path.resolve(import.meta.dirname, '..', '..');
const spec = loadTopologySpec(path.join(repoRoot, 'specs/topology.spec.json'));
export const topology = createTopologyRuntime(spec, repoRoot);
```

Application scripts (`app-dev.mjs`, `app-build.mjs`, gateway pack scripts) import `topology` instead of copying helpers.

## Application Contract

Every adopting application MUST commit:

| Path | Purpose |
| --- | --- |
| `specs/topology.spec.json` | Machine-readable topology contract (`kind: sdkwork.app.topology`) |
| `configs/topology/*.env` | Profile values only; no duplicate hardcoding in scripts |
| `docs/topology-standard.md` | App-local command matrix and URLs |
| `scripts/lib/*-topology.mjs` | Thin adapter over `@sdkwork/app-topology` |
| `.env.postgres.example` | IAM/database bootstrap template when IAM login is used |

Application scripts MUST:

- accept `--topology standalone|cloud` (never `--gateway-mode`)
- load profiles through the framework runtime
- keep app-specific process spawning local (Vite, Tauri, API servers, gateway crates)

Application scripts MUST NOT:

- duplicate IAM URL builders
- embed production API URLs outside profile env files
- cross-build `sdkwork-api-gateway` (cloud binary belongs to `sdkwork-api-gateway`)

## CLI

```bash
node scripts/sdkwork-topology.mjs validate --root ../sdkwork-drive --spec specs/drive-topology.spec.json
node scripts/sdkwork-topology.mjs print-matrix --root ../sdkwork-drive --spec specs/drive-topology.spec.json
node scripts/sdkwork-topology.mjs scaffold-profiles --root ../my-app
```

## Library API

```javascript
import {
  loadTopologySpec,
  createTopologyRuntime,
} from '@sdkwork/app-topology';

const spec = loadTopologySpec('specs/topology.spec.json');
const runtime = createTopologyRuntime(spec, repoRoot);

runtime.loadTopologyProfile('standalone', 'development');
runtime.applyTopologyEnv('cloud', [process.env]);
runtime.resolveGatewayBind(env, 'standalone');
runtime.resolveIamDevEnv(process.env);
runtime.listPackageTargetsByProfile('standalone');
```

## CI Integration

Declare packaging targets in `specs/topology.spec.json` under `packaging.targets` and keep `sdkwork.workflow.json` in sync.

Recommended profiles:

| Profile | Artifact owner |
| --- | --- |
| `standalone` | Application repository (standalone gateway binary) |
| `cloud-config` | Application repository (gateway route/config bundle only) |

Cloud gateway binaries are built and released from `sdkwork-api-gateway`.

Use:

```bash
node scripts/sdkwork-topology.mjs print-matrix --root ../sdkwork-drive --spec specs/drive-topology.spec.json
```

to inspect the matrix an app's CI should expose.

## Reference Adoption

`../sdkwork-drive` is the first consumer:

- spec: `specs/drive-topology.spec.json`
- adapter: `scripts/lib/drive-topology.mjs`
- docs: `docs/drive-topology-standard.md`

## Related Standards

- `../sdkwork-specs/NAMING_SPEC.md`
- `../sdkwork-specs/SDKWORK_WORKSPACE_SPEC.md`
- `../sdkwork-github-workflow/README.md`
- `docs/topology-standard.md`
- `docs/adoption-guide.md`

## Verification

```bash
pnpm test
pnpm run validate:example
```
# sdkwork-app-topology
