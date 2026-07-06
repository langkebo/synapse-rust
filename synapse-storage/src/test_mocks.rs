//! Pre-positioned Mock adapters for the storage layer.
//!
//! Aggregates in-memory test doubles for storage traits so service-layer
//! unit tests can run without a real PostgreSQL pool. Engineers extend
//! these mocks by adding methods that mirror the production trait surface.
//!
//! See `.claude/skills/tdd-rust/SKILL.md` §4 for usage rules and
//! `.trae/documents/TDD落地执行清单.md` Phase 3 for the extension plan.
//!
//! # Existing patterns
//!
//! - [`crate::UserStore`] trait + [`crate::UserStorage`] (Postgres) + [`FakeUserStore`] (in-memory)
//! - Engineers should follow the same trait + 2-impl seam for new storage mocks.

pub use crate::user_store_fake::FakeUserStore;

use std::sync::Arc;

/// Type alias for ergonomic service-layer injection.
pub type SharedFakeUserStore = Arc<FakeUserStore>;

/// Construct an [`Arc<FakeUserStore>`] ready for injection into services
/// that expect `Arc<dyn UserStore>`.
pub fn shared_fake_user_store() -> SharedFakeUserStore {
    Arc::new(FakeUserStore::new())
}

/// Async seeding helper — applies a list of pre-built `LockedUser` rows
/// through the public trait method so the fake store's invariants stay
/// consistent. Use inside `#[tokio::test]` rather than a sync builder.
///
/// Example:
///
/// ```no_run
/// # use synapse_storage::test_mocks::{shared_fake_user_store, seed_locked_users};
/// # use synapse_storage::LockedUser;
/// # async fn example() {
/// let store = shared_fake_user_store();
/// seed_locked_users(&store, vec![
///     LockedUser {
///         id: 1, user_id: "@bad:example.com".into(),
///         reason: Some("spam".into()), locked_by: "@admin:example.com".into(),
///         created_ts: 1_700_000_000_000, unlocked_ts: None, is_active: true,
///     },
/// ]).await;
/// # }
/// ```
pub async fn seed_locked_users(store: &FakeUserStore, users: Vec<crate::LockedUser>) {
    use crate::user::UserStore;
    for user in users {
        // FakeUserStore::lock_user is idempotent for active locks per its
        // own internal invariant, so re-applying seed rows is safe.
        let _ = store.lock_user(&user.user_id, user.reason.as_deref(), &user.locked_by, user.created_ts).await;
    }
}

// =============================================================================
// InMemory storage adapters — standalone test doubles (no DB required)
// =============================================================================
//
// These mirror the key methods of the production storage types and store data
// in `HashMap`/`Vec` behind `RwLock`. Tests construct them directly without
// needing a Postgres pool.
//
// Trait extraction (Phase 3) will enable `Arc<dyn Trait>` injection so that
// these can be swapped into services. Until then, use them directly in
// unit tests or via `MockSyncServiceDepsBuilder` in synapse-services.

use std::collections::{HashMap, HashSet};
use tokio::sync::RwLock;

use crate::admin_media::{
    encode_media_cursor, AdminMediaInfo, AdminMediaPage, AdminMediaQuotaSummary, AdminMediaStoreApi, MediaCursor,
};
use crate::audit::{
    encode_audit_event_cursor, AuditEvent, AuditEventCursor, AuditEventFilters, AuditEventStoreApi,
    CreateAuditEventRequest,
};
use crate::background_update::{
    BackgroundUpdate, BackgroundUpdateHistory, BackgroundUpdateStats, BackgroundUpdateStoreApi,
    CreateBackgroundUpdateRequest,
};
#[cfg(feature = "cas-sso")]
use crate::cas::{
    CasProxyGrantingTicket, CasProxyTicket, CasRegisteredService, CasSloSession, CasStoreApi, CasTicket,
    CasUserAttribute, CreatePgtRequest, CreateProxyTicketRequest, CreateTicketRequest, RegisterServiceRequest,
};
use crate::rate_limit::RateLimitStoreApi;
use crate::room_tag::RoomTagStoreApi;
use crate::threepid::{ThreepidStoreApi, UserThreepid};
use chrono::Utc;
use synapse_common::ApiError;

// ── InMemoryRoomStore ────────────────────────────────────────────────

/// In-memory room store mirroring [`crate::room::RoomStorage`].
#[derive(Clone, Default)]
pub struct InMemoryRoomStore {
    rooms: Arc<RwLock<HashMap<String, crate::room::Room>>>,
    aliases: Arc<RwLock<HashMap<String, String>>>,   // alias → room_id
    directories: Arc<RwLock<HashMap<String, bool>>>, // room_id → is_public
}

