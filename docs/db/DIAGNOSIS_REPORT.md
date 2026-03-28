# synapse-rust 数据库问题诊断与优化方案(企业级交付版)

[![合规检查通过](https://github.com/langkebo/synapse-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/langkebo/synapse-rust/actions/workflows/ci.yml)

| 字段 | 值 |
|---|---|
| 文档 ID | DIAG-DB-2026-03-28 |
| 修订号 | v1.1.0 |
| 生成日期 | 2026-03-28 |
| 作者 | TBD |
| 审批人 | TBD |

## 目录

- [1. 背景与范围](#1-背景与范围)
- [2. 复现环境](#2-复现环境)
- [3. 证据索引](#3-证据索引)
- [4. 诊断步骤(现象→复现→定位→根因)](#4-诊断步骤现象复现定位根因)
- [5. 问题根因(5 Whys)](#5-问题根因5-whys)
- [6. 影响范围](#6-影响范围)
- [7. 可执行改进建议清单](#7-可执行改进建议清单)
- [8. 修复验证](#8-修复验证)
- [9. 风险与回滚](#9-风险与回滚)
- [10. 附录](#10-附录)
- [11. 配套交付物](#11-配套交付物)

## 1. 背景与范围

本报告覆盖 synapse-rust 的数据库 Schema, 迁移链闭环, 以及与存储层/路由层契约一致性相关问题。报告仅基于仓库可追溯证据与可复现实验输出，不包含未提供的生产日志与监控截图。

## 2. 复现环境

| 项 | 值 | 证据 |
|---|---|---|
| PostgreSQL | 15(CI 服务镜像) | [ci.yml:L39-L53](../../.github/workflows/ci.yml#L39-L53) |
| Schema 基线 | 00000000_unified_schema_v6.sql | [00000000_unified_schema_v6.sql](../../migrations/00000000_unified_schema_v6.sql) |
| 迁移执行方式 | sqlx migrate run(CI 中使用) | [ci.yml:L91-L98](../../.github/workflows/ci.yml#L91-L98) |

## 3. 证据索引

每条结论均绑定唯一追溯编号，证据链至少包含：代码引用点 + 迁移/Schema 状态 + 可复现实验或命令输出(若该项可在仓库环境复现)。

| ISSUE ID | 标题 | 严重级别 | 结论 | 证据 |
|---|---|---|---|---|
| ISSUE-2026-03-28-001 | Invite 黑/白名单表缺失 | P0 | 已修复：补齐 room_invite_blocklist/allowlist schema 闭环 | [invite_blocklist.rs](../../src/storage/invite_blocklist.rs), [unified_schema_v6.sql:L1612-L1653](../../migrations/00000000_unified_schema_v6.sql#L1612-L1653), [20260328000003_add_invite_restrictions...sql](../../migrations/20260328000003_add_invite_restrictions_and_device_verification_request.sql) |
| ISSUE-2026-03-28-002 | 设备验证请求表缺失 | P0 | 已修复：补齐 device_verification_request schema 闭环(TIMESTAMPTZ) | [device_trust/storage.rs](../../src/e2ee/device_trust/storage.rs), [unified_schema_v6.sql:L568-L592](../../migrations/00000000_unified_schema_v6.sql#L568-L592), [20260328000003_add_invite_restrictions...sql](../../migrations/20260328000003_add_invite_restrictions_and_device_verification_request.sql) |
| ISSUE-2026-03-28-003 | thread_read_receipts 表缺失 | P1 | 已修复：补齐 unified schema 与增量迁移，并通过存储层 roundtrip 验证 | [thread.rs](../../src/storage/thread.rs), [00000000_unified_schema_v6.sql](../../migrations/00000000_unified_schema_v6.sql), [20260330000001_add_thread_replies_and_receipts.sql](../../migrations/20260330000001_add_thread_replies_and_receipts.sql), [thread_storage_tests.rs](../../tests/unit/thread_storage_tests.rs) |
| ISSUE-2026-03-28-004 | thread_replies 表缺失 | P1 | 已修复：补齐 unified schema 与增量迁移，并补强 thread_roots/thread_relations 契约 | [thread.rs](../../src/storage/thread.rs), [00000000_unified_schema_v6.sql](../../migrations/00000000_unified_schema_v6.sql), [20260330000001_add_thread_replies_and_receipts.sql](../../migrations/20260330000001_add_thread_replies_and_receipts.sql), [20260330000002_align_thread_schema_and_relations.sql](../../migrations/20260330000002_align_thread_schema_and_relations.sql) |
| ISSUE-2026-03-28-005 | room_retention_policies 表缺失 | P1 | 已修复：补齐 unified schema 与增量迁移，并通过 retention 存储层 roundtrip 验证 | [retention.rs](../../src/storage/retention.rs), [00000000_unified_schema_v6.sql](../../migrations/00000000_unified_schema_v6.sql), [20260330000003_align_retention_and_room_summary_schema.sql](../../migrations/20260330000003_align_retention_and_room_summary_schema.sql), [retention_storage_tests.rs](../../tests/unit/retention_storage_tests.rs) |
| ISSUE-2026-03-28-006 | room_summary_members 表缺失 | P1 | 已修复：补齐 unified schema 与增量迁移，并通过 room summary 存储层 roundtrip 验证 | [room_summary.rs](../../src/storage/room_summary.rs), [00000000_unified_schema_v6.sql](../../migrations/00000000_unified_schema_v6.sql), [20260330000003_align_retention_and_room_summary_schema.sql](../../migrations/20260330000003_align_retention_and_room_summary_schema.sql), [room_summary_storage_tests.rs](../../tests/unit/room_summary_storage_tests.rs) |
| ISSUE-2026-03-28-007 | space_statistics/space_summaries/space_events 入口不闭环 | P2 | 已修复：统一 spaces/space_members/space_summaries/space_statistics/space_events 进入 unified schema，并补齐对齐迁移与最小数据库回归测试 | [space.rs](../../src/storage/space.rs), [00000000_unified_schema_v6.sql](../../migrations/00000000_unified_schema_v6.sql), [20260326000006_create_space_statistics_table.sql](../../migrations/20260326000006_create_space_statistics_table.sql), [20260327_p2_fixes.sql](../../migrations/20260327_p2_fixes.sql), [20260330000004_align_space_schema_and_add_space_events.sql](../../migrations/20260330000004_align_space_schema_and_add_space_events.sql), [db_schema_smoke_tests.rs](../../tests/unit/db_schema_smoke_tests.rs) |

## 4. 诊断步骤(现象→复现→定位→根因)

### 4.1 ISSUE-2026-03-28-001：Invite 黑/白名单表缺失(已修复)

| 段落 | 内容 |
|---|---|
| 现象 | 运行 Invite Blocklist/Allowlist 相关接口时，存储层对 room_invite_blocklist/allowlist 的读写依赖表存在；表缺失会导致 SQL 执行失败。 |
| 复现 | 在 PostgreSQL 15 环境中执行 unified schema 后，执行以下 SQL 与 EXPLAIN, 验证索引命中与执行耗时。 |
| 定位 | 存储层使用 created_ts (ms) 字段，且依赖 room_id 维度查询索引；插入使用 ON CONFLICT DO NOTHING，因此必须有 UNIQUE(room_id,user_id)。 |
| 根因 | 迁移链与存储层契约未闭环：代码已实现，schema 缺失导致运行期失败。 |

复现命令(CI 等价环境，PostgreSQL 15)：

```bash
docker exec -i pg15diag psql -U synapse -d synapse -v ON_ERROR_STOP=1 -c "INSERT INTO users (user_id, username, created_ts) VALUES ('@diag:localhost','diag',0),('@blocked:localhost','blocked',0),('@allowed:localhost','allowed',0) ON CONFLICT (user_id) DO NOTHING;"
docker exec -i pg15diag psql -U synapse -d synapse -v ON_ERROR_STOP=1 -c "INSERT INTO rooms (room_id, creator, created_ts) VALUES ('!diagroom:localhost','@diag:localhost',0) ON CONFLICT (room_id) DO NOTHING;"
docker exec -i pg15diag psql -U synapse -d synapse -v ON_ERROR_STOP=1 -c "INSERT INTO room_invite_blocklist (room_id, user_id, created_ts) VALUES ('!diagroom:localhost','@blocked:localhost',0) ON CONFLICT (room_id, user_id) DO NOTHING;"
docker exec -i pg15diag psql -U synapse -d synapse -v ON_ERROR_STOP=1 -c "INSERT INTO room_invite_allowlist (room_id, user_id, created_ts) VALUES ('!diagroom:localhost','@allowed:localhost',0) ON CONFLICT (room_id, user_id) DO NOTHING;"
```

执行计划与哈希(计划文本 SHA-256；单位：ms)：

```text
blocklist_select
plan_sha256: dcfaafb4dc6ee77d2d6dd90ed36adee8767a17b31acd5eaac545218a62236b88
Planning Time: 0.204 ms
Execution Time: 0.035 ms
```

```text
allowlist_select
plan_sha256: e5b69167c2a2bce0bed7f07e8e59ae284d55b362c84266cbcdebacccbf450020
Planning Time: 0.136 ms
Execution Time: 0.063 ms
```

### 4.2 ISSUE-2026-03-28-002：device_verification_request 表缺失(已修复)

| 段落 | 内容 |
|---|---|
| 现象 | Device Trust / Device Verification 相关 API 依赖 device_verification_request 表；表缺失会导致请求创建/查询失败。 |
| 复现 | 在 PostgreSQL 15 环境插入 1 条 pending 记录后，验证按 token 与按 (user_id,new_device_id) 的查询均走索引。 |
| 定位 | 代码使用 TIMESTAMPTZ 字段 created_at/expires_at/completed_at，与仓库字段标准(_ts/_at)存在风格差异，但与当前 Rust 类型(DateTime<Utc>)一致。 |
| 根因 | 迁移链与存储层契约未闭环：代码已实现，schema 缺失导致运行期失败。 |

复现命令：

```bash
docker exec -i pg15diag psql -U synapse -d synapse -v ON_ERROR_STOP=1 -c "INSERT INTO device_verification_request (user_id, new_device_id, requesting_device_id, verification_method, status, request_token, commitment, pubkey, created_at, expires_at, completed_at) VALUES ('@diag:localhost','DEVICE_NEW','DEVICE_REQ','sas','pending','tok_diag_1','commitment_1','pubkey_1',NOW(),NOW() + interval '5 minutes',NULL) ON CONFLICT (request_token) DO NOTHING;"
```

执行计划与哈希(单位：ms)：

```text
dvr_by_token
plan_sha256: 1274dfa452fc5860e4132aea034ebba7f5eecc77483bb6b2f6fb9bbb80b294e8
Planning Time: 0.162 ms
Execution Time: 0.022 ms
```

```text
dvr_pending
plan_sha256: b621de9dd76f40b1143e940461f215f0aefacd89e9a6fc76e01e7cc069aadf3e
Planning Time: 0.180 ms
Execution Time: 0.014 ms
```

### 4.3 ISSUE-2026-03-28-003/004：thread_read_receipts, thread_replies 表缺失(已修复)

| 段落 | 内容 |
|---|---|
| 现象 | Thread 相关功能会对 thread_replies 与 thread_read_receipts 进行插入/查询/更新；同时 thread_roots 聚合字段与 thread_relations 依赖存在契约收口需求。 |
| 复现 | 在 PostgreSQL 15 容器中执行 unified schema + 线程增量迁移后，运行 thread_storage_tests 的 roundtrip 用例，验证 root/reply/read-receipt/relation/summary/statistics/search 全链路通过。 |
| 定位 | 存储层除 thread_replies 与 thread_read_receipts 外，还依赖 thread_roots 的 reply_count/last_reply_*/participants 聚合字段，以及 thread_relations 的关系写入。 |
| 根因 | 线程相关 Schema 与存储层契约演进不同步：缺表、缺索引与聚合字段未闭环并存，导致新环境与真实读写路径之间存在运行期风险。 |

证据(代码/迁移/验证)：

- [thread.rs](../../src/storage/thread.rs)
- [00000000_unified_schema_v6.sql](../../migrations/00000000_unified_schema_v6.sql)
- [20260330000001_add_thread_replies_and_receipts.sql](../../migrations/20260330000001_add_thread_replies_and_receipts.sql)
- [20260330000002_align_thread_schema_and_relations.sql](../../migrations/20260330000002_align_thread_schema_and_relations.sql)
- [thread_storage_tests.rs](../../tests/unit/thread_storage_tests.rs)

### 4.4 ISSUE-2026-03-28-005/006/007：retention / room summary / space schema 闭环(已修复)

| 段落 | 内容 |
|---|---|
| 现象 | retention、room summary 与 space 相关读写路径在新环境依赖 `room_retention_policies`、`room_summary_members`、`space_*` 表；统一 schema 与增量迁移未完全收口时，容易在 fresh DB 或 CI 路径中出现运行期失败。 |
| 复现 | 对 unified schema 与增量迁移进行交叉审计，补齐 `spaces` 主表字段漂移、`space_events` 缺表，以及 `space_members`/`space_summaries`/`space_statistics` 的 unified schema 缺口；随后以最小数据库 smoke test 验证表存在与基础读写。 |
| 定位 | `SpaceStorage` 直接依赖 `join_rule`、`visibility`、`parent_space_id`、`space_summaries`、`space_statistics`、`space_events`；`RetentionStorage` 与 `RoomSummaryStorage` 则分别依赖 `room_retention_policies` 与 `room_summary_members`。 |
| 根因 | 统一 schema、增量迁移、代码引用表集合三者演进不同步，导致“增量可用但 unified schema 不闭环”与“依赖 exceptions 掩盖缺口”并存。 |

证据(代码/迁移/验证)：

- [retention.rs](../../src/storage/retention.rs)
- [room_summary.rs](../../src/storage/room_summary.rs)
- [space.rs](../../src/storage/space.rs)
- [20260330000003_align_retention_and_room_summary_schema.sql](../../migrations/20260330000003_align_retention_and_room_summary_schema.sql)
- [20260330000004_align_space_schema_and_add_space_events.sql](../../migrations/20260330000004_align_space_schema_and_add_space_events.sql)
- [db_schema_smoke_tests.rs](../../tests/unit/db_schema_smoke_tests.rs)

## 5. 问题根因(5 Whys)

### 5.1 Schema 闭环缺失类问题(以 ISSUE-2026-03-28-001 为例)

1. 为什么 API 不可用：SQL 执行依赖表不存在。  
2. 为什么表不存在：迁移脚本未包含对应 CREATE TABLE。  
3. 为什么迁移未覆盖：存储层实现与迁移维护缺少同一来源约束与门禁。  
4. 为什么缺少门禁：CI 仅验证 migrate run 是否成功，不验证代码引用表集合 subset-of schema.  
5. 为什么未建立该验证：缺少面向契约一致性的自动化检查(仓库级规则未固化为流水线门禁)。  

## 6. 影响范围

| 维度 | 影响描述 | 说明 |
|---|---|---|
| 集群 | TBD | 未提供生产集群清单与拓扑 |
| 数据库 | PostgreSQL(主从复制) | 术语统一为“主从复制” |
| 表 | room_invite_blocklist/allowlist, device_verification_request, room_retention_policies, room_summary_members, thread_*, space_* 等 | 详见证据索引 |
| 模块 | web/routes, storage, services | 存储层 SQL 直接依赖 |
| 用户量 | TBD | 未提供业务侧用户量与访问峰值 |
| SLA 降级 | TBD | 未提供 SLO/SLA 与监控截图 |

## 7. 可执行改进建议清单

按严重程度分级：

- P0：阻塞发布或功能硬失败
- P1：性能衰减 ≥ 20% 或稳定性显著风险
- P2：性能衰减 < 20% 或技术债

| 优先级 | 编号 | 标题 | 描述 | 关联 ISSUE | 预估工作量(人/日) | 验证方式 | 回滚策略 |
|---|---|---|---|---|---:|---|---|
| P0 | ACT-2026-03-28-001 | 已补齐 Invite 黑/白名单两表 | schema 闭环已补齐并加索引/FK | ISSUE-2026-03-28-001 | 0.5 | 集成测试 + EXPLAIN 回归 | 回滚迁移：DROP TABLE/INDEX |
| P0 | ACT-2026-03-28-002 | 已补齐 device_verification_request 表 | 与 DateTime<Utc> 类型一致；补索引 | ISSUE-2026-03-28-002 | 0.5 | 集成测试 + EXPLAIN 回归 | 回滚迁移：DROP TABLE/INDEX |
| P1 | ACT-2026-03-28-003 | 已补齐 thread_replies/thread_read_receipts 与 thread_relations | 统一 schema、增量迁移、thread_roots 聚合字段与关键索引已同步收口 | ISSUE-2026-03-28-003, ISSUE-2026-03-28-004 | 1.0 | 存储层 roundtrip + 关键查询验证 | 回滚迁移：DROP TABLE/INDEX |
| P1 | ACT-2026-03-28-004 | 已落地并收缩 schema 对应性门禁 | 扫描 Rust SQL 引用表名、修正误报规则、移除已闭环 exceptions，并在 CI 执行 | ISSUE-2026-03-28-001 | 1.5 | 脚本校验 + CI 门禁 | 例外清单渐进收缩 |
| P2 | ACT-2026-03-28-005 | 已统一 space_* 进入统一 schema | `spaces` 主表字段与 `space_members`/`space_summaries`/`space_statistics`/`space_events` 已同步收口，并补最小数据库回归测试 | ISSUE-2026-03-28-007 | 0.5 | sqlx migrate run + 全量建库验证 + 最小数据库回归测试 | 保留增量迁移幂等 |

## 8. 修复验证

### 8.1 验证 SQL

```sql
SELECT to_regclass('public.room_invite_blocklist') IS NOT NULL AS room_invite_blocklist_exists;
SELECT to_regclass('public.room_invite_allowlist') IS NOT NULL AS room_invite_allowlist_exists;
SELECT to_regclass('public.device_verification_request') IS NOT NULL AS device_verification_request_exists;
SELECT to_regclass('public.room_retention_policies') IS NOT NULL AS room_retention_policies_exists;
SELECT to_regclass('public.room_summary_members') IS NOT NULL AS room_summary_members_exists;
SELECT to_regclass('public.thread_replies') IS NOT NULL AS thread_replies_exists;
SELECT to_regclass('public.thread_read_receipts') IS NOT NULL AS thread_read_receipts_exists;
SELECT to_regclass('public.thread_relations') IS NOT NULL AS thread_relations_exists;
SELECT to_regclass('public.space_members') IS NOT NULL AS space_members_exists;
SELECT to_regclass('public.space_summaries') IS NOT NULL AS space_summaries_exists;
SELECT to_regclass('public.space_statistics') IS NOT NULL AS space_statistics_exists;
SELECT to_regclass('public.space_events') IS NOT NULL AS space_events_exists;
```

### 8.2 自动化测试用例

- Invite 黑/白名单：见 [invite_blocklist_tests.rs](../../tests/unit/invite_blocklist_tests.rs)(当前为 ignored; 需要 CI DB 环境启用并断言表存在).
- Retention 存储层：见 [retention_storage_tests.rs](../../tests/unit/retention_storage_tests.rs)(覆盖 room_retention_policies / server_retention_policy roundtrip).
- Room Summary 存储层：见 [room_summary_storage_tests.rs](../../tests/unit/room_summary_storage_tests.rs)(覆盖 room_summary_members 与 room_summaries roundtrip).
- Thread 存储层：见 [thread_storage_tests.rs](../../tests/unit/thread_storage_tests.rs)(覆盖 root/reply/read-receipt/relation/summary/statistics/search roundtrip).
- Schema Smoke：见 [db_schema_smoke_tests.rs](../../tests/unit/db_schema_smoke_tests.rs)(覆盖 retention / room summary / space 关键表存在性与最小读写).

### 8.3 回滚脚本(示例)

```sql
DROP TABLE IF EXISTS room_invite_blocklist;
DROP TABLE IF EXISTS room_invite_allowlist;
DROP TABLE IF EXISTS device_verification_request;
DROP TABLE IF EXISTS thread_relations;
DROP TABLE IF EXISTS thread_read_receipts;
DROP TABLE IF EXISTS thread_replies;
```

### 8.4 验证人签字

| 角色 | 姓名 | 签字/链接 | 日期 |
|---|---|---|---|
| 开发 | TBD | TBD | TBD |
| 测试 | TBD | TBD | TBD |
| DBA | TBD | TBD | TBD |
| 安全 | TBD | TBD | TBD |

## 9. 风险与回滚

| 风险 | 触发条件 | 缓解措施 | 回滚策略 |
|---|---|---|---|
| 表结构变更影响线上写入 | 热路径写入依赖新表 | 低峰发布；先建表后灰度启用功能 | DROP TABLE/INDEX；禁用相关路由 |
| 索引创建影响写入吞吐 | 大表上创建索引 | 使用 CONCURRENTLY(仅适用于增量迁移) | DROP INDEX CONCURRENTLY |

## 10. 附录

### 10.1 版本变更记录

| 修订号 | 日期 | 作者 | 变更摘要 | 审批人电子签名 |
|---|---|---|---|---|
| v1.0.0 | 2026-03-28 | TBD | 初始版本 | TBD |
| v1.1.0 | 2026-03-28 | TBD | 引入证据索引、Issue 编号、EXPLAIN 哈希与企业级结构 | TBD |
| v1.2.0 | 2026-03-28 | TBD | 补充 thread P1 修复结果、门禁基线与存储层 roundtrip 验证 | TBD |
| v1.3.0 | 2026-03-28 | TBD | 修正 retention/room summary/space 诊断结论，补齐 unified schema、space_events 迁移与最小数据库回归测试 | TBD |

### 10.2 配置示例(PostgreSQL 慢查询与死锁日志)

变更前后 diff(以 postgresql.conf 为例；作用域=实例级；需要 reload/restart 以官方说明为准)：

```diff
 log_min_duration_statement = -1
 log_lock_waits = off
 deadlock_timeout = 1s
 log_line_prefix = '%m [%p] %u@%d '
 log_min_duration_statement = 200ms
 log_lock_waits = on
 deadlock_timeout = 200ms
 log_line_prefix = '%m [%p] %u@%d '
```

官方文档：

- https://www.postgresql.org/docs/15/runtime-config-logging.html
- https://www.postgresql.org/docs/15/runtime-config-locks.html

### 10.3 性能指标(基线/峰值/优化后)

本仓库未包含 Grafana 截图与导出数据。请将以下三组数据补入并在内部系统保存 ≥90 天：

- 基线：QPS, P95/P99 (ms), CPU, IO, 锁等待 (ms)
- 峰值：QPS, P95/P99 (ms), CPU, IO, 锁等待 (ms)
- 优化后：QPS, P95/P99 (ms), CPU, IO, 锁等待 (ms)

跳转锚点(用于链接检查与目录稳定)：

- [Grafana 截图(占位)](#103-性能指标基线峰值优化后)

### 10.4 质量门禁与后续工程化

静态检查门禁(CI 工作流见 docs-quality-gate.yml)：

- markdownlint: `markdownlint -c .markdownlint.json .`
- 拼写(英文): `bash scripts/check_doc_spelling.sh FILE.md`
- 链接检查: `lychee --base . --exclude-loopback .`

数据库一致性检查(本项目为 PostgreSQL，以下为等价替代项)：

- 完整性检查：pgamcheck(校验索引与系统表一致性)  
  https://www.postgresql.org/docs/15/app-pgamcheck.html
- 主从复制一致性：以业务可接受窗口为准，使用逻辑校验(例如按关键表分段 checksum/计数对齐)，并在报告中记录“差异位点数=0”的证据输出。

当前仓库已落地的门禁脚手架：

- `scripts/run_pg_amcheck.py`：在 CI/PostgreSQL 环境执行 `pg_amcheck`
- `scripts/generate_logical_checksum_report.py`：生成关键表逻辑 checksum 报告；提供 `REPLICA_DATABASE_URL` 时进入主从对比模式
- `scripts/logical_checksum_tables.txt`：关键表清单基线
- `db-migration-gate.yml`：上传逻辑 checksum 报告产物，供发布审批与外部证据回填

同行评审与门禁要求：

- Merge Request 必须指派至少 2 名领域专家(后端/DBA 或后端/安全组合)。
- 所有 review 对话必须在合并前 resolved。
- docs-quality-gate 工作流、主测试工作流、[db-migration-gate.yml](../../.github/workflows/db-migration-gate.yml) 全部通过后方可进入发布审批。

## 11. 配套交付物

| 交付物 | 用途 | 路径 |
|---|---|---|
| 外部证据模板 | 补齐生产日志, Grafana, 抓包, 签字, MR 证据 | [DIAGNOSIS_EXTERNAL_EVIDENCE_TEMPLATE.md](./DIAGNOSIS_EXTERNAL_EVIDENCE_TEMPLATE.md) |
| 迁移治理方案 | 落地迁移目录治理与数据库门禁 | [MIGRATION_GOVERNANCE.md](./MIGRATION_GOVERNANCE.md) |
| 优化任务清单 | 供 Jira/飞书多维表导入 | [DB_OPTIMIZATION_TASKS_2026-03-28.csv](./DB_OPTIMIZATION_TASKS_2026-03-28.csv) |
