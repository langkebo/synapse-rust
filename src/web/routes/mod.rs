pub mod account_data;
pub mod admin;
pub mod ai_connection;
pub mod app_service;
mod assembly;
pub mod background_update;
pub mod burn_after_read;
pub mod captcha;
pub mod cas;
pub mod device;
pub mod directory;
pub mod dm;
pub mod e2ee_routes;
pub mod ephemeral;
pub mod event_report;
pub mod external_service;
pub mod extractors;
pub mod federation;
pub mod friend_room;
pub mod guest;
pub mod handlers;
pub mod invite_blocklist;
pub mod key_backup;
pub mod key_rotation;
pub mod media;
pub mod module;
pub mod oidc;
pub mod push;
pub mod push_notification;
pub mod push_rules;
pub mod validators;
pub mod qr_login;
pub mod reactions;
pub mod relations;
pub mod rendezvous;
pub mod room_summary;
pub mod saml;
pub mod search;
pub mod sliding_sync;
pub mod space;
pub mod state;
pub mod sticky_event;
pub mod tags;
pub mod telemetry;
pub mod thirdparty;
pub mod thread;
pub mod typing;
pub mod verification_routes;
pub mod voice;
pub mod voip;
pub mod widget;
pub mod worker;

pub use account_data::create_account_data_router;
pub use admin::create_admin_module_router;
pub use ai_connection::create_ai_connection_router;
pub use app_service::create_app_service_router;
pub use assembly::create_router;
pub use handlers::{
    get_capabilities, get_client_versions, get_server_version, get_well_known_client,
    get_well_known_server, get_well_known_support, health_check,
};
pub use background_update::create_background_update_router;
pub use burn_after_read::create_burn_after_read_router;
pub use captcha::create_captcha_router;
pub use cas::cas_routes;
pub use device::create_device_router;
pub use dm::create_dm_router;
pub use e2ee_routes::create_e2ee_router;
pub use event_report::create_event_report_router;
pub use external_service::create_external_service_router;
pub(crate) use extractors::extract_token_from_headers;
pub use extractors::{
    AdminUser, AuthExtractor, AuthenticatedUser, MatrixJson, OptionalAuthenticatedUser,
};
pub use federation::create_federation_router;
pub use friend_room::create_friend_router;
pub use guest::create_guest_router;
pub use key_backup::create_key_backup_router;
pub use key_rotation::create_key_rotation_router;
pub use media::create_media_router;
pub use module::create_module_router;
pub use oidc::create_oidc_router;
pub use push::create_push_router;
pub use push_notification::create_push_notification_router;
pub use push_rules::{get_push_rules_default, get_push_rules_global_default, get_default_push_rules};
pub use reactions::create_reactions_router;
pub use relations::create_relations_router;
pub use rendezvous::create_rendezvous_router;
pub use room_summary::create_room_summary_router;
pub use saml::create_saml_router;
pub use search::create_search_router;
pub use sliding_sync::create_sliding_sync_router;
pub use space::create_space_router;
pub use state::AppState;
pub use validators::{validate_user_id, validate_room_id, validate_event_id, validate_presence_status, validate_receipt_type};
pub use tags::create_tags_router;
pub use telemetry::create_telemetry_router;
pub use thirdparty::create_thirdparty_router;
pub use thread::create_thread_routes;
pub use verification_routes::create_verification_router;
pub use voice::create_voice_router;
pub use voip::call_answer;
pub use voip::call_candidates;
pub use voip::call_hangup;
pub use voip::call_invite;
pub use voip::get_call_session;
pub use voip::get_turn_credentials_guest;
pub use voip::get_turn_server;
pub use voip::get_voip_config;
pub use widget::create_widget_router;
pub use worker::create_worker_router;

