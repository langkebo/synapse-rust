//! `m.room.member` 的权限校验：目标维度 + 邀请阈值。
//!
//! `PUT /rooms/{id}/state/m.room.member/{userId}` 与 MSC4140 延迟写入面都能在
//! `state_key`（延迟面来自请求体）里点名任意用户。`AuthService::verify_state_event_write`
//! 只把**发送者**与 `state_default` 比，对**目标**一无所知；本模块把这类写入路由到既有的
//! 成员关系规则（invite/kick/ban 阈值、"严格高于目标"、创建者保护）。
//!
//! 从 `power_levels.rs` 拆出，仅为控制该文件的行数棘轮（≤ 1205），**无行为变化**。

use super::super::AuthService;
use synapse_common::{ApiError, ApiResult};

impl AuthService {
    /// See [`Self::verify_membership_state_write`].
    ///
    /// `PUT /rooms/{id}/state/m.room.member/{userId}` — and the MSC4140 delay
    /// surface, whose `state_key` comes from the request *body* — can name any
    /// user as the target. [`Self::verify_state_event_write`] only compares the
    /// sender against `state_default`; it says nothing about the target. On its
    /// own that let a moderator at the `state_default` level ban or kick a user
    /// above them, or the room creator, just by writing that user's membership
    /// state directly.
    ///
    /// Matrix authorizes `m.room.member` through the **membership** rules
    /// (`invite`/`kick`/`ban` thresholds, "strictly above the target's power",
    /// creator protection), not through `power_levels.events`. Those rules
    /// already exist as [`Self::can_invite_user`] / [`Self::can_kick_user`] /
    /// [`Self::can_ban_user`] / [`Self::can_unban_user`], so this only routes
    /// the requested transition to the right one — it adds no new thresholds.
    pub async fn verify_membership_state_write(
        &self,
        room_id: &str,
        actor_user_id: &str,
        target_user_id: &str,
        content: &serde_json::Value,
    ) -> ApiResult<()> {
        // `join`/`knock` are self-only, and a self-`leave` is always allowed:
        // none of them can touch another user, so no target check applies.
        let membership = content.get("membership").and_then(|v| v.as_str()).unwrap_or("");
        if actor_user_id == target_user_id && matches!(membership, "join" | "knock" | "leave") {
            return Ok(());
        }

        if target_user_id.is_empty() {
            return Err(ApiError::bad_request("m.room.member state_key must be a user ID".to_string()));
        }

        // The target's *current* membership decides whether a `leave` is a kick
        // or an unban.
        let current = self
            .member_storage
            .get_membership_state(room_id, target_user_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Database error", e))?;

        match membership {
            // A different actor can never produce these transitions.
            "join" | "knock" => Err(ApiError::forbidden(
                "Only the target user may set their own membership to join or knock".to_string(),
            )),
            "invite" => {
                if current.as_deref() == Some("ban") {
                    return Err(ApiError::forbidden("Cannot invite a banned user".to_string()));
                }
                self.can_invite_user(room_id, actor_user_id).await
            }
            // Reached only for a non-self `leave` (self-`leave` returned above).
            "leave" => {
                if current.as_deref() == Some("ban") {
                    self.can_unban_user(room_id, actor_user_id, target_user_id).await
                } else {
                    self.can_kick_user(room_id, actor_user_id, target_user_id).await
                }
            }
            // A self-`ban` reaches this arm too; `can_ban_user` rejects it on the
            // "strictly above the target" rule, so it needs no special case.
            "ban" => self.can_ban_user(room_id, actor_user_id, target_user_id).await,
            "" => Err(ApiError::bad_request("Missing membership".to_string())),
            other => Err(ApiError::bad_request(format!("Invalid membership: {}", other))),
        }
    }
}

#[cfg(test)]
mod tests {
    //! 目标维度鉴权的真实单测：走真实 `AuthService`（不是 `FakeRoomAuth`），
    //! 因此拒绝一定是鉴权真的生效，而不是 mock 恒 `Ok(())`。
    //!
    //! 夹具（`member`/`state_event`/`harness_with_room` …）仍留在父模块的
    //! `tests` 里，这里通过 `pub(super)` 复用，避免复制两份会漂移的脚手架。

    use super::super::super::test_harness::build_test_auth_service;
    use super::super::tests::{
        default_power_levels, harness_with_room, member, state_event, ALICE, BOB, CAROL, DAVE, ROOM,
    };
    use synapse_common::MatrixErrorCode;

    fn membership_content(membership: &str) -> serde_json::Value {
        serde_json::json!({ "membership": membership })
    }

    #[tokio::test]
    async fn invite_uses_default_zero_threshold() {
        let h = harness_with_room().await;
        assert!(h.service.can_invite_user(ROOM, CAROL).await.is_ok());
        assert!(h.service.can_invite_user(ROOM, DAVE).await.is_err(), "非成员 power -1 < invite 0");
    }

    #[tokio::test]
    async fn membership_self_join_knock_leave_always_allowed() {
        let h = harness_with_room().await;
        for m in ["join", "knock", "leave"] {
            assert!(
                h.service.verify_membership_state_write(ROOM, CAROL, CAROL, &membership_content(m)).await.is_ok(),
                "self {m} 不涉及他人目标，必须放行（CAROL 无任何阈值权力）"
            );
        }
        // 非成员自 join 同样不因目标检查被拒：能否入房由 join_rules 等另一层决定。
        assert!(h.service.verify_membership_state_write(ROOM, DAVE, DAVE, &membership_content("join")).await.is_ok());
    }

