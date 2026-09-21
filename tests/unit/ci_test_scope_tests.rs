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

/// `require_tests_ran.sh` 必须能看穿 ANSI 颜色。
///
/// workflow 级 `CARGO_TERM_COLOR: always` 让 nextest 即使在管道里也带颜色，
/// 日志里是 `\x1b[32;1m    Starting\x1b[0m \x1b[1m3\x1b[0m tests`；裸
/// `grep 'Starting [0-9]* tests'` 因此匹配不到，脚本会对一个**真跑了 3 个测试**的
/// 步骤报 `ran ZERO tests` 并 exit 1（CI 实测 2026-09-20：新增的
/// "Run latency benchmarks serially" 车道 3/3 passed 却被判空转）。本地没复现是因为
/// 本地没有 `CARGO_TERM_COLOR=always`。
///
/// 三种输入都要对：带颜色的非零 ⇒ 绿；带颜色的 `0 passed` ⇒ 红（颜色不能把真空转
/// 一起放过）；无颜色的 libtest ⇒ 行为不变。
///
/// **红证明**：把脚本里的 `strip_ansi` 去掉（回到裸 `grep "$log"`）→ 本测试 FAILED。
#[test]
fn require_tests_ran_sees_through_ansi_color() {
    let script = repo_root().join("scripts/ci/require_tests_ran.sh");
    let run = |body: &str| {
        std::process::Command::new("bash")
            .arg(&script)
            .arg("bash")
            .arg("-c")
            .arg(body)
            .output()
            .expect("require_tests_ran.sh must be runnable")
    };
    let combined = |out: &std::process::Output| {
        format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
    };

    let colored_ok =
        run(r#"printf '\033[32;1m    Starting\033[0m \033[1m3\033[0m tests across \033[1m9\033[0m binaries\n'"#);
    assert!(
        colored_ok.status.success(),
        "带 ANSI 颜色的 `Starting 3 tests` 必须被判为「真的跑了」：\n{}",
        combined(&colored_ok)
    );

    let colored_zero = run(r#"printf '\033[32;1mtest result\033[0m: ok. 0 passed; 0 failed\n'"#);
    assert!(!colored_zero.status.success(), "带 ANSI 颜色的 `0 passed` 必须仍然判红：\n{}", combined(&colored_zero));

    let plain_ok = run(r#"printf 'test result: ok. 12 passed; 0 failed; 0 ignored\n'"#);
    assert!(plain_ok.status.success(), "无颜色的 `12 passed` 必须仍然绿：\n{}", combined(&plain_ok));
}

/// Build Check 的 matrix 里有一个 **feature 列表为空**的车道（`core-matrix-min`，
/// 证明不打开任何可选 feature 也能编译）。把 matrix 值直接插进命令行会变成
/// `--features  --locked`，cargo 直接报
/// `error: a value is required for '--features <FEATURES>' but none was supplied`
/// —— 本 job 第一次真正执行时（run 35517095792）就是这样红的（§14.14）。
///
/// 判据：ci.yml 不得出现裸插值 `--features ${{ matrix`；Build 步骤必须先把
/// matrix 值放进 shell 变量并判空。
///
/// **红证明**：把 Build 步骤改回
/// `run: cargo build --release --no-default-features --features ${{ matrix.profile.features }} --locked`
/// → 本测试 FAILED。
#[test]
fn build_matrix_features_are_not_interpolated_raw() {
    let ci = fs::read_to_string(repo_root().join(".github/workflows/ci.yml")).expect("read ci.yml");
    assert!(
        ci.contains("features: \"\""),
        "Build Check 的 matrix 必须仍有一个空 feature 列表的车道（core-matrix-min），否则本守卫的前提消失"
    );
    assert!(
        !ci.contains("--features ${{ matrix"),
        "不得把 matrix 的 features 直接插进命令行：空值会让 cargo 报 \
         `a value is required for '--features <FEATURES>' but none was supplied`"
    );
    assert!(
        ci.contains("PROFILE_FEATURES: ${{ matrix.profile.features }}")
            && ci.contains("if [ -n \"${PROFILE_FEATURES}\" ]"),
        "Build 步骤必须把 matrix features 放进 shell 变量并判空后再决定是否传 --features"
    );
}

/// 供应链门禁必须先清掉 runner 镜像里那份**半成品** RustSec DB。
///
/// `cargo audit` 把 DB clone 到 `${CARGO_HOME}/advisory-db`，并**拒绝**把它初始化进一个
/// 非空目录。GitHub runner 镜像自带一个残留的 `~/.cargo/advisory-db`，于是本 job
/// 第一次真正执行时（run 35517095792）报
/// `error: couldn't fetch advisory database: git operation failed: failed to prepare clone
///  -> Refusing to initialize the non-empty directory as '/home/runner/.cargo/advisory-db'`，
/// 后面的 `jq` 再对空 JSON 报 parse error（§14.14）。
///
/// **红证明**：删掉脚本里 `cargo audit` 之前那行 `rm -rf … advisory-db` → 本测试 FAILED。
#[test]
fn supply_chain_gate_resets_the_cached_advisory_db() {
    let gate =
        fs::read_to_string(repo_root().join("scripts/ci/supply_chain_gate.sh")).expect("read supply_chain_gate.sh");
    let audit = gate.find("cargo audit \\").or_else(|| gate.find("cargo audit")).expect("脚本必须调用 cargo audit");
    let before = &gate[..audit];
    let cleanup = before
        .rfind("advisory-db")
        .expect("cargo audit 之前必须先重置缓存的 advisory-db（否则非空目录会让 fetch 失败）");
    assert!(
        before[..cleanup].contains("rm -rf"),
        "重置 advisory-db 必须用 `rm -rf`（runner 镜像里那份可能是半成品 git 目录）"
    );
}

/// `rand::rng()` 的 CI 步骤必须是**棘轮**，而且 baseline 必须等于实测值。
///
/// `.cargo/audit.toml` 对 RUSTSEC-2026-0097 的裁定是"禁止**新增** `rand::rng()`
/// 用法"（本项目不在暴露面内：用 tracing_subscriber 而非自定义 logger）。旧步骤写成
/// 绝对禁令 `if git grep -n "rand::rng()" -- '*.rs'; then exit 1; fi`，而树上有
/// **47 处**存量 ⇒ 永远不可能绿；又因为该 job 长期死在更早的 advisory-db 步骤，
/// 它从未被执行过（§14.14.1）。
///
/// 本测试钉住三件事：① ci.yml 调棘轮脚本而不是绝对禁令；② baseline 文件存在且是整数；
/// ③ baseline == 实测计数（松了会放过新增，紧了会假红）。
///
/// **扫描面排除本文件**：本守卫必须写出被禁模式（文档注释、断言消息、它自己那条 `git grep`
/// 命令），否则无法自证能变红；不排除的话这 8 处自指命中会把实测值从 47 抬到 55。这里的
/// 排除路径与 `scripts/ci/check_rand_rng_ratchet.sh` 的 `EXCLUDE_GUARD` **必须一致**，
/// 否则两者算出的实测值不同 —— 本测试当场变红。
///
/// **红证明**：往任意 `.rs` 加一处 `rand::rng()` → 本测试 FAILED（且棘轮脚本同时 FAILED）；
/// 把 ci.yml 改回绝对禁令 → FAILED。
#[test]
fn rand_rng_step_is_a_ratchet_with_an_honest_baseline() {
    let root = repo_root();
    let ci = fs::read_to_string(root.join(".github/workflows/ci.yml")).expect("read ci.yml");
    assert!(
        ci.contains("scripts/ci/check_rand_rng_ratchet.sh"),
        "Security Audit 必须调用 `scripts/ci/check_rand_rng_ratchet.sh`"
    );
    assert!(
        !ci.contains("Assert rand::rng() is unused"),
        "不得回到绝对禁令步骤 `Assert rand::rng() is unused`：树上有 47 处存量，政策是禁止新增"
    );

    let measured = std::process::Command::new("bash")
        .arg("-c")
        .arg("git grep -n 'rand::rng()' -- '*.rs' ':(exclude)tests/unit/ci_test_scope_tests.rs' | wc -l")
        .current_dir(&root)
        .output()
        .expect("git grep must be runnable");
    let measured: usize =
        String::from_utf8_lossy(&measured.stdout).trim().parse().expect("git grep count must be an integer");
    let baseline_raw =
        fs::read_to_string(root.join("scripts/ci/rand_rng_baseline")).expect("rand_rng_baseline must exist");
    let baseline: usize = baseline_raw.trim().parse().expect("rand_rng_baseline must be a single integer");
    assert_eq!(
        measured, baseline,
        "rand::rng() 的 baseline 必须等于实测计数（实测 {measured} / baseline {baseline}）：\
         偏松会放过新增用法，偏紧会假红。收紧/放宽都要走 \
         `bash scripts/ci/check_rand_rng_ratchet.sh --update` 并说明理由。"
    );
}

/// `room_aliases` 的主键列是 `room_alias`，不是 `alias`。
///
/// 写 `DELETE FROM room_aliases WHERE alias = $1` 会 `42703 column "alias" does not
/// exist`。这个错列名存在于 `tests/integration/api_federation_tests.rs` 的清理代码里，
/// 而 integration 目标在 CI 里从未真正跑过（§14.14.1），所以直到慢速车道第一次执行
/// 才暴露。本守卫扫全部 `.rs`，防止同一列名漂移再犯。
///
/// **扫描面排除本文件**：理由同 `rand_rng_step_is_a_ratchet_with_an_honest_baseline` ——
/// 本守卫的文档注释与扫描命令里各有一份被禁模式，不排除就会自己告自己。
///
/// **红证明**：把任一处改回 `room_aliases WHERE alias` → 本测试 FAILED。
#[test]
fn no_source_queries_a_non_existent_room_aliases_column() {
    let root = repo_root();
    let out = std::process::Command::new("bash")
        .arg("-c")
        .arg("git grep -n 'room_aliases WHERE alias' -- '*.rs' ':(exclude)tests/unit/ci_test_scope_tests.rs' || true")
        .current_dir(&root)
        .output()
        .expect("git grep must be runnable");
    let hits = String::from_utf8_lossy(&out.stdout);
    assert!(
        hits.trim().is_empty(),
        "`room_aliases` 的列名是 `room_alias`：以下位置仍在用不存在的 `alias` 列（SQLSTATE 42703）:\n{hits}"
    );
}

/// 慢速车道的 integration 步骤必须**报出全部失败**，而不是只报第一个；并且并发度不得
/// 超过 CI 锁预算能承受的上限。
///
/// nextest 默认 `fail-fast = true`（`.config/nextest.toml` 只定义了 `ci`/`tdd`/`test`
/// 三个 profile，该步骤用的是**默认** profile）。run 35542783982 因此中止在
/// `tests/integration/api_federation_tests.rs` 的清理语句（错列名，SQLSTATE 42703）上：
/// 1424 个测试只跑了 139 个，后面还有多少失败**无从得知** —— 而这条车道一轮 15–40 分钟，
/// N 个缺陷要摊成 N 轮。`--no-fail-fast` 只是不再**隐藏**失败，不改变计数与退出码，
/// 也不是 retry（retry 会掩盖 flake，见 `ci_nextest_steps_do_not_retry_flaky_tests`）。
///
/// `--test-threads` ≤ 4：run 35553786373 全量跑完 1424 条后只剩 3 条红，且 3 条全是
/// `53200 out of shared memory`（PostgreSQL 锁表被"并发克隆 227 表模板 schema"挤爆，
/// hint 是 `increase max_locks_per_transaction`）—— 基础设施故障。CI 的 postgres
/// service container **无法**传 `-c max_locks_per_transaction`（runner 把 `-c` 当
/// `--cpu-shares`，见 §14.13 ③），所以唯一的结构性杠杆是降并发。
///
/// **红证明**：把该步骤 `run:` 末尾的 `--no-fail-fast` 删掉 → 本测试 FAILED；
/// 把 `--test-threads` 改回 6 → 本测试 FAILED。
#[test]
fn integration_step_reports_every_failure() {
    let ci = fs::read_to_string(repo_root().join(".github/workflows/ci.yml")).expect("read ci.yml");
    let step = ci
        .split("- name: ")
        .find(|s| s.starts_with("Run integration tests (--test integration)"))
        .expect("ci.yml 必须有 `Run integration tests (--test integration)` 步骤");
    let run = step
        .lines()
        .find(|l| l.trim_start().starts_with("run: cargo nextest run"))
        .expect("该步骤必须有 cargo nextest run 命令");
    assert!(
        run.contains("--test integration") && run.contains("--all-features"),
        "integration 步骤必须仍然跑全量 integration 目标：{run}"
    );
    assert!(
        run.contains("--no-fail-fast"),
        "integration 步骤必须带 `--no-fail-fast`：nextest 默认 fail-fast，会让一轮慢速车道只报\
         第一个失败（run 35542783982：1424 个测试只跑了 139 个）。实际命令：{run}"
    );
    assert!(
        run.contains("--test-threads 4") || run.contains("--test-threads 3") || run.contains("--test-threads 2"),
        "integration 步骤的并发必须 <= 4：CI 的 postgres 无法加大 max_locks_per_transaction\
         （runner 把 service container 的 `-c` 当 `--cpu-shares`，§14.13 ③），而 6 并发克隆\
         227 表模板 schema 会挤爆锁表，run 35553786373 因此有 3/1424 条 `53200 out of shared\
         memory`。实际命令：{run}"
    );
}

/// k6 guardrail 必须读得懂 k6 **0.47 的扁平** `--summary-export`，并且 `--fail-on-breach`
/// 真的以非零码退出。
///
/// 2026-09-21 本地第一次真跑 k6 冒烟（docker `grafana/k6:0.47.0`，与 CI 同版本）时发现：
/// 0.47 的导出把聚合值**平铺**在 `metrics.<name>` 下（`{"p(95)":12}` / errors 是
/// `{"value":1}`），而 `guardrail.py` 只认 `metric["values"]["p(95)"]`（更老的
/// summary-handler 形态）⇒ 七项指标全部渲染成 `Actual: missing / Status: FAIL`，
/// **即使目标完全健康也只会 FAIL**。这正是本会话反复出现的"从未执行过的门禁"缺陷。
///
/// **红证明**：把 `metric_value` 改回只读 `metric["values"]` → 本测试的健康用例报
/// `missing` 且退出非零 → FAILED。
#[test]
fn k6_guardrail_reads_the_flat_summary_export() {
    let root = repo_root();
    let tmp = std::env::temp_dir().join(format!("dsh-k6-guard-{}", std::process::id()));
    fs::create_dir_all(&tmp).expect("create temp dir for synthetic k6 summaries");
    let run = |metrics_json: &str| -> (i32, String) {
        fs::write(tmp.join("smoke_results.json"), format!("{{\"metrics\":{metrics_json}}}"))
            .expect("write synthetic k6 summary");
        let out = std::process::Command::new("python3")
            .arg(root.join("scripts/test/perf/guardrail.py"))
            .arg("--results-dir")
            .arg(&tmp)
            .arg("--scenario")
            .arg("smoke")
            .arg("--fail-on-breach")
            .output()
            .expect("guardrail.py must be runnable with python3");
        (out.status.code().unwrap_or(-1), String::from_utf8_lossy(&out.stdout).to_string())
    };

    const FLAT_HEALTHY: &str = r#"{"login_duration":{"p(95)":12},"create_room_duration":{"p(95)":8},
        "send_message_duration":{"p(95)":5},"sync_duration":{"p(95)":40},
        "room_summary_duration":{"p(95)":9},"errors":{"value":0.0}}"#;
    let (code, out) = run(FLAT_HEALTHY);
    assert_eq!(code, 0, "健康目标（所有 P95 远低于阈值、错误率 0）必须 PASS：\n{out}");
    assert!(
        !out.contains("missing"),
        "k6 0.47 的扁平 summary-export 必须被读到；出现 `missing` 说明解析器只认旧的`values` 形态：\n{out}"
    );

    const FLAT_BREACH: &str = r#"{"login_duration":{"p(95)":12},"create_room_duration":{"p(95)":8},
        "send_message_duration":{"p(95)":5},"sync_duration":{"p(95)":40},
        "room_summary_duration":{"p(95)":9},"errors":{"value":1.0}}"#;
    let (code, out) = run(FLAT_BREACH);
    assert_eq!(code, 1, "错误率 100% 必须让 `--fail-on-breach` 以非零退出：\n{out}");

    // 旧的嵌套形态（summary-handler 风格）仍要能读：同一份数据的另一种写法。
    const NESTED_HEALTHY: &str = r#"{"login_duration":{"values":{"p(95)":12}},
        "create_room_duration":{"values":{"p(95)":8}},"send_message_duration":{"values":{"p(95)":5}},
        "sync_duration":{"values":{"p(95)":40}},"room_summary_duration":{"values":{"p(95)":9}},
        "errors":{"values":{"rate":0.0}}}"#;
    let (code, out) = run(NESTED_HEALTHY);
    assert_eq!(code, 0, "嵌套形态（旧 summary-handler 输出）也必须能读：\n{out}");
    assert!(!out.contains("missing"), "嵌套形态不应出现 `missing`：\n{out}");

    let _ = fs::remove_dir_all(&tmp);
}

