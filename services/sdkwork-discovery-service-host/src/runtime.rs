use std::collections::BTreeMap;
use std::fs;

use sdkwork_discovery_contract::{DiscoveryError, DiscoveryResult};
use sdkwork_discovery_rpc::DiscoveryRpcServerConfig;

use crate::bootstrap::{DiscoveryServiceHostBootstrap, DiscoveryServiceHostGrpcServer};

pub struct DiscoveryServiceHostRuntime {
    bootstrap: DiscoveryServiceHostBootstrap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryRuntimeOptions {
    pub config_file: Option<String>,
    pub env_overlay: BTreeMap<String, String>,
}

impl DiscoveryServiceHostRuntime {
    pub fn options_from_env(
        env: &BTreeMap<String, String>,
    ) -> DiscoveryResult<DiscoveryRuntimeOptions> {
        let mut env_overlay = BTreeMap::new();
        let config_file = env.get("SDKWORK_DISCOVERY_CONFIG_FILE").cloned();

        for (key, value) in env {
            if key == "SDKWORK_DISCOVERY_CONFIG_FILE" {
                continue;
            }

            if is_retired_database_env_key(key) {
                return Err(DiscoveryError::InvalidConfig(format!(
                    "retired database env key {key}; use SDKWORK_DATABASE_* as the only database authority"
                )));
            }

            if key.starts_with("SDKWORK_DATABASE_ADMIN_") {
                continue;
            }

            if key.starts_with("SDKWORK_DISCOVERY_") || key.starts_with("SDKWORK_DATABASE_") {
                env_overlay.insert(key.clone(), value.clone());
            }
        }

        Ok(DiscoveryRuntimeOptions {
            config_file,
            env_overlay,
        })
    }

    pub fn from_toml_str_with_env(
        input: &str,
        env: &BTreeMap<String, String>,
    ) -> DiscoveryResult<Self> {
        let options = Self::options_from_env(env)?;
        let bootstrap =
            DiscoveryServiceHostBootstrap::from_toml_str_with_env(input, &options.env_overlay)?;
        Ok(Self { bootstrap })
    }

    pub fn from_process_env() -> DiscoveryResult<Self> {
        let env = std::env::vars().collect::<BTreeMap<_, _>>();
        let options = Self::options_from_env(&env)?;
        let config_file = options
            .config_file
            .as_deref()
            .unwrap_or("etc/discovery.example.toml");
        let config = fs::read_to_string(config_file).map_err(|error| {
            DiscoveryError::InvalidConfig(format!(
                "failed to read discovery config file {config_file}: {error}"
            ))
        })?;
        let bootstrap =
            DiscoveryServiceHostBootstrap::from_toml_str_with_env(&config, &options.env_overlay)?;
        Ok(Self { bootstrap })
    }

    pub fn safe_summary(&self) -> String {
        let config = self.bootstrap.config();
        format!(
            "sdkwork-discovery provider={} grpc={}:{} admin_grpc={}:{} environment={} storage=\"{}\"",
            self.bootstrap.storage_provider_name(),
            config.server.grpc_bind_host,
            config.server.grpc_port,
            config.server.grpc_bind_host,
            config.server.admin_grpc_port,
            config.runtime.environment.as_str(),
            self.bootstrap.storage_safe_summary()
        )
    }

    pub fn bootstrap(&self) -> &DiscoveryServiceHostBootstrap {
        &self.bootstrap
    }

    pub fn bootstrap_mut(&mut self) -> &mut DiscoveryServiceHostBootstrap {
        &mut self.bootstrap
    }

    pub fn internal_rpc_server_config(&self) -> DiscoveryResult<DiscoveryRpcServerConfig> {
        self.bootstrap.internal_rpc_server_config()
    }

    pub fn backend_rpc_server_config(&self) -> DiscoveryResult<DiscoveryRpcServerConfig> {
        self.bootstrap.backend_rpc_server_config()
    }

    pub async fn serve_grpc(mut self) -> DiscoveryResult<DiscoveryServiceHostGrpcServer> {
        self.bootstrap.serve_grpc().await
    }
}

fn is_retired_database_env_key(key: &str) -> bool {
    key.starts_with("SDKWORK_DISCOVERY_STORAGE_POSTGRES_")
        || key.starts_with("SDKWORK_DISCOVERY_STORAGE_SQLITE_")
        || (key.starts_with("SDKWORK_")
            && !key.starts_with("SDKWORK_DATABASE_")
            && key.contains("_DATABASE_"))
}