impl InMemoryRoomStore {
    pub fn new() -> Self {
        Self {
            rooms: Arc::new(RwLock::new(HashMap::new())),
            aliases: Arc::new(RwLock::new(HashMap::new())),
            directories: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn create_room(
        &self,
        room_id: &str,
        creator: &str,
        join_rule: &str,
        version: &str,
        is_public: bool,
    ) -> Result<crate::room::Room, String> {
        let room = crate::room::Room {
            room_id: room_id.to_string(),
            name: None,
            topic: None,
            avatar_url: None,
            canonical_alias: None,
            join_rule: join_rule.to_string(),
            creator_user_id: Some(creator.to_string()),
            room_version: version.to_string(),
            encryption: None,
            is_public,
            member_count: 0,
            history_visibility: "shared".to_string(),
            created_ts: 1_700_000_000_000,
            is_federatable: true,
            is_spotlight: false,
            is_flagged: false,
        };
        self.rooms.write().await.insert(room_id.to_string(), room.clone());
        Ok(room)
    }

    pub async fn get_room(&self, room_id: &str) -> Result<Option<crate::room::Room>, String> {
        Ok(self.rooms.read().await.get(room_id).cloned())
    }

    pub async fn get_rooms_batch(&self, room_ids: &[String]) -> Result<Vec<crate::room::Room>, String> {
        let rooms = self.rooms.read().await;
        Ok(room_ids.iter().filter_map(|id| rooms.get(id).cloned()).collect())
    }

    pub async fn room_exists(&self, room_id: &str) -> Result<bool, String> {
        Ok(self.rooms.read().await.contains_key(room_id))
    }

    pub async fn get_user_rooms(&self, _user_id: &str) -> Result<Vec<String>, String> {
        // This data lives in InMemoryMemberStore — stub returns all rooms.
        // Real implementation would join with membership data.
        Ok(self.rooms.read().await.keys().cloned().collect())
    }

    pub async fn get_rooms_map(&self, room_ids: &[String]) -> Result<HashMap<String, crate::room::Room>, String> {
        let rooms = self.rooms.read().await;
        Ok(room_ids.iter().filter_map(|id| rooms.get(id).map(|r| (id.clone(), r.clone()))).collect())
    }

    pub async fn update_room_name(&self, room_id: &str, name: &str) -> Result<(), String> {
        self.rooms
            .write()
            .await
            .get_mut(room_id)
            .map(|r| r.name = Some(name.to_string()))
            .ok_or_else(|| format!("room {room_id} not found"))
    }

    pub async fn set_room_alias(&self, room_id: &str, alias: &str, _created_by: &str) -> Result<(), String> {
        if !self.rooms.read().await.contains_key(room_id) {
            return Err(format!("room {room_id} not found"));
        }
        self.aliases.write().await.insert(alias.to_string(), room_id.to_string());
        Ok(())
    }

    pub async fn get_room_by_alias(&self, alias: &str) -> Result<Option<String>, String> {
        Ok(self.aliases.read().await.get(alias).cloned())
    }

    pub async fn delete_room(&self, room_id: &str) -> Result<(), String> {
        self.rooms.write().await.remove(room_id);
        Ok(())
    }
}

// ── RoomStoreApi impl for InMemoryRoomStore ───────────────────────────

#[async_trait::async_trait]
impl crate::room::api::RoomStoreApi for InMemoryRoomStore {
    fn pool(&self) -> &Arc<sqlx::PgPool> {
        unimplemented!("InMemoryRoomStore has no database pool")
    }

    async fn create_room(
        &self,
        room_id: &str,
        creator: &str,
        join_rule: &str,
        version: &str,
        is_public: bool,
    ) -> Result<crate::room::Room, sqlx::Error> {
        let room = crate::room::Room {
            room_id: room_id.to_string(),
            name: None,
            topic: None,
            avatar_url: None,
            canonical_alias: None,
            join_rule: join_rule.to_string(),
            creator_user_id: Some(creator.to_string()),
            room_version: version.to_string(),
            encryption: None,
            is_public,
            member_count: 0,
            history_visibility: "shared".to_string(),
            created_ts: 1_700_000_000_000,
            is_federatable: true,
            is_spotlight: false,
            is_flagged: false,
        };
        self.rooms.write().await.insert(room_id.to_string(), room.clone());
        Ok(room)
    }

    async fn create_room_in_tx(
        &self,
        _tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        room_id: &str,
        creator: &str,
        join_rule: &str,
        version: &str,
        is_public: bool,
    ) -> Result<crate::room::Room, sqlx::Error> {
        let room = crate::room::Room {
            room_id: room_id.to_string(),
            name: None,
            topic: None,
            avatar_url: None,
            canonical_alias: None,
            join_rule: join_rule.to_string(),
            creator_user_id: Some(creator.to_string()),
            room_version: version.to_string(),
            encryption: None,
            is_public,
            member_count: 0,
            history_visibility: "shared".to_string(),
            created_ts: 1_700_000_000_000,
            is_federatable: true,
            is_spotlight: false,
            is_flagged: false,
        };
        self.rooms.write().await.insert(room_id.to_string(), room.clone());
        Ok(room)
    }

    async fn get_room(&self, room_id: &str) -> Result<Option<crate::room::Room>, sqlx::Error> {
        Ok(self.rooms.read().await.get(room_id).cloned())
    }

    async fn room_exists(&self, room_id: &str) -> Result<bool, sqlx::Error> {
        Ok(self.rooms.read().await.contains_key(room_id))
    }

    async fn get_public_rooms(&self, limit: i64) -> Result<Vec<crate::room::Room>, sqlx::Error> {
        let rooms = self.rooms.read().await;
        let mut matched: Vec<_> = rooms.values().filter(|r| r.is_public).cloned().collect();
        matched.truncate(limit as usize);
        Ok(matched)
    }

    async fn get_room_count(&self) -> Result<i64, sqlx::Error> {
        Ok(self.rooms.read().await.len() as i64)
    }

    async fn set_canonical_alias(&self, room_id: &str, alias: Option<&str>) -> Result<(), sqlx::Error> {
        let mut rooms = self.rooms.write().await;
        if let Some(room) = rooms.get_mut(room_id) {
            room.canonical_alias = alias.map(str::to_string);
        }
        Ok(())
    }

    async fn set_room_alias(&self, room_id: &str, alias: &str, _created_by: &str) -> Result<(), sqlx::Error> {
        if !self.rooms.read().await.contains_key(room_id) {
            return Err(sqlx::Error::Protocol("room not found".into()));
        }
        self.aliases.write().await.insert(alias.to_string(), room_id.to_string());
        Ok(())
    }

    async fn update_join_rule_in_tx(
        &self,
        _tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        room_id: &str,
        join_rule: &str,
    ) -> Result<(), sqlx::Error> {
        if let Some(room) = self.rooms.write().await.get_mut(room_id) {
            room.join_rule = join_rule.to_string();
        }
        Ok(())
    }

    async fn decrement_member_count(&self, room_id: &str) -> Result<(), sqlx::Error> {
        if let Some(room) = self.rooms.write().await.get_mut(room_id) {
            room.member_count = room.member_count.saturating_sub(1);
        }
        Ok(())
    }

    async fn get_unread_counts(
        &self,
        room_id: &str,
        _user_id: &str,
    ) -> Result<crate::room::RoomUnreadCounts, sqlx::Error> {
        Ok(crate::room::RoomUnreadCounts { room_id: room_id.to_string(), highlight_count: 0, notification_count: 0 })
    }

    async fn get_unread_counts_batch(
        &self,
        _room_ids: &[String],
        _user_id: &str,
    ) -> Result<Vec<crate::room::RoomUnreadCounts>, sqlx::Error> {
        Ok(Vec::new())
    }

    async fn update_room_name(&self, room_id: &str, name: &str) -> Result<(), sqlx::Error> {
        if let Some(room) = self.rooms.write().await.get_mut(room_id) {
            room.name = Some(name.to_string());
        }
        Ok(())
    }

    async fn update_room_name_in_tx(
        &self,
        _tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        room_id: &str,
        name: &str,
    ) -> Result<(), sqlx::Error> {
        if let Some(room) = self.rooms.write().await.get_mut(room_id) {
            room.name = Some(name.to_string());
        }
        Ok(())
    }

    async fn update_room_topic(&self, room_id: &str, topic: &str) -> Result<(), sqlx::Error> {
        if let Some(room) = self.rooms.write().await.get_mut(room_id) {
            room.topic = Some(topic.to_string());
        }
        Ok(())
    }

    async fn update_room_topic_in_tx(
        &self,
        _tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        room_id: &str,
        topic: &str,
    ) -> Result<(), sqlx::Error> {
        self.update_room_topic(room_id, topic).await
    }

    async fn copy_room_state(&self, _source_room_id: &str, _target_room_id: &str) -> Result<(), sqlx::Error> {
        Ok(())
    }

    async fn get_room_aliases(&self, room_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let aliases = self.aliases.read().await;
        Ok(aliases.iter().filter(|(_, rid)| *rid == room_id).map(|(alias, _)| alias.clone()).collect())
    }

    async fn get_room_by_alias(&self, alias: &str) -> Result<Option<String>, sqlx::Error> {
        Ok(self.aliases.read().await.get(alias).cloned())
    }

    async fn remove_room_alias(&self, room_id: &str) -> Result<(), sqlx::Error> {
        let mut aliases = self.aliases.write().await;
        aliases.retain(|_, rid| *rid != room_id);
        Ok(())
    }

    async fn remove_room_alias_by_name(&self, alias: &str) -> Result<(), sqlx::Error> {
        self.aliases.write().await.remove(alias);
        Ok(())
    }

    async fn is_room_in_directory(&self, room_id: &str) -> Result<bool, sqlx::Error> {
        let directories = self.directories.read().await;
        Ok(directories.get(room_id).copied().unwrap_or(false))
    }

    async fn set_room_directory(&self, room_id: &str, is_public: bool) -> Result<(), sqlx::Error> {
        self.directories.write().await.insert(room_id.to_string(), is_public);
        Ok(())
    }

    async fn remove_room_directory(&self, room_id: &str) -> Result<(), sqlx::Error> {
        self.directories.write().await.remove(room_id);
        Ok(())
    }

    // ── receipts / read markers ──────────────────────────────────────────

    async fn add_receipt(
        &self,
        _user_id: &str,
        _sent_to: &str,
        _room_id: &str,
        _event_id: &str,
        _receipt_type: &str,
        _data: &serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        // Receipts are not modeled in InMemoryRoomStore; no-op.
        Ok(())
    }

    async fn get_receipts(
        &self,
        _room_id: &str,
        _receipt_type: &str,
        _event_id: &str,
    ) -> Result<Vec<crate::room::Receipt>, sqlx::Error> {
        Ok(Vec::new())
    }

    async fn update_read_marker_with_type(
        &self,
        _room_id: &str,
        _user_id: &str,
        _event_id: &str,
        _marker_type: &str,
    ) -> Result<(), sqlx::Error> {
        // Read markers are not modeled in InMemoryRoomStore; no-op.
        Ok(())
    }
}

// ── InMemoryEventStore ───────────────────────────────────────────────

/// In-memory event store mirroring [`crate::event::EventStorage`].
#[derive(Clone, Default)]
pub struct InMemoryEventStore {
    events: Arc<RwLock<HashMap<String, crate::event::RoomEvent>>>, // event_id → event
}

impl InMemoryEventStore {
    pub fn new() -> Self {
        Self { events: Arc::new(RwLock::new(HashMap::new())) }
    }

    pub async fn create_event(
        &self,
        params: crate::event::CreateEventParams,
    ) -> Result<crate::event::RoomEvent, String> {
        let event = crate::event::RoomEvent {
            event_id: params.event_id.clone(),
            room_id: params.room_id.clone(),
            user_id: params.user_id.clone(),
            event_type: params.event_type.clone(),
            content: params.content.clone(),
            state_key: params.state_key.clone(),
            depth: 0,
            origin_server_ts: params.origin_server_ts,
            processed_ts: 1_700_000_000_000,
            not_before: 0,
            status: None,
            reference_image: None,
            origin: String::new(),
            stream_ordering: None,
            redacts: params.redacts.clone(),
        };
        self.events.write().await.insert(params.event_id, event.clone());
        Ok(event)
    }

    pub async fn get_event(&self, event_id: &str) -> Result<Option<crate::event::RoomEvent>, String> {
        Ok(self.events.read().await.get(event_id).cloned())
    }

    pub async fn get_room_events(&self, room_id: &str, limit: i64) -> Result<Vec<crate::event::RoomEvent>, String> {
        let events = self.events.read().await;
        let mut matched: Vec<_> = events.values().filter(|e| e.room_id == room_id).cloned().collect();
        matched.sort_by_key(|e| std::cmp::Reverse(e.origin_server_ts));
        matched.truncate(limit as usize);
        Ok(matched)
    }

    pub async fn get_room_events_paginated(
        &self,
        room_id: &str,
        _from: Option<i64>,
        limit: i64,
        _direction: &str,
    ) -> Result<Vec<crate::event::RoomEvent>, String> {
        self.get_room_events(room_id, limit).await
    }

    pub async fn find_missing_event_ids(&self, event_ids: &[String]) -> Result<Vec<String>, String> {
        let events = self.events.read().await;
        Ok(event_ids.iter().filter(|id| !events.contains_key(*id)).cloned().collect())
    }

    pub async fn redact_event_content(&self, event_id: &str, _redacted_by: Option<&str>) -> Result<(), String> {
        let mut events = self.events.write().await;
        if let Some(event) = events.get_mut(event_id) {
            event.content = serde_json::json!({});
            event.event_type = "m.room.redaction".to_string();
        }
        Ok(())
    }

    pub async fn get_room_events_by_type(
        &self,
        room_id: &str,
        event_type: &str,
    ) -> Result<Vec<crate::event::RoomEvent>, String> {
        let events = self.events.read().await;
        let mut matched: Vec<_> =
            events.values().filter(|e| e.room_id == room_id && e.event_type == event_type).cloned().collect();
        matched.sort_by_key(|e| std::cmp::Reverse(e.origin_server_ts));
        Ok(matched)
    }

    pub async fn count_room_events(&self, room_id: &str) -> Result<i64, String> {
        let events = self.events.read().await;
        Ok(events.values().filter(|e| e.room_id == room_id).count() as i64)
    }

    pub async fn get_state_event(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
    ) -> Result<Option<crate::event::StateEvent>, String> {
        let events = self.events.read().await;
        let found = events
            .values()
            .filter(|e| e.room_id == room_id && e.event_type == event_type && e.state_key.as_deref() == Some(state_key))
            .max_by_key(|e| e.origin_server_ts)
            .cloned();
        Ok(found.map(|e| crate::event::StateEvent {
            event_id: e.event_id,
            room_id: e.room_id,
            sender: e.user_id.clone(),
            event_type: Some(e.event_type),
            content: e.content,
            state_key: e.state_key,
            unsigned: None,
            is_redacted: Some(false),
            origin_server_ts: e.origin_server_ts,
            depth: Some(e.depth),
            processed_ts: Some(e.processed_ts),
            not_before: Some(e.not_before),
            status: e.status,
            reference_image: e.reference_image,
            origin: Some(e.origin),
            user_id: Some(e.user_id),
            stream_ordering: e.stream_ordering,
        }))
    }

    pub async fn get_state_events_by_type(
        &self,
        room_id: &str,
        event_type: &str,
    ) -> Result<Vec<crate::event::StateEvent>, String> {
        let events = self.events.read().await;
        let mut by_state_key: HashMap<&str, &crate::event::RoomEvent> = HashMap::new();
        for event in events.values() {
            if event.room_id != room_id || event.event_type != event_type {
                continue;
            }
            let Some(key) = event.state_key.as_deref() else { continue };
            by_state_key
                .entry(key)
                .and_modify(|prev| {
                    if event.origin_server_ts > prev.origin_server_ts {
                        *prev = event;
                    }
                })
                .or_insert(event);
        }
        let mut results: Vec<crate::event::StateEvent> = by_state_key
            .into_values()
            .map(|e| crate::event::StateEvent {
                event_id: e.event_id.clone(),
                room_id: e.room_id.clone(),
                sender: e.user_id.clone(),
                event_type: Some(e.event_type.clone()),
                content: e.content.clone(),
                state_key: e.state_key.clone(),
                unsigned: None,
                is_redacted: Some(false),
                origin_server_ts: e.origin_server_ts,
                depth: Some(e.depth),
                processed_ts: Some(e.processed_ts),
                not_before: Some(e.not_before),
                status: e.status.clone(),
                reference_image: e.reference_image.clone(),
                origin: Some(e.origin.clone()),
                user_id: Some(e.user_id.clone()),
                stream_ordering: e.stream_ordering,
            })
            .collect();
        results.sort_by_key(|e| std::cmp::Reverse(e.origin_server_ts));
        Ok(results)
    }

    pub async fn get_state_events_at_or_before(
        &self,
        room_id: &str,
        origin_server_ts: i64,
    ) -> Result<Vec<crate::event::StateEvent>, String> {
        let events = self.events.read().await;
        let mut by_state_key: HashMap<&str, &crate::event::RoomEvent> = HashMap::new();
        for event in events.values() {
            if event.room_id != room_id || event.origin_server_ts > origin_server_ts {
                continue;
            }
            let Some(key) = event.state_key.as_deref() else { continue };
            by_state_key
                .entry(key)
                .and_modify(|prev| {
                    if event.origin_server_ts > prev.origin_server_ts {
                        *prev = event;
                    }
                })
                .or_insert(event);
        }
        let mut results: Vec<crate::event::StateEvent> = by_state_key
            .into_values()
            .map(|e| crate::event::StateEvent {
                event_id: e.event_id.clone(),
                room_id: e.room_id.clone(),
                sender: e.user_id.clone(),
                event_type: Some(e.event_type.clone()),
                content: e.content.clone(),
                state_key: e.state_key.clone(),
                unsigned: None,
                is_redacted: Some(false),
                origin_server_ts: e.origin_server_ts,
                depth: Some(e.depth),
                processed_ts: Some(e.processed_ts),
                not_before: Some(e.not_before),
                status: e.status.clone(),
                reference_image: e.reference_image.clone(),
                origin: Some(e.origin.clone()),
                user_id: Some(e.user_id.clone()),
                stream_ordering: e.stream_ordering,
            })
            .collect();
        results.sort_by_key(|e| std::cmp::Reverse(e.origin_server_ts));
        Ok(results)
    }

    pub async fn seed_events(&self, events: Vec<crate::event::RoomEvent>) {
        let mut store = self.events.write().await;
        for event in events {
            store.insert(event.event_id.clone(), event);
        }
    }
}

// ── EventStoreApi impl for InMemoryEventStore ─────────────────────────

#[async_trait::async_trait]
impl crate::event::api::EventStoreApi for InMemoryEventStore {
    fn pool(&self) -> &Arc<sqlx::PgPool> {
        unimplemented!("InMemoryEventStore has no database pool")
    }

    async fn get_event(&self, event_id: &str) -> Result<Option<crate::event::RoomEvent>, sqlx::Error> {
        Ok(self.events.read().await.get(event_id).cloned())
    }

    async fn get_room_events(&self, room_id: &str, limit: i64) -> Result<Vec<crate::event::RoomEvent>, sqlx::Error> {
        let events = self.events.read().await;
        let mut matched: Vec<_> = events.values().filter(|e| e.room_id == room_id).cloned().collect();
        matched.sort_by_key(|e| std::cmp::Reverse(e.origin_server_ts));
        matched.truncate(limit as usize);
        Ok(matched)
    }

    async fn get_room_events_paginated(
        &self,
        room_id: &str,
        _from: Option<i64>,
        limit: i64,
        _direction: &str,
    ) -> Result<Vec<crate::event::RoomEvent>, sqlx::Error> {
        let events = self.events.read().await;
        let mut matched: Vec<_> = events.values().filter(|e| e.room_id == room_id).cloned().collect();
        matched.sort_by_key(|e| std::cmp::Reverse(e.origin_server_ts));
        matched.truncate(limit as usize);
        Ok(matched)
    }

    async fn get_room_events_batch(
        &self,
        room_ids: &[String],
        limit_per_room: i64,
    ) -> Result<HashMap<String, Vec<crate::event::RoomEvent>>, sqlx::Error> {
        let events = self.events.read().await;
        let mut result: HashMap<String, Vec<crate::event::RoomEvent>> =
            room_ids.iter().map(|id| (id.clone(), Vec::new())).collect();
        for (_eid, event) in events.iter() {
            if let Some(bucket) = result.get_mut(&event.room_id) {
                if bucket.len() < limit_per_room as usize {
                    bucket.push(event.clone());
                }
            }
        }
        for bucket in result.values_mut() {
            bucket.sort_by_key(|e| std::cmp::Reverse(e.origin_server_ts));
        }
        Ok(result)
    }

    async fn get_room_events_since_batch(
        &self,
        room_ids: &[String],
        since: i64,
        limit_per_room: i64,
    ) -> Result<HashMap<String, Vec<crate::event::RoomEvent>>, sqlx::Error> {
        let events = self.events.read().await;
        let mut result: HashMap<String, Vec<crate::event::RoomEvent>> =
            room_ids.iter().map(|id| (id.clone(), Vec::new())).collect();
        for (_eid, event) in events.iter() {
            if event.origin_server_ts <= since {
                continue;
            }
            if let Some(bucket) = result.get_mut(&event.room_id) {
                if bucket.len() < limit_per_room as usize {
                    bucket.push(event.clone());
                }
            }
        }
        for bucket in result.values_mut() {
            bucket.sort_by_key(|e| e.origin_server_ts);
        }
        Ok(result)
    }

    async fn get_room_events_since_stream_batch(
        &self,
        room_ids: &[String],
        since_stream_ordering: i64,
        limit_per_room: i64,
    ) -> Result<HashMap<String, Vec<crate::event::RoomEvent>>, sqlx::Error> {
        let events = self.events.read().await;
        let mut result: HashMap<String, Vec<crate::event::RoomEvent>> =
            room_ids.iter().map(|id| (id.clone(), Vec::new())).collect();
        for (_eid, event) in events.iter() {
            let stream_ord = event.stream_ordering.unwrap_or(0);
            if stream_ord <= since_stream_ordering {
                continue;
            }
            if let Some(bucket) = result.get_mut(&event.room_id) {
                if bucket.len() < limit_per_room as usize {
                    bucket.push(event.clone());
                }
            }
        }
        for bucket in result.values_mut() {
            bucket.sort_by_key(|e| e.stream_ordering.unwrap_or(0));
        }
        Ok(result)
    }

    async fn get_state_event(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
    ) -> Result<Option<crate::event::StateEvent>, sqlx::Error> {
        let events = self.events.read().await;
        let found = events
            .values()
            .filter(|e| e.room_id == room_id && e.event_type == event_type && e.state_key.as_deref() == Some(state_key))
            .max_by_key(|e| e.origin_server_ts)
            .cloned();
        Ok(found.map(|e| crate::event::StateEvent {
            event_id: e.event_id,
            room_id: e.room_id,
            sender: e.user_id.clone(),
            event_type: Some(e.event_type.clone()),
            content: e.content.clone(),
            state_key: e.state_key.clone(),
            unsigned: None,
            is_redacted: Some(false),
            origin_server_ts: e.origin_server_ts,
            depth: Some(e.depth),
            processed_ts: Some(e.processed_ts),
            not_before: Some(e.not_before),
            status: e.status,
            reference_image: e.reference_image,
            origin: Some(e.origin),
            user_id: Some(e.user_id),
            stream_ordering: e.stream_ordering,
        }))
    }

    async fn get_state_events_by_type(
        &self,
        room_id: &str,
        event_type: &str,
    ) -> Result<Vec<crate::event::StateEvent>, sqlx::Error> {
        self.get_state_events_by_type(room_id, event_type).await.map_err(sqlx::Error::Protocol)
    }

    async fn get_state_events_at_or_before(
        &self,
        room_id: &str,
        origin_server_ts: i64,
    ) -> Result<Vec<crate::event::StateEvent>, sqlx::Error> {
        self.get_state_events_at_or_before(room_id, origin_server_ts).await.map_err(sqlx::Error::Protocol)
    }

    async fn get_state_events(&self, room_id: &str) -> Result<Vec<crate::event::StateEvent>, sqlx::Error> {
        let events = self.events.read().await;
        let matched: Vec<_> = events
            .values()
            .filter(|e| e.room_id == room_id && e.state_key.is_some())
            .map(|e| crate::event::StateEvent {
                event_id: e.event_id.clone(),
                room_id: e.room_id.clone(),
                sender: e.user_id.clone(),
                event_type: Some(e.event_type.clone()),
                content: e.content.clone(),
                state_key: e.state_key.clone(),
                unsigned: None,
                is_redacted: Some(false),
                origin_server_ts: e.origin_server_ts,
                depth: Some(e.depth),
                processed_ts: Some(e.processed_ts),
                not_before: Some(e.not_before),
                status: e.status.clone(),
                reference_image: e.reference_image.clone(),
                origin: Some(e.origin.clone()),
                user_id: Some(e.user_id.clone()),
                stream_ordering: e.stream_ordering,
            })
            .collect();
        Ok(matched)
    }

    async fn get_events_map(
        &self,
        event_ids: &[String],
    ) -> Result<HashMap<String, crate::event::RoomEvent>, sqlx::Error> {
        let events = self.events.read().await;
        Ok(event_ids.iter().filter_map(|id| events.get(id).map(|e| (id.clone(), e.clone()))).collect())
    }

    async fn get_max_origin_server_ts_for_room(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        let events = self.events.read().await;
        Ok(events.values().filter(|e| e.room_id == room_id).map(|e| e.origin_server_ts).max().unwrap_or(0))
    }

    async fn get_latest_event_ids_in_room(&self, room_id: &str, limit: i64) -> Result<Vec<String>, sqlx::Error> {
        let events = self.events.read().await;
        let mut matched: Vec<_> = events.values().filter(|e| e.room_id == room_id).cloned().collect();
        matched.sort_by_key(|e| std::cmp::Reverse(e.origin_server_ts));
        matched.truncate(limit as usize);
        Ok(matched.into_iter().map(|e| e.event_id).collect())
    }

    async fn count_room_events_by_status(&self, room_id: &str, status: &str) -> Result<i64, sqlx::Error> {
        let events = self.events.read().await;
        Ok(events.values().filter(|e| e.room_id == room_id && e.status.as_deref() == Some(status)).count() as i64)
    }

    async fn create_event(
        &self,
        params: crate::event::CreateEventParams,
        _tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<crate::event::RoomEvent, sqlx::Error> {
        let event = crate::event::RoomEvent {
            event_id: params.event_id.clone(),
            room_id: params.room_id,
            user_id: params.user_id,
            event_type: params.event_type,
            content: params.content,
            state_key: params.state_key,
            depth: 0,
            origin_server_ts: params.origin_server_ts,
            processed_ts: chrono::Utc::now().timestamp_millis(),
            not_before: 0,
            status: Some("processed".to_string()),
            reference_image: None,
            origin: "self".to_string(),
            stream_ordering: Some(0),
            redacts: params.redacts,
        };
        self.events.write().await.insert(event.event_id.clone(), event.clone());
        Ok(event)
    }

    async fn update_event_signatures_and_hashes(
        &self,
        _event_id: &str,
        _signatures: &serde_json::Value,
        _hashes: &serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        Ok(())
    }

    async fn redact_event_content(&self, event_id: &str, _redacted_by: Option<&str>) -> Result<(), sqlx::Error> {
        let mut events = self.events.write().await;
        if let Some(event) = events.get_mut(event_id) {
            event.content = serde_json::json!({});
        }
        Ok(())
    }

    async fn get_ephemeral_events(
        &self,
        _room_id: &str,
        _now: i64,
        _limit: i64,
    ) -> Result<Vec<crate::event::RoomEphemeralEvent>, sqlx::Error> {
        Ok(Vec::new())
    }

    async fn get_ephemeral_events_batch(
        &self,
        room_ids: &[String],
        _now: i64,
        _limit: i64,
    ) -> Result<HashMap<String, Vec<crate::event::RoomEphemeralEvent>>, sqlx::Error> {
        Ok(room_ids.iter().map(|id| (id.clone(), Vec::new())).collect())
    }

    async fn get_state_events_batch(
        &self,
        room_ids: &[String],
    ) -> Result<HashMap<String, Vec<crate::event::StateEvent>>, sqlx::Error> {
        let events = self.events.read().await;
        let mut result: HashMap<String, Vec<crate::event::StateEvent>> =
            room_ids.iter().map(|id| (id.clone(), Vec::new())).collect();
        for event in events.values() {
            if let Some(bucket) = result.get_mut(&event.room_id) {
                if event.state_key.is_some() {
                    bucket.push(crate::event::StateEvent {
                        event_id: event.event_id.clone(),
                        room_id: event.room_id.clone(),
                        sender: event.user_id.clone(),
                        event_type: Some(event.event_type.clone()),
                        content: event.content.clone(),
                        state_key: event.state_key.clone(),
                        unsigned: None,
                        is_redacted: Some(false),
                        origin_server_ts: event.origin_server_ts,
                        depth: Some(event.depth),
                        processed_ts: Some(event.processed_ts),
                        not_before: Some(event.not_before),
                        status: event.status.clone(),
                        reference_image: event.reference_image.clone(),
                        origin: Some(event.origin.clone()),
                        user_id: Some(event.user_id.clone()),
                        stream_ordering: event.stream_ordering,
                    });
                }
            }
        }
        Ok(result)
    }

    async fn get_state_events_by_type_batch(
        &self,
        room_ids: &[String],
        event_type: &str,
    ) -> Result<HashMap<String, Vec<crate::event::StateEvent>>, sqlx::Error> {
        let events = self.events.read().await;
        let mut result: HashMap<String, Vec<crate::event::StateEvent>> =
            room_ids.iter().map(|id| (id.clone(), Vec::new())).collect();
        for event in events.values() {
            if let Some(bucket) = result.get_mut(&event.room_id) {
                if event.state_key.is_some() && event.event_type == event_type {
                    bucket.push(crate::event::StateEvent {
                        event_id: event.event_id.clone(),
                        room_id: event.room_id.clone(),
                        sender: event.user_id.clone(),
                        event_type: Some(event.event_type.clone()),
                        content: event.content.clone(),
                        state_key: event.state_key.clone(),
                        unsigned: None,
                        is_redacted: Some(false),
                        origin_server_ts: event.origin_server_ts,
                        depth: Some(event.depth),
                        processed_ts: Some(event.processed_ts),
                        not_before: Some(event.not_before),
                        status: event.status.clone(),
                        reference_image: event.reference_image.clone(),
                        origin: Some(event.origin.clone()),
                        user_id: Some(event.user_id.clone()),
                        stream_ordering: event.stream_ordering,
                    });
                }
            }
        }
        Ok(result)
    }

    async fn get_state_events_since_batch(
        &self,
        room_ids: &[String],
        since: crate::event::SinceFilter,
    ) -> Result<HashMap<String, Vec<crate::event::StateEvent>>, sqlx::Error> {
        let events = self.events.read().await;
        let filter_by = match since {
            crate::event::SinceFilter::OriginServerTs(ts) => {
                Box::new(move |e: &&crate::event::RoomEvent| e.state_key.is_some() && e.origin_server_ts > ts)
                    as Box<dyn Fn(&&crate::event::RoomEvent) -> bool>
            }
            crate::event::SinceFilter::StreamOrdering(ord) => Box::new(move |e: &&crate::event::RoomEvent| {
                e.state_key.is_some() && e.stream_ordering.unwrap_or(0) > ord
            }),
        };
        let mut result: HashMap<String, Vec<crate::event::StateEvent>> =
            room_ids.iter().map(|id| (id.clone(), Vec::new())).collect();
        for event in events.values().filter(filter_by) {
            if let Some(bucket) = result.get_mut(&event.room_id) {
                bucket.push(crate::event::StateEvent {
                    event_id: event.event_id.clone(),
                    room_id: event.room_id.clone(),
                    sender: event.user_id.clone(),
                    event_type: Some(event.event_type.clone()),
                    content: event.content.clone(),
                    state_key: event.state_key.clone(),
                    unsigned: None,
                    is_redacted: Some(false),
                    origin_server_ts: event.origin_server_ts,
                    depth: Some(event.depth),
                    processed_ts: Some(event.processed_ts),
                    not_before: Some(event.not_before),
                    status: event.status.clone(),
                    reference_image: event.reference_image.clone(),
                    origin: Some(event.origin.clone()),
                    user_id: Some(event.user_id.clone()),
                    stream_ordering: event.stream_ordering,
                });
            }
        }
        Ok(result)
    }

    async fn get_membership_state_keys_since_batch(
        &self,
        room_ids: &[String],
        _since: crate::event::SinceFilter,
    ) -> Result<HashMap<String, HashSet<String>>, sqlx::Error> {
        let events = self.events.read().await;
        let mut result: HashMap<String, HashSet<String>> =
            room_ids.iter().map(|id| (id.clone(), HashSet::new())).collect();
        for event in events.values() {
            if event.event_type == "m.room.member" {
                if let Some(ref state_key) = event.state_key {
                    if let Some(bucket) = result.get_mut(&event.room_id) {
                        bucket.insert(state_key.clone());
                    }
                }
            }
        }
        Ok(result)
    }

    async fn get_state_change_timestamps_batch(
        &self,
        room_ids: &[String],
        _since: crate::event::SinceFilter,
    ) -> Result<HashMap<String, i64>, sqlx::Error> {
        Ok(room_ids.iter().map(|id| (id.clone(), 0)).collect())
    }

    async fn get_room_events_batch_filtered(
        &self,
        room_ids: &[String],
        limit_per_room: i64,
        _filter: &crate::event::EventQueryFilter,
    ) -> Result<HashMap<String, Vec<crate::event::RoomEvent>>, sqlx::Error> {
        // Simplified: ignores filter, delegates to unfiltered batch
        self.get_room_events_batch(room_ids, limit_per_room).await
    }

    async fn get_room_events_since_batch_filtered(
        &self,
        room_ids: &[String],
        since: i64,
        limit_per_room: i64,
        _filter: &crate::event::EventQueryFilter,
    ) -> Result<HashMap<String, Vec<crate::event::RoomEvent>>, sqlx::Error> {
        self.get_room_events_since_batch(room_ids, since, limit_per_room).await
    }

    async fn get_room_events_since_stream_batch_filtered(
        &self,
        room_ids: &[String],
        since_stream_ordering: i64,
        limit_per_room: i64,
        _filter: &crate::event::EventQueryFilter,
    ) -> Result<HashMap<String, Vec<crate::event::RoomEvent>>, sqlx::Error> {
        self.get_room_events_since_stream_batch(room_ids, since_stream_ordering, limit_per_room).await
    }

    async fn has_room_events_since(&self, room_ids: &[String], since: i64) -> Result<bool, sqlx::Error> {
        let events = self.events.read().await;
        for event in events.values() {
            if room_ids.contains(&event.room_id) && event.origin_server_ts > since {
                return Ok(true);
            }
        }
        Ok(false)
    }

    // ── graph / dag ──────────────────────────────────────────────────────

    async fn find_missing_event_ids(&self, event_ids: &[String]) -> Result<Vec<String>, sqlx::Error> {
        let events = self.events.read().await;
        Ok(event_ids.iter().filter(|id| !events.contains_key(*id)).cloned().collect())
    }

    async fn get_missing_events_between(
        &self,
        _room_id: &str,
        _earliest_events: &[String],
        _latest_events: &[String],
        _limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        // Graph traversal is not modeled in-memory; return empty.
        Ok(Vec::new())
    }

    async fn get_forward_extremities_count(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        // Approximation: count events in the room. Real extremity tracking
        // is not modeled in-memory.
        let events = self.events.read().await;
        Ok(events.values().filter(|e| e.room_id == room_id).count() as i64)
    }

    // ── context / pagination ────────────────────────────────────────────

    async fn find_event_id_by_timestamp(
        &self,
        room_id: &str,
        ts: i64,
        forward: bool,
    ) -> Result<Option<(String, i64)>, sqlx::Error> {
        let events = self.events.read().await;
        let mut matched: Vec<_> = events.values().filter(|e| e.room_id == room_id).collect();
        if forward {
            matched.sort_by_key(|e| e.origin_server_ts);
        } else {
            matched.sort_by_key(|e| std::cmp::Reverse(e.origin_server_ts));
        }
        let found = if forward {
            matched.iter().find(|e| e.origin_server_ts >= ts)
        } else {
            matched.iter().find(|e| e.origin_server_ts <= ts)
        };
        Ok(found.map(|e| (e.event_id.clone(), e.origin_server_ts)))
    }

    async fn get_events_before_context(
        &self,
        _room_id: &str,
        _before_ts: i64,
        _limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        // Context pagination is not modeled in-memory; return empty.
        Ok(Vec::new())
    }

    async fn get_events_after_context(
        &self,
        _room_id: &str,
        _after_ts: i64,
        _limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        // Context pagination is not modeled in-memory; return empty.
        Ok(Vec::new())
    }

    // ── by-type / pending / counts ──────────────────────────────────────

    async fn get_room_events_by_type(
        &self,
        room_id: &str,
        event_type: &str,
        limit: i64,
    ) -> Result<Vec<crate::event::RoomEvent>, sqlx::Error> {
        let events = self.events.read().await;
        let mut matched: Vec<_> =
            events.values().filter(|e| e.room_id == room_id && e.event_type == event_type).cloned().collect();
        matched.sort_by_key(|e| std::cmp::Reverse(e.origin_server_ts));
        matched.truncate(limit as usize);
        Ok(matched)
    }

    async fn get_pending_room_events(
        &self,
        room_id: &str,
        limit: i64,
    ) -> Result<Vec<crate::event::RoomEvent>, sqlx::Error> {
        // In-memory events are immediately "processed"; no pending queue.
        // Inline the trait's get_room_events logic to avoid calling the
        // inherent String-returning method of the same name.
        let events = self.events.read().await;
        let mut matched: Vec<_> = events.values().filter(|e| e.room_id == room_id).cloned().collect();
        matched.sort_by_key(|e| std::cmp::Reverse(e.origin_server_ts));
        matched.truncate(limit as usize);
        Ok(matched)
    }

    async fn get_daily_message_count(&self) -> Result<i64, sqlx::Error> {
        let one_day_ago = chrono::Utc::now().timestamp_millis() - 86_400_000;
        let events = self.events.read().await;
        Ok(events.values().filter(|e| e.origin_server_ts >= one_day_ago).count() as i64)
    }

    // ── mutation: graph / signatures / reports ─────────────────────────

    async fn create_event_with_graph(
        &self,
        params: crate::event::CreateEventParams,
        _prev_events: &[String],
        _auth_events: &[String],
        depth: i64,
        _tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<crate::event::RoomEvent, sqlx::Error> {
        // Reuse the simpler create_event path; ignore graph metadata.
        let _ = depth;
        let event = crate::event::RoomEvent {
            event_id: params.event_id.clone(),
            room_id: params.room_id,
            user_id: params.user_id,
            event_type: params.event_type,
            content: params.content,
            state_key: params.state_key,
            depth,
            origin_server_ts: params.origin_server_ts,
            processed_ts: chrono::Utc::now().timestamp_millis(),
            not_before: 0,
            status: None,
            reference_image: None,
            origin: String::new(),
            stream_ordering: None,
            redacts: params.redacts,
        };
        self.events.write().await.insert(event.event_id.clone(), event.clone());
        Ok(event)
    }

    async fn save_event_signature(
        &self,
        _event_id: &str,
        _user_id: &str,
        _device_id: &str,
        _signature: &str,
        _key_id: &str,
        _algorithm: &str,
        _created_ts: i64,
    ) -> Result<(), sqlx::Error> {
        // Signatures are not modeled in-memory; no-op.
        Ok(())
    }

    async fn get_event_signatures(&self, _event_id: &str) -> Result<Vec<crate::event::EventSignature>, sqlx::Error> {
        Ok(Vec::new())
    }

    async fn report_event(
        &self,
        _event_id: &str,
        _room_id: &str,
        _reported_user_id: &str,
        _reporter_user_id: &str,
        _reason: Option<&str>,
        _score: i32,
    ) -> Result<i64, sqlx::Error> {
        // Reports are not modeled in-memory; return a synthetic id.
        Ok(0)
    }

    async fn search_room_messages_admin(
        &self,
        _room_id: &str,
        _search_pattern: &str,
        _limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        // Admin search is not modeled in-memory; return empty.
        Ok(Vec::new())
    }

    // ── ephemeral mutations ─────────────────────────────────────────────

    async fn add_ephemeral_event(
        &self,
        _room_id: &str,
        _user_id: &str,
        _event_type: &str,
        _content: &serde_json::Value,
        _stream_id: i64,
    ) -> Result<(), sqlx::Error> {
        // Ephemeral events are not modeled in-memory; no-op.
        Ok(())
    }

    async fn upsert_ephemeral_event(
        &self,
        _room_id: &str,
        _user_id: &str,
        _event_type: &str,
        _content: &serde_json::Value,
        _stream_id: i64,
        _created_ts: i64,
        _expires_at: Option<i64>,
    ) -> Result<(), sqlx::Error> {
        Ok(())
    }

    async fn delete_ephemeral_event(
        &self,
        _room_id: &str,
        _event_type: &str,
        _user_id: &str,
    ) -> Result<(), sqlx::Error> {
        Ok(())
    }

    // ── encryption / retention ─────────────────────────────────────────────

    async fn check_room_has_encryption(&self, room_id: &str) -> Result<bool, sqlx::Error> {
        let events = self.events.read().await;
        Ok(events
            .values()
            .any(|e| e.room_id == room_id && e.event_type == "m.room.encryption" && e.state_key.is_some()))
    }

    async fn delete_events_before(&self, room_id: &str, timestamp: i64) -> Result<u64, sqlx::Error> {
        let mut events = self.events.write().await;
        let before = events.len() as u64;
        events.retain(|_, e| {
            !(e.room_id == room_id && e.origin_server_ts < timestamp && e.event_type != "m.room.create")
        });
        Ok(before - events.len() as u64)
    }

    async fn upsert_power_levels_event(
        &self,
        event_id: &str,
        room_id: &str,
        user_id: &str,
        content: serde_json::Value,
        origin_server_ts: i64,
        _sender: &str,
    ) -> Result<(), sqlx::Error> {
        use crate::event::RoomEvent;
        self.events.write().await.insert(
            event_id.to_string(),
            RoomEvent {
                event_id: event_id.to_string(),
                room_id: room_id.to_string(),
                user_id: user_id.to_string(),
                event_type: "m.room.power_levels".to_string(),
                content,
                state_key: Some(String::new()),
                depth: 0,
                origin_server_ts,
                processed_ts: 0,
                not_before: 0,
                status: None,
                reference_image: None,
                origin: String::new(),
                stream_ordering: None,
                redacts: None,
            },
        );
        Ok(())
    }
}

// ── InMemoryMemberStore ──────────────────────────────────────────────

/// In-memory member store mirroring [`crate::membership::RoomMemberStorage`].
#[derive(Clone, Default)]
pub struct InMemoryMemberStore {
    #[allow(clippy::type_complexity)]
    members: Arc<RwLock<HashMap<(String, String), crate::membership::RoomMember>>>, // (room_id, user_id) → member
}

impl InMemoryMemberStore {
    pub fn new() -> Self {
        Self { members: Arc::new(RwLock::new(HashMap::new())) }
    }

    pub async fn add_member(
        &self,
        room_id: &str,
        user_id: &str,
        membership: &str,
        display_name: Option<&str>,
    ) -> Result<crate::membership::RoomMember, String> {
        let member = crate::membership::RoomMember {
            room_id: room_id.to_string(),
            user_id: user_id.to_string(),
            sender: None,
            membership: membership.to_string(),
            event_id: None,
            event_type: None,
            display_name: display_name.map(str::to_string),
            avatar_url: None,
            is_banned: Some(membership == "ban"),
            invite_token: None,
            updated_ts: Some(1_700_000_000_000),
            joined_ts: if membership == "join" { Some(1_700_000_000_000) } else { None },
            left_ts: None,
            reason: None,
            banned_by: None,
            ban_reason: None,
            banned_ts: None,
            join_reason: None,
        };
        self.members.write().await.insert((room_id.to_string(), user_id.to_string()), member.clone());
        Ok(member)
    }

    pub async fn get_member(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> Result<Option<crate::membership::RoomMember>, String> {
        Ok(self.members.read().await.get(&(room_id.to_string(), user_id.to_string())).cloned())
    }

    pub async fn get_room_members(
        &self,
        room_id: &str,
        membership_type: &str,
    ) -> Result<Vec<crate::membership::RoomMember>, String> {
        let members = self.members.read().await;
        Ok(members.values().filter(|m| m.room_id == room_id && m.membership == membership_type).cloned().collect())
    }

    pub async fn get_joined_members(&self, room_id: &str) -> Result<Vec<crate::membership::RoomMember>, String> {
        self.get_room_members(room_id, "join").await
    }

    pub async fn get_joined_rooms(&self, user_id: &str) -> Result<Vec<String>, String> {
        let members = self.members.read().await;
        Ok(members
            .iter()
            .filter(|((_, uid), m)| uid == user_id && m.membership == "join")
            .map(|((rid, _), _)| rid.clone())
            .collect())
    }

    pub async fn get_membership_state(&self, room_id: &str, user_id: &str) -> Result<Option<String>, String> {
        Ok(self.members.read().await.get(&(room_id.to_string(), user_id.to_string())).map(|m| m.membership.clone()))
    }

    pub async fn get_room_member_count(&self, room_id: &str) -> Result<i64, String> {
        let members = self.members.read().await;
        Ok(members.values().filter(|m| m.room_id == room_id && m.membership == "join").count() as i64)
    }

    pub async fn remove_member(&self, room_id: &str, user_id: &str) -> Result<(), String> {
        self.members.write().await.remove(&(room_id.to_string(), user_id.to_string()));
        Ok(())
    }

    pub async fn ban_member(&self, room_id: &str, user_id: &str, banned_by: &str) -> Result<(), String> {
        let mut members = self.members.write().await;
        if let Some(member) = members.get_mut(&(room_id.to_string(), user_id.to_string())) {
            member.membership = "ban".to_string();
            member.is_banned = Some(true);
            member.banned_by = Some(banned_by.to_string());
            member.banned_ts = Some(1_700_000_000_000);
        }
        Ok(())
    }

    pub async fn is_member(&self, room_id: &str, user_id: &str) -> Result<bool, String> {
        Ok(self.members.read().await.contains_key(&(room_id.to_string(), user_id.to_string())))
    }

    /// Seed multiple members at once for test setup.
    pub async fn seed_members(&self, members: Vec<crate::membership::RoomMember>) {
        let mut store = self.members.write().await;
        for member in members {
            store.insert((member.room_id.clone(), member.user_id.clone()), member);
        }
    }
}

// ── MemberStoreApi impl for InMemoryMemberStore ───────────────────────

#[async_trait::async_trait]
impl crate::membership::api::MemberStoreApi for InMemoryMemberStore {
    fn pool(&self) -> &Arc<sqlx::PgPool> {
        unimplemented!("InMemoryMemberStore has no database pool")
    }

    async fn get_room_members(
        &self,
        room_id: &str,
        membership_type: &str,
    ) -> Result<Vec<crate::membership::RoomMember>, sqlx::Error> {
        let members = self.members.read().await;
        Ok(members.values().filter(|m| m.room_id == room_id && m.membership == membership_type).cloned().collect())
    }

    async fn get_members_batch(
        &self,
        room_ids: &[String],
        membership_type: &str,
    ) -> Result<HashMap<String, Vec<crate::membership::RoomMember>>, sqlx::Error> {
        let members = self.members.read().await;
        let mut result: HashMap<String, Vec<crate::membership::RoomMember>> =
            room_ids.iter().map(|id| (id.clone(), Vec::new())).collect();
        for member in members.values() {
            if member.membership == membership_type {
                if let Some(bucket) = result.get_mut(&member.room_id) {
                    bucket.push(member.clone());
                }
            }
        }
        Ok(result)
    }

    async fn get_joined_rooms(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let members = self.members.read().await;
        Ok(members
            .iter()
            .filter(|((_, uid), m)| uid == user_id && m.membership == "join")
            .map(|((rid, _), _)| rid.clone())
            .collect())
    }

    async fn get_shared_room_users(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let members = self.members.read().await;
        // Find rooms the user is joined to
        let user_rooms: Vec<String> = members
            .iter()
            .filter(|((_, uid), m)| uid == user_id && m.membership == "join")
            .map(|((rid, _), _)| rid.clone())
            .collect();
        // Collect all other users in those rooms
        let mut shared: Vec<String> = members
            .iter()
            .filter(|((_, uid), m)| uid != user_id && m.membership == "join" && user_rooms.contains(&m.room_id))
            .map(|((_, uid), _)| uid.clone())
            .collect();
        shared.sort();
        shared.dedup();
        Ok(shared)
    }

    async fn get_sync_rooms(
        &self,
        user_id: &str,
        include_leave: bool,
    ) -> Result<Vec<crate::membership::UserRoomMembership>, sqlx::Error> {
        let members = self.members.read().await;
        let mut result: Vec<crate::membership::UserRoomMembership> = members
            .iter()
            .filter(|((_, uid), m)| uid == user_id && (include_leave || m.membership != "leave"))
            .map(|((rid, _), m)| crate::membership::UserRoomMembership {
                room_id: rid.clone(),
                membership: m.membership.clone(),
            })
            .collect();
        result.sort_by(|a, b| a.room_id.cmp(&b.room_id));
        result.dedup_by(|a, b| a.room_id == b.room_id);
        Ok(result)
    }

    async fn remove_member(&self, room_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
        let mut members = self.members.write().await;
        if let Some(member) = members.get_mut(&(room_id.to_string(), user_id.to_string())) {
            member.membership = "leave".to_string();
        }
        Ok(())
    }

    async fn is_member(&self, room_id: &str, user_id: &str) -> Result<bool, sqlx::Error> {
        Ok(self.members.read().await.contains_key(&(room_id.to_string(), user_id.to_string())))
    }

    async fn get_room_member(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> Result<Option<crate::membership::RoomMember>, sqlx::Error> {
        Ok(self.members.read().await.get(&(room_id.to_string(), user_id.to_string())).cloned())
    }

    #[allow(clippy::too_many_arguments)]
    async fn add_member(
        &self,
        room_id: &str,
        user_id: &str,
        membership: &str,
        display_name: Option<&str>,
        _join_reason: Option<&str>,
        sender: Option<&str>,
        _tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<crate::membership::RoomMember, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let joined_ts = if membership == "join" { Some(now) } else { None };
        let member = crate::membership::RoomMember {
            room_id: room_id.to_string(),
            user_id: user_id.to_string(),
            sender: sender.map(|s| s.to_string()),
            membership: membership.to_string(),
            event_id: Some(format!("$auto_{}", chrono::Utc::now().timestamp_millis())),
            event_type: Some("m.room.member".to_string()),
            display_name: display_name.map(|s| s.to_string()),
            avatar_url: None,
            is_banned: None,
            invite_token: None,
            updated_ts: Some(now),
            joined_ts,
            left_ts: None,
            reason: None,
            banned_by: None,
            ban_reason: None,
            banned_ts: None,
            join_reason: _join_reason.map(|s| s.to_string()),
        };
        self.members.write().await.insert((room_id.to_string(), user_id.to_string()), member.clone());
        Ok(member)
    }

    async fn get_joined_room_count(&self, user_id: &str) -> Result<i64, sqlx::Error> {
        let members = self.members.read().await;
        Ok(members.iter().filter(|((_, uid), m)| uid == user_id && m.membership == "join").count() as i64)
    }
}

// =============================================================================
// Phase 3 complete: all storage traits extracted
// =============================================================================
//
// EventStoreApi (event/api.rs) — 14 methods covering single-event, bulk-read,
//   state events, helpers, and mutation operations.
// RoomStoreApi (room/api.rs) — 11 methods covering room CRUD, aliases, join rules,
//   and member counts.
// MemberStoreApi (membership/api.rs) — 6 methods covering member queries, batches,
//   shared-room discovery, sync, and removal.
//
// Remaining work: update service consumers from Arc<ConcreteType> to Arc<dyn Trait>.

// ── InMemoryOidcUserMappingStore ──────────────────────────────────────

/// In-memory OIDC user mapping store mirroring [`crate::oidc_user_mapping::OidcUserMappingStorage`].
#[derive(Clone, Default)]
#[allow(clippy::type_complexity)]
pub struct InMemoryOidcUserMappingStore {
    mappings: Arc<RwLock<HashMap<(String, String), (String, i64, i64, i64)>>>,
}

impl InMemoryOidcUserMappingStore {
    pub fn new() -> Self {
        Self { mappings: Arc::new(RwLock::new(HashMap::new())) }
    }
}

#[async_trait::async_trait]
impl crate::oidc_user_mapping::OidcUserMappingStoreApi for InMemoryOidcUserMappingStore {
    async fn get_bound_user_id(&self, issuer: &str, subject: &str) -> Result<Option<String>, sqlx::Error> {
        Ok(self.mappings.read().await.get(&(issuer.to_string(), subject.to_string())).map(|v| v.0.clone()))
    }

    async fn update_last_authenticated(&self, issuer: &str, subject: &str, now_ts: i64) -> Result<(), sqlx::Error> {
        let mut map = self.mappings.write().await;
        if let Some(entry) = map.get_mut(&(issuer.to_string(), subject.to_string())) {
            entry.2 = now_ts;
            entry.3 += 1;
        }
        Ok(())
    }

    async fn insert_mapping(&self, issuer: &str, subject: &str, user_id: &str, now_ts: i64) -> Result<(), sqlx::Error> {
        self.mappings
            .write()
            .await
            .insert((issuer.to_string(), subject.to_string()), (user_id.to_string(), now_ts, now_ts, 1));
        Ok(())
    }
}

// ── InMemoryAiConnectionStore ─────────────────────────────────────────

#[cfg(feature = "openclaw-routes")]
#[derive(Clone, Default)]
pub struct InMemoryAiConnectionStore {
    connections: Arc<RwLock<HashMap<String, crate::ai_connection::AiConnection>>>,
}

#[cfg(feature = "openclaw-routes")]
impl InMemoryAiConnectionStore {
    pub fn new() -> Self {
        Self { connections: Arc::new(RwLock::new(HashMap::new())) }
    }
}

#[cfg(feature = "openclaw-routes")]
#[async_trait::async_trait]
impl crate::ai_connection::AiConnectionStoreApi for InMemoryAiConnectionStore {
    async fn create_connection(&self, conn: &crate::ai_connection::AiConnection) -> Result<(), sqlx::Error> {
        self.connections.write().await.insert(conn.id.clone(), conn.clone());
        Ok(())
    }

    async fn get_connection(&self, id: &str) -> Result<Option<crate::ai_connection::AiConnection>, sqlx::Error> {
        Ok(self.connections.read().await.get(id).cloned())
    }

    async fn get_user_connections(
        &self,
        user_id: &str,
    ) -> Result<Vec<crate::ai_connection::AiConnection>, sqlx::Error> {
        let mut results: Vec<_> =
            self.connections.read().await.values().filter(|c| c.user_id == user_id).cloned().collect();
        results.sort_by_key(|c| std::cmp::Reverse(c.created_ts));
        Ok(results)
    }

    async fn get_user_provider_connection(
        &self,
        user_id: &str,
        provider: &str,
    ) -> Result<Option<crate::ai_connection::AiConnection>, sqlx::Error> {
        let conns = self.connections.read().await;
        let mut matches: Vec<_> =
            conns.values().filter(|c| c.user_id == user_id && c.provider == provider && c.is_active).collect();
        matches.sort_by_key(|c| std::cmp::Reverse(c.created_ts));
        Ok(matches.first().cloned().cloned())
    }

    async fn update_connection_status(&self, id: &str, is_active: bool) -> Result<(), sqlx::Error> {
        if let Some(conn) = self.connections.write().await.get_mut(id) {
            conn.is_active = is_active;
            conn.updated_ts = Some(chrono::Utc::now().timestamp_millis());
        }
        Ok(())
    }

    async fn delete_connection(&self, id: &str) -> Result<(), sqlx::Error> {
        self.connections.write().await.remove(id);
        Ok(())
    }
}

// ── InMemoryRateLimitStore ─────────────────────────────────────────────

#[derive(Default)]
pub struct InMemoryRateLimitStore {
    limits: Arc<RwLock<HashMap<String, crate::rate_limit::RateLimitRecord>>>,
}

impl InMemoryRateLimitStore {
    pub fn new() -> Self {
        Self { limits: Arc::new(RwLock::new(HashMap::new())) }
    }
}

#[async_trait::async_trait]
impl RateLimitStoreApi for InMemoryRateLimitStore {
    async fn get_user_rate_limit(
        &self,
        user_id: &str,
    ) -> Result<Option<crate::rate_limit::RateLimitRecord>, sqlx::Error> {
        Ok(self.limits.read().await.get(user_id).cloned())
    }

    async fn upsert_user_rate_limit(
        &self,
        user_id: &str,
        messages_per_second: f64,
        burst_count: i32,
    ) -> Result<(), sqlx::Error> {
        self.limits.write().await.insert(
            user_id.to_string(),
            crate::rate_limit::RateLimitRecord {
                messages_per_second: Some(messages_per_second),
                burst_count: Some(burst_count),
            },
        );
        Ok(())
    }

    async fn delete_user_rate_limit(&self, user_id: &str) -> Result<(), sqlx::Error> {
        self.limits.write().await.remove(user_id);
        Ok(())
    }
}

// ── InMemoryRoomTagStore ──────────────────────────────────────────────

pub struct InMemoryRoomTagStore {
    tags: Arc<RwLock<Vec<crate::room_tag::RoomTag>>>,
    next_id: Arc<RwLock<i32>>,
}

impl Default for InMemoryRoomTagStore {
    fn default() -> Self {
        Self { tags: Arc::new(RwLock::new(Vec::new())), next_id: Arc::new(RwLock::new(1)) }
    }
}

impl InMemoryRoomTagStore {
    pub fn new() -> Self {
        Self { tags: Arc::new(RwLock::new(Vec::new())), next_id: Arc::new(RwLock::new(1)) }
    }
}

#[async_trait::async_trait]
impl RoomTagStoreApi for InMemoryRoomTagStore {
    async fn get_all_tags(&self, user_id: &str) -> Result<Vec<crate::room_tag::RoomTag>, sqlx::Error> {
        Ok(self.tags.read().await.iter().filter(|t| t.user_id == user_id).cloned().collect())
    }

    async fn get_tags(&self, user_id: &str, room_id: &str) -> Result<Vec<crate::room_tag::RoomTag>, sqlx::Error> {
        Ok(self.tags.read().await.iter().filter(|t| t.user_id == user_id && t.room_id == room_id).cloned().collect())
    }

    async fn add_tag(&self, user_id: &str, room_id: &str, tag: &str, order: Option<f64>) -> Result<(), sqlx::Error> {
        let mut tags = self.tags.write().await;
        // Remove existing tag with same key before inserting.
        tags.retain(|t| !(t.user_id == user_id && t.room_id == room_id && t.tag == tag));
        let mut next_id = self.next_id.write().await;
        let id = *next_id;
        *next_id += 1;
        tags.push(crate::room_tag::RoomTag {
            id,
            user_id: user_id.to_string(),
            room_id: room_id.to_string(),
            tag: tag.to_string(),
            order,
            created_ts: chrono::Utc::now().timestamp_millis(),
        });
        Ok(())
    }

    async fn remove_tag(&self, user_id: &str, room_id: &str, tag: &str) -> Result<(), sqlx::Error> {
        self.tags.write().await.retain(|t| !(t.user_id == user_id && t.room_id == room_id && t.tag == tag));
        Ok(())
    }
}

use crate::device::{Device, DeviceListStoreApi};

/// In-memory lazy-loaded members: `(user_id, device_id, room_id) → member set`.
type LazyLoadedMembersMap = HashMap<(String, String, String), std::collections::HashSet<String>>;

/// In-memory device list store mirroring [`crate::device::DeviceStorage`].
///
/// Stores devices in a `HashMap<device_id, Device>` and tracks stream
/// position with a monotonically increasing counter.
#[derive(Clone, Default)]
pub struct InMemoryDeviceListStore {
    devices: Arc<tokio::sync::RwLock<HashMap<String, Device>>>,
    stream_id: Arc<tokio::sync::RwLock<i64>>,
    /// (user_id, device_id, room_id) → set of member user_ids.
    lazy_loaded_members: Arc<tokio::sync::RwLock<LazyLoadedMembersMap>>,
}

impl InMemoryDeviceListStore {
    pub fn new() -> Self {
        Self {
            devices: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            stream_id: Arc::new(tokio::sync::RwLock::new(0)),
            lazy_loaded_members: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait::async_trait]
impl DeviceListStoreApi for InMemoryDeviceListStore {
    async fn create_device(
        &self,
        device_id: &str,
        user_id: &str,
        display_name: Option<&str>,
    ) -> Result<Device, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let device = Device {
            device_id: device_id.to_string(),
            user_id: user_id.to_string(),
            display_name: display_name.map(|s| s.to_string()),
            device_key: None,
            last_seen_ts: Some(now),
            last_seen_ip: None,
            created_ts: now,
            first_seen_ts: now,
            user_agent: None,
            appservice_id: None,
            ignored_user_list: None,
        };
        self.devices.write().await.insert(device_id.to_string(), device.clone());
        let mut sid = self.stream_id.write().await;
        *sid += 1;
        Ok(device)
    }

    async fn delete_device(&self, device_id: &str) -> Result<(), sqlx::Error> {
        let removed = self.devices.write().await.remove(device_id);
        if removed.is_some() {
            let mut sid = self.stream_id.write().await;
            *sid += 1;
        }
        Ok(())
    }

    async fn get_user_devices(&self, user_id: &str) -> Result<Vec<Device>, sqlx::Error> {
        let devices: Vec<Device> =
            self.devices.read().await.values().filter(|d| d.user_id == user_id).cloned().collect();
        Ok(devices)
    }

    async fn get_device(&self, device_id: &str) -> Result<Option<Device>, sqlx::Error> {
        Ok(self.devices.read().await.get(device_id).cloned())
    }

    async fn update_user_device_display_name(
        &self,
        user_id: &str,
        device_id: &str,
        display_name: &str,
    ) -> Result<u64, sqlx::Error> {
        let mut devices = self.devices.write().await;
        if let Some(device) = devices.get_mut(device_id) {
            if device.user_id == user_id {
                device.display_name = Some(display_name.to_string());
                return Ok(1);
            }
        }
        Ok(0)
    }

    async fn get_max_device_list_stream_id(&self) -> Result<i64, sqlx::Error> {
        Ok(*self.stream_id.read().await)
    }

    async fn get_device_list_changed_users(
        &self,
        _from: i64,
        _to: i64,
        _requester_id: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        // Simplified: return all users that have devices
        let user_ids: Vec<String> = self.devices.read().await.values().map(|d| d.user_id.clone()).collect();
        Ok(user_ids)
    }

    async fn get_device_list_left_users(
        &self,
        _from: i64,
        _to: i64,
        _requester_id: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        Ok(Vec::new())
    }

    async fn get_users_devices_batch(
        &self,
        users: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<Device>>, sqlx::Error> {
        let devices = self.devices.read().await;
        let mut result: std::collections::HashMap<String, Vec<Device>> =
            users.iter().map(|id| (id.clone(), Vec::new())).collect();
        for device in devices.values() {
            if let Some(user_devices) = result.get_mut(&device.user_id) {
                user_devices.push(device.clone());
            }
        }
        Ok(result)
    }

    async fn get_device_list_changes(
        &self,
        _since: i64,
        _to: i64,
        _users: &[String],
    ) -> Result<Vec<(String, Option<String>, String, i64)>, sqlx::Error> {
        Ok(Vec::new())
    }

    async fn get_devices_by_user_device_pairs(
        &self,
        user_ids: &[&str],
        device_ids: &[&str],
    ) -> Result<Vec<(String, String, Option<String>, Option<i64>)>, sqlx::Error> {
        let devices = self.devices.read().await;
        let mut result = Vec::new();
        for (&user_id, &device_id) in user_ids.iter().zip(device_ids.iter()) {
            if let Some(device) = devices.get(device_id) {
                if device.user_id == user_id {
                    result.push((
                        user_id.to_string(),
                        device_id.to_string(),
                        device.display_name.clone(),
                        device.last_seen_ts,
                    ));
                }
            }
        }
        Ok(result)
    }

    async fn filter_existing_users(&self, users: &[String]) -> Result<Vec<String>, sqlx::Error> {
        let devices = self.devices.read().await;
        let device_user_ids: HashSet<String> = devices.values().map(|d| d.user_id.clone()).collect();
        Ok(users.iter().filter(|u| device_user_ids.contains(u.as_str())).cloned().collect())
    }

    // ── incremental device-list polling ──────────────────────────────────

    async fn has_device_list_updates_since(&self, since_stream_id: i64) -> Result<bool, sqlx::Error> {
        Ok(*self.stream_id.read().await > since_stream_id)
    }

    async fn get_device_lists_since_with_shared_rooms(
        &self,
        _since_stream_id: i64,
        exclude_user_id: &str,
    ) -> Result<(Vec<String>, Vec<String>), sqlx::Error> {
        // Simplified: changed = all users with devices except exclude_user_id;
        // left = empty (no departures tracked in-memory).
        let changed: Vec<String> = self
            .devices
            .read()
            .await
            .values()
            .map(|d| d.user_id.clone())
            .filter(|u| u != exclude_user_id)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        Ok((changed, Vec::new()))
    }

    // ── lazy-loaded members ──────────────────────────────────────────────

    async fn get_lazy_loaded_members(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
    ) -> Result<HashSet<String>, sqlx::Error> {
        let key = (user_id.to_string(), device_id.to_string(), room_id.to_string());
        Ok(self.lazy_loaded_members.read().await.get(&key).cloned().unwrap_or_default())
    }

    async fn upsert_lazy_loaded_members(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        member_user_ids: &HashSet<String>,
    ) -> Result<u64, sqlx::Error> {
        let key = (user_id.to_string(), device_id.to_string(), room_id.to_string());
        let mut store = self.lazy_loaded_members.write().await;
        let entry = store.entry(key).or_default();
        let before = entry.len() as u64;
        for member in member_user_ids {
            entry.insert(member.clone());
        }
        Ok((entry.len() as u64).saturating_sub(before))
    }

    async fn insert_device_list_change(
        &self,
        _user_id: &str,
        _device_id: Option<&str>,
        _change_type: &str,
        _stream_id: i64,
    ) -> Result<(), sqlx::Error> {
        Ok(())
    }

    async fn get_max_device_list_stream_id_for_user(&self, _user_id: &str) -> Result<i64, sqlx::Error> {
        Ok(0)
    }
}

use crate::relations::{
    AggregationResult, CreateRelationParams, EventRelation, RelationQueryParams, RelationsStoreApi,
};

/// In-memory relations store for testing [`RelationsService`].
///
/// Stores relations in a `Vec<EventRelation>` behind a `RwLock` with
/// auto-incrementing IDs.
#[derive(Clone, Default)]
pub struct InMemoryRelationsStore {
    relations: Arc<tokio::sync::RwLock<Vec<EventRelation>>>,
    next_id: Arc<tokio::sync::RwLock<i64>>,
}

impl InMemoryRelationsStore {
    pub fn new() -> Self {
        Self {
            relations: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            next_id: Arc::new(tokio::sync::RwLock::new(1)),
        }
    }
}

#[async_trait::async_trait]
impl RelationsStoreApi for InMemoryRelationsStore {
    async fn create_relation(&self, params: CreateRelationParams) -> Result<EventRelation, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut next = self.next_id.write().await;
        let id = *next;
        *next += 1;

        // Upsert: replace existing (event_id, relation_type, sender) match
        let mut relations = self.relations.write().await;
        if let Some(existing) = relations.iter_mut().find(|r| {
            r.event_id == params.event_id && r.relation_type == params.relation_type && r.sender == params.sender
        }) {
            existing.content = params.content;
            existing.origin_server_ts = params.origin_server_ts;
            existing.is_redacted = false;
            return Ok(existing.clone());
        }

        let relation = EventRelation {
            id,
            room_id: params.room_id,
            event_id: params.event_id,
            relates_to_event_id: params.relates_to_event_id,
            relation_type: params.relation_type,
            sender: params.sender,
            origin_server_ts: params.origin_server_ts,
            content: params.content,
            is_redacted: false,
            created_ts: now,
        };
        relations.push(relation.clone());
        Ok(relation)
    }

    async fn get_relation(&self, room_id: &str, event_id: &str) -> Result<Option<EventRelation>, sqlx::Error> {
        Ok(self
            .relations
            .read()
            .await
            .iter()
            .find(|r| r.room_id == room_id && r.event_id == event_id && !r.is_redacted)
            .cloned())
    }

    async fn get_relations(&self, params: RelationQueryParams) -> Result<Vec<EventRelation>, sqlx::Error> {
        let limit = params.limit.unwrap_or(50).clamp(1, 100) as usize;
        let rels = self.relations.read().await;
        let mut filtered: Vec<&EventRelation> = rels
            .iter()
            .filter(|r| {
                r.room_id == params.room_id
                    && r.relates_to_event_id == params.relates_to_event_id
                    && params.relation_type.as_ref().is_none_or(|t| r.relation_type == *t)
                    && !r.is_redacted
            })
            .collect();

        let direction = params.direction.as_deref().unwrap_or("f");
        match direction {
            "b" => {
                filtered.sort_by(|a, b| {
                    b.origin_server_ts.cmp(&a.origin_server_ts).then_with(|| b.event_id.cmp(&a.event_id))
                });
                if let Some(ref from) = params.from {
                    if let Some(pos) = filtered.iter().position(|r| r.event_id == *from) {
                        filtered = filtered.into_iter().skip(pos + 1).collect();
                    }
                }
            }
            _ => {
                filtered.sort_by(|a, b| {
                    a.origin_server_ts.cmp(&b.origin_server_ts).then_with(|| a.event_id.cmp(&b.event_id))
                });
                if let Some(ref from) = params.from {
                    if let Some(pos) = filtered.iter().position(|r| r.event_id == *from) {
                        filtered = filtered.into_iter().skip(pos + 1).collect();
                    }
                }
            }
        }

        Ok(filtered.into_iter().take(limit).cloned().collect())
    }

    async fn count_relations(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
        relation_type: Option<&str>,
    ) -> Result<i64, sqlx::Error> {
        let count = self
            .relations
            .read()
            .await
            .iter()
            .filter(|r| {
                r.room_id == room_id
                    && r.relates_to_event_id == relates_to_event_id
                    && relation_type.is_none_or(|t| r.relation_type == t)
                    && !r.is_redacted
            })
            .count();
        Ok(count as i64)
    }

    async fn get_replacement(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
        sender: &str,
    ) -> Result<Option<EventRelation>, sqlx::Error> {
        Ok(self
            .relations
            .read()
            .await
            .iter()
            .filter(|r| {
                r.room_id == room_id
                    && r.relates_to_event_id == relates_to_event_id
                    && r.relation_type == "m.replace"
                    && r.sender == sender
                    && !r.is_redacted
            })
            .max_by_key(|r| r.origin_server_ts)
            .cloned())
    }

    async fn aggregate_annotations(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
    ) -> Result<Vec<AggregationResult>, sqlx::Error> {
        use std::collections::HashMap;
        let rels = self.relations.read().await;
        let mut map: HashMap<String, (i64, Option<String>)> = HashMap::new();
        for r in rels.iter() {
            if r.room_id == room_id
                && r.relates_to_event_id == relates_to_event_id
                && r.relation_type == "m.annotation"
                && !r.is_redacted
            {
                let key = r.content.get("body").and_then(|v| v.as_str()).map(|s| s.to_string());
                let entry = map.entry(key.clone().unwrap_or_default()).or_insert((0, None));
                entry.0 += 1;
                entry.1 = key.clone();
            }
        }
        let mut results: Vec<AggregationResult> = map
            .into_iter()
            .map(|(_, (count, key))| AggregationResult {
                relation_type: "m.annotation".to_string(),
                key,
                count,
                sender: None,
            })
            .collect();
        results.sort_by(|a, b| b.count.cmp(&a.count));
        Ok(results)
    }

    async fn redact_relation(&self, room_id: &str, event_id: &str) -> Result<(), sqlx::Error> {
        if let Some(r) =
            self.relations.write().await.iter_mut().find(|r| r.room_id == room_id && r.event_id == event_id)
        {
            r.is_redacted = true;
            r.content = serde_json::json!({});
        }
        Ok(())
    }

    async fn relation_exists(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
        relation_type: &str,
        sender: &str,
    ) -> Result<bool, sqlx::Error> {
        Ok(self.relations.read().await.iter().any(|r| {
            r.room_id == room_id
                && r.relates_to_event_id == relates_to_event_id
                && r.relation_type == relation_type
                && r.sender == sender
                && !r.is_redacted
        }))
    }
}

// ── InMemoryBackgroundUpdateStore ───────────────────────────────────────

pub struct InMemoryBackgroundUpdateStore {
    updates: tokio::sync::RwLock<HashMap<String, BackgroundUpdate>>,
    locks: tokio::sync::RwLock<HashMap<String, bool>>,
    history: tokio::sync::RwLock<Vec<BackgroundUpdateHistory>>,
    next_history_id: tokio::sync::RwLock<i64>,
}

impl Default for InMemoryBackgroundUpdateStore {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryBackgroundUpdateStore {
    pub fn new() -> Self {
        Self {
            updates: tokio::sync::RwLock::new(HashMap::new()),
            locks: tokio::sync::RwLock::new(HashMap::new()),
            history: tokio::sync::RwLock::new(Vec::new()),
            next_history_id: tokio::sync::RwLock::new(1),
        }
    }
}

#[async_trait::async_trait]
impl BackgroundUpdateStoreApi for InMemoryBackgroundUpdateStore {
    async fn create_update(&self, request: CreateBackgroundUpdateRequest) -> Result<BackgroundUpdate, sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        let update = BackgroundUpdate {
            job_name: request.job_name.clone(),
            job_type: request.job_type,
            description: request.description,
            table_name: request.table_name,
            column_name: request.column_name,
            status: "pending".to_string(),
            progress: serde_json::json!(0),
            total_items: request.total_items.unwrap_or(0),
            processed_items: 0,
            created_ts: now,
            started_ts: None,
            completed_ts: None,
            updated_ts: None,
            error_message: None,
            retry_count: 0,
            max_retries: 3,
            batch_size: request.batch_size.unwrap_or(100),
            sleep_ms: request.sleep_ms.unwrap_or(1000),
            depends_on: request
                .depends_on
                .map(|deps| serde_json::Value::Array(deps.into_iter().map(serde_json::Value::String).collect())),
            metadata: request.metadata,
        };
        self.updates.write().await.insert(update.job_name.clone(), update.clone());
        Ok(update)
    }

    async fn get_update(&self, job_name: &str) -> Result<Option<BackgroundUpdate>, sqlx::Error> {
        Ok(self.updates.read().await.get(job_name).cloned())
    }

    async fn get_all_updates(
        &self,
        limit: i64,
        from: Option<String>,
    ) -> Result<(Vec<BackgroundUpdate>, Option<String>), sqlx::Error> {
        let updates = self.updates.read().await;
        let mut sorted: Vec<BackgroundUpdate> = updates.values().cloned().collect();
        sorted.sort_by(|a, b| b.created_ts.cmp(&a.created_ts).then_with(|| b.job_name.cmp(&a.job_name)));
        let from_idx = from
            .as_deref()
            .and_then(|cursor| {
                let (ts, name) = cursor.split_once('|')?;
                let ts = ts.parse::<i64>().ok()?;
                sorted.iter().position(|u| u.created_ts == ts && u.job_name == name)
            })
            .map(|p| p + 1)
            .unwrap_or(0);
        let page: Vec<BackgroundUpdate> = sorted.into_iter().skip(from_idx).take(limit as usize).collect();
        let next = if page.len() as i64 == limit {
            page.last().map(|u| format!("{}|{}", u.created_ts, u.job_name))
        } else {
            None
        };
        Ok((page, next))
    }

    async fn get_pending_updates(&self) -> Result<Vec<BackgroundUpdate>, sqlx::Error> {
        Ok(self.updates.read().await.values().filter(|u| u.status == "pending").cloned().collect())
    }

    async fn get_running_updates(&self) -> Result<Vec<BackgroundUpdate>, sqlx::Error> {
        Ok(self.updates.read().await.values().filter(|u| u.status == "running").cloned().collect())
    }

    async fn update_status(&self, job_name: &str, status: &str) -> Result<BackgroundUpdate, sqlx::Error> {
        let mut updates = self.updates.write().await;
        let update = updates.get_mut(job_name).ok_or_else(|| sqlx::Error::RowNotFound)?;
        let now = Utc::now().timestamp_millis();
        update.status = status.to_string();
        update.updated_ts = Some(now);
        if status == "running" {
            update.started_ts = Some(now);
        }
        if status == "completed" {
            update.completed_ts = Some(now);
        }
        Ok(update.clone())
    }

    async fn update_progress(
        &self,
        job_name: &str,
        items_processed: i32,
        total_items: Option<i32>,
    ) -> Result<BackgroundUpdate, sqlx::Error> {
        let mut updates = self.updates.write().await;
        let update = updates.get_mut(job_name).ok_or_else(|| sqlx::Error::RowNotFound)?;
        update.processed_items += items_processed;
        if let Some(t) = total_items {
            update.total_items = t;
        }
        if update.total_items > 0 {
            update.progress =
                serde_json::json!(((update.processed_items as f64 / update.total_items as f64) * 100.0).round() as i64);
        }
        update.updated_ts = Some(Utc::now().timestamp_millis());
        Ok(update.clone())
    }

    async fn set_error(&self, job_name: &str, error_message: &str) -> Result<BackgroundUpdate, sqlx::Error> {
        let mut updates = self.updates.write().await;
        let update = updates.get_mut(job_name).ok_or_else(|| sqlx::Error::RowNotFound)?;
        update.status = "failed".to_string();
        update.error_message = Some(error_message.to_string());
        update.updated_ts = Some(Utc::now().timestamp_millis());
        update.retry_count += 1;
        Ok(update.clone())
    }

    async fn delete_update(&self, job_name: &str) -> Result<(), sqlx::Error> {
        self.updates.write().await.remove(job_name);
        Ok(())
    }

    async fn acquire_lock_with_retry(
        &self,
        job_name: &str,
        _locked_by: &str,
        _lock_duration_ms: i64,
        _max_retries: u32,
        _max_retry_interval_ms: u64,
    ) -> Result<bool, sqlx::Error> {
        let mut locks = self.locks.write().await;
        if locks.get(job_name).copied().unwrap_or(false) {
            Ok(false)
        } else {
            locks.insert(job_name.to_string(), true);
            Ok(true)
        }
    }

    async fn release_lock(&self, job_name: &str) -> Result<(), sqlx::Error> {
        self.locks.write().await.remove(job_name);
        Ok(())
    }

    async fn is_locked(&self, job_name: &str) -> Result<bool, sqlx::Error> {
        Ok(self.locks.read().await.get(job_name).copied().unwrap_or(false))
    }

    async fn cleanup_expired_locks(&self) -> Result<i64, sqlx::Error> {
        let count = self.locks.read().await.len() as i64;
        self.locks.write().await.clear();
        Ok(count)
    }

    async fn add_history(
        &self,
        job_name: &str,
        status: &str,
        items_processed: i32,
        error_message: Option<&str>,
        metadata: Option<serde_json::Value>,
    ) -> Result<BackgroundUpdateHistory, sqlx::Error> {
        let mut next_id = self.next_history_id.write().await;
        let id = *next_id;
        *next_id += 1;
        let now = Utc::now().timestamp_millis();
        let entry = BackgroundUpdateHistory {
            id,
            job_name: job_name.to_string(),
            execution_start_ts: now,
            execution_end_ts: Some(now),
            status: status.to_string(),
            items_processed,
            error_message: error_message.map(|s| s.to_string()),
            metadata,
        };
        self.history.write().await.push(entry.clone());
        Ok(entry)
    }

    async fn get_history(&self, job_name: &str, limit: i64) -> Result<Vec<BackgroundUpdateHistory>, sqlx::Error> {
        let mut entries: Vec<BackgroundUpdateHistory> =
            self.history.read().await.iter().filter(|h| h.job_name == job_name).cloned().collect();
        entries.sort_by(|a, b| b.execution_start_ts.cmp(&a.execution_start_ts));
        entries.truncate(limit as usize);
        Ok(entries)
    }

    async fn retry_failed(&self) -> Result<i64, sqlx::Error> {
        let mut updates = self.updates.write().await;
        let mut count = 0i64;
        for update in updates.values_mut() {
            if update.status == "failed" && update.retry_count < update.max_retries {
                update.status = "pending".to_string();
                update.error_message = None;
                update.retry_count += 1;
                count += 1;
            }
        }
        Ok(count)
    }

    async fn count_by_status(&self, status: &str) -> Result<i64, sqlx::Error> {
        Ok(self.updates.read().await.values().filter(|u| u.status == status).count() as i64)
    }

    async fn count_all(&self) -> Result<i64, sqlx::Error> {
        Ok(self.updates.read().await.len() as i64)
    }

    async fn get_stats(&self, _limit: i32) -> Result<Vec<BackgroundUpdateStats>, sqlx::Error> {
        Ok(Vec::new())
    }
}

// ── InMemoryCasStore ───────────────────────────────────────────────────

#[cfg(feature = "cas-sso")]
#[derive(Clone, Default)]
#[allow(clippy::type_complexity)]
pub struct InMemoryCasStore {
    tickets: Arc<tokio::sync::RwLock<HashMap<String, CasTicket>>>,
    proxy_tickets: Arc<tokio::sync::RwLock<HashMap<String, CasProxyTicket>>>,
    pgts: Arc<tokio::sync::RwLock<HashMap<String, CasProxyGrantingTicket>>>,
    services: Arc<tokio::sync::RwLock<HashMap<String, CasRegisteredService>>>,
    user_attributes: Arc<tokio::sync::RwLock<HashMap<String, HashMap<String, String>>>>,
    slo_sessions: Arc<tokio::sync::RwLock<Vec<CasSloSession>>>,
    next_id: Arc<tokio::sync::RwLock<i64>>,
}

#[cfg(feature = "cas-sso")]
impl InMemoryCasStore {
    pub fn new() -> Self {
        Self {
            tickets: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            proxy_tickets: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            pgts: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            services: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            user_attributes: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            slo_sessions: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            next_id: Arc::new(tokio::sync::RwLock::new(1)),
        }
    }

    async fn next_id(&self) -> i64 {
        let mut id = self.next_id.write().await;
        let current = *id;
        *id += 1;
        current
    }
}

#[cfg(feature = "cas-sso")]
#[async_trait::async_trait]
impl CasStoreApi for InMemoryCasStore {
    async fn create_ticket(&self, request: CreateTicketRequest) -> Result<CasTicket, ApiError> {
        let now = Utc::now().timestamp_millis();
        let ticket = CasTicket {
            id: self.next_id().await,
            ticket_id: request.ticket_id.clone(),
            user_id: request.user_id.clone(),
            service_url: request.service_url.clone(),
            created_ts: now,
            expires_at: now + (request.expires_in_seconds) * 1000,
            consumed_ts: None,
            consumed_by: None,
            is_valid: true,
        };
        self.tickets.write().await.insert(ticket.ticket_id.clone(), ticket.clone());
        Ok(ticket)
    }

    async fn validate_ticket(&self, ticket_id: &str, service_url: &str) -> Result<Option<CasTicket>, ApiError> {
        let mut tickets = self.tickets.write().await;
        if let Some(ticket) = tickets.get_mut(ticket_id) {
            if ticket.is_valid {
                let now = Utc::now().timestamp_millis();
                ticket.consumed_ts = Some(now);
                ticket.consumed_by = Some(service_url.to_string());
                return Ok(Some(ticket.clone()));
            }
        }
        Ok(None)
    }

    async fn get_ticket(&self, ticket_id: &str) -> Result<Option<CasTicket>, ApiError> {
        Ok(self.tickets.read().await.get(ticket_id).cloned())
    }

    async fn get_user_attributes(&self, user_id: &str) -> Result<Vec<CasUserAttribute>, ApiError> {
        let attrs = self.user_attributes.read().await;
        let user_attrs = attrs.get(user_id);
        Ok(user_attrs.map_or(Vec::new(), |map| {
            map.iter()
                .map(|(name, value)| CasUserAttribute {
                    id: 0,
                    user_id: user_id.to_string(),
                    attribute_name: name.clone(),
                    attribute_value: value.clone(),
                    created_ts: 0,
                    updated_ts: 0,
                })
                .collect()
        }))
    }

    async fn create_pgt(&self, request: CreatePgtRequest) -> Result<CasProxyGrantingTicket, ApiError> {
        let now = Utc::now().timestamp_millis();
        let pgt = CasProxyGrantingTicket {
            id: self.next_id().await,
            pgt_id: request.pgt_id.clone(),
            user_id: request.user_id.clone(),
            service_url: request.service_url.clone(),
            iou: request.iou.clone(),
            created_ts: now,
            expires_at: now + (request.expires_in_seconds) * 1000,
            is_valid: true,
        };
        self.pgts.write().await.insert(pgt.pgt_id.clone(), pgt.clone());
        Ok(pgt)
    }

    async fn get_pgt(&self, pgt_id: &str) -> Result<Option<CasProxyGrantingTicket>, ApiError> {
        Ok(self.pgts.read().await.get(pgt_id).cloned())
    }

    async fn create_proxy_ticket(&self, request: CreateProxyTicketRequest) -> Result<CasProxyTicket, ApiError> {
        let now = Utc::now().timestamp_millis();
        let ticket = CasProxyTicket {
            id: self.next_id().await,
            proxy_ticket_id: request.proxy_ticket_id.clone(),
            user_id: request.user_id.clone(),
            service_url: request.service_url.clone(),
            pgt_url: request.pgt_url.clone(),
            created_ts: now,
            expires_at: now + (request.expires_in_seconds) * 1000,
            consumed_ts: None,
            is_valid: true,
        };
        self.proxy_tickets.write().await.insert(ticket.proxy_ticket_id.clone(), ticket.clone());
        Ok(ticket)
    }

    async fn validate_proxy_ticket(
        &self,
        proxy_ticket_id: &str,
        _service_url: &str,
    ) -> Result<Option<CasProxyTicket>, ApiError> {
        let mut tickets = self.proxy_tickets.write().await;
        if let Some(ticket) = tickets.get_mut(proxy_ticket_id) {
            if ticket.is_valid {
                let now = Utc::now().timestamp_millis();
                ticket.consumed_ts = Some(now);
                return Ok(Some(ticket.clone()));
            }
        }
        Ok(None)
    }

    async fn register_service(&self, request: RegisterServiceRequest) -> Result<CasRegisteredService, ApiError> {
        let now = Utc::now().timestamp_millis();
        let service = CasRegisteredService {
            id: self.next_id().await,
            service_id: request.service_id.clone(),
            name: request.name.clone(),
            description: request.description.clone(),
            service_url_pattern: request.service_url_pattern.clone(),
            allowed_attributes: serde_json::Value::Null,
            allowed_proxy_callbacks: serde_json::Value::Null,
            is_enabled: true,
            is_require_secure: request.is_require_secure.unwrap_or(false),
            is_single_logout: request.is_single_logout.unwrap_or(false),
            created_ts: now,
            updated_ts: now,
        };
        self.services.write().await.insert(service.service_id.clone(), service.clone());
        Ok(service)
    }

    async fn get_service(&self, service_id: &str) -> Result<Option<CasRegisteredService>, ApiError> {
        Ok(self.services.read().await.get(service_id).cloned())
    }

    async fn get_service_by_url(&self, service_url: &str) -> Result<Option<CasRegisteredService>, ApiError> {
        Ok(self.services.read().await.values().find(|s| s.service_url_pattern == service_url).cloned())
    }

    async fn list_services(&self) -> Result<Vec<CasRegisteredService>, ApiError> {
        Ok(self.services.read().await.values().cloned().collect())
    }

    async fn delete_service(&self, service_id: &str) -> Result<bool, ApiError> {
        Ok(self.services.write().await.remove(service_id).is_some())
    }

    async fn set_user_attribute(
        &self,
        user_id: &str,
        attribute_name: &str,
        attribute_value: &str,
    ) -> Result<CasUserAttribute, ApiError> {
        let mut attrs = self.user_attributes.write().await;
        let user_attrs = attrs.entry(user_id.to_string()).or_default();
        user_attrs.insert(attribute_name.to_string(), attribute_value.to_string());
        Ok(CasUserAttribute {
            id: 0,
            user_id: user_id.to_string(),
            attribute_name: attribute_name.to_string(),
            attribute_value: attribute_value.to_string(),
            created_ts: 0,
            updated_ts: 0,
        })
    }

    async fn get_active_slo_sessions(&self, user_id: &str) -> Result<Vec<CasSloSession>, ApiError> {
        Ok(self.slo_sessions.read().await.iter().filter(|s| s.user_id == user_id).cloned().collect())
    }

    async fn cleanup_expired_tickets(&self) -> Result<u64, ApiError> {
        let now = Utc::now().timestamp_millis();
        let mut count = 0u64;
        let mut tickets = self.tickets.write().await;
        tickets.retain(|_, t| {
            if t.expires_at < now {
                count += 1;
                false
            } else {
                true
            }
        });
        Ok(count)
    }
}

// ── InMemoryAuditEventStore ──────────────────────────────────────────

#[derive(Clone, Default)]
pub struct InMemoryAuditEventStore {
    events: Arc<tokio::sync::RwLock<HashMap<String, AuditEvent>>>,
}

impl InMemoryAuditEventStore {
    pub fn new() -> Self {
        Self { events: Arc::new(tokio::sync::RwLock::new(HashMap::new())) }
    }
}

#[async_trait::async_trait]
impl AuditEventStoreApi for InMemoryAuditEventStore {
    async fn create_event(
        &self,
        event_id: &str,
        created_ts: i64,
        request: &CreateAuditEventRequest,
    ) -> Result<AuditEvent, sqlx::Error> {
        let event = AuditEvent {
            event_id: event_id.to_string(),
            actor_id: request.actor_id.clone(),
            action: request.action.clone(),
            resource_type: request.resource_type.clone(),
            resource_id: request.resource_id.clone(),
            result: request.result.clone(),
            request_id: request.request_id.clone(),
            details: request.details.clone().unwrap_or(serde_json::json!({})),
            created_ts,
        };
        self.events.write().await.insert(event_id.to_string(), event.clone());
        Ok(event)
    }

    async fn get_event(&self, event_id: &str) -> Result<Option<AuditEvent>, sqlx::Error> {
        Ok(self.events.read().await.get(event_id).cloned())
    }

    async fn list_events(
        &self,
        filters: &AuditEventFilters,
    ) -> Result<(Vec<AuditEvent>, i64, Option<String>), sqlx::Error> {
        let events = self.events.read().await;
        let mut results: Vec<AuditEvent> = events.values().cloned().collect();

        if let Some(ref actor_id) = filters.actor_id {
            results.retain(|e| e.actor_id == *actor_id);
        }
        if let Some(ref action) = filters.action {
            results.retain(|e| e.action == *action);
        }
        if let Some(ref resource_type) = filters.resource_type {
            results.retain(|e| e.resource_type == *resource_type);
        }
        if let Some(ref resource_id) = filters.resource_id {
            results.retain(|e| e.resource_id == *resource_id);
        }
        if let Some(ref result) = filters.result {
            results.retain(|e| e.result == *result);
        }

        results.sort_by(|a, b| b.created_ts.cmp(&a.created_ts).then_with(|| b.event_id.cmp(&a.event_id)));

        if let Some(ref cursor) = filters.from {
            results.retain(|e| (e.created_ts, e.event_id.as_str()) < (cursor.created_ts, cursor.event_id.as_str()));
        }

        let total = results.len() as i64;
        let next_batch = if results.len() > filters.limit as usize {
            results.get(filters.limit as usize).map(|event| {
                encode_audit_event_cursor(&AuditEventCursor {
                    created_ts: event.created_ts,
                    event_id: event.event_id.clone(),
                })
            })
        } else {
            None
        };

        results.truncate(filters.limit as usize);
        Ok((results, total, next_batch))
    }

    async fn delete_events_before(&self, cutoff_ts: i64) -> Result<u64, sqlx::Error> {
        let mut events = self.events.write().await;
        let before = events.len() as u64;
        events.retain(|_, e| e.created_ts >= cutoff_ts);
        Ok(before - events.len() as u64)
    }
}

// ── InMemoryAdminMediaStore ─────────────────────────────────────────

#[derive(Clone, Default)]
pub struct InMemoryAdminMediaStore {
    media: Arc<tokio::sync::RwLock<HashMap<String, AdminMediaInfo>>>,
}

impl InMemoryAdminMediaStore {
    pub fn new() -> Self {
        Self { media: Arc::new(tokio::sync::RwLock::new(HashMap::new())) }
    }
}

#[async_trait::async_trait]
impl AdminMediaStoreApi for InMemoryAdminMediaStore {
    async fn get_all_media(&self, limit: i64, cursor: Option<MediaCursor>) -> Result<AdminMediaPage, ApiError> {
        let media = self.media.read().await;
        let mut results: Vec<AdminMediaInfo> = media.values().cloned().collect();
        results.sort_by(|a, b| b.created_ts.cmp(&a.created_ts).then_with(|| b.media_id.cmp(&a.media_id)));

        if let Some(ref cursor) = cursor {
            results.retain(|m| (m.created_ts, m.media_id.as_str()) < (cursor.created_ts, cursor.media_id.as_str()));
        }

        let next_batch = if results.len() > limit as usize {
            results
                .get(limit as usize)
                .map(|m| encode_media_cursor(&MediaCursor { created_ts: m.created_ts, media_id: m.media_id.clone() }))
        } else {
            None
        };

        results.truncate(limit as usize);
        Ok(AdminMediaPage { media: results, next_batch })
    }

    async fn get_media_info(&self, media_id: &str) -> Result<Option<AdminMediaInfo>, ApiError> {
        Ok(self.media.read().await.get(media_id).cloned())
    }

    async fn delete_media(&self, media_id: &str) -> Result<bool, ApiError> {
        Ok(self.media.write().await.remove(media_id).is_some())
    }

    async fn get_media_quota(&self) -> Result<AdminMediaQuotaSummary, ApiError> {
        let media = self.media.read().await;
        let total_size: i64 = media.values().map(|m| m.size).sum();
        Ok(AdminMediaQuotaSummary { total_size, total_count: media.len() as i64 })
    }

    async fn get_user_media(&self, user_id: &str) -> Result<Vec<AdminMediaInfo>, ApiError> {
        let media = self.media.read().await;
        let mut results: Vec<AdminMediaInfo> =
            media.values().filter(|m| m.uploader_user_id.as_deref() == Some(user_id)).cloned().collect();
        results.sort_by(|a, b| b.created_ts.cmp(&a.created_ts));
        Ok(results)
    }

    async fn delete_user_media(&self, user_id: &str) -> Result<u64, ApiError> {
        let mut media = self.media.write().await;
        let before = media.len();
        media.retain(|_, m| m.uploader_user_id.as_deref() != Some(user_id));
        Ok((before - media.len()) as u64)
    }
}

impl InMemoryAdminMediaStore {
    /// Convenience seed method for tests — not part of the trait.
    pub async fn insert_media(&self, info: AdminMediaInfo) {
        self.media.write().await.insert(info.media_id.clone(), info);
    }
}

// ── InMemoryThreepidStore ───────────────────────────────────────────

#[derive(Clone, Default)]
pub struct InMemoryThreepidStore {
    threepids: Arc<tokio::sync::RwLock<Vec<UserThreepid>>>,
    next_id: Arc<tokio::sync::RwLock<i64>>,
}

impl InMemoryThreepidStore {
    pub fn new() -> Self {
        Self {
            threepids: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            next_id: Arc::new(tokio::sync::RwLock::new(1)),
        }
    }
}

#[async_trait::async_trait]
impl ThreepidStoreApi for InMemoryThreepidStore {
    async fn get_verified_threepid_by_address(
        &self,
        medium: &str,
        address: &str,
    ) -> Result<Option<UserThreepid>, ApiError> {
        Ok(self
            .threepids
            .read()
            .await
            .iter()
            .find(|t| t.medium == medium && t.address == address && t.is_verified)
            .cloned())
    }

    async fn get_threepids_by_user(&self, user_id: &str) -> Result<Vec<UserThreepid>, ApiError> {
        Ok(self.threepids.read().await.iter().filter(|t| t.user_id == user_id).cloned().collect())
    }

    async fn add_verified_threepid(
        &self,
        user_id: &str,
        medium: &str,
        address: &str,
        validated_at: i64,
        added_ts: i64,
    ) -> Result<u64, ApiError> {
        let mut threepids = self.threepids.write().await;
        let id = *self.next_id.read().await;
        *self.next_id.write().await = id + 1;
        threepids.push(UserThreepid {
            id,
            user_id: user_id.to_string(),
            medium: medium.to_string(),
            address: address.to_string(),
            validated_at: Some(validated_at),
            added_ts,
            is_verified: true,
            verification_token: None,
            verification_expires_at: None,
        });
        Ok(1)
    }

    async fn remove_threepid(&self, user_id: &str, medium: &str, address: &str) -> Result<bool, ApiError> {
        let mut threepids = self.threepids.write().await;
        let before = threepids.len();
        threepids.retain(|t| !(t.user_id == user_id && t.medium == medium && t.address == address));
        Ok(threepids.len() < before)
    }
}

impl InMemoryThreepidStore {
    /// Seed a verified threepid for tests.
    pub async fn seed_threepid(&self, user_id: &str, medium: &str, address: &str) {
        let mut threepids = self.threepids.write().await;
        let id = threepids.len() as i64 + 1;
        threepids.push(UserThreepid {
            id,
            user_id: user_id.to_string(),
            medium: medium.to_string(),
            address: address.to_string(),
            validated_at: Some(chrono::Utc::now().timestamp_millis()),
            added_ts: chrono::Utc::now().timestamp_millis(),
            is_verified: true,
            verification_token: None,
            verification_expires_at: None,
        });
    }
}

// =============================================================================
// InMemoryAccessTokenStore
// =============================================================================

use crate::refresh_token::{
    CreateRefreshTokenRequest, RecordUsageRequest, RefreshToken, RefreshTokenFamily, RefreshTokenRotation,
    RefreshTokenStats, RefreshTokenStoreApi, RefreshTokenUsage,
};
use crate::token::{AccessToken, AccessTokenStoreApi};

/// In-memory test double for [`AccessTokenStoreApi`].
#[derive(Clone, Default)]
pub struct InMemoryAccessTokenStore {
    tokens: Arc<tokio::sync::RwLock<HashMap<i64, AccessToken>>>,
}

impl InMemoryAccessTokenStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn seed_token(&self, user_id: &str, token_id: i64, device_id: Option<&str>) {
        let mut tokens = self.tokens.write().await;
        tokens.insert(
            token_id,
            AccessToken {
                id: token_id,
                token_hash: format!("hash_{token_id}"),
                user_id: user_id.to_string(),
                device_id: device_id.map(|d| d.to_string()),
                created_ts: 1_700_000_000_000,
                expires_at: None,
                last_used_ts: None,
                user_agent: None,
                ip_address: None,
                is_revoked: false,
            },
        );
    }
}

#[async_trait::async_trait]
impl AccessTokenStoreApi for InMemoryAccessTokenStore {
    async fn get_user_tokens(&self, user_id: &str) -> Result<Vec<AccessToken>, sqlx::Error> {
        let tokens = self.tokens.read().await;
        Ok(tokens.values().filter(|t| t.user_id == user_id).cloned().collect())
    }

    async fn delete_user_token_by_id(&self, user_id: &str, token_id: i64) -> Result<bool, sqlx::Error> {
        let mut tokens = self.tokens.write().await;
        if let Some(token) = tokens.get(&token_id) {
            if token.user_id == user_id {
                tokens.remove(&token_id);
                return Ok(true);
            }
        }
        Ok(false)
    }
}

// =============================================================================
// InMemoryRefreshTokenStore
// =============================================================================

/// In-memory test double for [`RefreshTokenStoreApi`].
#[derive(Clone, Default)]
pub struct InMemoryRefreshTokenStore {
    tokens: Arc<tokio::sync::RwLock<HashMap<i64, RefreshToken>>>,
    token_hash_index: Arc<tokio::sync::RwLock<HashMap<String, i64>>>,
    next_id: Arc<tokio::sync::Mutex<i64>>,
    blacklist: Arc<tokio::sync::RwLock<HashMap<String, i64>>>,
}

impl InMemoryRefreshTokenStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn seed_token(&self, user_id: &str, token_id: i64, token_hash: &str, device_id: Option<&str>) {
        let mut tokens = self.tokens.write().await;
        tokens.insert(
            token_id,
            RefreshToken {
                id: token_id,
                token_hash: token_hash.to_string(),
                user_id: user_id.to_string(),
                device_id: device_id.map(|d| d.to_string()),
                access_token_id: None,
                scope: None,
                created_ts: 1_700_000_000_000,
                expires_at: None,
                last_used_ts: None,
                use_count: 0,
                is_revoked: false,
                revoked_reason: None,
                client_info: None,
                ip_address: None,
                user_agent: None,
            },
        );
        self.token_hash_index.write().await.insert(token_hash.to_string(), token_id);
    }

    async fn next_id(&self) -> i64 {
        let mut guard = self.next_id.lock().await;
        *guard += 1;
        *guard
    }
}

#[async_trait::async_trait]
impl RefreshTokenStoreApi for InMemoryRefreshTokenStore {
    async fn get_user_tokens(&self, user_id: &str) -> Result<Vec<RefreshToken>, sqlx::Error> {
        let tokens = self.tokens.read().await;
        Ok(tokens.values().filter(|t| t.user_id == user_id).cloned().collect())
    }

    async fn get_token_by_id(&self, id: i64) -> Result<Option<RefreshToken>, sqlx::Error> {
        Ok(self.tokens.read().await.get(&id).cloned())
    }

    async fn delete_token(&self, token_hash: &str) -> Result<(), sqlx::Error> {
        if let Some(id) = self.token_hash_index.read().await.get(token_hash) {
            self.tokens.write().await.remove(id);
        }
        Ok(())
    }

    async fn create_token(&self, request: CreateRefreshTokenRequest) -> Result<RefreshToken, sqlx::Error> {
        let id = self.next_id().await;
        let now = chrono::Utc::now().timestamp_millis();
        let token = RefreshToken {
            id,
            token_hash: request.token_hash.clone(),
            user_id: request.user_id.clone(),
            device_id: request.device_id.clone(),
            access_token_id: request.access_token_id.clone(),
            scope: request.scope.clone(),
            created_ts: now,
            expires_at: Some(request.expires_at),
            last_used_ts: None,
            use_count: 0,
            is_revoked: false,
            revoked_reason: None,
            client_info: request.client_info.clone(),
            ip_address: request.ip_address.clone(),
            user_agent: request.user_agent.clone(),
        };
        self.token_hash_index.write().await.insert(token.token_hash.clone(), id);
        self.tokens.write().await.insert(id, token.clone());
        Ok(token)
    }

    async fn get_token(&self, token_hash: &str) -> Result<Option<RefreshToken>, sqlx::Error> {
        let index = self.token_hash_index.read().await;
        if let Some(id) = index.get(token_hash) {
            Ok(self.tokens.read().await.get(id).cloned())
        } else {
            Ok(None)
        }
    }

    async fn get_active_tokens(&self, user_id: &str) -> Result<Vec<RefreshToken>, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let tokens = self.tokens.read().await;
        Ok(tokens
            .values()
            .filter(|t| t.user_id == user_id && !t.is_revoked && t.expires_at.is_none_or(|exp| exp > now))
            .cloned()
            .collect())
    }

    async fn revoke_token(&self, token_hash: &str, reason: &str) -> Result<(), sqlx::Error> {
        let index = self.token_hash_index.read().await;
        if let Some(id) = index.get(token_hash) {
            if let Some(token) = self.tokens.write().await.get_mut(id) {
                token.is_revoked = true;
                token.revoked_reason = Some(reason.to_string());
            }
        }
        Ok(())
    }

    async fn revoke_token_cas(&self, token_hash: &str, reason: &str) -> Result<bool, sqlx::Error> {
        let index = self.token_hash_index.read().await;
        if let Some(id) = index.get(token_hash) {
            if let Some(token) = self.tokens.write().await.get_mut(id) {
                if token.is_revoked {
                    return Ok(false);
                }
                token.is_revoked = true;
                token.revoked_reason = Some(reason.to_string());
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn revoke_token_by_id(&self, id: i64, reason: &str) -> Result<(), sqlx::Error> {
        if let Some(token) = self.tokens.write().await.get_mut(&id) {
            token.is_revoked = true;
            token.revoked_reason = Some(reason.to_string());
        }
        Ok(())
    }

    async fn revoke_all_user_tokens(&self, user_id: &str, reason: &str) -> Result<i64, sqlx::Error> {
        let mut tokens = self.tokens.write().await;
        let mut count = 0i64;
        for token in tokens.values_mut() {
            if token.user_id == user_id && !token.is_revoked {
                token.is_revoked = true;
                token.revoked_reason = Some(reason.to_string());
                count += 1;
            }
        }
        Ok(count)
    }

    async fn record_usage(&self, _request: &RecordUsageRequest) -> Result<(), sqlx::Error> {
        Ok(())
    }

    async fn create_family(
        &self,
        family_id: &str,
        user_id: &str,
        device_id: Option<&str>,
    ) -> Result<RefreshTokenFamily, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        Ok(RefreshTokenFamily {
            id: self.next_id().await,
            family_id: family_id.to_string(),
            user_id: user_id.to_string(),
            device_id: device_id.map(|d| d.to_string()),
            created_ts: now,
            last_refresh_ts: None,
            refresh_count: 0,
            is_compromised: false,
            compromised_ts: None,
        })
    }

    async fn mark_family_compromised(&self, _family_id: &str) -> Result<(), sqlx::Error> {
        Ok(())
    }

    async fn record_rotation(
        &self,
        _family_id: &str,
        _old_token_hash: Option<&str>,
        _new_token_hash: &str,
        _reason: &str,
    ) -> Result<(), sqlx::Error> {
        Ok(())
    }

    async fn get_rotations(&self, _family_id: &str) -> Result<Vec<RefreshTokenRotation>, sqlx::Error> {
        Ok(Vec::new())
    }

    async fn add_to_blacklist(
        &self,
        token_hash: &str,
        _token_type: &str,
        _user_id: &str,
        expires_at: i64,
        _reason: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        self.blacklist.write().await.insert(token_hash.to_string(), expires_at);
        Ok(())
    }

    async fn is_blacklisted(&self, token_hash: &str) -> Result<bool, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let blacklist = self.blacklist.read().await;
        Ok(blacklist.get(token_hash).is_some_and(|exp| *exp > now))
    }

    async fn cleanup_expired_tokens(&self) -> Result<i64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut tokens = self.tokens.write().await;
        let mut hash_index = self.token_hash_index.write().await;
        let mut removed = 0i64;
        let expired_ids: Vec<i64> = tokens
            .iter()
            .filter(|(_, t)| !t.is_revoked && t.expires_at.is_some_and(|exp| exp < now))
            .map(|(id, _)| *id)
            .collect();
        for id in expired_ids {
            if let Some(token) = tokens.remove(&id) {
                hash_index.remove(&token.token_hash);
                removed += 1;
            }
        }
        Ok(removed)
    }

    async fn cleanup_blacklist(&self) -> Result<i64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut blacklist = self.blacklist.write().await;
        let expired: Vec<String> = blacklist.iter().filter(|(_, exp)| **exp < now).map(|(k, _)| k.clone()).collect();
        let count = expired.len() as i64;
        for key in expired {
            blacklist.remove(&key);
        }
        Ok(count)
    }

    async fn get_user_stats(&self, _user_id: &str) -> Result<Option<RefreshTokenStats>, sqlx::Error> {
        Ok(None)
    }

    async fn get_usage_history(&self, _user_id: &str, _limit: i64) -> Result<Vec<RefreshTokenUsage>, sqlx::Error> {
        Ok(Vec::new())
    }
}

use crate::presence::PresenceStoreApi;

/// In-memory presence snapshot: `(presence, status_msg, last_active_ts)`.
type PresenceSnapshot = (String, Option<String>, Option<i64>);

/// In-memory presence store for testing [`PresenceService`] and
/// [`FriendRoomService`] without a real PostgreSQL pool.
///
/// Stores presence snapshots in a `HashMap<user_id, PresenceSnapshot>`
/// and subscriptions in a `Vec<(subscriber, target)>`.
///
/// Ref: TDD落地执行清单 §8.3 ARC-12a (Problem #6 Trait 采纳补齐)
#[derive(Clone, Default)]
pub struct InMemoryPresenceStore {
    presences: Arc<tokio::sync::RwLock<HashMap<String, PresenceSnapshot>>>,
    subscriptions: Arc<tokio::sync::RwLock<Vec<(String, String)>>>,
}

impl InMemoryPresenceStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl PresenceStoreApi for InMemoryPresenceStore {
    fn pool(&self) -> &Arc<sqlx::PgPool> {
        unimplemented!("InMemoryPresenceStore has no database pool")
    }

    async fn set_presence(&self, user_id: &str, presence: &str, status_msg: Option<&str>) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        self.presences
            .write()
            .await
            .insert(user_id.to_string(), (presence.to_string(), status_msg.map(|s| s.to_string()), Some(now)));
        Ok(())
    }

    async fn get_presences(
        &self,
        user_ids: &[String],
    ) -> Result<HashMap<String, (String, Option<String>)>, sqlx::Error> {
        let map = self.presences.read().await;
        let mut result = HashMap::new();
        for user_id in user_ids {
            if let Some((presence, status_msg, _)) = map.get(user_id) {
                result.insert(user_id.clone(), (presence.clone(), status_msg.clone()));
            }
        }
        Ok(result)
    }

    async fn get_presence_with_meta(
        &self,
        user_id: &str,
    ) -> Result<Option<(String, Option<String>, Option<i64>)>, sqlx::Error> {
        Ok(self.presences.read().await.get(user_id).cloned())
    }

    async fn remove_subscription(&self, subscriber_id: &str, target_id: &str) -> Result<(), sqlx::Error> {
        self.subscriptions.write().await.retain(|(s, t)| !(*s == subscriber_id && *t == target_id));
        Ok(())
    }

    async fn add_subscription(&self, subscriber_id: &str, target_id: &str) -> Result<(), sqlx::Error> {
        let mut subs = self.subscriptions.write().await;
        let entry = (subscriber_id.to_string(), target_id.to_string());
        if !subs.contains(&entry) {
            subs.push(entry);
        }
        Ok(())
    }

    async fn get_subscriptions(&self, subscriber_id: &str) -> Result<Vec<String>, sqlx::Error> {
        Ok(self.subscriptions.read().await.iter().filter(|(s, _)| s == subscriber_id).map(|(_, t)| t.clone()).collect())
    }

    async fn get_presence_batch_with_meta(
        &self,
        user_ids: &[String],
    ) -> Result<Vec<(String, String, Option<String>, Option<i64>)>, sqlx::Error> {
        let map = self.presences.read().await;
        let mut results = Vec::new();
        for user_id in user_ids {
            if let Some((presence, status_msg, last_active_ts)) = map.get(user_id) {
                results.push((user_id.clone(), presence.clone(), status_msg.clone(), *last_active_ts));
            }
        }
        Ok(results)
    }

    async fn get_presence_snapshots(
        &self,
        user_ids: &[String],
    ) -> Result<HashMap<String, crate::presence::PresenceSnapshot>, sqlx::Error> {
        let map = self.presences.read().await;
        let mut result = HashMap::new();
        for user_id in user_ids {
            if let Some((presence, status_msg, last_active_ts)) = map.get(user_id) {
                result.insert(
                    user_id.clone(),
                    crate::presence::PresenceSnapshot {
                        user_id: user_id.clone(),
                        presence: presence.clone(),
                        status_msg: status_msg.clone(),
                        last_active_ts: *last_active_ts,
                    },
                );
            }
        }
        Ok(result)
    }

    async fn set_typing(&self, _room_id: &str, _user_id: &str, _typing: bool) -> Result<(), sqlx::Error> {
        Ok(())
    }
}

// =========================================================================
// InMemoryAccountDataStore
// =========================================================================

#[derive(Clone, Debug, Default)]
pub struct InMemoryAccountDataStore {
    #[allow(clippy::type_complexity)]
    data: Arc<tokio::sync::RwLock<HashMap<(String, String), serde_json::Value>>>,
}

impl InMemoryAccountDataStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl crate::account_data::AccountDataStoreApi for InMemoryAccountDataStore {
    async fn get_account_data_content(
        &self,
        user_id: &str,
        data_type: &str,
    ) -> Result<Option<serde_json::Value>, ApiError> {
        Ok(self.data.read().await.get(&(user_id.to_string(), data_type.to_string())).cloned())
    }

    async fn list_account_data(&self, user_id: &str) -> Result<Vec<crate::account_data::AccountDataRecord>, ApiError> {
        let mut records: Vec<_> = self
            .data
            .read()
            .await
            .iter()
            .filter(|((uid, _), _)| uid == user_id)
            .map(|((_, data_type), content)| crate::account_data::AccountDataRecord {
                data_type: data_type.clone(),
                content: content.clone(),
            })
            .collect();
        records.sort_by(|a, b| a.data_type.cmp(&b.data_type));
        Ok(records)
    }

    async fn delete_account_data(&self, user_id: &str, data_type: &str) -> Result<bool, ApiError> {
        Ok(self.data.write().await.remove(&(user_id.to_string(), data_type.to_string())).is_some())
    }

    async fn upsert_account_data(
        &self,
        user_id: &str,
        data_type: &str,
        content: serde_json::Value,
    ) -> Result<(), ApiError> {
        self.data.write().await.insert((user_id.to_string(), data_type.to_string()), content);
        Ok(())
    }
}

// ── InMemoryRoomSummaryStore ───────────────────────────────────────────

#[allow(clippy::type_complexity)]
#[derive(Clone, Default)]
pub struct InMemoryRoomSummaryStore {
    summaries: Arc<tokio::sync::RwLock<HashMap<String, crate::room_summary::RoomSummary>>>,
    members: Arc<tokio::sync::RwLock<HashMap<(String, String), crate::room_summary::RoomSummaryMember>>>,
    states: Arc<tokio::sync::RwLock<HashMap<(String, String, String), crate::room_summary::RoomSummaryState>>>,
    stats: Arc<tokio::sync::RwLock<HashMap<String, crate::room_summary::RoomSummaryStats>>>,
    queue: Arc<tokio::sync::RwLock<Vec<crate::room_summary::RoomSummaryUpdateQueueItem>>>,
    next_id: Arc<std::sync::atomic::AtomicI64>,
}

impl InMemoryRoomSummaryStore {
    pub fn new() -> Self {
        Self {
            summaries: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            members: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            states: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            stats: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            queue: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            next_id: Arc::new(std::sync::atomic::AtomicI64::new(1)),
        }
    }
}

#[async_trait::async_trait]
impl crate::room_summary::RoomSummaryStoreApi for InMemoryRoomSummaryStore {
    async fn create_summary(
        &self,
        request: crate::room_summary::CreateRoomSummaryRequest,
    ) -> Result<crate::room_summary::RoomSummary, sqlx::Error> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let now = chrono::Utc::now().timestamp_millis();
        let summary = crate::room_summary::RoomSummary {
            id: Some(id),
            room_id: request.room_id.clone(),
            room_type: request.room_type,
            name: request.name,
            topic: request.topic,
            avatar_url: request.avatar_url,
            canonical_alias: request.canonical_alias,
            join_rule: request.join_rule.unwrap_or_else(|| "invite".to_string()),
            history_visibility: request.history_visibility.unwrap_or_else(|| "shared".to_string()),
            guest_access: request.guest_access.unwrap_or_else(|| "forbidden".to_string()),
            is_direct: request.is_direct.unwrap_or(false),
            is_space: request.is_space.unwrap_or(false),
            is_encrypted: false,
            member_count: 0,
            joined_member_count: 0,
            invited_member_count: 0,
            hero_users: serde_json::json!([]),
            last_event_id: None,
            last_event_ts: None,
            last_message_ts: None,
            unread_notifications: 0,
            unread_highlight: 0,
            updated_ts: Some(now),
            created_ts: Some(now),
        };
        self.summaries.write().await.insert(request.room_id, summary.clone());
        Ok(summary)
    }

    async fn get_summary(&self, room_id: &str) -> Result<Option<crate::room_summary::RoomSummary>, sqlx::Error> {
        Ok(self.summaries.read().await.get(room_id).cloned())
    }

    async fn update_summary(
        &self,
        room_id: &str,
        request: crate::room_summary::UpdateRoomSummaryRequest,
    ) -> Result<crate::room_summary::RoomSummary, sqlx::Error> {
        let mut summaries = self.summaries.write().await;
        let summary = summaries.get_mut(room_id).ok_or_else(|| sqlx::Error::RowNotFound)?;
        if let Some(v) = request.name {
            summary.name = Some(v);
        }
        if let Some(v) = request.topic {
            summary.topic = Some(v);
        }
        if let Some(v) = request.avatar_url {
            summary.avatar_url = Some(v);
        }
        if let Some(v) = request.canonical_alias {
            summary.canonical_alias = Some(v);
        }
        if let Some(v) = request.join_rule {
            summary.join_rule = v;
        }
        if let Some(v) = request.history_visibility {
            summary.history_visibility = v;
        }
        if let Some(v) = request.guest_access {
            summary.guest_access = v;
        }
        if let Some(v) = request.is_direct {
            summary.is_direct = v;
        }
        if let Some(v) = request.is_space {
            summary.is_space = v;
        }
        if let Some(v) = request.is_encrypted {
            summary.is_encrypted = v;
        }
        if let Some(v) = request.last_event_id {
            summary.last_event_id = Some(v);
        }
        if let Some(v) = request.last_event_ts {
            summary.last_event_ts = Some(v);
        }
        if let Some(v) = request.last_message_ts {
            summary.last_message_ts = Some(v);
        }
        if let Some(v) = request.hero_users {
            summary.hero_users = v;
        }
        summary.updated_ts = Some(chrono::Utc::now().timestamp_millis());
        Ok(summary.clone())
    }

    async fn set_canonical_alias(
        &self,
        room_id: &str,
        canonical_alias: Option<&str>,
    ) -> Result<crate::room_summary::RoomSummary, sqlx::Error> {
        let mut summaries = self.summaries.write().await;
        let summary = summaries.get_mut(room_id).ok_or_else(|| sqlx::Error::RowNotFound)?;
        summary.canonical_alias = canonical_alias.map(|s| s.to_string());
        summary.updated_ts = Some(chrono::Utc::now().timestamp_millis());
        Ok(summary.clone())
    }

    async fn delete_summary(&self, room_id: &str) -> Result<(), sqlx::Error> {
        self.summaries.write().await.remove(room_id);
        Ok(())
    }

    async fn get_summaries_by_ids(
        &self,
        room_ids: &[String],
    ) -> Result<Vec<crate::room_summary::RoomSummary>, sqlx::Error> {
        let summaries = self.summaries.read().await;
        Ok(room_ids.iter().filter_map(|id| summaries.get(id).cloned()).collect())
    }

    async fn get_summaries_for_user(
        &self,
        user_id: &str,
    ) -> Result<Vec<crate::room_summary::RoomSummary>, sqlx::Error> {
        let members = self.members.read().await;
        let summaries = self.summaries.read().await;
        let room_ids: std::collections::HashSet<String> = members
            .iter()
            .filter(|((_, uid), m)| uid == user_id && (m.membership == "join" || m.membership == "invite"))
            .map(|((rid, _), _)| rid.clone())
            .collect();
        let mut result: Vec<_> = room_ids.iter().filter_map(|rid| summaries.get(rid).cloned()).collect();
        result.sort_by(|a, b| b.last_event_ts.cmp(&a.last_event_ts));
        Ok(result)
    }

    async fn get_heroes(
        &self,
        room_id: &str,
        limit: i64,
    ) -> Result<Vec<crate::room_summary::RoomSummaryMember>, sqlx::Error> {
        let members = self.members.read().await;
        let mut result: Vec<_> = members
            .iter()
            .filter(|((rid, _), m)| rid == room_id && m.membership == "join")
            .map(|(_, m)| m.clone())
            .collect();
        result.sort_by(|a, b| b.is_hero.cmp(&a.is_hero).then_with(|| a.user_id.cmp(&b.user_id)));
        result.truncate(limit as usize);
        Ok(result)
    }

    async fn get_heroes_batch(
        &self,
        room_ids: &[String],
        limit: i64,
    ) -> Result<HashMap<String, Vec<crate::room_summary::RoomSummaryMember>>, sqlx::Error> {
        let members = self.members.read().await;
        let mut map: HashMap<String, Vec<crate::room_summary::RoomSummaryMember>> = HashMap::new();
        for rid in room_ids {
            let mut room_members: Vec<_> = members
                .iter()
                .filter(|((r, _), m)| r == rid && m.membership == "join")
                .map(|(_, m)| m.clone())
                .collect();
            room_members.sort_by(|a, b| b.is_hero.cmp(&a.is_hero).then_with(|| a.user_id.cmp(&b.user_id)));
            room_members.truncate(limit as usize);
            map.insert(rid.clone(), room_members);
        }
        Ok(map)
    }

    async fn add_member(
        &self,
        request: crate::room_summary::CreateSummaryMemberRequest,
    ) -> Result<crate::room_summary::RoomSummaryMember, sqlx::Error> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let now = chrono::Utc::now().timestamp_millis();
        let member = crate::room_summary::RoomSummaryMember {
            id,
            room_id: request.room_id.clone(),
            user_id: request.user_id.clone(),
            display_name: request.display_name,
            avatar_url: request.avatar_url,
            membership: request.membership,
            is_hero: request.is_hero.unwrap_or(false),
            last_active_ts: request.last_active_ts,
            updated_ts: now,
            created_ts: now,
        };
        self.members.write().await.insert((request.room_id, request.user_id), member.clone());
        Ok(member)
    }

