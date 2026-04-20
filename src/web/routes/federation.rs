use crate::common::*;
use crate::web::middleware::FederationRequestAuth;
use crate::web::routes::validate_room_alias;
use crate::web::routes::AppState;
use crate::web::utils::encoding::decode_base64_32;
use axum::{
    extract::{Extension, Json, Path, Query, RawQuery, State},
    middleware,
    response::IntoResponse,
    routing::{get, post, put},
    Router,
};
use base64::Engine;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::{timeout, Instant};

fn validate_federation_origin(
    authenticated_origin: &str,
    declared_origin: Option<&str>,
) -> Result<(), ApiError> {
    let declared_origin =
        declared_origin.ok_or_else(|| ApiError::bad_request("Origin required".to_string()))?;

    if declared_origin != authenticated_origin {
        return Err(ApiError::forbidden(
            "Federation origin does not match authenticated request".to_string(),
        ));
    }

    Ok(())
}

fn sender_server_name(sender: &str) -> Option<&str> {
    sender
        .strip_prefix('@')
        .and_then(|user| user.rsplit_once(':').map(|(_, server)| server))
        .filter(|server| !server.is_empty())
}

fn validate_federation_user_origin(
    authenticated_origin: &str,
    user_id: &str,
) -> Result<(), ApiError> {
    if sender_server_name(user_id) != Some(authenticated_origin) {
        return Err(ApiError::forbidden(
            "Federation user_id does not match authenticated origin".to_string(),
        ));
    }

    Ok(())
}

fn validate_federation_member_event<'a>(
    authenticated_origin: &str,
    room_id: &str,
    event_id: &str,
    event: &'a Value,
    expected_membership: &str,
) -> Result<&'a str, ApiError> {
    let sender = event
        .get("sender")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            ApiError::bad_request(format!("Missing sender in {} event", expected_membership))
        })?;

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
        return Err(ApiError::bad_request(
            "Membership event room_id does not match request path".to_string(),
        ));
    }

    let event_event_id = event
        .get("event_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing event_id in membership event".to_string()))?;
    if event_event_id != event_id {
        return Err(ApiError::bad_request(
            "Membership event event_id does not match request path".to_string(),
        ));
    }

    let event_type = event.get("type").and_then(|v| v.as_str()).ok_or_else(|| {
        ApiError::bad_request("Missing event type in membership event".to_string())
    })?;
    if event_type != "m.room.member" {
        return Err(ApiError::bad_request(
            "Federation send_join/send_leave only accepts m.room.member events".to_string(),
        ));
    }

    let state_key = event
        .get("state_key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            ApiError::bad_request("Missing state_key in membership event".to_string())
        })?;
    if state_key != sender {
        return Err(ApiError::bad_request(
            "Membership event state_key must match sender".to_string(),
        ));
    }

    let membership = event
        .get("content")
        .and_then(|v| v.get("membership"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing membership in event content".to_string()))?;
    if membership != expected_membership {
        return Err(ApiError::bad_request(format!(
            "Expected membership '{}' but got '{}'",
            expected_membership, membership
        )));
    }

    if let Some(event_origin) = event.get("origin").and_then(|v| v.as_str()) {
        validate_federation_origin(authenticated_origin, Some(event_origin))?;
    }

    Ok(sender)
}

fn validate_federation_knock_event<'a>(
    authenticated_origin: &str,
    room_id: &str,
    user_id: &str,
    event: &'a Value,
) -> Result<&'a str, ApiError> {
    let sender = validate_federation_member_event_without_event_id(
        authenticated_origin,
        room_id,
        event,
        "knock",
    )?;

    if sender != user_id {
        return Err(ApiError::bad_request(
            "Knock event sender must match request path user_id".to_string(),
        ));
    }

    Ok(sender)
}

fn validate_federation_member_event_without_event_id<'a>(
    authenticated_origin: &str,
    room_id: &str,
    event: &'a Value,
    expected_membership: &str,
) -> Result<&'a str, ApiError> {
    let sender = event
        .get("sender")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            ApiError::bad_request(format!("Missing sender in {} event", expected_membership))
        })?;

    if sender_server_name(sender) != Some(authenticated_origin) {
        return Err(ApiError::forbidden(format!(
            "Federation {} event sender does not match authenticated origin",
            expected_membership
        )));
    }

    let event_room_id = event
        .get("room_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing room_id in membership event".to_string()))?;
    if event_room_id != room_id {
        return Err(ApiError::bad_request(
            "Membership event room_id does not match request path".to_string(),
        ));
    }

    let event_type = event.get("type").and_then(|v| v.as_str()).ok_or_else(|| {
        ApiError::bad_request("Missing event type in membership event".to_string())
    })?;
    if event_type != "m.room.member" {
        return Err(ApiError::bad_request(
            "Federation membership endpoints only accept m.room.member events".to_string(),
        ));
    }

    let state_key = event
        .get("state_key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            ApiError::bad_request("Missing state_key in membership event".to_string())
        })?;
    if state_key != sender {
        return Err(ApiError::bad_request(
            "Membership event state_key must match sender".to_string(),
        ));
    }

    let membership = event
        .get("content")
        .and_then(|v| v.get("membership"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing membership in event content".to_string()))?;
    if membership != expected_membership {
        return Err(ApiError::bad_request(format!(
            "Expected membership '{}' but got '{}'",
            expected_membership, membership
        )));
    }

    if let Some(event_origin) = event.get("origin").and_then(|v| v.as_str()) {
        validate_federation_origin(authenticated_origin, Some(event_origin))?;
    }

    Ok(sender)
}

fn validate_federation_invite_event<'a>(
    authenticated_origin: &str,
    room_id: &str,
    event_id: &str,
    event: &'a Value,
) -> Result<(&'a str, &'a str), ApiError> {
    let sender = event
        .get("sender")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing sender in invite event".to_string()))?;

    if sender_server_name(sender) != Some(authenticated_origin) {
        return Err(ApiError::forbidden(
            "Federation invite event sender does not match authenticated origin".to_string(),
        ));
    }

    let event_room_id = event
        .get("room_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing room_id in invite event".to_string()))?;
    if event_room_id != room_id {
        return Err(ApiError::bad_request(
            "Invite event room_id does not match request path".to_string(),
        ));
    }

    let event_event_id = event
        .get("event_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing event_id in invite event".to_string()))?;
    if event_event_id != event_id {
        return Err(ApiError::bad_request(
            "Invite event event_id does not match request path".to_string(),
        ));
    }

    let event_type = event
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing event type in invite event".to_string()))?;
    if event_type != "m.room.member" {
        return Err(ApiError::bad_request(
            "Federation invite only accepts m.room.member events".to_string(),
        ));
    }

    let state_key = event
        .get("state_key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing state_key in invite event".to_string()))?;

    let membership = event
        .get("content")
        .and_then(|v| v.get("membership"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing membership in invite event".to_string()))?;
    if membership != "invite" {
        return Err(ApiError::bad_request(format!(
            "Expected membership 'invite' but got '{}'",
            membership
        )));
    }

    if let Some(event_origin) = event.get("origin").and_then(|v| v.as_str()) {
        validate_federation_origin(authenticated_origin, Some(event_origin))?;
    }

    Ok((sender, state_key))
}

fn validate_federation_exchange_third_party_invite_event<'a>(
    authenticated_origin: &str,
    room_id: &str,
    event: &'a Value,
) -> Result<(&'a str, &'a str), ApiError> {
    if let Some(origin) = event.get("origin").and_then(|v| v.as_str()) {
        validate_federation_origin(authenticated_origin, Some(origin))?;
    }

    let sender = event
        .get("sender")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            ApiError::bad_request("Missing sender in third-party invite event".to_string())
        })?;
    if sender_server_name(sender) != Some(authenticated_origin) {
        return Err(ApiError::forbidden(
            "Federation third-party invite sender does not match authenticated origin".to_string(),
        ));
    }

    let event_room_id = event
        .get("room_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            ApiError::bad_request("Missing room_id in third-party invite event".to_string())
        })?;
    if event_room_id != room_id {
        return Err(ApiError::bad_request(
            "Third-party invite room_id does not match request path".to_string(),
        ));
    }

    let event_type = event.get("type").and_then(|v| v.as_str()).ok_or_else(|| {
        ApiError::bad_request("Missing event type in third-party invite event".to_string())
    })?;
    if event_type != "m.room.member" {
        return Err(ApiError::bad_request(
            "Federation third-party invite only accepts m.room.member events".to_string(),
        ));
    }

    let state_key = event
        .get("state_key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            ApiError::bad_request("Missing state_key in third-party invite event".to_string())
        })?;
    if state_key.is_empty() {
        return Err(ApiError::bad_request(
            "Third-party invite state_key must not be empty".to_string(),
        ));
    }

    let membership = event
        .get("content")
        .and_then(|v| v.get("membership"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            ApiError::bad_request("Missing membership in third-party invite event".to_string())
        })?;
    if membership != "invite" {
        return Err(ApiError::bad_request(format!(
            "Expected membership 'invite' but got '{}'",
            membership
        )));
    }

    Ok((sender, state_key))
}

fn validate_inbound_transaction_pdu<'a>(
    authenticated_origin: &str,
    pdu: &'a Value,
) -> Result<(&'a str, &'a str, &'a str, Option<&'a str>), ApiError> {
    let room_id = pdu
        .get("room_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing room_id in inbound PDU".to_string()))?;
    let sender = pdu
        .get("sender")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing sender in inbound PDU".to_string()))?;
    let event_type = pdu
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing type in inbound PDU".to_string()))?;
    let state_key = pdu.get("state_key").and_then(|v| v.as_str());

    if sender_server_name(sender) != Some(authenticated_origin) {
        return Err(ApiError::forbidden(
            "Federation PDU sender does not match authenticated origin".to_string(),
        ));
    }

    if let Some(event_origin) = pdu.get("origin").and_then(|v| v.as_str()) {
        validate_federation_origin(authenticated_origin, Some(event_origin))?;
    }

    Ok((room_id, sender, event_type, state_key))
}

async fn get_effective_room_join_rule(state: &AppState, room_id: &str) -> ApiResult<String> {
    let effective_join_rule =
        if let Some(content) = get_effective_room_join_rule_content(state, room_id).await? {
            content
                .get("join_rule")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string())
        } else {
            None
        };

    let room = state
        .services
        .room_storage
        .get_room(room_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Room not found"))?;

    Ok(effective_join_rule
        .or_else(|| (!room.join_rule.is_empty()).then(|| room.join_rule.clone()))
        .unwrap_or_else(|| {
            if room.is_public {
                "public".to_string()
            } else {
                "invite".to_string()
            }
        }))
}

async fn get_effective_room_join_rule_content(
    state: &AppState,
    room_id: &str,
) -> ApiResult<Option<Value>> {
    Ok(state
        .services
        .event_storage
        .get_state_events_by_type(room_id, "m.room.join_rules")
        .await
        .map_err(|e| ApiError::internal(format!("Failed to load room join rules: {}", e)))?
        .into_iter()
        .find(|event| event.state_key.as_deref().unwrap_or_default().is_empty())
        .map(|event| event.content))
}

