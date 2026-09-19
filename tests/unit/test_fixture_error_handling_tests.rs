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

/// Returns, for each line index, whether it sits inside a `#[cfg(test)]` module.
///
/// Test fixtures also live in inline `#[cfg(test)] mod tests { ... }` blocks
/// inside otherwise-production files (e.g. `synapse-storage/src/widget.rs`,
/// `invite_blocklist.rs`). A file-name check alone misses those, and they carry
/// the same swallowed-setup-error hazard.
fn cfg_test_mask(lines: &[&str]) -> Vec<bool> {
    let mut mask = vec![false; lines.len()];
    let mut depth: i32 = 0; // brace depth once inside a cfg(test) module
    let mut armed = false; // saw `#[cfg(test)]`, waiting for its `{`

    for (i, raw) in lines.iter().enumerate() {
        let line = raw.trim();
        if line.starts_with("#[cfg(test)]") || line.starts_with("#[cfg(all(test") {
            armed = true;
        }
        if armed || depth > 0 {
            mask[i] = true;
        }
        if armed && line.contains('{') {
            depth += line.matches('{').count() as i32;
            armed = false;
            if depth <= 0 {
                depth = 0;
            }
            continue;
        }
        if depth > 0 {
            depth += line.matches('{').count() as i32;
            depth -= line.matches('}').count() as i32;
            if depth < 0 {
                depth = 0;
            }
        }
    }
    mask
}

/// Every source root this guard walks. Shared by the enforcement test and the
/// non-vacuity sanity test so the two can never disagree about the scan surface.
const SCAN_ROOTS: [&str; 8] = [
    "src",
    "synapse-common/src",
    "synapse-cache/src",
    "synapse-storage/src",
    "synapse-e2ee/src",
    "synapse-federation/src",
    "synapse-services/src",
    "tests",
];

/// Lines where a DB write's result is discarded with `.ok()`.
///
/// **Scope:** the `.ok();` spelling only. `tests/` was missing from the root
/// list even though `is_test_support()` already claimed it (gate-integrity
/// sweep B8); that is fixed here.
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

/// Lines where a DB write's result is discarded with `let _ = …await;`.
///
/// This is the second spelling of the same swallowed-setup-error mistake as
/// `.ok()`. The 2026-09-19 sweep counted 187 such statements overall, 99 under
/// path-named test support; this statement-level scan (which also catches
/// multi-line chains and inline `#[cfg(test)]` blocks) measures 231 in the
/// test-support scope it shares with the `.ok()` guard — see
/// [`LET_UNDERSCORE_AWAIT_WRITE_BASELINE`]. It is deliberately **not** forbidden
/// outright: most of these sites are best-effort cleanup/teardown
/// (`cleanup_with_suffix`, `cleanup_summary_data`) where deleting zero rows is
/// not an error, and flagging every one would get this guard disabled. The
/// risky sub-case — a swallowed *setup* write such as an `ensure_test_user`
/// insert — is instead **ratcheted**: the test-support count is pinned in
/// [`LET_UNDERSCORE_AWAIT_WRITE_BASELINE`] so it can only shrink.
///
/// The debt is recorded right here, next to the scanner that measures it. An
/// earlier version of this docstring pointed at
/// `docs/audit/GATE_INTEGRITY_FOLLOWUP_2026-09-19.md`; that document only
/// records that this form is *not* enforced, so the pointer named a record
/// that does not exist.
fn swallowed_let_underscore_lines(source: &str) -> Vec<String> {
    let lines: Vec<&str> = source.lines().collect();
    let mut found = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.starts_with("//") || !trimmed.starts_with("let _ =") {
            i += 1;
            continue;
        }
        // A `let _ = …;` statement routinely spans lines
        // (`sqlx::query(…).bind(…).execute(…).await;`), so accumulate up to its
        // terminating `;` instead of matching a single line. The cap keeps a
        // malformed statement from swallowing the rest of the file.
        let start = i;
        let mut end = i;
        let mut window = String::new();
        while end < lines.len() && end - start < 50 {
            window.push_str(lines[end]);
            window.push(' ');
            if lines[end].trim_end().ends_with(';') {
                break;
            }
            end += 1;
        }
        if window.contains(".execute(") && window.contains(".await") {
            found.push(format!("line {}: {}", start + 1, trimmed));
        }
        i = end + 1;
    }
    found
}

