//! Guard: test fixtures must not swallow database errors.
//!
//! ## Why this file exists
//!
//! `CLAUDE.md` states the rule directly:
//!
//! > **Never `unwrap_or_default()` on DB queries in security-relevant paths** —
//! > it silently converts DB errors to "empty/false" defaults.
//!
//! The same failure mode appears throughout the storage test fixtures as
//! `.execute(pool).await.ok();`. A swallowed *setup* error is worse than a
//! swallowed read: the fixture silently does nothing, the test proceeds with
//! missing preconditions, and the eventual failure surfaces three statements
//! later as something unrelated.
//!
//! Measured example (round 26–27 of this audit):
//! `room_summary::db_tests::ensure_test_room` discarded its `INSERT INTO rooms`
//! result. The test then failed with
//! `room_summary_members violates foreign key constraint fk_room_summary_members_room`
//! — which points at the *member* insert, not at the room insert that actually
//! went wrong. The real cause stayed hidden behind `.ok()` and cost
//! considerable debugging time.
//!
//! ## What this guard enforces
//!
//! In files that are unambiguously test support — `*db_tests*.rs`,
//! `test_mocks/*`, and `tests/` — a DB write must not discard its result with
//! `.ok()`. Setup preconditions should use `.expect(...)` so a failure names
//! itself at the point of failure.
//!
//! Production call sites are deliberately **out of scope**: many are legitimate
//! (an `Option` extraction, a best-effort cache write, awaiting a shutdown
//! signal). Flagging those would make this guard noisy and get it disabled.

use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// True when the path names a test-support file rather than production code.
fn is_test_support(path: &Path) -> bool {
    let s = path.to_string_lossy();
    if s.contains("/tests/") || s.contains("/test_mocks/") {
        return true;
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    name.contains("db_tests") || name.ends_with("_tests.rs") || name == "db_tests.rs"
}

/// Collects `.rs` files under the given roots.
fn rust_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Skip build output and the stale worktree copy.
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if matches!(name, "target" | ".git" | ".claude" | "node_modules") {
                continue;
            }
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Lines where a DB write's result is discarded.
///
/// Matches the two spellings in use:
///   * `... .execute(pool).await.ok();`
///   * `let _ = sqlx::query(...).execute(pool).await;`
fn swallowed_write_lines(source: &str) -> Vec<String> {
    let mut found = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") {
            continue;
        }
        // `.ok()` directly terminating a write.
        if trimmed.ends_with(".ok();") {
            // Walk back a few lines to see whether this chain executed a write.
            let start = idx.saturating_sub(6);
            let window = lines[start..=idx].join(" ");
            if window.contains(".execute(") || window.contains(".fetch_") {
                found.push(format!("line {}: {}", idx + 1, trimmed));
            }
        }
    }
    found
}

#[test]
fn test_fixtures_do_not_swallow_database_writes() {
    let root = repo_root();
    let mut files = Vec::new();
    for crate_dir in [
        "src",
        "synapse-common/src",
        "synapse-cache/src",
        "synapse-storage/src",
        "synapse-e2ee/src",
        "synapse-federation/src",
        "synapse-services/src",
    ] {
        rust_files(&root.join(crate_dir), &mut files);
    }

    let mut offenders: Vec<String> = Vec::new();
    for path in files.iter().filter(|p| is_test_support(p)) {
        let Ok(source) = fs::read_to_string(path) else { continue };
        for hit in swallowed_write_lines(&source) {
            let rel = path.strip_prefix(&root).unwrap_or(path);
            offenders.push(format!("{}: {hit}", rel.display()));
        }
    }

    assert!(
        offenders.is_empty(),
        "测试夹具不得吞掉数据库写入错误（CLAUDE.md 明令的 unwrap_or_default 同型问题）。\n\
         吞掉 setup 错误会让夹具静默不生效，真正的失败在几条语句之后以无关的形式爆出。\n\
         请改用 .expect(\"...\")，让错误在发生处点名自己：\n{}",
        offenders.join("\n")
    );
}

/// Sanity: the guard must actually be scanning files (otherwise it is vacuous).
#[test]
fn guard_scans_test_support_files() {
    let root = repo_root();
    let mut files = Vec::new();
    rust_files(&root.join("synapse-storage/src"), &mut files);
    let test_files: Vec<_> = files.iter().filter(|p| is_test_support(p)).collect();
    assert!(test_files.len() >= 10, "应扫描到 >= 10 个测试支持文件，实际 {}；路径或命名是否变了？", test_files.len());
}
