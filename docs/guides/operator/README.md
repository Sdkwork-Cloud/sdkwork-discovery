# Operator Guide

Deployment, monitoring, and incident response for SDKWork Discovery.

## Start here

1. [Production server deployment](../../runbooks/RUNBOOK-production-server-deployment.md)
2. [Database migration rollback](../../runbooks/RUNBOOK-database-migration-rollback.md)
3. Repository [README.md](../../../README.md) — runtime env keys and storage providers
4. [Topology standard](../../topology-standard.md) — hosting and ingress profiles

## Configuration templates

- Development: `etc/discovery.example.toml`
- Production: `etc/discovery.production.example.toml`
- Topology env: `configs/topology/*.production.env`

## Verification

```bash
pnpm run verify
pnpm run release:validate
```

See `DOCUMENTATION_SPEC.md` section 2.
