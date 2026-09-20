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

// ── the media exemption must not silently come back ─────────────────────────
//
// The main lib gate used to exclude `synapse-services::media::tests` (13 tests)
// because that suite had in-process cross-talk (measured: 0/1/3/3 failures across
// four runs of the same command, with the failing set drifting). §4 fixed the root
// cause (partial-schema fixture + public fallback) by switching the media pool to
// the shared isolated pool. The exclusion and its self-cleaning guard script
// `scripts/ci/check_media_exemption_still_needed.sh` were then removed.
//
// The tests below lock that removal: if anyone reintroduces the exemption (the
// `-E 'not test(/^media::tests::/)'` filter, the guard step, or the guard script),
// CI must fail loudly instead of silently shrinking coverage again. This replaces
// the old guard-contract tests, which are now moot because the script is gone.

#[test]
fn media_exemption_is_fully_removed_from_ci() {
    let root = repo_root();
    let src = fs::read_to_string(root.join(".github/workflows/ci.yml")).expect("ci.yml readable");

    // Only inspect non-comment lines (the file's comments legitimately retell
    // the history of the removal; the exemption must not exist as *configuration*).
    let code_lines: Vec<&str> = src.lines().map(str::trim).filter(|l| !l.starts_with('#')).collect();
    let code = code_lines.join("\n");

    // The main lib gate must NOT exclude media::tests any more.
    assert!(
        !code.contains("not test(/^media::tests::/)"),
        "media::tests 已修复并回归主门禁，ci.yml 不得再出现排除式 `not test(/^media::tests::/)`。\
         若串扰复发，请先修根因再考虑豁免，而不是直接加回排除式。"
    );
    // The self-cleaning guard step must be gone (its only purpose was to force
    // removal of the exemption, which is now done).
    assert!(
        !code.contains("Check media exemption is still necessary"),
        "media 豁免守卫步骤已随 §4 修复删除，ci.yml 不得再出现该步骤。"
    );
    // The guard script must be gone too.
    assert!(
        !root.join("scripts/ci/check_media_exemption_still_needed.sh").exists(),
        "media 豁免守卫脚本 scripts/ci/check_media_exemption_still_needed.sh 已随 §4 修复删除。"
    );
}

/// A7 ruling (2026-09-19, maintainer decision): `integration-test`, `build` and
/// `coverage` are deliberately **push/schedule-only**.
///
/// They need a Postgres/Redis service and minutes of runtime, so running them on
/// every PR was traded away; PR protection relies on `Repo Sanity`,
/// `Test & Lint`, `Security Audit` and `PR Benchmark Gate` instead
/// (`TESTING.md` §2.4, `docs/audit/GATE_INTEGRITY_FOLLOWUP_2026-09-19.md` §6.6).
///
/// This pins the *decision*, not just the YAML: enabling any of these on
/// `pull_request` (or dropping the push/schedule trigger that is the only place
/// they actually run) must be a conscious edit of this test plus the docs,
/// never a silent drift. Red proof: swap `schedule` for `pull_request` in any
/// job's `if:` and this test fails.
#[test]
fn push_only_ci_jobs_keep_their_deliberate_trigger_scope() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let ci = std::fs::read_to_string(root.join(".github/workflows/ci.yml")).expect("read ci.yml");
    let mut checked = 0;
    for job in ["integration-test", "build", "coverage"] {
        let header = format!("\n  {job}:\n");
        let start = ci.find(&header).unwrap_or_else(|| panic!("the `{job}` job must exist in ci.yml"));
        let rest = &ci[start..];
        let end = rest[1..].find("\n  [a-z]").map_or(rest.len(), |offset| offset + 1);
        let block = &rest[..end];
        let job_if = &block[..block.find("steps:").unwrap_or(block.len())];
        assert!(
            job_if.contains("github.event_name == 'push'") && job_if.contains("github.event_name == 'schedule'"),
            "the `{job}` job must stay push/schedule-only (TESTING.md §2.4): it is the only place this gate runs"
        );
        assert!(
            !job_if.contains("pull_request"),
            "the `{job}` job must not gain a pull_request trigger without a deliberate ruling (update this test and TESTING.md §2.4)"
        );
        checked += 1;
    }
    assert_eq!(checked, 3, "the push-only job list shrank; the ruling covers exactly these three jobs");
}

/// 收回 `NEXTEST_RETRIES`（2026-09-20）：重试把偶发失败重跑成"绿"。
///
/// main 上那 11 个 `out of shared memory` 克隆失败，就是靠 `NEXTEST_RETRIES: 2`
/// 在两轮里被重跑成 flaky 而不是红的（§14.13）；而根因（锁表）当时**根本没被修**。
/// flake 的正确处理是修根因或给它专用 `--test-threads 1` 车道，不是重试。
///
/// **红证明**：把 `NEXTEST_RETRIES: 2` 加回任一 nextest 步骤 → 本测试 FAILED。
#[test]
fn ci_nextest_steps_do_not_retry_flaky_tests() {
    let ci = fs::read_to_string(repo_root().join(".github/workflows/ci.yml")).expect("read ci.yml");
    assert!(
        !ci.contains("NEXTEST_RETRIES:"),
        "ci.yml 不得再给 nextest 步骤设 NEXTEST_RETRIES：它把偶发失败重跑成绿，\
         掩盖过 main 上 11 个 `out of shared memory`（§14.13）。要处理 flake 请修根因，\
         或给它专用 `--test-threads 1` 车道。"
    );
}

