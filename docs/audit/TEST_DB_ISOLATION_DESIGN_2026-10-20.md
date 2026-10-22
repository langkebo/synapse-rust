# 测试库隔离：从 schema clone 换到 database clone（设计稿）

- 日期：2026-10-20
- 状态：**P0–P4 全部接线**（`1e77cbae7`、`157a911d0`、`ee9a96b0d`、`ae11e342c`、`2575dfa6f`）；P3 的矩阵已进 CI，**待首轮结果**再决定是否把默认切到 database
- 前置：`562a58d4e`（index 改名）、`3909daddd`（atexit 泄漏修复）、`69eeb090e`（健康守卫）
- 实测环境：Mac mini / PostgreSQL 16.15 容器（`127.0.0.1:55432`），基线 221 张业务表 / 20 MB

## 1. 为什么还值得动

今天为止 schema 级 clone 已经做过三轮优化：索引改两段式改名（不再重建 577 个索引）、
泄漏修到零、catalog 可压缩。单条测试的摊销成本仍是 **1.47 s**（286 条 4 线程跑
421 s），其中绝大部分是「221 张表的 `CREATE TABLE LIKE` + 数据拷贝 + 约束/序列重建」。

只要隔离单元仍是 schema，就绕不开这些 DDL；换成 database 级，隔离单元是一份**已经
建好的库文件副本**，创建时不做任何 DDL。

## 2. 实测数据（本次新测，非估算）

模板库：`CREATE DATABASE synapse_dtmpl` + 逐字执行
`migrations/00000000_unified_schema_v12.sql`，用时 **15.1 s**（一次性，可缓存）。

| 操作 | 耗时 |
|---|---|
| `CREATE DATABASE x TEMPLATE synapse_dtmpl`（模板 `datallowconn=false`） | 0.29 / 0.37 / 0.44 / 0.61 / 0.79 s |
| 同上，4 个并发（模拟 4 线程同时要池） | **1.15 s 总墙钟**（≈0.29 s/个，模板锁串行） |
| `DROP DATABASE x`（热身后） | 0.28 / 0.30 / 0.32 s |
| `DROP DATABASE x WITH (FORCE)` | 0.28 / 0.30 s |
| 首轮满负载后的第一次 DROP | **26.4 s**（见 §5.4） |
| 现状：schema clone（单条测试池） | 1.2 – 3.5 s |
| 现状：`DROP SCHEMA ... CASCADE` | 0.7 – 1.8 s |

结论：**创建 4–10x 更快，清理不更贵，且不需要任何"改名/重定向"后处理**。
每条约省 1–3 s；按 1619 条集成测试算，全量可省 30–80 分钟。

## 3. 方案对比

| | A. schema clone（现状） | B. database clone（本设计） |
|---|---|---|
| 隔离单元 | `test_<pid>_<n>_<nanos>` schema | 同名 database |
| 创建动作 | 221 × `CREATE TABLE LIKE` + 数据 + 索引改名 + 约束改名 + FK/函数/视图重定向 + 序列 `setval` | `CREATE DATABASE ... TEMPLATE`（0.3 s） |
| 正确性风险 | 名字/依赖重定向每一步都可能漏（D-80 那类） | 结构与模板**逐字节一致**，无重定向 |
| 清理 | `DROP SCHEMA`（lease 守卫） | `DROP DATABASE [WITH FORCE]` |
| 清理失败的后果 | 残留 schema，catalog 膨胀（可自愈） | 残留 20 MB 库 + 一行 `pg_database` |
| 连接串 | 同一 URL + `search_path` | 每个测试一个 URL（数据库名 不同） |

B 的主要吸引力不只是快，而是**删掉了整块"重建后修正"的逻辑**：`clone_table_chunk_statement`
的 phase 1c/1d/1e/2、`advance_schema_sequences`、`validate_clone` 的名字比对，在 B 下
要么不需要，要么变成"模板建好时验证一次"。

## 4. 设计（B）

### 4.1 模板库

- 名字：`synapse_tmpl_v<REVISION>_<fingerprint16>`，与现有
  `test_template_v<REVISION>_<fingerprint>` 同构，避免两套指纹规则。
