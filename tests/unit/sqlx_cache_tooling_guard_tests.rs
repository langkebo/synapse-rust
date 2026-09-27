//! Guards for the three scripts the 2026-09-26 `.sqlx`/`public` 事故 touched
//! (`docs/audit/SQLX_STATICIZATION_PLAN_2026-09-23.md` §7.1 D-75/D-76/D-77).
//!
//! ## Why this file exists
//!
//! `synapse_test.public` was found with **0 tables** (it must hold the ~220-table
//! baseline; `prepare_test_db.sh`'s own `[4/4]` step asserts `>= 200`). Two
//! independent tooling hazards produce exactly that state, and both were silent:
//!
//! * `converge_public_schema.sh` evaluated its delete list **twice** — the second
//!   time inside the `apply` heredoc — while `prepare_test_db.sh`'s `[2/4]` step
//!   `DROP SCHEMA test_template_ci CASCADE`s the reference schema and spends tens
//!   of seconds rebuilding it. During that window the reference is empty, so
//!   *every* `public` object looks "not in the baseline". The post-apply invariant
//!   re-evaluated the **same collapsed reference**, so `extra=0 / missing=0` held
//!   while `public` had just been emptied (D-75).
//! * `init_test_public_schema.sh` defaulted `RESET_PUBLIC=1`, so running it bare
//!   did `DROP SCHEMA public CASCADE` on the shared test database; a failure or
//!   Ctrl-C before the first migration leaves 0 tables (D-76).
//!
//! Separately, `check_sqlx_cache_fresh.sh --full` against that empty schema is not
//! "cache is stale" but **1443 misleading `E0282`/`E0277` errors** (measured), and
//! a bare `cargo sqlx prepare` would have wiped `.sqlx/` (its destination is
//! `.sqlx/` itself, cleared before rewriting) — D-77.
//!
//! Every test below **executes the production script** in a hermetic sandbox with
//! stub `psql`/`cargo` on `PATH`. Only the database client and the cargo binary are
//! fakes: the guards, the shell logic, the sqlx invocation shape, the rollback and
//! the exit codes are all the real thing. That is deliberate — the repo has already
//! paid for source-text-only gate tests (`sliding_sync_perf_gate_tests.rs`, deleted
//! 2026-09-19) that stayed green while the script was broken.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// A temp tree holding the scripts under test plus stub `psql` / `cargo`.
struct Sandbox {
    root: PathBuf,
    log: PathBuf,
    state: PathBuf,
}

