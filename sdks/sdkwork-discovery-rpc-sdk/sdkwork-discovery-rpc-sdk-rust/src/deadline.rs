use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, Default)]
pub struct RpcDeadlineOptions {
    pub timeout_ms: Option<u64>,
    pub deadline: Option<SystemTime>,
    pub now: Option<SystemTime>,
}

pub fn resolve_rpc_deadline_ms(options: RpcDeadlineOptions) -> Option<u64> {
    if let Some(timeout_ms) = options.timeout_ms {
        return Some(timeout_ms);
    }

    let deadline = options.deadline?;
    let now = options.now.unwrap_or_else(SystemTime::now);
    let remaining = deadline.duration_since(now).unwrap_or(Duration::ZERO);
    Some(remaining.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn resolve_rpc_deadline_ms_prefers_timeout() {
        assert_eq!(
            resolve_rpc_deadline_ms(RpcDeadlineOptions {
                timeout_ms: Some(5_000),
                ..RpcDeadlineOptions::default()
            }),
            Some(5_000)
        );
    }

    #[test]
    fn resolve_rpc_deadline_ms_computes_remaining_time() {
        let now = UNIX_EPOCH + Duration::from_secs(10);
        let deadline = now + Duration::from_millis(2_500);

        assert_eq!(
            resolve_rpc_deadline_ms(RpcDeadlineOptions {
                deadline: Some(deadline),
                now: Some(now),
                ..RpcDeadlineOptions::default()
            }),
            Some(2_500)
        );
    }
}
