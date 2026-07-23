use super::devices::{decode_key_request_cursor, encode_key_request_cursor};
use crate::e2ee::secure_backup::RestoreSecureBackupRequest;
use crate::web::routes::context::E2eeRoomContext;
use crate::web::routes::response_helpers::empty_json;
use crate::web::routes::{AuthenticatedUser, MatrixJson};
use crate::ApiError;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::Value;

#[axum::debug_handler]
pub(crate) async fn get_secure_backup_list(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let backups = ctx.secure_backup_service.list_backups(&auth_user.user_id).await?;

    let mut response = serde_json::Map::with_capacity(backups.len());
    for backup in backups {
        response.insert(
            backup.backup_id.clone(),
            serde_json::json!({
                "backup_id": backup.backup_id,
                "version": backup.version,
                "algorithm": backup.algorithm,
                "auth_data": backup.auth_data,
                "key_count": backup.key_count
            }),
        );
    }

    Ok(Json(Value::Object(response)))
}

#[axum::debug_handler]
pub(crate) async fn create_secure_backup(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    // Support two modes:
    // 1. Passphrase mode: { "passphrase": "..." } -> server derives key
    // 2. Standard mode: { "algorithm": "...", "auth_data": {...} } -> client provides auth data
    let passphrase = body.get("passphrase").and_then(|v| v.as_str());
    let algorithm = body.get("algorithm").and_then(|v| v.as_str());
    let auth_data_val = body.get("auth_data");

    if let Some(passphrase) = passphrase {
        // Passphrase mode: server derives key from passphrase
        let response = ctx.secure_backup_service.create_backup(&auth_user.user_id, passphrase).await?;

        Ok(Json(serde_json::json!({
            "backup_id": response.backup_id,
            "version": response.version,
            "algorithm": response.algorithm,
            "auth_data": response.auth_data,
            "key_count": response.key_count
        })))
    } else if let (Some(algorithm), Some(auth_data_val)) = (algorithm, auth_data_val) {
        // Standard mode: client provides algorithm and auth_data
        let response =
            ctx.secure_backup_service.create_backup_with_data(&auth_user.user_id, algorithm, auth_data_val).await?;

        Ok(Json(serde_json::json!({
            "backup_id": response.backup_id,
            "version": response.version,
            "algorithm": response.algorithm,
            "auth_data": response.auth_data,
            "key_count": response.key_count
        })))
    } else {
        Err(ApiError::bad_request("Either 'passphrase' or 'algorithm'+'auth_data' required".to_string()))
    }
}

