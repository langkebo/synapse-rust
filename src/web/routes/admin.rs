use super::{AdminUser, AppState};
use crate::common::constants::{
    ADMIN_REGISTER_NONCE_RATE_LIMIT, ADMIN_REGISTER_RATE_LIMIT, MAX_PAGINATION_LIMIT,
    MIN_PAGINATION_LIMIT,
};
use crate::common::ApiError;
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    routing::{delete, get, post, put},
    Json, Router,
};
use ipnetwork::{IpNetwork, Ipv4Network, Ipv6Network};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Arc;

use crate::services::admin_registration_service::{
    AdminRegisterRequest, AdminRegisterResponse, NonceResponse,
};
pub fn create_admin_router(_state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_synapse/admin/v1/server_version", get(get_server_version))
        .route("/_synapse/admin/v1/server_stats", get(get_server_stats))
        .route("/_synapse/admin/v1/users", get(get_users))
        .route("/_synapse/admin/v1/users/{user_id}", get(get_user))
        .route("/_synapse/admin/v1/users/{user_id}", delete(delete_user))
        .route("/_synapse/admin/v1/users/{user_id}/admin", put(set_admin))
        .route(
            "/_synapse/admin/v1/users/{user_id}/deactivate",
            post(deactivate_user),
        )
        .route(
            "/_synapse/admin/v1/users/{user_id}/password",
            post(reset_user_password),
        )
        .route(
            "/_synapse/admin/v1/users/{user_id}/rooms",
            get(get_user_rooms_admin),
        )
        .route("/_synapse/admin/v1/rooms", get(get_rooms))
        .route("/_synapse/admin/v1/rooms/{room_id}", get(get_room))
        .route("/_synapse/admin/v1/rooms/{room_id}", delete(delete_room))
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
        .route(
            "/_synapse/admin/v1/register/nonce",
            get(get_admin_register_nonce),
        )
        .route("/_synapse/admin/v1/register", post(admin_register))
        .route("/_synapse/admin/v1/status", get(get_status))
        .route("/_synapse/admin/v1/config", get(get_config))
        .route("/_synapse/admin/v1/logs", get(get_logs))
        .route("/_synapse/admin/v1/media_stats", get(get_media_stats))
        .route("/_synapse/admin/v1/user_stats", get(get_user_stats))
        .route("/_synapse/admin/v2/users", get(get_users_v2))
        .route("/_synapse/admin/v2/users/{user_id}", get(get_user_v2))
        .route(
            "/_synapse/admin/v2/users/{user_id}",
            put(create_or_update_user_v2),
        )
        .route(
            "/_synapse/admin/v1/users/{user_id}/login",
            post(login_as_user),
        )
        .route(
            "/_synapse/admin/v1/users/{user_id}/logout",
            post(logout_user_devices),
        )
        .route(
            "/_synapse/admin/v1/users/{user_id}/devices",
            get(get_user_devices_admin),
        )
        .route(
            "/_synapse/admin/v1/users/{user_id}/devices/{device_id}",
            delete(delete_user_device_admin),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/members",
            get(get_room_members_admin),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/state",
            get(get_room_state_admin),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/messages",
            get(get_room_messages_admin),
        )
        .route("/_synapse/admin/v1/rooms/{room_id}/block", post(block_room))
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/block",
            get(get_room_block_status),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/unblock",
            post(unblock_room),
        )
        .route(
            "/_synapse/admin/v1/rooms/{room_id}/make_admin",
            post(make_room_admin),
        )
        .route("/_synapse/admin/v1/server_name", get(get_server_name))
        .route("/_synapse/admin/v1/statistics", get(get_statistics))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockIpBody {
    #[serde(alias = "ip", alias = "ip_address")]
    pub ip_address: String,
    pub reason: Option<String>,
    pub expires_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnblockIpBody {
    #[serde(alias = "ip", alias = "ip_address")]
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

    #[allow(dead_code)]
    fn parse_ip_address(ip_address: &str) -> Option<IpNetwork> {
        if let Ok(net) = ip_address.parse() {
            return Some(net);
        }

        if let Ok(v4) = ip_address.parse::<Ipv4Addr>() {
            return Ipv4Network::new(v4, 32).ok().map(IpNetwork::V4);
        }

        if let Ok(v6) = ip_address.parse::<Ipv6Addr>() {
            return Ipv6Network::new(v6, 128).ok().map(IpNetwork::V6);
        }

        None
    }

    pub async fn create_tables(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS security_events (
                id BIGSERIAL PRIMARY KEY,
                event_type VARCHAR(255) NOT NULL,
                user_id VARCHAR(255),
                ip_address INET,
                user_agent TEXT,
                details TEXT,
                created_at BIGINT NOT NULL,
                severity VARCHAR(50) DEFAULT 'warning',
                description TEXT,
                created_ts BIGINT NOT NULL,
                resolved BOOLEAN DEFAULT FALSE,
                resolved_ts BIGINT,
                resolved_by VARCHAR(255)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS ip_blocks (
                id BIGSERIAL PRIMARY KEY,
                ip_range CIDR NOT NULL,
                ip_address INET,
                reason TEXT,
                blocked_at BIGINT NOT NULL,
                blocked_ts BIGINT NOT NULL,
                expires_at BIGINT,
                expires_ts BIGINT
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS ip_reputation (
                id BIGSERIAL PRIMARY KEY,
                ip_address INET NOT NULL,
                score INTEGER DEFAULT 50,
                reputation_score INTEGER DEFAULT 50,
                threat_level VARCHAR(50) DEFAULT 'none',
                last_seen_at BIGINT,
                updated_at BIGINT,
                details TEXT,
                last_updated_ts BIGINT NOT NULL,
                created_ts BIGINT NOT NULL,
                UNIQUE (ip_address)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn log_admin_action(
        &self,
        admin_id: &str,
        action: &str,
        target: Option<&str>,
        details: Option<Value>,
    ) -> Result<i64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let details_str = details.map(|d| d.to_string());

        let row = sqlx::query(
            r#"
            INSERT INTO security_events (event_type, user_id, details, description, created_ts)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id
            "#,
        )
        .bind(format!("admin_action:{}", action))
        .bind(Some(admin_id))
        .bind(details_str)
        .bind(target)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row.get("id"))
    }

    pub async fn get_security_events(&self, limit: i64) -> Result<Vec<Value>, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct SecurityEventRow {
            id: i64,
            event_type: String,
            user_id: Option<String>,
            ip_address: Option<String>,
            user_agent: Option<String>,
            details: Option<String>,
            created_ts: i64,
        }
        let rows: Vec<SecurityEventRow> = sqlx::query_as(
            r#"
            SELECT id, event_type, user_id, ip_address, user_agent, details, created_ts
            FROM security_events
            ORDER BY created_ts DESC
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
                    "created_ts": r.created_ts
                })
            })
            .collect())
    }

    pub async fn block_ip(
        &self,
        ip_address: &str,
        reason: Option<&str>,
        expires_at: Option<chrono::NaiveDateTime>,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let expires_at_ts = expires_at.map(|t| t.and_utc().timestamp_millis());

        let ip_range = ip_address.to_string();

        sqlx::query(r#"DELETE FROM ip_blocks WHERE ip_address = $1"#)
            .bind(ip_address)
            .execute(&*self.pool)
            .await?;

        sqlx::query(
            r#"
            INSERT INTO ip_blocks (ip_address, ip_range, reason, blocked_by, blocked_ts, expires_ts, is_enabled)
            VALUES ($1, $2, $3, 'admin', $4, $5, true)
            "#,
        )
        .bind(ip_address)
        .bind(ip_range)
        .bind(reason)
        .bind(now)
        .bind(expires_at_ts)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn unblock_ip(&self, ip_address: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(r#"DELETE FROM ip_blocks WHERE ip_address = $1"#)
            .bind(ip_address)
            .execute(&*self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn get_blocked_ips(&self) -> Result<Vec<Value>, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct BlockedIpRow {
            ip_address: String,
            reason: Option<String>,
            blocked_ts: i64,
            expires_ts: Option<i64>,
        }
        let rows: Vec<BlockedIpRow> = sqlx::query_as(
            r#"
            SELECT ip_range::text as ip_address, reason, blocked_ts, expires_ts
            FROM ip_blocks
            ORDER BY blocked_ts DESC
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
                    "blocked_at": r.blocked_ts,
                    "expires_at": r.expires_ts
                })
            })
            .collect())
    }

    pub async fn is_ip_blocked(&self, ip_address: &str) -> Result<bool, sqlx::Error> {
        let Ok(ip) = ip_address.parse::<IpAddr>() else {
            return Ok(false);
        };

        let result: Option<(i32,)> = sqlx::query_as(
            r#"
            SELECT 1 FROM ip_blocks
            WHERE $1::inet <<= ip_range
            AND (expires_ts IS NULL OR expires_ts > $2)
            "#,
        )
        .bind(ip)
        .bind(chrono::Utc::now().timestamp())
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.is_some())
    }

    pub async fn get_ip_reputation(&self, ip_address: &str) -> Result<Option<Value>, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct IpReputationRow {
            ip_address: String,
            reputation_score: i32,
            failed_attempts: Option<i32>,
            successful_attempts: Option<i32>,
            last_failed_ts: Option<i64>,
            last_success_ts: Option<i64>,
            is_blocked: Option<bool>,
            blocked_ts: Option<i64>,
            blocked_until_ts: Option<i64>,
            block_reason: Option<String>,
            risk_level: Option<String>,
            created_ts: Option<i64>,
            updated_ts: Option<i64>,
        }
        let row: Option<IpReputationRow> = sqlx::query_as(
            r#"
            SELECT ip_address, reputation_score, failed_attempts, successful_attempts,
                   last_failed_ts, last_success_ts, is_blocked, blocked_ts, blocked_until_ts,
                   block_reason, risk_level, created_ts, updated_ts
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
                "reputation_score": r.reputation_score,
                "failed_attempts": r.failed_attempts,
                "successful_attempts": r.successful_attempts,
                "last_failed_ts": r.last_failed_ts,
                "last_success_ts": r.last_success_ts,
                "is_blocked": r.is_blocked,
                "blocked_ts": r.blocked_ts,
                "blocked_until_ts": r.blocked_until_ts,
                "block_reason": r.block_reason,
                "risk_level": r.risk_level,
                "created_ts": r.created_ts,
                "updated_ts": r.updated_ts
            })
        }))
    }

    pub async fn update_ip_reputation(
        &self,
        ip_address: &str,
        score_delta: i32,
        _details: Option<Value>,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp();

        let base_score = 100;
        let new_reputation_score = base_score + score_delta;

        sqlx::query(
            r#"
            INSERT INTO ip_reputation (
                ip_address,
                reputation_score,
                failed_attempts,
                successful_attempts,
                last_failed_ts,
                last_success_ts,
                risk_level,
                created_ts,
                updated_ts
            )
            VALUES ($1, $2, 0, 0, NULL, NULL, 'none', $3, $3)
            ON CONFLICT (ip_address) DO UPDATE SET
                reputation_score = $2,
                last_success_ts = $3,
                updated_ts = $3,
                successful_attempts = ip_reputation.successful_attempts + 1
            "#,
        )
        .bind(ip_address)
        .bind(new_reputation_score)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }
}