    #[tokio::test]
    async fn membership_ban_rejects_target_with_equal_or_higher_power() {
        let h = harness_with_room().await;
        // 头条缺陷：BOB(50) 满足 state_default(50) 后，可把 ALICE(100，创建者)
        // 直接写成 ban。成员关系规则要求"严格高于目标"，必须拒绝。
        let err = h.service.verify_membership_state_write(ROOM, BOB, ALICE, &membership_content("ban")).await;
        assert_eq!(err.unwrap_err().code(), &MatrixErrorCode::Forbidden);
        // 同等级（自我封禁）也不行。
        assert!(h.service.verify_membership_state_write(ROOM, BOB, BOB, &membership_content("ban")).await.is_err());
        // 但对自己的下级照常放行。
        assert!(h.service.verify_membership_state_write(ROOM, BOB, CAROL, &membership_content("ban")).await.is_ok());
    }

    #[tokio::test]
    async fn membership_kick_rejects_creator_even_when_actor_outpowers() {
        // 房间创建者 = BOB(50)，ALICE(100) 只是高权管理员：纯粹比 PL 会放行，
        // 必须由创建者保护拦下。
        let h = build_test_auth_service();
        let room = "!room6:test.server";
        h.member_store.seed_members(vec![member(room, ALICE, "join"), member(room, BOB, "join")]).await;
        h.event_store
            .seed_events(vec![
                state_event(
                    room,
                    "$create6",
                    BOB,
                    "m.room.create",
                    serde_json::json!({"creator": BOB, "room_version": "10"}),
                ),
                state_event(room, "$pl6", BOB, "m.room.power_levels", default_power_levels()),
            ])
            .await;

        let err = h.service.verify_membership_state_write(room, ALICE, BOB, &membership_content("leave")).await;
        assert_eq!(err.unwrap_err().code(), &MatrixErrorCode::Forbidden, "创建者不可被踢");
        let err = h.service.verify_membership_state_write(room, ALICE, BOB, &membership_content("ban")).await;
        assert_eq!(err.unwrap_err().code(), &MatrixErrorCode::Forbidden, "创建者不可被封禁");
    }

    #[tokio::test]
    async fn membership_leave_routes_to_kick_thresholds() {
        let h = harness_with_room().await;
        // 非自写 leave = 踢人。BOB(50) >= kick(50) 且 > CAROL(0)。
        assert!(h.service.verify_membership_state_write(ROOM, BOB, CAROL, &membership_content("leave")).await.is_ok());
        // CAROL(0) < kick(50)，不得踢 BOB。
        assert_eq!(
            h.service
                .verify_membership_state_write(ROOM, CAROL, BOB, &membership_content("leave"))
                .await
                .unwrap_err()
                .code(),
            &MatrixErrorCode::Forbidden
        );
    }

    #[tokio::test]
    async fn membership_foreign_join_and_knock_rejected() {
        let h = harness_with_room().await;
        for m in ["join", "knock"] {
            let err = h.service.verify_membership_state_write(ROOM, BOB, CAROL, &membership_content(m)).await;
            assert_eq!(err.unwrap_err().code(), &MatrixErrorCode::Forbidden, "他人不得代为 {m}");
        }
    }

    #[tokio::test]
    async fn membership_invite_checks_threshold_and_ban_state() {
        let h = harness_with_room().await;
        // CAROL(0) >= invite(0)，可邀请非成员 DAVE。
        assert!(h
            .service
            .verify_membership_state_write(ROOM, CAROL, DAVE, &membership_content("invite"))
            .await
            .is_ok());
        // 非成员（power -1）不能邀请任何人。
        assert!(h
            .service
            .verify_membership_state_write(ROOM, DAVE, CAROL, &membership_content("invite"))
            .await
            .is_err());

        // 已 ban 的目标不能靠 invite 直接"复活"，必须先走 unban。
        h.member_store.seed_members(vec![member(ROOM, DAVE, "ban")]).await;
        let err = h.service.verify_membership_state_write(ROOM, ALICE, DAVE, &membership_content("invite")).await;
        assert_eq!(err.unwrap_err().code(), &MatrixErrorCode::Forbidden);
    }

    #[tokio::test]
    async fn membership_leave_on_banned_target_uses_unban_rules() {
        let h = harness_with_room().await;
        h.member_store.seed_members(vec![member(ROOM, DAVE, "ban")]).await;
        // CAROL(0) < ban(50)：不能解封。
        assert_eq!(
            h.service
                .verify_membership_state_write(ROOM, CAROL, DAVE, &membership_content("leave"))
                .await
                .unwrap_err()
                .code(),
            &MatrixErrorCode::Forbidden
        );
        // BOB(50) >= ban(50) 且严格高于 DAVE(0)：解封（leave）放行。
        assert!(h.service.verify_membership_state_write(ROOM, BOB, DAVE, &membership_content("leave")).await.is_ok());
    }

    #[tokio::test]
    async fn membership_missing_invalid_value_or_empty_target_is_bad_request() {
        let h = harness_with_room().await;
        // 缺 membership。
        let err = h.service.verify_membership_state_write(ROOM, BOB, CAROL, &serde_json::json!({})).await;
        assert_eq!(err.unwrap_err().code(), &MatrixErrorCode::BadJson);
        // 非法 membership。
        let err = h.service.verify_membership_state_write(ROOM, BOB, CAROL, &membership_content("foobar")).await;
        assert_eq!(err.unwrap_err().code(), &MatrixErrorCode::BadJson);
        // 空 state_key：不能当成"无目标"静默放行。
        let err = h.service.verify_membership_state_write(ROOM, BOB, "", &membership_content("ban")).await;
        assert_eq!(err.unwrap_err().code(), &MatrixErrorCode::BadJson);
    }
}
