use super::*;

use crate::admin_federation::{
    AdminFederationStoreApi, FederationCacheRecord, FederationDestinationRecord, PendingFederationRecord,
};

/// In-memory [`AdminFederationStoreApi`].
///
/// Backs the `federation_servers` table with a `HashMap` keyed on
/// `server_name` and the `federation_cache` table with a `HashMap` keyed on the
/// cache key. Methods that would query unrelated tables in production
/// (`get_destination_rooms` → `federation_queue`,
/// `count_distinct_rooms_by_sender_server` → `events`) return empty/zero
/// results, since those tables are not modelled here.
#[derive(Clone, Debug, Default)]
pub struct InMemoryAdminFederationStore {
    destinations: Arc<RwLock<HashMap<String, FederationDestinationRecord>>>,
    cache: Arc<RwLock<HashMap<String, FederationCacheRecord>>>,
}

impl InMemoryAdminFederationStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl AdminFederationStoreApi for InMemoryAdminFederationStore {
    async fn count_destinations(&self) -> Result<i64, sqlx::Error> {
        Ok(self.destinations.read().await.len() as i64)
    }

    async fn list_destinations(
        &self,
        after_server_name: Option<&str>,
        limit: i64,
    ) -> Result<Vec<FederationDestinationRecord>, sqlx::Error> {
        let destinations = self.destinations.read().await;
        let mut names: Vec<&String> = destinations.keys().collect();
        names.sort();
        Ok(names
            .into_iter()
            .filter(|name| after_server_name.is_none_or(|after| name.as_str() > after))
            .take(limit.max(0) as usize)
            .map(|name| destinations[name].clone())
            .collect())
    }

    async fn get_destination(&self, server_name: &str) -> Result<Option<FederationDestinationRecord>, sqlx::Error> {
        Ok(self.destinations.read().await.get(server_name).cloned())
    }

    async fn reset_connection(&self, server_name: &str) -> Result<u64, sqlx::Error> {
        let mut destinations = self.destinations.write().await;
        match destinations.get_mut(server_name) {
            Some(record) => {
                record.last_failed_connect_at = None;
                record.failure_count = Some(0);
                Ok(1)
            }
            None => Ok(0),
        }
    }

    async fn delete_destination(&self, server_name: &str) -> Result<u64, sqlx::Error> {
        Ok(if self.destinations.write().await.remove(server_name).is_some() { 1 } else { 0 })
    }

    async fn destination_exists(&self, server_name: &str) -> Result<bool, sqlx::Error> {
        Ok(self.destinations.read().await.contains_key(server_name))
    }

    async fn get_destination_rooms(&self, _server_name: &str) -> Result<Vec<String>, sqlx::Error> {
        // Production reads `federation_queue`, which this mock does not model.
        Ok(Vec::new())
    }

    async fn count_distinct_rooms_by_sender_server(&self, _server_name: &str) -> Result<i64, sqlx::Error> {
        // Production reads `events`, which this mock does not model.
        Ok(0)
    }

    async fn get_destination_status(&self, server_name: &str) -> Result<Option<String>, sqlx::Error> {
        Ok(self
            .destinations
            .read()
            .await
            .get(server_name)
            .map(|record| record.status.clone().unwrap_or_else(|| "active".to_string())))
    }

    async fn get_server_admission_status(&self, server_name: &str) -> Result<Option<Option<String>>, sqlx::Error> {
        Ok(self.destinations.read().await.get(server_name).map(|record| record.status.clone()))
    }

    async fn insert_pending_server(&self, server_name: &str, now_ts: i64) -> Result<u64, sqlx::Error> {
        let mut destinations = self.destinations.write().await;
        if destinations.contains_key(server_name) {
            return Ok(0);
        }
        destinations.insert(
            server_name.to_string(),
            FederationDestinationRecord {
                server_name: Some(server_name.to_string()),
                last_failed_connect_at: None,
                last_successful_connect_at: None,
                failure_count: None,
                status: Some("pending".to_string()),
                updated_ts: Some(now_ts),
            },
        );
        Ok(1)
    }

    async fn update_destination_status(
        &self,
        server_name: &str,
        status: &str,
        updated_ts: i64,
    ) -> Result<u64, sqlx::Error> {
        let mut destinations = self.destinations.write().await;
        match destinations.get_mut(server_name) {
            Some(record) => {
                record.status = Some(status.to_string());
                record.updated_ts = Some(updated_ts);
                Ok(1)
            }
            None => Ok(0),
        }
    }

    async fn list_pending_federation(
        &self,
        updated_ts: Option<i64>,
        server_name: Option<&str>,
        limit: i64,
    ) -> Result<Vec<PendingFederationRecord>, sqlx::Error> {
        let no_cursor = updated_ts.is_none() && server_name.is_none();
        let cursor_ts = updated_ts.unwrap_or(0);
        let cursor_name = server_name.unwrap_or("");

        let destinations = self.destinations.read().await;
        let mut pending: Vec<&FederationDestinationRecord> = destinations
            .values()
            .filter(|record| record.status.as_deref() == Some("pending"))
            .filter(|record| {
                if no_cursor {
                    return true;
                }
                let ts = record.updated_ts.unwrap_or(0);
                let name = record.server_name.as_deref().unwrap_or("");
                ts < cursor_ts || (ts == cursor_ts && name < cursor_name)
            })
            .collect();
        // ORDER BY COALESCE(updated_ts, 0) DESC, server_name DESC
        pending.sort_by(|a, b| {
            let a_ts = a.updated_ts.unwrap_or(0);
            let b_ts = b.updated_ts.unwrap_or(0);
            b_ts.cmp(&a_ts).then_with(|| b.server_name.cmp(&a.server_name))
        });
        Ok(pending
            .into_iter()
            .take(limit.max(0) as usize)
            .map(|record| PendingFederationRecord {
                server_name: record.server_name.clone().unwrap_or_default(),
                failure_count: record.failure_count,
                last_failed_connect_at: record.last_failed_connect_at,
                last_successful_connect_at: record.last_successful_connect_at,
                updated_ts: record.updated_ts,
            })
            .collect())
    }

    async fn count_pending_federation(&self) -> Result<i64, sqlx::Error> {
        Ok(self.destinations.read().await.values().filter(|record| record.status.as_deref() == Some("pending")).count()
            as i64)
    }

    async fn get_federation_cache(&self) -> Result<Vec<FederationCacheRecord>, sqlx::Error> {
        let mut records: Vec<_> = self.cache.read().await.values().cloned().collect();
        records.sort_by(|a, b| a.key.cmp(&b.key));
        Ok(records)
    }

    async fn delete_federation_cache_entry(&self, key: &str) -> Result<u64, sqlx::Error> {
        Ok(if self.cache.write().await.remove(key).is_some() { 1 } else { 0 })
    }

    async fn clear_federation_cache(&self) -> Result<u64, sqlx::Error> {
        let mut cache = self.cache.write().await;
        let count = cache.len() as u64;
        cache.clear();
        Ok(count)
    }
}
