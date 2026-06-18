use async_trait::async_trait;
use sdkwork_discovery_contract::{DiscoveryEvent, DiscoveryResult, WatchEventsQuery};
use sdkwork_discovery_storage_contract::WatchEventStore;
use sqlx::Row;

use crate::codec::{event_from_row, sqlx_error};
use crate::sql;
use crate::store::PostgresDiscoveryStore;
use crate::validation::{to_i64, usize_to_i64, validate_non_empty};

#[async_trait]
impl WatchEventStore for PostgresDiscoveryStore {
    async fn watch_events(&self, query: WatchEventsQuery) -> DiscoveryResult<Vec<DiscoveryEvent>> {
        validate_watch_query(&query)?;

        let rows = sqlx::query(sql::SELECT_WATCH_EVENTS)
            .bind(&query.namespace)
            .bind(&query.environment)
            .bind(to_i64("from_revision", query.from_revision)?)
            .bind(usize_to_i64("max_events", query.max_events)?)
            .fetch_all(self.pool())
            .await
            .map_err(sqlx_error)?;

        let events: Vec<DiscoveryEvent> = rows
            .iter()
            .map(event_from_row)
            .collect::<DiscoveryResult<Vec<_>>>()?
            .into_iter()
            .filter(|event| query.matches_event(event))
            .collect();
        Ok(events)
    }

    async fn gc_watch_events(
        &mut self,
        before_revision: u64,
        max_deletes: usize,
    ) -> DiscoveryResult<usize> {
        let result = sqlx::query(sql::GC_WATCH_EVENTS)
            .bind(to_i64("before_revision", before_revision)?)
            .bind(usize_to_i64("max_deletes", max_deletes)?)
            .execute(self.pool())
            .await
            .map_err(sqlx_error)?;
        Ok(result.rows_affected() as usize)
    }

    async fn compact_watch_events(
        &mut self,
        namespace: &str,
        environment: &str,
        max_events_per_resource: usize,
    ) -> DiscoveryResult<usize> {
        validate_non_empty("namespace", namespace)?;
        validate_non_empty("environment", environment)?;

        let before = sqlx::query("SELECT COUNT(*) as cnt FROM discovery_watch_event WHERE namespace = $1 AND environment = $2 AND deleted_at IS NULL")
            .bind(namespace)
            .bind(environment)
            .fetch_one(self.pool())
            .await
            .map_err(sqlx_error)?;
        let before_count: i64 = before.try_get("cnt").map_err(sqlx_error)?;

        sqlx::query(sql::COMPACT_WATCH_EVENTS)
            .bind(namespace)
            .bind(environment)
            .bind(usize_to_i64(
                "max_events_per_resource",
                max_events_per_resource,
            )?)
            .bind(namespace)
            .bind(environment)
            .execute(self.pool())
            .await
            .map_err(sqlx_error)?;

        let after = sqlx::query("SELECT COUNT(*) as cnt FROM discovery_watch_event WHERE namespace = $1 AND environment = $2 AND deleted_at IS NULL")
            .bind(namespace)
            .bind(environment)
            .fetch_one(self.pool())
            .await
            .map_err(sqlx_error)?;
        let after_count: i64 = after.try_get("cnt").map_err(sqlx_error)?;

        Ok((before_count - after_count).max(0) as usize)
    }
}

fn validate_watch_query(query: &WatchEventsQuery) -> DiscoveryResult<()> {
    validate_non_empty("namespace", &query.namespace)?;
    validate_non_empty("environment", &query.environment)?;
    validate_optional_filter("service_name", query.service_name.as_deref())?;
    validate_optional_filter("config_group", query.config_group.as_deref())?;
    validate_optional_filter("config_application", query.config_application.as_deref())?;
    if query.max_events == 0 {
        return Err(sdkwork_discovery_contract::DiscoveryError::InvalidArgument(
            "max_events must be greater than zero".to_string(),
        ));
    }
    Ok(())
}

fn validate_optional_filter(field: &str, value: Option<&str>) -> DiscoveryResult<()> {
    if let Some(value) = value {
        validate_non_empty(field, value)?;
    }
    Ok(())
}
