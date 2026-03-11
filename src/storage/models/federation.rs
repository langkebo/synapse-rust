use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct FederationServer {
    pub id: i64,
    pub server_name: String,
    pub is_blocked: bool,
    pub blocked_at: Option<i64>,
    pub blocked_reason: Option<String>,
    pub last_successful_connect_at: Option<i64>,
    pub last_failed_connect_at: Option<i64>,
    pub failure_count: i32,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct FederationBlacklist {
    pub id: i64,
    pub server_name: String,
    pub reason: Option<String>,
    pub added_ts: i64,
    pub added_by: Option<String>,
    pub updated_ts: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct FederationQueue {
    pub id: i64,
    pub destination: String,
    pub event_id: String,
    pub event_type: String,
    pub room_id: Option<String>,
    pub content: serde_json::Value,
    pub created_ts: i64,
    pub sent_at: Option<i64>,
    pub retry_count: i32,
    pub status: String,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct ApplicationService {
    pub id: i64,
    pub as_id: String,
    pub url: String,
    pub as_token: String,
    pub hs_token: String,
    pub sender_localpart: String,
    pub is_enabled: bool,
    pub rate_limited: bool,
    pub protocols: Option<Vec<String>>,
    pub namespaces: Option<serde_json::Value>,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
    pub description: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_federation_server() {
        let server = FederationServer {
            id: 1,
            server_name: "matrix.org".to_string(),
            is_blocked: false,
            blocked_at: None,
            blocked_reason: None,
            last_successful_connect_at: Some(1234567890000),
            last_failed_connect_at: None,
            failure_count: 0,
        };

        assert_eq!(server.server_name, "matrix.org");
        assert!(!server.is_blocked);
    }

    #[test]
    fn test_federation_blacklist() {
        let blacklist = FederationBlacklist {
            id: 1,
            server_name: "bad.server.com".to_string(),
            reason: Some("Spam source".to_string()),
            added_ts: 1234567890000,
            added_by: Some("@admin:example.com".to_string()),
            updated_ts: None,
        };

        assert_eq!(blacklist.server_name, "bad.server.com");
        assert!(blacklist.reason.is_some());
    }

    #[test]
    fn test_federation_queue() {
        let queue = FederationQueue {
            id: 1,
            destination: "matrix.org".to_string(),
            event_id: "$event:example.com".to_string(),
            event_type: "m.room.message".to_string(),
            room_id: Some("!room:example.com".to_string()),
            content: serde_json::json!({"msgtype": "m.text", "body": "Hello"}),
            created_ts: 1234567890000,
            sent_at: None,
            retry_count: 0,
            status: "pending".to_string(),
        };

        assert_eq!(queue.destination, "matrix.org");
        assert_eq!(queue.status, "pending");
    }

    #[test]
    fn test_application_service() {
        let appservice = ApplicationService {
            id: 1,
            as_id: "irc-bridge".to_string(),
            url: "https://irc-bridge.example.com".to_string(),
            as_token: "as_token_abc123".to_string(),
            hs_token: "hs_token_xyz789".to_string(),
            sender_localpart: "irc-bot".to_string(),
            is_enabled: true,
            rate_limited: true,
            protocols: Some(vec!["irc".to_string()]),
            namespaces: Some(serde_json::json!({"users": [{"exclusive": true, "regex": "@irc_.*"}]})),
            created_ts: 1234567890000,
            updated_ts: None,
            description: Some("IRC Bridge".to_string()),
        };

        assert_eq!(appservice.as_id, "irc-bridge");
        assert!(appservice.is_enabled);
    }
}
