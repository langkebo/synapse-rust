//! Database-level test isolation (`TEST_DB_ISOLATION=database`).
//!
//! Schema cloning pays 221 `CREATE TABLE LIKE`s plus index/constraint renames
//! on every test (measured 1.2–3.5 s). This module clones a whole **database**
//! from a prepared template instead: `CREATE DATABASE … TEMPLATE` copies the
//! files and touches no DDL (measured 0.117 s warm, 0.29–0.44 s cold), so the
//! clone is structurally identical by construction — nothing to rename, no
//! foreign keys or views to retarget, no sequences to advance.
//!
//! Opt-in and off by default:
//!
//! * `TEST_DB_ISOLATION=database` — use this path in `prepare_shared_test_pool`.
//! * `TEST_DB_TEMPLATE_DATABASE` — pin the template database by name.
//!
//! **The template is provisioned outside the test process**, once, by
//! `scripts/test_db_template.sh ensure`. Building it here was tried first and
//! removed: `CREATE DATABASE` commits before the migrations run, so "the row
//! exists" is not "it is usable"; with four test processes racing, one rebuilt
//! the template while another cloned it (measured 2026-10-20: 6.4 s per test
//! instead of 0.3 s, and a half-built template left behind). A sealed template
//! (`datallowconn = false`) is the readiness marker, and it also guarantees
//! `CREATE DATABASE … TEMPLATE` can never fail with "being accessed".
//!
//! Cleanup goes through [`synapse_common::test_database_guard`]: the registry,
//! janitor and atexit drain are the same ones schema isolation uses, only the
//! action differs (`DROP DATABASE` + `pg_stat_activity` instead of a lease).

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::sync::Arc;

/// True when the operator asked for database-level isolation.
pub fn enabled() -> bool {
    std::env::var("TEST_DB_ISOLATION").map(|value| value.trim().eq_ignore_ascii_case("database")).unwrap_or(false)
}

/// Acquire a pool whose database is a fresh clone of the template.
///
/// `base_url` is the test database URL; only its database component is replaced.
pub async fn prepare_pool(base_url: &str) -> Result<Arc<PgPool>, String> {
    let template_database = resolve_template_database(base_url).await?;

    let database = crate::next_test_schema_name();
    create_database_from_template(base_url, &template_database, &database).await?;

    let url = synapse_common::test_database_guard::with_database_name(base_url, &database);
    let pool = match connect_plain(&url).await {
        Ok(pool) => pool,
        Err(error) => {
            // Do not leave the clone behind when the pool cannot be built.
            synapse_common::test_database_guard::drop_database_blocking(base_url, &database);
            return Err(error);
        }
    };
    let cleanup = synapse_common::test_database_guard::database_cleanup(base_url, &database);
    synapse_common::test_schema_guard::register_schema_cleanup(&pool, &database, cleanup);
    Ok(pool)
}

/// Expected template database name for the current sources.
///
/// `synapse_tmpl_v<REVISION>_<sha256 前 16 位>`, where the hash covers every
/// `migrations/*.sql` **and** `test-support/test_schema_contract.sql`, in sorted
/// path order — exactly what `scripts/test_db_template.sh` hashes. One
/// derivation, two consumers: a migration or contract change renames the
/// template, so a stale template cannot be picked up silently.
pub fn expected_template_database() -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let root = std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
    let migrations = root.join("migrations");
    let contract = root.join("test-support/test_schema_contract.sql");
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&migrations)
        .map_err(|error| format!("could not read {}: {error}", migrations.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "sql"))
        .collect();
    files.sort();
    files.push(contract);
    let mut hasher = Sha256::new();
    for file in files {
        let bytes = std::fs::read(&file).map_err(|error| format!("could not read {}: {error}", file.display()))?;
        hasher.update(&bytes);
    }
    let digest = hasher.finalize();
    let hex: String = digest.iter().take(8).map(|byte| format!("{byte:02x}")).collect();
    Ok(format!("synapse_tmpl_v{}_{hex}", crate::TEST_TEMPLATE_SCHEMA_REVISION))
}

