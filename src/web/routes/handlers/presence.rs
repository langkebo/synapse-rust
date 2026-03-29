use crate::common::{ApiError, MAX_MESSAGE_LENGTH};
use crate::web::routes::{validate_presence_status, validate_user_id, AppState, AuthenticatedUser};
use axum::extract::{Json, Path, State};
use serde_json::{json, Value};

pub(crate) async fn get_presence(
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

pub(crate) async fn set_presence(
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

pub(crate) async fn presence_list(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let user_id = &auth_user.user_id;

    if let Some(subscribe) = body.get("subscribe").and_then(|v| v.as_array()) {
        for target in subscribe {
            if let Some(target_id) = target.as_str() {
                validate_user_id(target_id)?;

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
