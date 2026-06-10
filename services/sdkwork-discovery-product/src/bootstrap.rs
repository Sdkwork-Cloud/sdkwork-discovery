use std::collections::BTreeMap;
use std::fs;

use sdkwork_discovery_config::{DiscoveryRuntimeConfig, StorageProvider};
use sdkwork_discovery_core::{ConfigPolicy, DiscoveryControlPlane, RegistryPolicy};
use sdkwork_discovery_rpc::{
    DiscoveryRpcRuntime, DiscoveryRpcRuntimeConfig, DiscoveryRpcServerConfig,
    DiscoveryRpcServerHandle, DiscoveryRpcServiceTokenVerifierConfig, DiscoveryRpcServices,
    DiscoveryRpcTlsIdentity,
};
use sdkwork_discovery_storage_contract::{ConfigStore, RegistryStore, WatchEventStore};
use sdkwork_discovery_storage_memory::MemoryDiscoveryStore;
use sdkwork_discovery_storage_postgres::PostgresDiscoveryStore;
use sdkwork_discovery_storage_sqlite::SqliteDiscoveryStore;

pub struct DiscoveryProductBootstrap {
    config: DiscoveryRuntimeConfig,
    runtime_storage: DiscoveryRuntimeStorage,
}

pub struct DiscoveryProductGrpcServer {
    handles: Vec<DiscoveryRpcServerHandle>,
}

const MIN_SERVICE_TOKEN_HMAC_SECRET_BYTES: usize = 32;

enum DiscoveryRuntimeStorage {
    Memory {
        control_plane: Option<DiscoveryControlPlane<MemoryDiscoveryStore>>,
    },
    Postgres {
        control_plane: Option<DiscoveryControlPlane<PostgresDiscoveryStore>>,
        safe_summary: String,
    },
    Sqlite {
        control_plane: Option<DiscoveryControlPlane<SqliteDiscoveryStore>>,
        safe_summary: String,
    },
}

impl DiscoveryProductBootstrap {
    pub fn from_toml_str_with_env(
        input: &str,
        env: &BTreeMap<String, String>,
    ) -> sdkwork_discovery_contract::DiscoveryResult<Self> {
        let config = DiscoveryRuntimeConfig::from_toml_str_with_env(input, env)?;
        let runtime_storage = build_runtime_storage(&config)?;

        Ok(Self {
            config,
            runtime_storage,
        })
    }

    pub fn config(&self) -> &DiscoveryRuntimeConfig {
        &self.config
    }

