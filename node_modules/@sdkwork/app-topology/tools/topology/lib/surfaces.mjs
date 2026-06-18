import { normalizeText } from './env-file.mjs';

function stripTrailingSlashes(value) {
  return value.replace(/\/+$/u, '');
}

function assertHttpUrl(value, label) {
  const normalized = normalizeText(value);
  if (!normalized) {
    return undefined;
  }
  let parsed;
  try {
    parsed = new URL(normalized);
  } catch {
    throw new Error(`${label} must be a valid absolute http(s) URL`);
  }
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
    throw new Error(`${label} must be a valid absolute http(s) URL`);
  }
  return stripTrailingSlashes(normalized);
}

function assertWebsocketOrigin(value, label) {
  const normalized = normalizeText(value);
  if (!normalized) {
    return undefined;
  }
  let parsed;
  try {
    parsed = new URL(normalized);
  } catch {
    throw new Error(`${label} must be a valid ws(s):// origin`);
  }
  if (parsed.protocol !== 'ws:' && parsed.protocol !== 'wss:') {
    throw new Error(`${label} must be a valid ws(s):// origin`);
  }
  if (parsed.pathname !== '/' || parsed.search || parsed.hash) {
    throw new Error(`${label} must be origin only (no path, query, or hash)`);
  }
  return stripTrailingSlashes(normalized);
}

function assertBind(value, label) {
  const normalized = normalizeText(value);
  if (!normalized) {
    return undefined;
  }
  if (normalized.startsWith('http://') || normalized.startsWith('https://')) {
    throw new Error(`${label} must be host:port, not a URL`);
  }
  return normalized;
}

export function createSurfaceHelpers(spec) {
  const surfaces = spec.surfaces ?? {};

  function getSurface(surfaceId) {
    const surface = surfaces[surfaceId];
    if (!surface) {
      throw new Error(`unknown surface id: ${surfaceId}`);
    }
    return surface;
  }

  function resolveSurfaceBind(env, surfaceId) {
    const surface = getSurface(surfaceId);
    const bindKey = surface.bindEnv;
    if (!bindKey) {
      return undefined;
    }
    return assertBind(env[bindKey], bindKey);
  }

  function resolveSurfaceHttpUrl(env, surfaceId) {
    const surface = getSurface(surfaceId);
    const urlKeys = [
      surface.httpUrlEnv,
      surface.publicHttpEnv,
      surface.clientHttpEnv,
    ].filter(Boolean);
    for (const key of urlKeys) {
      const url = assertHttpUrl(env[key], key);
      if (url) {
        return url;
      }
    }
    const bind = resolveSurfaceBind(env, surfaceId);
    if (bind) {
      return `http://${bind}`;
    }
    return undefined;
  }

  function resolveSurfaceWebsocketOrigin(env, surfaceId) {
    const surface = getSurface(surfaceId);
    const urlKeys = [
      surface.websocketUrlEnv,
      surface.publicWsEnv,
      surface.clientWebsocketEnv,
      surface.clientWsEnv,
    ].filter(Boolean);
    for (const key of urlKeys) {
      const origin = assertWebsocketOrigin(env[key], key);
      if (origin) {
        return origin;
      }
    }
    const httpUrl = resolveSurfaceHttpUrl(env, surfaceId);
    if (httpUrl) {
      const parsed = new URL(httpUrl);
      const protocol = parsed.protocol === 'https:' ? 'wss:' : 'ws:';
      return `${protocol}//${parsed.host}`;
    }
    return undefined;
  }

  function resolveSurfaceWebsocketPath(surfaceId) {
    const surface = getSurface(surfaceId);
    return surface.websocketPath ?? surface.wsPath ?? '';
  }

  function resolveSurfaceAutostart(env, surfaceId, defaultValue = true) {
    const surface = getSurface(surfaceId);
    const key = surface.autostartEnv;
    if (!key) {
      return defaultValue;
    }
    const normalized = normalizeText(env[key])?.toLowerCase();
    if (!normalized) {
      return defaultValue;
    }
    if (['1', 'true', 'on', 'yes'].includes(normalized)) {
      return true;
    }
    if (['0', 'false', 'off', 'no'].includes(normalized)) {
      return false;
    }
    return defaultValue;
  }

  return {
    getSurface,
    resolveSurfaceBind,
    resolveSurfaceHttpUrl,
    resolveSurfaceWebsocketOrigin,
    resolveSurfaceWebsocketPath,
    resolveSurfaceAutostart,
  };
}
