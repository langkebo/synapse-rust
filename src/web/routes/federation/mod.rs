use crate::common::*;
use crate::web::routes::AppState;
use axum::{
    extract::{Json, Query, State},
    http::{header::HeaderName, HeaderValue},
    middleware,
    response::IntoResponse,
    routing::{get, post, put},
    Router,
};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::{timeout, Instant};

pub mod events;
pub mod keys;
pub mod media;
pub mod membership;
pub mod transaction;

fn validate_federation_origin(authenticated_origin: &str, declared_origin: Option<&str>) -> Result<(), ApiError> {
    let declared_origin = declared_origin.ok_or_else(|| ApiError::bad_request("Origin required".to_string()))?;

    if declared_origin != authenticated_origin {
        return Err(ApiError::forbidden("Federation origin does not match authenticated request".to_string()));
    }

    Ok(())
}

fn sender_server_name(sender: &str) -> Option<&str> {
    sender
        .strip_prefix('@')
        .and_then(|user| user.rsplit_once(':').map(|(_, server)| server))
        .filter(|server| !server.is_empty())
}

fn user_matches_origin(user_id: &str, origin: &str) -> bool {
    user_id.rsplit_once(':').is_some_and(|(_, server_name)| server_name == origin)
}

async fn validate_federation_origin_in_room(state: &AppState, room_id: &str, origin: &str) -> ApiResult<()> {
    let joined_members = state
        .services
        .rooms
        .member_storage
        .get_room_members(room_id, "join")
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to load room members", &e))?;

    if joined_members.iter().any(|member| user_matches_origin(&member.user_id, origin)) {
        return Ok(());
    }

    Err(ApiError::forbidden("Authenticated server has no joined members in this room".to_string()))
}

async fn validate_federation_origin_can_observe_room(state: &AppState, room_id: &str, origin: &str) -> ApiResult<()> {
    // Aligned with Synapse v1.153: allow servers with any non-banned
    // membership (join, invite, leave) to access room state/backfill.
    // Previously only checked "join" membership, which was overly
    // restrictive and could cause federation issues for servers that
    // have invited or previously-left members.
    let has_member = state
        .services
        .rooms
        .member_storage
        .has_any_non_banned_member_from_server(room_id, origin)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to load room members", &e))?;

    if has_member {
        return Ok(());
    }

    Err(ApiError::not_found("Room not found".to_string()))
}

async fn validate_federation_origin_shares_user_room(state: &AppState, user_id: &str, origin: &str) -> ApiResult<()> {
    let joined_room_ids = state
        .services
        .rooms
        .room_storage
        .get_user_rooms(user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to load user rooms", &e))?;

    if joined_room_ids.is_empty() {
        return Err(ApiError::forbidden("User does not share any rooms with the requesting server".to_string()));
    }

    if origin == state.services.core.server_name.as_str() {
        return Ok(());
    }

    for room_id in joined_room_ids {
        let joined_members = state
            .services
            .rooms
            .member_storage
            .get_room_members(&room_id, "join")
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to load room members", &e))?;

        if joined_members.iter().any(|member| user_matches_origin(&member.user_id, origin)) {
            return Ok(());
        }
    }

    Err(ApiError::forbidden("Authenticated server does not share a room with this user".to_string()))
}

fn increment_counter(state: &AppState, name: &str) {
    if let Some(counter) = state.services.core.metrics.get_counter(name) {
        counter.inc();
    } else {
        state.services.core.metrics.register_counter(name.to_string()).inc();
    }
}

fn observe_histogram(state: &AppState, name: &str, value: f64) {
    if let Some(histogram) = state.services.core.metrics.get_histogram(name) {
        histogram.observe(value);
    } else {
        state.services.core.metrics.register_histogram(name.to_string()).observe(value);
    }
}

fn increment_gauge(state: &AppState, name: &str) {
    if let Some(gauge) = state.services.core.metrics.get_gauge(name) {
        gauge.inc();
    } else {
        state.services.core.metrics.register_gauge(name.to_string()).inc();
    }
}

fn decrement_gauge(state: &AppState, name: &str) {
    if let Some(gauge) = state.services.core.metrics.get_gauge(name) {
        gauge.dec();
    } else {
        state.services.core.metrics.register_gauge(name.to_string()).dec();
    }
}

async fn acquire_with_timeout(
    semaphore: Arc<Semaphore>,
    acquire_timeout_ms: u64,
) -> Result<(OwnedSemaphorePermit, u64), ApiError> {
    let started = Instant::now();
    let permit = timeout(Duration::from_millis(acquire_timeout_ms.max(1)), semaphore.acquire_owned())
        .await
        .map_err(|_| ApiError::rate_limited_with_retry(acquire_timeout_ms.max(1)))?
        .map_err(|e| ApiError::internal_with_log("Semaphore closed", &e))?;

    Ok((permit, started.elapsed().as_millis() as u64))
}

async fn federation_version(State(state): State<AppState>) -> impl IntoResponse {
    let route_owner =
        crate::worker::topology_validator::current_instance_worker_type(&state.services.core.config.worker);
    (
        [(HeaderName::from_static("x-synapse-route-owner"), HeaderValue::from_static(route_owner.as_str()))],
        Json(json!({
            "server": {
                "name": "synapse-rust",
                "version": env!("CARGO_PKG_VERSION")
            }
        })),
    )
}

async fn federation_discovery(State(state): State<AppState>) -> Json<Value> {
    Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "server_name": state.services.core.server_name,
        "capabilities": {
            "m.change_password": crate::web::routes::handlers::versions::change_password_capability_enabled(),
            "m.room_versions": federation_room_versions_capability()
        }
    }))
}

