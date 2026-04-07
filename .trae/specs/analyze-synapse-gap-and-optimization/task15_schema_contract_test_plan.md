# Task 15 - Schema Contract Test 方案

## 1. 测试目标

建立“迁移后真实 schema + 关键 SQL/SQLx 查询 + 最小业务闭环”的统一校验链，避免 schema 漂移仅靠人工心证发现。

## 2. 测试层级

| 层级 | 检查内容 | 示例 |
| --- | --- | --- |
| Schema 存在性 | 表、列、索引、约束是否存在 | `space_summaries` 表与关键列存在 |
| 类型与默认值 | `_ts`, `_at`, `is_` 等命名与类型契约 | `expires_at` 为可空、布尔列带 `is_` 前缀 |
| 查询契约 | SQL/SQLx 是否还能正确映射结果 | `room_summary`、`thread_roots` 查询仍可 decode |
| 行为契约 | 最小写入/读取/更新闭环 | 写 retention policy 后再次读取一致 |
| 错误分支 | 缺列、缺索引、命名漂移时应明确失败 | 删除列后 gate 返回清晰报错 |

## 3. 首批 contract 测试包

说明：本仓库以“独立 integration test target”为最小可执行单元。首批建议按能力域拆分为：

1. `schema_contract_room_core_tests`（rooms / events / room_memberships）
2. `schema_contract_account_data_tests`
3. `schema_contract_space_tests`
4. `schema_contract_search_tests`
5. `schema_contract_e2ee_verification_tests`
6. `schema_contract_thread_tests` + `schema_contract_retention_tests`

## 4. 每包最小闭环要求

- 至少 1 条 schema 存在性断言
- 至少 1 条关键查询断言
- 至少 1 条写后读一致性断言
- 至少 1 条错误场景阻断说明

## 5. 失败分类

- `migration_missing`
- `schema_missing`
- `schema_shape_mismatch`
- `query_decode_failure`
- `domain_behavior_mismatch`
- `test_fixture_error`

## 6. 落地建议（目录与入口）

当前已落地的 P0 最小契约基线：

- 测试文件：`tests/unit/schema_contract_p0_tests.rs`
- 运行方式：作为 `unit` test target 的一部分执行（与 `db_schema_smoke_tests` 同一测试入口）
- 已拆出的独立 integration target：
  - `tests/integration/schema_contract_room_core_tests.rs`
  - `tests/integration/schema_contract_room_summary_tests.rs`
  - `tests/integration/schema_contract_space_tests.rs`
  - `tests/integration/schema_contract_account_data_tests.rs`
  - `tests/integration/schema_contract_search_tests.rs`
  - `tests/integration/schema_contract_e2ee_verification_tests.rs`
  - `tests/integration/schema_contract_thread_tests.rs`
  - `tests/integration/schema_contract_retention_tests.rs`
  - `tests/integration/schema_contract_invite_restrictions_tests.rs`
  - `tests/integration/schema_contract_auth_tokens_tests.rs`
  - `tests/integration/schema_contract_presence_tests.rs`
  - `tests/integration/schema_contract_openid_token_tests.rs`
  - `tests/integration/schema_contract_media_quota_tests.rs`
  - `tests/integration/schema_contract_receipts_tests.rs`
- 独立入口命令：
  - `cargo test --locked --test schema_contract_room_core_tests -- --test-threads=1`
  - `cargo test --locked --test schema_contract_room_summary_tests -- --test-threads=1`
  - `cargo test --locked --test schema_contract_space_tests -- --test-threads=1`
  - `cargo test --locked --test schema_contract_account_data_tests -- --test-threads=1`
  - `cargo test --locked --test schema_contract_search_tests -- --test-threads=1`
  - `cargo test --locked --test schema_contract_e2ee_verification_tests -- --test-threads=1`
  - `cargo test --locked --test schema_contract_thread_tests -- --test-threads=1`
  - `cargo test --locked --test schema_contract_retention_tests -- --test-threads=1`
  - `cargo test --locked --test schema_contract_invite_restrictions_tests -- --test-threads=1`
  - `cargo test --locked --test schema_contract_auth_tokens_tests -- --test-threads=1`
  - `cargo test --locked --test schema_contract_presence_tests -- --test-threads=1`
  - `cargo test --locked --test schema_contract_openid_token_tests -- --test-threads=1`
  - `cargo test --locked --test schema_contract_media_quota_tests -- --test-threads=1`
  - `cargo test --locked --test schema_contract_receipts_tests -- --test-threads=1`
