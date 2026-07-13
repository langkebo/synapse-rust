use super::*;

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

    async fn get_room_events_batch_since(
        &self,
        room_ids: &[String],
        since: crate::event::SinceFilter,
        limit_per_room: i64,
    ) -> Result<HashMap<String, Vec<crate::event::RoomEvent>>, sqlx::Error> {
        let events = self.events.read().await;
        let mut result: HashMap<String, Vec<crate::event::RoomEvent>> =
            room_ids.iter().map(|id| (id.clone(), Vec::new())).collect();
        for (_eid, event) in events.iter() {
            match since {
                crate::event::SinceFilter::OriginServerTs(ts) => {
                    if event.origin_server_ts <= ts {
                        continue;
                    }
                }
                crate::event::SinceFilter::StreamOrdering(so) => {
                    if event.stream_ordering.unwrap_or(0) <= so {
                        continue;
                    }
                }
            }
            if let Some(bucket) = result.get_mut(&event.room_id) {
                if bucket.len() < limit_per_room as usize {
                    bucket.push(event.clone());
                }
            }
        }
        match since {
            crate::event::SinceFilter::OriginServerTs(_) => {
                for bucket in result.values_mut() {
                    bucket.sort_by_key(|e| e.origin_server_ts);
                }
            }
            crate::event::SinceFilter::StreamOrdering(_) => {
                for bucket in result.values_mut() {
                    bucket.sort_by_key(|e| e.stream_ordering.unwrap_or(0));
                }
            }
        }
        Ok(result)
    }

    async fn get_room_events_batch_since_filtered(
        &self,
        room_ids: &[String],
        since: crate::event::SinceFilter,
        limit_per_room: i64,
        _filter: &crate::event::EventQueryFilter,
    ) -> Result<HashMap<String, Vec<crate::event::RoomEvent>>, sqlx::Error> {
        self.get_room_events_batch_since(room_ids, since, limit_per_room).await
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

#[async_trait::async_trait]
impl crate::event::reader::EventReader for InMemoryEventStore {
    fn pool(&self) -> &Arc<sqlx::PgPool> {
        <Self as crate::event::api::EventStoreApi>::pool(self)
    }

    async fn get_event(&self, event_id: &str) -> Result<Option<crate::event::RoomEvent>, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::get_event(self, event_id).await
    }

    async fn get_room_events(&self, room_id: &str, limit: i64) -> Result<Vec<crate::event::RoomEvent>, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::get_room_events(self, room_id, limit).await
    }

    async fn get_room_events_paginated(
        &self,
        room_id: &str,
        from: Option<i64>,
        limit: i64,
        direction: &str,
    ) -> Result<Vec<crate::event::RoomEvent>, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::get_room_events_paginated(self, room_id, from, limit, direction)
            .await
    }

    async fn get_room_events_batch(
        &self,
        room_ids: &[String],
        limit_per_room: i64,
    ) -> Result<std::collections::HashMap<String, Vec<crate::event::RoomEvent>>, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::get_room_events_batch(self, room_ids, limit_per_room).await
    }

    async fn get_room_events_batch_since(
        &self,
        room_ids: &[String],
        since: crate::event::SinceFilter,
        limit_per_room: i64,
    ) -> Result<std::collections::HashMap<String, Vec<crate::event::RoomEvent>>, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::get_room_events_batch_since(self, room_ids, since, limit_per_room)
            .await
    }

    async fn get_room_events_batch_since_filtered(
        &self,
        room_ids: &[String],
        since: crate::event::SinceFilter,
        limit_per_room: i64,
        filter: &crate::event::EventQueryFilter,
    ) -> Result<std::collections::HashMap<String, Vec<crate::event::RoomEvent>>, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::get_room_events_batch_since_filtered(
            self,
            room_ids,
            since,
            limit_per_room,
            filter,
        )
        .await
    }

    async fn get_state_event(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
    ) -> Result<Option<crate::event::StateEvent>, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::get_state_event(self, room_id, event_type, state_key).await
    }

    async fn get_state_events(&self, room_id: &str) -> Result<Vec<crate::event::StateEvent>, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::get_state_events(self, room_id).await
    }

    async fn get_state_events_by_type(
        &self,
        room_id: &str,
        event_type: &str,
    ) -> Result<Vec<crate::event::StateEvent>, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::get_state_events_by_type(self, room_id, event_type).await
    }

    async fn get_state_events_at_or_before(
        &self,
        room_id: &str,
        origin_server_ts: i64,
    ) -> Result<Vec<crate::event::StateEvent>, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::get_state_events_at_or_before(self, room_id, origin_server_ts).await
    }

    async fn get_events_map(
        &self,
        event_ids: &[String],
    ) -> Result<std::collections::HashMap<String, crate::event::RoomEvent>, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::get_events_map(self, event_ids).await
    }

    async fn get_max_origin_server_ts_for_room(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::get_max_origin_server_ts_for_room(self, room_id).await
    }

    async fn get_latest_event_ids_in_room(&self, room_id: &str, limit: i64) -> Result<Vec<String>, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::get_latest_event_ids_in_room(self, room_id, limit).await
    }

    async fn count_room_events_by_status(&self, room_id: &str, status: &str) -> Result<i64, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::count_room_events_by_status(self, room_id, status).await
    }

    async fn get_ephemeral_events(
        &self,
        room_id: &str,
        now: i64,
        limit: i64,
    ) -> Result<Vec<crate::event::RoomEphemeralEvent>, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::get_ephemeral_events(self, room_id, now, limit).await
    }

    async fn get_ephemeral_events_batch(
        &self,
        room_ids: &[String],
        now: i64,
        limit: i64,
    ) -> Result<std::collections::HashMap<String, Vec<crate::event::RoomEphemeralEvent>>, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::get_ephemeral_events_batch(self, room_ids, now, limit).await
    }

    async fn get_state_events_batch(
        &self,
        room_ids: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<crate::event::StateEvent>>, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::get_state_events_batch(self, room_ids).await
    }

    async fn get_state_events_by_type_batch(
        &self,
        room_ids: &[String],
        event_type: &str,
    ) -> Result<std::collections::HashMap<String, Vec<crate::event::StateEvent>>, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::get_state_events_by_type_batch(self, room_ids, event_type).await
    }

    async fn get_state_events_since_batch(
        &self,
        room_ids: &[String],
        since: crate::event::SinceFilter,
    ) -> Result<std::collections::HashMap<String, Vec<crate::event::StateEvent>>, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::get_state_events_since_batch(self, room_ids, since).await
    }

    async fn get_membership_state_keys_since_batch(
        &self,
        room_ids: &[String],
        since: crate::event::SinceFilter,
    ) -> Result<std::collections::HashMap<String, std::collections::HashSet<String>>, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::get_membership_state_keys_since_batch(self, room_ids, since).await
    }

    async fn get_state_change_timestamps_batch(
        &self,
        room_ids: &[String],
        since: crate::event::SinceFilter,
    ) -> Result<std::collections::HashMap<String, i64>, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::get_state_change_timestamps_batch(self, room_ids, since).await
    }

    async fn get_room_events_batch_filtered(
        &self,
        room_ids: &[String],
        limit_per_room: i64,
        filter: &crate::event::EventQueryFilter,
    ) -> Result<std::collections::HashMap<String, Vec<crate::event::RoomEvent>>, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::get_room_events_batch_filtered(
            self,
            room_ids,
            limit_per_room,
            filter,
        )
        .await
    }

    async fn has_room_events_since(&self, room_ids: &[String], since: i64) -> Result<bool, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::has_room_events_since(self, room_ids, since).await
    }

    async fn find_missing_event_ids(&self, event_ids: &[String]) -> Result<Vec<String>, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::find_missing_event_ids(self, event_ids).await
    }

    async fn get_missing_events_between(
        &self,
        room_id: &str,
        earliest_events: &[String],
        latest_events: &[String],
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::get_missing_events_between(
            self,
            room_id,
            earliest_events,
            latest_events,
            limit,
        )
        .await
    }

    async fn get_forward_extremities_count(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::get_forward_extremities_count(self, room_id).await
    }

    async fn find_event_id_by_timestamp(
        &self,
        room_id: &str,
        ts: i64,
        forward: bool,
    ) -> Result<Option<(String, i64)>, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::find_event_id_by_timestamp(self, room_id, ts, forward).await
    }

    async fn get_events_before_context(
        &self,
        room_id: &str,
        before_ts: i64,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::get_events_before_context(self, room_id, before_ts, limit).await
    }

    async fn get_events_after_context(
        &self,
        room_id: &str,
        after_ts: i64,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::get_events_after_context(self, room_id, after_ts, limit).await
    }

    async fn get_room_events_by_type(
        &self,
        room_id: &str,
        event_type: &str,
        limit: i64,
    ) -> Result<Vec<crate::event::RoomEvent>, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::get_room_events_by_type(self, room_id, event_type, limit).await
    }

    async fn get_pending_room_events(
        &self,
        room_id: &str,
        limit: i64,
    ) -> Result<Vec<crate::event::RoomEvent>, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::get_pending_room_events(self, room_id, limit).await
    }

    async fn get_daily_message_count(&self) -> Result<i64, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::get_daily_message_count(self).await
    }

    async fn get_event_signatures(&self, event_id: &str) -> Result<Vec<crate::event::EventSignature>, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::get_event_signatures(self, event_id).await
    }

    async fn search_room_messages_admin(
        &self,
        room_id: &str,
        search_pattern: &str,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::search_room_messages_admin(self, room_id, search_pattern, limit)
            .await
    }

    async fn check_room_has_encryption(&self, room_id: &str) -> Result<bool, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::check_room_has_encryption(self, room_id).await
    }
}

