# PR 性能门禁完整性核查（第四个失效门禁）

> **日期**: 2026-09-11
> **基线提交**: `2319f3a6`
> **范围**: `.github/workflows/ci.yml` 的 `pr-benchmark-gate` job + `scripts/ci/benchmark_pr_gate.sh`

---

## 0. 结论

`ci.yml` 的 `pr-benchmark-gate` job 名为"PR Benchmark Gate"，注释写着
"Runs on every PR to block performance regressions"。核查发现它在**三个互相独立**
的层面都无法检测回归 —— 任一单独成立就足以让它形同虚设：

| # | 缺陷 | 后果 |
|---|---|---|
| 1 | 在**顶层 `while` 循环**里用了 `local` | 一旦基线存在，脚本立刻以 `local: can only be used in a function` 中止（exit 1），**从不输出判定** |
| 2 | 基线格式与解析器**不匹配** | 解析出 0 个基准，比较为空 |
| 3 | 缺基线时打印 `PASSED (baseline comparison skipped)`，且 workflow 对下载设了 `continue-on-error: true` | "基线没下到"与"没有回归"**不可区分** → 永久假绿 |

另有一处使数字不可比：门禁用**默认 profile** 跑基准，而基线是用
`--profile release-perf` 录制的。

这是本项目发现的**第四个**"文书存在但不生效"的门禁（前三个：sliding sync 性能门禁、
SQLx 比例门禁、分页门禁），也是最彻底的一个 —— 前三者各自只坏一处。

---

## 1. 缺陷取证

### 1.1 `local` 用于函数外

`benchmark_pr_gate.sh`（修复前）在基线比较循环里：

```bash
    while IFS= read -r current_line; do
        local bench_name bench_value bench_unit      # ← 顶层循环，不是函数
```

以及 JSON 报告循环里同样一处。实测 bash 行为：

```console
$ bash -n t.sh && echo "syntax: OK"
syntax: OK
$ bash t.sh
before
t.sh: line 4: local: can only be used in a function
EXIT=1
```

即 `bash -n` 查不出来，但**运行时立即中止**。
由于该分支位于 `if [ -f "$BENCH_BASELINE_PATH" ]` 之内，
**有基线时脚本必定崩在这里**。

### 1.2 格式不匹配

* `benchmark.yml` 用 `--output-format bencher` 写 `benchmark.txt`
  （第 93–94 行），产物形如：
  `test pagination_offset_deep_page ... bench: 71,200,000 ns/iter (+/- 1,200,000)`
* `benchmark_pr_gate.sh` 的解析器只匹配 Criterion **文本**格式
  （`grep -q 'time: \['`）
* `ci.yml` 却把 `baseline/benchmark.txt` 交给它

两种格式**没有交集**，因此解析结果恒为空：

```console
$ printf 'test pagination_offset_deep_page ... bench:  71,200,000 ns/iter\n' > /tmp/l
$ grep -q 'time: \[' /tmp/l && echo matches || echo "NO MATCH -> 解析出 0 个基准"
NO MATCH -> 解析出 0 个基准
```

### 1.3 缺基线 = 假绿

```bash
else
    echo "No baseline found. Recording results for future comparison."
    echo "PR Benchmark Gate: PASSED (baseline comparison skipped)"   # ← exit 0
fi
```

配合 workflow：

```yaml
      - name: Download baseline benchmark results
        uses: dawidd6/action-download-artifact@v3
        continue-on-error: true        # ← 下载失败被吞掉
```

于是基线下载失败 → 文件不存在 → 脚本打印 PASSED → job 全绿。
**门禁永远不会因为性能退化而变红。**

### 1.4 profile 不可比

* 基线：`cargo bench --locked --profile release-perf ...`（`benchmark.yml:94,97`）
* 门禁：`cargo bench --locked ...`（默认 profile，`benchmark_pr_gate.sh:26,29`）

即便前三项都修好，两个 profile 的数字也不可直接比较。

---

## 2. 修复

### 2.1 `scripts/ci/benchmark_pr_gate.sh`（重写）