use crate::common::*;
use crate::services::*;
use crate::storage::CreateEventParams;
use axum::{
    extract::{Json, Path, Query, State},
    http::HeaderMap,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{types::JsonValue, Row};

#[cfg(test)]
mod top_level_router_tests {
    #[test]
    fn test_top_level_compat_routes_structure() {
        let compat_routes = [
            "/_matrix/client/r0/capabilities",
            "/_matrix/client/v3/capabilities",
            "/_matrix/client/r0/voip/config",
            "/_matrix/client/v3/rooms/{room_id}/call/{call_id}",
            "/_matrix/client/v1/media/config",
            "/_matrix/client/r0/media/config",
            "/_matrix/client/v3/media/config",
        ];

        assert!(compat_routes
            .iter()
            .all(|route| route.starts_with("/_matrix/client/")));
    }

    #[test]
    fn test_voip_compat_router_contains_shared_paths() {
        let shared_paths = [
            "/voip/turnServer",
            "/voip/config",
            "/voip/turnServer/guest",
            "/rooms/{room_id}/send/m.call.invite/{txn_id}",
            "/rooms/{room_id}/send/m.call.candidates/{txn_id}",
            "/rooms/{room_id}/send/m.call.answer/{txn_id}",
            "/rooms/{room_id}/send/m.call.hangup/{txn_id}",
            "/rooms/{room_id}/call/{call_id}",
        ];

        assert_eq!(shared_paths.len(), 8);
        assert!(shared_paths.iter().all(|path| path.starts_with('/')));
    }

    #[test]
    fn test_top_level_router_keeps_version_boundaries() {
        let capabilities_paths = ["/capabilities"];
        let media_config_paths = ["/media/config"];
        let direct_only_paths = ["/_matrix/client/versions", "/_matrix/client/v3/versions"];

        assert!(capabilities_paths
            .iter()
            .all(|path| !path.contains("/versions")));
        assert!(media_config_paths
            .iter()
            .all(|path| path.starts_with("/media/")));
        assert!(direct_only_paths
            .iter()
            .all(|path| !path.ends_with("/capabilities")));
    }
}

#[cfg(test)]
mod auth_router_tests {
    #[test]
    fn test_auth_routes_structure() {
        let compat_routes = [
            "/_matrix/client/r0/register",
            "/_matrix/client/v3/login",
            "/_matrix/client/r0/logout/all",
            "/_matrix/client/v3/refresh",
        ];
        let v1_only_routes = [
            "/_matrix/client/v1/login/get_qr_code",
            "/_matrix/client/v1/login/qr/{transaction_id}/status",
        ];

        assert!(compat_routes
            .iter()
            .all(|route| route.starts_with("/_matrix/client/")));
        assert!(v1_only_routes
            .iter()
            .all(|route| route.starts_with("/_matrix/client/v1/")));
    }

    #[test]
    fn test_auth_compat_router_contains_shared_paths() {
        let shared_paths = [
            "/register",
            "/register/available",
            "/register/email/requestToken",
            "/register/email/submitToken",
            "/login",
            "/logout",
            "/logout/all",
            "/refresh",
        ];

        assert_eq!(shared_paths.len(), 8);
        assert!(shared_paths.iter().all(|path| path.starts_with('/')));
    }

    #[test]
    fn test_auth_router_keeps_qr_login_outside_compat_scope() {
        let compat_paths = ["/register", "/login", "/refresh"];
        let qr_paths = [
            "/_matrix/client/v1/login/get_qr_code",
            "/_matrix/client/v1/login/qr/start",
        ];

        assert!(compat_paths.iter().all(|path| !path.contains("/login/qr")));
        assert!(qr_paths
            .iter()
            .all(|path| path.starts_with("/_matrix/client/v1/login/")));
    }
}

#[cfg(test)]
mod account_router_tests {
    #[test]
    fn test_account_routes_structure() {
        let compat_routes = [
            "/_matrix/client/r0/account/whoami",
            "/_matrix/client/v3/account/password",
            "/_matrix/client/r0/account/3pid",
            "/_matrix/client/v3/profile/{user_id}/avatar_url",
        ];
        let r0_only_routes = [
            "/_matrix/client/r0/account/profile/{user_id}",
            "/_matrix/client/r0/account/profile/{user_id}/displayname",
            "/_matrix/client/r0/account/profile/{user_id}/avatar_url",
        ];

        assert!(compat_routes
            .iter()
            .all(|route| route.starts_with("/_matrix/client/")));
        assert!(r0_only_routes
            .iter()
            .all(|route| route.starts_with("/_matrix/client/r0/")));
    }

    #[test]
    fn test_account_compat_router_contains_shared_paths() {
        let shared_paths = [
            "/account/whoami",
            "/account/password",
            "/account/deactivate",
            "/account/3pid",
            "/profile/{user_id}",
            "/profile/{user_id}/displayname",
            "/profile/{user_id}/avatar_url",
        ];

        assert_eq!(shared_paths.len(), 7);
        assert!(shared_paths.iter().all(|path| path.starts_with('/')));
    }

    #[test]
    fn test_account_router_keeps_r0_account_profile_outside_compat_scope() {
        let compat_paths = ["/account/whoami", "/account/3pid", "/profile/{user_id}"];
        let r0_only_paths = ["/_matrix/client/r0/account/profile/{user_id}"];
        let absent_v3_paths = ["/_matrix/client/v3/account/profile/{user_id}"];

        assert!(compat_paths
            .iter()
            .all(|path| !path.starts_with("/account/profile/")));
        assert!(r0_only_paths
            .iter()
            .all(|path| path.starts_with("/_matrix/client/r0/")));
        assert!(absent_v3_paths
            .iter()
            .all(|path| path.starts_with("/_matrix/client/v3/account/profile/")));
    }
}

#[cfg(test)]
mod directory_router_tests {
    #[test]
    fn test_directory_routes_structure() {
        let compat_routes = [
            "/_matrix/client/r0/user_directory/search",
            "/_matrix/client/v3/user_directory/list",
            "/_matrix/client/r0/directory/room/{room_alias}",
            "/_matrix/client/v3/publicRooms",
        ];
        let r0_only_routes = [
            "/_matrix/client/r0/directory/room/{room_id}/alias",
            "/_matrix/client/r0/directory/room/{room_id}/alias/{room_alias}",
        ];

        assert!(compat_routes
            .iter()
            .all(|route| route.starts_with("/_matrix/client/")));
        assert!(r0_only_routes
            .iter()
            .all(|route| route.starts_with("/_matrix/client/r0/")));
    }

    #[test]
    fn test_directory_compat_router_contains_shared_paths() {
        let shared_paths = [
            "/user_directory/search",
            "/user_directory/list",
            "/directory/list/room/{room_id}",
            "/directory/room/{room_alias}",
            "/publicRooms",
        ];

        assert_eq!(shared_paths.len(), 5);
        assert!(shared_paths.iter().all(|path| path.starts_with('/')));
    }

    #[test]
    fn test_directory_router_keeps_r0_alias_management_outside_compat_scope() {
        let compat_paths = [
            "/directory/room/{room_alias}",
            "/publicRooms",
            "/directory/list/room/{room_id}",
        ];
        let r0_only_paths = [
            "/_matrix/client/r0/directory/room/{room_id}/alias",
            "/_matrix/client/r0/directory/room/{room_id}/alias/{room_alias}",
        ];

        assert!(compat_paths
            .iter()
            .all(|path| !path.contains("/alias/{room_alias}") && !path.ends_with("/alias")));
        assert!(r0_only_paths
            .iter()
            .all(|path| path.starts_with("/_matrix/client/r0/")));
    }
}

#[cfg(test)]
mod room_router_tests {
    #[test]
    fn test_room_routes_structure() {
        let r0_v3_compat_routes = [
            "/_matrix/client/r0/rooms/{room_id}",
            "/_matrix/client/v3/rooms/{room_id}/messages",
            "/_matrix/client/r0/rooms/{room_id}/state/{event_type}",
            "/_matrix/client/v3/rooms/{room_id}/event/{event_id}",
        ];
        let all_version_report_routes = [
            "/_matrix/client/r0/rooms/{room_id}/report/{event_id}",
            "/_matrix/client/v1/rooms/{room_id}/report/{event_id}/score",
            "/_matrix/client/v3/rooms/{room_id}/report/{event_id}",
        ];
        let version_specific_routes = [
            "/_matrix/client/r0/createRoom",
            "/_matrix/client/v1/rooms/{room_id}/report/{event_id}/scanner_info",
            "/_matrix/client/v3/rooms/{room_id}/notifications",
        ];

        assert!(r0_v3_compat_routes
            .iter()
            .all(|route| route.starts_with("/_matrix/client/")));
        assert_eq!(all_version_report_routes.len(), 3);
        assert_eq!(version_specific_routes.len(), 3);
    }

    #[test]
    fn test_room_compat_router_contains_shared_paths() {
        let shared_paths = [
            "/rooms/{room_id}",
            "/events",
            "/rooms/{room_id}/messages",
            "/rooms/{room_id}/send/{event_type}/{txn_id}",
            "/rooms/{room_id}/event/{event_id}",
        ];

        assert_eq!(shared_paths.len(), 5);
        assert!(shared_paths.iter().all(|path| path.starts_with('/')));
    }

    #[test]
    fn test_room_router_keeps_version_boundaries() {
        let report_compat_paths = ["/rooms/{room_id}/report/{event_id}"];
        let v1_only_paths = ["/_matrix/client/v1/rooms/{room_id}/report/{event_id}/scanner_info"];
        let v3_only_paths = [
            "/_matrix/client/v3/rooms/{room_id}/report",
            "/_matrix/client/v3/rooms/{room_id}/notifications",
        ];

        assert!(report_compat_paths
            .iter()
            .all(|path| !path.contains("scanner_info") && !path.ends_with("/report")));
        assert!(v1_only_paths
            .iter()
            .all(|path| path.starts_with("/_matrix/client/v1/")));
        assert!(v3_only_paths
            .iter()
            .all(|path| path.starts_with("/_matrix/client/v3/")));
    }
}

#[cfg(test)]
mod presence_router_tests {
    #[test]
    fn test_presence_routes_structure() {
        let compat_routes = [
            "/_matrix/client/r0/presence/{user_id}/status",
            "/_matrix/client/v3/presence/{user_id}/status",
        ];
        let v3_only_routes = ["/_matrix/client/v3/presence/list"];

        assert!(compat_routes
            .iter()
            .all(|route| route.starts_with("/_matrix/client/")));
        assert!(v3_only_routes
            .iter()
            .all(|route| route.starts_with("/_matrix/client/v3/")));
    }

    #[test]
    fn test_presence_compat_router_contains_shared_paths() {
        let shared_paths = ["/presence/{user_id}/status"];

        assert_eq!(shared_paths.len(), 1);
        assert!(shared_paths
            .iter()
            .all(|path| path.starts_with("/presence/")));
    }

    #[test]
    fn test_presence_router_keeps_presence_list_outside_compat_scope() {
        let compat_paths = ["/presence/{user_id}/status"];
        let v3_only_paths = ["/_matrix/client/v3/presence/list"];

        assert!(compat_paths
            .iter()
            .all(|path| !path.ends_with("/presence/list")));
        assert!(v3_only_paths
            .iter()
            .all(|path| path.ends_with("/presence/list")));
    }
}

async fn register(
    State(state): State<AppState>,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let auth = body.get("auth").cloned();
    let auth_type = auth
        .as_ref()
        .and_then(|a| a.get("type"))
        .and_then(|t| t.as_str());

    let username = body.get("username").and_then(|v| v.as_str());
    let password = body.get("password").and_then(|v| v.as_str());

    if username.is_none() || password.is_none() {
        if auth_type == Some("m.login.dummy") || auth_type == Some("m.login.password") {
            return Err(ApiError::bad_request(
                "Username and password required".to_string(),
            ));
        }
        return Ok(Json(json!({
            "flows": [
                { "stages": ["m.login.dummy"] },
                { "stages": ["m.login.password"] }
            ],
            "params": {},
            "session": uuid::Uuid::new_v4().to_string()
        })));
    }

    let username =
        username.ok_or_else(|| ApiError::bad_request("Username required".to_string()))?;
    let password =
        password.ok_or_else(|| ApiError::bad_request("Password required".to_string()))?;

    state
        .services
        .auth_service
        .validator
        .validate_username(username)?;
    state
        .services
        .auth_service
        .validator
        .validate_password(password)?;

    let admin = false;
    let displayname = body.get("displayname").and_then(|v| v.as_str());

    Ok(Json(
        state
            .services
            .registration_service
            .register_user(username, password, admin, displayname)
            .await?,
    ))
}

async fn check_username_availability(
    State(state): State<AppState>,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    let username = params
        .get("username")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Username required".to_string()))?;

    if let Err(e) = state
        .services
        .auth_service
        .validator
        .validate_username(username)
    {
        return Err(e.into());
    }

    let user_id = format!("@{}:{}", username, state.services.server_name);
    let exists = state
        .services
        .user_storage
        .user_exists(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "available": !exists,
        "username": username
    })))
}

async fn request_email_verification(
    State(state): State<AppState>,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let email = body
        .get("email")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Email is required".to_string()))?;

    if state
        .services
        .auth_service
        .validator
        .validate_email(email)
        .is_err()
    {
        return Err(ApiError::bad_request(
            "Invalid email address format".to_string(),
        ));
    }

    let client_secret = body.get("client_secret").and_then(|v| v.as_str());
    if client_secret.is_none() {
        return Err(ApiError::bad_request(
            "client_secret is required".to_string(),
        ));
    }

    let _send_attempt = body
        .get("send_attempt")
        .and_then(|v| v.as_u64())
        .unwrap_or(1);

    let token = state
        .services
        .auth_service
        .generate_email_verification_token()
        .map_err(|e| {
            ::tracing::error!("Failed to generate email verification token: {}", e);
            ApiError::internal(
                "Failed to generate verification token. Please try again later.".to_string(),
            )
        })?;

    let session_data = body.get("client_secret").cloned();

    let token_id = state
        .services
        .email_verification_storage
        .create_verification_token(email, &token, 3600, None, session_data)
        .await
        .map_err(|e| {
            ::tracing::error!("Failed to store email verification token: {}", e);
            ApiError::internal(
                "Failed to store verification token. Please try again later.".to_string(),
            )
        })?;

    let sid = format!("{}", token_id);

    let submit_url = format!(
        "https://{}:{}/_matrix/client/r0/register/email/submitToken",
        state.services.config.server.host, state.services.config.server.port
    );

    ::tracing::info!(
        "Email verification token created for {}: sid={}",
        email,
        sid
    );

    Ok(Json(json!({
        "sid": sid,
        "submit_url": submit_url,
        "expires_in": 3600
    })))
}

async fn submit_email_token(
    State(state): State<AppState>,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let sid = body
        .get("sid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Session ID (sid) is required".to_string()))?;

    let client_secret = body
        .get("client_secret")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Client secret is required".to_string()))?;

    let token = body
        .get("token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Verification token is required".to_string()))?;

    let sid_int: i64 = sid
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid session ID format".to_string()))?;

    let verification_token = state
        .services
        .email_verification_storage
        .get_verification_token_by_id(sid_int)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get verification token: {}", e)))?;

    let verification_token = match verification_token {
        Some(t) => t,
        None => {
            return Err(ApiError::bad_request(
                "Invalid session ID or session not found".to_string(),
            ))
        }
    };

    if verification_token.used {
        return Err(ApiError::bad_request(
            "Verification token has already been used".to_string(),
        ));
    }

    if verification_token.expires_at < chrono::Utc::now().timestamp() {
        return Err(ApiError::bad_request(
            "Verification token has expired".to_string(),
        ));
    }

    if verification_token.token != token {
        return Err(ApiError::bad_request(
            "Invalid verification token".to_string(),
        ));
    }

    if verification_token.session_data != Some(serde_json::Value::String(client_secret.to_string()))
        && verification_token.session_data.as_ref().map(|v| v.as_str()) != Some(Some(client_secret))
    {
        return Err(ApiError::bad_request("Client secret mismatch".to_string()));
    }

    state
        .services
        .email_verification_storage
        .mark_token_used(sid_int)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to mark token as used: {}", e)))?;

    Ok(Json(json!({
        "success": true
    })))
}