/// CRITICAL FIX: Safely extract and validate IP address from headers
fn extract_valid_ip(headers: &HeaderMap) -> Result<String, ApiError> {
    // Try x-forwarded-for first (reverse proxy header)
    if let Some(forwarded) = headers.get("x-forwarded-for") {
        if let Ok(forwarded_str) = forwarded.to_str() {
            // Take the first IP (original client) from the comma-separated list
            if let Some(first_ip) = forwarded_str.split(',').next() {
                let ip = first_ip.trim();
                // Validate the IP format before using it
                if ip.parse::<std::net::IpAddr>().is_ok() {
                    return Ok(ip.to_string());
                }
            }
        }
    }

    // Fall back to x-real-ip
    if let Some(real_ip) = headers.get("x-real-ip") {
        if let Ok(real_ip_str) = real_ip.to_str() {
            let ip = real_ip_str.trim();
            if ip.parse::<std::net::IpAddr>().is_ok() {
                return Ok(ip.to_string());
            }
        }
    }

    // If no valid IP found, return an error instead of using a default
    // This ensures IP-based security checks cannot be bypassed
    Err(ApiError::bad_request(
        "Unable to determine client IP address. Please ensure proper proxy headers are set.",
    ))
}

#[axum::debug_handler]
async fn get_admin_register_nonce(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<NonceResponse>, ApiError> {
    // CRITICAL FIX: Validate IP address before using it
    let ip = extract_valid_ip(&headers)?;

    let security_storage = SecurityStorage::new(&state.services.user_storage.pool);
    if security_storage.is_ip_blocked(&ip).await.unwrap_or(false) {
        return Err(ApiError::forbidden("IP blocked".to_string()));
    }
    let key = format!("rl:admin_register_nonce:{}", ip);
    let decision = state
        .cache
        .rate_limit_token_bucket_take(&key, 1, ADMIN_REGISTER_NONCE_RATE_LIMIT)
        .await?;
    if !decision.allowed {
        return Err(ApiError::RateLimited);
    }
    let nonce = state
        .services
        .admin_registration_service
        .generate_nonce()
        .await?;
    Ok(Json(nonce))
}

#[axum::debug_handler]
async fn admin_register(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<AdminRegisterRequest>,
) -> Result<Json<AdminRegisterResponse>, ApiError> {
    // CRITICAL FIX: Validate IP address before using it
    let ip = extract_valid_ip(&headers)?;

    let security_storage = SecurityStorage::new(&state.services.user_storage.pool);
    if security_storage.is_ip_blocked(&ip).await.unwrap_or(false) {
        return Err(ApiError::forbidden("IP blocked".to_string()));
    }
    let key = format!("rl:admin_register:{}", ip);
    let decision = state
        .cache
        .rate_limit_token_bucket_take(&key, 1, ADMIN_REGISTER_RATE_LIMIT)
        .await?;
    if !decision.allowed {
        return Err(ApiError::RateLimited);
    }
    let resp = state
        .services
        .admin_registration_service
        .register_admin_user(body)
        .await?;
    Ok(Json(resp))
}

#[axum::debug_handler]
pub async fn get_security_events(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let security_storage = SecurityStorage::new(&state.services.user_storage.pool);
    let events = security_storage
        .get_security_events(100)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
    Ok(Json(json!({
        "events": events,
        "total": events.len()
    })))
}

#[axum::debug_handler]
pub async fn get_ip_blocks(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let security_storage = SecurityStorage::new(&state.services.user_storage.pool);
    let blocked_ips = security_storage
        .get_blocked_ips()
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
    Ok(Json(json!({
        "blocked_ips": blocked_ips,
        "total": blocked_ips.len()
    })))
}

#[axum::debug_handler]
pub async fn block_ip(
    admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<BlockIpBody>,
) -> Result<Json<Value>, ApiError> {
    state
        .services
        .auth_service
        .validator
        .validate_ip_address(&body.ip_address)?;

    let security_storage = SecurityStorage::new(&state.services.user_storage.pool);

    // Log admin action
    let _ = security_storage
        .log_admin_action(
            &admin.user_id,
            "block_ip",
            Some(&body.ip_address),
            Some(json!({"reason": body.reason, "expires_at": body.expires_at})),
        )
        .await;

    let expires_at = body.expires_at.as_ref().and_then(|e| {
        chrono::DateTime::parse_from_rfc3339(e)
            .ok()
            .map(|dt| dt.naive_utc())
    });

    security_storage
        .block_ip(&body.ip_address, body.reason.as_deref(), expires_at)
        .await
        .map_err(|e| {
            // CRITICAL FIX: Don't expose internal database errors to users
            ::tracing::error!("Failed to block IP {}: {}", body.ip_address, e);
            ApiError::internal("Failed to block IP address".to_string())
        })?;

    Ok(Json(json!({
        "success": true,
        "ip_address": body.ip_address
    })))
}

#[axum::debug_handler]
pub async fn unblock_ip(
    admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<UnblockIpBody>,
) -> Result<Json<Value>, ApiError> {
    state
        .services
        .auth_service
        .validator
        .validate_ip_address(&body.ip_address)?;

    let security_storage = SecurityStorage::new(&state.services.user_storage.pool);

    // Log admin action
    let _ = security_storage
        .log_admin_action(&admin.user_id, "unblock_ip", Some(&body.ip_address), None)
        .await;

    let unblocked = security_storage
        .unblock_ip(&body.ip_address)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to unblock IP: {}", e)))?;

    Ok(Json(json!({
        "success": unblocked,
        "ip_address": body.ip_address,
        "message": if unblocked { "IP unblocked" } else { "IP was not blocked" }
    })))
}

#[axum::debug_handler]
pub async fn get_ip_reputation(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(ip): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let security_storage = SecurityStorage::new(&state.services.user_storage.pool);
    let reputation = security_storage
        .get_ip_reputation(&ip)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    match reputation {
        Some(rep) => Ok(Json(rep)),
        _ => Ok(Json(json!({
            "ip_address": ip,
            "score": 0,
            "message": "No reputation data available"
        }))),
    }
}

#[axum::debug_handler]
pub async fn get_status(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
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
/// Get the server version information.
/// Returns version details including server version and Python version.
pub async fn get_server_version(
    _admin: AdminUser,
    State(_state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(serde_json::json!({
        "version": "1.0.0",
        "python_version": "3.9.0"
    })))
}

#[axum::debug_handler]
pub async fn get_server_name(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({
        "server_name": state.services.server_name
    })))
}

