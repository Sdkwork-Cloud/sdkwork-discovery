use metrics::{counter, describe_counter, describe_gauge, describe_histogram, gauge, histogram};
use std::sync::OnceLock;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct RpcTelemetryContext {
    pub process_service: String,
    pub environment: String,
    pub deployment_profile: String,
    pub runtime_target: String,
}

impl Default for RpcTelemetryContext {
    fn default() -> Self {
        Self {
            process_service: "sdkwork-discovery".to_string(),
            environment: "development".to_string(),
            deployment_profile: "standalone".to_string(),
            runtime_target: "server".to_string(),
        }
    }
}

static TELEMETRY: OnceLock<RpcTelemetryContext> = OnceLock::new();

pub fn init_telemetry_context(context: RpcTelemetryContext) {
    let _ = TELEMETRY.set(context);
}

fn telemetry() -> RpcTelemetryContext {
    TELEMETRY.get().cloned().unwrap_or_default()
}

fn normalize_status(status_code: &str) -> String {
    status_code.to_ascii_lowercase()
}

pub struct RpcMetrics {
    package: String,
    service: String,
    method: String,
    operation_id: String,
    api_surface: String,
    start: Instant,
}

impl RpcMetrics {
    pub fn new(
        package: impl Into<String>,
        service: impl Into<String>,
        method: impl Into<String>,
        operation_id: impl Into<String>,
    ) -> Self {
        let package = package.into();
        let api_surface = api_surface_for_package(&package);
        Self::with_api_surface(package, service, method, operation_id, api_surface)
    }

    pub fn with_api_surface(
        package: impl Into<String>,
        service: impl Into<String>,
        method: impl Into<String>,
        operation_id: impl Into<String>,
        api_surface: impl Into<String>,
    ) -> Self {
        Self {
            package: package.into(),
            service: service.into(),
            method: method.into(),
            operation_id: operation_id.into(),
            api_surface: api_surface.into(),
            start: Instant::now(),
        }
    }

    pub fn record_success(self, status_code: &str) {
        let duration = self.start.elapsed();
        let labels = telemetry();
        let status = normalize_status(status_code);
        counter!(
            "discovery_rpc_requests_total",
            "service" => labels.process_service.clone(),
            "environment" => labels.environment.clone(),
            "deployment_profile" => labels.deployment_profile.clone(),
            "runtime_target" => labels.runtime_target.clone(),
            "package" => self.package.clone(),
            "rpc_service" => self.service.clone(),
            "method" => self.method.clone(),
            "operation_id" => self.operation_id.clone(),
            "status" => status,
            "api_surface" => self.api_surface.clone(),
        )
        .increment(1);
        histogram!(
            "discovery_rpc_request_duration_seconds",
            "service" => labels.process_service.clone(),
            "environment" => labels.environment.clone(),
            "deployment_profile" => labels.deployment_profile.clone(),
            "runtime_target" => labels.runtime_target.clone(),
            "package" => self.package.clone(),
            "rpc_service" => self.service.clone(),
            "method" => self.method.clone(),
            "operation_id" => self.operation_id.clone(),
            "api_surface" => self.api_surface.clone(),
        )
        .record(duration);
    }

