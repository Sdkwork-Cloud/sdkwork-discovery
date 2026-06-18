export type RpcMetadata = Record<string, string>;

export interface RpcMetadataProviderContext {
  service?: string;
  method?: string;
  operationId?: string;
}

export type RpcMetadataProvider = (context: RpcMetadataProviderContext) => RpcMetadata | Promise<RpcMetadata>;

export function createStaticMetadataProvider(metadata: RpcMetadata): RpcMetadataProvider {
  return () => ({ ...metadata });
}

export function mergeRpcMetadata(...metadata: Array<RpcMetadata | undefined>): RpcMetadata {
  const merged: RpcMetadata = {};
  for (const item of metadata) {
    if (!item) {
      continue;
    }
    for (const [key, value] of Object.entries(item)) {
      if (value !== undefined && value !== null && value !== '') {
        merged[key] = value;
      }
    }
  }
  return merged;
}
