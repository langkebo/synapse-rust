//! Guards the account-deactivation purge against **silently forgetting a table**.
//!
//! Deactivation only flips `users.is_deactivated`; the `users` row itself is
//! never deleted, so every `ON DELETE CASCADE` pointing at `users` stays dormant.
//! Anything account-scoped therefore has to be deleted explicitly, and the only
//! thing a per-table row count can never tell you is that a table was left out
//! entirely — a table nobody deletes from reports `0` forever and looks exactly
//! like "this account had none".
//!
//! Two tests close that hole:
//!
//! * [`test_every_registered_purge_target_is_a_real_user_scoped_table`] checks the
//!   registry against the migration baseline: every entry exists and carries a
//!   `user_id` column, so `DELETE … WHERE user_id = $1` is well-formed. Runs
//!   without a database.
//! * [`test_purge_empties_every_registered_table_for_that_user_only`] seeds one
//!   row per registered table for two users, runs the real
//!   [`StorageUserDataPurger`], and asserts the target user is emptied in *every*
//!   table while the control user is untouched. A forgotten table, a
//!   `WHERE`-clause that is too broad, or a count that does not match the rows
//!   actually removed all fail here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use synapse_cache::{CacheConfig, CacheManager};
use synapse_services::account_deactivation_service::{
    PurgedUserData, StorageUserDataPurger, UserDataPurger, ACCOUNT_DEACTIVATION_PURGED_TABLES,
};

const BASELINE_SQL: &str = include_str!("../../migrations/00000000_unified_schema_v12.sql");

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

/// Body of `CREATE TABLE [IF NOT EXISTS] <table> ( … )` in the migration baseline.
///
/// The trailing ` (` is part of the needle on purpose: without it `presence` would
/// also match the `presence_subscriptions` declaration that follows it.
fn table_body<'a>(sql: &'a str, table: &str) -> Option<&'a str> {
    let needle = format!("CREATE TABLE IF NOT EXISTS {table} (");
    let start = sql.find(&needle)? + needle.len();
    let rest = &sql[start..];
    let end = rest.find("\n);")?;
    Some(&rest[..end])
}

/// Does this table's DDL declare a `user_id` column?
fn declares_user_id_column(body: &str) -> bool {
    body.lines().any(|line| {
        let line = line.trim();
        // `user_id TEXT NOT NULL,` — but not `CONSTRAINT fk_x FOREIGN KEY (user_id) …`.
        !line.starts_with("CONSTRAINT")
            && !line.starts_with("--")
            && line
                .strip_prefix("user_id")
                .is_some_and(|rest| rest.starts_with(char::is_whitespace) || rest.starts_with('\t'))
    })
}

#[test]
fn test_every_registered_purge_target_is_a_real_user_scoped_table() {
    // Non-empty: an accidentally emptied registry would otherwise pass vacuously.
    assert!(
        !ACCOUNT_DEACTIVATION_PURGED_TABLES.is_empty(),
        "ACCOUNT_DEACTIVATION_PURGED_TABLES must list the tables the purge owns"
    );

    for table in ACCOUNT_DEACTIVATION_PURGED_TABLES {
        let body = table_body(BASELINE_SQL, table)
            .unwrap_or_else(|| panic!("`{table}` is listed as purged but is not in the migration baseline"));
        assert!(
            declares_user_id_column(body),
            "`{table}` is listed as purged but its DDL has no `user_id` column, so `DELETE … WHERE user_id = $1` cannot be scoped"
        );
    }

    // No duplicates: the same table listed twice would mean a double delete and a
    // misleading "N rows purged" report.
    let mut seen = std::collections::HashSet::new();
    for table in ACCOUNT_DEACTIVATION_PURGED_TABLES {
        assert!(seen.insert(*table), "`{table}` is listed twice in ACCOUNT_DEACTIVATION_PURGED_TABLES");
    }
}