impl Sandbox {
    /// `scripts` are repo-relative paths, copied byte-for-byte so `ROOT_DIR`
    /// resolves inside the sandbox.
    fn new(tag: &str, scripts: &[&str]) -> Self {
        let unique =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("clock after epoch").as_nanos();
        let root = std::env::temp_dir().join(format!("sqlx_tool_{tag}_{}_{unique}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("scripts/ci")).expect("create scripts dir");
        fs::create_dir_all(root.join("bin")).expect("create bin dir");
        fs::create_dir_all(root.join("state")).expect("create state dir");
        for rel in scripts {
            let src = repo_root().join(rel);
            let dst = root.join(rel);
            fs::copy(&src, &dst).unwrap_or_else(|e| panic!("copy {src:?}: {e}"));
        }
        let log = root.join("state/log");
        fs::write(&log, "").expect("create log");
        let state = root.join("state");
        Self { root, log, state }
    }

    fn stub(&self, name: &str, body: &str) {
        let path = self.root.join("bin").join(name);
        fs::write(&path, format!("#!/usr/bin/env bash\n{body}\n")).expect("write stub");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod stub");
    }

    /// Write `n` fake cache entries so the rollback logic has something to restore.
    fn seed_cache(&self, n: usize) {
        let dir = self.root.join(".sqlx");
        fs::create_dir_all(&dir).expect("create .sqlx");
        for i in 0..n {
            fs::write(dir.join(format!("query-{i:04}.json")), "{}").expect("write entry");
        }
    }

    fn cache_entries(&self) -> usize {
        fs::read_dir(self.root.join(".sqlx"))
            .map(|entries| {
                entries.filter_map(Result::ok).filter(|e| e.file_name().to_string_lossy().starts_with("query-")).count()
            })
            .unwrap_or(0)
    }

    fn log_text(&self) -> String {
        fs::read_to_string(&self.log).unwrap_or_default()
    }

    fn state_text(&self, name: &str) -> String {
        fs::read_to_string(self.state.join(name)).unwrap_or_default()
    }

    /// `check_sqlx_cache_fresh.sh` 的静态不变量里有 `git ls-files .sqlx`
    /// （缓存必须是版本控制产物），所以沙箱要先是一个 repo，`--full` 才会走到委托分支。
    fn init_git_repo(&self) {
        for args in [vec!["init", "-q"], vec!["add", ".sqlx"]] {
            let out = Command::new("git").args(&args).current_dir(&self.root).output().expect("git must be runnable");
            assert!(out.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
        }
    }

    fn run(&self, script: &str, args: &[&str], env: &[(&str, &str)], drop_env: &[&str]) -> Output {
        let mut cmd = Command::new("bash");
        cmd.arg(self.root.join(script))
            .args(args)
            .current_dir(&self.root)
            .env("PATH", format!("{}:{}", self.root.join("bin").display(), std::env::var("PATH").unwrap_or_default()))
            .env("TOOL_LOG", &self.log)
            .env("TOOL_STATE", &self.state);
        for key in drop_env {
            cmd.env_remove(key);
        }
        for (key, value) in env {
            cmd.env(key, value);
        }
        cmd.output().expect("bash must be runnable")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn render(output: &Output) -> String {
    format!(
        "exit={:?}\n--- stdout ---\n{}--- stderr ---\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn exit_code(output: &Output) -> i32 {
    output.status.code().unwrap_or(-1)
}

fn combined(output: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))
}

/// `psql` stub for `sqlx_prepare.sh`: answers the three precondition probes.
///
/// `TABLE_COUNT` / `CORE_MISSING` / `SCHEMA_NAME` decide whether the guard sees a
/// migrated schema. Every invocation is logged so a test can prove that a refusal
/// happened **before** any `cargo sqlx prepare` ran.
const PSQL_PRECONDITION_STUB: &str = r#"
echo "psql $*" >> "$TOOL_LOG"
sql=""
for a in "$@"; do sql="$a"; done
if [[ -z "$sql" || "$sql" == "-" ]]; then sql="$(cat)"; fi
case "$sql" in
    *"VALUES ('events')"*) echo "${CORE_MISSING:-0}" ;;
    *"information_schema.tables"*"BASE TABLE"*) echo "${TABLE_COUNT:-222}" ;;
    *"current_schema()"*) echo "${SCHEMA_NAME:-public}" ;;
    *"SELECT 1"*) echo "1" ;;
    *) echo "" ;;
esac
"#;

/// `cargo` stub for `sqlx_prepare.sh`. `FAKE_PREPARE_ACTION`:
/// `ok` (no-op), `fail` (non-zero), `shrink` (delete 2 entries then succeed),
/// `add` (create one entry then succeed).
const CARGO_STUB: &str = r#"
echo "cargo $*" >> "$TOOL_LOG"
if [[ "${1:-}" == "sqlx" && "${2:-}" == "--version" ]]; then echo "sqlx-cli 0.8.6"; exit 0; fi
if [[ "${1:-}" == "sqlx" && "${2:-}" == "prepare" ]]; then
    case "${FAKE_PREPARE_ACTION:-ok}" in
        fail) exit 1 ;;
        shrink)
            ls "$PWD"/.sqlx/query-*.json 2>/dev/null | LC_ALL=C sort | head -2 | while read -r f; do rm -f "$f"; done
            exit 0 ;;
        add)
            echo '{}' > "$PWD/.sqlx/query-ffff-added.json"
            exit 0 ;;
        *) exit 0 ;;
    esac
fi
exit 0
"#;

// =============================================================================
// D-77: `scripts/ci/sqlx_prepare.sh`（唯一允许的 `.sqlx` 写入入口）
// =============================================================================

#[test]
fn sqlx_prepare_refuses_without_database_url_and_never_invokes_cargo() {
    let sb = Sandbox::new("nourl", &["scripts/ci/sqlx_prepare.sh"]);
    sb.seed_cache(5);
    sb.stub("psql", PSQL_PRECONDITION_STUB);
    sb.stub("cargo", CARGO_STUB);

    let out = sb.run("scripts/ci/sqlx_prepare.sh", &[], &[], &["DATABASE_URL"]);
    assert_ne!(exit_code(&out), 0, "missing DATABASE_URL must be fatal: {}", render(&out));
    assert!(combined(&out).contains("必须显式给出 DATABASE_URL"), "{}", render(&out));
    assert_eq!(sb.cache_entries(), 5, "a refusal must not touch the cache");
    assert!(!sb.log_text().contains("prepare"), "prepare must not start: {}", sb.log_text());
}

