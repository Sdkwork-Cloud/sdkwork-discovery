#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HealthCheckConfig {
    pub probe: HealthCheckProbe,
    pub interval_ms: u64,
    pub timeout_ms: u64,
    pub unhealthy_threshold: u32,
    pub healthy_threshold: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HealthCheckProbe {
    Tcp,
    Http { path: String, expected_status: u16 },
    Grpc { service_name: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct HealthCheckRuntimeState {
    pub consecutive_successes: u32,
    pub consecutive_failures: u32,
    pub last_check_ms: u64,
}

impl HealthCheckRuntimeState {
    pub fn is_healthy(&self, config: &HealthCheckConfig) -> bool {
        self.consecutive_successes >= config.healthy_threshold
    }

    pub fn is_unhealthy(&self, config: &HealthCheckConfig) -> bool {
        self.consecutive_failures >= config.unhealthy_threshold
    }

    pub fn due_for_check(&self, config: &HealthCheckConfig, now_ms: u64) -> bool {
        if config.interval_ms == 0 {
            return false;
        }
        self.last_check_ms == 0 || now_ms.saturating_sub(self.last_check_ms) >= config.interval_ms
    }
}
