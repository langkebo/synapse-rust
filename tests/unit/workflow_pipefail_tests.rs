//! Guard: a workflow `run:` block that pipes into `tee` must enable `pipefail`.
//!
//! ## Why this file exists
//!
//! Bash pipelines report the exit status of their **last** command by default.
//! `cargo bench … | tee benchmark.txt` therefore exits 0 whenever `tee`
//! succeeds, no matter how the real command died, and the CI step reports
//! success with a truncated or empty log. Commit `dee46f6e` found **6** such
//! steps (4 in `benchmark.yml`, plus `ci.yml`, `db-migration-gate.yml`,
//! `e2ee-interop.yml`): a compile error or panic inside a benchmarked binary
//! was indistinguishable from a clean run, and the gates that consume the
//! captured output (`check_pagination_benchmark.py`, `compute_perf_gate.sh`,
//! the e2ee interop smoke checks) then judged missing data instead of a failure
//! — the same false-green pattern as the empty `cargo test --doc` gate.
//!
//! Fixing the six sites once is not enough: the next `| tee` someone adds
//! reintroduces the hole silently. This scanner makes the fix自证 — it parses
//! every `run:` block in `.github/workflows/*.yml` and requires the literal
//! `set -o pipefail` (or `set -euo pipefail`) before the first pipeline into
//! `tee`. It counts what it inspected and fails if the count drops, so a
//! scanner that silently stops matching anything cannot pass.
//!
//! Deliberately **not** flagged: steps marked `continue-on-error: true` whose
//! pipeline is inside their own error-tolerant expression — but those steps
//! still carry `set -o pipefail` so the reported status is the command's, not
//! `tee`'s.

use std::fs;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// True when the (comment-stripped) line contains a shell pipeline into `tee`,
/// i.e. a single `|` (not `||`, not `|&`) followed by the `tee` command word.
fn pipes_into_tee(line: &str) -> bool {
    // Comments never run; dropping them also keeps the explanatory comments
    // that mention `| tee` from being mistaken for the real thing.
    let code = line.split('#').next().unwrap_or("");
    let chars: Vec<char> = code.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if *c != '|' {
            continue;
        }
        if i > 0 && chars[i - 1] == '|' {
            continue; // `||`
        }
        if chars.get(i + 1) == Some(&'|') {
            continue; // `||` seen from the left operand
        }
        let tail: String = chars[i + 1..].iter().collect();
        let tail = tail.trim_start();
        if tail == "tee" || tail.starts_with("tee ") || tail.starts_with("tee\t") {
            return true;
        }
    }
    false
}

/// True when the line is the literal shell option that turns on pipeline exit
/// propagation. Matching the statement (not the word) matters: the repo's
/// explanatory comments contain the word `pipefail` right above several of
/// these blocks, and a substring check would let a missing `set` line pass.
fn enables_pipefail(line: &str) -> bool {
    let t = line.trim();
    t.starts_with("set -o pipefail")
        || t.starts_with("set -euo pipefail")
        || t.starts_with("set -eo pipefail")
        || t.starts_with("set -oue pipefail")
}

/// Extracts every literal `run: |` block body: lines indented deeper than the
/// `run:` key, paired with their 1-based line number in the file.
fn run_blocks(text: &str) -> Vec<Vec<(usize, String)>> {
    let lines: Vec<&str> = text.lines().collect();
    let mut blocks = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if trimmed != "run: |" {
            i += 1;
            continue;
        }
        let key_indent = line.len() - trimmed.len();
        let mut body: Vec<(usize, String)> = Vec::new();
        let mut j = i + 1;
        while j < lines.len() {
            let current = lines[j];
            if current.trim().is_empty() {
                body.push((j + 1, String::new()));
                j += 1;
                continue;
            }
            let indent = current.len() - current.trim_start().len();
            if indent <= key_indent {
                break;
            }
            body.push((j + 1, current.to_string()));
            j += 1;
        }
        blocks.push(body);
        i = j;
    }
    blocks
}

