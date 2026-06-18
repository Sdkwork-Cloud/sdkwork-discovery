export interface RpcDeadlineOptions {
  timeoutMs?: number;
  deadline?: Date;
  now?: () => number;
}

export function resolveRpcDeadlineMs(options: RpcDeadlineOptions = {}): number | undefined {
  if (typeof options.timeoutMs === 'number') {
    return options.timeoutMs;
  }
  if (!options.deadline) {
    return undefined;
  }
  const now = options.now ? options.now() : Date.now();
  return Math.max(0, options.deadline.getTime() - now);
}