/// cargo-geiger 门禁必须是**单向棘轮**，且基线里的逐条理由必须自洽（2026-09-21 裁定 B）。
///
/// 背景（§14.14.6）：解析器修好 cargo-geiger 0.13 的 schema 之后，这道门禁第一次给出真判定
/// —— production unsafe = 2 / test-only = 8 —— 于是"production 硬零、无白名单"的政策被违反。
/// 裁定：改成"极紧的逐条棘轮"——只许减少，减少时**必须**同步收紧基线，每一处都要在基线里
/// 写明理由与 `review_by`，并且**逐条清单之和必须等于总数**（否则基线写的和门禁管的是两回事）。
///
/// 本测试用**合成报告**离线驱动脚本（不需要 cargo-geiger），钉住 5 件事：
/// ① 与仓库基线一致的 prod=2 / test=8 ⇒ exit 0；② prod=3 ⇒ exit 1（有人新增）：
/// ③ prod=1 ⇒ exit 1（好事，但必须收紧基线）；④ test=9 ⇒ exit 1；⑤ 逐条清单之和与总数
/// 不一致的基线 ⇒ exit 2。
///
/// **红证明**：把 Gate 1 改回硬零（`prod_total > 0 ⇒ FAIL`）→ ① 变成 exit 1，本测试 FAILED；
/// 把 `validate_baseline_keys` 的求和校验删掉 → ⑤ 变成 exit 0，本测试 FAILED。
#[test]
fn cargo_geiger_gate_is_a_one_way_ratchet() {
    let root = repo_root();
    let tmp = std::env::temp_dir().join(format!("dsh-geiger-guard-{}", std::process::id()));
    fs::create_dir_all(&tmp).expect("create temp dir for synthetic reports");

    // 合成报告的形状与 cargo-geiger 0.13 一致：`packages` 是 list，计数器嵌套在
    // `unsafety.used.exprs.unsafe_`，workspace 成员用 `id.source = {"Path": …}` 标记。
    // 用 `str::replace` 而不是 `format!`，省掉 JSON 花括号的转义噪音。
    let write_pair = |prod: i64, test: i64, tag: &str| -> (std::path::PathBuf, std::path::PathBuf) {
        const TEMPLATE: &str = r#"{"packages":[{"package":{"id":{"name":"synapse-rust","version":"0.1.0","source":{"Path":"file:///w/root"}}},"unsafety":{"used":{"exprs":{"safe":1,"unsafe_":USED}}}}],"packages_without_metrics":[],"used_but_not_scanned_files":[]}"#;
        let report = |used: i64| TEMPLATE.replace("USED", &used.to_string());
        let prod_path = tmp.join(format!("prod-{tag}.json"));
        let all_path = tmp.join(format!("all-{tag}.json"));
        fs::write(&prod_path, report(prod)).expect("write synthetic prod report");
        fs::write(&all_path, report(prod + test)).expect("write synthetic all report");
        (prod_path, all_path)
    };

    let script = root.join("scripts/ci/run_cargo_geiger.py");
    let repo_baseline = root.join("scripts/ci/geiger_baseline.json");
    let run = |prod: i64, test: i64, tag: &str, baseline: &std::path::Path| -> i32 {
        let (p, a) = write_pair(prod, test, tag);
        let out = std::process::Command::new("python3")
            .arg(&script)
            .arg("--prod-report")
            .arg(&p)
            .arg("--all-report")
            .arg(&a)
            .arg("--baseline")
            .arg(baseline)
            .output()
            .expect("run_cargo_geiger.py must be runnable with python3");
        let code = out.status.code().unwrap_or(-1);
        if std::env::var_os("GEIGER_GUARD_VERBOSE").is_some() {
            eprintln!(
                "--- prod={prod} test={test} baseline={} -> {code}\n{}{}",
                baseline.display(),
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
        }
        code
    };

    assert_eq!(run(2, 8, "ok", &repo_baseline), 0, "基线内的 prod=2/test=8 必须 PASS");
    assert_eq!(run(3, 8, "increase", &repo_baseline), 1, "prod 增加到 3 必须 FAIL（单向棘轮）");
    assert_eq!(run(1, 8, "decrease", &repo_baseline), 1, "prod 降到 1 必须 FAIL 并要求收紧基线");
    assert_eq!(run(2, 9, "test-increase", &repo_baseline), 1, "test-only 超过基线必须 FAIL");

    // ⑤ 逐条清单之和 != 总数：基线在骗人，必须 exit 2 而不是照常判定。
    let bad_baseline = tmp.join("bad-baseline.json");
    fs::write(
        &bad_baseline,
        r#"{"prod_unsafe_total":2,"test_unsafe_total":8,
            "prod_unsafe_sites":[{"package":"x","count":1,"why":"synthetic","review_by":"2099-01-01"}],
            "test_unsafe_sites":[{"package":"y","count":8,"why":"synthetic","review_by":"2099-01-01"}]}"#,
    )
    .expect("write synthetic baseline");
    assert_eq!(run(2, 8, "sum-mismatch", &bad_baseline), 2, "逐条清单之和与总数不一致必须 exit 2");

    let _ = fs::remove_dir_all(&tmp);
}

