use crate::storage::matrixrtc::*;
use crate::services::matrixrtc_service::*;
use crate::cache::{CacheManager, CacheConfig};

fn create_test_storage(pool: sqlx::PgPool) -> MatrixRTCStorage {
    MatrixRTCStorage::new(std::sync::Arc::new(pool))
}

fn create_test_service(storage: MatrixRTCStorage, cache: CacheManager) -> MatrixRTCService {
    MatrixRTCService::new(storage, std::sync::Arc::new(cache))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    #[test]
    fn test_rtc_session_struct() {
        let session = RTCSession {
            id: 1,
            room_id: "!room:example.com".to_string(),
            session_id: "session123".to_string(),
            application: "m.call".to_string(),
            call_id: Some("call456".to_string()),
            creator: "@alice:example.com".to_string(),
            created_ts: 1234567890000,
            updated_ts: 1234567890000,
            is_active: true,
            config: serde_json::json!({"capabilities": ["audio", "video"]}),
        };

        assert_eq!(session.room_id, "!room:example.com");
        assert_eq!(session.session_id, "session123");
        assert_eq!(session.application, "m.call");
        assert!(session.is_active);
    }

    #[test]
    fn test_rtc_membership_struct() {
        let membership = RTCMembership {
            id: 1,
            room_id: "!room:example.com".to_string(),
            session_id: "session123".to_string(),
            user_id: "@bob:example.com".to_string(),
            device_id: "DEVICE456".to_string(),
            membership_id: "membership789".to_string(),
            application: "m.call".to_string(),
            call_id: Some("call456".to_string()),
            created_ts: 1234567890000,
            updated_ts: 1234567890000,
            expires_ts: Some(1234571490000),
            foci_active: Some("livekit".to_string()),
            foci_preferred: Some(serde_json::json!(["livekit", "native-webrtc"])),
            application_data: Some(serde_json::json!({"muted": false})),
            is_active: true,
        };

        assert_eq!(membership.user_id, "@bob:example.com");
        assert_eq!(membership.device_id, "DEVICE456");
        assert!(membership.is_active);
    }

    #[test]
    fn test_rtc_encryption_key_struct() {
        let key = RTCEncryptionKey {
            id: 1,
            room_id: "!room:example.com".to_string(),
            session_id: "session123".to_string(),
            key_index: 1,
            key: "base64_encoded_key".to_string(),
            created_ts: 1234567890000,
            expires_ts: Some(1234654290000),
            sender_user_id: "@alice:example.com".to_string(),
            sender_device_id: "DEVICE123".to_string(),
        };

        assert_eq!(key.key_index, 1);
        assert_eq!(key.sender_user_id, "@alice:example.com");
    }

    #[test]
    fn test_create_session_params() {
        let params = CreateSessionParams {
            room_id: "!room:example.com".to_string(),
            session_id: "new_session".to_string(),
            application: "m.call".to_string(),
            call_id: None,
            creator: "@alice:example.com".to_string(),
            config: serde_json::json!({"audio": true, "video": true}),
        };

        assert_eq!(params.room_id, "!room:example.com");
        assert!(params.call_id.is_none());
    }

    #[test]
    fn test_create_membership_params() {
        let params = CreateMembershipParams {
            room_id: "!room:example.com".to_string(),
            session_id: "session123".to_string(),
            user_id: "@charlie:example.com".to_string(),
            device_id: "DEVICE789".to_string(),
            membership_id: "membership123".to_string(),
            application: "m.call".to_string(),
            call_id: Some("call456".to_string()),
            foci_active: Some("native-webrtc".to_string()),
            foci_preferred: None,
            application_data: None,
        };

        assert_eq!(params.user_id, "@charlie:example.com");
        assert!(params.foci_active.is_some());
    }

    #[test]
    fn test_session_with_memberships() {
        let session = RTCSession {
            id: 1,
            room_id: "!room:example.com".to_string(),
            session_id: "session123".to_string(),
            application: "m.call".to_string(),
            call_id: None,
            creator: "@alice:example.com".to_string(),
            created_ts: 1234567890000,
            updated_ts: 1234567890000,
            is_active: true,
            config: serde_json::json!({}),
        };

        let memberships = vec![
            RTCMembership {
                id: 1,
                room_id: "!room:example.com".to_string(),
                session_id: "session123".to_string(),
                user_id: "@bob:example.com".to_string(),
                device_id: "DEVICE1".to_string(),
                membership_id: "m1".to_string(),
                application: "m.call".to_string(),
                call_id: None,
                created_ts: 1234567890000,
                updated_ts: 1234567890000,
                expires_ts: None,
                foci_active: None,
                foci_preferred: None,
                application_data: None,
                is_active: true,
            },
        ];

        let combined = SessionWithMemberships {
            session,
            memberships,
        };

        assert_eq!(combined.session.session_id, "session123");
        assert_eq!(combined.memberships.len(), 1);
    }

    #[test]
    fn test_application_types() {
        let valid_apps = vec!["m.call", "m.custom"];

        for app in valid_apps {
            assert!(!app.is_empty());
        }
    }

    #[test]
    fn test_focus_types() {
        let valid_foci = vec!["livekit", "native-webrtc", "jitsi"];

        for focus in valid_foci {
            assert!(!focus.is_empty());
        }
    }

    #[test]
    fn test_to_matrix_event() {
        let session = RTCSession {
            id: 1,
            room_id: "!room:example.com".to_string(),
            session_id: "session123".to_string(),
            application: "m.call".to_string(),
            call_id: Some("call456".to_string()),
            creator: "@alice:example.com".to_string(),
            created_ts: 1234567890000,
            updated_ts: 1234567890000,
            is_active: true,
            config: serde_json::json!({"audio": true}),
        };

        let memberships = vec![];

        let event = to_matrix_event(&session, &memberships);

        assert_eq!(event["type"], "org.matrix.msc3401.call");
        assert_eq!(event["room_id"], "!room:example.com");
        assert!(event["content"]["session_id"].is_string());
    }
}
