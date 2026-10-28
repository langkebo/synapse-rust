//! Tests for the cross-process schema pool table (the nextest path): a name
//! parked by one process must reach exactly one later process, and a name that
//! can no longer serve a test must never be handed out.
//!
//! These drive the pool helpers directly against a throwaway schema (`CREATE
//! SCHEMA`, no template clone) so they stay cheap.
//!
//! Split out of `lib.rs` (2026-10-28): that file is grandfathered under a
//! 2995-line ceiling in `scripts/ci/check_file_size_ratchet.py`, and a test
//! module that only touches the pool has no reason to live inside it.

use crate::{
    claim_schema_from_db_pool, connect_schema_pool_admin, current_timestamp_millis, drop_schema,
    ensure_db_schema_pool_table, park_schema_in_db_pool, reap_expired_db_pool_entries, DB_SCHEMA_POOL_TABLE,
    DB_SCHEMA_POOL_TTL,
};
use sqlx::PgPool;
use std::time::Duration;
use synapse_common::test_schema_orphans::{reclaim_orphan_test_schemas, stamped_schema_created_nanos};

/// Unique schema/template names, so a concurrent test (or a row left behind by
/// a killed run) can never collide with this one.
fn scratch_names(tag: &str) -> (String, String) {
    let unique = format!("{}_{}", std::process::id(), current_timestamp_millis());
    (format!("test_pool_guard_{tag}_{unique}"), format!("test_template_pool_guard_{tag}_{unique}"))
}

async fn admin_pool() -> Option<PgPool> {
    if std::env::var("TEST_DATABASE_URL").is_err() {
        eprintln!("SKIPPING cross-process schema pool test: TEST_DATABASE_URL is unset, so this proves nothing.");
        return None;
    }
    let database_url = crate::resolve_test_database_url().await.expect("database url");
    let pool = connect_schema_pool_admin(&database_url).await.expect("schema pool admin connection");
    ensure_db_schema_pool_table(&pool).await.expect("pool table");
    Some(pool)
}

async fn create_scratch_schema(pool: &PgPool, schema_name: &str) {
    sqlx::query(&format!("CREATE SCHEMA {schema_name}"))
        .execute(pool)
        .await
        .unwrap_or_else(|error| panic!("could not create scratch schema {schema_name}: {error}"));
}

/// 停靠表里 `schema_name` 的行数；`min_parked_ts` 给定时只数 `parked_ts >= ` 的那些。
///
/// 收成一个 helper 而不是让每个用例各写一条 SQL：`#[cfg(test)]` 里的宏不进
/// `cargo sqlx prepare`，每多一处 `query_scalar(&format!(…))` 都会顶高
/// `scripts/ci/check_sqlx_dynamic_ratio.sh` 的 **dynamic_test** 棘轮
/// （`dynamic_test 不得增加`）—— 而"多一条只差一个谓词的查询"纯粹是重复。
async fn pool_rows(pool: &PgPool, schema_name: &str, min_parked_ts: Option<i64>) -> i64 {
    sqlx::query_scalar(&format!(
        "SELECT count(*) FROM {DB_SCHEMA_POOL_TABLE} \
         WHERE schema_name = $1 AND ($2::bigint IS NULL OR parked_ts >= $2)"
    ))
    .bind(schema_name)
    .bind(min_parked_ts)
    .fetch_one(pool)
    .await
    .expect("count pool rows")
}

/// The happy path the whole optimization rests on: park, claim once, and the
/// name is gone — never handed to two processes.
#[tokio::test]
async fn a_parked_schema_is_handed_out_exactly_once() {
    let Some(pool) = admin_pool().await else { return };
    let (schema_name, template_name) = scratch_names("park_once");
    create_scratch_schema(&pool, &schema_name).await;
    park_schema_in_db_pool(&pool, &schema_name, &template_name, current_timestamp_millis())
        .await
        .expect("park the scratch schema");

    let claimed = claim_schema_from_db_pool(&pool, &template_name).await.expect("claim");
    assert_eq!(
        claimed.as_deref(),
        Some(schema_name.as_str()),
        "the parked schema must be handed to the next acquisition"
    );
    let claimed_again = claim_schema_from_db_pool(&pool, &template_name).await.expect("second claim");
    assert_eq!(claimed_again, None, "a claimed schema must never be handed out twice");

    let _ = drop_schema(&pool, &schema_name).await;
    pool.close().await;
}

/// A name belongs to the template it was TRUNCATEd against. Handing it to a
/// different template would serve mismatched tables.
#[tokio::test]
async fn a_schema_parked_for_one_template_is_not_claimed_for_another() {
    let Some(pool) = admin_pool().await else { return };
    let (schema_name, template_name) = scratch_names("template");
    create_scratch_schema(&pool, &schema_name).await;
    park_schema_in_db_pool(&pool, &schema_name, &template_name, current_timestamp_millis())
        .await
        .expect("park the scratch schema");

    let mismatched = claim_schema_from_db_pool(&pool, &format!("{template_name}_other")).await.expect("claim");
    assert_eq!(mismatched, None, "a schema parked for another template must not be claimed");
    let claimed = claim_schema_from_db_pool(&pool, &template_name).await.expect("claim");
    assert_eq!(
        claimed.as_deref(),
        Some(schema_name.as_str()),
        "the row must survive the mismatched claim and still serve its own template"
    );

    let _ = drop_schema(&pool, &schema_name).await;
    pool.close().await;
}