/// Rows the purge reported for `table`, read off [`PurgedUserData`].
fn reported_count(counts: &PurgedUserData, table: &str) -> u64 {
    match table {
        "account_data" => counts.account_data_deleted,
        "room_account_data" => counts.room_account_data_deleted,
        "push_rules" => counts.push_rules_deleted,
        "thread_subscriptions" => counts.thread_subscriptions_deleted,
        "user_threepids" => counts.threepids_removed,
        "key_backups" => counts.key_backups_deleted,
        "secure_key_backups" => counts.secure_backups_deleted,
        "room_tags" => counts.room_tags_deleted,
        "event_receipts" => counts.event_receipts_deleted,
        "event_receipts_stream" => counts.event_receipts_stream_deleted,
        "thread_read_receipts" => counts.thread_read_receipts_deleted,
        "presence" => counts.presence_deleted,
        other => panic!("`{other}` is in ACCOUNT_DEACTIVATION_PURGED_TABLES but PurgedUserData has no count for it"),
    }
}

async fn count_rows(pool: &sqlx::PgPool, table: &str, user_id: &str) -> i64 {
    // `table` comes from ACCOUNT_DEACTIVATION_PURGED_TABLES (a compile-time
    // constant), never from input; identifiers cannot be bound as `$1` anyway.
    let sql = format!("SELECT COUNT(*) FROM {table} WHERE user_id = $1");
    sqlx::query_scalar::<_, i64>(&sql).bind(user_id).fetch_one(pool).await.unwrap()
}

