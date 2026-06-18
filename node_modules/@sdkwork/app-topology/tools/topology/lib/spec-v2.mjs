import { normalizeText } from './env-file.mjs';
import { listProfileIdsFromVocabulary, resolveProfileRelativePath } from './profile-id.mjs';

const ARCHETYPES = [
  'application-http-gateway',
  'realtime-application-platform',
  'application-rest-edge-device',
];

const REQUIRED_SURFACES_BY_ARCHETYPE = {
  'application-http-gateway': ['application.public-ingress'],
  'realtime-application-platform': ['application.public-ingress', 'platform.api-gateway'],
  'application-rest-edge-device': ['application.app-http', 'edge.device-ingress', 'platform.api-gateway'],
};

export function validateTopologySpecV2(spec, specPath = 'topology.spec.json') {
  if (!spec || typeof spec !== 'object') {
    throw new Error(`${specPath} must be a JSON object`);
  }
  if (spec.schemaVersion !== 2) {
    throw new Error(`${specPath} schemaVersion must be 2`);
  }
  if (spec.kind !== 'sdkwork.app.topology') {
    throw new Error(`${specPath} kind must be sdkwork.app.topology`);
  }
  if (!normalizeText(spec.appId)) {
    throw new Error(`${specPath} appId is required`);
  }
  if (!normalizeText(spec.archetype) || !ARCHETYPES.includes(spec.archetype)) {
    throw new Error(`${specPath} archetype must be one of: ${ARCHETYPES.join(', ')}`);
  }

  const hosting = spec.vocabulary?.hosting?.allowed;
  const serviceLayout = spec.vocabulary?.serviceLayout?.allowed;
  const environment = spec.vocabulary?.environment?.allowed;
  if (!Array.isArray(hosting) || hosting.length === 0) {
    throw new Error(`${specPath} vocabulary.hosting.allowed must be a non-empty array`);
  }
  if (!Array.isArray(serviceLayout) || serviceLayout.length === 0) {
    throw new Error(`${specPath} vocabulary.serviceLayout.allowed must be a non-empty array`);
  }
  if (!Array.isArray(environment) || environment.length === 0) {
    throw new Error(`${specPath} vocabulary.environment.allowed must be a non-empty array`);
  }

  spec.profileFiles ??= {};
  for (const profileId of listProfileIdsFromVocabulary(spec)) {
    if (!spec.profileFiles[profileId]) {
      spec.profileFiles[profileId] = resolveProfileRelativePath(spec, profileId);
    }
  }

  const surfaces = spec.surfaces ?? {};
  for (const surfaceId of REQUIRED_SURFACES_BY_ARCHETYPE[spec.archetype] ?? []) {
    if (!surfaces[surfaceId]) {
      throw new Error(`${specPath} missing required surface for archetype ${spec.archetype}: ${surfaceId}`);
    }
  }

  for (const [surfaceId, surface] of Object.entries(surfaces)) {
    if (!normalizeText(surface.connectivityPlane)) {
      throw new Error(`${specPath} surfaces.${surfaceId}.connectivityPlane is required`);
    }
    const hasHttp = surface.httpUrlEnv || surface.publicHttpEnv || surface.bindEnv;
    if (!hasHttp && !surface.optional) {
      throw new Error(`${specPath} surfaces.${surfaceId} must declare bindEnv or httpUrlEnv`);
    }
  }

  const orchestrationProfiles = spec.orchestration?.profiles ?? {};
  for (const profileId of Object.keys(orchestrationProfiles)) {
    if (!spec.profileFiles[profileId]) {
      throw new Error(`${specPath} orchestration profile not declared in profileFiles: ${profileId}`);
    }
  }

  return spec;
}
