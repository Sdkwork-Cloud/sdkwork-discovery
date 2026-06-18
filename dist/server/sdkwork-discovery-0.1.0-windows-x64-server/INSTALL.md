# SDKWork Discovery Server Package

Package: windows-x64-server-tar-gz
Version: 0.1.0
Target: windows/x64

## Start

Copy `config/discovery.example.toml` to a host-local protected config file, then start the service host:

```sh
export SDKWORK_DISCOVERY_CONFIG_FILE=/etc/sdkwork/discovery/production.toml
.\bin\sdkwork-discovery-service-host.exe
```

## Health

When `[server].enable_health = true`, use the tonic health service on the configured gRPC bind.
