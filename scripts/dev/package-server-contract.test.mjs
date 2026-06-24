#!/usr/bin/env node
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const packageScript = fs.readFileSync(
  path.join(repoRoot, 'scripts', 'package-server.mjs'),
  'utf8',
);

assert.match(packageScript, /discovery\.production\.example\.toml/);
assert.match(packageScript, /INSTALL\.md/);
assert.match(packageScript, /prometheusFeature:\s*true/);
assert.match(packageScript, /SDKWORK_DISCOVERY_METRICS_BIND/);
assert.match(packageScript, /SIGTERM/);
assert.match(packageScript, /copyRunbooks/);
assert.match(packageScript, /copyReleaseEvidence/);
assert.match(packageScript, /RUNBOOK-production-server-deployment\.md/);
assert.match(packageScript, /releaseEvidence/);
assert.match(packageScript, /docs\/changelogs\/CHANGELOG\.md/);

const requiredArchiveEntries = [
  'config/discovery.production.example.toml',
  'docs/runbooks/RUNBOOK-production-server-deployment.md',
  'docs/runbooks/RUNBOOK-database-migration-rollback.md',
  'docs/changelogs/CHANGELOG.md',
  'docs/releases/RELEASE-v',
  'INSTALL.md',
  'install-manifest.json',
];
for (const entry of requiredArchiveEntries) {
  assert.match(packageScript, new RegExp(entry.replaceAll('.', '\\.')));
}

assert.ok(
  fs.existsSync(path.join(repoRoot, 'etc', 'discovery.production.example.toml')),
  'production example config must exist',
);
assert.ok(
  fs.existsSync(path.join(repoRoot, 'docs/changelogs/CHANGELOG.md')),
  'CHANGELOG.md must exist',
);
assert.ok(
  fs.existsSync(path.join(repoRoot, 'docs/releases/RELEASE-v0.1.0.md')),
  'RELEASE-v0.1.0.md must exist',
);
