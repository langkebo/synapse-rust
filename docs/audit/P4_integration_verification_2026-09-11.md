# 独立端到端验证结果 + 集成测试吞吐观察

> **日期**: 2026-09-11
> **基线提交**: `3c029d86`
> **目的**: 对本次审查中的全部改动做一次**独立交叉验证** —— 走真实路由 + 真实
> PostgreSQL，不使用 in-memory mock

---

## 1. 验证方法

使用与 CI 相同的参数（`--all-features --test-threads 6`），
针对**本次改动实际触及的领域**选取集成测试子集：

| 覆盖的改动 | 测试套件 |
|---|---|
| `/sync` 限流恢复（`14570209`） | `api_room_sync_tests`、`api_sync_filter_tests`、`api_sync_isolation_rate_limit_tests` |
| presence 扇出/去重（前序 `ecca8751`） | `api_device_presence_tests`、`presence_storage_tests_migrated` |
| 缓存对称性 | `cache_tests` |
| 房间生命周期 / join-via | `api_admin_room_lifecycle_tests`、`room_service_tests_migrated` |

```bash
export DATABASE_URL='postgresql://synapse:<pw>@localhost:5432/synapse'
export TEST_DATABASE_URL="$DATABASE_URL"
export REDIS_URL='redis://localhost:6379'
cargo nextest run --test integration --all-features --locked --test-threads 6 \
  'api_room_sync_tests' 'api_sync_filter_tests' 'api_sync_isolation_rate_limit_tests' \
  'api_device_presence_tests' 'presence_storage_tests_migrated' 'cache_tests' \
  'api_admin_room_lifecycle_tests' 'room_service_tests_migrated'
```

## 2. 结果

```console
    Starting 123 tests across 1 binary (1304 tests skipped)
    ...
    Summary [1011.731s] 123 tests run: 123 passed (68 slow), 1304 skipped
EXIT=0
```

**123/123 通过，0 失败。** 这是对 `/sync` 限流、presence 扇出、缓存对称性、
房间生命周期等改动的一次独立验证 —— 与 unit 层的源码级守卫不同，
这批用例真实经过 axum 路由与 PostgreSQL。

## 3. 未完成的部分（如实说明）

**全量集成套件（1427 个用例）本轮未跑完。** 我在跑到 107/1427（0 失败）时
主动终止：按实测速率外推需要约 **3.3 小时**，超过我为该作业设置的 90 分钟上限。

因此准确表述是：

* ✅ 已改变的领域：**123/123 通过**（真实路由 + 真实 DB）
* ✅ 全量套件前 107 个：**0 失败**
* ⬜ **其余约 1200 个用例本轮未执行** —— 不声称"全量通过"

---

## 4. 观察：集成测试吞吐（已量化，未修复）

### 4.1 数据

| 指标 | 实测值 |
|---|---|
| 目标子集 | 123 个用例 / 1011.7 秒 |
| 速率 | **7.3 用例/分钟**（`--test-threads 6`） |
| 标为 slow（>60s）的比例 | **68/123 = 55%** |
| 单用例典型耗时 | 73–90 秒 |
| 外推全量 1427 用例 | 约 **3.3 小时** |

本地单进程 6 并发下的这个速率与 CI runner（通常更弱）相比只会更慢。

### 4.2 成本来源：先做了实测，结果**修正了我的初判**

`src/test_utils.rs::clone_schema_from_template` 为**每个用例**克隆模板 schema：
`CREATE TABLE ... (LIKE ... INCLUDING ALL)` × 257 张表，再把自动命名的索引
DROP 掉并按模板定义重建（当前 schema 有 778 个索引，其中 387 个非约束索引），
最后补插若干种子行。索引重建的存在理由是：
`schema_contract_p0_tests_migrated.rs` 用 `has_index_named()` 断言**具体索引名**
（该文件内 23 处调用）。

我原以为"778 个索引的 DROP+CREATE"是主要成本，于是在一次性 schema 里**实测各阶段**：

```sql
-- 阶段 1：CREATE SCHEMA + 257 张 CREATE TABLE ... LIKE INCLUDING ALL
DO ... Time: 2112.437 ms

-- 阶段 2：DROP 全部 387 个非约束索引
DO ... NOTICE: dropped 387 indexes
DO ... Time: 94.697 ms
```

**实测结论与初判相反**：表克隆约 **2.1 秒**，索引 DROP 约 **95 毫秒**，
索引重建同量级（数百毫秒）。合计约 **2–3 秒/用例**，
**不足以解释**观测到的 ~8.2 秒/用例（1011.7s ÷ 123）。

