#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::fs;
use std::path::{Path, PathBuf};

/// Collect `*.rs` files under `dir`, **failing loudly if `dir` is not a
/// directory**.
///
/// Regression context (measured 2026-09-19): this used to be
/// `if let Ok(entries) = fs::read_dir(dir) { … }`, so a scan root that no longer
/// existed produced an *empty* file list and both guards below passed on an
/// empty set. That is exactly what happened: the roots were
/// `src/web/routes[/handlers]`, which stopped existing once the HTTP surface
/// moved to the `synapse-web` crate (commit `4ca7a635`), so this whole file was
/// dead for an unknown number of releases while reporting green. A guard whose
/// input set can silently become empty is not a guard.
fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|error| {
        panic!(
            "placeholder_scan: scan root {} is not a readable directory ({error}). \
             If the code moved, fix the path here — do NOT let the scan silently examine nothing.",
            dir.display()
        )
    });
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out);
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// Assert the scan actually saw a plausible amount of code.
///
/// A path typo that still resolves (e.g. an empty directory) must not turn the
/// guards into no-ops; the crate has dozens of route files.
fn assert_scan_is_non_trivial(dir: &Path, files: &[PathBuf]) {
    assert!(
        files.len() >= 20,
        "placeholder_scan: only {} .rs file(s) found under {} — the scan is not looking at the \
         route sources it is supposed to police (expected dozens)",
        files.len(),
        dir.display()
    );
}

fn load_allowlist_entries(file: &Path) -> Vec<String> {
    let content = fs::read_to_string(file).expect("allowlist should be readable");

    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(ToOwned::to_owned)
        .collect()
}

#[test]
fn test_no_placeholder_auth_user_ignores_in_handlers() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let handlers_dir = root.join("synapse-web").join("src").join("routes").join("handlers");
    let mut files = Vec::new();
    collect_rs_files(&handlers_dir, &mut files);
    files.sort();
    assert_scan_is_non_trivial(&handlers_dir, &files);

    let mut violations = Vec::new();
    for file in files {
        let content = match fs::read_to_string(&file) {
            Ok(c) => c,
            Err(_) => continue,
        };

        if content.contains("let _ = auth_user;") {
            violations.push(file.display().to_string());
        }
    }

    assert!(violations.is_empty(), "Found placeholder-style auth ignores in handlers:\n{}", violations.join("\n"));
}

#[test]
fn test_empty_json_successes_are_allowlisted() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let routes_dir = root.join("synapse-web").join("src").join("routes");
    let allowlist_file = root.join("scripts").join("shell_routes_allowlist.txt");
    let allowlist = load_allowlist_entries(&allowlist_file);
    let mut files = Vec::new();
    collect_rs_files(&routes_dir, &mut files);
    files.sort();
    assert_scan_is_non_trivial(&routes_dir, &files);

    let mut violations = Vec::new();
    let mut hits: std::collections::HashSet<String> = std::collections::HashSet::new();
    for file in files {
        let content = match fs::read_to_string(&file) {
            Ok(c) => c,
            Err(_) => continue,
        };

        for (index, line) in content.lines().enumerate() {
            if !line.contains("Ok(empty_json())") {
                continue;
            }

            let relative = file
                .strip_prefix(&routes_dir)
                .expect("file should live under routes_dir")
                .to_string_lossy()
                .replace('\\', "/");
            let entry = format!("{}:{}", relative, index + 1);
            hits.insert(entry.clone());
            if !allowlist.iter().any(|item| item == &entry) {
                violations.push(entry);
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Found Ok(empty_json()) matches missing from shell route allowlist:\n{}",
        violations.join("\n")
    );

    // A one-way allowlist rots: entries for code that was deleted or reflowed
    // stay behind and keep "covering" positions that no longer exist. Measured
    // 2026-09-19, 26 of 37 entries matched nothing (the file had not been touched
    // since the routes moved crates). Requiring an exact match both ways keeps
    // the file a description of reality instead of a growing pile of
    // exemptions — the same "no 'keep it just in case'" rule the repo applies to
    // code (AGENTS.md iron law 1).
    let stale: Vec<&String> = allowlist.iter().filter(|entry| !hits.contains(*entry)).collect();
    assert!(
        stale.is_empty(),
        "shell_routes_allowlist.txt lists entries that match no `Ok(empty_json())` site. Delete \
         them (or fix the line number if the site moved):\n{}",
        stale.iter().map(|entry| entry.as_str()).collect::<Vec<_>>().join("\n")
    );
}
