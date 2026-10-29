//! Prune-focused regressions, split out of `test_isolation.rs`.
//!
//! Why a separate file: `check_file_size_ratchet.py` freezes the parent file's
//! line count, so adding regressions there would either grow past that ceiling or
//! force a baseline rewrite (`--update`), which this workspace forbids. Living
//! under `.../tests/` also makes `check_doc_metrics.py` classify this file as a
//! test file, so its best-effort cleanup lines stay out of the production counts.

use super::super::{
    ensure_template_schema, is_vanished_candidate, prune_stale_isolation_templates, template_schema_name,
    TEMPLATE_READY_TABLE,
};
use super::test_database_url;
use sqlx::postgres::PgPoolOptions;

/// [`prune_isolation_templates`] treats a marker table with zero rows
/// (legacy template) as "potentially in use", backfills a fresh timestamp,
/// and spares it — preventing accidental deletion of a template another
/// session may still be cloning from.
///
/// The zero-row template must be a **candidate**: prune deliberately skips the
/// `keep` schema, so the backfill only ever happens for the *other* templates.
/// This test used to delete the marker rows of `keep` itself and assert they came
/// back, which prune cannot do — it passed only when another test happened to
/// rebuild that same baseline concurrently, and failed deterministically otherwise.
#[tokio::test]
async fn prune_backfills_legacy_zero_row_marker_and_spares_it() {
    let Some(url) = test_database_url() else {
        return;
    };
    let baseline = "CREATE TABLE IF NOT EXISTS unify_prune_test (id bigint PRIMARY KEY);";
    let template = template_schema_name(baseline);
    let admin = PgPoolOptions::new().max_connections(1).connect(&url).await.expect("admin pool");

    // A complete template, which prune is told to keep.
    let _ = ensure_template_schema(&url, baseline).await.expect("build template");
    let has_rows: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM \"{template}\".\"{TEMPLATE_READY_TABLE}\""))
        .fetch_one(&admin)
        .await
        .expect("count marker rows after build");
    assert!(has_rows > 0, "a freshly built template must have at least one marker row, got {has_rows}");

    // A legacy candidate: the schema naming pattern prune looks for, with an
    // existing but empty marker table (`built_at` is the only column).
    let legacy = format!("test_isolation_template_{:016x}", 0xfeed_face_dead_beefu64);
    sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{legacy}" CASCADE"#)).execute(&admin).await.expect("cleanup");
    sqlx::query(&format!(r#"CREATE SCHEMA "{legacy}""#)).execute(&admin).await.expect("create legacy schema");
    sqlx::query(&format!(
        r#"CREATE TABLE "{legacy}"."{TEMPLATE_READY_TABLE}" (built_at timestamptz NOT NULL DEFAULT now())"#
    ))
    .execute(&admin)
    .await
    .expect("create legacy marker table");

    let dropped = prune_stale_isolation_templates(&admin, &template).await.expect("prune");
    assert!(!dropped.iter().any(|s| s == &legacy), "the zero-row legacy candidate must be spared, got {dropped:?}");
    assert!(!dropped.iter().any(|s| s == &template), "the protected template must never be dropped");

    // The legacy candidate must have been backfilled (starting its age clock).
    let backfilled: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM \"{legacy}\".\"{TEMPLATE_READY_TABLE}\""))
        .fetch_one(&admin)
        .await
        .expect("count marker rows after prune");
    assert!(backfilled > 0, "prune must have backfilled a marker row, got {backfilled}");

    // Cleanup
    let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{legacy}" CASCADE"#)).execute(&admin).await;
    let _ = sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{template}" CASCADE"#)).execute(&admin).await;
}

