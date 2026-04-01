use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::common::error::ApiError;
use crate::web::routes::{AppState, AuthenticatedUser};

#[derive(Debug, Deserialize)]
pub struct PinRequest {
    pub event_id: String,
}

#[derive(Debug, Serialize)]
pub struct PinnedEventsResponse {
    pub pinned_events: Vec<String>,
}

pub async fn get_pinned_events(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<PinnedEventsResponse>, ApiError> {
    let pinned: Option<String> = sqlx::query_scalar(
        r#"
        SELECT content FROM events
        WHERE room_id = $1 AND event_type = 'm.room.pinned_events' AND state_key = ''
        ORDER BY origin_server_ts DESC
        LIMIT 1
        "#,
    )
    .bind(&room_id)
    .fetch_optional(&*state.services.event_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let pinned_list: Vec<String> = match pinned {
        Some(p) => serde_json::from_str(&p).unwrap_or_default(),
        None => vec![],
    };

    Ok(Json(PinnedEventsResponse {
        pinned_events: pinned_list,
    }))
}

pub async fn pin_event(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<PinRequest>,
) -> Result<Json<Value>, ApiError> {
    let now = chrono::Utc::now().timestamp_millis();
    let event_id = format!("${}", uuid::Uuid::new_v4());

    let existing_pinned: Option<String> = sqlx::query_scalar(
        r#"
        SELECT content FROM events
        WHERE room_id = $1 AND event_type = 'm.room.pinned_events' AND state_key = ''
        ORDER BY origin_server_ts DESC
        LIMIT 1
        "#,
    )
    .bind(&room_id)
    .fetch_optional(&*state.services.event_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let mut pinned_list: Vec<String> = match existing_pinned {
        Some(p) => serde_json::from_str(&p).unwrap_or_default(),
        None => vec![],
    };

    if !pinned_list.contains(&body.event_id) {
        pinned_list.push(body.event_id.clone());
    }

    sqlx::query(
        r#"
        INSERT INTO events (event_id, room_id, user_id, event_type, content, origin_server_ts, sender, state_key)
        VALUES ($1, $2, $3, 'm.room.pinned_events', $4, $5, $6, '')
        ON CONFLICT (event_id) DO NOTHING
        "#,
    )
    .bind(&event_id)
    .bind(&room_id)
    .bind(&auth_user.user_id)
    .bind(serde_json::json!({ "pinned_events": pinned_list }))
    .bind(now)
    .bind(&auth_user.user_id)
    .execute(&*state.services.event_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(serde_json::json!({
        "pinned_event": body.event_id
    })))
}

pub async fn unpin_event(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let now = chrono::Utc::now().timestamp_millis();
    let new_event_id = format!("${}", uuid::Uuid::new_v4());

    let existing_pinned: Option<String> = sqlx::query_scalar(
        r#"
        SELECT content FROM events
        WHERE room_id = $1 AND event_type = 'm.room.pinned_events' AND state_key = ''
        ORDER BY origin_server_ts DESC
        LIMIT 1
        "#,
    )
    .bind(&room_id)
    .fetch_optional(&*state.services.event_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let mut pinned_list: Vec<String> = match existing_pinned {
        Some(p) => serde_json::from_str(&p).unwrap_or_default(),
        None => vec![],
    };

    pinned_list.retain(|e| e != &event_id);

    sqlx::query(
        r#"
        INSERT INTO events (event_id, room_id, user_id, event_type, content, origin_server_ts, sender, state_key)
        VALUES ($1, $2, $3, 'm.room.pinned_events', $4, $5, $6, '')
        ON CONFLICT (event_id) DO NOTHING
        "#,
    )
    .bind(&new_event_id)
    .bind(&room_id)
    .bind(&auth_user.user_id)
    .bind(serde_json::json!({ "pinned_events": pinned_list }))
    .bind(now)
    .bind(&auth_user.user_id)
    .execute(&*state.services.event_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(serde_json::json!({
        "unpinned_event": event_id
    })))
}