#[test]
fn sqlx_prepare_refuses_on_unmigrated_schema_before_running_prepare() {
    let sb = Sandbox::new("emptyschema", &["scripts/ci/sqlx_prepare.sh"]);
    sb.seed_cache(5);
    sb.stub("psql", PSQL_PRECONDITION_STUB);
    sb.stub("cargo", CARGO_STUB);

    // The incident state: schema resolves, but holds 0 baseline tables.
    let out = sb.run(
        "scripts/ci/sqlx_prepare.sh",
        &["--check"],
        &[("DATABASE_URL", "postgresql://stub/db"), ("TABLE_COUNT", "0"), ("CORE_MISSING", "3")],
        &[],
    );
    assert_ne!(exit_code(&out), 0, "{}", render(&out));
    assert!(combined(&out).contains("不满足前置条件"), "{}", render(&out));
    assert!(combined(&out).contains("1443"), "the message must record the measured symptom: {}", render(&out));
    assert_eq!(sb.cache_entries(), 5, "a refusal must not touch the cache");
    assert!(
        !sb.log_text().contains("prepare"),
        "prepare must never start against an unmigrated schema (it clears .sqlx first): {}",
        sb.log_text()
    );
}

#[test]
fn sqlx_prepare_rolls_back_when_the_cache_would_shrink() {
    let sb = Sandbox::new("shrink", &["scripts/ci/sqlx_prepare.sh"]);
    sb.seed_cache(5);
    sb.stub("psql", PSQL_PRECONDITION_STUB);
    sb.stub("cargo", CARGO_STUB);

    let out = sb.run(
        "scripts/ci/sqlx_prepare.sh",
        &[],
        &[("DATABASE_URL", "postgresql://stub/db"), ("FAKE_PREPARE_ACTION", "shrink")],
        &["ALLOW_CACHE_SHRINK"],
    );
    assert_ne!(exit_code(&out), 0, "an unexplained shrink must fail: {}", render(&out));
    assert!(combined(&out).contains("已回滚"), "{}", render(&out));
    assert_eq!(sb.cache_entries(), 5, "the snapshot must be restored verbatim");
}

#[test]
fn sqlx_prepare_keeps_a_deliberate_shrink_only_with_the_escape_hatch() {
    let sb = Sandbox::new("shrinkok", &["scripts/ci/sqlx_prepare.sh"]);
    sb.seed_cache(5);
    sb.stub("psql", PSQL_PRECONDITION_STUB);
    sb.stub("cargo", CARGO_STUB);

    let out = sb.run(
        "scripts/ci/sqlx_prepare.sh",
        &[],
        &[("DATABASE_URL", "postgresql://stub/db"), ("FAKE_PREPARE_ACTION", "shrink"), ("ALLOW_CACHE_SHRINK", "1")],
        &[],
    );
    assert_eq!(exit_code(&out), 0, "{}", render(&out));
    assert_eq!(sb.cache_entries(), 3, "the deliberate shrink must be kept");
    assert!(combined(&out).contains("ALLOW_CACHE_SHRINK=1"), "{}", render(&out));
}

#[test]
fn sqlx_prepare_rolls_back_when_prepare_fails() {
    let sb = Sandbox::new("prep fail", &["scripts/ci/sqlx_prepare.sh"]);
    sb.seed_cache(5);
    sb.stub("psql", PSQL_PRECONDITION_STUB);
    sb.stub("cargo", CARGO_STUB);

    let out = sb.run(
        "scripts/ci/sqlx_prepare.sh",
        &[],
        &[("DATABASE_URL", "postgresql://stub/db"), ("FAKE_PREPARE_ACTION", "fail")],
        &[],
    );
    assert_ne!(exit_code(&out), 0, "{}", render(&out));
    assert!(combined(&out).contains("已回滚"), "{}", render(&out));
    assert_eq!(sb.cache_entries(), 5, "a failed prepare must not damage the cache");
}

