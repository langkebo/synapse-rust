//! Pure constructor/serialization tests for Megolm session storage.

use super::*;
use chrono::{Duration, Utc};

fn create_test_session() -> MegolmSession {
    MegolmSession {
        id: uuid::Uuid::new_v4(),
        session_id: format!("test_session_{}", uuid::Uuid::new_v4()),
        room_id: "!testroom:example.com".to_string(),
        sender_key: "test_sender_key_base64".to_string(),
        session_key: "test_session_key_base64".to_string(),
        algorithm: "m.megolm.v1.aes-sha2".to_string(),
        message_index: 0,
        created_ts: Utc::now(),
        last_used_ts: Utc::now(),
        expires_at: None,
    }
}

#[test]
fn test_megolm_session_storage_creation() {
    let session = MegolmSession {
        id: uuid::Uuid::new_v4(),
        session_id: format!("test_session_{}", uuid::Uuid::new_v4()),
        room_id: "!testroom:example.com".to_string(),
        sender_key: "test_sender_key_base64".to_string(),
        session_key: "test_session_key_base64".to_string(),
        algorithm: "m.megolm.v1.aes-sha2".to_string(),
        message_index: 0,
        created_ts: Utc::now(),
        last_used_ts: Utc::now(),
        expires_at: None,
    };

    assert!(!session.session_id.is_empty());
    assert!(!session.room_id.is_empty());
    assert!(!session.sender_key.is_empty());
    assert!(!session.session_key.is_empty());
    assert_eq!(session.algorithm, "m.megolm.v1.aes-sha2");
}

#[test]
fn test_megolm_session_field_validation() {
    let session = MegolmSession {
        id: uuid::Uuid::new_v4(),
        session_id: "test".to_string(),
        room_id: "!testroom:example.com".to_string(),
        sender_key: "test_sender_key_base64".to_string(),
        session_key: "test_session_key_base64".to_string(),
        algorithm: "m.megolm.v1.aes-sha2".to_string(),
        message_index: 0,
        created_ts: Utc::now(),
        last_used_ts: Utc::now(),
        expires_at: None,
    };

    assert!(session.room_id.starts_with('!'), "Room ID should start with !");
    assert!(session.algorithm.starts_with("m.megolm"), "Algorithm should be megolm");
    assert!(session.message_index >= 0, "Message index should be non-negative");
}

#[test]
fn test_megolm_session_with_expiry() {
    let expiry_time = Utc::now() + Duration::hours(24);
    let mut session = create_test_session();
    session.expires_at = Some(expiry_time);

    assert!(session.expires_at.is_some());
    let expires = session.expires_at.unwrap();
    assert!(expires > Utc::now(), "Expiry time should be in the future");
    assert!(expires > session.created_ts, "Expiry should be after creation");
}

#[test]
fn test_megolm_session_without_expiry() {
    let session = create_test_session();

    assert!(session.expires_at.is_none(), "Session should not have expiry by default");
}

#[test]
fn test_megolm_session_message_index_increment() {
    let mut session = create_test_session();

    assert_eq!(session.message_index, 0);

    session.message_index += 1;
    assert_eq!(session.message_index, 1);

    session.message_index = 100;
    assert_eq!(session.message_index, 100);
}

#[test]
fn test_megolm_session_last_used_update() {
    let mut session = create_test_session();
    let original_last_used = session.last_used_ts;

    // sleep-reason: 时间戳铺开（让 last_used_ts 变化可观测）
    std::thread::sleep(std::time::Duration::from_millis(10));
    session.last_used_ts = Utc::now();

    assert!(session.last_used_ts > original_last_used, "Last used should be updated");
}

#[test]
fn test_megolm_session_algorithm_validation() {
    let valid_algorithms = vec!["m.megolm.v1.aes-sha2"];

    for algo in valid_algorithms {
        let mut session = create_test_session();
        session.algorithm = algo.to_string();

        assert!(session.algorithm.starts_with("m.megolm"));
        assert!(session.algorithm.contains("aes-sha2"));
    }
}

#[test]
fn test_megolm_session_room_id_format() {
    let session = create_test_session();

    assert!(session.room_id.starts_with('!'), "Room ID must start with !");
    assert!(session.room_id.contains(':'), "Room ID must contain ':' separator");

    let parts: Vec<&str> = session.room_id[1..].split(':').collect();
    assert!(parts.len() >= 2, "Room ID should have localpart and server name");
}

#[test]
fn test_megolm_session_key_base64_format() {
    let session = create_test_session();

    assert!(!session.session_key.is_empty(), "Session key should not be empty");
    assert!(!session.sender_key.is_empty(), "Sender key should not be empty");

    assert!(session.session_key.len() > 10, "Session key should have reasonable length");
    assert!(session.sender_key.len() > 10, "Sender key should have reasonable length");
}

#[test]
fn test_megolm_session_boundary_conditions() {
    let mut session = create_test_session();

    session.message_index = i64::MAX;
    assert_eq!(session.message_index, i64::MAX);

    session.message_index = 0;
    assert_eq!(session.message_index, 0);
}

#[test]
fn test_megolm_session_time_ordering() {
    let created = Utc::now() - Duration::hours(1);
    let last_used = Utc::now();
    let expires = Utc::now() + Duration::hours(24);

    let session = MegolmSession {
        id: uuid::Uuid::new_v4(),
        session_id: "time_test".to_string(),
        room_id: "!room:example.com".to_string(),
        sender_key: "key".to_string(),
        session_key: "key".to_string(),
        algorithm: "m.megolm.v1.aes-sha2".to_string(),
        message_index: 0,
        created_ts: created,
        last_used_ts: last_used,
        expires_at: Some(expires),
    };

    assert!(session.created_ts <= session.last_used_ts);
    assert!(session.last_used_ts <= session.expires_at.unwrap());
}

#[test]
fn test_megolm_session_id_uniqueness() {
    let session1 = create_test_session();
    let session2 = create_test_session();

    assert_ne!(session1.id, session2.id, "Session IDs should be unique");
    assert_ne!(session1.session_id, session2.session_id, "Session identifiers should be unique");
}

#[test]
fn test_megolm_session_clone() {
    let session = create_test_session();
    let cloned = session.clone();

    assert_eq!(session.id, cloned.id);
    assert_eq!(session.session_id, cloned.session_id);
    assert_eq!(session.room_id, cloned.room_id);
    assert_eq!(session.algorithm, cloned.algorithm);
    assert_eq!(session.message_index, cloned.message_index);
}