/// A row whose schema has been dropped (killed run, manual cleanup) must be
/// discarded at claim time — a name from a `CREATE SCHEMA`-less test pool
/// fails `ensure_schema_has_baseline` hard.
#[tokio::test]
async fn a_row_whose_schema_is_gone_is_discarded_instead_of_handed_out() {
    let Some(pool) = admin_pool().await else { return };
    let (schema_name, template_name) = scratch_names("vanished");
    create_scratch_schema(&pool, &schema_name).await;
    park_schema_in_db_pool(&pool, &schema_name, &template_name, current_timestamp_millis())
        .await
        .expect("park the scratch schema");
    let _ = drop_schema(&pool, &schema_name).await;

    let claimed = claim_schema_from_db_pool(&pool, &template_name).await.expect("claim");
    assert_eq!(claimed, None, "a name whose schema no longer exists must never reach a test pool");
    assert_eq!(
        pool_rows(&pool, &schema_name, None).await,
        0,
        "the stale row must be deleted, not retried on every acquisition"
    );

    pool.close().await;
}

/// Entries parked past the TTL are reaped together with their schemas, so a
/// crashed run cannot hold 255 tables per abandoned schema forever.
///
/// The TTL is minutes, not milliseconds, because the reaper is unscoped by
/// design (`DELETE ... WHERE parked_ts < cutoff` over the whole shared
/// table): a one-second window reaped whatever another test had parked a
/// moment earlier and dropped its schema under it. Five minutes reclaims
/// exactly the back-dated row below and cannot touch a row parked now.
#[tokio::test]
async fn entries_parked_past_the_ttl_are_reaped_and_their_schema_dropped() {
    let Some(pool) = admin_pool().await else { return };
    let (schema_name, template_name) = scratch_names("reap");
    create_scratch_schema(&pool, &schema_name).await;
    let stale_ts = current_timestamp_millis() - 600_000;
    park_schema_in_db_pool(&pool, &schema_name, &template_name, stale_ts).await.expect("park the scratch schema");

    reap_expired_db_pool_entries(&pool, current_timestamp_millis(), Duration::from_secs(300))
        .await
        .expect("reap expired entries");

    assert_eq!(pool_rows(&pool, &schema_name, None).await, 0, "an entry parked past the TTL must be deleted");
    let remaining: Option<String> = sqlx::query_scalar("SELECT to_regnamespace($1)::text")
        .bind(&schema_name)
        .fetch_one(&pool)
        .await
        .expect("probe the reaped schema");
    assert!(remaining.is_none(), "reaping must drop the schema, not just forget its name");

    pool.close().await;
}

