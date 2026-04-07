# Task 15 - Migration Gate 设计

## 1. 目标链路

```text
创建隔离数据库/Schema
    -> 执行统一迁移入口
    -> schema table coverage
    -> schema contract coverage
    -> amcheck / 结构健康检查
    -> 关键 schema smoke test
    -> 关键 domain contract test
    -> logical checksum / 结果归档
```

## 2. 推荐 gate 阶段

| 阶段     | 目的                        | 当前落点                                                | 失败阻断 |
| ------ | ------------------------- | --------------------------------------------------- | ---- |
| Gate 0 | 迁移布局审计                    | `db-migration-gate.yml` + migration audit 脚本        | 阻断   |
| Gate 1 | 表覆盖率                      | `check_schema_table_coverage.py`                    | 阻断   |
| Gate 2 | 列/索引/约束 contract          | `check_schema_contract_coverage.py`                 | 阻断   |
| Gate 3 | 统一 schema apply + amcheck | workflow + `run_pg_amcheck.py`                      | 阻断   |
| Gate 4 | 关键 smoke test             | thread / retention / room summary / db schema smoke | 阻断   |
| Gate 5 | 领域 contract test          | Task 15 新增 contract 包                               | 阻断   |
| Gate 6 | 逻辑校验与报告                   | logical checksum / artifacts                        | 先非阻断 |

## 3. 失败输出规范

* `migration_id`

* `domain`

* `table`

* `column_or_index`

* `query_or_test_name`

* `failure_class`

* `reproduce_command`

说明：

* Gate 0/1/2 的 Python gate 脚本已支持输出结构化 JSON 报告（包含上述字段），便于 CI 归档与定位。

* 推荐将报告写入 `artifacts/*.json` 并仅在失败时上传，避免 PR 噪音。

## 4. 当前已知治理缺口

* `DatabaseInitService` 仍保留兼容入口，应继续维持默认关闭，避免第二迁移口径。

### 4.1 当前已落地样板

* `push_rules` 已形成第一条最小领域 contract 样板：

  * schema 形状：列、唯一约束、排序相关索引

  * 查询契约：`ORDER BY priority DESC, created_ts ASC`

  * 行为闭环：插入 -> 查询 decode -> 更新 -> 再读取

* `account_data` / `room_account_data` 已形成第二条样板：

  * schema 形状：唯一约束、用户维度索引、JSONB 内容列

  * 查询契约：按 `user_id + data_type` / `user_id + room_id + data_type` 精确读取

  * 行为闭环：插入 -> 读取 -> upsert 更新 -> 再读取

* `search_index` 已形成第三条样板：

  * schema 形状：唯一约束、房间/用户/事件类型索引

  * 查询契约：`LOWER(content) LIKE` + `ORDER BY created_ts DESC`

  * 行为闭环：插入两条 fixture -> 验证排序 -> upsert 更新 -> 再读取

* `room_summaries` / `room_summary_members` 已形成第四条样板：

  * schema 形状：主键、成员唯一约束、summary/member 相关索引

  * 查询契约：成员可见性查询、hero 排序、member count 刷新

  * 行为闭环：创建 summary -> 添加成员 -> 更新 membership/is\_hero -> 校验聚合计数和排序

* `space_summaries` / `space_members` / `space_events` / `space_statistics` 已形成第五条样板：

  * schema 形状：summary/member/event/statistics 相关列、主键、唯一约束与索引

  * 查询契约：`get_space_summary`、`get_space_events`、`get_space_statistics`

  * 行为闭环：创建 space -> 添加成员 -> 添加 child -> `update_space_summary` 刷新计数 -> 删除 child/member -> 再刷新

* `room_summary_state` / `room_summary_stats` 已形成第六条样板：

  * schema 形状：state/stats 相关列、唯一约束与索引

  * 查询契约：`get_state`、`get_all_state`、`get_stats`

  * 行为闭环：state 插入 -> 同 key 更新 -> 全量读取 -> stats 插入 -> 覆盖更新 -> 再读取

* `room_summary_update_queue` / `room_children` 已形成第七条样板：

  * schema 形状：queue/children 相关列、唯一约束与索引

  * 查询契约：`get_pending_updates`、`room_children` 精确读取

  * 行为闭环：queue 入列 -> pending 排序 -> processed/failed 状态迁移 -> child upsert -> 再读取

* `space_children` / hierarchy 已形成第八条样板：

  * schema 形状：`space_children` 相关列、唯一约束与索引

  * 查询契约：`get_space_children`、`get_child_spaces`、`get_recursive_hierarchy`、`get_space_hierarchy_paginated`

  * 行为闭环：root space -> child space -> leaf room -> 递归层级查询 -> 分页输出与 `children_state` 校验