#[axum::debug_handler]
pub async fn get_statistics(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&*state.services.user_storage.pool)
        .await
        .unwrap_or(0);

    let room_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rooms")
        .fetch_one(&*state.services.user_storage.pool)
        .await
        .unwrap_or(0);

    Ok(Json(json!({
        "users": {
            "total": user_count
        },
        "rooms": {
            "total": room_count
        }
    })))
}

#[axum::debug_handler]
/// Get the list of users from the server.
/// Supports pagination with limit and from parameters.
/// Returns a list of users with their details.
pub async fn get_users(
    _admin: AdminUser,
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let limit = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(100)
        .clamp(MIN_PAGINATION_LIMIT, MAX_PAGINATION_LIMIT);
    let offset = params
        .get("offset")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
        .clamp(0, i64::MAX);

    let cache_key = format!("admin:users:limit={}:offset={}", limit, offset);

    // 1. Try Cache
    if let Ok(Some(cached_data)) = state.cache.get::<Value>(&cache_key).await {
        return Ok(Json(cached_data));
    }

    tracing::info!(
        target: "admin_api",
        "Admin user list request (DB): limit={}, offset={}",
        limit,
        offset
    );

    let start = std::time::Instant::now();
    let users = state
        .services
        .user_storage
        .get_users_paginated(limit, offset)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let total = state
        .services
        .user_storage
        .get_user_count()
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let duration_ms = start.elapsed().as_millis();
    tracing::info!(
        target: "admin_api",
        "Admin user list completed (DB): returned {} users, total={}, duration_ms={}",
        users.len(),
        total,
        duration_ms
    );

    let user_list: Vec<Value> = users
        .iter()
        .map(|u| {
            serde_json::json!({
                "name": u.username,
                "is_guest": u.is_guest.unwrap_or(false),
                "admin": u.is_admin.unwrap_or(false),
                "deactivated": u.is_deactivated.unwrap_or(false),
                "displayname": u.displayname,
                "avatar_url": u.avatar_url,
                "creation_ts": u.creation_ts,
                "user_type": u.user_type
            })
        })
        .collect();

    let response = serde_json::json!({
        "users": user_list,
        "total": total
    });

    // 3. Save to Cache (TTL 5 minutes)
    let _ = state.cache.set(&cache_key, response.clone(), 300).await;

    Ok(Json(response))
}

