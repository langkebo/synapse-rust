use super::{AppState, AuthenticatedUser};
use crate::routes::context::E2eeRoomContext;
use crate::routes::extractors::RoomId;
use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use synapse_common::current_timestamp_millis;
use synapse_common::ApiError;
use validator::Validate;

/// See [`create_key_backup_router`].
pub fn create_key_backup_router(state: AppState) -> Router<AppState> {
    let router = Router::new()
        .route(
            "/room_keys/version",
            get(get_all_backup_versions).post(create_backup_version),
        )
        .route(
            "/room_keys/version/{version}",
            get(get_backup_version)
                .put(update_backup_version)
                .delete(delete_backup_version),
        )
        // Spec-compliant: version is a query param, not a path segment.
        .route(
            "/room_keys/keys",
            get(get_room_keys_all)
                .put(put_room_keys_all)
                .delete(delete_room_keys_all),
        )
        .route(
            "/room_keys/keys/{room_id}",
            get(get_room_keys_for_room)
                .put(put_room_keys_for_room)
                .delete(delete_room_keys_for_room),
        )
        .route(
            "/room_keys/keys/{room_id}/{session_id}",
            get(get_room_key).put(put_room_key).delete(delete_room_key),
        )
        // Legacy/MSC compatibility: version is encoded in the path.
        .route(
            "/room_keys/{version}/keys",
            get(get_room_keys_all_legacy)
                .put(put_room_keys_all_legacy)
                .delete(delete_room_keys_all_legacy),
        )
        .route(
            "/room_keys/{version}/keys/{room_id}",
            get(get_room_keys_for_room_legacy)
                .put(put_room_keys_for_room_legacy)
                .delete(delete_room_keys_for_room_legacy),
        )
        .route(
            "/room_keys/{version}/keys/{room_id}/{session_id}",
            get(get_room_key_legacy)
                .put(put_room_key_legacy)
                .delete(delete_room_key_legacy),
        )
        .route("/room_keys/recover", post(recover_keys))
        .route(
            "/room_keys/recovery/{version}/progress",
            get(get_recovery_progress),
        )
        .route("/room_keys/verify/{version}", get(verify_backup))
        .route("/room_keys/batch_recover", post(batch_recover_keys))
        .route(
            "/room_keys/recover/{version}/{room_id}",
            get(recover_room_keys),
        )
        .route(
            "/room_keys/recover/{version}/{room_id}/{session_id}",
            get(recover_session_key),
        )
        // Key Export/Import (E2EE 100%)
        .route("/room_keys/export", get(export_keys))
        .route("/room_keys/export/{version}", get(export_keys_by_version))
        .route("/room_keys/import", post(import_keys))
        .route("/room_keys/import/{version}", post(import_keys_by_version));
    // Note: /keys/backup/secure routes are handled in e2ee_routes.rs

    Router::new().nest("/_matrix/client/v1", router.clone()).nest("/_matrix/client/v3", router).with_state(state)
}

/// The `VersionQuery` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionQuery {
    /// The `version` field.
    pub version: String,
}

/// The `CreateBackupVersionBody` struct.
#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CreateBackupVersionBody {
    #[validate(length(max = 255, message = "Algorithm name too long"))]
    /// The `algorithm` field.
    pub algorithm: Option<String>,
    /// The `auth_data` field.
    pub auth_data: Option<Value>,
}

/// The `UpdateBackupVersionBody` struct.
#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct UpdateBackupVersionBody {
    /// The `auth_data` field.
    pub auth_data: Option<Value>,
}

