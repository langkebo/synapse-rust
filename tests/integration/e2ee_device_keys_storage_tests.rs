//! DB round-trips for `synapse-e2ee` device-key storage defects registered in
//! `docs/audit/SQLX_STATICIZATION_PLAN_2026-09-23.md` §7.2.
//!
//! * **D-07** `record_device_list_change` used to return `()` and swallow both statements'
//!   errors (`let Ok(..) else { return }` / `let _ =`), so a device-list change that never
//!   reached `device_lists_changes` looked exactly like success — peers would silently keep
//!   a stale device list. It is now fallible and each caller decides.
//!   **RED**: with the old code the failure-injection assertion below could not be written at
//!   all (there was no error to observe), and the "both rows" assertion fails because the
//!   first statement's failure returns early — the method reports success either way.
//! * **D-08** `claim_one_time_key`'s `target`/`fb` CTEs had `LIMIT 1` with no `ORDER BY`, so
//!   which key a claim handed out was whatever the heap scan happened to hit first. The test
//!   inserts the *oldest* key **last** so heap order and `added_ts` order disagree.
//!   **RED**: without `ORDER BY added_ts, id` the first claim returns `NEWEST`.
//!
//! Both tests run on a per-test schema cloned from the migrated v12 baseline, so the real
//! NOT NULL / UNIQUE constraints apply (D-36).

use std::sync::Arc;

use sqlx::PgPool;
use synapse_common::test_isolation::IsolatedTestPool;
use synapse_e2ee::device_keys::{DeviceKeyStorage, DeviceKeyStoreApi};

const BASELINE_SQL: &str = include_str!("../../migrations/00000000_unified_schema_v12.sql");

async fn test_pool() -> (IsolatedTestPool, Arc<PgPool>) {
    let isolated = IsolatedTestPool::new(BASELINE_SQL).await.expect("isolated test pool");
    let pool = isolated.pool();
    (isolated, pool)
}

fn unique(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::new_v4().as_simple())
}

// =============================================================================
// D-07
// =============================================================================

#[tokio::test]
async fn record_device_list_change_writes_both_stream_and_changes_rows() {
    let (_isolated, pool) = test_pool().await;
    let storage = DeviceKeyStorage::new(&pool);
    let user_id = format!("@{}:test.local", unique("dl"));

    storage
        .record_device_list_change(&user_id, Some("DEV1"), "changed")
        .await
        .expect("recording a device list change must succeed on the migrated schema");

    let (stream_id, change_type): (i64, String) = sqlx::query_as(
        "SELECT (SELECT stream_id FROM device_lists_stream WHERE user_id = $1), \
                (SELECT change_type FROM device_lists_changes WHERE user_id = $1)",
    )
    .bind(&user_id)
    .fetch_one(&*pool)
    .await
    .expect("both device-list rows must exist");

    assert!(stream_id > 0, "stream_id must come from the sequence, got {stream_id}");
    assert_eq!(change_type, "changed");
}

/// Failure injection, first statement: make the `device_lists_stream` insert itself fail.
///
/// A `CHECK` that rejects only this test's rows is used rather than `DROP TABLE`, because the
/// isolated pool's `search_path` is `"<schema>", public`: dropping the per-test table simply
/// lets the unqualified INSERT fall through to `public.device_lists_stream` and **succeed**.
/// (That silent fallback is itself worth remembering — it is why "drop the table" is a bad
/// failure injector here.)
#[tokio::test]
async fn record_device_list_change_propagates_a_stream_insert_failure() {
    let (_isolated, pool) = test_pool().await;
    let storage = DeviceKeyStorage::new(&pool);
    let user_id = format!("@{}:test.local", unique("dl_stream_fail"));

    sqlx::query("ALTER TABLE device_lists_stream ADD CONSTRAINT probe_reject_stream_ts CHECK (created_ts < 0)")
        .execute(&*pool)
        .await
        .expect("failure injection: the probe constraint must be added to the per-test table");

    // `current_timestamp_millis()` is always > 0, so the stream insert must fail. Before D-07
    // that failure was swallowed by `let Ok(..) else { return }` and the call reported success.
    let error = storage
        .record_device_list_change(&user_id, Some("DEV1"), "changed")
        .await
        .expect_err("a failed device_lists_stream insert must surface as an error, not a silent success");
    assert!(!error.to_string().is_empty(), "the error must carry the database message");

    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM device_lists_stream WHERE user_id = $1")
        .bind(&user_id)
        .fetch_one(&*pool)
        .await
        .expect("count must succeed");
    assert_eq!(rows, 0, "the failed insert must not have written a row");
}

