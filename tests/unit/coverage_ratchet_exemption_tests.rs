#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Guards for the coverage ratchet's `--non-unit-coverable` exemption
//! (`scripts/check_file_coverage.py` + `scripts/ci/non_unit_coverable_prefixes.txt`).
//!
//! ## Why this exists
//!
//! CI's coverage legs run `cargo llvm-cov --workspace --lib`, which never links or
//! executes a binary entry point. Every line of `src/bin/*.rs` and `src/main.rs`
//! is therefore structurally unreachable from that leg and the report records
//! 0.0% no matter what integration/e2e coverage exists. Those files are
//! grandfathered today because they are in `coverage_baseline.json`, but the
//! ratchet's new-file floors (30%, or 70% for a core prefix) would fail a
//! low-coverage file the moment it is renamed or moved and loses its baseline
//! entry — for a reason the coverage leg cannot fix.
//!
//! Two properties are pinned here:
//!
//! 1. **The list is deliberately tiny.** Widening the exemption (i.e. silently
//!    dropping coverage enforcement for more of the product) requires editing
//!    both the list and this guard.
//! 2. **The exemption is not a blanket skip.** It removes only the NEW-file
//!    floors; a baseline-known file is still checked for regression. That half is
//!    proven end-to-end here, not asserted from the source text.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Repository root (`CARGO_MANIFEST_DIR` is the root crate directory).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn script_path() -> PathBuf {
    repo_root().join("scripts/check_file_coverage.py")
}

fn list_path() -> PathBuf {
    repo_root().join("scripts/ci/non_unit_coverable_prefixes.txt")
}

