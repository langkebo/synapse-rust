use super::*;
use synapse_common::current_timestamp_millis;

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
        let now = current_timestamp_millis();
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
            member_count: Some(0),
            joined_member_count: Some(0),
            invited_member_count: Some(0),
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
        summary.updated_ts = Some(current_timestamp_millis());
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
        summary.updated_ts = Some(current_timestamp_millis());
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
        let now = current_timestamp_millis();
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
        let now = current_timestamp_millis();
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
        member.updated_ts = current_timestamp_millis();
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
        let now = current_timestamp_millis();
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
        let now = current_timestamp_millis();
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
        let now = current_timestamp_millis();
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
        let now = current_timestamp_millis();
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
            item.processed_ts = Some(current_timestamp_millis());
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
            s.updated_ts = Some(current_timestamp_millis());
        }
        Ok(())
    }

    async fn clear_unread_notifications(&self, room_id: &str) -> Result<(), sqlx::Error> {
        let mut summaries = self.summaries.write().await;
        if let Some(s) = summaries.get_mut(room_id) {
            s.unread_notifications = 0;
            s.unread_highlight = 0;
            s.updated_ts = Some(current_timestamp_millis());
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
