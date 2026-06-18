import net from 'node:net';

export const DEFAULT_POSTGRES_REACHABILITY_TIMEOUT_MS = 2000;

export function isTcpPortReachable(
  port,
  host = '127.0.0.1',
  timeoutMs = DEFAULT_POSTGRES_REACHABILITY_TIMEOUT_MS,
) {
  if (!Number.isFinite(port)) {
    return Promise.resolve(false);
  }

  return new Promise((resolve) => {
    const socket = net.connect({ host, port, timeout: timeoutMs });
    const finish = (reachable) => {
      socket.destroy();
      resolve(reachable);
    };
    socket.once('connect', () => finish(true));
    socket.once('error', () => finish(false));
    socket.once('timeout', () => finish(false));
  });
}