/// 绝对延迟断言的用例必须**既**被并行步骤排除、**又**在单线程车道里真跑。
///
/// 只排除 = 永久不跑；只加车道 = 并行与串行各跑一遍（并发时仍会红）。
/// `#[serial]`（serial_test，未开 file_locks）在 nextest 下**不生效**：每个测试一个
/// 进程，进程内互斥形同虚设 —— 真正串行的只有 `--test-threads 1`。
///
/// **红证明**：删掉并行步骤里的 `-E 'not test(/…/)'`（或车道里的 `--test-threads 1`、
/// `require_tests_ran.sh`）→ 本测试 FAILED。
#[test]
fn latency_tests_are_excluded_from_parallel_and_run_in_a_serial_lane() {
    let ci = fs::read_to_string(repo_root().join(".github/workflows/ci.yml")).expect("read ci.yml");
    let pattern = "friend_room_service::tests::bench_friend_list_";
    assert!(
        ci.contains(&format!("-E 'not test(/{pattern}/)'")),
        "并行 lib 步骤必须用 `-E 'not test(/{pattern}/)'` 排除绝对延迟断言的用例"
    );
    let lane = ci.find("Run latency benchmarks serially").expect("必须存在单线程延迟车道步骤");
    let lane_block = &ci[lane..];
    let end = lane_block[1..].find("\n      - name:").map_or(lane_block.len(), |offset| offset + 1);
    let lane_block = &lane_block[..end];
    assert!(
        lane_block.contains("--test-threads 1"),
        "延迟车道必须以 --test-threads 1 运行（nextest 每测试一进程，`#[serial]` 不生效）"
    );
    assert!(
        lane_block.contains(&format!("test(/{pattern}/)")),
        "延迟车道必须用同一个模式选中这批用例，否则它们既不在并行步骤、也不在车道里（永久不跑）"
    );
    assert!(
        lane_block.contains("require_tests_ran.sh"),
        "延迟车道必须用 require_tests_ran.sh 包裹：过滤器不再匹配时这步必须失败，而不是空转报绿"
    );
}

/// 慢速车道"必须真的跑过"的哨兵（§14.14）。
///
/// 最近 30 个 CI run 里 `Integration Tests` / `Code Coverage` / `Build Check`
/// **全部 skipped**（`needs: test` 长期红），而唯一兜底 weekly schedule 的最近 5 次
/// run 每个 job 都在 2–11s 内失败、一个 step 都没执行 ⇒ 这三条门禁从未真正执行过。
/// 哨兵把"要求的车道必须真的跑了"变成退出码；这条测试钉住哨兵本身不会被删/被架空。
///
/// **红证明**：从 `ci-summary.needs` 删掉 `coverage` → 本测试 FAILED；把任一慢速 job 的
/// `if` 里的 `run_slow_tier` 分支删掉 → FAILED。
#[test]
fn ci_summary_sentinel_requires_the_slow_tier_to_have_run() {
    let ci = fs::read_to_string(repo_root().join(".github/workflows/ci.yml")).expect("read ci.yml");
    let sentinel = ci.find("Sentinel — required lanes actually ran").expect("ci-summary 必须有慢速车道哨兵步骤");
    let block = &ci[sentinel..];
    for needle in ["TEST_RESULT", "INTEGRATION_RESULT", "COVERAGE_RESULT", "BUILD_RESULT", "did not run"] {
        assert!(block.contains(needle), "哨兵必须检查 {needle}（否则它无法证明被要求的车道真的跑过）");
    }
    // Anchor on the `ci-summary` job itself: `coverage`'s own
    // `needs: [integration-test, changes]` line would otherwise match first.
    let summary_start = ci.find("\n  ci-summary:\n").expect("ci-summary job must exist");
    let needs_line = ci[summary_start..]
        .lines()
        .find(|line| line.trim_start().starts_with("needs:"))
        .expect("ci-summary must have a needs line");
    assert!(
        needs_line.contains("coverage"),
        "ci-summary 的 needs 必须含 coverage，否则摘要可能早于覆盖率先结束：{needs_line}"
    );
    assert!(ci.contains("run_slow_tier"), "workflow_dispatch 必须提供 run_slow_tier 输入（慢速车道的按需触发路径）");
    let mut checked = 0;
    for job in ["integration-test", "build", "coverage"] {
        let header = format!("\n  {job}:\n");
        let start = ci.find(&header).unwrap_or_else(|| panic!("the `{job}` job must exist in ci.yml"));
        let rest = &ci[start..];
        let end = rest[1..].find("\n  [a-z]").map_or(rest.len(), |offset| offset + 1);
        assert!(
            rest[..end].contains("run_slow_tier"),
            "`{job}` 的 if 必须认 run_slow_tier 输入，否则 dispatch 无法强制慢速车道"
        );
        checked += 1;
    }
    assert_eq!(checked, 3, "按需慢速车道覆盖的 job 数变了；请同步 §14.14 的说明");
}
