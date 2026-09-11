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

/// The root-crate lib step specifically must be workspace-wide, otherwise the
/// ~5400 workspace-crate lib tests stay unenforced.
#[test]
fn lib_test_step_covers_the_workspace() {
    let invocations = nextest_invocations();
    let lib_steps: Vec<&(String, String)> =
        invocations.iter().filter(|(_, cmd)| cmd.contains("--lib") && !cmd.contains("--test ")).collect();

    assert!(!lib_steps.is_empty(), "应存在一个 `--lib` 测试步骤；若已改名/迁移，请更新本守卫");
    for (loc, cmd) in lib_steps {
        assert!(
            cmd.contains("--workspace"),
            "{loc} 的 `--lib` 步骤缺少 `--workspace`：{cmd}\n\
             缺少它时只测试根包 lib（687 个），workspace crate 的 lib 测试（合计约 5400 个）全部不执行。"
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
