//! Gate tests for `scripts/check_pagination_benchmark.py` 以及
//! bench harness 的"静默跳过"防护（`BENCH_REQUIRE`）。
//!
//! ## 为什么需要这个文件
//!
//! `.github/workflows/benchmark.yml` 的阻塞步骤
//! `python3 scripts/check_pagination_benchmark.py benchmark.txt --minimum-improvement 0.30`
//! 断言 `benchmark.txt` 含 `pagination_offset_deep_page` 与
//! `pagination_keyset_deep_page` 两行 Criterion 输出。
//!
//! 但这两个基准在 **`8c7b4860`（2026-06-05）** 随
//! `benchmark_pagination_strategies` 一起被从
//! `benches/performance_api_benchmarks.rs` 删除，而 workflow 的这一步
//! 没有被同步移除。于是：
//!
//! * 脚本 `raise SystemExit("pagination benchmark rows were not found ...")` → EXIT=1
//! * 该步骤在 **push/PR 上必然失败**（除非有人手工放入含这两行的 `benchmark.txt`）
//! * 该门禁守护的分页性能**完全没有被测量**
//!
//! 同类问题的第二面：`performance_api_benchmarks` 中 11 个基准里有 10 个
//! 依赖 homeserver 或 `BENCH_ADMIN_TOKEN`，缺失时只打印一行日志就 `return`，
//! 而 `cargo bench` 仍退出 0。CI 的 benchmark job 既无服务也无 token，
//! 于是"跑过基准"与"全部静默跳过"不可区分。
//!
//! ## 测试策略
//!
//! 本文件刻意**不在测试里 spawn `cargo bench`**：那会嵌套等待 cargo 构建锁，
//! 实测单个这样的测试要跑十几分钟且会拖垮整个测试套件。
//! 因此：
//!   * 门禁脚本（Python，毫秒级）→ 直接子进程执行，真实覆盖
//!   * bench harness 契约 → 源码级断言（快、无副作用）
//!   * bench harness 端到端 → `#[ignore]`，按需显式运行（命令见各测试文档）

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// 门禁断言的两个基准名（来自 `scripts/check_pagination_benchmark.py`）。
const REQUIRED_BENCHMARKS: [&str; 2] = ["pagination_offset_deep_page", "pagination_keyset_deep_page"];

/// `BENCH_REQUIRE` 可点名的所有组（来自 benchmark 源码中的注册调用）。
const REQUIRABLE_GROUPS: [&str; 7] =
    ["versions", "user_directory", "rooms", "sync", "auth", "concurrent_throughput", "pagination"];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn gate_script() -> PathBuf {
    repo_root().join("scripts/check_pagination_benchmark.py")
}

fn api_bench_source() -> PathBuf {
    repo_root().join("benches/performance_api_benchmarks.rs")
}

fn api_bench_source_text() -> String {
    fs::read_to_string(api_bench_source()).expect("bench source must be readable")
}

/// 在临时目录写一个 `benchmark.txt` 并运行门禁，返回 `(exit_code, output)`。
fn run_gate_with(contents: &str) -> (i32, String) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = PathBuf::from(format!("/tmp/pagination_gate_{}_{}", std::process::id(), id));
    fs::create_dir_all(&dir).expect("temp dir must be creatable");
    let bench_file = dir.join("benchmark.txt");
    fs::write(&bench_file, contents).expect("benchmark.txt must be writable");

    let out = Command::new("python3")
        .arg(gate_script())
        .arg(&bench_file)
        .arg("--minimum-improvement")
        .arg("0.30")
        .current_dir(repo_root())
        .output()
        .expect("gate script must be spawnable");

    let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&out.stderr));
    let _ = fs::remove_dir_all(&dir);
    (out.status.code().unwrap_or(-1), combined)
}

// =============================================================================
// 基准必须真实存在（本轮修复的核心）
// =============================================================================

/// 门禁断言的每个基准都必须真的定义在 API 基准源码里。
///
/// 这是对 `8c7b4860` 那次"删基准但留门禁"事故的回归保护：
/// 缺少任意一个，CI 的阻塞步骤就必然失败且分页性能无任何测量。
#[test]
fn pagination_gate_benchmarks_exist_in_api_bench_source() {
    let source = api_bench_source_text();
    for name in REQUIRED_BENCHMARKS {
        assert!(
            source.contains(&format!("\"{name}\"")),
            "基准 `{name}` 未在 benches/performance_api_benchmarks.rs 中定义；\
             scripts/check_pagination_benchmark.py 断言它存在，\
             缺失会让 .github/workflows/benchmark.yml 的阻塞步骤必然失败"
        );
    }
}