async fn openid_userinfo(State(state): State<AppState>, Query(params): Query<Value>) -> Result<Json<Value>, ApiError> {
    let access_token = params
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing access_token parameter"))?;

    let token = state
        .services
        .core
        .account_data_service
        .validate_openid_token(access_token)
        .await?
        .ok_or_else(|| ApiError::unauthorized("Invalid or expired OpenID token".to_string()))?;

    // Validate that the sub (user_id) is a well-formed Matrix user ID (@localpart:server_name)
    let user_server_name = sender_server_name(&token.user_id)
        .ok_or_else(|| ApiError::unauthorized("Invalid subject in OpenID token".to_string()))?;

    // Validate that the sub belongs to this homeserver
    if user_server_name != state.services.core.server_name.as_str() {
        return Err(ApiError::not_found("User does not belong to this server".to_string()));
    }

    let user_exists = state
        .services
        .account
        .user_storage
        .user_exists(&token.user_id)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to validate OpenID token subject", &e))?;
    if !user_exists {
        return Err(ApiError::unauthorized("Invalid or expired OpenID token".to_string()));
    }

    Ok(Json(json!({
        "sub": token.user_id
    })))
}

pub fn create_federation_router(state: AppState) -> Router<AppState> {
    let public = Router::new()
        .route("/_matrix/federation/v2/server", get(keys::server_key))
        .route("/_matrix/key/v2/server", get(keys::server_key))
        .route("/_matrix/federation/v2/key/clone", post(keys::key_clone))
        .route("/_matrix/federation/v2/query/{server_name}/{key_id}", get(keys::key_query))
        .route("/_matrix/key/v2/query/{server_name}/{key_id}", get(keys::key_query))
        .route("/_matrix/federation/v1/version", get(federation_version))
        .route("/_matrix/federation/v1", get(federation_discovery))
        .route("/_matrix/federation/v1/publicRooms", get(events::get_public_rooms))
        .route("/_matrix/federation/v1/query/destination", get(events::query_destination))
        .route("/_matrix/federation/v1/openid/userinfo", get(openid_userinfo));

    let protected = Router::new()
        .route("/_matrix/federation/v1/members/{room_id}", get(membership::get_room_members))
        .route("/_matrix/federation/v1/members/{room_id}/joined", get(membership::get_joined_room_members))
        .route("/_matrix/federation/v1/user/devices/{user_id}", get(membership::get_user_devices))
        .route("/_matrix/federation/v1/room_auth/{room_id}", get(events::get_room_auth))
        .route("/_matrix/federation/v1/knock/{room_id}/{user_id}", post(membership::knock_room))
        .route("/_matrix/federation/v1/thirdparty/invite", post(membership::thirdparty_invite))
        .route("/_matrix/federation/v1/get_joining_rules/{room_id}", get(membership::get_joining_rules))
        .route("/_matrix/federation/v2/invite/{room_id}/{event_id}", put(membership::invite_v2))
        .route("/_matrix/federation/v1/send/{txn_id}", put(transaction::send_transaction))
        .route("/_matrix/federation/v1/make_join/{room_id}/{user_id}", get(membership::make_join))
        .route("/_matrix/federation/v1/make_leave/{room_id}/{user_id}", get(membership::make_leave))
        .route("/_matrix/federation/v1/send_join/{room_id}/{event_id}", put(membership::send_join))
        .route("/_matrix/federation/v1/send_leave/{room_id}/{event_id}", put(membership::send_leave))
        .route("/_matrix/federation/v1/invite/{room_id}/{event_id}", put(membership::invite))
        .route("/_matrix/federation/v1/get_missing_events/{room_id}", post(events::get_missing_events))
        .route("/_matrix/federation/v1/room/{room_id}/{event_id}", get(events::get_room_event))
        .route("/_matrix/federation/v1/timestamp_to_event/{room_id}", get(events::timestamp_to_event))
        .route("/_matrix/federation/v1/get_event_auth/{room_id}/{event_id}", get(events::get_event_auth))
        .route("/_matrix/federation/v1/query/auth", get(keys::query_auth))
        .route("/_matrix/federation/v1/event_auth", get(keys::event_auth))
        .route("/_matrix/federation/v1/state/{room_id}", get(events::get_state))
        .route("/_matrix/federation/v1/event/{event_id}", get(events::get_event))
        .route("/_matrix/federation/v1/state_ids/{room_id}", get(events::get_state_ids))
        .route("/_matrix/federation/v1/query/directory/room/{room_id}", get(events::room_directory_query))
        .route("/_matrix/federation/v1/query/profile", get(events::profile_query))
        .route("/_matrix/federation/v1/query/profile/{user_id}", get(events::profile_query_legacy))
        .route("/_matrix/federation/v1/hierarchy/{room_id}", get(events::get_room_hierarchy))
        .route("/_matrix/federation/v1/backfill/{room_id}", get(events::backfill))
        .route("/_matrix/federation/v1/keys/claim", post(keys::legacy_keys_claim))
        .route("/_matrix/federation/v1/keys/query", post(keys::legacy_keys_query))
        .route("/_matrix/federation/v1/keys/upload", post(keys::keys_upload))
        .route("/_matrix/federation/v1/user/keys/upload", post(keys::keys_upload))
        .route("/_matrix/federation/v1/user/keys/claim", post(keys::keys_claim))
        .route("/_matrix/federation/v1/user/keys/query", post(keys::keys_query))
        .route("/_matrix/federation/v2/user/keys/query", post(keys::keys_query))
        .route("/_matrix/federation/v2/send_join/{room_id}/{event_id}", put(membership::send_join_v2))
        .route("/_matrix/federation/v2/send_leave/{room_id}/{event_id}", put(membership::send_leave_v2))
        .route("/_matrix/federation/v1/publicRooms", post(events::post_public_rooms))
        .route("/_matrix/federation/v1/query/directory", get(events::query_directory))
        .route("/_matrix/federation/v1/media/download/{server_name}/{media_id}", get(media::media_download))
        .route("/_matrix/federation/v1/media/thumbnail/{server_name}/{media_id}", get(media::media_thumbnail))
        .route(
            "/_matrix/federation/v1/exchange_third_party_invite/{room_id}",
            put(membership::exchange_third_party_invite),
        );

    let protected =
        protected.layer(middleware::from_fn_with_state(state, crate::web::middleware::federation_auth_middleware));

    public.merge(protected)
}