    async fn add_members_batch(
        &self,
        room_id: &str,
        members: Vec<crate::room_summary::CreateSummaryMemberRequest>,
    ) -> Result<usize, sqlx::Error> {
        let count = members.len();
        let now = chrono::Utc::now().timestamp_millis();
        let mut store = self.members.write().await;
        for m in members {
            let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            store.insert(
                (room_id.to_string(), m.user_id.clone()),
                crate::room_summary::RoomSummaryMember {
                    id,
                    room_id: room_id.to_string(),
                    user_id: m.user_id,
                    display_name: m.display_name,
                    avatar_url: m.avatar_url,
                    membership: m.membership,
                    is_hero: m.is_hero.unwrap_or(false),
                    last_active_ts: m.last_active_ts,
                    updated_ts: now,
                    created_ts: now,
                },
            );
        }
        Ok(count)
    }

    async fn update_member(
        &self,
        room_id: &str,
        user_id: &str,
        request: crate::room_summary::UpdateSummaryMemberRequest,
    ) -> Result<crate::room_summary::RoomSummaryMember, sqlx::Error> {
        let mut members = self.members.write().await;
        let key = (room_id.to_string(), user_id.to_string());
        let member = members.get_mut(&key).ok_or_else(|| sqlx::Error::RowNotFound)?;
        if let Some(v) = request.display_name {
            member.display_name = Some(v);
        }
        if let Some(v) = request.avatar_url {
            member.avatar_url = Some(v);
        }
        if let Some(v) = request.membership {
            member.membership = v;
        }
        if let Some(v) = request.is_hero {
            member.is_hero = v;
        }
        if let Some(v) = request.last_active_ts {
            member.last_active_ts = Some(v);
        }
        member.updated_ts = chrono::Utc::now().timestamp_millis();
        Ok(member.clone())
    }