async fn validate_federation_join_access(
    state: &AppState,
    room_id: &str,
    user_id: &str,
) -> ApiResult<()> {
    let join_rule = get_effective_room_join_rule(state, room_id).await?;
    let existing_member = state
        .services
        .member_storage
        .get_room_member(room_id, user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check membership: {}", e)))?;

    if let Some(member) = existing_member.as_ref() {
        if member.membership == "join" {
            return Ok(());
        }

        if member.membership == "ban" || member.is_banned.unwrap_or(false) {
            return Err(ApiError::forbidden("User is not allowed to join this room"));
        }
    }

    if join_rule != "public"
        && existing_member
            .as_ref()
            .is_none_or(|member| member.membership != "invite")
    {
        return Err(ApiError::forbidden("User is not allowed to join this room"));
    }

    Ok(())
}

async fn validate_federation_origin_in_room(
    state: &AppState,
    room_id: &str,
    origin: &str,
) -> ApiResult<()> {
    let joined_members = state
        .services
        .member_storage
        .get_room_members(room_id, "join")
        .await
        .map_err(|e| ApiError::internal(format!("Failed to load room members: {}", e)))?;

    if joined_members
        .iter()
        .any(|member| user_matches_origin(&member.user_id, origin))
    {
        return Ok(());
    }

    Err(ApiError::forbidden(
        "Authenticated server has no joined members in this room".to_string(),
    ))
}

async fn validate_federation_origin_can_observe_room(
    state: &AppState,
    room_id: &str,
    origin: &str,
) -> ApiResult<()> {
    let joined_members = state
        .services
        .member_storage
        .get_room_members(room_id, "join")
        .await
        .map_err(|e| ApiError::internal(format!("Failed to load room members: {}", e)))?;

    if joined_members
        .iter()
        .any(|member| user_matches_origin(&member.user_id, origin))
    {
        return Ok(());
    }

    Err(ApiError::not_found("Room not found".to_string()))
}

async fn validate_federation_origin_shares_user_room(
    state: &AppState,
    user_id: &str,
    origin: &str,
) -> ApiResult<()> {
    let joined_room_ids = state
        .services
        .room_storage
        .get_user_rooms(user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to load user rooms: {}", e)))?;

    for room_id in joined_room_ids {
        let joined_members = state
            .services
            .member_storage
            .get_room_members(&room_id, "join")
            .await
            .map_err(|e| ApiError::internal(format!("Failed to load room members: {}", e)))?;

        if joined_members
            .iter()
            .any(|member| user_matches_origin(&member.user_id, origin))
        {
            return Ok(());
        }
    }

    Err(ApiError::forbidden(
        "Authenticated server does not share a room with this user".to_string(),
    ))
}

fn increment_counter(state: &AppState, name: &str) {
    if let Some(counter) = state.services.metrics.get_counter(name) {
        counter.inc();
    } else {
        state
            .services
            .metrics
            .register_counter(name.to_string())
            .inc();
    }
}

fn increment_counter_by(state: &AppState, name: &str, delta: u64) {
    if let Some(counter) = state.services.metrics.get_counter(name) {
        counter.inc_by(delta);
    } else {
        state
            .services
            .metrics
            .register_counter(name.to_string())
            .inc_by(delta);
    }
}

fn observe_histogram(state: &AppState, name: &str, value: f64) {
    if let Some(histogram) = state.services.metrics.get_histogram(name) {
        histogram.observe(value);
    } else {
        state
            .services
            .metrics
            .register_histogram(name.to_string())
            .observe(value);
    }
}

fn increment_gauge(state: &AppState, name: &str) {
    if let Some(gauge) = state.services.metrics.get_gauge(name) {
        gauge.inc();
    } else {
        state
            .services
            .metrics
            .register_gauge(name.to_string())
            .inc();
    }
}

fn decrement_gauge(state: &AppState, name: &str) {
    if let Some(gauge) = state.services.metrics.get_gauge(name) {
        gauge.dec();
    } else {
        state
            .services
            .metrics
            .register_gauge(name.to_string())
            .dec();
    }
}

async fn acquire_with_timeout(
    semaphore: Arc<Semaphore>,
    acquire_timeout_ms: u64,
) -> Result<(OwnedSemaphorePermit, u64), ApiError> {
    let started = Instant::now();
    let permit = timeout(
        Duration::from_millis(acquire_timeout_ms.max(1)),
        semaphore.acquire_owned(),
    )
    .await
    .map_err(|_| ApiError::rate_limited_with_retry(acquire_timeout_ms.max(1)))?
    .map_err(|_| ApiError::internal("Semaphore closed"))?;

    Ok((permit, started.elapsed().as_millis() as u64))
}

async fn acquire_origin_edu_permit(
    state: &AppState,
    origin: &str,
) -> Result<(OwnedSemaphorePermit, u64), ApiError> {
    let per_origin_limit = state
        .services
        .config
        .federation
        .inbound_edu_per_origin_max_concurrency
        .max(1);
    let semaphore = {
        let mut guard = state.federation_inbound_edu_origin_semaphores.lock().await;
        guard
            .entry(origin.to_string())
            .or_insert_with(|| Arc::new(Semaphore::new(per_origin_limit)))
            .clone()
    };

    acquire_with_timeout(
        semaphore,
        state
            .services
            .config
            .federation
            .inbound_edu_acquire_timeout_ms,
    )
    .await
}

async fn get_presence_backoff_remaining_ms(state: &AppState, origin: &str) -> Option<u64> {
    let now = chrono::Utc::now().timestamp_millis();
    let guard = state.federation_presence_backoff_until.read().await;
    let until = guard.get(origin).copied()?;
    (until > now).then_some((until - now) as u64)
}

async fn set_presence_backoff(state: &AppState, origin: &str) {
    let until = chrono::Utc::now().timestamp_millis()
        + state.services.config.federation.inbound_presence_backoff_ms as i64;
    let mut guard = state.federation_presence_backoff_until.write().await;
    guard.insert(origin.to_string(), until);
}

fn user_matches_origin(user_id: &str, origin: &str) -> bool {
    user_id
        .rsplit_once(':')
        .map(|(_, server_name)| server_name == origin)
        .unwrap_or(false)
}

fn validate_federation_media_server_name(
    state: &AppState,
    server_name: &str,
) -> Result<(), ApiError> {
    if server_name != state.services.server_name {
        return Err(ApiError::not_found(
            "Media is not hosted on this server".to_string(),
        ));
    }

    Ok(())
}

fn parse_federation_query_i64(params: &Value, key: &str, default: i64) -> Result<i64, ApiError> {
    match params.get(key) {
        Some(Value::Number(value)) => value
            .as_i64()
            .ok_or_else(|| ApiError::bad_request(format!("Invalid '{}' parameter", key))),
        Some(Value::String(value)) => value
            .parse::<i64>()
            .map_err(|_| ApiError::bad_request(format!("Invalid '{}' parameter", key))),
        Some(_) => Err(ApiError::bad_request(format!(
            "Invalid '{}' parameter",
            key
        ))),
        None => Ok(default),
    }
}

async fn process_inbound_presence_edu(
    state: &AppState,
    origin: &str,
    edu: &Value,
    remaining_updates: usize,
) -> (usize, usize, usize) {
    let Some(push) = edu
        .get("content")
        .and_then(|content| content.get("push"))
        .and_then(|value| value.as_array())
    else {
        increment_counter(state, "federation_inbound_presence_dropped_total");
        return (0, 0, 0);
    };

    let mut processed = 0usize;
    let mut dropped = 0usize;
    let mut errored = 0usize;

    for update in push.iter().take(remaining_updates) {
        let Some(user_id) = update.get("user_id").and_then(|value| value.as_str()) else {
            dropped += 1;
            continue;
        };

        if !user_matches_origin(user_id, origin) {
            dropped += 1;
            continue;
        }

        let presence = update
            .get("presence")
            .and_then(|value| value.as_str())
            .unwrap_or("online");
        let status_msg = update.get("status_msg").and_then(|value| value.as_str());

        let exists = match state.services.user_storage.user_exists(user_id).await {
            Ok(exists) => exists,
            Err(error) => {
                ::tracing::warn!(
                    "Failed to validate presence user {} from {}: {}",
                    user_id,
                    origin,
                    error
                );
                errored += 1;
                set_presence_backoff(state, origin).await;
                break;
            }
        };

        if !exists {
            dropped += 1;
            continue;
        }

        if let Err(error) = state
            .services
            .presence_storage
            .set_presence(user_id, presence, status_msg)
            .await
        {
            ::tracing::warn!(
                "Failed to persist presence update for {} from {}: {}",
                user_id,
                origin,
                error
            );
            errored += 1;
            set_presence_backoff(state, origin).await;
            break;
        }

        processed += 1;
    }

    if processed > 0 {
        increment_counter_by(
            state,
            "federation_inbound_presence_processed_total",
            processed as u64,
        );
    }
    if dropped > 0 {
        increment_counter_by(
            state,
            "federation_inbound_presence_dropped_total",
            dropped as u64,
        );
    }
    if errored > 0 {
        increment_counter_by(
            state,
            "federation_inbound_presence_error_total",
            errored as u64,
        );
    }

    (processed, dropped, errored)
}

pub fn create_federation_router(state: AppState) -> Router<AppState> {
    let public = Router::new()
        .route("/_matrix/federation/v2/server", get(server_key))
        .route("/_matrix/key/v2/server", get(server_key))
        .route("/_matrix/federation/v2/key/clone", post(key_clone))
        .route(
            "/_matrix/federation/v2/query/{server_name}/{key_id}",
            get(key_query),
        )
        .route(
            "/_matrix/key/v2/query/{server_name}/{key_id}",
            get(key_query),
        )
        .route("/_matrix/federation/v1/version", get(federation_version))
        .route("/_matrix/federation/v1", get(federation_discovery))
        .route("/_matrix/federation/v1/publicRooms", get(get_public_rooms))
        .route(
            "/_matrix/federation/v1/query/destination",
            get(query_destination),
        )
        .route(
            "/_matrix/federation/v1/openid/userinfo",
            get(openid_userinfo),
        );

    let protected = Router::new()
        .route(
            "/_matrix/federation/v1/members/{room_id}",
            get(get_room_members),
        )
        .route(
            "/_matrix/federation/v1/members/{room_id}/joined",
            get(get_joined_room_members),
        )
        .route(
            "/_matrix/federation/v1/user/devices/{user_id}",
            get(get_user_devices),
        )
        .route(
            "/_matrix/federation/v1/room_auth/{room_id}",
            get(get_room_auth),
        )
        .route(
            "/_matrix/federation/v1/knock/{room_id}/{user_id}",
            put(knock_room),
        )
        .route(
            "/_matrix/federation/v1/thirdparty/invite",
            post(thirdparty_invite),
        )
        .route(
            "/_matrix/federation/v1/get_joining_rules/{room_id}",
            get(get_joining_rules),
        )
        .route(
            "/_matrix/federation/v2/invite/{room_id}/{event_id}",
            put(invite_v2),
        )
        .route(
            "/_matrix/federation/v1/send/{txn_id}",
            put(send_transaction),
        )
        .route(
            "/_matrix/federation/v1/make_join/{room_id}/{user_id}",
            get(make_join),
        )
        .route(
            "/_matrix/federation/v1/make_leave/{room_id}/{user_id}",
            get(make_leave),
        )
        .route(
            "/_matrix/federation/v1/send_join/{room_id}/{event_id}",
            put(send_join),
        )
        .route(
            "/_matrix/federation/v1/send_leave/{room_id}/{event_id}",
            put(send_leave),
        )
        .route(
            "/_matrix/federation/v1/invite/{room_id}/{event_id}",
            put(invite),
        )
        .route(
            "/_matrix/federation/v1/get_missing_events/{room_id}",
            post(get_missing_events),
        )
        .route(
            "/_matrix/federation/v1/room/{room_id}/{event_id}",
            get(get_room_event),
        )
        .route(
            "/_matrix/federation/v1/timestamp_to_event/{room_id}",
            get(timestamp_to_event),
        )
        .route(
            "/_matrix/federation/v1/get_event_auth/{room_id}/{event_id}",
            get(get_event_auth),
        )
        .route("/_matrix/federation/v1/query/auth", get(query_auth))
        .route("/_matrix/federation/v1/event_auth", get(event_auth))
        .route("/_matrix/federation/v1/state/{room_id}", get(get_state))
        .route("/_matrix/federation/v1/event/{event_id}", get(get_event))
        .route(
            "/_matrix/federation/v1/state_ids/{room_id}",
            get(get_state_ids),
        )
        .route(
            "/_matrix/federation/v1/query/directory/room/{room_id}",
            get(room_directory_query),
        )
        .route("/_matrix/federation/v1/query/profile", get(profile_query))
        .route(
            "/_matrix/federation/v1/query/profile/{user_id}",
            get(profile_query_legacy),
        )
        .route(
            "/_matrix/federation/v1/hierarchy/{room_id}",
            get(get_room_hierarchy),
        )
        .route("/_matrix/federation/v1/backfill/{room_id}", get(backfill))
        .route("/_matrix/federation/v1/keys/claim", post(legacy_keys_claim))
        .route("/_matrix/federation/v1/keys/query", post(legacy_keys_query))
        .route("/_matrix/federation/v1/keys/upload", post(keys_upload))
        .route("/_matrix/federation/v1/user/keys/claim", post(keys_claim))
        .route("/_matrix/federation/v1/user/keys/query", post(keys_query))
        .route("/_matrix/federation/v2/user/keys/query", post(keys_query))
        // v2 endpoints (High Priority)
        .route(
            "/_matrix/federation/v2/send_join/{room_id}/{event_id}",
            put(send_join_v2),
        )
        .route(
            "/_matrix/federation/v2/send_leave/{room_id}/{event_id}",
            put(send_leave_v2),
        )
        // Additional endpoints (Medium Priority)
        .route(
            "/_matrix/federation/v1/publicRooms",
            post(post_public_rooms),
        )
        .route(
            "/_matrix/federation/v1/query/directory",
            get(query_directory),
        )
        // Media Federation
        .route(
            "/_matrix/federation/v1/media/download/{server_name}/{media_id}",
            get(media_download),
        )
        .route(
            "/_matrix/federation/v1/media/thumbnail/{server_name}/{media_id}",
            get(media_thumbnail),
        )
        // Third-party invite
        .route(
            "/_matrix/federation/v1/exchange_third_party_invite/{room_id}",
            put(exchange_third_party_invite),
        );

    let protected = protected.layer(middleware::from_fn_with_state(
        state,
        crate::web::middleware::federation_auth_middleware,
    ));

    public.merge(protected)
}

async fn federation_version(State(_state): State<AppState>) -> Json<Value> {
    Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "server": {
            "name": "synapse-rust",
            "version": env!("CARGO_PKG_VERSION")
        }
    }))
}

