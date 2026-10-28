//! 守卫：`scripts/ci/run_coverage.sh` 必须做到「测试失败也留下覆盖率数据」，且红还是红。
//!
//! ## 为什么要有这层守卫
//!
//! 旧实现把 `--lcov --output-path` 直接交给 `cargo llvm-cov`，也就是把"跑测试"和
//! "写报告"绑成一次调用。而 cargo-llvm-cov 在测试失败时**根本不写报告**
//! （0.8.7 实测，2026-10-28）。后果不是"少一个数字"，而是整条车道的产出被抹掉：
//!
//! ```text
//! Run coverage 红（一条测试假红）
//!   ⇒ coverage/lcov.info 不存在
//!   ⇒ Per-file coverage ratchet 无数据可比
//!   ⇒ per-file 棘轮**从未真正执行过**
//! ```
//!
//! 这正是"134 个文件未逐条定性"的成因（`docs/audit/TEST_QUALITY_FINDINGS_2026-10-28.md` §6.1）。
//! 现在的做法是拆成两步：`--no-report` 只跑测试（失败也无妨，profraw 已落盘），
//! 再由 `cargo llvm-cov report` 从 profraw 生成报告；退出码仍取测试的结论。
//!
//! 本文件把三件事钉住：
//!
//! 1. `--no-report` / `--no-fail-fast` 必须落在 **`--` 之前**（它们是 cargo 参数；
//!    放到 `--` 之后会被 libtest 当成未知参数，整条腿直接报 unexpected argument）；
//! 2. 测试腿失败时**报告仍然写出**（否则回到"无数据"）；
//! 3. 测试腿失败时脚本**仍以非零退出**（否则"有数据"会变成"假绿"——这比无数据更糟）。
//!
//! ## 为什么不跑真 cargo
//!
//! 单测批次共享 `target/`，嵌套 cargo 会与其它构建抢锁；真覆盖率一轮 30–60 分钟。
//! 这里用一个**假 cargo**：把收到的 argv 落盘、按 `FAKE_TEST_RC` 决定测试腿的退出码、
//! 收到 `llvm-cov report` 时写一份最小 lcov。于是"注入了什么参数、顺序如何、
//! 报告有没有写出来、退出码是多少"四件事都能直接断言。

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const SCRIPT: &str = "scripts/ci/run_coverage.sh";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// 一次探针的产物目录。`case` 必须每个用例互不相同：nextest 默认并行，
/// 共用一个目录会让并发用例的 argv.log / lcov 相互污染。
struct Probe {
    bin: PathBuf,
    argv_log: PathBuf,
    output_dir: PathBuf,
    target_dir: PathBuf,
}

fn write_executable(path: &Path, body: &str) {
    fs::write(path, body).expect("probe script must be writable");
    let mut perms = fs::metadata(path).expect("probe script metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).expect("probe script must be executable");
}

fn probe(case: &str) -> Probe {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("coverage_leg_probe").join(case);
    let bin = root.join("bin");
    fs::create_dir_all(&bin).expect("probe bin dir must be creatable");
    let argv_log = root.join("argv.log");
    if argv_log.exists() {
        fs::remove_file(&argv_log).expect("stale argv log must be removable");
    }

    write_executable(
        &bin.join("cargo"),
        r#"#!/usr/bin/env bash
printf '%s\n' "$*" >> "$PROBE_ARGV"

# 只有 `cargo llvm-cov report …` 会写报告：这正是被守卫的那条「兜底」路径。
if [ "$1" = "llvm-cov" ] && [ "$2" = "report" ]; then
    out=""
    prev=""
    for arg in "$@"; do
        if [ "$prev" = "--output-path" ]; then out="$arg"; fi
        prev="$arg"
    done
    [ -n "$out" ] || { echo "fake cargo: report without --output-path" >&2; exit 1; }
    mkdir -p "$(dirname "$out")"
    printf 'SF:synapse-common/src/probe.rs\nDA:1,1\nLF:1\nLH:1\nend_of_record\n' > "$out"
    exit 0
fi

exit "${FAKE_TEST_RC:-0}"
"#,
    );

    // 脚本只对 `cargo-llvm-cov` 做 `command -v` 自检。
    write_executable(&bin.join("cargo-llvm-cov"), "#!/usr/bin/env bash\nexit 0\n");

    // 前置自检用 `psql` 拿两件事：模板表数（必须 >100）与锁表上限（只报告）。
    write_executable(
        &bin.join("psql"),
        r#"#!/usr/bin/env bash
case "$*" in
  *max_locks_per_transaction*) printf '%s\n' "${FAKE_MAX_LOCKS:-256}" ;;
  *information_schema.tables*) printf '222\n' ;;
  *) printf '\n' ;;
esac
"#,
    );

    Probe { bin, argv_log, output_dir: root.join("coverage"), target_dir: root.join("target") }
}

struct Outcome {
    code: i32,
    argv: String,
    stderr: String,
    report_written: bool,
}

