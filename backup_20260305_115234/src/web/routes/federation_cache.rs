use crate::cache::SignatureCacheStats;
use crate::common::ApiError;
use crate::web::routes::{AdminUser, AppState};
use axum::{
    extract::{Path, State},
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Serialize)]
pub struct CacheStatsResponse {
    pub signature_cache: SignatureCacheStats,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct ClearCacheRequest {
    pub origin: Option<String>,
    pub key_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ClearCacheResponse {
    pub cleared: bool,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct KeyRotationNotification {
    pub origin: String,
    pub old_key_id: String,
    pub new_key_id: String,
}

#[derive(Debug, Serialize)]
pub struct CacheConfigResponse {
    pub signature_ttl: u64,
    pub key_ttl: u64,
    pub key_rotation_grace_period_ms: u64,
    pub max_capacity: u64,
}

pub fn create_federation_cache_router() -> Router<AppState> {
    Router::new()
        .route(
            "/_synapse/admin/v1/federation/cache/stats",
            get(get_cache_stats),
        )
        .route(
            "/_synapse/admin/v1/federation/cache/clear",
            post(clear_cache),
        )
        .route(
            "/_synapse/admin/v1/federation/cache/clear/origin/{origin}",
            delete(clear_cache_for_origin),
        )
        .route(
            "/_synapse/admin/v1/federation/cache/clear/origin/{origin}/key/{key_id}",
            delete(clear_cache_for_key),
        )
        .route(
            "/_synapse/admin/v1/federation/cache/key-rotation",
            post(notify_key_rotation),
        )
        .route(
            "/_synapse/admin/v1/federation/cache/config",
            get(get_cache_config),
        )
}

async fn get_cache_stats(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<Json<CacheStatsResponse>, ApiError> {
    let stats = state.federation_signature_cache.get_stats();
    Ok(Json(CacheStatsResponse {
        signature_cache: stats,
        message: "Cache statistics retrieved successfully".to_string(),
    }))
}

async fn clear_cache(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<ClearCacheRequest>,
) -> Result<Json<ClearCacheResponse>, ApiError> {
    let cache = &state.federation_signature_cache;
    match (body.origin, body.key_id) {
        (Some(origin), Some(key_id)) => {
            cache.invalidate_signatures_for_key(&origin, &key_id);
            Ok(Json(ClearCacheResponse {
                cleared: true,
                message: format!("Cache cleared for origin {} and key {}", origin, key_id),
            }))
        }
        (Some(origin), None) => {
            cache.invalidate_signatures_for_origin(&origin);
            Ok(Json(ClearCacheResponse {
                cleared: true,
                message: format!("Cache cleared for origin {}", origin),
            }))
        }
        (None, None) => {
            cache.clear_all();
            Ok(Json(ClearCacheResponse {
                cleared: true,
                message: "All cache cleared".to_string(),
            }))
        }
        (None, Some(_)) => Err(ApiError::bad_request(
            "Origin is required when key_id is specified".to_string(),
        )),
    }
}

async fn clear_cache_for_origin(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(origin): Path<String>,
) -> Result<Json<ClearCacheResponse>, ApiError> {
    state
        .federation_signature_cache
        .invalidate_signatures_for_origin(&origin);
    Ok(Json(ClearCacheResponse {
        cleared: true,
        message: format!("Cache cleared for origin {}", origin),
    }))
}

async fn clear_cache_for_key(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path((origin, key_id)): Path<(String, String)>,
) -> Result<Json<ClearCacheResponse>, ApiError> {
    state
        .federation_signature_cache
        .invalidate_signatures_for_key(&origin, &key_id);
    Ok(Json(ClearCacheResponse {
        cleared: true,
        message: format!("Cache cleared for origin {} and key {}", origin, key_id),
    }))
}

async fn notify_key_rotation(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<KeyRotationNotification>,
) -> Result<Json<serde_json::Value>, ApiError> {
    use crate::cache::KeyRotationEvent;
    use std::time::Instant;

    let event = KeyRotationEvent {
        origin: body.origin.clone(),
        old_key_id: body.old_key_id.clone(),
        new_key_id: body.new_key_id.clone(),
        timestamp: Instant::now(),
    };

    state.federation_signature_cache.notify_key_rotation(event);

    Ok(Json(json!({
        "notified": true,
        "origin": body.origin,
        "old_key_id": body.old_key_id,
        "new_key_id": body.new_key_id,
        "message": "Key rotation event processed and cache invalidated"
    })))
}

async fn get_cache_config(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<Json<CacheConfigResponse>, ApiError> {
    let config = state.federation_signature_cache.get_config();
    Ok(Json(CacheConfigResponse {
        signature_ttl: config.signature_ttl,
        key_ttl: config.key_ttl,
        key_rotation_grace_period_ms: config.key_rotation_grace_period_ms,
        max_capacity: config.max_capacity,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::SignatureCacheConfig;

    #[test]
    fn test_clear_cache_request_deserialization() {
        let json = r#"{"origin": "example.com", "key_id": "ed25519:1"}"#;
        let req: ClearCacheRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.origin, Some("example.com".to_string()));
        assert_eq!(req.key_id, Some("ed25519:1".to_string()));
    }

    #[test]
    fn test_clear_cache_request_empty() {
        let json = r#"{}"#;
        let req: ClearCacheRequest = serde_json::from_str(json).unwrap();
        assert!(req.origin.is_none());
        assert!(req.key_id.is_none());
    }

    #[test]
    fn test_key_rotation_notification_deserialization() {
        let json = r#"{
            "origin": "example.com",
            "old_key_id": "ed25519:1",
            "new_key_id": "ed25519:2"
        }"#;
        let req: KeyRotationNotification = serde_json::from_str(json).unwrap();
        assert_eq!(req.origin, "example.com");
        assert_eq!(req.old_key_id, "ed25519:1");
        assert_eq!(req.new_key_id, "ed25519:2");
    }

    #[test]
    fn test_cache_stats_response_serialization() {
        let config = SignatureCacheConfig::default();
        let stats = crate::cache::SignatureCacheStats {
            entry_count: 100,
            invalidated_key_count: 5,
            listener_count: 2,
            config,
        };
        let response = CacheStatsResponse {
            signature_cache: stats,
            message: "test".to_string(),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"entry_count\":100"));
        assert!(json.contains("\"invalidated_key_count\":5"));
    }

    #[test]
    fn test_cache_config_response_serialization() {
        let response = CacheConfigResponse {
            signature_ttl: 3600,
            key_ttl: 3600,
            key_rotation_grace_period_ms: 600000,
            max_capacity: 10000,
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"signature_ttl\":3600"));
        assert!(json.contains("\"key_ttl\":3600"));
    }

    #[test]
    fn test_clear_cache_response_serialization() {
        let response = ClearCacheResponse {
            cleared: true,
            message: "Cache cleared".to_string(),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"cleared\":true"));
        assert!(json.contains("Cache cleared"));
    }
}
