//! Integration tests for `KeyRotationStorage` (synapse-e2ee/src/key_rotation/service.rs).
//! Covers every public storage method plus `KeyRotationConfig` load/persist.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use synapse_rust::e2ee::key_rotation::service::{KeyRotationConfig, KeyRotationStorage};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn key_rot_guard() -> &'static Mutex<()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD.get_or_init(|| Mutex::new(()))
}

async fn warm_up_pool(pool: &Arc<sqlx::PgPool>) {
    for _ in 0..8 {
        match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            sqlx::query("SELECT 1").execute(pool.as_ref()),
        )
        .await
        {
            Ok(Ok(_)) => return,
            Ok(Err(_)) | Err(_) => {
                tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            }
        }
    }
    let _ = sqlx::query("SELECT 1").execute(pool.as_ref()).await;
}

/// Clear key_rotation-exclusive tables (not shared with device_trust).
async fn setup(pool: &Arc<sqlx::PgPool>) {
    warm_up_pool(pool).await;
    sqlx::query("DELETE FROM key_rotation_state").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM key_rotation_pending").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM megolm_key_shares").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM key_rotation_config").execute(pool.as_ref()).await.ok();
}

fn unique_user_id() -> String {
    format!("@keyrot_{}:localhost", unique_id())
}

fn unique_room_id() -> String {
    format!("!keyrot_room_{}:localhost", unique_id())
}

fn unique_session_id() -> String {
    format!("session_keyrot_{}", unique_id())
}

async fn cleanup_user(pool: &sqlx::PgPool, user_id: &str) {
    sqlx::query("DELETE FROM key_rotation_log WHERE user_id = $1").bind(user_id).execute(pool).await.ok();
    sqlx::query("DELETE FROM key_rotation_state WHERE user_id = $1").bind(user_id).execute(pool).await.ok();
}

async fn cleanup_room(pool: &sqlx::PgPool, room_id: &str) {
    sqlx::query("DELETE FROM key_rotation_pending WHERE room_id = $1").bind(room_id).execute(pool).await.ok();
    sqlx::query("DELETE FROM megolm_key_shares WHERE room_id = $1").bind(room_id).execute(pool).await.ok();
    sqlx::query("DELETE FROM megolm_sessions WHERE room_id = $1").bind(room_id).execute(pool).await.ok();
    sqlx::query("DELETE FROM room_memberships WHERE room_id = $1").bind(room_id).execute(pool).await.ok();
    sqlx::query("DELETE FROM events WHERE room_id = $1").bind(room_id).execute(pool).await.ok();
    sqlx::query("DELETE FROM rooms WHERE room_id = $1").bind(room_id).execute(pool).await.ok();
}

async fn insert_encrypted_room(pool: &sqlx::PgPool, room_id: &str, user_id: &str) {
    let now_ts = chrono::Utc::now().timestamp_millis();
    sqlx::query("INSERT INTO rooms (room_id, created_ts) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(room_id)
        .bind(now_ts)
        .execute(pool)
        .await
        .ok();
    sqlx::query(
        "INSERT INTO room_memberships (room_id, user_id, membership, joined_ts) VALUES ($1, $2, 'join', $3) ON CONFLICT DO NOTHING",
    )
    .bind(room_id)
    .bind(user_id)
    .bind(now_ts)
    .execute(pool)
    .await
    .ok();
    sqlx::query(
        "INSERT INTO events (event_id, room_id, sender, event_type, content, origin_server_ts, state_key) VALUES ($1, $2, $3, 'm.room.encryption', '{}'::jsonb, $4, '') ON CONFLICT DO NOTHING",
    )
    .bind(format!("evt_{room_id}"))
    .bind(room_id)
    .bind(user_id)
    .bind(now_ts)
    .execute(pool)
    .await
    .ok();
}

async fn insert_megolm_session(pool: &sqlx::PgPool, session_id: &str, room_id: &str, expires_at: Option<i64>) {
    let now_ts = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        "INSERT INTO megolm_sessions (session_id, room_id, sender_key, session_key, algorithm, created_ts, expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT (session_id) DO NOTHING",
    )
    .bind(session_id)
    .bind(room_id)
    .bind("sender_key_value")
    .bind("session_key_value")
    .bind("m.megolm.v1.aes-sha2")
    .bind(now_ts)
    .bind(expires_at)
    .execute(pool)
    .await
    .ok();
}