/// 基准必须被注册进 `criterion_group!`，否则定义了也不会执行。
#[test]
fn pagination_benchmarks_are_registered_in_criterion_group() {
    let source = api_bench_source_text();
    let group_start = source.find("criterion_group!").expect("bench source must declare a criterion_group!");
    // 注意：不能在第一个 `);` 处截断 —— `config = Criterion::default()...`
    // 内部就有 `);`，会让切片只覆盖到 config 而看不到 targets 列表。
    // 这里取到 `criterion_group!` 之后的整个尾部（该宏是本文件最后一个顶层项）。
    let group = &source[group_start..];
    assert!(
        group.contains("benchmark_pagination_strategies"),
        "`benchmark_pagination_strategies` 必须在 criterion_group! 的 targets 列表中，\
         否则基准定义了也不会运行"
    );
}

/// 分页基准必须**不依赖服务或数据库** —— 这正是它能在 CI 中真实运行的原因。
#[test]
fn pagination_benchmarks_do_not_require_a_server() {
    let source = api_bench_source_text();
    let start = source.find("fn benchmark_pagination_strategies").expect("benchmark_pagination_strategies must exist");
    let rest = &source[start..];
    let end = rest[1..].find("\nfn ").map_or(rest.len(), |i| i + 1);
    let body = &rest[..end];

    assert!(
        !body.contains("server_required"),
        "分页基准不应依赖运行中的 homeserver —— CI 没有服务，依赖它会永远静默跳过"
    );
    assert!(!body.contains("bench_admin_token"), "分页基准不应依赖 BENCH_ADMIN_TOKEN —— CI 未提供该变量");
}

// =============================================================================
// 门禁本身的行为（用真实子进程执行，而非在 Rust 里重写一遍逻辑）
// =============================================================================

/// 缺少基准行时必须**明确失败**，而不是静默通过。
#[test]
fn pagination_gate_fails_when_rows_are_missing() {
    let (code, output) = run_gate_with("");
    assert_ne!(code, 0, "空 benchmark.txt 必须让门禁失败（这正是 benchmark.yml 修复前的必然结果）\n输出:\n{output}");
    assert!(output.contains("not found"), "失败信息应说明基准行缺失\n输出:\n{output}");
}

/// 提供真实的 Criterion 输出时必须通过 —— 否则门禁接入 CI 就是永远红。
#[test]
fn pagination_gate_passes_when_keyset_beats_offset() {
    // keyset 43.1ms 对比 offset 71.2ms → 改善约 39.5% > 30%
    let log = "\
test pagination_offset_deep_page ... bench:  71,200,000 ns/iter (+/- 1,200,000)
test pagination_keyset_deep_page ... bench:  43,100,000 ns/iter (+/- 900,000)
";
    let (code, output) = run_gate_with(log);
    assert_eq!(code, 0, "keyset 明显优于 offset 时门禁应通过\n输出:\n{output}");
}

/// 当优化收益不足时必须失败（证明门禁真的有判别力）。
#[test]
fn pagination_gate_fails_when_improvement_is_below_threshold() {
    // keyset 70.0ms 对比 offset 71.2ms → 仅 1.7% 改善 < 30%
    let log = "\
test pagination_offset_deep_page ... bench:  71,200,000 ns/iter (+/- 1,200,000)
test pagination_keyset_deep_page ... bench:  70,000,000 ns/iter (+/- 900,000)
";
    let (code, output) = run_gate_with(log);
    assert_ne!(code, 0, "改善不足 30% 时门禁必须失败\n输出:\n{output}");
}

/// 门禁脚本必须存在（避免"测试通过但脚本被删"）。
#[test]
fn pagination_gate_script_exists() {
    let path = gate_script();
    assert!(path.is_file(), "门禁脚本应存在于 {path:?}");
}

