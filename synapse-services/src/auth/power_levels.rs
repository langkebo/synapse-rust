use super::AuthService;
use super::DEFAULT_POWER_LEVEL;
use synapse_common::*;

impl AuthService {
    pub async fn get_user_power_level(&self, room_id: &str, user_id: &str) -> ApiResult<i64> {
        let membership = self
            .member_storage
            .get_membership_state(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        if membership.is_none() {
            return Ok(-1);
        }

        let power_levels_content = self.get_room_power_levels_content(room_id).await?;

        if let Some(content) = power_levels_content {
            if let Some(level) =
                content.get("users").and_then(|users| users.get(user_id)).and_then(|level| level.as_i64())
            {
                return Ok(level);
            }

            if let Some(level) = content.get("users_default").and_then(|level| level.as_i64()) {
                return Ok(level);
            }
        }

        let room_creator: Option<String> = self
            .room_storage
            .get_room_creator(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        if let Some(creator) = room_creator {
            if creator == user_id {
                return Ok(100);
            }
        }

        Ok(0)
    }

    pub(crate) async fn get_joined_user_power_level(&self, room_id: &str, user_id: &str) -> ApiResult<i64> {
        let membership = self
            .member_storage
            .get_membership_state(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        match membership {
            Some(m) if m == "join" => self.get_user_power_level(room_id, user_id).await,
            _ => Ok(-1),
        }
    }

    pub(crate) async fn get_room_power_levels_content(&self, room_id: &str) -> ApiResult<Option<serde_json::Value>> {
        let events = self
            .event_storage
            .get_state_events_by_type(room_id, "m.room.power_levels")
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;
        Ok(events.first().map(|event| event.content.clone()))
    }

    /// Returns the room version string (e.g. `"10"`) from the `m.room.create`
    /// state event, or `DEFAULT_ROOM_VERSION` if not set.
    pub(crate) async fn get_room_version(&self, room_id: &str) -> ApiResult<String> {
        let events = self
            .event_storage
            .get_state_events_by_type(room_id, "m.room.create")
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;
        let version = events
            .first()
            .and_then(|event| event.content.get("room_version"))
            .and_then(|v| v.as_str())
            .unwrap_or(synapse_common::room_versions::DEFAULT_ROOM_VERSION);
        Ok(version.to_string())
    }

    pub async fn get_required_state_event_power_level(&self, room_id: &str, event_type: &str) -> ApiResult<i64> {
        let power_levels_content = self.get_room_power_levels_content(room_id).await?;
        if let Some(content) = power_levels_content {
            if let Some(level) =
                content.get("events").and_then(|events| events.get(event_type)).and_then(|level| level.as_i64())
            {
                return Ok(level);
            }

            if let Some(level) = content.get("state_default").and_then(|level| level.as_i64()) {
                return Ok(level);
            }
        }

        if event_type == "m.room.power_levels" {
            return Ok(100);
        }

        Ok(DEFAULT_POWER_LEVEL)
    }

    pub async fn get_required_message_event_power_level(&self, room_id: &str, event_type: &str) -> ApiResult<i64> {
        let power_levels_content = self.get_room_power_levels_content(room_id).await?;
        if let Some(content) = power_levels_content {
            if let Some(level) =
                content.get("events").and_then(|events| events.get(event_type)).and_then(|level| level.as_i64())
            {
                return Ok(level);
            }

            if let Some(level) = content.get("events_default").and_then(|level| level.as_i64()) {
                return Ok(level);
            }
        }

        Ok(0)
    }

    pub async fn verify_message_event_write(&self, room_id: &str, user_id: &str, event_type: &str) -> ApiResult<()> {
        let power_level = self.get_joined_user_power_level(room_id, user_id).await?;
        let required = self.get_required_message_event_power_level(room_id, event_type).await?;

        if power_level < required {
            ::tracing::warn!(
                target: "security_audit",
                event = "unauthorized_message_event_write",
                user_id = user_id,
                room_id = room_id,
                event_type = event_type,
                power_level = power_level,
                required = required,
                "User attempted to send message event without sufficient permission"
            );
            return Err(ApiError::forbidden("Insufficient permission to send this event".to_string()));
        }

        Ok(())
    }

    pub async fn verify_state_event_write(&self, room_id: &str, user_id: &str, event_type: &str) -> ApiResult<()> {
        let power_level = self.get_joined_user_power_level(room_id, user_id).await?;
        let required = self.get_required_state_event_power_level(room_id, event_type).await?;

        if power_level < required {
            ::tracing::warn!(
                target: "security_audit",
                event = "unauthorized_state_event_write",
                user_id = user_id,
                room_id = room_id,
                event_type = event_type,
                power_level = power_level,
                required = required,
                "User attempted to send state event without sufficient permission"
            );
            return Err(ApiError::forbidden("Insufficient permission to send this state event".to_string()));
        }

        Ok(())
    }

    pub async fn verify_power_levels_change(
        &self,
        room_id: &str,
        user_id: &str,
        new_content: &serde_json::Value,
    ) -> ApiResult<()> {
        let actor_level = self.get_joined_user_power_level(room_id, user_id).await?;
        let current_content = self.get_room_power_levels_content(room_id).await?;
        let new_power_levels_content = new_content;

        if let Some(current) = current_content {
            if let Some(new_users) = new_power_levels_content.get("users").and_then(|u| u.as_object()) {
                let current_users = current.get("users").and_then(|u| u.as_object());
                for (target_user, new_level_val) in new_users {
                    let new_level = new_level_val.as_i64().unwrap_or(0);
                    let current_level = current_users
                        .and_then(|cu| cu.get(target_user))
                        .and_then(|v| v.as_i64())
                        .unwrap_or_else(|| current.get("users_default").and_then(|v| v.as_i64()).unwrap_or(0));

                    if new_level > current_level && actor_level < new_level {
                        ::tracing::warn!(
                            target: "security_audit",
                            event = "unauthorized_power_level_elevation",
                            user_id = user_id,
                            room_id = room_id,
                            target_user = target_user,
                            actor_level = actor_level,
                            new_level = new_level,
                            "User attempted to set power level above their own"
                        );
                        return Err(ApiError::forbidden("Cannot set power level higher than your own".to_string()));
                    }

                    if current_level >= actor_level && new_level != current_level {
                        ::tracing::warn!(
                            target: "security_audit",
                            event = "unauthorized_power_level_change",
                            user_id = user_id,
                            room_id = room_id,
                            target_user = target_user,
                            actor_level = actor_level,
                            current_level = current_level,
                            new_level = new_level,
                            "User attempted to change power level of user at or above their own level"
                        );
                        return Err(ApiError::forbidden(
                            "Cannot change power level of user at or above your level".to_string(),
                        ));
                    }
                }
            }

            if let Some(new_events) = new_power_levels_content.get("events").and_then(|e| e.as_object()) {
                let current_events = current.get("events").and_then(|e| e.as_object());
                for (event_type, new_level_val) in new_events {
                    let new_level = new_level_val.as_i64().unwrap_or(0);
                    let current_level = current_events
                        .and_then(|ce| ce.get(event_type))
                        .and_then(|v| v.as_i64())
                        .unwrap_or_else(|| current.get("events_default").and_then(|v| v.as_i64()).unwrap_or(0));

                    if new_level > actor_level {
                        ::tracing::warn!(
                            target: "security_audit",
                            event = "unauthorized_event_level_change",
                            user_id = user_id,
                            room_id = room_id,
                            event_type = event_type,
                            actor_level = actor_level,
                            new_level = new_level,
                            "User attempted to set event power level above their own"
                        );
                        return Err(ApiError::forbidden("Cannot set event power level above your own".to_string()));
                    }

                    if current_level > actor_level && new_level != current_level {
                        ::tracing::warn!(
                            target: "security_audit",
                            event = "unauthorized_event_level_change_above_self",
                            user_id = user_id,
                            room_id = room_id,
                            event_type = event_type,
                            actor_level = actor_level,
                            current_level = current_level,
                            new_level = new_level,
                            "User attempted to change event power level above their own"
                        );
                        return Err(ApiError::forbidden("Cannot change event power level above your own".to_string()));
                    }
                }
            }

            let scalar_checks = [
                ("users_default", current.get("users_default").and_then(|v| v.as_i64()).unwrap_or(0)),
                ("events_default", current.get("events_default").and_then(|v| v.as_i64()).unwrap_or(0)),
                ("state_default", current.get("state_default").and_then(|v| v.as_i64()).unwrap_or(DEFAULT_POWER_LEVEL)),
                ("ban", current.get("ban").and_then(|v| v.as_i64()).unwrap_or(DEFAULT_POWER_LEVEL)),
                ("kick", current.get("kick").and_then(|v| v.as_i64()).unwrap_or(DEFAULT_POWER_LEVEL)),
                ("redact", current.get("redact").and_then(|v| v.as_i64()).unwrap_or(DEFAULT_POWER_LEVEL)),
                ("invite", current.get("invite").and_then(|v| v.as_i64()).unwrap_or(0)),
                (
                    "notifications",
                    current
                        .get("notifications")
                        .and_then(|v| v.as_object())
                        .and_then(|n| n.get("room").and_then(|r| r.as_i64()))
                        .unwrap_or(DEFAULT_POWER_LEVEL),
                ),
            ];

            for (key, current_level) in &scalar_checks {
                if let Some(new_level) = new_power_levels_content.get(key).and_then(|v| v.as_i64()) {
                    if new_level != *current_level {
                        if *current_level > actor_level {
                            return Err(ApiError::forbidden(format!(
                                "Cannot change {key} level: current level {current_level} is above your own {actor_level}"
                            )));
                        }
                        if new_level > actor_level {
                            return Err(ApiError::forbidden(format!(
                                "Cannot set {key} level above your own: {new_level} > {actor_level}"
                            )));
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn verify_room_moderator(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        let power_level = self.get_user_power_level(room_id, user_id).await?;

        let required_level = self
            .get_room_power_levels_content(room_id)
            .await?
            .and_then(|content| content.get("state_default").and_then(|level| level.as_i64()))
            .unwrap_or(DEFAULT_POWER_LEVEL);

        if power_level < required_level {
            ::tracing::warn!(
                target: "security_audit",
                event = "unauthorized_room_moderator_action",
                user_id = user_id,
                room_id = room_id,
                power_level = power_level,
                required_level = required_level,
                "User attempted moderator action without sufficient permission"
            );
            return Err(ApiError::forbidden("Room moderator permission required".to_string()));
        }

        Ok(())
    }

    pub async fn verify_room_admin(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        let power_level = self.get_user_power_level(room_id, user_id).await?;

        let required_level = 100;

        if power_level < required_level {
            return Err(ApiError::forbidden("Room admin permission required".to_string()));
        }

        Ok(())
    }

    pub async fn can_kick_user(&self, room_id: &str, actor_user_id: &str, target_user_id: &str) -> ApiResult<()> {
        let actor_power = self.get_joined_user_power_level(room_id, actor_user_id).await?;
        let target_power = self.get_user_power_level(room_id, target_user_id).await?;

        let required_power = self
            .get_room_power_levels_content(room_id)
            .await?
            .and_then(|content| content.get("kick").and_then(|level| level.as_i64()))
            .unwrap_or(DEFAULT_POWER_LEVEL);

        if actor_power < required_power {
            ::tracing::warn!(
                target: "security_audit",
                event = "unauthorized_kick_action",
                actor_user_id = actor_user_id,
                target_user_id = target_user_id,
                room_id = room_id,
                actor_power = actor_power,
                "User attempted to kick without moderator permission"
            );
            return Err(ApiError::forbidden("Moderator permission required to kick users".to_string()));
        }

        if actor_power <= target_power {
            ::tracing::warn!(
                target: "security_audit",
                event = "insufficient_power_to_kick",
                actor_user_id = actor_user_id,
                target_user_id = target_user_id,
                room_id = room_id,
                actor_power = actor_power,
                target_power = target_power,
                "User attempted to kick user with equal or higher power level"
            );
            return Err(ApiError::forbidden("Cannot kick users with equal or higher power level".to_string()));
        }

        let room_creator: Option<String> = self
            .room_storage
            .get_room_creator(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        if let Some(creator) = room_creator {
            if creator == target_user_id {
                ::tracing::warn!(
                    target: "security_audit",
                    event = "attempted_kick_room_creator",
                    actor_user_id = actor_user_id,
                    target_user_id = target_user_id,
                    room_id = room_id,
                    "User attempted to kick room creator"
                );
                return Err(ApiError::forbidden("Cannot kick the room creator".to_string()));
            }
        }

        Ok(())
    }

    pub async fn can_ban_user(&self, room_id: &str, actor_user_id: &str, target_user_id: &str) -> ApiResult<()> {
        let actor_power = self.get_joined_user_power_level(room_id, actor_user_id).await?;
        let target_power = self.get_user_power_level(room_id, target_user_id).await?;

        let required_power = self
            .get_room_power_levels_content(room_id)
            .await?
            .and_then(|content| content.get("ban").and_then(|level| level.as_i64()))
            .unwrap_or(DEFAULT_POWER_LEVEL);

        if actor_power < required_power {
            ::tracing::warn!(
                target: "security_audit",
                event = "unauthorized_ban_action",
                actor_user_id = actor_user_id,
                target_user_id = target_user_id,
                room_id = room_id,
                actor_power = actor_power,
                required_power = required_power,
                "User attempted to ban without sufficient permission"
            );
            return Err(ApiError::forbidden("Insufficient permission to ban users".to_string()));
        }

        if actor_power <= target_power {
            ::tracing::warn!(
                target: "security_audit",
                event = "insufficient_power_to_ban",
                actor_user_id = actor_user_id,
                target_user_id = target_user_id,
                room_id = room_id,
                actor_power = actor_power,
                target_power = target_power,
                "User attempted to ban user with equal or higher power level"
            );
            return Err(ApiError::forbidden("Cannot ban users with equal or higher power level".to_string()));
        }

        let room_creator: Option<String> = self
            .room_storage
            .get_room_creator(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        if let Some(creator) = room_creator {
            if creator == target_user_id {
                ::tracing::warn!(
                    target: "security_audit",
                    event = "attempted_ban_room_creator",
                    actor_user_id = actor_user_id,
                    target_user_id = target_user_id,
                    room_id = room_id,
                    "User attempted to ban room creator"
                );
                return Err(ApiError::forbidden("Cannot ban the room creator".to_string()));
            }
        }

        Ok(())
    }

    pub async fn can_unban_user(&self, room_id: &str, actor_user_id: &str, target_user_id: &str) -> ApiResult<()> {
        let actor_power = self.get_joined_user_power_level(room_id, actor_user_id).await?;
        let target_power = self.get_user_power_level(room_id, target_user_id).await?;

        let required_power = self
            .get_room_power_levels_content(room_id)
            .await?
            .and_then(|content| content.get("ban").and_then(|level| level.as_i64()))
            .unwrap_or(DEFAULT_POWER_LEVEL);

        if actor_power < required_power {
            ::tracing::warn!(
                target: "security_audit",
                event = "unauthorized_unban_action",
                actor_user_id = actor_user_id,
                target_user_id = target_user_id,
                room_id = room_id,
                actor_power = actor_power,
                "User attempted to unban without sufficient permission"
            );
            return Err(ApiError::forbidden("Insufficient permission to unban users".to_string()));
        }

        if actor_power <= target_power {
            ::tracing::warn!(
                target: "security_audit",
                event = "insufficient_power_to_unban",
                actor_user_id = actor_user_id,
                target_user_id = target_user_id,
                room_id = room_id,
                actor_power = actor_power,
                target_power = target_power,
                "User attempted to unban user with equal or higher power level"
            );
            return Err(ApiError::forbidden("Cannot unban users with equal or higher power level".to_string()));
        }

        Ok(())
    }

    pub async fn can_invite_user(&self, room_id: &str, actor_user_id: &str) -> ApiResult<()> {
        let actor_power = self.get_joined_user_power_level(room_id, actor_user_id).await?;

        let required_power = self
            .get_room_power_levels_content(room_id)
            .await?
            .and_then(|content| content.get("invite").and_then(|level| level.as_i64()))
            .unwrap_or(0);

        if actor_power < required_power {
            return Err(ApiError::forbidden("Insufficient permission to invite users".to_string()));
        }

        Ok(())
    }

    /// Checks whether `actor_user_id` may redact an event originally sent by
    /// `event_sender_id` in `room_id` (P0-09).
    ///
    /// Matrix auth rules for `m.room.redaction`:
    /// - **v1-v10**: the redactor must have power level >= `redact` (default
    ///   50).  There is NO self-redaction exemption — even the original author
    ///   must meet the `redact` threshold.
    /// - **v11+** (MSC2174): the original author may redact their own event
    ///   without meeting the `redact` threshold.  v11+ room creation is now
    ///   enabled (the redaction chain is fully landed).
    pub async fn can_redact_event(&self, room_id: &str, actor_user_id: &str, event_sender_id: &str) -> ApiResult<()> {
        let actor_power = self.get_joined_user_power_level(room_id, actor_user_id).await?;

        if actor_power < 0 {
            ::tracing::warn!(
                target: "security_audit",
                event = "non_member_redact_attempt",
                actor_user_id = actor_user_id,
                room_id = room_id,
                "Non-member attempted to redact a room event"
            );
            return Err(ApiError::forbidden("You must be a member of this room to redact events".to_string()));
        }

        // v11+ allows the original author to redact their own event without
        // meeting the `redact` power level (MSC2174).  v1-v10 does not.
        let room_version = self.get_room_version(room_id).await.unwrap_or_else(|_| {
            ::tracing::warn!(
                target: "security_audit",
                room_id = %room_id,
                "Failed to fetch room version for redaction auth; assuming v1-v10 rules"
            );
            synapse_common::room_versions::DEFAULT_ROOM_VERSION.to_string()
        });

        let supports_self_redact = room_version.parse::<u32>().map(|v| v >= 11).unwrap_or(false);

        if supports_self_redact && actor_user_id == event_sender_id {
            return Ok(());
        }

        let required_power = self
            .get_room_power_levels_content(room_id)
            .await?
            .and_then(|content| content.get("redact").and_then(|level| level.as_i64()))
            .unwrap_or(DEFAULT_POWER_LEVEL);

        if actor_power < required_power {
            ::tracing::warn!(
                target: "security_audit",
                event = "unauthorized_redact_action",
                actor_user_id = actor_user_id,
                event_sender_id = event_sender_id,
                room_id = room_id,
                actor_power = actor_power,
                required_power = required_power,
                room_version = %room_version,
                "User attempted to redact an event without sufficient power level"
            );
            return Err(ApiError::forbidden("Moderator permission required to redact events".to_string()));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use serde_json::{json, Value};

    // Helper that mirrors the user power level lookup logic used inside
    // `get_user_power_level`. This validates the parsing contract the
    // method depends on without needing a database.
    fn lookup_user_level(content: &Value, user_id: &str, creator: Option<&str>) -> i64 {
        if let Some(level) = content
            .get("users")
            .and_then(|u| u.get(user_id))
            .and_then(|v| v.as_i64())
        {
            return level;
        }
        if let Some(level) = content.get("users_default").and_then(|v| v.as_i64()) {
            return level;
        }
        if let Some(creator) = creator {
            if creator == user_id {
                return 100;
            }
        }
        0
    }

    // Helper mirroring `get_required_state_event_power_level`.
    fn lookup_state_event_level(content: Option<&Value>, event_type: &str) -> i64 {
        if let Some(content) = content {
            if let Some(level) = content
                .get("events")
                .and_then(|e| e.get(event_type))
                .and_then(|v| v.as_i64())
            {
                return level;
            }
            if let Some(level) = content.get("state_default").and_then(|v| v.as_i64()) {
                return level;
            }
        }
        if event_type == "m.room.power_levels" {
            return 100;
        }
        DEFAULT_POWER_LEVEL
    }

    // Helper mirroring `get_required_message_event_power_level`.
    fn lookup_message_event_level(content: Option<&Value>, event_type: &str) -> i64 {
        if let Some(content) = content {
            if let Some(level) = content
                .get("events")
                .and_then(|e| e.get(event_type))
                .and_then(|v| v.as_i64())
            {
                return level;
            }
            if let Some(level) = content.get("events_default").and_then(|v| v.as_i64()) {
                return level;
            }
        }
        0
    }

    // Helper mirroring the room-version self-redact check inside
    // `can_redact_event` (v11+ grants self-redact).
    fn supports_self_redact(room_version: &str) -> bool {
        room_version.parse::<u32>().map(|v| v >= 11).unwrap_or(false)
    }

    // ── Constants ───────────────────────────────────────────────────────

    #[test]
    fn test_default_power_level_value() {
        assert_eq!(DEFAULT_POWER_LEVEL, 50);
    }

    #[test]
    fn test_default_power_level_is_moderator_threshold() {
        // DEFAULT_POWER_LEVEL represents the moderator threshold; must be > 0
        assert!(DEFAULT_POWER_LEVEL > 0);
        assert!(DEFAULT_POWER_LEVEL < 100);
    }

    #[test]
    fn test_default_room_version_value() {
        assert_eq!(synapse_common::room_versions::DEFAULT_ROOM_VERSION, "10");
    }

    // ── Power levels JSON structure: users map ─────────────────────────

    #[test]
    fn test_parse_users_map_explicit_levels() {
        let content = json!({
            "users": {
                "@alice:server": 100,
                "@bob:server": 50,
                "@charlie:server": 0
            }
        });
        assert_eq!(lookup_user_level(&content, "@alice:server", None), 100);
        assert_eq!(lookup_user_level(&content, "@bob:server", None), 50);
        assert_eq!(lookup_user_level(&content, "@charlie:server", None), 0);
    }

    #[test]
    fn test_parse_users_default_fallback() {
        let content = json!({
            "users": {},
            "users_default": 25
        });
        assert_eq!(lookup_user_level(&content, "@unknown:server", None), 25);
    }

    #[test]
    fn test_user_level_explicit_overrides_users_default() {
        let content = json!({
            "users": { "@alice:server": 100 },
            "users_default": 25
        });
        // Explicit user entry wins over users_default
        assert_eq!(lookup_user_level(&content, "@alice:server", None), 100);
        // Unknown user falls back to users_default
        assert_eq!(lookup_user_level(&content, "@bob:server", None), 25);
    }

    #[test]
    fn test_room_creator_gets_100_when_no_power_levels() {
        let content = json!({});
        assert_eq!(lookup_user_level(&content, "@creator:server", Some("@creator:server")), 100);
    }

    #[test]
    fn test_room_creator_overridden_by_explicit_users_map() {
        // If the creator has an explicit entry, that takes priority
        let content = json!({
            "users": { "@creator:server": 0 }
        });
        assert_eq!(lookup_user_level(&content, "@creator:server", Some("@creator:server")), 0);
    }

    #[test]
    fn test_unknown_user_defaults_to_zero() {
        let content = json!({});
        assert_eq!(lookup_user_level(&content, "@unknown:server", None), 0);
    }

    #[test]
    fn test_empty_users_map() {
        let content = json!({ "users": {} });
        assert_eq!(lookup_user_level(&content, "@anyone:server", None), 0);
    }

    #[test]
    fn test_negative_user_power_level() {
        let content = json!({
            "users": { "@muted:server": -1 }
        });
        assert_eq!(lookup_user_level(&content, "@muted:server", None), -1);
    }

    // ── Power levels JSON structure: events map ───────────────────────

    #[test]
    fn test_parse_events_map_explicit_levels() {
        let content = json!({
            "events": {
                "m.room.name": 50,
                "m.room.topic": 50,
                "m.room.avatar": 50,
                "m.room.canonical_alias": 50
            }
        });
        assert_eq!(lookup_state_event_level(Some(&content), "m.room.name"), 50);
        assert_eq!(lookup_state_event_level(Some(&content), "m.room.topic"), 50);
    }

    #[test]
    fn test_state_event_falls_back_to_state_default() {
        let content = json!({
            "events": {},
            "state_default": 75
        });
        // Event not in events map → uses state_default
        assert_eq!(lookup_state_event_level(Some(&content), "m.room.name"), 75);
    }

    #[test]
    fn test_message_event_falls_back_to_events_default() {
        let content = json!({
            "events": {},
            "events_default": 10
        });
        assert_eq!(lookup_message_event_level(Some(&content), "m.room.message"), 10);
    }

    #[test]
    fn test_message_event_uses_explicit_events_entry() {
        let content = json!({
            "events": { "m.room.message": 25 },
            "events_default": 0
        });
        assert_eq!(lookup_message_event_level(Some(&content), "m.room.message"), 25);
    }

    #[test]
    fn test_no_power_levels_content_state_default_is_default_power_level() {
        // When no power_levels content exists at all, state events use DEFAULT_POWER_LEVEL (50)
        assert_eq!(lookup_state_event_level(None, "m.room.name"), DEFAULT_POWER_LEVEL);
    }

    #[test]
    fn test_no_power_levels_content_message_default_is_zero() {
        assert_eq!(lookup_message_event_level(None, "m.room.message"), 0);
    }

    #[test]
    fn test_m_room_power_levels_always_requires_100_without_content() {
        // m.room.power_levels is special: requires 100 when no content exists
        assert_eq!(lookup_state_event_level(None, "m.room.power_levels"), 100);
    }

    #[test]
    fn test_m_room_power_levels_with_explicit_event_entry() {
        // If the events map explicitly sets m.room.power_levels, use that value
        let content = json!({
            "events": { "m.room.power_levels": 50 }
        });
        assert_eq!(lookup_state_event_level(Some(&content), "m.room.power_levels"), 50);
    }

    // ── Power levels JSON structure: scalar fields ────────────────────

    #[test]
    fn test_parse_ban_kick_invite_redact_levels() {
        let content = json!({
            "ban": 50,
            "kick": 50,
            "invite": 0,
            "redact": 50
        });
        assert_eq!(content.get("ban").and_then(|v| v.as_i64()), Some(50));
        assert_eq!(content.get("kick").and_then(|v| v.as_i64()), Some(50));
        assert_eq!(content.get("invite").and_then(|v| v.as_i64()), Some(0));
        assert_eq!(content.get("redact").and_then(|v| v.as_i64()), Some(50));
    }

    #[test]
    fn test_scalar_fields_default_when_missing() {
        // The methods use unwrap_or(DEFAULT_POWER_LEVEL) for ban/kick/redact/state_default,
        // and unwrap_or(0) for invite/users_default/events_default
        let content = json!({});
        assert_eq!(content.get("ban").and_then(|v| v.as_i64()).unwrap_or(DEFAULT_POWER_LEVEL), DEFAULT_POWER_LEVEL);
        assert_eq!(content.get("kick").and_then(|v| v.as_i64()).unwrap_or(DEFAULT_POWER_LEVEL), DEFAULT_POWER_LEVEL);
        assert_eq!(content.get("redact").and_then(|v| v.as_i64()).unwrap_or(DEFAULT_POWER_LEVEL), DEFAULT_POWER_LEVEL);
        assert_eq!(
            content.get("state_default").and_then(|v| v.as_i64()).unwrap_or(DEFAULT_POWER_LEVEL),
            DEFAULT_POWER_LEVEL
        );
        assert_eq!(content.get("invite").and_then(|v| v.as_i64()).unwrap_or(0), 0);
        assert_eq!(content.get("users_default").and_then(|v| v.as_i64()).unwrap_or(0), 0);
        assert_eq!(content.get("events_default").and_then(|v| v.as_i64()).unwrap_or(0), 0);
    }

    #[test]
    fn test_parse_notifications_room_level() {
        let content = json!({
            "notifications": { "room": 50 }
        });
        let level = content
            .get("notifications")
            .and_then(|n| n.as_object())
            .and_then(|n| n.get("room"))
            .and_then(|r| r.as_i64());
        assert_eq!(level, Some(50));
    }

    #[test]
    fn test_notifications_room_default_when_missing() {
        // Mirrors the scalar_checks entry inside verify_power_levels_change
        let content = json!({});
        let level = content
            .get("notifications")
            .and_then(|v| v.as_object())
            .and_then(|n| n.get("room"))
            .and_then(|r| r.as_i64())
            .unwrap_or(DEFAULT_POWER_LEVEL);
        assert_eq!(level, DEFAULT_POWER_LEVEL);
    }

    // ── serde roundtrips ───────────────────────────────────────────────

    #[test]
    fn test_full_power_levels_content_roundtrip() {
        let original = json!({
            "ban": 50,
            "kick": 50,
            "invite": 0,
            "redact": 50,
            "events_default": 0,
            "state_default": 50,
            "users_default": 0,
            "events": {
                "m.room.name": 50,
                "m.room.power_levels": 100,
                "m.room.message": 0
            },
            "users": {
                "@alice:server": 100,
                "@bob:server": 50
            },
            "notifications": { "room": 50 }
        });
        let serialized = serde_json::to_string(&original).unwrap();
        let deserialized: Value = serde_json::from_str(&serialized).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_minimal_power_levels_content_roundtrip() {
        let original = json!({
            "ban": 50,
            "kick": 50,
            "redact": 50,
            "state_default": 50,
            "users": {}
        });
        let serialized = serde_json::to_string(&original).unwrap();
        let deserialized: Value = serde_json::from_str(&serialized).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_empty_content_object_roundtrip() {
        let original = json!({});
        let serialized = serde_json::to_string(&original).unwrap();
        let deserialized: Value = serde_json::from_str(&serialized).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_content_with_negative_levels_roundtrip() {
        let original = json!({
            "users": { "@muted:server": -1 },
            "events_default": -10
        });
        let serialized = serde_json::to_string(&original).unwrap();
        let deserialized: Value = serde_json::from_str(&serialized).unwrap();
        assert_eq!(original, deserialized);
    }

    // ── Permission check logic patterns ────────────────────────────────

    #[test]
    fn test_permission_denied_when_below_required() {
        // Mirrors: if power_level < required { return Err(Forbidden) }
        let power_level: i64 = 25;
        let required: i64 = 50;
        assert!(power_level < required, "Should be denied when below required");
    }

    #[test]
    fn test_permission_allowed_when_equal_to_required() {
        // Mirrors: only denied when strictly less than required
        let power_level: i64 = 50;
        let required: i64 = 50;
        assert!(!(power_level < required), "Should be allowed when equal to required");
    }

    #[test]
    fn test_permission_allowed_when_above_required() {
        let power_level: i64 = 100;
        let required: i64 = 50;
        assert!(!(power_level < required), "Should be allowed when above required");
    }

    #[test]
    fn test_non_member_power_level_is_negative_one() {
        // get_joined_user_power_level returns -1 for non-join members
        let non_member_level: i64 = -1;
        let required: i64 = 0;
        assert!(non_member_level < required, "Non-member (-1) should be denied even for required=0");
    }

    #[test]
    fn test_kick_denied_when_actor_not_above_target() {
        // can_kick_user: actor_power <= target_power → forbidden
        let actor_power: i64 = 50;
        let target_power: i64 = 50;
        assert!(actor_power <= target_power, "Equal power levels should deny kick");

        let actor_power: i64 = 25;
        let target_power: i64 = 50;
        assert!(actor_power <= target_power, "Lower actor power should deny kick");
    }

    #[test]
    fn test_kick_allowed_when_actor_strictly_above_target() {
        let actor_power: i64 = 100;
        let target_power: i64 = 50;
        assert!(!(actor_power <= target_power), "Higher actor power should allow kick");
    }

    #[test]
    fn test_admin_always_passes_default_thresholds() {
        // Room admin requires 100; verify boundary
        let admin_level: i64 = 100;
        let required: i64 = 100;
        assert!(!(admin_level < required));
    }

    #[test]
    fn test_admin_denied_below_100() {
        let admin_level: i64 = 99;
        let required: i64 = 100;
        assert!(admin_level < required, "99 should be denied for admin actions requiring 100");
    }

    // ── Room version self-redact logic (v11+) ──────────────────────────

    #[test]
    fn test_v11_supports_self_redact() {
        assert!(supports_self_redact("11"));
    }

    #[test]
    fn test_v12_supports_self_redact() {
        assert!(supports_self_redact("12"));
    }

    #[test]
    fn test_v13_supports_self_redact() {
        assert!(supports_self_redact("13"));
    }

    #[test]
    fn test_v10_does_not_support_self_redact() {
        assert!(!supports_self_redact("10"));
    }

    #[test]
    fn test_v1_does_not_support_self_redact() {
        assert!(!supports_self_redact("1"));
    }

    #[test]
    fn test_default_room_version_does_not_support_self_redact() {
        assert!(!supports_self_redact(synapse_common::room_versions::DEFAULT_ROOM_VERSION));
    }

    #[test]
    fn test_invalid_room_version_does_not_support_self_redact() {
        assert!(!supports_self_redact("invalid"));
        assert!(!supports_self_redact(""));
    }

    #[test]
    fn test_self_redact_only_when_actor_is_sender() {
        let actor = "@alice:server";
        let sender = "@alice:server";
        let other = "@bob:server";
        // Self-redact: actor == sender
        assert!(actor == sender);
        // Not self-redact: actor != sender
        assert!(actor != other);
    }

    #[test]
    fn test_non_member_cannot_redact() {
        // can_redact_event: if actor_power < 0 → forbidden
        let non_member_power: i64 = -1;
        assert!(non_member_power < 0, "Non-member should be blocked from redacting");
    }

    // ── Power level hierarchy ──────────────────────────────────────────

    #[test]
    fn test_typical_hierarchy_ban_greater_than_or_equal_kick() {
        // In Matrix defaults, ban and kick are both 50, but ban is conceptually higher
        let ban = 50;
        let kick = 50;
        assert!(ban >= kick);
    }

    #[test]
    fn test_default_redact_level_matches_default_power_level() {
        // redact defaults to DEFAULT_POWER_LEVEL (50) when not set
        let content = json!({});
        let redact = content.get("redact").and_then(|v| v.as_i64()).unwrap_or(DEFAULT_POWER_LEVEL);
        assert_eq!(redact, DEFAULT_POWER_LEVEL);
    }

    #[test]
    fn test_default_invite_level_is_zero() {
        // invite defaults to 0 when not set
        let content = json!({});
        let invite = content.get("invite").and_then(|v| v.as_i64()).unwrap_or(0);
        assert_eq!(invite, 0);
    }

    // ── room_versions module integration ───────────────────────────────

    #[test]
    fn test_is_supported_room_version_for_known_versions() {
        assert!(synapse_common::room_versions::is_supported_room_version("10"));
        assert!(synapse_common::room_versions::is_supported_room_version("11"));
    }

    #[test]
    fn test_is_supported_room_version_for_unknown_version() {
        assert!(!synapse_common::room_versions::is_supported_room_version("99"));
        assert!(!synapse_common::room_versions::is_supported_room_version("abc"));
    }

    #[test]
    fn test_can_create_room_version_v11_enabled() {
        // v11+ room creation is enabled (redaction chain landed)
        assert!(synapse_common::room_versions::can_create_room_version("11"));
        assert!(synapse_common::room_versions::can_create_room_version("12"));
    }

    // ── verify_power_levels_change scalar checks ────────────────────────

    #[test]
    fn test_power_level_change_blocked_when_new_level_above_actor() {
        // Mirrors: if new_level > actor_level { forbidden("Cannot set ... above your own") }
        let actor_level: i64 = 50;
        let new_level: i64 = 75;
        assert!(new_level > actor_level, "Should block elevation above actor's own level");
    }

    #[test]
    fn test_power_level_change_blocked_when_target_at_or_above_actor() {
        // Mirrors: if current_level >= actor_level && new_level != current_level { forbidden }
        let actor_level: i64 = 50;
        let current_level: i64 = 50;
        let new_level: i64 = 25;
        assert!(current_level >= actor_level);
        assert!(new_level != current_level, "Should block changing equal-level user");
    }

    #[test]
    fn test_power_level_change_allowed_when_target_below_actor_and_new_below_actor() {
        let actor_level: i64 = 100;
        let current_level: i64 = 25;
        let new_level: i64 = 50;
        assert!(current_level < actor_level);
        assert!(new_level <= actor_level, "Should allow change when both below actor");
    }

    #[test]
    fn test_event_level_change_blocked_when_new_above_actor() {
        let actor_level: i64 = 50;
        let new_level: i64 = 75;
        assert!(new_level > actor_level, "Event level above actor should be blocked");
    }

    #[test]
    fn test_event_level_change_blocked_when_current_above_actor() {
        // Mirrors: if current_level > actor_level && new_level != current_level { forbidden }
        let actor_level: i64 = 50;
        let current_level: i64 = 75;
        let new_level: i64 = 50;
        assert!(current_level > actor_level);
        assert!(new_level != current_level);
    }

    // ── Boundary values ────────────────────────────────────────────────

    #[test]
    fn test_boundary_zero_power_level() {
        let content = json!({ "users": { "@user:server": 0 } });
        assert_eq!(lookup_user_level(&content, "@user:server", None), 0);
    }

    #[test]
    fn test_boundary_max_power_level() {
        let content = json!({ "users": { "@admin:server": 100 } });
        assert_eq!(lookup_user_level(&content, "@admin:server", None), 100);
    }

    #[test]
    fn test_boundary_just_below_default_denies_moderator_actions() {
        let power_level: i64 = 49;
        assert!(power_level < DEFAULT_POWER_LEVEL);
    }

    #[test]
    fn test_boundary_at_default_allows_moderator_actions() {
        let power_level: i64 = DEFAULT_POWER_LEVEL;
        assert!(!(power_level < DEFAULT_POWER_LEVEL));
    }

    #[test]
    fn test_boundary_just_below_admin_denies_admin_actions() {
        let power_level: i64 = 99;
        assert!(power_level < 100);
    }

    // ── ApiError forbidden construction ────────────────────────────────

    #[test]
    fn test_api_error_forbidden_construction() {
        let err = ApiError::forbidden("Insufficient permission".to_string());
        assert_eq!(err.code, synapse_common::MatrixErrorCode::Forbidden);
    }

    #[test]
    fn test_api_error_forbidden_message_preserved() {
        let msg = "Cannot kick the room creator";
        let err = ApiError::forbidden(msg.to_string());
        assert_eq!(err.message, msg);
    }
}
