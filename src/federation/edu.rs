//! Unified EDU (Ephemeral Data Unit) dispatch for inbound federation transactions.
//!
//! Pure types (`EduType`, `EduProcessResult`, `user_matches_origin`) live in
//! `synapse_federation::edu`. This module provides the dispatcher and handlers
//! that depend on `FederationContext` and the service container.

pub use synapse_federation::edu::{user_matches_origin, EduProcessResult, EduType, UnknownEduType};

use crate::web::routes::context::FederationContext;
use serde_json::Value;
use std::str::FromStr;

fn increment_counter(ctx: &FederationContext, name: &str) {
    if let Some(counter) = ctx.metrics.get_counter(name) {
        counter.inc();
    } else {
        ctx.metrics.register_counter(name.to_string()).inc();
    }
}

fn increment_counter_by(ctx: &FederationContext, name: &str, delta: u64) {
    if let Some(counter) = ctx.metrics.get_counter(name) {
        counter.inc_by(delta);
    } else {
        ctx.metrics.register_counter(name.to_string()).inc_by(delta);
    }
}

async fn set_presence_backoff(ctx: &FederationContext, origin: &str) {
    let until = chrono::Utc::now().timestamp_millis() + ctx.config.federation.inbound_presence_backoff_ms as i64;
    let mut guard = ctx.federation_presence_backoff_until.write().await;
    guard.insert(origin.to_string(), until);
}

// ---------------------------------------------------------------------------
// Per-type processing functions
// ---------------------------------------------------------------------------

async fn handle_presence_edu(ctx: &FederationContext, origin: &str, edu: &Value, remaining: usize) -> EduProcessResult {
    let Some(push) = edu.get("content").and_then(|c| c.get("push")).and_then(|v| v.as_array()) else {
        increment_counter(ctx, "federation_inbound_presence_dropped_total");
        return EduProcessResult::default();
    };

    let mut result = EduProcessResult::default();

    for update in push.iter().take(remaining) {
        let Some(user_id) = update.get("user_id").and_then(|v| v.as_str()) else {
            result.dropped += 1;
            continue;
        };

        if !user_matches_origin(user_id, origin) {
            result.dropped += 1;
            continue;
        }

        let presence_str = update.get("presence").and_then(|v| v.as_str()).unwrap_or("online");
        let presence =
            crate::common::PresenceState::from_str_opt(presence_str).unwrap_or(crate::common::PresenceState::Online);
        let status_msg = update.get("status_msg").and_then(|v| v.as_str());

        let exists = match ctx.user_storage.user_exists(user_id).await {
            Ok(exists) => exists,
            Err(error) => {
                ::tracing::warn!("Failed to validate presence user {} from {}: {}", user_id, origin, error);
                result.errored += 1;
                set_presence_backoff(ctx, origin).await;
                break;
            }
        };

        if !exists {
            result.dropped += 1;
            continue;
        }

        if let Err(error) = ctx.presence_storage.set_presence(user_id, presence.as_str(), status_msg).await {
            ::tracing::warn!("Failed to persist presence update for {} from {}: {}", user_id, origin, error);
            result.errored += 1;
            set_presence_backoff(ctx, origin).await;
            break;
        }

        result.processed += 1;
    }

    if result.processed > 0 {
        increment_counter_by(ctx, "federation_inbound_presence_processed_total", result.processed as u64);
    }
    if result.dropped > 0 {
        increment_counter_by(ctx, "federation_inbound_presence_dropped_total", result.dropped as u64);
    }
    if result.errored > 0 {
        increment_counter_by(ctx, "federation_inbound_presence_error_total", result.errored as u64);
    }

    result
}

