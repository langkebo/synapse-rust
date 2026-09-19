#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Static guards for `scripts/cleanup_test_schemas.sh`.
//!
//! The script is the only byte-level remedy for test-schema accumulation, and it
//! shipped with four defects that made it simultaneously ineffective and unsafe
//! (see `docs/audit/P5_test_schema_accumulation_2026-09-12.md`):
//!
//! 1. matched only `test_%`, so `media_test_*` (1,033) and `synapse_test_*` (48)
//!    could never be cleaned;
//! 2. kept *every* `test_template%` unconditionally, so the 38 superseded
//!    fingerprint templates were immortal;
//! 3. defaulted to `localhost:15432/synapse_test` and never reported which
//!    database it actually reached, so in a differently-configured environment
//!    it connected somewhere else, found nothing, and printed success;
//! 4. discarded all `psql` stderr, so failures printed only `WARN`, and ran
//!    destructively with no dry-run.
//!
//! These are all properties of the script text, so they are guarded statically —
//! no database required.

use regex::Regex;
use std::fs;
use std::path::PathBuf;

fn script() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/cleanup_test_schemas.sh");
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

/// Read any repository file by path relative to the crate root.
fn repo_file(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

#[test]
fn cleanup_script_covers_every_leaked_schema_family() {
    let source = script();
    for pattern in ["test\\_%", "media\\_test\\_%", "synapse\\_test\\_%"] {
        assert!(
            source.contains(pattern),
            "cleanup script must match the `{pattern}` family; matching only `test_%` leaves \
             media_test_* and synapse_test_* to accumulate forever"
        );
    }
}

#[test]
fn cleanup_script_prunes_superseded_templates_but_spares_arbitrary_names() {
    let source = script();
    // The template family must be selected by an ANCHORED fingerprint regex, so
    // that a template configured under an arbitrary name (e.g.
    // `TEST_DB_TEMPLATE_SCHEMA=public`, or a production database) can never be a
    // candidate.
    // NOTE: the script writes this inside a double-quoted bash string, so the
    // end-anchor appears as `\$`. Match the anchor-free prefix and assert the
    // escape separately rather than embedding bash quoting rules here.
    assert!(
        source.contains("^test_template_v[0-9]+_[0-9a-f]{16}"),
        "superseded templates must be matched by the anchored fingerprint pattern, not by a \
         `test_template%` LIKE — an unconditional keep makes 38 stale templates immortal, while \
         an unanchored match risks dropping a hand-configured template"
    );
    assert!(
        source.contains(r"\$'") || source.contains(r"\\$"),
        "the fingerprint regex must be end-anchored so `test_template_v2_<hex>_extra` cannot match"
    );
    // The live template must come from the ready-marker directory.
    assert!(
        source.contains("synapse_test_templates"),
        "the live template set must be derived from the harness's ready-marker directory"
    );
    // Fail-safe: refusing to proceed with an empty keep-set is required.
    assert!(
        source.contains("找不到任何 live 模板标记"),
        "the script must abort when no live template marker can be found, rather than falling \
         back to dropping every template"
    );
}

/// The shared template family (`test_isolation_template_<16 hex>`, minted by
/// `synapse-common::test_isolation::template_schema_name`) was absent from the
/// script's template predicate. On 2026-09-13 a dry run listed the live
/// `test_isolation_template_bec240fb79ed438b` as a cleanup candidate in both
/// listing modes (`--keep-all-templates` and `--keep-template <live>`), so an
/// `--apply` would have dropped it. That cascades into every concurrent clone
/// because `LIKE ... INCLUDING ALL` copies serial columns' DEFAULT *expressions*,
/// which still name the template's sequences.
///
/// This guard is deliberately about the *shape* that made the bug possible:
///   * both fingerprint families must appear in the predicate (including the
///     `--keep-all-templates` branch, which used a `test\_template\_%` LIKE and
///     so missed `test_isolation_template_*` too), and
///   * the keep set must be applied as an exclusion (`NOT IN`). The original
///     `OR nspname IN ($KEEP_SQL)` inverted the polarity: an explicitly kept
///     live template became a candidate for DROP while a superseded template
///     outside the keep set was spared.
#[test]
fn cleanup_script_preserves_both_live_template_families() {
    let source = script();

    assert!(
        source.contains("^test_isolation_template_[0-9a-f]{16}"),
        "the cleanup script must know the shared template family \
         (`^test_isolation_template_[0-9a-f]{{16}}$`); without it the live shared template is a \
         cleanup candidate and `--apply` drops it"
    );
    // Both branches use it: the `--keep-all-templates` branch AND the marker-driven
    // branch. A single occurrence would leave one of the two listing modes unsafe.
    let family_mentions = source.matches("^test_isolation_template_[0-9a-f]{16}").count();
    assert!(
        family_mentions >= 2,
        "both template predicates (`--keep-all-templates` and the marker-driven keep set) must \
         exclude the shared family; found {family_mentions} occurrence(s)"
    );

    // Keep set polarity: `NOT IN` excludes kept templates from the candidate
    // set; `IN` would select them for deletion.
    assert!(
        source.contains("KEEP_EXCLUDE=\"AND nspname NOT IN ("),
        "the keep set must be applied as a `NOT IN (...)` hard exclusion so a template listed by \
         a ready-marker, `--keep-template`, `TEST_DB_TEMPLATE_SCHEMA` or the static list is \
         excluded from the candidate set"
    );
    assert!(
        source.contains("$STATIC_KEEP_SQL") && source.contains("${KEEP_SQL:+,$KEEP_SQL}"),
        "the hard exclusion must combine the unconditional static list with the dynamic keep set; \
         dropping either half re-opens a way to delete a live template"
    );
    assert!(
        !source.contains("nspname IN ($KEEP_SQL)"),
        "`nspname IN ($KEEP_SQL)` would make every *kept* template a DROP candidate while \
         sparing stale ones — the polarity of the preservation predicate"
    );
    // The old `--keep-all-templates` branch excluded only `test_template_*` via LIKE.
    assert!(
        !source.contains("nspname NOT LIKE 'test\\_template\\_%'"),
        "`--keep-all-templates` must exclude both fingerprint families, not just the old \
         `test_template_*` family"
    );
}

/// The `§9` hard exclusion must not depend on the keep set being non-empty.
///
/// Regression context (measured 2026-09-19): `KEEP_EXCLUDE` used to be assigned
/// **inside** `if [ -n "$KEEP_SQL" ]`. In the most common local path — no
/// ready-marker file (fresh `CARGO_TARGET_DIR`) and no `TEST_DB_TEMPLATE_SCHEMA`
/// (only CI exports it) — `KEEP_SQL` is empty, so the clause the script's own
/// comment calls the "HARD EXCLUSION" disappeared exactly where it was needed.
/// The dry run then reported `待清理: 1 个 schema / test_template_ci`: a real
/// `--apply` would have CASCADE-dropped the shared template that CI pins in
/// every test step. `test_template_ci` matches `test\_%` but neither fingerprint
/// family regex, so nothing else in the candidate predicate spares it.
///
/// Both halves of the fix are pinned here: the static list exists, and the
/// exclusion clause is built unconditionally from it (with a loud error if the
/// list is ever emptied), so the exclusion can never collapse to "" again.
#[test]
fn cleanup_script_hard_excludes_static_live_templates_without_markers() {
    let source = script();
    assert!(
        source.contains(r#"STATIC_KEEP=("test_template_ci")"#),
        "`test_template_ci` must stay in the unconditional static keep list as a BACKSTOP: \
         `scripts/ci/prepare_test_db.sh` now writes the ready-marker, so the marker mechanism \
         (#1) normally recognises it — but a database seeded before that change (or by a tool \
         that does not follow the marker convention) has no marker and no \
         `TEST_DB_TEMPLATE_SCHEMA`, and then a local `--apply` would CASCADE the CI template"
    );
    // The old, broken shape: an empty assignment that is only filled conditionally.
    assert!(
        !source.contains(r#"KEEP_EXCLUDE="""#),
        "`KEEP_EXCLUDE` must be assigned exactly once, unconditionally: an empty default filled \
         only when `KEEP_SQL` is non-empty is what allowed `test_template_ci` to become a \
         candidate in the no-marker/no-env path"
    );
    assert!(
        source.contains(r#"KEEP_EXCLUDE="AND nspname NOT IN ($STATIC_KEEP_SQL"#),
        "the hard exclusion must be built from `$STATIC_KEEP_SQL` first, so it is non-empty even \
         when the marker/env-derived keep set is empty"
    );
    assert!(
        source.contains(r#"[ -n "$STATIC_KEEP_SQL" ] ||"#),
        "an emptied static list must fail loudly instead of silently disabling the exclusion"
    );
}

#[test]
fn cleanup_script_is_dry_run_by_default_and_reports_its_target() {
    let source = script();
    // `APPLY` must default to 0 and the default branch must not drop anything.
    assert!(
        source.contains("APPLY=0\n") || source.contains("APPLY=0\r\n"),
        "the script must default to dry-run (APPLY=0); destructive-by-default is how a \
         wrongly-targeted run silently drops someone else's schemas"
    );
    assert!(source.contains("--apply"), "the destructive path must require an explicit --apply flag");
    // It must tell the operator which database it actually reached.
    assert!(
        source.contains("current_database()") && source.contains("inet_server_addr()"),
        "the script must print the database and server it actually connected to; the previous \
         version silently targeted localhost:15432/synapse_test"
    );
    // Connection must be overridable through the URLs the test harness uses.
    assert!(
        source.contains("TEST_DATABASE_URL") && source.contains("DATABASE_URL"),
        "connection resolution must honour DATABASE_URL / TEST_DATABASE_URL"
    );
}

#[test]
fn cleanup_script_preserves_drop_failure_reasons() {
    let source = script();
    // The DROP invocation must not send stderr to /dev/null.
    let drop_lines: Vec<&str> = source.lines().filter(|line| line.contains("DROP SCHEMA")).collect();
    assert!(!drop_lines.is_empty(), "script must contain a DROP SCHEMA statement");
    for line in drop_lines {
        assert!(
            !line.contains("2>/dev/null"),
            "DROP failures must keep their stderr; discarding it reduces every failure to a \
             bare WARN with no cause: {line}"
        );
    }
}

#[test]
fn cleanup_script_fails_fast_on_lock_exhaustion() {
    let source = script();
    // A schema with more objects than `max_locks_per_transaction` can NEVER be
    // dropped by `DROP SCHEMA ... CASCADE`: Postgres takes one lock per cascaded
    // object. Each attempt therefore costs ~12s and is guaranteed to fail.
    // Measured 2026-09-12: 25 template schemas at 1,197 objects each, all failing.
    //
    // The script must recognise this specific error and stop with an actionable
    // message rather than grinding through the whole candidate list.
    assert!(
        source.contains("out of shared memory"),
        "the script must detect `out of shared memory`; without this it retries a guaranteed \
         failure once per schema (hours of no-ops)"
    );
    assert!(
        source.contains("max_locks_per_transaction"),
        "the script must name the actual knob (`max_locks_per_transaction`) so the operator knows \
         what to raise"
    );
    assert!(
        source.contains("DROP DATABASE"),
        "the fail-fast message must point at the viable alternative (rebuild the test database), \
         since per-schema DROP can never succeed for over-large schemas"
    );
    // Consecutive-failure counter, not a one-shot: a single unlucky schema must
    // not abort an otherwise productive run.
    assert!(
        source.contains("LOCK_FAILURES"),
        "lock exhaustion must be tracked as consecutive failures, so one bad schema does not abort \
         a productive cleanup"
    );
}

/// Guard for iron rule #2 over the migrator.
///
/// `docker/deploy/scripts/container-migrate.sh` used to carry its own copy of the
/// migration engine (schema_migrations DDL, `is_migration_applied`,
/// `record_migration`, baseline selection, superseded-baseline skipping). That is a
/// second implementation of `docker/db_migrate.sh`: every fix — most recently
/// content-checksum drift detection — had to be written twice, and the deploy copy
/// was the one that decided what a real deployment actually applied. It must stay a
/// thin wrapper: resolve the container's env/paths, then exec the single
/// implementation with the subcommand passed through.
///
/// The guard is **structural**, not a string blacklist. Any re-implementation of
/// the engine has to touch the ledger table, issue SQL, or call a DB client —
/// no matter how the statement is spelled (`CREATE TABLE`, `CREATE TABLE IF NOT
/// EXISTS`, `ALTER TABLE ... ADD COLUMN`, ...). It also caps the file length: the
/// wrapper was 484 lines when it still carried the engine, is 66 now, and a real
/// second implementation cannot fit in 80 lines.
///
/// Shell comments are stripped before the SQL/client checks. The wrapper
/// legitimately mentions `psql` in a comment (the host-side H-14 guard explains
/// it) and exports `SYNAPSE_DB_MIGRATE_ALLOW_HOST_PSQL`, so a whole-file
/// `contains("psql")` would be a false positive.
#[test]
fn deploy_migrator_delegates_to_the_single_implementation() {
    let deploy = repo_file("docker/deploy/scripts/container-migrate.sh");

    // (1) It must delegate to the single implementation and pass the subcommand
    // through. The surface must not shrink to a hardcoded `migrate`: `validate`
    // and `status` (and `docker/db_migrate.sh`'s `init`) have to pass through.
    assert!(
        deploy.contains("docker/db_migrate.sh"),
        "部署迁移器必须委托给 docker/db_migrate.sh（唯一实现），否则每次修复都要写两遍"
    );
    assert!(
        deploy.contains("\"$@\""),
        "wrapper 必须把子命令透传给唯一实现（\"$@\"），否则 validate/status 入口静默消失"
    );

    // Everything below looks at **code**, not comments.
    let code = strip_shell_comments(&deploy);

    // (2) No DB client may be invoked as a command.
    for client in ["psql", "pg_dump", "createdb"] {
        assert!(
            !contains_command(&code, client),
            "container-migrate.sh 的代码里不得调用 DB 客户端 `{client}`：\
             一旦它自己连库执行 SQL，就不再是薄包装（唯一实现是 docker/db_migrate.sh）"
        );
    }

    // (3) No DDL/DML statement shape. The regex requires the verb to be
    //     *immediately* followed by a known keyword, so it catches
    //     `CREATE TABLE`, `ALTER TABLE`, `DROP INDEX`, `INSERT INTO`,
    //     `UPDATE <table> SET` (adjacent only), `DELETE FROM` and
    //     `SELECT ... FROM` — and it is case-insensitive, so a lowercase
    //     rewrite cannot slip past.
    //
    //     Honest limits (measured, not assumed): `CREATE OR REPLACE FUNCTION`
    //     is *not* caught (FUNCTION is not in the keyword list), `select * from t`
    //     is *not* caught (the verb is not immediately followed by a keyword),
    //     and a client invoked through a variable (`cmd=$PSQL; "$cmd" -f x.sql`)
    //     is *not* caught. What still contains those cases is the `<= 80` line
    //     cap plus the ban on the `schema_migrations` literal and on any DB
    //     client name appearing in code: re-growing a migration engine trips
    //     those even when this regex misses. This is a speed bump, not a proof.
    let ddl = Regex::new(
        r"(?i)\b(CREATE|ALTER|DROP|INSERT|UPDATE|DELETE|SELECT)\s+(TABLE|INDEX|CONSTRAINT|COLUMN|INTO|FROM|SET)\b",
    )
    .expect("the DDL/DML regex must compile");
    assert!(
        !ddl.is_match(&code),
        "container-migrate.sh 的代码里不得出现 DDL/DML 语句形态（命中 {:?}）：\
         wrapper 只做委托，SQL 只在 docker/db_migrate.sh",
        ddl.find(&code).map(|matched| matched.as_str())
    );

    // (4) The migration ledger is owned exclusively by the single implementation.
    assert!(
        !code.contains("schema_migrations"),
        "container-migrate.sh 的代码里不得出现 `schema_migrations`：\
         迁移台账由 docker/db_migrate.sh 独占读写"
    );

    // (5) Size cap. The engine alone was ~420 lines; 80 leaves room for the
    //     env/path bridge and nothing else.
    let line_count = deploy.lines().count();
    assert!(
        line_count <= 80,
        "container-migrate.sh 必须保持薄包装（<= 80 行），实际 {line_count} 行：\
         长出来的那部分几乎一定是第二份实现"
    );

    // Delegation is only real if the migrator container can actually reach the
    // implementation. `postgres:16-alpine` does not ship the repo, so the deploy
    // compose must bind-mount it at exactly the path the wrapper execs — a wrapper
    // that execs a path the container never receives fails with exit 2 at deploy
    // time. Pin the literal: mentioning `db_migrate.sh` anywhere is not proof the
    // mount lands on `/scripts/db_migrate.sh`.
    let compose = repo_file("docker/deploy/docker-compose.yml");
    assert!(
        compose.contains("../db_migrate.sh:/scripts/db_migrate.sh"),
        "docker/deploy/docker-compose.yml 的 migrator 服务必须把 docker/db_migrate.sh \
         挂载到 `/scripts/db_migrate.sh`（wrapper exec 的确切路径）；\
         仅仅出现 `db_migrate.sh` 字样不算数"
    );
}

/// Drop whole-line shell comments, so the structural checks above look at code
/// only.
///
/// The wrapper legitimately names `psql` and `schema_migrations` in comments
/// (they explain what it must *not* do) and exports
/// `SYNAPSE_DB_MIGRATE_ALLOW_HOST_PSQL`; none of that is a second
/// implementation.
fn strip_shell_comments(source: &str) -> String {
    source.lines().filter(|line| !line.trim_start().starts_with('#')).collect::<Vec<_>>().join("\n")
}

/// True when `word` appears as a standalone token — not embedded in a longer
/// identifier such as `SYNAPSE_DB_MIGRATE_ALLOW_HOST_PSQL` (uppercase, so it
/// would not match anyway) or `pg_isready`.
fn contains_command(source: &str, word: &str) -> bool {
    let is_boundary = |character: Option<char>| {
        !character.is_some_and(|character| character.is_alphanumeric() || character == '_' || character == '-')
    };
    source.match_indices(word).any(|(start, _)| {
        is_boundary(source[..start].chars().next_back()) && is_boundary(source[start + word.len()..].chars().next())
    })
}
