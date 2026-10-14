//! The two write-authorization gates for room events.
//!
//! They are a pair, in one module, because they answer the same question — "may
//! *this* caller write *this* event to *this* room?" — for the two kinds of
//! write: [`ensure_room_state_write_access`] for state events and
//! [`ensure_message_write_access`] for everything else.
//!
//! They live together, and outside `mod.rs`, because of a failure mode that has
//! now shown up twice: a **second surface** that writes the same events needs the
//! *identical* gate, and a hand-rolled near-copy is how the two doors drift
//! apart. `PUT .../rooms/{roomId}/delayed_event/{eventType}/{txnId}` (MSC4140) is
//! such a surface, and it is the sharpest case — it only *schedules* the write;
//! the background dispatcher performs it later with **no** further authorization
//! check (`src/server/mod.rs`). A gate that is even slightly weaker here is an
//! escalation that lands minutes later, on a task nobody is watching.
//!
//! Splitting them out also keeps `handlers/room/mod.rs` under the 500-line
//! file-size ratchet (`scripts/ci/check_file_size_ratchet.py`), which the previous
//! path had just crossed.

use crate::routes::context::RoomContext;
use crate::routes::{ensure_room_member_ctx, AuthenticatedUser};
use synapse_common::{ApiError, ContentSanitizer};

/// See [`ensure_room_state_write_access`].
pub(crate) async fn ensure_room_state_write_access(
    ctx: &RoomContext,
    auth_user: &AuthenticatedUser,
    room_id: &str,
    event_type: &str,
    content: &serde_json::Value,
) -> Result<(), ApiError> {
    ensure_room_member_ctx(ctx, auth_user, room_id, "You must be a member of this room to send state events").await?;

    ctx.room_auth.verify_state_event_write(room_id, &auth_user.user_id, event_type).await?;

    // A power-levels event must also satisfy the power-levels *change* rules
    // (cannot elevate above yourself, cannot touch an equal-or-higher level, and
    // — room v12+, MSC4289 rule 10.4 — must not name a creator). Those rules need
    // the new content, which is why this is not folded into
    // `verify_state_event_write`. The canonical client path for a power-levels
    // update is `PUT /state/m.room.power_levels`, so the check has to live here
    // and not only on the `/send` path.
    if event_type == "m.room.power_levels" {
        ctx.room_auth.verify_power_levels_change(room_id, &auth_user.user_id, content).await?;
    }

    Ok(())
}

/// The message-path gate, i.e. everything `/send` checks **before** it writes.
///
/// Membership is enforced implicitly by
/// [`RoomAuth::verify_message_event_write`]: a non-member resolves to power
/// level `-1`, which is below every `events_default` the room can hold.
///
/// This is the **only** authorization a delayed event ever gets (see the module
/// docs), so it is deliberately the *whole* of `/send`'s preflight — power level,
/// room encryption, the power-levels change rules and the MSC3806 content scan —
/// rather than the subset that looks load-bearing today.
///
/// `content` is `&mut` because a `m.room.message` body is rewritten in place:
/// the HTML sanitizer replaces `formatted_body`, and the content scanner may
/// veto the text. Both run *before* the delayed-event branch, so a scheduled
/// message is sanitized and scanned exactly like an immediate one.
pub(crate) async fn ensure_message_write_access(
    ctx: &RoomContext,
    auth_user: &AuthenticatedUser,
    room_id: &str,
    event_type: &str,
    content: &mut serde_json::Value,
) -> Result<(), ApiError> {
    ctx.room_auth.verify_message_event_write(room_id, &auth_user.user_id, event_type).await?;

    if event_type == "m.room.encrypted" {
        let is_encrypted = ctx.room_service.state().check_room_has_encryption(room_id).await?;

        if !is_encrypted {
            return Err(ApiError::bad_request(
                "Cannot send encrypted message to a room where encryption is not enabled. Enable encryption first by sending an m.room.encryption state event.".to_string(),
            ));
        }
    }

    if event_type == "m.room.power_levels" {
        ctx.room_auth.verify_power_levels_change(room_id, &auth_user.user_id, content).await?;
    }

    if event_type == "m.room.message" {
        let format = content.get("format").and_then(|v| v.as_str()).unwrap_or("");
        if format == "org.matrix.custom.html" {
            if let Some(html) = content.get("formatted_body").and_then(|v| v.as_str()) {
                let sanitizer = ContentSanitizer::default();
                let cleaned = sanitizer.sanitize(html);
                content["formatted_body"] = serde_json::Value::String(cleaned);
            }
        }
        // MSC3806: scan the message body **when scanning is configured**.
        //
        // The policy (disabled ⇒ pass-through, scanner unreachable ⇒ fail-closed,
        // `safe: false` ⇒ 403) lives in exactly one place,
        // `content_scanner::scan_text_when_enabled`, the same way the media
        // upload path uses `scan_when_enabled`.  Calling
        // `ctx.content_scanner.scan_text(..).await?` directly here propagated
        // `M_CONTENT_SCAN_DISABLED` (501) whenever scanning was off — and since
        // `content_scanner.enabled: false` is the shipped default
        // (`docker/config/homeserver.yaml`), that made **every** `m.room.message`
        // send fail with 501 in the default configuration.
        let text = content.get("body").and_then(|v| v.as_str()).unwrap_or("");
        synapse_services::content_scanner::scan_text_when_enabled(
            ctx.content_scanner.as_ref(),
            ctx.metrics.as_ref(),
            &format!("msg:{}:{}", room_id, auth_user.user_id),
            text,
        )
        .await?;
    }

    Ok(())
}