    async fn remove_member(&self, room_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
        self.members.write().await.remove(&(room_id.to_string(), user_id.to_string()));
        Ok(())
    }

    async fn get_members(&self, room_id: &str) -> Result<Vec<crate::room_summary::RoomSummaryMember>, sqlx::Error> {
        let members = self.members.read().await;
        let mut result: Vec<_> =
            members.iter().filter(|((rid, _), _)| rid == room_id).map(|(_, m)| m.clone()).collect();
        result.sort_by(|a, b| b.is_hero.cmp(&a.is_hero).then_with(|| a.user_id.cmp(&b.user_id)));
        Ok(result)
    }

    async fn set_state(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
        event_id: Option<&str>,
        content: serde_json::Value,
    ) -> Result<crate::room_summary::RoomSummaryState, sqlx::Error> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let now = chrono::Utc::now().timestamp_millis();
        let state = crate::room_summary::RoomSummaryState {
            id,
            room_id: room_id.to_string(),
            event_type: event_type.to_string(),
            state_key: state_key.to_string(),
            event_id: event_id.map(|s| s.to_string()),
            content,
            updated_ts: now,
        };
        self.states
            .write()
            .await
            .insert((room_id.to_string(), event_type.to_string(), state_key.to_string()), state.clone());
        Ok(state)
    }

    async fn set_states_batch(
        &self,
        room_id: &str,
        entries: &[crate::room_summary::RoomSummaryStateEntry],
    ) -> Result<u64, sqlx::Error> {
        let count = entries.len() as u64;
        let now = chrono::Utc::now().timestamp_millis();
        let mut states = self.states.write().await;
        for entry in entries {
            let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            states.insert(
                (room_id.to_string(), entry.event_type.clone(), entry.state_key.clone()),
                crate::room_summary::RoomSummaryState {
                    id,
                    room_id: room_id.to_string(),
                    event_type: entry.event_type.clone(),
                    state_key: entry.state_key.clone(),
                    event_id: entry.event_id.clone(),
                    content: entry.content.clone(),
                    updated_ts: now,
                },
            );
        }
        Ok(count)
    }

    async fn get_state(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
    ) -> Result<Option<crate::room_summary::RoomSummaryState>, sqlx::Error> {
        Ok(self.states.read().await.get(&(room_id.to_string(), event_type.to_string(), state_key.to_string())).cloned())
    }

    async fn get_all_state(&self, room_id: &str) -> Result<Vec<crate::room_summary::RoomSummaryState>, sqlx::Error> {
        Ok(self.states.read().await.iter().filter(|((rid, _, _), _)| rid == room_id).map(|(_, s)| s.clone()).collect())
    }

    async fn get_stats(&self, room_id: &str) -> Result<Option<crate::room_summary::RoomSummaryStats>, sqlx::Error> {
        Ok(self.stats.read().await.get(room_id).cloned())
    }

    async fn update_stats(
        &self,
        room_id: &str,
        total_events: i64,
        total_state_events: i64,
        total_messages: i64,
        total_media: i64,
        storage_size: i64,
    ) -> Result<crate::room_summary::RoomSummaryStats, sqlx::Error> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let now = chrono::Utc::now().timestamp_millis();
        let stats = crate::room_summary::RoomSummaryStats {
            id,
            room_id: room_id.to_string(),
            total_events,
            total_state_events,
            total_messages,
            total_media,
            storage_size,
            last_updated_ts: now,
        };
        self.stats.write().await.insert(room_id.to_string(), stats.clone());
        Ok(stats)
    }

    async fn queue_update(
        &self,
        room_id: &str,
        event_id: &str,
        event_type: &str,
        state_key: Option<&str>,
        priority: i32,
    ) -> Result<(), sqlx::Error> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let now = chrono::Utc::now().timestamp_millis();
        self.queue.write().await.push(crate::room_summary::RoomSummaryUpdateQueueItem {
            id,
            room_id: room_id.to_string(),
            event_id: event_id.to_string(),
            event_type: event_type.to_string(),
            state_key: state_key.map(|s| s.to_string()),
            priority,
            status: "pending".to_string(),
            created_ts: now,
            processed_ts: None,
            error_message: None,
            retry_count: 0,
        });
        Ok(())
    }

    async fn get_pending_updates(
        &self,
        limit: i64,
    ) -> Result<Vec<crate::room_summary::RoomSummaryUpdateQueueItem>, sqlx::Error> {
        let mut queue = self.queue.read().await.clone();
        queue.retain(|q| q.status == "pending");
        queue.sort_by(|a, b| b.priority.cmp(&a.priority).then_with(|| a.created_ts.cmp(&b.created_ts)));
        queue.truncate(limit as usize);
        Ok(queue)
    }

    async fn mark_update_processed(&self, id: i64) -> Result<(), sqlx::Error> {
        if let Some(item) = self.queue.write().await.iter_mut().find(|q| q.id == id) {
            item.status = "processed".to_string();
            item.processed_ts = Some(chrono::Utc::now().timestamp_millis());
        }
        Ok(())
    }

    async fn mark_update_failed(&self, id: i64, error: &str) -> Result<(), sqlx::Error> {
        if let Some(item) = self.queue.write().await.iter_mut().find(|q| q.id == id) {
            item.status = "failed".to_string();
            item.error_message = Some(error.to_string());
            item.retry_count += 1;
        }
        Ok(())
    }

    async fn increment_unread_notifications(&self, room_id: &str, highlight: bool) -> Result<(), sqlx::Error> {
        let mut summaries = self.summaries.write().await;
        if let Some(s) = summaries.get_mut(room_id) {
            s.unread_notifications += 1;
            if highlight {
                s.unread_highlight += 1;
            }
            s.updated_ts = Some(chrono::Utc::now().timestamp_millis());
        }
        Ok(())
    }

    async fn clear_unread_notifications(&self, room_id: &str) -> Result<(), sqlx::Error> {
        let mut summaries = self.summaries.write().await;
        if let Some(s) = summaries.get_mut(room_id) {
            s.unread_notifications = 0;
            s.unread_highlight = 0;
            s.updated_ts = Some(chrono::Utc::now().timestamp_millis());
        }
        Ok(())
    }

    async fn get_hero_candidates(
        &self,
        room_id: &str,
        limit: i64,
    ) -> Result<Vec<crate::room_summary::RoomSummaryMember>, sqlx::Error> {
        let members = self.members.read().await;
        let mut result: Vec<_> = members
            .iter()
            .filter(|((rid, _), m)| rid == room_id && m.membership == "join")
            .map(|(_, m)| m.clone())
            .collect();
        result.sort_by(|a, b| b.last_active_ts.cmp(&a.last_active_ts));
        result.truncate(limit as usize);
        Ok(result)
    }

    async fn set_hero_members(&self, room_id: &str, hero_user_ids: &[String]) -> Result<(), sqlx::Error> {
        let hero_set: std::collections::HashSet<&str> = hero_user_ids.iter().map(|s| s.as_str()).collect();
        let mut members = self.members.write().await;
        for ((rid, _), member) in members.iter_mut() {
            if rid == room_id {
                member.is_hero = hero_set.contains(member.user_id.as_str());
            }
        }
        Ok(())
    }
}

