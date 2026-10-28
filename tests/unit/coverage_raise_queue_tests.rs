//! 守卫：覆盖率「提升队列」台账（`scripts/ci/coverage_raise_queue.json`）必须
//! **逐文件记账**，而且"提升到 30%"这个目标**不能**变成绝对下限。
//!
//! ## 为什么这层守卫不能省
//!
//! 审计 P2 的原话是"给 0% 文件里有生产入口的建『提升到 30%』的一次性任务，逐文件记账
//! （**不是**一刀切重贴全局下限 —— 那会重演 188 文件红）"。这句话里有两个相反的诱惑：
//!
//! * 只写文档 ⇒ 三个月后没人知道这三条到底提到了多少（台账变第二个真相）；
//! * 把 30% 直接当下限 ⇒ 今日立刻 **3 条红**，而 `188 files red` 那次证明：红的是
//!   "从没达标"，不是"回退了"。一个刚上线就红着的门禁，两周内必然被静音。
//!
//! 所以本文件钉住的是第三种形态：
//!
//! 1. `check_file_coverage.py --raise-queue` 只**打印**进度，退出码不受影响（目标不是下限）；
//! 2. `check_coverage_raise_queue.py` 才是牙齿 —— 条目达到目标仍在册 ⇒ 红（防僵尸），
//!    记账起点高于当前基线 ⇒ 红（floor 被偷偷下调）；
//! 3. 台账不许**悄悄**被删空：删空是"任务完成"的动作，不是"让门禁变绿"的动作 —— 所以
//!    最后一条也达标时，必须附一份带实测值的 `closed` 收尾块（判据 9–11）。三种形态各有
//!    各的命运：有条目 = 开着；空 + 收尾块 = 关着；空 + 什么都没有 = 任务被丢弃（红）。
//!
//! ## 为什么不跑真覆盖率
//!
//! 一轮 `cargo llvm-cov` 要 30–60 分钟，且本地这条腿本来就不产出报告
//! （`docs/audit/TEST_QUALITY_FINDINGS_2026-10-28.md` §6.1）。这里给合成 lcov +
//! 合成基线：断言的是**判据本身**（标签、退出码、红/绿阈值），与真实数值无关。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn ratchet_script() -> PathBuf {
    repo_root().join("scripts/check_file_coverage.py")
}

fn ledger_script() -> PathBuf {
    repo_root().join("scripts/ci/check_coverage_raise_queue.py")
}

fn committed_ledger() -> PathBuf {
    repo_root().join("scripts/ci/coverage_raise_queue.json")
}

/// 台账条目：`(path, opened_at, target)`。
type Row<'a> = (&'a str, f64, f64);

