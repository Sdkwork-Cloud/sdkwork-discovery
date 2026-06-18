use sdkwork_discovery_contract::{HealthCheckConfig, HealthCheckRuntimeState};

#[test]
fn state_becomes_healthy_after_consecutive_successes() {
    let config = HealthCheckConfig {
        probe: sdkwork_discovery_contract::HealthCheckProbe::Tcp,
        interval_ms: 1_000,
        timeout_ms: 500,
        unhealthy_threshold: 2,
        healthy_threshold: 2,
    };
    let mut state = HealthCheckRuntimeState::default();

    state.consecutive_successes += 1;
    state.last_check_ms = 1;
    assert!(!state.is_healthy(&config));

    state.consecutive_successes += 1;
    state.last_check_ms = 2;
    assert!(state.is_healthy(&config));
    assert!(!state.is_unhealthy(&config));
}

#[test]
fn state_becomes_unhealthy_after_consecutive_failures() {
    let config = HealthCheckConfig {
        probe: sdkwork_discovery_contract::HealthCheckProbe::Http {
            path: "/health".to_string(),
            expected_status: 200,
        },
        interval_ms: 1_000,
        timeout_ms: 500,
        unhealthy_threshold: 2,
        healthy_threshold: 2,
    };
    let mut state = HealthCheckRuntimeState::default();

    state.consecutive_failures += 1;
    state.last_check_ms = 1;
    assert!(!state.is_unhealthy(&config));

    state.consecutive_failures += 1;
    state.last_check_ms = 2;
    assert!(state.is_unhealthy(&config));
}