#[test]
fn sqlx_prepare_reports_additions_without_rolling_back() {
    let sb = Sandbox::new("add", &["scripts/ci/sqlx_prepare.sh"]);
    sb.seed_cache(5);
    sb.stub("psql", PSQL_PRECONDITION_STUB);
    sb.stub("cargo", CARGO_STUB);

    let out = sb.run(
        "scripts/ci/sqlx_prepare.sh",
        &[],
        &[("DATABASE_URL", "postgresql://stub/db"), ("FAKE_PREPARE_ACTION", "add")],
        &[],
    );
    assert_eq!(exit_code(&out), 0, "{}", render(&out));
    assert_eq!(sb.cache_entries(), 6, "additions are the normal case");
}

#[test]
fn cache_fresh_full_delegates_to_the_single_sanctioned_entry_point() {
    let sb = Sandbox::new("full", &["scripts/ci/check_sqlx_cache_fresh.sh", "scripts/ci/sqlx_prepare.sh"]);
    sb.seed_cache(5);
    sb.init_git_repo();
    sb.stub("psql", PSQL_PRECONDITION_STUB);
    sb.stub("cargo", CARGO_STUB);

    let out = sb.run(
        "scripts/ci/check_sqlx_cache_fresh.sh",
        &["--full"],
        &[("DATABASE_URL", "postgresql://stub/db"), ("TABLE_COUNT", "0"), ("CORE_MISSING", "3")],
        &[],
    );
    assert_ne!(exit_code(&out), 0, "--full must fail fast on an unmigrated schema: {}", render(&out));
    assert!(
        combined(&out).contains(".sqlx 缓存入口"),
        "--full must run sqlx_prepare.sh (one implementation of the guard): {}",
        render(&out)
    );
    assert!(!sb.log_text().contains("prepare"), "no prepare may start: {}", sb.log_text());
}

// =============================================================================
// D-75: `scripts/ci/converge_public_schema.sh` 的 TOCTOU 与栏杆
// =============================================================================

/// `psql` stub for converge: models the reference schema shrinking between rail 1
/// and the apply (`REF_FIRST` then `REF_LATER`), records the delete list that the
/// apply actually consumed (`state/dropped`) and counts how often the diff ran.
const PSQL_CONVERGE_STUB: &str = r#"
echo "psql $*" >> "$TOOL_LOG"
stdin_sql=""
if [[ ! -t 0 ]]; then stdin_sql="$(cat)"; fi
sql=""
for a in "$@"; do sql="$a"; done
if [[ -n "$stdin_sql" ]]; then sql="$sql
$stdin_sql"; fi

if [[ "$sql" == *"CREATE TEMP TABLE converge_extra"* ]]; then
    echo "apply" >> "$TOOL_LOG"
    # The apply heredoc must NOT re-evaluate the diff. If it does (the pre-fix
    # defect), that second evaluation is recorded here — the first version of this
    # stub short-circuited before this check, which made the "diff ran once"
    # assertion vacuous (caught by the R11 mutation sweep).
    if [[ "$sql" == *"SELECT kind, name"* ]]; then echo "diff" >> "$TOOL_LOG"; fi
    frozen="$(printf '%s\n' "$sql" | sed -n "s/.*converge_extra FROM '\([^']*\)'.*/\1/p" | head -1)"
    cp "$frozen" "$TOOL_STATE/dropped" 2>/dev/null || true
    exit 0
fi
if [[ "$sql" == *"BASE TABLE"* && "$sql" == *"test_template_ci"* ]]; then
    n=$(cat "$TOOL_STATE/ref_calls" 2>/dev/null || echo 0)
    n=$((n + 1)); echo "$n" > "$TOOL_STATE/ref_calls"
    if [[ "$n" -le 1 ]]; then echo "${REF_FIRST:-220}"
    elif [[ "$n" -eq 2 ]]; then echo "${REF_LATER:-220}"
    else echo "${REF_POST:-${REF_LATER:-220}}"; fi
    exit 0
fi
if [[ "$sql" == *"BASE TABLE"* && "$sql" == *"'public'"* ]]; then
    echo "${PUBLIC_TABLES:-222}"; exit 0
fi
if [[ "$sql" == *"relkind IN ('r','p','v','m','S')"* ]]; then
    if [[ "$sql" == *"n.nspname = 'public'"* ]]; then echo "${PUBLIC_OBJECTS:-222}"; else echo "${REFERENCE_OBJECTS:-220}"; fi
    exit 0
fi
if [[ "$sql" == *"EXCEPT"* && "$sql" == *"SELECT kind, name"* ]]; then
    echo "diff" >> "$TOOL_LOG"
    if [[ -s "$TOOL_STATE/diff_rows" ]]; then cat "$TOOL_STATE/diff_rows"; fi
    exit 0
