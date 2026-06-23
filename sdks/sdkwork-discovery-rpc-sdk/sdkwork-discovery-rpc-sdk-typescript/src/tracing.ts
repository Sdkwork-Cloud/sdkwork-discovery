import type { RpcMetadata } from './metadata.js';

const TRACE_ID_LENGTH = 32;
const SPAN_ID_LENGTH = 16;

function normalizeHex(value: string, expectedLength: number, label: string): string {
  const normalized = value.replace(/-/g, '').toLowerCase();
  if (!/^[0-9a-f]+$/.test(normalized) || normalized.length !== expectedLength) {
    throw new Error(`${label} must be ${expectedLength} hex characters`);
  }
  return normalized;
}

function randomSpanId(): string {
  const bytes = new Uint8Array(8);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('');
}

export function createTraceparent(traceId: string, parentSpanId?: string): string {
  const normalizedTraceId = normalizeHex(traceId, TRACE_ID_LENGTH, 'traceId');
  const normalizedSpanId = normalizeHex(
    parentSpanId ?? randomSpanId(),
    SPAN_ID_LENGTH,
    'parentSpanId',
  );
  return `00-${normalizedTraceId}-${normalizedSpanId}-01`;
}

export function createTraceparentMetadata(
  traceId: string,
  parentSpanId?: string,
): RpcMetadata {
  return {
    traceparent: createTraceparent(traceId, parentSpanId),
  };
}
