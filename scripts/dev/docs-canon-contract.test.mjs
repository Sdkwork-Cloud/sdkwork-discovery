#!/usr/bin/env node
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');

const prdPath = path.join(repoRoot, 'docs/product/prd/PRD.md');
const techPath = path.join(repoRoot, 'docs/architecture/tech/TECH_ARCHITECTURE.md');
const indexPath = path.join(repoRoot, 'docs/INDEX.yaml');
const agentsPath = path.join(repoRoot, 'AGENTS.md');
const packageJsonPath = path.join(repoRoot, 'package.json');

const prd = fs.readFileSync(prdPath, 'utf8');
const tech = fs.readFileSync(techPath, 'utf8');
const index = fs.readFileSync(indexPath, 'utf8');
const agents = fs.readFileSync(agentsPath, 'utf8');
const packageJson = fs.readFileSync(packageJsonPath, 'utf8');

assert.match(prd, /Status:\s*\w+/);
assert.match(prd, /REQUIREMENTS_SPEC\.md/);
assert.match(prd, /##\s+1\.\s+Background And Problem/);
assert.match(prd, /##\s+3\.\s+Goals And Non-Goals/);
assert.match(prd, /##\s+4\.\s+Scope/);
assert.match(prd, /Service Registry/);
assert.match(prd, /Config Registry/);

assert.match(tech, /ARCHITECTURE_DECISION_SPEC\.md/);
assert.match(tech, /##\s+1\.\s+Architecture Overview/);
assert.match(tech, /##\s+2\.\s+Technology Choices/);
assert.match(tech, /##\s+6\.\s+Security, Privacy, And Observability/);
assert.match(tech, /sdkwork-discovery-rpc-sdk/);
assert.match(tech, /pnpm run verify/);

assert.match(index, /kind:\s*sdkwork\.docs\.index/);
assert.match(index, /docs\/product\/prd\/PRD\.md/);
assert.match(index, /docs\/architecture\/tech\/TECH_ARCHITECTURE\.md/);

assert.match(agents, /docs\/product\/prd\/PRD\.md/);
assert.match(agents, /docs\/architecture\/tech\/TECH_ARCHITECTURE\.md/);

assert.match(packageJson, /verify:docs/);
assert.match(packageJson, /test:docs-canon/);

const changelog = fs.readFileSync(path.join(repoRoot, 'docs/changelogs/CHANGELOG.md'), 'utf8');
assert.match(changelog, /## \[0\.1\.0\]/);
assert.match(changelog, /RELEASE-v0\.1\.0\.md/);

const releaseNotes = fs.readFileSync(
  path.join(repoRoot, 'docs/releases/RELEASE-v0.1.0.md'),
  'utf8',
);
assert.match(releaseNotes, /Version:\s*0\.1\.0/);
assert.match(releaseNotes, /pnpm run verify/);
assert.match(releaseNotes, /Rollback/);

const reviewPath = path.join(
  repoRoot,
  'docs/engineering/reviews/REVIEW-20260623-release-gate-v0.1.0.md',
);
assert.ok(fs.existsSync(reviewPath), 'release gate review must exist');
const review = fs.readFileSync(reviewPath, 'utf8');
assert.match(review, /QUALITY_GATE_SPEC\.md/);
assert.match(review, /pnpm run verify/);

const cargoToml = fs.readFileSync(path.join(repoRoot, 'Cargo.toml'), 'utf8');
const versionMatch = cargoToml.match(/^version\s*=\s*"([^"]+)"/m);
assert.ok(versionMatch, 'Cargo.toml workspace version must be set');
const workspaceVersion = versionMatch[1];
assert.match(changelog, new RegExp(`\\[${workspaceVersion.replace('.', '\\.')}\\]`));
assert.match(releaseNotes, new RegExp(`Version:\\s*${workspaceVersion}`));
