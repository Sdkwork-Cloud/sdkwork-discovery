use std::collections::HashMap;
use std::time::{Duration, Instant};

use sdkwork_discovery_contract::{DiscoveryError, DiscoveryResult};

#[derive(Debug)]
pub(crate) struct StaleReadCache<T: Clone> {
    entries: HashMap<String, (Instant, T)>,
}

impl<T: Clone> Default for StaleReadCache<T> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }
}

impl<T: Clone> StaleReadCache<T> {
    pub fn resolve(
        &mut self,
        key: String,
        max_age: Duration,
        serve_stale: bool,
        result: DiscoveryResult<T>,
    ) -> DiscoveryResult<T> {
        match result {
            Ok(value) => {
                self.entries.insert(key, (Instant::now(), value.clone()));
                Ok(value)
            }
            Err(error) if serve_stale && is_storage_unavailable(&error) => {
                if let Some((cached_at, value)) = self.entries.get(&key) {
                    if cached_at.elapsed() <= max_age {
                        return Ok(value.clone());
                    }
                }
                Err(error)
            }
            Err(error) => Err(error),
        }
    }
}

fn is_storage_unavailable(error: &DiscoveryError) -> bool {
    matches!(error, DiscoveryError::Unavailable(_))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serves_cached_value_when_storage_is_unavailable() {
        let mut cache = StaleReadCache::<String>::default();
        let key = "read-1".to_string();

        assert_eq!(
            cache
                .resolve(
                    key.clone(),
                    Duration::from_secs(60),
                    true,
                    Ok("fresh".to_string()),
                )
                .unwrap(),
            "fresh"
        );

        assert_eq!(
            cache
                .resolve(
                    key,
                    Duration::from_secs(60),
                    true,
                    Err(DiscoveryError::Unavailable(
                        "postgres unavailable".to_string()
                    )),
                )
                .unwrap(),
            "fresh"
        );
    }

    #[test]
    fn does_not_serve_stale_value_when_degradation_is_disabled() {
        let mut cache = StaleReadCache::<String>::default();
        let key = "read-2".to_string();

        let _ = cache.resolve(
            key.clone(),
            Duration::from_secs(60),
            true,
            Ok("fresh".to_string()),
        );

        let error = cache
            .resolve(
                key,
                Duration::from_secs(60),
                false,
                Err(DiscoveryError::Unavailable(
                    "postgres unavailable".to_string(),
                )),
            )
            .unwrap_err();

        assert!(matches!(error, DiscoveryError::Unavailable(_)));
    }
}