- 当前已补充 DB 级回归确认：
  - `tests/integration/database_integrity_tests.rs` 新增 `verification_requests` 完整迁移链回归断言，显式检查 `idx_verification_requests_to_user_state` 在完整迁移后仍存在
  - `database_integrity_tests` 已改为自动解析可用测试数据库 URL，并在连库后执行 `DatabaseInitService` 严格迁移初始化，避免因本地未显式导出 `TEST_DATABASE_URL` 而静默跳过
  - 针对 `room_summary_state`、`room_summary_stats`、`room_summary_update_queue`、`room_children`、`retention_*`、`deleted_events_index` 的 public schema orphan room 引用，已补充清理迁移与外键恢复迁移，确保 DB 级约束校验可执行
- 当前已补的首个“schema + query + write/read”样板：`push_rules`
  - 检查 `priority` 列与 `idx_push_rules_user_priority`
  - 复用真实查询：`ORDER BY priority DESC, created_ts ASC`
  - 覆盖插入 -> 查询 decode -> 更新 -> 再读取 -> 清理 的最小闭环
- 当前已补的第二个样板：`account_data` / `room_account_data`
  - 检查 `idx_account_data_user` 与两张表的唯一约束
  - 复用真实 upsert 语义：`ON CONFLICT ... DO UPDATE`
  - 覆盖插入 -> 读取 -> 再次 upsert -> 校验 `updated_ts` 与内容更新 -> 清理
- 当前已补的第三个样板：`search_index`
  - 检查 `idx_search_index_room`、`idx_search_index_user`、`idx_search_index_type`
  - 复用真实查询语义：`LOWER(content) LIKE ... ORDER BY created_ts DESC`
  - 复用真实 upsert 语义：`ON CONFLICT (event_id) DO UPDATE`
- 当前已补的第四个样板：`room_summaries` / `room_summary_members`
  - 检查主表主键、关联表唯一约束、summary 相关索引
  - 复用真实查询语义：`get_summaries_for_user`、`get_heroes`
  - 覆盖创建 summary -> 添加成员 -> 更新 membership/is_hero -> 校验聚合计数与可见性 -> 清理
- 当前已补的第五个样板：`space_summaries` / `space_members` / `space_events` / `space_statistics`
  - 检查 summary/member/event/statistics 相关列、唯一约束、主键与索引
  - 复用真实查询语义：`get_space_summary`、`get_space_events`、`get_space_statistics`
  - 复用真实聚合语义：`update_space_summary` 基于 `space_children` + `space_members` 刷新 `children_count` / `member_count`
  - 覆盖创建 space -> 添加成员 -> 添加 child -> 刷新 summary -> 校验 event/statistics -> 删除 child/member -> 再刷新
- 当前已补的第六个样板：`room_summary_state` / `room_summary_stats`
  - 检查 state/stats 相关列、唯一约束与索引
  - 复用真实查询语义：`get_state`、`get_all_state`、`get_stats`
  - 复用真实 upsert 语义：`set_state` 与 `update_stats`
  - 覆盖 state 插入 -> 同 key 更新 -> 全量读取 -> stats 插入 -> 覆盖更新 -> 再读取
- 当前已补的第七个样板：`room_summary_update_queue` / `room_children`
  - 检查 queue/children 相关列、唯一约束与索引
  - 复用真实查询语义：`get_pending_updates`
  - 复用真实状态迁移语义：`queue_update`、`mark_update_processed`、`mark_update_failed`
  - 覆盖 queue 入列 -> pending 排序 -> processed/failed 状态迁移 -> room_children upsert -> 再读取
- 当前已补的第八个样板：`space_children` / hierarchy
  - 检查 `space_children` 相关列、唯一约束与索引
  - 复用真实查询语义：`get_space_children`、`get_child_spaces`、`get_recursive_hierarchy`、`get_space_hierarchy_paginated`
  - 复用真实层级语义：space child 识别、递归深度、分页输出、`children_state` 构造
  - 覆盖 root space -> child space -> leaf room 的递归链路，并验证 hierarchy 输出中的 `room_type`、成员数和 child state
