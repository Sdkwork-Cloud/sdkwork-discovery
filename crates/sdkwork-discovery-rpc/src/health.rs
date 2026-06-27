//! Shared discovery runtime health state.
//!
//! Bridges three signals so they stay consistent:
//! - the RPC actor records command outcomes into [`RuntimeResilience`];
//! - [`DiscoveryHealthState`] mirrors the derived status into a lock-free
//!   shared cell that the gRPC health reporter and HTTP readiness probes read;
//! - [`spawn_health_sync`] coalesces status transitions and pushes them to the
//!   `grpc.health.v1` reporter and the `discovery_health_status` gauge.
//!
//! The shared cell is intentionally free of `tonic_health` types so HTTP
//! listeners in the service host can depend on [`DiscoveryHealthStatus`] alone.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;
use tonic_health::server::HealthReporter;
use tracing::info;

use crate::circuit_breaker::CircuitState;
use crate::degradation::DegradationState;
use crate::metrics::set_health_status_value;
use crate::resilience::RuntimeResilience;

/// Three-valued discovery runtime health.
///
/// Mapping to the `discovery_health_status` gauge and gRPC serving status:
/// - `NotServing` (`0`): a required dependency is unavailable and the runtime
///   cannot serve requests (storage circuit breaker open without stale
///   fallback). gRPC serving status becomes `NOT_SERVING`.
/// - `Serving` (`1`): all dependencies are healthy and the runtime is fully
///   ready. gRPC serving status is `SERVING`.
/// - `Degraded` (`2`): the runtime is serving stale reads because a storage
///   dependency is unavailable and read-only degradation is configured. gRPC
///   serving status remains `SERVING` so traffic keeps flowing, but HTTP
///   `/readyz` returns `503` because a dependency is not ready.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoveryHealthStatus {
    NotServing = 0,
    Serving = 1,
    Degraded = 2,
}

impl DiscoveryHealthStatus {
    pub fn as_gauge_value(self) -> f64 {
        self as u8 as f64
    }

    /// Readiness gate for HTTP `/readyz`. Only `Serving` is ready; `Degraded`
    /// is intentionally not ready because a required dependency is unavailable.
    pub fn is_ready(self) -> bool {
        matches!(self, DiscoveryHealthStatus::Serving)
    }

    /// Maps to `grpc.health.v1` serving status. `Degraded` stays `SERVING` so
    /// load balancers keep routing to a pod that can still serve stale reads.
    pub fn grpc_serving_status(self) -> tonic_health::ServingStatus {
        match self {
            DiscoveryHealthStatus::Serving | DiscoveryHealthStatus::Degraded => {
                tonic_health::ServingStatus::Serving
            }
            DiscoveryHealthStatus::NotServing => tonic_health::ServingStatus::NotServing,
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            0 => DiscoveryHealthStatus::NotServing,
            1 => DiscoveryHealthStatus::Serving,
            _ => DiscoveryHealthStatus::Degraded,
        }
    }
}

impl From<DiscoveryHealthStatus> for &'static str {
    fn from(status: DiscoveryHealthStatus) -> Self {
        match status {
            DiscoveryHealthStatus::NotServing => "not_serving",
            DiscoveryHealthStatus::Serving => "serving",
            DiscoveryHealthStatus::Degraded => "degraded",
        }
    }
}

#[derive(Debug)]
struct DiscoveryHealthInner {
    status: AtomicU8,
    last_reason: RwLock<Option<String>>,
}

/// Shared, cheaply cloneable discovery runtime health state.
///
/// Cloning shares the same underlying cell, so the RPC actor, the gRPC health
/// sync task, and the HTTP readiness handler all observe the same status.
#[derive(Clone)]
pub struct DiscoveryHealthState {
    inner: Arc<DiscoveryHealthInner>,
}

impl DiscoveryHealthState {
    /// Creates a new state initialised to [`DiscoveryHealthStatus::Serving`],
    /// matching the resilience default (circuit breaker closed, normal mode).
    pub fn new() -> Self {
        Self::with_status(DiscoveryHealthStatus::Serving)
    }

    pub fn with_status(status: DiscoveryHealthStatus) -> Self {
        Self {
            inner: Arc::new(DiscoveryHealthInner {
                status: AtomicU8::new(status as u8),
                last_reason: RwLock::new(None),
            }),
        }
    }

    pub fn status(&self) -> DiscoveryHealthStatus {
        DiscoveryHealthStatus::from_u8(self.inner.status.load(Ordering::Relaxed))
    }