    pub fn storage_provider_name(&self) -> &'static str {
        self.config.storage.provider.as_str()
    }

    pub fn storage_safe_summary(&self) -> String {
        match &self.runtime_storage {
            DiscoveryRuntimeStorage::Memory { .. } => "memory".to_string(),
            DiscoveryRuntimeStorage::Postgres { safe_summary, .. } => safe_summary.clone(),
            DiscoveryRuntimeStorage::Sqlite { safe_summary, .. } => safe_summary.clone(),
        }
    }

    pub fn internal_rpc_server_config(
        &self,
    ) -> sdkwork_discovery_contract::DiscoveryResult<DiscoveryRpcServerConfig> {
        rpc_server_config(
            &self.config,
            self.config.server.grpc_port,
            self.config.server.enable_reflection,
        )
    }

    pub fn backend_rpc_server_config(
        &self,
    ) -> sdkwork_discovery_contract::DiscoveryResult<DiscoveryRpcServerConfig> {
        rpc_server_config(
            &self.config,
            self.config.server.admin_grpc_port,
            self.config.server.enable_reflection,
        )
    }

    pub async fn initialize_storage(&self) -> sdkwork_discovery_contract::DiscoveryResult<()> {
        if !self.config.storage.apply_initial_schema {
            return Ok(());
        }

        match &self.runtime_storage {
            DiscoveryRuntimeStorage::Memory { .. } => Ok(()),
            DiscoveryRuntimeStorage::Postgres {
                control_plane: Some(control_plane),
                ..
            } => control_plane.store().apply_initial_schema().await,
            DiscoveryRuntimeStorage::Postgres {
                control_plane: None,
                ..
            } => Err(sdkwork_discovery_contract::DiscoveryError::InvalidConfig(
                "postgres control plane has already been moved into the gRPC runtime".to_string(),
            )),
            DiscoveryRuntimeStorage::Sqlite {
                control_plane: Some(control_plane),
                ..
            } => control_plane.store().apply_initial_schema().await,
            DiscoveryRuntimeStorage::Sqlite {
                control_plane: None,
                ..
            } => Err(sdkwork_discovery_contract::DiscoveryError::InvalidConfig(
                "sqlite control plane has already been moved into the gRPC runtime".to_string(),
            )),
        }
    }

    pub async fn serve_grpc(
        &mut self,
    ) -> sdkwork_discovery_contract::DiscoveryResult<DiscoveryProductGrpcServer> {
        let internal_config = rpc_server_config(
            &self.config,
            self.config.server.grpc_port,
            self.config.server.enable_reflection,
        )?;
        let backend_config = rpc_server_config(
            &self.config,
            self.config.server.admin_grpc_port,
            self.config.server.enable_reflection,
        )?;
        let runtime_config = rpc_runtime_config(&self.config)?;

        match &mut self.runtime_storage {
            DiscoveryRuntimeStorage::Memory { control_plane } => {
                let control_plane = control_plane.take().ok_or_else(|| {
                    sdkwork_discovery_contract::DiscoveryError::InvalidConfig(
                        "memory control plane has already been moved into the gRPC runtime"
                            .to_string(),
                    )
                })?;
                let runtime = DiscoveryRpcRuntime::with_config(control_plane, runtime_config);
                serve_runtime(internal_config, backend_config, runtime).await
            }
            DiscoveryRuntimeStorage::Postgres { control_plane, .. } => {
                let control_plane = control_plane.take().ok_or_else(|| {
                    sdkwork_discovery_contract::DiscoveryError::InvalidConfig(
                        "postgres control plane has already been moved into the gRPC runtime"
                            .to_string(),
                    )
                })?;
                let runtime = DiscoveryRpcRuntime::with_config(control_plane, runtime_config);
                serve_runtime(internal_config, backend_config, runtime).await
            }
            DiscoveryRuntimeStorage::Sqlite { control_plane, .. } => {
                let control_plane = control_plane.take().ok_or_else(|| {
                    sdkwork_discovery_contract::DiscoveryError::InvalidConfig(
                        "sqlite control plane has already been moved into the gRPC runtime"
                            .to_string(),
                    )
                })?;
                let runtime = DiscoveryRpcRuntime::with_config(control_plane, runtime_config);
                serve_runtime(internal_config, backend_config, runtime).await
            }
        }
    }
}

impl DiscoveryProductGrpcServer {
    pub fn bound_server_count(&self) -> usize {
        self.handles.len()
    }

    pub async fn shutdown(self) {
        for handle in self.handles {
            handle.shutdown().await;
        }
    }
}

