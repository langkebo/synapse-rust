//! Guards for `.github/workflows/*.yml` footguns that make a CI step report
//! success — or a misleading failure — while the work it claims to do did not
//! happen at all.
//!
//! * **Guard 1** — a `run:` block that pipes into `tee` must enable `pipefail`.
//! * **Guard 2** — a folded `run: >` scalar must not use deeper-indented
//!   continuation lines (YAML keeps a line break there, so one intended command
//!   becomes several shell commands).
//! * **Guard 3** — every `docker build` in a workflow must name its Dockerfile
//!   with `-f` / `--file`.
//!
//! Guard 1's rationale follows; Guards 2 and 3 document themselves at their
//! sections.
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

// =============================================================================
// Guard 3: every `docker build` in a workflow must pass `-f`/`--file`.
// =============================================================================
//
// `docker build` defaults to `<context>/Dockerfile`. In this repo the build
// context is the **repository root** — the Dockerfile's `COPY Cargo.toml
// Cargo.lock ./` and friends are root-relative — while the Dockerfile itself
// lives in `docker/`. A bare `docker build … .` therefore looks for
// `./Dockerfile`, which does not exist.
//
// Measured on `main` (run 35497543078): the Trivy step in
// `docker-security-scan.yml` died within 5 s with
// `failed to read dockerfile: open Dockerfile: no such file or directory`, so
// `trivy-results.sarif` was never produced and the SARIF upload that follows
// then reported `Path does not exist`. The image was never scanned, and the job
// looked *broken* rather than *unscanned*.
//
// Every other call site in the repo already passes an explicit path
// (`Makefile`, `build-and-push.sh`, `docker/deploy/deploy.sh`,
// `docker/docker-compose.yml`, `scripts/ci/run_complement_tests.sh`); the
// workflow was the only one that did not. Fixing it once is not enough — the
// next `docker build` added to a workflow reintroduces the same silent
// unscanned-image failure, so this guard makes the fix self-proving.

/// Drops a trailing `# …` comment.
///
/// A `#` only starts a comment at the start of a line or after whitespace;
/// inside a word (`--build-arg FOO=a#b`) it is part of the argument. The
/// repo's own explanatory comments mention `docker build`, so not stripping
/// them would flag prose.
fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'#' && (i == 0 || bytes[i - 1].is_ascii_whitespace()) {
            return &line[..i];
        }
    }
    line
}

/// Folds a `run:` block into logical commands: `\`-continued lines join into
/// one command, comments and blank lines are dropped.
///
/// Joining matters here because the build command in
/// `docker-security-scan.yml` is written across six `\`-continued lines — a
/// per-line scan would see neither the verb nor the flags together.
fn logical_commands(block: &[(usize, String)]) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = Vec::new();
    let mut start: Option<usize> = None;
    let mut acc = String::new();

    for (line_no, raw) in block {
        let code = strip_comment(raw).trim();
        if code.is_empty() {
            continue;
        }
        if start.is_none() {
            start = Some(*line_no);
        }
        let continues = code.ends_with('\\');
        let piece = code.trim_end_matches('\\').trim();
        if !acc.is_empty() {
            acc.push(' ');
        }
        acc.push_str(piece);
        if !continues {
            out.push((start.take().unwrap_or(*line_no), std::mem::take(&mut acc)));
        }
    }
    if let Some(line_no) = start {
        out.push((line_no, acc));
    }
    out
}

/// True when `command` invokes `docker build` / `docker buildx build`.
///
/// The `docker` word only counts as the verb when it starts the command or
/// follows a shell operator / wrapper, so prose such as `echo docker build .`
/// (and the explanatory comments, once stripped) is not mistaken for a build.
fn is_docker_build(command: &str) -> bool {
    let tokens: Vec<&str> = command.split_whitespace().collect();
    for (i, token) in tokens.iter().enumerate() {
        if *token != "docker" {
            continue;
        }
        let preceded_by_operator = match i.checked_sub(1).and_then(|p| tokens.get(p)) {
            None => true,
            Some(prev) => matches!(*prev, "&&" | "||" | ";" | "|" | "!" | "sudo" | "command" | "exec" | "time"),
        };
        if !preceded_by_operator {
            continue;
        }
        return matches!(
            (tokens.get(i + 1), tokens.get(i + 2)),
            (Some(&"build"), _) | (Some(&"buildx"), Some(&"build"))
        );
    }
    false
}