    pub fn is_ready(&self) -> bool {
        self.status().is_ready()
    }

    pub fn last_reason(&self) -> Option<String> {
        self.inner
            .last_reason
            .read()
            .ok()
            .and_then(|guard| guard.clone())
    }

    pub fn set_serving(&self) {
        self.set(DiscoveryHealthStatus::Serving, None);
    }

    pub fn set_degraded(&self, reason: impl Into<String>) {
        self.set(DiscoveryHealthStatus::Degraded, Some(reason.into()));
    }

    pub fn set_not_serving(&self, reason: impl Into<String>) {
        self.set(DiscoveryHealthStatus::NotServing, Some(reason.into()));
    }

    fn set(&self, status: DiscoveryHealthStatus, reason: Option<String>) {
        self.inner.status.store(status as u8, Ordering::Relaxed);
        if let Ok(mut guard) = self.inner.last_reason.write() {
            *guard = reason;
        }
    }

    /// Derives the health status from runtime resilience state.
    ///
    /// Precedence (matches [`RuntimeResilience::gate`] behaviour):
    /// 1. Read-only degradation wins: the runtime serves stale reads, so it is
    ///    `Degraded` rather than `NotServing`.
    /// 2. An open circuit breaker without stale fallback means the runtime
    ///    cannot serve any operation: `NotServing`.
    /// 3. Otherwise: `Serving`. `HalfOpen` is treated as serving because the
    ///    breaker is actively probing recovery and allows limited traffic.
    pub fn from_resilience(resilience: &RuntimeResilience) -> DiscoveryHealthStatus {
        if resilience.degradation_state() == DegradationState::ReadOnly {
            DiscoveryHealthStatus::Degraded
        } else if resilience.circuit_state() == CircuitState::Open {
            DiscoveryHealthStatus::NotServing
        } else {
            DiscoveryHealthStatus::Serving
        }
    }

    /// Mirrors the resilience-derived status into the shared cell. Called by the
    /// RPC actor after recording each command result. Returns the status that
    /// was active before the call so callers can log transitions.
    ///
    /// Only writes when the derived status differs from the current cell value,
    /// keeping the common (unchanged) path to a single relaxed atomic load.
    pub fn sync_from_resilience(&self, resilience: &RuntimeResilience) -> DiscoveryHealthStatus {
        let previous = self.status();
        let next = Self::from_resilience(resilience);
        if previous == next {
            return previous;
        }
        let reason = match next {
            DiscoveryHealthStatus::Degraded => {
                Some("storage degraded: serving stale reads".to_string())
            }
            DiscoveryHealthStatus::NotServing => Some("storage circuit breaker open".to_string()),
            DiscoveryHealthStatus::Serving => None,
        };
        self.set(next, reason);
        previous
    }
}

impl Default for DiscoveryHealthState {
    fn default() -> Self {
        Self::new()
    }
}

/// Spawns a background task that mirrors [`DiscoveryHealthState`] into the
/// `grpc.health.v1` reporter and the `discovery_health_status` gauge.
///
/// The task samples the shared cell every `sync_interval` and only pushes to
/// the reporter when the status transitions, coalescing rapid resilience
/// changes into a single gRPC health update. The returned handle should be
/// aborted during server shutdown after the reporter is flipped to
/// `NOT_SERVING`.
pub fn spawn_health_sync(
    state: DiscoveryHealthState,
    reporter: HealthReporter,
    service_names: Vec<String>,
    sync_interval: Duration,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(sync_interval);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut last_reported: Option<DiscoveryHealthStatus> = None;

        loop {
            interval.tick().await;
            let status = state.status();
            if last_reported == Some(status) {
                continue;
            }

            let serving_status = status.grpc_serving_status();
            for name in &service_names {
                reporter
                    .set_service_status(name.as_str(), serving_status)
                    .await;
            }
            set_health_status_value(status.as_gauge_value());
            last_reported = Some(status);
            info!(
                health_status = <&'static str>::from(status),
                reason = ?state.last_reason(),
                "discovery health status synced"
            );
        }
    })
}

