import path from 'node:path';

import { normalizeText, parseBooleanEnv } from './env-file.mjs';

export function createGatewayHelpers(spec, topologyHelpers) {
  const envKeys = spec.envKeys ?? {};
  const defaultBind = spec.defaults?.gatewayBind ?? '127.0.0.1:3900';

  function shouldAutostartGateway(env) {
    const key = envKeys.gatewayAutostart ?? 'SDKWORK_APP_GATEWAY_AUTOSTART';
    return parseBooleanEnv(env[key], true);
  }

  function resolveGatewayBind(env, topology) {
    topologyHelpers.assertTopology(topology);
    if (topology === 'standalone') {
      const standaloneKey = envKeys.standaloneGatewayBind ?? 'SDKWORK_APP_STANDALONE_GATEWAY_BIND';
      const bind = normalizeText(env[standaloneKey]);
      if (bind) {
        if (bind.startsWith('http://') || bind.startsWith('https://')) {
          throw new Error(`${standaloneKey} must be host:port, not a URL`);
        }
        return bind;
      }
    }

    const cloudKey = envKeys.cloudGatewayBind ?? 'SDKWORK_API_GATEWAY_BIND';
    const cloudBind = normalizeText(env[cloudKey]);
    if (cloudBind) {
      if (cloudBind.startsWith('http://') || cloudBind.startsWith('https://')) {
        throw new Error(`${cloudKey} must be host:port, not a URL`);
      }
      return cloudBind;
    }

    return defaultBind;
  }

  function resolveGatewayBaseUrl(env, topology) {
    const explicitKeys = [
      envKeys.clientApiGatewayBaseUrl,
      envKeys.apiGatewayBaseUrl,
      'SDKWORK_API_GATEWAY_BASE_URL',
    ].filter(Boolean);

    for (const key of explicitKeys) {
      const value = normalizeText(env[key]);
      if (value) {
        return value.replace(/\/+$/u, '');
      }
    }

    return `http://${resolveGatewayBind(env, topology)}`;
  }

  function resolveStandaloneGatewayConfigPath(env, repoRoot) {
    const configKey = envKeys.standaloneGatewayConfig ?? 'SDKWORK_APP_STANDALONE_GATEWAY_CONFIG';
    const explicit = normalizeText(env[configKey]);
    if (explicit) {
      return path.isAbsolute(explicit) ? explicit : path.resolve(repoRoot, explicit);
    }

    const environmentKey = envKeys.standaloneGatewayEnvironment ?? 'SDKWORK_APP_STANDALONE_GATEWAY_ENVIRONMENT';
    const environment = normalizeText(env[environmentKey]) || 'development';
    const pattern = spec.components?.standaloneGateway?.configGlob
      ?? 'configs/{app}-standalone-gateway.{profile}.toml';
    const appId = spec.appId ?? 'app';
    const relative = pattern
      .replaceAll('{app}', appId)
      .replaceAll('{profile}', environment);
    return path.resolve(repoRoot, relative);
  }

  function resolveCloudGatewayConfigPath(env, profile = 'development', repoRoot) {
    const configKey = envKeys.cloudGatewayConfig ?? 'SDKWORK_API_GATEWAY_CONFIG';
    const explicit = normalizeText(env[configKey]);
    if (explicit) {
      return path.isAbsolute(explicit) ? explicit : path.resolve(repoRoot, explicit);
    }

    const pattern = spec.components?.cloudGateway?.configGlob
      ?? 'configs/sdkwork-api-gateway.{app}.{profile}.toml';
    const appId = spec.appId ?? 'app';
    const relative = pattern
      .replaceAll('{app}', appId)
      .replaceAll('{profile}', profile);
    return path.resolve(repoRoot, relative);
  }

  return {
    shouldAutostartGateway,
    resolveGatewayBind,
    resolveGatewayBaseUrl,
    resolveStandaloneGatewayConfigPath,
    resolveCloudGatewayConfigPath,
  };
}