#[test]
fn every_run_block_that_pipes_into_tee_enables_pipefail() {
    let dir = repo_root().join(".github/workflows");
    let entries = fs::read_dir(&dir).unwrap_or_else(|e| panic!("expected {dir:?} to be readable: {e}"));

    let mut inspected = 0usize;
    let mut offenders: Vec<String> = Vec::new();

    for entry in entries {
        let path = entry.expect("workflow dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("yml") {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("expected {path:?} to be readable: {e}"));
        for block in run_blocks(&text) {
            let tee_lines: Vec<usize> =
                block.iter().filter(|(_, line)| pipes_into_tee(line)).map(|(n, _)| *n).collect();
            let Some(first_tee) = tee_lines.first().copied() else {
                continue;
            };
            inspected += 1;
            let guarded = block.iter().any(|(n, line)| *n < first_tee && enables_pipefail(line));
            if !guarded {
                offenders.push(format!(
                    "{}:{} — pipes into `tee` on line {} with no `set -o pipefail` earlier in the block",
                    path.display(),
                    first_tee,
                    tee_lines.iter().map(usize::to_string).collect::<Vec<_>>().join(", ")
                ));
            }
        }
    }

    assert!(
        inspected >= 6,
        "the scanner inspected only {inspected} `| tee` run-blocks; the known set is 6 \
         (benchmark.yml, ci.yml, db-migration-gate.yml, 3x e2ee-interop.yml). A scanner that \
         finds nothing proves nothing — check the `run: |` extraction before trusting a pass."
    );
    assert!(
        offenders.is_empty(),
        "these CI steps swallow a failing command's exit status by piping into `tee` without \
         `set -o pipefail`, so the step reports success while the real command failed:\n  {}",
        offenders.join("\n  ")
    );
}

/// The two halves of the mechanism, pinned locally so the guard's premise is
/// never taken on faith: without `pipefail` the pipeline reports `tee`'s 0 for
/// a command that failed; with it the pipeline reports the command's status.
#[test]
fn pipefail_is_what_turns_a_swallowed_failure_into_a_failed_step() {
    use std::process::Command;

    let pipeline_status = |script: &str| {
        let out = Command::new("bash").arg("-c").arg(script).output().expect("bash must be runnable");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };

    // `echo $?` prints the status the CI step would report after the pipeline.
    assert_eq!(pipeline_status("false | tee /dev/null >/dev/null 2>&1; echo $?"), "0");
    assert_eq!(pipeline_status("set -o pipefail; false | tee /dev/null >/dev/null 2>&1; echo $?"), "1");
    assert_eq!(pipeline_status("set -o pipefail; true | tee /dev/null >/dev/null 2>&1; echo $?"), "0");
}

// =============================================================================
// Guard 2: `run:` **folded** scalars must not use deeper-indented continuation
// lines — YAML keeps the newline before them, so one intended command becomes
// several shell commands.
// =============================================================================
//
// Measured on `main` (run 35494142003): `db-migration-gate.yml` wrote six smoke
// steps as
//
//     run: >-
//       bash scripts/ci/require_tests_ran.sh
//       cargo test --locked
//         --features test-utils,privacy-ext,…
//         --test integration thread_storage_tests_migrated -- --test-threads=1
//
// The two deeper-indented lines keep their preceding line breaks, so the shell
// ran `bash scripts/ci/require_tests_ran.sh cargo test --locked` **without any
// features** and then tried to execute `--features …` as its own command. The
// feature-less `cargo test --locked` compiles the lib test target, which needs
// `synapse_test_utils`, so the job died with
// `error[E0432]: unresolved import synapse_test_utils` at
// `src/common/config/tests.rs:9` — a hard-to-attribute failure caused purely by
// YAML indentation. The six sites are now `run: |` with `\` continuations; this
// guard pins the pattern so the next folded multi-line `run:` cannot silently
// reintroduce it.

/// Returns `(line_no, content)` for every line of a `run:` **folded** scalar
/// (`run: >`, `run: >-`, `run: >+`) that is indented deeper than the scalar's
/// base indent (the first non-empty line), i.e. every line YAML will keep a
/// line break before.
fn folded_run_deeper_continuations(text: &str) -> Vec<(usize, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut offenders = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        // Accept both `run: >-` on its own line and the inline `- run: >-` form.
        let trimmed = line.trim_start().trim_start_matches("- ");
        if !matches!(trimmed, "run: >" | "run: >-" | "run: >+") {
            i += 1;
            continue;
        }
        let key_indent = line.len() - trimmed.len();
        let mut block: Vec<(usize, String, usize)> = Vec::new();
        let mut j = i + 1;
        while j < lines.len() {
            let current = lines[j];
            if current.trim().is_empty() {
                j += 1;
                continue;
            }
            let indent = current.len() - current.trim_start().len();
            if indent <= key_indent {
                break;
            }
            block.push((j + 1, current.trim_start().to_string(), indent));
            j += 1;
        }
        if let Some(base) = block.iter().map(|(_, _, indent)| *indent).min() {
            for (line_no, content, indent) in &block {
                if *indent > base {
                    offenders.push((*line_no, content.clone()));
                }
            }
        }
        i = j;
    }
    offenders
}

#[test]
fn no_folded_run_scalar_uses_deeper_indented_continuation_lines() {
    // Mechanism self-proof (the real tree currently has zero folded `run:`
    // scalars, so a scan-only assertion would be vacuous).
    let broken = "jobs:\n  a:\n    steps:\n      - run: >-\n          cmd --flag\n            --deeper\n";
    let offenders = folded_run_deeper_continuations(broken);
    assert_eq!(
        offenders.len(),
        1,
        "the scanner must catch a deeper-indented continuation line of a folded run scalar: {offenders:?}"
    );
    assert_eq!(offenders[0].1, "--deeper");

    // Same-indent continuation lines fold into ONE command → correct, not flagged.
    let folded_ok = "jobs:\n  a:\n    steps:\n      - run: >-\n          cmd --flag\n          --same\n";
    assert!(
        folded_run_deeper_continuations(folded_ok).is_empty(),
        "a folded scalar with equally-indented lines folds to a single command and must pass"
    );

    // A literal block with backslash continuations is the recommended form.
    let literal_ok = "jobs:\n  a:\n    steps:\n      - run: |\n          cmd --flag \\\n            --deeper\n";
    assert!(
        folded_run_deeper_continuations(literal_ok).is_empty(),
        "deeper indentation inside a `run: |` block is fine (the backslash joins the lines)"
    );

    // Real tree.
    let dir = repo_root().join(".github/workflows");
    let mut entries: Vec<PathBuf> = fs::read_dir(&dir)
        .expect(".github/workflows must be readable")
        .map(|entry| entry.expect("readable dir entry").path())
        .filter(|path| matches!(path.extension().and_then(|ext| ext.to_str()), Some("yml" | "yaml")))
        .collect();
    entries.sort();
    assert!(!entries.is_empty(), "no workflow files found to scan");

    let mut real: Vec<String> = Vec::new();
    for path in entries {
        let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path:?} must be readable: {e}"));
        for (line_no, content) in folded_run_deeper_continuations(&text) {
            real.push(format!("{}:{line_no} — {content}", path.display()));
        }
    }
    assert!(
        real.is_empty(),
        "these `run:` folded scalars have deeper-indented continuation lines, so YAML keeps a \
         line break and the shell runs them as separate commands (a feature-less `cargo test` \
         was how run 35494142003 failed with E0432):\n  {}",
        real.join("\n  ")
    );
}