/// 同一个基准名出现两次时必须失败。
///
/// `benchmark.txt` 由多个 `cargo bench` 步骤 `tee -a` 追加而成。旧实现用
/// `dict[name] = value` 保留**最后**一行，于是"第一次采样 900ms、第二次 71ms"
/// 会被读成 71ms —— 门禁拿一个并非本次测量的值去比较，实测 exit 0。
#[test]
fn pagination_gate_rejects_duplicate_rows_instead_of_last_wins() {
    let log = "\
test pagination_offset_deep_page ... bench: 900,000,000 ns/iter (+/- 1,200,000)
test pagination_offset_deep_page ... bench:  71,200,000 ns/iter (+/- 1,200,000)
test pagination_keyset_deep_page ... bench:  43,100,000 ns/iter (+/- 900,000)
";
    let (code, output) = run_gate_with(log);
    assert_ne!(code, 0, "重复样本必须失败，而不是静默取最后一行\n输出:\n{output}");
    assert!(output.contains("more than once"), "失败信息应指出重复行\n输出:\n{output}");
}

/// 输入文件不存在时必须以清晰信息 fail-closed，而不是抛裸 `FileNotFoundError`。
#[test]
fn pagination_gate_fails_closed_when_input_file_is_missing() {
    let dir = unique_temp_dir("pagination_missing");
    let missing = dir.join("does-not-exist.txt");

    let out = Command::new("python3")
        .arg(gate_script())
        .arg(&missing)
        .arg("--minimum-improvement")
        .arg("0.30")
        .current_dir(repo_root())
        .output()
        .expect("gate script must be spawnable");
    let _ = fs::remove_dir_all(&dir);

    let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&out.stderr));
    assert_ne!(out.status.code().unwrap_or(-1), 0, "缺失输入必须非零退出\n输出:\n{combined}");
    assert!(combined.contains("nothing was measured"), "失败信息应说明没有测量数据\n输出:\n{combined}");
}

// =============================================================================
// "被请求的基准必须真的执行"（BENCH_REQUIRE）—— 消除静默跳过的假绿
// =============================================================================
//
// 注意一个容易踩的坑：**不能用"有没有任何基准跑过"来判定**。
// pagination 组是纯内存计算，总会执行，所以 `executed > 0` 恒为真，
// 那种判据永远抓不住服务端基准的静默跳过。
// （最初的 BENCH_STRICT 实现就掉进了这个陷阱：实测 strict 模式下
//   服务不可达仍然 EXIT=0，因为 pagination 跑了。已改为 BENCH_REQUIRE。）
//
// 正确判据是：**被 BENCH_REQUIRE 点名的组**必须真的执行。

/// bench harness 必须使用显式 `main`，以便在所有组尝试注册后做检查。
///
/// 只检查**非注释**代码：文档注释里提到 `criterion_main!`（说明为何弃用）
/// 不应让本测试失败 —— 那正是本文件要避免的"测试测错东西"。
#[test]
fn api_bench_uses_explicit_main_with_requirement_check() {
    let effective = strip_line_comments(&api_bench_source_text());
    assert!(
        !effective.contains("criterion_main!"),
        "不应继续使用 criterion_main! —— 它无法在退出前检查被请求的基准是否真的执行了"
    );
    assert!(effective.contains("fn main()"), "bench 必须定义显式 main");
    assert!(
        effective.contains("enforce_required_groups()"),
        "main 必须调用 enforce_required_groups() 才能把\"请求的基准被跳过\"变成失败"
    );
}

/// 必须支持 `BENCH_REQUIRE`，且缺失必需组时以非零状态退出。
#[test]
fn api_bench_require_guard_exits_non_zero() {
    let source = api_bench_source_text();
    assert!(source.contains("BENCH_REQUIRE"), "必须支持 BENCH_REQUIRE 环境开关");
    assert!(source.contains("std::process::exit(1)"), "必需组未执行时必须以非零状态退出");
}

/// 每个可被 `BENCH_REQUIRE` 点名的组都必须有对应的注册调用。
#[test]
fn api_bench_registers_every_requirable_group() {
    let source = api_bench_source_text();
    for group in REQUIRABLE_GROUPS {
        assert!(
            source.contains(&format!("require_bench_group(\"{group}\")")),
            "组 `{group}` 缺少 require_bench_group 注册，BENCH_REQUIRE={group} 会永远失败"
        );
    }
}