| 修复 | 做法 |
|---|---|
| 去掉 `local` 误用 | 循环内改用普通变量；新增 `pr_gate_has_no_local_outside_a_function` 守卫 |
| 基线格式 | 只接受 Criterion 文本格式；解析出 0 个基准时**明确失败**并指出 `benchmark.txt` 是 bencher 格式、应改用 `benchmark_standard.txt` |
| 缺基线 | `BENCH_BASELINE_PATH` 未设或文件不存在 → **exit 1**，并注明"历史缺陷：此处曾打印 PASSED" |
| 无可比较项 | `COMPARED == 0` → **exit 1**（不得在没有比较的情况下声称通过） |
| profile | 默认 `release-perf`，可用 `BENCH_PROFILE` 覆盖 |
| 未知单位 | `normalize_to_ns` 失败即 **exit 1**，拒绝用不可比数字出结论 |
| 可测试性 | 新增 `BENCH_PR_GATE_SKIP_BENCH` / `BENCH_PR_GATE_CURRENT_PATH` / `BENCH_PR_GATE_PARSED_PATH`，便于用合成数据验证 |
| JSON 报告 | 增补 `profile` / `compared` / `regressions` 字段 |

解析器同时支持两种行布局（长基准名单独一行、或名与时间同行）。

### 2.2 `.github/workflows/ci.yml`

* **移除** `continue-on-error: true`，并就地写明为何不得恢复；
* 新增 `Locate baseline (Criterion text format)` 步骤：优先取
  `baseline/benchmark_standard.txt`（Criterion 文本格式，且恰好覆盖
  federation + membership，与本门禁运行的基准一致）；找不到就
  `::error::` 并给出行动指引（先在 main 上跑一次 `benchmark.yml`）；
* 传入 `BENCH_PROFILE: release-perf`。

> `benchmark.yml` 的 `Store benchmark results` 步骤本就同时上传
> `benchmark.txt` 与 `benchmark_standard.txt`，无需改动。

---

## 3. 回归证据（7 个测试）

`tests/unit/pr_benchmark_gate_tests.rs`：

```console
    PASS pr_gate_has_no_local_outside_a_function
    PASS pr_gate_parses
    PASS pr_gate_detects_a_regression
    PASS pr_gate_passes_when_within_threshold
    PASS pr_gate_does_not_crash_with_a_baseline_present
    PASS pr_gate_fails_loudly_when_baseline_is_missing
    PASS pr_gate_rejects_a_baseline_in_the_wrong_format
    PASS pr_gate_uses_the_same_profile_as_the_baseline
    PASS pr_gate_baseline_points_at_the_text_format_artifact
    PASS pr_gate_workflow_does_not_mask_a_missing_baseline
```

关键一条：**`pr_gate_detects_a_regression`** —— 构造 +46% 的退化（400 ns vs 274 ns 基线，
阈值 15%），要求 exit 1。修复前该场景要么崩溃、要么解析为空而通过。

### 手工验证四个场景

```console
# A) 真实退化 → FAILED, EXIT=1
OK: auth_chain_build_10 changed by 0.75% (within threshold)
REGRESSION: state_resolution_chain_10 changed by 45.99% (threshold: 15%)
PR Benchmark Gate: FAILED (1 regression(s) detected)

# B) 缺基线 → EXIT=1
ERROR: 基线文件不存在: /tmp/nope.txt
       （历史缺陷：此处曾打印 PASSED，使缺失基线伪装成通过）

# C) bencher 格式基线 → EXIT=1 + 明确格式指引
ERROR: 基线格式无法解析（解析出 0 个基准）: /tmp/bl_bencher.txt
       注意 benchmark.yml 的 benchmark.txt 使用 --output-format bencher ...

# D) 无回归 → PASSED, EXIT=0
Compared 2 benchmark(s); 0 had no baseline entry.
PR Benchmark Gate: PASSED (no regressions beyond 15%)
```

### 写测试时踩到的两个坑（已修）

1. **并行竞态**：多个测试共用 `artifacts/pr_benchmark_current.txt`，
   一个测试删除它时另一个正在读 → `sort: No such file or directory`。
   改为 `BENCH_PR_GATE_CURRENT_PATH` 指向各自临时目录。
