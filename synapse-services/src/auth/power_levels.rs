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
