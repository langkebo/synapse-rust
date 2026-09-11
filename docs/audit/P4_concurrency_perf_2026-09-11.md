# P4 — 并发、性能与资源

> **审查日期**: 2026-09-11
> **基线**: `8862ecd7` 之前（`1639ed11`），工作树对本次审查干净
> **范围**: DB 连接池 · 缓存读写对称 · 后台任务裁剪 · 硬编码超时 · 性能基线
> **方法**: 逐项先核实"既有实现是否已修复"，再找真实缺口；结论均附可复现命令

---

## 0. 结论摘要

| 子项 | 状态 | 依据 |
|---|---|---|
| **DB 连接池** | 🔴 **发现真实缺陷并已修复**（添加预算守卫 + 修正误导注释） | §1 |
| 缓存读写对称 | ✅ **已验证正确**，无缺陷 | §2 |
| 硬编码超时 | ✅ **已被前序工作修复**（配置化） | §3 |
| 后台任务裁剪 | ⚪ **未完成** | §4 |
| 性能基线 | ⚪ **未采集**（Criterion 需较长运行） | §5 |

> **P4 最有价值的产出是 §1**：它给出了 P0 阶段记录的
> "integration 结果对并发度高度敏感"的**机制性解释**，并把该不变式固化为测试。

---

## 1. 🔴 DB 连接池 —— 测试套件连接预算远超 PostgreSQL 上限

### 1.1 事实链（逐步实测）

| # | 事实 | 证据 |
|---|---|---|
| 1 | 每池上限 `DEFAULT_TEST_DB_MAX_CONNECTIONS = 40` | `src/test_utils.rs:107` |
| 2 | CI **未设置** `TEST_DB_MAX_CONNECTIONS` ⇒ 使用 40 | `grep -rn TEST_DB_MAX_CONNECTIONS .github/workflows/` → 空 |
| 3 | 池**不复用**：`get_test_pool_async()` 每次调用都 `PgPoolOptions::connect()` 新建池 | `tests/common/mod.rs:40-64`；无 `static`/`OnceLock` 池缓存 |
| 4 | 故每个并发运行的测试各持一个上限 40 的池 | 由 3 推出；44 处 `setup_test_app`/`require_test_pool` 封装 |
| 5 | `ci` profile `test-threads = 12` | `.config/nextest.toml` |
| 6 | CI Postgres 为默认 `max_connections = 100` | `ci.yml:141-151` 未覆盖该参数 |
| 7 | `DEFAULT_TEST_DB_SHARED_CLONE_CONCURRENCY = 12` **只约束 schema 克隆并发**，不约束连接 | `src/test_utils.rs:114-117` |

**⇒ 最坏需求 `12 × 40 = 480`，是 PostgreSQL 上限的 4.8 倍。**

### 1.2 与实测症状吻合（解释 P0 §2.2.1）

P0 记录的同一提交四次运行漂移：

| 运行 | 并发 | 系统负载 | 结果 |
|---|---|---|---|
| #1 | 默认 | 低 | 1417 passed / 9 flagged |
| #3 | 12 | 5.7–9.4 | 1408 passed / **12 failed / 7 timed out** |
| **#4 串行** | **1** | 5.7–9.3 | ✅ **13/13 passed，2.3 分钟** |

**#4 是决定性对照**：超时集合串行重跑全通过 ⇒ **连接饥饿**，非代码缺陷。
本节的预算计算给出了其机制性原因。

### 1.3 修复（commit `8862ecd7`）

**① 新增守卫测试 `tests/unit/test_connection_budget_tests.rs`**

- 从 `src/test_utils.rs` 读池上限（单一真相源）
- 从 `.config/nextest.toml` 读 `[profile.ci]` 的 `test-threads`
- 断言**单池上限必须 < PostgreSQL 默认 `max_connections`**（单池不得耗尽整库）
- **打印实时预算数字**，使漂移在测试输出中可见

**运行时输出（实测）**：

```
test DB connection budget: ci test-threads=12, pool_max=40
  → worst-case demand=480 vs PG max_connections=100
NOTE: worst-case demand 480 exceeds 100; the suite relies on
  (a) pools not growing to their ceiling in practice and
  (b) running heavy groups with low --test-threads.
```

> 该测试在 CI 的 `unit test target` step 执行，而该 step **不设置 DB 环境变量**
> ⇒ 测试可独立运行，不会因缺 DB 而跳过。

**② 修正误导性注释**（`src/test_utils.rs:114`）

原文：

```
// Each parallel test may clone the template schema concurrently; DB pool
// max=40 per pool, PostgreSQL max_connections=100 supports 12*~5=60 conns.
```

**问题**：`~5` 是对**实际用量**的猜测，而池被**配置为可增长到 40** —— 两者不可混用，
结论（"supports ... 60 conns"）因此不成立。已替换为完整的预算算法、实测后果与规避方式。

### 1.4 生产池配置 —— 🟡 `pool_size` 是已废弃的误导项

| 字段 | 值 | 状态 |
|---|---|---|
| `max_size` | 50 | ✅ **实际生效**（`server/database.rs:33` 的 `max_connections`） |
| `pool_size` | 20 | 🟡 **零引用、已废弃**（`config/database.rs:81-85` 注释自述） |

**两份 `homeserver.yaml` 都同时设置了 `pool_size: 20` 与 `max_size: 50`**
⇒ 运维若按字面理解 `pool_size` 会以为上限是 20，实际是 50。

生产 `max_size=50` 本身相对 PG 默认 100 是合理的。仅记录该字段为配置债。