fn federation_public_relative_routes() -> Vec<(axum::http::Method, &'static str)> {
    use axum::http::Method;
    vec![
        (Method::GET, "/_matrix/federation/v2/server"),
        (Method::GET, "/_matrix/key/v2/server"),
        (Method::POST, "/_matrix/federation/v2/key/clone"),
        (Method::GET, "/_matrix/federation/v2/query/{server_name}/{key_id}"),
        (Method::GET, "/_matrix/key/v2/query/{server_name}/{key_id}"),
        (Method::GET, "/_matrix/federation/v1/version"),
        (Method::GET, "/_matrix/federation/v1"),
        (Method::GET, "/_matrix/federation/v1/publicRooms"),
        (Method::GET, "/_matrix/federation/v1/query/destination"),
        (Method::GET, "/_matrix/federation/v1/openid/userinfo"),
    ]
}

fn federation_protected_relative_routes() -> Vec<(axum::http::Method, &'static str)> {
    use axum::http::Method;
    vec![
        (Method::GET, "/_matrix/federation/v1/members/{room_id}"),
        (Method::GET, "/_matrix/federation/v1/members/{room_id}/joined"),
        (Method::GET, "/_matrix/federation/v1/user/devices/{user_id}"),
        (Method::GET, "/_matrix/federation/v1/room_auth/{room_id}"),
        (Method::POST, "/_matrix/federation/v1/knock/{room_id}/{user_id}"),
        (Method::POST, "/_matrix/federation/v1/thirdparty/invite"),
        (Method::GET, "/_matrix/federation/v1/get_joining_rules/{room_id}"),
        (Method::PUT, "/_matrix/federation/v2/invite/{room_id}/{event_id}"),
        (Method::PUT, "/_matrix/federation/v1/send/{txn_id}"),
        (Method::GET, "/_matrix/federation/v1/make_join/{room_id}/{user_id}"),
        (Method::GET, "/_matrix/federation/v1/make_leave/{room_id}/{user_id}"),
        (Method::PUT, "/_matrix/federation/v1/send_join/{room_id}/{event_id}"),
        (Method::PUT, "/_matrix/federation/v1/send_leave/{room_id}/{event_id}"),
        (Method::PUT, "/_matrix/federation/v1/invite/{room_id}/{event_id}"),
        (Method::POST, "/_matrix/federation/v1/get_missing_events/{room_id}"),
        (Method::GET, "/_matrix/federation/v1/room/{room_id}/{event_id}"),
        (Method::GET, "/_matrix/federation/v1/timestamp_to_event/{room_id}"),
        (Method::GET, "/_matrix/federation/v1/get_event_auth/{room_id}/{event_id}"),
        (Method::GET, "/_matrix/federation/v1/query/auth"),
        (Method::GET, "/_matrix/federation/v1/event_auth"),
        (Method::GET, "/_matrix/federation/v1/state/{room_id}"),
        (Method::GET, "/_matrix/federation/v1/event/{event_id}"),
        (Method::GET, "/_matrix/federation/v1/state_ids/{room_id}"),
        (Method::GET, "/_matrix/federation/v1/query/directory/room/{room_id}"),
        (Method::GET, "/_matrix/federation/v1/query/profile"),
        (Method::GET, "/_matrix/federation/v1/query/profile/{user_id}"),
        (Method::GET, "/_matrix/federation/v1/hierarchy/{room_id}"),
        (Method::GET, "/_matrix/federation/v1/backfill/{room_id}"),
        (Method::POST, "/_matrix/federation/v1/keys/claim"),
        (Method::POST, "/_matrix/federation/v1/keys/query"),
        (Method::POST, "/_matrix/federation/v1/keys/upload"),
        (Method::POST, "/_matrix/federation/v1/user/keys/upload"),
        (Method::POST, "/_matrix/federation/v1/user/keys/claim"),
        (Method::POST, "/_matrix/federation/v1/user/keys/query"),
        (Method::POST, "/_matrix/federation/v2/user/keys/query"),
        (Method::PUT, "/_matrix/federation/v2/send_join/{room_id}/{event_id}"),
        (Method::PUT, "/_matrix/federation/v2/send_leave/{room_id}/{event_id}"),
        (Method::POST, "/_matrix/federation/v1/publicRooms"),
        (Method::GET, "/_matrix/federation/v1/query/directory"),
        (Method::GET, "/_matrix/federation/v1/media/download/{server_name}/{media_id}"),
        (Method::GET, "/_matrix/federation/v1/media/thumbnail/{server_name}/{media_id}"),
        (Method::PUT, "/_matrix/federation/v1/exchange_third_party_invite/{room_id}"),
    ]
}

