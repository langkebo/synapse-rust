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
    let baseline_entries: Vec<String> =
        baseline.iter().map(|(path, pct)| format!("{{\"path\":\"{path}\",\"line_pct\":{pct}}}")).collect();
    let (code, output, _saved) = run_ratchet_in(dir.path(), list, None, records, &baseline_entries);
    (code, output)
}

/// `run_ratchet` 的完整版：可以让调用方指定 `--core-files`，并自定义基线条目
/// （`[CORE-KNOWN]` 的用例需要先埋一个 `target` 字段）。
///
/// 返回 `(exit_code, combined_output, saved_baseline_json)`；`saved` 是脚本按
/// `--save-baseline` 默认写回 `--baseline` 之后的内容。
fn run_ratchet_in(
    workdir: &Path,
    list: &Path,
    core: Option<&Path>,
    records: &[(&str, u32, u32)],
    baseline_entries: &[String],
) -> (i32, String, String) {
    let report = workdir.join("probe.info");
    let baseline_file = workdir.join("baseline.json");

    let mut lcov = String::new();
    for (path, lines_found, lines_hit) in records {
        lcov.push_str(&format!("SF:{path}\nLF:{lines_found}\nLH:{lines_hit}\nend_of_record\n"));
    }
    fs::write(&report, lcov).expect("synthetic lcov must be writable");
    fs::write(&baseline_file, format!("{{\"files\":[{}]}}\n", baseline_entries.join(",")))
        .expect("synthetic baseline must be writable");

    let mut command = Command::new("python3");
    command
        .arg(script_path())
        .arg("--report")
        .arg(&report)
        .arg("--baseline")
        .arg(&baseline_file)
        .arg("--global-floor")
        .arg("40")
        .arg("--new-file-floor")
        .arg("30")
        .arg("--non-unit-coverable")
        .arg(list);
    if let Some(core_list) = core {
        command.arg("--core-files").arg(core_list);
    }
    let output = command.current_dir(repo_root()).output().expect("python3 must be runnable");
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    let saved = fs::read_to_string(&baseline_file).unwrap_or_default();
    (output.status.code().unwrap_or(-1), combined, saved)
}

