#!/usr/bin/env node
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');

const scanRoots = [
  'crates',
  'services',
  'scripts',
  'configs',
  'docs',
  'specs',
  'README.md',
  'AGENTS.md',
];

const skipPathFragments = [
  '/target/',
  '/node_modules/',
  '/generated/',
  'sdkwork-discovery-topology-baggage.test.mjs',
  'docs/topology-standard.md',
  'specs/implementation-plan-registry-alignment.md',
  'scripts/discovery-dev.mjs',
  'crates/sdkwork-discovery-config/tests/runtime_config.rs',
];

const allowlistPathFragments = ['specs/topology.spec.json'];

const bannedPatterns = [
  { id: 'local-minimal profile', pattern: /(?<![\w-])local-minimal(?![\w-])/u },
  { id: 'local-default profile', pattern: /(?<![\w-])local-default(?![\w-])/u },
  { id: 'topology v1 env key', pattern: /SDKWORK_DISCOVERY_TOPOLOGY/u },
  { id: 'topology CLI flag', pattern: /--topology\b/u },
  {
    id: 'legacy grpc bind env key',
    pattern: /SDKWORK_DISCOVERY_(GRPC_BIND_HOST|GRPC_PORT|ADMIN_GRPC_PORT)/u,
  },
];

function slash(value) {
  return String(value).replaceAll('\\', '/');
}

function shouldSkip(relativePath) {
  const normalized = slash(relativePath);
  return skipPathFragments.some((fragment) => normalized.includes(fragment));
}

function isAllowlisted(relativePath) {
  const normalized = slash(relativePath);
  return allowlistPathFragments.some((fragment) => normalized.endsWith(fragment));
}

function collectFiles(relativeRoot) {
  const absoluteRoot = path.join(repoRoot, relativeRoot);
  if (!fs.existsSync(absoluteRoot)) {
    return [];
  }
  const stat = fs.statSync(absoluteRoot);
  if (stat.isFile()) {
    return [relativeRoot];
  }
  const files = [];
  for (const entry of fs.readdirSync(absoluteRoot, { withFileTypes: true })) {
    const relativePath = path.join(relativeRoot, entry.name);
    if (shouldSkip(relativePath)) {
      continue;
    }
    if (entry.isDirectory()) {
      files.push(...collectFiles(relativePath));
      continue;
    }
    files.push(relativePath);
  }
  return files;
}

function readText(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

const files = scanRoots.flatMap((root) => collectFiles(root));

for (const { id, pattern } of bannedPatterns) {
  const hits = [];
  for (const relativePath of files) {
    if (isAllowlisted(relativePath)) {
      continue;
    }
    const text = readText(relativePath);
    if (pattern.test(text)) {
      hits.push(relativePath);
    }
  }
  assert.equal(
    hits.length,
    0,
    `topology baggage (${id}) found in active paths: ${hits.join(', ')}`,
  );
}

assert.ok(fs.existsSync(path.join(repoRoot, 'specs/topology.spec.json')), 'topology spec required');
const spec = JSON.parse(readText('specs/topology.spec.json'));
assert.equal(spec.schemaVersion, 5);
assert.equal(spec.archetype, 'application-http-gateway');
assert.equal(spec.defaults.developmentProfileId, 'standalone.development');
assert.equal(spec.defaults.productionProfileId, 'cloud.production');
assert.ok(spec.surfaces['application.public-ingress']);
assert.ok(spec.surfaces['operations.control-ingress']);

for (const profileId of Object.keys(spec.profileFiles ?? {})) {
  const profilePath = spec.profileFiles[profileId];
  const absoluteProfilePath = path.join(repoRoot, profilePath);
  assert.ok(fs.existsSync(absoluteProfilePath), `${profilePath} should exist for ${profileId}`);
  const profileEnv = readText(profilePath);
  assert.match(profileEnv, /SDKWORK_DISCOVERY_PROFILE_ID=/u);
  assert.match(profileEnv, /SDKWORK_DISCOVERY_ENVIRONMENT=/u);
  assert.match(profileEnv, /SDKWORK_DISCOVERY_APPLICATION_PUBLIC_GRPC_URL=/u);
  for (const retiredKey of spec.retired?.envKeys ?? []) {
    assert.doesNotMatch(
      profileEnv,
      new RegExp(`^${retiredKey}=`, 'm'),
      `${profileId} must not declare retired env key ${retiredKey}`,
    );
  }
  assert.ok(
    spec.orchestration?.profiles?.[profileId],
    `${profileId} must declare orchestration profile`,
  );
}

const profileDir = path.join(repoRoot, 'etc/topology');
const profileFiles = fs.readdirSync(profileDir).filter((name) => name.endsWith('.env'));
assert.ok(profileFiles.length >= 2, 'topology profile env files required');

const packageJson = JSON.parse(readText('package.json'));
assert.match(
  JSON.stringify(packageJson.dependencies ?? {}),
  /"@sdkwork\/app-topology"/u,
  'package.json must depend on @sdkwork/app-topology',
);
assert.match(
  JSON.stringify(packageJson.scripts ?? {}),
  /"dev"/u,
  'package.json must expose dev',
);
assert.match(
  JSON.stringify(packageJson.scripts ?? {}),
  /"dev:cloud"/u,
  'package.json must expose dev:cloud',
);
assert.match(
  JSON.stringify(packageJson.scripts ?? {}),
  /topology:validate/u,
  'package.json must expose topology:validate',
);
assert.match(
  JSON.stringify(packageJson.scripts ?? {}),
  /topology:matrix/u,
  'package.json must expose topology:matrix',
);

assert.equal(spec.scripts?.discoveryDev, 'scripts/discovery-dev.mjs');
assert.equal(spec.scripts?.pnpm?.['dev']?.deploymentProfile, 'standalone');
assert.equal(spec.scripts?.pnpm?.['dev:cloud']?.deploymentProfile, 'cloud');

const { loadProfile, resolveSurfaceGrpcUrl } = await import(
  pathToFileURL(path.join(repoRoot, 'scripts/lib/discovery-topology.mjs')).href
);
const devProfileEnv = loadProfile('standalone.development');
assert.equal(
  resolveSurfaceGrpcUrl(devProfileEnv),
  'grpc://127.0.0.1:19090',
  'adapter should resolve application.public-ingress grpc url from profile env',
);

const discoveryDevScript = readText('scripts/discovery-dev.mjs');
assert.match(discoveryDevScript, /listOrchestrationProcesses/u);
assert.match(discoveryDevScript, /--topology is retired/u);

assert.ok(
  fs.existsSync(path.join(repoRoot, 'scripts/discovery-dev.mjs')),
  'discovery-dev orchestrator required',
);
assert.ok(
  fs.existsSync(path.join(repoRoot, 'scripts/lib/discovery-topology.mjs')),
  'discovery topology adapter required',
);
assert.ok(
  fs.existsSync(path.join(repoRoot, 'docs/topology-standard.md')),
  'topology-standard doc required',
);

console.log('[sdkwork-discovery-topology-baggage] ok');
