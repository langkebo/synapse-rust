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

/// 基础镜像的 digest **只能写一处**：`docker/Dockerfile` 的 ARG。
///
/// 2026-09-22 之前，`docker-security-scan.yml` 的 `Digest Pin Integrity` job 把三个
/// digest 抄了一份写死在 workflow 里。后果与"门禁不会红"同型：Dockerfile 升级 pin
/// 之后，那个 job 仍在验证**旧** digest，于是它既不能说"当前 pin 有效"，也不会因为
/// pin 被换掉而失败。修法是唯一真相源 + `scripts/ci/read_base_image_pins.sh`（两个 job
/// 共用同一份读取逻辑）。
///
/// **红证明**：往 workflow 里粘回一行 `@sha256:…` → FAILED；把 Dockerfile 的某个 ARG
/// 改成 `:latest`（去掉 digest）→ FAILED。
#[test]
fn base_image_digests_are_read_from_the_dockerfile_not_copied_into_workflows() {
    let root = repo_root();
    let workflow = fs::read_to_string(root.join(".github/workflows/docker-security-scan.yml"))
        .expect("read docker-security-scan.yml");
    let dockerfile = fs::read_to_string(root.join("docker/Dockerfile")).expect("read docker/Dockerfile");
    let helper = fs::read_to_string(root.join("scripts/ci/read_base_image_pins.sh"))
        .expect("read scripts/ci/read_base_image_pins.sh");

    // ① workflow 里不得再出现 digest 字面量（注释里也不行：注释同样会腐烂）。
    let digests: Vec<&str> = workflow.lines().filter(|l| l.contains("sha256:")).collect();
    assert!(
        digests.is_empty(),
        "docker-security-scan.yml 不得内联 digest（唯一真相源是 docker/Dockerfile 的 ARG）：\n{}",
        digests.join("\n")
    );

    // ② 两个 job 都必须通过同一个脚本读取（禁止各写一份 sed）。
    let helper_calls = workflow.matches("scripts/ci/read_base_image_pins.sh").count();
    assert!(
        helper_calls >= 2,
        "`base-image-scan` 与 `digest-pin-check` 都必须调用 read_base_image_pins.sh（实际 {helper_calls} 处）"
    );

    // ③ Dockerfile 必须真的定义这三个 ARG 且带 digest —— 否则上面的断言全是空转。
    for arg in ["RUNTIME_BASE_IMAGE", "DEBIAN_BASE_IMAGE", "RUST_BUILDER_IMAGE"] {
        let line = dockerfile
            .lines()
            .find(|l| l.trim_start().starts_with(&format!("ARG {arg}=")))
            .unwrap_or_else(|| panic!("docker/Dockerfile must define ARG {arg}"));
        assert!(line.contains("@sha256:"), "ARG {arg} must stay digest-pinned: {line}");
    }

    // ④ 读取逻辑本身必须能在失败时响亮退出（否则空值会被当成"没有基础镜像"）。
    assert!(
        helper.contains("not digest-pinned") && helper.contains("exit 2"),
        "read_base_image_pins.sh 必须在 ARG 缺失/未 pin 时 exit 2（响亮失败），而不是打印空值"
    );
    assert!(
        helper.contains("|| exit 2"),
        "多处读取时命令替换的失败必须显式传播（`ref=\"$(read_pin …)\" || exit 2`），\
         否则脚本会打印空值并以 0 退出 —— 正是\"门禁不会红\"的形态"
    );
}

/// 三个 pinned 基础镜像都必须被 Trivy 扫到，且"报告不阻断"只能有一个（builder）。
///
/// 缺口（P0-4）：`trivy-scan` 只扫 `--target tools` 的出货镜像，三个基础镜像本身从未扫过。
/// 其中 distroless 是 `runtime-distroless` 的根、debian 的库经 `runtime-libs` 复制进出货
/// 镜像 ⇒ 两者必须阻断；builder 只在构建期存在 ⇒ 允许 report-only，但必须**显式**且是唯一
/// 一个（否则"scan 步骤存在"会掩盖"其实不阻断"）。
///
/// **红证明**：把 distroless 扫描的 `exit-code` 改成 0 → FAILED；删掉 builder 扫描步骤 →
/// FAILED；把三个 `image-ref` 都换成同一个 ARG → FAILED。
#[test]
fn every_pinned_base_image_is_scanned_and_report_only_is_an_explicit_exception() {
    let root = repo_root();
    let workflow = fs::read_to_string(root.join(".github/workflows/docker-security-scan.yml"))
        .expect("read docker-security-scan.yml");

    // 只看 `base-image-scan` 这个 job 的正文（到下一个顶层 job 定义为止）。
    let job_start = workflow.find("\n  base-image-scan:").expect("the workflow must define a `base-image-scan` job");
    let job = &workflow[job_start..];
    let job_body = match job[1..].find("\n  # ──").map(|offset| offset + 1) {
        Some(next_job) => &job[..next_job],
        None => job,
    };

    let mut scanned: Vec<(String, String, String)> = Vec::new(); // (step, image-ref, exit-code)
    for step in job_body.split("- name: ").skip(1) {
        let name = step.lines().next().unwrap_or_default().trim().to_string();
        if !step.contains("aquasecurity/trivy-action") {
            continue;
        }
        let pick = |key: &str| -> String {
            step.lines()
                .map(str::trim)
                .find(|l| l.starts_with(key) && !l.starts_with('#'))
                .map(|l| l[key.len()..].trim().trim_matches('\'').to_string())
                .unwrap_or_default()
        };
        scanned.push((name, pick("image-ref:"), pick("exit-code:")));
    }

    assert_eq!(
        scanned.len(),
        3,
        "base-image-scan 必须恰好扫三个 pinned 基础镜像（distroless / debian / builder）；实际扫描：{scanned:?}"
    );

    for arg in ["RUNTIME_BASE_IMAGE", "DEBIAN_BASE_IMAGE", "RUST_BUILDER_IMAGE"] {
        let hits = scanned.iter().filter(|(_, image_ref, _)| image_ref.contains(arg)).count();
        assert_eq!(
            hits, 1,
            "ARG {arg} 必须被扫且只扫一次（用 `steps.pins.outputs.{arg}` 引用）；实际扫描：{scanned:?}"
        );
    }

    let report_only: Vec<&(String, String, String)> =
        scanned.iter().filter(|(_, _, exit_code)| exit_code == "0").collect();
    assert_eq!(
        report_only.len(),
        1,
        "只允许**一个** report-only 扫描（builder，构建期镜像）；实际：{report_only:?} / 全部 {scanned:?}"
    );
    let (builder_step, builder_ref, _) = report_only[0];
    assert!(
        builder_ref.contains("RUST_BUILDER_IMAGE"),
        "report-only 的必须是 builder（构建期镜像，产物才是运行镜像），而不是 {builder_ref}"
    );
    assert!(
        builder_step.contains("report-only") && job_body.contains("build-time only"),
        "builder 那条 report-only 扫描必须在步骤名/注释里显式写明理由，否则下一个人会以为它阻断：{builder_step}"
    );

    for (step, image_ref, exit_code) in &scanned {
        if image_ref.contains("RUST_BUILDER_IMAGE") {
            continue;
        }
        assert_eq!(exit_code, "1", "`{step}` 扫的是出货路径上的基础镜像（{image_ref}），必须阻断（exit-code 1）");
    }
}
