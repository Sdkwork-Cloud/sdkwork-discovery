use std::time::Duration;

use sdkwork_discovery_contract::{
    DiscoverInstancesResult, DiscoveryError, DiscoveryEvent, DiscoveryResult, EffectiveConfig,
    ListServicesResult, ServiceInstance,
};

use crate::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};
use crate::degradation::{DegradationConfig, DegradationState, OperationType};
use crate::rate_limiter::{RateLimitConfig, TokenBucketRateLimiter};
use crate::stale_read_cache::StaleReadCache;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RuntimeResilienceConfig {
    pub circuit_breaker: CircuitBreakerConfig,
    pub rate_limit: RateLimitConfig,
    pub degradation: DegradationConfig,
}

#[derive(Debug)]
pub struct RuntimeResilience {
    circuit_breaker: CircuitBreaker,
    rate_limiter: TokenBucketRateLimiter,
    rate_limit: RateLimitConfig,
    degradation: DegradationState,
    degradation_config: DegradationConfig,
    discover_reads: StaleReadCache<DiscoverInstancesResult>,
    retrieve_reads: StaleReadCache<Option<ServiceInstance>>,
    list_services_reads: StaleReadCache<ListServicesResult>,
    effective_config_reads: StaleReadCache<EffectiveConfig>,
    watch_reads: StaleReadCache<Vec<DiscoveryEvent>>,
}

impl RuntimeResilience {
    pub fn new(config: RuntimeResilienceConfig) -> Self {
        let rate_limit = config.rate_limit.clone();
        Self {
            circuit_breaker: CircuitBreaker::new(config.circuit_breaker),
            rate_limiter: TokenBucketRateLimiter::new(
                rate_limit.burst_capacity,
                rate_limit.requests_per_second as f64,
            ),
            rate_limit,
            degradation: DegradationState::Normal,
            degradation_config: config.degradation,
            discover_reads: StaleReadCache::default(),
            retrieve_reads: StaleReadCache::default(),
            list_services_reads: StaleReadCache::default(),
            effective_config_reads: StaleReadCache::default(),
            watch_reads: StaleReadCache::default(),
        }
    }

    pub fn gate(&mut self, operation: OperationType) -> DiscoveryResult<()> {
        if self.configured_rate_limit() && !self.rate_limiter.try_acquire() {
            return Err(DiscoveryError::Unavailable(
                "rpc rate limit exceeded".to_string(),
            ));
        }

        if !self.degradation.allows(operation) {
            return Err(DiscoveryError::Unavailable(
                "discovery runtime is in read-only degradation mode".to_string(),
            ));
        }

        let enforce_circuit_breaker = matches!(operation, OperationType::Write)
            || self.degradation == DegradationState::Normal;
        if enforce_circuit_breaker && !self.circuit_breaker.allow_request() {
            return Err(DiscoveryError::Unavailable(
                "storage circuit breaker is open".to_string(),
            ));
        }

        Ok(())
    }

    pub fn record_result<T>(&mut self, result: &DiscoveryResult<T>) {
        match result {
            Ok(_) => {
                self.circuit_breaker.record_success();
                if self.degradation == DegradationState::ReadOnly {
                    self.degradation = DegradationState::Normal;
                }
            }
            Err(error) if is_storage_failure(error) => {
                self.circuit_breaker.record_failure();
                if self.degradation_config.read_only_on_storage_failure {
                    self.degradation = DegradationState::ReadOnly;
                }
            }
            Err(_) => {}
        }
    }

    pub fn degradation_state(&self) -> DegradationState {
        self.degradation
    }

    pub fn circuit_state(&self) -> crate::circuit_breaker::CircuitState {
        self.circuit_breaker.state()
    }

    pub fn resolve_discover_instances(
        &mut self,
        key: String,
        result: DiscoveryResult<DiscoverInstancesResult>,
    ) -> DiscoveryResult<DiscoverInstancesResult> {
        self.discover_reads.resolve(
            key,
            self.stale_read_max_age(),
            self.serve_stale_reads(),
            result,
        )
    }

    pub fn resolve_retrieve_instance(
        &mut self,
        key: String,
        result: DiscoveryResult<Option<ServiceInstance>>,
    ) -> DiscoveryResult<Option<ServiceInstance>> {
        self.retrieve_reads.resolve(
            key,
            self.stale_read_max_age(),
            self.serve_stale_reads(),
            result,
        )
    }

    pub fn resolve_list_services(
        &mut self,
        key: String,
        result: DiscoveryResult<ListServicesResult>,
    ) -> DiscoveryResult<ListServicesResult> {
        self.list_services_reads.resolve(
            key,
            self.stale_read_max_age(),
            self.serve_stale_reads(),
            result,
        )
    }

    pub fn resolve_effective_config(
        &mut self,
        key: String,
        result: DiscoveryResult<EffectiveConfig>,
    ) -> DiscoveryResult<EffectiveConfig> {
        self.effective_config_reads.resolve(
            key,
            self.stale_read_max_age(),
            self.serve_stale_reads(),
            result,
        )
    }

