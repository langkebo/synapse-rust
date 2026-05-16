use super::route_ledger::{expand_under_prefixes, RouteEntry};
use super::{AppState, AuthenticatedUser};
use crate::common::ApiError;
use crate::services::uia_service::UiaService;
use axum::{
    extract::{Path, Query, State},
    http::{Method, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use validator::Validate;

/// Nest prefixes under which `create_key_backup_router` mounts its internal
/// router. Kept as a module-level constant so both the [`Router`] assembly
/// below and [`key_backup_route_manifest`] cannot drift apart.
const NEST_PREFIXES: &[&str] = &[
    "/_matrix/client/v1",
    "/_matrix/client/r0",
    "/_matrix/client/v3",
];

/// Manifest entry for every `(method, relative_path)` registered by
/// `create_key_backup_router`. Mirrors the `.route(...)` calls in
/// [`create_key_backup_router`] one-for-one — new routes there MUST add a
/// matching entry here.
fn relative_routes() -> Vec<(Method, &'static str)> {
    vec![
        // Backup version lifecycle.
        (Method::GET, "/room_keys/version"),
        (Method::POST, "/room_keys/version"),
        (Method::GET, "/room_keys/version/{version}"),
        (Method::PUT, "/room_keys/version/{version}"),
        (Method::DELETE, "/room_keys/version/{version}"),
        // Spec endpoints — version is a `?version=` query parameter.
        (Method::GET, "/room_keys/keys"),
        (Method::PUT, "/room_keys/keys"),
        (Method::DELETE, "/room_keys/keys"),
        (Method::GET, "/room_keys/keys/{room_id}"),
        (Method::PUT, "/room_keys/keys/{room_id}"),
        (Method::DELETE, "/room_keys/keys/{room_id}"),
        (Method::GET, "/room_keys/keys/{room_id}/{session_id}"),
        (Method::PUT, "/room_keys/keys/{room_id}/{session_id}"),
        (Method::DELETE, "/room_keys/keys/{room_id}/{session_id}"),
        // Legacy / MSC-compatibility: version is a path segment.
        (Method::GET, "/room_keys/{version}/keys"),
        (Method::PUT, "/room_keys/{version}/keys"),
        (Method::DELETE, "/room_keys/{version}/keys"),
        (Method::GET, "/room_keys/{version}/keys/{room_id}"),
        (Method::PUT, "/room_keys/{version}/keys/{room_id}"),
        (Method::DELETE, "/room_keys/{version}/keys/{room_id}"),
        (
            Method::GET,
            "/room_keys/{version}/keys/{room_id}/{session_id}",
        ),
        (
            Method::PUT,
            "/room_keys/{version}/keys/{room_id}/{session_id}",
        ),
        (
            Method::DELETE,
            "/room_keys/{version}/keys/{room_id}/{session_id}",
        ),
        // Recovery / verify helpers.
        (Method::POST, "/room_keys/recover"),
        (Method::GET, "/room_keys/recovery/{version}/progress"),
        (Method::GET, "/room_keys/verify/{version}"),
        (Method::POST, "/room_keys/batch_recover"),
        (Method::GET, "/room_keys/recover/{version}/{room_id}"),
        (
            Method::GET,
            "/room_keys/recover/{version}/{room_id}/{session_id}",
        ),
        // Export / import.
        (Method::GET, "/room_keys/export"),
        (Method::GET, "/room_keys/export/{version}"),
        (Method::POST, "/room_keys/import"),
        (Method::POST, "/room_keys/import/{version}"),
    ]
}

/// Manifest for the route ledger (§R4 / §O2 in SPEC_ALIGNMENT_PLAN_2026-05-01).
pub fn key_backup_route_manifest() -> Vec<RouteEntry> {
    expand_under_prefixes("key_backup", NEST_PREFIXES, &relative_routes())
}

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

    Router::new()
        .nest("/_matrix/client/v1", router.clone())
        .nest("/_matrix/client/r0", router.clone())
        .nest("/_matrix/client/v3", router)
        .with_state(state)
}

#[derive(Debug, Deserialize)]
pub struct VersionQuery {
    pub version: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateBackupVersionBody {
    #[validate(length(max = 255, message = "Algorithm name too long"))]
    pub algorithm: Option<String>,
    pub auth_data: Option<Value>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateBackupVersionBody {
    pub auth_data: Option<Value>,
}

#[axum::debug_handler]
async fn create_backup_version(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<axum::response::Response, ApiError> {
    let auth = body.get("auth");

    match auth {
        None => {
            let session = state
                .services
                .uia_service
                .create_session(&auth_user.user_id, UiaService::get_default_flows())
                .await;
            return Ok((
                StatusCode::UNAUTHORIZED,
                Json(state.services.uia_service.build_uia_response(
                    &session,
                    "M_UIA_REQUIRED",
                    "User-Interactive Authentication required",
                )),
            )
                .into_response());
        }
        Some(auth_val) => {
            let result = state
                .services
                .uia_service
                .validate_auth(
                    auth_val,
                    &auth_user.user_id,
                    UiaService::get_default_flows(),
                )
                .await;

            match result {
                Ok(_) => {}
                Err(uia_response) => {
                    return Ok((StatusCode::UNAUTHORIZED, Json(uia_response)).into_response());
                }
            }

            let auth_type = auth_val.get("type").and_then(|v| v.as_str()).unwrap_or("");
            match auth_type {
                "m.login.password" => {
                    if let Err(e) = state
                        .services
                        .uia_service
                        .verify_password_stage(
                            auth_val,
                            &auth_user.user_id,
                            &state.services.auth_service,
                        )
                        .await
                    {
                        let session = state
                            .services
                            .uia_service
                            .create_session(&auth_user.user_id, UiaService::get_default_flows())
                            .await;
                        return Ok((
                            StatusCode::UNAUTHORIZED,
                            Json(state.services.uia_service.build_uia_response(
                                &session,
                                "M_FORBIDDEN",
                                &e.to_string(),
                            )),
                        )
                            .into_response());
                    }
                }
                "m.login.token" => {}
                _ => {
                    let session = state
                        .services
                        .uia_service
                        .create_session(&auth_user.user_id, UiaService::get_default_flows())
                        .await;
                    return Ok((
                        StatusCode::UNAUTHORIZED,
                        Json(state.services.uia_service.build_uia_response(
                            &session,
                            "M_UIA_REQUIRED",
                            "Unsupported authentication type",
                        )),
                    )
                        .into_response());
                }
            }
        }
    }

    let algorithm = body
        .get("algorithm")
        .and_then(|v| v.as_str())
        .unwrap_or("m.megolm_backup.v1.curve25519-aes-sha2");
    let auth_data = body.get("auth_data").cloned();

    if let Some(ref data) = auth_data {
        if data.get("public_key").is_none() {
            return Err(ApiError::bad_request(
                "auth_data must contain public_key".to_string(),
            ));
        }
    }

    let version = state
        .services
        .backup_service
        .create_backup(&auth_user.user_id, algorithm, auth_data)
        .await?;

    Ok(Json(json!({
        "version": version
    }))
    .into_response())
}

#[axum::debug_handler]
async fn get_all_backup_versions(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, crate::error::ApiError> {
    let backups = state
        .services
        .backup_service
        .get_all_backups(&auth_user.user_id)
        .await?;

    let latest = backups
        .into_iter()
        .max_by_key(|b| b.version)
        .ok_or_else(|| {
            crate::error::ApiError::not_found("No current backup version".to_string())
        })?;

    let version_str = latest.version.to_string();
    let count = state
        .services
        .backup_service
        .get_backup_key_count_for_version(&auth_user.user_id, &version_str)
        .await?;

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
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(version): Path<String>,
) -> Result<Json<Value>, crate::error::ApiError> {
    let backup = state
        .services
        .backup_service
        .get_backup(&auth_user.user_id, &version)
        .await?;

    match backup {
        Some(b) => {
            let version_str = b.version.to_string();
            let count = state
                .services
                .backup_service
                .get_backup_key_count_for_version(&auth_user.user_id, &version_str)
                .await?;
            Ok(Json(serde_json::json!({
                "algorithm": b.algorithm,
                "auth_data": b.backup_data,
                "count": count,
                "etag": b.etag.unwrap_or_else(|| version_str.clone()),
                "version": version_str
            })))
        }
        None => Err(crate::error::ApiError::not_found(format!(
            "Backup version '{version}' not found"
        ))),
    }
}

#[axum::debug_handler]
async fn update_backup_version(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(version): Path<String>,
    Json(body): Json<UpdateBackupVersionBody>,
) -> Result<Json<Value>, crate::error::ApiError> {
    if let Err(e) = body.validate() {
        return Err(crate::error::ApiError::bad_request(e.to_string()));
    }

    let auth_data = body.auth_data;

    state
        .services
        .backup_service
        .update_backup_auth_data(&auth_user.user_id, &version, auth_data)
        .await?;

    Ok(Json(serde_json::json!({
        "version": version
    })))
}

#[axum::debug_handler]
async fn delete_backup_version(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(version): Path<String>,
) -> Result<Json<Value>, crate::error::ApiError> {
    let backup = state
        .services
        .backup_service
        .get_backup(&auth_user.user_id, &version)
        .await?;

    if backup.is_none() {
        return Err(crate::error::ApiError::not_found(format!(
            "Backup version '{version}' not found"
        )));
    }

    state
        .services
        .backup_service
        .delete_backup(&auth_user.user_id, &version)
        .await?;

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
    format!("{}_{}", version, chrono::Utc::now().timestamp_millis())
}

fn write_response(version: &str, count: u64) -> Json<Value> {
    Json(serde_json::json!({
        "etag": current_etag(version),
        "count": count,
    }))
}

async fn ensure_backup_exists(
    state: &AppState,
    user_id: &str,
    version: &str,
) -> Result<(), crate::error::ApiError> {
    state
        .services
        .backup_service
        .get_backup(user_id, version)
        .await?
        .ok_or_else(|| {
            crate::error::ApiError::not_found(format!("Backup version '{version}' not found"))
        })
        .map(|_| ())
}

// ----------------------------------------------------------------------------
// GET /room_keys/keys?version=...
// Returns {rooms: {room_id: {sessions: {session_id: KeyBackupData}}}}
// ----------------------------------------------------------------------------
async fn read_all_rooms(
    state: &AppState,
    user_id: &str,
    version: &str,
) -> Result<Json<Value>, crate::error::ApiError> {
    ensure_backup_exists(state, user_id, version).await?;
    let keys = state
        .services
        .backup_service
        .get_keys_for_version(user_id, version)
        .await?;

    let mut rooms = serde_json::Map::<String, Value>::new();
    for k in keys {
        let entry = rooms
            .entry(k.room_id.clone())
            .or_insert_with(|| serde_json::json!({"sessions": {}}));
        if let Some(sessions) = entry.get_mut("sessions").and_then(|v| v.as_object_mut()) {
            sessions.insert(k.session_id.clone(), k.session_data.clone());
        }
    }

    Ok(Json(serde_json::json!({ "rooms": rooms })))
}

#[axum::debug_handler]
async fn get_room_keys_all(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Query(q): Query<VersionQuery>,
) -> Result<Json<Value>, crate::error::ApiError> {
    read_all_rooms(&state, &auth_user.user_id, &q.version).await
}

#[axum::debug_handler]
async fn get_room_keys_all_legacy(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(version): Path<String>,
) -> Result<Json<Value>, crate::error::ApiError> {
    read_all_rooms(&state, &auth_user.user_id, &version).await
}

// ----------------------------------------------------------------------------
// GET /room_keys/keys/{room_id}?version=...
// Returns {sessions: {session_id: KeyBackupData}}
// ----------------------------------------------------------------------------
async fn read_room(
    state: &AppState,
    user_id: &str,
    version: &str,
    room_id: &str,
) -> Result<Json<Value>, crate::error::ApiError> {
    ensure_backup_exists(state, user_id, version).await?;
    let keys = state
        .services
        .backup_service
        .get_room_backup_keys(user_id, room_id, version)
        .await?;

    let mut sessions = serde_json::Map::<String, Value>::new();
    for k in keys {
        sessions.insert(k.session_id.clone(), k.session_data.clone());
    }

    Ok(Json(serde_json::json!({ "sessions": sessions })))
}

#[axum::debug_handler]
async fn get_room_keys_for_room(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Query(q): Query<VersionQuery>,
) -> Result<Json<Value>, crate::error::ApiError> {
    read_room(&state, &auth_user.user_id, &q.version, &room_id).await
}

#[axum::debug_handler]
async fn get_room_keys_for_room_legacy(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((version, room_id)): Path<(String, String)>,
) -> Result<Json<Value>, crate::error::ApiError> {
    read_room(&state, &auth_user.user_id, &version, &room_id).await
}

// ----------------------------------------------------------------------------
// GET /room_keys/keys/{room_id}/{session_id}?version=...
// Returns KeyBackupData
// ----------------------------------------------------------------------------
async fn read_session(
    state: &AppState,
    user_id: &str,
    version: &str,
    room_id: &str,
    session_id: &str,
) -> Result<Json<Value>, crate::error::ApiError> {
    let key = state
        .services
        .backup_service
        .get_backup_key(user_id, room_id, session_id, version)
        .await?
        .ok_or_else(|| {
            crate::error::ApiError::not_found(format!(
                "Session '{session_id}' in room '{room_id}' not found"
            ))
        })?;

    Ok(Json(key.session_data))
}

#[axum::debug_handler]
async fn get_room_key(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, session_id)): Path<(String, String)>,
    Query(q): Query<VersionQuery>,
) -> Result<Json<Value>, crate::error::ApiError> {
    read_session(
        &state,
        &auth_user.user_id,
        &q.version,
        &room_id,
        &session_id,
    )
    .await
}

#[axum::debug_handler]
async fn get_room_key_legacy(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((version, room_id, session_id)): Path<(String, String, String)>,
) -> Result<Json<Value>, crate::error::ApiError> {
    read_session(&state, &auth_user.user_id, &version, &room_id, &session_id).await
}

// ----------------------------------------------------------------------------
// PUT /room_keys/keys?version=...
// Body: {rooms: {room_id: {sessions: {session_id: KeyBackupData}}}}
// ----------------------------------------------------------------------------
async fn write_all_rooms(
    state: &AppState,
    user_id: &str,
    version: &str,
    body: RoomKeysBody,
) -> Result<Json<Value>, crate::error::ApiError> {
    ensure_backup_exists(state, user_id, version).await?;

    let mut count: u64 = 0;
    for (room_id, room_payload) in body.rooms {
        let sessions = room_payload
            .get("sessions")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        for (session_id, key_data) in sessions {
            state
                .services
                .backup_service
                .upload_session(user_id, version, &room_id, &session_id, key_data)
                .await?;
            count += 1;
        }
    }

    Ok(write_response(version, count))
}

#[axum::debug_handler]
async fn put_room_keys_all(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Query(q): Query<VersionQuery>,
    Json(body): Json<RoomKeysBody>,
) -> Result<Json<Value>, crate::error::ApiError> {
    write_all_rooms(&state, &auth_user.user_id, &q.version, body).await
}

#[axum::debug_handler]
async fn put_room_keys_all_legacy(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(version): Path<String>,
    Json(body): Json<RoomKeysBody>,
) -> Result<Json<Value>, crate::error::ApiError> {
    write_all_rooms(&state, &auth_user.user_id, &version, body).await
}

// ----------------------------------------------------------------------------
// PUT /room_keys/keys/{room_id}?version=...
// Body: {sessions: {session_id: KeyBackupData}}
// ----------------------------------------------------------------------------
async fn write_room(
    state: &AppState,
    user_id: &str,
    version: &str,
    room_id: &str,
    body: RoomSessionsBody,
) -> Result<Json<Value>, crate::error::ApiError> {
    ensure_backup_exists(state, user_id, version).await?;

    let mut count: u64 = 0;
    for (session_id, key_data) in body.sessions {
        state
            .services
            .backup_service
            .upload_session(user_id, version, room_id, &session_id, key_data)
            .await?;
        count += 1;
    }

    Ok(write_response(version, count))
}

#[axum::debug_handler]
async fn put_room_keys_for_room(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Query(q): Query<VersionQuery>,
    Json(body): Json<RoomSessionsBody>,
) -> Result<Json<Value>, crate::error::ApiError> {
    write_room(&state, &auth_user.user_id, &q.version, &room_id, body).await
}

#[axum::debug_handler]
async fn put_room_keys_for_room_legacy(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((version, room_id)): Path<(String, String)>,
    Json(body): Json<RoomSessionsBody>,
) -> Result<Json<Value>, crate::error::ApiError> {
    write_room(&state, &auth_user.user_id, &version, &room_id, body).await
}

// ----------------------------------------------------------------------------
// PUT /room_keys/keys/{room_id}/{session_id}?version=...
// Body: KeyBackupData
// ----------------------------------------------------------------------------
async fn write_session(
    state: &AppState,
    user_id: &str,
    version: &str,
    room_id: &str,
    session_id: &str,
    key_data: Value,
) -> Result<Json<Value>, crate::error::ApiError> {
    ensure_backup_exists(state, user_id, version).await?;
    state
        .services
        .backup_service
        .upload_session(user_id, version, room_id, session_id, key_data)
        .await?;
    Ok(write_response(version, 1))
}

#[axum::debug_handler]
async fn put_room_key(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, session_id)): Path<(String, String)>,
    Query(q): Query<VersionQuery>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, crate::error::ApiError> {
    write_session(
        &state,
        &auth_user.user_id,
        &q.version,
        &room_id,
        &session_id,
        body,
    )
    .await
}

#[axum::debug_handler]
async fn put_room_key_legacy(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((version, room_id, session_id)): Path<(String, String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, crate::error::ApiError> {
    write_session(
        &state,
        &auth_user.user_id,
        &version,
        &room_id,
        &session_id,
        body,
    )
    .await
}

// ----------------------------------------------------------------------------
// DELETE handlers (spec + legacy)
// ----------------------------------------------------------------------------
async fn delete_all_rooms_impl(
    state: &AppState,
    user_id: &str,
    version: &str,
) -> Result<Json<Value>, crate::error::ApiError> {
    ensure_backup_exists(state, user_id, version).await?;
    let count = state
        .services
        .backup_service
        .delete_all_for_version(user_id, version)
        .await?;
    Ok(write_response(version, count))
}

#[axum::debug_handler]
async fn delete_room_keys_all(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Query(q): Query<VersionQuery>,
) -> Result<Json<Value>, crate::error::ApiError> {
    delete_all_rooms_impl(&state, &auth_user.user_id, &q.version).await
}

#[axum::debug_handler]
async fn delete_room_keys_all_legacy(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(version): Path<String>,
) -> Result<Json<Value>, crate::error::ApiError> {
    delete_all_rooms_impl(&state, &auth_user.user_id, &version).await
}

async fn delete_room_impl(
    state: &AppState,
    user_id: &str,
    version: &str,
    room_id: &str,
) -> Result<Json<Value>, crate::error::ApiError> {
    ensure_backup_exists(state, user_id, version).await?;
    let count = state
        .services
        .backup_service
        .delete_room_for_version(user_id, version, room_id)
        .await?;
    Ok(write_response(version, count))
}

#[axum::debug_handler]
async fn delete_room_keys_for_room(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Query(q): Query<VersionQuery>,
) -> Result<Json<Value>, crate::error::ApiError> {
    delete_room_impl(&state, &auth_user.user_id, &q.version, &room_id).await
}

#[axum::debug_handler]
async fn delete_room_keys_for_room_legacy(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((version, room_id)): Path<(String, String)>,
) -> Result<Json<Value>, crate::error::ApiError> {
    delete_room_impl(&state, &auth_user.user_id, &version, &room_id).await
}

async fn delete_session_impl(
    state: &AppState,
    user_id: &str,
    version: &str,
    room_id: &str,
    session_id: &str,
) -> Result<Json<Value>, crate::error::ApiError> {
    ensure_backup_exists(state, user_id, version).await?;
    let count = state
        .services
        .backup_service
        .delete_session_for_version(user_id, version, room_id, session_id)
        .await?;
    Ok(write_response(version, count))
}

#[axum::debug_handler]
async fn delete_room_key(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, session_id)): Path<(String, String)>,
    Query(q): Query<VersionQuery>,
) -> Result<Json<Value>, crate::error::ApiError> {
    delete_session_impl(
        &state,
        &auth_user.user_id,
        &q.version,
        &room_id,
        &session_id,
    )
    .await
}

#[axum::debug_handler]
async fn delete_room_key_legacy(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((version, room_id, session_id)): Path<(String, String, String)>,
) -> Result<Json<Value>, crate::error::ApiError> {
    delete_session_impl(&state, &auth_user.user_id, &version, &room_id, &session_id).await
}

#[derive(Debug, Deserialize, Validate)]
pub struct RecoverKeysBody {
    pub version: String,
    pub rooms: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct BatchRecoverBody {
    pub version: String,
    pub room_ids: Vec<String>,
    pub session_limit: Option<i32>,
}

#[axum::debug_handler]
async fn recover_keys(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<RecoverKeysBody>,
) -> Result<Json<Value>, crate::error::ApiError> {
    if let Err(e) = body.validate() {
        return Err(crate::error::ApiError::bad_request(e.to_string()));
    }

    let response = state
        .services
        .backup_service
        .recover_keys(&auth_user.user_id, &body.version, body.rooms)
        .await?;

    Ok(Json(serde_json::to_value(response)?))
}

#[axum::debug_handler]
async fn get_recovery_progress(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(version): Path<String>,
) -> Result<Json<Value>, crate::error::ApiError> {
    let progress = state
        .services
        .backup_service
        .get_recovery_progress(&auth_user.user_id, &version)
        .await?;

    Ok(Json(serde_json::to_value(progress)?))
}

#[axum::debug_handler]
async fn verify_backup(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(version): Path<String>,
) -> Result<Json<Value>, crate::error::ApiError> {
    let verification = state
        .services
        .backup_service
        .verify_backup(&auth_user.user_id, &version)
        .await?;

    Ok(Json(serde_json::to_value(verification)?))
}

#[axum::debug_handler]
async fn batch_recover_keys(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<BatchRecoverBody>,
) -> Result<Json<Value>, crate::error::ApiError> {
    if let Err(e) = body.validate() {
        return Err(crate::error::ApiError::bad_request(e.to_string()));
    }

    let response = state
        .services
        .backup_service
        .batch_recover_keys(
            &auth_user.user_id,
            crate::e2ee::backup::models::BatchRecoveryRequest {
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
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((version, room_id)): Path<(String, String)>,
) -> Result<Json<Value>, crate::error::ApiError> {
    let keys = state
        .services
        .backup_service
        .recover_room_keys(&auth_user.user_id, &version, &room_id)
        .await?;

    Ok(Json(serde_json::json!({
        "room_id": room_id,
        "sessions": keys
    })))
}

#[axum::debug_handler]
async fn recover_session_key(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((version, room_id, session_id)): Path<(String, String, String)>,
) -> Result<Json<Value>, crate::error::ApiError> {
    let key = state
        .services
        .backup_service
        .recover_session_key(&auth_user.user_id, &version, &room_id, &session_id)
        .await?;

    match key {
        Some(k) => Ok(Json(serde_json::json!({
            "room_id": room_id,
            "session_id": session_id,
            "session_data": k
        }))),
        None => Err(crate::error::ApiError::not_found(format!(
            "Session '{session_id}' not found in room '{room_id}'"
        ))),
    }
}

// ============================================================================
// Key Export/Import (E2EE 100%)
// ============================================================================

/// Export all keys
/// GET /_matrix/client/r0/room_keys/export
#[axum::debug_handler]
async fn export_keys(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, crate::error::ApiError> {
    let backup_keys = state
        .services
        .backup_service
        .get_all_backup_keys(&auth_user.user_id)
        .await?;

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
/// GET /_matrix/client/r0/room_keys/export/{version}
#[axum::debug_handler]
async fn export_keys_by_version(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(version): Path<String>,
) -> Result<Json<Value>, crate::error::ApiError> {
    let backup_keys = state
        .services
        .backup_service
        .get_keys_for_version(&auth_user.user_id, &version)
        .await?;

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

/// Import keys
/// POST /_matrix/client/r0/room_keys/import
#[axum::debug_handler]
async fn import_keys(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, crate::error::ApiError> {
    let room_keys = body
        .get("room_keys")
        .and_then(|v| v.as_array())
        .ok_or_else(|| crate::error::ApiError::bad_request("Missing room_keys".to_string()))?;

    let version = body.get("version").and_then(|v| v.as_str()).unwrap_or("1");

    let mut imported_count = 0;
    let mut failed_count = 0;

    for key_data in room_keys.iter() {
        let room_id = key_data
            .get("room_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let session_id = key_data
            .get("session_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let session_data = key_data
            .get("session_data")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if !room_id.is_empty() && !session_id.is_empty() {
            let params = crate::e2ee::backup::BackupKeyUploadParams {
                user_id: auth_user.user_id.clone(),
                room_id: room_id.to_string(),
                session_id: session_id.to_string(),
                session_data: session_data.to_string(),
                version: version.to_string(),
                is_verified: key_data
                    .get("is_verified")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                first_message_index: key_data
                    .get("first_message_index")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0),
                forwarded_count: key_data
                    .get("forwarded_count")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0),
            };

            if state
                .services
                .backup_service
                .upload_backup_key(params)
                .await
                .is_ok()
            {
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
/// POST /_matrix/client/r0/room_keys/import/{version}
#[axum::debug_handler]
async fn import_keys_by_version(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(version): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, crate::error::ApiError> {
    let room_keys = body
        .get("room_keys")
        .and_then(|v| v.as_array())
        .ok_or_else(|| crate::error::ApiError::bad_request("Missing room_keys".to_string()))?;

    let mut imported_count = 0;
    let mut failed_count = 0;

    for key_data in room_keys.iter() {
        let room_id = key_data
            .get("room_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let session_id = key_data
            .get("session_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let session_data = key_data
            .get("session_data")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if !room_id.is_empty() && !session_id.is_empty() {
            let params = crate::e2ee::backup::BackupKeyUploadParams {
                user_id: auth_user.user_id.clone(),
                room_id: room_id.to_string(),
                session_id: session_id.to_string(),
                session_data: session_data.to_string(),
                version: version.clone(),
                is_verified: key_data
                    .get("is_verified")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                first_message_index: key_data
                    .get("first_message_index")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0),
                forwarded_count: key_data
                    .get("forwarded_count")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0),
            };

            if state
                .services
                .backup_service
                .upload_backup_key(params)
                .await
                .is_ok()
            {
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