async fn handle_typing_edu(ctx: &FederationContext, origin: &str, edu: &Value, _remaining: usize) -> EduProcessResult {
    let room_id = match edu.get("room_id").and_then(|v| v.as_str()) {
        Some(r) => r,
        None => {
            ::tracing::debug!("Dropping m.typing EDU from {} without room_id", origin);
            return EduProcessResult { dropped: 1, ..Default::default() };
        }
    };

    let user_ids = edu
        .get("content")
        .and_then(|c| c.get("user_ids"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .filter(|uid| user_matches_origin(uid, origin))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if user_ids.is_empty() {
        ::tracing::debug!("No valid user_ids in m.typing EDU from {} for room {}", origin, room_id);
        return EduProcessResult { dropped: 1, ..Default::default() };
    }

    let mut result = EduProcessResult::default();
    for user_id in &user_ids {
        match ctx.presence_storage.set_typing(room_id, user_id, true).await {
            Ok(()) => result.processed += 1,
            Err(e) => {
                ::tracing::warn!("Failed to persist typing EDU for {} in {} from {}: {}", user_id, room_id, origin, e);
                result.errored += 1;
            }
        }
    }

    if result.processed > 0 {
        increment_counter_by(ctx, "federation_inbound_typing_processed_total", result.processed as u64);
    }

    result
}

async fn handle_device_list_update_edu(
    ctx: &FederationContext,
    origin: &str,
    edu: &Value,
    _remaining: usize,
) -> EduProcessResult {
    let content = match edu.get("content") {
        Some(c) => c,
        None => {
            ::tracing::debug!("Dropping m.device_list_update EDU from {} without content", origin);
            return EduProcessResult { dropped: 1, ..Default::default() };
        }
    };

    let user_id = match content.get("user_id").and_then(|v| v.as_str()) {
        Some(uid) => uid,
        None => {
            ::tracing::debug!("Dropping m.device_list_update EDU from {} without user_id", origin);
            return EduProcessResult { dropped: 1, ..Default::default() };
        }
    };

    if !user_matches_origin(user_id, origin) {
        ::tracing::debug!("Dropping m.device_list_update EDU: user_id {} does not match origin {}", user_id, origin);
        return EduProcessResult { dropped: 1, ..Default::default() };
    }

    let device_id = content.get("device_id").and_then(|v| v.as_str());

    let stream_id =
        content.get("stream_id").and_then(|v| v.as_i64()).unwrap_or_else(|| chrono::Utc::now().timestamp_millis());

    let change_type =
        if content.get("deleted").and_then(|v| v.as_bool()).unwrap_or(false) { "deleted" } else { "updated" };

    let result = ctx.device_storage.insert_device_list_change(user_id, device_id, change_type, stream_id).await;

    match result {
        Ok(_) => {
            ::tracing::debug!(
                "Processed m.device_list_update EDU for user {} device {:?} from {}",
                user_id,
                device_id,
                origin
            );
            increment_counter(ctx, "federation_inbound_device_list_update_processed_total");
            EduProcessResult { processed: 1, ..Default::default() }
        }
        Err(e) => {
            ::tracing::warn!("Failed to persist m.device_list_update EDU for {} from {}: {}", user_id, origin, e);
            increment_counter(ctx, "federation_inbound_device_list_update_error_total");
            EduProcessResult { errored: 1, ..Default::default() }
        }
    }
}

const MAX_FEDERATION_TO_DEVICE_RECIPIENTS: usize = 5000;
const MAX_FEDERATION_TO_DEVICE_MSG_BYTES: usize = 64 * 1024;

async fn handle_direct_to_device_edu(
    ctx: &FederationContext,
    origin: &str,
    edu: &Value,
    _remaining: usize,
) -> EduProcessResult {
    let sender = match edu.get("sender").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => {
            ::tracing::debug!("Dropping m.direct_to_device EDU from {} without sender", origin);
            return EduProcessResult { dropped: 1, ..Default::default() };
        }
    };

    if !user_matches_origin(sender, origin) {
        ::tracing::debug!("Dropping m.direct_to_device EDU: sender {} does not match origin {}", sender, origin);
        return EduProcessResult { dropped: 1, ..Default::default() };
    }

    let event_type = edu.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if event_type.is_empty() {
        ::tracing::debug!("Dropping m.direct_to_device EDU from {} without type", origin);
        return EduProcessResult { dropped: 1, ..Default::default() };
    }

    let messages = match edu.get("content").and_then(|c| c.get("messages")) {
        Some(m) => m,
        None => {
            ::tracing::debug!("Dropping m.direct_to_device EDU from {} without content.messages", origin);
            return EduProcessResult { dropped: 1, ..Default::default() };
        }
    };

    let mut result = EduProcessResult::default();
    let mut recipient_count: usize = 0;

    if let Some(msg_map) = messages.as_object() {
        for (recipient_user_id, device_map) in msg_map {
            if let Some(devices) = device_map.as_object() {
                for (recipient_device_id, content) in devices {
                    recipient_count += 1;
                    if recipient_count > MAX_FEDERATION_TO_DEVICE_RECIPIENTS {
                        ::tracing::warn!(
                            origin = origin,
                            sender = sender,
                            limit = MAX_FEDERATION_TO_DEVICE_RECIPIENTS,
                            "m.direct_to_device EDU exceeded recipient limit, truncating"
                        );
                        result.dropped += 1;
                        continue;
                    }

                    let msg_size = serde_json::to_string(content).map(|s| s.len()).unwrap_or(0);
                    if msg_size > MAX_FEDERATION_TO_DEVICE_MSG_BYTES {
                        ::tracing::warn!(
                            origin = origin,
                            sender = sender,
                            size = msg_size,
                            limit = MAX_FEDERATION_TO_DEVICE_MSG_BYTES,
                            "m.direct_to_device EDU message exceeds size limit, dropping"
                        );
                        result.dropped += 1;
                        continue;
                    }

                    match ctx
                        .to_device_service
                        .send_messages(
                            sender,
                            "",
                            event_type,
                            None,
                            &serde_json::json!({
                                recipient_user_id: { recipient_device_id: content }
                            }),
                        )
                        .await
                    {
                        Ok(()) => result.processed += 1,
                        Err(e) => {
                            ::tracing::warn!(
                                "Failed to persist m.direct_to_device EDU for {}:{} from {}: {}",
                                recipient_user_id,
                                recipient_device_id,
                                origin,
                                e
                            );
                            result.errored += 1;
                        }
                    }
                }
            }
        }
    }

    if result.processed > 0 {
        increment_counter_by(ctx, "federation_inbound_direct_to_device_processed_total", result.processed as u64);
    }
    if result.dropped > 0 {
        increment_counter_by(ctx, "federation_inbound_direct_to_device_dropped_total", result.dropped as u64);
    }
    if result.errored > 0 {
        increment_counter_by(ctx, "federation_inbound_direct_to_device_error_total", result.errored as u64);
    }

    result
}

// ---------------------------------------------------------------------------
// EduDispatcher — routes inbound EDUs to the correct handler
// ---------------------------------------------------------------------------

pub struct EduDispatcher;

impl EduDispatcher {
    pub async fn dispatch(
        ctx: &FederationContext,
        origin: &str,
        edu: &Value,
        remaining: usize,
    ) -> Option<EduProcessResult> {
        let edu_type_str = edu.get("edu_type").and_then(|v| v.as_str()).unwrap_or("");
        let edu_type = EduType::from_str(edu_type_str).ok()?;

        let result = match edu_type {
            EduType::Presence => handle_presence_edu(ctx, origin, edu, remaining).await,
            EduType::Typing => handle_typing_edu(ctx, origin, edu, remaining).await,
            EduType::DeviceListUpdate => handle_device_list_update_edu(ctx, origin, edu, remaining).await,
            EduType::DirectToDevice => handle_direct_to_device_edu(ctx, origin, edu, remaining).await,
        };

        Some(result)
    }
}
