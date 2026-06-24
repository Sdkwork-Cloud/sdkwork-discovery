# Developer Guide

Local setup, verification, and contribution workflow for `sdkwork-discovery`.

## Prerequisites

- Rust stable (rustfmt + clippy)
- Node.js 22 and pnpm 10.33.0
- Sibling workspace checkouts (local dev):
  - `../sdkwork-specs`
  - `../sdkwork-database`
  - `../sdkwork-app-topology`

## Quick Start

```bash
cargo test --workspace
pnpm run verify
pnpm run discovery:dev
```

On Windows, prefer `pnpm.cmd` if PowerShell blocks `pnpm.ps1`.

Topology-aware dev loads profiles from `configs/topology/` via `@sdkwork/app-topology`. See [topology standard](../../topology-standard.md).

## Run The Service Host

```bash
cargo run -p sdkwork-discovery-service-host --offline
```

Point at a config file with `SDKWORK_DISCOVERY_CONFIG_FILE`. Defaults bind application public ingress to `127.0.0.1:19090` and operations control to `127.0.0.1:19091`.

Templates:

- Development: `etc/discovery.example.toml`
- Production shape: `etc/discovery.production.example.toml`

## Database Workflow

Canonical migrations live under `database/migrations/{postgres,sqlite}/`.

```bash
pnpm run db:validate
pnpm run db:migrate
pnpm run db:status
```

Do not hand-edit crate-local SQL under deprecated `crates/*/migrations/` paths.

## Change Boundaries

Read before editing public behavior:

1. [AGENTS.md](../../../AGENTS.md)
2. [specs/component.spec.json](../../../specs/component.spec.json)
3. [PRD.md](../../product/PRD.md) and [TECH_ARCHITECTURE.md](../../architecture/TECH_ARCHITECTURE.md)

Rules:

- Do not hand-edit generated protobuf or SDK output under `sdks/`
- Keep RPC adapters thin; domain logic belongs in `sdkwork-discovery-core`
- Rust `src/lib.rs` files are module assembly boundaries only

## RPC And SDK Changes

1. Edit contracts in `proto/`
2. Regenerate SDK artifacts through the SDKWork RPC generation workflow
3. Run `pnpm run verify:rpc-proto` and SDK checks (`verify:sdk-rust`, `verify:sdk-typescript`)

Handwritten SDK helpers live beside generated code: `deadline`, `idempotency`, `pagination`, `tracing`.

## Verification Commands

| Command | Purpose |
| --- | --- |
| `pnpm run verify` | Full repository gate (fmt, clippy, tests, topology, package contract, docs) |
| `cargo test -p sdkwork-discovery-standards` | Module boundaries and production ops artifacts |
| `pnpm run package:server:validate` | Release archive contract (after `package:server`) |
| `node ../sdkwork-specs/tools/check-repository-docs-standard.mjs --root .` | Documentation canon |

## Related Specs

`DOCUMENTATION_SPEC.md`, `RUST_CODE_SPEC.md`, `RUST_RPC_SPEC.md`, `TEST_SPEC.md`