#[axum::debug_handler]
async fn create_backup_version(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<axum::response::Response, ApiError> {
    let algorithm = body.get("algorithm").and_then(|v| v.as_str()).unwrap_or("m.megolm_backup.v1.curve25519-aes-sha2");
    let auth_data = body.get("auth_data").cloned();

    if let Some(ref data) = auth_data {
        if data.get("public_key").is_none() {
            return Err(ApiError::bad_request("auth_data must contain public_key".to_string()));
        }
    }

    let version = ctx.e2ee_backup_service.create_backup(&auth_user.user_id, algorithm, auth_data).await?;

    Ok(Json(json!({
        "version": version
    }))
    .into_response())
}

#[axum::debug_handler]
async fn get_all_backup_versions(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    let backups = ctx.e2ee_backup_service.get_all_backups(&auth_user.user_id).await?;

    let latest = backups
        .into_iter()
        .max_by_key(|b| b.version)
        .ok_or_else(|| synapse_common::error::ApiError::not_found("No current backup version".to_string()))?;

    let version_str = latest.version.to_string();
    let count = ctx.e2ee_backup_service.get_backup_key_count_for_version(&auth_user.user_id, &version_str).await?;

    Ok(Json(serde_json::json!({
        "algorithm": latest.algorithm,
        "auth_data": latest.backup_data,
        "count": count,
        "etag": latest.etag.unwrap_or_else(|| version_str.clone()),
        "version": version_str
    })))
}

#[axum::debug_handler]
async fn get_backup_version(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Path(version): Path<RoomId>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    let backup = ctx.e2ee_backup_service.get_backup(&auth_user.user_id, &version).await?;

    match backup {
        Some(b) => {
            let version_str = b.version.to_string();
            let count =
                ctx.e2ee_backup_service.get_backup_key_count_for_version(&auth_user.user_id, &version_str).await?;
            Ok(Json(serde_json::json!({
                "algorithm": b.algorithm,
                "auth_data": b.backup_data,
                "count": count,
                "etag": b.etag.unwrap_or_else(|| version_str.clone()),
                "version": version_str
            })))
        }
        None => Err(synapse_common::error::ApiError::not_found(format!("Backup version '{version}' not found"))),
    }
}

#[axum::debug_handler]
async fn update_backup_version(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Path(version): Path<RoomId>,
    Json(body): Json<UpdateBackupVersionBody>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    if let Err(e) = body.validate() {
        return Err(synapse_common::error::ApiError::bad_request(e.to_string()));
    }

    let auth_data = body.auth_data;

    ctx.e2ee_backup_service.update_backup_auth_data(&auth_user.user_id, &version, auth_data).await?;

    Ok(Json(serde_json::json!({
        "version": version
    })))
}

#[axum::debug_handler]
async fn delete_backup_version(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Path(version): Path<RoomId>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    let backup = ctx.e2ee_backup_service.get_backup(&auth_user.user_id, &version).await?;

    if backup.is_none() {
        return Err(synapse_common::error::ApiError::not_found(format!("Backup version '{version}' not found")));
    }

    ctx.e2ee_backup_service.delete_backup(&auth_user.user_id, &version).await?;

    Ok(Json(serde_json::json!({
        "deleted": true,
        "version": version
    })))
}

// ----------------------------------------------------------------------------
// Spec body shapes for /room_keys/keys[*] (Matrix C-S §11.13)
// ----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RoomSessionsBody {
    #[serde(default)]
    sessions: std::collections::HashMap<String, Value>,
}

#[derive(Debug, Deserialize)]
struct RoomKeysBody {
    #[serde(default)]
    rooms: std::collections::HashMap<String, Value>,
}

fn current_etag(version: &str) -> String {
    format!("{}_{}", version, current_timestamp_millis())
}

fn write_response(version: &str, count: u64) -> Json<Value> {
    Json(serde_json::json!({
        "etag": current_etag(version),
        "count": count,
    }))
}

async fn ensure_backup_exists(
    ctx: &E2eeRoomContext,
    user_id: &str,
    version: &str,
) -> Result<(), synapse_common::error::ApiError> {
    ctx.e2ee_backup_service
        .get_backup(user_id, version)
        .await?
        .ok_or_else(|| synapse_common::error::ApiError::not_found(format!("Backup version '{version}' not found")))
        .map(|_| ())
}