fn run_script(probe: &Probe, test_rc: &str, max_locks: &str) -> Outcome {
    let inherited = std::env::var("PATH").unwrap_or_else(|_| String::new());
    let output = Command::new("bash")
        .arg(SCRIPT)
        .current_dir(repo_root())
        .env("PATH", format!("{}:{}", probe.bin.display(), inherited))
        .env("PROBE_ARGV", &probe.argv_log)
        .env("FAKE_TEST_RC", test_rc)
        .env("FAKE_MAX_LOCKS", max_locks)
        .env("OUTPUT_DIR", &probe.output_dir)
        // 脚本会清掉 `$CARGO_TARGET_DIR/llvm-cov-target` 下的残留 profraw。指到探针目录，
        // 免得一条单测去删**真实** target 里别人正在写的数据。
        .env("CARGO_TARGET_DIR", &probe.target_dir)
        .env("DATABASE_URL", "postgresql://synapse:probe@localhost:5432/synapse_test")
        .env("TEST_DATABASE_URL", "postgresql://synapse:probe@localhost:5432/synapse_test")
        .env("TEST_DB_TEMPLATE_SCHEMA", "test_template_ci")
        .env("DB_PASSWORD", "probe")
        .env("SKIP_CLEANUP", "1")
        // `BASH_ENV` 会把外层 shell 的 shim 目录重新前置到 PATH（WorkBuddy CLI 的行为），
        // 那会让假 cargo 被真 cargo 覆盖 —— 本用例就变成"什么也没证明"。
        .env_remove("BASH_ENV")
        .output()
        .expect("run_coverage.sh must be runnable");

    Outcome {
        code: output.status.code().unwrap_or(-1),
        argv: fs::read_to_string(&probe.argv_log).unwrap_or_else(|_| String::new()),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned() + &String::from_utf8_lossy(&output.stdout),
        report_written: probe.output_dir.join("lcov.info").is_file(),
    }
}

/// `--no-report` / `--no-fail-fast` 是 cargo 参数。步骤 2 的命令以 `-- --skip …` 结尾，
/// 一旦追加到末尾就会被交给 libtest（`unexpected argument`），整条腿不产出任何数据。
#[test]
fn llvm_cov_flags_land_before_the_harness_separator() {
    let probe = probe("flag_placement");
    let outcome = run_script(&probe, "0", "256");

    let mut seen_storage = false;
    let mut seen_rest = false;
    for line in outcome.argv.lines() {
        if line.starts_with("llvm-cov report") {
            continue;
        }
        let (cargo_args, harness_args) = match line.find(" -- ") {
            Some(index) => (&line[..index], &line[index..]),
            None => (line, ""),
        };
        assert!(
            cargo_args.contains("--no-report") && cargo_args.contains("--no-fail-fast"),
            "cargo 参数里必须同时有 --no-report 与 --no-fail-fast，实得：{line}"
        );
        assert!(
            !harness_args.contains("--no-report") && !harness_args.contains("--no-fail-fast"),
            "这些旗标不能落到 `--` 之后（会被 libtest 当成未知参数），实得：{line}"
        );
        if cargo_args.contains("-p synapse-storage") {
            seen_storage = true;
        }
        if cargo_args.contains("--workspace --exclude synapse-storage") {
            seen_rest = true;
            assert!(
                harness_args.contains("--skip ledger_export_tests"),
                "步骤 2 的 `-- --skip ledger_export_tests` 必须还在，实得：{line}"
            );
        }
    }
    assert!(seen_storage, "步骤 1 必须真的被调用，实得 argv：\n{}", outcome.argv);
    assert!(seen_rest, "步骤 2 必须真的被调用，实得 argv：\n{}", outcome.argv);
}

/// `cargo llvm-cov report` **不接受 `--workspace`**，缺省按"当前包"过滤。本仓根 crate 是
/// `synapse-rust`（只有 `src/` 与 `scripts/`），于是 run 40621674176 实测报告从 627 个文件
/// 缩到 **18 个**（`files=18, coverage=36.46%`），棘轮会把其余文件读成 `GONE-NOT-BUILT`。
/// 所以每条腿的 `report` 都必须显式带包范围。
#[test]
fn every_report_carries_an_explicit_package_scope() {
    let probe = probe("report_scope");
    let outcome = run_script(&probe, "0", "256");

    let mut reports = 0;
    for line in outcome.argv.lines() {
        if !line.starts_with("llvm-cov report") {
            continue;
        }
        reports += 1;
        assert!(line.contains("-p "), "report 必须显式给包范围（缺省只有根 crate ⇒ 627 → 18 个文件），实得：{line}");
        assert!(!line.contains("--workspace"), "report 不接受 --workspace，写上去会直接被 clap 拒绝，实得：{line}");
    }
    assert_eq!(reports, 2, "两条腿各应生成一次报告，实得 argv：\n{}", outcome.argv);
}

