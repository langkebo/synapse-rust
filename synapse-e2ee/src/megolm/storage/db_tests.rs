//! DB round trip for the 10 statements converted in C26, run against the real v12
//! baseline.
//!
//! Before C26 this module had **no DB coverage at all**: every case in the module
//! above is a pure constructor/serialization check. That is the blind spot which
//! hid D-46/D-49 behind simplified fixtures (D-36/D-47 family), and this file is
//! where D-49's second site lives (`megolm_sessions.message_index`, tightened to
//! `NOT NULL DEFAULT 0` in the C26 "fix first" commit — which is why the
//! `MegolmSessionRow.message_index: i64` projection needs no `AS "col!"`).
//!
//! `megolm_sessions` / `megolm_session_keys` carry no foreign keys in the baseline,
//! so no seed rows are needed (contrast `backup::storage::db_tests`, which must
//! create the `rooms` row `fk_backup_keys_room` demands).

use super::*;
use synapse_common::test_isolation::IsolatedTestPool;

/// The workspace baseline migration. The bytes are load-bearing (the shared
/// template name is a content fingerprint of this string), so it must stay
/// byte-identical to the copies in `synapse-storage/src/test_isolation.rs`,
/// `synapse-e2ee/src/olm/storage.rs` and `synapse-services/src/test_utils.rs`.
const BASELINE_SQL: &str = include_str!("../../../../migrations/00000000_unified_schema_v12.sql");

fn make_session(session_id: &str, room_id: &str, index: i64) -> MegolmSession {
    let created = Utc::now();
    MegolmSession {
        id: uuid::Uuid::new_v4(),
        session_id: session_id.to_string(),
        room_id: room_id.to_string(),
        sender_key: format!("sender-{session_id}"),
        session_key: format!("pickle-{session_id}"),
        algorithm: "m.megolm.v1.aes-sha2".to_string(),
        message_index: index,
        created_ts: created,
        last_used_ts: created,
        expires_at: None,
    }
}