// ----------------------------------------------------------------------------
// GET /room_keys/keys?version=...
// Returns {rooms: {room_id: {sessions: {session_id: KeyBackupData}}}}
// ----------------------------------------------------------------------------
async fn read_all_rooms(
    ctx: &E2eeRoomContext,
    user_id: &str,
    version: &str,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    ensure_backup_exists(ctx, user_id, version).await?;
    let keys = ctx.e2ee_backup_service.get_keys_for_version(user_id, version).await?;

    let mut rooms = serde_json::Map::<String, Value>::new();
    for k in keys {
        let entry = rooms.entry(k.room_id.clone()).or_insert_with(|| serde_json::json!({"sessions": {}}));
        if let Some(sessions) = entry.get_mut("sessions").and_then(|v| v.as_object_mut()) {
            sessions.insert(k.session_id.clone(), k.session_data.clone());
        }
    }

    Ok(Json(serde_json::json!({ "rooms": rooms, "etag": version })))
}

#[axum::debug_handler]
async fn get_room_keys_all(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Query(q): Query<VersionQuery>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    read_all_rooms(&ctx, &auth_user.user_id, &q.version).await
}

#[axum::debug_handler]
async fn get_room_keys_all_legacy(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Path(version): Path<RoomId>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    read_all_rooms(&ctx, &auth_user.user_id, &version).await
}

// ----------------------------------------------------------------------------
// GET /room_keys/keys/{room_id}?version=...
// Returns {sessions: {session_id: KeyBackupData}}
// ----------------------------------------------------------------------------
async fn read_room(
    ctx: &E2eeRoomContext,
    user_id: &str,
    version: &str,
    room_id: &str,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    ensure_backup_exists(ctx, user_id, version).await?;
    let keys = ctx.e2ee_backup_service.get_room_backup_keys(user_id, room_id, version).await?;

    let mut sessions = serde_json::Map::<String, Value>::new();
    for k in keys {
        sessions.insert(k.session_id.clone(), k.session_data.clone());
    }

    Ok(Json(serde_json::json!({ "sessions": sessions })))
}

#[axum::debug_handler]
async fn get_room_keys_for_room(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
    Query(q): Query<VersionQuery>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    read_room(&ctx, &auth_user.user_id, &q.version, &room_id).await
}

#[axum::debug_handler]
async fn get_room_keys_for_room_legacy(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Path((version, room_id)): Path<(String, String)>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    read_room(&ctx, &auth_user.user_id, &version, &room_id).await
}

// ----------------------------------------------------------------------------
// GET /room_keys/keys/{room_id}/{session_id}?version=...
// Returns KeyBackupData
// ----------------------------------------------------------------------------
async fn read_session(
    ctx: &E2eeRoomContext,
    user_id: &str,
    version: &str,
    room_id: &str,
    session_id: &str,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    let key =
        ctx.e2ee_backup_service.get_backup_key(user_id, room_id, session_id, version).await?.ok_or_else(|| {
            synapse_common::error::ApiError::not_found(format!("Session '{session_id}' in room '{room_id}' not found"))
        })?;

    Ok(Json(key.session_data))
}

#[axum::debug_handler]
async fn get_room_key(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, session_id)): Path<(String, String)>,
    Query(q): Query<VersionQuery>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    read_session(&ctx, &auth_user.user_id, &q.version, &room_id, &session_id).await
}

#[axum::debug_handler]
async fn get_room_key_legacy(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Path((version, room_id, session_id)): Path<(String, String, String)>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    read_session(&ctx, &auth_user.user_id, &version, &room_id, &session_id).await
}

// ----------------------------------------------------------------------------
// PUT /room_keys/keys?version=...
// Body: {rooms: {room_id: {sessions: {session_id: KeyBackupData}}}}
// ----------------------------------------------------------------------------
async fn write_all_rooms(
    ctx: &E2eeRoomContext,
    user_id: &str,
    version: &str,
    body: RoomKeysBody,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    ensure_backup_exists(ctx, user_id, version).await?;

    let mut count: u64 = 0;
    for (room_id, room_payload) in body.rooms {
        let sessions = room_payload.get("sessions").and_then(|v| v.as_object()).cloned().unwrap_or_default();
        for (session_id, key_data) in sessions {
            ctx.e2ee_backup_service.upload_session(user_id, version, &room_id, &session_id, key_data).await?;
            count += 1;
        }
    }

    Ok(write_response(version, count))
}

