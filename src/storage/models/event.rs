use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct Event {
    pub event_id: String,
    pub room_id: String,
    pub sender: String,
    pub event_type: String,
    pub content: serde_json::Value,
    pub origin_server_ts: i64,
    pub state_key: Option<String>,
    pub is_redacted: bool,
    pub redacted_at: Option<i64>,
    pub redacted_by: Option<String>,
    pub transaction_id: Option<String>,
    pub depth: Option<i64>,
    pub prev_events: Option<serde_json::Value>,
    pub auth_events: Option<serde_json::Value>,
    pub signatures: Option<serde_json::Value>,
    pub hashes: Option<serde_json::Value>,
    pub unsigned: Option<serde_json::Value>,
    pub processed_at: Option<i64>,
    pub not_before: i64,
    pub status: Option<String>,
    pub reference_image: Option<String>,
    pub origin: Option<String>,
    pub user_id: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct EventReceipt {
    pub id: i64,
    pub event_id: String,
    pub room_id: String,
    pub user_id: String,
    pub receipt_type: String,
    pub ts: i64,
    pub data: Option<serde_json::Value>,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct EventReport {
    pub id: i64,
    pub event_id: String,
    pub room_id: String,
    pub reporter_user_id: String,
    pub reported_user_id: Option<String>,
    pub event_json: Option<serde_json::Value>,
    pub reason: Option<String>,
    pub description: Option<String>,
    pub status: String,
    pub score: i32,
    pub received_ts: i64,
    pub resolved_at: Option<i64>,
    pub resolved_by: Option<String>,
    pub resolution_reason: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct EventReportHistory {
    pub id: i64,
    pub report_id: i64,
    pub action: String,
    pub actor_user_id: Option<String>,
    pub actor_role: Option<String>,
    pub old_status: Option<String>,
    pub new_status: Option<String>,
    pub reason: Option<String>,
    pub created_ts: i64,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct DelayedEvent {
    pub id: i64,
    pub room_id: String,
    pub event_id: String,
    pub event_type: String,
    pub sender: String,
    pub content: serde_json::Value,
    pub delay_ms: i64,
    pub created_ts: i64,
    pub scheduled_at: i64,
    pub is_sent: bool,
    pub sent_at: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct VoiceMessage {
    pub id: i64,
    pub event_id: String,
    pub user_id: String,
    pub room_id: Option<String>,
    pub media_id: Option<String>,
    pub duration_ms: i32,
    pub waveform: Option<String>,
    pub mime_type: Option<String>,
    pub file_size: Option<i64>,
    pub transcription: Option<String>,
    pub encryption: Option<serde_json::Value>,
    pub is_processed: bool,
    pub processed_at: Option<i64>,
    pub created_ts: i64,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct VoiceUsageStats {
    pub id: i64,
    pub user_id: String,
    pub room_id: Option<String>,
    pub date: chrono::NaiveDate,
    pub period_start: Option<chrono::DateTime<chrono::Utc>>,
    pub period_end: Option<chrono::DateTime<chrono::Utc>>,
    pub total_duration_ms: i64,
    pub total_file_size: i64,
    pub message_count: i64,
    pub last_active_ts: Option<i64>,
    // 注意: last_activity_at 已移除（与 last_active_ts 冗余）
    pub created_ts: Option<i64>,
    pub updated_ts: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_struct() {
        let event = Event {
            event_id: "$event:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            sender: "@alice:example.com".to_string(),
            event_type: "m.room.message".to_string(),
            content: serde_json::json!({"msgtype": "m.text", "body": "Hello"}),
            origin_server_ts: 1234567890000,
            state_key: None,
            is_redacted: false,
            redacted_at: None,
            redacted_by: None,
            transaction_id: None,
            depth: Some(1),
            prev_events: Some(serde_json::json!([])),
            auth_events: Some(serde_json::json!([])),
            signatures: None,
            hashes: None,
            unsigned: Some(serde_json::json!({})),
            processed_at: Some(1234567890100),
            not_before: 0,
            status: Some("processed".to_string()),
            reference_image: None,
            origin: Some("example.com".to_string()),
            user_id: Some("@alice:example.com".to_string()),
        };

        assert_eq!(event.event_id, "$event:example.com");
        assert_eq!(event.event_type, "m.room.message");
        assert!(!event.is_redacted);
    }

    #[test]
    fn test_event_receipt() {
        let receipt = EventReceipt {
            id: 1,
            event_id: "$event:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            user_id: "@alice:example.com".to_string(),
            receipt_type: "m.read".to_string(),
            ts: 1234567890000,
            data: Some(serde_json::json!({})),
            created_ts: 1234567890000,
            updated_ts: 1234567890000,
        };

        assert_eq!(receipt.receipt_type, "m.read");
    }

    #[test]
    fn test_event_report() {
        let report = EventReport {
            id: 1,
            event_id: "$event:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            reporter_user_id: "@bob:example.com".to_string(),
            reported_user_id: Some("@alice:example.com".to_string()),
            event_json: None,
            reason: Some("Spam".to_string()),
            description: Some("This is spam content".to_string()),
            status: "open".to_string(),
            score: 0,
            received_ts: 1234567890000,
            resolved_at: None,
            resolved_by: None,
            resolution_reason: None,
        };

        assert_eq!(report.status, "open");
        assert!(report.reason.is_some());
    }

    #[test]
    fn test_voice_message() {
        let voice = VoiceMessage {
            id: 1,
            event_id: "$voice:example.com".to_string(),
            user_id: "@alice:example.com".to_string(),
            room_id: Some("!room:example.com".to_string()),
            media_id: Some("media_123".to_string()),
            duration_ms: 5000,
            waveform: Some("waveform_data".to_string()),
            mime_type: Some("audio/ogg".to_string()),
            file_size: Some(102400),
            transcription: None,
            encryption: None,
            is_processed: false,
            processed_at: None,
            created_ts: 1234567890000,
        };

        assert_eq!(voice.duration_ms, 5000);
        assert!(!voice.is_processed);
    }
}
