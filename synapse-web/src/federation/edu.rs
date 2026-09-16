//! Unified EDU (Ephemeral Data Unit) dispatch for inbound federation transactions.
//!
//! Pure types (`EduType`, `EduProcessResult`, `user_matches_origin`) live in
//! `synapse_federation::edu`. This module provides the dispatcher and handlers
//! that depend on `FederationContext` and the service container.

pub use synapse_federation::edu::{user_matches_origin, EduProcessResult, EduType, UnknownEduType};

use crate::routes::context::FederationContext;
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
// Pure validation helpers
// ---------------------------------------------------------------------------
//
// Each helper is a `fn(...)` on `&Value` / `&str` — no `FederationContext`,
// no `async`, no DB calls.  Handlers delegate to these at every drop gate
// so unit tests exercise the same code that runs at runtime (single source
// of truth).
// ---------------------------------------------------------------------------

// --- presence ---

/// Validated presence update entry: `(user_id, presence_str, status_msg)`.
type PresenceUpdateFields<'a> = (&'a str, &'a str, Option<&'a str>);

/// Extract the `push` array from a `m.presence` EDU.
/// Returns `None` if the EDU lacks `content.push` (whole EDU should be dropped).
fn parse_presence_push(edu: &Value) -> Option<&Vec<Value>> {
    edu.get("content").and_then(|c| c.get("push")).and_then(|v| v.as_array())
}

/// Validate a single presence update entry against `origin`. Returns
/// `Some((user_id, presence_str, status_msg))` when the entry has a valid
/// `user_id` belonging to `origin`; `None` means the entry should be dropped.
/// Defaults: missing `presence` → `"online"`, missing `status_msg` → `None`.
fn validate_presence_update<'u>(update: &'u Value, origin: &str) -> Option<PresenceUpdateFields<'u>> {
    let user_id = update.get("user_id").and_then(|v| v.as_str())?;
    if !user_matches_origin(user_id, origin) {
        return None;
    }
    let presence_str = update.get("presence").and_then(|v| v.as_str()).unwrap_or("online");
    let status_msg = update.get("status_msg").and_then(|v| v.as_str());
    Some((user_id, presence_str, status_msg))
}

// --- typing ---

/// Extract `room_id` from a `m.typing` EDU.
/// Returns `None` if missing (whole EDU should be dropped).
fn extract_typing_room_id(edu: &Value) -> Option<&str> {
    edu.get("room_id").and_then(|v| v.as_str())
}

/// Filter `m.typing` `user_ids` by origin. Returns the list of user_ids
/// that match `origin`. The caller should drop the EDU if the list is empty.
fn filter_typing_user_ids(edu: &Value, origin: &str) -> Vec<String> {
    edu.get("content")
        .and_then(|c| c.get("user_ids"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .filter(|uid| user_matches_origin(uid, origin))
                .collect()
        })
        .unwrap_or_default()
}

// --- device_list_update ---

/// Parsed device-list-update fields: `(user_id, device_id, stream_id, change_type)`.
type DeviceListUpdateFields<'a> = (&'a str, Option<&'a str>, i64, &'a str);

/// Validate a `m.device_list_update` EDU. Returns `Some(...)` if structurally
/// valid and origin matches. `None` means the EDU should be dropped.
fn validate_device_list_update_content<'a>(edu: &'a Value, origin: &str) -> Option<DeviceListUpdateFields<'a>> {
    let content = edu.get("content")?;
    let user_id = content.get("user_id").and_then(|v| v.as_str())?;
    if !user_matches_origin(user_id, origin) {
        return None;
    }
    let device_id = content.get("device_id").and_then(|v| v.as_str());
    let stream_id = content.get("stream_id").and_then(|v| v.as_i64()).unwrap_or_else(current_timestamp_millis);
    let change_type =
        if content.get("deleted").and_then(|v| v.as_bool()).unwrap_or(false) { "deleted" } else { "updated" };
    Some((user_id, device_id, stream_id, change_type))
}

// --- direct_to_device ---

/// Parsed direct-to-device fields: `(sender, event_type, messages)`.
type DirectToDeviceFields<'a> = (&'a str, &'a str, &'a Value);

