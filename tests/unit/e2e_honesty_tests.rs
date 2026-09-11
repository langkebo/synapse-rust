//! Honesty guard for the `e2e` test target.
//!
//! ## Why this file exists
//!
//! `tests/e2e/e2e_scenarios.rs` contained 20 functions named `test_*` under the
//! module header *"E2E (End-to-End) Test Scenarios — verify complete user
//! workflows across multiple API modules"*.
//!
//! They perform **no I/O at all** — no HTTP client, no database pool, no
//! storage. Every assertion is a tautology over a local literal:
//!
//! ```ignore
//! let room_id = "!created_room:localhost";
//! assert!(room_id.starts_with('!'));   // true of the literal
//! let logout_success = true;
//! assert!(logout_success);             // true of the literal
//! ```
//!
//! Running the target printed `=== E2E: Complete Room Lifecycle ===` step by
//! step while executing nothing, so a reader — and a CI log — was led to believe
//! a workflows had been exercised. The whole target finished in 48 ms.
//!
//! This is the same "dishonest green" class as the empty `cargo test --doc`
//! gate, the `performance_api_benchmarks` silent skip, and
//! `query_performance_tests.rs` asserting a `yield_now()` duration.
//!
//! The scenarios have been renamed to `simulated_*`, documented as
//! non-verifying, and the target is now run in CI — but only so that it cannot
//! rot silently. **It verifies nothing.**
//!
//! These tests therefore pin two things:
//!   1. the file must not silently gain real I/O without its documentation
//!      being updated (which is the moment it becomes a meaningful gate);
//!   2. no function may be named `test_*` there, which would re-introduce the
//!      implication of verification.

use std::fs;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    let p = repo_root().join(rel);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("expected {p:?} to be readable: {e}"))
}

/// Markers of real I/O. Their absence is what makes the file non-verifying.
const IO_MARKERS: [&str; 8] =
    ["reqwest", "sqlx", "PgPool", "get_test_pool", "Client::new", "TcpStream", "E2E_BASE_URL", "hyper"];

/// `e2e_scenarios.rs` performs no I/O, and must stay documented as such.
///
/// If this test fails because I/O was added: that is **good news** — the file
/// is becoming a real end-to-end test. Update the module header (remove the
/// "SIMULATED / not end-to-end" wording), rename the functions back to `test_*`,
/// and delete this guard.
#[test]
fn e2e_scenarios_performs_no_io_and_says_so() {
    let src = read("tests/e2e/e2e_scenarios.rs");
    // Strip line comments first: the module header deliberately *names* these
    // markers (it tells the reader to grep for them), so a naive substring
    // check would match its own documentation on every run.
    let code: String = src
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with("//") && !t.starts_with("///") && !t.starts_with("//!")
        })
        .collect::<Vec<_>>()
        .join("\n");

    let found: Vec<&str> = IO_MARKERS.iter().copied().filter(|m| code.contains(m)).collect();
    assert!(
        found.is_empty(),
        "tests/e2e/e2e_scenarios.rs 的**非注释代码**中出现了真实 I/O 标记 {found:?}。\
         这本身是好事（它开始真正验证端到端行为），但必须同步：\
         更新模块头（去掉「SIMULATED / not end-to-end」表述）、把函数名改回 test_*、\
         并删除本守卫。否则文件名与内容会互相矛盾。"
    );

    assert!(
        src.contains("SIMULATED") && src.contains("performs no I/O"),
        "既然该文件不执行任何 I/O，其模块头必须明确写着它是模拟的、不做 I/O —— \
         否则读者会以为端到端行为已被验证"
    );
}

/// No function in that file may be named `test_*`: the prefix implies verification.
#[test]
fn e2e_scenarios_functions_do_not_imply_verification() {
    let src = read("tests/e2e/e2e_scenarios.rs");
    let offenders: Vec<&str> = src.lines().map(str::trim).filter(|l| l.starts_with("fn test_")).collect();
    assert!(
        offenders.is_empty(),
        "tests/e2e/e2e_scenarios.rs 中不应再有 `fn test_*` 命名：\
         该前缀暗示「这条断言在验证某行为」，而这些函数只对局部字面量断言重言式。\
         请用 `simulated_*`，直到该文件真正产生 I/O。\n实际:\n{}",
        offenders.join("\n")
    );
}

/// The `e2e` target must be invoked by CI, so it cannot rot unnoticed.
#[test]
fn e2e_target_is_invoked_by_ci() {
    let workflows = repo_root().join(".github/workflows");
    let mut found = Vec::new();
    for entry in fs::read_dir(&workflows).expect("workflows dir").flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        if !(name.ends_with(".yml") || name.ends_with(".yaml")) {
            continue;
        }
        if let Ok(text) = fs::read_to_string(&path) {
            if text.contains("--test e2e") {
                found.push(name.to_string());
            }
        }
    }
    assert!(
        !found.is_empty(),
        "e2e 目标必须被某个 workflow 调用：它此前完全未被 CI 执行，\
         而 CLAUDE.md 把它列为可用命令 —— 不被执行的测试会静默腐坏"
    );
}

