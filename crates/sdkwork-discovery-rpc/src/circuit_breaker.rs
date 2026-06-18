use std::time::{Duration, Instant};
use tracing::warn;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CircuitBreakerConfig {
    pub enabled: bool,
    pub failure_threshold: u32,
    pub recovery_timeout_ms: u64,
    pub half_open_max_requests: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            failure_threshold: 5,
            recovery_timeout_ms: 30_000,
            half_open_max_requests: 3,
        }
    }
}

#[derive(Debug)]
pub struct CircuitBreaker {
    state: CircuitState,
    failure_count: u32,
    success_count: u32,
    config: CircuitBreakerConfig,
    last_failure: Option<Instant>,
    half_open_requests: u32,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            success_count: 0,
            config,
            last_failure: None,
            half_open_requests: 0,
        }
    }

    pub fn state(&self) -> CircuitState {
        self.state
    }

    pub fn is_open(&self) -> bool {
        if !self.config.enabled {
            return false;
        }

        match self.state {
            CircuitState::Open => {
                if let Some(last_failure) = self.last_failure {
                    let elapsed = last_failure.elapsed();
                    if elapsed >= Duration::from_millis(self.config.recovery_timeout_ms) {
                        return false;
                    }
                }
                true
            }
            _ => false,
        }
    }

    pub fn allow_request(&mut self) -> bool {
        if !self.config.enabled {
            return true;
        }

        match self.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                if let Some(last_failure) = self.last_failure {
                    if last_failure.elapsed()
                        >= Duration::from_millis(self.config.recovery_timeout_ms)
                    {
                        self.state = CircuitState::HalfOpen;
                        self.half_open_requests = 0;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => {
                if self.half_open_requests < self.config.half_open_max_requests {
                    self.half_open_requests += 1;
                    true
                } else {
                    false
                }
            }
        }
    }

    pub fn record_success(&mut self) {
        if !self.config.enabled {
            return;
        }

        match self.state {
            CircuitState::Closed => {
                self.failure_count = 0;
            }
            CircuitState::HalfOpen => {
                self.success_count += 1;
                if self.success_count >= self.config.half_open_max_requests {
                    self.state = CircuitState::Closed;
                    self.failure_count = 0;
                    self.success_count = 0;
                }
            }
            CircuitState::Open => {}
        }
    }

    pub fn record_failure(&mut self) {
        if !self.config.enabled {
            return;
        }

        match self.state {
            CircuitState::Closed => {
                self.failure_count += 1;
                if self.failure_count >= self.config.failure_threshold {
                    self.state = CircuitState::Open;
                    self.last_failure = Some(Instant::now());
                    warn!(failures = self.failure_count, "circuit breaker opened");
                }
            }
            CircuitState::HalfOpen => {
                self.state = CircuitState::Open;
                self.last_failure = Some(Instant::now());
                self.success_count = 0;
                warn!("circuit breaker reopened from half-open");
            }
            CircuitState::Open => {}
        }
    }

    pub fn reset(&mut self) {
        self.state = CircuitState::Closed;
        self.failure_count = 0;
        self.success_count = 0;
        self.last_failure = None;
        self.half_open_requests = 0;
    }
}
