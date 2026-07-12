use crate::federation_blacklist_service::{AddBlacklistRequest, FederationBlacklistService};
use serde::Serialize;
use std::sync::Arc;
use synapse_common::ApiError;
use synapse_storage::{
    admin_federation::{AdminFederationStoreApi, FederationCacheRecord, FederationDestinationRecord},
    federation_blacklist::FederationBlacklistStoreApi,
};
use tracing::{info, instrument, warn};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DestinationCursor {
    pub server_name: String,
}

pub fn decode_destination_cursor(cursor: Option<&str>) -> Option<DestinationCursor> {
    let cursor = cursor?;
    let server_name = cursor.strip_prefix("v1|")?;
    if server_name.is_empty() {
        return None;
    }
    Some(DestinationCursor { server_name: server_name.to_string() })
}

pub fn encode_destination_cursor(cursor: &DestinationCursor) -> String {
    format!("v1|{}", cursor.server_name)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingFederationCursor {
    pub updated_ts: i64,
    pub server_name: String,
}

pub fn decode_pending_federation_cursor(cursor: Option<&str>) -> Option<PendingFederationCursor> {
    let cursor = cursor?;
    let (updated_ts, server_name) = cursor.split_once('|')?;
    let updated_ts = updated_ts.parse::<i64>().ok()?;
    if server_name.is_empty() {
        return None;
    }
    Some(PendingFederationCursor { updated_ts, server_name: server_name.to_string() })
}

pub fn encode_pending_federation_cursor(cursor: &PendingFederationCursor) -> String {
    format!("{}|{}", cursor.updated_ts, cursor.server_name)
}

#[derive(Debug, Clone, Serialize)]
pub struct DestinationInfo {
    pub destination: Option<String>,
    pub retry_last_ts: Option<i64>,
    pub retry_interval: Option<i64>,
    pub failure_ts: Option<i64>,
    pub last_successful_ts: Option<i64>,
    pub failure_count: i32,
    pub status: String,
    pub updated_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PendingFederationInfo {
    pub server_name: String,
    pub failure_count: i32,
    pub last_failed_connect_at: Option<i64>,
    pub last_successful_connect_at: Option<i64>,
    pub status: String,
    pub updated_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FederationCacheEntry {
    pub key: String,
    pub value: Option<serde_json::Value>,
    pub expiry_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResolveFederationResult {
    pub resolved: bool,
    pub blacklisted: bool,
    pub in_destinations: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfirmFederationResult {
    pub status: String,
    pub previous_status: String,
    pub updated_ts: i64,
}

type DestinationListResult = Result<(Vec<DestinationInfo>, i64, Option<DestinationCursor>), ApiError>;
type PendingFederationListResult = Result<(Vec<PendingFederationInfo>, i64, Option<PendingFederationCursor>), ApiError>;

pub struct AdminFederationService {
    storage: Arc<dyn AdminFederationStoreApi>,
    federation_blacklist_storage: Arc<dyn FederationBlacklistStoreApi>,
    federation_blacklist_service: Arc<FederationBlacklistService>,
}

impl AdminFederationService {
    pub fn new(
        storage: Arc<dyn AdminFederationStoreApi>,
        federation_blacklist_storage: Arc<dyn FederationBlacklistStoreApi>,
        federation_blacklist_service: Arc<FederationBlacklistService>,
    ) -> Self {
        Self { storage, federation_blacklist_storage, federation_blacklist_service }
    }

    #[instrument(skip(self))]
    pub async fn list_destinations(&self, limit: i32, cursor: Option<DestinationCursor>) -> DestinationListResult {
        let total =
            self.storage.count_destinations().await.map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        let fetch_limit = limit as i64 + 1;
        let rows = self
            .storage
            .list_destinations(cursor.as_ref().map(|cursor| cursor.server_name.as_str()), fetch_limit)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        let has_more = rows.len() as i64 > limit as i64;
        let visible_rows = rows.into_iter().take(limit as usize).collect::<Vec<_>>();
        let next_batch = if has_more {
            visible_rows.last().and_then(|row| {
                row.server_name.as_ref().map(|server_name| DestinationCursor { server_name: server_name.clone() })
            })
        } else {
            None
        };

        Ok((visible_rows.iter().map(map_destination_row).collect(), total, next_batch))
    }

    #[instrument(skip(self))]
    pub async fn get_destination(&self, destination: &str) -> Result<Option<DestinationInfo>, ApiError> {
        let destination = self
            .storage
            .get_destination(destination)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        Ok(destination.as_ref().map(map_destination_row))
    }

    #[instrument(skip(self))]
    pub async fn reset_connection(&self, destination: &str) -> Result<(), ApiError> {
        let rows_affected = self
            .storage
            .reset_connection(destination)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        if rows_affected == 0 {
            return Err(ApiError::not_found("Destination not found".to_string()));
        }

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn delete_destination(&self, destination: &str) -> Result<(), ApiError> {
        let rows_affected = self
            .storage
            .delete_destination(destination)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        if rows_affected == 0 {
            return Err(ApiError::not_found("Destination not found".to_string()));
        }

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_destination_rooms(&self, destination: &str) -> Result<Vec<String>, ApiError> {
        let exists = self
            .storage
            .destination_exists(destination)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        if !exists {
            return Err(ApiError::not_found("Destination not found".to_string()));
        }

        self.storage
            .get_destination_rooms(destination)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))
    }

    #[instrument(skip(self))]
    pub async fn rewrite_federation(
        &self,
        from_server: &str,
        to_server: &str,
        rewritten_by: &str,
    ) -> Result<usize, ApiError> {
        let exists = self
            .storage
            .destination_exists(from_server)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        if !exists {
            return Err(ApiError::not_found(format!("Source server {from_server} not found")));
        }

        let room_count = self
            .storage
            .count_distinct_rooms_by_sender_server(from_server)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        info!(
            "Federation rewrite from {} to {}: {} rooms affected by {}",
            from_server, to_server, room_count, rewritten_by
        );

        Ok(room_count as usize)
    }

    #[instrument(skip(self))]
    pub async fn resolve_federation(&self, server_name: &str) -> Result<ResolveFederationResult, ApiError> {
        let blacklist = self.federation_blacklist_service.check_server(server_name).await?;
        let in_destinations = self
            .storage
            .destination_exists(server_name)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        Ok(ResolveFederationResult {
            resolved: in_destinations && !blacklist.is_blocked,
            blacklisted: blacklist.is_blocked,
            in_destinations,
        })
    }

    #[instrument(skip(self))]
    pub async fn confirm_federation(
        &self,
        server_name: &str,
        accept: bool,
        admin_user_id: &str,
    ) -> Result<ConfirmFederationResult, ApiError> {
        let now = chrono::Utc::now().timestamp_millis();
        let new_status = if accept { "active" } else { "rejected" };

        let existing = self
            .storage
            .get_destination_status(server_name)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        let previous_status = match existing {
            Some(status) => status,
            None => {
                return Err(ApiError::not_found(format!("Server '{}' not found in federation registry", server_name)));
            }
        };

        if previous_status != "pending" {
            return Err(ApiError::bad_request(format!(
                "Server '{}' is not pending admission (current status: {})",
                server_name, previous_status
            )));
        }

        self.storage
            .update_destination_status(server_name, new_status, now)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        if !accept {
            let request = AddBlacklistRequest {
                server_name: server_name.to_string(),
                block_type: "blacklist".to_string(),
                reason: Some("Rejected federation admission request".to_string()),
                expires_in_days: None,
            };
            if let Err(e) = self.federation_blacklist_service.add_to_blacklist(request, admin_user_id).await {
                warn!(
                    error = %e,
                    server_name = %server_name,
                    admin_user_id = %admin_user_id,
                    block_type = %"blacklist",
                    "Failed to add rejected server to blacklist"
                );
            }
        }

        Ok(ConfirmFederationResult { status: new_status.to_string(), previous_status, updated_ts: now })
    }

    #[instrument(skip(self))]
    pub async fn list_pending_federation(
        &self,
        limit: i32,
        cursor: Option<PendingFederationCursor>,
    ) -> PendingFederationListResult {
        let pending = self
            .storage
            .list_pending_federation(
                cursor.as_ref().map(|cursor| cursor.updated_ts),
                cursor.as_ref().map(|cursor| cursor.server_name.as_str()),
                limit as i64,
            )
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        let total = self
            .storage
            .count_pending_federation()
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        let list: Vec<PendingFederationInfo> = pending
            .iter()
            .map(|row| PendingFederationInfo {
                server_name: row.server_name.clone(),
                failure_count: row.failure_count.unwrap_or_default(),
                last_failed_connect_at: row.last_failed_connect_at,
                last_successful_connect_at: row.last_successful_connect_at,
                status: "pending".to_string(),
                updated_ts: row.updated_ts,
            })
            .collect();

        let next_batch = if pending.len() as i32 == limit {
            pending.last().map(|row| PendingFederationCursor {
                updated_ts: row.updated_ts.unwrap_or_default(),
                server_name: row.server_name.clone(),
            })
        } else {
            None
        };

        Ok((list, total, next_batch))
    }

    #[instrument(skip(self))]
    pub async fn add_to_blacklist(&self, server_name: &str, admin_user_id: &str) -> Result<(), ApiError> {
        let existing = self.federation_blacklist_storage.get_blacklist_entry(server_name).await?;
        if existing.as_ref().is_some_and(|entry| entry.is_enabled) {
            return Err(ApiError::conflict("Server is already blacklisted".to_string()));
        }

        self.federation_blacklist_service
            .add_to_blacklist(
                AddBlacklistRequest {
                    server_name: server_name.to_string(),
                    block_type: "blacklist".to_string(),
                    reason: None,
                    expires_in_days: None,
                },
                admin_user_id,
            )
            .await?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn remove_from_blacklist(&self, server_name: &str, admin_user_id: &str) -> Result<(), ApiError> {
        let existing = self.federation_blacklist_storage.get_blacklist_entry(server_name).await?;
        if !existing.as_ref().is_some_and(|entry| entry.is_enabled) {
            return Err(ApiError::not_found("Blacklist entry not found".to_string()));
        }

        self.federation_blacklist_service.remove_from_blacklist(server_name, admin_user_id).await
    }

    #[instrument(skip(self))]
    pub async fn get_federation_cache(&self) -> Result<Vec<FederationCacheEntry>, ApiError> {
        let cache =
            self.storage.get_federation_cache().await.map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        Ok(cache.iter().map(map_cache_entry).collect())
    }

    #[instrument(skip(self))]
    pub async fn delete_federation_cache_entry(&self, key: &str) -> Result<(), ApiError> {
        let rows_affected = self
            .storage
            .delete_federation_cache_entry(key)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        if rows_affected == 0 {
            return Err(ApiError::not_found("Cache entry not found".to_string()));
        }

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn clear_federation_cache(&self) -> Result<u64, ApiError> {
        let rows_affected = self
            .storage
            .clear_federation_cache()
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;
        Ok(rows_affected)
    }

    /// Federation admission probe used by the federation auth middleware.
    ///
    /// Returns `Ok(Some(status))` when the server is already known with a
    /// non-empty status (caller decides whether to allow or reject based on
    /// the status value), or `Ok(None)` when the server was previously
    /// unknown and has just been registered as `pending` (caller should
    /// reject with a "pending approval" message).
    ///
    /// Errors are mapped to `ApiError::internal_with_log` so the middleware
    /// can propagate them without leaking sqlx error details.
    #[instrument(skip(self))]
    pub async fn check_admission(&self, server_name: &str) -> Result<Option<String>, ApiError> {
        let existing = self
            .storage
            .get_server_admission_status(server_name)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        match existing {
            // Row exists with a non-NULL status.
            Some(Some(status)) => Ok(Some(status)),
            // Row exists but status is NULL — treat as active to preserve
            // the middleware's historical behaviour where a NULL status did
            // not trigger the "pending" branch.
            Some(None) => Ok(Some("active".to_string())),
            // Server is unknown: register as pending and signal the caller.
            None => {
                let now = chrono::Utc::now().timestamp_millis();
                let _ = self
                    .storage
                    .insert_pending_server(server_name, now)
                    .await
                    .map_err(|e| ApiError::internal_with_log("Database error", &e))?;
                info!("New federation server '{}' registered as pending", server_name);
                Ok(None)
            }
        }
    }
}

fn map_destination_row(row: &FederationDestinationRecord) -> DestinationInfo {
    DestinationInfo {
        destination: row.server_name.clone(),
        retry_last_ts: row.last_failed_connect_at,
        retry_interval: None,
        failure_ts: row.last_failed_connect_at,
        last_successful_ts: row.last_successful_connect_at,
        failure_count: row.failure_count.unwrap_or_default(),
        status: row.status.clone().unwrap_or_else(|| "active".to_string()),
        updated_ts: row.updated_ts,
    }
}

fn map_cache_entry(row: &FederationCacheRecord) -> FederationCacheEntry {
    FederationCacheEntry {
        key: row.key.clone(),
        value: row.value.as_ref().and_then(|value| serde_json::from_str(value).ok()),
        expiry_ts: row.expiry_ts,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        decode_destination_cursor, decode_pending_federation_cursor, encode_destination_cursor,
        encode_pending_federation_cursor, map_cache_entry, map_destination_row, DestinationCursor,
        PendingFederationCursor,
    };
    use synapse_storage::admin_federation::{FederationCacheRecord, FederationDestinationRecord};

    #[test]
    fn destination_cursor_round_trip() {
        let cursor = DestinationCursor { server_name: "matrix.example.com".to_string() };
        let encoded = encode_destination_cursor(&cursor);
        assert_eq!(decode_destination_cursor(Some(&encoded)), Some(cursor));
    }

    #[test]
    fn pending_cursor_round_trip() {
        let cursor =
            PendingFederationCursor { updated_ts: 1_700_000_000_000, server_name: "matrix.example.com".to_string() };
        let encoded = encode_pending_federation_cursor(&cursor);
        assert_eq!(decode_pending_federation_cursor(Some(&encoded)), Some(cursor));
    }

    // ── map_destination_row ────────────────────────────────────────

    fn make_dest_record() -> FederationDestinationRecord {
        FederationDestinationRecord {
            server_name: Some("matrix.example.com".to_string()),
            last_failed_connect_at: Some(1_700_000_000_000),
            last_successful_connect_at: Some(1_700_000_001_000),
            failure_count: Some(3),
            status: Some("active".to_string()),
            updated_ts: Some(1_700_000_002_000),
        }
    }

    #[test]
    fn map_destination_row_transfers_all_fields() {
        let row = make_dest_record();
        let info = map_destination_row(&row);
        assert_eq!(info.destination, Some("matrix.example.com".to_string()));
        assert_eq!(info.retry_last_ts, Some(1_700_000_000_000));
        assert_eq!(info.failure_ts, Some(1_700_000_000_000));
        assert_eq!(info.last_successful_ts, Some(1_700_000_001_000));
        assert_eq!(info.failure_count, 3);
        assert_eq!(info.status, "active");
        assert_eq!(info.updated_ts, Some(1_700_000_002_000));
    }

    #[test]
    fn map_destination_row_defaults_for_nulls() {
        let mut row = make_dest_record();
        row.failure_count = None;
        row.status = None;
        let info = map_destination_row(&row);
        assert_eq!(info.failure_count, 0);
        assert_eq!(info.status, "active");
    }

    // ── map_cache_entry ────────────────────────────────────────────

    #[test]
    fn map_cache_entry_transfers_key_and_expiry() {
        let row =
            FederationCacheRecord { key: "cache_key_1".to_string(), value: None, expiry_ts: Some(1_700_000_000_000) };
        let entry = map_cache_entry(&row);
        assert_eq!(entry.key, "cache_key_1");
        assert_eq!(entry.value, None);
        assert_eq!(entry.expiry_ts, Some(1_700_000_000_000));
    }

    #[test]
    fn map_cache_entry_parses_json_value() {
        let row = FederationCacheRecord {
            key: "key_1".to_string(),
            value: Some(r#"{"foo":"bar"}"#.to_string()),
            expiry_ts: None,
        };
        let entry = map_cache_entry(&row);
        assert_eq!(entry.key, "key_1");
        assert_eq!(entry.value, Some(serde_json::json!({"foo": "bar"})));
        assert_eq!(entry.expiry_ts, None);
    }

    #[test]
    fn map_cache_entry_handles_invalid_json() {
        let row = FederationCacheRecord {
            key: "key_1".to_string(),
            value: Some("not valid json".to_string()),
            expiry_ts: None,
        };
        let entry = map_cache_entry(&row);
        assert_eq!(entry.value, None);
    }
}
