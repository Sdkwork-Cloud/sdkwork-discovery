#!/usr/bin/env node
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');

const requiredRunbooks = [
  'docs/runbooks/RUNBOOK-production-server-deployment.md',
  'docs/runbooks/RUNBOOK-database-migration-rollback.md',
];

for (const relativePath of requiredRunbooks) {
  const absolutePath = path.join(repoRoot, relativePath);
  assert.ok(fs.existsSync(absolutePath), `missing runbook ${relativePath}`);
  const source = fs.readFileSync(absolutePath, 'utf8');
  assert.match(source, /## Rollback/);
  assert.match(source, /pnpm run (verify|db:|release:validate)/);
}

const operatorGuide = fs.readFileSync(
  path.join(repoRoot, 'docs/guides/operator/README.md'),
  'utf8',
);
assert.match(operatorGuide, /RUNBOOK-production-server-deployment\.md/);

const developerGuide = fs.readFileSync(
  path.join(repoRoot, 'docs/guides/developer/README.md'),
  'utf8',
);
assert.match(developerGuide, /pnpm run verify/);
assert.match(developerGuide, /sdkwork-discovery-core/);

const integratorGuide = fs.readFileSync(
  path.join(repoRoot, 'docs/guides/integrator/README.md'),
  'utf8',
);
assert.match(integratorGuide, /sdkwork-discovery-rpc-sdk/);
assert.match(integratorGuide, /idempotency-key/);

const adrPath = path.join(
  repoRoot,
  'docs/architecture/decisions/ADR-20260609-rust-grpc-control-plane.md',
);
assert.ok(fs.existsSync(adrPath), 'ADR for control plane must exist');

const index = fs.readFileSync(path.join(repoRoot, 'docs/INDEX.yaml'), 'utf8');
assert.match(index, /runbook-production-server-deployment/);
assert.match(index, /review-release-gate-v0-1-0/);
