# 性能阈值真实性 + 真实可执行门禁（P4 §8.2 #3）

> **日期**: 2026-09-11
> **基线提交**: `b22bcf77`
> **对应待办**: `docs/audit/P4_performance_baseline_2026-09-11.md` §8.2 第 3 项

---

## 0. 决策

该项自 P4 阶段起挂着"**需决策**：重新标定 `TESTING.md` 的 P95 阈值，或删除以免误导"。
三轮未决后本轮自行裁定并执行，方案是**两者都做**：

1. **新增真实可执行门禁** —— `scripts/ci/compute_perf_gate.sh`，测量无需 server/DB 的
   Criterion 基准，阈值写在脚本内并附实测基线；
2. **删除无人执行的数字** —— 把 `TESTING.md` 中三组 P95 目标与逐端点目标替换为
   "指向真实门禁 + 明确标注哪些指标只有基线、没有门禁"。

只做 (1) 会留下误导性文档；只做 (2) 会让项目失去性能防复发能力。两者互补。

---

## 1. 核查：原阈值到底有多假

`TESTING.md` 原声明：

| 位置 | 数字 |
|---|---|
| §1.2 测试类型表 | 性能测试 P95 ≤500ms |
| §性能质量门禁 | 搜索 500ms / 同步 1000ms / DB 100ms |
| §4.2 性能指标定义 | P95 ≤500ms、P99 ≤1000ms、吞吐 ≥100 RPS、错误率 ≤1% |
| §4.3 性能测试场景 | whoami ≤20ms、状态查询 ≤50ms、成员列表 ≤100ms … |

三条核查结论：

1. **无任何执行者** —— 没有测试断言这些数字，没有 CI 步骤读取它们。
2. **与代码里的唯一阈值口径不同** —— 代码中只有
   `sliding_sync_perf_gate.sh` 的 **5000ms**（且当时从未接线）。
3. **实测比阈值好 15–30 倍** —— whoami 1.95ms vs 20ms、
   room 状态查询 3.15ms vs 50ms。即便运行也拦不住任何现实规模的退化。

### 更糟的一处：`tests/performance/query_performance_tests.rs`

该文件自称"验证 JOIN 查询高效"，实际**完全不连数据库** ——
每个"查询"都是 `tokio::task::yield_now()`：

```rust
let start = Instant::now();
tokio::task::yield_now().await;          // ← 这就是被测的"JOIN 查询"
let duration = start.elapsed();
assert!(duration.as_millis() < 100, "JOIN query took too long: {:?}", duration);
```

`duration` 测的是**一次任务让出**。这个断言在任何机器上都会通过，
无论真实 JOIN 慢成什么样 —— 一个永远绿、且不可能因名字所暗示的原因失败的检查。

其余文件同样不能作为门禁：

| 文件 | 实情 |
|---|---|
| `query_performance_tests.rs` | 纯模拟，零 DB 引用 |
| `api_load_tests.rs` | 零 DB 引用 |
| `manual_smoke_tests.rs` | 零 DB 引用；关键用例 `#[ignore]` |
| `appservice_scheduler_perf_tests.rs` | **确实连库**并打印 p50/p95/p99，但**全部 `#[ignore]`**，只报告不断言 |

---

## 2. 新增：`scripts/ci/compute_perf_gate.sh`

### 覆盖范围

只跑**纯计算**基准（不需要 server、不需要 DB），因此 CI 中**总能真正运行**：

| 基准 | 实测均值（2026-09-11 基线） | 阈值 |
|---|---|---|
| `state_resolution_chain_10` | 274.52 ns | 3000 ns |
| `state_resolution_chain_100` | 295.60 ns | 3000 ns |
| `auth_chain_build_10` | 5.36 µs | 60000 ns |
| `membership_transitions/*`（14 个） | 1.07 – 1.81 ns | 5000 ns（统一上限） |

### 阈值哲学（写在脚本头部）

阈值刻意**宽松（约 10× 基线）**：

* CI runner 硬件与录制基线的机器不可比，紧阈值必然是 flaky 的；
* **而 flaky 的门禁会被关掉 —— 这正是本项目走到今天这一步的原因**；
* 目标锁定**数量级退化**（丢失向量化、热路径上意外分配/加锁、数据结构选错），
  不是 10–20% 漂移。

收紧阈值是一个**需要显式动作**的决策：只在同规格 runner 上重录基线后才下调。

### 防"静默跳过"

* `COMPUTE_PERF_GATE_STRICT=1`（默认）：没测到基准即失败；
* `EXPECTED=4` 下限：测得数量少于下限也会失败，
  避免基准被静默跳过后门禁仍然变绿（这正是 P4 记录的 bench 假绿形态）。

### 实测结果

```console
$ bash scripts/ci/compute_perf_gate.sh
==> Compute Performance Regression Gate
    running performance_federation_benchmarks...
    running performance_membership_benchmarks...
    measurements:
      OK: state_resolution_chain_10: 278.63 ns (279 ns <= 3000 ns)
      OK: state_resolution_chain_100: 302.13 ns (302 ns <= 3000 ns)
      OK: auth_chain_build_10: 5.3896 µs (5390 ns <= 60000 ns)
      OK: membership_transitions/ban_to_join: 1.3319 ns (1 ns <= 5000 ns)
      ... (14 个 membership 用例)
==> Summary: measured=17 breaches=0 missing=0
==> Compute Performance Gate: PASSED
```

**门禁有效性验证**（把 `state_resolution_chain_10` 阈值改成 1ns）：

```console
      BREACH: state_resolution_chain_10: 289.41 ns (289 ns) > ceiling 1 ns
==> Summary: measured=26 breaches=1 missing=0
==> Compute Performance Gate: FAILED
EXIT=1
```

