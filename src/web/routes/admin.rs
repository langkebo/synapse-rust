use super::{AdminUser, AppState};
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

const MAX_LIMIT: i64 = 1000;

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
        let now = chrono::Utc::now().timestamp();
        let details_str = details.map(|d| d.to_string());

        let row = sqlx::query(
            r#"
            INSERT INTO security_events (event_type, user_id, details, description, created_at, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id
            "#,
        )
        .bind(format!("admin_action:{}", action))
        .bind(Some(admin_id))
        .bind(details_str)
        .bind(target)
        .bind(now)
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
            ip_address: Option<IpNetwork>,
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
                    "ip_address": r.ip_address.map(|ip| ip.ip().to_string()),
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
        expires_at: Option<chrono::NaiveDateTime>,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp();

        let ip_network = Self::parse_ip_address(ip_address)
            .ok_or_else(|| sqlx::Error::Protocol("Invalid IP address format".to_string()))?;

        let expires_at_ts = expires_at.map(|t| t.and_utc().timestamp());
        sqlx::query(r#"DELETE FROM ip_blocks WHERE ip_range = $1"#)
            .bind(ip_network)
            .execute(&*self.pool)
            .await?;

        sqlx::query(
            r#"
            INSERT INTO ip_blocks (ip_range, reason, blocked_ts, expires_ts)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(ip_network)
        .bind(reason)
        .bind(now)
        .bind(expires_at_ts)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn unblock_ip(&self, ip_address: &str) -> Result<bool, sqlx::Error> {
        let ip_network = Self::parse_ip_address(ip_address)
            .ok_or_else(|| sqlx::Error::Protocol("Invalid IP address format".to_string()))?;

        let result = sqlx::query(r#"DELETE FROM ip_blocks WHERE ip_range = $1"#)
            .bind(ip_network)
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
            ip_address: IpAddr,
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
        .bind(
            ip_address
                .parse::<IpAddr>()
                .map_err(|e| sqlx::Error::Protocol(e.to_string()))?,
        )
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row.map(|r| {
            json!({
                "ip_address": r.ip_address.to_string(),
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
        let details_str = details.map(|d| d.to_string());

        let ip: IpAddr = ip_address
            .parse::<IpAddr>()
            .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
        let ip: IpNetwork = ip.into();
        let base_score = 50;
        let initial_score = base_score + score_delta;

        sqlx::query(
            r#"
            INSERT INTO ip_reputation (
                ip_address,
                score,
                reputation_score,
                last_seen_at,
                updated_at,
                details,
                last_updated_ts,
                created_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (ip_address) DO UPDATE SET
                score = ip_reputation.score + $9,
                reputation_score = ip_reputation.reputation_score + $9,
                last_seen_at = EXCLUDED.last_seen_at,
                updated_at = EXCLUDED.updated_at,
                last_updated_ts = EXCLUDED.last_updated_ts,
                details = COALESCE(EXCLUDED.details, ip_reputation.details)
            "#,
        )
        .bind(ip)
        .bind(initial_score)
        .bind(initial_score)
        .bind(now)
        .bind(now)
        .bind(details_str)
        .bind(now)
        .bind(now)
        .bind(score_delta)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }
}

#[axum::debug_handler]
async fn get_admin_register_nonce(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<NonceResponse>, ApiError> {
    let ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .unwrap_or("0.0.0.0")
        .trim()
        .to_string();
    let security_storage = SecurityStorage::new(&state.services.user_storage.pool);
    if security_storage.is_ip_blocked(&ip).await.unwrap_or(false) {
        return Err(ApiError::forbidden("IP blocked".to_string()));
    }
    let key = format!("rl:admin_register_nonce:{}", ip);
    let decision = state.cache.rate_limit_token_bucket_take(&key, 1, 3).await?;
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
    let ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .unwrap_or("0.0.0.0")
        .trim()
        .to_string();
    let security_storage = SecurityStorage::new(&state.services.user_storage.pool);
    if security_storage.is_ip_blocked(&ip).await.unwrap_or(false) {
        return Err(ApiError::forbidden("IP blocked".to_string()));
    }
    let key = format!("rl:admin_register:{}", ip);
    let decision = state.cache.rate_limit_token_bucket_take(&key, 1, 2).await?;
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
        .map_err(|e| ApiError::internal(format!("Failed to block IP: {}", e)))?;

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
pub async fn get_users(
    _admin: AdminUser,
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let limit = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(100)
        .clamp(1, MAX_LIMIT);
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
                "deactivated": u.deactivated.unwrap_or(false),
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
pub async fn deactivate_user(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
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

    state
        .services
        .registration_service
        .change_password(&user_id, &body.new_password)
        .await?;

    Ok(Json(serde_json::json!({})))
}

#[axum::debug_handler]
pub async fn get_rooms(
    _admin: AdminUser,
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let limit = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(100)
        .clamp(1, MAX_LIMIT);
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
