# deployments

Holds the SDKWork application deploy manifest (`deploy.yaml`) for `sdkwork-discovery`.

## Purpose

Single deployment contract consumed by `deployctl` and SDKWork Deploy Server. Resolves install layout, public domains, client package artifacts, and overrides for the `cloud.unified-process.production` topology profile.

## Owner

- Repository: `sdkwork-discovery`
- Maintainers: SDKWork discovery workstream

## Allowed

- `deploy.yaml` (version 1, simple mode)
- `templates/` (optional Nginx / deploy templates)
- `nginx/` (optional generated site files)

## Forbidden

- Workspace-wide deployment manifests at repository root
- Plaintext secrets in `deploy.yaml` (use `secret://` references)
- Server tar.gz entries in `packages` (reserved for client artifacts per `SDKWORK_DEPLOY_SPEC.md` §10)
- Hand-edited generated Nginx site files outside `deployctl nginx render` flow

## Specs

- `sdkwork-specs/SDKWORK_DEPLOY_SPEC.md` (manifest schema, validation rules)
- `sdkwork-specs/schemas/sdkwork.deploy.schema.v1.json` (JSON Schema)
- `sdkwork-specs/APP_RUNTIME_TOPOLOGY_SPEC.md` (profile id `<deploymentProfile>.<serviceLayout>.<environment>`)
- `sdkwork-specs/DEPLOYMENT_SPEC.md` (standalone/cloud definitions)

## Verification

```bash
node ../sdkwork-specs/tools/deployctl.mjs validate --root .
node ../sdkwork-specs/tools/deployctl.mjs plan --root .
```

`deploy.yaml` MUST validate against `schemas/sdkwork.deploy.schema.v1.json` (structural rules V1–V16).
