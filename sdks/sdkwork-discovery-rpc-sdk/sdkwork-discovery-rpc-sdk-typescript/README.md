# SdkworkDiscoveryRpc

SdkworkDiscoveryRpc is an SDKWork RPC SDK scaffold generated from proto packages and an SDKWork RPC manifest.

## Proto packages

- sdkwork.discovery.backend.v3
- sdkwork.discovery.internal.v1

## Service catalog

- sdkwork.discovery.internal.v1.RegistryService (internal)
  - RegisterInstance: discovery.registry.instances.register, unary, auth=service-identity, idempotency=optional
  - BatchRegisterInstances: discovery.registry.instances.batch_register, unary, auth=service-identity, idempotency=optional
  - RenewLease: discovery.registry.leases.renew, unary, auth=service-identity, idempotency=optional
  - DeregisterInstance: discovery.registry.instances.deregister, unary, auth=service-identity, idempotency=optional
  - ReportInstanceStatus: discovery.registry.instances.status.report, unary, auth=service-identity, idempotency=optional
  - RetrieveInstance: discovery.registry.instances.retrieve, unary, auth=service-identity, idempotency=none
  - DiscoverInstances: discovery.registry.instances.discover, unary, auth=service-identity, idempotency=none
- sdkwork.discovery.internal.v1.DiscoveryConfigService (internal)
  - RetrieveEffectiveConfig: discovery.config.effective.retrieve, unary, auth=service-identity, idempotency=none
  - WatchConfig: discovery.config.releases.watch, server, auth=service-identity, idempotency=none
- sdkwork.discovery.internal.v1.DiscoveryWatchService (internal)
  - WatchService: discovery.registry.services.watch, server, auth=service-identity, idempotency=none
- sdkwork.discovery.backend.v3.DiscoveryAdminService (backend)
  - CreateConfigDraft: discovery.config.drafts.create, unary, auth=backend-operator, idempotency=required
  - PublishConfig: discovery.config.releases.publish, unary, auth=backend-operator, idempotency=required
  - RollbackConfig: discovery.config.releases.rollback, unary, auth=backend-operator, idempotency=required
  - ListServices: discovery.registry.services.list, unary, auth=backend-operator, idempotency=none

## Endpoint and TLS/mTLS

Configure the endpoint through application SDK bootstrap. Use TLS for protected remote endpoints and mTLS when the deployment policy requires client certificates.

## Metadata auth

Use metadata providers for authorization, access-token, traceparent, idempotency-key, and x-request-hash. Application code should inject providers through SDK bootstrap instead of assembling raw metadata in business modules.

## Deadline and cancellation

Set a deadline for each RPC call through the generated deadline helpers or the language transport options. Callers should pass cancellation through the platform-native signal when available.

## Unary call example

```ts
import { createRpcIdempotencyMetadata, createStaticMetadataProvider, resolveRpcDeadlineMs } from './src/index.js';

const metadataProvider = createStaticMetadataProvider({
  authorization: 'Bearer <auth-token>',
  'access-token': '<access-token>',
  'idempotency-key': 'create-message-001',
});
const deadlineMs = resolveRpcDeadlineMs({ timeoutMs: 5000 });
const idempotencyMetadata = createRpcIdempotencyMetadata({ idempotencyKey: 'create-message-001' });
// Call RegistryService.RegisterInstance with metadataProvider, idempotencyMetadata, and deadlineMs using the generated protobuf client.
```

## Regeneration evidence

RPC generation defaults to convention-first source output and does not write persisted generator evidence in normal generated language workspaces.

Use `sdkgen inspect --protocol rpc` to verify the RPC SDK family name, language workspace name, RPC manifest, proto source reference, generated client files, and native package manifest. Add `--emit-control-plane` only when release, CI, audit, or migration workflows need persisted generator evidence; the evidence paths are derived by generator convention.

## Verification commands

- buf lint
- buf breaking
- sdkgen generate --protocol rpc --dry-run
- run the generated client compile command for this language
