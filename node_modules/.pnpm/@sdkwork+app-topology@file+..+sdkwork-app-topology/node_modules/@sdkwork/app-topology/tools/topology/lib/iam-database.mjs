import { loadEnvFile, normalizeText } from './env-file.mjs';
import { isTcpPortReachable } from './postgres.mjs';

function appendQueryParam(params, name, value) {
  const normalized = String(value ?? '').trim();
  if (normalized) {
    params.set(name, normalized);
  }
}

function encodePostgresPath(databaseName) {
  return encodeURIComponent(databaseName).replaceAll('%2F', '/');
}

function buildPostgresDatabaseUrl({
  host,
  port,
  database,
  username,
  password,
  sslMode,
}) {
  const credentials = `${encodeURIComponent(username)}:${encodeURIComponent(password)}`;
  const authority = `${credentials}@${host}${port ? `:${port}` : ''}`;
  const params = new URLSearchParams();
  appendQueryParam(params, 'sslmode', sslMode);
  const query = params.toString();
  return `postgresql://${authority}/${encodePostgresPath(database)}${query ? `?${query}` : ''}`;
}

export function createIamDatabaseHelpers(spec) {
  const databaseKeys = spec.database ?? {};
  const appPrefix = databaseKeys.appPrefix ?? 'SDKWORK_APP';

  function resolveIamDatabaseEnv(env) {
    const merged = { ...env };
    const existingUrl = normalizeText(merged.SDKWORK_IAM_DATABASE_URL)
      || normalizeText(merged.SDKWORK_DATABASE_URL)
      || normalizeText(merged.DATABASE_URL)
      || normalizeText(merged.SDKWORK_CLAW_DATABASE_URL);
    if (existingUrl) {
      merged.SDKWORK_IAM_DATABASE_URL = merged.SDKWORK_IAM_DATABASE_URL || existingUrl;
      merged.SDKWORK_DATABASE_URL = merged.SDKWORK_DATABASE_URL || existingUrl;
      merged.SDKWORK_CLAW_DATABASE_URL = merged.SDKWORK_CLAW_DATABASE_URL || existingUrl;
      return merged;
    }

    const appUrlKey = databaseKeys.url ?? `${appPrefix}_DATABASE_URL`;
    const appUrl = normalizeText(merged[appUrlKey]);
    if (appUrl && (appUrl.startsWith('postgres://') || appUrl.startsWith('postgresql://'))) {
      merged.SDKWORK_IAM_DATABASE_URL = appUrl;
      merged.SDKWORK_DATABASE_URL = appUrl;
      merged.SDKWORK_CLAW_DATABASE_URL = appUrl;
      return merged;
    }

    const engineKey = databaseKeys.engine ?? `${appPrefix}_DATABASE_ENGINE`;
    const engine = normalizeText(merged[engineKey])?.toLowerCase();
    if (engine === 'postgresql' || engine === 'postgres') {
      const host = normalizeText(merged[databaseKeys.host ?? `${appPrefix}_DATABASE_HOST`]);
      const database = normalizeText(merged[databaseKeys.name ?? `${appPrefix}_DATABASE_NAME`]);
      const username = normalizeText(merged[databaseKeys.username ?? `${appPrefix}_DATABASE_USERNAME`]);
      const password = merged[databaseKeys.password ?? `${appPrefix}_DATABASE_PASSWORD`];
      if (host && database && username && password !== undefined) {
        const port = normalizeText(merged[databaseKeys.port ?? `${appPrefix}_DATABASE_PORT`]) || '5432';
        const sslMode = normalizeText(merged[databaseKeys.sslMode ?? `${appPrefix}_DATABASE_SSL_MODE`]) || 'disable';
        const url = buildPostgresDatabaseUrl({
          host,
          port,
          database,
          username,
          password: password ?? '',
          sslMode,
        });
        merged.SDKWORK_IAM_DATABASE_URL = url;
        merged.SDKWORK_DATABASE_URL = url;
        merged.SDKWORK_CLAW_DATABASE_URL = url;
        merged.SDKWORK_CLAW_DATABASE_ENGINE = 'postgresql';
        merged.SDKWORK_CLAW_DATABASE_HOST = host;
        merged.SDKWORK_CLAW_DATABASE_PORT = port;
        merged.SDKWORK_CLAW_DATABASE_NAME = database;
        merged.SDKWORK_CLAW_DATABASE_USERNAME = username;
        merged.SDKWORK_CLAW_DATABASE_PASSWORD = password ?? '';
        merged.SDKWORK_CLAW_DATABASE_SSL_MODE = sslMode;
      }
    }

    return merged;
  }

  function describeIamDatabaseTarget(env) {
    const url = normalizeText(env.SDKWORK_IAM_DATABASE_URL)
      || normalizeText(env.SDKWORK_DATABASE_URL)
      || normalizeText(env.SDKWORK_CLAW_DATABASE_URL);
    if (url) {
      try {
        const parsed = new URL(url);
        const database = decodeURIComponent(parsed.pathname.replace(/^\//u, ''));
        return `${parsed.hostname}:${parsed.port || '5432'}/${database}`;
      } catch {
        return url;
      }
    }

    const host = normalizeText(env.SDKWORK_CLAW_DATABASE_HOST)
      || normalizeText(env[databaseKeys.host ?? `${appPrefix}_DATABASE_HOST`]);
    const port = normalizeText(env.SDKWORK_CLAW_DATABASE_PORT)
      || normalizeText(env[databaseKeys.port ?? `${appPrefix}_DATABASE_PORT`])
      || '5432';
    const database = normalizeText(env.SDKWORK_CLAW_DATABASE_NAME)
      || normalizeText(env[databaseKeys.name ?? `${appPrefix}_DATABASE_NAME`]);
    if (host && database) {
      return `${host}:${port}/${database}`;
    }
    return 'unknown';
  }

  async function assertPostgresReachableForIam(env, options = {}) {
    const url = normalizeText(env.SDKWORK_IAM_DATABASE_URL)
      || normalizeText(env.SDKWORK_DATABASE_URL)
      || normalizeText(env.SDKWORK_CLAW_DATABASE_URL);
    if (!url) {
      throw new Error(
        options.missingDatabaseMessage
          ?? 'IAM requires PostgreSQL for dev login. Configure .env.postgres and start PostgreSQL.',
      );
    }

    let host = '127.0.0.1';
    let port = 5432;
    try {
      const parsed = new URL(url);
      host = parsed.hostname || host;
      port = Number(parsed.port || '5432');
    } catch {
      // keep defaults
    }

    if (!(await isTcpPortReachable(port, host))) {
      throw new Error(
        options.unreachableDatabaseMessage
          ?? `PostgreSQL is not reachable at ${host}:${port} (IAM database).`,
      );
    }
  }

  function resolveIamDevEnv(env = process.env, repoRoot, options = {}) {
    const postgresFile = options.postgresEnvFile ?? '.env.postgres';
    const postgresEnv = loadEnvFile(postgresFile, repoRoot);
    const defaults = {
      ...(options.iamDefaults ?? {}),
    };

    return resolveIamDatabaseEnv({
      ...defaults,
      ...postgresEnv,
      ...env,
    });
  }

  return {
    resolveIamDatabaseEnv,
    describeIamDatabaseTarget,
    assertPostgresReachableForIam,
    resolveIamDevEnv,
  };
}