- 当前已补的第九个样板：`room_summary queue processor`
  - 复用真实 service 驱动语义：`queue_update`、`process_pending_updates`
  - 复用真实存储副作用：`room_summary_update_queue` 状态流转、`room_summary_state` 写入、`room_summaries` 最终摘要更新
  - 覆盖 state event 成功消费、普通 event 成功消费、缺失 event 失败消费 三种分支
  - 补齐 driver/worker 侧批处理契约：`limit` 分批消费、`failed` 不应被重复消费、`m.room.message` 应写入 `last_message_ts`（且后续普通事件不应清空）
- 当前已补的第十个样板：`media_quota_config` / `user_media_quota` / `media_usage_log` / `media_quota_alerts` / `server_media_quota`
  - 检查 quota 配置、用户累计使用量、未读告警与服务器总量相关列、默认值与索引
  - 复用真实查询语义：`get_default_config`、`get_or_create_user_quota`、`get_user_alerts`、`get_usage_stats`
  - 复用真实写入语义：`create_config`、`set_user_quota`、`update_usage`、`create_alert`、`mark_alert_read`、`update_server_quota`
  - 覆盖配置创建 -> 用户配额生成 -> 上传/删除计量 -> 告警读写 -> 服务器阈值更新 -> 清理 的最小闭环
- 当前已补的第十一个样板：`read_markers` / `event_receipts`
  - 检查 read marker / receipt 的主键、唯一键、关键索引与 JSON 默认值
  - 复用真实查询语义：`get_read_marker`、`get_all_read_markers`、`get_receipts`
  - 复用真实写入语义：`update_read_marker`、`update_read_marker_with_type`、`update_receipt`
  - 覆盖 `m.fully_read` upsert、`m.read` marker 写入、`m.read` / `m.read.private` receipt 去重与回读 的最小闭环

后续若需要按能力域拆分为独立“包”，再把 schema contract tests 迁移为可独立运行的 integration tests，建议落在：

```text
tests/integration/schema/
├── schema_contract_room_core_tests.rs
├── schema_contract_account_data_tests.rs
├── schema_contract_space_tests.rs
├── schema_contract_search_tests.rs
├── schema_contract_e2ee_verification_tests.rs
├── schema_contract_thread_tests.rs
├── schema_contract_retention_tests.rs
├── schema_contract_invite_restrictions_tests.rs
├── schema_contract_auth_tokens_tests.rs
├── schema_contract_presence_tests.rs
├── schema_contract_openid_token_tests.rs
├── schema_contract_media_quota_tests.rs
└── schema_contract_receipts_tests.rs
```

每个文件至少包含：
- schema 条目断言：对齐 [task15_schema_dependency_inventory.md](task15_schema_dependency_inventory.md) 的 P0 清单
- 关键查询断言：至少 1 条 SQLx/SQL decode 断言
- 行为闭环断言：至少 1 条写后读一致性断言

## 7. 本地复现（最小命令集）

- 运行 P0 schema contract baseline（当前已实现）：
  - `cargo test --locked --test unit schema_contract_p0 -- --test-threads=1`
- 若未来按 `tests/integration/schema/` 拆分为独立包：
  - 单包：`cargo test --locked --test schema_contract_room_core_tests -- --test-threads=1`
  - 全量：`cargo test --locked --test schema_contract_* -- --test-threads=1`

## 8. CI 对齐（与 migration gate 的口径闭环）

CI 中建议至少满足以下阻断链路（来自 `.github/workflows/db-migration-gate.yml`）：
- `Schema Table Coverage`：`scripts/check_schema_table_coverage.py --json-report artifacts/schema_table_coverage.json`
- `Schema Contract Coverage`：`scripts/check_schema_contract_coverage.py --threshold 90 --json-report artifacts/contract_coverage_report.json`
- `Unified Schema Apply`：统一 schema apply + `scripts/run_pg_amcheck.py`
- `sqlx Migrate Run`：`sqlx migrate run` + `db_schema_smoke_tests`

Schema contract tests（本任务新增）建议接在 `sqlx Migrate Run` 之后作为阻断项，确保“迁移可跑”并不等于“查询仍可用”；当前 `room core`、`room summary`、`space`、`account_data`、`search`、`e2ee` 已接入独立 integration target，`database_integrity_tests` 也已补上迁移回归与 public schema DB 级确认，后续可按相同模式继续拆更细粒度的子域查询。