/// Snapshot gate 必须是 **insta assert-only**，并且不依赖 cargo-insta。
///
/// 这条门禁在真 CI 里两次执行、两次因 cargo-insta 的 CLI 语义变动而红：
///   ① run `35563084512`：`--no-review` 在 cargo-insta 1.48 已被删除
///      （`error: unexpected argument '--no-review' found`）；
///   ② run `35571855133`：改用 `--check --test-runner nextest -- --all-features … --test unit`
///      后，`--` 之后的 **cargo/nextest 旗标被当成测试二进制参数**传下去，
///      nextest 报 `failed to parse test binary arguments … arguments are unsupported`。
/// 结论：工具链语义反复变动，而"快照漂移必须红"**不需要它** —— insta 在 `INSTA_UPDATE=no`
/// 下就会让漂移的测试失败。因此本步骤改为直接用 nextest 跑 unit 目标 + 显式 assert-only 模式
/// + `.snap.new` 兜底检查，并删除 cargo-insta 的安装步骤。
///
/// 判定：① ci.yml 里**没有** `cargo insta` / `cargo-insta`（工具已移除）；
/// ② 该步骤跑 `cargo nextest run --test unit --all-features`（20 个 unit 快照的覆盖来源）；
/// ③ 显式 `INSTA_UPDATE: "no"`（只断言、不写 `.snap.new`，不依赖环境里的 CI 变量）；
/// ④ 有 `.snap.new` 兜底检查（已提交 + 未跟踪两种）；⑤ 并发 ≤ 4（锁预算，同 integration）。
///
/// **红证明**：删掉 `INSTA_UPDATE` → FAILED；把 `cargo insta test` 加回来 → FAILED；
/// 删掉 `.snap.new` 兜底检查 → FAILED。
#[test]
fn snapshot_gate_is_insta_assert_only() {
    let ci = fs::read_to_string(repo_root().join(".github/workflows/ci.yml")).expect("read ci.yml");
    // 只看**非注释行**：本步骤的注释里必须能解释"为什么不再用 cargo-insta"，
    // 否则守卫会把自己的说明文字当成违规（扫描型守卫的老坑）。
    let insta_tool_lines: Vec<&str> = ci
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with('#')
                && (t.contains("cargo insta test") || t.contains("cargo-insta") || t.contains("tool: cargo-insta"))
        })
        .collect();
    assert!(
        insta_tool_lines.is_empty(),
        "Snapshot gate 不得再依赖 cargo-insta（它的 CLI 语义两次把这条门禁弄红：`--no-review` 被删、\
         `--` 之后的 cargo 旗标被当成测试二进制参数），而 insta 自身的 `INSTA_UPDATE=no` 已足够。\
         违规行：{insta_tool_lines:?}"
    );
    let step =
        ci.split("- name: ").find(|s| s.starts_with("Snapshot gate")).expect("ci.yml 必须有 `Snapshot gate` 步骤");
    let run = step
        .lines()
        .find(|l| l.trim_start().starts_with("cargo nextest run --test unit"))
        .unwrap_or_else(|| panic!("Snapshot gate 必须用 `cargo nextest run --test unit` 跑 unit 快照：\n{step}"));
    assert!(
        run.contains("--all-features") && run.contains("--test unit"),
        "Snapshot gate 必须在 `--all-features` 下跑 unit 目标（fast tier 的 unit 车道是 \
         `--features test-utils` 或带 `-E` 的子集，覆盖不到这 20 个快照）：{run}"
    );
    assert!(
        !run.contains("--test-threads 8")
            && (run.contains("--test-threads 4")
                || run.contains("--test-threads 3")
                || run.contains("--test-threads 2")
                || run.contains("--test-threads 1")),
        "Snapshot gate 的并发必须 <= 4（CI 锁预算，同 integration 车道）：{run}"
    );
    assert!(
        step.contains("INSTA_UPDATE: \"no\"") || step.contains("INSTA_UPDATE: no"),
        "Snapshot gate 必须显式设 `INSTA_UPDATE=no`（只断言、绝不写 `.snap.new`），\
         否则漂移会被写成待接受文件而门禁可能放行"
    );
    assert!(
        step.contains("git ls-files --error-unmatch '*.snap.new'")
            && step.contains("git ls-files --others --exclude-standard '*.snap.new'"),
        "Snapshot gate 必须有 `.snap.new` 兜底检查（已提交与未跟踪两种）"
    );
}

