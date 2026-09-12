//! Guard: CI must not silently scope test runs to the root package.
//!
//! ## Why this file exists
//!
//! Every `cargo nextest run` step in `.github/workflows/` was written without
//! `-p <crate>` or `--workspace`. Cargo defaults such a command to the **current
//! package** — the root crate `synapse-rust`. The six workspace members
//! (`synapse-common`, `synapse-cache`, `synapse-storage`, `synapse-e2ee`,
//! `synapse-federation`, `synapse-services`) were therefore never tested by CI.
//!
//! Measured with `--lib --all-features`:
//!
//! | scope | tests |
//! |---|---|
//! | root package only (what CI ran) | **687** |
//! | `--workspace` | **6118** |
//!
//! `synapse-common` alone holds 862 lib tests. The blind spot covered precisely
//! the layers this audit changed most (config / error / rate-limit leaf types in
//! `synapse-common`, persistence in `synapse-storage`, business logic in
//! `synapse-services`) — and it meant several guards added during the audit lived
//! in crates CI never compiled tests for.
//!
//! This is the same shape as the other gate defects found in this audit: the
//! command *looks* like it runs the suite, and its scope silently excludes most
//! of it.
//!
//! ## Activation status (read this before changing)
//!
//! Both scope-asserting tests are **active** as of 2026-09-12: the CI lib step
//! is now `--workspace` (with the known-flaky `media::tests` suite excluded to a
//! separate non-blocking step). `synapse-storage` was migrated to schema-per-test
//! isolation, so the 5464-statement replay cost that previously blocked this is
//! gone.
//!
//! ~~When the `--isolated` migration lands, remove the `#[ignore]` attributes~~
//! and add `--workspace` to the `--lib` step in `.github/workflows/ci.yml`.**
//! See `docs/audit/P5_workspace_test_isolation_2026-09-11.md`.
//!
//! ## What this test enforces
//!
//! Any workflow step invoking `cargo nextest run` **without** `--test <target>`
//! must state its scope explicitly (`--workspace` or `-p`). Steps targeting an
//! explicit `--test <name>` are exempt: those targets are root-package test
//! binaries (`tests/unit`, `tests/integration`, `tests/e2e`) by construction, and
//! naming them is already an explicit scope.

use std::fs;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn workflows_dir() -> PathBuf {
    repo_root().join(".github/workflows")
}

/// Every `cargo nextest run ...` line across all workflows.
fn nextest_invocations() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for entry in fs::read_dir(workflows_dir()).expect("workflows dir must be readable").flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        if !(name.ends_with(".yml") || name.ends_with(".yaml")) {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else { continue };
        for (idx, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with('#') {
                continue;
            }
            if let Some(pos) = trimmed.find("cargo nextest run") {
                out.push((format!("{name}:{}", idx + 1), trimmed[pos..].to_string()));
            }
        }
    }
    out
}

/// A lib/test-run step must declare its scope; a `--test <target>` step is
/// already explicit.
///
/// `#[ignore]`d on purpose (user-approved trade-off): this assertion currently
/// fails because the CI scope gap is **real but not yet fixable** — widening to
/// `--workspace` pulls in `synapse-storage`'s per-test baseline replay
/// (~40–134 s/test), which needs the test-isolation consolidation first.
/// Leaving it failing would break the unit target for everyone.
///
/// **Remove the `#[ignore]` in the same commit that adds `--workspace` to the
/// `--lib` step in `ci.yml`.** See
/// `docs/audit/P5_workspace_test_isolation_2026-09-11.md`.
#[test]
fn nextest_invocations_declare_their_scope() {
    let invocations = nextest_invocations();
    assert!(!invocations.is_empty(), "未在 workflows 中找到任何 `cargo nextest run` —— 若测试入口已迁移，请更新本守卫");

    let mut offenders = Vec::new();
    for (loc, cmd) in &invocations {
        let targets_test_binary = cmd.contains("--test ");
        let declares_scope =
            cmd.contains("--workspace") || cmd.contains(" -p ") || cmd.starts_with("cargo nextest run -p");
        if !targets_test_binary && !declares_scope {
            offenders.push(format!("{loc}: {cmd}"));
        }
    }

    assert!(
        offenders.is_empty(),
        "以下 nextest 调用既未指定 `--test <target>` 也未声明作用域（--workspace / -p），\
         因此只会测试**根包**，workspace crate 的测试被静默跳过。\
         实测：根包 lib 687 个 vs --workspace 6118 个。\n请补 `--workspace`（或显式 `-p`）：\n{}",
        offenders.join("\n")
    );
}