#[axum::debug_handler]
async fn put_room_keys_all(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Query(q): Query<VersionQuery>,
    Json(body): Json<RoomKeysBody>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    write_all_rooms(&ctx, &auth_user.user_id, &q.version, body).await
}

#[axum::debug_handler]
async fn put_room_keys_all_legacy(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Path(version): Path<RoomId>,
    Json(body): Json<RoomKeysBody>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    write_all_rooms(&ctx, &auth_user.user_id, &version, body).await
}

// ----------------------------------------------------------------------------
// PUT /room_keys/keys/{room_id}?version=...
// Body: {sessions: {session_id: KeyBackupData}}
// ----------------------------------------------------------------------------
async fn write_room(
    ctx: &E2eeRoomContext,
    user_id: &str,
    version: &str,
    room_id: &str,
    body: RoomSessionsBody,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    ensure_backup_exists(ctx, user_id, version).await?;

    let mut count: u64 = 0;
    for (session_id, key_data) in body.sessions {
        ctx.e2ee_backup_service.upload_session(user_id, version, room_id, &session_id, key_data).await?;
        count += 1;
    }

    Ok(write_response(version, count))
}

#[axum::debug_handler]
async fn put_room_keys_for_room(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
    Query(q): Query<VersionQuery>,
    Json(body): Json<RoomSessionsBody>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    write_room(&ctx, &auth_user.user_id, &q.version, &room_id, body).await
}

#[axum::debug_handler]
async fn put_room_keys_for_room_legacy(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Path((version, room_id)): Path<(String, String)>,
    Json(body): Json<RoomSessionsBody>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    write_room(&ctx, &auth_user.user_id, &version, &room_id, body).await
}

// ----------------------------------------------------------------------------
// PUT /room_keys/keys/{room_id}/{session_id}?version=...
// Body: KeyBackupData
// ----------------------------------------------------------------------------
async fn write_session(
    ctx: &E2eeRoomContext,
    user_id: &str,
    version: &str,
    room_id: &str,
    session_id: &str,
    key_data: Value,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    ensure_backup_exists(ctx, user_id, version).await?;
    ctx.e2ee_backup_service.upload_session(user_id, version, room_id, session_id, key_data).await?;
    Ok(write_response(version, 1))
}

#[axum::debug_handler]
async fn put_room_key(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, session_id)): Path<(String, String)>,
    Query(q): Query<VersionQuery>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    write_session(&ctx, &auth_user.user_id, &q.version, &room_id, &session_id, body).await
}

#[axum::debug_handler]
async fn put_room_key_legacy(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Path((version, room_id, session_id)): Path<(String, String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    write_session(&ctx, &auth_user.user_id, &version, &room_id, &session_id, body).await
}

// ----------------------------------------------------------------------------
// DELETE handlers (spec + legacy)
// ----------------------------------------------------------------------------
async fn delete_all_rooms_impl(
    ctx: &E2eeRoomContext,
    user_id: &str,
    version: &str,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    ensure_backup_exists(ctx, user_id, version).await?;
    let count = ctx.e2ee_backup_service.delete_all_for_version(user_id, version).await?;
    Ok(write_response(version, count))
}

#[axum::debug_handler]
async fn delete_room_keys_all(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Query(q): Query<VersionQuery>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    delete_all_rooms_impl(&ctx, &auth_user.user_id, &q.version).await
}

#[axum::debug_handler]
async fn delete_room_keys_all_legacy(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Path(version): Path<RoomId>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    delete_all_rooms_impl(&ctx, &auth_user.user_id, &version).await
}

async fn delete_room_impl(
    ctx: &E2eeRoomContext,
    user_id: &str,
    version: &str,
    room_id: &str,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    ensure_backup_exists(ctx, user_id, version).await?;
    let count = ctx.e2ee_backup_service.delete_room_for_version(user_id, version, room_id).await?;
    Ok(write_response(version, count))
}

#[axum::debug_handler]
async fn delete_room_keys_for_room(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
    Query(q): Query<VersionQuery>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    delete_room_impl(&ctx, &auth_user.user_id, &q.version, &room_id).await
}

#[axum::debug_handler]
async fn delete_room_keys_for_room_legacy(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Path((version, room_id)): Path<(String, String)>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    delete_room_impl(&ctx, &auth_user.user_id, &version, &room_id).await
}

async fn delete_session_impl(
    ctx: &E2eeRoomContext,
    user_id: &str,
    version: &str,
    room_id: &str,
    session_id: &str,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    ensure_backup_exists(ctx, user_id, version).await?;
    let count = ctx.e2ee_backup_service.delete_session_for_version(user_id, version, room_id, session_id).await?;
    Ok(write_response(version, count))
}

#[axum::debug_handler]
async fn delete_room_key(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, session_id)): Path<(String, String)>,
    Query(q): Query<VersionQuery>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    delete_session_impl(&ctx, &auth_user.user_id, &q.version, &room_id, &session_id).await
}