// ── InMemoryWorkerStore ───────────────────────────────────────────

#[allow(clippy::type_complexity)]
#[derive(Clone, Default)]
pub struct InMemoryWorkerStore {
    workers: Arc<RwLock<HashMap<String, crate::worker::WorkerInfo>>>,
    commands: Arc<RwLock<Vec<crate::worker::WorkerCommand>>>,
    events: Arc<RwLock<Vec<crate::worker::WorkerEvent>>>,
    replication_positions: Arc<RwLock<HashMap<(String, String), crate::worker::ReplicationPosition>>>,
    tasks: Arc<RwLock<Vec<crate::worker::WorkerTaskAssignment>>>,
    next_id: Arc<std::sync::atomic::AtomicI64>,
}

impl InMemoryWorkerStore {
    pub fn new() -> Self {
        Self {
            workers: Arc::new(RwLock::new(HashMap::new())),
            commands: Arc::new(RwLock::new(Vec::new())),
            events: Arc::new(RwLock::new(Vec::new())),
            replication_positions: Arc::new(RwLock::new(HashMap::new())),
            tasks: Arc::new(RwLock::new(Vec::new())),
            next_id: Arc::new(std::sync::atomic::AtomicI64::new(1)),
        }
    }
}

#[async_trait::async_trait]
impl crate::worker::WorkerStoreApi for InMemoryWorkerStore {
    async fn register_worker(
        &self,
        request: crate::worker::RegisterWorkerRequest,
    ) -> Result<crate::worker::WorkerInfo, sqlx::Error> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let now = chrono::Utc::now().timestamp_millis();
        let info = crate::worker::WorkerInfo {
            id,
            worker_id: request.worker_id.clone(),
            worker_name: request.worker_name,
            worker_type: request.worker_type.as_str().to_string(),
            host: request.host,
            port: request.port as i32,
            status: "running".to_string(),
            last_heartbeat_ts: Some(now),
            started_ts: now,
            stopped_ts: None,
            config: request.config.unwrap_or(serde_json::Value::Null),
            metadata: request.metadata.unwrap_or(serde_json::Value::Null),
            version: request.version,
        };
        self.workers.write().await.insert(info.worker_id.clone(), info.clone());
        Ok(info)
    }

    async fn get_worker(&self, worker_id: &str) -> Result<Option<crate::worker::WorkerInfo>, sqlx::Error> {
        Ok(self.workers.read().await.get(worker_id).cloned())
    }

    async fn get_workers_by_type(&self, worker_type: &str) -> Result<Vec<crate::worker::WorkerInfo>, sqlx::Error> {
        Ok(self.workers.read().await.values().filter(|w| w.worker_type == worker_type).cloned().collect())
    }

    async fn get_active_workers(&self) -> Result<Vec<crate::worker::WorkerInfo>, sqlx::Error> {
        Ok(self
            .workers
            .read()
            .await
            .values()
            .filter(|w| w.status == "running" || w.status == "starting")
            .cloned()
            .collect())
    }

    async fn update_worker_status(&self, worker_id: &str, status: &str) -> Result<(), sqlx::Error> {
        let mut workers = self.workers.write().await;
        if let Some(worker) = workers.get_mut(worker_id) {
            worker.status = status.to_string();
            if status == "stopped" {
                worker.stopped_ts = Some(chrono::Utc::now().timestamp_millis());
            }
        }
        Ok(())
    }

    async fn update_heartbeat(&self, worker_id: &str) -> Result<(), sqlx::Error> {
        let mut workers = self.workers.write().await;
        if let Some(worker) = workers.get_mut(worker_id) {
            worker.last_heartbeat_ts = Some(chrono::Utc::now().timestamp_millis());
        }
        Ok(())
    }

    async fn unregister_worker(&self, worker_id: &str) -> Result<(), sqlx::Error> {
        self.workers.write().await.remove(worker_id);
        Ok(())
    }

    async fn create_command(
        &self,
        request: crate::worker::SendCommandRequest,
    ) -> Result<crate::worker::WorkerCommand, sqlx::Error> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let now = chrono::Utc::now().timestamp_millis();
        let command_id = format!("cmd-{id}");
        let command = crate::worker::WorkerCommand {
            id,
            command_id: command_id.clone(),
            target_worker_id: request.target_worker_id,
            source_worker_id: None,
            command_type: request.command_type,
            command_data: request.command_data,
            priority: request.priority.unwrap_or(0),
            status: "pending".to_string(),
            created_ts: now,
            sent_ts: None,
            completed_ts: None,
            error_message: None,
            retry_count: 0,
            max_retries: request.max_retries.unwrap_or(3),
        };
        self.commands.write().await.push(command.clone());
        Ok(command)
    }

    async fn get_pending_commands(
        &self,
        worker_id: &str,
        limit: i64,
    ) -> Result<Vec<crate::worker::WorkerCommand>, sqlx::Error> {
        let commands = self.commands.read().await;
        let mut result: Vec<_> =
            commands.iter().filter(|c| c.target_worker_id == worker_id && c.status == "pending").cloned().collect();
        result.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.created_ts.cmp(&b.created_ts)));
        result.truncate(limit as usize);
        Ok(result)
    }

    async fn mark_command_sent(&self, command_id: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut commands = self.commands.write().await;
        for cmd in commands.iter_mut() {
            if cmd.command_id == command_id {
                cmd.status = "sent".to_string();
                cmd.sent_ts = Some(now);
                break;
            }
        }
        Ok(())
    }

    async fn complete_command(&self, command_id: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut commands = self.commands.write().await;
        for cmd in commands.iter_mut() {
            if cmd.command_id == command_id {
                cmd.status = "completed".to_string();
                cmd.completed_ts = Some(now);
                break;
            }
        }
        Ok(())
    }

    async fn fail_command(&self, command_id: &str, error: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut commands = self.commands.write().await;
        for cmd in commands.iter_mut() {
            if cmd.command_id == command_id {
                cmd.status = "failed".to_string();
                cmd.completed_ts = Some(now);
                cmd.error_message = Some(error.to_string());
                cmd.retry_count += 1;
                break;
            }
        }
        Ok(())
    }

    async fn add_event(
        &self,
        event_id: &str,
        event_type: &str,
        room_id: Option<&str>,
        sender: Option<&str>,
        event_data: serde_json::Value,
    ) -> Result<crate::worker::WorkerEvent, sqlx::Error> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let stream_id = id;
        let now = chrono::Utc::now().timestamp_millis();
        let event = crate::worker::WorkerEvent {
            id,
            event_id: event_id.to_string(),
            stream_id,
            event_type: event_type.to_string(),
            room_id: room_id.map(|s| s.to_string()),
            sender: sender.map(|s| s.to_string()),
            event_data,
            created_ts: now,
            processed_by: Some(Vec::new()),
        };
        self.events.write().await.push(event.clone());
        Ok(event)
    }

    async fn get_events_since(
        &self,
        stream_id: i64,
        limit: i64,
    ) -> Result<Vec<crate::worker::WorkerEvent>, sqlx::Error> {
        let events = self.events.read().await;
        let mut result: Vec<_> = events.iter().filter(|e| e.stream_id > stream_id).cloned().collect();
        result.sort_by_key(|e| e.stream_id);
        result.truncate(limit as usize);
        Ok(result)
    }

    async fn mark_event_processed(&self, event_id: &str, worker_id: &str) -> Result<(), sqlx::Error> {
        let mut events = self.events.write().await;
        for event in events.iter_mut() {
            if event.event_id == event_id {
                let processed = event.processed_by.get_or_insert_with(Vec::new);
                if !processed.iter().any(|w| w == worker_id) {
                    processed.push(worker_id.to_string());
                }
                break;
            }
        }
        Ok(())
    }

    async fn update_replication_position(
        &self,
        worker_id: &str,
        stream_name: &str,
        position: i64,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let key = (worker_id.to_string(), stream_name.to_string());
        self.replication_positions.write().await.insert(
            key,
            crate::worker::ReplicationPosition {
                id,
                worker_id: worker_id.to_string(),
                stream_name: stream_name.to_string(),
                stream_position: position,
                updated_ts: now,
            },
        );
        Ok(())
    }

    async fn get_replication_position(&self, worker_id: &str, stream_name: &str) -> Result<Option<i64>, sqlx::Error> {
        let key = (worker_id.to_string(), stream_name.to_string());
        Ok(self.replication_positions.read().await.get(&key).map(|p| p.stream_position))
    }

    fn record_load_stats(
        &self,
        _worker_id: &str,
        _stats: &crate::worker::WorkerLoadStatsUpdate,
    ) -> Result<(), sqlx::Error> {
        Ok(())
    }

    async fn assign_task(
        &self,
        request: crate::worker::AssignTaskRequest,
    ) -> Result<crate::worker::WorkerTaskAssignment, sqlx::Error> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let now = chrono::Utc::now().timestamp_millis();
        let task_id = format!("task-{id}");
        let task = crate::worker::WorkerTaskAssignment {
            id,
            task_id: task_id.clone(),
            task_type: request.task_type,
            task_data: request.task_data,
            assigned_worker_id: request.preferred_worker_id,
            status: "pending".to_string(),
            priority: request.priority.unwrap_or(0),
            created_ts: now,
            assigned_ts: None,
            completed_ts: None,
            result: None,
            error_message: None,
        };
        self.tasks.write().await.push(task.clone());
        Ok(task)
    }

    async fn get_pending_tasks(&self, limit: i64) -> Result<Vec<crate::worker::WorkerTaskAssignment>, sqlx::Error> {
        let tasks = self.tasks.read().await;
        let mut result: Vec<_> = tasks.iter().filter(|t| t.status == "pending").cloned().collect();
        result.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.created_ts.cmp(&b.created_ts)));
        result.truncate(limit as usize);
        Ok(result)
    }

    async fn claim_next_pending_task(
        &self,
        worker_id: &str,
    ) -> Result<Option<crate::worker::WorkerTaskAssignment>, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut tasks = self.tasks.write().await;
        // Pick highest priority, earliest created pending task
        let mut chosen: Option<usize> = None;
        for (idx, task) in tasks.iter().enumerate() {
            if task.status != "pending" {
                continue;
            }
            match chosen {
                None => chosen = Some(idx),
                Some(prev) => {
                    let prev_task = &tasks[prev];
                    let better = (task.priority > prev_task.priority)
                        || (task.priority == prev_task.priority && task.created_ts < prev_task.created_ts);
                    if better {
                        chosen = Some(idx);
                    }
                }
            }
        }
        if let Some(idx) = chosen {
            tasks[idx].status = "assigned".to_string();
            tasks[idx].assigned_worker_id = Some(worker_id.to_string());
            tasks[idx].assigned_ts = Some(now);
            Ok(Some(tasks[idx].clone()))
        } else {
            Ok(None)
        }
    }

    async fn claim_next_pending_task_for_types(
        &self,
        worker_id: &str,
        allowed_task_types: &[String],
    ) -> Result<Option<crate::worker::WorkerTaskAssignment>, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut tasks = self.tasks.write().await;
        let mut chosen: Option<usize> = None;
        for (idx, task) in tasks.iter().enumerate() {
            if task.status != "pending" {
                continue;
            }
            if !allowed_task_types.iter().any(|t| t == &task.task_type) {
                continue;
            }
            match chosen {
                None => chosen = Some(idx),
                Some(prev) => {
                    let prev_task = &tasks[prev];
                    let better = (task.priority > prev_task.priority)
                        || (task.priority == prev_task.priority && task.created_ts < prev_task.created_ts);
                    if better {
                        chosen = Some(idx);
                    }
                }
            }
        }
        if let Some(idx) = chosen {
            tasks[idx].status = "assigned".to_string();
            tasks[idx].assigned_worker_id = Some(worker_id.to_string());
            tasks[idx].assigned_ts = Some(now);
            Ok(Some(tasks[idx].clone()))
        } else {
            Ok(None)
        }
    }

    async fn assign_task_to_worker(&self, task_id: &str, worker_id: &str) -> Result<bool, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut tasks = self.tasks.write().await;
        for task in tasks.iter_mut() {
            if task.task_id == task_id && task.status == "pending" {
                task.status = "assigned".to_string();
                task.assigned_worker_id = Some(worker_id.to_string());
                task.assigned_ts = Some(now);
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn complete_task(&self, task_id: &str, result: Option<serde_json::Value>) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut tasks = self.tasks.write().await;
        for task in tasks.iter_mut() {
            if task.task_id == task_id {
                task.status = "completed".to_string();
                task.completed_ts = Some(now);
                task.result = result;
                break;
            }
        }
        Ok(())
    }

    async fn fail_task(&self, task_id: &str, error: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut tasks = self.tasks.write().await;
        for task in tasks.iter_mut() {
            if task.task_id == task_id {
                task.status = "failed".to_string();
                task.completed_ts = Some(now);
                task.error_message = Some(error.to_string());
                break;
            }
        }
        Ok(())
    }

    fn record_connection(
        &self,
        _source_worker_id: &str,
        _target_worker_id: &str,
        _connection_type: &str,
    ) -> Result<(), sqlx::Error> {
        Ok(())
    }

    fn update_connection_stats(
        &self,
        _request: &crate::worker::UpdateConnectionStatsRequest,
    ) -> Result<(), sqlx::Error> {
        Ok(())
    }

    async fn get_statistics(&self, _limit: i64) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        Ok(Vec::new())
    }

    async fn get_type_statistics(&self) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        Ok(Vec::new())
    }
}

// ── InMemorySlidingSyncStore ────────────────────────────────────────

#[allow(clippy::type_complexity)]
#[derive(Clone, Default)]
pub struct InMemorySlidingSyncStore {
    tokens: std::sync::Arc<
        tokio::sync::RwLock<
            std::collections::HashMap<(String, String, Option<String>), crate::sliding_sync::SlidingSyncToken>,
        >,
    >,
    lists: std::sync::Arc<
        tokio::sync::RwLock<
            std::collections::HashMap<(String, String, Option<String>, String), crate::sliding_sync::SlidingSyncList>,
        >,
    >,
    rooms: std::sync::Arc<
        tokio::sync::RwLock<
            std::collections::HashMap<(String, String, Option<String>, String), crate::sliding_sync::SlidingSyncRoom>,
        >,
    >,
    global_account_data: std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<String, serde_json::Value>>>,
    room_account_data:
        std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<(String, String), serde_json::Value>>>,
    receipts: std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<String, serde_json::Value>>>,
    next_id: std::sync::Arc<std::sync::atomic::AtomicI64>,
}

impl InMemorySlidingSyncStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn key(user_id: &str, device_id: &str, conn_id: Option<&str>) -> (String, String, Option<String>) {
        (user_id.to_string(), device_id.to_string(), conn_id.map(|s| s.to_string()))
    }
}

#[async_trait::async_trait]
impl crate::sliding_sync::SlidingSyncStoreApi for InMemorySlidingSyncStore {
    async fn create_or_update_token(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
    ) -> Result<crate::sliding_sync::SlidingSyncToken, sqlx::Error> {
        let key = Self::key(user_id, device_id, conn_id);
        let now = chrono::Utc::now().timestamp_millis();
        let token_id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let token = crate::sliding_sync::SlidingSyncToken {
            id: token_id,
            user_id: user_id.to_string(),
            device_id: device_id.to_string(),
            conn_id: conn_id.map(|s| s.to_string()),
            token: format!("sst_{}", token_id),
            pos: token_id,
            created_ts: now,
            expires_at: Some(now + 1_800_000),
        };
        self.tokens.write().await.insert(key, token.clone());
        Ok(token)
    }

    async fn get_token(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
    ) -> Result<Option<crate::sliding_sync::SlidingSyncToken>, sqlx::Error> {
        Ok(self.tokens.read().await.get(&Self::key(user_id, device_id, conn_id)).cloned())
    }

    async fn validate_pos(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        pos: &str,
    ) -> Result<bool, sqlx::Error> {
        let Ok(pos_i64) = pos.parse::<i64>() else {
            return Ok(false);
        };
        Ok(self.tokens.read().await.get(&Self::key(user_id, device_id, conn_id)).is_some_and(|t| t.pos == pos_i64))
    }

    async fn save_list(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        list_key: &str,
        sort: &[String],
        filters: Option<&crate::sliding_sync::SlidingSyncFilters>,
        room_subscription: Option<&serde_json::Value>,
        ranges: &[(u32, u32)],
    ) -> Result<crate::sliding_sync::SlidingSyncList, sqlx::Error> {
        let list_key_owned =
            (user_id.to_string(), device_id.to_string(), conn_id.map(|s| s.to_string()), list_key.to_string());
        let now = chrono::Utc::now().timestamp_millis();
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let list = crate::sliding_sync::SlidingSyncList {
            id,
            user_id: user_id.to_string(),
            device_id: device_id.to_string(),
            conn_id: conn_id.map(|s| s.to_string()),
            list_key: list_key.to_string(),
            sort: serde_json::to_value(sort).unwrap_or_default(),
            filters: filters.map(|f| serde_json::to_value(f).unwrap_or_default()),
            room_subscription: room_subscription.cloned(),
            ranges: Some(serde_json::to_value(ranges).unwrap_or_default()),
            created_ts: now,
            updated_ts: now,
        };
        self.lists.write().await.insert(list_key_owned, list.clone());
        Ok(list)
    }

    async fn get_lists(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
    ) -> Result<Vec<crate::sliding_sync::SlidingSyncList>, sqlx::Error> {
        let uid = user_id.to_string();
        let did = device_id.to_string();
        let cid = conn_id.map(|s| s.to_string());
        Ok(self
            .lists
            .read()
            .await
            .iter()
            .filter(|((u, d, c, _), _)| *u == uid && *d == did && *c == cid)
            .map(|(_, v)| v.clone())
            .collect())
    }

    async fn delete_list(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        list_key: &str,
    ) -> Result<(), sqlx::Error> {
        let key = (user_id.to_string(), device_id.to_string(), conn_id.map(|s| s.to_string()), list_key.to_string());
        self.lists.write().await.remove(&key);
        Ok(())
    }

