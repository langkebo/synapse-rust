use crate::common::{ApiError, MAX_MESSAGE_LENGTH};
use crate::web::routes::response_helpers::filter_users_with_shared_rooms;
use crate::web::routes::{validate_presence_status, validate_user_id, AppState, AuthenticatedUser};
use axum::extract::{Json, Path, State};
use serde_json::{json, Value};
use std::collections::HashSet;

fn ensure_presence_access(
    auth_user: &AuthenticatedUser,
    target_user_id: &str,
) -> Result<(), ApiError> {
    if auth_user.user_id != target_user_id {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    Ok(())
}

async fn filter_visible_presence_targets(
    state: &AppState,
    current_user_id: &str,
    targets: &[String],
) -> Vec<String> {
    let allowed: HashSet<String> = filter_users_with_shared_rooms(state, current_user_id, targets)
        .await
        .into_iter()
        .collect();

    targets
        .iter()
        .filter(|target_id| allowed.contains(*target_id))
        .cloned()
        .collect()
}

pub(crate) async fn get_presence(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;
    ensure_presence_access(&auth_user, &user_id)?;

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

pub(crate) async fn set_presence(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;
    ensure_presence_access(&auth_user, &user_id)?;

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

pub(crate) async fn presence_list(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let user_id = &auth_user.user_id;

    if let Some(subscribe) = body.get("subscribe").and_then(|v| v.as_array()) {
        let mut requested_targets = Vec::new();
        for target in subscribe {
            if let Some(target_id) = target.as_str() {
                validate_user_id(target_id)?;
                requested_targets.push(target_id.to_string());
            }
        }

        let visible_targets =
            filter_visible_presence_targets(&state, user_id, &requested_targets).await;

        for target_id in visible_targets {
            if let Err(e) = state
                .services
                .presence_storage
                .add_subscription(user_id, &target_id)
                .await
            {
                ::tracing::warn!("Failed to add presence subscription: {}", e);
            }
        }
    }

    if let Some(unsubscribe) = body.get("unsubscribe").and_then(|v| v.as_array()) {
        for target in unsubscribe {
            if let Some(target_id) = target.as_str() {
                validate_user_id(target_id)?;

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

    let subscriptions = state
        .services
        .presence_storage
        .get_subscriptions(user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get subscriptions: {}", e)))?;
    let subscriptions = filter_visible_presence_targets(&state, user_id, &subscriptions).await;

    let presence_batch = state
        .services
        .presence_storage
        .get_presence_batch(&subscriptions)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get presence batch: {}", e)))?;

    let mut presences = Vec::new();

    for (target_id, presence, status_msg) in presence_batch {
        let last_active_ago = if presence != "offline" { Some(0) } else { None };

        presences.push(json!({
            "user_id": target_id,
            "presence": presence,
            "status_msg": status_msg,
            "last_active_ago": last_active_ago
        }));
    }

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

pub(crate) async fn get_presence_list(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    ensure_presence_access(&auth_user, &user_id)?;

    let subscriptions = state
        .services
        .presence_storage
        .get_subscriptions(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get subscriptions: {}", e)))?;
    let subscriptions = filter_visible_presence_targets(&state, &user_id, &subscriptions).await;

    let presence_batch = state
        .services
        .presence_storage
        .get_presence_batch(&subscriptions)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get presence batch: {}", e)))?;

    let mut presences = Vec::new();

    for (target_id, presence, status_msg) in presence_batch {
        let last_active_ago = if presence != "offline" { Some(0) } else { None };

        presences.push(json!({
            "user_id": target_id,
            "presence": presence,
            "status_msg": status_msg,
            "last_active_ago": last_active_ago
        }));
    }

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