/// Default health sync sampling interval. One second balances promptness
/// against reporter churn; transitions are coalesced so a higher rate is not
/// required.
pub const DEFAULT_HEALTH_SYNC_INTERVAL: Duration = Duration::from_secs(1);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::circuit_breaker::CircuitBreakerConfig;
    use crate::degradation::DegradationConfig;
    use crate::rate_limiter::RateLimitConfig;
    use crate::resilience::RuntimeResilienceConfig;
    use sdkwork_discovery_contract::DiscoveryError;

    fn resilience_with(
        circuit_enabled: bool,
        failure_threshold: u32,
        read_only_on_failure: bool,
    ) -> RuntimeResilience {
        RuntimeResilience::new(RuntimeResilienceConfig {
            circuit_breaker: CircuitBreakerConfig {
                enabled: circuit_enabled,
                failure_threshold,
                recovery_timeout_ms: 30_000,
                half_open_max_requests: 1,
            },
            rate_limit: RateLimitConfig::default(),
            degradation: DegradationConfig {
                read_only_on_storage_failure: read_only_on_failure,
                stale_read_max_age_ms: 60_000,
            },
        })
    }

    #[test]
    fn fresh_resilience_is_serving() {
        let resilience = resilience_with(true, 1, true);
        assert_eq!(
            DiscoveryHealthState::from_resilience(&resilience),
            DiscoveryHealthStatus::Serving
        );
    }

    #[test]
    fn storage_failure_with_stale_fallback_is_degraded() {
        let mut resilience = resilience_with(true, 1, true);
        resilience.record_result::<()>(&Err(DiscoveryError::Unavailable(
            "postgres down".to_string(),
        )));
        assert_eq!(
            DiscoveryHealthState::from_resilience(&resilience),
            DiscoveryHealthStatus::Degraded
        );
    }

    #[test]
    fn storage_failure_without_stale_fallback_is_not_serving() {
        let mut resilience = resilience_with(true, 1, false);
        resilience.record_result::<()>(&Err(DiscoveryError::Unavailable(
            "postgres down".to_string(),
        )));
        assert_eq!(
            DiscoveryHealthState::from_resilience(&resilience),
            DiscoveryHealthStatus::NotServing
        );
    }

    #[test]
    fn degraded_is_not_ready_but_grpc_serving() {
        assert!(!DiscoveryHealthStatus::Degraded.is_ready());
        assert_eq!(
            DiscoveryHealthStatus::Degraded.grpc_serving_status(),
            tonic_health::ServingStatus::Serving
        );
    }

    #[test]
    fn not_serving_is_neither_ready_nor_grpc_serving() {
        assert!(!DiscoveryHealthStatus::NotServing.is_ready());
        assert_eq!(
            DiscoveryHealthStatus::NotServing.grpc_serving_status(),
            tonic_health::ServingStatus::NotServing
        );
    }

    #[test]
    fn sync_from_resilience_updates_cell_only_on_transition() {
        let state = DiscoveryHealthState::new();
        let mut resilience = resilience_with(true, 1, true);

        // No change: stays Serving, previous returned as Serving.
        let previous = state.sync_from_resilience(&resilience);
        assert_eq!(previous, DiscoveryHealthStatus::Serving);
        assert_eq!(state.status(), DiscoveryHealthStatus::Serving);
        assert!(state.last_reason().is_none());

        // Failure -> Degraded.
        resilience.record_result::<()>(&Err(DiscoveryError::Unavailable(
            "postgres down".to_string(),
        )));
        let previous = state.sync_from_resilience(&resilience);
        assert_eq!(previous, DiscoveryHealthStatus::Serving);
        assert_eq!(state.status(), DiscoveryHealthStatus::Degraded);
        assert!(state.last_reason().is_some());

        // Same state: no rewrite of reason.
        let reason_before = state.last_reason();
        let previous = state.sync_from_resilience(&resilience);
        assert_eq!(previous, DiscoveryHealthStatus::Degraded);
        assert_eq!(state.last_reason(), reason_before);
    }

    #[test]
    fn shared_state_observed_across_clones() {
        let state = DiscoveryHealthState::new();
        let observer = state.clone();
        state.set_not_serving("circuit open");
        assert_eq!(observer.status(), DiscoveryHealthStatus::NotServing);
        assert!(!observer.is_ready());
        assert_eq!(observer.last_reason(), Some("circuit open".to_string()));
    }

    #[test]
    fn recovery_clears_degraded_reason() {
        let state = DiscoveryHealthState::new();
        state.set_degraded("storage degraded");
        assert!(state.last_reason().is_some());
        state.set_serving();
        assert_eq!(state.status(), DiscoveryHealthStatus::Serving);
        assert!(state.last_reason().is_none());
    }
}
