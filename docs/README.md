# SDKWork Discovery Documentation

## Audience Routing

| I am… | Read first | Then read |
| --- | --- | --- |
| Product or business | [product/PRD.md](product/PRD.md) | [product/requirements/](product/requirements/) |
| Architect | [architecture/TECH_ARCHITECTURE.md](architecture/TECH_ARCHITECTURE.md) | [architecture/decisions/](architecture/decisions/) |
| Developer | [guides/developer/README.md](guides/developer/README.md) | [engineering/plans/](engineering/plans/) |
| Operator | [guides/operator/README.md](guides/operator/README.md) | [runbooks/](runbooks/) |
| Integrator | [guides/integrator/README.md](guides/integrator/README.md) | repository `apis/` and `sdks/` |
| Agent | [../AGENTS.md](../AGENTS.md) | [INDEX.yaml](INDEX.yaml) |

## Canon Documents

| Document | Path |
| --- | --- |
| Product PRD | [product/PRD.md](product/PRD.md) |
| Technical architecture | [architecture/TECH_ARCHITECTURE.md](architecture/TECH_ARCHITECTURE.md) |

## Related Specs

- `DOCUMENTATION_SPEC.md`
- `SDKWORK_WORKSPACE_SPEC.md`
- `REQUIREMENTS_SPEC.md`
- `ARCHITECTURE_DECISION_SPEC.md`

## Verification

```bash
pnpm run verify
pnpm run verify:docs
pnpm run package:server:validate
node ../sdkwork-specs/tools/check-repository-docs-standard.mjs --root .
```
