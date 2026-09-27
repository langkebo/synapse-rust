use super::AuthService;
use super::DEFAULT_POWER_LEVEL;
use std::collections::BTreeSet;
use synapse_common::*;

/// The power level of a room v12+ creator (MSC4289).
///
/// MSC4289 gives the create event's `sender` and every `additional_creators`
/// entry **unlimited** power: they cannot be demoted by any `m.room.power_levels`
/// event, and any action gated on a numeric threshold passes for them. `i64::MAX`
/// is the sentinel for that — every consumer compares power levels, so a value
/// above any representable threshold is exactly "unlimited" without a second
/// code path per call site.
pub const CREATOR_POWER_LEVEL: i64 = i64::MAX;

impl AuthService {
    /// See [`get_user_power_level`].
    pub async fn get_user_power_level(&self, room_id: &str, user_id: &str) -> ApiResult<i64> {
        let membership = self
            .member_storage
            .get_membership_state(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Database error", e))?;

        if membership.is_none() {
            return Ok(-1);
        }

        let (creators, room_version) = self.room_creators_and_version(room_id).await?;

        // MSC4289 (room v12+): a creator's power is unlimited and **cannot be
        // demoted** by a `m.room.power_levels` event, so the creator check runs
        // *before* the event is read. Running it after (as this used to) let a
        // power-levels event set the creator to any value, including 0 — the
        // defect the plan records as G-32.
        //
        // The version must be **stated** by the create event: granting unlimited
        // power on an unknown version would be a fail-open privilege escalation,
        // so `None` (no `content.room_version`, or no create event at all) keeps
        // the pre-v12 behaviour.
        let is_v12_plus = room_version
            .as_deref()
            .is_some_and(|version| synapse_common::room_versions::room_version_at_least(version, 12));
        if is_v12_plus && creators.contains(user_id) {
            return Ok(CREATOR_POWER_LEVEL);
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

        // Below v12 the creator's fallback is the long-standing 100: v1-v11 have
        // no unlimited-creator concept, and their behaviour must not change.
        if creators.contains(user_id) {
            return Ok(100);
        }

        Ok(0)
    }

    /// See [`get_joined_user_power_level`].
    pub(crate) async fn get_joined_user_power_level(&self, room_id: &str, user_id: &str) -> ApiResult<i64> {
        let membership = self
            .member_storage
            .get_membership_state(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Database error", e))?;

        match membership {
            Some(m) if m == "join" => self.get_user_power_level(room_id, user_id).await,
            _ => Ok(-1),
        }
    }

    /// See [`get_room_power_levels_content`].
    pub(crate) async fn get_room_power_levels_content(&self, room_id: &str) -> ApiResult<Option<serde_json::Value>> {
        let events = self
            .event_reader
            .get_state_events_by_type(room_id, "m.room.power_levels")
            .await
            .map_err(|e| ApiError::internal_with_cause("Database error", e))?;
        Ok(events.first().map(|event| event.content.clone()))
    }

    /// 解析房间**创建者集合**与房间版本：从 `m.room.create` 状态事件一次读出。
    ///
    /// 创建者定义（MSC4289 / Matrix）：
    /// * v1–v10 的 `content.creator`；
    /// * v11+ 的创建者即 create 事件的 `sender`（`content.creator` 已被移除）；
    /// * v12+ 再加上 `content.additional_creators` 的每一项。
    ///
    /// 三者一并收集（对任意版本都成立：低版本里 sender 与 `content.creator` 本就应当一致）。
    /// 事件缺失时回退到 rooms 表；版本回落到 `DEFAULT_ROOM_VERSION`。
    ///
    /// 合并成一个 helper 是因为每次授权都要同时拿到两者，而它们同源于同一条 create 事件
    /// —— 分开取会让每次 power-level 查询多一次状态读取。
    async fn room_creators_and_version(&self, room_id: &str) -> ApiResult<(BTreeSet<String>, Option<String>)> {
        let events = self
            .event_reader
            .get_state_events_by_type(room_id, "m.room.create")
            .await
            .map_err(|e| ApiError::internal_with_cause("Database error", e))?;

        if let Some(event) = events.first() {
            // One implementation of "who is a creator", shared with the inbound
            // federation auth rules (`synapse_common::room_creator`).
            let sender = if event.sender.is_empty() { event.user_id.as_deref().unwrap_or("") } else { &event.sender };
            let creators = synapse_common::room_creator::creators_from_create_event(sender, &event.content);
            // `None` when the create event does not state one: an unknown version
            // must never be *assumed* to be v12+ — see `get_user_power_level`.
            let version = event.content.get("room_version").and_then(|v| v.as_str()).map(str::to_string);
            return Ok((creators, version));
        }

        let creator = self
            .room_storage
            .get_room_creator(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Database error", e))?;
        Ok((creator.into_iter().collect(), None))
    }

    /// The room's creator **set** (MSC4289: `sender` ∪ `additional_creators`,
    /// plus `content.creator` for v1-v10).
    pub(crate) async fn resolve_room_creators(&self, room_id: &str) -> ApiResult<BTreeSet<String>> {
        Ok(self.room_creators_and_version(room_id).await?.0)
    }

    /// Returns the room version string (e.g. `"10"`) from the `m.room.create`
    /// state event, or `DEFAULT_ROOM_VERSION` if not set.
    pub(crate) async fn get_room_version(&self, room_id: &str) -> ApiResult<String> {
        Ok(self
            .room_creators_and_version(room_id)
            .await?
            .1
            .unwrap_or_else(|| synapse_common::room_versions::DEFAULT_ROOM_VERSION.to_string()))
    }

    /// See [`get_required_state_event_power_level`].
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

    /// See [`get_required_message_event_power_level`].
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

    /// See [`verify_message_event_write`].
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

    /// See [`verify_state_event_write`].
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

    /// See [`verify_power_levels_change`].
    pub async fn verify_power_levels_change(
        &self,
        room_id: &str,
        user_id: &str,
        new_content: &serde_json::Value,
    ) -> ApiResult<()> {
        let actor_level = self.get_joined_user_power_level(room_id, user_id).await?;
        let current_content = self.get_room_power_levels_content(room_id).await?;
        let new_power_levels_content = new_content;

        // MSC4289 rule 10.4 (room v12+): `users` must not name a creator.
        //
        // A creator's power is unlimited (see `get_user_power_level`), so listing
        // one here is either redundant or an attempt to pin a number that the
        // event can never enforce. Checked before any threshold comparison so the
        // rejection reason is the real one.
        //
        // This runs on the *client* path. The local creation sequence writes its
        // initial power_levels directly (`LifecycleService::write_creation_event`)
        // and does not come through here, so building a room whose first
        // power_levels names the creator does not self-reject.
        let (creators, room_version) = self.room_creators_and_version(room_id).await?;
        if room_version
            .as_deref()
            .is_some_and(|version| synapse_common::room_versions::room_version_at_least(version, 12))
        {
            if let Some(new_users) = new_power_levels_content.get("users").and_then(|u| u.as_object()) {
                if let Some(creator) = new_users.keys().find(|target| creators.contains(*target)) {
                    ::tracing::warn!(
                        target: "security_audit",
                        event = "power_levels_names_room_creator",
                        user_id = user_id,
                        room_id = room_id,
                        creator = %creator,
                        "power_levels.users must not name a room creator (MSC4289 rule 10.4)"
                    );
                    return Err(ApiError::forbidden(
                        "power_levels.users must not name a room creator (MSC4289 rule 10.4)".to_string(),
                    ));
                }
            }
        }

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

    /// See [`verify_room_moderator`].
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

    /// See [`verify_room_admin`].
    pub async fn verify_room_admin(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        let power_level = self.get_user_power_level(room_id, user_id).await?;

        let required_level = 100;

        if power_level < required_level {
            return Err(ApiError::forbidden("Room admin permission required".to_string()));
        }

        Ok(())
    }

    /// See [`can_kick_user`].
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

        // MSC4289: **every** creator is protected, not just the single "the"
        // creator — `sender` ∪ `additional_creators` for v12+, and the one
        // `content.creator` below it.
        if self.resolve_room_creators(room_id).await?.contains(target_user_id) {
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

        Ok(())
    }

    /// See [`can_ban_user`].
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

        // MSC4289: **every** creator is protected, not just the single "the"
        // creator — `sender` ∪ `additional_creators` for v12+, and the one
        // `content.creator` below it.
        if self.resolve_room_creators(room_id).await?.contains(target_user_id) {
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

        Ok(())
    }

    /// See [`can_unban_user`].
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

    /// See [`can_invite_user`].
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
    //! T1 修复的 TDD 测试：对 13 个授权判定函数的真实单元测试。
    //!
    //! 此前本模块（574 行授权原语）零直接单测，`tests/unit/security_critical_tests.rs`
    //! 中的同名测试仅内联重算整数比较、从不调用真实服务。以下测试通过内存 mock
    //! （InMemoryMemberStore / InMemoryEventStore）构造房间状态，断言真实放行/拒绝路径。

    use super::super::test_harness::{build_test_auth_service, TestAuthHarness};
    use super::CREATOR_POWER_LEVEL;
    use synapse_storage::event::RoomEvent;
    use synapse_storage::membership::RoomMember;

    const ROOM: &str = "!room:test.server";
    const ALICE: &str = "@alice:test.server"; // power 100（创建者）
    const BOB: &str = "@bob:test.server"; // power 50（moderator）
    const CAROL: &str = "@carol:test.server"; // power 0（普通成员）
    const DAVE: &str = "@dave:test.server"; // 非成员

    fn member(room_id: &str, user_id: &str, membership: &str) -> RoomMember {
        RoomMember {
            room_id: room_id.to_string(),
            user_id: user_id.to_string(),
            sender: None,
            membership: membership.to_string(),
            event_id: None,
            event_type: None,
            display_name: None,
            avatar_url: None,
            is_banned: None,
            invite_token: None,
            updated_ts: None,
            joined_ts: None,
            left_ts: None,
            reason: None,
            banned_by: None,
            ban_reason: None,
            banned_ts: None,
            join_reason: None,
        }
    }

    fn state_event(
        room_id: &str,
        event_id: &str,
        sender: &str,
        event_type: &str,
        content: serde_json::Value,
    ) -> RoomEvent {
        RoomEvent {
            event_id: event_id.to_string(),
            room_id: room_id.to_string(),
            user_id: sender.to_string(),
            event_type: event_type.to_string(),
            content,
            state_key: Some(String::new()),
            depth: 1,
            origin_server_ts: 1_700_000_000_000,
            processed_ts: 1_700_000_000_000,
            not_before: 0,
            status: None,
            origin: "test.server".to_string(),
            stream_ordering: None,
            redacts: None,
        }
    }

    fn default_power_levels() -> serde_json::Value {
        serde_json::json!({
            "users": { ALICE: 100, BOB: 50 },
            "users_default": 0,
            "events_default": 0,
            "state_default": 50,
            "ban": 50,
            "kick": 50,
            "redact": 50,
            "invite": 0
        })
    }

    /// 标准房间：alice 创建（create 事件提供创建者），power_levels 如上。
    async fn harness_with_room() -> TestAuthHarness {
        let h = build_test_auth_service();
        h.member_store
            .seed_members(vec![member(ROOM, ALICE, "join"), member(ROOM, BOB, "join"), member(ROOM, CAROL, "join")])
            .await;
        h.event_store
            .seed_events(vec![
                state_event(
                    ROOM,
                    "$create",
                    ALICE,
                    "m.room.create",
                    serde_json::json!({"creator": ALICE, "room_version": "10"}),
                ),
                state_event(ROOM, "$pl", ALICE, "m.room.power_levels", default_power_levels()),
            ])
            .await;
        h
    }

    // ── get_user_power_level ─────────────────────────────────────────

    #[tokio::test]
    async fn power_level_explicit_entry_wins() {
        let h = harness_with_room().await;
        assert_eq!(h.service.get_user_power_level(ROOM, ALICE).await.unwrap(), 100);
        assert_eq!(h.service.get_user_power_level(ROOM, BOB).await.unwrap(), 50);
    }

    #[tokio::test]
    async fn power_level_falls_back_to_users_default() {
        let h = harness_with_room().await;
        // CAROL 在 users 中无条目 → users_default = 0
        assert_eq!(h.service.get_user_power_level(ROOM, CAROL).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn power_level_non_member_is_minus_one() {
        let h = harness_with_room().await;
        assert_eq!(h.service.get_user_power_level(ROOM, DAVE).await.unwrap(), -1);
    }

    #[tokio::test]
    async fn power_level_creator_fallback_when_no_power_levels_event() {
        let h = build_test_auth_service();
        let room2 = "!room2:test.server";
        h.member_store.seed_members(vec![member(room2, DAVE, "join"), member(room2, BOB, "join")]).await;
        // 只有 create 事件，无 power_levels 事件：创建者 100，其他成员 0
        h.event_store
            .seed_events(vec![state_event(
                room2,
                "$create2",
                DAVE,
                "m.room.create",
                serde_json::json!({"creator": DAVE, "room_version": "10"}),
            )])
            .await;
        assert_eq!(h.service.get_user_power_level(room2, DAVE).await.unwrap(), 100);
        assert_eq!(h.service.get_user_power_level(room2, BOB).await.unwrap(), 0);
    }

    // ── verify_message_event_write / verify_state_event_write ────────

    #[tokio::test]
    async fn message_write_allowed_for_joined_member() {
        let h = harness_with_room().await;
        assert!(h.service.verify_message_event_write(ROOM, CAROL, "m.room.message").await.is_ok());
    }

    #[tokio::test]
    async fn message_write_rejected_for_non_member() {
        let h = harness_with_room().await;
        assert!(h.service.verify_message_event_write(ROOM, DAVE, "m.room.message").await.is_err());
    }

    #[tokio::test]
    async fn state_write_respects_state_default() {
        let h = harness_with_room().await;
        // state_default = 50：BOB(50) 放行，CAROL(0) 拒绝
        assert!(h.service.verify_state_event_write(ROOM, BOB, "m.room.topic").await.is_ok());
        assert!(h.service.verify_state_event_write(ROOM, CAROL, "m.room.topic").await.is_err());
    }

    #[tokio::test]
    async fn state_write_respects_events_override() {
        let h = build_test_auth_service();
        let room = "!room3:test.server";
        h.member_store.seed_members(vec![member(room, BOB, "join")]).await;
        let mut pl = default_power_levels();
        pl["events"] = serde_json::json!({"m.room.topic": 75});
        h.event_store
            .seed_events(vec![
                state_event(room, "$create3", BOB, "m.room.create", serde_json::json!({"creator": BOB})),
                state_event(room, "$pl3", BOB, "m.room.power_levels", pl),
            ])
            .await;
        // events["m.room.topic"] = 75 > BOB(50) → 拒绝
        assert!(h.service.verify_state_event_write(room, BOB, "m.room.topic").await.is_err());
    }

    // ── verify_power_levels_change ───────────────────────────────────

    #[tokio::test]
    async fn power_levels_change_cannot_elevate_above_self() {
        let h = harness_with_room().await;
        // BOB(50) 试图把 CAROL 提到 75（超过自己）→ 拒绝
        let new = serde_json::json!({"users": {CAROL: 75}});
        assert!(h.service.verify_power_levels_change(ROOM, BOB, &new).await.is_err());
        // ALICE(100) 把 BOB 提到 75 → 放行
        let ok = serde_json::json!({"users": {BOB: 75}});
        assert!(h.service.verify_power_levels_change(ROOM, ALICE, &ok).await.is_ok());
    }

    #[tokio::test]
    async fn power_levels_change_cannot_touch_equal_or_higher_user() {
        let h = harness_with_room().await;
        // BOB(50) 试图改动 ALICE(100) 的等级 → 拒绝
        let new = serde_json::json!({"users": {ALICE: 0}});
        assert!(h.service.verify_power_levels_change(ROOM, BOB, &new).await.is_err());
    }

    #[tokio::test]
    async fn power_levels_change_scalar_above_self_rejected() {
        let h = harness_with_room().await;
        // BOB(50) 试图把 kick 阈值改成 75（超过自己）→ 拒绝
        let new = serde_json::json!({"kick": 75});
        assert!(h.service.verify_power_levels_change(ROOM, BOB, &new).await.is_err());
    }

    // ── verify_room_moderator / verify_room_admin ────────────────────

    #[tokio::test]
    async fn moderator_check_uses_state_default() {
        let h = harness_with_room().await;
        assert!(h.service.verify_room_moderator(ROOM, BOB).await.is_ok());
        assert!(h.service.verify_room_moderator(ROOM, CAROL).await.is_err());
    }

    #[tokio::test]
    async fn admin_check_requires_100() {
        let h = harness_with_room().await;
        assert!(h.service.verify_room_admin(ROOM, ALICE).await.is_ok());
        assert!(h.service.verify_room_admin(ROOM, BOB).await.is_err());
    }

    // ── can_kick_user / can_ban_user / can_unban_user ────────────────

    #[tokio::test]
    async fn kick_allowed_for_moderator_on_lower_power_member() {
        let h = harness_with_room().await;
        assert!(h.service.can_kick_user(ROOM, BOB, CAROL).await.is_ok());
    }

    #[tokio::test]
    async fn kick_rejected_when_actor_below_kick_threshold() {
        let h = harness_with_room().await;
        // CAROL(0) < kick(50)
        assert!(h.service.can_kick_user(ROOM, CAROL, BOB).await.is_err());
    }

    #[tokio::test]
    async fn kick_rejected_when_target_equal_or_higher_power() {
        let h = harness_with_room().await;
        // BOB(50) 不能踢 ALICE(100)
        assert!(h.service.can_kick_user(ROOM, BOB, ALICE).await.is_err());
        // 同等级也不可（严格大于才可踢）
        assert!(h.service.can_kick_user(ROOM, BOB, BOB).await.is_err());
    }

    #[tokio::test]
    async fn kick_rejected_when_target_is_room_creator() {
        let h = harness_with_room().await;
        // ALICE(100) 权力足够，但目标是创建者（create 事件 creator = ALICE 自身不行；
        // 构造 BOB 为创建者、ALICE 为管理员的房间来验证创建者豁免）
        let room = "!room4:test.server";
        h.member_store.seed_members(vec![member(room, ALICE, "join"), member(room, BOB, "join")]).await;
        h.event_store
            .seed_events(vec![
                state_event(
                    room,
                    "$create4",
                    BOB,
                    "m.room.create",
                    serde_json::json!({"creator": BOB, "room_version": "10"}),
                ),
                state_event(room, "$pl4", BOB, "m.room.power_levels", default_power_levels()),
            ])
            .await;
        assert!(h.service.can_kick_user(room, ALICE, BOB).await.is_err(), "创建者不可被踢");
        assert!(h.service.can_ban_user(room, ALICE, BOB).await.is_err(), "创建者不可被封禁");
    }

    #[tokio::test]
    async fn ban_allowed_and_threshold_enforced() {
        let h = harness_with_room().await;
        assert!(h.service.can_ban_user(ROOM, BOB, CAROL).await.is_ok());
        assert!(h.service.can_ban_user(ROOM, CAROL, BOB).await.is_err());
        assert!(h.service.can_ban_user(ROOM, BOB, ALICE).await.is_err());
    }

    #[tokio::test]
    async fn unban_requires_ban_threshold_and_strictly_higher_power() {
        let h = harness_with_room().await;
        assert!(h.service.can_unban_user(ROOM, BOB, CAROL).await.is_ok());
        assert!(h.service.can_unban_user(ROOM, CAROL, BOB).await.is_err());
        assert!(h.service.can_unban_user(ROOM, BOB, ALICE).await.is_err());
    }

    // ── can_invite_user ──────────────────────────────────────────────

    #[tokio::test]
    async fn invite_uses_default_zero_threshold() {
        let h = harness_with_room().await;
        assert!(h.service.can_invite_user(ROOM, CAROL).await.is_ok());
        assert!(h.service.can_invite_user(ROOM, DAVE).await.is_err(), "非成员 power -1 < invite 0");
    }

    // ── can_redact_event（v1-v10 vs v11+ 规则）────────────────────────

    #[tokio::test]
    async fn redact_v10_requires_redact_threshold_even_for_self() {
        let h = harness_with_room().await;
        // v10 房间：无自我撤回豁免。BOB(50) >= redact(50) 放行；CAROL(0) 即使撤回自己的也拒绝
        assert!(h.service.can_redact_event(ROOM, BOB, CAROL).await.is_ok());
        assert!(h.service.can_redact_event(ROOM, CAROL, CAROL).await.is_err());
    }

    #[tokio::test]
    async fn redact_v11_allows_self_redaction_below_threshold() {
        let h = build_test_auth_service();
        let room = "!room5:test.server";
        h.member_store.seed_members(vec![member(room, CAROL, "join"), member(room, BOB, "join")]).await;
        h.event_store
            .seed_events(vec![
                state_event(
                    room,
                    "$create5",
                    BOB,
                    "m.room.create",
                    serde_json::json!({"creator": BOB, "room_version": "11"}),
                ),
                state_event(room, "$pl5", BOB, "m.room.power_levels", default_power_levels()),
            ])
            .await;
        // v11（MSC2174）：CAROL(0) 可撤回自己的事件，但不能撤回他人的
        assert!(h.service.can_redact_event(room, CAROL, CAROL).await.is_ok());
        assert!(h.service.can_redact_event(room, CAROL, BOB).await.is_err());
    }

    #[tokio::test]
    async fn redact_rejected_for_non_member() {
        let h = harness_with_room().await;
        assert!(h.service.can_redact_event(ROOM, DAVE, DAVE).await.is_err());
    }

    // ── get_required_*_power_level ───────────────────────────────────

    #[tokio::test]
    async fn required_levels_follow_content_and_defaults() {
        let h = harness_with_room().await;
        // state_default = 50
        assert_eq!(h.service.get_required_state_event_power_level(ROOM, "m.room.topic").await.unwrap(), 50);
        // m.room.power_levels 在有 power_levels 事件时走 state_default（50），
        // 特例 100 仅在无 power_levels 事件时作为兜底。
        assert_eq!(h.service.get_required_state_event_power_level(ROOM, "m.room.power_levels").await.unwrap(), 50);
        // events_default = 0
        assert_eq!(h.service.get_required_message_event_power_level(ROOM, "m.room.message").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn required_state_event_power_level_fallback_100_when_no_pl_event() {
        // 无 power_levels 事件时，m.room.power_levels 特例返回 100
        let h = build_test_auth_service();
        let room = "!no_pl:test.server";
        h.member_store.seed_members(vec![member(room, ALICE, "join")]).await;
        h.event_store
            .seed_events(vec![state_event(
                room,
                "$create_no_pl",
                ALICE,
                "m.room.create",
                serde_json::json!({"creator": ALICE}),
            )])
            .await;
        assert_eq!(h.service.get_required_state_event_power_level(room, "m.room.power_levels").await.unwrap(), 100);
    }

    // ── E-2 / MSC4289: creators have unlimited power in v12+ ───────────────

    /// A v12 room whose create event names `additional_creators` and whose
    /// power_levels tries to demote them.
    async fn v12_harness(create_content: serde_json::Value, pl: serde_json::Value) -> TestAuthHarness {
        let h = build_test_auth_service();
        h.member_store
            .seed_members(vec![member(ROOM, ALICE, "join"), member(ROOM, BOB, "join"), member(ROOM, CAROL, "join")])
            .await;
        h.event_store
            .seed_events(vec![
                state_event(ROOM, "$create", ALICE, "m.room.create", create_content),
                state_event(ROOM, "$pl", ALICE, "m.room.power_levels", pl),
            ])
            .await;
        h
    }

    /// The creator cannot be demoted by a power_levels event: the check runs
    /// before the event is read, so `users: {ALICE: 0}` has no effect. (Before
    /// E-2 the PL entry was read first, which is G-32.)
    #[tokio::test]
    async fn v12_creator_cannot_be_demoted_by_power_levels() {
        let mut pl = default_power_levels();
        pl["users"] = serde_json::json!({ALICE: 0, BOB: 50});
        let h = v12_harness(serde_json::json!({"creator": ALICE, "room_version": "12"}), pl).await;

        assert_eq!(
            h.service.get_user_power_level(ROOM, ALICE).await.unwrap(),
            CREATOR_POWER_LEVEL,
            "a v12 creator keeps unlimited power even when power_levels names them"
        );
        // A non-creator is unaffected.
        assert_eq!(h.service.get_user_power_level(ROOM, BOB).await.unwrap(), 50);
    }

    /// MSC4289: every `additional_creators` entry is a creator too.
    #[tokio::test]
    async fn v12_additional_creators_are_unlimited_too() {
        let h = v12_harness(
            serde_json::json!({"creator": ALICE, "room_version": "12", "additional_creators": [CAROL]}),
            default_power_levels(),
        )
        .await;

        assert_eq!(h.service.get_user_power_level(ROOM, CAROL).await.unwrap(), CREATOR_POWER_LEVEL);
        assert_eq!(h.service.get_user_power_level(ROOM, ALICE).await.unwrap(), CREATOR_POWER_LEVEL);
    }

    /// Below v12 there is no unlimited-creator concept: the power_levels event
    /// wins (the long-standing behaviour must not change).
    #[tokio::test]
    async fn pre_v12_creator_is_still_demotable_by_power_levels() {
        let mut pl = default_power_levels();
        pl["users"] = serde_json::json!({ALICE: 0, BOB: 50});
        let h = v12_harness(serde_json::json!({"creator": ALICE, "room_version": "11"}), pl).await;

        assert_eq!(
            h.service.get_user_power_level(ROOM, ALICE).await.unwrap(),
            0,
            "v11 keeps the power_levels entry: no unlimited creator below v12"
        );
    }

    /// An unknown room version must not be assumed to be v12+ — granting
    /// unlimited power there would be a fail-open privilege escalation.
    #[tokio::test]
    async fn unknown_room_version_does_not_grant_unlimited_power() {
        let mut pl = default_power_levels();
        pl["users"] = serde_json::json!({ALICE: 0, BOB: 50});
        // No `room_version` key on the create event.
        let h = v12_harness(serde_json::json!({"creator": ALICE}), pl).await;

        assert_eq!(h.service.get_user_power_level(ROOM, ALICE).await.unwrap(), 0);
    }

    /// The kick/ban creator protection covers the whole creator set, not just
    /// the single `content.creator`.
    #[tokio::test]
    async fn v12_additional_creator_cannot_be_kicked_or_banned() {
        let h = v12_harness(
            serde_json::json!({"creator": ALICE, "room_version": "12", "additional_creators": [CAROL]}),
            default_power_levels(),
        )
        .await;

        assert!(
            h.service.can_kick_user(ROOM, ALICE, CAROL).await.is_err(),
            "an additional creator must not be kickable"
        );
        assert!(
            h.service.can_ban_user(ROOM, ALICE, CAROL).await.is_err(),
            "an additional creator must not be bannable"
        );
        // Positive control: BOB is an ordinary member and can be kicked.
        assert!(h.service.can_kick_user(ROOM, ALICE, BOB).await.is_ok());
    }

    /// The pure helper exposes the same set the authorisation path uses.
    #[tokio::test]
    async fn resolve_room_creators_is_the_sender_union_additional_creators() {
        let h = v12_harness(
            serde_json::json!({"creator": ALICE, "room_version": "12", "additional_creators": [CAROL, BOB]}),
            default_power_levels(),
        )
        .await;

        let creators = h.service.resolve_room_creators(ROOM).await.unwrap();
        assert!(creators.contains(ALICE) && creators.contains(CAROL) && creators.contains(BOB));
    }

    // ── E-3 / MSC4289 rule 10.4: power_levels.users must not name a creator ──

    /// v12+: naming the creator in `users` is rejected. A creator's power is
    /// unlimited, so the entry could never be enforced.
    #[tokio::test]
    async fn v12_power_levels_naming_the_creator_is_rejected() {
        let h = v12_harness(serde_json::json!({"creator": ALICE, "room_version": "12"}), default_power_levels()).await;
        let new = serde_json::json!({"users": {ALICE: 100, BOB: 50}});
        let err = h.service.verify_power_levels_change(ROOM, ALICE, &new).await.unwrap_err();
        assert!(err.message().contains("rule 10.4"), "unexpected error: {err:?}");
    }

    /// v12+: an `additional_creators` entry is a creator too.
    #[tokio::test]
    async fn v12_power_levels_naming_an_additional_creator_is_rejected() {
        let h = v12_harness(
            serde_json::json!({"creator": ALICE, "room_version": "12", "additional_creators": [CAROL]}),
            default_power_levels(),
        )
        .await;
        let new = serde_json::json!({"users": {CAROL: 100}});
        assert!(h.service.verify_power_levels_change(ROOM, ALICE, &new).await.is_err());
    }

    /// Positive control: a non-creator in `users` is still fine.
    #[tokio::test]
    async fn v12_power_levels_naming_a_non_creator_is_allowed() {
        let h = v12_harness(serde_json::json!({"creator": ALICE, "room_version": "12"}), default_power_levels()).await;
        let new = serde_json::json!({"users": {BOB: 75}});
        assert!(h.service.verify_power_levels_change(ROOM, ALICE, &new).await.is_ok());
    }

    /// Below v12 the creator *should* appear in `users` — that is how the
    /// creator's power was expressed before MSC4289, so the rule must not apply.
    #[tokio::test]
    async fn pre_v12_power_levels_may_name_the_creator() {
        let h = v12_harness(serde_json::json!({"creator": ALICE, "room_version": "11"}), default_power_levels()).await;
        let new = serde_json::json!({"users": {ALICE: 100, BOB: 50}});
        assert!(h.service.verify_power_levels_change(ROOM, ALICE, &new).await.is_ok());
    }

    /// v12 semantics (both the unlimited creator of E-2 and this restriction)
    /// apply only when the create event **states** v12+. An unknown version
    /// keeps the pre-v12 behaviour rather than guessing.
    #[tokio::test]
    async fn unknown_version_applies_no_v12_creator_rule() {
        let h = v12_harness(serde_json::json!({"creator": ALICE}), default_power_levels()).await;
        let new = serde_json::json!({"users": {ALICE: 100}});
        assert!(h.service.verify_power_levels_change(ROOM, ALICE, &new).await.is_ok());
    }
}