/// 注册必须发生在组的守卫 `return` **之后**，否则跳过时也会被记为"已执行"。
///
/// 做法：取该注册点所在函数体内**最后一个** `return;` 的位置，要求它严格
/// 早于注册点。若注册点落在守卫之前，这里就会失败。
///
/// （此前的实现按"函数体片段中是否含 return"判断，但守卫的闭合 `}` 与注册点
/// 紧邻，切片必然包含 `return;`，属于测不准的断言 —— 已改为位置比较。）
#[test]
fn api_bench_registers_after_the_guarded_returns() {
    let effective = strip_line_comments(&api_bench_source_text());

    for group in REQUIRABLE_GROUPS {
        let needle = format!("require_bench_group(\"{group}\")");
        let pos = effective.find(&needle).unwrap_or_else(|| panic!("missing {needle}"));

        // 函数起点：`\nfn `（带前导换行）保证匹配真正的函数定义，
        // 而不是文档注释里的 `fn foo()` 字样。
        let fn_start = effective[..pos].rfind("\nfn ").expect("registration must be inside a fn");
        let before = &effective[fn_start..pos];

        if let Some(last_return) = before.rfind("return;") {
            assert!(
                fn_start + last_return < pos,
                "组 `{group}` 的注册点位于守卫 return 之前；\
                 应把 require_bench_group 放在最后一个守卫 return 之后"
            );
            // 注册点与守卫之间必须已有语句分隔（闭合 `}` 或 `;`），
            // 避免 `return; require_bench_group(..)` 这种写在同一分支里的错误。
            let between = &before[last_return + "return;".len()..];
            assert!(
                between.contains('}'),
                "组 `{group}` 的 require_bench_group 必须放在守卫分支之外（`}}` 之后），\
                 否则被守卫时仍会被登记为已执行\n中间片段:\n{between}"
            );
        }
    }
}

/// CI 里绝不能在没有 `BENCH_REQUIRE` 的同一步骤中运行
/// **依赖服务**的基准 —— 那正是"静默跳过 + 退出 0"假绿的来源。
#[test]
fn ci_does_not_run_server_dependent_benchmarks_without_require_guard() {
    let workflows = repo_root().join(".github/workflows");
    let entries = fs::read_dir(&workflows).expect("workflows dir must be readable");
    let mut offenders: Vec<String> = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !(name.ends_with(".yml") || name.ends_with(".yaml")) {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        if !text.contains("BENCH_REQUIRE")
            && (text.contains("--bench performance_api_benchmarks")
                || text.contains("--bench performance_sliding_sync_benchmarks"))
        {
            offenders.push(format!("{name}: 运行了依赖服务的基准但未设置 BENCH_REQUIRE"));
        }
    }

    assert!(offenders.is_empty(), "以下 workflow 会把\"基准被静默跳过\"变成绿色：\n{}", offenders.join("\n"));
}

/// CI 的 API 基准步骤必须只请求 pagination 组（本 job 无服务、无 token）。
#[test]
fn ci_api_bench_step_requires_only_pagination() {
    let workflow = fs::read_to_string(repo_root().join(".github/workflows/benchmark.yml"))
        .expect("benchmark.yml must be readable");
    assert!(
        workflow.contains("BENCH_REQUIRE: \"pagination\""),
        "benchmark.yml 必须设置 BENCH_REQUIRE: \"pagination\"；\
         否则 requests 的组无法与执行情况对账"
    );
}

// =============================================================================
// 端到端（显式运行；不在默认门禁中）
// =============================================================================

/// 端到端：`BENCH_REQUIRE=pagination` 时 pagination 真实执行 → 退出 0。
///
/// 这是 CI 使用的组合，必须通过，否则 CI 误红。
///
/// 显式运行（约 3–5 分钟，含 bench 构建）：
/// ```console
/// cargo nextest run --profile test --features test-utils --test unit \
///   -E 'test(api_bench_require_pagination_passes)' --run-ignored ignored-only
/// ```
#[test]
#[ignore = "spawns `cargo bench`, which nests on the cargo build lock; run explicitly"]
fn api_bench_require_pagination_passes() {
    let out = run_api_bench(&[("BENCH_REQUIRE", "pagination")], &["--test", "pagination_"]);
    let combined = combined_output(&out);
    assert!(out.status.success(), "BENCH_REQUIRE=pagination 且 pagination 真实执行时必须成功\n输出:\n{combined}");
    assert!(combined.contains("all required group(s) executed"), "应确认必需组已执行\n输出:\n{combined}");
}

