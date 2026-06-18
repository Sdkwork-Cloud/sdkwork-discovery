import { normalizeText } from './env-file.mjs';

export function buildProfileId(hosting, serviceLayout, environment) {
  const parts = [hosting, serviceLayout, environment].map((value) => normalizeText(value));
  if (parts.some((value) => !value)) {
    throw new Error('hosting, serviceLayout, and environment are required to build a profile id');
  }
  return parts.join('.');
}

export function parseProfileId(profileId) {
  const normalized = normalizeText(profileId);
  if (!normalized) {
    throw new Error('profile id is required');
  }
  const segments = normalized.split('.');
  if (segments.length !== 3) {
    throw new Error(
      `profile id must be <hosting>.<serviceLayout>.<environment>, received: ${profileId}`,
    );
  }
  const [hosting, serviceLayout, environment] = segments;
  return { hosting, serviceLayout, environment, profileId: normalized };
}

export function resolveProfileRelativePath(spec, profileId) {
  const explicit = spec.profileFiles?.[profileId];
  if (explicit) {
    return explicit;
  }
  const { hosting, serviceLayout, environment } = parseProfileId(profileId);
  const pattern = spec.profilePattern ?? '{hosting}.{serviceLayout}.{environment}.env';
  const profileRoot = spec.profileRoot ?? 'configs/topology';
  return `${profileRoot}/${pattern
    .replaceAll('{hosting}', hosting)
    .replaceAll('{serviceLayout}', serviceLayout)
    .replaceAll('{environment}', environment)}`;
}

export function listProfileIdsFromVocabulary(spec) {
  const hosting = spec.vocabulary?.hosting?.allowed ?? [];
  const serviceLayout = spec.vocabulary?.serviceLayout?.allowed ?? [];
  const environment = spec.vocabulary?.environment?.allowed ?? [];
  const profileIds = [];
  for (const host of hosting) {
    for (const layout of serviceLayout) {
      for (const tier of environment) {
        profileIds.push(buildProfileId(host, layout, tier));
      }
    }
  }
  return profileIds;
}