/// Validate a `m.direct_to_device` EDU. Returns `Some((sender, event_type,
/// messages))` if structurally valid and origin matches. `None` means the
/// EDU should be dropped.
fn validate_direct_to_device_content<'a>(edu: &'a Value, origin: &str) -> Option<DirectToDeviceFields<'a>> {
    let sender = edu.get("sender").and_then(|v| v.as_str())?;
    if !user_matches_origin(sender, origin) {
        return None;
    }
    let event_type = edu.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if event_type.is_empty() {
        return None;
    }
    let messages = edu.get("content").and_then(|c| c.get("messages"))?;
    Some((sender, event_type, messages))
}

/// Flattened to-device batch: `(recipient_user_id, recipient_device_id, content)`.
type ToDeviceMessage<'a> = (&'a String, &'a String, &'a Value);

/// Pure recipient-limit check: returns `true` when `count` has exceeded the
/// federation to-device recipient cap. Extracted so the cap is unit-testable.
fn exceeds_to_device_recipient_limit(count: usize) -> bool {
    count > MAX_FEDERATION_TO_DEVICE_RECIPIENTS
}

/// Pure message-size check: returns `true` when the serialized JSON length of
/// `content` exceeds `MAX_FEDERATION_TO_DEVICE_MSG_BYTES`.
fn exceeds_to_device_msg_size(content: &Value) -> bool {
    serde_json::to_string(content).map(|s| s.len()).unwrap_or(0) > MAX_FEDERATION_TO_DEVICE_MSG_BYTES
}

/// Flatten the `messages` map (user → device → content) into an ordered list
/// of batches. Pure — no `FederationContext`, no async, no DB.
///
/// Returns `(batches, dropped)`:
/// - `batches`: entries within the recipient cap and size limit, each ready
///   for one `send_messages` call.
/// - `dropped`: number of entries skipped (recipient cap exceeded, message
///   too large, or `messages` not a JSON object). The cap counts *seen*
///   entries, matching the previous handler behaviour exactly.
fn collect_to_device_messages(messages: &Value) -> (Vec<ToDeviceMessage<'_>>, usize) {
    let mut batches = Vec::new();
    let mut dropped = 0usize;
    let Some(msg_map) = messages.as_object() else {
        // Malformed `messages` (not an object): drop the whole EDU rather
        // than silently processing nothing with a zero result.
        return (batches, 1);
    };
    let mut recipient_count = 0usize;
    for (recipient_user_id, device_map) in msg_map {
        let Some(devices) = device_map.as_object() else {
            continue;
        };
        for (recipient_device_id, content) in devices {
            recipient_count += 1;
            if exceeds_to_device_recipient_limit(recipient_count) {
                dropped += 1;
                continue;
            }
            if exceeds_to_device_msg_size(content) {
                dropped += 1;
                continue;
            }
            batches.push((recipient_user_id, recipient_device_id, content));
        }
    }
    (batches, dropped)
}

// --- receipt ---

/// Extract the receipt content map from a `m.receipt` EDU.
/// Returns `None` if content is missing or not an object (whole EDU dropped).
fn parse_receipt_content(edu: &Value) -> Option<&serde_json::Map<String, Value>> {
    edu.get("content").and_then(|c| c.as_object())
}

// --- signing_key_update ---

/// Validate a signing key type. Returns `true` for valid key types, `false`
/// otherwise.
fn validate_signing_key_type(key_type: &str) -> bool {
    matches!(key_type, "master_key" | "self_signing_key" | "user_signing_key")
}

/// Extract the content map from a `m.signing_key_update` EDU.
/// Returns `None` if content is missing or not an object (whole EDU dropped).
fn parse_signing_key_content(edu: &Value) -> Option<&serde_json::Map<String, Value>> {
    edu.get("content").and_then(|c| c.as_object())
}

// ---------------------------------------------------------------------------
// Per-type processing functions
// ---------------------------------------------------------------------------