fi
if [[ "$sql" == *"EXCEPT"* ]]; then
    echo "${EXTRA_AFTER:-0} ${MISSING_AFTER:-0}"; exit 0
fi
if [[ "$sql" == *"current_database()"* ]]; then echo "synapse_test"; exit 0; fi
if [[ "$sql" == *"SELECT 1"* ]]; then echo "1"; exit 0; fi
echo ""
"#;

fn seed_diff(sb: &Sandbox, rows: &str) {
    fs::write(sb.state.join("diff_rows"), rows).expect("seed diff rows");
}

#[test]
fn converge_refuses_when_the_reference_schema_shrank_before_the_apply() {
    // Exactly the incident: rail 1 sees 220 tables, then the concurrent seed's
    // `DROP SCHEMA test_template_ci CASCADE` drops it to 0 while the diff would
    // mark every public object as extra.
    let sb = Sandbox::new("convshrink", &["scripts/ci/converge_public_schema.sh"]);
    sb.stub("psql", PSQL_CONVERGE_STUB);
    seed_diff(&sb, "TABLE\td57_probe\nTABLE\trooms\n");

    let out = sb.run(
        "scripts/ci/converge_public_schema.sh",
        &[],
        &[("TEST_DATABASE_URL", "postgresql://stub/synapse_test"), ("REF_FIRST", "220"), ("REF_LATER", "0")],
        &[],
    );
    assert_ne!(exit_code(&out), 0, "{}", render(&out));
    assert!(combined(&out).contains("参考 schema"), "{}", render(&out));
    assert!(
        !sb.state_text("dropped").contains("d57_probe"),
        "nothing may be dropped when the reference shrank: {:?}",
        sb.state_text("dropped")
    );
    assert!(!sb.log_text().contains("apply"), "the apply must not run at all: {}", sb.log_text());
}

#[test]
fn converge_refuses_a_delete_list_that_would_empty_public() {
    // A frozen list that is *wrong* (e.g. computed while the reference was empty)
    // would remove everything; the survivors rail must stop it.
    let sb = Sandbox::new("convempty", &["scripts/ci/converge_public_schema.sh"]);
    sb.stub("psql", PSQL_CONVERGE_STUB);
    let rows: String = (0..222).map(|i| format!("TABLE\td57_probe_{i}\n")).collect();
    seed_diff(&sb, &rows);

    let out = sb.run(
        "scripts/ci/converge_public_schema.sh",
        &[],
        &[
            ("TEST_DATABASE_URL", "postgresql://stub/synapse_test"),
            ("PUBLIC_OBJECTS", "222"),
            ("REFERENCE_OBJECTS", "220"),
        ],
        &[],
    );
    assert_ne!(exit_code(&out), 0, "{}", render(&out));
    assert!(combined(&out).contains("这不是收敛，是清空"), "{}", render(&out));
    assert!(!sb.log_text().contains("apply"), "no DROP may run: {}", sb.log_text());
}

#[test]
fn converge_drops_the_frozen_list_and_never_re_consults_the_reference() {
    // Healthy converge: the reference stays put, one extra object is removed.
    // The proof that the TOCTOU is gone is the diff invocation count — the old
    // script ran it a second time inside the apply heredoc.
    let sb = Sandbox::new("convok", &["scripts/ci/converge_public_schema.sh"]);
    sb.stub("psql", PSQL_CONVERGE_STUB);
    seed_diff(&sb, "TABLE\td57_probe\n");

    let out = sb.run(
        "scripts/ci/converge_public_schema.sh",
        &[],
        &[
            ("TEST_DATABASE_URL", "postgresql://stub/synapse_test"),
            ("PUBLIC_OBJECTS", "221"),
            ("REFERENCE_OBJECTS", "220"),
        ],
        &[],
    );
    assert_eq!(exit_code(&out), 0, "{}", render(&out));
    assert_eq!(
        sb.log_text().matches("\ndiff\n").count(),
        1,
        "the diff must be evaluated exactly once (frozen list): {}",
        sb.log_text()
    );
    assert!(
        sb.state_text("dropped").contains("d57_probe"),
        "the apply must consume the frozen list: {:?}",
        sb.state_text("dropped")
    );
}

