//! Guard tests for `scripts/check_config_consistency.py`.
//!
//! ## Why this file exists
//!
//! The repo ships **two** config trees:
//!
//! | tree | used by |
//! |---|---|
//! | `docker/deploy/config/` | `docker/deploy/docker-compose.yml` (the deployment orchestration) |
//! | `docker/config/` | `docker/docker-compose.yml` (dev compose) + baked into the image |
//!
//! They are kept in sync **by hand**. `docker/deploy/README.md` documents the
//! convention ("修改 canonical 后需同步（`cp docker/config/<file> config/<file>`）"),
//! but nothing enforces it: `deploy.sh` does not copy, and no CI step compares.
//!
//! This is the *same failure mode* that the migrations duplicate directory had —
//! a hand-synced copy that silently drifted from its source. Migrations were
//! fixed (`2b16dc3c`: single source + `check_migration_consistency.py` blocking
//! in CI); configs were not.
//!
//! A concrete instance of the harm: on 2026-09-11 the two `rate_limit.yaml`
//! files disagreed on `sync.enabled` (`false` vs `true`) while both
//! `homeserver.yaml` files declared `true`. Because the file config replaces the
//! whole `rate_limit:` section, the file won — `/sync` ended up with **no**
//! rate limiting at all (120/120 requests returned 200; see
//! `docs/audit/S_series_verification_2026-09-11.md` §2).
//!
//! These tests lock in a semantic (comment-insensitive) comparison plus an
//! explicit allowlist, so genuine drift fails CI while the *intentional*
//! dev/prod differences stay permitted and documented.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn script_path() -> PathBuf {
    repo_root().join("scripts/check_config_consistency.py")
}

/// Runs the checker, returning `(exit_code, combined_output)`.
fn run_checker(extra_args: &[&str]) -> (i32, String) {
    let out = Command::new("python3")
        .arg(script_path())
        .args(extra_args)
        .current_dir(repo_root())
        .output()
        .expect("check_config_consistency.py must be runnable");

    let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), combined)
}

// =============================================================================
// The checker exists and passes on the current tree
// =============================================================================

#[test]
fn config_consistency_checker_exists() {
    let p = script_path();
    assert!(p.is_file(), "应存在配置一致性检查脚本: {p:?}");
}

#[test]
fn checker_passes_on_current_tree() {
    let (code, output) = run_checker(&[]);
    assert_eq!(
        code, 0,
        "当前工作树的配置应当一致；若刚改了某一侧的配置，\
         请同步另一侧或把差异登记进脚本的 ALLOWED_DIFFERENCES\n输出:\n{output}"
    );
    assert!(output.contains("config"), "检查器应打印它比较了哪些配置，实际输出:\n{output}");
}

/// `--json-report` mirrors `check_migration_consistency.py` so CI can archive it.
#[test]
fn checker_supports_json_report() {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let report = std::env::temp_dir().join(format!("cfg_consistency_{}_{}.json", std::process::id(), n));

    let (code, output) = run_checker(&["--json-report", report.to_str().expect("utf8 path")]);
    assert_eq!(code, 0, "带 --json-report 也应通过\n输出:\n{output}");

    let body = fs::read_to_string(&report).expect("json report must be written");
    let json: serde_json::Value = serde_json::from_str(&body).expect("report must be valid JSON");
    assert!(json.get("issues").is_some(), "报告必须含 issues 字段");
    assert!(json.get("warnings").is_some(), "报告必须含 warnings 字段");

    let _ = fs::remove_file(&report);
}

// =============================================================================
// Semantic comparison must be comment-insensitive
// =============================================================================

/// Comment-only differences must NOT be reported as drift.
///
/// The two trees legitimately carry different explanatory comments (the deploy
/// copy documents that `homeserver.yaml`'s `rate_limit:` section is inert).
/// Failing on those would make the gate unusable and get it disabled.
#[test]
fn checker_ignores_comment_only_differences() {
    let (code, output) = run_checker(&["--explain"]);
    assert_eq!(code, 0, "注释差异不应导致失败\n输出:\n{output}");
    assert!(!output.contains("rate_limit.yaml: semantic difference"), "不应把纯注释差异报成语义差异\n输出:\n{output}");
}

// =============================================================================
// The intentional dev/prod difference is allowlisted AND justified
// =============================================================================

/// `rate_limit.yaml`'s `sync.enabled` is intentionally `false` in dev and
/// `true` in deploy. The checker must allow exactly that, and the script must
/// document why.
#[test]
fn intentional_sync_enabled_difference_is_allowlisted_and_documented() {
    let script = fs::read_to_string(script_path()).expect("checker must be readable");
    assert!(
        script.contains("sync.enabled") || script.contains("sync") && script.contains("enabled"),
        "脚本应显式登记 sync.enabled 的开发/生产差异"
    );
    assert!(
        script.contains("生产") || script.contains("production") || script.contains("deploy"),
        "登记差异时必须写明理由，否则等于把真实的漂移也放行"
    );
    assert!(
        script.contains("ALLOWED_DIFFERENCES") || script.contains("允许差异") || script.contains("allowlist"),
        "差异应集中在一处显式白名单里，便于审计"
    );
}

/// The checker must be wired into CI, otherwise it is documentation, not a gate.
#[test]
fn checker_is_wired_into_ci() {
    let ci = fs::read_to_string(repo_root().join(".github/workflows/ci.yml")).expect("ci.yml readable");
    assert!(
        ci.contains("check_config_consistency"),
        "check_config_consistency.py 必须接入 ci.yml（阻塞），\
         否则与它要取代的手工 cp 约定一样不可靠"
    );
}

// =============================================================================
// The drifted-once file is specifically guarded
// =============================================================================

/// `rate_limit.yaml` is the file whose drift caused the `/sync` limiter outage;
/// it must be in the compared set.
#[test]
fn rate_limit_config_is_compared() {
    let (code, output) = run_checker(&["--explain"]);
    assert_eq!(code, 0, "输出:\n{output}");
    assert!(
        output.contains("rate_limit.yaml"),
        "rate_limit.yaml 必须在比较集合内（它正是曾漂移并导致 /sync 无限流的文件）\n输出:\n{output}"
    );
    assert!(
        output.contains("homeserver.yaml") && output.contains("postgres.conf"),
        "三份配置都应被比较\n输出:\n{output}"
    );
}