    async fn upsert_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
        list_key: Option<&str>,
        bump_stamp: i64,
        highlight_count: i32,
        notification_count: i32,
        is_dm: bool,
        is_encrypted: bool,
        is_tombstoned: bool,
        invited: bool,
        name: Option<&str>,
        avatar: Option<&str>,
        timestamp: i64,
    ) -> Result<crate::sliding_sync::SlidingSyncRoom, sqlx::Error> {
        let key = (user_id.to_string(), device_id.to_string(), conn_id.map(|s| s.to_string()), room_id.to_string());
        let now = chrono::Utc::now().timestamp_millis();
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let room = crate::sliding_sync::SlidingSyncRoom {
            id,
            user_id: user_id.to_string(),
            device_id: device_id.to_string(),
            room_id: room_id.to_string(),
            conn_id: conn_id.map(|s| s.to_string()),
            list_key: list_key.map(|s| s.to_string()),
            bump_stamp,
            highlight_count,
            notification_count,
            is_dm,
            is_encrypted,
            is_tombstoned,
            is_invited: invited,
            name: name.map(|s| s.to_string()),
            avatar: avatar.map(|s| s.to_string()),
            timestamp,
            created_ts: now,
            updated_ts: now,
        };
        self.rooms.write().await.insert(key, room.clone());
        Ok(room)
    }

    async fn get_rooms_for_list(
        &self,
        query_params: crate::sliding_sync::SlidingSyncListQuery<'_>,
    ) -> Result<Vec<crate::sliding_sync::SlidingSyncRoom>, sqlx::Error> {
        let uid = query_params.user_id.to_string();
        let did = query_params.device_id.to_string();
        let cid = query_params.conn_id.map(|s| s.to_string());
        let lk = query_params.list_key.to_string();
        let mut rooms: Vec<_> = self
            .rooms
            .read()
            .await
            .iter()
            .filter(|((u, d, c, _), r)| *u == uid && *d == did && *c == cid && r.list_key.as_deref() == Some(&lk))
            .map(|(_, v)| v.clone())
            .collect();
        rooms.sort_by_key(|r| -r.bump_stamp);
        let start = query_params.start as usize;
        let end = query_params.end as usize;
        if start >= rooms.len() {
            return Ok(Vec::new());
        }
        let end = end.min(rooms.len());
        Ok(rooms[start..end].to_vec())
    }

    async fn count_rooms_for_list(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        list_key: &str,
        _filters: Option<&crate::sliding_sync::SlidingSyncFilters>,
    ) -> Result<i64, sqlx::Error> {
        let uid = user_id.to_string();
        let did = device_id.to_string();
        let cid = conn_id.map(|s| s.to_string());
        let lk = list_key.to_string();
        Ok(self
            .rooms
            .read()
            .await
            .iter()
            .filter(|((u, d, c, _), r)| *u == uid && *d == did && *c == cid && r.list_key.as_deref() == Some(&lk))
            .count() as i64)
    }

    async fn get_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
    ) -> Result<Option<crate::sliding_sync::SlidingSyncRoom>, sqlx::Error> {
        Ok(self
            .rooms
            .read()
            .await
            .get(&(user_id.to_string(), device_id.to_string(), conn_id.map(|s| s.to_string()), room_id.to_string()))
            .cloned())
    }

    async fn materialize_room_from_activity(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
    ) -> Result<Option<crate::sliding_sync::SlidingSyncRoom>, sqlx::Error> {
        // For the mock, simply delegate to get_room — real impl queries
        // activity tables, but tests seed data via upsert_room.
        self.get_room(user_id, device_id, room_id, conn_id).await
    }

    async fn delete_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        self.rooms.write().await.remove(&(
            user_id.to_string(),
            device_id.to_string(),
            conn_id.map(|s| s.to_string()),
            room_id.to_string(),
        ));
        Ok(())
    }

    async fn update_notification_counts(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
        highlight_count: i32,
        notification_count: i32,
    ) -> Result<(), sqlx::Error> {
        let key = (user_id.to_string(), device_id.to_string(), conn_id.map(|s| s.to_string()), room_id.to_string());
        if let Some(room) = self.rooms.write().await.get_mut(&key) {
            room.highlight_count = highlight_count;
            room.notification_count = notification_count;
        }
        Ok(())
    }

    async fn bump_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
        bump_stamp: i64,
    ) -> Result<(), sqlx::Error> {
        let key = (user_id.to_string(), device_id.to_string(), conn_id.map(|s| s.to_string()), room_id.to_string());
        if let Some(room) = self.rooms.write().await.get_mut(&key) {
            room.bump_stamp = bump_stamp;
        }
        Ok(())
    }

    async fn cleanup_expired_tokens(&self) -> Result<u64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut tokens = self.tokens.write().await;
        let before = tokens.len() as u64;
        tokens.retain(|_, t| t.expires_at.is_none_or(|e| e > now));
        Ok(before - tokens.len() as u64)
    }

    async fn list_room_token_sync(
        &self,
        room_id: &str,
        limit: i64,
        _from: Option<&crate::sliding_sync::RoomTokenSyncCursor>,
    ) -> Result<Vec<crate::sliding_sync::AdminRoomTokenSyncEntry>, sqlx::Error> {
        let _rid = room_id.to_string();
        let mut entries: Vec<_> = self
            .tokens
            .read()
            .await
            .iter()
            .filter(|((_, _, _), _t)| true)
            .take(limit as usize)
            .map(|((uid, did, cid), t)| crate::sliding_sync::AdminRoomTokenSyncEntry {
                user_id: uid.clone(),
                device_id: did.clone(),
                conn_id: cid.clone(),
                list_key: None,
                pos: Some(t.pos),
                token_created_ts: Some(t.created_ts),
                token_expires_at: t.expires_at,
                room_timestamp: t.created_ts,
                room_updated_ts: t.created_ts,
                bump_stamp: t.pos,
                highlight_count: 0,
                notification_count: 0,
                is_dm: false,
                is_encrypted: false,
                is_tombstoned: false,
                is_invited: false,
                name: None,
                avatar: None,
                is_expired: t.expires_at.is_some_and(|e| e <= chrono::Utc::now().timestamp_millis()),
            })
            .collect();
        entries.sort_by_key(|e| e.room_updated_ts);
        Ok(entries)
    }

    async fn count_room_token_sync(&self, _room_id: &str) -> Result<i64, sqlx::Error> {
        Ok(self.tokens.read().await.len() as i64)
    }

    async fn get_global_account_data(&self, user_id: &str) -> Result<serde_json::Value, sqlx::Error> {
        Ok(self
            .global_account_data
            .read()
            .await
            .get(user_id)
            .cloned()
            .unwrap_or(serde_json::Value::Object(Default::default())))
    }

    async fn get_room_account_data(
        &self,
        user_id: &str,
        room_ids: &[String],
    ) -> Result<serde_json::Value, sqlx::Error> {
        let data = self.room_account_data.read().await;
        let mut result = serde_json::Map::new();
        for room_id in room_ids {
            if let Some(v) = data.get(&(user_id.to_string(), room_id.clone())) {
                result.insert(room_id.clone(), v.clone());
            }
        }
        Ok(serde_json::Value::Object(result))
    }

    async fn get_receipts_for_rooms(&self, room_ids: &[String]) -> Result<serde_json::Value, sqlx::Error> {
        let data = self.receipts.read().await;
        let mut result = serde_json::Map::new();
        for room_id in room_ids {
            if let Some(v) = data.get(room_id) {
                result.insert(room_id.clone(), v.clone());
            }
        }
        Ok(serde_json::Value::Object(result))
    }

    async fn delete_connection_data(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let uid = user_id.to_string();
        let did = device_id.to_string();
        let cid = conn_id.map(|s| s.to_string());
        // Remove tokens
        self.tokens.write().await.remove(&Self::key(user_id, device_id, conn_id));
        // Remove lists
        self.lists.write().await.retain(|(u, d, c, _), _| !(*u == uid && *d == did && *c == cid));
        // Remove rooms
        self.rooms.write().await.retain(|(u, d, c, _), _| !(*u == uid && *d == did && *c == cid));
        Ok(())
    }
}

// ── InMemoryThreadStore ───────────────────────────────────────────

#[allow(clippy::type_complexity)]
#[derive(Clone, Default)]
pub struct InMemoryThreadStore {
    roots: Arc<tokio::sync::RwLock<Vec<crate::thread::ThreadRoot>>>,
    replies: Arc<tokio::sync::RwLock<Vec<crate::thread::ThreadReply>>>,
    subscriptions: Arc<tokio::sync::RwLock<HashMap<(String, String, String), crate::thread::ThreadSubscription>>>,
    read_receipts: Arc<tokio::sync::RwLock<HashMap<(String, String, String), crate::thread::ThreadReadReceipt>>>,
    relations: Arc<tokio::sync::RwLock<Vec<crate::thread::ThreadRelation>>>,
    summaries: Arc<tokio::sync::RwLock<HashMap<(String, String), crate::thread::ThreadSummary>>>,
    statistics: Arc<tokio::sync::RwLock<HashMap<(String, String), crate::thread::ThreadStatistics>>>,
    frozen: Arc<tokio::sync::RwLock<HashSet<(String, String)>>>,
    next_id: Arc<std::sync::atomic::AtomicI64>,
}

impl InMemoryThreadStore {
    pub fn new() -> Self {
        Self {
            roots: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            replies: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            subscriptions: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            read_receipts: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            relations: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            summaries: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            statistics: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            frozen: Arc::new(tokio::sync::RwLock::new(HashSet::new())),
            next_id: Arc::new(std::sync::atomic::AtomicI64::new(1)),
        }
    }

    fn next_id_val(&self) -> i64 {
        self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }
}

#[async_trait::async_trait]
impl crate::thread::ThreadStoreApi for InMemoryThreadStore {
    async fn create_thread_root(
        &self,
        params: crate::thread::CreateThreadRootParams,
    ) -> Result<crate::thread::ThreadRoot, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let root = crate::thread::ThreadRoot {
            id: self.next_id_val(),
            room_id: params.room_id.clone(),
            root_event_id: params.root_event_id.clone(),
            sender: params.sender.clone(),
            thread_id: params.thread_id.clone(),
            reply_count: 0,
            last_reply_event_id: None,
            last_reply_sender: None,
            last_reply_ts: None,
            participants: Some(serde_json::json!([params.sender])),
            is_fetched: false,
            created_ts: now,
            updated_ts: None,
        };
        self.roots.write().await.push(root.clone());
        Ok(root)
    }

    async fn get_thread_root(
        &self,
        room_id: &str,
        thread_id: &str,
    ) -> Result<Option<crate::thread::ThreadRoot>, sqlx::Error> {
        Ok(self
            .roots
            .read()
            .await
            .iter()
            .find(|r| r.room_id == room_id && r.thread_id.as_deref() == Some(thread_id))
            .cloned())
    }

    async fn get_thread_root_by_event(
        &self,
        room_id: &str,
        root_event_id: &str,
    ) -> Result<Option<crate::thread::ThreadRoot>, sqlx::Error> {
        Ok(self.roots.read().await.iter().find(|r| r.room_id == room_id && r.root_event_id == root_event_id).cloned())
    }

    async fn list_thread_roots(
        &self,
        params: crate::thread::ThreadListParams,
    ) -> Result<Vec<crate::thread::ThreadRoot>, sqlx::Error> {
        let limit = params.limit.unwrap_or(50) as usize;
        let roots = self.roots.read().await;
        let mut filtered: Vec<&crate::thread::ThreadRoot> = roots
            .iter()
            .filter(|r| r.room_id == params.room_id)
            .filter(|r| if params.include_all { true } else { !r.is_fetched })
            .filter(|r| match (&params.from, &r.thread_id) {
                (Some(from), Some(tid)) => tid.as_str() > from.as_str(),
                _ => true,
            })
            .collect();
        filtered.sort_by(|a, b| a.thread_id.as_deref().unwrap_or("").cmp(b.thread_id.as_deref().unwrap_or("")));
        Ok(filtered.into_iter().take(limit).cloned().collect())
    }

    async fn list_all_thread_roots(
        &self,
        limit: Option<i32>,
        from: Option<String>,
    ) -> Result<Vec<crate::thread::ThreadRoot>, sqlx::Error> {
        let limit = limit.unwrap_or(50) as usize;
        let roots = self.roots.read().await;
        let mut filtered: Vec<&crate::thread::ThreadRoot> = roots
            .iter()
            .filter(|r| match (&from, &r.thread_id) {
                (Some(from), Some(tid)) => tid.as_str() > from.as_str(),
                _ => true,
            })
            .collect();
        filtered.sort_by(|a, b| a.thread_id.as_deref().unwrap_or("").cmp(b.thread_id.as_deref().unwrap_or("")));
        Ok(filtered.into_iter().take(limit).cloned().collect())
    }

    async fn create_thread_reply(
        &self,
        params: crate::thread::CreateThreadReplyParams,
    ) -> Result<crate::thread::ThreadReply, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let reply = crate::thread::ThreadReply {
            id: self.next_id_val(),
            room_id: params.room_id.clone(),
            thread_id: params.thread_id.clone(),
            event_id: params.event_id.clone(),
            root_event_id: params.root_event_id.clone(),
            sender: params.sender.clone(),
            in_reply_to_event_id: params.in_reply_to_event_id.clone(),
            content: params.content.clone(),
            origin_server_ts: params.origin_server_ts,
            is_edited: false,
            is_redacted: false,
            created_ts: now,
        };
        self.replies.write().await.push(reply.clone());

        // Update the matching thread root: bump reply_count, refresh last reply
        // metadata, and merge sender into the participants JSON array.
        let mut roots = self.roots.write().await;
        for root in roots.iter_mut() {
            if root.room_id == params.room_id && root.thread_id.as_deref() == Some(params.thread_id.as_str()) {
                root.reply_count += 1;
                root.last_reply_event_id = Some(params.event_id.clone());
                root.last_reply_sender = Some(params.sender.clone());
                root.last_reply_ts = Some(params.origin_server_ts);
                root.updated_ts = Some(now);
                let mut parts: Vec<String> = root
                    .participants
                    .as_ref()
                    .and_then(|v| serde_json::from_str(v.to_string().as_str()).ok())
                    .unwrap_or_default();
                if !parts.iter().any(|p| p == &params.sender) {
                    parts.push(params.sender.clone());
                }
                root.participants =
                    Some(serde_json::Value::Array(parts.into_iter().map(serde_json::Value::String).collect()));
                break;
            }
        }
        Ok(reply)
    }

    async fn get_thread_replies(
        &self,
        room_id: &str,
        thread_id: &str,
        limit: Option<i32>,
        from: Option<String>,
    ) -> Result<Vec<crate::thread::ThreadReply>, sqlx::Error> {
        let limit = limit.unwrap_or(50) as usize;
        let replies = self.replies.read().await;
        let mut filtered: Vec<&crate::thread::ThreadReply> = replies
            .iter()
            .filter(|r| r.room_id == room_id && r.thread_id == thread_id)
            .filter(|r| match &from {
                Some(from) => r.event_id.as_str() > from.as_str(),
                None => true,
            })
            .collect();
        filtered.sort_by(|a, b| a.event_id.cmp(&b.event_id));
        Ok(filtered.into_iter().take(limit).cloned().collect())
    }

    async fn get_reply_count(&self, room_id: &str, thread_id: &str) -> Result<i32, sqlx::Error> {
        let count =
            self.replies.read().await.iter().filter(|r| r.room_id == room_id && r.thread_id == thread_id).count()
                as i32;
        Ok(count)
    }

    async fn get_thread_participants(&self, room_id: &str, thread_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let roots = self.roots.read().await;
        let root = roots.iter().find(|r| r.room_id == room_id && r.thread_id.as_deref() == Some(thread_id));
        Ok(root
            .and_then(|r| r.participants.as_ref())
            .and_then(|v| {
                v.as_array().map(|arr| arr.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
            })
            .unwrap_or_default())
    }

    async fn subscribe_to_thread(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
        notification_level: &str,
    ) -> Result<crate::thread::ThreadSubscription, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let sub = crate::thread::ThreadSubscription {
            id: self.next_id_val(),
            room_id: room_id.to_string(),
            thread_id: thread_id.to_string(),
            user_id: user_id.to_string(),
            notification_level: notification_level.to_string(),
            is_muted: false,
            is_pinned: false,
            subscribed_ts: now,
            updated_ts: now,
        };
        self.subscriptions
            .write()
            .await
            .insert((room_id.to_string(), thread_id.to_string(), user_id.to_string()), sub.clone());
        Ok(sub)
    }

    async fn unsubscribe_from_thread(&self, room_id: &str, thread_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
        self.subscriptions.write().await.remove(&(room_id.to_string(), thread_id.to_string(), user_id.to_string()));
        Ok(())
    }

    async fn mute_thread(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
    ) -> Result<crate::thread::ThreadSubscription, sqlx::Error> {
        let mut subs = self.subscriptions.write().await;
        let key = (room_id.to_string(), thread_id.to_string(), user_id.to_string());
        let sub = subs.entry(key).or_insert_with(|| {
            let now = chrono::Utc::now().timestamp_millis();
            crate::thread::ThreadSubscription {
                id: 0,
                room_id: room_id.to_string(),
                thread_id: thread_id.to_string(),
                user_id: user_id.to_string(),
                notification_level: "none".to_string(),
                is_muted: false,
                is_pinned: false,
                subscribed_ts: now,
                updated_ts: now,
            }
        });
        sub.is_muted = true;
        sub.updated_ts = chrono::Utc::now().timestamp_millis();
        Ok(sub.clone())
    }

    async fn get_thread_subscription(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
    ) -> Result<Option<crate::thread::ThreadSubscription>, sqlx::Error> {
        Ok(self
            .subscriptions
            .read()
            .await
            .get(&(room_id.to_string(), thread_id.to_string(), user_id.to_string()))
            .cloned())
    }

    async fn get_user_thread_subscriptions(
        &self,
        user_id: &str,
        limit: Option<i32>,
    ) -> Result<Vec<crate::thread::ThreadSubscription>, sqlx::Error> {
        let limit = limit.unwrap_or(50) as usize;
        let subs = self.subscriptions.read().await;
        let mut filtered: Vec<&crate::thread::ThreadSubscription> =
            subs.values().filter(|s| s.user_id == user_id).collect();
        filtered.sort_by(|a, b| b.subscribed_ts.cmp(&a.subscribed_ts));
        Ok(filtered.into_iter().take(limit).cloned().collect())
    }

    async fn update_read_receipt(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
        event_id: &str,
        origin_server_ts: i64,
    ) -> Result<crate::thread::ThreadReadReceipt, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let key = (room_id.to_string(), thread_id.to_string(), user_id.to_string());
        let mut receipts = self.read_receipts.write().await;
        let receipt = receipts.entry(key).or_insert_with(|| crate::thread::ThreadReadReceipt {
            id: 0,
            room_id: room_id.to_string(),
            thread_id: thread_id.to_string(),
            user_id: user_id.to_string(),
            last_read_event_id: None,
            last_read_ts: now,
            unread_count: 0,
            updated_ts: now,
        });
        receipt.id = if receipt.id == 0 { self.next_id_val() } else { receipt.id };
        receipt.last_read_event_id = Some(event_id.to_string());
        receipt.last_read_ts = origin_server_ts;
        receipt.unread_count = 0;
        receipt.updated_ts = now;
        Ok(receipt.clone())
    }

    async fn get_read_receipt(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
    ) -> Result<Option<crate::thread::ThreadReadReceipt>, sqlx::Error> {
        Ok(self
            .read_receipts
            .read()
            .await
            .get(&(room_id.to_string(), thread_id.to_string(), user_id.to_string()))
            .cloned())
    }

    async fn increment_unread_count(&self, room_id: &str, thread_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let key = (room_id.to_string(), thread_id.to_string(), user_id.to_string());
        let mut receipts = self.read_receipts.write().await;
        let receipt = receipts.entry(key).or_insert_with(|| crate::thread::ThreadReadReceipt {
            id: self.next_id_val(),
            room_id: room_id.to_string(),
            thread_id: thread_id.to_string(),
            user_id: user_id.to_string(),
            last_read_event_id: None,
            last_read_ts: now,
            unread_count: 0,
            updated_ts: now,
        });
        receipt.unread_count += 1;
        receipt.updated_ts = now;
        Ok(())
    }

    async fn create_thread_relation(
        &self,
        room_id: &str,
        event_id: &str,
        relates_to_event_id: &str,
        relation_type: &str,
        thread_id: Option<&str>,
        is_falling_back: bool,
    ) -> Result<crate::thread::ThreadRelation, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let relation = crate::thread::ThreadRelation {
            id: self.next_id_val(),
            room_id: room_id.to_string(),
            event_id: event_id.to_string(),
            relates_to_event_id: relates_to_event_id.to_string(),
            relation_type: relation_type.to_string(),
            thread_id: thread_id.map(|s| s.to_string()),
            is_falling_back,
            created_ts: now,
        };
        self.relations.write().await.push(relation.clone());
        Ok(relation)
    }

    async fn mark_reply_edited(&self, room_id: &str, event_id: &str) -> Result<(), sqlx::Error> {
        let mut replies = self.replies.write().await;
        for reply in replies.iter_mut() {
            if reply.room_id == room_id && reply.event_id == event_id {
                reply.is_edited = true;
            }
        }
        Ok(())
    }

    async fn mark_reply_redacted(&self, room_id: &str, event_id: &str) -> Result<(), sqlx::Error> {
        let mut replies = self.replies.write().await;
        for reply in replies.iter_mut() {
            if reply.room_id == room_id && reply.event_id == event_id {
                reply.is_redacted = true;
            }
        }
        Ok(())
    }

    async fn delete_thread(&self, room_id: &str, thread_id: &str) -> Result<(), sqlx::Error> {
        // Remove thread root(s) matching the (room_id, thread_id) pair.
        self.roots.write().await.retain(|r| !(r.room_id == room_id && r.thread_id.as_deref() == Some(thread_id)));
        // Remove all replies belonging to the thread.
        self.replies.write().await.retain(|r| !(r.room_id == room_id && r.thread_id == thread_id));
        // Remove all subscriptions for the thread.
        self.subscriptions.write().await.retain(|(rid, tid, _), _| !(rid == room_id && tid == thread_id));
        // Remove read receipts for the thread.
        self.read_receipts.write().await.retain(|(rid, tid, _), _| !(rid == room_id && tid == thread_id));
        // Remove thread relations associated with the thread.
        self.relations.write().await.retain(|r| !(r.room_id == room_id && r.thread_id.as_deref() == Some(thread_id)));
        // Remove cached summary/statistics entries.
        self.summaries.write().await.remove(&(room_id.to_string(), thread_id.to_string()));
        self.statistics.write().await.remove(&(room_id.to_string(), thread_id.to_string()));
        // Unfreeze if frozen.
        self.frozen.write().await.remove(&(room_id.to_string(), thread_id.to_string()));
        Ok(())
    }

    async fn get_threads_with_unread(
        &self,
        user_id: &str,
        room_id: Option<&str>,
    ) -> Result<Vec<crate::thread::ThreadReadReceipt>, sqlx::Error> {
        let receipts = self.read_receipts.read().await;
        Ok(receipts
            .values()
            .filter(|r| r.user_id == user_id && r.unread_count > 0)
            .filter(|r| match room_id {
                Some(rid) => r.room_id == rid,
                None => true,
            })
            .cloned()
            .collect())
    }

    async fn get_thread_summary(
        &self,
        room_id: &str,
        thread_id: &str,
    ) -> Result<Option<crate::thread::ThreadSummary>, sqlx::Error> {
        Ok(self.summaries.read().await.get(&(room_id.to_string(), thread_id.to_string())).cloned())
    }

    async fn get_thread_statistics(
        &self,
        room_id: &str,
        thread_id: &str,
    ) -> Result<Option<crate::thread::ThreadStatistics>, sqlx::Error> {
        Ok(self.statistics.read().await.get(&(room_id.to_string(), thread_id.to_string())).cloned())
    }

    async fn search_threads(
        &self,
        room_id: &str,
        query: &str,
        limit: Option<i32>,
    ) -> Result<Vec<crate::thread::ThreadSummary>, sqlx::Error> {
        let limit = limit.unwrap_or(50) as usize;
        let summaries = self.summaries.read().await;
        let q = query.to_lowercase();
        Ok(summaries
            .values()
            .filter(|s| s.room_id == room_id)
            .filter(|s| {
                s.thread_id.to_lowercase().contains(&q)
                    || s.root_event_id.to_lowercase().contains(&q)
                    || s.root_sender.to_lowercase().contains(&q)
            })
            .take(limit)
            .cloned()
            .collect())
    }

    async fn freeze_thread(&self, room_id: &str, thread_id: &str) -> Result<(), sqlx::Error> {
        self.frozen.write().await.insert((room_id.to_string(), thread_id.to_string()));
        Ok(())
    }

    async fn unfreeze_thread(&self, room_id: &str, thread_id: &str) -> Result<(), sqlx::Error> {
        self.frozen.write().await.remove(&(room_id.to_string(), thread_id.to_string()));
        Ok(())
    }
}

// ── InMemorySpaceStore ───────────────────────────────────────────

#[allow(clippy::type_complexity)]
#[derive(Clone, Default)]
pub struct InMemorySpaceStore {
    spaces: Arc<tokio::sync::RwLock<HashMap<String, crate::space::Space>>>,
    children: Arc<tokio::sync::RwLock<Vec<crate::space::SpaceChild>>>,
    members: Arc<tokio::sync::RwLock<HashMap<(String, String), crate::space::SpaceMember>>>,
    summaries: Arc<tokio::sync::RwLock<HashMap<String, crate::space::SpaceSummary>>>,
    events: Arc<tokio::sync::RwLock<Vec<crate::space::SpaceEvent>>>,
    next_id: Arc<std::sync::atomic::AtomicI64>,
}

impl InMemorySpaceStore {
    pub fn new() -> Self {
        Self {
            spaces: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            children: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            members: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            summaries: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            events: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            next_id: Arc::new(std::sync::atomic::AtomicI64::new(1)),
        }
    }
}

#[async_trait::async_trait]
impl crate::space::SpaceStoreApi for InMemorySpaceStore {
    async fn create_space(
        &self,
        request: crate::space::CreateSpaceRequest,
    ) -> Result<crate::space::Space, sqlx::Error> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let now = chrono::Utc::now().timestamp_millis();
        let server_name = request.room_id.split(':').next_back().unwrap_or("localhost");
        let space_id = format!("!space_{}:{}", id, server_name);
        let creator = request.creator.clone();
        let space = crate::space::Space {
            space_id: space_id.clone(),
            room_id: request.room_id,
            name: request.name,
            topic: request.topic,
            avatar_url: request.avatar_url,
            creator: creator.clone(),
            join_rule: request.join_rule.unwrap_or_else(|| "invite".to_string()),
            visibility: Some(request.visibility.unwrap_or_else(|| "private".to_string())),
            created_ts: now,
            updated_ts: None,
            is_public: request.is_public.unwrap_or(false),
            parent_space_id: request.parent_space_id,
            room_type: None,
        };
        self.spaces.write().await.insert(space_id.clone(), space.clone());
        // Mirror production: creator is added as the first space member.
        self.add_space_member(&space_id, &creator, "join", None).await?;
        Ok(space)
    }

    async fn get_space(&self, space_id: &str) -> Result<Option<crate::space::Space>, sqlx::Error> {
        Ok(self.spaces.read().await.get(space_id).cloned())
    }

    async fn get_space_by_room(&self, room_id: &str) -> Result<Option<crate::space::Space>, sqlx::Error> {
        Ok(self.spaces.read().await.values().find(|s| s.room_id == room_id).cloned())
    }

    async fn get_spaces_by_rooms_batch(
        &self,
        room_ids: &[String],
    ) -> Result<HashMap<String, crate::space::Space>, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let spaces = self.spaces.read().await;
        let mut map = HashMap::with_capacity(room_ids.len());
        for space in spaces.values() {
            if room_ids.iter().any(|rid| rid == &space.room_id) {
                map.insert(space.room_id.clone(), space.clone());
            }
        }
        Ok(map)
    }

    async fn update_space(
        &self,
        space_id: &str,
        request: &crate::space::UpdateSpaceRequest,
    ) -> Result<crate::space::Space, sqlx::Error> {
        let mut spaces = self.spaces.write().await;
        let space = spaces.get_mut(space_id).ok_or(sqlx::Error::RowNotFound)?;
        if let Some(v) = &request.name {
            space.name = Some(v.clone());
        }
        if let Some(v) = &request.topic {
            space.topic = Some(v.clone());
        }
        if let Some(v) = &request.avatar_url {
            space.avatar_url = Some(v.clone());
        }
        if let Some(v) = &request.join_rule {
            space.join_rule = v.clone();
        }
        if let Some(v) = &request.visibility {
            space.visibility = Some(v.clone());
        }
        if let Some(v) = request.is_public {
            space.is_public = v;
        }
        space.updated_ts = Some(chrono::Utc::now().timestamp_millis());
        Ok(space.clone())
    }

    async fn delete_space(&self, space_id: &str) -> Result<(), sqlx::Error> {
        self.spaces.write().await.remove(space_id);
        Ok(())
    }

    async fn add_child(&self, request: crate::space::AddChildRequest) -> Result<crate::space::SpaceChild, sqlx::Error> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let now = chrono::Utc::now().timestamp_millis();
        let child = crate::space::SpaceChild {
            id,
            space_id: request.space_id,
            room_id: request.room_id,
            sender: request.sender,
            is_suggested: request.is_suggested,
            via_servers: request.via_servers,
            added_ts: now,
            order: None,
            suggested: Some(request.is_suggested),
            added_by: None,
            removed_ts: None,
        };
        self.children.write().await.push(child.clone());
        Ok(child)
    }

    async fn remove_child(&self, space_id: &str, room_id: &str) -> Result<(), sqlx::Error> {
        let mut children = self.children.write().await;
        children.retain(|c| !(c.space_id == space_id && c.room_id == room_id));
        Ok(())
    }

    async fn get_space_children(&self, space_id: &str) -> Result<Vec<crate::space::SpaceChild>, sqlx::Error> {
        Ok(self.children.read().await.iter().filter(|c| c.space_id == space_id).cloned().collect())
    }

    async fn get_child_spaces(&self, room_id: &str) -> Result<Vec<crate::space::SpaceChild>, sqlx::Error> {
        Ok(self.children.read().await.iter().filter(|c| c.room_id == room_id).cloned().collect())
    }

    async fn add_space_member(
        &self,
        space_id: &str,
        user_id: &str,
        membership: &str,
        inviter: Option<&str>,
    ) -> Result<crate::space::SpaceMember, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let member = crate::space::SpaceMember {
            space_id: space_id.to_string(),
            user_id: user_id.to_string(),
            membership: membership.to_string(),
            joined_ts: now,
            updated_ts: Some(now),
            left_ts: None,
            inviter: inviter.map(|s| s.to_string()),
        };
        self.members.write().await.insert((space_id.to_string(), user_id.to_string()), member.clone());
        Ok(member)
    }

    async fn remove_space_member(&self, space_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
        self.members.write().await.remove(&(space_id.to_string(), user_id.to_string()));
        Ok(())
    }

    async fn get_space_members(&self, space_id: &str) -> Result<Vec<crate::space::SpaceMember>, sqlx::Error> {
        Ok(self.members.read().await.values().filter(|m| m.space_id == space_id).cloned().collect())
    }

    async fn get_space_member(
        &self,
        space_id: &str,
        user_id: &str,
    ) -> Result<Option<crate::space::SpaceMember>, sqlx::Error> {
        Ok(self.members.read().await.get(&(space_id.to_string(), user_id.to_string())).cloned())
    }

    async fn get_user_spaces(&self, user_id: &str) -> Result<Vec<crate::space::Space>, sqlx::Error> {
        let members = self.members.read().await;
        let spaces = self.spaces.read().await;
        let mut result = Vec::new();
        for m in members.values() {
            if m.user_id == user_id && m.membership == "join" {
                if let Some(space) = spaces.get(&m.space_id) {
                    result.push(space.clone());
                }
            }
        }
        Ok(result)
    }

    async fn get_public_spaces(
        &self,
        limit: i64,
        cursor_created_ts: Option<i64>,
        cursor_space_id: Option<&str>,
    ) -> Result<Vec<crate::space::Space>, sqlx::Error> {
        let spaces = self.spaces.read().await;
        let mut public: Vec<crate::space::Space> = spaces.values().filter(|s| s.is_public).cloned().collect();
        public.sort_by(|a, b| b.created_ts.cmp(&a.created_ts).then(a.space_id.cmp(&b.space_id)));
        let mut result = Vec::new();
        for s in public {
            if let (Some(ts), Some(sid)) = (cursor_created_ts, cursor_space_id) {
                if s.created_ts > ts || (s.created_ts == ts && s.space_id.as_str() <= sid) {
                    continue;
                }
            }
            if result.len() as i64 >= limit {
                break;
            }
            result.push(s);
        }
        Ok(result)
    }

    async fn get_space_hierarchy(
        &self,
        space_id: &str,
        _max_depth: i32,
    ) -> Result<crate::space::SpaceHierarchy, sqlx::Error> {
        let space = self.spaces.read().await.get(space_id).cloned().ok_or(sqlx::Error::RowNotFound)?;
        let children: Vec<_> = self.children.read().await.iter().filter(|c| c.space_id == space_id).cloned().collect();
        let members: Vec<_> = self.members.read().await.values().filter(|m| m.space_id == space_id).cloned().collect();
        Ok(crate::space::SpaceHierarchy { space, children, members })
    }

    async fn get_space_summary(&self, space_id: &str) -> Result<Option<crate::space::SpaceSummary>, sqlx::Error> {
        Ok(self.summaries.read().await.get(space_id).cloned())
    }

    async fn update_space_summary(&self, space_id: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let children_count = self.children.read().await.iter().filter(|c| c.space_id == space_id).count() as i64;
        let member_count =
            self.members.read().await.values().filter(|m| m.space_id == space_id && m.membership == "join").count()
                as i64;
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let summary = crate::space::SpaceSummary {
            id,
            space_id: space_id.to_string(),
            summary: serde_json::json!({}),
            children_count,
            member_count,
            updated_ts: now,
        };
        self.summaries.write().await.insert(space_id.to_string(), summary);
        Ok(())
    }

    async fn add_space_event(
        &self,
        event_id: &str,
        space_id: &str,
        event_type: &str,
        sender: &str,
        content: serde_json::Value,
        state_key: Option<&str>,
    ) -> Result<crate::space::SpaceEvent, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let event = crate::space::SpaceEvent {
            event_id: event_id.to_string(),
            space_id: space_id.to_string(),
            event_type: event_type.to_string(),
            sender: sender.to_string(),
            content,
            state_key: state_key.map(|s| s.to_string()),
            origin_server_ts: now,
            processed_ts: Some(now),
        };
        self.events.write().await.push(event.clone());
        Ok(event)
    }

    async fn get_space_events(
        &self,
        space_id: &str,
        event_type: Option<&str>,
        limit: i64,
    ) -> Result<Vec<crate::space::SpaceEvent>, sqlx::Error> {
        let mut result: Vec<crate::space::SpaceEvent> = self
            .events
            .read()
            .await
            .iter()
            .filter(|e| e.space_id == space_id && event_type.is_none_or(|t| e.event_type == t))
            .cloned()
            .collect();
        result.sort_by(|a, b| b.origin_server_ts.cmp(&a.origin_server_ts));
        result.truncate(limit as usize);
        Ok(result)
    }

    async fn search_spaces(
        &self,
        query: &str,
        limit: i64,
        user_id: Option<&str>,
    ) -> Result<Vec<crate::space::Space>, sqlx::Error> {
        let _ = user_id;
        let spaces = self.spaces.read().await;
        let q = query.to_lowercase();
        let mut result: Vec<crate::space::Space> = spaces
            .values()
            .filter(|s| {
                s.name.as_ref().is_some_and(|n| n.to_lowercase().contains(&q))
                    || s.topic.as_ref().is_some_and(|t| t.to_lowercase().contains(&q))
            })
            .cloned()
            .collect();
        result.truncate(limit as usize);
        Ok(result)
    }

    async fn is_space_member(&self, space_id: &str, user_id: &str) -> Result<bool, sqlx::Error> {
        Ok(self
            .members
            .read()
            .await
            .get(&(space_id.to_string(), user_id.to_string()))
            .is_some_and(|m| m.membership == "join"))
    }

    async fn get_space_statistics(&self, _limit: i64) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        Ok(Vec::new())
    }

    async fn get_recursive_hierarchy(
        &self,
        _space_id: &str,
        _max_depth: i32,
        _suggested_only: bool,
    ) -> Result<Vec<crate::space::SpaceChildInfo>, sqlx::Error> {
        Ok(Vec::new())
    }

    async fn get_space_hierarchy_paginated(
        &self,
        _space_id: &str,
        _max_depth: i32,
        _suggested_only: bool,
        _limit: Option<i32>,
        _from: Option<&str>,
    ) -> Result<crate::space::SpaceHierarchyResponse, sqlx::Error> {
        Ok(crate::space::SpaceHierarchyResponse { rooms: Vec::new(), next_batch: None })
    }

    async fn check_user_can_see_space(&self, space_id: &str, user_id: &str) -> Result<bool, sqlx::Error> {
        let spaces = self.spaces.read().await;
        if let Some(space) = spaces.get(space_id) {
            if space.is_public {
                return Ok(true);
            }
        }
        Ok(self
            .members
            .read()
            .await
            .get(&(space_id.to_string(), user_id.to_string()))
            .is_some_and(|m| m.membership == "join"))
    }

    async fn get_parent_spaces(&self, room_id: &str) -> Result<Vec<crate::space::Space>, sqlx::Error> {
        let children = self.children.read().await;
        let spaces = self.spaces.read().await;
        let mut result = Vec::new();
        for child in children.iter().filter(|c| c.room_id == room_id) {
            if let Some(space) = spaces.get(&child.space_id) {
                result.push(space.clone());
            }
        }
        Ok(result)
    }

    async fn get_space_tree_path(&self, space_id: &str) -> Result<Vec<crate::space::Space>, sqlx::Error> {
        let spaces = self.spaces.read().await;
        let mut path = Vec::new();
        let mut current = spaces.get(space_id).cloned();
        while let Some(space) = current {
            path.push(space.clone());
            current = space.parent_space_id.as_ref().and_then(|pid| spaces.get(pid).cloned());
        }
        path.reverse();
        Ok(path)
    }

    async fn resolve_space_id(&self, identifier: &str) -> Result<Option<String>, sqlx::Error> {
        let spaces = self.spaces.read().await;
        if spaces.get(identifier).is_some() {
            return Ok(Some(identifier.to_string()));
        }
        Ok(spaces.values().find(|s| s.room_id == identifier).map(|s| s.space_id.clone()))
    }

    async fn get_all_spaces_for_admin(&self) -> Result<Vec<crate::space::Space>, sqlx::Error> {
        Ok(self.spaces.read().await.values().cloned().collect())
    }

    async fn get_space_by_identifier(&self, identifier: &str) -> Result<Option<crate::space::Space>, sqlx::Error> {
        let spaces = self.spaces.read().await;
        if let Some(space) = spaces.get(identifier) {
            return Ok(Some(space.clone()));
        }
        Ok(spaces.values().find(|s| s.room_id == identifier).cloned())
    }

    async fn get_space_user_ids(&self, space_id: &str) -> Result<Vec<String>, sqlx::Error> {
        Ok(self
            .members
            .read()
            .await
            .values()
            .filter(|m| m.space_id == space_id && m.membership == "join")
            .map(|m| m.user_id.clone())
            .collect())
    }

    async fn get_space_room_ids(&self, space_id: &str) -> Result<Vec<String>, sqlx::Error> {
        Ok(self.children.read().await.iter().filter(|c| c.space_id == space_id).map(|c| c.room_id.clone()).collect())
    }

    async fn get_space_member_and_child_count(&self, space_id: &str) -> Result<(i64, i64), sqlx::Error> {
        let member_count =
            self.members.read().await.values().filter(|m| m.space_id == space_id && m.membership == "join").count()
                as i64;
        let child_count = self.children.read().await.iter().filter(|c| c.space_id == space_id).count() as i64;
        Ok((member_count, child_count))
    }

    async fn delete_space_returning_count(&self, space_id: &str) -> Result<u64, sqlx::Error> {
        let removed = self.spaces.write().await.remove(space_id);
        Ok(if removed.is_some() { 1 } else { 0 })
    }

    async fn get_space_children_paginated(
        &self,
        space_id: &str,
        limit: i64,
        from_added_ts: Option<i64>,
        from_id: Option<i64>,
    ) -> Result<Vec<crate::space::SpaceChild>, sqlx::Error> {
        let mut children: Vec<crate::space::SpaceChild> =
            self.children.read().await.iter().filter(|c| c.space_id == space_id).cloned().collect();
        children.sort_by(|a, b| a.added_ts.cmp(&b.added_ts).then(a.id.cmp(&b.id)));
        let mut result = Vec::new();
        for c in children {
            if let (Some(ts), Some(id)) = (from_added_ts, from_id) {
                if c.added_ts < ts || (c.added_ts == ts && c.id <= id) {
                    continue;
                }
            }
            if result.len() as i64 >= limit {
                break;
            }
            result.push(c);
        }
        Ok(result)
    }

    async fn get_space_members_paginated(
        &self,
        space_id: &str,
        limit: i64,
        from_joined_ts: Option<i64>,
        from_user_id: Option<&str>,
    ) -> Result<Vec<crate::space::SpaceMember>, sqlx::Error> {
        let mut members: Vec<crate::space::SpaceMember> = self
            .members
            .read()
            .await
            .values()
            .filter(|m| m.space_id == space_id && m.membership == "join")
            .cloned()
            .collect();
        members.sort_by(|a, b| a.joined_ts.cmp(&b.joined_ts).then(a.user_id.cmp(&b.user_id)));
        let mut result = Vec::new();
        for m in members {
            if let (Some(ts), Some(uid)) = (from_joined_ts, from_user_id) {
                if m.joined_ts < ts || (m.joined_ts == ts && m.user_id.as_str() <= uid) {
                    continue;
                }
            }
            if result.len() as i64 >= limit {
                break;
            }
            result.push(m);
        }
        Ok(result)
    }
}

