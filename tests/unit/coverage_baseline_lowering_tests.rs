#![allow(clippy::unwrap_used, clippy::expect_used)]

//! 守卫：覆盖率棘轮的 `--allow-lower`（合法重构的唯一下调出口）。
//!
//! ## 缺陷
//!
//! `scripts/check_file_coverage.py::save_baseline` 取 `max(previous, current)` ——
//! 上界只降不升，这本身是对的（否则"先降级、基线跟着降"就能绕过棘轮）。但它把
//! **合法重构**也堵死了：
//!
//! * 把一个 600 行文件里已覆盖的逻辑抽到新文件，原文件剩下的多是不易覆盖的分支；
//! * 原文件覆盖率下降，新文件受 ramp-up 门槛约束（整体覆盖没变差）；
//! * 原文件 floor 仍是历史最高值 ⇒ 永久红，`--save-baseline` 也救不了；
//! * 唯一出路是手改 54 KB 的 `coverage_baseline.json`，而那在 review 里
//!   与"偷偷放宽门禁"无法区分。
//!
//! 2026-10-28 加了 `--allow-lower PATH`：逐条点名的文件才允许把 floor 落到本次实测值。
//!
//! ## 钉住三条性质
//!
//! 1. **默认仍是 max**：不点名时，降了就红，且写回的基线保持旧值（不能靠跑一次就洗白）。
//! 2. **点名后：不红 + 写回实测值，而且打印出来**（静默放行与"门禁没在工作"无法区分）。
//! 3. **不扩散**：点名 A 不能让 B 也蒙过去；点名一个不存在的路径必须报错（否则打错字
//!    就是静默空操作）。
//!
//! 全部走真实脚本 + 合成 lcov / baseline，不依赖 cargo 构建。

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

struct Outcome {
    code: i32,
    output: String,
    baseline: String,
}

/// 用合成 lcov + baseline 跑一次真实棘轮，回传退出码、合并输出、以及**写回后**的基线。
///
/// `records` = `(path, lines_found, lines_hit)`；`baseline` = `(path, line_pct)`；
/// `extra_args` 追加在固定参数之后（如 `--allow-lower`）。
fn run_ratchet(records: &[(&str, u32, u32)], baseline: &[(&str, f64)], extra_args: &[&str]) -> Outcome {
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
        .arg(repo_root().join("scripts/check_file_coverage.py"))
        .arg("--report")
        .arg(&report)
        .arg("--baseline")
        .arg(&baseline_file)
        // 把两个绝对门槛压到 0：本文件只测"基线已知文件的回归"这一条路径，
        // 免得 ramp-up / core 门槛把结论搅混。
        .arg("--global-floor")
        .arg("0")
        .arg("--new-file-floor")
        .arg("0")
        .arg("--core-threshold")
        .arg("0")
        .args(extra_args)
        .current_dir(repo_root())
        .output()
        .expect("python3 must be runnable");

    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    Outcome {
        code: output.status.code().unwrap_or(-1),
        output: combined,
        baseline: fs::read_to_string(&baseline_file).unwrap_or_default(),
    }
}

fn pct_of(baseline_json: &str, path: &str) -> f64 {
    // 基线是 `{"files":[{"path":…,"line_pct":…}, …]}`；这里只需要那一个数字。
    let needle = format!("\"path\": \"{path}\"");
    let idx =
        baseline_json.find(&needle).unwrap_or_else(|| panic!("baseline must still contain {path}:\n{baseline_json}"));
    let rest = &baseline_json[idx..];
    let key = "\"line_pct\": ";
    let start = rest.find(key).expect("line_pct key") + key.len();
    let tail = &rest[start..];
    let end = tail.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(tail.len());
    tail[..end].parse().expect("line_pct must be numeric")
}