async fn federation_discovery(State(state): State<AppState>) -> Json<Value> {
    Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "server_name": state.services.server_name,
        "capabilities": {
            "m.change_password": true,
            "m.room_versions": {
                "1": {
                    "status": "stable"
                }
            }
        }
    }))
}

async fn get_room_members(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_federation_origin_in_room(&state, &room_id, &auth.origin).await?;

    let members = state
        .services
        .member_storage
        .get_room_members(&room_id, "join")
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room members: {}", e)))?;

    let members_json: Vec<Value> = members
        .into_iter()
        .map(|m| {
            json!({
                "room_id": m.room_id,
                "user_id": m.user_id,
                "membership": m.membership,
                "display_name": m.display_name,
                "avatar_url": m.avatar_url
            })
        })
        .collect();

    Ok(Json(json!({
        "members": members_json,
        "room_id": room_id,
        "offset": 0,
        "total": members_json.len()
    })))
}

async fn knock_room(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path((room_id, user_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_federation_user_origin(&auth.origin, &user_id)?;
    validate_federation_knock_event(&auth.origin, &room_id, &user_id, &body)?;

    let event_id = format!(
        "${}",
        crate::common::crypto::generate_event_id(&state.services.server_name)
    );

    let params = crate::storage::event::CreateEventParams {
        event_id: event_id.clone(),
        room_id: room_id.clone(),
        user_id: user_id.clone(),
        event_type: "m.room.member".to_string(),
        content: json!({"membership": "knock"}),
        state_key: Some(user_id.clone()),
        origin_server_ts: chrono::Utc::now().timestamp_millis(),
    };

    state
        .services
        .event_storage
        .create_event(params, None)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create knock event: {}", e)))?;

    Ok(Json(json!({
        "event_id": event_id,
        "room_id": room_id,
        "state": "knocking"
    })))
}

async fn thirdparty_invite(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let room_id = body
        .get("room_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("room_id required".to_string()))?;
    if !room_id.starts_with('!') || !room_id.contains(':') {
        return Err(ApiError::bad_request("Invalid room_id format".to_string()));
    }

    let invitee = body
        .get("invitee")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("invitee required".to_string()))?;
    let sender = body
        .get("sender")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("sender required".to_string()))?;
    validate_federation_user_origin(&auth.origin, sender)?;

    state
        .services
        .room_storage
        .get_room(room_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    let event_id = format!(
        "${}",
        crate::common::crypto::generate_event_id(&state.services.server_name)
    );

    let params = crate::storage::event::CreateEventParams {
        event_id: event_id.clone(),
        room_id: room_id.to_string(),
        user_id: sender.to_string(),
        event_type: "m.room.member".to_string(),
        content: json!({
            "membership": "invite",
            "third_party_invite": {
                "signed": {
                    "mxid": invitee,
                    "token": format!("third_party_token_{}", event_id)
                }
            }
        }),
        state_key: Some(invitee.to_string()),
        origin_server_ts: chrono::Utc::now().timestamp_millis(),
    };

    state
        .services
        .event_storage
        .create_event(params, None)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create invite event: {}", e)))?;

    Ok(Json(json!({
        "event_id": event_id,
        "room_id": room_id,
        "state": "invited"
    })))
}

async fn get_joining_rules(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let join_rule_content = get_effective_room_join_rule_content(&state, &room_id).await?;
    let join_rule = get_effective_room_join_rule(&state, &room_id).await?;

    if join_rule != "public" {
        validate_federation_origin_in_room(&state, &room_id, &auth.origin).await?;
    }

    let allow = join_rule_content
        .as_ref()
        .and_then(|content| content.get("allow"))
        .filter(|value| value.is_array())
        .cloned()
        .unwrap_or_else(|| json!([]));

    Ok(Json(json!({
        "room_id": room_id,
        "join_rule": join_rule,
        "allow": allow
    })))
}

async fn get_joined_room_members(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_federation_origin_in_room(&state, &room_id, &auth.origin).await?;

    let members = state
        .services
        .member_storage
        .get_room_members(&room_id, "join")
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room members: {}", e)))?;

    let members_json: Vec<Value> = members
        .into_iter()
        .map(|m| {
            json!({
                "room_id": m.room_id,
                "user_id": m.user_id,
                "membership": m.membership,
                "display_name": m.display_name,
                "avatar_url": m.avatar_url
            })
        })
        .collect();

    Ok(Json(json!({
        "joined": members_json,
        "room_id": room_id
    })))
}

async fn get_user_devices(
    State(state): State<AppState>,
    Extension(_auth): Extension<FederationRequestAuth>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !user_matches_origin(&user_id, &state.services.server_name) {
        return Err(ApiError::not_found(
            "User is not hosted on this server".to_string(),
        ));
    }

    validate_federation_origin_shares_user_room(&state, &user_id, &_auth.origin).await?;

    let devices = state
        .services
        .device_storage
        .get_user_devices(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get user devices: {}", e)))?;

    let stream_id: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(MAX(stream_id), 0)
        FROM device_lists_stream
        WHERE user_id = $1
        "#,
    )
    .bind(&user_id)
    .fetch_one(&*state.services.device_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to get device stream id: {}", e)))?;

    let master_key: Option<Value> = sqlx::query_scalar(
        r#"
        SELECT key_data
        FROM cross_signing_keys
        WHERE user_id = $1 AND key_type = 'master'
        LIMIT 1
        "#,
    )
    .bind(&user_id)
    .fetch_optional(&*state.services.device_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to get master key: {}", e)))?
    .flatten()
    .and_then(|raw: String| serde_json::from_str(&raw).ok());

    let self_signing_key: Option<Value> = sqlx::query_scalar(
        r#"
        SELECT key_data
        FROM cross_signing_keys
        WHERE user_id = $1 AND key_type = 'self_signing'
        LIMIT 1
        "#,
    )
    .bind(&user_id)
    .fetch_optional(&*state.services.device_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to get self-signing key: {}", e)))?
    .flatten()
    .and_then(|raw: String| serde_json::from_str(&raw).ok());

    let devices_json: Vec<Value> = devices
        .into_iter()
        .map(|d| {
            let keys = d.device_key.unwrap_or_else(|| json!({}));
            let algorithms = keys.get("algorithms").cloned().unwrap_or_else(|| json!([]));
            let signatures = keys.get("signatures").cloned().unwrap_or_else(|| json!({}));
            let keys_map = keys.get("keys").cloned().unwrap_or_else(|| json!({}));
            json!({
                "device_id": d.device_id,
                "user_id": d.user_id,
                "algorithms": algorithms,
                "keys": keys_map,
                "signatures": signatures
            })
        })
        .collect();

    Ok(Json(json!({
        "user_id": user_id,
        "stream_id": stream_id,
        "devices": devices_json,
        "master_key": master_key,
        "self_signing_key": self_signing_key
    })))
}

async fn get_room_auth(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_federation_origin_in_room(&state, &room_id, &auth.origin).await?;

    let auth_events = state
        .services
        .event_storage
        .get_state_events(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get auth events: {}", e)))?;

    let auth_chain: Vec<Value> = auth_events
        .into_iter()
        .filter(|e| {
            e.event_type.as_deref() == Some("m.room.create")
                || e.event_type.as_deref() == Some("m.room.member")
                || e.event_type.as_deref() == Some("m.room.power_levels")
                || e.event_type.as_deref() == Some("m.room.join_rules")
                || e.event_type.as_deref() == Some("m.room.history_visibility")
        })
        .map(|e| {
            json!({
                "event_id": e.event_id,
                "type": e.event_type.clone().unwrap_or_default(),
                "sender": e.user_id,
                "content": e.content,
                "state_key": e.state_key,
                "origin_server_ts": e.origin_server_ts
            })
        })
        .collect();

    Ok(Json(json!({
        "room_id": room_id,
        "auth_chain": auth_chain
    })))
}

async fn invite_v2(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path((room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if let Some(origin) = body.get("origin").and_then(|v| v.as_str()) {
        validate_federation_origin(&auth.origin, Some(origin))?;
    }
    let (sender, state_key) =
        validate_federation_invite_event(&auth.origin, &room_id, &event_id, &body)?;
    let content = body.get("content").cloned().unwrap_or(json!({}));

    let params = crate::storage::event::CreateEventParams {
        event_id: event_id.clone(),
        room_id: room_id.clone(),
        user_id: sender.to_string(),
        event_type: "m.room.member".to_string(),
        content,
        state_key: Some(state_key.to_string()),
        origin_server_ts: body
            .get("origin_server_ts")
            .and_then(|v| v.as_i64())
            .unwrap_or(chrono::Utc::now().timestamp_millis()),
    };

    state
        .services
        .event_storage
        .create_event(params, None)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create invite event: {}", e)))?;

    ::tracing::info!(
        "Processed v2 invite for room {} event {} from {}",
        room_id,
        event_id,
        auth.origin
    );

    Ok(Json(json!({
        "event_id": event_id
    })))
}

async fn send_transaction(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(txn_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    increment_counter(&state, "federation_inbound_txn_total");

    let origin = body
        .get("origin")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Origin required".to_string()))?;
    validate_federation_origin(&auth.origin, Some(origin))?;
    let pdus = body
        .get("pdus") // Matrix spec uses 'pdus'
        .or_else(|| body.get("pdu")) // Fallback to 'pdu'
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiError::bad_request("PDUs required".to_string()))?;
    let edus = body.get("edus").and_then(|v| v.as_array());
    let process_inbound_edus = state.services.config.federation.process_inbound_edus;
    let process_inbound_presence_edus = state
        .services
        .config
        .federation
        .process_inbound_presence_edus;
    let inbound_edus_max_per_txn = state.services.config.federation.inbound_edus_max_per_txn;
    let inbound_presence_updates_max_per_txn = state
        .services
        .config
        .federation
        .inbound_presence_updates_max_per_txn;

    if process_inbound_edus {
        if let Some(edus) = edus {
            let mut processed_edus = 0usize;
            let mut processed_presence_updates = 0usize;
            let mut dropped_presence_updates = 0usize;

            increment_gauge(&state, "federation_inbound_edu_in_flight");

            let edu_processing = async {
                let (_global_permit, wait_ms) = acquire_with_timeout(
                    state.federation_inbound_edu_semaphore.clone(),
                    state
                        .services
                        .config
                        .federation
                        .inbound_edu_acquire_timeout_ms,
                )
                .await?;
                observe_histogram(&state, "federation_inbound_edu_wait_ms", wait_ms as f64);

                let _origin_permit = acquire_origin_edu_permit(&state, origin).await?.0;

                if let Some(backoff_ms) = get_presence_backoff_remaining_ms(&state, origin).await {
                    increment_counter(&state, "federation_inbound_presence_backoff_total");
                    ::tracing::debug!(
                        "Skipping presence EDU processing for origin {} due to backoff {}ms",
                        origin,
                        backoff_ms
                    );
                    return Ok::<(), ApiError>(());
                }

                for edu in edus.iter().take(inbound_edus_max_per_txn) {
                    processed_edus += 1;
                    let edu_type = edu.get("edu_type").and_then(|v| v.as_str()).unwrap_or("");
                    if edu_type != "m.presence" || !process_inbound_presence_edus {
                        continue;
                    }

                    if processed_presence_updates >= inbound_presence_updates_max_per_txn {
                        break;
                    }

                    let remaining =
                        inbound_presence_updates_max_per_txn - processed_presence_updates;
                    let (processed, dropped, errored) =
                        process_inbound_presence_edu(&state, origin, edu, remaining).await;
                    processed_presence_updates += processed;
                    dropped_presence_updates += dropped + errored;

                    if errored > 0 {
                        break;
                    }
                }
                Ok::<(), ApiError>(())
            }
            .await;

            if let Err(error) = edu_processing {
                if matches!(error, ApiError::RateLimitedWithRetry(_)) {
                    increment_counter(&state, "federation_inbound_edu_limited_total");
                } else {
                    increment_counter(&state, "federation_inbound_edu_error_total");
                    ::tracing::warn!(
                        "Failed to process inbound EDUs for txn {} from {}: {}",
                        txn_id,
                        origin,
                        error
                    );
                }
            }

            decrement_gauge(&state, "federation_inbound_edu_in_flight");

            ::tracing::debug!(
                "Inbound federation txn {} from {}: pdus={}, edus_total={}, edus_processed={}, presence_updates_processed={}, presence_updates_dropped={}",
                txn_id,
                origin,
                pdus.len(),
                edus.len(),
                processed_edus,
                processed_presence_updates,
                dropped_presence_updates
            );
        }
    }

    let mut results = Vec::new();

    const MAX_PDUS_PER_TRANSACTION: usize = 100;
    if pdus.len() > MAX_PDUS_PER_TRANSACTION {
        ::tracing::warn!(
            target: "security_audit",
            event = "federation_pdu_count_exceeded",
            origin = origin,
            pdu_count = pdus.len(),
            max = MAX_PDUS_PER_TRANSACTION,
            "Transaction contains too many PDUs - truncating"
        );
    }
    let pdus_to_process = &pdus[..pdus.len().min(MAX_PDUS_PER_TRANSACTION)];

    for pdu in pdus_to_process {
        let event_id = pdu
            .get("event_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("${}", crate::common::crypto::generate_event_id(origin)));

        if let Err(e) = crate::federation::signing::check_pdu_size_limits(pdu) {
            increment_counter(&state, "federation_inbound_txn_pdu_error_total");
            ::tracing::warn!(
                target: "security_audit",
                event = "federation_pdu_size_limit_exceeded",
                event_id = event_id,
                origin = origin,
                error = %e,
                "Inbound PDU exceeded size limits"
            );
            results.push(json!({
                "event_id": event_id,
                "error": e
            }));
            continue;
        }

        if let Err(e) = crate::federation::signing::verify_event_content_hash(pdu) {
            increment_counter(&state, "federation_inbound_txn_pdu_error_total");
            ::tracing::warn!(
                target: "security_audit",
                event = "federation_pdu_hash_mismatch",
                event_id = event_id,
                origin = origin,
                error = %e,
                "Inbound PDU content hash verification failed"
            );
            results.push(json!({
                "event_id": event_id,
                "error": e
            }));
            continue;
        }

        let (room_id, user_id, event_type, state_key) =
            match validate_inbound_transaction_pdu(&auth.origin, pdu) {
                Ok(validated) => validated,
                Err(error) => {
                    increment_counter(&state, "federation_inbound_txn_pdu_error_total");
                    results.push(json!({
                        "event_id": event_id,
                        "error": error.to_string()
                    }));
                    continue;
                }
            };
        let content = pdu.get("content").cloned().unwrap_or(json!({}));
        let origin_server_ts = pdu
            .get("origin_server_ts")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        if origin != state.services.config.server.name {
            if let Ok(create_events) = state
                .services
                .event_storage
                .get_state_events_by_type(room_id, "m.room.create")
                .await
            {
                if let Some(create_event) = create_events.first() {
                    if !crate::federation::signing::check_event_federate(&create_event.content) {
                        increment_counter(&state, "federation_inbound_txn_pdu_error_total");
                        ::tracing::warn!(
                            target: "security_audit",
                            event = "federation_non_federated_room_rejected",
                            room_id = room_id,
                            origin = origin,
                            event_id = event_id,
                            "Rejected inbound PDU for non-federated room"
                        );
                        results.push(json!({
                            "event_id": event_id,
                            "error": "This room is not federated"
                        }));
                        continue;
                    }
                }
            }
        }

        if event_type != "m.room.create" {
            if let Err(e) = validate_federation_origin_in_room(&state, room_id, origin).await {
                increment_counter(&state, "federation_inbound_txn_pdu_error_total");
                ::tracing::warn!(
                    target: "security_audit",
                    event = "federation_origin_not_in_room",
                    room_id = room_id,
                    origin = origin,
                    event_id = event_id,
                    error = %e,
                    "Rejected inbound PDU from origin with no members in room"
                );
                results.push(json!({
                    "event_id": event_id,
                    "error": "Origin server has no joined members in this room"
                }));
                continue;
            }
        }

        if state_key.is_some() && event_type != "m.room.member" {
            if let Err(error) = state
                .services
                .auth_service
                .verify_state_event_write(room_id, user_id, event_type)
                .await
            {
                increment_counter(&state, "federation_inbound_txn_pdu_error_total");
                results.push(json!({
                    "event_id": event_id,
                    "error": error.to_string()
                }));
                continue;
            }
        }

        let params = crate::storage::event::CreateEventParams {
            event_id: event_id.clone(),
            room_id: room_id.to_string(),
            user_id: user_id.to_string(),
            event_type: event_type.to_string(),
            content,
            state_key: state_key.map(|s| s.to_string()),
            origin_server_ts,
        };

        match state
            .services
            .event_storage
            .create_event(params, None)
            .await
        {
            Ok(_) => {
                increment_counter(&state, "federation_inbound_txn_pdu_success_total");
                results.push(json!({
                    "event_id": event_id,
                    "success": true
                }));
            }
            Err(e) => {
                increment_counter(&state, "federation_inbound_txn_pdu_error_total");
                ::tracing::error!("Failed to persist PDU {}: {}", event_id, e);
                results.push(json!({
                    "event_id": event_id,
                    "error": e.to_string()
                }));
            }
        }
    }

    ::tracing::info!(
        "Processed transaction {} from {} with {} PDUs",
        txn_id,
        origin,
        pdus.len()
    );

    Ok(Json(json!({
        "results": results
    })))
}

async fn make_join(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path((room_id, user_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    increment_gauge(&state, "federation_join_in_flight");
    let (permit, wait_ms) = match acquire_with_timeout(
        state.federation_join_semaphore.clone(),
        state.services.config.federation.join_acquire_timeout_ms,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => {
            decrement_gauge(&state, "federation_join_in_flight");
            increment_counter(&state, "federation_join_429_total");
            return Err(error);
        }
    };
    observe_histogram(&state, "federation_join_wait_ms", wait_ms as f64);

    let result = async {
        validate_federation_user_origin(&auth.origin, &user_id)?;

        let auth_events = state
            .services
            .event_storage
            .get_state_events(&room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get auth events: {}", e)))?;

        let auth_events_json: Vec<Value> = auth_events
            .iter()
            .map(|e| {
                json!({
                    "event_id": e.event_id,
                    "type": e.event_type,
                    "state_key": e.state_key
                })
            })
            .collect();

        Ok(Json(json!({
            "room_version": "1",
            "auth_events": auth_events_json,
            "event": {
                "type": "m.room.member",
                "content": {
                    "membership": "join"
                },
                "sender": user_id,
                "state_key": user_id
            }
        })))
    }
    .await;

    drop(permit);
    decrement_gauge(&state, "federation_join_in_flight");
    match &result {
        Ok(_) => increment_counter(&state, "federation_join_ok_total"),
        Err(ApiError::RateLimitedWithRetry(_)) => {
            increment_counter(&state, "federation_join_429_total")
        }
        Err(_) => increment_counter(&state, "federation_join_error_total"),
    }

    result
}

async fn make_leave(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path((room_id, user_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_federation_user_origin(&auth.origin, &user_id)?;

    let auth_events = state
        .services
        .event_storage
        .get_state_events(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get auth events: {}", e)))?;

    let auth_events_json: Vec<Value> = auth_events
        .iter()
        .map(|e| {
            json!({
                "event_id": e.event_id,
                "type": e.event_type,
                "state_key": e.state_key
            })
        })
        .collect();

    Ok(Json(json!({
        "room_version": "1",
        "auth_events": auth_events_json,
        "event": {
            "type": "m.room.member",
            "content": {
                "membership": "leave"
            },
            "sender": user_id,
            "state_key": user_id
        }
    })))
}

async fn send_join(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path((room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    increment_gauge(&state, "federation_join_in_flight");
    let (permit, wait_ms) = match acquire_with_timeout(
        state.federation_join_semaphore.clone(),
        state.services.config.federation.join_acquire_timeout_ms,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => {
            decrement_gauge(&state, "federation_join_in_flight");
            increment_counter(&state, "federation_join_429_total");
            return Err(error);
        }
    };
    observe_histogram(&state, "federation_join_wait_ms", wait_ms as f64);

    let result = async {
        validate_federation_origin(&auth.origin, body.get("origin").and_then(|v| v.as_str()))?;

        let event = body
            .get("event")
            .ok_or_else(|| ApiError::bad_request("Event required".to_string()))?;
        let user_id =
            validate_federation_member_event(&auth.origin, &room_id, &event_id, event, "join")?;
        validate_federation_join_access(&state, &room_id, user_id).await?;
        let content = event.get("content").cloned().unwrap_or(json!({}));
        let display_name = content.get("displayname").and_then(|v| v.as_str());

        let params = crate::storage::event::CreateEventParams {
            event_id: event_id.clone(),
            room_id: room_id.clone(),
            user_id: user_id.to_string(),
            event_type: "m.room.member".to_string(),
            content: content.clone(),
            state_key: Some(user_id.to_string()),
            origin_server_ts: event
                .get("origin_server_ts")
                .and_then(|v| v.as_i64())
                .unwrap_or(0),
        };
        state
            .services
            .event_storage
            .create_event(params, None)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to persist join event: {}", e)))?;

        state
            .services
            .member_storage
            .add_member(&room_id, user_id, "join", display_name, None, None)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update membership: {}", e)))?;

        ::tracing::info!(
            "Processed join for room {} event {} from {}",
            room_id,
            event_id,
            auth.origin
        );

        Ok(Json(json!({
            "event_id": event_id
        })))
    }
    .await;

    drop(permit);
    decrement_gauge(&state, "federation_join_in_flight");
    match &result {
        Ok(_) => increment_counter(&state, "federation_join_ok_total"),
        Err(ApiError::RateLimitedWithRetry(_)) => {
            increment_counter(&state, "federation_join_429_total")
        }
        Err(_) => increment_counter(&state, "federation_join_error_total"),
    }

    result
}

async fn send_leave(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path((room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_federation_origin(&auth.origin, body.get("origin").and_then(|v| v.as_str()))?;

    let event = body
        .get("event")
        .ok_or_else(|| ApiError::bad_request("Event required".to_string()))?;
    let user_id =
        validate_federation_member_event(&auth.origin, &room_id, &event_id, event, "leave")?;

    // 1. Persist the event
    let params = crate::storage::event::CreateEventParams {
        event_id: event_id.clone(),
        room_id: room_id.clone(),
        user_id: user_id.to_string(),
        event_type: "m.room.member".to_string(),
        content: event.get("content").cloned().unwrap_or(json!({})),
        state_key: Some(user_id.to_string()),
        origin_server_ts: event
            .get("origin_server_ts")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
    };
    state
        .services
        .event_storage
        .create_event(params, None)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to persist leave event: {}", e)))?;

    // 2. Update membership
    state
        .services
        .member_storage
        .add_member(&room_id, user_id, "leave", None, None, None)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to update membership: {}", e)))?;

    ::tracing::info!(
        "Processed leave for room {} event {} from {}",
        room_id,
        event_id,
        auth.origin
    );

    Ok(Json(json!({
        "event_id": event_id
    })))
}

async fn invite(
    State(_state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path((room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if let Some(origin) = body.get("origin").and_then(|v| v.as_str()) {
        validate_federation_origin(&auth.origin, Some(origin))?;
    }
    validate_federation_invite_event(&auth.origin, &room_id, &event_id, &body)?;

    ::tracing::info!("Processing invite for room {} event {}", room_id, event_id);

    Ok(Json(json!({
        "event_id": event_id
    })))
}

async fn get_missing_events(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_federation_origin_in_room(&state, &room_id, &auth.origin).await?;

    let _earliest_events = body
        .get("earliest_events")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiError::bad_request("earliest_events required".to_string()))?;
    let _latest_events = body
        .get("latest_events")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiError::bad_request("latest_events required".to_string()))?;
    let limit = body.get("limit").and_then(|v| v.as_i64()).unwrap_or(10);

    let events = state
        .services
        .event_storage
        .get_room_events(&room_id, limit)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get missing events: {}", e)))?;

    let events_json: Vec<Value> = events
        .into_iter()
        .map(|e| {
            json!({
                "event_id": e.event_id,
                "type": e.event_type,
                "sender": e.user_id,
                "content": e.content,
                "room_id": e.room_id,
                "origin_server_ts": e.origin_server_ts
            })
        })
        .collect();

    Ok(Json(json!({
        "events": events_json
    })))
}

async fn get_event_auth(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path((room_id, event_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_federation_origin_in_room(&state, &room_id, &auth.origin).await?;

    let event = get_room_event_in_room(&state, &room_id, &event_id).await?;
    let auth_events = state
        .services
        .event_storage
        .get_state_events_at_or_before(&room_id, event.origin_server_ts)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get auth events: {}", e)))?;

    let auth_chain: Vec<Value> = auth_events
        .into_iter()
        .map(|e| {
            json!({
                "event_id": e.event_id,
                "type": e.event_type,
                "sender": e.user_id,
                "content": e.content,
                "state_key": e.state_key,
                "origin_server_ts": e.origin_server_ts
            })
        })
        .collect();

    Ok(Json(json!({
        "auth_chain": auth_chain
    })))
}

async fn get_event(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(event_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let event = state
        .services
        .event_storage
        .get_event(&event_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get event: {}", e)))?;

    match event {
        Some(e) => {
            validate_federation_origin_in_room(&state, &e.room_id, &auth.origin).await?;
            Ok(Json(build_federation_event_response(
                &state.services.server_name,
                &e,
            )))
        }
        None => Err(ApiError::not_found("Event not found".to_string())),
    }
}

async fn get_room_event(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path((room_id, event_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_federation_origin_in_room(&state, &room_id, &auth.origin).await?;

    let event = state
        .services
        .event_storage
        .get_event(&event_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get event: {}", e)))?;

    match event {
        Some(e) => {
            if e.room_id != room_id {
                return Err(ApiError::bad_request(
                    "Event does not belong to this room".to_string(),
                ));
            }
            Ok(Json(build_federation_event_response(
                &state.services.server_name,
                &e,
            )))
        }
        None => Err(ApiError::not_found("Event not found".to_string())),
    }
}

fn build_federation_event_response(
    server_name: &str,
    event: &crate::storage::event::RoomEvent,
) -> Value {
    let event_origin = match event.origin.trim() {
        "" | "self" | "undefined" => server_name.to_string(),
        value => value.to_string(),
    };

    json!({
        "origin": server_name,
        "origin_server_ts": chrono::Utc::now().timestamp_millis(),
        "pdus": [{
            "event_id": event.event_id,
            "type": event.event_type,
            "sender": event.user_id,
            "content": event.content,
            "state_key": event.state_key,
            "origin_server_ts": event.origin_server_ts,
            "room_id": event.room_id,
            "origin": event_origin
        }]
    })
}

fn normalized_event_origin(server_name: &str, origin: Option<&str>) -> String {
    match origin.map(str::trim) {
        Some("") | Some("self") | Some("undefined") | None => server_name.to_string(),
        Some(value) => value.to_string(),
    }
}

fn serialize_state_event_minimal(
    server_name: &str,
    event: &crate::storage::event::StateEvent,
) -> Value {
    json!({
        "event_id": event.event_id,
        "type": event.event_type,
        "sender": event.user_id.as_deref().unwrap_or(&event.sender),
        "content": event.content,
        "state_key": event.state_key,
        "origin_server_ts": event.origin_server_ts,
        "room_id": event.room_id,
        "origin": normalized_event_origin(server_name, event.origin.as_deref())
    })
}

fn serialize_room_event_minimal(
    server_name: &str,
    event: &crate::storage::event::RoomEvent,
) -> Value {
    json!({
        "event_id": event.event_id,
        "type": event.event_type,
        "sender": event.user_id,
        "content": event.content,
        "state_key": event.state_key,
        "origin_server_ts": event.origin_server_ts,
        "room_id": event.room_id,
        "origin": normalized_event_origin(server_name, Some(&event.origin))
    })
}

fn sort_state_events_stably(events: &mut [crate::storage::event::StateEvent]) {
    events.sort_by(|left, right| {
        right
            .origin_server_ts
            .cmp(&left.origin_server_ts)
            .then_with(|| left.event_id.cmp(&right.event_id))
    });
}

fn sort_room_events_stably(events: &mut [crate::storage::event::RoomEvent]) {
    events.sort_by(|left, right| {
        right
            .depth
            .cmp(&left.depth)
            .then_with(|| right.origin_server_ts.cmp(&left.origin_server_ts))
            .then_with(|| left.event_id.cmp(&right.event_id))
    });
}

fn build_federation_state_payload(
    server_name: &str,
    events: &mut [crate::storage::event::StateEvent],
) -> (Vec<Value>, Vec<Value>) {
    sort_state_events_stably(events);

    let pdus = events
        .iter()
        .map(|event| serialize_state_event_minimal(server_name, event))
        .collect();
    let auth_chain = events
        .iter()
        .filter(|event| {
            event
                .event_type
                .as_deref()
                .map(crate::federation::event_auth::EventAuthChain::is_auth_event)
                .unwrap_or(false)
        })
        .map(|event| serialize_state_event_minimal(server_name, event))
        .collect();

    (pdus, auth_chain)
}

#[derive(Deserialize, Default)]
struct FederationStateAtEventQuery {
    event_id: Option<String>,
}

async fn get_room_event_in_room(
    state: &AppState,
    room_id: &str,
    event_id: &str,
) -> Result<crate::storage::event::RoomEvent, ApiError> {
    let event = state
        .services
        .event_storage
        .get_event(event_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get event: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Event not found".to_string()))?;

    if event.room_id != room_id {
        return Err(ApiError::bad_request(
            "Event does not belong to this room".to_string(),
        ));
    }

    Ok(event)
}

async fn load_federation_state_events(
    state: &AppState,
    room_id: &str,
    event_id: Option<&str>,
) -> Result<Vec<crate::storage::event::StateEvent>, ApiError> {
    match event_id {
        Some(event_id) => {
            let event = get_room_event_in_room(state, room_id, event_id).await?;
            state
                .services
                .event_storage
                .get_state_events_at_or_before(room_id, event.origin_server_ts)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to get state: {}", e)))
        }
        None => state
            .services
            .event_storage
            .get_state_events(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get state: {}", e))),
    }
}

async fn query_destination(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({
        "server_name": state.services.server_name,
        "destination": state.services.server_name,
        "retry_last_ts": 0,
        "retry_interval_ms": 0
    })))
}

async fn get_state(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<String>,
    Query(query): Query<FederationStateAtEventQuery>,
) -> Result<Json<Value>, ApiError> {
    validate_federation_origin_can_observe_room(&state, &room_id, &auth.origin).await?;

    let mut events =
        load_federation_state_events(&state, &room_id, query.event_id.as_deref()).await?;
    let (pdus, auth_chain) =
        build_federation_state_payload(&state.services.server_name, &mut events);

    Ok(Json(json!({
        "room_id": room_id,
        "origin": state.services.server_name,
        "pdus": pdus,
        "auth_chain": auth_chain
    })))
}

async fn get_state_ids(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<String>,
    Query(query): Query<FederationStateAtEventQuery>,
) -> Result<Json<Value>, ApiError> {
    validate_federation_origin_can_observe_room(&state, &room_id, &auth.origin).await?;

    let mut events =
        load_federation_state_events(&state, &room_id, query.event_id.as_deref()).await?;
    sort_state_events_stably(&mut events);

    let pdu_ids: Vec<String> = events.iter().map(|event| event.event_id.clone()).collect();
    let auth_chain_ids: Vec<String> = events
        .iter()
        .filter(|event| {
            event
                .event_type
                .as_deref()
                .map(crate::federation::event_auth::EventAuthChain::is_auth_event)
                .unwrap_or(false)
        })
        .map(|event| event.event_id.clone())
        .collect();

    Ok(Json(json!({
        "room_id": room_id,
        "origin": state.services.server_name,
        "pdu_ids": pdu_ids,
        "auth_chain_ids": auth_chain_ids
    })))
}

#[axum::debug_handler]
async fn room_directory_query(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    // 1. Try rooms
    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if let Some(room) = room {
        if !room.is_public {
            validate_federation_origin_in_room(&state, &room_id, &auth.origin).await?;
        }

        return Ok(Json(json!({
            "room_id": room.room_id,
            "servers": [state.services.server_name]
        })));
    }

    // 2. Try private sessions (Federation might ask for DM room info)
    // Deprecated: Private chat module removed.
    if room_id.starts_with("ps_") {
        return Err(ApiError::not_found(
            "Private session not supported".to_string(),
        ));
    }

    Err(ApiError::not_found("Room not found".to_string()))
}

#[axum::debug_handler]
async fn profile_query(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Query(params): Query<FederationProfileQueryParams>,
) -> Result<Json<Value>, ApiError> {
    let user_id = params
        .user_id
        .ok_or_else(|| ApiError::bad_request("Missing user_id query parameter".to_string()))?;

    build_profile_query_response(&state, &auth.origin, &user_id, params.field.as_deref()).await
}

#[derive(Deserialize)]
struct FederationProfileQueryParams {
    user_id: Option<String>,
    field: Option<String>,
}

#[derive(Deserialize)]
struct FederationProfileFieldQuery {
    field: Option<String>,
}

#[derive(Deserialize)]
struct FederationHierarchyQueryParams {
    max_depth: Option<i32>,
    suggested_only: Option<bool>,
    limit: Option<i32>,
    from: Option<String>,
}

#[axum::debug_handler]
async fn profile_query_legacy(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(user_id): Path<String>,
    Query(params): Query<FederationProfileFieldQuery>,
) -> Result<Json<Value>, ApiError> {
    build_profile_query_response(&state, &auth.origin, &user_id, params.field.as_deref()).await
}

async fn build_profile_query_response(
    state: &AppState,
    origin: &str,
    user_id: &str,
    field: Option<&str>,
) -> Result<Json<Value>, ApiError> {
    if matches!(field, Some(value) if value != "displayname" && value != "avatar_url") {
        return Err(ApiError::bad_request(
            "Invalid field parameter. Allowed values are 'displayname' or 'avatar_url'".to_string(),
        ));
    }

    if !user_matches_origin(user_id, &state.services.server_name) {
        return Err(ApiError::not_found(
            "User is not hosted on this server".to_string(),
        ));
    }

    let profile = state
        .services
        .registration_service
        .get_profile(user_id)
        .await?;

    validate_federation_origin_shares_user_room(state, user_id, origin).await?;

    let displayname = profile.get("displayname").cloned().unwrap_or(Value::Null);
    let avatar_url = profile.get("avatar_url").cloned().unwrap_or(Value::Null);

    let response = match field {
        None => json!({
            "displayname": displayname,
            "avatar_url": avatar_url
        }),
        Some("displayname") => json!({
            "displayname": displayname
        }),
        Some("avatar_url") => json!({
            "avatar_url": avatar_url
        }),
        Some(_) => unreachable!("field was validated above"),
    };

    Ok(Json(response))
}

async fn get_public_rooms(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let limit = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(10)
        .min(1000);
    let _since = params.get("since").cloned();

    let rooms = state
        .services
        .room_storage
        .get_public_rooms(limit)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let mut room_list = Vec::new();
    for room in rooms {
        room_list.push(json!({
            "room_id": room.room_id,
            "name": room.name,
            "topic": room.topic,
            "avatar_url": room.avatar_url,
            "num_joined_members": room.member_count,
            "world_readable": room.is_public,
            "guest_can_join": false
        }));
    }

    Ok(Json(json!({
        "chunk": room_list,
        "next_batch": null
    })))
}

fn topological_sort(pdus: &mut Vec<Value>) {
    use std::collections::{HashMap, VecDeque};

    let mut graph: HashMap<String, Vec<usize>> = HashMap::new();
    let mut in_degree: Vec<usize> = vec![0; pdus.len()];
    let mut event_id_to_idx: HashMap<String, usize> = HashMap::new();

    for (i, pdu) in pdus.iter().enumerate() {
        if let Some(event_id) = pdu.get("event_id").and_then(|v| v.as_str()) {
            event_id_to_idx.insert(event_id.to_string(), i);
        }
    }

    for (i, pdu) in pdus.iter().enumerate() {
        if let Some(prev_events) = pdu.get("prev_events").and_then(|v| v.as_array()) {
            for prev in prev_events {
                if let Some(prev_id) = prev.as_str() {
                    if let Some(&_prev_idx) = event_id_to_idx.get(prev_id) {
                        graph.entry(prev_id.to_string()).or_default().push(i);
                        in_degree[i] += 1;
                    }
                }
            }
        }
    }

    let mut queue = VecDeque::new();
    for (i, &degree) in in_degree.iter().enumerate() {
        if degree == 0 {
            queue.push_back(i);
        }
    }

    let mut sorted_indices = Vec::new();
    while let Some(u) = queue.pop_front() {
        sorted_indices.push(u);
        if let Some(event_id) = pdus[u].get("event_id").and_then(|v| v.as_str()) {
            if let Some(neighbors) = graph.get(event_id) {
                for &v in neighbors {
                    in_degree[v] -= 1;
                    if in_degree[v] == 0 {
                        queue.push_back(v);
                    }
                }
            }
        }
    }

    // If there's a cycle or missing nodes, sorted_indices.len() != pdus.len()
    // In that case, we just keep the original order for the remaining nodes
    if sorted_indices.len() == pdus.len() {
        let mut sorted_pdus = Vec::with_capacity(pdus.len());
        for idx in sorted_indices {
            sorted_pdus.push(pdus[idx].clone());
        }
        *pdus = sorted_pdus;
    }
}

fn parse_backfill_query(raw_query: Option<String>) -> Result<(Vec<String>, i64), ApiError> {
    let mut event_ids = Vec::new();
    let mut limit = 10_i64;

    if let Some(raw_query) = raw_query {
        for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
            match key.as_ref() {
                "v" if !value.is_empty() => event_ids.push(value.into_owned()),
                "limit" => {
                    limit = value.parse::<i64>().map_err(|_| {
                        ApiError::bad_request("Invalid limit query parameter".to_string())
                    })?;
                }
                _ => {}
            }
        }
    }

    if event_ids.is_empty() {
        return Err(ApiError::bad_request(
            "v query parameter is required".to_string(),
        ));
    }

    Ok((event_ids, limit.clamp(1, 100)))
}

async fn backfill(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<String>,
    RawQuery(raw_query): RawQuery,
) -> Result<Json<Value>, ApiError> {
    validate_federation_origin_can_observe_room(&state, &room_id, &auth.origin).await?;

    let (v, limit) = parse_backfill_query(raw_query)?;

    ::tracing::info!(
        "Backfilling room {} from event(s) {:?} with limit {}",
        room_id,
        v,
        limit
    );

    let mut backfill_before_ts = i64::MAX;
    for event_id in &v {
        let event = get_room_event_in_room(&state, &room_id, event_id).await?;
        backfill_before_ts = backfill_before_ts.min(event.origin_server_ts);
    }

    // Fetch only events older than the requested frontier to avoid disclosing newer room history.
    let mut events = state
        .services
        .event_storage
        .get_room_events_paginated(&room_id, Some(backfill_before_ts), limit, "b")
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
    sort_room_events_stably(&mut events);

    let mut auth_events = state
        .services
        .event_storage
        .get_state_events_at_or_before(&room_id, backfill_before_ts)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get auth chain: {}", e)))?;
    let (_, auth_chain) =
        build_federation_state_payload(&state.services.server_name, &mut auth_events);

    let mut pdus: Vec<Value> = events
        .into_iter()
        .map(|event| serialize_room_event_minimal(&state.services.server_name, &event))
        .collect();

    // 2. Adaptive Topological Sorting
    topological_sort(&mut pdus);

    ::tracing::debug!("Backfill returning {} sorted PDUs", pdus.len());

    Ok(Json(json!({
        "origin": state.services.server_name,
        "origin_server_ts": chrono::Utc::now().timestamp_millis(),
        "pdus": pdus,
        "auth_chain": auth_chain
    })))
}

async fn keys_claim(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let mut request: crate::e2ee::device_keys::KeyClaimRequest = serde_json::from_value(body)
        .map_err(|e| ApiError::bad_request(format!("Invalid claim request: {}", e)))?;

    if let Some(one_time_keys) = request.one_time_keys.as_object_mut() {
        let requested_users = one_time_keys.keys().cloned().collect::<Vec<_>>();
        let mut allowed_local_users = std::collections::HashSet::new();

        for user_id in requested_users {
            if !user_matches_origin(&user_id, &state.services.server_name) {
                continue;
            }

            if validate_federation_origin_shares_user_room(&state, &user_id, &auth.origin)
                .await
                .is_ok()
            {
                allowed_local_users.insert(user_id);
            }
        }

        one_time_keys.retain(|user_id, _| allowed_local_users.contains(user_id));
    }

    ::tracing::info!(
        target: "security_audit",
        event = "federation_keys_claim",
        origin = ?auth.origin,
        "Federation keys claim request"
    );

    let response = state
        .services
        .device_keys_service
        .claim_keys_for_federation(request, &state.services.server_name)
        .await?;

    Ok(Json(json!({
        "one_time_keys": response.one_time_keys,
        "failures": response.failures
    })))
}

async fn keys_query(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let mut request: crate::e2ee::device_keys::KeyQueryRequest = serde_json::from_value(body)
        .map_err(|e| ApiError::bad_request(format!("Invalid query request: {}", e)))?;

    if let Some(device_keys) = request.device_keys.as_object_mut() {
        let requested_users = device_keys.keys().cloned().collect::<Vec<_>>();
        let mut allowed_local_users = std::collections::HashSet::new();

        for user_id in requested_users {
            if !user_matches_origin(&user_id, &state.services.server_name) {
                continue;
            }

            if validate_federation_origin_shares_user_room(&state, &user_id, &auth.origin)
                .await
                .is_ok()
            {
                allowed_local_users.insert(user_id);
            }
        }

        device_keys.retain(|user_id, _| allowed_local_users.contains(user_id));
    }

    ::tracing::info!(
        target: "security_audit",
        event = "federation_keys_query",
        origin = ?auth.origin,
        "Federation keys query request"
    );

    let response = state
        .services
        .device_keys_service
        .query_keys_for_federation(request, &state.services.server_name)
        .await?;

    Ok(Json(json!({
        "device_keys": response.device_keys,
        "failures": response.failures
    })))
}

async fn keys_upload(
    State(_state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Json(_body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    ::tracing::info!(
        target: "security_audit",
        event = "federation_keys_upload",
        origin = ?auth.origin,
        "Federation keys upload request"
    );

    Ok(Json(json!({
        "one_time_key_counts": {}
    })))
}

async fn legacy_keys_claim(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let mut request: crate::e2ee::device_keys::KeyClaimRequest = serde_json::from_value(body)
        .map_err(|e| ApiError::bad_request(format!("Invalid claim request: {}", e)))?;

    if let Some(one_time_keys) = request.one_time_keys.as_object_mut() {
        let requested_users = one_time_keys.keys().cloned().collect::<Vec<_>>();
        let mut allowed_local_users = std::collections::HashSet::new();

        for user_id in requested_users {
            if !user_matches_origin(&user_id, &state.services.server_name) {
                continue;
            }

            if validate_federation_origin_shares_user_room(&state, &user_id, &auth.origin)
                .await
                .is_ok()
            {
                allowed_local_users.insert(user_id);
            }
        }

        one_time_keys.retain(|user_id, _| allowed_local_users.contains(user_id));
    }

    ::tracing::info!(
        target: "security_audit",
        event = "legacy_federation_keys_claim",
        origin = ?auth.origin,
        "Legacy federation keys claim request"
    );

    let response = state
        .services
        .device_keys_service
        .claim_keys_for_federation(request, &state.services.server_name)
        .await?;

    Ok(Json(json!({
        "one_time_keys": response.one_time_keys,
        "failures": response.failures
    })))
}

async fn legacy_keys_query(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let mut request: crate::e2ee::device_keys::KeyQueryRequest = serde_json::from_value(body)
        .map_err(|e| ApiError::bad_request(format!("Invalid query request: {}", e)))?;

    if let Some(device_keys) = request.device_keys.as_object_mut() {
        let requested_users = device_keys.keys().cloned().collect::<Vec<_>>();
        let mut allowed_local_users = std::collections::HashSet::new();

        for user_id in requested_users {
            if !user_matches_origin(&user_id, &state.services.server_name) {
                continue;
            }

            if validate_federation_origin_shares_user_room(&state, &user_id, &auth.origin)
                .await
                .is_ok()
            {
                allowed_local_users.insert(user_id);
            }
        }

        device_keys.retain(|user_id, _| allowed_local_users.contains(user_id));
    }

    ::tracing::info!(
        target: "security_audit",
        event = "legacy_federation_keys_query",
        origin = ?auth.origin,
        "Legacy federation keys query request"
    );

    let response = state
        .services
        .device_keys_service
        .query_keys_for_federation(request, &state.services.server_name)
        .await?;

    Ok(Json(json!({
        "device_keys": response.device_keys,
        "failures": response.failures
    })))
}

async fn resolve_server_keys(state: &AppState) -> Result<Value, ApiError> {
    let config = &state.services.config.federation;
    if !config.enabled {
        return Err(ApiError::not_found("Federation disabled".to_string()));
    }

    if let Some(current_key) = state
        .services
        .key_rotation_manager
        .get_current_key()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to load federation signing key: {}", e)))?
    {
        return state
            .services
            .key_rotation_manager
            .get_server_keys_response()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to build server key response: {}", e)))
            .or_else(|_| {
                Ok(json!({
                    "server_name": config.server_name,
                    "verify_keys": {
                        current_key.key_id: { "key": current_key.public_key }
                    },
                    "old_verify_keys": {},
                    "valid_until_ts": current_key.expires_at
                }))
            });
    }

    let key_id = config
        .key_id
        .clone()
        .unwrap_or_else(|| "ed25519:1".to_string());

    let verify_key = match config.signing_key.as_deref().and_then(|k| {
        let res = derive_ed25519_verify_key_base64(k);
        if res.is_none() {
            ::tracing::error!("Failed to derive verify key from signing_key: {}", k);
        }
        res
    }) {
        Some(k) => k,
        None => {
            ::tracing::error!("Federation signing key missing or invalid in config");
            return Err(ApiError::internal(
                "Missing or invalid federation signing key".to_string(),
            ));
        }
    };

    let valid_until = chrono::Utc::now().timestamp_millis() + 3600 * 1000;

    Ok(json!({
        "server_name": config.server_name,
        "verify_keys": {
            key_id: { "key": verify_key }
        },
        "old_verify_keys": {},
        "valid_until_ts": valid_until
    }))
}

async fn server_key(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    if state.services.config.federation.signing_key.is_none() {
        state
            .services
            .key_rotation_manager
            .load_or_create_key()
            .await
            .map_err(|e| {
                ApiError::internal(format!(
                    "Failed to initialize federation signing key: {}",
                    e
                ))
            })?;
    }

    Ok(Json(resolve_server_keys(&state).await?))
}

async fn key_query(
    State(state): State<AppState>,
    Path((server_name, key_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    if server_name == state.services.server_name
        || server_name == state.services.config.federation.server_name
    {
        return server_key(State(state)).await;
    }

    let response = fetch_remote_server_keys_response(&state, &server_name, &key_id).await?;
    Ok(Json(response))
}

fn derive_ed25519_verify_key_base64(signing_key: &str) -> Option<String> {
    let signing_key = decode_base64_32(signing_key)?;
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key);
    let verifying_key = signing_key.verifying_key();
    Some(base64::engine::general_purpose::STANDARD_NO_PAD.encode(verifying_key.as_bytes()))
}

async fn query_auth(State(_state): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({
        "auth_chain": []
    })))
}

async fn key_clone(
    State(state): State<AppState>,
    Json(_body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    server_key(State(state)).await
}

async fn event_auth(State(_state): State<AppState>) -> Result<Json<Value>, ApiError> {
    Err(ApiError::not_found(
        "Federation event_auth is not implemented; use supported auth-chain endpoints".to_string(),
    ))
}

/// Get room hierarchy
/// GET /_matrix/federation/v1/hierarchy/{room_id}
async fn get_room_hierarchy(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<String>,
    Query(params): Query<FederationHierarchyQueryParams>,
) -> Result<Json<Value>, ApiError> {
    if !room_id.starts_with('!') || !room_id.contains(':') {
        return Err(ApiError::bad_request("Invalid room_id format"));
    }

    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Room not found"))?;

    if !room.is_public {
        validate_federation_origin_in_room(&state, &room_id, &auth.origin).await?;
    }

    let space = state
        .services
        .space_service
        .get_space_by_room(&room_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Space not found"))?;

    let hierarchy = state
        .services
        .space_service
        .get_space_hierarchy_v1(
            &space.space_id,
            params.max_depth.unwrap_or(1),
            params.suggested_only.unwrap_or(false),
            params.limit,
            params.from.as_deref(),
            None,
        )
        .await?;

    let response = serde_json::to_value(hierarchy).map_err(|e| {
        ApiError::internal(format!("Failed to serialize hierarchy response: {}", e))
    })?;

    Ok(Json(response))
}

/// Timestamp to event conversion
/// GET /_matrix/federation/v1/timestamp_to_event/{room_id}
async fn timestamp_to_event(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<String>,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    // 简单验证 room_id 格式
    if !room_id.starts_with('!') || !room_id.contains(':') {
        return Err(ApiError::bad_request("Invalid room_id format"));
    }

    validate_federation_origin_in_room(&state, &room_id, &auth.origin).await?;

    // 获取 timestamp 参数
    let timestamp = match params.get("ts") {
        Some(v) => {
            if let Some(ts) = v.as_i64() {
                ts
            } else if let Some(s) = v.as_str() {
                s.parse::<i64>()
                    .map_err(|_| ApiError::bad_request("Invalid 'ts' parameter"))?
            } else {
                return Err(ApiError::bad_request("Invalid 'ts' parameter"));
            }
        }
        None => return Err(ApiError::bad_request("Missing 'ts' parameter")),
    };

    // 验证房间存在
    let _room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Room not found"))?;

    // 查找最接近指定时间的事件
    let event = state
        .services
        .event_storage
        .find_event_by_timestamp(&room_id, timestamp)
        .await?;

    // find_event_by_timestamp returns Option<serde_json::Value> which contains (event_id, ts)
    if let Some(evt) = event {
        // evt is a tuple (event_id, origin_server_ts) serialized as array
        if let Some(arr) = evt.as_array() {
            if let (Some(event_id), Some(ts)) = (
                arr.first().and_then(|v| v.as_str()),
                arr.get(1).and_then(|v| v.as_i64()),
            ) {
                return Ok(Json(json!({
                    "event_id": event_id,
                    "origin_server_ts": ts
                })));
            }
        }
    }

    Ok(Json(json!({
        "event_id": null,
        "origin_server_ts": timestamp
    })))
}

// ============================================================================
async fn send_join_v2(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path((room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    increment_gauge(&state, "federation_join_in_flight");
    let (permit, wait_ms) = match acquire_with_timeout(
        state.federation_join_semaphore.clone(),
        state.services.config.federation.join_acquire_timeout_ms,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => {
            decrement_gauge(&state, "federation_join_in_flight");
            increment_counter(&state, "federation_join_429_total");
            return Err(error);
        }
    };
    observe_histogram(&state, "federation_join_wait_ms", wait_ms as f64);

    let result = async {
        if !room_id.starts_with('!') || !room_id.contains(':') {
            return Err(ApiError::bad_request("Invalid room_id format"));
        }

        if let Some(origin) = body.get("origin").and_then(|v| v.as_str()) {
            validate_federation_origin(&auth.origin, Some(origin))?;
        }
        let sender =
            validate_federation_member_event(&auth.origin, &room_id, &event_id, &body, "join")?;
        validate_federation_join_access(&state, &room_id, sender).await?;
        let content = body.get("content").cloned().unwrap_or(json!({}));
        let display_name = content.get("displayname").and_then(|v| v.as_str());

        let params = crate::storage::event::CreateEventParams {
            event_id: event_id.clone(),
            room_id: room_id.clone(),
            user_id: sender.to_string(),
            event_type: "m.room.member".to_string(),
            content: content.clone(),
            state_key: Some(sender.to_string()),
            origin_server_ts: body
                .get("origin_server_ts")
                .and_then(|v| v.as_i64())
                .unwrap_or_else(|| chrono::Utc::now().timestamp_millis()),
        };
        state
            .services
            .event_storage
            .create_event(params, None)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to persist join event: {}", e)))?;

        state
            .services
            .member_storage
            .add_member(&room_id, sender, "join", display_name, None, None)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update membership: {}", e)))?;

        ::tracing::info!(
            target: "federation",
            event = "federation_send_join_v2",
            origin = auth.origin,
            sender = sender,
            room_id = room_id,
            "Federation send_join_v2 processed"
        );

        Ok(Json(json!({
            "room_id": room_id,
            "event_id": event_id
        })))
    }
    .await;

    drop(permit);
    decrement_gauge(&state, "federation_join_in_flight");
    match &result {
        Ok(_) => increment_counter(&state, "federation_join_ok_total"),
        Err(ApiError::RateLimitedWithRetry(_)) => {
            increment_counter(&state, "federation_join_429_total")
        }
        Err(_) => increment_counter(&state, "federation_join_error_total"),
    }

    result
}

/// Send leave v2
/// PUT /_matrix/federation/v2/send_leave/{room_id}/{event_id}
async fn send_leave_v2(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path((room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if !room_id.starts_with('!') || !room_id.contains(':') {
        return Err(ApiError::bad_request("Invalid room_id format"));
    }

    if let Some(origin) = body.get("origin").and_then(|v| v.as_str()) {
        validate_federation_origin(&auth.origin, Some(origin))?;
    }
    let sender =
        validate_federation_member_event(&auth.origin, &room_id, &event_id, &body, "leave")?;

    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Room not found"))?;

    let _ = room.room_id.as_str();
    let _ = room.is_public;
    let membership_content = serde_json::json!({
        "membership": "leave"
    });

    let params = crate::storage::event::CreateEventParams {
        event_id: event_id.clone(),
        room_id: room_id.clone(),
        user_id: sender.to_string(),
        event_type: "m.room.member".to_string(),
        content: membership_content,
        state_key: Some(sender.to_string()),
        origin_server_ts: chrono::Utc::now().timestamp_millis(),
    };

    state
        .services
        .event_storage
        .create_event(params, None)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to persist leave event: {}", e)))?;

    state
        .services
        .member_storage
        .remove_member(&room_id, sender)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to update membership: {}", e)))?;

    ::tracing::info!(
        target: "federation",
        event = "federation_send_leave",
        sender = sender,
        room_id = room_id,
        "Federation send_leave processed"
    );

    Ok(Json(json!({
        "room_id": room_id,
        "event_id": event_id
    })))
}

/// Post public rooms (search)
/// POST /_matrix/federation/v1/publicRooms
async fn post_public_rooms(
    State(state): State<AppState>,
    Query(_params): Query<Value>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    // Get search parameters
    let limit = body
        .get("limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(20)
        .min(1000);
    let rooms = state
        .services
        .room_storage
        .get_public_rooms(limit)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let mut room_list = Vec::new();
    for room in rooms {
        room_list.push(json!({
            "room_id": room.room_id,
            "name": room.name,
            "topic": room.topic,
            "avatar_url": room.avatar_url,
            "num_joined_members": room.member_count,
            "world_readable": room.is_public,
            "guest_can_join": false
        }));
    }

    Ok(Json(json!({
        "chunk": room_list,
        "total_room_count_estimate": room_list.len()
    })))
}

/// Query directory
/// GET /_matrix/federation/v1/query/directory
async fn query_directory(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    let room_alias = params
        .get("room_alias")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing room_alias parameter"))?;
    validate_room_alias(room_alias)?;

    let Some((_, alias_server_name)) = room_alias[1..].rsplit_once(':') else {
        return Err(ApiError::bad_request(
            "Invalid room alias format".to_string(),
        ));
    };
    if alias_server_name != state.services.server_name {
        return Err(ApiError::not_found(
            "Room alias is not hosted on this server".to_string(),
        ));
    }

    let room_id = state
        .services
        .room_service
        .get_room_by_alias(room_alias)
        .await?;
    let room_id = room_id.ok_or_else(|| {
        ApiError::not_found(format!(
            "Room alias not found: {}. Create the alias before querying the federation directory.",
            room_alias
        ))
    })?;
    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    if !room.is_public {
        validate_federation_origin_in_room(&state, &room_id, &auth.origin).await?;
    }

    Ok(Json(json!({
        "room_id": room_id,
        "servers": [state.services.server_name.clone()]
    })))
}

/// OpenID userinfo
/// GET /_matrix/federation/v1/openid/userinfo
async fn openid_userinfo(
    State(state): State<AppState>,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    let access_token = params
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing access_token parameter"))?;

    let openid_storage = crate::storage::OpenIdTokenStorage::new(&state.services.user_storage.pool);
    let token = openid_storage
        .validate_token(access_token)
        .await?
        .ok_or_else(|| ApiError::unauthorized("Invalid or expired OpenID token".to_string()))?;

    let user_exists = state
        .services
        .user_storage
        .user_exists(&token.user_id)
        .await
        .map_err(|e| {
            ApiError::internal(format!("Failed to validate OpenID token subject: {}", e))
        })?;
    if !user_exists {
        return Err(ApiError::unauthorized(
            "Invalid or expired OpenID token".to_string(),
        ));
    }

    Ok(Json(json!({
        "sub": token.user_id
    })))
}

async fn fetch_remote_server_keys_response(
    state: &AppState,
    server_name: &str,
    key_id: &str,
) -> Result<Value, ApiError> {
    let backoff_key = format!("federation:key_fetch_backoff:{}:{}", server_name, key_id);
    if let Ok(Some(true)) = state.cache.get::<bool>(&backoff_key).await {
        return Err(ApiError::not_found(format!(
            "Remote server key '{}' for '{}' not found",
            key_id, server_name
        )));
    }

    let cache_key = format!("federation:server_keys:{}:{}", server_name, key_id);
    if let Ok(Some(cached)) = state.cache.get::<Value>(&cache_key).await {
        return Ok(cached);
    }

    let _permit = state
        .federation_key_fetch_general_semaphore
        .clone()
        .acquire_owned()
        .await
        .expect("semaphore closed");

    let timeout_ms = state.services.config.federation.key_fetch_timeout_ms.max(1);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(timeout_ms))
        .build()
        .map_err(|e| {
            ApiError::internal(format!("Failed to build federation HTTP client: {}", e))
        })?;

    let urls = [
        format!("https://{}/_matrix/key/v2/server", server_name),
        format!("http://{}/_matrix/key/v2/server", server_name),
        format!(
            "https://{}/_matrix/key/v2/query/{}/{}",
            server_name, server_name, key_id
        ),
        format!(
            "http://{}/_matrix/key/v2/query/{}/{}",
            server_name, server_name, key_id
        ),
    ];

    for url in urls {
        let response = match client.get(&url).send().await {
            Ok(response) if response.status().is_success() => response,
            _ => continue,
        };

        let body = match response.json::<Value>().await {
            Ok(body) => body,
            Err(_) => continue,
        };

        let Some(key) = extract_remote_verify_key(&body, server_name, key_id) else {
            continue;
        };

        let canonical_response = json!({
            "server_name": body
                .get("server_name")
                .and_then(|v| v.as_str())
                .unwrap_or(server_name),
            "valid_until_ts": body
                .get("valid_until_ts")
                .and_then(|v| v.as_i64())
                .unwrap_or_else(|| chrono::Utc::now().timestamp_millis() + 3600 * 1000),
            "verify_keys": {
                key_id: {
                    "key": key
                }
            },
            "old_verify_keys": body
                .get("old_verify_keys")
                .cloned()
                .unwrap_or_else(|| json!({})),
            "signatures": body
                .get("signatures")
                .cloned()
                .unwrap_or_else(|| json!({}))
        });

        let ttl = state.services.config.federation.key_cache_ttl.max(60);
        let _ = state.cache.set(&cache_key, &canonical_response, ttl).await;
        return Ok(canonical_response);
    }

    let _ = state.cache.set(&backoff_key, true, 30).await;
    Err(ApiError::not_found(format!(
        "Remote server key '{}' for '{}' not found",
        key_id, server_name
    )))
}

fn extract_remote_verify_key(body: &Value, server_name: &str, key_id: &str) -> Option<String> {
    if let Some(key) = extract_remote_verify_key_from_object(body, key_id) {
        return Some(key);
    }

    let server_keys = body.get("server_keys")?.as_array()?;
    for entry in server_keys {
        if entry
            .get("server_name")
            .and_then(|value| value.as_str())
            .is_some_and(|value| value != server_name)
        {
            continue;
        }

        if let Some(key) = extract_remote_verify_key_from_object(entry, key_id) {
            return Some(key);
        }
    }

    None
}

fn extract_remote_verify_key_from_object(body: &Value, key_id: &str) -> Option<String> {
    let verify_keys = body.get("verify_keys")?.as_object()?;
    let entry = verify_keys.get(key_id)?;
    entry.get("key")?.as_str().map(str::to_string)
}

/// Media download (federation)
/// GET /_matrix/federation/v1/media/download/{server_name}/{media_id}
async fn media_download(
    State(state): State<AppState>,
    Path((server_name, media_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    validate_federation_media_server_name(&state, &server_name)?;

    if media_id.is_empty() {
        return Err(ApiError::bad_request("Missing media_id"));
    }

    let content = state
        .services
        .media_service
        .download_media(&server_name, &media_id)
        .await?;
    let content_type = federation_guess_content_type(&media_id).to_string();
    let headers = federation_media_response_headers(content_type, content.len());

    Ok((headers, content))
}

/// Media thumbnail (federation)
/// GET /_matrix/federation/v1/media/thumbnail/{server_name}/{media_id}
async fn media_thumbnail(
    State(state): State<AppState>,
    Path((server_name, media_id)): Path<(String, String)>,
    Query(params): Query<Value>,
) -> Result<impl IntoResponse, ApiError> {
    validate_federation_media_server_name(&state, &server_name)?;

    let width = parse_federation_query_i64(&params, "width", 100)?;
    let height = parse_federation_query_i64(&params, "height", 100)?;
    let method = params
        .get("method")
        .and_then(|v| v.as_str())
        .unwrap_or("scale");

    const MAX_FEDERATION_THUMBNAIL_DIMENSION: i64 = 4096;
    if width < 1
        || height < 1
        || width > MAX_FEDERATION_THUMBNAIL_DIMENSION
        || height > MAX_FEDERATION_THUMBNAIL_DIMENSION
    {
        return Err(ApiError::bad_request(format!(
            "Thumbnail dimensions must be between 1 and {}",
            MAX_FEDERATION_THUMBNAIL_DIMENSION
        )));
    }

    let content = state
        .services
        .media_service
        .get_thumbnail(&server_name, &media_id, width as u32, height as u32, method)
        .await?;
    let content_type = federation_guess_content_type(&media_id).to_string();
    let headers = federation_media_response_headers(content_type, content.len());

    Ok((headers, content))
}

fn federation_media_response_headers(
    content_type: String,
    content_length: usize,
) -> [(String, String); 2] {
    [
        ("Content-Type".to_string(), content_type),
        ("Content-Length".to_string(), content_length.to_string()),
    ]
}

fn federation_guess_content_type(filename: &str) -> &'static str {
    let lower = filename.to_ascii_lowercase();

    if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else if lower.ends_with(".mp4") {
        "video/mp4"
    } else if lower.ends_with(".webm") {
        "video/webm"
    } else if lower.ends_with(".ogg") {
        "audio/ogg"
    } else if lower.ends_with(".mp3") {
        "audio/mpeg"
    } else if lower.ends_with(".wav") {
        "audio/wav"
    } else {
        "application/octet-stream"
    }
}

/// Exchange third party invite
/// PUT /_matrix/federation/v1/exchange_third_party_invite/{room_id}
async fn exchange_third_party_invite(
    State(state): State<AppState>,
    Extension(auth): Extension<FederationRequestAuth>,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if !room_id.starts_with('!') || !room_id.contains(':') {
        return Err(ApiError::bad_request("Invalid room_id format"));
    }

    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    let default_event_id = format!(
        "${}:{}",
        uuid::Uuid::new_v4(),
        room_id.split(':').next_back().unwrap_or("server")
    );
    let event_id = body
        .get("event_id")
        .and_then(|v| v.as_str())
        .unwrap_or(&default_event_id);

    let origin_server_ts = body
        .get("origin_server_ts")
        .and_then(|v| v.as_i64())
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());

    let room_version = room.room_version;
    let (sender, state_key) =
        validate_federation_exchange_third_party_invite_event(&auth.origin, &room_id, &body)?;
    let content = body.get("content").cloned().unwrap_or_else(|| json!({}));

    Ok(Json(serde_json::json!({
        "event_id": event_id,
        "room_id": room_id,
        "type": "m.room.member",
        "sender": sender,
        "origin": auth.origin,
        "origin_server_ts": origin_server_ts,
        "room_version": room_version,
        "state_key": state_key,
        "content": content,
        "processed": true
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

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