    pub fn record_error(self, status_code: &str, error_type: &str) {
        let duration = self.start.elapsed();
        let labels = telemetry();
        let status = normalize_status(status_code);
        counter!(
            "discovery_rpc_requests_total",
            "service" => labels.process_service.clone(),
            "environment" => labels.environment.clone(),
            "deployment_profile" => labels.deployment_profile.clone(),
            "runtime_target" => labels.runtime_target.clone(),
            "package" => self.package.clone(),
            "rpc_service" => self.service.clone(),
            "method" => self.method.clone(),
            "operation_id" => self.operation_id.clone(),
            "status" => status,
            "api_surface" => self.api_surface.clone(),
        )
        .increment(1);
        counter!(
            "discovery_rpc_errors_total",
            "service" => labels.process_service.clone(),
            "environment" => labels.environment.clone(),
            "deployment_profile" => labels.deployment_profile.clone(),
            "runtime_target" => labels.runtime_target.clone(),
            "package" => self.package.clone(),
            "rpc_service" => self.service.clone(),
            "method" => self.method.clone(),
            "error_type" => error_type.to_string(),
            "api_surface" => self.api_surface.clone(),
        )
        .increment(1);
        histogram!(
            "discovery_rpc_request_duration_seconds",
            "service" => labels.process_service.clone(),
            "environment" => labels.environment.clone(),
            "deployment_profile" => labels.deployment_profile.clone(),
            "runtime_target" => labels.runtime_target.clone(),
            "package" => self.package.clone(),
            "rpc_service" => self.service.clone(),
            "method" => self.method.clone(),
            "operation_id" => self.operation_id.clone(),
            "api_surface" => self.api_surface.clone(),
        )
        .record(duration);
    }

    pub fn record_timed_out(self) {
        let duration = self.start.elapsed();
        let labels = telemetry();
        counter!(
            "discovery_rpc_requests_total",
            "service" => labels.process_service.clone(),
            "environment" => labels.environment.clone(),
            "deployment_profile" => labels.deployment_profile.clone(),
            "runtime_target" => labels.runtime_target.clone(),
            "package" => self.package.clone(),
            "rpc_service" => self.service.clone(),
            "method" => self.method.clone(),
            "operation_id" => self.operation_id.clone(),
            "status" => "deadline_exceeded",
            "api_surface" => self.api_surface.clone(),
        )
        .increment(1);
        record_deadline_exceeded(&self.package, &self.service, &self.method);
        histogram!(
            "discovery_rpc_request_duration_seconds",
            "service" => labels.process_service.clone(),
            "environment" => labels.environment.clone(),
            "deployment_profile" => labels.deployment_profile.clone(),
            "runtime_target" => labels.runtime_target.clone(),
            "package" => self.package.clone(),
            "rpc_service" => self.service.clone(),
            "method" => self.method.clone(),
            "operation_id" => self.operation_id.clone(),
            "api_surface" => self.api_surface.clone(),
        )
        .record(duration);
    }
}

/// Ensures unary RPC handlers record deadline/cancellation metrics when the request
/// future is dropped before an explicit success or error outcome is recorded.
pub struct RpcMetricsGuard {
    inner: Option<RpcMetrics>,
}

impl RpcMetricsGuard {
    pub fn new(
        package: impl Into<String>,
        service: impl Into<String>,
        method: impl Into<String>,
        operation_id: impl Into<String>,
    ) -> Self {
        Self {
            inner: Some(RpcMetrics::new(package, service, method, operation_id)),
        }
    }

    pub fn record_success(&mut self, status_code: &str) {
        if let Some(metrics) = self.inner.take() {
            metrics.record_success(status_code);
        }
    }

    pub fn record_error(&mut self, status_code: &str, error_type: &str) {
        if let Some(metrics) = self.inner.take() {
            metrics.record_error(status_code, error_type);
        }
    }
}

impl Drop for RpcMetricsGuard {
    fn drop(&mut self) {
        if let Some(metrics) = self.inner.take() {
            metrics.record_timed_out();
        }
    }
}

pub fn record_auth_failure(package: &str, service: &str, method: &str) {
    let labels = telemetry();
    let api_surface = api_surface_for_package(package);
    counter!(
        "discovery_rpc_auth_failures_total",
        "service" => labels.process_service.clone(),
        "environment" => labels.environment.clone(),
        "deployment_profile" => labels.deployment_profile.clone(),
        "runtime_target" => labels.runtime_target.clone(),
        "package" => package.to_string(),
        "rpc_service" => service.to_string(),
        "method" => method.to_string(),
        "api_surface" => api_surface.to_string(),
    )
    .increment(1);
}

