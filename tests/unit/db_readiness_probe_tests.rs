//! Guards for **database readiness probing** in both compose stacks.
//!
//! ## The defect these guard against (measured 2026-09-19)
//!
//! The official `postgres` image entrypoint does its initdb work against a
//! **temporary server that only listens on the unix socket**
//! (`-c listen_addresses=''`). A healthcheck of the form
//!
//! ```text
//! pg_isready -U <user> -d <db>          # no -h → unix socket
//! ```
//!
//! therefore reports `accepting connections` (exit 0) **during initialisation**,
//! so the container is marked `healthy` long before the real server is reachable
//! over TCP. `pg_isready`'s exit code deliberately does not validate the
//! user/database/password either.
//!
//! Consequences in this repo:
//! * deploy stack — `depends_on: condition: service_healthy` **and**
//!   `deploy.sh`'s `wait_for_container_health synapse-postgres` both pass early,
//!   so the migrator starts against a database that cannot accept it, and its
//!   only retry window is `retry 3 5` (~10s);
//! * dev/CI stack — `synapse-rust`'s entrypoint migration and the
//!   `backend-validation` workflow start early for the same reason.
//!
//! Reproduced with a throwaway `postgres:16-alpine` whose init script simply
//! `SELECT pg_sleep(30)`: the bare probe returned `rc=0` for 24+ consecutive
//! seconds while a TCP probe failed every time, and both agreed only after the
//! real server came up. See `docs/audit/DB_REVIEW_2026-09-17.md` §15.9.
//!
//! The fix is to make "healthy" mean what the migrator actually needs: the
//! target user/database can be **reached over TCP and queried**. An empty
//! healthcheck, a `-h`-less one, or one without the query must fail these guards.

use std::fs;
use std::path::PathBuf;

fn repo_file(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

/// Extract a top-level service block (`  <name>:`) from a compose file.
fn compose_service(compose: &str, service: &str) -> String {
    let header = format!("  {service}:");
    let start = compose
        .split_inclusive('\n')
        .scan(0usize, |offset, line| {
            let at = *offset;
            *offset += line.len();
            Some((at, line))
        })
        .find(|(_, line)| *line == format!("{header}\n"))
        .map_or_else(|| panic!("compose file has no `{header}` service"), |(at, _)| at);
    let rest = &compose[start..];
    let mut end = rest.len();
    for (at, line) in rest.split_inclusive('\n').scan(0usize, |offset, line| {
        let at = *offset;
        *offset += line.len();
        Some((at, line))
    }) {
        // Skip the header itself; the next `  <name>:` at the same indent ends the block.
        if at == 0 {
            continue;
        }
        let trimmed = line.trim_end();
        if trimmed.len() > 3 && !trimmed.starts_with("   ") && trimmed.ends_with(':') && !trimmed.starts_with('#') {
            end = at;
            break;
        }
    }
    rest[..end].to_string()
}

/// The probe line must be TCP-based and must actually query the target database.
///
/// Comments are stripped **before** counting: the explanatory comments in these
/// compose files legitimately mention `pg_isready` while describing this very
/// bug, and a naive whole-block scan counts them as extra probes (found the hard
/// way — the first version of this guard failed on its own documentation).
fn assert_probe_is_real(compose_path: &str, service: &str) {
    let compose = repo_file(compose_path);
    let block = compose_service(&compose, service);
    let probe_lines: Vec<&str> = block
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .filter(|line| line.contains("pg_isready"))
        .collect();
    assert_eq!(
        probe_lines.len(),
        1,
        "{compose_path}: expected exactly one non-comment `pg_isready` probe in the `{service}` \
         service (0 means the service has no readiness probe at all), found {probe_lines:?}"
    );
    let probe = probe_lines[0];
    assert!(
        probe.contains("-h "),
        "{compose_path}: the `{service}` healthcheck must pass `-h` so it probes TCP. Without it \
         `pg_isready` talks to the initdb **unix-socket-only** temporary server and reports \
         `accepting connections` while the database is still initialising: {probe}"
    );
    assert!(
        probe.contains("psql"),
        "{compose_path}: the `{service}` healthcheck must additionally run a real authenticated \
         query (`psql … -tAc 'SELECT 1'`). `pg_isready`'s exit code does not validate the \
         user/database/password, so it cannot mean \"the migrator can connect\": {probe}"
    );
    assert!(
        probe.contains("SELECT 1"),
        "{compose_path}: the `{service}` healthcheck's psql invocation must run a query \
         (`SELECT 1`), not merely start a client: {probe}"
    );
}

#[test]
fn deploy_postgres_healthcheck_probes_tcp_and_queries() {
    assert_probe_is_real("docker/deploy/docker-compose.yml", "postgres");
}

#[test]
fn dev_postgres_healthcheck_probes_tcp_and_queries() {
    assert_probe_is_real("docker/docker-compose.yml", "db");
}

/// `compose run` ignores restart policies, so this does not change the standard
/// `deploy.sh` path (which has its own `retry 3 5`). It protects the plain
/// `docker compose up` path: with `"no"`, one transient failure leaves the stack
/// running with an unmigrated schema.
#[test]
fn migrator_retries_transient_failures() {
    let compose = repo_file("docker/deploy/docker-compose.yml");
    let block = compose_service(&compose, "migrator");
    assert!(
        block.contains(r#"restart: "on-failure:3""#),
        "the migrator must retry a transient failure (`restart: \"on-failure:3\"`) instead of \
         exiting for good; a wrong-database or wrong-schema failure still stops it after 3 tries. \
         Current block:\n{block}"
    );
}

/// `README.md` states there is no second migration entry point, and the migrator
/// guard in `cleanup_schema_script_tests.rs` enforces delegation to
/// `docker/db_migrate.sh`. A leftover `docker/deploy/scripts/migrate.sh` was a
/// third, unreferenced entry point that also carried the unix-socket probe bug —
/// patching dead code would have kept both problems. Guard the invariant, not
/// just the deletion.
#[test]
fn no_second_migration_entry_point_exists() {
    let dead = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker/deploy/scripts/migrate.sh");
    assert!(
        !dead.exists(),
        "{} must not exist: `README.md` states the only migration entry point is \
         `docker/db_migrate.sh` and nothing referenced this script",
        dead.display()
    );
}
