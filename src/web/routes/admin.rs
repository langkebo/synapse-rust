use super::AppState;
use crate::common::ApiError;
use axum::{
    extract::{Path, State},
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

pub fn create_admin_router(state: AppState) -> Router {
    Router::new()
        .route("/_synapse/admin/v1/server_version", get(get_server_version))
        .route("/_synapse/admin/v1/users", get(get_users))
        .route("/_synapse/admin/v1/users/{user_id}", get(get_user))
        .route("/_synapse/admin/v1/users/{user_id}/admin", put(set_admin))
        .route(
            "/_synapse/admin/v1/users/{user_id}/deactivate",
            post(deactivate_user),
        )
        .route("/_synapse/admin/v1/rooms", get(get_rooms))
        .route("/_synapse/admin/v1/rooms/{room_id}", get(get_room))
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/delete",
            post(delete_room),
        )
        .route("/_synapse/admin/v1/purge_history", post(purge_history))
        .route("/_synapse/admin/v1/shutdown_room", post(shutdown_room))
        .route(
            "/_synapse/admin/v1/security/events",
            get(get_security_events),
        )
        .route("/_synapse/admin/v1/security/ip/blocks", get(get_ip_blocks))
        .route("/_synapse/admin/v1/security/ip/block", post(block_ip))
        .route("/_synapse/admin/v1/security/ip/unblock", post(unblock_ip))
        .route(
            "/_synapse/admin/v1/security/ip/reputation/{ip}",
            get(get_ip_reputation),
        )
        .route("/_synapse/admin/v1/status", get(get_status))
        .with_state(state)
}

