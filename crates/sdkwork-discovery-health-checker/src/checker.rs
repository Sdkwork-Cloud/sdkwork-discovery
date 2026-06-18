use std::time::Duration;

use tokio::net::TcpStream;
use tokio::time::timeout;
use tracing::{debug, warn};

use sdkwork_discovery_contract::HealthCheckProbe;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthCheckResult {
    pub healthy: bool,
    pub latency_ms: u64,
    pub message: Option<String>,
}

pub async fn check_health(
    endpoint: &str,
    probe: &HealthCheckProbe,
    timeout_ms: u64,
) -> HealthCheckResult {
    let start = tokio::time::Instant::now();

    let result = match probe {
        HealthCheckProbe::Tcp => check_tcp(endpoint, timeout_ms).await,
        HealthCheckProbe::Http {
            path,
            expected_status,
        } => check_http(endpoint, path, *expected_status, timeout_ms).await,
        HealthCheckProbe::Grpc { service_name } => {
            check_grpc(endpoint, service_name, timeout_ms).await
        }
    };

    let latency_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(()) => HealthCheckResult {
            healthy: true,
            latency_ms,
            message: None,
        },
        Err(e) => HealthCheckResult {
            healthy: false,
            latency_ms,
            message: Some(e),
        },
    }
}

async fn check_tcp(endpoint: &str, timeout_ms: u64) -> Result<(), String> {
    let duration = Duration::from_millis(timeout_ms);
    match timeout(duration, TcpStream::connect(endpoint)).await {
        Ok(Ok(_)) => {
            debug!(endpoint = %endpoint, "TCP health check passed");
            Ok(())
        }
        Ok(Err(e)) => {
            warn!(endpoint = %endpoint, error = %e, "TCP health check failed");
            Err(format!("TCP connection failed: {e}"))
        }
        Err(_) => {
            warn!(endpoint = %endpoint, timeout_ms = timeout_ms, "TCP health check timed out");
            Err("TCP connection timed out".to_string())
        }
    }
}

async fn check_http(
    endpoint: &str,
    path: &str,
    expected_status: u16,
    timeout_ms: u64,
) -> Result<(), String> {
    let url = format!("http://{endpoint}{path}");
    let duration = Duration::from_millis(timeout_ms);

    let client = reqwest::Client::builder()
        .timeout(duration)
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

    match timeout(duration, client.get(&url).send()).await {
        Ok(Ok(response)) => {
            let status = response.status().as_u16();
            if status == expected_status {
                debug!(endpoint = %endpoint, status = status, "HTTP health check passed");
                Ok(())
            } else {
                warn!(endpoint = %endpoint, status = status, expected = expected_status, "HTTP health check failed");
                Err(format!(
                    "HTTP status {status} != expected {expected_status}"
                ))
            }
        }
        Ok(Err(e)) => {
            warn!(endpoint = %endpoint, error = %e, "HTTP health check failed");
            Err(format!("HTTP request failed: {e}"))
        }
        Err(_) => {
            warn!(endpoint = %endpoint, timeout_ms = timeout_ms, "HTTP health check timed out");
            Err("HTTP request timed out".to_string())
        }
    }
}

async fn check_grpc(endpoint: &str, service_name: &str, timeout_ms: u64) -> Result<(), String> {
    let duration = Duration::from_millis(timeout_ms);
    match timeout(duration, TcpStream::connect(endpoint)).await {
        Ok(Ok(_)) => {
            debug!(endpoint = %endpoint, service = %service_name, "gRPC health check passed");
            Ok(())
        }
        Ok(Err(e)) => {
            warn!(endpoint = %endpoint, error = %e, "gRPC health check failed");
            Err(format!("gRPC connection failed: {e}"))
        }
        Err(_) => {
            warn!(endpoint = %endpoint, timeout_ms = timeout_ms, "gRPC health check timed out");
            Err("gRPC connection timed out".to_string())
        }
    }
}
