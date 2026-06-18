import path from 'node:path';

import { createGatewayHelpers } from './gateway.mjs';
import { createIamDatabaseHelpers } from './iam-database.mjs';
import { loadEnvFile, mergeRuntimeEnv, normalizeText } from './env-file.mjs';
import {
  buildProfileId,
  listProfileIdsFromVocabulary,
  parseProfileId,
  resolveProfileRelativePath,
} from './profile-id.mjs';
import { createSurfaceHelpers } from './surfaces.mjs';
import {
  listPackageTargets,
  listPackageTargetsByProfile,
  findPackageTarget,
} from './spec.mjs';

function appEnvPrefix(appId) {
  return String(appId).replace(/-/g, '_').toUpperCase();
}

export function createTopologyRuntimeV2(spec, repoRoot) {
  const hostingValues = spec.vocabulary?.hosting?.allowed ?? ['self-hosted', 'cloud-hosted'];
  const serviceLayoutValues = spec.vocabulary?.serviceLayout?.allowed ?? ['unified-process', 'split-services'];
  const environmentValues = spec.vocabulary?.environment?.allowed ?? ['development', 'production'];
  const envKeys = spec.envKeys ?? {};
  const prefix = appEnvPrefix(spec.appId);
  const hostingKey = envKeys.hosting ?? `SDKWORK_${prefix}_HOSTING`;
  const serviceLayoutKey = envKeys.serviceLayout ?? `SDKWORK_${prefix}_SERVICE_LAYOUT`;
  const environmentKey = envKeys.environment ?? `SDKWORK_${prefix}_ENVIRONMENT`;
  const profileIdKey = envKeys.profileId ?? `SDKWORK_${prefix}_PROFILE_ID`;
  const clientHostingKey = envKeys.clientHosting ?? envKeys.clientTopology ?? `VITE_${prefix}_HOSTING`;
  const profileIds = Object.keys(spec.profileFiles ?? {});

  function assertHosting(value) {
    const normalized = normalizeText(value);
    if (!normalized || !hostingValues.includes(normalized)) {
      throw new Error(`hosting must be one of: ${hostingValues.join(', ')}`);
    }
    return normalized;
  }

  function assertServiceLayout(value) {
    const normalized = normalizeText(value);
    if (!normalized || !serviceLayoutValues.includes(normalized)) {
      throw new Error(`serviceLayout must be one of: ${serviceLayoutValues.join(', ')}`);
    }
    return normalized;
  }

  function assertEnvironment(value) {
    const normalized = normalizeText(value);
    if (!normalized || !environmentValues.includes(normalized)) {
      throw new Error(`environment must be one of: ${environmentValues.join(', ')}`);
    }
    return normalized;
  }

  function assertProfileId(value) {
    const normalized = normalizeText(value);
    if (!normalized) {
      throw new Error('profile id is required');
    }
    parseProfileId(normalized);
    if (profileIds.length > 0 && !profileIds.includes(normalized)) {
      throw new Error(`profile id must be one of: ${profileIds.join(', ')}`);
    }
    return normalized;
  }

  function profilePath(profileId) {
    const relative = resolveProfileRelativePath(spec, assertProfileId(profileId));
    return path.join(repoRoot, relative);
  }

  function loadProfile(profileId) {
    const resolvedProfileId = assertProfileId(profileId);
    const profilePathValue = profilePath(resolvedProfileId);
    const values = loadEnvFile(profilePathValue, repoRoot);
    const loadedProfileId = normalizeText(values[profileIdKey]);
    if (loadedProfileId && loadedProfileId !== resolvedProfileId) {
      throw new Error(
        `profile mismatch: ${profilePathValue} declares ${loadedProfileId}, expected ${resolvedProfileId}`,
      );
    }
    return values;
  }

  function applyProfileEnv(profileId, layers = []) {
    const { hosting, serviceLayout, environment } = parseProfileId(assertProfileId(profileId));
    return mergeRuntimeEnv(...layers, {
      [hostingKey]: hosting,
      [serviceLayoutKey]: serviceLayout,
      [environmentKey]: environment,
      [profileIdKey]: profileId,
      [clientHostingKey]: hosting,
    });
  }

  const legacyTopologyBridge = {
    assertTopology(value) {
      const normalized = normalizeText(value);
      if (normalized === 'self-hosted' || normalized === 'standalone') {
        return 'standalone';
      }
      if (normalized === 'cloud-hosted' || normalized === 'cloud') {
        return 'cloud';
      }
      throw new Error(`hosting must be one of: ${hostingValues.join(', ')}`);
    },
    assertProfile(environment) {
      return assertEnvironment(environment);
    },
  };

  const surfaces = createSurfaceHelpers(spec);
  const gateway = createGatewayHelpers(
    {
      ...spec,
      envKeys: {
        ...envKeys,
        gatewayAutostart:
          envKeys.gatewayAutostart
          ?? spec.surfaces?.['platform.api-gateway']?.autostartEnv
          ?? `SDKWORK_${prefix}_PLATFORM_API_GATEWAY_AUTOSTART`,
        standaloneGatewayBind:
          envKeys.standaloneGatewayBind
          ?? spec.surfaces?.['application.public-ingress']?.bindEnv,
        cloudGatewayBind: envKeys.cloudGatewayBind ?? 'SDKWORK_API_GATEWAY_BIND',
        clientApiGatewayBaseUrl:
          envKeys.clientApiGatewayBaseUrl
          ?? spec.surfaces?.['platform.api-gateway']?.clientHttpEnv
          ?? spec.surfaces?.['application.public-ingress']?.clientHttpEnv,
        apiGatewayBaseUrl:
          envKeys.apiGatewayBaseUrl
          ?? spec.surfaces?.['platform.api-gateway']?.httpUrlEnv
          ?? spec.surfaces?.['application.public-ingress']?.httpUrlEnv,
      },
    },
    legacyTopologyBridge,
  );

  function resolveGatewayBind(env, hosting) {
    const normalizedHosting = assertHosting(hosting);
    if (normalizedHosting === 'self-hosted') {
      const applicationBind = surfaces.resolveSurfaceBind(env, 'application.public-ingress');
      if (applicationBind) {
        return applicationBind;
      }
    }
    return gateway.resolveGatewayBind(env, normalizedHosting === 'self-hosted' ? 'standalone' : 'cloud');
  }

  function resolveGatewayBaseUrl(env, hosting) {
    const normalizedHosting = assertHosting(hosting);
    if (normalizedHosting === 'self-hosted') {
      const applicationUrl = surfaces.resolveSurfaceHttpUrl(env, 'application.public-ingress');
      if (applicationUrl) {
        return applicationUrl;
      }
    }
    const platformUrl = spec.surfaces?.['platform.api-gateway']
      ? surfaces.resolveSurfaceHttpUrl(env, 'platform.api-gateway')
      : undefined;
    if (platformUrl) {
      return platformUrl;
    }
    return gateway.resolveGatewayBaseUrl(env, normalizedHosting === 'self-hosted' ? 'standalone' : 'cloud');
  }

  const iam = createIamDatabaseHelpers(spec);

  return {
    spec,
    repoRoot,
    schemaVersion: 2,
    profileIds,
    hostingValues,
    serviceLayoutValues,
    environmentValues,
    envKeys,
    hostingKey,
    serviceLayoutKey,
    environmentKey,
    profileIdKey,
    clientHostingKey,
    defaults: {
      developmentProfileId: spec.defaults?.developmentProfileId
        ?? buildProfileId('self-hosted', 'split-services', 'development'),
      productionProfileId: spec.defaults?.productionProfileId
        ?? buildProfileId('cloud-hosted', 'split-services', 'production'),
      desktopBuildProfileId: spec.defaults?.desktopBuildProfileId
        ?? spec.defaults?.productionProfileId
        ?? buildProfileId('cloud-hosted', 'split-services', 'production'),
      gatewayBind: spec.defaults?.gatewayBind ?? '127.0.0.1:3900',
    },
    buildProfileId,
    parseProfileId,
    listProfileIds: () => (profileIds.length > 0 ? profileIds : listProfileIdsFromVocabulary(spec)),
    assertHosting,
    assertServiceLayout,
    assertEnvironment,
    assertProfileId,
    profilePath,
    loadProfile,
    applyProfileEnv,
    loadEnvFile: (envFile) => loadEnvFile(envFile, repoRoot),
    mergeRuntimeEnv,
    listPackageTargets: () => listPackageTargets(spec),
    listPackageTargetsByProfile: (profile) => listPackageTargetsByProfile(spec, profile),
    findPackageTarget: (targetId) => findPackageTarget(spec, targetId),
    ...surfaces,
    shouldAutostartGateway: (env) => {
      if (spec.surfaces?.['platform.api-gateway']) {
        return surfaces.resolveSurfaceAutostart(env, 'platform.api-gateway', true);
      }
      return gateway.shouldAutostartGateway(env);
    },
    resolveGatewayBind,
    resolveGatewayBaseUrl,
    resolveStandaloneGatewayConfigPath: (env) => gateway.resolveStandaloneGatewayConfigPath(env, repoRoot),
    resolveCloudGatewayConfigPath: (env, profile = 'development') =>
      gateway.resolveCloudGatewayConfigPath(env, profile, repoRoot),
    resolveIamDevEnv: (env = process.env, options = {}) => iam.resolveIamDevEnv(env, repoRoot, {
      iamDefaults: spec.iamDevDefaults,
      ...options,
    }),
    resolveIamDatabaseEnv: iam.resolveIamDatabaseEnv,
    describeIamDatabaseTarget: iam.describeIamDatabaseTarget,
    assertPostgresReachableForIam: (env, options = {}) => iam.assertPostgresReachableForIam(env, {
      missingDatabaseMessage: spec.messages?.missingPostgres
        ?? 'IAM requires PostgreSQL for dev login. Configure .env.postgres and start PostgreSQL.',
      unreachableDatabaseMessage: spec.messages?.unreachablePostgres,
      ...options,
    }),
    listOrchestrationProcesses: (profileId) =>
      spec.orchestration?.profiles?.[assertProfileId(profileId)]?.processes ?? [],
    listHealthSurfaces: (profileId) =>
      spec.orchestration?.profiles?.[assertProfileId(profileId)]?.healthSurfaces ?? [],
  };
}