- 建法：`CREATE DATABASE ... TEMPLATE template0` → 执行基线 SQL → `ANALYZE`。
- **建完立刻** `UPDATE pg_database SET datallowconn = false`。这既是并发安全的保证
  （`CREATE DATABASE ... TEMPLATE` 要求模板上没有任何会话），也防止测试误连模板。
  实测禁连后 clone 仍可用（0.44 s）。
- 一次性成本 15.1 s，由 `OnceCell` + 指纹判定，跨进程复用。

> **实测修正（2026-10-20）**：最初的设计是在测试进程内按需建模板。实测发现
> `CREATE DATABASE` 会先提交、迁移后跑，四个进程竞争时会把半成品当就绪模板
> （每条 6.4 s、留下 9 MB 半成品）。因此模板改为**进程外预置**
> （`scripts/test_db_template.sh ensure`），`datallowconn = false` 即就绪标记。

### 4.2 取池

1. `pg_try_advisory_lock(template_lock_key)`（会话级，长驻到 clone 结束）——
   串行化 `CREATE DATABASE`，避免模板锁冲突时互相报错；4 并发实测总墙钟 1.15 s。
2. `CREATE DATABASE test_<pid>_<n>_<nanos> TEMPLATE synapse_tmpl_...`。
3. 用 base URL 换 数据库名 建 `PgPool`；租约 key 沿用 `schema_lease_key(name)`（改名即可，
   语义从"schema 在用时"变成"库在用时"）。

命名继续用 **`test_<pid>_<n>_<nanos>`**：时间戳仍是"落盘清单"，跨进程按龄回收。

### 4.3 清理

- `on_release`：`DROP DATABASE ... WITH (FORCE)`（FORCE 只踢本测试可能残留的连接；
  实测 0.28–0.30 s）。用短连接、独立于即将关闭的池。
- `atexit` 兜底：与今天同一套 `drain_schemas_at_exit`（已修好入口注册）。
- 下一次运行的守卫：扫 `pg_database` 里 `test_<pid>_<n>_<nanos>`、名字年龄 > 10 min
  且 `pg_stat_activity` 无连接 → `DROP DATABASE WITH (FORCE)`；每进程上限 5 个。

### 4.4 兼容层

- `TEST_DB_ISOLATION=schema|database`，**默认 `schema`**。两条路径实现同一组内部契约
  （`prepare_shared_test_pool` / `acquire_pooled_schema` / `prepare_isolated_test_pool`
  的返回类型与租约语义不变），调用方零改动。
- B 路径下 `TEST_DB_TEMPLATE_SCHEMA=public` 这类 workaround 不再需要（每个库自带 public）；
  保留读取但忽略，并打印一次提示。
- 数据库不可用 / 无 `CREATEDB` 权限时**回落到 A**，日志写明原因。

### 4.5 CI

- `max_connections` 不变（DB 只隔离数据，不隔离连接）；但每个测试有自己的
  `PgPool`，`DEFAULT_TEST_DB_MAX_CONNECTIONS = 10` 仍生效。
- CI 的 PG service 需要预创建模板库（`scripts/dev-test-setup.sh` 与 `ci.yml` 各一步）。
- 磁盘：每个并发测试 20 MB；4 线程 ≈ 80 MB 峰值，可忽略。

## 5. 风险与缓解

1. **模板锁串行**：4 并发总墙钟 1.15 s，仍远快于 4 × schema clone。若成瓶颈，可做
   `N` 个同指纹模板（`..._0`/`..._1`）轮转——暂不做。
2. **`DROP DATABASE` 不能被事务包裹**：清理走独立短连接（已在 schema 版验证过的模式）。
3. **残留库比残留 schema 贵**：20 MB/个。守卫按龄回收 + `test_db_health.sh` 增加
   "残留库数 / 残留库体积"，阈值沿用 50。
4. **首轮第一次 DROP 26 s**：实测出现在大量写入后（迁移把 shared_buffers 打脏，
   drop 触发的 checkpoint 在首轮买单），其后每次 0.28–0.32 s。缓解：模板建好后
   `CHECKPOINT` 一次，把成本挪到一次性阶段。**这一条实施时必须先复测**。
