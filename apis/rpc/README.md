# RPC Contracts

This directory is the RPC contract workspace for `sdkwork-discovery`.

## Canonical Proto Source

The canonical proto source is located at:

- `../../proto/sdkwork/discovery/common/v1/` - Common discovery types
- `../../proto/sdkwork/discovery/internal/v1/` - Internal registry and config services
- `../../proto/sdkwork/discovery/backend/v3/` - Backend admin services

## Proto Package Structure

```
sdkwork.discovery.common.v1    # Common types (ResponseMetadata, ServiceInstance, etc.)
sdkwork.discovery.internal.v1  # Internal services (RegistryService, DiscoveryConfigService)
sdkwork.discovery.backend.v3   # Backend admin services (DiscoveryAdminService)
```

## Related Specs

- `../../sdkwork-specs/RPC_SPEC.md` - RPC/gRPC contract rules
- `../../sdkwork-specs/RPC_SDK_WORKSPACE_SPEC.md` - RPC SDK workspace rules