/// Parse the exemption list exactly as `check_file_coverage.py::load_prefix_list`
/// does: one prefix per line, comments (`#`) and blank lines ignored.
fn list_entries() -> Vec<String> {
    let path = list_path();
    let source = fs::read_to_string(&path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

/// Run the real ratchet in a throwaway directory against a synthetic lcov report
/// and baseline, returning `(exit_code, stdout+stderr)`.
///
/// `records` are `(path, lines_found, lines_hit)`; `baseline` is
/// `(path, line_pct)`. Nothing outside the temp dir is written: the script's
/// `--save-baseline` defaults to `--baseline`, which lives in the temp dir.
fn run_ratchet(list: &Path, records: &[(&str, u32, u32)], baseline: &[(&str, f64)]) -> (i32, String) {
    let dir = tempfile::tempdir().expect("a temp dir must be creatable");
    let report = dir.path().join("probe.info");
    let baseline_file = dir.path().join("baseline.json");

    let mut lcov = String::new();
    for (path, lines_found, lines_hit) in records {
        lcov.push_str(&format!("SF:{path}\nLF:{lines_found}\nLH:{lines_hit}\nend_of_record\n"));
    }
    fs::write(&report, lcov).expect("synthetic lcov must be writable");

    let entries: Vec<String> =
        baseline.iter().map(|(path, pct)| format!("{{\"path\":\"{path}\",\"line_pct\":{pct}}}")).collect();
    fs::write(&baseline_file, format!("{{\"files\":[{}]}}\n", entries.join(",")))
        .expect("synthetic baseline must be writable");

    let output = Command::new("python3")
        .arg(script_path())
        .arg("--report")
        .arg(&report)
        .arg("--format")
        .arg("lcov")
        .arg("--baseline")
        .arg(&baseline_file)
        .arg("--global-floor")
        .arg("40")
        .arg("--new-file-floor")
        .arg("30")
        .arg("--non-unit-coverable")
        .arg(list)
        .current_dir(repo_root())
        .output()
        .expect("python3 must be runnable");
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    (output.status.code().unwrap_or(-1), combined)
}

/// (a) The exemption list is exactly `src/bin/` and `src/main.rs`.
///
/// Widening it (e.g. whitelisting a whole crate) changes what the coverage gate
/// enforces, so it must be an explicit edit of both the list and this test.
#[test]
fn non_unit_coverable_list_is_exactly_bin_and_main() {
    let path = list_path();
    assert!(path.is_file(), "exemption list not found at {}", path.display());

    assert_eq!(
        list_entries(),
        vec!["src/bin/".to_string(), "src/main.rs".to_string()],
        "the non-unit-coverable list must contain exactly `src/bin/` and `src/main.rs`; any other \
         entry silently drops coverage enforcement for product code that a unit coverage leg \
         could otherwise measure"
    );

    // The rationale must stay written down: an unexplained exemption list is how
    // it grows unnoticed.
    let source = fs::read_to_string(&path).expect("exemption list must be readable");
    assert!(
        source.lines().any(|line| line.trim_start().starts_with('#')),
        "the exemption list must keep a header comment explaining why these paths cannot be \
         covered by the unit coverage leg"
    );
}

/// (b) Green proof: a NEW file under an exempt prefix with 0% coverage passes.
#[test]
fn new_file_under_an_exempt_prefix_passes_the_ratchet() {
    let (code, output) =
        run_ratchet(&list_path(), &[("src/bin/coverage_ratchet_probe_exempt.rs", 10, 0)], &[("src/lib.rs", 50.0)]);
    assert_eq!(
        code, 0,
        "a new 0%-covered file under `src/bin/` must be exempt from the new-file floor (no unit \
         coverage leg can execute a binary entry point)\noutput:\n{output}"
    );
    assert!(
        output.contains("src/bin/coverage_ratchet_probe_exempt.rs"),
        "the exempt file must still be reported, not silently dropped\noutput:\n{output}"
    );
    assert!(
        output.contains("[EXEMPT]"),
        "the run must name the exemption explicitly so a skipped floor is visible\noutput:\n{output}"
    );
}

/// (b) Red proof: the same 0% NEW file outside the exempt prefixes fails.
///
/// This is the deliberate-violation half (`AGENTS.md` rule 8): it proves the
/// exemption is scoped, not a blanket relaxation of the ratchet.
#[test]
fn new_file_outside_the_exempt_prefixes_fails_the_ratchet() {
    let probe = "src/coverage_ratchet_probe_new.rs";
    assert!(
        !probe.starts_with("src/bin/") && probe != "src/main.rs",
        "the red-proof probe must not itself fall under an exempt prefix"
    );
    let (code, output) = run_ratchet(&list_path(), &[(probe, 10, 0)], &[("src/lib.rs", 50.0)]);
    assert_eq!(
        code, 1,
        "a new 0%-covered file outside the exempt prefixes must still fail the 30% new-file \
         floor; exit {code} means the exemption leaked\noutput:\n{output}"
    );
    assert!(
        output.contains(probe) && output.contains("[NEW]"),
        "the failure must name the new file and its NEW-file tag\noutput:\n{output}"
    );
}

/// The exemption must not hide a regression of a baseline-known file.
///
/// `src/bin/` is covered by the exemption, but a file already recorded in the
/// baseline is checked for regression *before* the exemption branch runs.
#[test]
fn exemption_does_not_hide_a_baseline_regression() {
    let probe = "src/bin/coverage_ratchet_probe_exempt.rs";
    let (code, output) = run_ratchet(&list_path(), &[(probe, 10, 0)], &[(probe, 50.0)]);
    assert_eq!(
        code, 1,
        "a baseline-known file under an exempt prefix must still fail when it regresses below its \
         recorded value; exit {code} means the exemption hid a regression\noutput:\n{output}"
    );
    assert!(
        output.contains("[TOUCHED]") && output.contains(probe),
        "the regression must be reported as a TOUCHED failure\noutput:\n{output}"
    );
}

/// Fail-closed parity with `--core-files`: a missing, empty, or entirely stale
/// exemption list must abort (exit 2), never silently apply to nothing.
#[test]
fn non_unit_coverable_list_fails_closed_when_missing_empty_or_stale() {
    let dir = tempfile::tempdir().expect("a temp dir must be creatable");
    let records = [("src/bin/coverage_ratchet_probe_exempt.rs", 10, 0)];
    let baseline = [("src/lib.rs", 50.0)];

    let missing = dir.path().join("does_not_exist.txt");
    let (code, output) = run_ratchet(&missing, &records, &baseline);
    assert_eq!(code, 2, "a missing exemption list must exit 2, got {code}\noutput:\n{output}");

    let empty = dir.path().join("empty.txt");
    fs::write(&empty, "# only a comment\n\n").expect("empty list must be writable");
    let (code, output) = run_ratchet(&empty, &records, &baseline);
    assert_eq!(code, 2, "an empty exemption list must exit 2, got {code}\noutput:\n{output}");

    // `synapse-services/src/` is crate-qualified from the wrong side: the
    // normalizer strips `src/` for workspace crates (`synapse-services/src/x.rs`
    // -> `synapse-services/x.rs`), so this prefix matches no file on disk. A
    // stale prefix means the guard is dead and must fail loudly.
    let stale = dir.path().join("stale.txt");
    fs::write(&stale, "src/bin/\nsrc/main.rs\nsynapse-services/src/\n").expect("stale list must be writable");
    let (code, output) = run_ratchet(&stale, &records, &baseline);
    assert_eq!(code, 2, "a stale exemption prefix must exit 2, got {code}\noutput:\n{output}");
    assert!(
        output.contains("Stale non-unit-coverable prefixes"),
        "the stale-prefix failure must say what is wrong\noutput:\n{output}"
    );
}
