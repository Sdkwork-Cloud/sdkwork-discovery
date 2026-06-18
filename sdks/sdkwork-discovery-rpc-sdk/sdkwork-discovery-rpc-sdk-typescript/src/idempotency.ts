export interface RpcIdempotencyOptions {
  idempotencyKey?: string;
  requestHash?: string;
}

export function createRpcIdempotencyMetadata(options: RpcIdempotencyOptions = {}): Record<string, string> {
  const metadata: Record<string, string> = {};
  if (options.idempotencyKey) {
    metadata['idempotency-key'] = options.idempotencyKey;
  }
  if (options.requestHash) {
    metadata['x-request-hash'] = options.requestHash;
  }
  return metadata;
}