/// Failure injection, second statement: the stream row is written, then the change row is
/// rejected — the half that used to be swallowed by `let _ =`.
#[tokio::test]
async fn record_device_list_change_propagates_a_changes_insert_failure() {
    let (_isolated, pool) = test_pool().await;
    let storage = DeviceKeyStorage::new(&pool);
    let user_id = format!("@{}:test.local", unique("dl_changes_fail"));

    sqlx::query(
        "ALTER TABLE device_lists_changes ADD CONSTRAINT probe_reject_change_type CHECK (change_type <> 'probe_reject')",
    )
    .execute(&*pool)
    .await
    .expect("failure injection: the probe constraint must be added to the per-test table");

    let error = storage
        .record_device_list_change(&user_id, Some("DEV1"), "probe_reject")
        .await
        .expect_err("a rejected device_lists_changes insert must surface as an error");
    assert!(!error.to_string().is_empty(), "the error must carry the database message");

    // The partial failure is visible in the data: the stream row landed, the change row did not.
    let streamed: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM device_lists_stream WHERE user_id = $1")
        .bind(&user_id)
        .fetch_one(&*pool)
        .await
        .expect("count must succeed");
    let changed: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM device_lists_changes WHERE user_id = $1")
        .bind(&user_id)
        .fetch_one(&*pool)
        .await
        .expect("count must succeed");
    assert_eq!(streamed, 1, "the first statement had already written its row");
    assert_eq!(changed, 0, "the rejected change row must not exist");
}

// =============================================================================
// D-08
// =============================================================================

#[tokio::test]
async fn claim_one_time_key_hands_out_the_oldest_key_first() {
    let (_isolated, pool) = test_pool().await;
    let storage = DeviceKeyStorage::new(&pool);
    let user_id = format!("@{}:test.local", unique("otk"));
    let device_id = "DEVICE_OTK";
    let algorithm = "signed_curve25519";

    // `device_keys.user_id` carries `fk_device_keys_user_id`, so the fixture needs a real user.
    crate::ensure_test_user(&pool, &user_id).await;

    // Insert newest-first so heap/insertion order is the exact reverse of `added_ts` order:
    // `id` is BIGSERIAL, so `OLDEST` has the *largest* id and the smallest added_ts.
    let base = synapse_common::current_timestamp_millis();
    let rows = [("NEWEST", base + 200), ("MIDDLE", base + 100), ("OLDEST", base)];
    for (key_id, added_ts) in rows {
        sqlx::query(
            "INSERT INTO device_keys \
             (user_id, device_id, algorithm, key_id, public_key, added_ts, created_ts, updated_ts, is_fallback) \
             VALUES ($1, $2, $3, $4, $5, $6, $6, $6, FALSE)",
        )
        .bind(&user_id)
        .bind(device_id)
        .bind(algorithm)
        .bind(key_id)
        .bind(format!("pub_{key_id}"))
        .bind(added_ts)
        .execute(&*pool)
        .await
        .expect("fixture: inserting a one-time key must succeed");
    }

    let mut claimed = Vec::new();
    for _ in 0..3 {
        let key = storage
            .claim_one_time_key(&user_id, device_id, algorithm)
            .await
            .expect("claim must succeed")
            .expect("a key must still be available");
        claimed.push(key.key_id);
    }

    assert_eq!(
        claimed,
        vec!["OLDEST".to_string(), "MIDDLE".to_string(), "NEWEST".to_string()],
        "claims must follow `added_ts, id` (oldest first), not arbitrary heap order"
    );
}
