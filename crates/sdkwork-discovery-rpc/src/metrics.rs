use metrics::{counter, describe_counter, describe_gauge, describe_histogram, gauge, histogram};
use std::time::Instant;

pub struct RpcMetrics {
    package: String,
    service: String,
    method: String,
    operation_id: String,
    start: Instant,
}

impl RpcMetrics {
    pub fn new(
        package: impl Into<String>,
        service: impl Into<String>,
        method: impl Into<String>,
        operation_id: impl Into<String>,
    ) -> Self {
        Self {
            package: package.into(),
            service: service.into(),
            method: method.into(),
            operation_id: operation_id.into(),
            start: Instant::now(),
        }
    }

    pub fn record_success(self, status_code: &str) {
        let duration = self.start.elapsed();
        counter!(
            "discovery_rpc_requests_total",
            "package" => self.package.clone(),
            "service" => self.service.clone(),
            "method" => self.method.clone(),
            "operation_id" => self.operation_id.clone(),
            "status" => status_code.to_string(),
            "api_surface" => "rpc".to_string(),
        )
        .increment(1);
        histogram!(
            "discovery_rpc_request_duration_seconds",
            "package" => self.package,
            "service" => self.service,
            "method" => self.method,
        )
        .record(duration);
    }

    pub fn record_error(self, status_code: &str, error_type: &str) {
        let duration = self.start.elapsed();
        counter!(
            "discovery_rpc_requests_total",
            "package" => self.package.clone(),
            "service" => self.service.clone(),
            "method" => self.method.clone(),
            "operation_id" => self.operation_id.clone(),
            "status" => status_code.to_string(),
            "api_surface" => "rpc".to_string(),
        )
        .increment(1);
        counter!(
            "discovery_rpc_errors_total",
            "package" => self.package.clone(),
            "service" => self.service.clone(),
            "method" => self.method.clone(),
            "error_type" => error_type.to_string(),
        )
        .increment(1);
        histogram!(
            "discovery_rpc_request_duration_seconds",
            "package" => self.package,
            "service" => self.service,
            "method" => self.method,
        )
        .record(duration);
    }
}

pub fn record_auth_failure(package: &str, service: &str, method: &str) {
    counter!(
        "discovery_rpc_auth_failures_total",
        "package" => package.to_string(),
        "service" => service.to_string(),
        "method" => method.to_string(),
    )
    .increment(1);
}

pub fn record_deadline_exceeded(package: &str, service: &str, method: &str) {
    counter!(
        "discovery_rpc_deadline_exceeded_total",
        "package" => package.to_string(),
        "service" => service.to_string(),
        "method" => method.to_string(),
    )
    .increment(1);
}

pub fn record_cancellation(package: &str, service: &str, method: &str) {
    counter!(
        "discovery_rpc_cancellations_total",
        "package" => package.to_string(),
        "service" => service.to_string(),
        "method" => method.to_string(),
    )
    .increment(1);
}

pub fn increment_active_streams(surface: &str) {
    gauge!(
        "discovery_rpc_active_streams",
        "surface" => surface.to_string(),
    )
    .increment(1);
}

pub fn decrement_active_streams(surface: &str) {
    gauge!(
        "discovery_rpc_active_streams",
        "surface" => surface.to_string(),
    )
    .decrement(1);
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
}