/// 写一份**字段齐全**的台账。`plan` / `evidence` 保持非空且足够长 ——
/// 守卫的判据 8 只在这两个字段上失守时才会红，测试里不该顺手触发它。
fn write_ledger(dir: &Path, rows: &[Row<'_>]) -> PathBuf {
    let entries: Vec<String> = rows
        .iter()
        .map(|(path, opened_at, target)| {
            format!(
                r#"{{"path":"{path}","opened_at":{opened_at},"target":{target},"state":"needs-tests","production_entry":"src/server/mod.rs:1 — probe","plan":"probe plan that is long enough to pass the ledger check","evidence":"probe evidence"}}"#
            )
        })
        .collect();
    let out = dir.join("queue.json");
    fs::write(&out, format!("{{\"entries\":[{}]}}\n", entries.join(","))).expect("synthetic ledger must be writable");
    out
}

fn write_baseline(dir: &Path, rows: &[(&str, f64)]) -> PathBuf {
    let entries: Vec<String> =
        rows.iter().map(|(path, pct)| format!(r#"{{"path":"{path}","line_pct":{pct}}}"#)).collect();
    let out = dir.join("baseline.json");
    fs::write(&out, format!("{{\"files\":[{}]}}\n", entries.join(","))).expect("synthetic baseline must be writable");
    out
}

fn write_lcov(dir: &Path, rows: &[(&str, u32, u32)]) -> PathBuf {
    let mut body = String::new();
    for (path, lines_found, lines_hit) in rows {
        body.push_str(&format!("SF:{path}\nLF:{lines_found}\nLH:{lines_hit}\nend_of_record\n"));
    }
    let out = dir.join("probe.info");
    fs::write(&out, body).expect("synthetic lcov must be writable");
    out
}

/// `python3 <script> <args…>`，返回 `(退出码, stdout+stderr)`。
fn run(script: &Path, args: &[&str]) -> (i32, String) {
    let output = Command::new("python3")
        .arg(script)
        .args(args)
        .current_dir(repo_root())
        .output()
        .expect("python3 must be runnable");
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    (output.status.code().unwrap_or(-1), combined)
}

fn utf8(path: &Path) -> &str {
    path.to_str().expect("temp paths are UTF-8 on this platform")
}

/// 跑棘轮：`records` 是 `(path, LF, LH)`，`rows` 是台账条目。
///
/// 基线与 `--save-baseline` 都落在临时目录里：棘轮默认会**写回**基线
/// （`save_path = args.save_baseline or args.baseline`），指到仓库里就是事故。
fn run_ratchet(dir: &Path, records: &[(&str, u32, u32)], rows: &[Row<'_>]) -> (i32, String) {
    let report = write_lcov(dir, records);
    let baseline = write_baseline(dir, &[("src/probe_zero.rs", 0.0), ("src/probe_half.rs", 40.0)]);
    let queue = write_ledger(dir, rows);
    let saved = dir.join("saved.json");
    run(
        &ratchet_script(),
        &[
            "--report",
            utf8(&report),
            "--baseline",
            utf8(&baseline),
            "--save-baseline",
            utf8(&saved),
            "--global-floor",
            "40",
            "--new-file-floor",
            "50",
            "--raise-queue",
            utf8(&queue),
        ],
    )
}

fn run_gate(queue: &Path, baseline: &Path) -> (i32, String) {
    run(&ledger_script(), &["--queue", utf8(queue), "--baseline", utf8(baseline)])
}

/// (a) 台账只报告、不判红：0% 的文件配上 30% 目标，退出码仍是 0。
///
/// 这条是整份台账的**立论基础**。如果它红了，说明有人把 target 接成了 floor，
/// 于是 2026-09-19 的 `188 files red` 会以"3 条红"的形式重演一遍。
#[test]
fn a_raise_row_is_reported_but_never_fails_the_ratchet() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (code, text) = run_ratchet(dir.path(), &[("src/probe_zero.rs", 100, 0)], &[("src/probe_zero.rs", 0.0, 30.0)]);

    assert_eq!(code, 0, "a raise target must not act as an absolute floor:\n{text}");
    assert!(
        text.contains("[RAISE] src/probe_zero.rs: 0.0% → 30%"),
        "progress line must name the measured value and the target:\n{text}"
    );
}

/// (b) 达标必须是 `[RAISE-DONE]`（提醒删条目），而不是"及格"或"失败"。
#[test]
fn a_met_target_is_reported_as_done_so_the_row_gets_deleted() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (code, text) = run_ratchet(dir.path(), &[("src/probe_half.rs", 100, 40)], &[("src/probe_half.rs", 40.0, 30.0)]);

    assert_eq!(code, 0, "reaching the target is good news, not a failure:\n{text}");
    assert!(
        text.contains("[RAISE-DONE] src/probe_half.rs: 40.0% ≥ 30%"),
        "a met target must say 'delete this row':\n{text}"
    );
}

/// (c) 报告里没有该文件 ≠ 0%。这两件事被混为一谈时，台账会"看起来在推进"。
#[test]
fn a_file_missing_from_the_report_is_nodata_not_zero() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (code, text) = run_ratchet(
        dir.path(),
        &[("src/probe_zero.rs", 100, 0)],
        &[("src/probe_zero.rs", 0.0, 30.0), ("src/probe_absent.rs", 0.0, 30.0)],
    );

    assert_eq!(code, 0, "a missing file is a reporting problem, not a ratchet failure:\n{text}");
    assert!(text.contains("[RAISE-NODATA] src/probe_absent.rs"), "an absent file must not be reported as 0%:\n{text}");
    assert!(
        !text.contains("[RAISE] src/probe_absent.rs"),
        "an absent file must not get a numeric progress line:\n{text}"
    );
}

/// (d) 台账路径不存在 ⇒ 退出 2（fail-closed），不是"没有台账要报告"。
#[test]
fn a_missing_ledger_is_a_hard_error() {
    let dir = tempfile::tempdir().expect("temp dir");
    let report = write_lcov(dir.path(), &[("src/probe_zero.rs", 10, 10)]);
    let baseline = write_baseline(dir.path(), &[("src/probe_zero.rs", 100.0)]);
    let missing = dir.path().join("nope.json");
    let (code, text) = run(
        &ratchet_script(),
        &["--report", utf8(&report), "--baseline", utf8(&baseline), "--raise-queue", utf8(&missing)],
    );

    assert_eq!(code, 2, "an unusable ledger must not degrade to 'nothing to report':\n{text}");
    assert!(text.contains("raise queue not found"), "the error must name the problem:\n{text}");
}

/// (e) 已达标仍在册 ⇒ 守卫判红，且信息是"删条目"而不是"提高覆盖率"。
#[test]
fn the_ledger_gate_blocks_a_row_that_reached_its_target() {
    let dir = tempfile::tempdir().expect("temp dir");
    let baseline = write_baseline(dir.path(), &[("src/probe_done.rs", 30.0)]);
    let queue = write_ledger(dir.path(), &[("src/probe_done.rs", 0.0, 30.0)]);
    let (code, text) = run_gate(&queue, &baseline);

    assert_eq!(code, 1, "an achieved row left in the ledger must fail:\n{text}");
    assert!(text.contains("delete this entry"), "the fix is deleting the row, not raising coverage further:\n{text}");
}

/// (f) 记账起点高于当前基线 ⇒ 判红：说明 floor 被 `--allow-lower` 或手改下调了。
///
/// 起点取 20%（而不是 40%）：`opened_at >= target` 会先命中"这一行根本没在提升"的
/// **结构**判据（退出 2），那样就走不到这里要测的 blocking 分支了。20% → 30% 是
/// 一本自洽的账，与基线 5% 冲突才是真问题。
#[test]
fn the_ledger_gate_blocks_a_start_value_above_the_baseline() {
    let dir = tempfile::tempdir().expect("temp dir");
    let baseline = write_baseline(dir.path(), &[("src/probe_drop.rs", 5.0)]);
    let queue = write_ledger(dir.path(), &[("src/probe_drop.rs", 20.0, 30.0)]);
    let (code, text) = run_gate(&queue, &baseline);

    assert_eq!(code, 1, "a lowered floor must be reconciled with the ledger:\n{text}");
    assert!(text.contains("was lowered"), "the message must name the cause (--allow-lower / hand edit):\n{text}");
}

/// (g) 台账不许被删空：空 `entries` 且**没有收尾块** = "任务被静默丢弃"，不是"全部完成"。
///
/// 与 (j) 配对：空台账有两种命运，区别只在有没有那份带数字的 `closed`。
#[test]
fn the_ledger_gate_rejects_an_empty_ledger() {
    let dir = tempfile::tempdir().expect("temp dir");
    let baseline = write_baseline(dir.path(), &[("src/probe_zero.rs", 0.0)]);
    let queue = dir.path().join("empty.json");
    fs::write(&queue, "{\"entries\":[]}\n").expect("empty ledger must be writable");
    let (code, text) = run_gate(&queue, &baseline);

    assert_eq!(code, 2, "an empty ledger means the P2 task vanished:\n{text}");
    assert!(text.contains("dropped silently"), "the message must say what happened:\n{text}");
}

/// (h) 提交进仓库的那份台账必须仍覆盖 P2 点名的三条文件 —— **条目或收尾封存都算**。
///
/// 这条不测"判据"，测的是**任务本身没被悄悄缩水**：删掉一条最省事的理由就是
/// "它达不到 30%"，那正是台账要防止的事。台账按"达成即删"关账后，数字搬到
/// `closed.measurements` —— 所以"覆盖面"只能在这两个地方找，缺一处就算缩水。
#[test]
fn the_committed_ledger_still_accounts_for_the_three_p2_files() {
    let source = fs::read_to_string(committed_ledger()).expect("committed ledger must be readable");
    let parsed: Value = serde_json::from_str(&source).expect("committed ledger must be valid JSON");
    let entries = parsed["entries"].as_array().expect("ledger must have an `entries` array");
    let measurements = parsed["closed"]["measurements"].as_object();

    let mut accounted: Vec<&str> = entries.iter().filter_map(|entry| entry["path"].as_str()).collect();
    if let Some(map) = measurements {
        accounted.extend(map.keys().map(String::as_str));
    }

    for expected in ["src/server/router.rs", "src/tasks/mod.rs", "synapse-common/logging.rs"] {
        assert!(
            accounted.contains(&expected),
            "{expected} is missing from the P2 raise ledger (neither an open row nor a recorded \
             measurement) — removing a row is a decision, not a way to make the gate green \
             (found {accounted:?})"
        );
    }

    for entry in entries {
        let target = entry["target"].as_f64().expect("target must be a number");
        assert!(
            (target - 30.0).abs() < f64::EPSILON,
            "this ledger is the 30% tier; {} declares {target}",
            entry["path"]
        );
    }

    // 封存值低于 30% ⇒ 明明没达成却把台账关了。守卫也查这条，这里再加一道，
    // 免得"从 JSON 里读一遍"和"跑一遍守卫"两边同时被改坏。
    if let Some(map) = measurements {
        for (path, pct) in map {
            let pct = pct.as_f64().expect("recorded measurement must be a number");
            assert!(pct >= 30.0, "{path} is closed at {pct}% — below the 30% target");
        }
    }
}

/// (j) 关账要拿数字说话：`closed` + 残留条目 ⇒ 矛盾；封存值 < 30% ⇒ 假收尾。
#[test]
fn the_ledger_gate_only_accepts_a_closure_backed_by_numbers() {
    let dir = tempfile::tempdir().expect("temp dir");
    let baseline = write_baseline(dir.path(), &[("src/probe_done.rs", 30.0)]);

    let closure = r#""why":"fixture closure that is long enough to pass","measurements":{"src/probe_done.rs":82.9}"#;

    // ① 关账了却还有条目：两句互相矛盾的话，守卫必须挑出来。
    let contradiction = dir.path().join("contradiction.json");
    fs::write(
        &contradiction,
        format!(
            r#"{{"entries":[{{"path":"src/probe_done.rs","opened_at":0.0,"target":30.0,"state":"needs-tests","production_entry":"src/server/mod.rs:1 — probe","plan":"probe plan that is long enough to pass the ledger check","evidence":"probe evidence"}}],"closed":{{"at":"2026-10-28",{closure}}}}}"#
        ),
    )
    .expect("fixture must be writable");
    let (code, text) = run_gate(&contradiction, &baseline);
    assert_eq!(code, 2, "a closed ledger with rows left is contradictory:\n{text}");
    assert!(text.contains("reopened ledger"), "the message must name the contradiction:\n{text}");

    // ② 封存值低于目标：这就是"没做完却盖章"，必须红。
    let undersold = dir.path().join("undersold.json");
    fs::write(
        &undersold,
        r#"{"entries":[],"closed":{"at":"2026-10-28","why":"fixture closure that is long enough to pass","measurements":{"src/probe_done.rs":12.0}}}"#,
    )
    .expect("fixture must be writable");
    let (code, text) = run_gate(&undersold, &baseline);
    assert_eq!(code, 2, "a closure below the target is not evidence:\n{text}");
    assert!(text.contains("only legal once every row is met"), "the message must say why:\n{text}");
}

/// (k) 「测到了」≠「棘轮已确认」：基线还没吸收实测值时，收尾块必须说清等哪一轮。
///
/// 这条是 P2 关账时最容易骗自己的地方：本地腿测出 82.9%，提交版基线却还是 0.0% ——
/// 抬升要等一轮真实的 `Code Coverage` 跑到自动写回那一步。少一句说明，关账就会被读成
/// "机器已经确认过了"。
#[test]
fn the_ledger_gate_demands_a_pending_note_while_the_baseline_lags() {
    let dir = tempfile::tempdir().expect("temp dir");
    let baseline = write_baseline(dir.path(), &[("src/probe_done.rs", 0.0)]);
    let queue = dir.path().join("closed.json");
    fs::write(
        &queue,
        r#"{"entries":[],"closed":{"at":"2026-10-28","why":"fixture closure that is long enough to pass","measurements":{"src/probe_done.rs":82.9}}}"#,
    )
    .expect("fixture must be writable");
    let (code, text) = run_gate(&queue, &baseline);

    assert_eq!(code, 2, "a closure the baseline has not absorbed needs a pending note:\n{text}");
    assert!(text.contains("absorption_pending"), "the gate must name the field to add:\n{text}");
}

/// (l) 吸收完成后那句说明必须删掉 —— 它与 (k) 是同一条判据的两个方向。
///
/// "现在为真的句子，条件一变就是假的"：把 `absorption_pending` 留到基线已经 82.9% 之后，
/// 它就从证据变成了误导（读者会以为基线还是 0%）。
#[test]
fn the_ledger_gate_rejects_a_pending_note_that_outlived_its_cause() {
    let dir = tempfile::tempdir().expect("temp dir");
    let baseline = write_baseline(dir.path(), &[("src/probe_done.rs", 82.9)]);
    let queue = dir.path().join("absorbed.json");
    fs::write(
        &queue,
        r#"{"entries":[],"closed":{"at":"2026-10-28","why":"fixture closure that is long enough to pass","measurements":{"src/probe_done.rs":82.9},"absorption_pending":"fixture note that is long enough to pass"}}"#,
    )
    .expect("fixture must be writable");
    let (code, text) = run_gate(&queue, &baseline);

    assert_eq!(code, 2, "a stale absorption note is a contradiction:\n{text}");
    assert!(text.contains("is stale"), "the message must say why it is wrong:\n{text}");
}

/// (i) 判据自证：八次注入必须各自被报出来（与 CI 那一步同一命令）。
#[test]
fn the_ledger_gate_self_proves_its_own_rules() {
    let (code, text) = run(&ledger_script(), &["--mutation-check"]);
    assert_eq!(code, 0, "the mutation check must pass on its own fixtures:\n{text}");
    assert!(text.contains("mutation-check OK"), "the self-proof must print a verdict:\n{text}");
    assert!(!text.contains("MISS"), "every injected defect must be detected:\n{text}");
}