#[derive(Debug, Deserialize)]
pub struct BlockIpBody {
    pub ip_address: String,
    pub reason: Option<String>,
    pub expires_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UnblockIpBody {
    pub ip_address: String,
}

#[derive(Clone)]
pub struct SecurityStorage {
    pool: Arc<sqlx::PgPool>,
}

impl SecurityStorage {
    pub fn new(pool: &Arc<sqlx::PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_tables(&self) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            CREATE TABLE IF NOT EXISTS security_events (
                id SERIAL PRIMARY KEY,
                event_type VARCHAR(255) NOT NULL,
                user_id VARCHAR(255),
                ip_address VARCHAR(255),
                user_agent TEXT,
                details JSONB,
                created_at BIGINT NOT NULL
            )
            "#
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query!(
            r#"
            CREATE TABLE IF NOT EXISTS ip_blocks (
                id SERIAL PRIMARY KEY,
                ip_address VARCHAR(255) UNIQUE NOT NULL,
                reason TEXT,
                blocked_at BIGINT NOT NULL,
                expires_at BIGINT
            )
            "#
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query!(
            r#"
            CREATE TABLE IF NOT EXISTS ip_reputation (
                id SERIAL PRIMARY KEY,
                ip_address VARCHAR(255) UNIQUE NOT NULL,
                score INTEGER DEFAULT 0,
                last_seen_at BIGINT NOT NULL,
                updated_at BIGINT NOT NULL,
                details JSONB
            )
            "#
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn log_security_event(
        &self,
        event_type: &str,
        user_id: Option<&str>,
        ip_address: Option<&str>,
        user_agent: Option<&str>,
        details: Option<Value>,
    ) -> Result<i64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp();
        let details_str = details.as_ref().and_then(|d| serde_json::to_string(d).ok());
        let result = sqlx::query!(
            r#"
            INSERT INTO security_events (event_type, user_id, ip_address, user_agent, details, created_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id
            "#,
            event_type,
            user_id,
            ip_address,
            user_agent,
            details_str,
            now
        )
        .fetch_one(&*self.pool)
        .await?;
        Ok(result.id)
    }

    pub async fn get_security_events(&self, limit: i64) -> Result<Vec<Value>, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct SecurityEventRow {
            id: i32,
            event_type: Option<String>,
            user_id: Option<String>,
            ip_address: Option<String>,
            user_agent: Option<String>,
            details: Option<String>,
            created_at: i64,
        }
        let rows: Vec<SecurityEventRow> = sqlx::query_as(
            r#"
            SELECT id, event_type, user_id, ip_address, user_agent, details, created_at
            FROM security_events
            ORDER BY created_at DESC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| {
                json!({
                    "id": r.id,
                    "event_type": r.event_type,
                    "user_id": r.user_id,
                    "ip_address": r.ip_address,
                    "user_agent": r.user_agent,
                    "details": r.details,
                    "created_at": r.created_at
                })
            })
            .collect())
    }

    pub async fn block_ip(
        &self,
        ip_address: &str,
        reason: Option<&str>,
        expires_at: Option<i64>,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp();
        sqlx::query!(
            r#"
            INSERT INTO ip_blocks (ip_address, reason, blocked_at, expires_at)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (ip_address) DO UPDATE SET
                reason = EXCLUDED.reason,
                expires_at = EXCLUDED.expires_at
            "#,
            ip_address,
            reason,
            now,
            expires_at
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn unblock_ip(&self, ip_address: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(r#"DELETE FROM ip_blocks WHERE ip_address = $1"#, ip_address)
            .execute(&*self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn get_blocked_ips(&self) -> Result<Vec<Value>, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct BlockedIpRow {
            ip_address: String,
            reason: Option<String>,
            blocked_at: i64,
            expires_at: Option<i64>,
        }
        let rows: Vec<BlockedIpRow> = sqlx::query_as(
            r#"
            SELECT ip_address, reason, blocked_at, expires_at
            FROM ip_blocks
            ORDER BY blocked_at DESC
            "#,
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| {
                json!({
                    "ip_address": r.ip_address,
                    "reason": r.reason,
                    "blocked_at": r.blocked_at,
                    "expires_at": r.expires_at
                })
            })
            .collect())
    }

    pub async fn is_ip_blocked(&self, ip_address: &str) -> Result<bool, sqlx::Error> {
        let result: Option<(i32,)> = sqlx::query_as(
            r#"
            SELECT 1 FROM ip_blocks
            WHERE ip_address = $1
            AND (expires_at IS NULL OR expires_at > $2)
            "#,
        )
        .bind(ip_address)
        .bind(chrono::Utc::now().timestamp())
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.is_some())
    }

    pub async fn get_ip_reputation(&self, ip_address: &str) -> Result<Option<Value>, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct IpReputationRow {
            ip_address: String,
            score: i32,
            last_seen_at: i64,
            updated_at: i64,
            details: Option<String>,
        }
        let row: Option<IpReputationRow> = sqlx::query_as(
            r#"
            SELECT ip_address, score, last_seen_at, updated_at, details
            FROM ip_reputation
            WHERE ip_address = $1
            "#,
        )
        .bind(ip_address)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row.map(|r| {
            json!({
                "ip_address": r.ip_address,
                "score": r.score,
                "last_seen_at": r.last_seen_at,
                "updated_at": r.updated_at,
                "details": r.details
            })
        }))
    }

    pub async fn update_ip_reputation(
        &self,
        ip_address: &str,
        score_delta: i32,
        details: Option<Value>,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp();
        sqlx::query!(
            r#"
            INSERT INTO ip_reputation (ip_address, score, last_seen_at, updated_at, details)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (ip_address) DO UPDATE SET
                score = ip_reputation.score + $2,
                last_seen_at = $3,
                updated_at = $3,
                details = COALESCE($5, ip_reputation.details)
            "#,
            ip_address,
            score_delta,
            now,
            now,
            details
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }
}

#[axum::debug_handler]
async fn get_security_events(State(state): State<AppState>) -> Json<Value> {
    let security_storage = SecurityStorage::new(&state.services.user_storage.pool);
    let events = security_storage
        .get_security_events(100)
        .await
        .unwrap_or_else(|_| vec![]);
    Json(json!({
        "events": events,
        "total": events.len()
    }))
}

#[axum::debug_handler]
async fn get_ip_blocks(State(state): State<AppState>) -> Json<Value> {
    let security_storage = SecurityStorage::new(&state.services.user_storage.pool);
    let blocked_ips = security_storage
        .get_blocked_ips()
        .await
        .unwrap_or_else(|_| vec![]);
    Json(json!({
        "blocked_ips": blocked_ips,
        "total": blocked_ips.len()
    }))
}

#[axum::debug_handler]
async fn block_ip(State(state): State<AppState>, Json(body): Json<BlockIpBody>) -> Json<Value> {
    let security_storage = SecurityStorage::new(&state.services.user_storage.pool);
    let expires_at = body.expires_at.as_ref().and_then(|e| {
        chrono::DateTime::parse_from_rfc3339(e)
            .ok()
            .map(|dt| dt.timestamp())
    });

    let result = security_storage
        .block_ip(&body.ip_address, body.reason.as_deref(), expires_at)
        .await;

    match result {
        Ok(_) => Json(json!({
            "success": true,
            "ip_address": body.ip_address
        })),
        Err(e) => Json(json!({
            "success": false,
            "error": format!("Failed to block IP: {}", e)
        })),
    }
}

#[axum::debug_handler]
async fn unblock_ip(State(state): State<AppState>, Json(body): Json<UnblockIpBody>) -> Json<Value> {
    let security_storage = SecurityStorage::new(&state.services.user_storage.pool);
    let result = security_storage.unblock_ip(&body.ip_address).await;

    match result {
        Ok(unblocked) => Json(json!({
            "success": unblocked,
            "ip_address": body.ip_address,
            "message": if unblocked { "IP unblocked" } else { "IP was not blocked" }
        })),
        Err(e) => Json(json!({
            "success": false,
            "error": format!("Failed to unblock IP: {}", e)
        })),
    }
}

#[axum::debug_handler]
async fn get_ip_reputation(State(state): State<AppState>, Path(ip): Path<String>) -> Json<Value> {
    let security_storage = SecurityStorage::new(&state.services.user_storage.pool);
    let reputation = security_storage
        .get_ip_reputation(&ip)
        .await
        .unwrap_or(None);

    match reputation {
        Some(rep) => Json(rep),
        _ => Json(json!({
            "ip_address": ip,
            "score": 0,
            "message": "No reputation data available"
        })),
    }
}

#[axum::debug_handler]
async fn get_status(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let user_count = state
        .services
        .user_storage
        .get_user_count()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get user count: {}", e)))?;

    let room_count = state
        .services
        .room_storage
        .get_room_count()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room count: {}", e)))?;

    Ok(Json(json!({
        "status": "running",
        "version": "1.0.0",
        "users": user_count,
        "rooms": room_count,
        "uptime": 0
    })))
}

#[axum::debug_handler]
async fn get_server_version() -> Json<Value> {
    Json(serde_json::json!({
        "version": "1.0.0",
        "python_version": "3.9.0"
    }))
}

#[axum::debug_handler]
async fn get_users(State(_state): State<AppState>) -> Json<Value> {
    Json(serde_json::json!({
        "users": [],
        "total": 0
    }))
}

#[axum::debug_handler]
async fn get_user(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let user = state
        .services
        .user_storage
        .get_user_by_identifier(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    match user {
        Some(u) => Ok(Json(serde_json::json!({
            "name": u.username,
            "is_guest": u.is_guest.unwrap_or(false),
            "admin": u.admin.unwrap_or(false),
            "deactivated": u.deactivated.unwrap_or(false),
            "displayname": u.displayname,
            "avatar_url": u.avatar_url,
            "creation_ts": u.creation_ts,
            "user_type": u.user_type
        }))),
        None => Err(ApiError::not_found("User not found".to_string())),
    }
}

#[axum::debug_handler]
async fn set_admin(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let admin_status = body
        .get("admin")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| ApiError::bad_request("Missing 'admin' field".to_string()))?;

    state
        .services
        .user_storage
        .set_admin_status(&user_id, admin_status)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(serde_json::json!({
        "success": true
    })))
}

#[axum::debug_handler]
async fn deactivate_user(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    state
        .services
        .user_storage
        .deactivate_user(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(serde_json::json!({
        "id_server_unbind_result": "success"
    })))
}

#[axum::debug_handler]
async fn get_rooms(State(_state): State<AppState>) -> Json<Value> {
    Json(serde_json::json!({
        "rooms": [],
        "total": 0
    }))
}

#[axum::debug_handler]
async fn get_room(State(_state): State<AppState>, Path(_room_id): Path<String>) -> Json<Value> {
    Json(serde_json::json!({
        "room_id": "!test:localhost",
        "name": "Test Room",
        "topic": "",
        "creator": "@test:localhost",
        "joined_members": 1,
        "joined_local_members": 1,
        "state_events": 0
    }))
}

#[axum::debug_handler]
async fn delete_room(State(_state): State<AppState>, Path(_room_id): Path<String>) -> Json<Value> {
    Json(serde_json::json!({
        "delete_id": "!test:localhost"
    }))
}

#[axum::debug_handler]
async fn purge_history(State(_state): State<AppState>, Json(_body): Json<Value>) -> Json<Value> {
    Json(serde_json::json!({
        "success": true
    }))
}

#[axum::debug_handler]
async fn shutdown_room(State(_state): State<AppState>, Json(_body): Json<Value>) -> Json<Value> {
    Json(serde_json::json!({
        "kicked_users": [],
        "failed_to_kick_users": [],
        "closed_room": true
    }))
}