#[axum::debug_handler]
async fn delete_room_key_legacy(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Path((version, room_id, session_id)): Path<(String, String, String)>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    delete_session_impl(&ctx, &auth_user.user_id, &version, &room_id, &session_id).await
}

/// The `RecoverKeysBody` struct.
#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct RecoverKeysBody {
    /// The `version` field.
    pub version: String,
    /// The `rooms` field.
    pub rooms: Option<Vec<String>>,
}

/// The `BatchRecoverBody` struct.
#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct BatchRecoverBody {
    /// The `version` field.
    pub version: String,
    /// The `room_ids` field.
    pub room_ids: Vec<String>,
    /// The `session_limit` field.
    pub session_limit: Option<i32>,
}

#[axum::debug_handler]
async fn recover_keys(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Json(body): Json<RecoverKeysBody>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    if let Err(e) = body.validate() {
        return Err(synapse_common::error::ApiError::bad_request(e.to_string()));
    }

    let response = ctx.e2ee_backup_service.recover_keys(&auth_user.user_id, &body.version, body.rooms).await?;

    Ok(Json(serde_json::to_value(response)?))
}

#[axum::debug_handler]
async fn get_recovery_progress(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Path(version): Path<RoomId>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    let progress = ctx.e2ee_backup_service.get_recovery_progress(&auth_user.user_id, &version).await?;

    Ok(Json(serde_json::to_value(progress)?))
}

#[axum::debug_handler]
async fn verify_backup(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Path(version): Path<RoomId>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    let verification = ctx.e2ee_backup_service.verify_backup(&auth_user.user_id, &version).await?;

    Ok(Json(serde_json::to_value(verification)?))
}

#[axum::debug_handler]
async fn batch_recover_keys(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Json(body): Json<BatchRecoverBody>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    if let Err(e) = body.validate() {
        return Err(synapse_common::error::ApiError::bad_request(e.to_string()));
    }

    let response = ctx
        .e2ee_backup_service
        .batch_recover_keys(
            &auth_user.user_id,
            synapse_e2ee::backup::models::BatchRecoveryRequest {
                version: body.version,
                room_ids: body.room_ids,
                session_limit: body.session_limit,
            },
        )
        .await?;

    Ok(Json(serde_json::to_value(response)?))
}

#[axum::debug_handler]
async fn recover_room_keys(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Path((version, room_id)): Path<(String, String)>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    let keys = ctx.e2ee_backup_service.recover_room_keys(&auth_user.user_id, &version, &room_id).await?;

    Ok(Json(serde_json::json!({
        "room_id": room_id,
        "sessions": keys
    })))
}

#[axum::debug_handler]
async fn recover_session_key(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Path((version, room_id, session_id)): Path<(String, String, String)>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    let key = ctx.e2ee_backup_service.recover_session_key(&auth_user.user_id, &version, &room_id, &session_id).await?;

    match key {
        Some(k) => Ok(Json(serde_json::json!({
            "room_id": room_id,
            "session_id": session_id,
            "session_data": k
        }))),
        None => Err(synapse_common::error::ApiError::not_found(format!(
            "Session '{session_id}' not found in room '{room_id}'"
        ))),
    }
}

