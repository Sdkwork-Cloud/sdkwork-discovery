#!/usr/bin/env node
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const sdkRoot = path.join(
  repoRoot,
  'sdks/sdkwork-discovery-rpc-sdk/sdkwork-discovery-rpc-sdk-typescript',
);
const indexSource = fs.readFileSync(path.join(sdkRoot, 'src/index.ts'), 'utf8');
const helperFiles = [
  'metadata.ts',
  'deadline.ts',
  'idempotency.ts',
  'pagination.ts',
  'tracing.ts',
];

for (const fileName of helperFiles) {
  assert.ok(
    fs.existsSync(path.join(sdkRoot, 'src', fileName)),
    `missing TypeScript SDK helper ${fileName}`,
  );
  assert.match(indexSource, new RegExp(`['"]\\./${fileName.replace('.ts', '.js')}['"]`));
}

const requiredExports = [
  'createStaticMetadataProvider',
  'mergeRpcMetadata',
  'resolveRpcDeadlineMs',
  'createRpcIdempotencyMetadata',
  'createDiscoveryPageRequest',
  'nextDiscoveryPageToken',
  'createTraceparent',
  'createTraceparentMetadata',
  'RPC_SDK_PROTOCOL',
  'RPC_SDK_FAMILY',
];

for (const exportName of requiredExports) {
  const sourceFile = fs
    .readdirSync(path.join(sdkRoot, 'src'))
    .find((fileName) => {
      const source = fs.readFileSync(path.join(sdkRoot, 'src', fileName), 'utf8');
      return source.includes(`export function ${exportName}`) || source.includes(`export const ${exportName}`);
    });
  assert.ok(sourceFile, `missing TypeScript SDK export ${exportName}`);
}

const rustLib = fs.readFileSync(
  path.join(repoRoot, 'sdks/sdkwork-discovery-rpc-sdk/sdkwork-discovery-rpc-sdk-rust/src/lib.rs'),
  'utf8',
);
for (const moduleName of ['deadline', 'idempotency', 'pagination', 'tracing']) {
  assert.match(rustLib, new RegExp(`pub mod ${moduleName};`));
}
