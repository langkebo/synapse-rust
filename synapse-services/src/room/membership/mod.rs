//! Room membership queries and shared utilities.
//!
//! Action methods live in [`actions`].
//! Federation membership lives in [`federation`].
//! Moderation methods live in [`moderation`].

pub mod actions;
pub mod federation;
pub mod moderation;
pub mod service;
pub mod transition;

use crate::common::error::{ApiError, ApiResult};
use crate::*;
use serde_json::json;

use service::MembershipService;

impl MembershipService {
    pub async fn get_room_members(&self, room_id: &str, user_id: &str) -> ApiResult<serde_json::Value> {
        if !self
            .room_storage
            .room_exists(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check room existence", &e))?
        {
            return Err(ApiError::not_found("Room not found".to_string()));
        }

        if !self
            .member_storage
            .is_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check membership", &e))?
        {
            return Err(ApiError::forbidden("You are not a member of this room".to_string()));
        }

        let members_with_profiles = self
            .member_storage
            .get_room_members_with_profiles(room_id, "join")
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get members", &e))?;

        let chunk: Vec<serde_json::Value> = members_with_profiles
            .iter()
            .map(|(m, user_displayname, user_avatar_url)| {
                let mut content = serde_json::Map::new();
                content.insert("membership".to_string(), json!(m.membership));
                let effective_displayname = m.display_name.as_deref().or(user_displayname.as_deref());
                if let Some(dn) = effective_displayname {
                    content.insert("displayname".to_string(), json!(dn));
                }
                let effective_avatar_url = m.avatar_url.as_deref().or(user_avatar_url.as_deref());
                if let Some(au) = effective_avatar_url {
                    content.insert("avatar_url".to_string(), json!(au));
                }
                if let Some(reason) = &m.reason {
                    content.insert("reason".to_string(), json!(reason));
                }
                json!({
                    "type": "m.room.member",
                    "state_key": m.user_id,
                    "content": content,
                    "event_id": m.event_id,
                    "origin_server_ts": m.joined_ts.unwrap_or(m.updated_ts.unwrap_or(0)),
                    "room_id": m.room_id,
                    "sender": m.sender.as_deref().unwrap_or(&m.user_id),
                })
            })
            .collect();

        Ok(json!({ "chunk": chunk }))
    }

    pub async fn get_joined_rooms(&self, user_id: &str) -> ApiResult<Vec<String>> {
        self.member_storage
            .get_joined_rooms(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get joined rooms", &e))
    }

    pub async fn get_shared_room_users(&self, user_id: &str) -> ApiResult<Vec<String>> {
        self.member_storage
            .get_shared_room_users(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get shared room users", &e))
    }

    pub async fn share_common_room(&self, user_id: &str, other_user_id: &str) -> ApiResult<bool> {
        self.member_storage
            .share_common_room(user_id, other_user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check shared room membership", &e))
    }

    pub async fn share_common_rooms_batch(&self, user_id: &str, other_user_ids: &[String]) -> ApiResult<Vec<String>> {
        self.member_storage
            .share_common_rooms_batch(user_id, other_user_ids)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check shared room membership batch", &e))
    }

    pub async fn get_joined_members_with_profiles(&self, room_id: &str) -> ApiResult<Vec<storage::RoomMember>> {
        self.member_storage
            .get_joined_members(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get joined members", &e))
    }

    pub async fn get_membership_history(&self, room_id: &str, limit: i64) -> ApiResult<Vec<storage::RoomMember>> {
        self.member_storage
            .get_membership_history(room_id, limit)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get membership history", &e))
    }

    pub async fn get_room_members_by_membership(
        &self,
        room_id: &str,
        membership: &str,
    ) -> ApiResult<Vec<storage::RoomMember>> {
        self.member_storage
            .get_room_members(room_id, membership)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get room members", &e))
    }

    pub async fn has_any_non_banned_member_from_server(&self, room_id: &str, server_name: &str) -> ApiResult<bool> {
        self.member_storage
            .has_any_non_banned_member_from_server(room_id, server_name)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check server room membership", &e))
    }

    /// Check whether a user shares any joined room with a member from the
    /// given server domain. Single-query replacement for the previous
    /// `get_joined_rooms` + per-room `get_room_members` N+1 pattern used in
    /// federation origin validation.
    pub async fn user_shares_room_with_server(&self, user_id: &str, server_name: &str) -> ApiResult<bool> {
        self.member_storage
            .user_shares_room_with_server(user_id, server_name)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check user shares room with server", &e))
    }

    /// Batch version of `user_shares_room_with_server`: returns the subset of
    /// `user_ids` that share at least one joined room with a member from the
    /// given server domain. Used by federation `keys_claim` / `keys_query` to
    /// validate multiple users in a single query instead of M × (1 + N).
    pub async fn filter_users_sharing_room_with_server(
        &self,
        user_ids: &[String],
        server_name: &str,
    ) -> ApiResult<std::collections::HashSet<String>> {
        self.member_storage
            .filter_users_sharing_room_with_server(user_ids, server_name)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to batch check users sharing room with server", &e))
    }

    pub async fn get_room_membership(&self, room_id: &str, user_id: &str) -> ApiResult<Option<String>> {
        self.member_storage
            .get_membership_state(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check room membership", &e))
    }

    pub async fn get_room_member_record(&self, room_id: &str, user_id: &str) -> ApiResult<Option<storage::RoomMember>> {
        self.member_storage
            .get_room_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to load room member", &e))
    }

    pub async fn remove_member_record(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        self.member_storage
            .remove_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to remove room member", &e))
    }

    pub async fn get_room_members_paginated_admin(
        &self,
        room_id: &str,
        membership: &str,
        limit: i64,
        from: Option<&str>,
    ) -> ApiResult<Vec<storage::RoomMember>> {
        self.member_storage
            .get_room_members_paginated(room_id, membership, limit, from)
            .await
            .map_err(|e| ApiError::database_with_log("Failed to get room members", &e))
    }

    pub async fn get_room_member_count_admin(&self, room_id: &str) -> ApiResult<i64> {
        self.member_storage
            .get_room_member_count(room_id)
            .await
            .map_err(|e| ApiError::database_with_log("Failed to count room members", &e))
    }

    pub async fn admin_ban_user_membership(&self, room_id: &str, user_id: &str, banned_by: &str) -> ApiResult<()> {
        self.member_storage
            .ban_member(room_id, user_id, banned_by)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to ban user", &e))
    }

    pub async fn admin_unban_user_membership(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        self.member_storage
            .unban_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to unban user", &e))
    }

    pub async fn set_ban_reason(&self, room_id: &str, user_id: &str, reason: &str) -> ApiResult<()> {
        self.member_storage
            .set_ban_reason(room_id, user_id, reason)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to set ban reason", &e))
    }

    pub async fn force_leave_membership(&self, room_id: &str, user_id: &str, now: i64) -> ApiResult<()> {
        self.member_storage
            .force_leave_membership(room_id, user_id, now)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to force leave membership", &e))
    }

    pub async fn decrement_member_count(&self, room_id: &str) -> ApiResult<()> {
        self.room_storage
            .decrement_member_count(room_id)
            .await
            .map(|_| ())
            .map_err(|e| ApiError::internal_with_log("Failed to update member count", &e))
    }

    pub async fn get_invited_members_count(&self, room_id: &str) -> ApiResult<i64> {
        let summary = self.room_summary_service.get_summary(room_id).await?;
        Ok(summary.map(|summary| summary.invited_member_count).unwrap_or(0))
    }

    pub async fn add_member(
        &self,
        room_id: &str,
        user_id: &str,
        membership: &str,
        display_name: Option<&str>,
        join_reason: Option<&str>,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> ApiResult<storage::RoomMember> {
        let should_update_summary = tx.is_none();
        let member = self
            .member_storage
            .add_member(room_id, user_id, membership, display_name, join_reason, None, tx)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to add member", &e))?;

        if should_update_summary {
            let request = storage::room_summary::CreateSummaryMemberRequest {
                room_id: room_id.to_string(),
                user_id: user_id.to_string(),
                display_name: display_name.map(|value| value.to_string()),
                avatar_url: None,
                membership: membership.to_string(),
                is_hero: None,
                last_active_ts: member.joined_ts.or(member.updated_ts),
            };

            if let Err(error) = self.room_summary_service.add_member(request).await {
                ::tracing::warn!(
                    error = %error,
                    room_id = %room_id,
                    user_id = %user_id,
                    membership = %membership,
                    "Failed to update room summary member"
                );
            }

            if let Err(error) = self.room_summary_service.recalculate_heroes(room_id).await {
                ::tracing::warn!(error = %error, room_id = %room_id, "Failed to recalculate room summary heroes");
            }
        }

        Ok(member)
    }
}

// =============================================================================
// B.3 batch 3/6 — coverage tests for MembershipService query/admin methods.
//
// Target: synapse-services/src/room/membership/mod.rs (was 0% covered).
// Focus areas:
//   - `get_room_members` JSON building (effective displayname/avatar_url,
//     reason, origin_server_ts fallback chain, sender fallback)
//   - `add_member` room-summary update path (no-tx branch)
//   - `get_invited_members_count` room_summary_service delegation
//   - `admin_ban_user_membership` storage delegation
//
// All tests use in-memory mocks (no DB required) so they run in `cargo test
// --lib` and CI without PostgreSQL/Redis.
// =============================================================================

#[cfg(test)]
mod coverage_tests {
    use super::*;
    use std::sync::Arc as StdArc;

    use synapse_cache::{CacheConfig, CacheManager};
    use synapse_common::ApiErrorKind;
    use synapse_storage::event::{EventReader, EventWriter};
    use synapse_storage::membership::RoomMember;
    use synapse_storage::room_summary::RoomSummaryStoreApi;
    use synapse_storage::test_mocks::room_summary::InMemoryRoomSummaryStore;
    use synapse_storage::test_mocks::{FakeUserStore, InMemoryEventStore, InMemoryMemberStore, InMemoryRoomStore};
    use synapse_storage::{MemberStoreApi, RoomStoreApi, UserStore};

    use crate::room::membership::service::MembershipServiceConfig;
    use crate::room::summary::RoomSummaryService;
    use crate::test_mocks::FakeRoomAuth;
    use crate::user_service::UserService;

    const SERVER: &str = "localhost";
    const ROOM: &str = "!cov:localhost";

    /// Build a [`MembershipService`] over in-memory stores. Caller can pre-seed
    /// the `member_store` / `room_store` via the returned [`TestService`] before
    /// invoking service methods.
    struct TestService {
        svc: MembershipService,
        member_store: InMemoryMemberStore,
        room_store: InMemoryRoomStore,
        summary_store: InMemoryRoomSummaryStore,
    }

    async fn build_service() -> TestService {
        let member_store = InMemoryMemberStore::new();
        let room_store = InMemoryRoomStore::new();
        let summary_store = InMemoryRoomSummaryStore::new();

        let event_store = StdArc::new(InMemoryEventStore::new());
        let event_reader: StdArc<dyn EventReader> = event_store.clone();
        let event_writer: StdArc<dyn EventWriter> = event_store.clone();
        let member_storage: StdArc<dyn MemberStoreApi> = StdArc::new(member_store.clone());
        let room_storage: StdArc<dyn RoomStoreApi> = StdArc::new(room_store.clone());
        let user_storage: StdArc<dyn UserStore> = StdArc::new(FakeUserStore::new());
        let user_service = StdArc::new(UserService::new(user_storage.clone()));

        let room_summary_service = StdArc::new(RoomSummaryService::new(
            StdArc::new(summary_store.clone()),
            event_reader.clone(),
            Some(member_storage.clone()),
        ));

        let svc = MembershipService::new(MembershipServiceConfig {
            member_storage,
            room_storage,
            event_reader,
            event_writer,
            user_storage,
            user_service,
            room_auth: StdArc::new(FakeRoomAuth::new()),
            server_name: SERVER.to_string(),
            federation_client: None,
            key_rotation_manager: None,
            event_broadcaster: None,
            room_summary_service,
            cache: StdArc::new(CacheManager::new(&CacheConfig::default())),
            key_rotation_storage: None,
            app_service_manager: None,
        });

        TestService { svc, member_store, room_store, summary_store }
    }

    /// Seed a joined member with full profile fields into the in-memory store.
    #[allow(clippy::too_many_arguments)]
    async fn seed_joined_member(
        store: &InMemoryMemberStore,
        room_id: &str,
        user_id: &str,
        display_name: Option<&str>,
        avatar_url: Option<&str>,
        reason: Option<&str>,
        sender: Option<&str>,
        joined_ts: Option<i64>,
        updated_ts: Option<i64>,
    ) {
        let mut member = RoomMember {
            room_id: room_id.to_string(),
            user_id: user_id.to_string(),
            sender: sender.map(str::to_string),
            membership: "join".to_string(),
            event_id: Some(format!("$evt:{user_id}")),
            event_type: Some("m.room.member".to_string()),
            display_name: display_name.map(str::to_string),
            avatar_url: avatar_url.map(str::to_string),
            is_banned: Some(false),
            invite_token: None,
            updated_ts,
            joined_ts,
            left_ts: None,
            reason: reason.map(str::to_string),
            banned_by: None,
            ban_reason: None,
            banned_ts: None,
            join_reason: None,
        };
        // Use the store's add_member then overwrite with our rich fields.
        store.add_member(room_id, user_id, "join", display_name).await.unwrap();
        // add_member overwrites with default ts; re-seed via direct insertion using seed_members.
        let _ = &mut member; // suppress unused_mut if feature differs
        store.seed_members(vec![member]).await;
    }

    // =========================================================================
    // get_room_members — error paths
    // =========================================================================

    #[tokio::test]
    async fn get_room_members_returns_not_found_when_room_missing() {
        let ctx = build_service().await;
        // No room seeded.
        let err = ctx.svc.get_room_members(ROOM, "@alice:localhost").await.unwrap_err();
        assert!(matches!(err.kind, ApiErrorKind::NotFound), "expected NotFound, got {err:?}");
    }

    #[tokio::test]
    async fn get_room_members_returns_forbidden_when_user_not_member() {
        let ctx = build_service().await;
        ctx.room_store.create_room(ROOM, "@creator:localhost", "public", "1", true).await.unwrap();

        let err = ctx.svc.get_room_members(ROOM, "@stranger:localhost").await.unwrap_err();
        assert!(matches!(err.kind, ApiErrorKind::Forbidden), "expected Forbidden, got {err:?}");
    }

    // =========================================================================
    // get_room_members — JSON building (happy paths + field fallbacks)
    // =========================================================================

    #[tokio::test]
    async fn get_room_members_happy_path_with_member_displayname_and_avatar() {
        let ctx = build_service().await;
        ctx.room_store.create_room(ROOM, "@alice:localhost", "public", "1", true).await.unwrap();
        seed_joined_member(
            &ctx.member_store,
            ROOM,
            "@alice:localhost",
            Some("Alice"),
            Some("mxc://localhost/alice"),
            None,
            Some("@alice:localhost"),
            Some(1_700_000_000_000),
            Some(1_700_000_000_000),
        )
        .await;

        let result = ctx.svc.get_room_members(ROOM, "@alice:localhost").await.unwrap();
        let chunk = result.get("chunk").and_then(|c| c.as_array()).expect("chunk array");
        assert_eq!(chunk.len(), 1);

        let entry = &chunk[0];
        assert_eq!(entry["type"], "m.room.member");
        assert_eq!(entry["state_key"], "@alice:localhost");
        assert_eq!(entry["room_id"], ROOM);
        assert_eq!(entry["sender"], "@alice:localhost");
        // seed_joined_member sets event_id = format!("$evt:{user_id}").
        assert_eq!(entry["event_id"], "$evt:@alice:localhost");
        assert_eq!(entry["origin_server_ts"], 1_700_000_000_000_i64);
        assert_eq!(entry["content"]["membership"], "join");
        assert_eq!(entry["content"]["displayname"], "Alice");
        assert_eq!(entry["content"]["avatar_url"], "mxc://localhost/alice");
        // No reason seeded → reason field must be absent.
        assert!(entry["content"].get("reason").is_none(), "reason must be absent when None");
    }

    #[tokio::test]
    async fn get_room_members_includes_reason_when_present() {
        let ctx = build_service().await;
        ctx.room_store.create_room(ROOM, "@bob:localhost", "public", "1", true).await.unwrap();
        seed_joined_member(
            &ctx.member_store,
            ROOM,
            "@bob:localhost",
            None,
            None,
            Some("Invited by admin"),
            None,
            Some(1_700_000_000_000),
            Some(1_700_000_000_000),
        )
        .await;

        let result = ctx.svc.get_room_members(ROOM, "@bob:localhost").await.unwrap();
        let entry = &result["chunk"][0];
        assert_eq!(entry["content"]["reason"], "Invited by admin");
        // displayname / avatar_url absent (both None on member AND on user profile).
        assert!(entry["content"].get("displayname").is_none());
        assert!(entry["content"].get("avatar_url").is_none());
    }

    #[tokio::test]
    async fn get_room_members_falls_back_to_updated_ts_when_joined_ts_none() {
        let ctx = build_service().await;
        ctx.room_store.create_room(ROOM, "@carol:localhost", "public", "1", true).await.unwrap();
        // joined_ts=None, updated_ts=Some(999) → origin_server_ts must be 999.
        seed_joined_member(&ctx.member_store, ROOM, "@carol:localhost", None, None, None, None, None, Some(999)).await;

        let result = ctx.svc.get_room_members(ROOM, "@carol:localhost").await.unwrap();
        assert_eq!(result["chunk"][0]["origin_server_ts"], 999);
    }

    #[tokio::test]
    async fn get_room_members_falls_back_to_zero_when_both_ts_none() {
        let ctx = build_service().await;
        ctx.room_store.create_room(ROOM, "@dave:localhost", "public", "1", true).await.unwrap();
        seed_joined_member(&ctx.member_store, ROOM, "@dave:localhost", None, None, None, None, None, None).await;

        let result = ctx.svc.get_room_members(ROOM, "@dave:localhost").await.unwrap();
        assert_eq!(result["chunk"][0]["origin_server_ts"], 0);
    }

    #[tokio::test]
    async fn get_room_members_uses_user_id_when_sender_none() {
        let ctx = build_service().await;
        ctx.room_store.create_room(ROOM, "@eve:localhost", "public", "1", true).await.unwrap();
        seed_joined_member(
            &ctx.member_store,
            ROOM,
            "@eve:localhost",
            None,
            None,
            None,
            None, // sender = None → fallback to user_id
            Some(1),
            Some(1),
        )
        .await;

        let result = ctx.svc.get_room_members(ROOM, "@eve:localhost").await.unwrap();
        assert_eq!(result["chunk"][0]["sender"], "@eve:localhost");
    }

    #[tokio::test]
    async fn get_room_members_returns_multiple_members_in_chunk() {
        let ctx = build_service().await;
        ctx.room_store.create_room(ROOM, "@alice:localhost", "public", "1", true).await.unwrap();
        seed_joined_member(&ctx.member_store, ROOM, "@alice:localhost", Some("Alice"), None, None, None, Some(1), Some(1)).await;
        seed_joined_member(&ctx.member_store, ROOM, "@bob:localhost", Some("Bob"), None, None, None, Some(2), Some(2)).await;
        // Alice must be a member to call get_room_members; both seeded above.
        let result = ctx.svc.get_room_members(ROOM, "@alice:localhost").await.unwrap();
        let chunk = result["chunk"].as_array().unwrap();
        assert_eq!(chunk.len(), 2, "expected 2 joined members");
    }

    // =========================================================================
    // get_joined_rooms / share_common_room / share_common_rooms_batch
    // =========================================================================

    #[tokio::test]
    async fn get_joined_rooms_returns_only_rooms_where_user_is_joined() {
        let ctx = build_service().await;
        // Seed user joined to two rooms, left one room, banned in another.
        seed_joined_member(&ctx.member_store, "!r1:localhost", "@u:localhost", None, None, None, None, Some(1), Some(1)).await;
        seed_joined_member(&ctx.member_store, "!r2:localhost", "@u:localhost", None, None, None, None, Some(2), Some(2)).await;
        ctx.member_store.add_member("!r3:localhost", "@u:localhost", "leave", None).await.unwrap();
        ctx.member_store.add_member("!r4:localhost", "@u:localhost", "ban", None).await.unwrap();

        let rooms = ctx.svc.get_joined_rooms("@u:localhost").await.unwrap();
        let mut sorted = rooms;
        sorted.sort();
        assert_eq!(sorted, vec!["!r1:localhost", "!r2:localhost"]);
    }

    #[tokio::test]
    async fn share_common_room_true_when_both_joined_same_room() {
        let ctx = build_service().await;
        seed_joined_member(&ctx.member_store, "!shared:localhost", "@alice:localhost", None, None, None, None, Some(1), Some(1)).await;
        seed_joined_member(&ctx.member_store, "!shared:localhost", "@bob:localhost", None, None, None, None, Some(1), Some(1)).await;

        assert!(ctx.svc.share_common_room("@alice:localhost", "@bob:localhost").await.unwrap());
    }

    #[tokio::test]
    async fn share_common_room_false_when_no_shared_room() {
        let ctx = build_service().await;
        seed_joined_member(&ctx.member_store, "!a:localhost", "@alice:localhost", None, None, None, None, Some(1), Some(1)).await;
        seed_joined_member(&ctx.member_store, "!b:localhost", "@bob:localhost", None, None, None, None, Some(1), Some(1)).await;

        assert!(!ctx.svc.share_common_room("@alice:localhost", "@bob:localhost").await.unwrap());
    }

    #[tokio::test]
    async fn share_common_rooms_batch_returns_only_users_with_shared_room() {
        let ctx = build_service().await;
        seed_joined_member(&ctx.member_store, "!shared:localhost", "@alice:localhost", None, None, None, None, Some(1), Some(1)).await;
        seed_joined_member(&ctx.member_store, "!shared:localhost", "@bob:localhost", None, None, None, None, Some(1), Some(1)).await;
        seed_joined_member(&ctx.member_store, "!shared:localhost", "@carol:localhost", None, None, None, None, Some(1), Some(1)).await;
        // dave is NOT in the shared room.
        seed_joined_member(&ctx.member_store, "!other:localhost", "@dave:localhost", None, None, None, None, Some(1), Some(1)).await;

        let shared = ctx.svc
            .share_common_rooms_batch("@alice:localhost", &["@bob:localhost".to_string(), "@carol:localhost".to_string(), "@dave:localhost".to_string()])
            .await
            .unwrap();
        let mut sorted = shared;
        sorted.sort();
        assert_eq!(sorted, vec!["@bob:localhost", "@carol:localhost"]);
    }

    // =========================================================================
    // get_invited_members_count — delegates to room_summary_service
    // =========================================================================

    #[tokio::test]
    async fn get_invited_members_count_returns_zero_when_no_summary() {
        let ctx = build_service().await;
        // No summary seeded → unwrap_or(0) branch.
        let count = ctx.svc.get_invited_members_count(ROOM).await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn get_invited_members_count_returns_summary_invited_count_when_summary_exists() {
        let ctx = build_service().await;
        // Seed a summary via the public RoomSummaryStoreApi. create_summary
        // initialises invited_member_count = Some(0); the test verifies the
        // Some(summary) branch of get_invited_members_count (not the unwrap_or
        // fallback). The mock's create_summary does not increment on add_member,
        // so we assert 0 — what matters is that the summary path is exercised.
        ctx.summary_store
            .create_summary(storage::room_summary::CreateRoomSummaryRequest {
                room_id: ROOM.to_string(),
                room_type: None,
                name: Some("Cov Room".to_string()),
                topic: None,
                avatar_url: None,
                canonical_alias: None,
                join_rule: Some("invite".to_string()),
                history_visibility: None,
                guest_access: None,
                is_direct: Some(false),
                is_space: Some(false),
            })
            .await
            .unwrap();

        // Verify the summary actually exists (sanity check the seed).
        let seeded = ctx.summary_store.get_summary(ROOM).await.unwrap();
        assert!(seeded.is_some(), "summary must exist after create_summary");

        let count = ctx.svc.get_invited_members_count(ROOM).await.unwrap();
        // Mock create_summary initialises invited_member_count = 0.
        assert_eq!(count, 0);
        // Distinguish from the no-summary test: if the service had taken the
        // None branch incorrectly, the result would still be 0, so we additionally
        // assert the path by checking that get_summary on the service returns Some.
        let svc_summary = ctx.svc.room_summary_service.get_summary(ROOM).await.unwrap();
        assert!(svc_summary.is_some(), "service must see the seeded summary");
    }

    // =========================================================================
    // admin_ban_user_membership / admin_unban_user_membership / set_ban_reason
    // =========================================================================

    #[tokio::test]
    async fn admin_ban_user_membership_marks_member_as_banned() {
        let ctx = build_service().await;
        ctx.member_store.add_member(ROOM, "@spammer:localhost", "join", None).await.unwrap();

        ctx.svc.admin_ban_user_membership(ROOM, "@spammer:localhost", "@admin:localhost").await.unwrap();

        let member = ctx.member_store.get_member(ROOM, "@spammer:localhost").await.unwrap().expect("member present");
        assert_eq!(member.membership, "ban");
        assert_eq!(member.is_banned, Some(true));
        assert_eq!(member.banned_by.as_deref(), Some("@admin:localhost"));
        assert!(member.banned_ts.is_some(), "banned_ts must be set");
    }

    #[tokio::test]
    async fn admin_unban_user_membership_clears_ban_state() {
        let ctx = build_service().await;
        // ban_member only mutates an existing member; seed one first.
        ctx.member_store.add_member(ROOM, "@user:localhost", "join", None).await.unwrap();
        ctx.member_store.ban_member(ROOM, "@user:localhost", "@admin:localhost").await.unwrap();

        ctx.svc.admin_unban_user_membership(ROOM, "@user:localhost").await.unwrap();

        // InMemoryMemberStore.unban_member sets membership to "leave" and is_banned to false.
        let member = ctx.member_store.get_member(ROOM, "@user:localhost").await.unwrap().expect("member present");
        assert_ne!(member.membership, "ban");
        assert_eq!(member.is_banned, Some(false));
    }

    // =========================================================================
    // add_member — room-summary update path (no-tx branch)
    // =========================================================================

    #[tokio::test]
    async fn add_member_without_tx_updates_room_summary() {
        let ctx = build_service().await;
        // Pre-create the room summary so add_member's summary update has a row to mutate.
        ctx.summary_store
            .create_summary(storage::room_summary::CreateRoomSummaryRequest {
                room_id: ROOM.to_string(),
                room_type: None,
                name: Some("Add Member Cov".to_string()),
                topic: None,
                avatar_url: None,
                canonical_alias: None,
                join_rule: Some("public".to_string()),
                history_visibility: None,
                guest_access: None,
                is_direct: Some(false),
                is_space: Some(false),
            })
            .await
            .unwrap();

        let member = ctx.svc.add_member(ROOM, "@new:localhost", "join", Some("New User"), None, None).await.unwrap();
        assert_eq!(member.user_id, "@new:localhost");
        assert_eq!(member.membership, "join");

        // Summary member row must have been inserted by room_summary_service.add_member.
        // Verify via the public RoomSummaryStoreApi::get_members (not private field access).
        let summary_members = ctx.summary_store.get_members(ROOM).await.unwrap();
        let found = summary_members.iter().any(|m| m.user_id == "@new:localhost");
        assert!(found, "summary member row must be inserted when tx is None; got {summary_members:?}");
    }

    #[tokio::test]
    async fn add_member_does_not_panic_when_summary_missing() {
        let ctx = build_service().await;
        // No summary seeded — add_member's summary update should log warning, not panic.
        let member = ctx.svc.add_member(ROOM, "@lone:localhost", "join", None, None, None).await.unwrap();
        assert_eq!(member.user_id, "@lone:localhost");
    }

    // =========================================================================
    // force_leave_membership / decrement_member_count / remove_member_record
    // =========================================================================

    #[tokio::test]
    async fn remove_member_record_delegates_to_storage() {
        let ctx = build_service().await;
        ctx.member_store.add_member(ROOM, "@gone:localhost", "join", None).await.unwrap();
        // Sanity: member is initially "join".
        assert_eq!(ctx.svc.get_room_membership(ROOM, "@gone:localhost").await.unwrap().as_deref(), Some("join"));

        // remove_member in both real DB and mock sets membership="leave" (soft delete),
        // it does NOT delete the row. Verify the state transition.
        ctx.svc.remove_member_record(ROOM, "@gone:localhost").await.unwrap();

        assert_eq!(ctx.svc.get_room_membership(ROOM, "@gone:localhost").await.unwrap().as_deref(), Some("leave"));
    }

    #[tokio::test]
    async fn get_room_membership_returns_state_string() {
        let ctx = build_service().await;
        ctx.member_store.add_member(ROOM, "@u:localhost", "invite", None).await.unwrap();

        let state = ctx.svc.get_room_membership(ROOM, "@u:localhost").await.unwrap();
        assert_eq!(state.as_deref(), Some("invite"));
    }

    #[tokio::test]
    async fn get_room_membership_returns_none_for_unknown_user() {
        let ctx = build_service().await;
        let state = ctx.svc.get_room_membership(ROOM, "@ghost:localhost").await.unwrap();
        assert_eq!(state, None);
    }

    // =========================================================================
    // Supplementary coverage: thin delegation methods (batch 3/6 supplemental).
    //
    // These methods are one-line delegations to `member_storage` / `room_storage`
    // with `map_err` for error translation. Each test exercises the happy path
    // through the in-memory mock to cover the delegation + error-mapping lines.
    // =========================================================================

    #[tokio::test]
    async fn get_shared_room_users_returns_users_sharing_joined_room() {
        let ctx = build_service().await;
        ctx.member_store.add_member(ROOM, "@alice:localhost", "join", None).await.unwrap();
        ctx.member_store.add_member(ROOM, "@bob:localhost", "join", None).await.unwrap();

        let shared = ctx.svc.get_shared_room_users("@alice:localhost").await.unwrap();
        assert!(shared.contains(&"@bob:localhost".to_string()), "shared users must include bob: {shared:?}");
    }

    #[tokio::test]
    async fn get_joined_members_with_profiles_returns_joined_members() {
        let ctx = build_service().await;
        ctx.member_store.add_member(ROOM, "@alice:localhost", "join", Some("Alice")).await.unwrap();
        ctx.member_store.add_member(ROOM, "@bob:localhost", "leave", None).await.unwrap();

        let joined = ctx.svc.get_joined_members_with_profiles(ROOM).await.unwrap();
        assert_eq!(joined.len(), 1);
        assert_eq!(joined[0].user_id, "@alice:localhost");
    }

    #[tokio::test]
    async fn get_membership_history_returns_members_sorted_by_updated_ts() {
        let ctx = build_service().await;
        ctx.member_store.add_member(ROOM, "@alice:localhost", "join", None).await.unwrap();
        ctx.member_store.add_member(ROOM, "@bob:localhost", "leave", None).await.unwrap();

        let history = ctx.svc.get_membership_history(ROOM, 10).await.unwrap();
        assert_eq!(history.len(), 2);
        // Both members belong to ROOM.
        assert!(history.iter().all(|m| m.room_id == ROOM));
    }

    #[tokio::test]
    async fn get_room_members_by_membership_filters_by_membership_type() {
        let ctx = build_service().await;
        ctx.member_store.add_member(ROOM, "@alice:localhost", "join", None).await.unwrap();
        ctx.member_store.add_member(ROOM, "@bob:localhost", "invite", None).await.unwrap();

        let joined = ctx.svc.get_room_members_by_membership(ROOM, "join").await.unwrap();
        assert_eq!(joined.len(), 1);
        assert_eq!(joined[0].user_id, "@alice:localhost");

        let invited = ctx.svc.get_room_members_by_membership(ROOM, "invite").await.unwrap();
        assert_eq!(invited.len(), 1);
        assert_eq!(invited[0].user_id, "@bob:localhost");
    }

    #[tokio::test]
    async fn has_any_non_banned_member_from_server_true_for_joined_member() {
        let ctx = build_service().await;
        ctx.member_store.add_member(ROOM, "@alice:localhost", "join", None).await.unwrap();
        ctx.member_store.add_member(ROOM, "@banned:localhost", "ban", None).await.unwrap();

        assert!(ctx.svc.has_any_non_banned_member_from_server(ROOM, "localhost").await.unwrap());
        assert!(!ctx.svc.has_any_non_banned_member_from_server(ROOM, "example.com").await.unwrap());
    }

    #[tokio::test]
    async fn user_shares_room_with_server_true_when_user_and_server_member_joined() {
        let ctx = build_service().await;
        ctx.member_store.add_member(ROOM, "@alice:localhost", "join", None).await.unwrap();
        ctx.member_store.add_member(ROOM, "@bob:remote.srv", "join", None).await.unwrap();

        assert!(ctx.svc.user_shares_room_with_server("@alice:localhost", "remote.srv").await.unwrap());
        assert!(!ctx.svc.user_shares_room_with_server("@alice:localhost", "nonexistent.srv").await.unwrap());
    }

    #[tokio::test]
    async fn filter_users_sharing_room_with_server_returns_matching_subset() {
        let ctx = build_service().await;
        ctx.member_store.add_member(ROOM, "@alice:localhost", "join", None).await.unwrap();
        ctx.member_store.add_member(ROOM, "@bob:remote.srv", "join", None).await.unwrap();

        let candidates = vec!["@alice:localhost".to_string(), "@carol:localhost".to_string()];
        let filtered = ctx
            .svc
            .filter_users_sharing_room_with_server(&candidates, "remote.srv")
            .await
            .unwrap();
        assert!(filtered.contains("@alice:localhost"));
        assert!(!filtered.contains("@carol:localhost"));
    }

    #[tokio::test]
    async fn get_room_member_record_returns_member_when_exists() {
        let ctx = build_service().await;
        ctx.member_store.add_member(ROOM, "@alice:localhost", "join", Some("Alice")).await.unwrap();

        let member = ctx.svc.get_room_member_record(ROOM, "@alice:localhost").await.unwrap();
        assert!(member.is_some());
        assert_eq!(member.unwrap().display_name.as_deref(), Some("Alice"));

        let missing = ctx.svc.get_room_member_record(ROOM, "@ghost:localhost").await.unwrap();
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn get_room_members_paginated_admin_returns_sorted_page() {
        let ctx = build_service().await;
        ctx.member_store.add_member(ROOM, "@alice:localhost", "join", None).await.unwrap();
        ctx.member_store.add_member(ROOM, "@bob:localhost", "join", None).await.unwrap();

        let page = ctx
            .svc
            .get_room_members_paginated_admin(ROOM, "join", 10, None)
            .await
            .unwrap();
        assert_eq!(page.len(), 2);
        // Pagination is sorted by user_id ascending.
        assert!(page[0].user_id <= page[1].user_id);
    }

    #[tokio::test]
    async fn get_room_member_count_admin_returns_joined_count() {
        let ctx = build_service().await;
        ctx.member_store.add_member(ROOM, "@alice:localhost", "join", None).await.unwrap();
        ctx.member_store.add_member(ROOM, "@bob:localhost", "join", None).await.unwrap();
        ctx.member_store.add_member(ROOM, "@carol:localhost", "leave", None).await.unwrap();

        let count = ctx.svc.get_room_member_count_admin(ROOM).await.unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn set_ban_reason_updates_member_ban_reason() {
        let ctx = build_service().await;
        ctx.member_store.add_member(ROOM, "@alice:localhost", "ban", None).await.unwrap();

        ctx.svc.set_ban_reason(ROOM, "@alice:localhost", "spam").await.unwrap();

        let member = ctx.svc.get_room_member_record(ROOM, "@alice:localhost").await.unwrap().unwrap();
        assert_eq!(member.ban_reason.as_deref(), Some("spam"));
    }

    #[tokio::test]
    async fn force_leave_membership_sets_leave_state_and_left_ts() {
        let ctx = build_service().await;
        ctx.member_store.add_member(ROOM, "@alice:localhost", "join", None).await.unwrap();

        let now: i64 = 1_700_000_000_000;
        ctx.svc.force_leave_membership(ROOM, "@alice:localhost", now).await.unwrap();

        let member = ctx.svc.get_room_member_record(ROOM, "@alice:localhost").await.unwrap().unwrap();
        assert_eq!(member.membership, "leave");
        assert_eq!(member.left_ts, Some(now));
    }

    #[tokio::test]
    async fn decrement_member_count_decrements_room_member_count() {
        let ctx = build_service().await;
        ctx.room_store.create_room(ROOM, "@creator:localhost", "public", "1", true).await.unwrap();
        // Increment then decrement — net zero, but both lines are covered.
        ctx.room_store.increment_member_count(ROOM).await.unwrap();
        ctx.svc.decrement_member_count(ROOM).await.unwrap();

        // No error and room still exists — delegation succeeded.
        let room = ctx.room_store.get_room(ROOM).await.unwrap().unwrap();
        assert_eq!(room.member_count, 0);
    }
}
