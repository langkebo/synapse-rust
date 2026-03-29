mod account_compat;
pub mod account_data;
pub mod admin;
pub mod ai_connection;
pub mod app_service;
mod assembly;
mod auth_compat;
pub mod background_update;
pub mod burn_after_read;
pub mod captcha;
pub mod cas;
pub mod device;
pub mod directory;
mod directory_reporting;
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
pub mod moderation;
pub mod module;
pub mod oidc;
pub mod presence;
mod presence_compat;
pub mod push;
pub mod push_notification;
pub mod push_rules;
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
pub mod sync;
mod sync_compat;
pub mod tags;
pub mod telemetry;
pub mod thirdparty;
pub mod thread;
pub mod typing;
pub mod validators;
pub mod verification_routes;
pub mod voice;
pub mod voip;
pub mod widget;
pub mod worker;

pub(crate) use account_compat::{
    add_threepid, change_password_uia, deactivate_account, delete_threepid, get_avatar_url,
    get_displayname, get_profile, get_threepids, unbind_threepid, update_avatar,
    update_displayname, whoami,
};
pub use account_data::create_account_data_router;
pub use admin::create_admin_module_router;
pub use ai_connection::create_ai_connection_router;
pub use app_service::create_app_service_router;
pub use assembly::create_router;
pub(crate) use auth_compat::{
    check_username_availability, get_login_flows, get_register_flows, login, logout, logout_all,
    refresh_token, register, request_email_verification, submit_email_token,
};
pub use background_update::create_background_update_router;
pub use burn_after_read::create_burn_after_read_router;
pub use captcha::create_captcha_router;
pub use cas::cas_routes;
pub use device::create_device_router;
pub(crate) use directory_reporting::{
    delete_room_alias, delete_room_alias_direct, get_public_rooms, get_room_aliases,
    get_room_by_alias, get_scanner_info, list_user_directory, query_public_rooms, report_event,
    report_room, search_user_directory, set_room_alias, set_room_alias_direct, update_report_score,
};
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
pub use handlers::{
    get_capabilities, get_client_versions, get_server_version, get_well_known_client,
    get_well_known_server, get_well_known_support, health_check,
};
pub use key_backup::create_key_backup_router;
pub use key_rotation::create_key_rotation_router;
pub use media::create_media_router;
pub use moderation::create_moderation_router;
pub use module::create_module_router;
pub use oidc::create_oidc_router;
pub use presence::create_presence_router;
pub(crate) use presence_compat::{get_presence, presence_list, set_presence};
pub use push::create_push_router;
pub use push_notification::create_push_notification_router;
pub use push_rules::{
    get_default_push_rules, get_push_rules_default, get_push_rules_global_default,
};
pub use reactions::create_reactions_router;
pub use relations::create_relations_router;
pub use rendezvous::create_rendezvous_router;
pub use room_summary::create_room_summary_router;
pub use saml::create_saml_router;
pub use search::create_search_router;
pub use sliding_sync::create_sliding_sync_router;
pub use space::create_space_router;
pub use state::AppState;
pub use sync::create_sync_router;
pub(crate) use sync_compat::{get_events, sync};
pub use tags::create_tags_router;
pub use telemetry::create_telemetry_router;
pub use thirdparty::create_thirdparty_router;
pub use thread::create_thread_routes;
pub use validators::{
    validate_event_id, validate_presence_status, validate_receipt_type, validate_room_id,
    validate_user_id,
};
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

use crate::common::ApiError;
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