已还原。

> ⚠️ 上述 RED 运行同时暴露并修掉了一个真实缺陷：`measured=26`（而非 17）
> 说明测量结果文件在**跨运行累加** —— 写入用了 `>>` 而重置语句写在函数定义之后。
> 已改为先设定 `MEASUREMENTS_FILE` 并 `: >` 重置，再运行目标；复测 `measured=17`。
> 若不修，本地反复运行会让同一基准被重复计入，`measured` 下限形同虚设。

---

## 3. `TESTING.md` 的修正

| 位置 | 改为 |
|---|---|
| §1.2 测试类型表 | 标注 `tests/performance/` **多为模拟、非真实基线**，并加一段逐文件实情说明 |
| §性能质量门禁 | 指向 `compute_perf_gate.sh` 与 `sliding_sync_perf_gate.sh`；说明为何删除原 P95 数字 |
| §4.2 指标定义 | 表格改为区分「**有门禁**」与「⬜ **只有基线、无门禁**」 |
| §4.3 性能测试场景 | 替换为真实基准清单 + 各自的门禁归属 |
| §6.2 用户目录场景 | `响应≤500ms` → 功能性通过（延迟仅有基线，**无门禁**） |

核心改动是**不再让读者误以为 P95 有保护**。

---

## 4. 同时修正 `query_performance_tests.rs`

* 删除那条测 task yield 的 `assert!(duration.as_millis() < 100)`；
  留下注释说明**为何删除**（绿勾表达不了它名字暗示的保护）；
* 模块文档新增醒目说明：文件是模拟的、不连库、不能发现真实 N+1/索引/计划问题；
* 指向真正的守卫：`compute_perf_gate.sh`、`sliding_sync_perf_gate.sh`、
  `--all-features` 下的 DB 测试。

保留模拟结构作为 N+1-vs-batch **意图**的可执行描述，但不再冒充性能门禁。

---

## 5. CI 接线

`.github/workflows/benchmark.yml` 的 `benchmark` job 新增：

```yaml
      - name: Compute performance regression gate
        env:
          RUSTFLAGS: "-Ccodegen-units=16"
        run: bash scripts/ci/compute_perf_gate.sh

      - name: Upload compute perf logs
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: compute-perf-logs
          path: artifacts/compute_perf_*.log
```

---

## 6. 回归证据（8 个新测试）

`tests/unit/perf_gate_honesty_tests.rs`：

```console
    PASS compute_perf_gate_exists_and_is_executable
    PASS compute_perf_gate_is_wired_into_a_workflow
    PASS compute_perf_gate_fails_on_missing_measurements_by_default
    PASS testing_md_does_not_claim_unenforced_p95_gates
    PASS testing_md_points_at_the_real_gates
    PASS testing_md_flags_the_performance_directory_as_simulated
    PASS simulated_query_perf_tests_do_not_assert_latency
    PASS simulated_query_perf_tests_document_their_limits
    Summary [0.021s] 8 tests run: 8 passed
```

其中 `simulated_query_perf_tests_do_not_assert_latency` 会**剥掉注释再检查** ——
模块文档需要*提到*被删除的那条断言来解释它为何不在，因此不能按整文件字符串判断。
（第一版实现正是按整文件判断，被自己的文档注释误判为失败，已修正。）

---

## 7. 门禁

```console
$ ./scripts/check_fmt_ratchet.sh                 # OK: fmt debt at baseline (0)
$ python3 scripts/check_config_consistency.py    # OK
$ bash scripts/ci/check_sqlx_dynamic_ratio.sh    # OK
$ cargo clippy --workspace --all-targets --all-features --locked
CLIPPY_EXIT=0 errors=0 warnings=15               # 15 = 既有基线
$ cargo nextest run --profile test --features test-utils --lib --test unit
Summary 2504 tests run: 2504 passed (1 slow), 2 skipped
```

---

## 8. 复现方式

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust

# 真实门禁
bash scripts/ci/compute_perf_gate.sh                     # PASSED, measured=17
cat artifacts/compute_perf_measurements.txt              # 原始测量

# 验证它真的会红：把某阈值改到不可达后重跑
sed -i '' 's/^state_resolution_chain_10\t3000$/state_resolution_chain_10\t1/' scripts/ci/compute_perf_gate.sh
bash scripts/ci/compute_perf_gate.sh; echo "EXIT=$?"     # BREACH + 1
git checkout -- scripts/ci/compute_perf_gate.sh          # 还原

# 文档/测试守卫
cargo nextest run --profile test --features test-utils --test unit -E 'test(/perf_gate_honesty_tests/)'
grep -nE "P95≤|≤500ms" TESTING.md                        # 不应再出现无人执行的阈值
```

---

## 9. 剩余待办

| # | 项 | 优先级 |
|---|---|---|
| 1 | 同机同参数的 API 端点回归比对（P4 §8.2 #4）—— 需要稳定采集环境；本次只固化了纯计算侧 | 中 |
| 2 | 采集 `performance_sliding_sync_benchmarks`（P4 §8.2 #5） | 低 |
| 3 | 在真实 CI 上确认 `sliding-sync-perf-gate` 与 `compute_perf_gate` 首跑 | 中 |
| 4 | 给 `appservice_scheduler_perf_tests` 的 p95 报告加真实断言（目前 `#[ignore]` 只报告）—— 需先确定稳定阈值 | 中 |
| 5 | presence stream 游标 / 联邦 knock / `get_raw` 改名 / `RateLimitConfig` `deny_unknown_fields` | S 系列 P2/P3 |
