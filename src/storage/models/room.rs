use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct Room {
    pub room_id: String,
    pub creator: Option<String>,
    pub is_public: bool,
    pub room_version: String,
    pub created_ts: i64,
    pub last_activity_ts: Option<i64>,
    pub is_federated: bool,
    pub has_guest_access: bool,
    pub join_rules: String,
    pub history_visibility: String,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub canonical_alias: Option<String>,
    // 注意: member_count 字段已移除，请使用 room_summaries.member_count
    pub visibility: String,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct RoomSummary {
    pub room_id: String,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub canonical_alias: Option<String>,
    pub joined_members: i64,
    pub invited_members: i64,
    pub hero_users: Option<serde_json::Value>,
    pub is_world_readable: bool,
    pub can_guest_join: bool,
    pub is_federated: bool,
    pub encryption_state: Option<String>,
    pub updated_ts: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct RoomDirectory {
    pub id: i64,
    pub room_id: String,
    pub is_public: bool,
    pub is_searchable: bool,
    pub app_service_id: Option<String>,
    pub added_ts: i64,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct RoomAlias {
    pub room_alias: String,
    pub room_id: String,
    pub server_name: String,
    pub created_ts: i64,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct ThreadRoot {
    pub id: i64,
    pub room_id: String,
    pub event_id: String,
    pub sender: String,
    pub thread_id: Option<String>,
    pub reply_count: i64,
    pub last_reply_event_id: Option<String>,
    pub last_reply_sender: Option<String>,
    pub last_reply_ts: Option<i64>,
    pub participants: Option<serde_json::Value>,
    pub is_fetched: bool,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct ThreadStatistics {
    pub id: i64,
    pub room_id: String,
    pub thread_root_event_id: String,
    pub reply_count: i64,
    pub last_reply_event_id: Option<String>,
    pub last_reply_sender: Option<String>,
    pub last_reply_at: Option<i64>,
    pub participants: Option<serde_json::Value>,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct RoomParent {
    pub id: i64,
    pub room_id: String,
    pub parent_room_id: String,
    pub sender: String,
    pub is_suggested: bool,
    pub via_servers: Option<serde_json::Value>,
    pub added_ts: i64,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct SpaceChild {
    pub id: i64,
    pub space_id: String,
    pub room_id: String,
    pub sender: String,
    pub is_suggested: bool,
    pub via_servers: Option<serde_json::Value>,
    pub added_ts: i64,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct RoomInvite {
    pub id: i64,
    pub room_id: String,
    pub inviter: String,
    pub invitee: String,
    pub is_accepted: bool,
    pub accepted_at: Option<i64>,
    pub created_ts: i64,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct RoomTag {
    pub id: i64,
    pub user_id: String,
    pub room_id: String,
    pub tag: String,
    pub order: Option<f64>,
    pub created_ts: i64,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct ReadMarker {
    pub id: i64,
    pub room_id: String,
    pub user_id: String,
    pub event_id: String,
    pub marker_type: String,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct RoomStateEvent {
    pub id: i64,
    pub room_id: String,
    pub r#type: String,
    pub state_key: String,
    pub content: serde_json::Value,
    pub sender: String,
    pub origin_server_ts: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_room_struct() {
        let room = Room {
            room_id: "!room:example.com".to_string(),
            creator: Some("@alice:example.com".to_string()),
            is_public: false,
            room_version: "6".to_string(),
            created_ts: 1234567890,
            last_activity_ts: Some(1234567900),
            is_federated: true,
            has_guest_access: false,
            join_rules: "invite".to_string(),
            history_visibility: "shared".to_string(),
            name: Some("Test Room".to_string()),
            topic: Some("A test room".to_string()),
            avatar_url: Some("mxc://example.com/avatar".to_string()),
            canonical_alias: Some("#test:example.com".to_string()),
            // 注意: member_count 字段已移除
            visibility: "private".to_string(),
        };

        assert_eq!(room.room_id, "!room:example.com");
        assert_eq!(room.name, Some("Test Room".to_string()));
        assert!(!room.is_public);
    }

    #[test]
    fn test_room_summary() {
        let summary = RoomSummary {
            room_id: "!room:example.com".to_string(),
            name: Some("Test Room".to_string()),
            topic: None,
            canonical_alias: None,
            joined_members: 10,
            invited_members: 2,
            hero_users: Some(serde_json::json!(["@alice:example.com"])),
            is_world_readable: false,
            can_guest_join: false,
            is_federated: true,
            encryption_state: Some("m.megolm.v1.aes-sha2".to_string()),
            updated_ts: Some(1234567890),
        };

        assert_eq!(summary.joined_members, 10);
        assert!(summary.encryption_state.is_some());
    }

    #[test]
    fn test_room_alias() {
        let alias = RoomAlias {
            room_alias: "#test:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            server_name: "example.com".to_string(),
            created_ts: 1234567890,
        };

        assert_eq!(alias.room_alias, "#test:example.com");
        assert_eq!(alias.server_name, "example.com");
    }

    #[test]
    fn test_thread_root() {
        let thread = ThreadRoot {
            id: 1,
            room_id: "!room:example.com".to_string(),
            event_id: "$event:example.com".to_string(),
            sender: "@alice:example.com".to_string(),
            thread_id: Some("thread_123".to_string()),
            reply_count: 5,
            last_reply_event_id: Some("$reply:example.com".to_string()),
            last_reply_sender: Some("@bob:example.com".to_string()),
            last_reply_ts: Some(1234567890),
            participants: Some(serde_json::json!(["@alice:example.com", "@bob:example.com"])),
            is_fetched: true,
            created_ts: 1234567800,
            updated_ts: Some(1234567890),
        };

        assert_eq!(thread.reply_count, 5);
        assert!(thread.is_fetched);
        assert!(thread.participants.is_some());
    }

    #[test]
    fn test_space_child() {
        let child = SpaceChild {
            id: 1,
            space_id: "!space:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            sender: "@alice:example.com".to_string(),
            is_suggested: true,
            via_servers: Some(serde_json::json!(["example.com"])),
            added_ts: 1234567890,
        };

        assert_eq!(child.space_id, "!space:example.com");
        assert!(child.is_suggested);
    }
}