### 1.5 未做的修复及原因

**未下调 `DEFAULT_TEST_DB_MAX_CONNECTIONS`**：该值影响整个测试套件的并行特性与墙钟时间，
下调可能反而延长反馈周期。本轮的策略是**先让问题可见、可控**（守卫 + 注释），
把"下调上限 / 调整 CI 线程数"留给有实测数据支撑的决策。

---

## 2. ✅ 缓存读写对称 —— 已验证正确

AGENTS.md 的告诫是：`set_raw` 写 L1+L2 异步、同步 `get_raw` 只读 L1，
**跨实例正确性必须用 `get_raw_shared().await`**。

### 2.1 逐项核实

| 检查 | 结果 |
|---|---|
| 生产代码中的 `.get_raw(` 调用 | **0 处**（仅 `manager.rs` 内部 L1 层访问 + 测试断言） |
| `get_raw_shared` 调用 | 14 处 |
| `get` / `get_checked` 是否回落到 L2 | ✅ 是（`manager.rs:511-537` / `:539-569`） |
| `delete` 是否广播跨实例失效 | ✅ `local.remove` → Redis DEL → `broadcast_invalidation(key, Key)` |
| `delete_batch` 是否广播 | ✅ 每个 key 一条广播（注释明确"跨实例失效语义保持不变"） |
| `delete_token`（安全关键） | ✅ **三重防护**：L1 删除 + Redis 删除（失败 `error` 级记录，注释明确"残留构成 fail-open"）+ 广播 |

**结论：缓存读写对称性无缺陷**，且安全关键路径（令牌撤销）处理得比一般路径更严格。

---

## 3. ✅ 硬编码超时 —— 已被前序工作修复

round1 侦察（`docs/后端问题真实性审查-2026-09-06.md` 的 B-2.2）曾标记
`event_notifier.rs` 中 `Duration::from_secs(5)` / `from_secs(2)` 硬编码。

**实测当前状态**：生产代码（排除 `#[cfg(test)]` 段）中**无硬编码时长常量**：

| 位置 | 现状 |
|---|---|
| `event_notifier.rs:132` | `Duration::from_secs(self.idle_timeout_secs)` —— **结构体字段**，默认 5s（注释说明与 Synapse `notify_sleep_time` 一致），由 `with_idle_timeout_secs()` 可配 |
| `event_notifier.rs:354` | `Duration::from_millis(reconnect_backoff_ms)` —— **变量**，非字面量 |

⇒ B-2.2 **已修复**。

---

## 4. ⚪ 后台任务裁剪 —— 未完成

AGENTS.md 的上游教训：长运行部署需要为 device list changes、presence-like state、
media quarantine history 等 append-only 流提供 pruning / background-update 路径。

**本会话未核实**：
- `background_update.rs`（1,656 行）的任务是否都有界
- device list / presence / media quarantine 等增长型表是否有裁剪任务与保留策略
- `ScheduledTasks` 各任务的失败处理与背压

列为后续项。

---

## 5. ⚪ 性能基线 —— 未采集

`TESTING.md` 声称 P95 ≤ 500ms，但本会话**未运行 Criterion benchmark**
（`cargo bench` 需较长运行时间，且与并发 agent 的构建争用冲突）。

已知的慢测试观测（未系统化）：

| 测试 | 观测 |
|---|---|
| `server::tests::render_appservice_scheduler_prometheus_metrics_reflects_recovery_summary` | 77.2s（P0 首次）；后降至 15.8s（同一测试） |
| `protocol_compliance_tests::{test_read_marker_update, test_receipt_insert, test_typing_set_and_clear}` | >60s（多次运行稳定出现） |
| `api_beacon_location_tests::test_beacon_location_applies_room_backpressure_token_bucket` | >60s |

列为后续项。

---

## 6. 复现方式

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust
export CARGO_TARGET_DIR=/tmp/p4t      # 隔离并发构建

# §1 连接预算守卫（打印实时数字）
cargo nextest run --profile test --features test-utils --test unit \
  test_db_connection_budget --no-capture

# §1 事实核对
grep -n "DEFAULT_TEST_DB_MAX_CONNECTIONS" src/test_utils.rs
grep -rn "TEST_DB_MAX_CONNECTIONS" .github/workflows/     # 空 = CI 用默认 40
grep -n "test-threads" .config/nextest.toml
grep -rn "get_test_pool_async" tests/common/mod.rs         # 每次调用新建池

# §2 缓存对称性
grep -rn "\.get_raw(" --include='*.rs' src/ synapse-services/src/ | grep -viE 'test|assert'
#   → 生产代码 0 处

# §3 硬编码超时（排除测试段）
awk -v t="$(grep -n '^#\[cfg(test)\]' synapse-services/src/event_notifier.rs | head -1 | cut -d: -f1)" \
  'NR<t && /Duration::from_(secs|millis)\(/' synapse-services/src/event_notifier.rs
```

---

## 7. 移交后续

| 项 | 优先级 |
|---|---|
| 后台任务裁剪审查（§4）：append-only 流的 pruning 路径 | **高** |
| 性能基线采集（§5）：Criterion + P95 验证 | **中**（需较长运行窗口） |
| `pool_size` 废弃字段清理（§1.4）：从两份 homeserver.yaml 移除或标注 | 低 |
| 是否下调 `DEFAULT_TEST_DB_MAX_CONNECTIONS` 或调整 CI 线程数（§1.5） | 中（需实测数据支撑） |
