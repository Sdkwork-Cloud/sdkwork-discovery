# Repository Guidelines

<!-- SDKWORK-AGENTS-GENERATED: v1 -->

## SDKWORK Soul

Read `../sdkwork-specs/SOUL.md` before executing tasks in this root. Follow specs before memory, dictionary before context, stop on ambiguity, and evidence before completion.

## SDKWORK Standards

Canonical SDKWORK specs path from this root:

- `../sdkwork-specs/README.md`
- `../sdkwork-specs/SOUL.md`
- `../sdkwork-specs/AGENTS_SPEC.md`
- `../sdkwork-specs/CODE_STYLE_SPEC.md`
- `../sdkwork-specs/NAMING_SPEC.md`

Do not copy root standard text into this repository. If these relative paths do not resolve, stop and report the broken workspace layout.

## Application Identity

Read the root `sdkwork.app.config.json` before changing application behavior, runtime config, SDK wiring, release metadata, or app-owned capabilities.

## Local Dictionary Structure

- `AGENTS.md`: local agent entrypoint and relative SDKWORK spec index.
- `CLAUDE.md`: Claude Code compatibility shim that points to `AGENTS.md`.
- `GEMINI.md`: Gemini CLI compatibility shim that points to `AGENTS.md`.
- `CODEX.md`: Codex compatibility shim that points to `AGENTS.md`.
- `sdkwork.app.config.json`: application identity and release metadata.
- `.sdkwork/`: reserved local dictionary folder for local skills, plugins, manifests, or AI workspace metadata.
- `specs/`: local application/component contracts and narrowing rules.
- `proto/`: source-of-truth RPC protobuf contracts.
- `sdks/`: RPC SDK family manifests and generated SDK artifacts.
- `crates/`, `services/`, `etc/`, `docs/`: Rust crates, runnable services, runtime config templates, and design evidence.

## Spec Resolution Order

1. Read this `AGENTS.md` and any nearer component-level `AGENTS.md`.
2. Read `sdkwork.app.config.json` when changing app behavior, runtime config, SDK wiring, release metadata, or app-owned capabilities.
3. Read local `specs/README.md` and `specs/component.spec.json` when changing public exports, runtime entrypoints, SDK clients, generated artifacts, config keys, or verification commands.
4. Read local `.sdkwork/README.md`, `.sdkwork/skills/`, and `.sdkwork/plugins/` when relevant.
5. Read `../sdkwork-specs/README.md` and the task-specific root specs.
6. Inspect implementation files only after relevant dictionary entries are clear.

## Required Specs By Task Type

- Agent/workflow changes: `../sdkwork-specs/SOUL.md`, `../sdkwork-specs/AGENTS_SPEC.md`, `../sdkwork-specs/SDKWORK_WORKSPACE_SPEC.md`.
- Any code change: `../sdkwork-specs/CODE_STYLE_SPEC.md`, `../sdkwork-specs/NAMING_SPEC.md`, plus only the touched language/framework spec.
- Rust code: `../sdkwork-specs/RUST_CODE_SPEC.md`.
- Rust RPC or proto work: `../sdkwork-specs/RPC_SPEC.md`, `../sdkwork-specs/RUST_RPC_SPEC.md`, `../sdkwork-specs/RPC_SDK_WORKSPACE_SPEC.md`.
- Config/runtime/env changes: `../sdkwork-specs/CONFIG_SPEC.md`, `../sdkwork-specs/ENVIRONMENT_SPEC.md`, `../sdkwork-specs/RUNTIME_DIRECTORY_SPEC.md`.
- Persistence/cache changes: `../sdkwork-specs/DATABASE_SPEC.md`, `../sdkwork-specs/CACHE_SPEC.md`.
- Security/observability/testing changes: `../sdkwork-specs/SECURITY_SPEC.md`, `../sdkwork-specs/OBSERVABILITY_SPEC.md`, `../sdkwork-specs/TEST_SPEC.md`.

## Code Style Rules

Rust `src/lib.rs` files are module assembly and re-export boundaries only. Business logic, persistence, providers, DTOs, fixtures, and long services belong in focused modules.

RPC adapters must remain thin: metadata/context mapping, request validation, runtime dispatch, response mapping, error mapping, tracing. Domain logic belongs in core services.

Generated protobuf and SDK output must not be hand-edited. Fix proto contracts, RPC manifests, generator inputs, or approved handwritten facades, then regenerate.

## Build, Test, And Verification

Run commands from this directory.

- `cargo fmt --all -- --check`
- `cargo test --workspace`
- `pnpm.cmd test` when Node scripts are introduced
- `pnpm.cmd verify` when repository verification scripts are introduced

Run the narrowest relevant check first, then broader verification when RPC contracts, storage contracts, security, or cross-crate boundaries change.

## Agent Execution Rules

Use the convention dictionary instead of broad context loading. Do not hand-edit generated SDK output. Do not replace generated SDK integration with raw HTTP or raw gRPC stubs when an SDK/facade exists. Keep changes scoped to this new discovery control plane. No browser or UI design work is in scope.

## Human Review Rules

Request human review before breaking SDKWORK standards, changing public naming, altering security/auth behavior, changing database migrations or production deployment config, deleting data/files, or changing generated SDK ownership.