/// The lib step specifically must be workspace-wide, otherwise the
/// ~5400 workspace-crate lib tests stay unenforced.
#[test]
fn lib_test_step_covers_the_workspace() {
    let invocations = nextest_invocations();
    let lib_steps: Vec<&(String, String)> =
        invocations.iter().filter(|(_, cmd)| cmd.contains("--lib") && !cmd.contains("--test ")).collect();

    assert!(!lib_steps.is_empty(), "应存在一个 `--lib` 测试步骤；若已改名/迁移，请更新本守卫");
    for (loc, cmd) in lib_steps {
        // `--workspace` is the general fix. `-p <crate>` also declares an explicit
        // scope and is correct for a step that deliberately targets one crate
        // (e.g. the known-flaky media suite kept out of the main gate) — requiring
        // `--workspace` there would be wrong, not safer.
        let declares_scope = cmd.contains("--workspace") || cmd.contains(" -p ");
        assert!(
            declares_scope,
            "{loc} 的 `--lib` 步骤既无 `--workspace` 也无 `-p <crate>`：{cmd}\n\
             缺省时只测试根包 lib（687 个），workspace crate 的 lib 测试（合计约 5400 个）全部不执行。"
        );
    }
}

/// Sanity: the workflows directory is the one we expect.
#[test]
fn workflow_files_are_present() {
    let count = fs::read_dir(workflows_dir())
        .expect("workflows dir")
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "yml" || x == "yaml"))
        .count();
    assert!(count >= 5, "workflows 目录应包含多个 workflow，实际 {count}；路径是否变了？");
}

// ── the media exemption must be self-cleaning ───────────────────────────────
//
// The main lib gate excludes `synapse-services::media::tests` (13 tests) because
// that suite has in-process cross-talk (measured: 0/1/3/3 failures across four runs
// of the same command, with the failing set drifting). A temporary exemption like
// that rots into a permanent coverage reduction unless something forces its
// removal — so `scripts/ci/check_media_exemption_still_needed.sh` fails once the
// suite actually passes.
//
// This test locks the guard's own contract, so the guard cannot silently rot
// either. It drives the script through its documented `MEDIA_TEST_CMD` override,
// so no database is required.

fn media_guard_output(cmd_override: &str) -> std::process::Output {
    let root = repo_root();
    std::process::Command::new("bash")
        .arg(root.join("scripts/ci/check_media_exemption_still_needed.sh"))
        .env("MEDIA_TEST_CMD", cmd_override)
        .env("RUNS", "2")
        .current_dir(&root)
        .output()
        .expect("failed to run the media-exemption guard script")
}

#[test]
fn media_exemption_guard_keeps_the_exemption_while_the_suite_fails() {
    // At least one failing run ⇒ the exemption is still justified ⇒ exit 0.
    let out = media_guard_output("false");
    assert!(
        out.status.success(),
        "守卫在'套件仍失败'时必须 exit 0（豁免仍必要），实际 {}\nstdout:\n{}\nstderr:\n{}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn media_exemption_guard_demands_removal_once_the_suite_passes() {
    // Every run passes ⇒ the exemption is no longer justified ⇒ exit 1 + the
    // three concrete steps to retract it.
    let out = media_guard_output("true");
    assert!(
        !out.status.success(),
        "守卫在'套件全部通过'时必须 exit 1 要求收回豁免；否则豁免会无声永续。\nstdout:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    for needle in ["移除该排除模式", "删除 'Check media exemption is still necessary", "TESTING.md"] {
        assert!(stderr.contains(needle), "守卫的收回指引必须包含 {needle:?}，实际 stderr:\n{stderr}");
    }
}

#[test]
fn media_exemption_is_wired_without_continue_on_error() {
    // The guard's whole point is that a red means "retract the exemption". Wrapping
    // it in `continue-on-error` would hide exactly that signal.
    let root = repo_root();
    let src = fs::read_to_string(root.join(".github/workflows/ci.yml")).expect("ci.yml readable");
    let guard_step =
        src.split("- name: Check media exemption is still necessary").nth(1).expect("应存在 media 豁免守卫步骤");
    let body: String = guard_step.lines().take(20).collect::<Vec<_>>().join("\n");
    assert!(
        !body.contains("continue-on-error"),
        "media 豁免守卫步骤不得设 continue-on-error —— 那会让'该收回豁免'的红被吞掉：\n{body}"
    );
    assert!(
        body.contains("check_media_exemption_still_needed.sh"),
        "该步骤必须调用守卫脚本，而不是直接跑套件（直接跑会把随机失败当门禁）：\n{body}"
    );
}