#[tokio::test]
async fn test_megolm_round_trip_on_migration_template() {
    let isolated = IsolatedTestPool::new(BASELINE_SQL).await.expect("isolated test pool");
    let pool = isolated.pool();
    let storage = MegolmSessionStorage::new(&pool, KeyAtRest::new([0x42; 32]));

    let room_a = "!c26a:localhost";
    let room_b = "!c26b:localhost";

    // --- create_session -> get_session ---
    let s1 = make_session("sess-a1", room_a, 0);
    storage.create_session(&s1).await.unwrap();
    storage.create_session(&make_session("sess-a2", room_a, 7)).await.unwrap();
    storage.create_session(&make_session("sess-b1", room_b, 0)).await.unwrap();

    let loaded = storage.get_session("sess-a1").await.unwrap().expect("sess-a1");
    assert_eq!(loaded.id, s1.id);
    assert_eq!(loaded.session_id, "sess-a1");
    assert_eq!(loaded.room_id, room_a);
    assert_eq!(loaded.sender_key, "sender-sess-a1");
    assert_eq!(loaded.session_key, "pickle-sess-a1");
    assert_eq!(loaded.algorithm, "m.megolm.v1.aes-sha2");
    assert_eq!(loaded.message_index, 0);
    assert_eq!(loaded.created_ts.timestamp_millis(), s1.created_ts.timestamp_millis());
    assert_eq!(loaded.last_used_ts.timestamp_millis(), s1.last_used_ts.timestamp_millis());
    assert_eq!(loaded.expires_at, None);
    assert!(storage.get_session("missing").await.unwrap().is_none());

    // S-10: the `session_key` column must be sealed at rest while `get_session`
    // still hands back the plaintext pickle. The raw column is the only place
    // that is allowed to see ciphertext.
    let raw_key: String = sqlx::query_scalar("SELECT session_key FROM megolm_sessions WHERE session_id = 'sess-a1'")
        .fetch_one(&*pool)
        .await
        .unwrap();
    assert!(raw_key.starts_with("v1:"), "session_key must be at-rest encrypted (S-10): {raw_key}");
    assert_ne!(raw_key, "pickle-sess-a1", "the plaintext pickle must not be stored verbatim");

    // `session_id` is UNIQUE and `create_session` has no `ON CONFLICT`: the second
    // insert must surface a mapped DB error, not silently become an update.
    let duplicate = storage.create_session(&make_session("sess-a1", room_a, 0)).await;
    let err = duplicate.expect_err("a duplicate session_id must hit the UNIQUE constraint");
    assert_eq!(err.message, "Database error: Failed to create megolm session");

    // --- get_room_sessions ---
    // `message_index = 7` must survive as `i64` (D-49's second site) and the
    // projection must not need an `AS "message_index!"` assertion.
    let mut in_a = storage.get_room_sessions(room_a).await.unwrap();
    in_a.sort_by_key(|s| s.message_index);
    assert_eq!(in_a.iter().map(|s| s.session_id.as_str()).collect::<Vec<_>>(), vec!["sess-a1", "sess-a2"]);
    assert_eq!(in_a[1].message_index, 7);
    assert_eq!(storage.get_room_sessions(room_b).await.unwrap().len(), 1);
    assert!(storage.get_room_sessions("!empty:localhost").await.unwrap().is_empty());

    // --- update_session ---
    // Only the five SET columns may change; `created_ts`/`room_id` must survive.
    let mut updated = s1.clone();
    updated.session_key = "pickle-rotated".to_string();
    updated.message_index = 42;
    updated.last_used_ts = s1.last_used_ts + chrono::Duration::seconds(60);
    updated.expires_at = Some(s1.created_ts + chrono::Duration::hours(1));
    storage.update_session(&updated).await.unwrap();

    let reloaded = storage.get_session("sess-a1").await.unwrap().expect("sess-a1 after update");
    assert_eq!(reloaded.session_key, "pickle-rotated");
    assert_eq!(reloaded.message_index, 42);
    assert_eq!(reloaded.last_used_ts.timestamp_millis(), updated.last_used_ts.timestamp_millis());
    assert_eq!(reloaded.expires_at.map(|t| t.timestamp_millis()), updated.expires_at.map(|t| t.timestamp_millis()));
    assert_eq!(reloaded.room_id, room_a, "update_session must not touch room_id");
    assert_eq!(
        reloaded.created_ts.timestamp_millis(),
        s1.created_ts.timestamp_millis(),
        "update_session must not touch created_ts"
    );

    // --- increment_message_index (atomic UPDATE ... RETURNING) ---
    assert_eq!(storage.increment_message_index("sess-a1", 3, 1_000).await.unwrap(), Some(45));
    assert_eq!(storage.increment_message_index("sess-a1", 0, 2_000).await.unwrap(), Some(45));
    assert_eq!(
        storage.increment_message_index("missing", 1, 0).await.unwrap(),
        None,
        "an unknown session must yield None, not a fabricated index"
    );
    let after_increment = storage.get_session("sess-a1").await.unwrap().unwrap();
    assert_eq!(after_increment.message_index, 45);
    // The same statement also writes `last_used_ts`.
    assert_eq!(after_increment.last_used_ts.timestamp_millis(), 2_000);

    // --- cleanup_expired_sessions: only `expires_at IS NOT NULL AND < now` ---
    let mut past = make_session("sess-past", room_a, 0);
    past.expires_at = Some(Utc::now() - chrono::Duration::hours(1));
    storage.create_session(&past).await.unwrap();
    let mut future = make_session("sess-future", room_a, 0);
    future.expires_at = Some(Utc::now() + chrono::Duration::hours(1));
    storage.create_session(&future).await.unwrap();

    assert_eq!(storage.cleanup_expired_sessions().await.unwrap(), 1, "only the past-expiry row is due");
    assert!(storage.get_session("sess-past").await.unwrap().is_none());
    // A `NULL` expiry (sess-a2, sess-b1) and a non-NULL but not-yet-due expiry
    // (sess-future) must both survive.
    assert!(storage.get_session("sess-future").await.unwrap().is_some());
    assert!(storage.get_session("sess-a2").await.unwrap().is_some());
    assert!(storage.get_session("sess-b1").await.unwrap().is_some());

    // --- upsert_session_keys_batch / get_session_key ---
    assert_eq!(
        storage.upsert_session_keys_batch(&[], "sess-a1", "k", 1, None).await.unwrap(),
        0,
        "an empty user list must short-circuit to 0 without touching the table"
    );

    let users = vec!["@c26a:localhost".to_string(), "@c26b:localhost".to_string()];
    assert_eq!(storage.upsert_session_keys_batch(&users, "sess-a1", "enc-1", 100, None).await.unwrap(), 2);
    assert_eq!(storage.get_session_key("@c26a:localhost", "sess-a1").await.unwrap().as_deref(), Some("enc-1"));
    assert!(storage.get_session_key("@c26a:localhost", "sess-missing").await.unwrap().is_none());

    // Re-upserting the same (user_id, session_id) pairs must update in place, not
    // duplicate: `rows_affected` stays 2 and the key is rewritten.
    assert_eq!(
        storage.upsert_session_keys_batch(&users, "sess-a1", "enc-2", 200, Some(9_000_000_000_000)).await.unwrap(),
        2
    );
    assert_eq!(storage.get_session_key("@c26b:localhost", "sess-a1").await.unwrap().as_deref(), Some("enc-2"));
    let key_rows: i64 =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM megolm_session_keys").fetch_one(&*pool).await.unwrap();
    assert_eq!(key_rows, 2, "the ON CONFLICT upsert must not duplicate rows");
    // A second session for the same users must add exactly two more rows.
    assert_eq!(storage.upsert_session_keys_batch(&users, "sess-b1", "enc-3", 300, None).await.unwrap(), 2);
    let key_rows: i64 =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM megolm_session_keys").fetch_one(&*pool).await.unwrap();
    assert_eq!(key_rows, 4, "(user_id, session_id) is the conflict target, not user_id alone");

    // --- delete_session ---
    storage.delete_session("sess-a1").await.unwrap();
    assert!(storage.get_session("sess-a1").await.unwrap().is_none());
    // Deleting an already-absent row is a no-op, not an error.
    storage.delete_session("sess-a1").await.unwrap();
    // Deleting a session does not cascade to `megolm_session_keys` (no FK):
    // the keys are cleaned up by their own path. Pin that so an added FK is noticed.
    let key_rows: i64 =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM megolm_session_keys WHERE session_id = 'sess-a1'")
            .fetch_one(&*pool)
            .await
            .unwrap();
    assert_eq!(key_rows, 2, "megolm_session_keys has no FK to megolm_sessions in the baseline");
}