/// `user_flow_tests.rs` holds the real HTTP harness; its tests are `#[ignore]`d
/// behind `E2E_RUN=1`. That opt-in nature must be preserved and documented, so
/// nobody assumes CI exercises it.
#[test]
fn user_flow_tests_remain_an_explicit_opt_in() {
    let src = read("tests/e2e/user_flow_tests.rs");
    assert!(src.contains("reqwest"), "user_flow_tests.rs 应是真正的 HTTP 测试载体");
    assert!(src.contains("E2E_RUN"), "user_flow_tests.rs 的用例应由 E2E_RUN 显式开启，而不是在 CI 中默认运行");
    assert!(src.contains("#[ignore"), "user_flow_tests.rs 需要 `#[ignore]` 标注，否则会在没有 homeserver 的环境里失败");
}

// ── skipped tests must say why ──────────────────────────────────────────

/// A bare `#[ignore]` is a permanently-silent test: nothing records why it is
/// skipped, what would unblock it, or how to run it on purpose. Such a test rots
/// invisibly and still inflates the "test count" a reader trusts.
///
/// This repo reached 25 `#[ignore]` sites, 14 of which were bare (`grep -c` on
/// 2026-09-12). The four genuine ones — load/latency smoke tests whose assertions
/// depend on wall-clock timing and therefore cannot run in CI — now carry an
/// explicit reason **and the exact command to run them**. This guard keeps it
/// that way.
///
/// Note the scan deliberately matches a *real* attribute line, not the string
/// `#[ignore]` inside a comment or a `contains("#[ignore")` assertion — 10 of the
/// 14 original hits were prose.
#[test]
fn ignored_tests_must_carry_a_reason() {
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }

    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for rel in [
        "src",
        "tests",
        "synapse-common/src",
        "synapse-storage/src",
        "synapse-services/src",
        "synapse-e2ee/src",
        "synapse-cache/src",
        "synapse-federation/src",
    ] {
        walk(&root.join(rel), &mut files);
    }
    assert!(!files.is_empty(), "scanner found no .rs files");

    let mut offenders = Vec::new();
    for path in files {
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (idx, line) in source.lines().enumerate() {
            let trimmed = line.trim();
            // A real attribute: the line is exactly `#[ignore]`.
            if trimmed == "#[ignore]" {
                offenders.push(format!(
                    "{}:{} — bare `#[ignore]`; use `#[ignore = \"<why, and how to run it>\"]`",
                    path.strip_prefix(&root).unwrap_or(&path).display(),
                    idx + 1
                ));
            }
        }
    }

    assert!(offenders.is_empty(), "tests skipped without a stated reason:\n  {}", offenders.join("\n  "));
}

// ── assertions must not be skippable ────────────────────────────────────

/// A test that skips its **assertion** when a database operation fails is worse
/// than no test: it reports green while the operation is broken, and it destroys
/// the signal that would have found the bug.
///
/// This is a distinct, narrower failure than the ordinary "skip because the
/// environment is unavailable" guard (which legitimately precedes it): by the
/// time execution reaches the assertion, the schema and pool have already been
/// verified to exist, so a failure there is a real defect, not a missing
/// dependency.
///
/// Two such sites existed on 2026-09-12:
///   * `tests/unit/worker_tests.rs` — `Skipping test_heartbeat assertion:
///     database operation failed`
///   * `tests/unit/room_summary_tests.rs` — `Skipping test_add_member assertion:
///     database operation failed`
///
/// Both now assert. This guard keeps the pattern out.
#[test]
fn tests_must_not_skip_their_own_assertions() {
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }

    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    walk(&root.join("tests"), &mut files);
    assert!(!files.is_empty(), "scanner found no test .rs files");

    let mut offenders = Vec::new();
    for path in files {
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (idx, line) in source.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            if !trimmed.contains("eprintln!") {
                continue;
            }
            let lower = trimmed.to_ascii_lowercase();
            // Only the "assertion" flavour: skipping because a dependency is
            // missing is a separate (legitimate) concern.
            if lower.contains("skipping") && lower.contains("assertion") {
                offenders.push(format!(
                    "{}:{} — skips its own assertion when the operation fails; assert instead: {}",
                    path.strip_prefix(&root).unwrap_or(&path).display(),
                    idx + 1,
                    trimmed
                ));
            }
        }
    }

    assert!(offenders.is_empty(), "tests that silently skip their assertions:\n  {}", offenders.join("\n  "));
}
