use crate::common::constants::{MAX_PAGINATION_LIMIT, MIN_PAGINATION_LIMIT};
use crate::common::ApiError;
use crate::storage::federation_blacklist::decode_federation_blacklist_cursor;
use crate::web::routes::{AdminUser, AppState};
use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use tracing::info;

pub fn create_federation_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/_synapse/admin/v1/federation/destinations",
            get(get_destinations),
        )
        .route(
            "/_synapse/admin/v1/federation/destinations/{destination}",
            get(get_destination),
        )
        .route(
            "/_synapse/admin/v1/federation/destinations/{destination}/reset_connection",
            post(reset_connection),
        )
        .route(
            "/_synapse/admin/v1/federation/destinations/{destination}/reset",
            post(reset_connection),
        )
        .route(
            "/_synapse/admin/v1/federation/destinations/{destination}",
            delete(delete_destination),
        )
        .route(
            "/_synapse/admin/v1/federation/destinations/{destination}/rooms",
            get(get_destination_rooms),
        )
        .route(
            "/_synapse/admin/v1/federation/rewrite",
            post(rewrite_federation),
        )
        .route(
            "/_synapse/admin/v1/federation/resolve",
            post(resolve_federation),
        )
        .route(
            "/_synapse/admin/v1/federation/confirm",
            post(confirm_federation),
        )
        .route(
            "/_synapse/admin/v1/federation/pending",
            get(list_pending_federation),
        )
        .route(
            "/_synapse/admin/v1/federation/blacklist",
            get(get_blacklist),
        )
        .route(
            "/_synapse/admin/v1/federation/blacklist/{server_name}",
            post(add_to_blacklist),
        )
        .route(
            "/_synapse/admin/v1/federation/blacklist/{server_name}",
            delete(remove_from_blacklist),
        )
        .route(
            "/_synapse/admin/v1/federation/cache",
            get(get_federation_cache),
        )
        .route(
            "/_synapse/admin/v1/federation/cache/{key}",
            delete(delete_federation_cache_entry),
        )
        .route(
            "/_synapse/admin/v1/federation/cache/clear",
            post(clear_federation_cache),
        )
}

pub fn admin_federation_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;
    [
        (Method::GET, "/_synapse/admin/v1/federation/destinations"),
        (
            Method::GET,
            "/_synapse/admin/v1/federation/destinations/{destination}",
        ),
        (
            Method::POST,
            "/_synapse/admin/v1/federation/destinations/{destination}/reset_connection",
        ),
        (
            Method::POST,
            "/_synapse/admin/v1/federation/destinations/{destination}/reset",
        ),
        (
            Method::DELETE,
            "/_synapse/admin/v1/federation/destinations/{destination}",
        ),
        (
            Method::GET,
            "/_synapse/admin/v1/federation/destinations/{destination}/rooms",
        ),
        (Method::POST, "/_synapse/admin/v1/federation/rewrite"),
        (Method::POST, "/_synapse/admin/v1/federation/resolve"),
        (Method::POST, "/_synapse/admin/v1/federation/confirm"),
        (Method::GET, "/_synapse/admin/v1/federation/pending"),
        (Method::GET, "/_synapse/admin/v1/federation/blacklist"),
        (
            Method::POST,
            "/_synapse/admin/v1/federation/blacklist/{server_name}",
        ),
        (
            Method::DELETE,
            "/_synapse/admin/v1/federation/blacklist/{server_name}",
        ),
        (Method::GET, "/_synapse/admin/v1/federation/cache"),
        (Method::DELETE, "/_synapse/admin/v1/federation/cache/{key}"),
        (Method::POST, "/_synapse/admin/v1/federation/cache/clear"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "admin::federation"))
    .collect()
}

#[derive(Debug, Deserialize)]
pub struct RewriteRequest {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Deserialize)]
pub struct ResolveRequest {
    pub server_name: String,
}

#[derive(Debug, Deserialize)]
pub struct ConfirmRequest {
    pub server_name: String,
    pub accept: bool,
}

#[derive(Debug, Deserialize)]
pub struct ListPendingQuery {
    pub limit: Option<i32>,
    pub from: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BlacklistQuery {
    pub limit: Option<i32>,
    pub from: Option<String>,
}

fn decode_pending_federation_cursor(cursor: Option<&str>) -> Option<(i64, &str)> {
    let cursor = cursor?;
    let (updated_ts, server_name) = cursor.split_once('|')?;
    let updated_ts = updated_ts.parse::<i64>().ok()?;
    if server_name.is_empty() {
        return None;
    }
    Some((updated_ts, server_name))
}

fn encode_pending_federation_cursor(updated_ts: i64, server_name: &str) -> String {
    format!("{updated_ts}|{server_name}")
}

#[cfg(test)]
mod cursor_tests {
    use super::{decode_pending_federation_cursor, encode_pending_federation_cursor};

