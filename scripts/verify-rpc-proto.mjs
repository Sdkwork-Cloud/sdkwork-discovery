#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const protoRoot = path.join(root, 'proto');
const rustGenerated = path.join(
  root,
  'sdks/sdkwork-discovery-rpc-sdk/sdkwork-discovery-rpc-sdk-rust/generated/sdkwork.discovery.common.v1.rs',
);
const tsGeneratedDir = path.join(
  root,
  'sdks/sdkwork-discovery-rpc-sdk/sdkwork-discovery-rpc-sdk-typescript/generated/proto',
);

function fail(message) {
  console.error(`verify:rpc-proto: ${message}`);
  process.exit(1);
}

if (!fs.existsSync(path.join(protoRoot, 'buf.yaml'))) {
  fail('proto/buf.yaml is required');
}

const lint = spawnSync(
  process.platform === 'win32' ? 'npx.cmd' : 'npx',
  ['--yes', '@bufbuild/buf', 'lint', protoRoot],
  { cwd: root, stdio: 'inherit', shell: process.platform === 'win32' },
);
if (lint.status !== 0) {
  fail('buf lint failed');
}

if (!fs.existsSync(rustGenerated)) {
  fail(`missing Rust SDK generated proto: ${rustGenerated}`);
}
const rustSource = fs.readFileSync(rustGenerated, 'utf8');
for (const marker of ['PageRequest', 'PageResponse', 'next_page_token']) {
  if (!rustSource.includes(marker)) {
    fail(`Rust SDK generated proto is missing ${marker}`);
  }
}

if (!fs.existsSync(tsGeneratedDir)) {
  fail(`missing TypeScript generated proto directory: ${tsGeneratedDir}`);
}
const tsTypes = path.join(
  tsGeneratedDir,
  'sdkwork/discovery/common/v1/discovery_types_pb.ts',
);
if (!fs.existsSync(tsTypes)) {
  fail(`missing TypeScript generated proto types: ${tsTypes}`);
}
const tsSource = fs.readFileSync(tsTypes, 'utf8');
for (const marker of ['PageRequest', 'PageResponse', 'nextPageToken']) {
  if (!tsSource.includes(marker)) {
    fail(`TypeScript SDK generated proto is missing ${marker}`);
  }
}

process.stdout.write('verify:rpc-proto: buf lint and SDK proto markers passed\n');
