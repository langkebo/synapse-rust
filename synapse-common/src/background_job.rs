//! Background job runner (fire-and-forget tasks with panic capture).

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
/// Represents BackgroundJob; see per-variant docs.
pub enum BackgroundJob {
    /// `SendEmail` variant.
    SendEmail {
        /// Recipient email address.
        to: String,
        /// Email subject line.
        subject: String,
        /// Email body content.
        body: String,
    },
    /// `ProcessMedia` variant.
    ProcessMedia {
        /// Local media repository file id.
        file_id: String,
    },
    /// `FederationTransaction` variant.
    FederationTransaction {
        /// Federation transaction id.
        txn_id: String,
        /// Destination homeserver.
        destination: String,
    },
    /// `Generic` variant.
    Generic {
        /// Job name.
        name: String,
        /// Arbitrary JSON payload.
        payload: serde_json::Value,
    },
    /// `RedactEvent` variant.
    RedactEvent {
        /// Room id where the event lives.
        room_id: String,
        /// Event id to redact.
        event_id: String,
        /// Optional reason for the redaction.
        reason: Option<String>,
    },
    /// `DelayedEventProcessing` variant.
    DelayedEventProcessing {
        /// Event id whose processing is delayed.
        event_id: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_send_email_job() {
        let job = BackgroundJob::SendEmail {
            to: "user@example.com".to_string(),
            subject: "Test".to_string(),
            body: "Hello".to_string(),
        };
        let json = serde_json::to_string(&job).unwrap();
        assert!(json.contains("user@example.com"));
        assert!(json.contains("Test"));
    }

    #[test]
    fn test_process_media_job() {
        let job = BackgroundJob::ProcessMedia { file_id: "abc123".to_string() };
        let json = serde_json::to_string(&job).unwrap();
        assert!(json.contains("abc123"));
    }

    #[test]
    fn test_federation_transaction_job() {
        let job = BackgroundJob::FederationTransaction {
            txn_id: "txn123".to_string(),
            destination: "matrix.org".to_string(),
        };
        let json = serde_json::to_string(&job).unwrap();
        assert!(json.contains("txn123"));
        assert!(json.contains("matrix.org"));
    }

    #[test]
    fn test_generic_job() {
        let job =
            BackgroundJob::Generic { name: "test_task".to_string(), payload: serde_json::json!({"key": "value"}) };
        let json = serde_json::to_string(&job).unwrap();
        assert!(json.contains("test_task"));
    }

    #[test]
    fn test_redact_event_job() {
        let job = BackgroundJob::RedactEvent {
            room_id: "!room:example.com".to_string(),
            event_id: "$event".to_string(),
            reason: Some("spam".to_string()),
        };
        let json = serde_json::to_string(&job).unwrap();
        assert!(json.contains("!room:example.com"));
        assert!(json.contains("spam"));
    }

    #[test]
    fn test_redact_event_job_no_reason() {
        let job = BackgroundJob::RedactEvent {
            room_id: "!room:example.com".to_string(),
            event_id: "$event".to_string(),
            reason: None,
        };
        let json = serde_json::to_string(&job).unwrap();
        assert!(json.contains("!room:example.com"));
    }

    #[test]
    fn test_job_deserialization() {
        let json = r#"{"SendEmail":{"to":"user@test.com","subject":"Hi","body":"Hello"}}"#;
        let job: BackgroundJob = serde_json::from_str(json).unwrap();
        match job {
            BackgroundJob::SendEmail { to, subject, body } => {
                assert_eq!(to, "user@test.com");
                assert_eq!(subject, "Hi");
                assert_eq!(body, "Hello");
            }
            _ => panic!("Expected SendEmail variant"),
        }
    }
}
