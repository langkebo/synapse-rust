use super::types::*;
use super::SyncService;
use crate::storage::RoomEvent;
use serde_json::Value;
use std::collections::HashSet;

impl SyncService {
    pub(crate) async fn get_known_lazy_loaded_members(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        room_id: &str,
    ) -> HashSet<String> {
        let cache_key = LazyLoadedMembersCacheKey::new(user_id, device_id, room_id);
        {
            let cache = self.lazy_loaded_members_cache.read().await;
            if let Some(known_members) = cache.get(&cache_key) {
                return known_members.clone();
            }
        }

        let known_members = match device_id {
            Some(device_id) => {
                self.device_storage.get_lazy_loaded_members(user_id, device_id, room_id).await.unwrap_or_default()
            }
            None => HashSet::new(),
        };

        let mut cache = self.lazy_loaded_members_cache.write().await;
        cache.insert(cache_key, known_members.clone());
        known_members
    }

    pub(crate) async fn persist_lazy_loaded_members(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        room_id: &str,
        known_members: &HashSet<String>,
    ) {
        if known_members.is_empty() {
            return;
        }

        if let Some(device_id) = device_id {
            if let Err(e) = self.device_storage.upsert_lazy_loaded_members(user_id, device_id, room_id, known_members).await {
                ::tracing::warn!(%e, user_id, device_id, room_id, "Failed to upsert lazy loaded members");
            }
        }
    }

    pub(crate) async fn apply_lazy_load_members(&self, request: LazyLoadMembersRequest<'_>) -> Vec<Value> {
        let LazyLoadMembersRequest {
            state_events,
            timeline_events,
            user_id,
            device_id,
            room_id,
            room_filter,
            changed_member_ids,
            timeline_limited,
            enabled,
        } = request;
        if !enabled {
            return state_events;
        }

        let cache_key = LazyLoadedMembersCacheKey::new(user_id, device_id, room_id);
        let known_members = self.get_known_lazy_loaded_members(user_id, device_id, room_id).await;
        let include_redundant_members = Self::room_filter_requests_redundant_members(room_filter);
        let changed_member_ids = changed_member_ids.filter(|_| !timeline_limited).cloned().unwrap_or_default();
        let (filtered_events, known_now) = Self::apply_lazy_load_members_with_cache(
            state_events,
            timeline_events,
            user_id,
            &known_members,
            include_redundant_members,
            &changed_member_ids,
            timeline_limited,
        );

        if !known_now.is_empty() {
            let mut cache = self.lazy_loaded_members_cache.write().await;
            cache.entry(cache_key).or_default().extend(known_now.iter().cloned());
        }
        self.persist_lazy_loaded_members(user_id, device_id, room_id, &known_now).await;

        filtered_events
    }

    pub(crate) fn apply_lazy_load_members_with_cache(
        state_events: Vec<Value>,
        timeline_events: &[RoomEvent],
        user_id: &str,
        known_members: &HashSet<String>,
        include_redundant_members: bool,
        changed_member_ids: &HashSet<String>,
        timeline_limited: bool,
    ) -> (Vec<Value>, HashSet<String>) {
        if timeline_limited {
            let known_now: HashSet<String> = state_events
                .iter()
                .filter(|event| event.get("type").and_then(|value| value.as_str()) == Some("m.room.member"))
                .filter_map(|event| event.get("state_key").and_then(|value| value.as_str()))
                .map(|s| s.to_string())
                .collect();
            let filtered_events = state_events
                .into_iter()
                .filter(|event| {
                    if event.get("type").and_then(|value| value.as_str()) != Some("m.room.member") {
                        return true;
                    }
                    let Some(state_key) = event.get("state_key").and_then(|value| value.as_str()) else {
                        return false;
                    };
                    include_redundant_members || !known_members.contains(state_key)
                })
                .collect();
            return (filtered_events, known_now);
        }

        let mut required_members: HashSet<&str> = HashSet::from([user_id]);
        for event in timeline_events {
            required_members.insert(event.user_id.as_str());
            if event.event_type == "m.room.member" {
                if let Some(state_key) = event.state_key.as_deref() {
                    required_members.insert(state_key);
                }
            }
        }
        for user_id in changed_member_ids {
            required_members.insert(user_id.as_str());
        }

        let mut known_now: HashSet<String> = timeline_events
            .iter()
            .filter(|event| event.event_type == "m.room.member")
            .filter_map(|event| event.state_key.clone())
            .collect();
        let filtered_events = state_events
            .into_iter()
            .filter(|event| {
                if event.get("type").and_then(|value| value.as_str()) != Some("m.room.member") {
                    return true;
                }

                let Some(state_key) = event.get("state_key").and_then(|value| value.as_str()) else {
                    return false;
                };
                if !required_members.contains(state_key) {
                    return false;
                }

                known_now.insert(state_key.to_string());
                include_redundant_members
                    || changed_member_ids.contains(state_key)
                    || !known_members.contains(state_key)
            })
            .collect();

        (filtered_events, known_now)
    }
}