pub fn record_deadline_exceeded(package: &str, service: &str, method: &str) {
    let labels = telemetry();
    let api_surface = api_surface_for_package(package);
    counter!(
        "discovery_rpc_deadline_exceeded_total",
        "service" => labels.process_service.clone(),
        "environment" => labels.environment.clone(),
        "deployment_profile" => labels.deployment_profile.clone(),
        "runtime_target" => labels.runtime_target.clone(),
        "package" => package.to_string(),
        "rpc_service" => service.to_string(),
        "method" => method.to_string(),
        "api_surface" => api_surface.to_string(),
    )
    .increment(1);
}

pub fn record_cancellation(package: &str, service: &str, method: &str) {
    let labels = telemetry();
    let api_surface = api_surface_for_package(package);
    counter!(
        "discovery_rpc_cancellations_total",
        "service" => labels.process_service.clone(),
        "environment" => labels.environment.clone(),
        "deployment_profile" => labels.deployment_profile.clone(),
        "runtime_target" => labels.runtime_target.clone(),
        "package" => package.to_string(),
        "rpc_service" => service.to_string(),
        "method" => method.to_string(),
        "api_surface" => api_surface.to_string(),
    )
    .increment(1);
}

pub fn increment_active_streams(surface: &str) {
    let labels = telemetry();
    gauge!(
        "discovery_rpc_active_streams",
        "service" => labels.process_service.clone(),
        "environment" => labels.environment.clone(),
        "deployment_profile" => labels.deployment_profile.clone(),
        "runtime_target" => labels.runtime_target.clone(),
        "surface" => surface.to_string(),
    )
    .increment(1);
}

pub fn decrement_active_streams(surface: &str) {
    let labels = telemetry();
    gauge!(
        "discovery_rpc_active_streams",
        "service" => labels.process_service.clone(),
        "environment" => labels.environment.clone(),
        "deployment_profile" => labels.deployment_profile.clone(),
        "runtime_target" => labels.runtime_target.clone(),
        "surface" => surface.to_string(),
    )
    .decrement(1);
}

pub fn set_health_status(healthy: bool) {
    set_health_status_value(if healthy { 1.0 } else { 0.0 });
}

/// Records the runtime health status as a gauge value:
/// `0` = not serving, `1` = serving, `2` = degraded (serving stale reads).
pub fn set_health_status_value(value: f64) {
    let labels = telemetry();
    gauge!(
        "discovery_health_status",
        "service" => labels.process_service.clone(),
        "environment" => labels.environment.clone(),
        "deployment_profile" => labels.deployment_profile.clone(),
        "runtime_target" => labels.runtime_target.clone(),
    )
    .set(value);
}

pub fn describe_metrics() {
    describe_counter!(
        "discovery_rpc_requests_total",
        "Total number of RPC requests"
    );
    describe_counter!("discovery_rpc_errors_total", "Total number of RPC errors");
    describe_counter!(
        "discovery_rpc_auth_failures_total",
        "Total number of RPC authentication failures"
    );
    describe_counter!(
        "discovery_rpc_deadline_exceeded_total",
        "Total number of RPC deadline exceeded events"
    );
    describe_counter!(
        "discovery_rpc_cancellations_total",
        "Total number of RPC cancellations"
    );
    describe_histogram!(
        "discovery_rpc_request_duration_seconds",
        "RPC request duration in seconds"
    );
    describe_gauge!(
        "discovery_rpc_active_streams",
        "Number of active RPC streams"
    );
    describe_gauge!(
        "discovery_health_status",
        "Discovery runtime health status (0=not serving, 1=serving, 2=degraded)"
    );
}

fn api_surface_for_package(package: &str) -> &'static str {
    if package.contains("backend.v3") {
        "rpc-backend"
    } else {
        "rpc-internal"
    }
}

#[cfg(test)]
mod tests {
    use super::RpcMetricsGuard;

    #[test]
    fn guard_records_timed_out_when_not_completed() {
        let _guard = RpcMetricsGuard::new(
            "sdkwork.discovery.internal.v1",
            "RegistryService",
            "RegisterInstance",
            "discovery.registry.instances.register",
        );
    }
}