// ===========================================================================
// log_rotation
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_log_rotation_inserts_row() {
    let _guard = key_rot_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = KeyRotationStorage::new(pool.clone());

    let user_id = unique_user_id();
    let room_id = unique_room_id();
    storage.log_rotation(&user_id, &room_id, "megolm").await.unwrap();

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM key_rotation_log WHERE user_id = $1 AND room_id = $2")
            .bind(&user_id)
            .bind(&room_id)
            .fetch_one(pool.as_ref())
            .await
            .unwrap();
    assert_eq!(count, 1);

    cleanup_user(pool.as_ref(), &user_id).await;
    cleanup_room(pool.as_ref(), &room_id).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_log_rotation_records_multiple_entries() {
    let _guard = key_rot_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = KeyRotationStorage::new(pool.clone());

    let user_id = unique_user_id();
    let room_id = unique_room_id();
    storage.log_rotation(&user_id, &room_id, "megolm").await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    storage.log_rotation(&user_id, &room_id, "olm").await.unwrap();

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM key_rotation_log WHERE user_id = $1").bind(&user_id).fetch_one(pool.as_ref()).await.unwrap();
    assert_eq!(count, 2);

    cleanup_user(pool.as_ref(), &user_id).await;
    cleanup_room(pool.as_ref(), &room_id).await;
}

// ===========================================================================
// record_key_share
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_record_key_share_insert() {
    let _guard = key_rot_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = KeyRotationStorage::new(pool.clone());

    let room_id = unique_room_id();
    let session_id = unique_session_id();
    storage.record_key_share(&room_id, &session_id, "rotated").await.unwrap();

    let reason: String = sqlx::query_scalar(
        "SELECT share_reason FROM megolm_key_shares WHERE room_id = $1 AND session_id = $2",
    )
    .bind(&room_id)
    .bind(&session_id)
    .fetch_one(pool.as_ref())
    .await
    .unwrap();
    assert_eq!(reason, "rotated");

    cleanup_room(pool.as_ref(), &room_id).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_record_key_share_upsert_updates_reason() {
    let _guard = key_rot_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = KeyRotationStorage::new(pool.clone());

    let room_id = unique_room_id();
    let session_id = unique_session_id();
    storage.record_key_share(&room_id, &session_id, "rotated").await.unwrap();
    storage.record_key_share(&room_id, &session_id, "new_member").await.unwrap();

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM megolm_key_shares WHERE room_id = $1 AND session_id = $2",
    )
    .bind(&room_id)
    .bind(&session_id)
    .fetch_one(pool.as_ref())
    .await
    .unwrap();
    assert_eq!(count, 1, "upsert should not duplicate");

    let reason: String = sqlx::query_scalar(
        "SELECT share_reason FROM megolm_key_shares WHERE room_id = $1 AND session_id = $2",
    )
    .bind(&room_id)
    .bind(&session_id)
    .fetch_one(pool.as_ref())
    .await
    .unwrap();
    assert_eq!(reason, "new_member");

    cleanup_room(pool.as_ref(), &room_id).await;
}

// ===========================================================================
// mark_rotated / check_needs_rotation
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_mark_rotated_inserts_state() {
    let _guard = key_rot_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = KeyRotationStorage::new(pool.clone());

    let user_id = unique_user_id();
    let room_id = unique_room_id();
    storage.mark_rotated(&user_id, &room_id).await.unwrap();

    let is_rotated: bool = sqlx::query_scalar(
        "SELECT is_rotated FROM key_rotation_state WHERE user_id = $1 AND room_id = $2",
    )
    .bind(&user_id)
    .bind(&room_id)
    .fetch_one(pool.as_ref())
    .await
    .unwrap();
    assert!(is_rotated);

    cleanup_user(pool.as_ref(), &user_id).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_mark_rotated_upsert_updates_timestamp() {
    let _guard = key_rot_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = KeyRotationStorage::new(pool.clone());

    let user_id = unique_user_id();
    let room_id = unique_room_id();
    storage.mark_rotated(&user_id, &room_id).await.unwrap();
    let first_ts: Option<i64> = sqlx::query_scalar(
        "SELECT rotated_at FROM key_rotation_state WHERE user_id = $1 AND room_id = $2",
    )
    .bind(&user_id)
    .bind(&room_id)
    .fetch_one(pool.as_ref())
    .await
    .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    storage.mark_rotated(&user_id, &room_id).await.unwrap();
    let second_ts: Option<i64> = sqlx::query_scalar(
        "SELECT rotated_at FROM key_rotation_state WHERE user_id = $1 AND room_id = $2",
    )
    .bind(&user_id)
    .bind(&room_id)
    .fetch_one(pool.as_ref())
    .await
    .unwrap();

    assert!(second_ts >= first_ts, "rotated_at should advance on upsert");

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM key_rotation_state WHERE user_id = $1 AND room_id = $2")
            .bind(&user_id)
            .bind(&room_id)
            .fetch_one(pool.as_ref())
            .await
            .unwrap();
    assert_eq!(count, 1, "upsert should not duplicate");

    cleanup_user(pool.as_ref(), &user_id).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_check_needs_rotation_no_row_returns_true() {
    let _guard = key_rot_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = KeyRotationStorage::new(pool.clone());

    let user_id = unique_user_id();
    let room_id = unique_room_id();
    let needs = storage.check_needs_rotation(&user_id, &room_id).await.unwrap();
    assert!(needs, "no state row should mean rotation needed");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_check_needs_rotation_after_mark_returns_false() {
    let _guard = key_rot_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = KeyRotationStorage::new(pool.clone());

    let user_id = unique_user_id();
    let room_id = unique_room_id();
    storage.mark_rotated(&user_id, &room_id).await.unwrap();

    let needs = storage.check_needs_rotation(&user_id, &room_id).await.unwrap();
    assert!(!needs, "after mark_rotated, rotation should not be needed");

    cleanup_user(pool.as_ref(), &user_id).await;
}

// ===========================================================================
// delete_expired_sessions
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_delete_expired_sessions_deletes_expired_only() {
    let _guard = key_rot_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = KeyRotationStorage::new(pool.clone());

    let room_id = unique_room_id();
    let expired_sid = format!("{}_expired", unique_session_id());
    let valid_sid = unique_session_id();

    insert_megolm_session(pool.as_ref(), &expired_sid, &room_id, Some(1)).await;
    let future = chrono::Utc::now().timestamp_millis() + 3_600_000;
    insert_megolm_session(pool.as_ref(), &valid_sid, &room_id, Some(future)).await;

    let deleted = storage.delete_expired_sessions().await.unwrap();
    assert!(deleted >= 1, "at least the expired session should be deleted");

    let remaining: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM megolm_sessions WHERE session_id = $1").bind(&valid_sid).fetch_one(pool.as_ref()).await.unwrap();
    assert_eq!(remaining, 1, "valid session should survive");

    let expired_remaining: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM megolm_sessions WHERE session_id = $1").bind(&expired_sid).fetch_one(pool.as_ref()).await.unwrap();
    assert_eq!(expired_remaining, 0, "expired session should be gone");

    cleanup_room(pool.as_ref(), &room_id).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_delete_expired_sessions_none_expired() {
    let _guard = key_rot_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = KeyRotationStorage::new(pool.clone());

    let room_id = unique_room_id();
    let future = chrono::Utc::now().timestamp_millis() + 3_600_000;
    let sid = unique_session_id();
    insert_megolm_session(pool.as_ref(), &sid, &room_id, Some(future)).await;

    let deleted = storage.delete_expired_sessions().await.unwrap();
    assert_eq!(deleted, 0, "no expired sessions should be deleted");

    cleanup_room(pool.as_ref(), &room_id).await;
}

// ===========================================================================
// get_rotation_status
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_rotation_status_no_data() {
    let _guard = key_rot_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = KeyRotationStorage::new(pool.clone());

    let user_id = unique_user_id();
    let status = storage.get_rotation_status(&user_id).await.unwrap();
    assert_eq!(status.total_sessions, 0);
    assert_eq!(status.rotated_sessions, 0);
    assert!(status.last_rotation.is_none());
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_rotation_status_with_rotations() {
    let _guard = key_rot_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = KeyRotationStorage::new(pool.clone());

    let user_id = unique_user_id();
    let room1 = unique_room_id();
    let room2 = unique_room_id();
    storage.mark_rotated(&user_id, &room1).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    storage.mark_rotated(&user_id, &room2).await.unwrap();

    let status = storage.get_rotation_status(&user_id).await.unwrap();
    assert_eq!(status.total_sessions, 2);
    assert_eq!(status.rotated_sessions, 2, "both rotated within 7 days");
    assert!(status.last_rotation.is_some(), "last_rotation should be populated");

    cleanup_user(pool.as_ref(), &user_id).await;
}

// ===========================================================================
// mark_key_rotation_needed / clear_key_rotation_needed
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_mark_key_rotation_needed_insert() {
    let _guard = key_rot_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = KeyRotationStorage::new(pool.clone());

    let room_id = unique_room_id();
    let user_id = unique_user_id();
    storage.mark_key_rotation_needed(&room_id, &user_id).await.unwrap();

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM key_rotation_pending WHERE room_id = $1 AND triggered_by_user_id = $2",
    )
    .bind(&room_id)
    .bind(&user_id)
    .fetch_one(pool.as_ref())
    .await
    .unwrap();
    assert_eq!(count, 1);

    cleanup_room(pool.as_ref(), &room_id).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_mark_key_rotation_needed_upsert() {
    let _guard = key_rot_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = KeyRotationStorage::new(pool.clone());

    let room_id = unique_room_id();
    let user_id = unique_user_id();
    storage.mark_key_rotation_needed(&room_id, &user_id).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    storage.mark_key_rotation_needed(&room_id, &user_id).await.unwrap();

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM key_rotation_pending WHERE room_id = $1 AND triggered_by_user_id = $2",
    )
    .bind(&room_id)
    .bind(&user_id)
    .fetch_one(pool.as_ref())
    .await
    .unwrap();
    assert_eq!(count, 1, "upsert should not duplicate");

    cleanup_room(pool.as_ref(), &room_id).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_clear_key_rotation_needed() {
    let _guard = key_rot_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = KeyRotationStorage::new(pool.clone());

    let room_id = unique_room_id();
    let user_id = unique_user_id();
    storage.mark_key_rotation_needed(&room_id, &user_id).await.unwrap();

    storage.clear_key_rotation_needed(&room_id).await.unwrap();

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM key_rotation_pending WHERE room_id = $1").bind(&room_id).fetch_one(pool.as_ref()).await.unwrap();
    assert_eq!(count, 0);

    // Clearing again is a no-op.
    let result = storage.clear_key_rotation_needed(&room_id).await;
    assert!(result.is_ok());
}

// ===========================================================================
// get_rooms_needing_key_rotation
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_rooms_needing_key_rotation() {
    let _guard = key_rot_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = KeyRotationStorage::new(pool.clone());

    let user_id = unique_user_id();
    let room_id = unique_room_id();
    insert_encrypted_room(pool.as_ref(), &room_id, &user_id).await;
    storage.mark_key_rotation_needed(&room_id, &user_id).await.unwrap();

    let rooms = storage.get_rooms_needing_key_rotation(&user_id).await.unwrap();
    assert!(rooms.contains(&room_id), "room with pending rotation should be returned");

    cleanup_room(pool.as_ref(), &room_id).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_rooms_needing_key_rotation_empty() {
    let _guard = key_rot_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = KeyRotationStorage::new(pool.clone());

    let user_id = unique_user_id();
    let rooms = storage.get_rooms_needing_key_rotation(&user_id).await.unwrap();
    assert!(rooms.is_empty());
}

// ===========================================================================
// get_user_last_rotation_ts
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_user_last_rotation_ts_none() {
    let _guard = key_rot_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = KeyRotationStorage::new(pool.clone());

    let user_id = unique_user_id();
    let result = storage.get_user_last_rotation_ts(&user_id).await.unwrap();
    assert!(result.is_none());
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_user_last_rotation_ts_returns_max() {
    let _guard = key_rot_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = KeyRotationStorage::new(pool.clone());

    let user_id = unique_user_id();
    let room1 = unique_room_id();
    let room2 = unique_room_id();
    storage.log_rotation(&user_id, &room1, "megolm").await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    storage.log_rotation(&user_id, &room2, "olm").await.unwrap();

    let result = storage.get_user_last_rotation_ts(&user_id).await.unwrap();
    assert!(result.is_some());
    let now_ms = chrono::Utc::now().timestamp_millis();
    assert!(result.unwrap() <= now_ms, "timestamp should not be in the future");

    cleanup_user(pool.as_ref(), &user_id).await;
    cleanup_room(pool.as_ref(), &room1).await;
    cleanup_room(pool.as_ref(), &room2).await;
}

// ===========================================================================
// get_device_rotation_history
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_device_rotation_history_orders_desc_and_limits() {
    let _guard = key_rot_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = KeyRotationStorage::new(pool.clone());

    let user_id = unique_user_id();
    let device_id = format!("DEV_{}", unique_id());
    let now = chrono::Utc::now().timestamp_millis();

    // Insert 12 rows directly (log_rotation doesn't take device_id).
    for i in 0..12 {
        sqlx::query(
            "INSERT INTO key_rotation_log (user_id, device_id, room_id, rotation_type, old_key_id, new_key_id, reason, rotated_at) \
             VALUES ($1, $2, NULL, 'megolm', NULL, $3, NULL, $4)",
        )
        .bind(&user_id)
        .bind(&device_id)
        .bind(format!("key_{i}"))
        .bind(now + i)
        .execute(pool.as_ref())
        .await
        .unwrap();
    }

    let history = storage.get_device_rotation_history(&user_id, &device_id).await.unwrap();
    assert_eq!(history.len(), 10, "should be limited to 10 entries");
    // Descending by rotated_at.
    assert!(history[0].1 >= history[1].1, "should be ordered descending");

    cleanup_user(pool.as_ref(), &user_id).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_device_rotation_history_empty() {
    let _guard = key_rot_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = KeyRotationStorage::new(pool.clone());

    let user_id = unique_user_id();
    let device_id = format!("DEV_{}", unique_id());
    let history = storage.get_device_rotation_history(&user_id, &device_id).await.unwrap();
    assert!(history.is_empty());
}

// ===========================================================================
// get_last_rotation_for_key
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_last_rotation_for_key_by_new_key_id() {
    let _guard = key_rot_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = KeyRotationStorage::new(pool.clone());

    let user_id = unique_user_id();
    let device_id = format!("DEV_{}", unique_id());
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        "INSERT INTO key_rotation_log (user_id, device_id, room_id, rotation_type, old_key_id, new_key_id, reason, rotated_at) \
         VALUES ($1, $2, NULL, 'megolm', 'old_k', 'new_k', NULL, $3)",
    )
    .bind(&user_id)
    .bind(&device_id)
    .bind(now)
    .execute(pool.as_ref())
    .await
    .unwrap();

    let result = storage.get_last_rotation_for_key(&user_id, "new_k").await.unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap(), now);

    cleanup_user(pool.as_ref(), &user_id).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_last_rotation_for_key_by_old_key_id() {
    let _guard = key_rot_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = KeyRotationStorage::new(pool.clone());

    let user_id = unique_user_id();
    let device_id = format!("DEV_{}", unique_id());
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        "INSERT INTO key_rotation_log (user_id, device_id, room_id, rotation_type, old_key_id, new_key_id, reason, rotated_at) \
         VALUES ($1, $2, NULL, 'megolm', 'old_k', 'new_k', NULL, $3)",
    )
    .bind(&user_id)
    .bind(&device_id)
    .bind(now)
    .execute(pool.as_ref())
    .await
    .unwrap();

    let result = storage.get_last_rotation_for_key(&user_id, "old_k").await.unwrap();
    assert!(result.is_some());

    cleanup_user(pool.as_ref(), &user_id).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_last_rotation_for_key_not_found() {
    let _guard = key_rot_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = KeyRotationStorage::new(pool.clone());

    let user_id = unique_user_id();
    let result = storage.get_last_rotation_for_key(&user_id, "no_such_key").await.unwrap();
    assert!(result.is_none());
}

// ===========================================================================
// get_max_rotation_ts
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_max_rotation_ts_none_returns_zero() {
    let _guard = key_rot_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = KeyRotationStorage::new(pool.clone());

    let user_id = unique_user_id();
    let result = storage.get_max_rotation_ts(&user_id).await.unwrap();
    assert_eq!(result, 0);
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_max_rotation_ts_returns_max() {
    let _guard = key_rot_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = KeyRotationStorage::new(pool.clone());

    let user_id = unique_user_id();
    let device_id = format!("DEV_{}", unique_id());
    let ts1 = chrono::Utc::now().timestamp_millis();
    let ts2 = ts1 + 5000;
    sqlx::query(
        "INSERT INTO key_rotation_log (user_id, device_id, room_id, rotation_type, old_key_id, new_key_id, reason, rotated_at) \
         VALUES ($1, $2, NULL, 'megolm', NULL, 'k1', NULL, $3)",
    )
    .bind(&user_id)
    .bind(&device_id)
    .bind(ts1)
    .execute(pool.as_ref())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO key_rotation_log (user_id, device_id, room_id, rotation_type, old_key_id, new_key_id, reason, rotated_at) \
         VALUES ($1, $2, NULL, 'megolm', NULL, 'k2', NULL, $3)",
    )
    .bind(&user_id)
    .bind(&device_id)
    .bind(ts2)
    .execute(pool.as_ref())
    .await
    .unwrap();

    let result = storage.get_max_rotation_ts(&user_id).await.unwrap();
    assert_eq!(result, ts2, "should return the maximum timestamp");

    cleanup_user(pool.as_ref(), &user_id).await;
}

// ===========================================================================
// set_rotation_config / get_rotation_config
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_set_and_get_rotation_config() {
    let _guard = key_rot_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = KeyRotationStorage::new(pool.clone());

    let key = format!("test_key_{}", unique_id());
    storage.set_rotation_config(&key, "test_value").await.unwrap();

    let value = storage.get_rotation_config(&key).await.unwrap();
    assert_eq!(value.as_deref(), Some("test_value"));
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_rotation_config_missing_returns_none() {
    let _guard = key_rot_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = KeyRotationStorage::new(pool.clone());

    let value = storage.get_rotation_config("nonexistent_key").await.unwrap();
    assert!(value.is_none());
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_set_rotation_config_upsert() {
    let _guard = key_rot_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = KeyRotationStorage::new(pool.clone());

    let key = format!("upsert_key_{}", unique_id());
    storage.set_rotation_config(&key, "v1").await.unwrap();
    storage.set_rotation_config(&key, "v2").await.unwrap();

    let value = storage.get_rotation_config(&key).await.unwrap();
    assert_eq!(value.as_deref(), Some("v2"));
}

// ===========================================================================
// KeyRotationConfig load/persist
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_key_rotation_config_load_defaults_when_empty() {
    let _guard = key_rot_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = KeyRotationStorage::new(pool.clone());

    let config = KeyRotationConfig::load_from_storage(&storage).await.unwrap();
    assert_eq!(config.olm_rotation_days, 7);
    assert_eq!(config.megolm_rotation_messages, 100);
    assert_eq!(config.max_session_age_days, 90);
    assert!(config.enable_auto_rotation);
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_key_rotation_config_persist_and_load_roundtrip() {
    let _guard = key_rot_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = KeyRotationStorage::new(pool.clone());

    let config = KeyRotationConfig {
        olm_rotation_days: 14,
        megolm_rotation_messages: 200,
        max_session_age_days: 180,
        enable_auto_rotation: false,
    };
    config.persist_to_storage(&storage).await.unwrap();

    let loaded = KeyRotationConfig::load_from_storage(&storage).await.unwrap();
    assert_eq!(loaded.olm_rotation_days, 14);
    assert_eq!(loaded.megolm_rotation_messages, 200);
    assert_eq!(loaded.max_session_age_days, 180);
    assert!(!loaded.enable_auto_rotation);
}

// ===========================================================================
// get_encrypted_rooms / get_encrypted_room_members
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_encrypted_rooms() {
    let _guard = key_rot_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = KeyRotationStorage::new(pool.clone());

    let user_id = unique_user_id();
    let room_id = unique_room_id();
    insert_encrypted_room(pool.as_ref(), &room_id, &user_id).await;

    let rooms = storage.get_encrypted_rooms(&user_id).await.unwrap();
    assert!(rooms.contains(&room_id), "encrypted room should be returned");

    cleanup_room(pool.as_ref(), &room_id).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_encrypted_rooms_empty_for_unknown_user() {
    let _guard = key_rot_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = KeyRotationStorage::new(pool.clone());

    let user_id = unique_user_id();
    let rooms = storage.get_encrypted_rooms(&user_id).await.unwrap();
    assert!(rooms.is_empty());
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_encrypted_room_members() {
    let _guard = key_rot_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = KeyRotationStorage::new(pool.clone());

    let user_id = unique_user_id();
    let room_id = unique_room_id();
    insert_encrypted_room(pool.as_ref(), &room_id, &user_id).await;

    let members = storage.get_encrypted_room_members(&room_id).await.unwrap();
    assert!(members.contains(&user_id), "joining user should be a member");

    cleanup_room(pool.as_ref(), &room_id).await;
}