#[test]
fn converge_rejects_a_vacuous_post_check_when_the_reference_collapses_mid_apply() {
    // Third TOCTOU window: the reference survives rail 1 and the pre-apply re-check
    // but the concurrent seed's `DROP SCHEMA … CASCADE` lands **during** the apply.
    // The post-apply diff then compares public against an empty reference, so
    // `extra=0 / missing=0` holds for a public that lost objects — rail 8 must
    // refuse instead of reporting success.
    let sb = Sandbox::new("convpost", &["scripts/ci/converge_public_schema.sh"]);
    sb.stub("psql", PSQL_CONVERGE_STUB);
    seed_diff(&sb, "TABLE\td57_probe\n");

    let out = sb.run(
        "scripts/ci/converge_public_schema.sh",
        &[],
        &[
            ("TEST_DATABASE_URL", "postgresql://stub/synapse_test"),
            ("REF_FIRST", "220"),
            ("REF_LATER", "220"),
            ("REF_POST", "0"),
            ("PUBLIC_OBJECTS", "221"),
            ("REFERENCE_OBJECTS", "220"),
        ],
        &[],
    );
    assert_ne!(exit_code(&out), 0, "a collapsed reference must not pass: {}", render(&out));
    assert!(combined(&out).contains("判定不可信"), "the failure must name the vacuous invariant: {}", render(&out));
}

// =============================================================================
// D-76: `scripts/init_test_public_schema.sh` 的破坏性默认
// =============================================================================

/// `psql` stub for the init script: every statement is logged, table counts answer
/// `MIN_TABLES`-satisfying numbers.
const PSQL_INIT_STUB: &str = r#"
echo "psql $*" >> "$TOOL_LOG"
sql=""
for a in "$@"; do sql="$a"; done
if [[ "$sql" == *"information_schema.tables"* ]]; then echo "222"; exit 0; fi
if [[ "$sql" == *"SELECT 1"* ]]; then echo "1"; exit 0; fi
exit 0
"#;

fn sandbox_with_migrations(tag: &str) -> Sandbox {
    let sb = Sandbox::new(tag, &["scripts/init_test_public_schema.sh"]);
    fs::create_dir_all(sb.root.join("migrations")).expect("create migrations dir");
    fs::write(sb.root.join("migrations/00000000_unified_schema_v12.sql"), "-- stub\n").expect("write migration");
    sb.stub("psql", PSQL_INIT_STUB);
    sb
}

#[test]
fn init_public_schema_does_not_drop_public_by_default() {
    let sb = sandbox_with_migrations("initdefault");
    let out = sb.run(
        "scripts/init_test_public_schema.sh",
        &[],
        &[("TEST_DATABASE_URL", "postgresql://stub/synapse_test")],
        &["RESET_PUBLIC"],
    );
    assert_eq!(exit_code(&out), 0, "{}", render(&out));
    assert!(
        !sb.log_text().contains("DROP SCHEMA public CASCADE"),
        "the default must be a non-destructive idempotent apply: {}",
        sb.log_text()
    );
    assert!(sb.log_text().contains("CREATE SCHEMA IF NOT EXISTS public"), "{}", sb.log_text());
}

#[test]
fn init_public_schema_still_resets_when_explicitly_asked() {
    let sb = sandbox_with_migrations("initreset");
    let out = sb.run(
        "scripts/init_test_public_schema.sh",
        &[],
        &[("TEST_DATABASE_URL", "postgresql://stub/synapse_test"), ("RESET_PUBLIC", "1")],
        &[],
    );
    assert_eq!(exit_code(&out), 0, "{}", render(&out));
    assert!(
        sb.log_text().contains("DROP SCHEMA public CASCADE"),
        "RESET_PUBLIC=1 must still reset (the escape hatch stays alive): {}",
        sb.log_text()
    );
}

/// Cheap structural check that the scripts really are the ones this file sandboxes:
/// if someone renames a script, the sandbox's `fs::copy` would panic first, but a
/// stale reference in docs/rules would not. Keeps the file honest about its scope.
#[test]
fn the_guarded_scripts_exist() {
    for rel in [
        "scripts/ci/sqlx_prepare.sh",
        "scripts/ci/check_sqlx_cache_fresh.sh",
        "scripts/ci/converge_public_schema.sh",
        "scripts/init_test_public_schema.sh",
    ] {
        let path: &Path = &repo_root().join(rel);
        assert!(path.is_file(), "{rel} must exist");
    }
}