// ============================================================================
// Key Export/Import (E2EE 100%)
// ============================================================================

/// Export all keys
/// GET /_matrix/client/v3/room_keys/export
#[axum::debug_handler]
async fn export_keys(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    let backup_keys = ctx.e2ee_backup_service.get_all_backup_keys(&auth_user.user_id).await?;

    let mut room_keys = Vec::new();
    for key in backup_keys {
        room_keys.push(serde_json::json!({
            "room_id": key.room_id,
            "session_id": key.session_id,
            "session_data": key.session_data,
            "first_message_index": key.first_message_index,
            "forwarded_count": key.forwarded_count,
            "is_verified": key.is_verified
        }));
    }

    let export_data = serde_json::json!({
        "room_keys": room_keys,
        "version": "1"
    });

    Ok(Json(export_data))
}

/// Export keys by version
/// GET /_matrix/client/v3/room_keys/export/{version}
#[axum::debug_handler]
async fn export_keys_by_version(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Path(version): Path<RoomId>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    let backup_keys = ctx.e2ee_backup_service.get_keys_for_version(&auth_user.user_id, &version).await?;

    let mut room_keys = Vec::new();
    for key in backup_keys {
        room_keys.push(serde_json::json!({
            "room_id": key.room_id,
            "session_id": key.session_id,
            "session_data": key.session_data,
            "first_message_index": key.first_message_index,
            "forwarded_count": key.forwarded_count,
            "is_verified": key.is_verified
        }));
    }

    let export_data = serde_json::json!({
        "room_keys": room_keys,
        "version": version
    });

    Ok(Json(export_data))
}

/// Resolve the `version` field for `import_keys`.
///
/// FT-126: previously defaulted silently to "1" when missing, which could
/// write keys to the wrong backup version. Now requires the field and
/// returns a 400 Bad Request when it is absent or not a string.
pub fn resolve_import_version(body: &Value) -> Result<String, ApiError> {
    body.get("version")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| ApiError::bad_request("version is required".to_string()))
}

/// Import keys
/// POST /_matrix/client/v3/room_keys/import
#[axum::debug_handler]
async fn import_keys(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    let room_keys = body
        .get("room_keys")
        .and_then(|v| v.as_array())
        .ok_or_else(|| synapse_common::error::ApiError::bad_request("Missing room_keys".to_string()))?;

    let version = resolve_import_version(&body)?;

    let mut imported_count = 0;
    let mut failed_count = 0;

    for key_data in room_keys.iter() {
        let room_id = key_data.get("room_id").and_then(|v| v.as_str()).unwrap_or("");
        let session_id = key_data.get("session_id").and_then(|v| v.as_str()).unwrap_or("");
        let session_data = key_data.get("session_data").and_then(|v| v.as_str()).unwrap_or("");

        if !room_id.is_empty() && !session_id.is_empty() && !session_data.is_empty() {
            let params = synapse_e2ee::backup::BackupKeyUploadParams {
                user_id: auth_user.user_id.clone(),
                room_id: room_id.to_string(),
                session_id: session_id.to_string(),
                session_data: session_data.to_string(),
                version: version.to_string(),
                is_verified: key_data.get("is_verified").and_then(|v| v.as_bool()).unwrap_or(false),
                first_message_index: key_data.get("first_message_index").and_then(|v| v.as_i64()).unwrap_or(0),
                forwarded_count: key_data.get("forwarded_count").and_then(|v| v.as_i64()).unwrap_or(0),
            };

            if ctx.e2ee_backup_service.upload_backup_key(params).await.is_ok() {
                imported_count += 1;
            } else {
                failed_count += 1;
            }
        } else {
            failed_count += 1;
        }
    }

    Ok(Json(serde_json::json!({
        "count": imported_count,
        "failed": failed_count,
        "total": room_keys.len()
    })))
}

