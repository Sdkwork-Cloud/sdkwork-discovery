# RUNBOOK: Production Server Deployment

Status: active  
Owner: SDKWork Discovery platform  
Application: sdkwork-discovery  
Updated: 2026-06-23  
Specs: DOCUMENTATION_SPEC.md, CONFIG_SPEC.md, OBSERVABILITY_SPEC.md, SECURITY_SPEC.md

## Purpose

Deploy the Discovery gRPC control plane for production registry, config, and watch traffic using the packaged server binary or container image.

## Preconditions

- PostgreSQL durable storage is provisioned and reachable.
- Database migrations are applied before serving traffic (`pnpm db:migrate` or pipeline equivalent).
- Secret files exist on the host for:
  - TLS server certificate and key
  - Service-token HMAC secret
  - PostgreSQL password file
- Production policy rejects: memory/sqlite storage, loopback-only bind, unsigned local context, enabled gRPC reflection, and automatic schema application.

## Configuration

1. Copy `etc/discovery.production.example.toml` to a host-local path (for example `/etc/sdkwork/discovery/production.toml`).
2. Mount secret files at the paths referenced in the config.
3. Load topology env from `configs/topology/self-hosted.unified-process.production.env` or `configs/topology/cloud-hosted.unified-process.production.env` as appropriate.

Required process env:

```bash
export SDKWORK_DISCOVERY_CONFIG_FILE=/etc/sdkwork/discovery/production.toml
export SDKWORK_DISCOVERY_METRICS_BIND=0.0.0.0:9090
```

Release packages are built with the `prometheus` feature. Development-only smoke tests may use `etc/discovery.example.toml` on loopback with unsigned local context enabled.

## Deploy

From a release archive (`dist/server/sdkwork-discovery-*-server.tar.gz`):

```bash
tar -xzf sdkwork-discovery-*-linux-x64-server.tar.gz
export SDKWORK_DISCOVERY_CONFIG_FILE=/etc/sdkwork/discovery/production.toml
export SDKWORK_DISCOVERY_METRICS_BIND=0.0.0.0:9090
./bin/sdkwork-discovery-service-host
```

See `INSTALL.md` inside the package for checksum validation and layout.

## Signals

| Signal | Source | Healthy |
| --- | --- | --- |
| gRPC Health | tonic health on configured bind when `[server].enable_health = true` | `SERVING` |
| Process gauge | Prometheus `discovery_health_status` | `1` while serving, `0` during shutdown |
| RPC traffic | `discovery_rpc_requests_total`, `discovery_rpc_errors_total` | Stable error ratio for workload |
| Auth failures | `discovery_rpc_auth_failures_total` | No sustained spike without known rollout |
| Watch capacity | `discovery_rpc_errors_total{error_type="watch_stream_limit"}` | Near zero under normal load |

Scrape `SDKWORK_DISCOVERY_METRICS_BIND` (default `127.0.0.1:9090` when unset).

## Graceful shutdown

The process handles **SIGTERM** and **Ctrl+C**:

1. Sets `discovery_health_status` to `0`
2. Marks gRPC health services `NOT_SERVING`
3. Stops the tonic server

Orchestrators should use SIGTERM and allow enough time for in-flight unary RPCs and watch streams to drain.

## Rollback

1. Stop the current process (SIGTERM).
2. Restore the previous server binary or container image.
3. Restore the previous host-local config and secret mounts if they changed.
4. Do **not** roll back applied database migrations without following `RUNBOOK-database-migration-rollback.md`.
5. Verify gRPC Health and a smoke register/discover call before restoring traffic.

## Escalation

- Storage/bootstrap failures: check PostgreSQL connectivity, migration status (`pnpm db:status`), and secret file mounts.
- Auth failures: verify service-token HMAC secret, issuer, audience, and client `authorization` + `access-token` metadata.
- Watch `ResourceExhausted`: increase `[watch].max_streams` or scale sticky stream endpoints; clients should reconnect from last revision.

## Verification

```bash
pnpm run verify
pnpm run package:server:validate
```