fn build_runtime_storage(
    config: &DiscoveryRuntimeConfig,
) -> sdkwork_discovery_contract::DiscoveryResult<DiscoveryRuntimeStorage> {
    match config.storage.provider {
        StorageProvider::Memory => Ok(DiscoveryRuntimeStorage::Memory {
            control_plane: Some(DiscoveryControlPlane::new(
                MemoryDiscoveryStore::new(),
                config_policy(config),
                registry_policy(config),
            )),
        }),
        StorageProvider::Postgres => {
            let transport = config.storage.postgres.as_ref().ok_or_else(|| {
                sdkwork_discovery_contract::DiscoveryError::InvalidConfig(
                    "storage provider postgres requires [storage.postgres]".to_string(),
                )
            })?;
            let store = PostgresDiscoveryStore::new_lazy(transport, None)?;
            let safe_summary = store.safe_summary();
            Ok(DiscoveryRuntimeStorage::Postgres {
                control_plane: Some(DiscoveryControlPlane::new(
                    store,
                    config_policy(config),
                    registry_policy(config),
                )),
                safe_summary,
            })
        }
        StorageProvider::Sqlite => {
            let sqlite = config.storage.sqlite.as_ref().ok_or_else(|| {
                sdkwork_discovery_contract::DiscoveryError::InvalidConfig(
                    "storage provider sqlite requires [storage.sqlite]".to_string(),
                )
            })?;
            let store = SqliteDiscoveryStore::new_lazy(&sqlite.file, sqlite.max_connections)?;
            let safe_summary = store.safe_summary().to_string();
            Ok(DiscoveryRuntimeStorage::Sqlite {
                control_plane: Some(DiscoveryControlPlane::new(
                    store,
                    config_policy(config),
                    registry_policy(config),
                )),
                safe_summary,
            })
        }
        _ => Err(sdkwork_discovery_contract::DiscoveryError::InvalidConfig(
            format!(
                "storage provider {} is configured but the adapter is not implemented",
                config.storage.provider.as_str()
            ),
        )),
    }
}

async fn serve_runtime<S>(
    internal_config: DiscoveryRpcServerConfig,
    backend_config: DiscoveryRpcServerConfig,
    runtime: DiscoveryRpcRuntime<S>,
) -> sdkwork_discovery_contract::DiscoveryResult<DiscoveryProductGrpcServer>
where
    S: ConfigStore + RegistryStore + WatchEventStore + Send + Sync + 'static,
{
    if internal_config.bind_addr == backend_config.bind_addr {
        let handle =
            DiscoveryRpcServerHandle::serve(internal_config, DiscoveryRpcServices::new(runtime))
                .await?;
        return Ok(DiscoveryProductGrpcServer {
            handles: vec![handle],
        });
    }

    let internal = DiscoveryRpcServerHandle::serve_internal(
        internal_config,
        DiscoveryRpcServices::new(runtime.clone()),
    )
    .await?;
    let backend =
        DiscoveryRpcServerHandle::serve_backend(backend_config, DiscoveryRpcServices::new(runtime))
            .await?;

    Ok(DiscoveryProductGrpcServer {
        handles: vec![internal, backend],
    })
}

fn rpc_server_config(
    config: &DiscoveryRuntimeConfig,
    port: u16,
    enable_reflection: bool,
) -> sdkwork_discovery_contract::DiscoveryResult<DiscoveryRpcServerConfig> {
    let tls_identity = read_tls_identity(config)?;
    let client_ca_certificate_pem = read_client_ca_certificate(config)?;

    Ok(DiscoveryRpcServerConfig {
        bind_addr: format!("{}:{}", config.server.grpc_bind_host, port),
        enable_health: config.server.enable_health,
        enable_reflection,
        default_deadline_ms: config.server.default_deadline_ms,
        watch_enabled: config.watch.enabled,
        watch_max_streams: config.watch.max_streams,
        watch_event_buffer_size: config.watch.event_buffer_size,
        watch_heartbeat_interval_ms: config.watch.heartbeat_interval_ms,
        watch_durable_poll_interval_ms: config.watch.durable_poll_interval_ms,
        watch_durable_replay_batch_size: config.watch.durable_replay_batch_size,
        require_tls: config.security.tls_enabled || config.security.mtls_enabled,
        tls_identity,
        client_ca_certificate_pem,
    })
}

fn rpc_runtime_config(
    config: &DiscoveryRuntimeConfig,
) -> sdkwork_discovery_contract::DiscoveryResult<DiscoveryRpcRuntimeConfig> {
    Ok(DiscoveryRpcRuntimeConfig {
        registry_expiry_scan_interval_ms: config.registry.expiry_scan_interval_ms,
        registry_expiry_scan_batch_size: config.registry.expiry_scan_batch_size,
        allow_unsigned_local_context: config.security.allow_unsigned_local_context,
        service_token_verifier: service_token_verifier_config(config)?,
    })
}

