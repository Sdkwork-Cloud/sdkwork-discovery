use sdkwork_discovery_contract::{
    CallerContext, HealthCheckProbe, InstanceStatus, RegistryPermission,
    ReportInstanceStatusCommand,
};
use sdkwork_discovery_core::DiscoveryControlPlane;
use sdkwork_discovery_health_checker::{check_health, HealthCheckProbe as CheckerProbe};
use sdkwork_discovery_storage_contract::{ConfigStore, RegistryStore};

pub async fn run_health_checks<S>(control_plane: &mut DiscoveryControlPlane<S>, now_ms: u64)
where
    S: RegistryStore + ConfigStore,
{
    let instances = match control_plane
        .store()
        .list_active_instances_with_health_check(now_ms)
        .await
    {
        Ok(instances) => instances,
        Err(_) => return,
    };

    for instance in instances {
        let Some(config) = instance.health_check.clone() else {
            continue;
        };
        if !instance.health_check_state.due_for_check(&config, now_ms) {
            continue;
        }

        let probe = to_checker_probe(&config.probe);
        let result = check_health(&instance.endpoint, &probe, config.timeout_ms).await;
        let mut state = instance.health_check_state.clone();
        if result.healthy {
            state.consecutive_successes += 1;
            state.consecutive_failures = 0;
        } else {
            state.consecutive_failures += 1;
            state.consecutive_successes = 0;
        }
        state.last_check_ms = now_ms;

        let _ = control_plane
            .store_mut()
            .update_health_check_state(
                &instance.namespace,
                &instance.environment,
                &instance.service_name,
                &instance.instance_id,
                state.clone(),
            )
            .await;

        if state.is_unhealthy(&config) && instance.status.is_discoverable() {
            let caller = CallerContext::new("discovery-health-checker")
                .with_registry_permission(RegistryPermission::Write);
            let _ = control_plane
                .report_instance_status(
                    &caller,
                    ReportInstanceStatusCommand {
                        namespace: instance.namespace.clone(),
                        environment: instance.environment.clone(),
                        service_name: instance.service_name.clone(),
                        instance_id: instance.instance_id.clone(),
                        status: InstanceStatus::NotServing,
                        now_ms,
                        expected_revision: None,
                    },
                )
                .await;
        } else if state.is_healthy(&config) && !instance.status.is_discoverable() {
            let caller = CallerContext::new("discovery-health-checker")
                .with_registry_permission(RegistryPermission::Write);
            let _ = control_plane
                .report_instance_status(
                    &caller,
                    ReportInstanceStatusCommand {
                        namespace: instance.namespace,
                        environment: instance.environment,
                        service_name: instance.service_name,
                        instance_id: instance.instance_id,
                        status: InstanceStatus::Serving,
                        now_ms,
                        expected_revision: None,
                    },
                )
                .await;
        }
    }
}

fn to_checker_probe(probe: &HealthCheckProbe) -> CheckerProbe {
    match probe {
        HealthCheckProbe::Tcp => CheckerProbe::Tcp,
        HealthCheckProbe::Http {
            path,
            expected_status,
        } => CheckerProbe::Http {
            path: path.clone(),
            expected_status: *expected_status,
        },
        HealthCheckProbe::Grpc { service_name } => CheckerProbe::Grpc {
            service_name: service_name.clone(),
        },
    }
}