/// Pick the template database to clone.
///
/// The name is [`expected_template_database`] unless the operator pins one with
/// `TEST_DB_TEMPLATE_DATABASE`. Existence **and** `datallowconn = false` are
/// both required: the flag is the "fully built" marker, and it also guarantees
/// `CREATE DATABASE … TEMPLATE` cannot fail with "being accessed".
async fn resolve_template_database(base_url: &str) -> Result<String, String> {
    let expected = expected_template_database()?;
    let wanted = match std::env::var("TEST_DB_TEMPLATE_DATABASE") {
        Ok(pinned) if !pinned.trim().is_empty() => pinned.trim().to_string(),
        _ => expected.clone(),
    };
    let maintenance_url = synapse_common::test_database_guard::with_database_name(base_url, "postgres");
    let admin = connect_admin(&maintenance_url).await?;
    let ready: Option<bool> = sqlx::query_scalar("SELECT NOT datallowconn FROM pg_database WHERE datname = $1")
        .bind(&wanted)
        .fetch_optional(&*admin)
        .await
        .map_err(|error| format!("could not look up template database {wanted}: {error}"))?;
    admin.close().await;
    match ready {
        Some(true) => Ok(wanted),
        Some(false) => Err(format!(
            "template database {wanted} exists but is not sealed (datallowconn = true); \
             it is half-built or being rebuilt. Re-run: bash scripts/test_db_template.sh ensure"
        )),
        None => Err(format!(
            "template database {expected} is missing. Create it once with:\n  \
             bash scripts/test_db_template.sh ensure"
        )),
    }
}

/// `CREATE DATABASE "<database>" TEMPLATE "<template>"`, serialized by an
/// advisory lock: PostgreSQL holds an exclusive lock on the template while
/// copying, and concurrent creators would otherwise collide.
async fn create_database_from_template(base_url: &str, template_database: &str, database: &str) -> Result<(), String> {
    let maintenance_url = synapse_common::test_database_guard::with_database_name(base_url, "postgres");
    let admin = connect_admin(&maintenance_url).await?;
    let mut conn = admin.acquire().await.map_err(|error| format!("clone lock: {error}"))?;
    sqlx::query("SELECT pg_advisory_lock(hashtext($1))")
        .bind(template_database)
        .execute(&mut *conn)
        .await
        .map_err(|error| format!("clone lock: {error}"))?;
    let quoted_db = database.replace('"', "\"\"");
    let quoted_tmpl = template_database.replace('"', "\"\"");
    let sql = format!(r#"CREATE DATABASE "{quoted_db}" TEMPLATE "{quoted_tmpl}""#);
    let result = sqlx::query(&sql).execute(&mut *conn).await;
    sqlx::query("SELECT pg_advisory_unlock(hashtext($1))").bind(template_database).execute(&mut *conn).await.ok();
    drop(conn);
    admin.close().await;
    result.map(|_| ()).map_err(|error| format!("could not clone {template_database} into {database}: {error}"))
}

async fn connect_admin(url: &str) -> Result<Arc<PgPool>, String> {
    tokio::time::timeout(
        crate::configured_test_pool_connect_timeout(),
        PgPoolOptions::new().max_connections(1).acquire_timeout(std::time::Duration::from_secs(30)).connect(url),
    )
    .await
    .map_err(|_| format!("timed out connecting to {url}"))?
    .map(Arc::new)
    .map_err(|error| format!("could not connect to {url}: {error}"))
}

/// Pool for a cloned database: no `search_path` override and no connection
/// lease — the database itself is the isolation unit.
async fn connect_plain(url: &str) -> Result<Arc<PgPool>, String> {
    tokio::time::timeout(
        crate::configured_test_pool_connect_timeout(),
        PgPoolOptions::new()
            .max_connections(crate::configured_test_pool_max_connections())
            .min_connections(crate::configured_test_pool_min_connections())
            .acquire_timeout(crate::configured_test_pool_acquire_timeout())
            .idle_timeout(Some(crate::configured_test_pool_idle_timeout()))
            .max_lifetime(Some(crate::configured_test_pool_max_lifetime()))
            .connect(url),
    )
    .await
    .map_err(|_| format!("timed out connecting to {url}"))?
    .map(Arc::new)
    .map_err(|error| format!("could not connect to {url}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The name is a published contract with `scripts/test_db_template.sh`:
    /// `synapse_tmpl_v<REVISION>_<16 hex chars>`. A drift here would surface as
    /// "template database … is missing", which is loud, but pinning the shape
    /// keeps the two derivations comparable when someone edits either side.
    #[test]
    fn template_name_shape_is_stable() {
        let name = expected_template_database().expect("fingerprint");
        let prefix = format!("synapse_tmpl_v{}_", crate::TEST_TEMPLATE_SCHEMA_REVISION);
        let suffix = name.strip_prefix(&prefix).expect("revision prefix");
        assert_eq!(suffix.len(), 16, "{name} must carry 16 hex characters");
        assert!(suffix.chars().all(|c| c.is_ascii_hexdigit()), "{name} must be hex");
        // Stable within a process: the hash reads files, it must not depend on
        // iteration order.
        assert_eq!(name, expected_template_database().expect("fingerprint"));
    }
}
