# 测试库隔离：从 schema clone 换到 database clone（设计稿）

- 日期：2026-10-20
- 状态：**设计，未实施**
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
| P0 | 模板库创建 + 禁连 + `CHECKPOINT`，暴露为独立脚本 | 重复执行幂等；模板上 `datallowconn=false` |
| P1 | `TEST_DB_ISOLATION=database` 只接 `prepare_shared_test_pool` | 13 条 `api_account_data` + 286 条 `room_` 全绿；残留 = 0 |
| P2 | 接 `acquire_pooled_schema` / `prepare_isolated_test_pool` | 同上，且 `test_db_health.sh` 无新增残留 |
| P3 | CI 双跑一个周期（A 与 B 各一遍） | 两边全绿；B 的墙钟低于 A 的 60% |
| P4 | 默认切 B，保留 A 一个版本 | 文档/脚本更新，回落路径有测试覆盖 |

性能验收：286 条 `room_` 子集墙钟从 **421 s**（当前）降到 **≤ 240 s**；单条摊销
≤ 0.8 s。

## 7. 未决问题

- 是否需要"每测试一个 database" vs "database + 复用（TRUNCATE）"？后者能省创建，
  但要重新引入 TRUNCATE 列表维护（现有 `truncate_and_reseed_schema` 可复用）。
- `pg_database` 行数在多轮长跑后的 catalog 膨胀，是否也需要 `VACUUM FULL pg_database`？
- 模板指纹变化时，旧模板库的回收策略（现在 schema 模板靠 marker 文件判定，库版本同样需要）。
