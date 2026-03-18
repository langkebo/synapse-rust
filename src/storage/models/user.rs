use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct User {
    pub user_id: String,
    pub username: String,
    pub password_hash: Option<String>,
    pub is_admin: bool,
    pub is_guest: bool,
    pub is_shadow_banned: bool,
    pub is_deactivated: bool,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
    pub displayname: Option<String>,
    pub avatar_url: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub generation: i64,
    pub consent_version: Option<String>,
    pub appservice_id: Option<String>,
    pub user_type: Option<String>,
    pub invalid_update_at: Option<i64>,
    pub migration_state: Option<String>,
    pub password_changed_ts: Option<i64>,
    pub must_change_password: bool,
    pub password_expires_at: Option<i64>,
    pub failed_login_attempts: i32,
    pub locked_until: Option<i64>,
}

impl User {
    pub fn user_id(&self) -> String {
        self.user_id.clone()
    }
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct UserThreepid {
    pub id: i64,
    pub user_id: String,
    pub medium: String,
    pub address: String,
    pub validated_at: Option<i64>,
    pub added_ts: i64,
    pub is_verified: bool,
    pub verification_token: Option<String>,
    pub verification_expires_at: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct UserSearchResult {
    pub user_id: String,
    pub username: String,
    pub displayname: Option<String>,
    pub avatar_url: Option<String>,
    pub created_ts: i64,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct UserProfile {
    pub user_id: String,
    pub username: String,
    pub displayname: Option<String>,
    pub avatar_url: Option<String>,
    pub created_ts: i64,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct UserSearchResultWithPresence {
    pub user_id: String,
    pub username: String,
    pub displayname: Option<String>,
    pub avatar_url: Option<String>,
    pub created_ts: i64,
    pub presence: Option<String>,
    pub last_active_ts: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct Friend {
    pub id: i64,
    pub user_id: String,
    pub friend_id: String,
    pub created_ts: i64,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct FriendRequest {
    pub id: i64,
    pub sender_id: String,
    pub receiver_id: String,
    pub message: Option<String>,
    pub status: String,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct FriendCategory {
    pub id: i64,
    pub user_id: String,
    pub name: String,
    pub color: String,
    pub created_ts: i64,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct BlockedUser {
    pub id: i64,
    pub user_id: String,
    pub blocked_id: String,
    pub reason: Option<String>,
    pub created_ts: i64,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct Presence {
    pub user_id: String,
    pub status_msg: Option<String>,
    pub presence: String,
    pub last_active_ts: i64,
    pub status_from: Option<String>,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct UserDirectory {
    pub user_id: String,
    pub room_id: String,
    pub visibility: String,
    pub added_by: Option<String>,
    pub created_ts: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_struct() {
        let user = User {
            user_id: "@alice:example.com".to_string(),
            username: "alice".to_string(),
            password_hash: Some("hashed".to_string()),
            is_admin: false,
            is_guest: false,
            is_shadow_banned: false,
            is_deactivated: false,
            created_ts: 1234567890,
            updated_ts: None,
            displayname: Some("Alice".to_string()),
            avatar_url: Some("mxc://example.com/avatar".to_string()),
            email: Some("alice@example.com".to_string()),
            phone: None,
            generation: 1234567890,
            consent_version: None,
            appservice_id: None,
            user_type: None,
            invalid_update_at: None,
            migration_state: None,
            password_changed_ts: None,
            must_change_password: false,
            password_expires_at: None,
            failed_login_attempts: 0,
            locked_until: None,
        };

        assert_eq!(user.user_id(), "@alice:example.com");
        assert_eq!(user.username, "alice");
        assert!(user.password_hash.is_some());
        assert!(!user.is_admin);
        assert!(!user.is_guest);
        assert!(!user.must_change_password);
        assert_eq!(user.failed_login_attempts, 0);
    }

    #[test]
    fn test_user_threepid() {
        let threepid = UserThreepid {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            medium: "email".to_string(),
            address: "alice@example.com".to_string(),
            validated_at: Some(1234567890),
            added_ts: 1234567800,
            is_verified: true,
            verification_token: None,
            verification_expires_at: None,
        };

        assert_eq!(threepid.medium, "email");
        assert!(threepid.is_verified);
    }

    #[test]
    fn test_user_profile() {
        let profile = UserProfile {
            user_id: "@charlie:example.com".to_string(),
            username: "charlie".to_string(),
            displayname: Some("Charlie".to_string()),
            avatar_url: Some("mxc://example.com/charlie".to_string()),
            created_ts: 1234567890,
        };

        assert_eq!(profile.user_id, "@charlie:example.com");
        assert!(profile.displayname.is_some());
    }

    #[test]
    fn test_friend() {
        let friend = Friend {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            friend_id: "@bob:example.com".to_string(),
            created_ts: 1234567890,
        };

        assert_eq!(friend.user_id, "@alice:example.com");
        assert_eq!(friend.friend_id, "@bob:example.com");
    }

    #[test]
    fn test_presence() {
        let presence = Presence {
            user_id: "@alice:example.com".to_string(),
            status_msg: Some("Working".to_string()),
            presence: "online".to_string(),
            last_active_ts: 1234567890,
            status_from: None,
            created_ts: 1234567800,
            updated_ts: 1234567890,
        };

        assert_eq!(presence.presence, "online");
        assert!(presence.status_msg.is_some());
    }
}
