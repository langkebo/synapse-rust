//! Unified EDU (Ephemeral Data Unit) dispatch for inbound federation transactions.
//!
//! Pure types (`EduType`, `EduProcessResult`, `user_matches_origin`) live in
//! `synapse_federation::edu`. This module provides the dispatcher and handlers
//! that depend on `FederationContext` and the service container.

pub use synapse_federation::edu::{user_matches_origin, EduProcessResult, EduType, UnknownEduType};

use crate::web::routes::context::FederationContext;
use serde_json::Value;
use std::str::FromStr;
use synapse_common::current_timestamp_millis;
use synapse_e2ee::cross_signing::models::CrossSigningKey;

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
    let until = current_timestamp_millis() + ctx.config.federation.inbound_presence_backoff_ms as i64;
    let mut guard = ctx.federation_presence_backoff_until.write().await;
    guard.insert(origin.to_string(), until);
}

// ---------------------------------------------------------------------------
// Per-type processing functions
// ---------------------------------------------------------------------------

async fn handle_presence_edu(ctx: &FederationContext, origin: &str, edu: &Value, remaining: usize) -> EduProcessResult {
    let Some(push) = edu.get("content").and_then(|c| c.get("push")).and_then(|v| v.as_array()) else {
        ::tracing::debug!("Dropping m.presence EDU from {} without push content", origin);
        increment_counter(ctx, "federation_inbound_presence_dropped_total");
        return EduProcessResult { dropped: 1, ..Default::default() };
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

        let exists = match ctx.user_service.user_exists(user_id).await {
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
            increment_counter(ctx, "federation_inbound_typing_dropped_total");
            return EduProcessResult { dropped: 1, ..Default::default() };
        }
    };

    // MSC4163: enforce m.room.server_acl on room-scoped EDUs (typing).
    // If the origin server is denied by the room's ACL (or the ACL is
    // malformed — fail-closed), silently drop the EDU.
    if !crate::web::routes::federation::is_server_allowed_by_room_acl(ctx, room_id, origin).await {
        ::tracing::info!(
            room_id = %room_id,
            origin = %origin,
            "Dropping m.typing EDU from origin denied by room ACL (MSC4163)"
        );
        increment_counter_by(ctx, "federation_inbound_edu_acl_denied_total", 1);
        return EduProcessResult { dropped: 1, ..Default::default() };
    }

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
        increment_counter(ctx, "federation_inbound_typing_dropped_total");
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
    if result.errored > 0 {
        increment_counter_by(ctx, "federation_inbound_typing_error_total", result.errored as u64);
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
            increment_counter(ctx, "federation_inbound_device_list_update_dropped_total");
            return EduProcessResult { dropped: 1, ..Default::default() };
        }
    };

    let user_id = match content.get("user_id").and_then(|v| v.as_str()) {
        Some(uid) => uid,
        None => {
            ::tracing::debug!("Dropping m.device_list_update EDU from {} without user_id", origin);
            increment_counter(ctx, "federation_inbound_device_list_update_dropped_total");
            return EduProcessResult { dropped: 1, ..Default::default() };
        }
    };

    if !user_matches_origin(user_id, origin) {
        ::tracing::debug!("Dropping m.device_list_update EDU: user_id {} does not match origin {}", user_id, origin);
        increment_counter(ctx, "federation_inbound_device_list_update_dropped_total");
        return EduProcessResult { dropped: 1, ..Default::default() };
    }

    let device_id = content.get("device_id").and_then(|v| v.as_str());

    let stream_id = content.get("stream_id").and_then(|v| v.as_i64()).unwrap_or_else(current_timestamp_millis);

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
            increment_counter(ctx, "federation_inbound_direct_to_device_dropped_total");
            return EduProcessResult { dropped: 1, ..Default::default() };
        }
    };

    if !user_matches_origin(sender, origin) {
        ::tracing::debug!("Dropping m.direct_to_device EDU: sender {} does not match origin {}", sender, origin);
        increment_counter(ctx, "federation_inbound_direct_to_device_dropped_total");
        return EduProcessResult { dropped: 1, ..Default::default() };
    }

    let event_type = edu.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if event_type.is_empty() {
        ::tracing::debug!("Dropping m.direct_to_device EDU from {} without type", origin);
        increment_counter(ctx, "federation_inbound_direct_to_device_dropped_total");
        return EduProcessResult { dropped: 1, ..Default::default() };
    }

    let messages = match edu.get("content").and_then(|c| c.get("messages")) {
        Some(m) => m,
        None => {
            ::tracing::debug!("Dropping m.direct_to_device EDU from {} without content.messages", origin);
            increment_counter(ctx, "federation_inbound_direct_to_device_dropped_total");
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

/// Handles an inbound `m.receipt` EDU from a federated peer.
///
/// Matrix spec: `content` is a map of room_id → receipt_type → user_id →
/// `{ event_ids: [string], data: { ts: int } }`. We iterate and call
/// `MessagingService::process_federation_receipt` for each receipt entry.
async fn handle_receipt_edu(ctx: &FederationContext, origin: &str, edu: &Value, _remaining: usize) -> EduProcessResult {
    let content = match edu.get("content").and_then(|c| c.as_object()) {
        Some(c) => c,
        None => {
            ::tracing::debug!("Dropping m.receipt EDU from {} without content", origin);
            increment_counter(ctx, "federation_inbound_receipt_dropped_total");
            return EduProcessResult { dropped: 1, ..Default::default() };
        }
    };

    let mut result = EduProcessResult::default();
    let messaging = ctx.room_service.messaging();

    for (room_id, room_receipts) in content {
        let Some(receipt_map) = room_receipts.as_object() else {
            result.dropped += 1;
            continue;
        };

        for (receipt_type, user_receipts) in receipt_map {
            let Some(users) = user_receipts.as_object() else {
                result.dropped += 1;
                continue;
            };

            for (user_id, user_receipt) in users {
                if !user_matches_origin(user_id, origin) {
                    result.dropped += 1;
                    continue;
                }

                let event_ids = match user_receipt.get("event_ids").and_then(|v| v.as_array()) {
                    Some(arr) => arr,
                    None => {
                        result.dropped += 1;
                        continue;
                    }
                };

                let body = user_receipt.clone();
                for event_id_value in event_ids {
                    let Some(event_id) = event_id_value.as_str() else {
                        result.dropped += 1;
                        continue;
                    };

                    match messaging.process_federation_receipt(room_id, user_id, receipt_type, event_id, &body).await {
                        Ok(()) => result.processed += 1,
                        Err(e) => {
                            ::tracing::warn!(
                                "Failed to persist federated receipt for {} in {} from {}: {}",
                                user_id,
                                room_id,
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
        increment_counter_by(ctx, "federation_inbound_receipt_processed_total", result.processed as u64);
    }
    if result.dropped > 0 {
        increment_counter_by(ctx, "federation_inbound_receipt_dropped_total", result.dropped as u64);
    }
    if result.errored > 0 {
        increment_counter_by(ctx, "federation_inbound_receipt_error_total", result.errored as u64);
    }

    result
}

/// Handles an inbound `m.signing_key_update` EDU from a federated peer.
///
/// Matrix spec: `content` is a map of `user_id` → `{ master_key, self_signing_key, user_signing_key }`.
/// Each value contains the updated base64-encoded public cross-signing keys for that user.
/// We store/update them via `CrossSigningService` and notify local device keys.
async fn handle_signing_key_update_edu(
    ctx: &FederationContext,
    origin: &str,
    edu: &Value,
    _remaining: usize,
) -> EduProcessResult {
    let content = match edu.get("content").and_then(|c| c.as_object()) {
        Some(c) => c,
        None => {
            ::tracing::debug!("Dropping m.signing_key_update EDU from {} without content", origin);
            increment_counter(ctx, "federation_inbound_signing_key_dropped_total");
            return EduProcessResult { dropped: 1, ..Default::default() };
        }
    };

    let mut result = EduProcessResult::default();
    let cs_service = &ctx.cross_signing_service;

    for (user_id, keys_value) in content {
        if !user_matches_origin(user_id, origin) {
            ::tracing::debug!(
                "Ignoring m.signing_key_update for user {} from origin {} (origin mismatch)",
                user_id,
                origin
            );
            result.dropped += 1;
            continue;
        }

        let keys_obj = match keys_value.as_object() {
            Some(obj) => obj,
            None => {
                result.dropped += 1;
                continue;
            }
        };

        // Each key type is a base64-encoded ED25519 public key (32 bytes → ~43 chars)
        for (key_type, key_b64) in keys_obj {
            let key_str = match key_b64.as_str() {
                Some(s) => s,
                None => {
                    result.dropped += 1;
                    continue;
                }
            };

            // Validate key type
            if !matches!(key_type.as_str(), "master_key" | "self_signing_key" | "user_signing_key") {
                ::tracing::warn!("Unknown signing key type '{}' in m.signing_key_update from {}", key_type, origin);
                result.dropped += 1;
                continue;
            }

            let key_type_str = key_type.replace("_key", "");

            // upsert the cross-signing key into storage
            let now = chrono::Utc::now();
            let cross_signing_key = CrossSigningKey {
                id: uuid::Uuid::nil(),
                user_id: user_id.clone(),
                key_type: key_type_str.clone(),
                public_key: key_str.to_string(),
                usage: vec![key_type_str.clone()],
                signatures: serde_json::Value::Null,
                key_json: None,
                created_ts: now,
                updated_ts: now,
            };

            match cs_service.upsert_federation_cross_signing_key(&cross_signing_key).await {
                Ok(()) => {
                    result.processed += 1;
                }
                Err(e) => {
                    ::tracing::warn!("Failed to store m.signing_key_update for {} ({}): {}", user_id, key_type, e);
                    result.errored += 1;
                }
            }
        }
    }

    if result.processed > 0 {
        increment_counter_by(ctx, "federation_inbound_signing_key_processed_total", result.processed as u64);
    }
    if result.dropped > 0 {
        increment_counter_by(ctx, "federation_inbound_signing_key_dropped_total", result.dropped as u64);
    }
    if result.errored > 0 {
        increment_counter_by(ctx, "federation_inbound_signing_key_error_total", result.errored as u64);
    }

    result
}

/// The `EduDispatcher` struct.
pub struct EduDispatcher;

impl EduDispatcher {
    /// See [`dispatch`].
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
            EduType::Receipt => handle_receipt_edu(ctx, origin, edu, remaining).await,
            EduType::SigningKeyUpdate => handle_signing_key_update_edu(ctx, origin, edu, remaining).await,
            // MSC4262: Profile Update EDU - signals remote servers to invalidate cached profile data
            EduType::ProfileUpdate => handle_profile_update_edu(ctx, origin, edu, remaining).await,
        };

        Some(result)
    }
}

/// Handle `m.profile_update` EDU (MSC4262).
/// This EDU signals to remote servers that a user has updated their profile
/// (displayname or avatar_url). This server refreshes the new value in the local
/// `users` table (update-only) and bumps the device-list stream so that local
/// clients sharing rooms with the user will learn of the new profile.
async fn handle_profile_update_edu(
    ctx: &FederationContext,
    origin: &str,
    edu: &Value,
    _remaining: usize,
) -> EduProcessResult {
    let content = match edu.get("content") {
        Some(c) => c,
        None => {
            increment_counter(ctx, "federation_inbound_profile_update_dropped_total");
            return EduProcessResult { dropped: 1, ..Default::default() };
        }
    };

    let user_id = match content.get("user_id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => {
            increment_counter(ctx, "federation_inbound_profile_update_dropped_total");
            return EduProcessResult { dropped: 1, ..Default::default() };
        }
    };

    // Validate user belongs to origin
    if !user_matches_origin(user_id, origin) {
        increment_counter(ctx, "federation_inbound_profile_update_dropped_total");
        return EduProcessResult { dropped: 1, ..Default::default() };
    }

    let displayname = content.get("displayname").and_then(|v| v.as_str());
    let avatar_url = content.get("avatar_url").and_then(|v| v.as_str());

    // MSC4262: Persist the received profile into the local `users` table.
    // Returns `true` when a known local/remote user row was refreshed; `false`
    // when we have never seen this user locally (nothing to cache, no row to
    // update — we deliberately do not materialize unknown remote accounts).
    let updated = match ctx.user_service.apply_profile_update_from_federation(user_id, displayname, avatar_url).await {
        Ok(updated) => updated,
        Err(e) => {
            ::tracing::warn!(error = %e, user_id = %user_id, origin = %origin, "Failed to persist m.profile_update EDU");
            increment_counter(ctx, "federation_inbound_profile_update_error_total");
            return EduProcessResult { errored: 1, ..Default::default() };
        }
    };

    if !updated {
        // Unknown remote user: still drop any stale negative cache entry so a
        // later profile read re-queries the origin server, then count as processed.
        let _ = ctx.cache.delete(&format!("user:profile:{user_id}")).await;
        ::tracing::debug!(
            user_id = %user_id,
            origin = %origin,
            "m.profile_update EDU for unknown local user — invalidated profile cache"
        );
        increment_counter(ctx, "federation_inbound_profile_update_processed_total");
        return EduProcessResult { processed: 1, ..Default::default() };
    }

    ::tracing::info!(
        user_id = %user_id,
        origin = %origin,
        displayname = displayname.unwrap_or(""),
        avatar_url = avatar_url.unwrap_or(""),
        "Persisted m.profile_update EDU from federation"
    );

    // Bump the device-list change stream with a user-level profile change so
    // local clients sharing a room with `user_id` will be told the profile
    // changed (device_id is None for user-level profile updates).
    let stream_id = current_timestamp_millis();
    if let Err(e) = ctx.device_storage.insert_device_list_change(user_id, None, "profile", stream_id).await {
        // Non-fatal: the profile is already persisted; stream bump is best-effort.
        ::tracing::warn!(error = %e, user_id = %user_id, "Failed to record profile change for device-list stream");
    }

    increment_counter(ctx, "federation_inbound_profile_update_processed_total");

    EduProcessResult { processed: 1, dropped: 0, errored: 0 }
}