async fn create_user(pool: &sqlx::PgPool, user_id: &str, suffix: u64) {
    sqlx::query("INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
        .bind(user_id)
        .bind(format!("purge_probe_{suffix}"))
        .bind(1_i64)
        .execute(pool)
        .await
        .unwrap();
}

async fn create_room(pool: &sqlx::PgPool, room_id: &str) {
    sqlx::query("INSERT INTO rooms (room_id, created_ts) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(room_id)
        .bind(1_i64)
        .execute(pool)
        .await
        .unwrap();
}

/// Insert exactly one row per table in [`ACCOUNT_DEACTIVATION_PURGED_TABLES`].
///
/// Only the columns the schema requires (NOT NULL without DEFAULT) are supplied;
/// the point is that the row exists before the purge and is gone after it.
async fn seed_all_purged_tables(pool: &sqlx::PgPool, user_id: &str, room_id: &str, suffix: u64) {
    let statements: Vec<String> = vec![
        format!(
            "INSERT INTO account_data (user_id, data_type, content, created_ts, updated_ts) \
             VALUES ('{user_id}', 'm.direct', '{{}}', 1, 1)"
        ),
        format!(
            "INSERT INTO room_account_data (user_id, room_id, data_type, data, created_ts, updated_ts) \
             VALUES ('{user_id}', '{room_id}', 'm.fully_read', '{{}}', 1, 1)"
        ),
        format!(
            "INSERT INTO push_rules (user_id, scope, rule_id, kind, created_ts) \
             VALUES ('{user_id}', 'global', '.m.rule.master_{suffix}', 'override', 1)"
        ),
        format!(
            "INSERT INTO thread_subscriptions (room_id, thread_id, user_id, subscribed_ts, updated_ts) \
             VALUES ('{room_id}', '$thread_{suffix}', '{user_id}', 1, 1)"
        ),
        format!(
            "INSERT INTO user_threepids (user_id, medium, address, added_ts) \
             VALUES ('{user_id}', 'email', 'purge-{suffix}@example.org', 1)"
        ),
        format!(
            "INSERT INTO key_backups (user_id, algorithm, created_ts) VALUES ('{user_id}', 'm.megolm_backup.v1.curve25519-aes-sha2', 1)"
        ),
        format!(
            "INSERT INTO secure_key_backups (user_id, backup_id, version, algorithm, auth_data) \
             VALUES ('{user_id}', 'backup_{suffix}', '1', 'm.megolm_backup.v1.curve25519-aes-sha2', '{{}}')"
        ),
        format!(
            "INSERT INTO room_tags (user_id, room_id, tag, created_ts) VALUES ('{user_id}', '{room_id}', 'm.favourite_{suffix}', 1)"
        ),
        format!(
            "INSERT INTO event_receipts (event_id, room_id, user_id, receipt_type, ts, created_ts, updated_ts) \
             VALUES ('$receipt_{suffix}', '{room_id}', '{user_id}', 'm.read', 1, 1, 1)"
        ),
        format!(
            "INSERT INTO event_receipts_stream (room_id, event_id, user_id, receipt_type, ts, created_ts) \
             VALUES ('{room_id}', '$receipt_{suffix}', '{user_id}', 'm.read', 1, 1)"
        ),
        format!(
            "INSERT INTO thread_read_receipts (room_id, thread_id, user_id, updated_ts) \
             VALUES ('{room_id}', '$thread_{suffix}', '{user_id}', 1)"
        ),
        format!(
            "INSERT INTO presence (user_id, presence, created_ts, updated_ts) VALUES ('{user_id}', 'online', 1, 1)"
        ),
    ];
    assert_eq!(
        statements.len(),
        ACCOUNT_DEACTIVATION_PURGED_TABLES.len(),
        "seed_all_purged_tables is out of sync with ACCOUNT_DEACTIVATION_PURGED_TABLES"
    );

    for (table, sql) in ACCOUNT_DEACTIVATION_PURGED_TABLES.iter().zip(statements) {
        sqlx::query(&sql).execute(pool).await.unwrap_or_else(|e| panic!("failed to seed `{table}`: {e}"));
    }
}

#[tokio::test]
async fn test_purge_empties_every_registered_table_for_that_user_only() {
    let pool = crate::require_test_pool().await;
    let suffix = unique_id();
    let target = format!("@purge_target_{suffix}:localhost");
    let control = format!("@purge_control_{suffix}:localhost");
    let room_id = format!("!purge_room_{suffix}:localhost");

    create_user(&pool, &target, suffix).await;
    create_user(&pool, &control, suffix + 1_000_000).await;
    create_room(&pool, &room_id).await;

    seed_all_purged_tables(&pool, &target, &room_id, suffix).await;
    seed_all_purged_tables(&pool, &control, &room_id, suffix + 1_000_000).await;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let purger = StorageUserDataPurger::new(&pool, cache);
    let counts = purger.purge(&target).await.unwrap();

    for table in ACCOUNT_DEACTIVATION_PURGED_TABLES {
        assert_eq!(
            count_rows(&pool, table, &target).await,
            0,
            "`{table}` still holds rows for the deactivated account — it is listed as purged but the purge does not reach it"
        );
        assert_eq!(
            count_rows(&pool, table, &control).await,
            1,
            "`{table}` lost the control user's row — the purge is not scoped to the deactivated account"
        );
        assert_eq!(reported_count(&counts, table), 1, "`{table}` reported the wrong number of removed rows");
    }

    // The `users` row itself must survive: deactivation is a flag, not a delete,
    // and the whole reason the purge has to be explicit is that cascades stay dormant.
    let still_there = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users WHERE user_id = $1")
        .bind(&target)
        .fetch_one(&*pool)
        .await
        .unwrap();
    assert_eq!(still_there, 1, "deactivation must not delete the `users` row");
}

#[tokio::test]
async fn test_purge_is_idempotent_and_reports_zero_the_second_time() {
    let pool = crate::require_test_pool().await;
    let suffix = unique_id();
    let user_id = format!("@purge_idempotent_{suffix}:localhost");
    let room_id = format!("!purge_idempotent_room_{suffix}:localhost");

    create_user(&pool, &user_id, suffix).await;
    create_room(&pool, &room_id).await;
    seed_all_purged_tables(&pool, &user_id, &room_id, suffix).await;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let purger = StorageUserDataPurger::new(&pool, cache);

    let first = purger.purge(&user_id).await.unwrap();
    assert_eq!(reported_count(&first, "room_tags"), 1);

    // A retried deactivation (the pipeline is designed to be re-runnable) must not
    // blow up on already-absent rows, and must report the truth: nothing left.
    let second = purger.purge(&user_id).await.unwrap();
    for table in ACCOUNT_DEACTIVATION_PURGED_TABLES {
        assert_eq!(reported_count(&second, table), 0, "`{table}` reported rows on a second purge");
    }
}
