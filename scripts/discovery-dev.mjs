#!/usr/bin/env node

import { spawn } from 'node:child_process';
import process from 'node:process';

import {
  DEFAULT_DEV_PROFILE_ID,
  filterDiscoveryProcessEnv,
  isTcpPortReachable,
  listHealthSurfaces,
  listOrchestrationProcesses,
  loadProfile,
  mergeRuntimeEnv,
  REPO_ROOT,
  resolveDevProfileId,
  resolveSurfaceBind,
  resolveSurfaceGrpcUrl,
  splitHostPort,
} from './lib/discovery-topology.mjs';

const STARTUP_WAIT_MS = 500;
const MAX_STARTUP_ATTEMPTS = 60;

function cargoCommand() {
  return process.platform === 'win32' ? 'cargo.exe' : 'cargo';
}

function parseArgs(argv) {
  const settings = {
    hosting: 'self-hosted',
    serviceLayout: 'unified-process',
    dryRun: false,
    help: false,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--help' || arg === '-h') {
      settings.help = true;
      continue;
    }
    if (arg === '--hosting') {
      settings.hosting = argv[index + 1] ?? settings.hosting;
      index += 1;
      continue;
    }
    if (arg === '--service-layout') {
      settings.serviceLayout = argv[index + 1] ?? settings.serviceLayout;
      index += 1;
      continue;
    }
    if (arg === '--topology') {
      throw new Error(
        '--topology is retired; use --hosting (standalone -> self-hosted, cloud -> cloud-hosted)',
      );
    }
    if (arg === '--dry-run') {
      settings.dryRun = true;
    }
  }

  return settings;
}

function printHelp() {
  console.log(`Usage: node scripts/discovery-dev.mjs [options]

Topology-aware Discovery dev entry. Loads configs/topology profile env via @sdkwork/app-topology.

Options:
  --hosting <self-hosted|cloud-hosted>              Default: self-hosted
  --service-layout <unified-process>                Default: unified-process
  --dry-run                                         Print plan without executing
  --help, -h
`);
}

async function waitForSurfaceHealth(profileId, env) {
  const surfaces = listHealthSurfaces(profileId);
  for (const surfaceId of surfaces) {
    const bind = resolveSurfaceBind(env, surfaceId);
    if (!bind) {
      continue;
    }
    const { host, port } = splitHostPort(bind);
    if (!host || !port) {
      continue;
    }
    let ready = false;
    for (let attempt = 0; attempt < MAX_STARTUP_ATTEMPTS; attempt += 1) {
      ready = await isTcpPortReachable(Number(port), host);
      if (ready) {
        console.log(`[sdkwork-discovery] healthy ${surfaceId} (${bind})`);
        break;
      }
      await new Promise((resolve) => setTimeout(resolve, STARTUP_WAIT_MS));
    }
    if (!ready) {
      throw new Error(`timed out waiting for ${surfaceId} tcp health at ${bind}`);
    }
  }
}

async function main() {
  const settings = parseArgs(process.argv.slice(2));
  if (settings.help) {
    printHelp();
    process.exit(0);
  }

  const profileId =
    resolveDevProfileId(settings.hosting, settings.serviceLayout) || DEFAULT_DEV_PROFILE_ID;
  const profileEnv = loadProfile(profileId);
  const processes = listOrchestrationProcesses(profileId);
  const serviceProcess = processes.find((process) => process.id === 'application.public-ingress');
  if (!serviceProcess?.crate) {
    throw new Error(
      `orchestration profile ${profileId} is missing application.public-ingress crate`,
    );
  }

  const runtimeEnv = filterDiscoveryProcessEnv(
    mergeRuntimeEnv(process.env, profileEnv, {
      SDKWORK_DISCOVERY_PROFILE_ID: profileId,
    }),
  );

  const command = cargoCommand();
  const args = ['run', '-p', serviceProcess.crate];

  console.log(`[sdkwork-discovery] profile=${profileId}`);
  console.log(
    `[sdkwork-discovery] application.public-ingress=${resolveSurfaceGrpcUrl(runtimeEnv) ?? runtimeEnv.SDKWORK_DISCOVERY_APPLICATION_PUBLIC_INGRESS_BIND ?? 'unset'}`,
  );

  if (settings.dryRun) {
    console.log(`[sdkwork-discovery] dry-run ${command} ${args.join(' ')}`);
    return;
  }

  const child = spawn(command, args, {
    cwd: REPO_ROOT,
    env: runtimeEnv,
    stdio: 'inherit',
    shell: false,
    windowsHide: true,
  });

  child.on('error', (error) => {
    console.error(`[sdkwork-discovery] failed to start service host: ${error.message}`);
    process.exit(1);
  });

  child.on('exit', (code, signal) => {
    if (signal) {
      process.kill(process.pid, signal);
      return;
    }
    process.exit(code ?? 0);
  });

  await waitForSurfaceHealth(profileId, runtimeEnv);
}

main().catch((error) => {
  console.error(`[sdkwork-discovery] ${error.message}`);
  process.exit(1);
});