#[axum::debug_handler]
pub(crate) async fn get_secure_backup(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Path(backup_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let response = ctx.secure_backup_service.get_backup_info(&auth_user.user_id, &backup_id).await?;

    match response {
        Some(r) => Ok(Json(serde_json::json!({
            "backup_id": r.backup_id,
            "version": r.version,
            "algorithm": r.algorithm,
            "auth_data": r.auth_data,
            "key_count": r.key_count
        }))),
        None => Err(ApiError::not_found("Backup not found".to_string())),
    }
}

#[axum::debug_handler]
pub(crate) async fn store_secure_backup_keys(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Path(backup_id): Path<String>,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let passphrase = body
        .get("passphrase")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("passphrase required".to_string()))?;

    let session_keys = body
        .get("session_keys")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|k| {
                    Some(crate::e2ee::secure_backup::SessionKeyData {
                        room_id: k.get("room_id")?.as_str()?.to_string(),
                        session_id: k.get("session_id")?.as_str()?.to_string(),
                        first_message_index: k.get("first_message_index")?.as_i64().unwrap_or(0),
                        forwarded_count: k.get("forwarded_count")?.as_i64().unwrap_or(0),
                        is_verified: k.get("is_verified").and_then(|v| v.as_bool()).unwrap_or(false),
                        session_key: k
                            .get("session_key")
                            .or_else(|| k.get("session_data").and_then(|sd| sd.get("session_key")))
                            .and_then(|v| v.as_str())?
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let key_count =
        ctx.secure_backup_service.store_session_keys(&auth_user.user_id, &backup_id, passphrase, session_keys).await?;

    Ok(Json(serde_json::json!({
        "count": key_count,
        "key_count": key_count
    })))
}

#[axum::debug_handler]
pub(crate) async fn restore_secure_backup(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Path(backup_id): Path<String>,
    MatrixJson(body): MatrixJson<RestoreSecureBackupRequest>,
) -> Result<Json<Value>, ApiError> {
    let response =
        ctx.secure_backup_service.restore_backup(&auth_user.user_id, &backup_id, &body.passphrase, body.rooms).await?;

    Ok(Json(serde_json::json!({
        "recovered_keys": response.recovered_keys,
        "total_keys": response.total_keys
    })))
}

#[axum::debug_handler]
pub(crate) async fn verify_secure_backup_passphrase(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Path(backup_id): Path<String>,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let passphrase = body
        .get("passphrase")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("passphrase required".to_string()))?;

    let valid = ctx.secure_backup_service.verify_passphrase(&auth_user.user_id, &backup_id, passphrase).await?;

    Ok(Json(serde_json::json!({
        "valid": valid
    })))
}

#[axum::debug_handler]
pub(crate) async fn delete_secure_backup(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Path(backup_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    ctx.secure_backup_service.delete_backup(&auth_user.user_id, &backup_id).await?;

    Ok(empty_json())
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct AuditPaginationQuery {
    limit: Option<usize>,
    from: Option<String>,
}

#[axum::debug_handler]
pub(crate) async fn get_key_history(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Query(params): Query<AuditPaginationQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = params.limit.unwrap_or(100).clamp(1, 1000);
    let cursor = params.from.as_deref().and_then(decode_key_request_cursor);

    let audit_service = synapse_services::e2ee_audit::E2eeAuditService::new(ctx.pool.clone());
    let history: Vec<synapse_services::e2ee_audit::KeyAuditEntry> = audit_service
        .get_key_history_paginated(
            &auth_user.user_id,
            limit as i64,
            cursor.as_ref().map(|c| c.0),
            cursor.as_ref().and_then(|c| c.1.parse::<i64>().ok()),
        )
        .await?;

    let next_batch = if history.len() == limit {
        history.last().map(|h| encode_key_request_cursor(h.created_ts, &h.id.to_string()))
    } else {
        None
    };

    Ok(Json(serde_json::json!({
        "history": history,
        "next_batch": next_batch
    })))
}

// Key-backup version + per-session handlers live in
// `src/web/routes/key_backup.rs` (registered through `create_key_backup_router`
// in `assembly.rs`). They were duplicated here historically and the routes
// silently won the merge over the spec-compliant ones; see
// `docs/synapse-rust/SPEC_ALIGNMENT_PLAN_2026-05-01.md` §1.2 for the audit.

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn test_e2ee_routes_structure() {
        let compat_routes = [
            "/_matrix/client/r0/keys/upload",
            "/_matrix/client/v3/keys/query",
            "/_matrix/client/r0/keys/device_signing/upload",
            "/_matrix/client/v3/sendToDevice/{event_type}/{transaction_id}",
        ];

        let v3_only_routes = [
            "/_matrix/client/v3/device_verification/request",
            "/_matrix/client/v3/device_trust/{device_id}",
            "/_matrix/client/v3/security/summary",
            "/_matrix/client/v3/keys/backup/secure/{backup_id}/verify",
        ];

        assert!(compat_routes.iter().all(|route| route.starts_with("/_matrix/client/")));
        assert!(v3_only_routes.iter().all(|route| route.starts_with("/_matrix/client/v3/")));
    }

    #[test]
    fn test_e2ee_compat_router_contains_shared_paths() {
        let shared_paths = [
            "/keys/upload",
            "/keys/query",
            "/keys/claim",
            "/keys/changes",
            "/keys/signatures/upload",
            "/keys/device_signing/upload",
            "/room_keys/request",
            "/room_keys/request/{request_id}",
            "/rooms/{room_id}/keys/distribution",
            "/sendToDevice/{event_type}/{transaction_id}",
        ];

        assert_eq!(shared_paths.len(), 10);
        assert!(shared_paths.iter().all(|path| path.starts_with('/')));
    }

    #[test]
    fn test_serialize_room_key_request_statuses() {
        let pending = super::super::devices::serialize_room_key_request(crate::e2ee::key_request::KeyRequestInfo {
            request_id: "req-1".to_string(),
            user_id: "@alice:example.org".to_string(),
            device_id: "DEVICE".to_string(),
            room_id: "!room:example.org".to_string(),
            session_id: "sess-1".to_string(),
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            action: "request".to_string(),
            created_ts: 1,
            is_fulfilled: false,
            fulfilled_by_device: None,
            fulfilled_ts: None,
        });
        let cancelled = super::super::devices::serialize_room_key_request(crate::e2ee::key_request::KeyRequestInfo {
            request_id: "req-2".to_string(),
            user_id: "@alice:example.org".to_string(),
            device_id: "DEVICE".to_string(),
            room_id: "!room:example.org".to_string(),
            session_id: "sess-2".to_string(),
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            action: "cancellation".to_string(),
            created_ts: 2,
            is_fulfilled: true,
            fulfilled_by_device: None,
            fulfilled_ts: None,
        });

        assert_eq!(pending["status"], "pending");
        assert_eq!(pending["request_type"], "request");
        assert_eq!(cancelled["status"], "cancelled");
    }

    #[test]
    fn test_has_upload_device_signing_keys_rejects_empty_payload() {
        assert!(!super::super::devices::has_upload_device_signing_keys(&json!({})));
        assert!(!super::super::devices::has_upload_device_signing_keys(&json!({
            "master_key": {},
            "self_signing_key": {},
            "user_signing_key": {}
        })));
    }

    #[test]
    fn test_has_upload_device_signing_keys_accepts_non_empty_key() {
        assert!(super::super::devices::has_upload_device_signing_keys(&json!({
            "master_key": {
                "usage": ["master"],
                "keys": { "ed25519:MASTER": "pubkey" }
            }
        })));
    }
}
