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
    # `FAKE_REPORT_FAIL_FOR=<substring>` 让指定的那条腿**生成不出 lcov**，
    # 复刻 run 40637601681 的真实故障（rsproxy 504 ⇒ storage 腿无 profraw）。
    case " $* " in
        *"${FAKE_REPORT_FAIL_FOR:-__never__}"*)
            echo "fake cargo: report failed for $*" >&2
            exit 1
            ;;
    esac
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
    run_script_with(probe, test_rc, max_locks, &[])
}

fn run_script_with(probe: &Probe, test_rc: &str, max_locks: &str, extra_env: &[(&str, &str)]) -> Outcome {
    let inherited = std::env::var("PATH").unwrap_or_else(|_| String::new());
    let mut command = Command::new("bash");
    command
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
        .env_remove("BASH_ENV");
    for (key, value) in extra_env {
        command.env(key, value);
    }
    let output = command.output().expect("run_coverage.sh must be runnable");

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

/// 步骤 2 的报告范围必须等于**全部**工作区成员（含 `synapse-storage`）。
///
/// 历史口径是「成员 − synapse-storage」，理由是 storage 由步骤 1 单独跑。那把**测试范围**
/// 和**报告范围**混成了一件事：storage 被 services/web 依赖并**插桩**编译，集成测试对它的
/// 命中就写在步骤 2 的 profraw 里；报告范围排除它 ⇒ 这些命中被整段丢弃。
///
/// 实测（2026-10-28，步骤 2 形态、只跑 `membership_storage_tests_migrated` 一个文件）：
/// `synapse-storage/membership/mod.rs` 命中 **284/1019 = 27.87%**，旧口径下这 284 行一行
/// 也进不了报告。别拿同目录的 `membership/api.rs` 当证据 —— 它是 `impl MemberStoreApi for
/// RoomMemberStorage` 的委托层，每个方法只有一行转发，而调用方都持有具体类型、不经 trait
/// 对象，所以结构性不可达（恒 0%）。两者的落差与报告口径无关。
///
/// 测试范围仍排除 storage（db_tests 直连 public，并发会互相 DROP），那是**执行**约束；
/// 报告范围则必须是全工作区，否则新增/漏列的 crate 会静默少覆盖。
#[test]
fn rest_report_scope_matches_the_workspace() {
    let script = fs::read_to_string(repo_root().join(SCRIPT)).expect("run_coverage.sh must be readable");
    let declared = parse_rest_report_scope(&script);

    let mut names = cargo_workspace_members();
    names.sort();

    assert_eq!(
        declared,
        names,
        "run_coverage.sh 的 REST_REPORT_ARGS 必须等于**全部**工作区成员。\n\
         差集（漏了的 crate 会静默不参与覆盖率）：{:?}",
        names.iter().filter(|name| !declared.contains(name)).collect::<Vec<_>>()
    );
    assert!(
        declared.iter().any(|name| name == "synapse-storage"),
        "报告范围必须包含 synapse-storage：步骤 2 的集成测试会执行它的代码，\
         排除它等于把那些命中扔掉（实测 `membership/mod.rs` 284/1019 = 27.87% 会被整段丢弃）"
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

/// 一条腿连 lcov 都没产出时，**另一条腿的数据必须活下来**。
///
/// 这根钉子来自一次真实的空手而归（run 40637601681，2026-10-28）：storage 腿在下载依赖时
/// 撞上 rsproxy 的 `504 Gateway Time-out`，tests 没跑、profraw 一行没写；而 `rest.lcov`
/// 其实已经在磁盘上（覆盖 600+ 文件）。旧实现无条件 `merge_lcov.py storage.lcov rest.lcov`，
/// 于是 `FileNotFoundError` traceback 把整轮数据一起抹掉 —— 棘轮跳过，覆盖率车道再次
/// "跑了但没产出"，与 §6.1 里"134 个文件待定性"是同一个机制。
#[test]
fn a_missing_leg_does_not_wipe_the_other_legs_data() {
    let probe = probe("missing_leg");
    // 用 `--output-path` 里的文件名选腿，而不是 `-p synapse-storage`：自从步骤 2 的
    // **报告**范围也包含 storage（它被 services/web 依赖并插桩），`-p synapse-storage`
    // 会同时命中两条腿的报告命令，那就变成"两条腿都失败"，测不到降级合并。
    let outcome = run_script_with(&probe, "0", "256", &[("FAKE_REPORT_FAIL_FOR", "storage.lcov")]);

    assert!(outcome.report_written, "一条腿没产出 lcov 时，另一条腿的数据必须被合并写出。输出：\n{}", outcome.stderr);
    assert!(
        !outcome.stderr.contains("Traceback"),
        "缺输入要给出可归因的报错，不是 Python traceback。输出：\n{}",
        outcome.stderr
    );
    assert!(
        outcome.stderr.contains("只有 1 条腿有数据"),
        "降级合并必须在日志里说明「本轮少了一条腿」。输出：\n{}",
        outcome.stderr
    );
    assert_ne!(outcome.code, 0, "缺腿仍然是失败（红还是红），只是数据不再一起丢。输出：\n{}", outcome.stderr);
}

/// 反面：两条腿都空手时，**不写空报告**，而是明确失败。
///
/// 空的 `coverage/lcov.info` 会被棘轮读成"608 个文件全部 GONE-NOT-BUILT"，
/// 比"没有报告"更难归因 —— 前者看起来像覆盖率全掉，后者一眼就知道是采集失败。
#[test]
fn no_legs_at_all_fails_loudly_instead_of_writing_an_empty_report() {
    let probe = probe("no_legs");
    let outcome = run_script_with(&probe, "0", "256", &[("FAKE_REPORT_FAIL_FOR", "llvm-cov")]);

    assert!(!outcome.report_written, "两条腿都没数据时不许写空报告。输出：\n{}", outcome.stderr);
    assert!(outcome.stderr.contains("两条腿都没有产出 lcov"), "必须点明两条腿都没数据。输出：\n{}", outcome.stderr);
    assert_ne!(outcome.code, 0, "无数据必须非零退出。输出：\n{}", outcome.stderr);
}

/// `merge_lcov.py` **自己不替调用方做取舍**：缺一个输入就 fail-closed。
///
/// 理由与上一条互补：脚本（编排者）可以明确决定"只合并存在的腿并把它写进日志"，
/// 而工具层如果**静默丢弃**缺失输入，半份报告看起来就和完整报告一模一样 ——
/// 缺的文件被下游读成 0% / GONE-NOT-BUILT，比直接失败更难发现。
#[test]
fn merge_lcov_refuses_to_silently_drop_a_missing_input() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("merge_lcov_missing_input");
    fs::create_dir_all(&dir).expect("probe dir");
    let present = dir.join("present.lcov");
    fs::write(&present, "SF:synapse-common/src/probe.rs\nDA:1,1\nLF:1\nLH:1\nend_of_record\n").expect("fixture lcov");
    let absent = dir.join("absent.lcov");
    let output_path = dir.join("merged.lcov");
    for stale in [&absent, &output_path] {
        if stale.exists() {
            fs::remove_file(stale).expect("stale fixture must be removable");
        }
    }

    let output = Command::new("python3")
        .arg(repo_root().join("scripts/merge_lcov.py"))
        .arg(&present)
        .arg(&absent)
        .arg("-o")
        .arg(&output_path)
        .current_dir(repo_root())
        .output()
        .expect("merge_lcov.py must be runnable");
    let text = String::from_utf8_lossy(&output.stderr).into_owned();

    assert_eq!(output.status.code(), Some(2), "缺输入必须 fail-closed。输出：\n{text}");
    assert!(text.contains("not found"), "must name the missing input, got:\n{text}");
    assert!(!output_path.exists(), "缺输入时不许写出半份报告");
}

/// 合并**同一个 SF**时不许把 LF 塌成 DA 行数。
///
/// llvm-cov 的 `LF` 可以大于 `DA` 行数（函数定义行等"幽灵行"有覆盖区域但无执行计数）。
/// 若合并时用 `LF = len(da)` 重算，分母口径就变了，同一个文件的百分比会和它自己的
/// 基线不可比。2026-10-28 实测 `synapse-storage/src/membership/mod.rs`：
/// llvm-cov 450/1019 = 44.2%，按 DA 重算 403/590 = 68.3%，131/152 个 storage 文件受影响。
#[test]
fn merging_the_same_file_from_both_legs_keeps_the_lf_denominator() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("merge_lcov_lf_denominator");
    fs::create_dir_all(&dir).expect("probe dir");
    let leg_a = dir.join("leg_a.lcov");
    let leg_b = dir.join("leg_b.lcov");
    let output_path = dir.join("merged.lcov");
    // 两腿各报 2 个 DA 行，但 LF 都报 10（其余 8 行是幽灵行）。
    fs::write(&leg_a, "SF:synapse-storage/src/probe.rs\nDA:1,1\nDA:2,0\nLF:10\nLH:5\nend_of_record\n")
        .expect("fixture lcov A");
    fs::write(&leg_b, "SF:synapse-storage/src/probe.rs\nDA:1,0\nDA:2,1\nDA:3,0\nLF:10\nLH:5\nend_of_record\n")
        .expect("fixture lcov B");
    if output_path.exists() {
        fs::remove_file(&output_path).expect("stale fixture must be removable");
    }

    let output = Command::new("python3")
        .arg(repo_root().join("scripts/merge_lcov.py"))
        .arg(&leg_a)
        .arg(&leg_b)
        .arg("-o")
        .arg(&output_path)
        .current_dir(repo_root())
        .output()
        .expect("merge_lcov.py must be runnable");
    assert!(output.status.success(), "merge_lcov.py 必须成功。stderr：\n{}", String::from_utf8_lossy(&output.stderr));
    let merged = fs::read_to_string(&output_path).expect("merged output");

    let lf = merged
        .lines()
        .find_map(|l| l.strip_prefix("LF:"))
        .and_then(|v| v.parse::<usize>().ok())
        .expect("merged record must carry LF");
    let lh = merged
        .lines()
        .find_map(|l| l.strip_prefix("LH:"))
        .and_then(|v| v.parse::<usize>().ok())
        .expect("merged record must carry LH");

    assert_eq!(lf, 10, "LF 不许塌成 DA 行数（3 行），实际合并结果：\n{merged}");
    assert_eq!(lh, 5, "LH 不许被 DA 命中行数（2 行）压掉，实际合并结果：\n{merged}");
    assert!(
        merged.contains("DA:1,1") && merged.contains("DA:2,1") && merged.contains("DA:3,0"),
        "DA 仍要按行取 max 求和，实际合并结果：\n{merged}"
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