/// 端到端：`BENCH_REQUIRE=user_directory` 在无 token/无服务时 → 退出 1。
///
/// 这是对静默跳过假绿的直接回归保护：修复前该场景会跳过并退出 0。
///
/// 显式运行方式同 [`api_bench_require_pagination_passes`]。
#[test]
#[ignore = "spawns `cargo bench`, which nests on the cargo build lock; run explicitly"]
fn api_bench_require_unfulfillable_group_fails() {
    let out =
        run_api_bench(&[("BENCH_REQUIRE", "user_directory"), ("BENCH_ADMIN_TOKEN", "")], &["--test", "user_directory"]);
    let combined = combined_output(&out);
    assert!(
        !out.status.success(),
        "必需的 user_directory 组在缺少 token 时被跳过，必须以非零状态退出\
         （否则就是静默跳过的假绿）\n输出:\n{combined}"
    );
    assert!(combined.contains("did not execute"), "失败信息应说明哪个必需组没有执行\n输出:\n{combined}");
}

// =============================================================================
// 扫描面守卫（E6）：输入缺失或为空时必须响亮失败，而不是打印 PASS
// =============================================================================
//
// `scripts/quality/check_route_layering.sh` 此前可以在"扫到 0 个文件"的情况下成功
// 退出（对空目录打印 PASS）。本文件只被允许扩展现有的 `tests/unit/` 测试文件
// （不允许新建），因此这条门禁的"扫描面为空即失败"断言放在这里。
//
// （E7 的 `scripts/build_sqlx_migration_source.py` 守卫已随该脚本一并删除：
// forward-only sqlx source 在任何基线下都无法被 `sqlx migrate run` 应用 ——
// baseline 含 14 处 `CREATE INDEX CONCURRENTLY`，而 sqlx 会把每个文件放进事务；
// `migrations/` + `docker/db_migrate.sh` 是唯一迁移实现，增量折叠的完整性由
// `scripts/check_baseline_consolidation.py` 的扫描面自检独立把守。）

/// E6：扫描面存在但没有 `.rs` 文件时，旧实现打印 PASS 并 exit 0。
/// 这里用 `SYNAPSE_WEB_CRATE_DIR` 指向一个空的 `src/routes`，断言必须 exit 2。
#[test]
fn route_layering_gate_fails_closed_on_empty_scan_surface() {
    let dir = unique_temp_dir("route_layering_empty");
    let routes = dir.join("src/routes");
    fs::create_dir_all(&routes).expect("temp routes dir must be creatable");

    let (code, output) = run_bash_script(
        &repo_root().join("scripts/quality/check_route_layering.sh"),
        &[("SYNAPSE_WEB_CRATE_DIR", dir.to_str().expect("temp path must be UTF-8"))],
    );
    let _ = fs::remove_dir_all(&dir);

    assert_ne!(code, 0, "扫描面为空时门禁必须非零退出（旧实现对空目录打印 PASS/exit 0）\n输出:\n{output}");
    assert!(output.contains("inspect nothing"), "失败信息应说明门禁什么也没扫到\n输出:\n{output}");
}

/// E6 绿对照：真实路由树上门禁必须仍然通过，否则守卫会把 CI 判死。
#[test]
fn route_layering_gate_passes_on_the_real_route_tree() {
    // 空字符串会被 `${VAR:-default}` 当作"未设置"，从而落回 synapse-web。
    let (code, output) =
        run_bash_script(&repo_root().join("scripts/quality/check_route_layering.sh"), &[("SYNAPSE_WEB_CRATE_DIR", "")]);
    assert_eq!(code, 0, "真实路由树上门禁应通过\n输出:\n{output}");
}