async fn handle_presence_edu(ctx: &FederationContext, origin: &str, edu: &Value, remaining: usize) -> EduProcessResult {
    let Some(push) = parse_presence_push(edu) else {
        ::tracing::debug!("Dropping m.presence EDU from {} without push content", origin);
        increment_counter(ctx, "federation_inbound_presence_dropped_total");
        return EduProcessResult { dropped: 1, ..Default::default() };
    };

    let mut result = EduProcessResult::default();
    for update in push.iter().take(remaining) {
        let Some((user_id, presence_str, status_msg)) = validate_presence_update(update, origin) else {
            result.dropped += 1;
            continue;
        };

        let presence =
            synapse_common::PresenceState::from_str_opt(presence_str).unwrap_or(synapse_common::PresenceState::Online);

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
    let Some(room_id) = extract_typing_room_id(edu) else {
        increment_counter(ctx, "federation_inbound_typing_dropped_total");
        return EduProcessResult { dropped: 1, ..Default::default() };
    };

    // MSC4163: enforce m.room.server_acl on room-scoped EDUs (typing).
    // If the origin server is denied by the room's ACL (or the ACL is
    // malformed — fail-closed), silently drop the EDU.
    if !crate::routes::federation::is_server_allowed_by_room_acl(ctx, room_id, origin).await {
        ::tracing::info!(
            room_id = %room_id,
            origin = %origin,
            "Dropping m.typing EDU from origin denied by room ACL (MSC4163)"
        );
        increment_counter_by(ctx, "federation_inbound_edu_acl_denied_total", 1);
        return EduProcessResult { dropped: 1, ..Default::default() };
    }

    let user_ids = filter_typing_user_ids(edu, origin);

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
    let Some((user_id, device_id, stream_id, change_type)) = validate_device_list_update_content(edu, origin) else {
        ::tracing::debug!(
            "Dropping m.device_list_update EDU from {} (missing/malformed content or origin mismatch)",
            origin
        );
        increment_counter(ctx, "federation_inbound_device_list_update_dropped_total");
        return EduProcessResult { dropped: 1, ..Default::default() };
    };

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
    let Some((sender, event_type, messages)) = validate_direct_to_device_content(edu, origin) else {
        ::tracing::debug!("Dropping m.direct_to_device EDU from {} (missing/malformed sender/type/content.messages or origin mismatch)", origin);
        increment_counter(ctx, "federation_inbound_direct_to_device_dropped_total");
        return EduProcessResult { dropped: 1, ..Default::default() };
    };

    let (batches, dropped_from_limit) = collect_to_device_messages(messages);
    let mut result = EduProcessResult { dropped: dropped_from_limit, ..Default::default() };

    for (recipient_user_id, recipient_device_id, content) in batches {
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
    let Some(content) = parse_receipt_content(edu) else {
        ::tracing::debug!("Dropping m.receipt EDU from {} without content object", origin);
        increment_counter(ctx, "federation_inbound_receipt_dropped_total");
        return EduProcessResult { dropped: 1, ..Default::default() };
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
    let Some(content) = parse_signing_key_content(edu) else {
        ::tracing::debug!("Dropping m.signing_key_update EDU from {} without content object", origin);
        increment_counter(ctx, "federation_inbound_signing_key_dropped_total");
        return EduProcessResult { dropped: 1, ..Default::default() };
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

            if !validate_signing_key_type(key_type.as_str()) {
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

/// Parsed `m.profile_update` content: `(user_id, displayname, avatar_url)`.
///
/// P1: named so `clippy::type_complexity` stays satisfied for both the parser
/// and its validating wrapper without sprinkling `#[allow]` around.
type ProfileUpdateFields<'a> = (&'a str, Option<&'a str>, Option<&'a str>);

/// Parse profile update EDU content into its constituent fields.
/// Returns `Ok((user_id, displayname, avatar_url))` if the content is valid,
/// or `None` if required fields are missing or malformed.
///
/// This pure function is testable without `FederationContext` for unit testing.
fn parse_profile_update_content(content: &Value) -> Option<ProfileUpdateFields<'_>> {
    let user_id = content.get("user_id").and_then(|v| v.as_str())?;
    let displayname = content.get("displayname").and_then(|v| v.as_str());
    let avatar_url = content.get("avatar_url").and_then(|v| v.as_str());
    Some((user_id, displayname, avatar_url))
}

/// Validate profile update EDU content and origin. Returns `Some((user_id, displayname, avatar_url))`
/// if valid, `None` if the content is malformed or origin doesn't match user’s domain.
///
/// P1: the return type borrows from `edu`, so the elided-lifetime form
/// (`-> Option<(&str, Option<&str>, Option<&str>)>`) is not accepted by rustdoc.
/// Plain `cargo build`/`clippy` never checked it because the returned lifetimes
/// were unconstrained; `cargo test --doc --workspace` surfaced it as E0106.
fn validate_profile_update_content<'a>(edu: &'a Value, origin: &str) -> Option<ProfileUpdateFields<'a>> {
    let content = edu.get("content")?;
    let (user_id, displayname, avatar_url) = parse_profile_update_content(content)?;
    if !user_matches_origin(user_id, origin) {
        return None;
    }
    Some((user_id, displayname, avatar_url))
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
    let (user_id, displayname, avatar_url) = match validate_profile_update_content(edu, origin) {
        Some(v) => v,
        None => {
            increment_counter(ctx, "federation_inbound_profile_update_dropped_total");
            return EduProcessResult { dropped: 1, ..Default::default() };
        }
    };

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

// ---------------------------------------------------------------------------
// Tests for the EDU validation helpers
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // --- parse_profile_update_content ---

    #[test]
    fn test_parse_profile_update_content_all_fields() {
        let content = json!({
            "user_id": "@alice:example.com",
            "displayname": "Alice",
            "avatar_url": "mxc://example.com/abc"
        });
        let (user_id, displayname, avatar_url) =
            parse_profile_update_content(&content).expect("should parse valid content");
        assert_eq!(user_id, "@alice:example.com");
        assert_eq!(displayname, Some("Alice"));
        assert_eq!(avatar_url, Some("mxc://example.com/abc"));
    }

    #[test]
    fn test_parse_profile_update_content_user_id_only() {
        let content = json!({
            "user_id": "@bob:matrix.org",
        });
        let (user_id, displayname, avatar_url) =
            parse_profile_update_content(&content).expect("should parse with user_id only");
        assert_eq!(user_id, "@bob:matrix.org");
        assert_eq!(displayname, None);
        assert_eq!(avatar_url, None);
    }

    #[test]
    fn test_parse_profile_update_content_missing_user_id() {
        let content = json!({
            "displayname": "Alice",
            "avatar_url": "mxc://example.com/abc"
        });
        assert!(parse_profile_update_content(&content).is_none());
    }

    #[test]
    fn test_parse_profile_update_content_empty_content() {
        let content = json!({});
        assert!(parse_profile_update_content(&content).is_none());
    }

    // --- validate_profile_update_content ---

    #[test]
    fn test_validate_profile_update_content_valid_with_matching_origin() {
        let edu = json!({
            "content": {
                "user_id": "@alice:example.com",
                "displayname": "Alice",
            }
        });
        let (user_id, displayname, avatar_url) =
            validate_profile_update_content(&edu, "example.com").expect("valid content with matching origin");
        assert_eq!(user_id, "@alice:example.com");
        assert_eq!(displayname, Some("Alice"));
        assert_eq!(avatar_url, None);
    }

    #[test]
    fn test_validate_profile_update_content_origin_mismatch_rejected() {
        let edu = json!({
            "content": {
                "user_id": "@alice:example.com",
                "displayname": "Alice",
            }
        });
        // Origin mismatch: user_id domain ≠ origin server — reject to prevent
        // an EDU sender claiming a user that doesn't belong to their server.
        assert!(validate_profile_update_content(&edu, "evil.com").is_none());
    }

    #[test]
    fn test_validate_profile_update_content_origin_mismatch_localpart() {
        // Localpart-only user IDs (no colon) are always rejected by
        // `user_matches_origin`, which prevents bypassing the origin check.
        let edu = json!({
            "content": {
                "user_id": "alice",
            }
        });
        assert!(validate_profile_update_content(&edu, "example.com").is_none());
    }

    #[test]
    fn test_validate_profile_update_content_missing_content() {
        let edu = json!({
            "edu_type": "m.profile_update",
            // no "content" field
        });
        assert!(validate_profile_update_content(&edu, "example.com").is_none());
    }

    #[test]
    fn test_validate_profile_update_content_missing_user_id() {
        let edu = json!({
            "content": {
                "displayname": "Alice",
            }
        });
        assert!(validate_profile_update_content(&edu, "example.com").is_none());
    }

    #[test]
    fn test_validate_profile_update_content_empty_content_object() {
        let edu = json!({
            "content": {}
        });
        assert!(validate_profile_update_content(&edu, "example.com").is_none());
    }

    // --- presence: parse_presence_push + validate_presence_update ---

    #[test]
    fn test_parse_presence_push_present() {
        let edu = json!({
            "content": { "push": [{ "user_id": "@alice:example.com" }] }
        });
        let push = parse_presence_push(&edu).expect("should extract push array");
        assert_eq!(push.len(), 1);
    }

    #[test]
    fn test_parse_presence_push_missing() {
        let edu = json!({ "content": {} });
        assert!(parse_presence_push(&edu).is_none());
        let edu = json!({});
        assert!(parse_presence_push(&edu).is_none());
    }

    #[test]
    fn test_validate_presence_update_valid_with_defaults() {
        let update = json!({ "user_id": "@alice:example.com" });
        let (user_id, presence_str, status_msg) =
            validate_presence_update(&update, "example.com").expect("valid entry");
        assert_eq!(user_id, "@alice:example.com");
        assert_eq!(presence_str, "online"); // default
        assert_eq!(status_msg, None);
    }

    #[test]
    fn test_validate_presence_update_explicit_fields() {
        let update = json!({
            "user_id": "@alice:example.com",
            "presence": "unavailable",
            "status_msg": "away"
        });
        let (user_id, presence_str, status_msg) =
            validate_presence_update(&update, "example.com").expect("valid entry");
        assert_eq!(user_id, "@alice:example.com");
        assert_eq!(presence_str, "unavailable");
        assert_eq!(status_msg, Some("away"));
    }

    #[test]
    fn test_validate_presence_update_origin_mismatch_rejected() {
        let update = json!({ "user_id": "@alice:example.com" });
        assert!(validate_presence_update(&update, "evil.com").is_none());
    }

    #[test]
    fn test_validate_presence_update_missing_user_id() {
        let update = json!({ "presence": "online" });
        assert!(validate_presence_update(&update, "example.com").is_none());
    }

    #[test]
    fn test_validate_presence_update_localpart_only_user_id_rejected() {
        let update = json!({ "user_id": "alice" });
        assert!(validate_presence_update(&update, "example.com").is_none());
    }

    // --- typing: extract_typing_room_id + filter_typing_user_ids ---

    #[test]
    fn test_extract_typing_room_id_present() {
        let edu = json!({ "room_id": "!room:example.com" });
        assert_eq!(extract_typing_room_id(&edu), Some("!room:example.com"));
    }

    #[test]
    fn test_extract_typing_room_id_missing() {
        let edu = json!({});
        assert!(extract_typing_room_id(&edu).is_none());
    }

    #[test]
    fn test_filter_typing_user_ids_filters_by_origin() {
        let edu = json!({
            "content": {
                "user_ids": ["@alice:example.com", "@bob:other.com", "@carol:example.com"]
            }
        });
        let ids = filter_typing_user_ids(&edu, "example.com");
        assert_eq!(ids, vec!["@alice:example.com".to_string(), "@carol:example.com".to_string()]);
    }

    #[test]
    fn test_filter_typing_user_ids_missing_content() {
        let edu = json!({});
        assert!(filter_typing_user_ids(&edu, "example.com").is_empty());
    }

    #[test]
    fn test_filter_typing_user_ids_invalid_entries_filtered() {
        let edu = json!({
            "content": {
                "user_ids": [123, "@alice:example.com", null]
            }
        });
        let ids = filter_typing_user_ids(&edu, "example.com");
        assert_eq!(ids, vec!["@alice:example.com".to_string()]);
    }

    // --- device_list_update: validate_device_list_update_content ---

    #[test]
    fn test_validate_device_list_update_content_valid_updated() {
        let edu = json!({
            "content": { "user_id": "@alice:example.com", "device_id": "DEV", "stream_id": 42 }
        });
        let (user_id, device_id, stream_id, change_type) =
            validate_device_list_update_content(&edu, "example.com").expect("valid entry");
        assert_eq!(user_id, "@alice:example.com");
        assert_eq!(device_id, Some("DEV"));
        assert_eq!(stream_id, 42);
        assert_eq!(change_type, "updated");
    }

    #[test]
    fn test_validate_device_list_update_content_deleted_flag() {
        let edu = json!({
            "content": { "user_id": "@alice:example.com", "deleted": true }
        });
        let (_, _, _, change_type) = validate_device_list_update_content(&edu, "example.com").expect("valid entry");
        assert_eq!(change_type, "deleted");
    }

    #[test]
    fn test_validate_device_list_update_content_origin_mismatch() {
        let edu = json!({ "content": { "user_id": "@alice:example.com" } });
        assert!(validate_device_list_update_content(&edu, "evil.com").is_none());
    }

    #[test]
    fn test_validate_device_list_update_content_missing_content() {
        let edu = json!({});
        assert!(validate_device_list_update_content(&edu, "example.com").is_none());
    }

    #[test]
    fn test_validate_device_list_update_content_missing_user_id() {
        let edu = json!({ "content": { "device_id": "DEV" } });
        assert!(validate_device_list_update_content(&edu, "example.com").is_none());
    }

    #[test]
    fn test_validate_device_list_update_content_localpart_only_rejected() {
        // A bare localpart (no `:domain`) must not pass the origin gate.
        let edu = json!({ "content": { "user_id": "alice", "device_id": "DEV" } });
        assert!(validate_device_list_update_content(&edu, "example.com").is_none());
    }

    #[test]
    fn test_validate_device_list_update_content_missing_device_id_allowed() {
        // `device_id` is optional: a user-level device-list change (e.g. all
        // devices deleted) legitimately omits it.
        let edu = json!({ "content": { "user_id": "@alice:example.com", "stream_id": 7 } });
        let (user_id, device_id, stream_id, change_type) =
            validate_device_list_update_content(&edu, "example.com").expect("valid entry");
        assert_eq!(user_id, "@alice:example.com");
        assert_eq!(device_id, None);
        assert_eq!(stream_id, 7);
        assert_eq!(change_type, "updated");
    }

    #[test]
    fn test_validate_device_list_update_content_missing_stream_id_defaults_to_now() {
        let before = current_timestamp_millis();
        let edu = json!({ "content": { "user_id": "@alice:example.com", "device_id": "DEV" } });
        let (_, _, stream_id, _) = validate_device_list_update_content(&edu, "example.com").expect("valid entry");
        let after = current_timestamp_millis();
        assert!(stream_id >= before && stream_id <= after, "stream_id should fall back to current ms timestamp");
    }

    #[test]
    fn test_validate_device_list_update_content_non_integer_stream_id_defaults() {
        // Spec says stream_id is an integer; a non-numeric value must not
        // poison the persisted change with 0 — it falls back to wall clock.
        let edu = json!({ "content": { "user_id": "@alice:example.com", "stream_id": "not-a-number" } });
        let (_, _, stream_id, _) = validate_device_list_update_content(&edu, "example.com").expect("valid entry");
        assert!(stream_id > 1_700_000_000_000, "expected ms-epoch fallback, got {stream_id}");
    }

    #[test]
    fn test_validate_device_list_update_content_non_bool_deleted_treated_as_updated() {
        // `deleted` must be a real boolean to flip the change type; a string
        // or number is spec-invalid and must not silently delete devices.
        let edu = json!({ "content": { "user_id": "@alice:example.com", "deleted": "true" } });
        let (_, _, _, change_type) = validate_device_list_update_content(&edu, "example.com").expect("valid entry");
        assert_eq!(change_type, "updated");
    }

    #[test]
    fn test_validate_device_list_update_content_deleted_false_is_updated() {
        let edu = json!({ "content": { "user_id": "@alice:example.com", "deleted": false } });
        let (_, _, _, change_type) = validate_device_list_update_content(&edu, "example.com").expect("valid entry");
        assert_eq!(change_type, "updated");
    }

    #[test]
    fn test_validate_device_list_update_content_malformed_content_type() {
        // content is not an object → whole EDU dropped.
        let edu = json!({ "content": "string" });
        assert!(validate_device_list_update_content(&edu, "example.com").is_none());
    }

    #[test]
    fn test_validate_device_list_update_content_user_id_not_string() {
        let edu = json!({ "content": { "user_id": 42 } });
        assert!(validate_device_list_update_content(&edu, "example.com").is_none());
    }

    // --- direct_to_device: validate_direct_to_device_content ---

    #[test]
    fn test_validate_direct_to_device_content_valid() {
        let edu = json!({
            "sender": "@alice:example.com",
            "type": "m.room_key",
            "content": { "messages": { "@bob:example.com": { "DEV": {} } } }
        });
        let (sender, event_type, messages) = validate_direct_to_device_content(&edu, "example.com").expect("valid EDU");
        assert_eq!(sender, "@alice:example.com");
        assert_eq!(event_type, "m.room_key");
        assert!(messages.as_object().is_some());
        assert!(messages.get("@bob:example.com").and_then(|v| v.get("DEV")).is_some());
    }

    #[test]
    fn test_validate_direct_to_device_content_origin_mismatch() {
        let edu = json!({
            "sender": "@alice:example.com",
            "type": "m.room_key",
            "content": { "messages": {} }
        });
        assert!(validate_direct_to_device_content(&edu, "evil.com").is_none());
    }

    #[test]
    fn test_validate_direct_to_device_content_missing_sender() {
        let edu = json!({ "type": "m.room_key", "content": { "messages": {} } });
        assert!(validate_direct_to_device_content(&edu, "example.com").is_none());
    }

    #[test]
    fn test_validate_direct_to_device_content_empty_type() {
        let edu = json!({ "sender": "@alice:example.com", "type": "", "content": { "messages": {} } });
        assert!(validate_direct_to_device_content(&edu, "example.com").is_none());
    }

    #[test]
    fn test_validate_direct_to_device_content_missing_messages() {
        let edu = json!({ "sender": "@alice:example.com", "type": "m.room_key", "content": {} });
        assert!(validate_direct_to_device_content(&edu, "example.com").is_none());
    }

    #[test]
    fn test_validate_direct_to_device_content_sender_localpart_only_rejected() {
        // sender without `:domain` must not pass the origin gate.
        let edu = json!({ "sender": "alice", "type": "m.room_key", "content": { "messages": {} } });
        assert!(validate_direct_to_device_content(&edu, "example.com").is_none());
    }

    #[test]
    fn test_validate_direct_to_device_content_missing_type() {
        // No `type` field at all → empty-string default → dropped.
        let edu = json!({ "sender": "@alice:example.com", "content": { "messages": {} } });
        assert!(validate_direct_to_device_content(&edu, "example.com").is_none());
    }

    #[test]
    fn test_validate_direct_to_device_content_type_not_string() {
        let edu = json!({ "sender": "@alice:example.com", "type": 123, "content": { "messages": {} } });
        assert!(validate_direct_to_device_content(&edu, "example.com").is_none());
    }

    #[test]
    fn test_validate_direct_to_device_content_missing_content() {
        let edu = json!({ "sender": "@alice:example.com", "type": "m.room_key" });
        assert!(validate_direct_to_device_content(&edu, "example.com").is_none());
    }

    // --- direct_to_device: collect_to_device_messages (flattening + limits) ---

    #[test]
    fn test_collect_to_device_messages_flattens_and_preserves_order() {
        let messages = json!({
            "@bob:example.com": { "DEV1": { "ciphertext": "a" }, "DEV2": { "ciphertext": "b" } },
            "@carol:example.com": { "DEV3": { "ciphertext": "c" } }
        });
        let (batches, dropped) = collect_to_device_messages(&messages);
        assert_eq!(dropped, 0);
        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0].0, "@bob:example.com");
        assert_eq!(batches[0].1, "DEV1");
        assert_eq!(batches[1].1, "DEV2");
        assert_eq!(batches[2].0, "@carol:example.com");
    }

    #[test]
    fn test_collect_to_device_messages_empty_map() {
        let messages = json!({});
        let (batches, dropped) = collect_to_device_messages(&messages);
        assert!(batches.is_empty());
        assert_eq!(dropped, 0);
    }

    #[test]
    fn test_collect_to_device_messages_not_object_drops_whole_edu() {
        // Previously this fell through the `if let Some(msg_map)` and produced
        // a silent all-zero result; now it must count as dropped.
        let messages = json!("not-an-object");
        let (batches, dropped) = collect_to_device_messages(&messages);
        assert!(batches.is_empty());
        assert_eq!(dropped, 1);
    }

    #[test]
    fn test_collect_to_device_messages_device_map_not_object_skipped() {
        let messages = json!({
            "@bob:example.com": "malformed",
            "@carol:example.com": { "DEV": { "ok": true } }
        });
        let (batches, dropped) = collect_to_device_messages(&messages);
        assert_eq!(dropped, 0, "non-object device map is skipped, not counted");
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].0, "@carol:example.com");
    }

    #[test]
    fn test_collect_to_device_messages_recipient_cap_exceeded() {
        // Build 5000 valid + 3 over-cap entries; the over-cap ones must be
        // dropped and the first 5000 kept.
        let mut devices = serde_json::Map::new();
        for i in 0..5003 {
            devices.insert(format!("DEV{i}"), json!({ "idx": i }));
        }
        let messages = json!({ "@bob:example.com": devices });
        let (batches, dropped) = collect_to_device_messages(&messages);
        assert_eq!(batches.len(), MAX_FEDERATION_TO_DEVICE_RECIPIENTS);
        assert_eq!(dropped, 3);
    }

    #[test]
    fn test_collect_to_device_messages_recipient_cap_exactly_at_limit_kept() {
        let mut devices = serde_json::Map::new();
        for i in 0..MAX_FEDERATION_TO_DEVICE_RECIPIENTS {
            devices.insert(format!("DEV{i}"), json!({ "idx": i }));
        }
        let messages = json!({ "@bob:example.com": devices });
        let (batches, dropped) = collect_to_device_messages(&messages);
        assert_eq!(batches.len(), MAX_FEDERATION_TO_DEVICE_RECIPIENTS);
        assert_eq!(dropped, 0, "exactly at the limit must not drop");
    }

    #[test]
    fn test_collect_to_device_messages_oversized_content_dropped() {
        // 64KB limit: build content larger than the cap.
        let big = "x".repeat(MAX_FEDERATION_TO_DEVICE_MSG_BYTES + 1);
        let messages = json!({
            "@bob:example.com": {
                "DEV_BIG": { "payload": big },
                "DEV_OK": { "payload": "small" }
            }
        });
        let (batches, dropped) = collect_to_device_messages(&messages);
        assert_eq!(dropped, 1);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].1, "DEV_OK");
    }

    #[test]
    fn test_exceeds_to_device_recipient_limit_boundary() {
        assert!(!exceeds_to_device_recipient_limit(0));
        assert!(!exceeds_to_device_recipient_limit(MAX_FEDERATION_TO_DEVICE_RECIPIENTS));
        assert!(exceeds_to_device_recipient_limit(MAX_FEDERATION_TO_DEVICE_RECIPIENTS + 1));
    }

    #[test]
    fn test_exceeds_to_device_msg_size_boundary() {
        let small = json!({ "a": 1 });
        assert!(!exceeds_to_device_msg_size(&small));
        let big = json!({ "payload": "x".repeat(MAX_FEDERATION_TO_DEVICE_MSG_BYTES + 1) });
        assert!(exceeds_to_device_msg_size(&big));
    }

    // --- receipt: parse_receipt_content ---

    #[test]
    fn test_parse_receipt_content_present() {
        let edu = json!({ "content": { "!room:example.com": {} } });
        assert!(parse_receipt_content(&edu).is_some());
    }

    #[test]
    fn test_parse_receipt_content_missing() {
        let edu = json!({});
        assert!(parse_receipt_content(&edu).is_none());
    }

    #[test]
    fn test_parse_receipt_content_not_object() {
        let edu = json!({ "content": [] });
        assert!(parse_receipt_content(&edu).is_none());
    }

    // --- signing_key_update: validate_signing_key_type + parse_signing_key_content ---

    #[test]
    fn test_validate_signing_key_type_valid() {
        assert!(validate_signing_key_type("master_key"));
        assert!(validate_signing_key_type("self_signing_key"));
        assert!(validate_signing_key_type("user_signing_key"));
    }

    #[test]
    fn test_validate_signing_key_type_invalid() {
        assert!(!validate_signing_key_type("unknown_key"));
        assert!(!validate_signing_key_type(""));
        assert!(!validate_signing_key_type("master"));
    }

    #[test]
    fn test_parse_signing_key_content_present() {
        let edu = json!({ "content": { "@alice:example.com": {} } });
        assert!(parse_signing_key_content(&edu).is_some());
    }

    #[test]
    fn test_parse_signing_key_content_missing() {
        let edu = json!({});
        assert!(parse_signing_key_content(&edu).is_none());
    }
}
