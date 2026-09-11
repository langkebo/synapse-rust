//! Guard: user-supplied pagination `limit` parameters must be clamped on BOTH ends.
//!
//! Why this test exists
//! --------------------
//! Handlers parse `?limit=` into an `i64` and pass it straight to a storage
//! query. The dangerous shapes are:
//!
//! * `.min(1000)` only — no lower bound. A client-supplied `?limit=-1` then
//!   reaches PostgreSQL as a negative `LIMIT`, which errors with
//!   `LIMIT must not be negative` (verified against PostgreSQL 15). That error
//!   surfaces through `ApiError::database_with_context` as **HTTP 500** for what
//!   is ordinary malformed client input.
//! * `?limit=0` — the paginated queries fetch `limit + 1` rows to detect
//!   `has_more`, so 0 yields an empty page that still carries a
//!   `next_batch_token`, inviting an infinite client pagination loop.
//!
//! `.clamp(1, N)` avoids both. `room/management/metadata.rs` already did this;
//! `room/members.rs` and `room/management/query.rs` did not until this fix.
//!
//! The check is deliberately source-level: constructing an HTTP request for each
//! paginated handler requires a live DB and a full router, while the invariant
//! itself ("a parsed `limit` must be clamped") is visible in the AST-free text of
//! the handler.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Collect `(file, line_number, line)` for every line that parses a query
/// `limit` into an integer.
fn limit_parsing_lines() -> Vec<(String, usize, String)> {
    let root = project_root();
    let handler_roots = [root.join("src/web/routes/handlers"), root.join("src/web/routes")];

    let mut hits = Vec::new();
    let mut stack: Vec<PathBuf> = handler_roots.iter().filter(|p| p.exists()).cloned().collect();

    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            for (idx, line) in text.lines().enumerate() {
                // Only inspect code, not comments.
                let code = line.trim_start();
                if code.starts_with("//") {
                    continue;
                }
                // The pattern we care about: reading the "limit" query param and
                // parsing it to an integer type.
                if line.contains(r#".get("limit")"#) && line.contains("parse::<") {
                    let rel = path.strip_prefix(&root).unwrap_or(&path).display().to_string();
                    hits.push((rel, idx + 1, line.trim().to_string()));
                }
            }
        }
    }

    hits
}

#[test]
fn pagination_limit_is_clamped_on_both_ends() {
    let hits = limit_parsing_lines();
    assert!(
        !hits.is_empty(),
        "expected at least one handler parsing `limit`; the scan found none — \
         did the handler layout change? Update this guard rather than deleting it."
    );

    let mut offenders = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for (file, line_no, line) in &hits {
        // The same line can be reached twice when a file lives under more than
        // one scanned root; report each site once.
        if !seen.insert((file.clone(), *line_no)) {
            continue;
        }
        // Accept either an explicit clamp, or a documented allowance.
        let clamped = line.contains(".clamp(");
        if !clamped {
            offenders.push(format!("{file}:{line_no}: {line}"));
        }
    }

    assert!(
        offenders.is_empty(),
        "user-supplied `limit` must be clamped on BOTH ends, e.g. `.clamp(1, 1000)`. \
         A bare `.min(N)` lets `?limit=-1` reach PostgreSQL (negative LIMIT ⇒ error) \
         and lets `?limit=0` return an empty page with a next_batch_token. \
         Offending lines:\n  {}",
        offenders.join("\n  ")
    );
}

/// Sanity: the scan really does see the sites that were fixed, so a future
/// refactor that renames the query param cannot silently vacate this guard.
#[test]
fn guard_actually_sees_known_limit_sites() {
    let hits = limit_parsing_lines();
    let files: Vec<&str> = hits.iter().map(|(f, _, _)| f.as_str()).collect();
    let expected_any = ["src/web/routes/handlers/room/members.rs", "src/web/routes/handlers/room/management/query.rs"];
    let seen = expected_any.iter().any(|want| files.contains(want));
    assert!(
        seen,
        "guard did not see any of the known limit-parsing handlers {expected_any:?}; \
         scanned files were: {files:?}"
    );
}