/// S-10 legacy compatibility: a row written before at-rest encryption holds a
/// plaintext pickle (no `v1:` prefix). It must still read back verbatim, and a
/// subsequent write must re-seal it — the lazy migration path.
#[tokio::test]
async fn legacy_plaintext_session_key_reads_and_reseals() {
    let isolated = IsolatedTestPool::new(BASELINE_SQL).await.expect("isolated test pool");
    let pool = isolated.pool();
    let storage = MegolmSessionStorage::new(&pool, KeyAtRest::new([0x42; 32]));

    let legacy = make_session("sess-legacy", "!legacy:localhost", 0);
    sqlx::query(
        "INSERT INTO megolm_sessions (id, session_id, room_id, sender_key, session_key, algorithm, \
         message_index, created_ts, last_used_ts, expires_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(legacy.id)
    .bind(&legacy.session_id)
    .bind(&legacy.room_id)
    .bind(&legacy.sender_key)
    .bind("plaintext-legacy-pickle")
    .bind(&legacy.algorithm)
    .bind(legacy.message_index)
    .bind(legacy.created_ts.timestamp_millis())
    .bind(legacy.last_used_ts.timestamp_millis())
    .bind(Option::<i64>::None)
    .execute(&*pool)
    .await
    .unwrap();

    // Read: pre-S-10 plaintext is returned as-is (no error, no mangling).
    let loaded = storage.get_session("sess-legacy").await.unwrap().expect("legacy row");
    assert_eq!(loaded.session_key, "plaintext-legacy-pickle");

    // Write: the row is migrated to ciphertext on the next update.
    storage.update_session(&loaded).await.unwrap();
    let raw: String = sqlx::query_scalar("SELECT session_key FROM megolm_sessions WHERE session_id = 'sess-legacy'")
        .fetch_one(&*pool)
        .await
        .unwrap();
    assert!(raw.starts_with("v1:"), "the legacy row must be re-sealed on write: {raw}");
    assert_eq!(
        storage.get_session("sess-legacy").await.unwrap().expect("re-read").session_key,
        "plaintext-legacy-pickle",
        "the migrated row must still read back as the original plaintext"
    );
}