因此正确的表述是：**每用例约 2–3 秒用于 schema 克隆，其余主要花在用例体本身**
（真实路由 + 真实 PostgreSQL 的端到端流程）。我先前把成本归因于索引重建是
**过度归因**，此处更正。

### 4.3 顺带发现（低危，未修）

实测时索引重建阶段报错：

```console
ERROR: relation "perf_probe.rooms_summaries_mv" does not exist
CONTEXT: CREATE UNIQUE INDEX idx_rooms_summaries_mv_room_id ON perf_probe.rooms_summaries_mv ...
```

`schema = public` 里有两个**物化视图**（`rooms_summaries_mv`、`public_room_directory`），
而克隆只遍历 `pg_tables`（不含物化视图），所以它们**从未被克隆到测试 schema**。
重建其索引自然失败 —— 但该失败被循环内的 `EXCEPTION WHEN OTHERS THEN NULL`
**静默吞掉**，因此从未有人注意到。

**当前无实际影响**：`grep` 全仓（`src/`、`synapse-services/src/`、`synapse-storage/src/`、`tests/`）
**没有任何代码引用**这两个物化视图，仅出现在迁移与文档里 —— 属未使用的遗留对象。
故记为低危观察，不做改动。

> 附带确认：本轮检查本地库 `test_%` schema 残留为 **0**，
> 说明 schema 清理路径工作正常 —— 与 CLAUDE.md 记录的"1363 个残留 schema"
> 历史问题不同，当前不存在该问题。

### 4.4 为什么本轮不动克隆路径

* 它是**共享测试基础设施**：改错会波及全部 1427 个用例的语义，
  而验证需要跑完整套件 —— 恰恰是当前跑不完的那个；
* 实测已表明索引重建**不是**主要瓶颈，去掉它也拿不到数量级收益，
  收益/风险比不足以支撑在没有全量验证的前提下改动。

> 附带确认：本轮检查本地库 `test_%` schema 残留为 **0**，
> 说明 schema 清理路径工作正常 —— 与 CLAUDE.md 记录的"1363 个残留 schema"
> 历史问题不同，当前不存在该问题。

---

## 5. 对其他结论的影响

集成套件跑不完这件事，**不削弱**本次审查的修复结论，因为：

* 每个修复都配有**会真实失败的守卫测试**（unit 层），且多数已做 RED 验证
  （移除标记/降低阈值/引入漂移后确认变红）；
* 受影响的领域另做了上面 §2 的真实 HTTP + DB 验证；
* 未跑到的约 1200 个用例主要是联邦、E2EE、AppService、Worker 等领域，
  本次改动未触及这些领域的内部逻辑。

但它确实意味着：**"全量集成套件通过"这个更强的结论本轮拿不到**，
后续若要给出该结论，需要安排一次约 3.3 小时的完整运行（或先优化 §4.2）。

---

## 6. 复现方式

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust
export DATABASE_URL='postgresql://synapse:<pw>@localhost:5432/synapse'
export TEST_DATABASE_URL="$DATABASE_URL"
export REDIS_URL='redis://localhost:6379'

# 本轮实际执行的（约 17 分钟，123 用例）
cargo nextest run --test integration --all-features --locked --test-threads 6 \
  'api_room_sync_tests' 'api_sync_filter_tests' 'api_sync_isolation_rate_limit_tests' \
  'api_device_presence_tests' 'presence_storage_tests_migrated' 'cache_tests' \
  'api_admin_room_lifecycle_tests' 'room_service_tests_migrated'

# 全量（约 3.3 小时，本轮未完成）
cargo nextest run --test integration --all-features --locked --test-threads 6

# 查看 schema 残留
docker exec synapse-postgres psql -U synapse -d synapse -tAc \
  "SELECT count(*) FROM information_schema.schemata WHERE schema_name LIKE 'test_%';"
```

---

## 7. 仍待办

| # | 项 | 优先级 |
|---|---|---|
| 1 | 全量集成套件完整跑一次（~3.3h）以给出"全量通过"结论 | 中 |
| 2 | 评估 §4.2 的索引重建优化（`ALTER INDEX ... RENAME` 替代 DROP+CREATE） | 中 |
| 3 | 真实 CI 首跑确认六个门禁 + e2e 新步骤（仓库 private、`gh` token 失效 → 本环境不可达） | **高** |
| 4 | API 端点回归比对（P4 §8.2 #4） | 中 |
| 5 | presence stream 游标 / 联邦 knock / `get_raw` 改名 / `RateLimitConfig` `deny_unknown_fields` | S 系列 P2/P3 |