async fn get_login_flows(State(_state): State<AppState>) -> Json<Value> {
    let flows = vec![
        json!({"type": "m.login.password"}),
        json!({"type": "m.login.token"}),
    ];

    Json(json!({ "flows": flows }))
}

async fn get_register_flows() -> Json<Value> {
    Json(json!({
        "flows": [
            {"type": "m.login.dummy"},
            {"type": "m.login.password"}
        ],
        "params": {}
    }))
}

async fn login(
    State(state): State<AppState>,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let username = body
        .get("user")
        .or(body.get("username"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Username required".to_string()))?;
    let password = body
        .get("password")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Password required".to_string()))?;

    // P1 Quality: Basic validation
    if username.is_empty() || password.is_empty() {
        return Err(ApiError::bad_request(
            "Username and password are required".to_string(),
        ));
    }

    if username.len() > 255 {
        return Err(ApiError::bad_request("Username too long".to_string()));
    }

    // Check password length to prevent DoS - must match Validator::validate_password max (128)
    if password.len() > 128 {
        return Err(ApiError::bad_request(
            "Password too long (max 128 characters)".to_string(),
        ));
    }

    let device_id = body.get("device_id").and_then(|v| v.as_str());
    let initial_display_name = body.get("initial_display_name").and_then(|v| v.as_str());

    let (user, access_token, refresh_token, device_id) = state
        .services
        .auth_service
        .login(username, password, device_id, initial_display_name)
        .await?;

    Ok(Json(json!({
        "access_token": access_token,
        "refresh_token": refresh_token,
        "expires_in": state.services.auth_service.token_expiry,
        "device_id": device_id,
        "user_id": user.user_id(),
        "well_known": {
            "m.homeserver": {
                "base_url": format!(
                    "http://{}:{}",
                    state.services.config.server.host,
                    state.services.config.server.port
                )
            }
        }
    })))
}

async fn logout(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    state
        .services
        .auth_service
        .logout(&auth_user.access_token, auth_user.device_id.as_deref())
        .await?;

    Ok(Json(json!({})))
}

async fn get_single_event(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;

    let is_member = state
        .services
        .member_storage
        .is_member(&room_id, &auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check membership: {}", e)))?;

    if !is_member && !auth_user.is_admin {
        return Err(ApiError::forbidden(
            "You must be a member of this room to view events".to_string(),
        ));
    }

    let event = state
        .services
        .event_storage
        .get_event(&event_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get event: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Event not found".to_string()))?;

    if event.room_id != room_id {
        return Err(ApiError::not_found(
            "Event not found in this room".to_string(),
        ));
    }

    Ok(Json(json!({
        "event_id": event.event_id,
        "room_id": event.room_id,
        "sender": event.user_id,
        "type": event.event_type,
        "content": event.content,
        "origin_server_ts": event.origin_server_ts,
        "state_key": event.state_key
    })))
}

async fn logout_all(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    state
        .services
        .auth_service
        .logout_all(&auth_user.user_id)
        .await?;

    Ok(Json(json!({})))
}

async fn refresh_token(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let refresh_token = body
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Refresh token required".to_string()))?;

    let (new_access, new_refresh, device_id) = state
        .services
        .auth_service
        .refresh_token(refresh_token)
        .await?;

    Ok(Json(json!({
        "access_token": new_access,
        "refresh_token": new_refresh,
        "expires_in": state.services.auth_service.token_expiry,
        "device_id": device_id
    })))
}

async fn whoami(
    State(_state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    // Matrix spec: https://spec.matrix.org/v1.11/client-server-api/#get_matrixclientv3accountwhoami
    // Only user_id, device_id, and optionally is_guest
    Ok(Json(json!({
        "user_id": auth_user.user_id,
        "device_id": auth_user.device_id,
        "is_guest": false
    })))
}

async fn get_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;

    let token = extract_token_from_headers(&headers).ok();
    let requester_id = if let Some(t) = token {
        state
            .services
            .auth_service
            .validate_token(&t)
            .await
            .ok()
            .map(|(id, _, _)| id)
    } else {
        None
    };

    let privacy_settings = state
        .services
        .privacy_storage
        .get_settings(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if let Some(settings) = privacy_settings {
        let visibility = settings.profile_visibility.as_str();
        match visibility {
            "private" => {
                if requester_id.as_deref() != Some(user_id.as_str()) {
                    return Err(ApiError::forbidden("Profile is private".to_string()));
                }
            }
            "contacts" => {
                if requester_id.as_deref() != Some(user_id.as_str()) {
                    return Err(ApiError::forbidden(
                        "Profile is only visible to contacts".to_string(),
                    ));
                }
            }
            _ => {}
        }
    }

    Ok(Json(
        state
            .services
            .registration_service
            .get_profile(&user_id)
            .await?,
    ))
}

async fn get_displayname(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;

    let profile = state
        .services
        .registration_service
        .get_profile(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get profile: {}", e)))?;

    let displayname = profile
        .get("displayname")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    Ok(Json(json!({ "displayname": displayname })))
}

async fn get_avatar_url(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;

    let profile = state
        .services
        .registration_service
        .get_profile(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get profile: {}", e)))?;

    let avatar_url = profile
        .get("avatar_url")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    Ok(Json(json!({ "avatar_url": avatar_url })))
}

async fn update_displayname(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;

    let displayname = body
        .get("displayname")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Displayname required".to_string()))?;

    if displayname.len() > 255 {
        return Err(ApiError::bad_request(
            "Displayname too long (max 255 characters)".to_string(),
        ));
    }

    if user_id != auth_user.user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let user_exists = state
        .services
        .user_storage
        .user_exists(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check user existence: {}", e)))?;

    if !user_exists {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    state
        .services
        .registration_service
        .update_user_profile(&user_id, Some(displayname), None)
        .await?;
    Ok(Json(json!({})))
}

async fn update_avatar(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;

    let avatar_url = body
        .get("avatar_url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Avatar URL required".to_string()))?;

    if avatar_url.len() > 255 {
        return Err(ApiError::bad_request(
            "Avatar URL too long (max 255 characters)".to_string(),
        ));
    }

    if user_id != auth_user.user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let user_exists = state
        .services
        .user_storage
        .user_exists(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check user existence: {}", e)))?;

    if !user_exists {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    state
        .services
        .registration_service
        .update_user_profile(&user_id, None, Some(avatar_url))
        .await?;
    Ok(Json(json!({})))
}

#[allow(dead_code)]
async fn change_password(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let new_password = body
        .get("new_password")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("New password required".to_string()))?;

    state
        .services
        .auth_service
        .validator
        .validate_password(new_password)?;

    state
        .services
        .registration_service
        .change_password(&auth_user.user_id, new_password)
        .await?;

    Ok(Json(json!({})))
}

async fn change_password_uia(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let new_password = body
        .get("new_password")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("New password required".to_string()))?;

    let auth = body.get("auth").cloned().unwrap_or(serde_json::json!({}));
    let auth_type = auth.get("type").and_then(|v| v.as_str()).unwrap_or("");

    if auth_type == "m.login.dummy" {
        state
            .services
            .auth_service
            .validator
            .validate_password(new_password)?;

        state
            .services
            .registration_service
            .change_password(&auth_user.user_id, new_password)
            .await?;

        Ok(Json(json!({})))
    } else if auth_type == "m.login.password" {
        let password = auth
            .get("password")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ApiError::bad_request("Password required for m.login.password".to_string())
            })?;

        let user_identifier = auth
            .get("identifier")
            .and_then(|i| i.get("user"))
            .and_then(|u| u.as_str())
            .or_else(|| auth.get("user").and_then(|u| u.as_str()));

        if let Some(username) = user_identifier {
            let user_id = if username.starts_with('@') {
                username.to_string()
            } else {
                format!("@{}:{}", username, state.services.server_name)
            };

            if user_id != auth_user.user_id {
                return Err(ApiError::forbidden("User mismatch".to_string()));
            }

            state
                .services
                .auth_service
                .validator
                .validate_password(new_password)?;

            let user = state
                .services
                .user_storage
                .get_user_by_id(&user_id)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to get user: {}", e)))?
                .ok_or_else(|| ApiError::not_found("User not found".to_string()))?;

            let password_hash = user
                .password_hash
                .ok_or_else(|| ApiError::forbidden("User has no password set".to_string()))?;

            let valid = crate::common::crypto::verify_password(password, &password_hash, false)
                .map_err(|e| ApiError::internal(format!("Password verification failed: {}", e)))?;

            if !valid {
                return Err(ApiError::forbidden("Invalid password".to_string()));
            }

            state
                .services
                .registration_service
                .change_password(&auth_user.user_id, new_password)
                .await?;

            Ok(Json(json!({})))
        } else {
            Err(ApiError::bad_request(
                "User identifier required".to_string(),
            ))
        }
    } else {
        Err(ApiError::unauthorized(
            "Authentication required".to_string(),
        ))
    }
}

async fn deactivate_account(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let user_id = auth_user.user_id.clone();

    state
        .services
        .registration_service
        .deactivate_account(&user_id)
        .await?;

    state
        .services
        .cache
        .delete(&format!("user:active:{}", user_id))
        .await;

    state
        .services
        .cache
        .delete(&format!("token:{}", auth_user.access_token))
        .await;

    Ok(Json(json!({
        "id_server_unbind_result": "success"
    })))
}

async fn get_threepids(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let user_id = &auth_user.user_id;

    let threepids = sqlx::query(
        r#"
        SELECT medium, address, validated_ts, added_ts
        FROM user_threepids
        WHERE user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_all(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to get threepids: {}", e)))?;

    let threepids_list: Vec<Value> = threepids
        .iter()
        .map(|row| {
            json!({
                "medium": row.get::<String, _>("medium"),
                "address": row.get::<String, _>("address"),
                "validated_ts": row.get::<Option<i64>, _>("validated_ts").unwrap_or(0),
                "added_at": row.get::<Option<i64>, _>("added_ts").unwrap_or(0)
            })
        })
        .collect();

    Ok(Json(json!({
        "threepids": threepids_list
    })))
}

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
struct AddThreepidRequest {
    #[serde(rename = "threePidCreds")]
    three_pid_creds: Option<ThreepidCreds>,
    bind: Option<bool>,
}

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
struct ThreepidCreds {
    client_secret: Option<String>,
    sid: Option<String>,
}

async fn add_threepid(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let user_id = &auth_user.user_id;
    let now = chrono::Utc::now().timestamp_millis();

    let medium = body
        .get("medium")
        .and_then(|v| v.as_str())
        .unwrap_or("email");
    let address = body.get("address").and_then(|v| v.as_str()).unwrap_or("");

    if address.is_empty() {
        return Err(ApiError::bad_request("Address is required".to_string()));
    }

    sqlx::query(
        r#"
        INSERT INTO user_threepids (user_id, medium, address, validated_ts, added_ts)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (medium, address) DO UPDATE
        SET validated_ts = EXCLUDED.validated_ts
        "#,
    )
    .bind(user_id)
    .bind(medium)
    .bind(address)
    .bind(now)
    .bind(now)
    .execute(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to add threepid: {}", e)))?;

    Ok(Json(json!({})))
}

#[derive(Debug, serde::Deserialize)]
struct DeleteThreepidRequest {
    medium: String,
    address: String,
}

async fn delete_threepid(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<DeleteThreepidRequest>,
) -> Result<Json<Value>, ApiError> {
    let user_id = &auth_user.user_id;

    sqlx::query(
        r#"
        DELETE FROM user_threepids
        WHERE user_id = $1 AND medium = $2 AND address = $3
        "#,
    )
    .bind(user_id)
    .bind(&body.medium)
    .bind(&body.address)
    .execute(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to delete threepid: {}", e)))?;

    Ok(Json(json!({})))
}

async fn unbind_threepid(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<DeleteThreepidRequest>,
) -> Result<Json<Value>, ApiError> {
    let user_id = &auth_user.user_id;

    sqlx::query(
        r#"
        DELETE FROM user_threepids
        WHERE user_id = $1 AND medium = $2 AND address = $3
        "#,
    )
    .bind(user_id)
    .bind(&body.medium)
    .bind(&body.address)
    .execute(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to unbind threepid: {}", e)))?;

    Ok(Json(json!({})))
}

async fn search_user_directory(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (_, _, _) = state.services.auth_service.validate_token(&token).await?;

    let search_query = body
        .get("search_term")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let limit = body.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as i64;

    let results = state
        .services
        .user_storage
        .search_users(&search_query, limit)
        .await?;

    let users: Vec<Value> = results
        .into_iter()
        .map(|u| {
            json!({
                "user_id": u.user_id,
                "display_name": u.displayname.unwrap_or_else(|| u.username.clone()),
                "avatar_url": u.avatar_url
            })
        })
        .collect();

    Ok(Json(json!({
        "limited": users.len() >= limit as usize,
        "results": users
    })))
}

async fn list_user_directory(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (_, _, _) = state.services.auth_service.validate_token(&token).await?;

    let limit = body.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as i64;

    let offset = body.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as i64;

    let total_count = state.services.user_storage.get_user_count().await?;

    let users = state
        .services
        .user_storage
        .get_users_paginated(limit, offset)
        .await?;

    let users_json: Vec<Value> = users
        .into_iter()
        .map(|u| {
            json!({
                "user_id": u.user_id,
                "display_name": u.displayname.unwrap_or_else(|| u.username.clone()),
                "avatar_url": u.avatar_url
            })
        })
        .collect();

    Ok(Json(json!({
        "total": total_count,
        "offset": offset,
        "users": users_json
    })))
}

async fn report_event(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;

    let reason = body.get("reason").and_then(|v| v.as_str());
    let score = body.get("score").and_then(|v| v.as_i64()).unwrap_or(-100) as i32;

    let event = state
        .services
        .event_storage
        .get_event(&event_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Event not found".to_string()))?;

    if event.room_id != room_id {
        return Err(ApiError::bad_request(
            "Event does not belong to the specified room".to_string(),
        ));
    }

    let report_id = state
        .services
        .event_storage
        .report_event(
            &event_id,
            &room_id,
            &event.user_id,
            &auth_user.user_id,
            reason,
            score,
        )
        .await?;

    Ok(Json(json!({
        "report_id": report_id
    })))
}

async fn update_report_score(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path((_room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_event_id(&event_id)?;

    let score =
        body.get("score")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| ApiError::bad_request("Score is required".to_string()))? as i32;

    state
        .services
        .event_storage
        .update_event_report_score_by_event(&event_id, score)
        .await?;

    Ok(Json(json!({})))
}

// Report room (MSC3891) - report entire room without specific event
async fn report_room(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let members = sqlx::query("SELECT room_id FROM rooms WHERE room_id = $1 LIMIT 1")
        .bind(&room_id)
        .fetch_optional(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if members.is_none() {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    // Get reason and optional description from request body
    let reason = body
        .get("reason")
        .and_then(|v| v.as_str())
        .unwrap_or("No reason provided");
    let description = body.get("description").and_then(|v| v.as_str());

    // Log the room report for moderation purposes
    ::tracing::info!(
        "Room report submitted: room_id={}, user_id={}, reason={}",
        room_id,
        auth_user.user_id,
        reason
    );

    // Return a report ID (in a full implementation, this would be a real DB entry)
    let report_id = format!("{}_{}", room_id, chrono::Utc::now().timestamp_millis());

    Ok(Json(json!({
        "report_id": report_id,
        "room_id": room_id,
        "reason": reason,
        "description": description,
        "status": "submitted"
    })))
}

// Get content scanner info (MSC3891) - get scanner information for an event
async fn get_scanner_info(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    // In a full implementation, this would check with an external content scanner service
    // For now, return placeholder data indicating scanner is not configured
    Ok(Json(json!({
        "scanner_enabled": false,
        "room_id": room_id,
        "event_id": event_id,
        "status": "not_configured",
        "message": "Content scanner is not enabled on this server"
    })))
}

// Get room notifications (MSC3891) - get notifications for a specific room
async fn get_room_notifications(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let limit = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);

    let _from = params.get("from").cloned();

    // Get notifications for this specific room
    let notifications = sqlx::query(
        r#"
        SELECT event_id, room_id, ts, notification_type, is_read
        FROM notifications
        WHERE user_id = $1 AND room_id = $2
        ORDER BY ts DESC
        LIMIT $3
        "#,
    )
    .bind(&auth_user.user_id)
    .bind(&room_id)
    .bind(limit as i64)
    .fetch_all(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let notifications_list: Vec<Value> = notifications
        .iter()
        .map(|row| {
            let event_id = row.get::<Option<String>, _>("event_id").unwrap_or_default();
            json!({
                "event_id": event_id,
                "room_id": row.get::<Option<String>, _>("room_id"),
                "ts": row.get::<Option<i64>, _>("ts"),
                "profile_tag": row.get::<Option<String>, _>("notification_type"),
                "read": row.get::<Option<bool>, _>("is_read").unwrap_or(false),
                "room_name": None::<Value>,
                "sender": None::<Value>,
                "prio": "high",
                "client_action": "notify"
            })
        })
        .collect();

    Ok(Json(json!({
        "notifications": notifications_list,
        "next_token": None::<String>
    })))
}

async fn sync(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _) = state.services.auth_service.validate_token(&token).await?;

    let timeout = params
        .get("timeout")
        .and_then(|v| v.as_u64())
        .unwrap_or(30000);
    let full_state = params
        .get("full_state")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let set_presence = params
        .get("set_presence")
        .and_then(|v| v.as_str())
        .unwrap_or("online");
    let since = params.get("since").and_then(|v| v.as_str());

    let sync_result = tokio::time::timeout(
        std::time::Duration::from_secs(60),
        state
            .services
            .sync_service
            .sync(&user_id, timeout, full_state, set_presence, since),
    )
    .await;

    match sync_result {
        Ok(Ok(result)) => Ok(Json(result)),
        Ok(Err(e)) => {
            ::tracing::error!("Sync error for user {}: {}", user_id, e);
            Err(e)
        }
        Err(_) => {
            ::tracing::error!("Sync timeout for user {}", user_id);
            Err(ApiError::internal("Sync operation timed out".to_string()))
        }
    }
}

#[derive(serde::Serialize)]
#[allow(dead_code)]
struct FilterResponse {
    filter_id: String,
    room: Option<Value>,
    presence: Option<Value>,
}

#[allow(dead_code)]
async fn create_filter(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(_user_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<FilterResponse>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (_, _, _) = state.services.auth_service.validate_token(&token).await?;

    let filter_id = format!("f{}", uuid::Uuid::new_v4());

    Ok(Json(FilterResponse {
        filter_id,
        room: body.get("room").cloned(),
        presence: body.get("presence").cloned(),
    }))
}

#[allow(dead_code)]
async fn get_filter(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((_user_id, filter_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (_, _, _) = state.services.auth_service.validate_token(&token).await?;

    Ok(Json(json!({
        "filter_id": filter_id,
        "filter": {
            "room": {
                "state": {"limit": 50},
                "timeline": {"limit": 50}
            },
            "presence": {"limit": 100}
        }
    })))
}

async fn get_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _) = state.services.auth_service.validate_token(&token).await?;

    let from = params.get("from").and_then(|v| v.as_str()).unwrap_or("0");
    let timeout = params
        .get("timeout")
        .and_then(|v| v.as_u64())
        .unwrap_or(30000);

    let result = state
        .services
        .sync_service
        .get_events(&user_id, from, timeout)
        .await
        .unwrap_or(json!({
            "start": from,
            "end": from,
            "chunk": []
        }));

    Ok(Json(result))
}

// ============================================================================
// SECTION: Room Management
// ============================================================================

async fn get_room_info(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let user_id = &auth_user.user_id;

    let membership = sqlx::query(
        r#"
        SELECT membership 
        FROM room_memberships 
        WHERE room_id = $1 AND user_id = $2
        "#,
    )
    .bind(&room_id)
    .bind(user_id)
    .fetch_optional(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to check room membership: {}", e)))?;

    let membership = match membership {
        Some(row) => row.get::<Option<String>, _>("membership"),
        None => None,
    };

    if membership.is_none() {
        return Err(ApiError::not_found(
            "Room not found or not a member".to_string(),
        ));
    }

    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    Ok(Json(json!({
        "room_id": room_id,
        "name": room.name,
        "avatar_url": room.avatar_url,
        "topic": room.topic,
        "canonical_alias": room.canonical_alias,
        "joined_members_count": room.member_count,
        "invited_members_count": 0,
        "world_readable": room.is_public,
        "guest_can_join": false,
        "membership": membership
    })))
}

async fn get_joined_rooms(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let user_id = &auth_user.user_id;

    let rooms = sqlx::query(
        r#"
        SELECT DISTINCT room_id 
        FROM room_memberships 
        WHERE user_id = $1 AND membership = 'join'
        ORDER BY room_id
        "#,
    )
    .bind(user_id)
    .fetch_all(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to get joined rooms: {}", e)))?;

    let room_ids: Vec<String> = rooms
        .iter()
        .filter_map(|row| row.get::<Option<String>, _>("room_id"))
        .collect();

    Ok(Json(json!({
        "joined_rooms": room_ids
    })))
}

// TD-API-01: 实现 my_rooms 端点 (前端需要)
// 返回用户的所有房间（包括 invited, joined, left 等状态）
async fn get_my_rooms(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let user_id = &auth_user.user_id;

    // 获取所有房间（包括 join, invite, leave 状态）
    let rooms = sqlx::query(
        r#"
        SELECT room_id, membership, 
               COALESCE(name, '') as name,
               COALESCE(avatar_url, '') as avatar_url,
               updated_ts
        FROM room_memberships rm
        LEFT JOIN rooms r ON rm.room_id = r.room_id
        WHERE rm.user_id = $1
        ORDER BY rm.updated_ts DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to get rooms: {}", e)))?;

    let mut room_list = Vec::new();
    for row in rooms.iter() {
        let membership: Option<String> = row.get("membership");
        let room_id: Option<String> = row.get("room_id");
        let name: Option<String> = row.get("name");

        if let (Some(m), Some(r_id)) = (membership, room_id) {
            room_list.push(json!({
                "room_id": r_id,
                "membership": m,
                "name": name.unwrap_or_default(),
                "avatar_url": row.get::<Option<String>, _>("avatar_url").unwrap_or_default()
            }));
        }
    }

    Ok(Json(json!({
        "rooms": room_list,
        "total": room_list.len()
    })))
}

async fn get_messages(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let from = params
        .get("from")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(10);
    let direction = params.get("dir").and_then(|v| v.as_str()).unwrap_or("b");

    Ok(Json(
        state
            .services
            .room_service
            .get_room_messages(&room_id, from as i64, limit as i64, direction)
            .await?,
    ))
}

async fn send_message(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_type, _txn_id)): Path<(String, String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    // Validate content length to prevent DoS
    let s = body.to_string();
    if s.len() > 65536 {
        return Err(ApiError::bad_request(
            "Message content too long (max 64KB)".to_string(),
        ));
    }

    Ok(Json(
        state
            .services
            .room_service
            .send_message(&room_id, &auth_user.user_id, &event_type, &body)
            .await?,
    ))
}

async fn join_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _) = state.services.auth_service.validate_token(&token).await?;

    state
        .services
        .room_service
        .join_room(&room_id, &user_id)
        .await?;
    Ok(Json(json!({})))
}

async fn join_room_by_id_or_alias(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id_or_alias): Path<String>,
    body: Option<Json<serde_json::Value>>,
) -> Result<Json<Value>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _) = state.services.auth_service.validate_token(&token).await?;

    let room_id = if room_id_or_alias.starts_with('!') {
        room_id_or_alias.clone()
    } else if room_id_or_alias.starts_with('#') {
        state
            .services
            .room_service
            .get_room_by_alias(&room_id_or_alias)
            .await
            .map_err(|e| ApiError::not_found(format!("Room alias not found: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Room ID not found for alias".to_string()))?
    } else {
        let alias = format!(
            "#{}:{}",
            room_id_or_alias, state.services.config.server.name
        );
        state
            .services
            .room_service
            .get_room_by_alias(&alias)
            .await
            .map_err(|e| ApiError::not_found(format!("Room alias not found: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Room ID not found for alias".to_string()))?
    };

    let via_servers = body
        .and_then(|b| b.get("via_servers").and_then(|v| v.as_array()).cloned())
        .unwrap_or_default();

    ::tracing::info!(
        "User {} joining room {} via {:?}",
        user_id,
        room_id,
        via_servers
    );

    state
        .services
        .room_service
        .join_room(&room_id, &user_id)
        .await?;

    Ok(Json(json!({
        "room_id": room_id
    })))
}

async fn leave_room(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    state
        .services
        .room_service
        .leave_room(&room_id, &auth_user.user_id)
        .await?;
    Ok(Json(json!({})))
}

#[derive(Debug, Deserialize)]
struct UpgradeRoomRequest {
    new_version: String,
}

#[derive(Debug, Serialize)]
struct UpgradeRoomResponse {
    replacement_room: String,
}

/// Upgrade Room (MSC2174)
/// POST /_matrix/client/v3/rooms/{room_id}/upgrade
async fn upgrade_room(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<UpgradeRoomRequest>,
) -> Result<Json<UpgradeRoomResponse>, ApiError> {
    validate_room_id(&room_id)?;

    let new_room_id = state
        .services
        .room_service
        .upgrade_room(&room_id, &body.new_version, &auth_user.user_id)
        .await?;

    Ok(Json(UpgradeRoomResponse {
        replacement_room: new_room_id,
    }))
}

async fn forget_room(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    state
        .services
        .room_service
        .forget_room(&room_id, &auth_user.user_id)
        .await?;
    Ok(Json(json!({})))
}

/// Room Initial Sync
/// GET /_matrix/client/r0/rooms/{room_id}/initialSync
/// GET /_matrix/client/v3/rooms/{room_id}/initialSync
///
/// Room Initial Sync
/// GET /_matrix/client/r0/rooms/{room_id}/initialSync
/// GET /_matrix/client/v3/rooms/{room_id}/initialSync
///
/// Returns initial sync data for a room including:
/// - Room state events
/// - Timeline messages  
/// - Presence events
/// - Member list
/// - Account data
async fn room_initial_sync(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    // 检查房间是否存在
    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Room not found"))?;

    // 检查用户是否是房间成员
    let member = state
        .services
        .member_storage
        .get_member(&room_id, &auth_user.user_id)
        .await?;

    if member.is_none() {
        return Err(ApiError::forbidden("You are not a member of this room"));
    }

    // 获取成员列表
    let members = state
        .services
        .member_storage
        .get_joined_members(&room_id)
        .await
        .unwrap_or_default();

    // 构建完整响应
    let mut response = json!({
        "room_id": room_id,
        "messages": {
            "chunk": [],
            "start": "s",
            "end": "e"
        },
        "state": [],
        "presence": [],
        "account_data": [],
        "members": members,
        "num_joined_members": members.len(),
    });

    // 添加房间元数据
    if let Some(name) = room.name {
        response["name"] = serde_json::Value::String(name);
    }
    if let Some(topic) = room.topic {
        response["topic"] = serde_json::Value::String(topic);
    }
    if let Some(avatar_url) = room.avatar_url {
        response["avatar_url"] = serde_json::Value::String(avatar_url);
    }

    // 添加创建信息
    response["created_by"] = serde_json::Value::String(room.creator_user_id.unwrap_or_default());
    response["created_ts"] = serde_json::Value::Number(serde_json::Number::from(room.created_ts));

    Ok(Json(response))
}

async fn get_room_members(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _) = state.services.auth_service.validate_token(&token).await?;

    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let room = room.ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    let is_member = state
        .services
        .member_storage
        .get_member(&room_id, &user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
        .is_some();

    if !room.is_public && !is_member {
        ::tracing::warn!(
            target: "security_audit",
            event = "unauthorized_room_members_access",
            user_id = user_id,
            room_id = room_id,
            "User attempted to access members of private room without being a member"
        );
        return Err(ApiError::forbidden(
            "You must be a member to view the member list of this private room".to_string(),
        ));
    }

    let members = state
        .services
        .room_service
        .get_room_members(&room_id, &user_id)
        .await?;
    Ok(Json(members))
}

async fn get_joined_members(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let room = room.ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    let is_member = state
        .services
        .member_storage
        .get_member(&room_id, &auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
        .is_some();

    if !room.is_public && !is_member {
        return Err(ApiError::forbidden(
            "You must be a member to view the joined members of this private room".to_string(),
        ));
    }

    let members = state
        .services
        .member_storage
        .get_joined_members(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get joined members: {}", e)))?;

    let joined: std::collections::HashMap<String, Value> = members
        .into_iter()
        .map(|m| {
            let user_id = m.user_id.clone();
            (
                user_id,
                json!({
                    "display_name": m.display_name,
                    "avatar_url": m.avatar_url
                }),
            )
        })
        .collect();

    Ok(Json(json!({
        "joined": joined
    })))
}

async fn invite_user(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let invitee = body
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("User ID required".to_string()))?;

    validate_user_id(invitee)?;

    state
        .services
        .room_service
        .invite_user(&room_id, &auth_user.user_id, invitee)
        .await?;
    Ok(Json(json!({})))
}

async fn knock_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id_or_alias): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _) = state.services.auth_service.validate_token(&token).await?;

    let room_id = if room_id_or_alias.starts_with('!') {
        room_id_or_alias.clone()
    } else if room_id_or_alias.starts_with('#') {
        state
            .services
            .room_service
            .get_room_by_alias(&room_id_or_alias)
            .await
            .map_err(|e| ApiError::not_found(format!("Room alias not found: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Room ID not found for alias".to_string()))?
    } else {
        let alias = format!(
            "#{}:{}",
            room_id_or_alias, state.services.config.server.name
        );
        state
            .services
            .room_service
            .get_room_by_alias(&alias)
            .await
            .map_err(|e| ApiError::not_found(format!("Room alias not found: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Room ID not found for alias".to_string()))?
    };

    let reason = body
        .get("reason")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    ::tracing::info!("User {} knocking on room {}", user_id, room_id);

    state
        .services
        .room_service
        .knock_room(&room_id, &user_id, reason.as_deref())
        .await?;

    Ok(Json(json!({
        "room_id": room_id
    })))
}

async fn invite_user_by_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _) = state.services.auth_service.validate_token(&token).await?;

    validate_room_id(&room_id)?;

    let invitee = body
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("User ID required".to_string()))?;

    validate_user_id(invitee)?;

    ::tracing::info!("User {} inviting {} to room {}", user_id, invitee, room_id);

    state
        .services
        .room_service
        .invite_user(&room_id, &user_id, invitee)
        .await?;
    Ok(Json(json!({})))
}

async fn create_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _) = state.services.auth_service.validate_token(&token).await?;

    let visibility = body.get("visibility").and_then(|v| v.as_str());
    if let Some(v) = visibility {
        if v != "public" && v != "private" {
            return Err(ApiError::bad_request(
                "Visibility must be 'public' or 'private'".to_string(),
            ));
        }
    }

    let room_alias = body.get("room_alias_name").and_then(|v| v.as_str());
    if let Some(alias) = room_alias {
        if alias.len() > 255 {
            return Err(ApiError::bad_request(
                "Room alias name too long".to_string(),
            ));
        }
        // Validate alias format (localpart only, usually)
        // But spec says room_alias_name is the local part.
        // Let's rely on basic char check if needed, but length is most important for DoS.
        if !alias
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.')
        {
            return Err(ApiError::bad_request(
                "Invalid characters in room alias name".to_string(),
            ));
        }
    }

    let name = body.get("name").and_then(|v| v.as_str());
    if let Some(n) = name {
        if n.len() > 255 {
            return Err(ApiError::bad_request("Room name too long".to_string()));
        }
    }

    let topic = body.get("topic").and_then(|v| v.as_str());
    if let Some(t) = topic {
        if t.len() > 4096 {
            return Err(ApiError::bad_request("Room topic too long".to_string()));
        }
    }

    let invite = body.get("invite").and_then(|v| v.as_array()).map(|arr| {
        arr.iter()
            .filter_map(|x| x.as_str().map(String::from))
            .collect::<Vec<String>>()
    });

    if let Some(ref inv) = invite {
        if inv.len() > 100 {
            return Err(ApiError::bad_request(
                "Too many invites (max 100)".to_string(),
            ));
        }
    }

    let preset = body.get("preset").and_then(|v| v.as_str());

    let config = CreateRoomConfig {
        visibility: visibility.map(|s| s.to_string()),
        room_alias_name: room_alias.map(|s| s.to_string()),
        name: name.map(|s| s.to_string()),
        topic: topic.map(|s| s.to_string()),
        invite_list: invite,
        preset: preset.map(|s| s.to_string()),
        ..Default::default()
    };

    let result = state
        .services
        .room_service
        .create_room(&user_id, config)
        .await?;

    // Create room summary
    let room_id = result.get("room_id").and_then(|r| r.as_str()).unwrap_or("");
    if !room_id.is_empty() {
        let _ = state
            .services
            .room_summary_storage
            .create_summary(crate::storage::room_summary::CreateRoomSummaryRequest {
                room_id: room_id.to_string(),
                room_type: None,
                name: name.map(|s| s.to_string()),
                topic: topic.map(|s| s.to_string()),
                avatar_url: None,
                canonical_alias: None,
                join_rule: None,
                history_visibility: None,
                guest_access: None,
                is_direct: None,
                is_space: None,
            })
            .await;
    }

    Ok(Json(result))
}

#[axum::debug_handler]
async fn get_room_visibility(
    State(state): State<AppState>,
    _auth_user: OptionalAuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let visibility = state
        .services
        .room_service
        .get_room_visibility(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room visibility: {}", e)))?;

    Ok(Json(json!({
        "visibility": visibility
    })))
}

#[axum::debug_handler]
async fn set_room_visibility(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let visibility = body
        .get("visibility")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing visibility field".to_string()))?;

    if visibility != "public" && visibility != "private" {
        return Err(ApiError::bad_request(
            "visibility must be 'public' or 'private'".to_string(),
        ));
    }

    let is_public = visibility == "public";

    if is_public {
        let is_creator = state
            .services
            .room_service
            .is_room_creator(&room_id, &auth_user.user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check room creator: {}", e)))?;

        if !auth_user.is_admin && !is_creator {
            return Err(ApiError::forbidden(
                "Only room creator or server admin can set room to public".to_string(),
            ));
        }
    }

    state
        .services
        .room_service
        .set_room_directory(&room_id, is_public)
        .await?;

    Ok(Json(json!({})))
}

async fn get_room_aliases(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let aliases = state
        .services
        .room_service
        .get_room_aliases(&room_id)
        .await?;
    Ok(Json(json!({ "aliases": aliases })))
}

async fn set_room_alias(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, room_alias)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    if room_alias.len() > 255 {
        return Err(ApiError::bad_request(
            "Alias too long (max 255 characters)".to_string(),
        ));
    }

    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let is_member = state
        .services
        .member_storage
        .is_member(&room_id, &auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check membership: {}", e)))?;

    if !is_member && !auth_user.is_admin {
        return Err(ApiError::forbidden(
            "You must be a room member to set an alias".to_string(),
        ));
    }

    state
        .services
        .room_service
        .set_room_alias(&room_id, &room_alias, &auth_user.user_id)
        .await?;
    Ok(Json(json!({})))
}

async fn delete_room_alias(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path((room_id, _room_alias)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    state
        .services
        .room_service
        .remove_room_alias(&room_id)
        .await?;
    Ok(Json(json!({})))
}

async fn get_room_by_alias(
    State(state): State<AppState>,
    _auth_user: OptionalAuthenticatedUser,
    Path(room_alias): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let room_id = state
        .services
        .room_service
        .get_room_by_alias(&room_alias)
        .await?;
    match room_id {
        Some(rid) => Ok(Json(json!({ "room_id": rid }))),
        None => Err(ApiError::not_found("Room alias not found".to_string())),
    }
}

#[axum::debug_handler]
async fn set_room_alias_direct(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_alias): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let room_id = body
        .get("room_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing room_id field".to_string()))?;

    if !room_id.starts_with('!') {
        return Err(ApiError::bad_request("Invalid room_id format".to_string()));
    }

    if room_alias.len() > 255 {
        return Err(ApiError::bad_request(
            "Alias too long (max 255 characters)".to_string(),
        ));
    }

    if !state
        .services
        .room_storage
        .room_exists(room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let is_member = state
        .services
        .member_storage
        .is_member(room_id, &auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check membership: {}", e)))?;

    if !is_member && !auth_user.is_admin {
        return Err(ApiError::forbidden(
            "You must be a room member to set an alias".to_string(),
        ));
    }

    state
        .services
        .room_service
        .set_room_alias(room_id, &room_alias, &auth_user.user_id)
        .await?;
    Ok(Json(json!({})))
}

#[axum::debug_handler]
async fn delete_room_alias_direct(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(room_alias): Path<String>,
) -> Result<Json<Value>, ApiError> {
    state
        .services
        .room_service
        .remove_room_alias_by_name(&room_alias)
        .await?;
    Ok(Json(json!({})))
}

async fn get_public_rooms(
    State(state): State<AppState>,
    _auth_user: OptionalAuthenticatedUser,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(10);
    let _since = params.get("since").and_then(|v| v.as_str());

    Ok(Json(
        state
            .services
            .room_service
            .get_public_rooms(limit as i64)
            .await?,
    ))
}

#[axum::debug_handler]
async fn query_public_rooms(
    State(state): State<AppState>,
    _auth_user: OptionalAuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let limit = body.get("limit").and_then(|v| v.as_u64()).unwrap_or(10);
    let _since = body.get("since").and_then(|v| v.as_str());
    let _filter = body.get("filter");

    Ok(Json(
        state
            .services
            .room_service
            .get_public_rooms(limit as i64)
            .await?,
    ))
}

async fn get_user_rooms(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if auth_user.user_id != user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let rooms = state
        .services
        .room_service
        .get_joined_rooms(&user_id)
        .await?;

    Ok(Json(json!({
        "joined_rooms": rooms
    })))
}

async fn get_presence(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;

    let user_exists = state
        .services
        .user_storage
        .user_exists(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check user existence: {}", e)))?;

    if !user_exists {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    let presence = state
        .services
        .presence_storage
        .get_presence(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get presence: {}", e)))?;

    match presence {
        Some((presence, status_msg)) => Ok(Json(json!({
            "presence": presence,
            "status_msg": status_msg
        }))),
        _ => Ok(Json(json!({
            "presence": "offline",
            "status_msg": Option::<String>::None
        }))),
    }
}

async fn set_presence(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;

    if auth_user.user_id != user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let presence = body
        .get("presence")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Presence required".to_string()))?;

    validate_presence_status(presence)?;

    let status_msg = body.get("status_msg").and_then(|v| v.as_str());

    if let Some(msg) = status_msg {
        if msg.len() > MAX_MESSAGE_LENGTH {
            return Err(ApiError::bad_request(format!(
                "Status message too long (max {} characters)",
                MAX_MESSAGE_LENGTH
            )));
        }
    }

    state
        .services
        .presence_storage
        .set_presence(&user_id, presence, status_msg)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to set presence: {}", e)))?;

    Ok(Json(json!({})))
}

/// Presence list endpoint (MSC2776)
/// Subscribe/unsubscribe to presence status updates for a list of users
async fn presence_list(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let user_id = &auth_user.user_id;

    // Handle subscriptions (subscribe to users' presence)
    if let Some(subscribe) = body.get("subscribe").and_then(|v| v.as_array()) {
        for target in subscribe {
            if let Some(target_id) = target.as_str() {
                validate_user_id(target_id)?;

                // Add subscription
                if let Err(e) = state
                    .services
                    .presence_storage
                    .add_subscription(user_id, target_id)
                    .await
                {
                    ::tracing::warn!("Failed to add presence subscription: {}", e);
                }
            }
        }
    }

    // Handle unsubscriptions (unsubscribe from users' presence)
    if let Some(unsubscribe) = body.get("unsubscribe").and_then(|v| v.as_array()) {
        for target in unsubscribe {
            if let Some(target_id) = target.as_str() {
                validate_user_id(target_id)?;

                // Remove subscription
                if let Err(e) = state
                    .services
                    .presence_storage
                    .remove_subscription(user_id, target_id)
                    .await
                {
                    ::tracing::warn!("Failed to remove presence subscription: {}", e);
                }
            }
        }
    }

    // Get current subscriptions and their presence
    let subscriptions = state
        .services
        .presence_storage
        .get_subscriptions(user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get subscriptions: {}", e)))?;

    // Batch fetch presence for all subscribed users
    let presence_batch = state
        .services
        .presence_storage
        .get_presence_batch(&subscriptions)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get presence batch: {}", e)))?;

    // Build response - presence_batch already contains all the presence info we need
    let mut presences = Vec::new();

    for (target_id, presence, status_msg) in presence_batch {
        // Calculate last_active_ago - we don't have exact timestamp but use presence update time
        let last_active_ago = if presence != "offline" {
            Some(0) // Simplified: indicates user is currently active
        } else {
            None
        };

        presences.push(json!({
            "user_id": target_id,
            "presence": presence,
            "status_msg": status_msg,
            "last_active_ago": last_active_ago
        }));
    }

    // Also include any users that were just subscribed but not in the database yet
    for target_id in &subscriptions {
        if !presences.iter().any(|p| p["user_id"] == *target_id) {
            presences.push(json!({
                "user_id": target_id,
                "presence": "offline",
                "status_msg": None::<String>,
                "last_active_ago": None::<i64>
            }));
        }
    }

    Ok(Json(json!({
        "presences": presences
    })))
}

async fn get_room_state(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let room_exists = state
        .services
        .room_service
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?;

    if !room_exists {
        return Err(ApiError::not_found(format!("Room '{}' not found", room_id)));
    }

    let events = state
        .services
        .event_storage
        .get_state_events(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get state: {}", e)))?;
    let state_events: Vec<Value> = events
        .iter()
        .map(|e| {
            json!({
                "type": e.event_type,
                "event_id": e.event_id,
                "sender": e.user_id,
                "content": e.content,
                "state_key": e.state_key
            })
        })
        .collect();

    // Return array directly per Matrix spec
    Ok(Json(JsonValue::Array(state_events)))
}

async fn get_state_by_type(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path((room_id, event_type)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let room_exists = state
        .services
        .room_service
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?;

    if !room_exists {
        return Err(ApiError::not_found(format!("Room '{}' not found", room_id)));
    }

    let final_event_type = if event_type.starts_with("m.room.") || event_type.starts_with("m.") {
        event_type.clone()
    } else {
        format!("m.room.{}", event_type)
    };

    let events = state
        .services
        .event_storage
        .get_state_events_by_type(&room_id, &final_event_type)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get state: {}", e)))?;

    let state_events: Vec<Value> = events
        .iter()
        .map(|e| {
            json!({
                "type": e.event_type,
                "event_id": e.event_id,
                "sender": e.user_id,
                "content": e.content,
                "state_key": e.state_key
            })
        })
        .collect();

    Ok(Json(json!({ "events": state_events })))
}

async fn get_state_event(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path((room_id, event_type, state_key)): Path<(String, String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let final_event_type = if event_type.starts_with("m.room.") || event_type.starts_with("m.") {
        event_type.clone()
    } else {
        format!("m.room.{}", event_type)
    };

    let events = state
        .services
        .event_storage
        .get_state_events_by_type(&room_id, &final_event_type)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get state: {}", e)))?;

    let event = events
        .iter()
        .find(|e| {
            e.state_key.as_deref() == Some(state_key.as_str())
                || (e.state_key.as_ref().map(|s| s.is_empty()) == Some(true)
                    && state_key.is_empty())
        })
        .ok_or_else(|| ApiError::not_found("State event not found".to_string()))?;

    let mut response = json!({
        "type": event.event_type,
        "event_id": event.event_id,
        "sender": event.sender,
        "state_key": event.state_key
    });

    if let Some(content) = event.content.as_object() {
        for (k, v) in content {
            response[k] = v.clone();
        }
    }

    Ok(Json(response))
}

async fn send_state_event(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_type)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let content = body;

    let new_event_id = crate::common::crypto::generate_event_id(&state.services.server_name);
    let now = chrono::Utc::now().timestamp_millis();

    let final_event_type = if event_type.starts_with("m.room.") || event_type.starts_with("m.") {
        event_type.clone()
    } else {
        format!("m.room.{}", event_type)
    };

    let state_event = state
        .services
        .event_storage
        .create_event(
            CreateEventParams {
                event_id: new_event_id.clone(),
                room_id: room_id.clone(),
                user_id: auth_user.user_id.clone(),
                event_type: final_event_type.clone(),
                content,
                state_key: Some(auth_user.user_id.clone()),
                origin_server_ts: now,
            },
            None,
        )
        .await
        .map_err(|e| ApiError::internal(format!("Failed to send state event: {}", e)))?;

    Ok(Json(json!({
        "event_id": new_event_id,
        "type": state_event.event_type,
        "state_key": state_event.state_key
    })))
}

async fn put_state_event(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_type, state_key)): Path<(String, String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let new_event_id = crate::common::crypto::generate_event_id(&state.services.server_name);
    let now = chrono::Utc::now().timestamp_millis();

    let final_event_type = if event_type.starts_with("m.room.") || event_type.starts_with("m.") {
        event_type.clone()
    } else {
        format!("m.room.{}", event_type)
    };

    let event = state
        .services
        .event_storage
        .create_event(
            CreateEventParams {
                event_id: new_event_id.clone(),
                room_id: room_id.clone(),
                user_id: auth_user.user_id.clone(),
                event_type: final_event_type.clone(),
                content: body,
                state_key: Some(state_key),
                origin_server_ts: now,
            },
            None,
        )
        .await
        .map_err(|e| ApiError::internal(format!("Failed to put state event: {}", e)))?;

    Ok(Json(json!({
        "event_id": new_event_id,
        "type": event.event_type,
        "state_key": event.state_key
    })))
}

async fn get_state_event_empty_key(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path((room_id, event_type)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let final_event_type = if event_type.starts_with("m.room.") || event_type.starts_with("m.") {
        event_type.clone()
    } else {
        format!("m.room.{}", event_type)
    };

    let events = state
        .services
        .event_storage
        .get_state_events_by_type(&room_id, &final_event_type)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get state: {}", e)))?;

    let event = events
        .iter()
        .find(|e| e.state_key.as_ref().map(|s| s.is_empty()) == Some(true))
        .ok_or_else(|| ApiError::not_found("State event not found".to_string()))?;

    let mut response = json!({
        "type": event.event_type,
        "event_id": event.event_id,
        "sender": event.sender,
        "state_key": event.state_key
    });

    if let Some(content) = event.content.as_object() {
        for (k, v) in content {
            response[k] = v.clone();
        }
    }

    Ok(Json(response))
}

async fn get_power_levels(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let events = state
        .services
        .event_storage
        .get_state_events_by_type(&room_id, "m.room.power_levels")
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get power levels: {}", e)))?;

    let event = events
        .iter()
        .find(|e| e.state_key.as_ref().map(|s| s.is_empty()) == Some(true))
        .ok_or_else(|| ApiError::not_found("Power levels not found".to_string()))?;

    let power_levels_content = event.content.clone();

    Ok(Json(power_levels_content))
}

async fn put_state_event_empty_key(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_type)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let new_event_id = crate::common::crypto::generate_event_id(&state.services.server_name);
    let now = chrono::Utc::now().timestamp_millis();

    let final_event_type = if event_type.starts_with("m.room.") || event_type.starts_with("m.") {
        event_type.clone()
    } else {
        format!("m.room.{}", event_type)
    };

    let event = state
        .services
        .event_storage
        .create_event(
            CreateEventParams {
                event_id: new_event_id.clone(),
                room_id: room_id.clone(),
                user_id: auth_user.user_id.clone(),
                event_type: final_event_type.clone(),
                content: body,
                state_key: Some("".to_string()),
                origin_server_ts: now,
            },
            None,
        )
        .await
        .map_err(|e| ApiError::internal(format!("Failed to put state event: {}", e)))?;

    Ok(Json(json!({
        "event_id": new_event_id,
        "type": event.event_type,
        "state_key": event.state_key
    })))
}

async fn put_state_event_no_key(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_type)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let new_event_id = crate::common::crypto::generate_event_id(&state.services.server_name);
    let now = chrono::Utc::now().timestamp_millis();

    let final_event_type = if event_type.starts_with("m.room.") || event_type.starts_with("m.") {
        event_type.clone()
    } else {
        format!("m.room.{}", event_type)
    };

    let event = state
        .services
        .event_storage
        .create_event(
            CreateEventParams {
                event_id: new_event_id.clone(),
                room_id: room_id.clone(),
                user_id: auth_user.user_id.clone(),
                event_type: final_event_type,
                content: body,
                state_key: Some("".to_string()),
                origin_server_ts: now,
            },
            None,
        )
        .await
        .map_err(|e| ApiError::internal(format!("Failed to put state event: {}", e)))?;

    Ok(Json(json!({
        "event_id": new_event_id,
        "type": event.event_type,
        "state_key": event.state_key
    })))
}

// ============================================================================
// SECTION: Receipts & Read Markers
// ============================================================================

async fn send_receipt(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, receipt_type, event_id)): Path<(String, String, String)>,
    Json(_body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;
    validate_receipt_type(&receipt_type)?;

    let event = state
        .services
        .event_storage
        .get_event(&event_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get event: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Event not found".to_string()))?;

    state
        .services
        .room_storage
        .add_receipt(
            &auth_user.user_id,
            &event.user_id,
            &room_id,
            &event_id,
            &receipt_type,
        )
        .await
        .map_err(|e| ApiError::internal(format!("Failed to store receipt: {}", e)))?;

    Ok(Json(json!({})))
}

async fn set_read_markers(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let is_member = state
        .services
        .member_storage
        .is_member(&room_id, &auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check membership: {}", e)))?;

    if !is_member {
        return Err(ApiError::forbidden(
            "You are not a member of this room".to_string(),
        ));
    }

    // Handle m.fully_read (public read marker)
    if let Some(event_id) = body.get("m.fully_read").and_then(|v| v.as_str()) {
        validate_event_id(event_id)?;
        state
            .services
            .room_storage
            .update_read_marker_with_type(&room_id, &auth_user.user_id, event_id, "m.fully_read")
            .await
            .map_err(|e| ApiError::internal(format!("Failed to set fully_read marker: {}", e)))?;
    }

    // Handle m.private_read (private read marker - MSC2654)
    if let Some(event_id) = body.get("m.private_read").and_then(|v| v.as_str()) {
        validate_event_id(event_id)?;
        state
            .services
            .room_storage
            .update_read_marker_with_type(&room_id, &auth_user.user_id, event_id, "m.private_read")
            .await
            .map_err(|e| ApiError::internal(format!("Failed to set private_read marker: {}", e)))?;
    }

    // Handle m.marked_unread (mark as unread - MSC2654)
    if let Some(marked_unread) = body.get("m.marked_unread").and_then(|v| v.as_object()) {
        if let Some(events) = marked_unread.get("events").and_then(|v| v.as_array()) {
            for event in events {
                if let Some(event_id) = event.as_str() {
                    validate_event_id(event_id)?;
                    state
                        .services
                        .room_storage
                        .update_read_marker_with_type(
                            &room_id,
                            &auth_user.user_id,
                            event_id,
                            "m.marked_unread",
                        )
                        .await
                        .map_err(|e| {
                            ApiError::internal(format!("Failed to set marked_unread marker: {}", e))
                        })?;
                }
            }
        }
    }

    // Handle legacy m.read (treated as m.fully_read)
    if let Some(event_id) = body.get("m.read").and_then(|v| v.as_str()) {
        validate_event_id(event_id)?;
        state
            .services
            .room_storage
            .update_read_marker_with_type(&room_id, &auth_user.user_id, event_id, "m.fully_read")
            .await
            .map_err(|e| ApiError::internal(format!("Failed to set read marker: {}", e)))?;
    }

    Ok(Json(json!({})))
}
async fn get_membership_events(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let limit = body.get("limit").and_then(|v| v.as_u64()).unwrap_or(100) as i64;

    let memberships = state
        .services
        .member_storage
        .get_membership_history(&room_id, limit)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get membership events: {}", e)))?;

    let events: Vec<Value> = memberships
        .into_iter()
        .map(|m| {
            json!({
                "event_id": m.event_id,
                "type": m.event_type,
                "sender": m.sender,
                "state_key": m.user_id,
                "content": {
                    "membership": m.membership
                },
                "origin_server_ts": m.joined_ts
            })
        })
        .collect();

    Ok(Json(json!({
        "events": events
    })))
}

async fn redact_event(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id, _txn_id)): Path<(String, String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;

    let original_event = state
        .services
        .event_storage
        .get_event(&event_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get event: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Event not found".to_string()))?;

    if original_event.room_id != room_id {
        return Err(ApiError::bad_request(
            "Event does not belong to this room".to_string(),
        ));
    }

    state
        .services
        .auth_service
        .can_redact_event(
            &room_id,
            &auth_user.user_id,
            &original_event.user_id,
            auth_user.is_admin,
        )
        .await?;

    let reason = body.get("reason").and_then(|v| v.as_str());

    let new_event_id = crate::common::crypto::generate_event_id(&state.services.server_name);
    let now = chrono::Utc::now().timestamp_millis();

    let content = json!({
        "reason": reason
    });

    state
        .services
        .event_storage
        .create_event(
            CreateEventParams {
                event_id: new_event_id.clone(),
                room_id: room_id.clone(),
                user_id: auth_user.user_id,
                event_type: "m.room.redaction".to_string(),
                content,
                state_key: None,
                origin_server_ts: now,
            },
            None,
        )
        .await
        .map_err(|e| ApiError::internal(format!("Failed to redact event: {}", e)))?;

    Ok(Json(json!({
        "event_id": new_event_id
    })))
}

async fn kick_user(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let target = body
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("User ID required".to_string()))?;

    validate_user_id(target)?;

    let reason = body.get("reason").and_then(|v| v.as_str());
    if let Some(r) = reason {
        if r.len() > 512 {
            return Err(ApiError::bad_request("Reason too long".to_string()));
        }
    }

    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    if !state
        .services
        .user_storage
        .user_exists(target)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check user existence: {}", e)))?
    {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    state
        .services
        .auth_service
        .can_kick_user(&room_id, &auth_user.user_id, target, auth_user.is_admin)
        .await?;

    let event_id = crate::common::crypto::generate_event_id(&state.services.server_name);
    let content = json!({
        "membership": "leave",
        "reason": reason.unwrap_or("")
    });

    state
        .services
        .member_storage
        .remove_member(&room_id, target)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to kick user: {}", e)))?;

    state
        .services
        .event_storage
        .create_event(
            CreateEventParams {
                event_id,
                room_id: room_id.clone(),
                user_id: auth_user.user_id,
                event_type: "m.room.member".to_string(),
                content,
                state_key: Some(target.to_string()),
                origin_server_ts: chrono::Utc::now().timestamp_millis(),
            },
            None,
        )
        .await
        .map_err(|e| {
            ::tracing::warn!(
                "Failed to create membership event for room {}: {}",
                room_id,
                e
            );
        })
        .ok();

    Ok(Json(json!({})))
}

async fn ban_user(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let target = body
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("User ID required".to_string()))?;

    validate_user_id(target)?;

    let reason = body.get("reason").and_then(|v| v.as_str());
    if let Some(r) = reason {
        if r.len() > 512 {
            return Err(ApiError::bad_request("Reason too long".to_string()));
        }
    }

    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    if !state
        .services
        .user_storage
        .user_exists(target)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check user existence: {}", e)))?
    {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    state
        .services
        .auth_service
        .can_ban_user(&room_id, &auth_user.user_id, target, auth_user.is_admin)
        .await?;

    let event_id = crate::common::crypto::generate_event_id(&state.services.server_name);
    let content = json!({
        "membership": "ban",
        "reason": reason.unwrap_or("")
    });

    state
        .services
        .member_storage
        .add_member(&room_id, target, "ban", None, None, None)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to ban user: {}", e)))?;

    state
        .services
        .event_storage
        .create_event(
            CreateEventParams {
                event_id,
                room_id: room_id.clone(),
                user_id: auth_user.user_id,
                event_type: "m.room.member".to_string(),
                content,
                state_key: Some(target.to_string()),
                origin_server_ts: chrono::Utc::now().timestamp_millis(),
            },
            None,
        )
        .await
        .map_err(|e| {
            ::tracing::warn!(
                "Failed to create membership event for room {}: {}",
                room_id,
                e
            );
        })
        .ok();

    Ok(Json(json!({})))
}

async fn unban_user(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let target = body
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("User ID required".to_string()))?;

    validate_user_id(target)?;

    state
        .services
        .auth_service
        .verify_room_moderator(&room_id, &auth_user.user_id, auth_user.is_admin)
        .await?;

    state
        .services
        .member_storage
        .remove_member(&room_id, target)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to unban user: {}", e)))?;

    let event_id = crate::common::crypto::generate_event_id(&state.services.server_name);
    let content = json!({
        "membership": "leave"
    });

    state
        .services
        .event_storage
        .create_event(
            CreateEventParams {
                event_id,
                room_id: room_id.clone(),
                user_id: auth_user.user_id,
                event_type: "m.room.member".to_string(),
                content,
                state_key: Some(target.to_string()),
                origin_server_ts: chrono::Utc::now().timestamp_millis(),
            },
            None,
        )
        .await
        .map_err(|e| {
            ::tracing::warn!(
                "Failed to create membership event for room {}: {}",
                room_id,
                e
            );
        })
        .ok();

    Ok(Json(json!({})))
}