/// Both swallowed-write spellings found in `path`, restricted to test support.
///
/// Test support is either a whole-file property (named fixtures) or a per-line
/// property (inline `#[cfg(test)]` modules), exactly as the `.ok()` guard
/// already decided it. Returns `(ok_offenders, let_underscore_offenders)`.
fn test_support_swallowed_writes(path: &Path) -> (Vec<String>, Vec<String>) {
    let Ok(source) = fs::read_to_string(path) else {
        return (Vec::new(), Vec::new());
    };
    let lines: Vec<&str> = source.lines().collect();
    let in_cfg_test = cfg_test_mask(&lines);
    let whole_file = is_test_support(path);
    let keep = |hit: &str| -> bool {
        let lineno: usize =
            hit.split_whitespace().nth(1).and_then(|t| t.trim_end_matches(':').parse().ok()).unwrap_or(0);
        let idx = lineno.saturating_sub(1);
        whole_file || in_cfg_test.get(idx).copied().unwrap_or(false)
    };

    let mut ok_hits = Vec::new();
    for hit in swallowed_write_lines(&source) {
        if keep(&hit) {
            ok_hits.push(hit);
        }
    }
    let mut let_hits = Vec::new();
    for hit in swallowed_let_underscore_lines(&source) {
        if keep(&hit) {
            let_hits.push(hit);
        }
    }
    (ok_hits, let_hits)
}

/// The files this guard walks: every [`SCAN_ROOTS`] entry, failing loud when a
/// root is missing so a moved tree cannot silently narrow the scan.
fn scan_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for crate_dir in SCAN_ROOTS {
        let dir = root.join(crate_dir);
        assert!(
            dir.is_dir(),
            "guard scan root `{crate_dir}` does not exist (looked at {}) — the scan would cover \
             less than this guard claims",
            dir.display()
        );
        rust_files(&dir, &mut files);
    }
    files
}

#[test]
fn test_fixtures_do_not_swallow_database_writes() {
    let root = repo_root();
    let mut offenders: Vec<String> = Vec::new();
    for path in scan_files(&root) {
        let (ok_offenders, _) = test_support_swallowed_writes(&path);
        if ok_offenders.is_empty() {
            continue;
        }
        let rel = path.strip_prefix(&root).unwrap_or(&path);
        for hit in ok_offenders {
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

/// Measured count of `let _ = …execute(…).await;` sites in test-support code.
///
/// Measured 2026-09-19 over every [`SCAN_ROOTS`] entry, using the same
/// test-support scope as the `.ok()` guard: whole-file fixtures plus inline
/// `#[cfg(test)]` modules. The 2026-09-19 sweep quoted **99** because it was a
/// single-line grep restricted to path-named fixtures; the statement-level scan
/// used here also catches multi-line chains and `#[cfg(test)]` blocks, hence the
/// larger pinned number.
///
/// This is a ratchet, not a target: the count may only shrink. When you remove
/// such a site, tighten this constant by the same amount in the same change so
/// the debt can never silently grow back.
const LET_UNDERSCORE_AWAIT_WRITE_BASELINE: usize = 231;

/// Ratchet for the `let _ = …execute(…).await;` spelling.
///
/// It cannot be asserted as zero (see [`swallowed_let_underscore_lines`]), but
/// it must not exceed the measured baseline either — otherwise a new swallowed
/// setup write enters the tree with no gate noticing. Lowering the constant
/// below the measured count fails, so tightening it is a deliberate act.
#[test]
fn let_underscore_await_writes_do_not_exceed_baseline() {
    let root = repo_root();
    let mut found: Vec<String> = Vec::new();
    for path in scan_files(&root) {
        let (_, let_hits) = test_support_swallowed_writes(&path);
        if let_hits.is_empty() {
            continue;
        }
        let rel = path.strip_prefix(&root).unwrap_or(&path);
        for hit in let_hits {
            found.push(format!("{}: {hit}", rel.display()));
        }
    }

    assert!(
        found.len() <= LET_UNDERSCORE_AWAIT_WRITE_BASELINE,
        "测试支持代码里的 `let _ = …execute(…).await;` 数量 {} 超过了钉住的基线 {}。\n\
         这种写法与 `.ok()` 一样会吞掉 setup 错误。新增的这类写入请改成 `.expect(\"...\")`；\n\
         若确实只是 best-effort 清理，也请先把它从基线里去掉再说明理由（基线只允许缩小）：\n{}",
        found.len(),
        LET_UNDERSCORE_AWAIT_WRITE_BASELINE,
        found.join("\n")
    );
}

/// Sanity: the guard must actually be scanning files (otherwise it is vacuous).
#[test]
fn guard_scans_test_support_files() {
    let root = repo_root();
    let files = scan_files(&root);
    let test_files: Vec<_> = files.iter().filter(|p| is_test_support(p)).collect();
    assert!(test_files.len() >= 100, "应扫描到 >= 100 个测试支持文件，实际 {}；路径或命名是否变了？", test_files.len());
    // `tests/` was absent from the root list while `is_test_support()` already
    // treated it as test support, so its 14 `.ok()` sites escaped the guard
    // (sweep B8). Pin that the tree is really part of the scan.
    let under_tests = test_files.iter().filter(|p| p.to_string_lossy().contains("/tests/")).count();
    assert!(under_tests >= 10, "`tests/` must be scanned; found only {under_tests} test-support files there");
}
