use crate::common::{ApiError, PresenceState, MAX_MESSAGE_LENGTH};
use crate::web::routes::response_helpers::filter_users_with_shared_rooms;
use crate::web::routes::{validate_user_id, AppState, AuthenticatedUser};
use crate::web::utils::auth::resolve_request_id;
use axum::{
    extract::{Json, Path, State},
    http::HeaderMap,
};
use serde_json::{json, Value};
use std::collections::HashSet;

/// 把 `last_active_ts`（绝对时间戳，ms）换算为：
/// - `last_active_ago`：距离现在的毫秒数（presence != offline 时有意义）
/// - `currently_active`：是否在近 5 分钟内有活动（presence 为 online 时才可能 true）
fn derive_activity(presence: &PresenceState, last_active_ts: Option<i64>) -> (Option<i64>, Option<bool>) {
    let now = chrono::Utc::now().timestamp_millis();
    presence.derive_activity(last_active_ts, now)
}

fn ensure_presence_access(auth_user: &AuthenticatedUser, target_user_id: &str) -> Result<(), ApiError> {
    if auth_user.user_id != target_user_id {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    Ok(())
}

async fn ensure_presence_access_or_shared_room(
    state: &AppState,
    auth_user: &AuthenticatedUser,
    target_user_id: &str,
) -> Result<(), ApiError> {
    if auth_user.user_id == target_user_id {
        return Ok(());
    }

    let shared =
        state.services.rooms.room_service.membership.share_common_room(&auth_user.user_id, target_user_id).await?;

    if !shared {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    Ok(())
}

async fn filter_visible_presence_targets(state: &AppState, current_user_id: &str, targets: &[String]) -> Vec<String> {
    let allowed = filter_users_with_shared_rooms(state, current_user_id, targets).await;

    targets.iter().filter(|target_id| allowed.contains(*target_id)).cloned().collect()
}

pub(crate) async fn get_presence(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;
    ensure_presence_access_or_shared_room(&state, &auth_user, &user_id).await?;

    state.services.account.account_identity_service.ensure_active_user_exists(&user_id).await?;

    let presence = state.services.account.presence_service.get_presence_with_meta(&user_id).await?;

    match presence {
        Some((presence_state, status_msg, last_active_ts)) => {
            let presence_enum = PresenceState::from(presence_state.as_str());
            let (last_active_ago, currently_active) = derive_activity(&presence_enum, last_active_ts);
            Ok(Json(json!({
                "presence": presence_state,
                "status_msg": status_msg,
                "last_active_ago": last_active_ago,
                "currently_active": currently_active,
            })))
        }
        _ => Ok(Json(json!({
            "presence": PresenceState::Offline.to_string(),
            "status_msg": Option::<String>::None,
            "last_active_ago": Option::<i64>::None,
            "currently_active": Option::<bool>::None,
        }))),
    }
}

pub(crate) async fn set_presence(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;
    ensure_presence_access(&auth_user, &user_id)?;

    let presence_str = body
        .get("presence")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Presence required".to_string()))?;

    let presence_state = PresenceState::from_str_opt(presence_str).ok_or_else(|| {
        ApiError::invalid_input(format!(
            "Invalid presence status. Must be one of: {}",
            PresenceState::valid_strs().join(", ")
        ))
    })?;

    let status_msg = body.get("status_msg").and_then(|v| v.as_str());

    if let Some(msg) = status_msg {
        if msg.len() > MAX_MESSAGE_LENGTH {
            return Err(ApiError::bad_request(format!(
                "Status message too long (max {MAX_MESSAGE_LENGTH} characters)"
            )));
        }
    }

    state.services.account.presence_service.set_presence(&user_id, presence_state.as_str(), status_msg).await?;

    Ok(Json(json!({})))
}

pub(crate) async fn presence_list(
    State(state): State<AppState>,
    headers: HeaderMap,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let request_id = resolve_request_id(&headers);
    let user_id = &auth_user.user_id;

    if let Some(subscribe) = body.get("subscribe").and_then(|v| v.as_array()) {
        let mut requested_targets = Vec::new();
        for target in subscribe {
            if let Some(target_id) = target.as_str() {
                validate_user_id(target_id)?;
                requested_targets.push(target_id.to_string());
            }
        }

        let visible_targets = filter_visible_presence_targets(&state, user_id, &requested_targets).await;

        for target_id in visible_targets {
            if let Err(e) = state.services.account.presence_service.add_subscription(user_id, &target_id).await {
                ::tracing::warn!(
                    request_id = %request_id,
                    user_id = %user_id,
                    target_user_id = %target_id,
                    error = %e,
                    "Failed to add presence subscription"
                );
            }
        }
    }

    if let Some(unsubscribe) = body.get("unsubscribe").and_then(|v| v.as_array()) {
        for target in unsubscribe {
            if let Some(target_id) = target.as_str() {
                validate_user_id(target_id)?;

                if let Err(e) = state.services.account.presence_service.remove_subscription(user_id, target_id).await {
                    ::tracing::warn!(
                        request_id = %request_id,
                        user_id = %user_id,
                        target_user_id = %target_id,
                        error = %e,
                        "Failed to remove presence subscription"
                    );
                }
            }
        }
    }

    let subscriptions = state.services.account.presence_service.get_subscriptions(user_id).await?;
    let subscriptions = filter_visible_presence_targets(&state, user_id, &subscriptions).await;

    let presence_batch = state.services.account.presence_service.get_presence_batch_with_meta(&subscriptions).await?;

    let mut presences = Vec::new();

    for (target_id, presence_state, status_msg, last_active_ts) in presence_batch {
        let presence_enum = PresenceState::from(presence_state.as_str());
        let (last_active_ago, currently_active) = derive_activity(&presence_enum, last_active_ts);

        presences.push(json!({
            "user_id": target_id,
            "presence": presence_state.to_string(),
            "status_msg": status_msg,
            "last_active_ago": last_active_ago,
            "currently_active": currently_active,
        }));
    }

    let present_user_ids: HashSet<String> =
        presences.iter().filter_map(|p| p["user_id"].as_str().map(String::from)).collect();

    for target_id in &subscriptions {
        if !present_user_ids.contains(target_id.as_str()) {
            presences.push(json!({
                "user_id": target_id,
                "presence": PresenceState::Offline.to_string(),
                "status_msg": None::<String>,
                "last_active_ago": None::<i64>,
                "currently_active": None::<bool>,
            }));
        }
    }

    Ok(Json(json!({
        "presences": presences
    })))
}

pub(crate) async fn get_presence_list_no_path(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let user_id = &auth_user.user_id;

    let subscriptions = state.services.account.presence_service.get_subscriptions(user_id).await?;
    let subscriptions = filter_visible_presence_targets(&state, user_id, &subscriptions).await;

    let presence_batch = state.services.account.presence_service.get_presence_batch_with_meta(&subscriptions).await?;

    let mut presences = Vec::new();

    for (target_id, presence_state, status_msg, last_active_ts) in presence_batch {
        let presence_enum = PresenceState::from(presence_state.as_str());
        let (last_active_ago, currently_active) = derive_activity(&presence_enum, last_active_ts);
        presences.push(json!({
            "user_id": target_id,
            "presence": presence_state.to_string(),
            "status_msg": status_msg,
            "last_active_ago": last_active_ago,
            "currently_active": currently_active,
        }));
    }

    let present_user_ids: HashSet<String> =
        presences.iter().filter_map(|p| p["user_id"].as_str().map(String::from)).collect();

    for target_id in &subscriptions {
        if !present_user_ids.contains(target_id.as_str()) {
            presences.push(json!({
                "user_id": target_id,
                "presence": PresenceState::Offline.to_string(),
                "status_msg": None::<String>,
                "last_active_ago": None::<i64>,
                "currently_active": None::<bool>,
            }));
        }
    }

    Ok(Json(json!({
        "presences": presences
    })))
}

pub(crate) async fn get_presence_list(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;
    ensure_presence_access(&auth_user, &user_id)?;

    let subscriptions = state.services.account.presence_service.get_subscriptions(&user_id).await?;
    let subscriptions = filter_visible_presence_targets(&state, &user_id, &subscriptions).await;

    let presence_batch = state.services.account.presence_service.get_presence_batch_with_meta(&subscriptions).await?;

    let mut presences = Vec::new();

    for (target_id, presence_state, status_msg, last_active_ts) in presence_batch {
        let presence_enum = PresenceState::from(presence_state.as_str());
        let (last_active_ago, currently_active) = derive_activity(&presence_enum, last_active_ts);

        presences.push(json!({
            "user_id": target_id,
            "presence": presence_state.to_string(),
            "status_msg": status_msg,
            "last_active_ago": last_active_ago,
            "currently_active": currently_active,
        }));
    }

    let present_user_ids: HashSet<String> =
        presences.iter().filter_map(|p| p["user_id"].as_str().map(String::from)).collect();

    for target_id in &subscriptions {
        if !present_user_ids.contains(target_id.as_str()) {
            presences.push(json!({
                "user_id": target_id,
                "presence": PresenceState::Offline.to_string(),
                "status_msg": None::<String>,
                "last_active_ago": None::<i64>,
                "currently_active": None::<bool>,
            }));
        }
    }

    Ok(Json(json!({
        "presences": presences
    })))
}