    pub fn resolve_watch_events(
        &mut self,
        key: String,
        result: DiscoveryResult<Vec<DiscoveryEvent>>,
    ) -> DiscoveryResult<Vec<DiscoveryEvent>> {
        self.watch_reads.resolve(
            key,
            self.stale_read_max_age(),
            self.serve_stale_reads(),
            result,
        )
    }

    fn serve_stale_reads(&self) -> bool {
        self.degradation == DegradationState::ReadOnly
            && self.degradation_config.read_only_on_storage_failure
    }

    fn stale_read_max_age(&self) -> Duration {
        Duration::from_millis(self.degradation_config.stale_read_max_age_ms)
    }

    fn configured_rate_limit(&self) -> bool {
        self.rate_limit.enabled
    }
}

fn is_storage_failure(error: &DiscoveryError) -> bool {
    matches!(error, DiscoveryError::Unavailable(_))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_rejects_when_bucket_is_empty() {
        let mut resilience = RuntimeResilience::new(RuntimeResilienceConfig {
            rate_limit: RateLimitConfig {
                enabled: true,
                requests_per_second: 1,
                burst_capacity: 1,
            },
            ..RuntimeResilienceConfig::default()
        });

        assert!(resilience.gate(OperationType::Read).is_ok());
        let second = resilience.gate(OperationType::Read);
        assert!(matches!(
            second,
            Err(DiscoveryError::Unavailable(message)) if message.contains("rate limit")
        ));
    }

    #[test]
    fn storage_failure_opens_circuit_and_enables_read_only_degradation() {
        let mut resilience = RuntimeResilience::new(RuntimeResilienceConfig {
            circuit_breaker: CircuitBreakerConfig {
                enabled: true,
                failure_threshold: 1,
                recovery_timeout_ms: 30_000,
                half_open_max_requests: 1,
            },
            degradation: DegradationConfig {
                read_only_on_storage_failure: true,
                stale_read_max_age_ms: 60_000,
            },
            ..RuntimeResilienceConfig::default()
        });

        resilience.record_result::<()>(&Err(DiscoveryError::Unavailable(
            "postgres unavailable".to_string(),
        )));

        assert_eq!(resilience.degradation_state(), DegradationState::ReadOnly);
        assert_eq!(
            resilience.circuit_state(),
            crate::circuit_breaker::CircuitState::Open
        );
        assert!(matches!(
            resilience.gate(OperationType::Write),
            Err(DiscoveryError::Unavailable(message)) if message.contains("read-only")
        ));
        assert!(resilience.gate(OperationType::Read).is_ok());
    }

    #[test]
    fn storage_recovery_clears_read_only_degradation() {
        let mut resilience = RuntimeResilience::new(RuntimeResilienceConfig {
            circuit_breaker: CircuitBreakerConfig {
                enabled: true,
                failure_threshold: 1,
                recovery_timeout_ms: 30_000,
                half_open_max_requests: 1,
            },
            degradation: DegradationConfig {
                read_only_on_storage_failure: true,
                stale_read_max_age_ms: 60_000,
            },
            ..RuntimeResilienceConfig::default()
        });

        resilience.record_result::<()>(&Err(DiscoveryError::Unavailable(
            "postgres unavailable".to_string(),
        )));
        assert_eq!(resilience.degradation_state(), DegradationState::ReadOnly);

        resilience.record_result::<()>(&Ok(()));
        assert_eq!(resilience.degradation_state(), DegradationState::Normal);
    }

    #[test]
    fn read_only_mode_serves_stale_discover_results_after_storage_failure() {
        let mut resilience = RuntimeResilience::new(RuntimeResilienceConfig {
            circuit_breaker: CircuitBreakerConfig {
                enabled: true,
                failure_threshold: 1,
                recovery_timeout_ms: 30_000,
                half_open_max_requests: 1,
            },
            degradation: DegradationConfig {
                read_only_on_storage_failure: true,
                stale_read_max_age_ms: 60_000,
            },
            ..RuntimeResilienceConfig::default()
        });

        let key = "discover:sdkwork:dev:svc".to_string();
        let fresh = DiscoverInstancesResult {
            revision: 1,
            instances: vec![],
            next_page_token: None,
        };
        let cached = resilience
            .resolve_discover_instances(key.clone(), Ok(fresh.clone()))
            .unwrap();
        assert_eq!(cached.revision, 1);

        resilience.record_result::<()>(&Err(DiscoveryError::Unavailable(
            "postgres unavailable".to_string(),
        )));

        let stale = resilience
            .resolve_discover_instances(
                key,
                Err(DiscoveryError::Unavailable(
                    "postgres unavailable".to_string(),
                )),
            )
            .unwrap();
        assert_eq!(stale.revision, fresh.revision);
    }
}
