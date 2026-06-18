# SDKWork App Topology Standard

Version: 2.0 (platform); framework library still supports v1 Drive specs  
Scope: cross-application deployment topology for SDKWork apps with PC/H5/desktop surfaces

**Platform naming authority:** `../../sdkwork-specs/APP_RUNTIME_TOPOLOGY_NAMING.md`  
**Platform connectivity standard:** `../../sdkwork-specs/APP_RUNTIME_TOPOLOGY_SPEC.md`

This document describes the **framework package** layout. Vocabulary for new applications uses v2 (`hosting`, `serviceLayout`, `connectivityPlane`). Drive continues on v1 (`topology`, `profile`) until migrated.

## 1. Purpose

SDKWork applications frequently ship multiple deployment shapes:

- local standalone loop (embedded IAM + app APIs via standalone gateway)
- cloud unified API surface (`sdkwork-api-gateway`)

This standard defines one axis — **`topology`** — and one profile system — **`development` / `production`** — so developers, scripts, CI, runtime config, and documentation use the same words everywhere.

## 2. Required Vocabulary

| Key | Allowed values |
| --- | --- |
| topology | `standalone`, `cloud` |
| profile | `development`, `production` |

Forbidden public synonyms:

- `gateway-mode`
- `standalone-gateway` as a topology name (allowed only as packaging profile slug)
- duplicated per-script URL blocks

## 3. Required Files In Each Application

```text
specs/topology.spec.json
configs/topology/standalone.development.env
configs/topology/standalone.production.env
configs/topology/cloud.development.env
configs/topology/cloud.production.env
docs/topology-standard.md
scripts/lib/<app>-topology.mjs
```

Optional but recommended:

- `.env.postgres.example` when IAM login uses PostgreSQL
- gateway TOML configs referenced by the topology spec

## 4. Topology Spec

Each application commits `specs/topology.spec.json` with:

```json
{
  "schemaVersion": 1,
  "kind": "sdkwork.app.topology",
  "appId": "sdkwork-drive",
  "vocabulary": { "topology": { "allowed": ["standalone", "cloud"] }, "profile": { "allowed": ["development", "production"] } },
  "defaults": { "developmentTopology": "standalone", "buildTopology": "cloud" },
  "envKeys": { "topology": "SDKWORK_DRIVE_TOPOLOGY", "clientTopology": "VITE_DRIVE_PC_TOPOLOGY" },
  "packaging": { "targets": [] }
}
```

Validate with:

```bash
node ../sdkwork-app-topology/scripts/sdkwork-topology.mjs validate --root .
```

JSON Schema: `../sdkwork-app-topology/specs/topology.schema.json`

## 5. Profile Env Rules

Profiles live under `configs/topology/{topology}.{profile}.env`.

Rules:

1. Scripts load profiles through `@sdkwork/app-topology`; they do not hardcode URLs.
2. Profile files declare both server-side keys (`SDKWORK_*`) and client-side keys (`VITE_*`, `DART_*`, etc.).
3. CLI `--topology` selects the profile family; `--profile` is implied by the command (`development` for dev scripts, `production` for build scripts).
4. Autostart is controlled by `<APP>_GATEWAY_AUTOSTART`, default `true`.

## 6. Command Naming Standard

Application root `package.json` scripts SHOULD use:

| Script | Meaning |
| --- | --- |
| `<app>:dev` | browser dev, default topology |
| `<app>:dev:cloud` | browser dev, cloud topology |
| `<app>:dev:desktop` | desktop dev, default topology |
| `<app>:build` | release build, default topology (usually cloud) |
| `<app>:build:standalone` | release build, standalone topology |
| `gateway:standalone:run` | run standalone gateway |
| `gateway:standalone:pack` | package standalone gateway binary |
| `gateway:cloud:bundle` | bundle cloud gateway configs only |

Examples for Drive are implemented in `../sdkwork-drive/package.json`.

## 7. Gateway Ownership

| Topology | Gateway binary | Config owner | Binary owner |
| --- | --- | --- | --- |
| standalone | app standalone gateway crate | application repo | application repo |
| cloud | `sdkwork-api-gateway` | application repo (route/config bundle) | `sdkwork-api-gateway` repo |

## 8. Runtime Config Integration

Client runtime config MUST expose topology explicitly:

- PC/React: `VITE_<APP>_TOPOLOGY`
- runtime factory reads topology before inferring default API URLs

Inference rule:

- `local` / `test` deployment modes → standalone localhost defaults
- other deployment modes → cloud public defaults unless profile env overrides

## 9. CI Matrix Ownership

Packaging targets belong in `specs/topology.spec.json` → `packaging.targets`.

`sdkwork.workflow.json` in the application repo MUST reference the same target ids and profiles.

Use `@sdkwork/app-topology` to print the matrix and avoid manual duplication.

## 10. Framework Repository

Implementation and CLI live in `../sdkwork-app-topology`.

Applications depend on it via sibling path:

```json
"@sdkwork/app-topology": "file:../sdkwork-app-topology"
```

Published registry consumption is supported later; sibling path is the SDKWork workspace default.
