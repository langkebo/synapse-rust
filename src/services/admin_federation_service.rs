use crate::common::ApiError;
use crate::services::federation_blacklist_service::{AddBlacklistRequest, FederationBlacklistService};
use crate::storage::federation_blacklist::FederationBlacklistStorage;
use serde::Serialize;
use std::sync::Arc;
use tracing::{info, warn};

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

#[derive(Debug, sqlx::FromRow)]
struct DestinationRow {
    server_name: Option<String>,
    last_failed_connect_at: Option<i64>,
    last_successful_connect_at: Option<i64>,
    failure_count: Option<i32>,
    status: Option<String>,
    updated_ts: Option<i64>,
}

pub struct AdminFederationService {
    pool: Arc<sqlx::PgPool>,
    federation_blacklist_storage: Arc<FederationBlacklistStorage>,
    federation_blacklist_service: Arc<FederationBlacklistService>,
}

impl AdminFederationService {
    pub fn new(
        pool: Arc<sqlx::PgPool>,
        federation_blacklist_storage: Arc<FederationBlacklistStorage>,
        federation_blacklist_service: Arc<FederationBlacklistService>,
    ) -> Self {
        Self { pool, federation_blacklist_storage, federation_blacklist_service }
    }

    pub async fn list_destinations(
        &self,
        limit: i32,
        cursor: Option<DestinationCursor>,
        legacy_offset: Option<i64>,
    ) -> Result<(Vec<DestinationInfo>, i64, Option<DestinationCursor>, Option<i64>), ApiError> {
        let total: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM federation_servers")
            .fetch_one(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?
            .unwrap_or(0);

        let fetch_limit = limit as i64 + 1;
        let rows = if let Some(ref cursor) = cursor {
            sqlx::query_as!(
                DestinationRow,
                r#"SELECT server_name AS "server_name?", last_failed_connect_at AS "last_failed_connect_at?", last_successful_connect_at AS "last_successful_connect_at?", failure_count AS "failure_count?", status AS "status?", updated_ts AS "updated_ts?" FROM federation_servers WHERE server_name > $1 ORDER BY server_name ASC LIMIT $2"#,
                cursor.server_name,
                fetch_limit,
            )
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?
        } else {
            let offset = legacy_offset.unwrap_or(0);
            sqlx::query_as!(
                DestinationRow,
                r#"SELECT server_name AS "server_name?", last_failed_connect_at AS "last_failed_connect_at?", last_successful_connect_at AS "last_successful_connect_at?", failure_count AS "failure_count?", status AS "status?", updated_ts AS "updated_ts?" FROM federation_servers ORDER BY server_name ASC OFFSET $1 LIMIT $2"#,
                offset,
                fetch_limit,
            )
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?
        };

        let has_more = rows.len() as i64 > limit as i64;
        let visible_rows = rows.into_iter().take(limit as usize).collect::<Vec<_>>();
        let next_batch = if has_more {
            visible_rows.last().and_then(|row| {
                row.server_name
                    .as_ref()
                    .map(|server_name| DestinationCursor { server_name: server_name.clone() })
            })
        } else {
            None
        };
        let next_from = if cursor.is_none() && has_more {
            Some(legacy_offset.unwrap_or(0) + limit as i64)
        } else {
            None
        };

        Ok((visible_rows.iter().map(map_destination_row).collect(), total, next_batch, next_from))
    }

    pub async fn get_destination(&self, destination: &str) -> Result<Option<DestinationInfo>, ApiError> {
        let destination = sqlx::query_as!(
            DestinationRow,
            r#"SELECT server_name AS "server_name?", last_failed_connect_at AS "last_failed_connect_at?", last_successful_connect_at AS "last_successful_connect_at?", failure_count AS "failure_count?", status AS "status?", updated_ts AS "updated_ts?" FROM federation_servers WHERE server_name = $1"#,
            destination,
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        Ok(destination.as_ref().map(map_destination_row))
    }

    pub async fn reset_connection(&self, destination: &str) -> Result<(), ApiError> {
        let result = sqlx::query!(
            "UPDATE federation_servers SET last_failed_connect_at = NULL, failure_count = 0 WHERE server_name = $1",
            destination,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        if result.rows_affected() == 0 {
            return Err(ApiError::not_found("Destination not found".to_string()));
        }

        Ok(())
    }

    pub async fn delete_destination(&self, destination: &str) -> Result<(), ApiError> {
        let result = sqlx::query!("DELETE FROM federation_servers WHERE server_name = $1", destination)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        if result.rows_affected() == 0 {
            return Err(ApiError::not_found("Destination not found".to_string()));
        }

        Ok(())
    }

    pub async fn get_destination_rooms(&self, destination: &str) -> Result<Vec<String>, ApiError> {
        let exists = sqlx::query_scalar!("SELECT EXISTS(SELECT 1 FROM federation_servers WHERE server_name = $1)", destination)
            .fetch_one(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?
            .unwrap_or(false);

        if !exists {
            return Err(ApiError::not_found("Destination not found".to_string()));
        }

        let rooms = sqlx::query!(
            "SELECT DISTINCT room_id FROM federation_queue WHERE destination = $1 AND room_id IS NOT NULL ORDER BY room_id",
            destination,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        Ok(rooms.iter().filter_map(|row| row.room_id.clone()).collect())
    }

    pub async fn rewrite_federation(
        &self,
        from_server: &str,
        to_server: &str,
        rewritten_by: &str,
    ) -> Result<usize, ApiError> {
        let exists = sqlx::query_scalar!("SELECT EXISTS(SELECT 1 FROM federation_servers WHERE server_name = $1)", from_server)
            .fetch_one(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?
            .unwrap_or(false);

        if !exists {
            return Err(ApiError::not_found(format!("Source server {from_server} not found")));
        }

        let rooms = sqlx::query!(
            "SELECT DISTINCT room_id FROM events WHERE sender LIKE $1 AND state_key IS NOT NULL",
            format!("%:{from_server}")
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        info!(
            "Federation rewrite from {} to {}: {} rooms affected by {}",
            from_server,
            to_server,
            rooms.len(),
            rewritten_by
        );

        Ok(rooms.len())
    }

    pub async fn resolve_federation(&self, server_name: &str) -> Result<ResolveFederationResult, ApiError> {
        let blacklist = self.federation_blacklist_service.check_server(server_name).await?;
        let in_destinations =
            sqlx::query_scalar!("SELECT EXISTS(SELECT 1 FROM federation_servers WHERE server_name = $1)", server_name)
                .fetch_one(&*self.pool)
                .await
                .map_err(|e| ApiError::internal_with_log("Database error", &e))?
                .unwrap_or(false);

        Ok(ResolveFederationResult {
            resolved: in_destinations && !blacklist.is_blocked,
            blacklisted: blacklist.is_blocked,
            in_destinations,
        })
    }

    pub async fn confirm_federation(
        &self,
        server_name: &str,
        accept: bool,
        admin_user_id: &str,
    ) -> Result<ConfirmFederationResult, ApiError> {
        let now = chrono::Utc::now().timestamp_millis();
        let new_status = if accept { "active" } else { "rejected" };

        let existing = sqlx::query!(
            r#"SELECT status AS "status?" FROM federation_servers WHERE server_name = $1"#,
            server_name,
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        let previous_status = match existing {
            Some(row) => row.status.unwrap_or_else(|| "active".to_string()),
            None => {
                return Err(ApiError::not_found(format!(
                    "Server '{}' not found in federation registry",
                    server_name
                )));
            }
        };

        if previous_status != "pending" {
            return Err(ApiError::bad_request(format!(
                "Server '{}' is not pending admission (current status: {})",
                server_name, previous_status
            )));
        }

        sqlx::query!(
            "UPDATE federation_servers SET status = $1, updated_ts = $2 WHERE server_name = $3",
            new_status,
            now,
            server_name
        )
        .execute(&*self.pool)
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
                warn!("Failed to add rejected server to blacklist: {}", e);
            }
        }

        Ok(ConfirmFederationResult {
            status: new_status.to_string(),
            previous_status,
            updated_ts: now,
        })
    }

    pub async fn list_pending_federation(
        &self,
        limit: i32,
        cursor: Option<PendingFederationCursor>,
    ) -> Result<(Vec<PendingFederationInfo>, i64, Option<PendingFederationCursor>), ApiError> {
        let pending = sqlx::query!(
            "SELECT server_name, failure_count, last_failed_connect_at, last_successful_connect_at, updated_ts \
             FROM federation_servers WHERE status = 'pending' \
               AND (($1::BIGINT IS NULL AND $2::TEXT IS NULL)
                 OR COALESCE(updated_ts, 0) < $1
                 OR (COALESCE(updated_ts, 0) = $1 AND server_name < $2)) \
             ORDER BY COALESCE(updated_ts, 0) DESC, server_name DESC \
             LIMIT $3",
            cursor.as_ref().map(|cursor| cursor.updated_ts),
            cursor.as_ref().map(|cursor| cursor.server_name.as_str()),
            limit as i64
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        let total: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM federation_servers WHERE status = 'pending'")
            .fetch_one(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?
            .unwrap_or(0);

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

    pub async fn remove_from_blacklist(&self, server_name: &str, admin_user_id: &str) -> Result<(), ApiError> {
        let existing = self.federation_blacklist_storage.get_blacklist_entry(server_name).await?;
        if !existing.as_ref().is_some_and(|entry| entry.is_enabled) {
            return Err(ApiError::not_found("Blacklist entry not found".to_string()));
        }

        self.federation_blacklist_service.remove_from_blacklist(server_name, admin_user_id).await
    }

    pub async fn get_federation_cache(&self) -> Result<Vec<FederationCacheEntry>, ApiError> {
        let cache = sqlx::query!(
            r#"SELECT key AS "key!", value AS "value?", expiry_ts AS "expiry_ts?" FROM federation_cache ORDER BY key"#
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        Ok(cache
            .iter()
            .map(|row| FederationCacheEntry {
                key: row.key.clone(),
                value: row.value.clone().and_then(|v| serde_json::from_str(&v).ok()),
                expiry_ts: row.expiry_ts,
            })
            .collect())
    }

    pub async fn delete_federation_cache_entry(&self, key: &str) -> Result<(), ApiError> {
        let result = sqlx::query!("DELETE FROM federation_cache WHERE key = $1", key)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        if result.rows_affected() == 0 {
            return Err(ApiError::not_found("Cache entry not found".to_string()));
        }

        Ok(())
    }

    pub async fn clear_federation_cache(&self) -> Result<u64, ApiError> {
        let result = sqlx::query!("DELETE FROM federation_cache")
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;
        Ok(result.rows_affected())
    }
}

fn map_destination_row(row: &DestinationRow) -> DestinationInfo {
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