/// Sliding-sync 门禁的 `SLIDING_SYNC_REQUIRE` 必须点名 bench **注册过的组名**。
///
/// 该 bench 有两套名字：criterion 基准 id `sliding_sync_p95_p99_latency`
/// （只用于 CLI 过滤）与 `require_bench_group("p95_p99")` 注册的**组名**。
/// 门禁脚本曾把 id 填进 `SLIDING_SYNC_REQUIRE`，于是 30 条 `[perf]` 采样全部
/// 打印、bench 却在收尾时判 "required benchmark group(s) did not execute" 并
/// exit 1（本地实测 2026-09-20；CI 里被更早的 schema/env 阻塞掩盖）。本测试把
/// 映射钉死：值必须能在 `require_bench_group(...)` 里找到；注册表为空时也失败，
/// 避免"两个空集合相等"式的空扫通过。
#[test]
fn sliding_gate_require_names_a_registered_group() {
    let bench_path = repo_root().join("benches/performance_sliding_sync_benchmarks.rs");
    let bench_src = fs::read_to_string(&bench_path).expect("sliding sync bench must be readable");
    let marker = "require_bench_group(\"";
    let registered: Vec<String> = bench_src
        .match_indices(marker)
        .map(|(index, pattern)| {
            let rest = &bench_src[index + pattern.len()..];
            rest[..rest.find('"').expect("closing quote must exist")].to_string()
        })
        .collect();
    assert!(registered.len() >= 4, "bench 必须注册 ≥4 个组（含纯内存组），否则下面的匹配是空扫: {registered:?}");
    assert!(registered.iter().any(|group| group == "p95_p99"), "bench 必须注册 DB-backed 组 `p95_p99`: {registered:?}");

    let gate_path = repo_root().join("scripts/ci/sliding_sync_perf_gate.sh");
    let gate = fs::read_to_string(&gate_path).expect("sliding sync gate script must be readable");
    let line = gate
        .lines()
        .find(|line| line.trim_start().starts_with("SLIDING_SYNC_REQUIRE="))
        .expect("门禁脚本必须设置 SLIDING_SYNC_REQUIRE");
    let value = line.split('"').nth(1).expect("SLIDING_SYNC_REQUIRE 必须是双引号字符串").to_string();
    let required: Vec<&str> = value.split([',', ' ']).filter(|item| !item.is_empty()).collect();
    assert!(!required.is_empty(), "SLIDING_SYNC_REQUIRE 至少要点名一个组");
    for group in required {
        assert!(
            registered.iter().any(|registered_group| registered_group == group),
            "SLIDING_SYNC_REQUIRE 点名了 `{group}`，但它不是注册过的组名 \
             (`require_bench_group` 注册表: {registered:?})。criterion 基准 id 是另一个字符串，\
             填错会让 bench 收尾时判 \"required benchmark group(s) did not execute\" 并 exit 1。"
        );
    }
}

// =============================================================================
// helpers
// =============================================================================

/// 去掉整行 `//` 注释，让"代码契约"断言不被文档说明误伤。
///
/// 例：模块文档里解释"为什么弃用 `criterion_main!`"，这属于文档，
/// 不应让"不得使用 criterion_main!"的断言失败。
fn strip_line_comments(source: &str) -> String {
    source.lines().filter(|line| !line.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n")
}

/// 以给定环境运行 `performance_api_benchmarks`，`args` 传给 criterion。
fn run_api_bench(env: &[(&str, &str)], args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new("cargo");
    cmd.args(["bench", "--locked", "--bench", "performance_api_benchmarks", "--"]);
    cmd.args(args);
    // 确保继承的环境不会干扰：显式设定一个必然不可达的 base url。
    cmd.env("BENCH_BASE_URL", "http://198.51.100.7:8008");
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.current_dir(repo_root());
    cmd.output().expect("bench must be spawnable")
}

fn combined_output(out: &std::process::Output) -> String {
    let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
    s.push_str(&String::from_utf8_lossy(&out.stderr));
    s
}

/// 唯一的临时目录（进程内计数 + pid），避免并行测试互相踩。
fn unique_temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = PathBuf::from(format!("/tmp/{tag}_{}_{}", std::process::id(), id));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir must be creatable");
    dir
}

/// 以 `envs` 覆盖环境跑一个 shell 门禁，返回 `(exit_code, stdout+stderr)`。
fn run_bash_script(script: &Path, envs: &[(&str, &str)]) -> (i32, String) {
    let mut cmd = Command::new("bash");
    cmd.arg(script);
    for (key, value) in envs {
        cmd.env(key, value);
    }
    cmd.current_dir(repo_root());
    let out = cmd.output().expect("script must be spawnable");
    (out.status.code().unwrap_or(-1), combined_output(&out))
}
