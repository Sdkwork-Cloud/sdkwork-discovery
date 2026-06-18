import fs from 'node:fs';
import path from 'node:path';

import { normalizeText } from './env-file.mjs';
import { validateTopologySpecV2 } from './spec-v2.mjs';

export const DEFAULT_TOPOLOGIES = ['standalone', 'cloud'];
export const DEFAULT_PROFILES = ['development', 'production'];

export function validateTopologySpecV1(spec, specPath = 'topology.spec.json') {
  if (!spec || typeof spec !== 'object') {
    throw new Error(`${specPath} must be a JSON object`);
  }
  if (spec.schemaVersion !== 1) {
    throw new Error(`${specPath} schemaVersion must be 1`);
  }
  if (spec.kind !== 'sdkwork.app.topology') {
    throw new Error(`${specPath} kind must be sdkwork.app.topology`);
  }
  if (!normalizeText(spec.appId)) {
    throw new Error(`${specPath} appId is required`);
  }

  const topologies = spec.vocabulary?.topology?.allowed ?? DEFAULT_TOPOLOGIES;
  const profiles = spec.vocabulary?.profile?.allowed ?? DEFAULT_PROFILES;
  if (!Array.isArray(topologies) || topologies.length === 0) {
    throw new Error(`${specPath} vocabulary.topology.allowed must be a non-empty array`);
  }
  if (!Array.isArray(profiles) || profiles.length === 0) {
    throw new Error(`${specPath} vocabulary.profile.allowed must be a non-empty array`);
  }

  const profileRoot = spec.profileRoot ?? 'configs/topology';
  const profilePattern = spec.profilePattern ?? '{topology}.{profile}.env';
  for (const topology of topologies) {
    for (const profile of profiles) {
      const relative = `${profileRoot}/${profilePattern}`
        .replaceAll('{topology}', topology)
        .replaceAll('{profile}', profile);
      if (spec.profileFiles?.[topology]?.[profile]) {
        continue;
      }
      spec.profileFiles ??= {};
      spec.profileFiles[topology] ??= {};
      spec.profileFiles[topology][profile] = relative;
    }
  }

  return spec;
}

export function loadTopologySpec(specPath) {
  const resolved = path.resolve(specPath);
  if (!fs.existsSync(resolved)) {
    throw new Error(`topology spec not found: ${resolved}`);
  }
  const spec = JSON.parse(fs.readFileSync(resolved, 'utf8'));
  validateTopologySpec(spec, resolved);
  return spec;
}

export function validateTopologySpec(spec, specPath = 'topology.spec.json') {
  if (spec?.schemaVersion === 2) {
    return validateTopologySpecV2(spec, specPath);
  }
  return validateTopologySpecV1(spec, specPath);
}

export function listPackageTargets(spec) {
  return spec.packaging?.targets ?? [];
}

export function findPackageTarget(spec, targetId) {
  return listPackageTargets(spec).find((target) => target.id === targetId);
}

export function listPackageTargetsByProfile(spec, profile) {
  const targets = listPackageTargets(spec);
  if (!profile || profile === 'all') {
    return targets;
  }
  return targets.filter((target) => target.profile === profile);
}
