use super::*;
use synapse_common::current_timestamp_millis;

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
        let now = current_timestamp_millis();
        let joined_ts = if membership == "join" { Some(now) } else { None };
        let member = crate::membership::RoomMember {
            room_id: room_id.to_string(),
            user_id: user_id.to_string(),
            sender: sender.map(|s| s.to_string()),
            membership: membership.to_string(),
            event_id: Some(format!("$auto_{}", current_timestamp_millis())),
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

    // ── Extended membership queries (in-memory implementations) ──

    async fn get_joined_members(&self, room_id: &str) -> Result<Vec<crate::membership::RoomMember>, sqlx::Error> {
        let members = self.members.read().await;
        Ok(members.values().filter(|m| m.room_id == room_id && m.membership == "join").cloned().collect())
    }

    async fn get_room_members_with_profiles(
        &self,
        room_id: &str,
        membership_type: &str,
    ) -> Result<Vec<(crate::membership::RoomMember, Option<String>, Option<String>)>, sqlx::Error> {
        let members = self.members.read().await;
        Ok(members
            .values()
            .filter(|m| m.room_id == room_id && m.membership == membership_type)
            .map(|m| (m.clone(), m.display_name.clone(), m.avatar_url.clone()))
            .collect())
    }

    async fn get_membership_history(
        &self,
        room_id: &str,
        limit: i64,
    ) -> Result<Vec<crate::membership::RoomMember>, sqlx::Error> {
        let members = self.members.read().await;
        let mut history: Vec<crate::membership::RoomMember> =
            members.values().filter(|m| m.room_id == room_id).cloned().collect();
        history.sort_by(|a, b| b.updated_ts.cmp(&a.updated_ts));
        history.truncate(limit as usize);
        Ok(history)
    }

    async fn get_membership_state(&self, room_id: &str, user_id: &str) -> Result<Option<String>, sqlx::Error> {
        Ok(self.members.read().await.get(&(room_id.to_string(), user_id.to_string())).map(|m| m.membership.clone()))
    }

    async fn get_room_members_paginated(
        &self,
        room_id: &str,
        membership_type: &str,
        limit: i64,
        from_user_id: Option<&str>,
    ) -> Result<Vec<crate::membership::RoomMember>, sqlx::Error> {
        let members = self.members.read().await;
        let mut filtered: Vec<crate::membership::RoomMember> = members
            .values()
            .filter(|m| m.room_id == room_id && m.membership == membership_type)
            .filter(|m| from_user_id.is_none_or(|from| m.user_id.as_str() > from))
            .cloned()
            .collect();
        filtered.sort_by(|a, b| a.user_id.cmp(&b.user_id));
        filtered.truncate(limit as usize);
        Ok(filtered)
    }

    async fn get_room_member_count(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        let members = self.members.read().await;
        Ok(members.values().filter(|m| m.room_id == room_id && m.membership == "join").count() as i64)
    }

    async fn share_common_room(&self, user_id_1: &str, user_id_2: &str) -> Result<bool, sqlx::Error> {
        let members = self.members.read().await;
        let rooms_1: std::collections::HashSet<String> = members
            .iter()
            .filter(|((_, uid), m)| uid == user_id_1 && m.membership == "join")
            .map(|((rid, _), _)| rid.clone())
            .collect();
        Ok(members.iter().any(|((rid, uid), m)| uid == user_id_2 && m.membership == "join" && rooms_1.contains(rid)))
    }

    async fn share_common_rooms_batch(
        &self,
        user_id: &str,
        other_user_ids: &[String],
    ) -> Result<Vec<String>, sqlx::Error> {
        let members = self.members.read().await;
        let user_rooms: std::collections::HashSet<String> = members
            .iter()
            .filter(|((_, uid), m)| uid == user_id && m.membership == "join")
            .map(|((rid, _), _)| rid.clone())
            .collect();
        let mut shared: Vec<String> = members
            .iter()
            .filter(|((_, uid), m)| {
                other_user_ids.iter().any(|o| o == uid) && m.membership == "join" && user_rooms.contains(&m.room_id)
            })
            .map(|((_, uid), _)| uid.clone())
            .collect();
        shared.sort();
        shared.dedup();
        Ok(shared)
    }

    async fn has_any_non_banned_member_from_server(
        &self,
        room_id: &str,
        server_name: &str,
    ) -> Result<bool, sqlx::Error> {
        let members = self.members.read().await;
        Ok(members.values().any(|m| {
            m.room_id == room_id
                && m.membership != "ban"
                && m.user_id.rsplit_once(':').is_some_and(|(_, srv)| srv == server_name)
        }))
    }

    async fn user_shares_room_with_server(&self, user_id: &str, server_name: &str) -> Result<bool, sqlx::Error> {
        let members = self.members.read().await;
        let user_rooms: std::collections::HashSet<String> = members
            .iter()
            .filter(|((_, uid), m)| uid == user_id && m.membership == "join")
            .map(|((rid, _), _)| rid.clone())
            .collect();
        Ok(members.iter().any(|((rid, uid), m)| {
            m.membership == "join"
                && user_rooms.contains(rid)
                && uid.rsplit_once(':').is_some_and(|(_, srv)| srv == server_name)
        }))
    }

    async fn filter_users_sharing_room_with_server(
        &self,
        user_ids: &[String],
        server_name: &str,
    ) -> Result<std::collections::HashSet<String>, sqlx::Error> {
        let members = self.members.read().await;
        let mut result = std::collections::HashSet::new();
        for user_id in user_ids {
            let user_rooms: std::collections::HashSet<String> = members
                .iter()
                .filter(|((_, uid), m)| uid == user_id && m.membership == "join")
                .map(|((rid, _), _)| rid.clone())
                .collect();
            let shares = members.iter().any(|((rid, _), m)| {
                m.membership == "join"
                    && user_rooms.contains(rid)
                    && m.user_id.rsplit_once(':').is_some_and(|(_, srv)| srv == server_name)
            });
            if shares {
                result.insert(user_id.clone());
            }
        }
        Ok(result)
    }

    async fn ban_member(&self, room_id: &str, user_id: &str, banned_by: &str) -> Result<(), sqlx::Error> {
        let mut members = self.members.write().await;
        if let Some(member) = members.get_mut(&(room_id.to_string(), user_id.to_string())) {
            member.membership = "ban".to_string();
            member.is_banned = Some(true);
            member.banned_by = Some(banned_by.to_string());
            member.banned_ts = Some(current_timestamp_millis());
        }
        Ok(())
    }

    async fn unban_member(&self, room_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
        let mut members = self.members.write().await;
        if let Some(member) = members.get_mut(&(room_id.to_string(), user_id.to_string())) {
            if member.membership == "ban" {
                member.membership = "leave".to_string();
                member.banned_by = None;
                member.is_banned = Some(false);
            }
        }
        Ok(())
    }

    async fn set_ban_reason(&self, room_id: &str, user_id: &str, reason: &str) -> Result<(), sqlx::Error> {
        let mut members = self.members.write().await;
        if let Some(member) = members.get_mut(&(room_id.to_string(), user_id.to_string())) {
            member.ban_reason = Some(reason.to_string());
        }
        Ok(())
    }

    async fn force_leave_membership(&self, room_id: &str, user_id: &str, now: i64) -> Result<(), sqlx::Error> {
        let mut members = self.members.write().await;
        if let Some(member) = members.get_mut(&(room_id.to_string(), user_id.to_string())) {
            member.membership = "leave".to_string();
            member.left_ts = Some(now);
        }
        Ok(())
    }

    async fn forget_member(&self, room_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
        let mut members = self.members.write().await;
        if let Some(member) = members.get_mut(&(room_id.to_string(), user_id.to_string())) {
            member.membership = "forget".to_string();
        }
        Ok(())
    }

    async fn remove_all_members(&self, room_id: &str) -> Result<(), sqlx::Error> {
        let mut members = self.members.write().await;
        members.retain(|(rid, _), _| rid != room_id);
        Ok(())
    }

    async fn get_joined_servers_in_room(
        &self,
        room_id: &str,
        local_server_name: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        let members = self.members.read().await;
        let mut servers: std::collections::HashSet<String> = std::collections::HashSet::new();
        for ((rid, _), member) in members.iter() {
            if rid == room_id && member.membership == "join" {
                if let Some(pos) = member.user_id.find(':') {
                    let server = &member.user_id[pos + 1..];
                    if server != local_server_name {
                        servers.insert(server.to_string());
                    }
                }
            }
        }
        Ok(servers.into_iter().collect())
    }
}

// =============================================================================
// Phase 3 complete: all storage traits extracted
// =============================================================================
//
// EventReader (event/reader.rs) + EventWriter (event/writer.rs) — 47 methods
//   covering single-event, bulk-read, state events, helpers, and mutation operations.
// RoomStoreApi (room/api.rs) — 11 methods covering room CRUD, aliases, join rules,
//   and member counts.
// MemberStoreApi (membership/api.rs) — 6 methods covering member queries, batches,
//   shared-room discovery, sync, and removal.
//
// Remaining work: update service consumers from Arc<ConcreteType> to Arc<dyn Trait>.
