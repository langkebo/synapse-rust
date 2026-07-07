use super::*;

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