/// A schema parked for reuse is older than `ORPHAN_SCHEMA_MIN_AGE` by design
/// — the name carries the instant of the *first* clone, not the last park —
/// so age alone classifies it as an orphan, and the orphan guard runs once
/// per test process, i.e. constantly under nextest. It must skip parked
/// names, or a long gate run drops the schemas the pool exists to recycle.
///
/// The budget is unbounded on purpose: with a limited one the fixture could
/// be merely *deferred* (and the test would pass even with the guard bug),
/// while the leftovers an unbounded sweep drops are exactly what the guard
/// drops in every test process anyway — aged, unleased per-test schemas.
///
/// The *age window*, in contrast, is narrowed to a single candidate: the
/// fixture is dated a month back, far past anything a previous run can
/// leave behind, so an unbounded budget still sweeps exactly one schema.
/// Against the production threshold the coverage lane's database held ~100
/// leftovers from an hour earlier: one second per `DROP SCHEMA ... CASCADE`
/// made this test run 139 s, the drops collided with other tests' clones
/// (`out of shared memory`), and the parked row was reaped by a concurrent
/// short-TTL reaper long before the surviving-schema probe ran.
#[tokio::test]
async fn the_orphan_guard_skips_schemas_parked_for_reuse() {
    let Some(pool) = admin_pool().await else { return };
    let min_age = Duration::from_secs(30 * 24 * 60 * 60);
    let now_nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    // Stamped shape (`test_<pid>_<n>_<nanos>`), created a month ago: exactly
    // what a recycled schema looks like in the second half of a gate run.
    let created_nanos = now_nanos.saturating_sub(min_age.as_nanos()).saturating_sub(3_600_000_000_000);
    let schema_name = format!("test_{}_{}_{}", std::process::id(), 97, created_nanos);
    assert!(
        stamped_schema_created_nanos(&schema_name)
            .is_some_and(|created| now_nanos.saturating_sub(created) >= min_age.as_nanos()),
        "precondition: the fixture name must look old enough to be an orphan, got {schema_name}"
    );
    let (_, template_name) = scratch_names("orphan_guard");
    // Park **before** creating: the orphan guard runs in every test process
    // (once per pool prep) and lists `parked_schemas` *before* it lists the
    // schemas to drop. Creating first leaves a window where a concurrent
    // guard snapshots the (still unlisted, hour-old-looking) name and drops
    // it — which is exactly how this fixture flaked in the full `--workspace
    // --lib` lane. Parking first closes the window: any guard that can see
    // the schema also sees it parked.
    // 时间戳刻意取**未来**（停靠与"先停靠后创建"是两件事，两者都不能省）：
    // `DB_SCHEMA_POOL_TTL` 是 120 s，而本用例在满载的覆盖率库里会跑 >60 s（按生产阈值
    // 遍历 ~100 条遗留 schema，每条 `DROP SCHEMA … CASCADE` ~1 s）。同一个测试 binary
    // 里任何一次 pool 准备都会跑 120 s TTL 的 reaper（`reap_expired_db_pool_entries`，
    // 见本文件 :470），于是这条"停靠行"可能在断言之前就被删掉、连带 schema 被 DROP ——
    // 断言 `the parked schema must survive the sweep` 报出来的其实是 reaper 干的，
    // 与 sweep 无关。让回收谓词（`parked_ts < now - ttl`）恒假即可；orphan guard 的
    // "跳过 parked"按**名字**判断、不看时间，语义不变。
    let parked_ts = current_timestamp_millis().saturating_add(3_600_000);
    park_schema_in_db_pool(&pool, &schema_name, &template_name, parked_ts).await.expect("park the scratch schema");
    create_scratch_schema(&pool, &schema_name).await;

    // 把「跑得慢」这件事变成**确定性断言**：本用例在满载的覆盖率库里会跑过 120 s
    // （CI 日志打了 `has been running for over 60 seconds`），而同一个 binary 的任何
    // 一次 pool 准备都会跑一次 `DB_SCHEMA_POOL_TTL`（120 s）的 reaper。
    // 这里**不真跑** reaper（那会连带回收并发用例的停靠行），只检查它用的谓词
    // `parked_ts < now - ttl` 对我们的行在"两分钟后的 now"下是否仍为假。
    let slow_run_now = current_timestamp_millis().saturating_add(121_000);
    let cutoff = slow_run_now.saturating_sub(i64::try_from(DB_SCHEMA_POOL_TTL.as_millis()).unwrap_or(i64::MAX));
    let survivors = pool_rows(&pool, &schema_name, Some(cutoff)).await;
    assert_eq!(
        survivors, 1,
        "a run lasting past DB_SCHEMA_POOL_TTL must not have its own park reaped (that is what killed this \
         fixture in CI: the reaper dropped the schema, and the sweep got the blame)"
    );

    let report =
        reclaim_orphan_test_schemas(&pool, now_nanos, min_age, usize::MAX).await.expect("sweep orphaned schemas");

    assert!(!report.dropped.contains(&schema_name), "the guard must never drop a parked schema");
    assert!(report.parked >= 1, "the parked schema must be counted as parked, got {report:?}");
    let remaining: Option<String> = sqlx::query_scalar("SELECT to_regnamespace($1)::text")
        .bind(&schema_name)
        .fetch_one(&pool)
        .await
        .expect("probe the parked schema");
    assert!(remaining.is_some(), "the parked schema must survive the sweep");

    let _ = drop_schema(&pool, &schema_name).await;
    pool.close().await;
}

/// 一次**真失败**的 `DROP DATABASE` 必须报 `Retry`，不能报 `Done`。
///
/// 为什么这条断言值钱：`Done` 会把该条目从 janitor 的注册表里摘掉，于是进程退出时
/// 那次**无条件** `WITH (FORCE)` 的兜底 drop 根本看不到它 —— 库就这么成了永久孤儿。
/// 一次性 CI runner 上"下一轮的 sweep 会收"这句话不成立（容器随 job 一起销毁），
/// 实测 run 40607555992 的 `46 leftover test database(s)` 就是这条语义的产物。
///
/// 不 mock sqlx：`template0` 是 PostgreSQL **拒绝**删除的库名
/// （`cannot drop a template database`），正好把 DDL 失败那条分支真的走一遍，
/// 而且这个操作按定义不可能成功，不存在"测试把库删掉了"的风险。
#[tokio::test]
async fn a_failed_drop_reports_retry_so_the_exit_drain_gets_another_shot() {
    if std::env::var("TEST_DATABASE_URL").is_err() {
        eprintln!(
            "SKIPPING a_failed_drop_reports_retry_so_the_exit_drain_gets_another_shot: \
         TEST_DATABASE_URL is unset, so this proves nothing."
        );
        return;
    }
    let database_url = crate::resolve_test_database_url().await.expect("database url");
    let outcome = synapse_common::test_database_guard::drop_database_if_unused_blocking(&database_url, "template0");
    assert_eq!(
        outcome,
        synapse_common::test_schema_guard::CleanupOutcome::Retry,
        "a refused DROP DATABASE must stay registered for the exit drain, not report Done"
    );
}