/// True when the command names the Dockerfile explicitly.
fn names_the_dockerfile(command: &str) -> bool {
    command
        .split_whitespace()
        .any(|token| token == "-f" || token == "--file" || token.starts_with("-f=") || token.starts_with("--file="))
}

#[test]
fn every_workflow_docker_build_names_its_dockerfile() {
    // Mechanism self-proof first. The real tree holds exactly one such command,
    // so a scan-only assertion would be thin: these fixtures pin both the
    // detection and the exemption, and they fail if `is_docker_build` /
    // `names_the_dockerfile` stop discriminating.
    assert!(
        is_docker_build("docker build -t x ."),
        "a bare `docker build` with the default Dockerfile path must be detected"
    );
    assert!(
        !names_the_dockerfile("docker build -t x ."),
        "a bare `docker build` must be reported as naming no Dockerfile"
    );
    for ok in [
        "docker build -f docker/Dockerfile -t x .",
        "docker build --file docker/Dockerfile -t x .",
        "docker build --file=docker/Dockerfile -t x .",
        "docker build -f=docker/Dockerfile -t x .",
        "docker buildx build -f docker/Dockerfile --push .",
        "sudo docker build -f docker/Dockerfile .",
        "cd repo && docker build -f docker/Dockerfile .",
    ] {
        assert!(is_docker_build(ok), "`{ok}` invokes a docker build and must be detected");
        assert!(names_the_dockerfile(ok), "`{ok}` names its Dockerfile and must not be flagged");
    }
    assert!(
        is_docker_build("docker buildx build --push ."),
        "`docker buildx build` shares the Dockerfile-path default and must be detected too"
    );
    for not_a_build in ["docker compose build", "docker image inspect x", "echo docker build ."] {
        assert!(
            !is_docker_build(not_a_build),
            "`{not_a_build}` does not invoke a docker build and must not be flagged"
        );
    }

    // Line continuation folding: the real step spans six `\`-continued lines.
    let block = vec![
        (100, "          docker build \\".to_string()),
        (101, "            -f docker/Dockerfile \\".to_string()),
        (102, "            --target tools \\".to_string()),
        (103, "            .".to_string()),
    ];
    let commands = logical_commands(&block);
    assert_eq!(commands.len(), 1, "a `\\`-continued command must fold into one logical command: {commands:?}");
    assert_eq!(commands[0].0, 100, "the folded command must keep its first line number");
    assert!(names_the_dockerfile(&commands[0].1), "the folded command must still carry the `-f` flag");

    let unguarded = vec![(7, "          docker build -t x .".to_string())];
    assert!(
        logical_commands(&unguarded)
            .iter()
            .any(|(_, command)| is_docker_build(command) && !names_the_dockerfile(command)),
        "a one-line unguarded build must fold and be flagged"
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

    let mut inspected = 0usize;
    let mut offenders: Vec<String> = Vec::new();
    for path in entries {
        let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path:?} must be readable: {e}"));
        for block in run_blocks(&text) {
            for (line_no, command) in logical_commands(&block) {
                if !is_docker_build(&command) {
                    continue;
                }
                inspected += 1;
                if !names_the_dockerfile(&command) {
                    offenders.push(format!("{}:{line_no} — {command}", path.display()));
                }
            }
        }
    }

    assert!(
        inspected >= 1,
        "the scanner inspected no `docker build` in any workflow. The known set is 1 \
         (`docker-security-scan.yml` → `Build image for scan`); if that step is genuinely gone, \
         lower this floor deliberately rather than letting the guard pass vacuously — a scanner \
         that matches nothing proves nothing."
    );
    assert!(
        offenders.is_empty(),
        "these workflow `docker build` commands omit `-f`/`--file`, so docker looks for \
         `<context>/Dockerfile` (i.e. `./Dockerfile` for the repo-root context) and dies with \
         `failed to read dockerfile` — the image is never built or scanned while the job looks \
         merely broken (measured: main run 35497543078):\n  {}",
        offenders.join("\n  ")
    );
}