/// 供应链例外（advisory ignore）必须**三件事同时成立**，否则红：
///
/// ① `deny.toml` 的 `ignore` 是 `.cargo/audit.toml` 的**子集**（cargo-deny 不得忽略一条
///    在理由单一真相源里根本不存在的 advisory）；注意**不要求两边相等** —— 同一个编号在
///    两个工具里命中面可能不同（实测：`RUSTSEC-2024-0436`/paste 在 cargo-audit 侧命中，
///    在 cargo-deny 侧是 `warning[advisory-not-detected]`），强行让清单"看起来一致"只会
///    在某一侧留下死条目；
/// ② `.cargo/audit.toml` 里每个被 ignore 的编号都**有注释说明**（理由单一真相源就在那里）；
/// ③ 两份文件里的每个 `Review-by YYYY-MM-DD` 都**没有过期**。
///
/// 为什么需要 ③：2026-09-21 复核发现三条裁定日期早就过期（RUSTSEC-2023-0071 /
/// RUSTSEC-2024-0436 的 2026-06-30、RUSTSEC-2026-0097 的 2026-05-15），同时**三条
/// ignore 已经不再匹配任何依赖**（derivative / proc-macro-error2 已不在 Cargo.lock，
/// rand 0.8.7/0.9.5 落在 advisory 的 `patched` 区间内）。没有任何门禁检查这些日期，
/// 所以它们只会烂在那里：`cargo-audit` 不读 `review-by`，过期不会红。日期一烂，
/// 一条不再匹配的 ignore 就变成"依赖降级回受影响版本时继续静默放行"的通道 ——
/// 门禁看起来更严，实际更弱。
///
/// 这两件事在 `docs/audit/PROJECT_ACTUAL_ISSUES_2026-09-14.md` M-6 里已被记录过
/// （cargo-deny 报了 3 条 `warning[advisory-not-detected]`），但当时建议"3 条全删"，
/// 没有核对 cargo-audit 侧，所以一直没修。
///
/// `docs/security/ci-security-grading.md` 里还曾有一份**第三份**例外清单表格（已删：
/// 它列了配置里根本没有的 RUSTSEC-2025-0123，又漏了配置里的 RUSTSEC-2024-0388），
/// 那正是"同一职责两份实现必然漂移"的例子。
///
/// **红证明**：把任一 `Review-by` 改成 `2020-01-01` → FAILED；往 `deny.toml` 的清单里
/// 加一个 `.cargo/audit.toml` 没有的编号 → FAILED。
#[test]
fn advisory_review_dates_are_not_overdue() {
    let root = repo_root();
    let today = std::process::Command::new("date")
        .arg("-u")
        .arg("+%F")
        .output()
        .expect("`date` must be runnable (guards run on macOS and ubuntu runners)");
    let today = String::from_utf8_lossy(&today.stdout).trim().to_string();
    assert!(
        today.len() == 10 && today.as_bytes()[4] == b'-',
        "`date -u +%F` 必须给出 ISO 日期（字符串比较按字典序 == 按时间序）：{today}"
    );

    let read =
        |relative: &str| fs::read_to_string(root.join(relative)).unwrap_or_else(|e| panic!("read {relative}: {e}"));
    let audit = read(".cargo/audit.toml");
    let deny = read("deny.toml");

    // 只取 `ignore = [ … ]` 块里的编号：注释里提到的编号（含"已删除"说明）不算清单成员。
    // 块尾必须用行首的 `\n]` 定位 —— 块内注释可能含方括号（如 `[patch.crates-io]`），
    // 用第一个 `]` 会提前截断，把后半段清单吞掉（实测会漏掉 paste 那条）。
    let ignore_ids = |text: &str, file: &str| -> Vec<String> {
        let start = text.find("ignore = [").unwrap_or_else(|| panic!("{file} 必须有 `ignore = [` 块"));
        let rest = &text[start..];
        let end = rest.find("\n]").unwrap_or_else(|| panic!("{file} 的 `ignore = [` 块必须闭合"));
        let block = &rest[..end];
        let mut ids = Vec::new();
        let mut cursor = 0usize;
        while let Some(offset) = block[cursor..].find("RUSTSEC-") {
            let at = cursor + offset;
            let id: String = block[at..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
            ids.push(id);
            cursor = at + 1;
        }
        ids.sort();
        ids.dedup();
        ids
    };

    let audit_ids = ignore_ids(&audit, ".cargo/audit.toml");
    let deny_ids = ignore_ids(&deny, "deny.toml");
    assert!(!audit_ids.is_empty(), ".cargo/audit.toml 的 ignore 清单不该是空的（守卫前提消失）");
    for id in &deny_ids {
        assert!(
            audit_ids.contains(id),
            "`deny.toml` 里的 {id} 不在 `.cargo/audit.toml` 的清单里：cargo-deny 的 ignore \
             必须是 cargo-audit 那份（理由与复核证据的单一真相源）的子集，否则就是一条没有\
             任何理由记录的白名单"
        );
    }
    for id in &audit_ids {
        assert!(
            audit.lines().any(|line| line.trim_start().starts_with('#') && line.contains(id.as_str())),
            "被 ignore 的 {id} 必须在 .cargo/audit.toml 里带注释说明理由与 `Review-by` 日期"
        );
    }

    for (file, text) in [(".cargo/audit.toml", &audit), ("deny.toml", &deny)] {
        let mut dates = Vec::new();
        let mut cursor = 0usize;
        while let Some(offset) = text[cursor..].find("Review-by ") {
            let at = cursor + offset + "Review-by ".len();
            dates.push(text[at..].chars().take(10).collect::<String>());
            cursor = at;
        }
        assert!(!dates.is_empty(), "{file} 必须带有 `Review-by YYYY-MM-DD`（没有日期的例外永不复核）");
        for date in dates {
            assert!(
                date.as_str() >= today.as_str(),
                "{file}: Review-by {date} 已过期（今天 {today}）。请重新做一次复核（把证据写进 \
                 .cargo/audit.toml）再续期；确认不再匹配任何依赖的条目应当**删除**而不是续期。"
            );
        }
    }
}

/// k6 冒烟测试必须由**显式**的 dispatch 输入触发，不能挂在裸 `workflow_dispatch` 上。
///
/// 该 job 打的是**外部**目标（`secrets.K6_SMOKE_BASE_URL`，缺省 `http://localhost:8448`），
/// 而它自己**不启动任何服务** —— 所以"只想验证慢速车道"的 `run_slow_tier` dispatch 会
/// 顺带把它拉起来，并因为一个与本次改动无关的原因变红。k6 进不进 `ci-summary` 哨兵
/// 也有明确答案：不进 —— 哨兵保证的是"事件要求的慢速车道没有被静默跳过"，而 k6 需要
/// 外部环境 + secret，只能由人显式要求并自行认领结果。
///
/// **红证明**：把该 job 的 `if` 改回裸 `github.event_name == 'workflow_dispatch'` → FAILED。
#[test]
fn k6_smoke_requires_an_explicit_dispatch_input() {
    let ci = fs::read_to_string(repo_root().join(".github/workflows/ci.yml")).expect("read ci.yml");
    let k6 = ci.split("k6-smoke-test:").nth(1).expect("ci.yml 必须有 k6-smoke-test job");
    // 只取该 job 自己的内容（到下一个顶层 job 头为止），避免把后续 job 的条件算进来。
    let mut head = String::new();
    for line in k6.lines() {
        let is_next_job = line.starts_with("  ")
            && !line.starts_with("   ")
            && line.ends_with(':')
            && line.trim_end_matches(':').chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if is_next_job {
            break;
        }
        head.push_str(line);
        head.push('\n');
    }
    assert!(
        head.contains("github.event.inputs.run_k6 == 'true'"),
        "k6 job 必须由显式输入 `run_k6` 触发（它打外部环境、自己不启动服务）：\n{head}"
    );
    assert!(
        !head.contains("if: github.event_name == 'workflow_dispatch'\n"),
        "不得把 k6 job 的触发条件退回裸 `workflow_dispatch`"
    );
    assert!(
        ci.contains("      run_k6:") && ci.contains("运行 k6 冒烟测试"),
        "`workflow_dispatch.inputs` 必须声明 `run_k6`（默认 false），否则没人能显式要求它"
    );
}

/// perf smoke 步骤必须列出 `performance_manual` 的**全部** `required-features`。
///
/// run 35580479156 第一次真正执行到 `Run performance smoke gate`（它排在 integration /
/// e2e / snapshot gate 之后，此前从未轮到）时立刻红：
///   `error: target \`performance_manual\` in package \`synapse-rust\` requires the`
///   `features: \`performance-tests\`, \`test-utils\``
/// —— `Cargo.toml` 的 `[[test]] performance_manual` 声明了**两个** required-features，而该步骤
/// 只传了 `performance-tests`。本守卫从清单（Cargo.toml）推导出要求，再断言 ci.yml 的
/// `--features` 里**逐项**列出，避免以后再加一个 required-feature 又漏传。
///
/// **红证明**：把步骤里的 `--features performance-tests,test-utils` 改回
/// `--features performance-tests` → FAILED。
#[test]
fn performance_smoke_step_declares_required_features() {
    let root = repo_root();
    let manifest = fs::read_to_string(root.join("Cargo.toml")).expect("read Cargo.toml");
    let block = manifest
        .split("[[test]]")
        .find(|b| b.contains("name = \"performance_manual\""))
        .expect("Cargo.toml 必须有 `[[test]] performance_manual`（守卫前提）");
    let required: Vec<String> = block
        .lines()
        .find(|l| l.trim_start().starts_with("required-features"))
        .and_then(|l| l.split('[').nth(1))
        .and_then(|l| l.split(']').next())
        .map(|list| {
            list.split(',')
                .map(|item| item.trim().trim_matches('"').to_string())
                .filter(|item| !item.is_empty())
                .collect()
        })
        .expect("`performance_manual` 必须声明 `required-features`");
    assert!(!required.is_empty(), "守卫前提：required-features 不应为空");

    let ci = fs::read_to_string(root.join(".github/workflows/ci.yml")).expect("read ci.yml");
    let step = ci
        .split("- name: ")
        .find(|s| s.starts_with("Run performance smoke gate"))
        .expect("ci.yml 必须有 `Run performance smoke gate` 步骤");
    let run = step
        .lines()
        .find(|l| l.trim_start().starts_with("cargo test --test performance_manual"))
        .expect("该步骤必须跑 `cargo test --test performance_manual`");
    let features = run.split("--features").nth(1).and_then(|rest| rest.split_whitespace().next()).unwrap_or("");
    for feature in &required {
        assert!(
            features.split(',').any(|f| f.trim() == feature),
            "perf smoke 步骤的 `--features` 必须列出 `performance_manual` 的全部 required-features；\
             缺 `{feature}`（Cargo.toml 声明 {required:?}，ci.yml 传 {features:?}）"
        );
    }
}

/// 每个"测试期会向 janitor 注册 schema"的 crate，都必须由**自己的测试构建**注册退出排空钩子。
///
/// 背景（B' 设计，见 `docs/audit/GATE_INTEGRITY_FOLLOWUP_2026-09-19.md` §14.14.8.1）：
/// `libc::atexit(drain_schemas_at_exit)` 是 `unsafe`，把它留在
/// `synapse_common::test_schema_guard` 里会让 cargo-geiger 的**生产**扫描把它算成
/// production unsafe。它被移到各个测试构建：生产库零 `unsafe`，而 `--include-tests`
/// 扫描照样看得见（落到 test-only 差值里），**运行时行为完全不变**。
///
/// 这条守卫把"哪些 crate 需要注册"钉成静态不变量：
/// ① 任何 `src/` 里调用 `register_schema_cleanup` 的 workspace crate，其 `Cargo.toml`
///    的 `[dev-dependencies]` 必须有 `libc`（否则注册代码编不过）；
/// ② 该 crate 的源码里必须出现 `drain_schemas_at_exit`（注册点或其模块）；
/// ③ 根 crate 的两个测试二进制（`tests/unit`、`tests/integration`）共用
///    `tests/common/mod.rs`，其中必须有 `ensure_schema_exit_hook` 且真正被调用；
/// ④ `synapse-common/src` 的**非注释**代码里不得出现 `unsafe`（这正是 B' 的目的）。
///
/// **红证明**：删掉任一 crate 的 `libc` dev-dependency → FAILED；删掉任一 crate 的注册
/// （模块/调用）→ FAILED；往 `synapse-common/src` 的生产代码插一个 `unsafe {}` → FAILED。
#[test]
fn every_db_test_binary_registers_the_exit_drain() {
    let root = repo_root();

    // ① 找出所有会在测试期注册 schema 的 workspace crate（以调用 register_schema_cleanup 为准）。
    // 用普通 `grep -rl` 而不是 `git grep`：注册点/钩子模块里有**新增文件**，而
    // `git grep` 默认只看已跟踪文件 —— 守卫不能因为文件还没 `git add` 就假红。
    let grep = std::process::Command::new("bash")
        .arg("-c")
        .arg("grep -rl register_schema_cleanup src synapse-common/src synapse-cache/src synapse-e2ee/src synapse-federation/src synapse-services/src synapse-storage/src synapse-test-utils/src synapse-web/src 2>/dev/null || true")
        .current_dir(&root)
        .output()
        .expect("git grep must be runnable");
    let hits = String::from_utf8_lossy(&grep.stdout);
    let mut crates: Vec<String> = hits
        .lines()
        .filter_map(|l| l.split('/').next())
        .map(|c| if c == "src" { "synapse-rust".to_string() } else { c.to_string() })
        .collect();
    crates.sort();
    crates.dedup();
    assert!(!crates.is_empty(), "守卫前提：必须至少有一个 crate 注册 schema 清理");

    for krate in &crates {
        let manifest =
            if krate == "synapse-rust" { root.join("Cargo.toml") } else { root.join(krate).join("Cargo.toml") };
        let toml = fs::read_to_string(&manifest).unwrap_or_else(|e| panic!("read {}: {e}", manifest.display()));
        assert!(
            toml.contains("libc = \"0.2\""),
            "{krate} 在测试期注册 schema 清理，因此它的 [dev-dependencies] 必须有 libc（atexit 钩子是 unsafe）"
        );
        // ② 该 crate 源码里必须出现注册点（测试期注册模块或测试目标里的注册函数）。
        let grep_reg = std::process::Command::new("bash")
            .arg("-c")
            .arg(format!("grep -rl drain_schemas_at_exit {krate}/src {krate}/tests-support 2>/dev/null || true"))
            .current_dir(&root)
            .output()
            .expect("git grep must be runnable");
        let reg = String::from_utf8_lossy(&grep_reg.stdout);
        assert!(
            !reg.trim().is_empty(),
            "{krate} 必须在自己可编译进测试构建的代码里注册 drain_schemas_at_exit（依赖里的 #[cfg(test)] 对它的测试构建不可见）"
        );
        // 只"存在钩子模块"不算注册：必须真的在某个共享夹具里调用 `test_exit_hook::ensure()`。
        let grep_call = std::process::Command::new("bash")
            .arg("-c")
            .arg(format!("grep -rl 'test_exit_hook::ensure()' {krate}/src 2>/dev/null || true"))
            .current_dir(&root)
            .output()
            .expect("grep must be runnable");
        let call = String::from_utf8_lossy(&grep_call.stdout);
        assert!(
            !call.trim().is_empty(),
            "{krate} 的钩子模块必须被**调用**（`test_exit_hook::ensure()`）——只有模块没有调用等于没注册"
        );
    }

    // ③ 根 crate 的测试目标（tests/unit、tests/integration）共用 tests/common/mod.rs。
    let common = fs::read_to_string(root.join("tests/common/mod.rs")).expect("read tests/common/mod.rs");
    assert!(
        common.contains("pub fn ensure_schema_exit_hook()"),
        "tests/common/mod.rs 必须定义 ensure_schema_exit_hook（被 tests/unit 与 tests/integration 两个测试二进制共用）"
    );
    let common_calls = common.matches("ensure_schema_exit_hook();").count();
    let integration = fs::read_to_string(root.join("tests/integration/mod.rs")).expect("read tests/integration/mod.rs");
    let integration_calls = integration.matches("ensure_schema_exit_hook()").count();
    assert!(
        common_calls >= 1 && integration_calls >= 1,
        "注册必须被真正调用：tests/common/mod.rs 的 `get_test_pool_async` 至少调一次、\
         tests/integration/mod.rs 的 `require_test_pool` 至少调一次（定义行的写法不以分号结尾，\
         因此不计入 `ensure_schema_exit_hook();` 的调用计数）\
         （common_calls={common_calls}, integration_calls={integration_calls}）"
    );

    // ④ B' 的两条硬不变量：`synapse-common/src` 不得再引用 `libc`（依赖已移到 dev），
    //    且 `test_schema_guard.rs` 里不得再有非注释的 `unsafe`；同时把整个 crate 里**其余**
    //    非注释 unsafe 行钉成"仅限已知的两处测试块"，这样新出现的 unsafe 一定被看见。
    let grep_libc = std::process::Command::new("bash")
        .arg("-c")
        .arg("grep -rn 'libc' synapse-common/src 2>/dev/null | grep -vE '^[^:]+:[0-9]+:[[:space:]]*(//|///|//!)' || true")
        .current_dir(&root)
        .output()
        .expect("grep must be runnable");
    let libc_hits = String::from_utf8_lossy(&grep_libc.stdout);
    assert!(
        libc_hits.trim().is_empty(),
        "`synapse-common/src` 的**非注释**代码不得再引用 libc（atexit 钩子已移到测试目标，libc 是 dev-dependency）：\n{libc_hits}"
    );

    let non_comment_unsafe = |pathspec: &str| -> String {
        // grep -rn omits the filename prefix when searching a single file,
        // which breaks the `^[^:]+:[0-9]+:` filter below. Normalize to a
        // directory search so every output line carries `file:line:`.
        let dir = if std::path::Path::new(pathspec).is_file() {
            std::path::Path::new(pathspec).parent().map_or(pathspec, |p| p.to_str().unwrap())
        } else {
            pathspec
        };
        let file_re = if std::path::Path::new(pathspec).is_file() {
            Some(format!("^{}:", pathspec.replace('\\', "/")))
        } else {
            None
        };
        let out = std::process::Command::new("bash")
            .arg("-c")
            .arg(format!(
                "grep -rn 'unsafe' {dir} 2>/dev/null | grep -vE '^[^:]+:[0-9]+:[[:space:]]*(//|///|//!)' || true"
            ))
            .current_dir(&root)
            .output()
            .expect("grep must be runnable");
        let raw = String::from_utf8_lossy(&out.stdout).to_string();
        raw.lines().filter(|l| file_re.as_ref().is_none_or(|re| l.starts_with(re))).collect::<Vec<_>>().join("\n")
    };

    let janitor_unsafe = non_comment_unsafe("synapse-common/src/test_schema_guard.rs");
    assert!(
        janitor_unsafe.trim().is_empty(),
        "B' 后 `test_schema_guard.rs`（生产模块）不得再有任何 unsafe：\n{janitor_unsafe}"
    );

    let all_unsafe = non_comment_unsafe("synapse-common/src");
    let unexpected: Vec<&str> =
        all_unsafe.lines().filter(|l| !l.starts_with("synapse-common/src/config/mod.rs:")).collect();
    assert!(
        unexpected.is_empty(),
        "`synapse-common/src` 里除 `config/mod.rs` 的两处已知 `#[cfg(test)]` set_var 块外，\
         不得有非注释 unsafe（新增的必须显式审阅后再加入本守卫的允许集合）：\n{unexpected:?}"
    );
    assert_eq!(
        all_unsafe.lines().count(),
        2,
        "已知集合是 config/mod.rs 的两处 `#[cfg(test)]` set_var 块；数量变了说明有新增/删除，\
         请复核后更新本断言：\n{all_unsafe}"
    );
}