2. **断言被自己的注释绊倒**：workflow 里的说明注释**提到**了
   `continue-on-error: true`（记录为何不得使用），朴素子串检查因此误报。
   改为先剥离注释再断言 —— 与本项目 sqlx / perf 守卫同样的处理。

---

## 4. 门禁

```console
$ ./scripts/check_fmt_ratchet.sh                 # OK: fmt debt at baseline (0)
$ python3 scripts/check_config_consistency.py    # OK
$ bash scripts/ci/check_sqlx_dynamic_ratio.sh    # OK
$ cargo clippy --workspace --all-targets --all-features --locked
CLIPPY_EXIT=0 errors=0 warnings=15               # 15 = 既有基线
$ cargo nextest run --profile test --features test-utils --lib --test unit
Summary 2511 tests run: 2511 passed (1 slow), 2 skipped
```

---

## 5. 四个门禁的横向对照（P5 "CI blocking 有效性"）

| 门禁 | 修复前状态 | 提交 |
|---|---|---|
| sliding sync 性能 | 逻辑完整但**从未接线**；预检依赖 `pg_isready` | `da59a971` |
| SQLx 动态/静态比例 | 未接线 + 只扫 `src/` + 阈值不可达 + 死引用 | `da59a971` |
| 分页性能 | 断言的基准已被删除 → **必然失败 98 天** | `da59a971` |
| **PR 性能门禁** | **三处独立缺陷 → 永久假绿** | 本次 |
| doc-test | 根 crate 0 个测试 → 空门禁 | `4fd572af`（前序） |

共同模式：**门禁存在 ≠ 门禁生效**。四者都缺少"门禁本身是否有效"的测试 ——
本次为每个门禁都补了会真实失败的守卫测试。

---

## 6. 复现方式

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust

# 守卫测试
cargo nextest run --profile test --features test-utils --test unit \
  -E 'test(/pr_benchmark_gate_tests/)'

# 手工四场景（跳过真实 bench，用合成数据）
printf 'state_resolution_chain_10\t400.00\tns\nauth_chain_build_10\t5.4000\tµs\n' > /tmp/cur.txt
cat > /tmp/bl.txt <<'EOF'
state_resolution_chain_10
                        time:   [270.00 ns 274.00 ns 278.00 ns]
auth_chain_build_10     time:   [5.3000 µs 5.3600 µs 5.4000 µs]
EOF
BENCH_PR_GATE_SKIP_BENCH=1 BENCH_PR_GATE_CURRENT_PATH=/tmp/cur.txt \
  BENCH_BASELINE_PATH=/tmp/bl.txt bash scripts/ci/benchmark_pr_gate.sh; echo "EXIT=$?"   # 1

BENCH_PR_GATE_SKIP_BENCH=1 BENCH_PR_GATE_CURRENT_PATH=/tmp/cur.txt \
  BENCH_BASELINE_PATH=/tmp/absent.txt bash scripts/ci/benchmark_pr_gate.sh; echo "EXIT=$?" # 1

# 结构检查
bash -n scripts/ci/benchmark_pr_gate.sh
awk '/^  pr-benchmark-gate:/,/^  integration-test:/' .github/workflows/ci.yml \
  | grep -v '^\s*#' | grep continue-on-error || echo "no continue-on-error (good)"
```

---

## 7. 仍待办

| # | 项 | 优先级 |
|---|---|---|
| 1 | 真实 CI 首跑确认四个门禁（`sliding-sync-perf-gate`、`compute_perf_gate`、`pr-benchmark-gate`、SQLx 棘轮） | **高**（本地无法复现 runner 环境） |
| 2 | API 端点回归比对（P4 §8.2 #4）—— 需稳定采集环境 | 中 |
| 3 | 采集 `performance_sliding_sync_benchmarks`（P4 §8.2 #5） | 低 |
| 4 | `appservice_scheduler_perf_tests` 的 p95 报告加真实断言（当前 `#[ignore]` 只报告） | 中 |
| 5 | presence stream 游标 / 联邦 knock / `get_raw` 改名 / `RateLimitConfig` `deny_unknown_fields` | S 系列 P2/P3 |
