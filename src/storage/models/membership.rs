use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct RoomMembership {
    pub id: i64,
    pub room_id: String,
    pub user_id: String,
    pub membership: String,
    pub joined_ts: Option<i64>,
    pub invited_ts: Option<i64>,
    pub left_ts: Option<i64>,
    pub banned_ts: Option<i64>,
    pub sender: Option<String>,
    pub reason: Option<String>,
    pub event_id: Option<String>,
    pub event_type: Option<String>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub is_banned: bool,
    pub invite_token: Option<String>,
    pub updated_ts: Option<i64>,
    pub join_reason: Option<String>,
    pub banned_by: Option<String>,
    pub ban_reason: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct PrivateSession {
    pub id: String,
    pub user_id_1: String,
    pub user_id_2: String,
    pub session_type: String,
    pub encryption_key: Option<String>,
    pub created_ts: i64,
    pub last_activity_ts: i64,
    pub updated_ts: Option<i64>,
    pub unread_count: i32,
    pub encrypted_content: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct PrivateMessage {
    pub id: i64,
    pub session_id: String,
    pub sender_id: String,
    pub content: String,
    pub encrypted_content: Option<String>,
    pub created_ts: i64,
    pub message_type: String,
    pub is_read: bool,
    pub read_by_receiver: bool,
    pub read_ts: Option<i64>, // 已修复: read_at → read_ts
    pub edit_history: Option<serde_json::Value>,
    pub is_deleted: bool,
    pub deleted_at: Option<i64>,
    pub is_edited: bool,
    pub unread_count: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_room_membership() {
        let membership = RoomMembership {
            id: 1,
            room_id: "!room:example.com".to_string(),
            user_id: "@alice:example.com".to_string(),
            membership: "join".to_string(),
            joined_ts: Some(1234567890000),
            invited_ts: Some(1234567800000),
            left_ts: None,
            banned_ts: None,
            sender: Some("@bob:example.com".to_string()),
            reason: None,
            event_id: Some("$event:example.com".to_string()),
            event_type: Some("m.room.member".to_string()),
            display_name: Some("Alice".to_string()),
            avatar_url: Some("mxc://example.com/avatar".to_string()),
            is_banned: false,
            invite_token: None,
            updated_ts: Some(1234567890000),
            join_reason: None,
            banned_by: None,
            ban_reason: None,
        };

        assert_eq!(membership.membership, "join");
        assert!(!membership.is_banned);
        assert!(membership.joined_ts.is_some());
    }

    #[test]
    fn test_private_session() {
        let session = PrivateSession {
            id: "session_123".to_string(),
            user_id_1: "@alice:example.com".to_string(),
            user_id_2: "@bob:example.com".to_string(),
            session_type: "direct".to_string(),
            encryption_key: Some("encrypted_key".to_string()),
            created_ts: 1234567890000,
            last_activity_ts: 1234567900000,
            updated_ts: Some(1234567900000),
            unread_count: 5,
            encrypted_content: None,
        };

        assert_eq!(session.session_type, "direct");
        assert_eq!(session.unread_count, 5);
    }

    #[test]
    fn test_private_message() {
        let message = PrivateMessage {
            id: 1,
            session_id: "session_123".to_string(),
            sender_id: "@alice:example.com".to_string(),
            content: "Hello!".to_string(),
            encrypted_content: None,
            created_ts: 1234567890000,
            message_type: "m.text".to_string(),
            is_read: false,
            read_by_receiver: false,
            read_ts: None, // 已修复
            edit_history: None,
            is_deleted: false,
            deleted_at: None,
            is_edited: false,
            unread_count: 1,
        };

        assert_eq!(message.content, "Hello!");
        assert!(!message.is_read);
        assert!(!message.is_deleted);
    }
}