fn service_token_verifier_config(
    config: &DiscoveryRuntimeConfig,
) -> sdkwork_discovery_contract::DiscoveryResult<Option<DiscoveryRpcServiceTokenVerifierConfig>> {
    let Some(secret_file) = config.security.service_token.hmac_secret_file.as_deref() else {
        return Ok(None);
    };

    Ok(Some(DiscoveryRpcServiceTokenVerifierConfig {
        hmac_secret: read_service_token_hmac_secret(secret_file)?,
        issuer: config.security.service_token.issuer.clone(),
        audience: config.security.service_token.audience.clone(),
        max_token_ttl_seconds: config.security.service_token.max_token_ttl_seconds,
    }))
}

fn read_tls_identity(
    config: &DiscoveryRuntimeConfig,
) -> sdkwork_discovery_contract::DiscoveryResult<Option<DiscoveryRpcTlsIdentity>> {
    let Some(certificate_file) = config.security.server_tls_cert_file.as_deref() else {
        return Ok(None);
    };
    let Some(private_key_file) = config.security.server_tls_key_file.as_deref() else {
        return Ok(None);
    };

    Ok(Some(DiscoveryRpcTlsIdentity {
        certificate_pem: read_pem_file("TLS server certificate", certificate_file)?,
        private_key_pem: read_pem_file("TLS server private key", private_key_file)?,
    }))
}

fn read_client_ca_certificate(
    config: &DiscoveryRuntimeConfig,
) -> sdkwork_discovery_contract::DiscoveryResult<Option<Vec<u8>>> {
    let Some(certificate_file) = config.security.client_ca_cert_file.as_deref() else {
        return Ok(None);
    };

    read_pem_file("mTLS client CA certificate", certificate_file).map(Some)
}

fn read_pem_file(label: &str, path: &str) -> sdkwork_discovery_contract::DiscoveryResult<Vec<u8>> {
    fs::read(path).map_err(|error| {
        sdkwork_discovery_contract::DiscoveryError::InvalidConfig(format!(
            "{label} file could not be read: {error}"
        ))
    })
}

fn read_service_token_hmac_secret(
    path: &str,
) -> sdkwork_discovery_contract::DiscoveryResult<Vec<u8>> {
    let mut secret = fs::read(path).map_err(|error| {
        sdkwork_discovery_contract::DiscoveryError::InvalidConfig(format!(
            "service-token HMAC secret file could not be read: {error}"
        ))
    })?;

    while secret.last().is_some_and(u8::is_ascii_whitespace) {
        secret.pop();
    }

    if secret.len() < MIN_SERVICE_TOKEN_HMAC_SECRET_BYTES {
        return Err(sdkwork_discovery_contract::DiscoveryError::InvalidConfig(format!(
            "service-token HMAC secret file must contain at least {MIN_SERVICE_TOKEN_HMAC_SECRET_BYTES} bytes"
        )));
    }

    Ok(secret)
}

fn config_policy(config: &DiscoveryRuntimeConfig) -> ConfigPolicy {
    ConfigPolicy {
        enabled: config.config_registry.enabled,
        require_publish_for_reads: config.config_registry.require_publish_for_reads,
        allow_secret_values: config.config_registry.allow_secret_values,
        allow_secret_refs: config.config_registry.allow_secret_refs,
        max_config_body_bytes: config.config_registry.max_config_body_bytes,
    }
}

fn registry_policy(config: &DiscoveryRuntimeConfig) -> RegistryPolicy {
    RegistryPolicy {
        default_lease_ttl_seconds: config.registry.default_lease_ttl_seconds,
        min_lease_ttl_seconds: config.registry.min_lease_ttl_seconds,
        max_lease_ttl_seconds: config.registry.max_lease_ttl_seconds,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_runtime_storage_owns_control_plane() {
        fn assert_postgres_variant(storage: &DiscoveryRuntimeStorage) {
            match storage {
                DiscoveryRuntimeStorage::Postgres {
                    control_plane: Some(_),
                    ..
                } => {}
                _ => panic!("postgres storage must own a discovery control plane"),
            }
        }

        let _ = assert_postgres_variant;
    }
}
