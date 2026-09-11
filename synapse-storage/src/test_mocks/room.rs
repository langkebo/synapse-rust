use super::*;

/// In-memory room store mirroring [`crate::room::RoomStorage`].
///
/// # Fidelity
///
/// This is **not** a complete model of the real store. Dimensions that are
/// deliberately unmodelled are marked inline with `⚠️ MOCK DEVIATION`:
///
/// * **Room blocking** — `block_room` is a no-op and `get_room_block_status`
///   always returns `None`; `blocked_by` / `reason` are not stored. See the
///   note on `block_room` below.
///
/// A test that needs a deviation-free store must use the database-backed
/// implementation. The guard `tests/unit/mock_fidelity_tests.rs` enumerates the
/// modules carrying deviation markers so a newly added one cannot go unnoticed.
#[derive(Clone, Default)]
pub struct InMemoryRoomStore {
    rooms: Arc<RwLock<HashMap<String, crate::room::Room>>>,
    aliases: Arc<RwLock<HashMap<String, String>>>,   // alias → room_id
    directories: Arc<RwLock<HashMap<String, bool>>>, // room_id → is_public
}

impl InMemoryRoomStore {
    /// See [`new`].
    pub fn new() -> Self {
        Self {
            rooms: Arc::new(RwLock::new(HashMap::new())),
            aliases: Arc::new(RwLock::new(HashMap::new())),
            directories: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// See [`create_room`].
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

    /// See [`get_room`].
    pub async fn get_room(&self, room_id: &str) -> Result<Option<crate::room::Room>, String> {
        Ok(self.rooms.read().await.get(room_id).cloned())
    }

    /// See [`get_rooms_batch`].
    pub async fn get_rooms_batch(&self, room_ids: &[String]) -> Result<Vec<crate::room::Room>, String> {
        let rooms = self.rooms.read().await;
        Ok(room_ids.iter().filter_map(|id| rooms.get(id).cloned()).collect())
    }

    /// See [`room_exists`].
    pub async fn room_exists(&self, room_id: &str) -> Result<bool, String> {
        Ok(self.rooms.read().await.contains_key(room_id))
    }

    /// See [`get_user_rooms`].
    pub async fn get_user_rooms(&self, _user_id: &str) -> Result<Vec<String>, String> {
        // This data lives in InMemoryMemberStore — stub returns all rooms.
        // Real implementation would join with membership data.
        Ok(self.rooms.read().await.keys().cloned().collect())
    }

    /// See [`get_rooms_map`].
    pub async fn get_rooms_map(&self, room_ids: &[String]) -> Result<HashMap<String, crate::room::Room>, String> {
        let rooms = self.rooms.read().await;
        Ok(room_ids.iter().filter_map(|id| rooms.get(id).map(|r| (id.clone(), r.clone()))).collect())
    }

    /// See [`update_room_name`].
    pub async fn update_room_name(&self, room_id: &str, name: &str) -> Result<(), String> {
        self.rooms
            .write()
            .await
            .get_mut(room_id)
            .map(|r| r.name = Some(name.to_string()))
            .ok_or_else(|| format!("room {room_id} not found"))
    }

    /// See [`set_room_alias`].
    pub async fn set_room_alias(&self, room_id: &str, alias: &str, _created_by: &str) -> Result<(), String> {
        if !self.rooms.read().await.contains_key(room_id) {
            return Err(format!("room {room_id} not found"));
        }
        self.aliases.write().await.insert(alias.to_string(), room_id.to_string());
        Ok(())
    }

    /// See [`get_room_by_alias`].
    pub async fn get_room_by_alias(&self, alias: &str) -> Result<Option<String>, String> {
        Ok(self.aliases.read().await.get(alias).cloned())
    }

    /// See [`delete_room`].
    pub async fn delete_room(&self, room_id: &str) -> Result<(), String> {
        self.rooms.write().await.remove(room_id);
        Ok(())
    }
}

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

    async fn decrement_member_count(
        &self,
        room_id: &str,
        _tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<(), sqlx::Error> {
        if let Some(room) = self.rooms.write().await.get_mut(room_id) {
            room.member_count = room.member_count.saturating_sub(1);
        }
        Ok(())
    }

    async fn decrement_member_counts_batch(&self, room_ids: &[String]) -> Result<u64, sqlx::Error> {
        let mut affected = 0u64;
        let mut guard = self.rooms.write().await;
        for room_id in room_ids {
            if let Some(room) = guard.get_mut(room_id) {
                room.member_count = room.member_count.saturating_sub(1);
                affected += 1;
            }
        }
        Ok(affected)
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

    async fn update_read_marker_monotonic(
        &self,
        _room_id: &str,
        _user_id: &str,
        _event_id: &str,
        _marker_type: &str,
        _allow_backward: bool,
    ) -> Result<bool, sqlx::Error> {
        // Read markers are not modeled in InMemoryRoomStore; pretend updated.
        Ok(true)
    }

    // ── Extended room queries (in-memory implementations) ──

    async fn get_rooms_batch(&self, room_ids: &[String]) -> Result<Vec<crate::room::Room>, sqlx::Error> {
        let rooms = self.rooms.read().await;
        Ok(room_ids.iter().filter_map(|id| rooms.get(id).cloned()).collect())
    }

    async fn increment_member_count(&self, room_id: &str) -> Result<(), sqlx::Error> {
        if let Some(room) = self.rooms.write().await.get_mut(room_id) {
            room.member_count = room.member_count.saturating_add(1);
        }
        Ok(())
    }

    // ── Admin / directory / stats queries (in-memory implementations) ──

    async fn get_public_rooms_paginated(
        &self,
        limit: i64,
        since_ts: Option<i64>,
        since_room_id: Option<&str>,
    ) -> Result<Vec<crate::room::Room>, sqlx::Error> {
        let rooms = self.rooms.read().await;
        let mut filtered: Vec<crate::room::Room> = rooms
            .values()
            .filter(|r| r.is_public)
            .filter(|r| {
                if let (Some(ts), Some(rid)) = (since_ts, since_room_id) {
                    r.created_ts < ts || (r.created_ts == ts && r.room_id.as_str() < rid)
                } else {
                    true
                }
            })
            .cloned()
            .collect();
        filtered.sort_by(|a, b| b.created_ts.cmp(&a.created_ts).then_with(|| b.room_id.cmp(&a.room_id)));
        filtered.truncate(limit as usize);
        Ok(filtered)
    }

    async fn count_public_rooms(&self) -> Result<i64, sqlx::Error> {
        let rooms = self.rooms.read().await;
        Ok(rooms.values().filter(|r| r.is_public).count() as i64)
    }

    async fn get_all_rooms_with_members(
        &self,
        limit: i64,
        from: Option<crate::room::RoomSearchCursor>,
        order_by: crate::room::RoomSearchOrder,
    ) -> Result<(Vec<(crate::room::Room, i64)>, Option<String>), sqlx::Error> {
        // P5-fix: this mock previously ignored BOTH `from` and `order_by` and always
        // returned `None` for next_batch, so cursor pagination was untestable — a
        // caller paging would receive page 1 again instead of the next slice. It now
        // mirrors the real query's rules (room/mod.rs):
        //   * keyset predicate is the strict `< (…key…)` for the active order
        //   * `LIMIT limit + 1` probes for a further page
        //   * `next_batch` anchors on the LAST RETURNED row (see P5 report §3.5)
        // Note: the real query filtering is `WHERE 1 = 1` — it does NOT restrict to
        // public rooms (that is `count_public_rooms`), so neither does this mock.
        use crate::room::{RoomSearchCursor, RoomSearchOrder};

        let rooms = self.rooms.read().await;
        let mut filtered: Vec<(crate::room::Room, i64)> = rooms.values().map(|r| (r.clone(), r.member_count)).collect();

        match (order_by, from.as_ref()) {
            (RoomSearchOrder::Created, Some(RoomSearchCursor::Created { created_ts, room_id })) => {
                filtered.retain(|(r, _)| (r.created_ts, r.room_id.as_str()) < (*created_ts, room_id.as_str()));
            }
            (RoomSearchOrder::Name, Some(RoomSearchCursor::Name { name, created_ts, room_id })) => {
                filtered.retain(|(r, _)| {
                    (r.name.as_deref(), r.created_ts, r.room_id.as_str())
                        < (name.as_deref(), *created_ts, room_id.as_str())
                });
            }
            (RoomSearchOrder::Size, Some(RoomSearchCursor::Size { member_count, created_ts, room_id })) => {
                filtered.retain(|(r, _)| {
                    (r.member_count, r.created_ts, r.room_id.as_str()) < (*member_count, *created_ts, room_id.as_str())
                });
            }
            _ => {}
        }

        match order_by {
            RoomSearchOrder::Created => {
                filtered.sort_by(|a, b| b.0.created_ts.cmp(&a.0.created_ts).then_with(|| b.0.room_id.cmp(&a.0.room_id)))
            }
            RoomSearchOrder::Name => filtered.sort_by(|a, b| {
                b.0.name
                    .cmp(&a.0.name)
                    .then_with(|| b.0.created_ts.cmp(&a.0.created_ts))
                    .then_with(|| b.0.room_id.cmp(&a.0.room_id))
            }),
            RoomSearchOrder::Size => filtered.sort_by(|a, b| {
                b.1.cmp(&a.1)
                    .then_with(|| b.0.created_ts.cmp(&a.0.created_ts))
                    .then_with(|| b.0.room_id.cmp(&a.0.room_id))
            }),
        }

        let next_batch = if filtered.len() as i64 > limit {
            filtered.get(limit.saturating_sub(1) as usize).map(|(last_room, last_count)| {
                let cursor = match order_by {
                    RoomSearchOrder::Created => RoomSearchCursor::Created {
                        created_ts: last_room.created_ts,
                        room_id: last_room.room_id.clone(),
                    },
                    RoomSearchOrder::Name => RoomSearchCursor::Name {
                        name: last_room.name.clone(),
                        created_ts: last_room.created_ts,
                        room_id: last_room.room_id.clone(),
                    },
                    RoomSearchOrder::Size => RoomSearchCursor::Size {
                        member_count: *last_count,
                        created_ts: last_room.created_ts,
                        room_id: last_room.room_id.clone(),
                    },
                };
                crate::room::encode_room_search_cursor(&cursor)
            })
        } else {
            None
        };

        filtered.truncate(limit.max(0) as usize);
        Ok((filtered, next_batch))
    }

    async fn get_user_room_list_summary(
        &self,
        _user_id: &str,
    ) -> Result<Vec<(String, String, String, String)>, sqlx::Error> {
        // InMemoryRoomStore does not track membership; return empty.
        Ok(Vec::new())
    }

    async fn delete_room(&self, room_id: &str) -> Result<(), sqlx::Error> {
        self.rooms.write().await.remove(room_id);
        Ok(())
    }

    async fn shutdown_room(&self, room_id: &str) -> Result<(), sqlx::Error> {
        if let Some(room) = self.rooms.write().await.get_mut(room_id) {
            room.name = Some("[room shutdown]".to_string());
        }
        Ok(())
    }

    // ⚠️ MOCK DEVIATION — room blocking is NOT modelled.
    //
    // The real store persists into `blocked_rooms`, and `get_room_block_status`
    // reads `blocked_at` back out (see `room/admin.rs`). These stubs deliberately
    // accept and discard everything:
    //
    //   block_room(&self, _room_id, _blocked_at, _blocked_by, _reason) -> Ok(())
    //   get_room_block_status(&self, _room_id)                         -> Ok(None)
    //   unblock_room(&self, _room_id)                                  -> Ok(())
    //
    // Consequently these dimensions are **untestable through this mock**:
    //   * that a blocked room reports its `blocked_at`
    //   * that unblock clears it
    //   * the `blocked_by` / `reason` round-trip
    //
    // Any test asserting block/unblock behaviour must use the real store against
    // a database. Note that `services::room::state::info` contains a test which
    // calls `block_room` and then asserts `None` — that assertion is vacuously
    // true against this mock and proves nothing (it is annotated accordingly).
    //
    // Adding a `blocked_rooms: HashMap<String, (i64, String, Option<String>)>`
    // field plus faithful implementations would be a small self-contained change;
    // it is not done here because no current test depends on it and the
    // deviation is now explicit rather than implicit.
    async fn block_room(
        &self,
        _room_id: &str,
        _blocked_at: i64,
        _blocked_by: &str,
        _reason: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        Ok(())
    }

    async fn get_room_block_status(&self, _room_id: &str) -> Result<Option<i64>, sqlx::Error> {
        // MOCK DEVIATION: always `None`; the real store reads `blocked_rooms`.
        Ok(None)
    }

    async fn unblock_room(&self, _room_id: &str) -> Result<(), sqlx::Error> {
        Ok(())
    }

    async fn get_room_stats_overview(&self) -> Result<serde_json::Value, sqlx::Error> {
        let rooms = self.rooms.read().await;
        let total = rooms.len();
        let public = rooms.values().filter(|r| r.is_public).count();
        Ok(serde_json::json!({
            "total_rooms": total,
            "public_rooms": public,
        }))
    }

    async fn get_single_room_stats(&self, room_id: &str) -> Result<Option<serde_json::Value>, sqlx::Error> {
        let rooms = self.rooms.read().await;
        Ok(rooms.get(room_id).map(|room| {
            serde_json::json!({
                "room_id": room.room_id,
                "name": room.name,
                "member_count": room.member_count,
                "is_public": room.is_public,
            })
        }))
    }

    async fn get_room_listings_status(&self, room_id: &str) -> Result<Option<(bool, bool)>, sqlx::Error> {
        let rooms = self.rooms.read().await;
        let is_public = rooms.get(room_id).map(|r| r.is_public);
        let directories = self.directories.read().await;
        let in_directory = directories.get(room_id).copied();
        match (is_public, in_directory) {
            (Some(p), Some(d)) => Ok(Some((p, d))),
            (Some(p), None) => Ok(Some((p, p))),
            (None, _) => Ok(None),
        }
    }

    async fn set_room_public_with_directory(&self, room_id: &str) -> Result<bool, sqlx::Error> {
        let mut rooms = self.rooms.write().await;
        if let Some(room) = rooms.get_mut(room_id) {
            room.is_public = true;
            drop(rooms);
            self.directories.write().await.insert(room_id.to_string(), true);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn set_room_private_with_directory(&self, room_id: &str) -> Result<bool, sqlx::Error> {
        let mut rooms = self.rooms.write().await;
        if let Some(room) = rooms.get_mut(room_id) {
            room.is_public = false;
            drop(rooms);
            self.directories.write().await.insert(room_id.to_string(), false);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn get_room_version_only(&self, room_id: &str) -> Result<Option<String>, sqlx::Error> {
        let rooms = self.rooms.read().await;
        Ok(rooms.get(room_id).map(|r| r.room_version.clone()))
    }

    async fn search_all_rooms_admin(
        &self,
        search_term: Option<&str>,
        limit: i64,
        _order_by: crate::room::RoomSearchOrder,
        _cursor: Option<crate::room::RoomSearchCursor>,
        is_public: Option<bool>,
        is_encrypted: Option<bool>,
    ) -> Result<(Vec<serde_json::Value>, i64, Option<String>), sqlx::Error> {
        let rooms = self.rooms.read().await;
        let term = search_term.map(|s| s.to_lowercase());
        let mut results: Vec<serde_json::Value> = rooms
            .values()
            .filter(|r| is_public.is_none_or(|p| r.is_public == p))
            .filter(|r| is_encrypted.is_none_or(|e| r.encryption.is_some() == e))
            .filter(|r| {
                term.as_ref().is_none_or(|t| {
                    r.name.as_deref().is_some_and(|n| n.to_lowercase().contains(t))
                        || r.topic.as_deref().is_some_and(|tp| tp.to_lowercase().contains(t))
                })
            })
            .map(|r| {
                serde_json::json!({
                    "room_id": r.room_id,
                    "name": r.name,
                    "topic": r.topic,
                    "creator": r.creator_user_id,
                    "is_public": r.is_public,
                    "creation_ts": r.created_ts,
                    "member_count": r.member_count,
                })
            })
            .collect();
        let total = results.len() as i64;
        results.truncate(limit as usize);
        Ok((results, total, None))
    }

    async fn cleanup_abnormal_data(&self, _min_age_ms: Option<i64>) -> Result<serde_json::Value, sqlx::Error> {
        Ok(serde_json::json!({"cleaned": 0}))
    }

    async fn get_user_rooms_paginated(
        &self,
        _user_id: &str,
        _limit: i64,
        _from_room_id: Option<&str>,
    ) -> Result<Vec<String>, sqlx::Error> {
        // InMemoryRoomStore does not track memberships; return empty.
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod pagination_tests {
    use super::InMemoryRoomStore;
    use crate::room::api::RoomStoreApi;
    use crate::room::{decode_room_search_cursor, RoomSearchOrder};

    /// Walk every page of a room search with the returned cursor and assert all
    /// rooms are visited exactly once.
    ///
    /// Regression guard for the gap documented in the P5 report §3.5: this mock
    /// used to ignore `from`/`order_by` entirely and always return `None` for
    /// next_batch, so a caller paging received page 1 again (and no test could
    /// exercise cursor pagination at all).
    #[tokio::test]
    async fn room_search_pagination_visits_every_room_exactly_once() {
        let store = InMemoryRoomStore::new();
        let mut expected = Vec::new();
        for i in 0..5 {
            let room_id = format!("!r{i}:t");
            store.create_room(&room_id, "@creator:t", "public", "10", true).await.unwrap();
            expected.push(room_id);
        }

        let mut seen: Vec<String> = Vec::new();
        let mut cursor = None;
        for _ in 0..10 {
            let (page, next) =
                store.get_all_rooms_with_members(2, cursor.clone(), RoomSearchOrder::Created).await.unwrap();
            if page.is_empty() {
                break;
            }
            seen.extend(page.into_iter().map(|(r, _)| r.room_id));
            match next {
                Some(token) => cursor = decode_room_search_cursor(Some(&token)),
                None => break,
            }
        }

        seen.sort();
        expected.sort();
        assert_eq!(seen, expected, "cursor pagination must visit every room exactly once");
    }

    /// A limit smaller than the set must produce a next_batch token; a limit at or
    /// above the set size must not.
    #[tokio::test]
    async fn room_search_next_batch_presence_follows_limit() {
        let store = InMemoryRoomStore::new();
        for i in 0..3 {
            store.create_room(&format!("!n{i}:t"), "@creator:t", "public", "10", true).await.unwrap();
        }

        let (page, next) = store.get_all_rooms_with_members(2, None, RoomSearchOrder::Created).await.unwrap();
        assert_eq!(page.len(), 2, "page honours the limit");
        assert!(next.is_some(), "a truncated page must yield next_batch");

        let (page_all, next_all) = store.get_all_rooms_with_members(10, None, RoomSearchOrder::Created).await.unwrap();
        assert_eq!(page_all.len(), 3, "limit above set size returns everything");
        assert!(next_all.is_none(), "a complete page must not yield next_batch");
    }
}
