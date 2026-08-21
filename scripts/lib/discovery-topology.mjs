import path from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  buildProfileId,
  createTopologyRuntime,
  isTcpPortReachable,
  loadTopologySpec,
  normalizeText,
} from '@sdkwork/app-topology';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

export const REPO_ROOT = path.resolve(__dirname, '..', '..');
export const SPEC_PATH = path.join(REPO_ROOT, 'specs/topology.spec.json');

const spec = loadTopologySpec(SPEC_PATH);
const runtime = createTopologyRuntime(spec, REPO_ROOT);

export const DEFAULT_DEV_PROFILE_ID = runtime.defaults.developmentProfileId;
export const DEFAULT_PRODUCTION_PROFILE_ID = runtime.defaults.productionProfileId;

export function resolveDevProfileId(deploymentProfile) {
  runtime.assertDeploymentProfile(deploymentProfile);
  return buildProfileId(deploymentProfile, 'development');
}

export function splitHostPort(bind) {
  const normalized = normalizeText(bind);
  if (!normalized) {
    return { host: undefined, port: undefined };
  }
  const separator = normalized.lastIndexOf(':');
  if (separator <= 0) {
    return { host: normalized, port: undefined };
  }
  return {
    host: normalized.slice(0, separator),
    port: normalized.slice(separator + 1),
  };
}

export function filterDiscoveryProcessEnv(env = {}) {
  const filtered = {};
  for (const [key, value] of Object.entries(env)) {
    if (!key.startsWith('SDKWORK_DISCOVERY_')) {
      continue;
    }
    filtered[key] = value;
  }
  return filtered;
}

export function resolveSurfaceGrpcUrl(profileEnv = {}, surfaceId = 'application.public-ingress') {
  const surface = spec.surfaces?.[surfaceId];
  if (!surface) {
    return undefined;
  }
  const grpcUrlKey = surface.grpcUrlEnv;
  if (grpcUrlKey && profileEnv[grpcUrlKey]) {
    return normalizeText(profileEnv[grpcUrlKey]);
  }
  const bind = resolveSurfaceBind(profileEnv, surfaceId);
  if (!bind) {
    return undefined;
  }
  return `grpc://${bind}`;
}

export const loadProfile = runtime.loadProfile;
export const applyProfileEnv = runtime.applyProfileEnv;
export const mergeRuntimeEnv = runtime.mergeRuntimeEnv;
export const loadEnvFile = runtime.loadEnvFile;
export const assertDeploymentProfile = runtime.assertDeploymentProfile;
export const resolveSurfaceBind = runtime.resolveSurfaceBind.bind(runtime);
export const listOrchestrationProcesses = runtime.listOrchestrationProcesses;
export const listHealthSurfaces = runtime.listHealthSurfaces;

export { buildProfileId, isTcpPortReachable, spec, runtime };