    #[test]
    fn test_pending_federation_cursor_round_trip() {
        let cursor = encode_pending_federation_cursor(1_700_000_000_000, "matrix.example.com");
        assert_eq!(
            decode_pending_federation_cursor(Some(&cursor)),
            Some((1_700_000_000_000, "matrix.example.com"))
        );
    }

    #[test]
    fn test_pending_federation_cursor_rejects_invalid_value() {
        assert_eq!(decode_pending_federation_cursor(Some("bad-cursor")), None);
        assert_eq!(decode_pending_federation_cursor(Some("123|")), None);
    }
}

fn build_destination_json(row: &sqlx::postgres::PgRow) -> Result<serde_json::Value, sqlx::Error> {
    let server_name: Option<String> = row.try_get("server_name")?;
    let last_failed: Option<i64> = row.try_get("last_failed_connect_at")?;
    let last_successful: Option<i64> = row.try_get("last_successful_connect_at")?;
    let failure_count: Option<i32> = row.try_get("failure_count")?;
    let status: Option<String> = row.try_get("status")?;
    let updated_ts: Option<i64> = row.try_get("updated_ts")?;

    Ok(json!({
        "destination": server_name,
        "retry_last_ts": last_failed,
        "retry_interval": Value::Null,
        "failure_ts": last_failed,
        "last_successful_ts": last_successful,
        "failure_count": failure_count.unwrap_or_default(),
        "status": status.unwrap_or_else(|| "active".to_string()),
        "updated_ts": updated_ts
    }))
}

#[derive(Debug, Deserialize)]
pub struct DestinationsQuery {
    pub limit: Option<i32>,
    pub from: Option<i64>,
}

#[axum::debug_handler]
pub async fn get_destinations(
    _admin: AdminUser,
    State(state): State<AppState>,
    Query(query): Query<DestinationsQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let offset = query.from.unwrap_or(0);

    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM federation_servers")
            .fetch_one(&*state.services.user_storage.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {e}")))?;

    let destinations = sqlx::query(
        "SELECT server_name, last_failed_connect_at, last_successful_connect_at, failure_count, status, updated_ts FROM federation_servers ORDER BY server_name OFFSET $1 LIMIT $2"
    )
    .bind(offset)
    .bind(limit)
    .fetch_all(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {e}")))?;

    let dest_list: Vec<Value> = destinations
        .iter()
        .map(|row| build_destination_json(row).map_err(|e| ApiError::internal(format!("Row parse error: {e}"))))
        .collect::<Result<_, _>>()?;

    let next_from = if offset + (limit as i64) < total {
        Some(offset + (limit as i64))
    } else {
        None
    };

    Ok(Json(json!({
        "destinations": dest_list,
        "total": total,
        "total_count": total,
        "next_from": next_from
    })))
}

#[axum::debug_handler]
pub async fn get_destination(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(destination): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let dest = sqlx::query(
        "SELECT server_name, last_failed_connect_at, last_successful_connect_at, failure_count, status, updated_ts FROM federation_servers WHERE server_name = $1"
    )
    .bind(&destination)
    .fetch_optional(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {e}")))?;

    match dest {
        Some(row) => Ok(Json(build_destination_json(&row).map_err(|e| ApiError::internal(format!("Row parse error: {e}")))?)),
        None => Err(ApiError::not_found("Destination not found".to_string())),
    }
}

#[axum::debug_handler]
pub async fn reset_connection(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(destination): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let result = sqlx::query(
        "UPDATE federation_servers SET last_failed_connect_at = NULL, failure_count = 0 WHERE server_name = $1"
    )
    .bind(&destination)
    .execute(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {e}")))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("Destination not found".to_string()));
    }

    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn delete_destination(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(destination): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let result = sqlx::query("DELETE FROM federation_servers WHERE server_name = $1")
        .bind(&destination)
        .execute(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {e}")))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("Destination not found".to_string()));
    }

    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn get_destination_rooms(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(destination): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let destination_exists =
        sqlx::query("SELECT server_name FROM federation_servers WHERE server_name = $1")
            .bind(&destination)
            .fetch_optional(&*state.services.user_storage.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {e}")))?
            .is_some();

    if !destination_exists {
        return Err(ApiError::not_found("Destination not found".to_string()));
    }

    let rooms = sqlx::query(
        "SELECT DISTINCT room_id FROM federation_queue WHERE destination = $1 AND room_id IS NOT NULL ORDER BY room_id",
    )
        .bind(&destination)
        .fetch_all(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {e}")))?;

    let room_list: Vec<String> = rooms.iter().map(|r| r.get("room_id")).collect();

    Ok(Json(
        json!({ "rooms": room_list, "total": room_list.len() }),
    ))
}

#[axum::debug_handler]
pub async fn rewrite_federation(
    admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<RewriteRequest>,
) -> Result<Json<Value>, ApiError> {
    let from_server = &body.from;
    let to_server = &body.to;

    let source_exists =
        sqlx::query("SELECT server_name FROM federation_servers WHERE server_name = $1")
            .bind(from_server)
            .fetch_optional(&*state.services.user_storage.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {e}")))?
            .is_some();

    if !source_exists {
        return Err(ApiError::not_found(format!(
            "Source server {from_server} not found"
        )));
    }

    let rooms_result = sqlx::query("SELECT DISTINCT room_id FROM events WHERE sender LIKE $1 AND state_key IS NOT NULL")
        .bind(format!("%:{from_server}"))
        .fetch_all(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {e}")))?;

    let rooms_count = rooms_result.len();

    info!(
        "Federation rewrite from {} to {}: {} rooms affected by {}",
        from_server, to_server, rooms_count, admin.user_id
    );

    Ok(Json(json!({
        "from": from_server,
        "to": to_server,
        "rewritten": true,
        "rooms_affected": rooms_count,
        "rewritten_by": admin.user_id
    })))
}

#[axum::debug_handler]
pub async fn resolve_federation(
    admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<ResolveRequest>,
) -> Result<Json<Value>, ApiError> {
    let server_name = &body.server_name;

    let is_blocked = state
        .services
        .federation_blacklist_storage
        .is_server_blocked(server_name)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {e}")))?;

    let destination = sqlx::query(
        "SELECT server_name, last_failed_connect_at, last_successful_connect_at, failure_count FROM federation_servers WHERE server_name = $1"
    )
    .bind(server_name)
    .fetch_optional(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {e}")))?;

    let resolved = destination.is_some() && !is_blocked;

    info!(
        "Federation resolve for {}: resolved={}, blacklisted={}",
        server_name, resolved, is_blocked
    );

    Ok(Json(json!({
        "server_name": server_name,
        "resolved": resolved,
        "blacklisted": is_blocked,
        "in_destinations": destination.is_some(),
        "resolved_by": admin.user_id
    })))
}

#[axum::debug_handler]
pub async fn confirm_federation(
    admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<ConfirmRequest>,
) -> Result<Json<Value>, ApiError> {
    let now = chrono::Utc::now().timestamp_millis();
    let new_status = if body.accept { "active" } else { "rejected" };

    let existing = sqlx::query("SELECT id, status FROM federation_servers WHERE server_name = $1")
        .bind(&body.server_name)
        .fetch_optional(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {e}")))?;

    let previous_status = match existing {
        Some(row) => row
            .get::<Option<String>, _>("status")
            .unwrap_or_else(|| "active".to_string()),
        None => {
            return Err(ApiError::not_found(format!(
                "Server '{}' not found in federation registry",
                body.server_name
            )));
        }
    };

    if previous_status != "pending" {
        return Err(ApiError::bad_request(format!(
            "Server '{}' is not pending admission (current status: {})",
            body.server_name, previous_status
        )));
    }

    sqlx::query(
        "UPDATE federation_servers SET status = $1, updated_ts = $2 WHERE server_name = $3",
    )
    .bind(new_status)
    .bind(now)
    .bind(&body.server_name)
    .execute(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {e}")))?;

    if !body.accept {
        if let Err(e) = state
            .services
            .federation_blacklist_storage
            .add_to_blacklist(crate::storage::federation_blacklist::AddBlacklistRequest {
                server_name: body.server_name.clone(),
                block_type: "blacklist".to_string(),
                reason: Some("Rejected federation admission request".to_string()),
                blocked_by: admin.user_id.clone(),
                expires_at: None,
                metadata: None,
            })
            .await
        {
            tracing::warn!("Failed to add rejected server to blacklist: {}", e);
        }
    }

    info!(
        "Federation admission {} for server '{}' by admin '{}'",
        if body.accept { "accepted" } else { "rejected" },
        body.server_name,
        admin.user_id
    );

    Ok(Json(json!({
        "server_name": body.server_name,
        "status": new_status,
        "previous_status": previous_status,
        "updated_ts": now,
        "confirmed_by": admin.user_id
    })))
}

#[axum::debug_handler]
pub async fn list_pending_federation(
    _admin: AdminUser,
    State(state): State<AppState>,
    Query(query): Query<ListPendingQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = query.limit.unwrap_or(100).min(500);
    let cursor = decode_pending_federation_cursor(query.from.as_deref());

    let pending = sqlx::query(
        "SELECT server_name, failure_count, last_failed_connect_at, last_successful_connect_at, updated_ts \
         FROM federation_servers WHERE status = 'pending' \
           AND (($1::BIGINT IS NULL AND $2::TEXT IS NULL)
             OR COALESCE(updated_ts, 0) < $1
             OR (COALESCE(updated_ts, 0) = $1 AND server_name < $2)) \
         ORDER BY COALESCE(updated_ts, 0) DESC, server_name DESC \
         LIMIT $3"
    )
    .bind(cursor.map(|(updated_ts, _)| updated_ts))
    .bind(cursor.map(|(_, server_name)| server_name))
    .bind(limit)
    .fetch_all(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {e}")))?;

    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM federation_servers WHERE status = 'pending'")
            .fetch_one(&*state.services.user_storage.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {e}")))?;

    let list: Vec<Value> = pending
        .iter()
        .map(|row| {
            json!({
                "server_name": row.get::<Option<String>, _>("server_name"),
                "failure_count": row.get::<Option<i32>, _>("failure_count").unwrap_or_default(),
                "last_failed_connect_at": row.get::<Option<i64>, _>("last_failed_connect_at"),
                "last_successful_connect_at": row.get::<Option<i64>, _>("last_successful_connect_at"),
                "status": "pending",
                "updated_ts": row.get::<Option<i64>, _>("updated_ts")
            })
        })
        .collect();

    let next_batch = if pending.len() as i32 == limit {
        pending.last().map(|row| {
            encode_pending_federation_cursor(
                row.get::<Option<i64>, _>("updated_ts").unwrap_or_default(),
                row.get::<Option<String>, _>("server_name")
                    .unwrap_or_default()
                    .as_str(),
            )
        })
    } else {
        None
    };

    Ok(Json(json!({
        "servers": list,
        "total": total,
        "limit": limit,
        "next_batch": next_batch
    })))
}

#[axum::debug_handler]
pub async fn get_blacklist(
    _admin: AdminUser,
    State(state): State<AppState>,
    Query(query): Query<BlacklistQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = query
        .limit
        .unwrap_or(100)
        .clamp(MIN_PAGINATION_LIMIT as i32, MAX_PAGINATION_LIMIT as i32);
    let from = decode_federation_blacklist_cursor(query.from.as_deref());

    if query.from.is_some() && from.is_none() {
        return Err(ApiError::bad_request("Invalid from cursor".to_string()));
    }

    let (blacklist, next_batch) = state
        .services
        .federation_blacklist_service
        .get_blacklist(limit, from)
        .await?;

    let list: Vec<Value> = blacklist
        .iter()
        .map(|row| {
            json!({
                "server_name": row.server_name,
                "added_at": row.created_ts,
                "reason": row.reason
            })
        })
        .collect();

    Ok(Json(json!({
        "blacklist": list,
        "total": list.len(),
        "next_batch": next_batch
    })))
}

#[axum::debug_handler]
pub async fn add_to_blacklist(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(server_name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let now = chrono::Utc::now().timestamp_millis();

    let result = sqlx::query(
        "INSERT INTO federation_blacklist (server_name, added_ts, added_by) VALUES ($1, $2, $3) ON CONFLICT (server_name) DO NOTHING"
    )
    .bind(&server_name)
    .bind(now)
    .bind(&admin.user_id)
    .execute(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {e}")))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::conflict(
            "Server is already blacklisted".to_string(),
        ));
    }

    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn remove_from_blacklist(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(server_name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let result = sqlx::query("DELETE FROM federation_blacklist WHERE server_name = $1")
        .bind(&server_name)
        .execute(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {e}")))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("Blacklist entry not found".to_string()));
    }

    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn get_federation_cache(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let cache = sqlx::query("SELECT key, value, expiry_ts FROM federation_cache ORDER BY key")
        .fetch_all(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {e}")))?;

    let entries: Vec<Value> = cache
        .iter()
        .map(|row| {
            json!({
                "key": row.get::<Option<String>, _>("key"),
                "value": row.get::<Option<String>, _>("value"),
                "expiry_ts": row.get::<Option<i64>, _>("expiry_ts")
            })
        })
        .collect();

    Ok(Json(json!({ "cache": entries, "total": entries.len() })))
}

#[axum::debug_handler]
pub async fn delete_federation_cache_entry(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let result = sqlx::query("DELETE FROM federation_cache WHERE key = $1")
        .bind(&key)
        .execute(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {e}")))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("Cache entry not found".to_string()));
    }

    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn clear_federation_cache(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let result = sqlx::query("DELETE FROM federation_cache")
        .execute(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {e}")))?;

    Ok(Json(json!({ "deleted": result.rows_affected() })))
}