/// Import keys by version
/// POST /_matrix/client/v3/room_keys/import/{version}
#[axum::debug_handler]
async fn import_keys_by_version(
    State(ctx): State<E2eeRoomContext>,
    auth_user: AuthenticatedUser,
    Path(version): Path<RoomId>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    let room_keys = body
        .get("room_keys")
        .and_then(|v| v.as_array())
        .ok_or_else(|| synapse_common::error::ApiError::bad_request("Missing room_keys".to_string()))?;

    let mut imported_count = 0;
    let mut failed_count = 0;

    for key_data in room_keys.iter() {
        let room_id = key_data.get("room_id").and_then(|v| v.as_str()).unwrap_or("");
        let session_id = key_data.get("session_id").and_then(|v| v.as_str()).unwrap_or("");
        let session_data = key_data.get("session_data").and_then(|v| v.as_str()).unwrap_or("");

        if !room_id.is_empty() && !session_id.is_empty() && !session_data.is_empty() {
            let params = synapse_e2ee::backup::BackupKeyUploadParams {
                user_id: auth_user.user_id.clone(),
                room_id: room_id.to_string(),
                session_id: session_id.to_string(),
                session_data: session_data.to_string(),
                version: version.to_string(),
                is_verified: key_data.get("is_verified").and_then(|v| v.as_bool()).unwrap_or(false),
                first_message_index: key_data.get("first_message_index").and_then(|v| v.as_i64()).unwrap_or(0),
                forwarded_count: key_data.get("forwarded_count").and_then(|v| v.as_i64()).unwrap_or(0),
            };

            if ctx.e2ee_backup_service.upload_backup_key(params).await.is_ok() {
                imported_count += 1;
            } else {
                failed_count += 1;
            }
        } else {
            failed_count += 1;
        }
    }

    Ok(Json(serde_json::json!({
        "count": imported_count,
        "failed": failed_count,
        "total": room_keys.len()
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_import_version_accepts_string() {
        let body = serde_json::json!({ "version": "7" });
        assert_eq!(super::resolve_import_version(&body).unwrap(), "7");
    }

    #[test]
    fn test_resolve_import_version_rejects_missing() {
        let body = serde_json::json!({ "room_keys": {} });
        let err = super::resolve_import_version(&body).unwrap_err();
        assert_eq!(err.kind, synapse_common::ApiErrorKind::BadRequest);
    }

    #[test]
    fn test_resolve_import_version_rejects_non_string() {
        let body = serde_json::json!({ "version": 7 });
        let err = super::resolve_import_version(&body).unwrap_err();
        assert_eq!(err.kind, synapse_common::ApiErrorKind::BadRequest);
    }

    #[test]
    fn test_current_etag_has_version_prefix() {
        let etag = super::current_etag("5");
        assert!(etag.starts_with("5_"), "etag should be {{version}}_{{ts}}: {etag}");
        let ts: u64 = etag.split('_').nth(1).unwrap().parse().expect("suffix should be timestamp");
        assert!(ts > 0);
    }

    #[test]
    fn test_write_response_shape() {
        let Json(resp) = super::write_response("5", 3);
        assert!(resp["etag"].as_str().unwrap().starts_with("5_"));
        assert_eq!(resp["count"], 3);
    }

    #[test]
    fn test_key_backup_routes_all_room_keys_prefixed() {
        // Sourced from the derived route table — the hand-copied
        // `relative_routes()` helper it replaced is gone with the manifests.
        let ledger = crate::routes::assembly::declared_ledger_all();
        let routes: Vec<_> = ledger.iter().filter(|e| e.registered_by == "key_backup").collect();
        assert!(!routes.is_empty());
        for entry in &routes {
            assert!(entry.path.contains("/room_keys/"), "unexpected path: {}", entry.path);
        }
        // 去重：同一 (method, path) 不应重复注册。
        let mut seen = std::collections::HashSet::new();
        for entry in &routes {
            assert!(
                seen.insert((entry.method.clone(), entry.path)),
                "duplicate route: {} {}",
                entry.method,
                entry.path
            );
        }
    }
}
