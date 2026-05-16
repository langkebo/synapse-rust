use crate::common::ApiError;
use crate::map_internal;
use crate::storage::event::EventStorage;
use crate::storage::room::{Receipt, RoomStorage};
use crate::web::routes::{
    validate_event_id, validate_receipt_type, validate_room_id, AppState, AuthenticatedUser,
    is_joined_room_member_or_creator, ensure_room_member,
};
use super::{ensure_room_view_access, get_room_event};
use axum::{
    extract::{Json, Path, State},
};
use serde_json::{json, Value};

pub(crate) async fn send_receipt(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, receipt_type, event_id)): Path<(String, String, String)>,
    body: String,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_receipt_type(&receipt_type)?;
    let event_id = event_id.replace("%24", "$");
    validate_event_id(&event_id)?;

    ensure_room_member(
        &state,
        &auth_user,
        &room_id,
        "You must be a member of this room to send receipts",
    )
    .await?;

    get_room_event(&state.services.event_storage, &room_id, &event_id).await?;

    let body: Value = if body.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(&body).unwrap_or(json!({}))
    };

    state
        .services
        .room_storage
        .add_receipt(
            &auth_user.user_id,
            &auth_user.user_id,
            &room_id,
            &event_id,
            &receipt_type,
            &body,
        )
        .await
        .map_err(map_internal!("Failed to store receipt"))?;

    let now_ts = chrono::Utc::now().timestamp_millis();
    let mut receipt_entry = body.as_object().cloned().unwrap_or_default();
    receipt_entry.insert("ts".to_string(), json!(now_ts));
    let receipt_content = json!({
        (&event_id): {
            (&receipt_type): {
                (&auth_user.user_id): receipt_entry
            }
        }
    });
    let _ = sqlx::query(
        r#"
        INSERT INTO room_ephemeral (room_id, event_type, user_id, content, stream_id, created_ts, expires_at)
        VALUES ($1, 'm.receipt', $2, $3, $4, $5, NULL)
        ON CONFLICT (room_id, event_type, user_id) DO UPDATE
        SET content = EXCLUDED.content, stream_id = EXCLUDED.stream_id, created_ts = EXCLUDED.created_ts
        "#,
    )
    .bind(&room_id)
    .bind(&auth_user.user_id)
    .bind(&receipt_content)
    .bind(now_ts)
    .bind(now_ts)
    .execute(&*state.services.event_storage.pool)
    .await;

    let receipt_edu = serde_json::json!({
        "edu_type": "m.receipt",
        "room_id": &room_id,
        "content": receipt_content
    });
    let _ = state
        .services
        .event_broadcaster
        .broadcast_edu_to_room(
            &room_id,
            &receipt_edu,
            state
                .services
                .config
                .server
                .server_name
                .as_deref()
                .unwrap_or("localhost"),
        )
        .await;

    Ok(Json(json!({
        "room_id": room_id,
        "event_id": event_id,
        "receipt_type": receipt_type,
        "ts": chrono::Utc::now().timestamp_millis()
    })))
}

pub(crate) async fn get_receipts(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, receipt_type, event_id)): Path<(String, String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_receipt_type(&receipt_type)?;
    let event_id = event_id.replace("%24", "$");
    validate_event_id(&event_id)?;

    ensure_room_view_access(&state, &auth_user, &room_id).await?;
    get_room_event(&state.services.event_storage, &room_id, &event_id).await?;

    let receipts = state
        .services
        .room_storage
        .get_receipts(&room_id, &receipt_type, &event_id)
        .await
        .map_err(map_internal!("Failed to get receipts"))?;

    Ok(Json(build_receipts_chunk(receipts)))
}

pub fn build_receipts_chunk(receipts: Vec<Receipt>) -> Value {
    let receipt_list: Vec<Value> = receipts
        .into_iter()
        .map(|r| {
            json!({
                "user_id": r.user_id,
                "receipt_type": r.receipt_type,
                "event_id": r.event_id,
                "ts": r.ts,
                "data": r.data
            })
        })
        .collect();

    json!({ "chunk": receipt_list })
}

pub(crate) async fn set_read_markers(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await
        .map_err(map_internal!("Failed to get room"))?
        .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    let is_member = is_joined_room_member_or_creator(
        &state,
        &auth_user.user_id,
        &room_id,
        room.creator_user_id.as_deref(),
    )
    .await?;

    if !is_member {
        return Err(ApiError::forbidden(
            "You are not a member of this room".to_string(),
        ));
    }

    write_read_markers_from_body(
        &state.services.room_storage,
        &state.services.event_storage,
        &room_id,
        &auth_user.user_id,
        &body,
    )
    .await?;

    Ok(Json(json!({
        "room_id": room_id,
        "updated_ts": chrono::Utc::now().timestamp_millis()
    })))
}

pub async fn write_read_markers_from_body(
    room_storage: &RoomStorage,
    event_storage: &EventStorage,
    room_id: &str,
    user_id: &str,
    body: &Value,
) -> Result<(), ApiError> {
    if let Some(event_id) = body.get("m.fully_read").and_then(|v| v.as_str()) {
        if event_id.starts_with('$') {
            validate_event_id(event_id)?;
            get_room_event(event_storage, room_id, event_id).await?;
            room_storage
                .update_read_marker_with_type(room_id, user_id, event_id, "m.fully_read")
                .await
                .map_err(|e| {
                    ApiError::internal(format!("Failed to set fully_read marker: {e}"))
                })?;
        }
    }

    if let Some(event_id) = body.get("m.private_read").and_then(|v| v.as_str()) {
        if event_id.starts_with('$') {
            validate_event_id(event_id)?;
            get_room_event(event_storage, room_id, event_id).await?;
            room_storage
                .update_read_marker_with_type(room_id, user_id, event_id, "m.private_read")
                .await
                .map_err(|e| {
                    ApiError::internal(format!("Failed to set private_read marker: {e}"))
                })?;
        }
    }

    if let Some(marked_unread) = body.get("m.marked_unread").and_then(|v| v.as_object()) {
        if let Some(events) = marked_unread.get("events").and_then(|v| v.as_array()) {
            for event in events {
                if let Some(event_id) = event.as_str() {
                    if event_id.starts_with('$') {
                        validate_event_id(event_id)?;
                        get_room_event(event_storage, room_id, event_id).await?;
                        room_storage
                            .update_read_marker_with_type(
                                room_id,
                                user_id,
                                event_id,
                                "m.marked_unread",
                            )
                            .await
                            .map_err(|e| {
                                ApiError::internal(format!(
                                    "Failed to set marked_unread marker: {e}"
                                ))
                            })?;
                    }
                }
            }
        }
    }

    if let Some(event_id) = body.get("m.read").and_then(|v| v.as_str()) {
        if event_id.starts_with('$') {
            validate_event_id(event_id)?;
            get_room_event(event_storage, room_id, event_id).await?;
            room_storage
                .update_read_marker_with_type(room_id, user_id, event_id, "m.fully_read")
                .await
                .map_err(map_internal!("Failed to set read marker"))?;
        }
    }

    Ok(())
}
