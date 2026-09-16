#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Test-schema housekeeping: the harness creates one isolated schema per test
//! run and, historically, never removed them. A long-lived local test database
//! reached **23,662** leftover schemas on 2026-09-12 (`test_*` 22,543,
//! `media_test_*` 1,033, `synapse_test_*` 48, stale `test_template_v2_*` 38),
//! which bloats the catalog badly enough that even `pg_database_size()` times
//! out and clone/setup queries slow down.
//!
//! Two independent leaks, two fixes:
//! * **Template schemas** — `default_template_schema_name()` embeds a
//!   fingerprint of every migration file, so *every migration edit mints a new
//!   template name*. Nothing deleted the old one. Fixed in Rust by
//!   [`prune_stale_template_schemas`], called at the end of
//!   `init_template_schema`; tested here.
//! * **Per-test schemas** (`test_*`, `media_test_*`) — created fresh by suites
//!   that hand-roll their own pool. Byte-level cleanup is
//!   `scripts/cleanup_test_schemas.sh`; the structural fix (route those suites
//!   through the shared harness) is tracked separately.

use sqlx::PgPool;

async fn connect_admin() -> Option<PgPool> {
    let url = std::env::var("TEST_DATABASE_URL").or_else(|_| std::env::var("DATABASE_URL")).ok()?;
    sqlx::postgres::PgPoolOptions::new().max_connections(1).connect(&url).await.ok()
}

fn schema_name(tag: &str) -> String {
    // Must match the anchored pattern `^test_template_v[0-9]+_[0-9a-f]{16}$`
    // for the stale ones, so use a fixed-width lowercase-hex suffix.
    format!("test_template_v2_{tag:0>16}")
}

async fn drop_schema_quietly(pool: &PgPool, name: &str) {
    let _ = sqlx::query(&format!("DROP SCHEMA IF EXISTS \"{name}\" CASCADE")).execute(pool).await;
}

#[tokio::test]
async fn prune_drops_superseded_templates_and_keeps_the_current_one() {
    let Some(pool) = connect_admin().await else {
        eprintln!("skipping: no TEST_DATABASE_URL/DATABASE_URL reachable");
        return;
    };

    let keep = schema_name("a4f2c0ffee000001");
    let stale_a = schema_name("a4f2c0ffee00dead");
    let stale_b = schema_name("a4f2c0ffee00beef");
    // Deliberately NOT matching the anchored pattern — must survive.
    let configured = format!("a4f2_configured_template_{}", std::process::id());

    for name in [&keep, &stale_a, &stale_b, &configured] {
        drop_schema_quietly(&pool, name).await;
        sqlx::query(&format!("CREATE SCHEMA \"{name}\"")).execute(&pool).await.unwrap();
    }

    let dropped = synapse_test_utils::prune_stale_template_schemas(&pool, &keep).await.expect("prune must succeed");

    assert!(dropped.contains(&stale_a), "stale template {stale_a} must be dropped; got {dropped:?}");
    assert!(dropped.contains(&stale_b), "stale template {stale_b} must be dropped; got {dropped:?}");

    let remaining: Vec<String> =
        sqlx::query_scalar("SELECT nspname FROM pg_namespace WHERE nspname = ANY($1) ORDER BY nspname")
            .bind(vec![keep.clone(), stale_a.clone(), stale_b.clone(), configured.clone()])
            .fetch_all(&pool)
            .await
            .unwrap();

    assert!(remaining.contains(&keep), "the current template must be preserved");
    assert!(remaining.contains(&configured), "a non-fingerprint template name must be preserved");
    assert!(!remaining.contains(&stale_a), "stale template {stale_a} must be gone");
    assert!(!remaining.contains(&stale_b), "stale template {stale_b} must be gone");

    // Idempotent: nothing left to prune.
    let again = synapse_test_utils::prune_stale_template_schemas(&pool, &keep).await.unwrap();
    assert!(!again.contains(&stale_a) && !again.contains(&stale_b), "second prune must be a no-op");

    for name in [&keep, &stale_a, &stale_b, &configured] {
        drop_schema_quietly(&pool, name).await;
    }
}

#[tokio::test]
async fn prune_refuses_when_replacement_template_is_missing() {
    let Some(pool) = connect_admin().await else {
        eprintln!("skipping: no TEST_DATABASE_URL/DATABASE_URL reachable");
        return;
    };

    let absent = schema_name("a4f2absent0000001");
    drop_schema_quietly(&pool, &absent).await;

    // A failed template build must never leave the database with no template at
    // all, so pruning has to bail out when the replacement does not exist.
    let result = synapse_test_utils::prune_stale_template_schemas(&pool, &absent).await;
    assert!(result.is_err(), "prune must refuse when the keep-template does not exist");
}