// ── InMemoryRegistrationTokenStore ────────────────────────────────────

#[derive(Clone, Default)]
pub struct InMemoryRegistrationTokenStore {
    tokens: Arc<RwLock<HashMap<String, crate::registration_token::RegistrationToken>>>,
    token_by_id: Arc<RwLock<HashMap<i64, crate::registration_token::RegistrationToken>>>,
    usage: Arc<RwLock<HashMap<i64, Vec<crate::registration_token::RegistrationTokenUsage>>>>,
    invites: Arc<RwLock<HashMap<String, crate::registration_token::RoomInvite>>>,
    batches: Arc<RwLock<HashMap<String, crate::registration_token::RegistrationTokenBatch>>>,
    next_id: Arc<std::sync::atomic::AtomicI64>,
}

impl InMemoryRegistrationTokenStore {
    pub fn new() -> Self {
        Self { next_id: Arc::new(std::sync::atomic::AtomicI64::new(1)), ..Default::default() }
    }
}

#[async_trait::async_trait]
impl crate::registration_token::RegistrationTokenStoreApi for InMemoryRegistrationTokenStore {
    async fn create_token(
        &self,
        request: crate::registration_token::CreateRegistrationTokenRequest,
    ) -> Result<crate::registration_token::RegistrationToken, sqlx::Error> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let now = chrono::Utc::now().timestamp_millis();
        let token_str = request.token.unwrap_or_else(|| {
            use rand::Rng;
            let mut rng = rand::rng();
            let chars: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghjkmnpqrstuvwxyz23456789";
            (0..32).map(|_| chars[rng.random_range(0..chars.len())] as char).collect()
        });
        let token_type = request.token_type.unwrap_or_else(|| "single_use".to_string());
        let t = crate::registration_token::RegistrationToken {
            id,
            token: token_str.clone(),
            token_type,
            description: request.description,
            max_uses: request.max_uses.unwrap_or(1),
            uses_count: 0,
            is_used: false,
            is_enabled: true,
            expires_at: request.expires_at,
            created_by: request.created_by,
            created_ts: now,
            updated_ts: now,
            last_used_ts: None,
            allowed_email_domains: request.allowed_email_domains,
            allowed_user_ids: request.allowed_user_ids,
            auto_join_rooms: request.auto_join_rooms,
            display_name: request.display_name,
            email: request.email,
        };
        self.tokens.write().await.insert(token_str, t.clone());
        self.token_by_id.write().await.insert(id, t.clone());
        Ok(t)
    }

    async fn get_token(
        &self,
        token: &str,
    ) -> Result<Option<crate::registration_token::RegistrationToken>, sqlx::Error> {
        Ok(self.tokens.read().await.get(token).cloned())
    }

    async fn get_token_by_id(
        &self,
        id: i64,
    ) -> Result<Option<crate::registration_token::RegistrationToken>, sqlx::Error> {
        Ok(self.token_by_id.read().await.get(&id).cloned())
    }

    async fn update_token(
        &self,
        id: i64,
        request: crate::registration_token::UpdateRegistrationTokenRequest,
    ) -> Result<crate::registration_token::RegistrationToken, sqlx::Error> {
        let mut by_id = self.token_by_id.write().await;
        let t = by_id.get_mut(&id).ok_or_else(|| sqlx::Error::RowNotFound)?;
        if let Some(desc) = request.description {
            t.description = Some(desc);
        }
        if let Some(mu) = request.max_uses {
            t.max_uses = mu;
        }
        if let Some(enabled) = request.is_enabled {
            t.is_enabled = enabled;
        }
        if let Some(exp) = request.expires_at {
            t.expires_at = Some(exp);
        }
        t.updated_ts = chrono::Utc::now().timestamp_millis();
        let result = t.clone();
        // Also update in tokens map
        self.tokens.write().await.insert(t.token.clone(), t.clone());
        Ok(result)
    }

    async fn delete_token(&self, id: i64) -> Result<(), sqlx::Error> {
        let t = { self.token_by_id.read().await.get(&id).cloned() };
        if let Some(t) = t {
            self.tokens.write().await.remove(&t.token);
            self.token_by_id.write().await.remove(&id);
        }
        Ok(())
    }

    async fn validate_token(
        &self,
        token: &str,
    ) -> Result<crate::registration_token::TokenValidationResult, sqlx::Error> {
        let t = self.tokens.read().await.get(token).cloned();
        match t {
            None => Ok(crate::registration_token::TokenValidationResult {
                is_valid: false,
                token_id: None,
                error_message: Some("Token not found".to_string()),
            }),
            Some(t) => {
                if !t.is_enabled {
                    return Ok(crate::registration_token::TokenValidationResult {
                        is_valid: false,
                        token_id: Some(t.id),
                        error_message: Some("Token is not active".to_string()),
                    });
                }
                if t.is_used && t.token_type == "single_use" {
                    return Ok(crate::registration_token::TokenValidationResult {
                        is_valid: false,
                        token_id: Some(t.id),
                        error_message: Some("Token has already been used".to_string()),
                    });
                }
                if t.max_uses > 0 && t.uses_count >= t.max_uses {
                    return Ok(crate::registration_token::TokenValidationResult {
                        is_valid: false,
                        token_id: Some(t.id),
                        error_message: Some("Token has reached maximum uses".to_string()),
                    });
                }
                if let Some(exp) = t.expires_at {
                    if exp < chrono::Utc::now().timestamp_millis() {
                        return Ok(crate::registration_token::TokenValidationResult {
                            is_valid: false,
                            token_id: Some(t.id),
                            error_message: Some("Token has expired".to_string()),
                        });
                    }
                }
                Ok(crate::registration_token::TokenValidationResult {
                    is_valid: true,
                    token_id: Some(t.id),
                    error_message: None,
                })
            }
        }
    }

    async fn use_token(
        &self,
        token: &str,
        user_id: &str,
        username: Option<&str>,
        email: Option<&str>,
        ip_address: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<bool, sqlx::Error> {
        let validation = self.validate_token(token).await?;
        if !validation.is_valid {
            return Ok(false);
        }
        let token_id = validation.token_id.unwrap_or(0);
        let mut by_id = self.token_by_id.write().await;
        if let Some(t) = by_id.get_mut(&token_id) {
            t.uses_count += 1;
            if t.token_type == "single_use" {
                t.is_used = true;
            }
            t.last_used_ts = Some(chrono::Utc::now().timestamp_millis());
            self.tokens.write().await.insert(t.token.clone(), t.clone());
        }
        let usage = crate::registration_token::RegistrationTokenUsage {
            id: self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            token_id,
            token: token.to_string(),
            user_id: user_id.to_string(),
            username: username.map(|s| s.to_string()),
            email: email.map(|s| s.to_string()),
            ip_address: ip_address.map(|s| s.to_string()),
            user_agent: user_agent.map(|s| s.to_string()),
            used_ts: chrono::Utc::now().timestamp_millis(),
            is_success: true,
            error_message: None,
        };
        self.usage.write().await.entry(token_id).or_default().push(usage);
        Ok(true)
    }

    async fn get_all_tokens(
        &self,
        limit: i64,
        from: Option<crate::registration_token::RegistrationTokenCursor>,
    ) -> Result<(Vec<crate::registration_token::RegistrationToken>, Option<String>), sqlx::Error> {
        let tokens = self.token_by_id.read().await;
        let mut all: Vec<_> = tokens.values().cloned().collect();
        all.sort_by_key(|t| std::cmp::Reverse((t.created_ts, t.id)));
        let mut iter = all.into_iter();
        if let Some(cursor) = from {
            iter = iter
                .skip_while(|t| {
                    t.created_ts > cursor.created_ts || (t.created_ts == cursor.created_ts && t.id >= cursor.id)
                })
                .collect::<Vec<_>>()
                .into_iter();
        }
        let rows: Vec<_> = iter.take((limit + 1) as usize).collect();
        let cursor = if rows.len() > limit as usize {
            rows.get(limit as usize).map(|t| {
                crate::registration_token::encode_registration_token_cursor(
                    &crate::registration_token::RegistrationTokenCursor { created_ts: t.created_ts, id: t.id },
                )
            })
        } else {
            None
        };
        Ok((rows.into_iter().take(limit as usize).collect(), cursor))
    }

    async fn get_active_tokens(&self) -> Result<Vec<crate::registration_token::RegistrationToken>, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        Ok(self
            .tokens
            .read()
            .await
            .values()
            .filter(|t| {
                t.is_enabled && t.expires_at.is_none_or(|e| e > now) && (t.max_uses == 0 || t.uses_count < t.max_uses)
            })
            .cloned()
            .collect())
    }

    async fn get_token_usage(
        &self,
        token_id: i64,
    ) -> Result<Vec<crate::registration_token::RegistrationTokenUsage>, sqlx::Error> {
        Ok(self.usage.read().await.get(&token_id).cloned().unwrap_or_default())
    }

    async fn deactivate_token(&self, id: i64) -> Result<(), sqlx::Error> {
        if let Some(t) = self.token_by_id.write().await.get_mut(&id) {
            t.is_enabled = false;
            self.tokens.write().await.insert(t.token.clone(), t.clone());
        }
        Ok(())
    }

    async fn cleanup_expired_tokens(&self) -> Result<i64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut count = 0i64;
        for (_, t) in self.token_by_id.write().await.iter_mut() {
            if t.is_enabled && t.expires_at.is_some_and(|e| e < now) {
                t.is_enabled = false;
                count += 1;
            }
        }
        Ok(count)
    }

    async fn create_room_invite(
        &self,
        request: crate::registration_token::CreateRoomInviteRequest,
    ) -> Result<crate::registration_token::RoomInvite, sqlx::Error> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let now = chrono::Utc::now().timestamp_millis();
        let invite_code: String = {
            use rand::Rng;
            let mut rng = rand::rng();
            let chars: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghjkmnpqrstuvwxyz23456789";
            (0..32).map(|_| chars[rng.random_range(0..chars.len())] as char).collect()
        };
        let invite = crate::registration_token::RoomInvite {
            id,
            invite_code: invite_code.clone(),
            room_id: request.room_id,
            inviter_user_id: request.inviter_user_id,
            invitee_email: request.invitee_email,
            invitee_user_id: None,
            is_used: false,
            is_revoked: false,
            expires_at: request.expires_at,
            created_ts: now,
            used_ts: None,
            revoked_at: None,
            revoked_reason: None,
        };
        self.invites.write().await.insert(invite_code, invite.clone());
        Ok(invite)
    }

    async fn get_room_invite(
        &self,
        invite_code: &str,
    ) -> Result<Option<crate::registration_token::RoomInvite>, sqlx::Error> {
        Ok(self.invites.read().await.get(invite_code).cloned())
    }

    async fn use_room_invite(&self, invite_code: &str, invitee_user_id: &str) -> Result<bool, sqlx::Error> {
        let mut invites = self.invites.write().await;
        if let Some(i) = invites.get_mut(invite_code) {
            if i.is_used || i.is_revoked {
                return Ok(false);
            }
            if let Some(exp) = i.expires_at {
                if exp < chrono::Utc::now().timestamp_millis() {
                    return Ok(false);
                }
            }
            i.is_used = true;
            i.invitee_user_id = Some(invitee_user_id.to_string());
            i.used_ts = Some(chrono::Utc::now().timestamp_millis());
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn revoke_room_invite(&self, invite_code: &str, reason: &str) -> Result<(), sqlx::Error> {
        if let Some(i) = self.invites.write().await.get_mut(invite_code) {
            i.is_revoked = true;
            i.revoked_at = Some(chrono::Utc::now().timestamp_millis());
            i.revoked_reason = Some(reason.to_string());
        }
        Ok(())
    }

    async fn create_batch(
        &self,
        batch: &crate::registration_token::RegistrationTokenBatch,
        tokens: &[String],
    ) -> Result<i64, sqlx::Error> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let b = crate::registration_token::RegistrationTokenBatch {
            id,
            batch_id: batch.batch_id.clone(),
            description: batch.description.clone(),
            token_count: batch.token_count,
            tokens_used: 0,
            created_by: batch.created_by.clone(),
            created_ts: chrono::Utc::now().timestamp_millis(),
            expires_at: batch.expires_at,
            is_enabled: true,
            allowed_email_domains: batch.allowed_email_domains.clone(),
            auto_join_rooms: batch.auto_join_rooms.clone(),
        };
        for token_str in tokens {
            let tid = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let now = chrono::Utc::now().timestamp_millis();
            let t = crate::registration_token::RegistrationToken {
                id: tid,
                token: token_str.clone(),
                token_type: "single_use".to_string(),
                description: batch.description.clone(),
                max_uses: 1,
                uses_count: 0,
                is_used: false,
                is_enabled: true,
                expires_at: batch.expires_at,
                created_by: batch.created_by.clone(),
                created_ts: now,
                updated_ts: now,
                last_used_ts: None,
                allowed_email_domains: None,
                allowed_user_ids: None,
                auto_join_rooms: None,
                display_name: None,
                email: None,
            };
            self.tokens.write().await.insert(token_str.clone(), t.clone());
            self.token_by_id.write().await.insert(tid, t);
        }
        self.batches.write().await.insert(batch.batch_id.clone(), b);
        Ok(id)
    }

    async fn get_batch(
        &self,
        batch_id: &str,
    ) -> Result<Option<crate::registration_token::RegistrationTokenBatch>, sqlx::Error> {
        Ok(self.batches.read().await.get(batch_id).cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "openclaw-routes")]
    use crate::ai_connection::AiConnectionStoreApi;
    use crate::burn_after_read::BurnAfterReadStoreApi;
    use crate::oidc_user_mapping::OidcUserMappingStoreApi;
    use crate::rate_limit::RateLimitStoreApi;
    use crate::room_summary::RoomSummaryStoreApi;
    use crate::room_tag::RoomTagStoreApi;
    use crate::sliding_sync::SlidingSyncStoreApi;
    use crate::user::UserStore;

    #[tokio::test]
    async fn shared_fake_user_store_is_usable_via_trait_object() {
        let store: SharedFakeUserStore = shared_fake_user_store();
        let _trait_ref: Arc<dyn UserStore> = store.clone();
        assert!(!store.is_user_locked("@nobody:example.com").await.unwrap());
    }

    #[tokio::test]
    async fn seed_locked_users_makes_lock_visible() {
        let store = shared_fake_user_store();
        seed_locked_users(
            &store,
            vec![crate::LockedUser {
                id: 1,
                user_id: "@bad:example.com".to_string(),
                reason: Some("spam".to_string()),
                locked_by: "@admin:example.com".to_string(),
                created_ts: 1_700_000_000_000,
                unlocked_ts: None,
                is_active: true,
            }],
        )
        .await;

        assert!(store.is_user_locked("@bad:example.com").await.unwrap());
        assert!(!store.is_user_locked("@innocent:example.com").await.unwrap());
    }

    // ── InMemoryRoomStore tests ──────────────────────────────────────

    #[tokio::test]
    async fn room_create_and_get() {
        let store = InMemoryRoomStore::new();
        store.create_room("!r:example.com", "@alice:example.com", "invite", "1", true).await.unwrap();
        let room = store.get_room("!r:example.com").await.unwrap().unwrap();
        assert_eq!(room.room_id, "!r:example.com");
        assert_eq!(room.creator_user_id.as_deref(), Some("@alice:example.com"));
        assert!(room.is_public);
    }

    #[tokio::test]
    async fn room_not_found_returns_none() {
        let store = InMemoryRoomStore::new();
        assert!(store.get_room("!nonexistent:example.com").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn room_batch_fetch_filters_missing() {
        let store = InMemoryRoomStore::new();
        store.create_room("!a:example.com", "@u:example.com", "invite", "1", false).await.unwrap();
        let rooms = store.get_rooms_batch(&["!a:example.com".into(), "!b:example.com".into()]).await.unwrap();
        assert_eq!(rooms.len(), 1);
    }

    #[tokio::test]
    async fn room_alias_round_trip() {
        let store = InMemoryRoomStore::new();
        store.create_room("!r:example.com", "@u:example.com", "invite", "1", false).await.unwrap();
        store.set_room_alias("!r:example.com", "#alias:example.com", "@u:example.com").await.unwrap();
        let room_id = store.get_room_by_alias("#alias:example.com").await.unwrap();
        assert_eq!(room_id.as_deref(), Some("!r:example.com"));
    }

    // ── InMemoryEventStore tests ─────────────────────────────────────

    #[tokio::test]
    async fn event_create_and_get() {
        let store = InMemoryEventStore::new();
        let params = crate::event::CreateEventParams {
            event_id: "$ev1:example.com".into(),
            room_id: "!r:example.com".into(),
            user_id: "@alice:example.com".into(),
            event_type: "m.room.message".into(),
            content: serde_json::json!({"body": "hello"}),
            state_key: None,
            origin_server_ts: 1_700_000_000_000,
            redacts: None,
        };
        let event = store.create_event(params).await.unwrap();
        assert_eq!(event.event_id, "$ev1:example.com");
        assert_eq!(event.event_type, "m.room.message");
    }

    #[tokio::test]
    async fn event_find_missing_ids() {
        let store = InMemoryEventStore::new();
        let params = crate::event::CreateEventParams {
            event_id: "$ev1:example.com".into(),
            room_id: "!r:example.com".into(),
            user_id: "@alice:example.com".into(),
            event_type: "m.room.message".into(),
            content: serde_json::json!({}),
            state_key: None,
            origin_server_ts: 1_700_000_000_000,
            redacts: None,
        };
        store.create_event(params).await.unwrap();
        let missing =
            store.find_missing_event_ids(&["$ev1:example.com".into(), "$ev2:example.com".into()]).await.unwrap();
        assert_eq!(missing, vec!["$ev2:example.com"]);
    }

    #[tokio::test]
    async fn event_redact_content_replaces_with_empty_json() {
        let store = InMemoryEventStore::new();
        let params = crate::event::CreateEventParams {
            event_id: "$ev1:example.com".into(),
            room_id: "!r:example.com".into(),
            user_id: "@alice:example.com".into(),
            event_type: "m.room.message".into(),
            content: serde_json::json!({"body": "secret"}),
            state_key: None,
            origin_server_ts: 1_700_000_000_000,
            redacts: None,
        };
        store.create_event(params).await.unwrap();
        store.redact_event_content("$ev1:example.com", None).await.unwrap();
        let redacted = store.get_event("$ev1:example.com").await.unwrap().unwrap();
        assert_eq!(redacted.content, serde_json::json!({}));
    }

    // ── EventStoreApi state event tests ─────────────────────────────

    #[tokio::test]
    async fn get_state_event_returns_matching_state_event() {
        let store = InMemoryEventStore::new();
        let params = crate::event::CreateEventParams {
            event_id: "$s1:example.com".into(),
            room_id: "!r:example.com".into(),
            user_id: "@alice:example.com".into(),
            event_type: "m.room.name".into(),
            content: serde_json::json!({"name": "Test Room"}),
            state_key: Some("".into()),
            origin_server_ts: 1_700_000_001_000,
            redacts: None,
        };
        store.create_event(params).await.unwrap();

        // Also create a non-state event (no state_key) — should be ignored.
        let non_state = crate::event::CreateEventParams {
            event_id: "$m1:example.com".into(),
            room_id: "!r:example.com".into(),
            user_id: "@alice:example.com".into(),
            event_type: "m.room.message".into(),
            content: serde_json::json!({"body": "hi"}),
            state_key: None,
            origin_server_ts: 1_700_000_002_000,
            redacts: None,
        };
        store.create_event(non_state).await.unwrap();

        let result = store.get_state_event("!r:example.com", "m.room.name", "").await.unwrap();
        assert!(result.is_some(), "should return state event for matching type+key");
        let ev = result.unwrap();
        assert_eq!(ev.event_id, "$s1:example.com");
        assert_eq!(ev.room_id, "!r:example.com");
        assert_eq!(ev.sender, "@alice:example.com");
        assert_eq!(ev.event_type.as_deref(), Some("m.room.name"));
    }

    #[tokio::test]
    async fn get_state_event_returns_none_for_missing_state_key() {
        let store = InMemoryEventStore::new();
        let result = store.get_state_event("!r:example.com", "m.room.name", "").await.unwrap();
        assert!(result.is_none(), "should return None when no matching state event");
    }

    #[tokio::test]
    async fn get_state_event_returns_none_for_non_matching_room() {
        let store = InMemoryEventStore::new();
        let params = crate::event::CreateEventParams {
            event_id: "$s1:example.com".into(),
            room_id: "!room_a:example.com".into(),
            user_id: "@alice:example.com".into(),
            event_type: "m.room.name".into(),
            content: serde_json::json!({"name": "A"}),
            state_key: Some("".into()),
            origin_server_ts: 1_700_000_000_000,
            redacts: None,
        };
        store.create_event(params).await.unwrap();

        let result = store.get_state_event("!room_b:example.com", "m.room.name", "").await.unwrap();
        assert!(result.is_none(), "should return None for different room");
    }

    // ── get_state_events_by_type tests ──────────────────────────────

    #[tokio::test]
    async fn get_state_events_by_type_returns_only_matching_type() {
        let store = InMemoryEventStore::new();
        // m.room.name state events (2 different state_keys)
        store
            .create_event(crate::event::CreateEventParams {
                event_id: "$n1:example.com".into(),
                room_id: "!r:example.com".into(),
                user_id: "@alice:example.com".into(),
                event_type: "m.room.name".into(),
                content: serde_json::json!({"name": "First"}),
                state_key: Some("".into()),
                origin_server_ts: 1_700_000_001_000,
                redacts: None,
            })
            .await
            .unwrap();
        store
            .create_event(crate::event::CreateEventParams {
                event_id: "$n2:example.com".into(),
                room_id: "!r:example.com".into(),
                user_id: "@alice:example.com".into(),
                event_type: "m.room.name".into(),
                content: serde_json::json!({"name": "Second"}),
                state_key: Some("alt".into()),
                origin_server_ts: 1_700_000_002_000,
                redacts: None,
            })
            .await
            .unwrap();
        // m.room.topic state event (different type, should be excluded)
        store
            .create_event(crate::event::CreateEventParams {
                event_id: "$t1:example.com".into(),
                room_id: "!r:example.com".into(),
                user_id: "@alice:example.com".into(),
                event_type: "m.room.topic".into(),
                content: serde_json::json!({"topic": "Chat"}),
                state_key: Some("".into()),
                origin_server_ts: 1_700_000_003_000,
                redacts: None,
            })
            .await
            .unwrap();

        let results = store.get_state_events_by_type("!r:example.com", "m.room.name").await.unwrap();
        assert_eq!(results.len(), 2, "should return exactly 2 m.room.name events");
        let event_ids: Vec<&str> = results.iter().map(|e| e.event_id.as_str()).collect();
        assert!(event_ids.contains(&"$n1:example.com"));
        assert!(event_ids.contains(&"$n2:example.com"));
    }

    #[tokio::test]
    async fn get_state_events_by_type_deduplicates_by_state_key() {
        let store = InMemoryEventStore::new();
        // Two m.room.name events with same state_key "" — only the latest should be returned.
        store
            .create_event(crate::event::CreateEventParams {
                event_id: "$old:example.com".into(),
                room_id: "!r:example.com".into(),
                user_id: "@alice:example.com".into(),
                event_type: "m.room.name".into(),
                content: serde_json::json!({"name": "Old"}),
                state_key: Some("".into()),
                origin_server_ts: 1_700_000_001_000,
                redacts: None,
            })
            .await
            .unwrap();
        store
            .create_event(crate::event::CreateEventParams {
                event_id: "$new:example.com".into(),
                room_id: "!r:example.com".into(),
                user_id: "@alice:example.com".into(),
                event_type: "m.room.name".into(),
                content: serde_json::json!({"name": "New"}),
                state_key: Some("".into()),
                origin_server_ts: 1_700_000_002_000,
                redacts: None,
            })
            .await
            .unwrap();

        let results = store.get_state_events_by_type("!r:example.com", "m.room.name").await.unwrap();
        assert_eq!(results.len(), 1, "should deduplicate by state_key, keeping latest");
        assert_eq!(results[0].event_id, "$new:example.com");
    }

    #[tokio::test]
    async fn get_state_events_by_type_returns_empty_for_no_match() {
        let store = InMemoryEventStore::new();
        let results = store.get_state_events_by_type("!r:example.com", "m.room.name").await.unwrap();
        assert!(results.is_empty());
    }

    // ── get_state_events_at_or_before tests ─────────────────────────

    #[tokio::test]
    async fn get_state_events_at_or_before_filters_by_timestamp() {
        let store = InMemoryEventStore::new();
        // Event at t=1000
        store
            .create_event(crate::event::CreateEventParams {
                event_id: "$old:example.com".into(),
                room_id: "!r:example.com".into(),
                user_id: "@alice:example.com".into(),
                event_type: "m.room.name".into(),
                content: serde_json::json!({"name": "Old"}),
                state_key: Some("".into()),
                origin_server_ts: 1000,
                redacts: None,
            })
            .await
            .unwrap();
        // Event at t=3000
        store
            .create_event(crate::event::CreateEventParams {
                event_id: "$new:example.com".into(),
                room_id: "!r:example.com".into(),
                user_id: "@alice:example.com".into(),
                event_type: "m.room.name".into(),
                content: serde_json::json!({"name": "New"}),
                state_key: Some("".into()),
                origin_server_ts: 3000,
                redacts: None,
            })
            .await
            .unwrap();

        // Query at t=2000 — only the t=1000 event should be visible.
        let results = store.get_state_events_at_or_before("!r:example.com", 2000).await.unwrap();
        assert_eq!(results.len(), 1, "only the old event should be visible at t=2000");
        assert_eq!(results[0].event_id, "$old:example.com");
    }

    #[tokio::test]
    async fn get_state_events_at_or_before_deduplicates_by_state_key() {
        let store = InMemoryEventStore::new();
        // Two events with same state_key, both at or before t=2000 — latest should win.
        store
            .create_event(crate::event::CreateEventParams {
                event_id: "$first:example.com".into(),
                room_id: "!r:example.com".into(),
                user_id: "@alice:example.com".into(),
                event_type: "m.room.name".into(),
                content: serde_json::json!({"name": "First"}),
                state_key: Some("".into()),
                origin_server_ts: 1000,
                redacts: None,
            })
            .await
            .unwrap();
        store
            .create_event(crate::event::CreateEventParams {
                event_id: "$second:example.com".into(),
                room_id: "!r:example.com".into(),
                user_id: "@alice:example.com".into(),
                event_type: "m.room.name".into(),
                content: serde_json::json!({"name": "Second"}),
                state_key: Some("".into()),
                origin_server_ts: 1500,
                redacts: None,
            })
            .await
            .unwrap();

        let results = store.get_state_events_at_or_before("!r:example.com", 2000).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].event_id, "$second:example.com");
    }

    // ── OidcUserMappingStore tests ──────────────────────────────────

    #[tokio::test]
    async fn oidc_insert_and_get_mapping() {
        let store = InMemoryOidcUserMappingStore::new();
        store
            .insert_mapping("https://idp.example.com", "sub-123", "@alice:example.com", 1_700_000_000_000)
            .await
            .unwrap();
        let bound = store.get_bound_user_id("https://idp.example.com", "sub-123").await.unwrap();
        assert_eq!(bound.as_deref(), Some("@alice:example.com"));
    }

    #[tokio::test]
    async fn oidc_get_none_for_unknown_subject() {
        let store = InMemoryOidcUserMappingStore::new();
        let bound = store.get_bound_user_id("https://idp.example.com", "unknown").await.unwrap();
        assert!(bound.is_none());
    }

    #[tokio::test]
    async fn oidc_update_last_authenticated_increments_counter() {
        let store = InMemoryOidcUserMappingStore::new();
        store
            .insert_mapping("https://idp.example.com", "sub-123", "@alice:example.com", 1_700_000_000_000)
            .await
            .unwrap();
        store.update_last_authenticated("https://idp.example.com", "sub-123", 1_700_000_010_000).await.unwrap();
        // Verify mapping still resolves correctly after update
        let bound = store.get_bound_user_id("https://idp.example.com", "sub-123").await.unwrap();
        assert_eq!(bound.as_deref(), Some("@alice:example.com"));
    }

    #[tokio::test]
    async fn oidc_issuer_isolation() {
        let store = InMemoryOidcUserMappingStore::new();
        store
            .insert_mapping("https://idp-a.example.com", "sub-1", "@alice:a.example.com", 1_700_000_000_000)
            .await
            .unwrap();
        store
            .insert_mapping("https://idp-b.example.com", "sub-1", "@alice:b.example.com", 1_700_000_000_000)
            .await
            .unwrap();

        let a = store.get_bound_user_id("https://idp-a.example.com", "sub-1").await.unwrap();
        let b = store.get_bound_user_id("https://idp-b.example.com", "sub-1").await.unwrap();
        assert_eq!(a.as_deref(), Some("@alice:a.example.com"));
        assert_eq!(b.as_deref(), Some("@alice:b.example.com"));
    }

    // ── AiConnectionStore tests (behind openclaw-routes feature) ──────

    #[cfg(feature = "openclaw-routes")]
    #[tokio::test]
    async fn ai_connection_create_and_get() {
        let store = InMemoryAiConnectionStore::new();
        let conn = crate::ai_connection::AiConnection {
            id: "conn-1".into(),
            user_id: "@alice:example.com".into(),
            provider: "openai".into(),
            config: Some(serde_json::json!({"api_key": "sk-test"})),
            is_active: true,
            created_ts: 1_700_000_000_000,
            updated_ts: None,
        };
        store.create_connection(&conn).await.unwrap();
        let got = store.get_connection("conn-1").await.unwrap().unwrap();
        assert_eq!(got.id, "conn-1");
        assert_eq!(got.provider, "openai");
        assert!(got.is_active);
    }

    #[cfg(feature = "openclaw-routes")]
    #[tokio::test]
    async fn ai_connection_get_none_for_unknown() {
        let store = InMemoryAiConnectionStore::new();
        assert!(store.get_connection("nonexistent").await.unwrap().is_none());
    }

    #[cfg(feature = "openclaw-routes")]
    #[tokio::test]
    async fn ai_connection_list_by_user() {
        let store = InMemoryAiConnectionStore::new();
        store
            .create_connection(&crate::ai_connection::AiConnection {
                id: "c1".into(),
                user_id: "@alice:example.com".into(),
                provider: "openai".into(),
                config: None,
                is_active: true,
                created_ts: 1000,
                updated_ts: None,
            })
            .await
            .unwrap();
        store
            .create_connection(&crate::ai_connection::AiConnection {
                id: "c2".into(),
                user_id: "@bob:example.com".into(),
                provider: "anthropic".into(),
                config: None,
                is_active: true,
                created_ts: 2000,
                updated_ts: None,
            })
            .await
            .unwrap();
        store
            .create_connection(&crate::ai_connection::AiConnection {
                id: "c3".into(),
                user_id: "@alice:example.com".into(),
                provider: "siliconflow".into(),
                config: None,
                is_active: false,
                created_ts: 3000,
                updated_ts: None,
            })
            .await
            .unwrap();

        let alice_conns = store.get_user_connections("@alice:example.com").await.unwrap();
        assert_eq!(alice_conns.len(), 2);
        assert_eq!(alice_conns[0].id, "c3"); // newest first
        assert_eq!(alice_conns[1].id, "c1");
    }

    #[cfg(feature = "openclaw-routes")]
    #[tokio::test]
    async fn ai_connection_filter_by_provider() {
        let store = InMemoryAiConnectionStore::new();
        store
            .create_connection(&crate::ai_connection::AiConnection {
                id: "c1".into(),
                user_id: "@alice:example.com".into(),
                provider: "openai".into(),
                config: None,
                is_active: true,
                created_ts: 1000,
                updated_ts: None,
            })
            .await
            .unwrap();
        store
            .create_connection(&crate::ai_connection::AiConnection {
                id: "c2".into(),
                user_id: "@alice:example.com".into(),
                provider: "openai".into(),
                config: None,
                is_active: false,
                created_ts: 2000,
                updated_ts: None,
            })
            .await
            .unwrap();

        let result = store.get_user_provider_connection("@alice:example.com", "openai").await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, "c1"); // only active one

        let result = store.get_user_provider_connection("@alice:example.com", "anthropic").await.unwrap();
        assert!(result.is_none());
    }

    #[cfg(feature = "openclaw-routes")]
    #[tokio::test]
    async fn ai_connection_update_status() {
        let store = InMemoryAiConnectionStore::new();
        store
            .create_connection(&crate::ai_connection::AiConnection {
                id: "c1".into(),
                user_id: "@alice:example.com".into(),
                provider: "openai".into(),
                config: None,
                is_active: true,
                created_ts: 1000,
                updated_ts: None,
            })
            .await
            .unwrap();

        store.update_connection_status("c1", false).await.unwrap();
        let conn = store.get_connection("c1").await.unwrap().unwrap();
        assert!(!conn.is_active);
    }

    #[cfg(feature = "openclaw-routes")]
    #[tokio::test]
    async fn ai_connection_delete() {
        let store = InMemoryAiConnectionStore::new();
        store
            .create_connection(&crate::ai_connection::AiConnection {
                id: "c1".into(),
                user_id: "@alice:example.com".into(),
                provider: "openai".into(),
                config: None,
                is_active: true,
                created_ts: 1000,
                updated_ts: None,
            })
            .await
            .unwrap();

        store.delete_connection("c1").await.unwrap();
        assert!(store.get_connection("c1").await.unwrap().is_none());
    }

    // ── InMemoryRateLimitStore tests ───────────────────────────────────

    #[tokio::test]
    async fn rate_limit_upsert_and_get() {
        let store = InMemoryRateLimitStore::new();
        store.upsert_user_rate_limit("@alice:example.com", 10.0, 5).await.unwrap();
        let record = store.get_user_rate_limit("@alice:example.com").await.unwrap().unwrap();
        assert_eq!(record.messages_per_second, Some(10.0));
        assert_eq!(record.burst_count, Some(5));
    }

    #[tokio::test]
    async fn rate_limit_get_none_for_unknown() {
        let store = InMemoryRateLimitStore::new();
        assert!(store.get_user_rate_limit("@unknown:example.com").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn rate_limit_upsert_overwrites_existing() {
        let store = InMemoryRateLimitStore::new();
        store.upsert_user_rate_limit("@alice:example.com", 10.0, 5).await.unwrap();
        store.upsert_user_rate_limit("@alice:example.com", 20.0, 10).await.unwrap();
        let record = store.get_user_rate_limit("@alice:example.com").await.unwrap().unwrap();
        assert_eq!(record.messages_per_second, Some(20.0));
        assert_eq!(record.burst_count, Some(10));
    }

    #[tokio::test]
    async fn rate_limit_delete_removes_record() {
        let store = InMemoryRateLimitStore::new();
        store.upsert_user_rate_limit("@alice:example.com", 10.0, 5).await.unwrap();
        store.delete_user_rate_limit("@alice:example.com").await.unwrap();
        assert!(store.get_user_rate_limit("@alice:example.com").await.unwrap().is_none());
    }

    // ── InMemoryMemberStore tests ────────────────────────────────────

    #[tokio::test]
    async fn member_join_and_query() {
        let store = InMemoryMemberStore::new();
        store.add_member("!r:example.com", "@alice:example.com", "join", Some("Alice")).await.unwrap();
        assert!(store.is_member("!r:example.com", "@alice:example.com").await.unwrap());
        assert_eq!(
            store.get_membership_state("!r:example.com", "@alice:example.com").await.unwrap().as_deref(),
            Some("join")
        );
    }

    #[tokio::test]
    async fn member_ban_updates_state() {
        let store = InMemoryMemberStore::new();
        store.add_member("!r:example.com", "@bad:example.com", "join", None).await.unwrap();
        store.ban_member("!r:example.com", "@bad:example.com", "@admin:example.com").await.unwrap();
        let member = store.get_member("!r:example.com", "@bad:example.com").await.unwrap().unwrap();
        assert_eq!(member.membership, "ban");
        assert_eq!(member.banned_by.as_deref(), Some("@admin:example.com"));
    }

    #[tokio::test]
    async fn member_joined_rooms_lists_only_joined() {
        let store = InMemoryMemberStore::new();
        store.add_member("!r1:example.com", "@alice:example.com", "join", None).await.unwrap();
        store.add_member("!r2:example.com", "@alice:example.com", "leave", None).await.unwrap();
        store.add_member("!r3:example.com", "@alice:example.com", "join", None).await.unwrap();
        let rooms = store.get_joined_rooms("@alice:example.com").await.unwrap();
        assert_eq!(rooms.len(), 2);
        assert!(rooms.contains(&"!r1:example.com".to_string()));
        assert!(rooms.contains(&"!r3:example.com".to_string()));
    }

    // ── InMemoryRoomTagStore tests ─────────────────────────────────────

    #[tokio::test]
    async fn room_tag_add_and_list() {
        let store = InMemoryRoomTagStore::new();
        store.add_tag("@alice:example.com", "!r:example.com", "favourite", Some(0.5)).await.unwrap();
        let tags = store.get_tags("@alice:example.com", "!r:example.com").await.unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag, "favourite");
    }

    #[tokio::test]
    async fn room_tag_upsert_replaces_order() {
        let store = InMemoryRoomTagStore::new();
        store.add_tag("@alice:example.com", "!r:example.com", "favourite", Some(0.5)).await.unwrap();
        store.add_tag("@alice:example.com", "!r:example.com", "favourite", Some(0.9)).await.unwrap();
        let tags = store.get_tags("@alice:example.com", "!r:example.com").await.unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].order, Some(0.9));
    }

    #[tokio::test]
    async fn room_tag_remove() {
        let store = InMemoryRoomTagStore::new();
        store.add_tag("@alice:example.com", "!r:example.com", "favourite", None).await.unwrap();
        store.add_tag("@alice:example.com", "!r:example.com", "u.lowpriority", None).await.unwrap();
        store.remove_tag("@alice:example.com", "!r:example.com", "favourite").await.unwrap();
        let tags = store.get_tags("@alice:example.com", "!r:example.com").await.unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag, "u.lowpriority");
    }

    #[tokio::test]
    async fn room_tag_get_all_tags_across_rooms() {
        let store = InMemoryRoomTagStore::new();
        store.add_tag("@alice:example.com", "!r1:example.com", "favourite", None).await.unwrap();
        store.add_tag("@alice:example.com", "!r2:example.com", "u.lowpriority", None).await.unwrap();
        let tags = store.get_all_tags("@alice:example.com").await.unwrap();
        assert_eq!(tags.len(), 2);
    }

    #[tokio::test]
    async fn room_tag_empty_for_unknown_user() {
        let store = InMemoryRoomTagStore::new();
        let tags = store.get_tags("@unknown:example.com", "!r:example.com").await.unwrap();
        assert!(tags.is_empty());
    }

    // ── InMemoryBurnAfterReadStore ───────────────────────────────────────

    #[derive(Clone, Default)]
    pub struct InMemoryBurnAfterReadStore {
        settings: std::sync::Arc<
            tokio::sync::RwLock<std::collections::HashMap<(String, String), crate::burn_after_read::BurnSettingsRow>>,
        >,
        pending: std::sync::Arc<tokio::sync::RwLock<Vec<crate::burn_after_read::BurnPendingRow>>>,
        logs: std::sync::Arc<tokio::sync::RwLock<Vec<crate::burn_after_read::BurnLogRow>>>,
        defaults: std::sync::Arc<
            tokio::sync::RwLock<std::collections::HashMap<String, crate::burn_after_read::BurnUserDefaultsRow>>,
        >,
        next_id: std::sync::Arc<std::sync::atomic::AtomicI64>,
    }

    impl InMemoryBurnAfterReadStore {
        pub fn new() -> Self {
            Self::default()
        }
    }

    #[async_trait::async_trait]
    impl crate::burn_after_read::BurnAfterReadStoreApi for InMemoryBurnAfterReadStore {
        async fn get_settings(
            &self,
            user_id: &str,
            room_id: &str,
        ) -> Result<Option<crate::burn_after_read::BurnSettingsRow>, sqlx::Error> {
            Ok(self.settings.read().await.get(&(user_id.to_string(), room_id.to_string())).cloned())
        }

        async fn set_settings(
            &self,
            user_id: &str,
            room_id: &str,
            is_enabled: bool,
            burn_after_ms: i64,
        ) -> Result<crate::burn_after_read::BurnSettingsRow, sqlx::Error> {
            let now = chrono::Utc::now().timestamp_millis();
            let row = crate::burn_after_read::BurnSettingsRow {
                user_id: user_id.to_string(),
                room_id: room_id.to_string(),
                is_enabled,
                burn_after_ms,
                created_ts: now,
                updated_ts: Some(now),
            };
            self.settings.write().await.insert((user_id.to_string(), room_id.to_string()), row.clone());
            Ok(row)
        }

        async fn schedule_burn(
            &self,
            user_id: &str,
            room_id: &str,
            event_id: &str,
            delete_ts: i64,
        ) -> Result<crate::burn_after_read::BurnPendingRow, sqlx::Error> {
            let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let row = crate::burn_after_read::BurnPendingRow {
                id,
                user_id: user_id.to_string(),
                room_id: room_id.to_string(),
                event_id: event_id.to_string(),
                created_ts: chrono::Utc::now().timestamp_millis(),
                delete_ts,
                is_processed: false,
            };
            self.pending.write().await.push(row.clone());
            Ok(row)
        }

        async fn cancel_burn(&self, user_id: &str, room_id: &str, event_id: &str) -> Result<(), sqlx::Error> {
            let mut pending = self.pending.write().await;
            for p in pending.iter_mut() {
                if p.user_id == user_id && p.room_id == room_id && p.event_id == event_id && !p.is_processed {
                    p.is_processed = true;
                }
            }
            Ok(())
        }

        async fn get_pending_burns(
            &self,
            user_id: &str,
            room_id: &str,
        ) -> Result<Vec<crate::burn_after_read::BurnPendingRow>, sqlx::Error> {
            Ok(self
                .pending
                .read()
                .await
                .iter()
                .filter(|p| p.user_id == user_id && p.room_id == room_id && !p.is_processed)
                .cloned()
                .collect())
        }

        async fn get_expired_burns(
            &self,
            now_ms: i64,
        ) -> Result<Vec<crate::burn_after_read::BurnPendingRow>, sqlx::Error> {
            Ok(self.pending.read().await.iter().filter(|p| p.delete_ts <= now_ms && !p.is_processed).cloned().collect())
        }

        async fn mark_burn_processed(&self, id: i64) -> Result<(), sqlx::Error> {
            let mut pending = self.pending.write().await;
            for p in pending.iter_mut() {
                if p.id == id {
                    p.is_processed = true;
                }
            }
            Ok(())
        }

        async fn log_burned_event(
            &self,
            user_id: &str,
            room_id: &str,
            event_id: &str,
            burned_ts: i64,
        ) -> Result<(), sqlx::Error> {
            let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.logs.write().await.push(crate::burn_after_read::BurnLogRow {
                id,
                user_id: user_id.to_string(),
                room_id: room_id.to_string(),
                event_id: event_id.to_string(),
                burned_ts,
            });
            Ok(())
        }

        async fn get_user_stats(&self, user_id: &str) -> Result<crate::burn_after_read::BurnStatsRow, sqlx::Error> {
            let pending = self.pending.read().await;
            let logs = self.logs.read().await;
            let settings = self.settings.read().await;
            Ok(crate::burn_after_read::BurnStatsRow {
                total_burned: logs.iter().filter(|l| l.user_id == user_id).count() as i64,
                total_pending: pending.iter().filter(|p| p.user_id == user_id && !p.is_processed).count() as i64,
                rooms_enabled: settings.iter().filter(|((uid, _), s)| uid == user_id && s.is_enabled).count() as i64,
            })
        }

        async fn get_user_default(
            &self,
            user_id: &str,
        ) -> Result<Option<crate::burn_after_read::BurnUserDefaultsRow>, sqlx::Error> {
            Ok(self.defaults.read().await.get(user_id).cloned())
        }

        async fn set_user_default(&self, user_id: &str, default_burn_ms: i64) -> Result<(), sqlx::Error> {
            let now = chrono::Utc::now().timestamp_millis();
            self.defaults.write().await.insert(
                user_id.to_string(),
                crate::burn_after_read::BurnUserDefaultsRow {
                    user_id: user_id.to_string(),
                    default_burn_ms,
                    created_ts: now,
                    updated_ts: Some(now),
                },
            );
            Ok(())
        }
    }

    #[tokio::test]
    async fn burn_after_read_settings_round_trip() {
        let store = InMemoryBurnAfterReadStore::new();
        let result = store.get_settings("@alice:example.com", "!room:example.com").await.unwrap();
        assert!(result.is_none());

        let row = store.set_settings("@alice:example.com", "!room:example.com", true, 60_000).await.unwrap();
        assert!(row.is_enabled);
        assert_eq!(row.burn_after_ms, 60_000);

        let fetched = store.get_settings("@alice:example.com", "!room:example.com").await.unwrap().unwrap();
        assert!(fetched.is_enabled);
    }

    #[tokio::test]
    async fn burn_after_read_schedule_and_expire() {
        let store = InMemoryBurnAfterReadStore::new();
        let now = chrono::Utc::now().timestamp_millis();

        store.schedule_burn("@alice:example.com", "!room:example.com", "$ev1", now + 60_000).await.unwrap();
        store.schedule_burn("@alice:example.com", "!room:example.com", "$ev2", now - 60_000).await.unwrap();

        let pending = store.get_pending_burns("@alice:example.com", "!room:example.com").await.unwrap();
        assert_eq!(pending.len(), 2);

        let expired = store.get_expired_burns(now).await.unwrap();
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].event_id, "$ev2");
    }

    #[tokio::test]
    async fn burn_after_read_cancel_removes_from_pending() {
        let store = InMemoryBurnAfterReadStore::new();
        let now = chrono::Utc::now().timestamp_millis();

        store.schedule_burn("@alice:example.com", "!room:example.com", "$ev1", now + 60_000).await.unwrap();

        store.cancel_burn("@alice:example.com", "!room:example.com", "$ev1").await.unwrap();

        let pending = store.get_pending_burns("@alice:example.com", "!room:example.com").await.unwrap();
        assert!(pending.is_empty());
    }

    #[tokio::test]
    async fn burn_after_read_user_default() {
        let store = InMemoryBurnAfterReadStore::new();
        let result = store.get_user_default("@alice:example.com").await.unwrap();
        assert!(result.is_none());

        store.set_user_default("@alice:example.com", 30_000).await.unwrap();

        let fetched = store.get_user_default("@alice:example.com").await.unwrap().unwrap();
        assert_eq!(fetched.default_burn_ms, 30_000);
    }

    // ── InMemoryRoomSummaryStore tests ────────────────────────────────

    #[tokio::test]
    async fn room_summary_create_and_get() {
        let store = InMemoryRoomSummaryStore::new();
        let summary = store
            .create_summary(crate::room_summary::CreateRoomSummaryRequest {
                room_id: "!r:example.com".into(),
                name: Some("Test Room".into()),
                topic: None,
                room_type: None,
                avatar_url: None,
                canonical_alias: None,
                join_rule: None,
                history_visibility: None,
                guest_access: None,
                is_direct: None,
                is_space: None,
            })
            .await
            .unwrap();
        assert_eq!(summary.room_id, "!r:example.com");
        assert_eq!(summary.name.as_deref(), Some("Test Room"));

        let fetched = store.get_summary("!r:example.com").await.unwrap().unwrap();
        assert_eq!(fetched.room_id, "!r:example.com");
    }

    #[tokio::test]
    async fn room_summary_add_and_get_members() {
        let store = InMemoryRoomSummaryStore::new();
        store
            .create_summary(crate::room_summary::CreateRoomSummaryRequest {
                room_id: "!r:example.com".into(),
                name: None,
                topic: None,
                room_type: None,
                avatar_url: None,
                canonical_alias: None,
                join_rule: None,
                history_visibility: None,
                guest_access: None,
                is_direct: None,
                is_space: None,
            })
            .await
            .unwrap();

        let member = store
            .add_member(crate::room_summary::CreateSummaryMemberRequest {
                room_id: "!r:example.com".into(),
                user_id: "@alice:example.com".into(),
                display_name: Some("Alice".into()),
                avatar_url: None,
                membership: "join".into(),
                is_hero: Some(true),
                last_active_ts: None,
            })
            .await
            .unwrap();
        assert_eq!(member.user_id, "@alice:example.com");
        assert!(member.is_hero);

        let members = store.get_members("!r:example.com").await.unwrap();
        assert_eq!(members.len(), 1);
    }

    #[tokio::test]
    async fn room_summary_state_round_trip() {
        let store = InMemoryRoomSummaryStore::new();
        let state = store
            .set_state("!r:example.com", "m.room.name", "", None, serde_json::json!({"name": "Lobby"}))
            .await
            .unwrap();
        assert_eq!(state.event_type, "m.room.name");

        let fetched = store.get_state("!r:example.com", "m.room.name", "").await.unwrap().unwrap();
        assert_eq!(fetched.content, serde_json::json!({"name": "Lobby"}));
    }

    #[tokio::test]
    async fn room_summary_stats_update_and_get() {
        let store = InMemoryRoomSummaryStore::new();
        let stats = store.update_stats("!r:example.com", 10, 2, 8, 1, 1024).await.unwrap();
        assert_eq!(stats.total_events, 10);

        let fetched = store.get_stats("!r:example.com").await.unwrap().unwrap();
        assert_eq!(fetched.total_messages, 8);
    }

    #[tokio::test]
    async fn room_summary_queue_lifecycle() {
        let store = InMemoryRoomSummaryStore::new();
        store.queue_update("!r:example.com", "$ev1", "m.room.message", None, 10).await.unwrap();
        store.queue_update("!r:example.com", "$ev2", "m.room.member", Some("@a:ex.com"), 5).await.unwrap();

        let pending = store.get_pending_updates(10).await.unwrap();
        assert_eq!(pending.len(), 2);
        // Highest priority first
        assert_eq!(pending[0].event_id, "$ev1");
        assert_eq!(pending[0].priority, 10);

        store.mark_update_processed(pending[0].id).await.unwrap();
        let remaining = store.get_pending_updates(10).await.unwrap();
        assert_eq!(remaining.len(), 1);
    }

    // ── InMemorySlidingSyncStore tests ────────────────────────────────

    #[tokio::test]
    async fn sliding_sync_token_create_and_get() {
        let store = InMemorySlidingSyncStore::new();
        let token = store.create_or_update_token("@alice:ex.com", "DEV1", Some("conn1")).await.unwrap();
        assert_eq!(token.user_id, "@alice:ex.com");
        assert!(token.token.starts_with("sst_"));

        let fetched = store.get_token("@alice:ex.com", "DEV1", Some("conn1")).await.unwrap().unwrap();
        assert_eq!(fetched.id, token.id);

        assert!(store.validate_pos("@alice:ex.com", "DEV1", Some("conn1"), &token.pos.to_string()).await.unwrap());
        assert!(!store.validate_pos("@alice:ex.com", "DEV1", Some("conn1"), "bad_pos").await.unwrap());
    }

    #[tokio::test]
    async fn sliding_sync_list_save_and_get() {
        let store = InMemorySlidingSyncStore::new();
        let list = store
            .save_list(
                "@alice:ex.com",
                "DEV1",
                Some("conn1"),
                "my_list",
                &["by_name".to_string()],
                None,
                None,
                &[(0, 10)],
            )
            .await
            .unwrap();
        assert_eq!(list.list_key, "my_list");

        let lists = store.get_lists("@alice:ex.com", "DEV1", Some("conn1")).await.unwrap();
        assert_eq!(lists.len(), 1);

        store.delete_list("@alice:ex.com", "DEV1", Some("conn1"), "my_list").await.unwrap();
        let empty = store.get_lists("@alice:ex.com", "DEV1", Some("conn1")).await.unwrap();
        assert!(empty.is_empty());
    }

    #[tokio::test]
    async fn sliding_sync_room_upsert_and_get() {
        let store = InMemorySlidingSyncStore::new();
        let room = store
            .upsert_room(
                "@alice:ex.com",
                "DEV1",
                "!r:ex.com",
                Some("conn1"),
                Some("my_list"),
                100,
                0,
                2,
                true,
                false,
                false,
                false,
                Some("Test Room"),
                None,
                200,
            )
            .await
            .unwrap();
        assert_eq!(room.room_id, "!r:ex.com");
        assert!(room.is_dm);

        let fetched = store.get_room("@alice:ex.com", "DEV1", "!r:ex.com", Some("conn1")).await.unwrap().unwrap();
        assert_eq!(fetched.bump_stamp, 100);
        assert_eq!(fetched.notification_count, 2);
    }

    #[tokio::test]
    async fn sliding_sync_rooms_for_list() {
        let store = InMemorySlidingSyncStore::new();
        store
            .upsert_room(
                "@alice:ex.com",
                "DEV1",
                "!r:ex.com",
                Some("conn1"),
                Some("my_list"),
                200,
                0,
                0,
                false,
                false,
                false,
                false,
                Some("Room A"),
                None,
                200,
            )
            .await
            .unwrap();
        store
            .upsert_room(
                "@alice:ex.com",
                "DEV1",
                "!r2:ex.com",
                Some("conn1"),
                Some("my_list"),
                100,
                0,
                0,
                false,
                false,
                false,
                false,
                Some("Room B"),
                None,
                100,
            )
            .await
            .unwrap();

        let rooms = store
            .get_rooms_for_list(crate::sliding_sync::SlidingSyncListQuery {
                user_id: "@alice:ex.com",
                device_id: "DEV1",
                conn_id: Some("conn1"),
                list_key: "my_list",
                start: 0,
                end: 10,
                filters: None,
            })
            .await
            .unwrap();
        assert_eq!(rooms.len(), 2);
        // Higher bump_stamp first
        assert_eq!(rooms[0].room_id, "!r:ex.com");
    }

    #[tokio::test]
    async fn sliding_sync_notification_counts_and_bump() {
        let store = InMemorySlidingSyncStore::new();
        store
            .upsert_room(
                "@alice:ex.com",
                "DEV1",
                "!r:ex.com",
                Some("conn1"),
                Some("my_list"),
                0,
                0,
                0,
                false,
                false,
                false,
                false,
                None,
                None,
                0,
            )
            .await
            .unwrap();

        store.update_notification_counts("@alice:ex.com", "DEV1", "!r:ex.com", Some("conn1"), 5, 10).await.unwrap();
        store.bump_room("@alice:ex.com", "DEV1", "!r:ex.com", Some("conn1"), 42).await.unwrap();

        let room = store.get_room("@alice:ex.com", "DEV1", "!r:ex.com", Some("conn1")).await.unwrap().unwrap();
        assert_eq!(room.highlight_count, 5);
        assert_eq!(room.notification_count, 10);
        assert_eq!(room.bump_stamp, 42);
    }

    #[tokio::test]
    async fn sliding_sync_token_cleanup() {
        let store = InMemorySlidingSyncStore::new();
        store.create_or_update_token("@alice:ex.com", "DEV1", Some("conn1")).await.unwrap();
        // Token has a future expiry, so cleanup should remove none
        let removed = store.cleanup_expired_tokens().await.unwrap();
        assert_eq!(removed, 0);
        assert!(store.get_token("@alice:ex.com", "DEV1", Some("conn1")).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn sliding_sync_delete_connection_data() {
        let store = InMemorySlidingSyncStore::new();
        store.create_or_update_token("@alice:ex.com", "DEV1", Some("conn1")).await.unwrap();
        store.save_list("@alice:ex.com", "DEV1", Some("conn1"), "l1", &[], None, None, &[(0, 5)]).await.unwrap();

        store.delete_connection_data("@alice:ex.com", "DEV1", Some("conn1")).await.unwrap();

        assert!(store.get_token("@alice:ex.com", "DEV1", Some("conn1")).await.unwrap().is_none());
        assert!(store.get_lists("@alice:ex.com", "DEV1", Some("conn1")).await.unwrap().is_empty());
    }
}