* `room_summary queue processor` 已形成第九条样板：

  * service 契约：`queue_update`、`process_pending_updates`

  * 副作用契约：processed/failed 分流、state event 写入 `room_summary_state`、普通 event 更新 `room_summaries.last_event_*`

  * 行为闭环：入列三条更新 -> 两条成功消费 + 一条失败消费 -> 校验 queue 状态、state 内容与 summary 最终值

  * driver/worker 契约：`limit` 分批消费、`failed` 不重复消费、`m.room.message` 写入 `last_message_ts`

* 后续可按相同模式扩展到 worker 调度、批量消费与重试退避策略。

## 5. CI 接线建议

* PR 默认阻断 Gate 0-5。

* Gate 6 先作为报告产物保留，后续按稳定度决定是否升级为阻断。

* 失败时上传：schema dump（expected/actual schema json）、contract diff（alignment report）、amcheck 输出、Gate 0/1/2 JSON 报告、对应测试日志。

* 成功时只保留精简 summary，避免产物噪音。

* 领域 contract test 已开始从 `tests/unit/schema_contract_p0_tests.rs` 拆到独立 integration target；

  * `room core`：`cargo test --locked --test schema_contract_room_core_tests -- --test-threads=1`

  * `room summary`：`cargo test --locked --test schema_contract_room_summary_tests -- --test-threads=1`

  * `space`：`cargo test --locked --test schema_contract_space_tests -- --test-threads=1`

  * `account_data`：`cargo test --locked --test schema_contract_account_data_tests -- --test-threads=1`

  * `search`：`cargo test --locked --test schema_contract_search_tests -- --test-threads=1`

  * `e2ee`：`cargo test --locked --test schema_contract_e2ee_verification_tests -- --test-threads=1`

  * `thread`：`cargo test --locked --test schema_contract_thread_tests -- --test-threads=1`

  * `retention`：`cargo test --locked --test schema_contract_retention_tests -- --test-threads=1`

  * `invite restrictions`：`cargo test --locked --test schema_contract_invite_restrictions_tests -- --test-threads=1`

  * `auth tokens`：`cargo test --locked --test schema_contract_auth_tokens_tests -- --test-threads=1`

  * `presence`：`cargo test --locked --test schema_contract_presence_tests -- --test-threads=1`

  * `openid token`：`cargo test --locked --test schema_contract_openid_token_tests -- --test-threads=1`

  * `media quota`：`cargo test --locked --test schema_contract_media_quota_tests -- --test-threads=1`

  * `receipts`：`cargo test --locked --test schema_contract_receipts_tests -- --test-threads=1`

* `db-migration-gate.yml` 当前已把 `room core`、`room summary`、`space`、`account_data`、`search`、`e2ee`、`thread`、`retention`、`invite restrictions`、`auth tokens`、`presence`、`openid token`、`media quota`、`receipts` 共 14 条 integration target 接到 `sqlx Migrate Run` 阻断链路中。

* 为避免 gate 只验证 isolated schema 而漏掉 public schema 漂移，`database_integrity_tests` 已补充：

  * `test_verification_requests_pending_index_survives_full_migration_chain`

  * `test_audit_critical_indexes_exist`

  * `test_audit_critical_constraints_exist`

  * `test_public_schema_contract_repairs_apply_cleanly`

* 进一步收口后，`db-migration-gate.yml` 已把以下 5 条 DB integrity 测试接入同一阻断链路：

  * `test_audit_critical_indexes_exist`

  * `test_audit_critical_constraints_exist`

  * `test_verification_requests_pending_index_survives_full_migration_chain`

  * `test_public_schema_contract_repairs_apply_cleanly`

  * `test_orphan_data_diagnostics_query_executes`

* 上述 DB 级确认依赖 `DatabaseInitService` 严格迁移初始化，并覆盖 `verification_requests` 索引回归、public schema orphan room 引用清理与 room summary / retention 外键恢复。

* unified schema 之后的关键补偿迁移现以 `scripts/ci/critical_migrations.txt` 为准；`20260406000001-00006` 已纳入该清单，确保 unified apply、expected baseline、sqlx forward-only source 与 migration gate 口径一致。

## 6. 2026-04-06 交付摘要

* 迁移修复：

  * 恢复 `verification_requests` 关键索引

  * 恢复 schema contract 关键外键

  * 恢复 public schema 作用域外键检查

  * 清理 room-derived orphan rows 以解除 FK 恢复阻塞

* 测试修复：

  * `database_integrity_tests` 不再依赖手工导出 `TEST_DATABASE_URL`

  * orphan data 诊断升级为按表输出 `count + samples`

  * 新增 `test_orphan_data_diagnostics_query_executes`，防止诊断 SQL 只存在于失败路径

* CI 修复：

  * `sqlx Migrate Run` job 已把 DB integrity 检查转为真实阻断

  * critical increments 清单与迁移文档已同步更新

