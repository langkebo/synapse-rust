pub(crate) mod invite;
pub(crate) mod join;
pub(crate) mod knock;
pub(crate) mod leave;
pub(crate) mod query;

use crate::common::*;
use crate::web::routes::context::FederationContext;
use crate::web::routes::AppState;
use axum::{
    routing::{get, post, put},
    Router,
};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Re-export federation-level helpers so submodules can access them via
// `super::` (instead of the more verbose `super::super::`).
// ---------------------------------------------------------------------------
use super::{
    acquire_with_timeout, decrement_gauge, increment_counter, increment_gauge, observe_histogram, sender_server_name,
    user_matches_origin, validate_federation_origin, validate_federation_origin_can_observe_room,
    validate_federation_origin_shares_user_room,
};

// ---------------------------------------------------------------------------
// Shared helper functions used across multiple submodules
// ---------------------------------------------------------------------------

pub(crate) async fn federatable_room_version(ctx: &FederationContext, room_id: &str) -> Result<String, ApiError> {
    let room = ctx
        .room_service
        .state()
        .get_room_record(room_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Room not found"))?;

    if !can_federate_room_version(&room.room_version) {
        return Err(ApiError::incompatible_room_version(format!(
            "Room version {} is not supported for federation",
            room.room_version
        )));
    }

    Ok(room.room_version)
}

pub(crate) async fn dispatch_federation_member_event_to_appservice(
    ctx: &FederationContext,
    event_id: &str,
    room_id: &str,
    sender: &str,
    content: &Value,
    state_key: Option<&str>,
) {
    ctx.room_service.dispatch_appservice_event(event_id, room_id, "m.room.member", sender, content, state_key).await;
}

pub(crate) fn validate_federation_user_origin(authenticated_origin: &str, user_id: &str) -> Result<(), ApiError> {
    if sender_server_name(user_id) != Some(authenticated_origin) {
        return Err(ApiError::forbidden("Federation user_id does not match authenticated origin".to_string()));
    }

    Ok(())
}

pub(crate) fn validate_federation_member_event<'a>(
    authenticated_origin: &str,
    room_id: &str,
    event_id: &str,
    event: &'a Value,
    expected_membership: &str,
) -> Result<&'a str, ApiError> {
    let sender = event
        .get("sender")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request(format!("Missing sender in {expected_membership} event")))?;

    if sender_server_name(sender) != Some(authenticated_origin) {
        return Err(ApiError::forbidden(
            "Federation member event sender does not match authenticated origin".to_string(),
        ));
    }

    let event_room_id = event
        .get("room_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing room_id in membership event".to_string()))?;
    if event_room_id != room_id {
        return Err(ApiError::bad_request("Membership event room_id does not match request path".to_string()));
    }

    let event_event_id = event
        .get("event_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing event_id in membership event".to_string()))?;
    if event_event_id != event_id {
        return Err(ApiError::bad_request("Membership event event_id does not match request path".to_string()));
    }

    let event_type = event
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing event type in membership event".to_string()))?;
    if event_type != "m.room.member" {
        return Err(ApiError::bad_request(
            "Federation send_join/send_leave only accepts m.room.member events".to_string(),
        ));
    }

    let state_key = event
        .get("state_key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing state_key in membership event".to_string()))?;
    if state_key != sender {
        return Err(ApiError::bad_request("Membership event state_key must match sender".to_string()));
    }

    let membership = event
        .get("content")
        .and_then(|v| v.get("membership"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing membership in event content".to_string()))?;
    if membership != expected_membership {
        return Err(ApiError::bad_request(format!(
            "Expected membership '{expected_membership}' but got '{membership}'"
        )));
    }

    if let Some(event_origin) = event.get("origin").and_then(|v| v.as_str()) {
        validate_federation_origin(authenticated_origin, Some(event_origin))?;
    }

    Ok(sender)
}

pub(crate) async fn get_effective_room_join_rule_content(
    ctx: &FederationContext,
    room_id: &str,
) -> ApiResult<Option<Value>> {
    Ok(ctx
        .room_service
        .messaging()
        .get_state_events_by_type(room_id, "m.room.join_rules")
        .await?
        .into_iter()
        .find(|event| event.get("state_key").and_then(Value::as_str).unwrap_or_default().is_empty())
        .and_then(|event| event.get("content").cloned()))
}

pub(crate) async fn get_effective_room_join_rule(ctx: &FederationContext, room_id: &str) -> ApiResult<String> {
    let effective_join_rule = if let Some(content) = get_effective_room_join_rule_content(ctx, room_id).await? {
        content.get("join_rule").and_then(|value| value.as_str()).map(|value| value.to_string())
    } else {
        None
    };

    let room = ctx
        .room_service
        .state()
        .get_room_record(room_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Room not found"))?;

    Ok(effective_join_rule.or_else(|| (!room.join_rule.is_empty()).then(|| room.join_rule.clone())).unwrap_or_else(
        || {
            if room.is_public {
                "public".to_string()
            } else {
                "invite".to_string()
            }
        },
    ))
}

// ---------------------------------------------------------------------------
// Router assembly
// ---------------------------------------------------------------------------

/// Build the membership sub-router with all federation membership routes.
pub(crate) fn create_router() -> Router<AppState> {
    Router::new()
        .route("/_matrix/federation/v1/members/{room_id}", get(query::get_room_members))
        .route("/_matrix/federation/v1/members/{room_id}/joined", get(query::get_joined_room_members))
        .route("/_matrix/federation/v1/user/devices/{user_id}", get(query::get_user_devices))
        .route("/_matrix/federation/v1/knock/{room_id}/{user_id}", post(knock::knock_room))
        .route("/_matrix/federation/v1/thirdparty/invite", post(invite::thirdparty_invite))
        .route("/_matrix/federation/v2/invite/{room_id}/{event_id}", put(invite::invite_v2))
        .route("/_matrix/federation/v1/make_join/{room_id}/{user_id}", get(join::make_join))
        .route("/_matrix/federation/v1/make_leave/{room_id}/{user_id}", get(leave::make_leave))
        .route("/_matrix/federation/v1/send_join/{room_id}/{event_id}", put(join::send_join))
        .route("/_matrix/federation/v1/send_leave/{room_id}/{event_id}", put(leave::send_leave))
        .route("/_matrix/federation/v1/invite/{room_id}/{event_id}", put(invite::invite))
        .route("/_matrix/federation/v2/send_join/{room_id}/{event_id}", put(join::send_join_v2))
        .route("/_matrix/federation/v2/send_leave/{room_id}/{event_id}", put(leave::send_leave_v2))
        .route("/_matrix/federation/v1/exchange_third_party_invite/{room_id}", put(invite::exchange_third_party_invite))
        .route("/_synapse/federation/v1/get_joining_rules/{room_id}", get(query::get_joining_rules))
}

// ---------------------------------------------------------------------------
// Route manifest – keeps the route ledger aligned with the router
// ---------------------------------------------------------------------------

pub(crate) fn membership_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    vec![
        RouteEntry::new(axum::http::Method::GET, "/_matrix/federation/v1/members/{room_id}", "federation"),
        RouteEntry::new(axum::http::Method::GET, "/_matrix/federation/v1/members/{room_id}/joined", "federation"),
        RouteEntry::new(axum::http::Method::GET, "/_matrix/federation/v1/user/devices/{user_id}", "federation"),
        RouteEntry::new(axum::http::Method::POST, "/_matrix/federation/v1/knock/{room_id}/{user_id}", "federation"),
        RouteEntry::new(axum::http::Method::POST, "/_matrix/federation/v1/thirdparty/invite", "federation"),
        RouteEntry::new(axum::http::Method::PUT, "/_matrix/federation/v2/invite/{room_id}/{event_id}", "federation"),
        RouteEntry::new(axum::http::Method::GET, "/_matrix/federation/v1/make_join/{room_id}/{user_id}", "federation"),
        RouteEntry::new(axum::http::Method::GET, "/_matrix/federation/v1/make_leave/{room_id}/{user_id}", "federation"),
        RouteEntry::new(axum::http::Method::PUT, "/_matrix/federation/v1/send_join/{room_id}/{event_id}", "federation"),
        RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/federation/v1/send_leave/{room_id}/{event_id}",
            "federation",
        ),
        RouteEntry::new(axum::http::Method::PUT, "/_matrix/federation/v1/invite/{room_id}/{event_id}", "federation"),
        RouteEntry::new(axum::http::Method::PUT, "/_matrix/federation/v2/send_join/{room_id}/{event_id}", "federation"),
        RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/federation/v2/send_leave/{room_id}/{event_id}",
            "federation",
        ),
        RouteEntry::new(
            axum::http::Method::PUT,
            "/_matrix/federation/v1/exchange_third_party_invite/{room_id}",
            "federation",
        ),
        RouteEntry::new(axum::http::Method::GET, "/_synapse/federation/v1/get_joining_rules/{room_id}", "federation"),
    ]
}