fn core_list_path() -> PathBuf {
    repo_root().join("scripts/ci/core_file_coverage_prefixes.txt")
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

/// (f) 声明文件（只有 trait/struct 签名、没有函数体）不该被"新文件地板"判红。
///
/// 实测依据（run 40649970973，第一份完整报告 674 个文件）：`synapse-services/src/auth/
/// {credential_auth,token_auth}.rs` 各是 94/55 行的**纯 trait 声明**，在 llvm-cov 里只剩
/// `LF:2 / LH:0` —— 那两行是 `#[async_trait]` 展开的、**不可达**的默认体垫片
/// （每个 impl 都自带方法体）。把它们按"新文件 core ≥70%"判红，是把结构性 0/2 当成
/// 质量缺陷；而这两条曾让 `Core-path coverage failures` 整段变红。
#[test]
fn a_declaration_only_file_is_exempt_from_the_new_file_floor() {
    let (code, output) =
        run_ratchet(&list_path(), &[("synapse-services/src/auth/probe_trait.rs", 2, 0)], &[("src/lib.rs", 50.0)]);

    assert_eq!(code, 0, "a declaration-only file must not fail the new-file floor:\n{output}");
    assert!(
        output.contains("[DECL-ONLY] synapse-services/auth/probe_trait.rs"),
        "the exemption must be printed with its reason, never silent:\n{output}"
    );
    assert!(
        !output.contains("[NEW] synapse-services/auth/probe_trait.rs"),
        "a declared exemption must not also be reported as a floor failure:\n{output}"
    );
}

/// (g) 边界：豁免只覆盖 `LF <= 3`，也不能吞掉"小文件但真的没测试"。
///
/// 4 行可执行、0 命中 ⇒ 仍然是 `[NEW]` 红。放宽这条会让"只写 4 行没人调用"的模块
/// 白拿走豁免，而它和声明文件在语义上完全不同（有函数体 = 可以被测）。
#[test]
fn a_tiny_file_with_bodies_still_fails_the_new_file_floor() {
    let (code, output) =
        run_ratchet(&list_path(), &[("synapse-services/src/auth/probe_tiny.rs", 4, 0)], &[("src/lib.rs", 50.0)]);

    assert_eq!(code, 1, "a four-line file with bodies must still be judged:\n{output}");
    assert!(output.contains("[NEW] synapse-services/auth/probe_tiny.rs"), "expected a [NEW] failure:\n{output}");
    assert!(
        !output.contains("[DECL-ONLY] synapse-services/auth/probe_tiny.rs"),
        "the declaration-only exemption must not cover files with function bodies:\n{output}"
    );
}

/// (h) 进了基线的声明文件也必须**被命名为** `[DECL-ONLY]`，而不是静默通过。
///
/// 没有这条时，同一个文件只因"曾经进过基线"就从 `[DECL-ONLY]` 掉进 `[TOUCHED]` 分支：
/// 结论一样（0.0 不低于 0.0），但审计侧数不出全仓到底有几个结构性 0% 的文件 ——
/// 它们会和"真没测试"混在同一个 0.0 堆里。
#[test]
fn a_baseline_known_declaration_file_is_named_as_decl_only() {
    // 基线键与 lcov 的 SF 都走 `_normalize_path`：根 crate 的 `src/…` 原样保留，
    // 而 `synapse-services/src/…` 会被折成 `synapse-services/…`。用根 crate 路径，
    // 两边键才真的相同（否则这条用例退化成"新文件"分支，什么也没证明）。
    let probe = "src/probe_decl_baselined.rs";
    let (code, output) = run_ratchet(&list_path(), &[(probe, 2, 0)], &[(probe, 0.0)]);

    assert_eq!(code, 0, "a baseline-known declaration-only file must not fail the ratchet:\n{output}");
    assert!(
        output.contains("[DECL-ONLY] src/probe_decl_baselined.rs"),
        "the structural exemption must be printed even for a baseline-known file:\n{output}"
    );
}

/// (i) 反面：基线里那个文件**有过正覆盖** ⇒ 0% 是真跌，不许被声明豁免吞掉。
///
/// `prev > 0` 证明它有可命中行，就不是声明文件。放宽这条等于给"曾经有测试、后来删光"
/// 的文件开一条静默通道。
#[test]
fn a_baseline_known_file_with_positive_coverage_is_not_exempted() {
    let probe = "src/probe_decl_regressed.rs";
    let (code, output) = run_ratchet(&list_path(), &[(probe, 2, 0)], &[(probe, 50.0)]);

    assert_eq!(code, 1, "a baseline-known file that had positive coverage must still be judged:\n{output}");
    assert!(
        output.contains("[TOUCHED]") && output.contains(probe),
        "the regression must be reported as a TOUCHED failure:\n{output}"
    );
    assert!(
        !output.contains("[DECL-ONLY] src/probe_decl_regressed.rs"),
        "the declaration-only exemption must not cover a file that was previously reachable:\n{output}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// `[CORE-KNOWN]`：core 老文件的"改动即负责 + 分档目标"
//
// 规则：`--core-files` 命中的**基线已知**文件，floor = `min(target, recorded)`。
// `target` 按档位播种（0–10% → 30、10–30% → 40、30–50% → 55、50–70% → 70），
// 持久化在基线条目里且只升不降。`min()` 是"不制造红墙"的那一半：3% 的 core 文件
// 不会一夜之间因为没到 30% 而红 —— 它被钉在 3%，直到自己爬过 target。
// ─────────────────────────────────────────────────────────────────────────────

/// (j) 已播种 30 的 3% core 老文件**不会**因为没到档位目标而当天变红。
///
/// 2026-10-29 起 `target` 必须由 `--seed-targets` **离线**写进基线，运行期缺失
/// 直接 fail-closed（见用例 (j0)）。因此这里喂的就是播种后的形态：3% 档 ⇒ 30。
#[test]
fn core_known_file_below_its_tier_target_is_not_red_overnight() {
    let dir = tempfile::tempdir().expect("a temp dir must be creatable");
    let probe = "synapse-services/src/auth/probe_core.rs";
    let key = "synapse-services/auth/probe_core.rs";
    let baseline = vec![format!("{{\"path\":\"{key}\",\"line_pct\":3.0,\"lines_found\":100,\"target\":30.0}}")];

    let (code, output, saved) =
        run_ratchet_in(dir.path(), &list_path(), Some(&core_list_path()), &[(probe, 100, 3)], &baseline);

    assert_eq!(code, 0, "a 3%-covered core file must not be red for missing its 30% target:\n{output}");
    assert!(
        output.contains("Core-known guard: 1 baseline-known core file(s), 0 already at/above target"),
        "the run must show the file was judged on the CORE-KNOWN ladder (and is not at target yet):\n{output}"
    );
    assert!(
        !output.contains("[CORE-KNOWN]"),
        "a pinned-at-recorded-value file must not be reported as a failure:\n{output}"
    );
    assert!(
        saved.contains("\"target\": 30.0"),
        "the seeded tier target (3% -> 30) must survive the write-back unchanged:\n{saved}"
    );
}

/// (j0) 运行期缺 `target` 必须 fail-closed，且不许顺手静默播种。
///
/// 反例：若运行期从 `prev` 现推 `target`，则 `min(target, prev)` 恒等于 `prev`，
/// 分档推进退化成纯 no-regression —— 2026-10-29 之前 683 条基线里 0 条带
/// `target`，机制自上线起就没生效过。所以缺字段不是"补一下就好"，而是判定
/// 这一轮的读数不可信，必须先把基线补齐再谈门禁。
#[test]
fn a_core_baseline_entry_without_a_persisted_target_fails_closed() {
    let dir = tempfile::tempdir().expect("a temp dir must be creatable");
    let probe = "synapse-services/src/auth/probe_core_unseeded.rs";
    let key = "synapse-services/auth/probe_core_unseeded.rs";
    let baseline = vec![format!("{{\"path\":\"{key}\",\"line_pct\":3.0,\"lines_found\":100}}")];

    let (code, output, saved) =
        run_ratchet_in(dir.path(), &list_path(), Some(&core_list_path()), &[(probe, 100, 3)], &baseline);

    assert_eq!(code, 2, "a missing [CORE-KNOWN] target must fail closed:\n{output}");
    assert!(
        output.contains("1 baseline core file(s) have no persisted [CORE-KNOWN] target")
            && output.contains("--seed-targets"),
        "the failure must name the offending entry and the offline seed command:\n{output}"
    );
    assert!(
        !saved.contains("\"target\""),
        "the runtime must not silently seed the field it just declared missing:\n{saved}"
    );
}

/// (k) 有 `target` 且已经到点：低于 target 就是红 —— "到点必须达标"。
#[test]
fn core_known_file_below_a_ratcheted_target_is_red() {
    let dir = tempfile::tempdir().expect("a temp dir must be creatable");
    let probe = "synapse-services/src/auth/probe_core_ratcheted.rs";
    let key = "synapse-services/auth/probe_core_ratcheted.rs";
    // prev 55 / target 55：`min(55, 55) = 55`，cur 45 既跌穿 prev 也低于 target。
    let baseline = vec![format!("{{\"path\":\"{key}\",\"line_pct\":55.0,\"lines_found\":100,\"target\":55.0}}")];

    let (code, output, _saved) =
        run_ratchet_in(dir.path(), &list_path(), Some(&core_list_path()), &[(probe, 100, 45)], &baseline);

    assert_eq!(code, 1, "45% must fail a 55% target (and a 55% recorded value):\n{output}");
    assert!(
        output.contains(&format!("[CORE-KNOWN] {key}")) && output.contains("target 55%"),
        "the failure must name the tag and the target it missed:\n{output}"
    );
}

/// (l) 棘轮只升：`cur > target` 时 target 抬到 `floor(cur/5)*5`，并写回基线。
#[test]
fn core_known_target_ratchets_up_to_the_five_percent_grid() {
    let dir = tempfile::tempdir().expect("a temp dir must be creatable");
    let probe = "synapse-services/src/auth/probe_core_climb.rs";
    let key = "synapse-services/auth/probe_core_climb.rs";
    // 播种值 30（3% 档），这次跑到 63% ⇒ target 应抬到 60，而不是停在 30 或跳到 63。
    let baseline = vec![format!("{{\"path\":\"{key}\",\"line_pct\":3.0,\"lines_found\":100,\"target\":30.0}}")];

    let (code, output, saved) =
        run_ratchet_in(dir.path(), &list_path(), Some(&core_list_path()), &[(probe, 100, 63)], &baseline);

    assert_eq!(code, 0, "63% clears both the recorded 3% and the 30% target:\n{output}");
    assert!(
        saved.contains("\"target\": 60.0"),
        "`cur > target` must raise target to floor(cur/5)*5 = 60 and persist it:\n{saved}"
    );
    assert!(
        saved.contains("\"line_pct\": 63.0"),
        "the recorded value must follow the measurement, not the target:\n{saved}"
    );
}

/// (m) core 标签不能外溢：同一份基线数据、同样的低覆盖，非 core 路径仍是 `[TOUCHED]`。
#[test]
fn non_core_baseline_known_files_keep_the_touched_tag() {
    let dir = tempfile::tempdir().expect("a temp dir must be creatable");
    let probe = "synapse-services/src/not_core/probe_plain.rs";
    let baseline =
        vec![format!("{{\"path\":\"synapse-services/not_core/probe_plain.rs\",\"line_pct\":3.0,\"lines_found\":100}}")];

    let (code, output, _saved) =
        run_ratchet_in(dir.path(), &list_path(), Some(&core_list_path()), &[(probe, 100, 0)], &baseline);

    assert_eq!(code, 1, "0% against a recorded 3% is a regression, core or not:\n{output}");
    assert!(
        output.contains("[TOUCHED] synapse-services/not_core/probe_plain.rs"),
        "only `--core-files` matches may take the CORE-KNOWN branch:\n{output}"
    );
    assert!(!output.contains("[CORE-KNOWN]"), "the core tag must not leak onto non-core paths:\n{output}");
}