#[async_trait::async_trait]
impl crate::event::writer::EventWriter for InMemoryEventStore {
    fn pool(&self) -> &Arc<sqlx::PgPool> {
        <Self as crate::event::api::EventStoreApi>::pool(self)
    }

    async fn create_event(
        &self,
        params: crate::event::CreateEventParams,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<crate::event::RoomEvent, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::create_event(self, params, tx).await
    }

    async fn update_event_signatures_and_hashes(
        &self,
        event_id: &str,
        signatures: &serde_json::Value,
        hashes: &serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::update_event_signatures_and_hashes(
            self, event_id, signatures, hashes,
        )
        .await
    }

    async fn redact_event_content(&self, event_id: &str, redacted_by: Option<&str>) -> Result<(), sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::redact_event_content(self, event_id, redacted_by).await
    }

    #[allow(clippy::too_many_arguments)]
    async fn create_event_with_graph(
        &self,
        params: crate::event::CreateEventParams,
        prev_events: &[String],
        auth_events: &[String],
        depth: i64,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<crate::event::RoomEvent, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::create_event_with_graph(
            self,
            params,
            prev_events,
            auth_events,
            depth,
            tx,
        )
        .await
    }

    async fn save_event_signature(
        &self,
        event_id: &str,
        user_id: &str,
        device_id: &str,
        signature: &str,
        key_id: &str,
        algorithm: &str,
        created_ts: i64,
    ) -> Result<(), sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::save_event_signature(
            self, event_id, user_id, device_id, signature, key_id, algorithm, created_ts,
        )
        .await
    }

    async fn report_event(
        &self,
        event_id: &str,
        room_id: &str,
        reported_user_id: &str,
        reporter_user_id: &str,
        reason: Option<&str>,
        score: i32,
    ) -> Result<i64, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::report_event(
            self,
            event_id,
            room_id,
            reported_user_id,
            reporter_user_id,
            reason,
            score,
        )
        .await
    }

    async fn add_ephemeral_event(
        &self,
        room_id: &str,
        user_id: &str,
        event_type: &str,
        content: &serde_json::Value,
        stream_id: i64,
    ) -> Result<(), sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::add_ephemeral_event(
            self, room_id, user_id, event_type, content, stream_id,
        )
        .await
    }

    async fn upsert_ephemeral_event(
        &self,
        room_id: &str,
        user_id: &str,
        event_type: &str,
        content: &serde_json::Value,
        stream_id: i64,
        created_ts: i64,
        expires_at: Option<i64>,
    ) -> Result<(), sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::upsert_ephemeral_event(
            self, room_id, user_id, event_type, content, stream_id, created_ts, expires_at,
        )
        .await
    }

    async fn delete_ephemeral_event(&self, room_id: &str, event_type: &str, user_id: &str) -> Result<(), sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::delete_ephemeral_event(self, room_id, event_type, user_id).await
    }

    async fn delete_events_before(&self, room_id: &str, timestamp: i64) -> Result<u64, sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::delete_events_before(self, room_id, timestamp).await
    }

    async fn upsert_power_levels_event(
        &self,
        event_id: &str,
        room_id: &str,
        user_id: &str,
        content: serde_json::Value,
        origin_server_ts: i64,
        sender: &str,
    ) -> Result<(), sqlx::Error> {
        <Self as crate::event::api::EventStoreApi>::upsert_power_levels_event(
            self,
            event_id,
            room_id,
            user_id,
            content,
            origin_server_ts,
            sender,
        )
        .await
    }
}