5. **测试里对 `current_schema()` / 固定 schema 名的假设**：B 下 `current_schema()` 仍是
   `public`，比现在更自然也更容易满足；但 `SET search_path TO <schema>, public` 这类
   夹具代码需要按路径分支。
6. **`CREATEDB` 权限**：本地 `synapse` 是超级用户；CI 的 service 用户同样。若某个环境
   不是，回落 A 而不是硬失败。

## 6. 分期与验收

| 阶段 | 内容 | 验收 |
|---|---|---|
| P0 ✅ | 模板库创建 + 禁连 + `CHECKPOINT`，暴露为独立脚本 | 已验收：`ensure` 幂等、`datallowconn=false`、`clone` 0.117 s / 221 表、非测试集群 exit 2 |
| P1 ✅ | `TEST_DB_ISOLATION=database` 只接 `prepare_shared_test_pool` | 已验收：13 条 4.1 s（schema 11.9–15.3 s）、286 条 **193.6 s**（schema 421.3 s）、残留 0 |
| P2 ✅ | 接 `acquire_pooled_schema` / `prepare_isolated_test_pool` | 已验收：41 条（`nullable_decode`/`cleanup_`/`api_one_time_invite`） database 模式 7.6 s vs schema 41.6 s，两边 41/41，残留库 0 |
| P3 🔧 | CI 双跑一个周期（A 与 B 各一遍） | 已接线 `2575dfa6f`：`integration-test` 矩阵 `schema`(90 min, 全车道) / `database`(60 min, 仅 integration)，`fail-fast: false`，database 腿额外断言残留库为 0。**首轮本机双跑已做（结果与判定见 §8）：未达标**；目标仍是两边全绿 + B 墙钟 < A 的 60% |
| P4 ✅ | 模板命名收敛 + 契约 SQL 共享 | 已交付 `ae11e342c`：指纹 = sha256(排序后的 migrations/*.sql + test-support/test_schema_contract.sql) 前 16 位，脚本与 crate 逐字节一致；契约 SQL 抽成共享文件，两边各用一份（`psql -f` / `include_str!`） |

性能验收：286 条 `room_` 子集墙钟从 **421 s**（当前）降到 **≤ 240 s**；单条摊销
≤ 0.8 s。

## 7. 未决问题

- 是否需要"每测试一个 database" vs "database + 复用（TRUNCATE）"？后者能省创建，
  但要重新引入 TRUNCATE 列表维护（现有 `truncate_and_reseed_schema` 可复用）。
- `pg_database` 行数在多轮长跑后的 catalog 膨胀，是否也需要 `VACUUM FULL pg_database`？
- 模板指纹变化时，旧模板库的回收策略（现在 schema 模板靠 marker 文件判定，库版本同样需要）。
- **模板命名已统一**（`ae11e342c`）：单一指纹覆盖迁移与契约 SQL，`resolve_template_database`
  严格匹配期望名字，旧模板会以"不存在"报错而不是被静默复用。

## 8. P3 首轮双跑结果（2026-10-21 本机）

### 8.1 原始数据

两腿都是 `cargo nextest run --test integration --all-features --locked --test-threads 4`，
只改 `TEST_DB_ISOLATION`；`TEST_DATABASE_URL` 指向本机 55432 的一次性库。

| 腿 | `TEST_DB_ISOLATION` | 起止（CST） | nextest 墙钟 | tests run | passed | failed | exit |
|---|---|---|---|---|---|---|---|
| A | `schema` | 18:10:39 → 18:57:10 | 2786.8 s | 1624 | 1619 | **5** | 100 |
| B | `database` | 21:50:10 → 22:43:47 | 1795.5 s | 1637 | 1616 | **21** | 100 |

### 8.2 先说一个前提：两腿跑的不是同一份代码

**这轮数据不能直接做"模式 vs 模式"的对比。** A 腿启动于 18:10，早于当天
`90a0ebecf`（19:30）/ `325c1ef16`（20:17）两次 admin-RBAC 收紧；B 腿启动于 21:50，
包含它们以及 `8d7bc3a97`（21:21）。

测试清单本身就是证据（逐条 diff `PASS/FAIL` 行）：

- **B 有、A 无 15 条**：`api_admin_rbac_wire_tests` 5 条、`api_admin_target_guard_tests` 8 条、
  `api_admin_user_lifecycle_tests` 2 条 —— 都是 21:21 那次提交新增的。
- **A 有、B 无 2 条**：`permission_escalation_tests::test_rbac_{super,non_super}_admin_can_modify_admin_status`
  —— 同一次提交删掉的空转用例。
- 1624 − 2 + 15 = 1637 ✓。也就是说 A 腿的数字属于**旧二进制**，B 腿才是当前 HEAD。

### 8.3 21 条失败逐条归因

| 类别 | 条数 | 用例 | 判定 |
|---|---|---|---|
| 两腿共有 | 4 | `api_route_snapshots_tests::snapshot_register_uia_401_challenge`、`api_space_routes_tests::test_space_children_hierarchy_suite_keeps_nested_chain_verified`、`sync_service_tests_migrated::test_incremental_lazy_load_limited_timeline_does_not_replay_state_delta_members`、`api_appservice_p1_tests::test_appservice_transaction_push` | **与隔离模式无关的真 bug**：两腿 panic 点与断言值一致（`left: Null`、insta 快照不一致等） |
| A 腿独有 | 1 | `room_service_tests_migrated::test_appservice_scheduler_long_window_mixed_backlog_preserves_fairness_and_prevents_event_starvation` | 4 线程下的调度时序敏感，未定根因 |
| B 腿独有（已定位） | 16 | 15 条 `api_appservice_*` + `api_feature_flags_tests::test_feature_flag_create_get_list_and_update` | **RBAC 收紧与测试口径不一致**，与隔离模式无关（见 8.4） |
| B 腿独有（未定） | 1 | `api_beacon_location_tests::test_beacon_location_applies_room_backpressure_token_bucket`（期望 429 未拿到） | 令牌桶依赖墙钟，疑似时间敏感，未定根因 |

B 腿日志里的 16 条 403 形态完全一致：

```
RBAC check result role=admin method=POST path=/_synapse/admin/v1/appservices
                 allowed=false rbac_enabled=true rbac_allowed=false
assertion `left == right` failed  left: 403  right: 201
```

对应 [privileged.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/utils/admin_auth/privileged.rs#L55-L75)：
`appservices` 与 `feature-flags` 的写操作在 2026-10-21 被列为 **super_admin-only**，
而这些用例仍用 `get_admin_token`（`user_type='admin'`）。

### 8.4 关键对照：schema 模式下同样 403

为排除"是不是 database 隔离导致的"，在**当前代码**上用 `TEST_DB_ISOLATION=schema`
单跑两条（`--test-threads 1`）：

| 用例 | 结果 | 日志 |
|---|---|---|
| `api_appservice_p1_tests::test_appservice_as_token_authentication` | `left: 403, right: 201` | `role=admin method=POST path=/_synapse/admin/v1/appservices allowed=false rbac_enabled=true` |
| `api_feature_flags_tests::test_feature_flag_create_get_list_and_update` | `left: 403, right: 200` | `role=admin method=POST path=/_synapse/admin/v1/feature-flags allowed=false` |

结论：**16 条 403 与隔离方式无关**，是 RBAC 收紧后测试未跟着改类型。

**产品裁决（2026-10-22）：不放宽放行面**，让测试使用正确的身份 —— 写面改用
`get_super_admin_token`，读面/自服务面保持 `get_admin_token`。落地与实测见 §8.6 第 1 条。

### 8.5 验收判定

| 验收项（§6 P3） | 目标 | 实测 | 判定 |
|---|---|---|---|
| 两腿全绿 | 0 failed | A 5 / B 21 | ❌ 未达标 |
| B 墙钟 < A 墙钟的 60% | ≤ 1672 s | 1795.5 s（64.4%） | ❌ 差 123 s（B 还多跑了 13 条） |
| B 腿残留库为 0 | 0 | 本机未单独断言，留给 CI 那一步 | — |
| 隔离模式自身的失败 | 0 | B 腿 21 条中 0 条可归因于 database 隔离 | ✅ 未发现 |

> 注：B 腿 21 条中的 16 条 RBAC 类已随 `2c9743df6` 修复（§8.6 第 1 条，子集实测
> 19/20 通过），上表数字保留为**首轮双跑的原始快照**，不随修复回填。

### 8.6 结论与后续

1. **database 方案本身没有被证伪**：P1/P2 的小样本对照（41/41、286 条）仍成立，本轮也没有
   出现任何"只在 database 模式复现"的隔离缺陷。
2. **但 P3 未通过**，原因不在隔离层：两腿代码版本不同 + 仓库里既有的 RBAC 收紧没有同步
   测试。要做有效的模式对比，必须先让两条腿跑同一份代码。
3. 下一步顺序（第 1 步已完成）：
   - [x] **修 16 条 RBAC 测试** —— 产品裁决：不放宽放行面，测试改用 `get_super_admin_token`。
     落地 `2c9743df6`：`api_appservice_p1_tests`（含共享 helper，覆盖 14 条）、
     `api_appservice_tests` 2 条写面、`api_feature_flags_tests` 1 条；读面/自服务面仍用
     `get_admin_token`。实测 `TEST_DB_ISOLATION=database` 跑这 20 条：
     **19 passed / 1 failed**，唯一失败 `api_appservice_transaction_push` 正是 §8.3 的
     "两腿共有"真 bug，已单列在下方第 4 条；
   - [x] **修 4 条两腿共有的真 bug**（2026-10-22，根因与验证见下方第 4 条）；
   - [ ] 两条时序用例（appservice 调度、beacon 令牌桶）仍未定根因，见下方第 5 条；
   - [ ] 然后同一提交上重跑两腿，再按 §6 的三条验收项判定是否把默认切到 `database`。
4. **4 条两腿共有的真 bug 已修**（2026-10-22，与隔离模式无关）：

   | 用例 | 根因 | 修复面 |
   |---|---|---|
   | `snapshot_register_uia_401_challenge` | 401 UIA 体按设计含 `errcode`/`error`（`docs/sdk/authentication.md`），`.snap` 停留在旧版 | 接受快照（删 `.snap.new`） |
   | `test_space_children_hierarchy_suite_keeps_nested_chain_verified` | `get_parent_spaces` 拿 `space_children.room_id` 去匹配 `spaces.room_id`，返回空数组 ⇒ `parents_json[0]` 为 `Null` | `synapse-storage/src/space/repository.rs` 改用 `space_id` 查父空间 |
   | `test_incremental_lazy_load_limited_timeline_does_not_replay_state_delta_members` | `1e801b33e` 把 event-id 抑制语义扩散到 limited 重建分支，已变更的成员 leave 被重放 | `synapse-services/src/sync_service/lazy_load.rs`：重建分支按成员抑制，`state_after`/非 limited 分支保留 event-id 语义（#20289 不回退） |
   | `test_appservice_transaction_push` | B-28 已改为 `{events, next_token}`，测试仍按裸数组读 | `tests/integration/api_appservice_p1_tests.rs` 改读 `events` 并断言 `next_token` 为 `null` |

   实测（`TEST_DB_ISOLATION=database`）：4 条单跑 **4 passed**；关联回归
   `api_space_routes_tests`/`api_appservice_p1_tests`/`sync_service_tests_migrated`/
   `api_route_snapshots_tests` 全量 **78 passed**；`synapse-services` 懒加载/空间单测
   **54 passed**；`cargo fmt --all --check` 通过。
5. **两条时序用例未定位根因**：A 腿独有的 appservice 调度
   （`test_appservice_scheduler_long_window_mixed_backlog_preserves_fairness_and_prevents_event_starvation`）
   与 B 腿独有的 beacon 令牌桶
   （`test_beacon_location_applies_room_backpressure_token_bucket`）在修复后的两次复跑中均
   PASS（单独复跑 3.8 s / 9.3 s），仍按**时序敏感疑似 flake** 记录，**不声称已修**。