/// lcov 侧写法（`SF:` 行）。
const PROBE_SF: &str = "synapse-common/src/probe.rs";
/// 基线 / `--allow-lower` 侧写法：`_normalize_path` 之后的 key，**没有** `src/` 段。
const PROBE: &str = "synapse-common/probe.rs";

/// (1) 默认仍是 `max`：降了就红，且写回的基线**保持旧值**。
///
/// 第二半是关键：如果写回跟着降到 80，"跑一次就洗白"这条绕过路径就通了。
#[test]
fn regression_is_red_and_the_floor_stays() {
    let out = run_ratchet(&[(PROBE_SF, 100, 80)], &[(PROBE, 90.0)], &[]);
    assert_eq!(out.code, 1, "覆盖率从 90% 掉到 80% 必须红：\n{}", out.output);
    assert_eq!(
        pct_of(&out.baseline, PROBE),
        90.0,
        "不得把 floor 悄悄降到本次实测值，否则跑一次就洗白：\n{}",
        out.baseline
    );
}

/// (2) 显式点名后：不红、写回实测值、并且**打印**出来。
#[test]
fn explicit_allow_lower_goes_green_and_writes_the_measurement() {
    let out = run_ratchet(&[(PROBE_SF, 100, 80)], &[(PROBE, 90.0)], &["--allow-lower", PROBE]);
    assert_eq!(out.code, 0, "点名的下调不该判红：\n{}", out.output);
    assert_eq!(pct_of(&out.baseline, PROBE), 80.0, "点名的文件必须把 floor 落到本次实测值：\n{}", out.baseline);
    assert!(
        out.output.contains("[LOWERED]") || out.output.contains("Floors lowered on purpose"),
        "下调必须被打印 —— 静默放行与门禁没在工作无法区分：\n{}",
        out.output
    );
    assert!(out.output.contains("--allow-lower"), "输出必须指明这是显式授权（而不是豁免列表）：\n{}", out.output);
}

/// (3) 不扩散：点名 A 不能让同样下降的 B 也蒙过去。
#[test]
fn allow_lower_does_not_leak_to_other_files() {
    let a_sf = "synapse-common/src/a.rs";
    let b_sf = "synapse-common/src/b.rs";
    let a = "synapse-common/a.rs";
    let b = "synapse-common/b.rs";
    let out = run_ratchet(&[(a_sf, 100, 80), (b_sf, 100, 70)], &[(a, 90.0), (b, 90.0)], &["--allow-lower", a]);
    assert_eq!(out.code, 1, "B 没被点名，必须仍然红：\n{}", out.output);
    assert!(out.output.contains(b), "失败信息必须点名没被授权的那个文件：\n{}", out.output);
    assert_eq!(pct_of(&out.baseline, a), 80.0, "A 被授权，floor 应下降");
    assert_eq!(pct_of(&out.baseline, b), 90.0, "B 未被授权，floor 必须保持 90：\n{}", out.baseline);
}

/// (4) 点名一个基线里没有的路径必须报错，而不是静默空操作。
///
/// 打错一个字符，用户会以为"已经放行了"，实际门禁照旧 —— 这种"看起来生效"的
/// 假象比直接报错危险得多。
#[test]
fn allow_lower_typo_is_rejected() {
    // 故意用 lcov 写法（多一段 `src/`）去点名 —— 这是最可能踩的真实陷阱。
    let out = run_ratchet(&[(PROBE_SF, 100, 80)], &[(PROBE, 90.0)], &["--allow-lower", "synapse-common/src/probe.rs"]);
    assert_eq!(out.code, 2, "打错路径必须按用法错误拒绝：\n{}", out.output);
    assert!(out.output.contains("no baseline entry"), "错误信息必须说明该路径没有基线条目：\n{}", out.output);
    assert!(
        out.output.contains("did you mean") && out.output.contains(PROBE),
        "报错必须直接给出 normalize 之后的候选（否则用户只能猜 `src/` 段）：\n{}",
        out.output
    );
}