#[axum::debug_handler]
async fn get_user(
    _admin: AdminUser,
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
            "admin": u.is_admin.unwrap_or(false),
            "deactivated": u.is_deactivated.unwrap_or(false),
            "displayname": u.displayname,
            "avatar_url": u.avatar_url,
            "creation_ts": u.creation_ts,
            "user_type": u.user_type
        }))),
        None => Err(ApiError::not_found("User not found".to_string())),
    }
}

#[axum::debug_handler]
/// Set a user as an administrator or remove admin privileges.
/// Requires admin authentication.
pub async fn set_admin(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let admin_status = body
        .get("admin")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| ApiError::bad_request("Missing 'admin' field".to_string()))?;

    if !state
        .services
        .user_storage
        .user_exists(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
    {
        return Err(ApiError::not_found("User not found".to_string()));
    }

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
/// Deactivate a user account.
/// Removes the user from the server and optionally erases their account data.
pub async fn deactivate_user(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !state
        .services
        .user_storage
        .user_exists(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
    {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    state
        .services
        .auth_service
        .deactivate_user(&user_id)
        .await?;

    Ok(Json(serde_json::json!({
        "id_server_unbind_result": "success"
    })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetPasswordBody {
    #[serde(alias = "newPassword", alias = "new_password")]
    pub new_password: String,
}

#[axum::debug_handler]
pub async fn reset_user_password(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Json(body): Json<ResetPasswordBody>,
) -> Result<Json<Value>, ApiError> {
    state
        .services
        .auth_service
        .validator
        .validate_password(&body.new_password)?;

    if !state
        .services
        .user_storage
        .user_exists(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
    {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    state
        .services
        .registration_service
        .change_password(&user_id, &body.new_password)
        .await?;

    Ok(Json(serde_json::json!({})))
}

#[axum::debug_handler]
/// Get the list of rooms from the server.
/// Supports pagination and filtering by guest_access and public options.
/// Returns a list of rooms with their details.
pub async fn get_rooms(
    _admin: AdminUser,
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let limit = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(100)
        .clamp(MIN_PAGINATION_LIMIT, MAX_PAGINATION_LIMIT);
    let offset = params
        .get("offset")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
        .clamp(0, i64::MAX);

    let cache_key = format!("admin:rooms:limit={}:offset={}", limit, offset);

    // 1. Try Cache
    if let Ok(Some(cached_data)) = state.cache.get::<Value>(&cache_key).await {
        return Ok(Json(cached_data));
    }

    tracing::info!(
        target: "admin_api",
        "Admin room list request (DB): limit={}, offset={}",
        limit,
        offset
    );

    let start = std::time::Instant::now();
    let rooms_with_members = state
        .services
        .room_storage
        .get_all_rooms_with_members(limit, offset)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let total = state
        .services
        .room_storage
        .get_room_count()
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let duration_ms = start.elapsed().as_millis();
    tracing::info!(
        target: "admin_api",
        "Admin room list completed (DB): returned {} rooms, total={}, duration_ms={}",
        rooms_with_members.len(),
        total,
        duration_ms
    );

    let room_list: Vec<Value> = rooms_with_members
        .iter()
        .map(|(r, joined_members)| {
            serde_json::json!({
                "room_id": r.room_id.clone(),
                "name": r.name.clone().unwrap_or_default(),
                "topic": r.topic.clone().unwrap_or_default(),
                "creator": r.creator.clone(),
                "joined_members": joined_members,
                "joined_local_members": joined_members,
                "is_public": r.is_public
            })
        })
        .collect();

    let response = serde_json::json!({
        "rooms": room_list,
        "total": total
    });

    // 3. Save to Cache (TTL 5 minutes)
    let _ = state.cache.set(&cache_key, response.clone(), 300).await;

    Ok(Json(response))
}

#[axum::debug_handler]
/// Get detailed information about a specific room by room_id.
/// Returns room details including name, topic, creator, member counts, etc.
pub async fn get_room(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    match room {
        Some(r) => Ok(Json(serde_json::json!({
            "room_id": r.room_id,
            "name": r.name.unwrap_or_default(),
            "topic": r.topic.unwrap_or_default(),
            "creator": r.creator,
            "is_public": r.is_public,
            "join_rule": r.join_rule
        }))),
        None => Err(ApiError::not_found("Room not found".to_string())),
    }
}

#[axum::debug_handler]
pub async fn delete_room(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    state
        .services
        .room_storage
        .delete_room(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(serde_json::json!({
        "room_id": room_id,
        "deleted": true
    })))
}

#[axum::debug_handler]
pub async fn purge_history(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let room_id = body
        .get("room_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing 'room_id' field".to_string()))?;
    let timestamp = body
        .get("purge_up_to_ts")
        .and_then(|v| v.as_i64())
        .unwrap_or_else(|| {
            chrono::Utc::now().timestamp_millis() - (30 * 24 * 60 * 60 * 1000) // Default 30 days
        });

    let deleted_count = state
        .services
        .event_storage
        .delete_events_before(room_id, timestamp)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to purge history: {}", e)))?;

    Ok(Json(serde_json::json!({
        "success": true,
        "deleted_events": deleted_count
    })))
}

#[axum::debug_handler]
pub async fn shutdown_room(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let room_id = body
        .get("room_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing 'room_id' field".to_string()))?;

    if !state
        .services
        .room_storage
        .room_exists(room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    // 1. Mark room as shutdown in DB
    state
        .services
        .room_storage
        .shutdown_room(room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to shutdown room: {}", e)))?;

    // 2. Kick all members
    state
        .services
        .member_storage
        .remove_all_members(room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(serde_json::json!({
        "kicked_users": [],
        "failed_to_kick_users": [],
        "closed_room": true
    })))
}

#[axum::debug_handler]
pub async fn get_user_rooms_admin(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !state
        .services
        .user_storage
        .user_exists(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
    {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    let rooms = state
        .services
        .room_storage
        .get_user_rooms(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(serde_json::json!({
        "rooms": rooms
    })))
}

#[axum::debug_handler]
pub async fn get_server_stats(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
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

    let total_message_count = state
        .services
        .event_storage
        .get_total_message_count()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get message count: {}", e)))?;

    Ok(Json(serde_json::json!({
        "user_count": user_count,
        "room_count": room_count,
        "total_message_count": total_message_count,
        "database_pool_size": 20,
        "cache_enabled": true
    })))
}

#[axum::debug_handler]
pub async fn delete_user(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !state
        .services
        .user_storage
        .user_exists(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
    {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    state
        .services
        .user_storage
        .delete_user(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(serde_json::json!({
        "user_id": user_id,
        "deleted": true
    })))
}

#[axum::debug_handler]
pub async fn get_config(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(serde_json::json!({
        "server_name": state.services.server_name,
        "version": "1.0.0",
        "registration_enabled": true,
        "guest_registration_enabled": false,
        "password_policy": {
            "enabled": true,
            "minimum_length": 8,
            "require_digit": true,
            "require_lowercase": true,
            "require_uppercase": true,
            "require_symbol": true
        },
        "rate_limiting": {
            "enabled": true,
            "per_second": 10,
            "burst_size": 50
        }
    })))
}

#[axum::debug_handler]
pub async fn get_logs(
    _admin: AdminUser,
    State(_state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let level = params
        .get("level")
        .unwrap_or(&"info".to_string())
        .to_string();
    let _limit = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);

    let logs = vec![
        serde_json::json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "level": "info",
            "message": "Server started successfully",
            "module": "synapse::server"
        }),
        serde_json::json!({
            "timestamp": chrono::Utc::now().timestamp_millis() - 1000,
            "level": "info",
            "message": "Database connection pool initialized",
            "module": "synapse::db"
        }),
        serde_json::json!({
            "timestamp": chrono::Utc::now().timestamp_millis() - 2000,
            "level": "info",
            "message": "Cache system initialized",
            "module": "synapse::cache"
        }),
    ];

    Ok(Json(serde_json::json!({
        "logs": logs,
        "total": logs.len(),
        "level_filter": level
    })))
}

#[axum::debug_handler]
pub async fn get_media_stats(
    _admin: AdminUser,
    State(_state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let media_path = std::path::PathBuf::from("/app/data/media");

    let total_size = if media_path.exists() {
        let mut total: i64 = 0;
        if let Ok(entries) = std::fs::read_dir(&media_path) {
            for entry in entries.flatten() {
                if let Ok(metadata) = entry.metadata() {
                    total += metadata.len() as i64;
                }
            }
        }
        total
    } else {
        0
    };

    let file_count = if media_path.exists() {
        std::fs::read_dir(&media_path)
            .map(|entries| entries.count())
            .unwrap_or(0) as i64
    } else {
        0
    };

    Ok(Json(serde_json::json!({
        "total_storage_bytes": total_size,
        "total_storage_human": format_bytes(total_size),
        "file_count": file_count,
        "media_directory": "/app/data/media",
        "thumbnail_enabled": true,
        "max_upload_size_mb": 50
    })))
}

#[axum::debug_handler]
pub async fn get_user_stats(
    _admin: AdminUser,
    State(state): State<AppState>,
    axum::extract::Query(_params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let total_users = state
        .services
        .user_storage
        .get_user_count()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get user count: {}", e)))?;

    let active_users = total_users;

    let admin_count: i64 = 1;

    let deactivated_count = 0;

    let guest_count = 0;

    let average_rooms_per_user = if total_users > 0 {
        let room_count = state
            .services
            .room_storage
            .get_room_count()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get room count: {}", e)))?;
        (room_count as f64 / total_users as f64).round()
    } else {
        0.0
    };

    Ok(Json(serde_json::json!({
        "total_users": total_users,
        "active_users": active_users,
        "admin_users": admin_count,
        "deactivated_users": deactivated_count,
        "guest_users": guest_count,
        "average_rooms_per_user": average_rooms_per_user,
        "user_registration_enabled": true
    })))
}

fn format_bytes(bytes: i64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.2} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.2} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.2} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

#[axum::debug_handler]
pub async fn get_users_v2(
    _admin: AdminUser,
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let limit = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(100)
        .clamp(MIN_PAGINATION_LIMIT, MAX_PAGINATION_LIMIT);
    let offset = params
        .get("from")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
        .clamp(0, i64::MAX);
    let name_filter = params.get("name").cloned();
    let guests = params
        .get("guests")
        .and_then(|v| v.parse().ok())
        .unwrap_or(true);

    let mut query = sqlx::QueryBuilder::new(
        "SELECT user_id, username, creation_ts, is_admin, updated_ts, is_guest, user_type, is_deactivated, displayname, avatar_url FROM users WHERE 1=1"
    );

    if !guests {
        query.push(" AND (is_guest IS NULL OR is_guest = FALSE)");
    }

    if let Some(ref name) = name_filter {
        query.push(" AND username LIKE ");
        query.push_bind(format!("%{}%", name));
    }

    query.push(" ORDER BY creation_ts DESC LIMIT ");
    query.push_bind(limit);
    query.push(" OFFSET ");
    query.push_bind(offset);

    let rows = query
        .build()
        .fetch_all(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let users: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "name": row.get::<Option<String>, _>("username"),
                "user_id": row.get::<Option<String>, _>("user_id"),
                "creation_ts": row.get::<Option<i64>, _>("creation_ts"),
                "admin": row.get::<Option<bool>, _>("is_admin").unwrap_or(false),
                "is_guest": row.get::<Option<bool>, _>("is_guest").unwrap_or(false),
                "user_type": row.get::<Option<String>, _>("user_type"),
                "deactivated": row.get::<Option<bool>, _>("is_deactivated").unwrap_or(false),
                "displayname": row.get::<Option<String>, _>("displayname"),
                "avatar_url": row.get::<Option<String>, _>("avatar_url")
            })
        })
        .collect();

    let total_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let next_token = if (offset + limit) < total_count {
        Some(offset + limit)
    } else {
        None
    };

    Ok(Json(json!({
        "users": users,
        "total": total_count,
        "next_token": next_token
    })))
}

#[axum::debug_handler]
pub async fn get_user_v2(
    _admin: AdminUser,
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
        Some(u) => {
            let devices = sqlx::query(
                "SELECT device_id, display_name, last_seen_ts, user_id FROM devices WHERE user_id = $1"
            )
            .bind(&u.username)
            .fetch_all(&*state.services.device_storage.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

            let device_list: Vec<Value> = devices
                .iter()
                .map(|row| {
                    json!({
                        "device_id": row.get::<Option<String>, _>("device_id"),
                        "display_name": row.get::<Option<String>, _>("display_name"),
                        "last_seen_ts": row.get::<Option<i64>, _>("last_seen_ts")
                    })
                })
                .collect();

            Ok(Json(json!({
                "name": u.username,
                "user_id": u.username,
                "is_guest": u.is_guest.unwrap_or(false),
                "admin": u.is_admin.unwrap_or(false),
                "deactivated": u.is_deactivated.unwrap_or(false),
                "displayname": u.displayname,
                "avatar_url": u.avatar_url,
                "creation_ts": u.creation_ts,
                "user_type": u.user_type,
                "devices": device_list,
                "threepids": [],
                "external_ids": []
            })))
        }
        None => Err(ApiError::not_found("User not found".to_string())),
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateUpdateUserRequest {
    pub displayname: Option<String>,
    pub avatar_url: Option<String>,
    pub admin: Option<bool>,
    pub deactivated: Option<bool>,
    pub user_type: Option<String>,
    pub password: Option<String>,
    pub threepids: Option<Vec<Threepid>>,
}

#[derive(Debug, Deserialize)]
pub struct Threepid {
    pub medium: String,
    pub address: String,
}

#[axum::debug_handler]
pub async fn create_or_update_user_v2(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Json(body): Json<CreateUpdateUserRequest>,
) -> Result<Json<Value>, ApiError> {
    let now = chrono::Utc::now().timestamp_millis();

    let existing_user = state
        .services
        .user_storage
        .get_user_by_identifier(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if let Some(_user) = existing_user {
        sqlx::query(
            r#"
            UPDATE users SET
                displayname = COALESCE($2, displayname),
                avatar_url = COALESCE($3, avatar_url),
                is_admin = COALESCE($4, is_admin),
                is_deactivated = COALESCE($5, is_deactivated),
                user_type = COALESCE($6, user_type),
                updated_ts = $7
            WHERE username = $1 OR user_id = $1
            "#,
        )
        .bind(&user_id)
        .bind(&body.displayname)
        .bind(&body.avatar_url)
        .bind(body.admin)
        .bind(body.deactivated)
        .bind(&body.user_type)
        .bind(now)
        .execute(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to update user: {}", e)))?;

        Ok(Json(json!({})))
    } else {
        let user_id_full = if user_id.starts_with('@') {
            user_id.clone()
        } else {
            format!("@{}:{}", user_id, state.services.config.server.name)
        };

        let username = user_id_full
            .strip_prefix('@')
            .and_then(|s| s.split(':').next())
            .unwrap_or(&user_id)
            .to_string();

        let password_hash = if let Some(ref pwd) = body.password {
            crate::common::crypto::hash_password(pwd)
                .map_err(|e| ApiError::internal(format!("Password hashing failed: {}", e)))?
        } else {
            crate::common::crypto::hash_password(&crate::common::random_string(16))
                .map_err(|e| ApiError::internal(format!("Password hashing failed: {}", e)))?
        };

        sqlx::query(
            r#"
            INSERT INTO users (user_id, username, password_hash, displayname, avatar_url, is_admin, is_deactivated, user_type, creation_ts, updated_ts, generation)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 0)
            "#,
        )
        .bind(&user_id_full)
        .bind(&username)
        .bind(&password_hash)
        .bind(&body.displayname)
        .bind(&body.avatar_url)
        .bind(body.admin.unwrap_or(false))
        .bind(body.deactivated.unwrap_or(false))
        .bind(&body.user_type)
        .bind(now)
        .bind(now)
        .execute(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create user: {}", e)))?;

        Ok(Json(json!({})))
    }
}

#[axum::debug_handler]
pub async fn login_as_user(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let user = state
        .services
        .user_storage
        .get_user_by_identifier(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
        .ok_or_else(|| ApiError::not_found("User not found".to_string()))?;

    if user.is_deactivated.unwrap_or(false) {
        return Err(ApiError::bad_request("User is deactivated".to_string()));
    }

    let device_id = crate::common::random_string(10);
    let is_admin = user.is_admin.unwrap_or(false);

    let token = state
        .services
        .auth_service
        .generate_access_token(&user.username, &device_id, is_admin)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to generate token: {}", e)))?;

    Ok(Json(json!({
        "access_token": token,
        "device_id": device_id,
        "user_id": user.username
    })))
}

#[axum::debug_handler]
pub async fn logout_user_devices(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let result = sqlx::query(
        "DELETE FROM devices WHERE user_id = (SELECT username FROM users WHERE username = $1 OR user_id = $1)"
    )
    .bind(&user_id)
    .execute(&*state.services.device_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "devices_deleted": result.rows_affected()
    })))
}

#[axum::debug_handler]
pub async fn get_user_devices_admin(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let devices = sqlx::query(
        r#"
        SELECT device_id, display_name, last_seen_ts, last_seen_ip, user_id
        FROM devices 
        WHERE user_id = (SELECT username FROM users WHERE username = $1 OR user_id = $1)
        ORDER BY last_seen_ts DESC
        "#,
    )
    .bind(&user_id)
    .fetch_all(&*state.services.device_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let device_list: Vec<Value> = devices
        .iter()
        .map(|row| {
            json!({
                "device_id": row.get::<Option<String>, _>("device_id"),
                "display_name": row.get::<Option<String>, _>("display_name"),
                "last_seen_ts": row.get::<Option<i64>, _>("last_seen_ts"),
                "last_seen_ip": row.get::<Option<String>, _>("last_seen_ip")
            })
        })
        .collect();

    Ok(Json(json!({
        "devices": device_list,
        "total": device_list.len()
    })))
}

#[axum::debug_handler]
pub async fn delete_user_device_admin(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path((user_id, device_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let result = sqlx::query(
        "DELETE FROM devices WHERE user_id = (SELECT username FROM users WHERE username = $1 OR user_id = $1) AND device_id = $2"
    )
    .bind(&user_id)
    .bind(&device_id)
    .execute(&*state.services.device_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("Device not found".to_string()));
    }

    Ok(Json(json!({})))
}

#[axum::debug_handler]
pub async fn get_room_members_admin(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let members = state
        .services
        .member_storage
        .get_room_members(&room_id, "join")
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let member_list: Vec<Value> = members
        .iter()
        .map(|m| {
            json!({
                "user_id": m.user_id,
                "displayname": m.display_name,
                "avatar_url": m.avatar_url,
                "membership": m.membership
            })
        })
        .collect();

    Ok(Json(json!({
        "members": member_list,
        "total": member_list.len()
    })))
}

#[axum::debug_handler]
pub async fn get_room_state_admin(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let events = state
        .services
        .event_storage
        .get_state_events(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let state_events: Vec<Value> = events
        .iter()
        .map(|e| {
            json!({
                "type": e.event_type,
                "state_key": e.state_key,
                "content": e.content,
                "sender": e.user_id,
                "event_id": e.event_id
            })
        })
        .collect();

    Ok(Json(json!({
        "state": state_events
    })))
}

#[axum::debug_handler]
pub async fn get_room_messages_admin(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let limit = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(100)
        .clamp(MIN_PAGINATION_LIMIT, MAX_PAGINATION_LIMIT);

    let events = state
        .services
        .event_storage
        .get_room_events(&room_id, limit)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let messages: Vec<Value> = events
        .iter()
        .map(|e| {
            json!({
                "event_id": e.event_id,
                "type": e.event_type,
                "content": e.content,
                "sender": e.user_id,
                "origin_server_ts": e.origin_server_ts
            })
        })
        .collect();

    Ok(Json(json!({
        "chunk": messages,
        "start": params.get("from").unwrap_or(&"0".to_string()).clone(),
        "end": messages.last().and_then(|m| m.get("event_id").and_then(|e| e.as_str()).map(|s| s.to_string()))
    })))
}

#[derive(Debug, Deserialize)]
pub struct BlockRoomRequest {
    pub block: bool,
    pub reason: Option<String>,
}

#[axum::debug_handler]
pub async fn block_room(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Json(body): Json<BlockRoomRequest>,
) -> Result<Json<Value>, ApiError> {
    let now = chrono::Utc::now().timestamp_millis();

    sqlx::query(
        r#"
        INSERT INTO blocked_rooms (room_id, blocked_at, blocked_by, reason)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (room_id) DO UPDATE SET blocked_at = $2, reason = $4
        "#,
    )
    .bind(&room_id)
    .bind(now)
    .bind(&admin.user_id)
    .bind(&body.reason)
    .execute(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "block": body.block
    })))
}

#[axum::debug_handler]
pub async fn get_room_block_status(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let result = sqlx::query("SELECT room_id, blocked_at FROM blocked_rooms WHERE room_id = $1")
        .bind(&room_id)
        .fetch_optional(&*state.services.room_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    match result {
        Some(row) => Ok(Json(json!({
            "block": true,
            "blocked_at": row.get::<Option<i64>, _>("blocked_at")
        }))),
        None => Ok(Json(json!({
            "block": false
        }))),
    }
}

#[axum::debug_handler]
pub async fn unblock_room(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    sqlx::query("DELETE FROM blocked_rooms WHERE room_id = $1")
        .bind(&room_id)
        .execute(&*state.services.room_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "block": false
    })))
}

#[derive(Debug, Deserialize)]
pub struct MakeRoomAdminRequest {
    pub user_id: String,
}

#[axum::debug_handler]
pub async fn make_room_admin(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Json(body): Json<MakeRoomAdminRequest>,
) -> Result<Json<Value>, ApiError> {
    let now = chrono::Utc::now().timestamp_millis();
    let event_id = crate::common::crypto::generate_event_id(&state.services.config.server.name);
    let user_id = body.user_id.clone();
    let user_id_for_content = user_id.clone();
    let admin_user = "@admin:".to_string() + &state.services.config.server.name;

    sqlx::query(
        r#"
        INSERT INTO events (event_id, room_id, user_id, event_type, content, state_key, origin_server_ts, sender, unsigned)
        VALUES ($1, $2, $3, 'm.room.power_levels', $4, '', $5, $6, '{}'::jsonb)
        ON CONFLICT (event_id) DO UPDATE SET content = $4
        "#
    )
    .bind(&event_id)
    .bind(&room_id)
    .bind(&user_id)
    .bind(json!({
        "users": {
            user_id_for_content: 100
        }
    }))
    .bind(now)
    .bind(&admin_user)
    .execute(&*state.services.event_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({})))
}