/// 步骤 2 的报告范围必须等于「工作区成员 − synapse-storage」，否则新增 crate 会静默
/// 少覆盖（报告里连它的文件都不会出现，棘轮与定性工具都看不见）。
#[test]
fn rest_report_scope_matches_the_workspace() {
    let script = fs::read_to_string(repo_root().join(SCRIPT)).expect("run_coverage.sh must be readable");
    let declared = parse_rest_report_scope(&script);

    let mut names: Vec<String> =
        cargo_workspace_members().into_iter().filter(|name| name != "synapse-storage").collect();
    names.sort();

    assert_eq!(
        declared,
        names,
        "run_coverage.sh 的 REST_REPORT_ARGS 必须等于「工作区成员 − synapse-storage」。\n\
         差集（漏了的 crate 会静默不参与覆盖率）：{:?}",
        names.iter().filter(|name| !declared.contains(name)).collect::<Vec<_>>()
    );
}

fn parse_rest_report_scope(script: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut inside = false;
    for line in script.lines() {
        if line.starts_with("REST_REPORT_ARGS=(") {
            inside = true;
            continue;
        }
        if inside {
            if line.starts_with(')') {
                break;
            }
            if let Some(name) = line.trim().strip_prefix("-p ") {
                names.push(name.trim().to_string());
            }
        }
    }
    names.sort();
    names
}

fn cargo_workspace_members() -> Vec<String> {
    let output = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1", "--locked"])
        .current_dir(repo_root())
        .output()
        .expect("cargo metadata must be runnable");
    assert!(output.status.success(), "cargo metadata failed: {}", String::from_utf8_lossy(&output.stderr));
    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("cargo metadata must be valid JSON");

    // 必须走 `packages[]`，不能对全文扫 `"name":"…"`：`--no-deps` 仍会输出每个包的
    // `dependencies` 数组，那里面的 `name` 是 serde/tokio 之类，扫全文必然误判。
    let mut names: Vec<String> = metadata["packages"]
        .as_array()
        .expect("cargo metadata must carry a packages array")
        .iter()
        .filter_map(|package| package["name"].as_str().map(str::to_string))
        .collect();
    names.sort();
    names.dedup();
    names
}

/// 核心断言：一条腿的测试红，不许把整份报告一起抹掉。
#[test]
fn a_failing_leg_still_writes_the_report() {
    let probe = probe("failing_leg");
    let outcome = run_script(&probe, "101", "256");

    assert!(
        outcome.report_written,
        "测试腿失败时也必须写出覆盖率报告（否则棘轮永远无数据可比）。stderr/stdout：\n{}",
        outcome.stderr
    );
    assert_ne!(outcome.code, 0, "测试失败必须仍以非零退出（否则就是假绿）。输出：\n{}", outcome.stderr);
    assert!(
        outcome.stderr.contains("WARNING") && outcome.stderr.contains("rc=101"),
        "日志必须点明是哪条腿失败及其退出码，便于归因。输出：\n{}",
        outcome.stderr
    );
}

/// 反面：测试全绿时脚本不能因为"兜底逻辑"自己变红。
#[test]
fn a_green_run_exits_zero() {
    let probe = probe("green_run");
    let outcome = run_script(&probe, "0", "256");

    assert_eq!(outcome.code, 0, "全绿必须 exit 0，实得 {}。输出：\n{}", outcome.code, outcome.stderr);
    assert!(outcome.report_written, "全绿时报告必须写出");
}

/// 残留 profraw 必须被清掉：`cargo llvm-cov report` 不区分 profraw 是哪一轮写的，
/// 上一轮被 kill 留下的数据会混进本轮 ⇒ 覆盖率虚高 ⇒ **假绿**（比无数据更糟）。
#[test]
fn stale_profraw_from_a_killed_run_is_purged() {
    let probe = probe("stale_profraw");
    let profraw_dir = probe.target_dir.join("llvm-cov-target");
    fs::create_dir_all(&profraw_dir).expect("fake llvm-cov target dir");
    let stale = profraw_dir.join("synapse-rust-1-2_0.profraw");
    fs::write(&stale, b"stale").expect("stale profraw fixture");

    let outcome = run_script(&probe, "0", "256");

    assert_eq!(outcome.code, 0, "清残留不应改变结论。输出：\n{}", outcome.stderr);
    assert!(!stale.exists(), "上一轮被 kill 留下的 profraw 必须先删掉，否则会污染这次的报告");
    assert!(
        outcome.stderr.contains("清掉上一轮残留的 profraw: 1 个"),
        "清理数量要打印出来（静默清理等于无法判断是否真的清过）。输出：\n{}",
        outcome.stderr
    );
}

/// 锁表上限是环境前置条件，且它的缺失会被表达成"上百个测试失败"。
/// 脚本不替人改配置（CI 的 service container 传不了 `-c`），但必须把它打印出来。
#[test]
fn the_lock_table_ceiling_is_reported_and_flagged() {
    let low = probe("lock_table_low");
    let outcome = run_script(&low, "0", "64");
    assert!(
        outcome.stderr.contains("max_locks_per_transaction = 64"),
        "必须打印实际锁表上限。输出：\n{}",
        outcome.stderr
    );
    assert!(
        outcome.stderr.contains("tune_test_db.sh"),
        "低于阈值时必须给出可执行的修法（不是「可能有点慢」这种描述）。输出：\n{}",
        outcome.stderr
    );
}