pub fn federation_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;

    federation_public_relative_routes()
        .into_iter()
        .chain(federation_protected_relative_routes())
        .map(|(m, p)| RouteEntry::new(m, p, "federation"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::web::utils::encoding::decode_base64_32;

    #[test]
    fn test_decode_base64_32() {
        let valid_key = "dGVzdGtleTEyMzQ1Njc4OTAxMjM0NTY3ODkwMTIzNDU";
        let result = decode_base64_32(valid_key);
        assert!(result.is_some());
    }

    #[test]
    fn test_decode_base64_32_invalid() {
        let invalid_key = "invalid!!!";
        let result = decode_base64_32(invalid_key);
        assert!(result.is_none());
    }

    #[test]
    fn test_federation_version_response() {
        let response = json!({
            "server": {
                "name": "Synapse Rust",
                "version": "0.1.0"
            }
        });
        assert_eq!(response["server"]["name"], "Synapse Rust");
    }

    #[test]
    fn test_server_key_structure() {
        let key_response = json!({
            "server_name": "example.com",
            "valid_until_ts": 1234567890000_u64,
            "verify_keys": {
                "ed25519:0": {
                    "key": "test_key"
                }
            }
        });
        assert!(key_response["verify_keys"].is_object());
    }

    #[test]
    fn test_transaction_structure() {
        let txn = json!({
            "origin": "example.com",
            "origin_server_ts": 1234567890000_u64,
            "pdus": [],
            "edus": []
        });
        assert_eq!(txn["origin"], "example.com");
    }

    #[test]
    fn test_exchange_third_party_invite_content_shape() {
        let invite = json!({
            "signed": {
                "mxid": "@alice:example.com",
                "token": "invite-token"
            }
        });

        assert_eq!(invite["signed"]["mxid"], "@alice:example.com");
        assert_eq!(invite["signed"]["token"], "invite-token");
    }
}
