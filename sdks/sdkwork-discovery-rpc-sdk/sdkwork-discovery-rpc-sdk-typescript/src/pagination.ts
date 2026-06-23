import { create } from '@bufbuild/protobuf';
import type {
  PageRequest,
  PageResponse,
} from '../generated/proto/sdkwork/discovery/common/v1/discovery_types_pb.js';
import {
  PageRequestSchema,
} from '../generated/proto/sdkwork/discovery/common/v1/discovery_types_pb.js';

export const DEFAULT_DISCOVERY_PAGE_SIZE = 100;
export const MAX_DISCOVERY_PAGE_SIZE = 200;

export interface DiscoveryPageParams {
  pageSize?: number;
  pageToken?: string;
}

export function clampDiscoveryPageSize(pageSize: number): number {
  if (pageSize <= 0) {
    return DEFAULT_DISCOVERY_PAGE_SIZE;
  }
  return Math.min(pageSize, MAX_DISCOVERY_PAGE_SIZE);
}

export function createDiscoveryPageRequest(params: DiscoveryPageParams = {}): PageRequest {
  return create(PageRequestSchema, {
    pageSize: params.pageSize ?? 0,
    pageToken: params.pageToken ?? '',
  });
}

export function nextDiscoveryPageToken(page?: PageResponse): string | undefined {
  const token = page?.nextPageToken?.trim();
  return token ? token : undefined;
}