/// 候选在探测中途消失（42P01）时，prune 必须把它当成"已经没了"跳过，不得返回 Err。
///
/// 为什么这条值得单独钉：CI 上 `prune_backfills_legacy_zero_row_marker_and_spares_it`
/// 偶发 panic 在 `expect("prune")`，报错是
/// `failed to read marker age for test_isolation_template_49016b12955b87c3: … relation
/// "…_synapse_test_template_ready" does not exist`。那个 schema 属于
/// `validate_clone_rejects_an_incomplete_clone`，它清理时**不持** advisory lock；
/// 而 prune 的探测是三条独立语句，并发清理可以插进任意两条之间。
#[tokio::test]
async fn vanished_candidate_is_skipped_not_a_prune_failure() {
    let Some(url) = test_database_url() else {
        return;
    };
    let admin = PgPoolOptions::new().max_connections(1).connect(&url).await.expect("admin pool");

    // 一个"看起来像模板"的候选：marker 表零行 ⇒ prune 只会回填并豁免，
    // 所以并发 prune 不会把它删掉，下面的 DROP 完全由本测试控制。
    //
    // ⚠️ schema 与 marker 表必须**在同一个事务里**建（2026-10-29 实测两次 CI 红）：
    // `prune_isolation_templates` 对候选的判定是
    //   ① 列 `pg_namespace` 得到候选 → ② `to_regclass(marker)` 为 NULL ⇒ `eligible_to_drop = true` → ③ `DROP SCHEMA`
    // ①②③ 是三条独立语句。若本测试分两次 DDL 建（① 见 schema、② 见空），中间那段
    // **"schema 已存在但没有 marker 表"的窗口**会被并发 prune 判成"不完整构建，可删"，
    // 于是：窗口内被删 ⇒ 下面 `CREATE TABLE` 报 schema 不存在；
    // 或 ② 读到 NULL 而 ③ 的 DROP 落在 `CREATE TABLE` 之后 ⇒ 第 104 行探针或第 106 行 DROP 撞 3F000。
    // 两种交错在 CI 都出现过（`prune_tests.rs:104` 与 `:106`）。
    // PostgreSQL 的 DDL 是事务性的 ⇒ 放进一个事务后，schema 与 marker 表**同时可见**，
    // 并发 prune 要么看不到候选、要么看到 marker 表（零行 ⇒ 回填并豁免），窗口消失。
    let schema = format!("test_isolation_template_{:016x}", 0x0bad_c0de_0bad_c0deu64);
    let marker = format!("{schema}.{TEMPLATE_READY_TABLE}");
    sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{schema}" CASCADE"#)).execute(&admin).await.expect("cleanup");
    {
        let mut tx = admin.begin().await.expect("begin create transaction");
        sqlx::query(&format!(r#"CREATE SCHEMA "{schema}""#)).execute(&mut *tx).await.expect("create schema");
        sqlx::query(&format!(
            r#"CREATE TABLE "{schema}"."{TEMPLATE_READY_TABLE}" (built_at timestamptz NOT NULL DEFAULT now())"#
        ))
        .execute(&mut *tx)
        .await
        .expect("create marker table");
        tx.commit().await.expect("commit create transaction");
    }

    let table_exists: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
        .bind(&marker)
        .fetch_one(&admin)
        .await
        .expect("probe 1");
    assert!(table_exists, "the first probe must still see the freshly created marker table");

    sqlx::query(&format!(r#"DROP SCHEMA "{schema}" CASCADE"#)).execute(&admin).await.expect("drop candidate");

    // 与 prune 的第二、第三条探测逐字相同的语句。
    let count_error = sqlx::query_scalar::<_, i64>(&format!("SELECT count(*) FROM {marker}"))
        .fetch_one(&admin)
        .await
        .expect_err("probing a vanished candidate must fail at the SQL level");
    assert!(is_vanished_candidate(&count_error), "expected SQLSTATE 42P01, got: {count_error}");

    let age_error = sqlx::query_scalar::<_, Option<i64>>(&format!(
        "SELECT EXTRACT(EPOCH FROM age(now(), max(built_at)))::BIGINT FROM {marker}"
    ))
    .fetch_one(&admin)
    .await
    .expect_err("the CI failure came from exactly this statement");
    assert!(is_vanished_candidate(&age_error), "expected SQLSTATE 42P01, got: {age_error}");

    // 反例：无关的 SQL 错误不得被误判成"候选已消失"，否则 prune 会静默漏删。
    let other = sqlx::query("SELECT 1 / 0").execute(&admin).await.expect_err("division by zero");
    assert!(!is_vanished_candidate(&other), "42P01 must not swallow unrelated errors: {other}");
}
