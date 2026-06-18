import net from 'node:net';

import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const http = require('http');

export function isHttpHealthy(url, path = '/healthz', timeoutMs = 2000) {
  return new Promise((resolve) => {
    let parsed;
    try {
      parsed = new URL(path, url);
    } catch {
      resolve(false);
      return;
    }
    const request = http.get(
      {
        hostname: parsed.hostname,
        port: parsed.port || (parsed.protocol === 'https:' ? 443 : 80),
        path: `${parsed.pathname}${parsed.search}`,
        timeout: timeoutMs,
      },
      (response) => {
        response.resume();
        resolve(response.statusCode >= 200 && response.statusCode < 300);
      },
    );
    request.on('error', () => resolve(false));
    request.on('timeout', () => {
      request.destroy();
      resolve(false);
    });
  });
}

export function isTcpPortOpen(host, port, timeoutMs = 1000) {
  return new Promise((resolve) => {
    const socket = net.connect({ host, port, timeout: timeoutMs });
    socket.once('connect', () => {
      socket.end();
      resolve(true);
    });
    socket.once('error', () => resolve(false));
    socket.once('timeout', () => {
      socket.destroy();
      resolve(false);
    });
  });
}

export async function waitForHttpHealthy(url, options = {}) {
  const {
    path = '/healthz',
    timeoutMs = 2000,
    attempts = 90,
    intervalMs = 1000,
  } = options;
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    if (await isHttpHealthy(url, path, timeoutMs)) {
      return true;
    }
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }
  return false;
}
