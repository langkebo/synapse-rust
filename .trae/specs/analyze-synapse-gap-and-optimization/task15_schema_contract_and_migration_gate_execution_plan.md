# Task 15 - schema contract test 与 migration gate 执行文档

## 1. 目标

建立“迁移文件 -> 实际 schema -> SQL 查询 -> 集成测试”的闭环校验链，阻断 schema 漂移，并为 CI 接入 migration gate 提供可执行方案。

## 2. 范围

- 优先模块：E2EE、account_data、space、search
- 关键对象：表、列、索引、默认值、可空性、关键 SQL/SQLx 查询
- 执行链路：本地初始化、迁移执行、schema 校验、查询校验、集成验证
- 非目标：本任务不直接重写所有迁移脚本

## 3. 输入

- `tasks.md` 中 `Task 15 详细说明`
- `spec.md` 中“建立 schema 契约与迁移闭环”要求
- 项目数据库字段命名与类型规范
- 各能力域的关键查询与存储层实现

## 4. 输出

- schema 依赖总表
- schema contract test 设计
- migration gate 设计与 CI 接线建议

## 5. 执行阶段

### Phase 1: 依赖清单梳理

- 识别关键表、关键字段、关键索引
- 标记高风险查询和依赖默认值的写入逻辑
- 按能力域归类优先级

### Phase 2: contract test 设计

- 为每个能力域定义最小闭环测试
- 覆盖字段存在性、类型、默认值、可空性与查询语义
- 明确读写成功和错误分支的验证方式

### Phase 2.1: 当前已验证 P0 基线

- 当前 `tests/unit/schema_contract_p0_tests.rs` 已具备 9 条真实可执行样板：
  - `push_rules`
  - `account_data` / `room_account_data`
  - `search_index`
  - `room_summaries` / `room_summary_members`
  - `space_summaries` / `space_members` / `space_events` / `space_statistics`
  - `room_summary_state` / `room_summary_stats`
  - `room_summary_update_queue` / `room_children`
  - `space_children` / hierarchy
  - `room_summary queue processor`
- 其中 `room_summaries` 与 `space_summaries` 两条样板都已从“单表字段存在性”推进到“主表 + 关联表”层级，覆盖：
  - 主表主键、成员唯一约束、summary/member 相关索引
  - `get_summaries_for_user`、`get_heroes` 等真实查询契约
  - 创建 summary -> 添加成员 -> 更新 membership/is_hero -> 校验聚合计数和排序 -> 清理 的最小闭环
- `space_summaries` 样板当前补齐了：
  - `get_space_summary`、`get_space_events`、`get_space_statistics` 查询契约
  - `update_space_summary` 基于 `space_children` + `space_members` 刷新 `children_count` / `member_count`
  - 创建 space -> 添加成员 -> 添加 child -> 刷新 summary -> 校验 event/statistics -> 删除 child/member -> 再刷新 的最小闭环
- `room_summary_state` / `room_summary_stats` 样板当前补齐了：
  - `get_state`、`get_all_state`、`get_stats` 查询契约
  - `set_state` 与 `update_stats` 的 upsert 覆盖更新语义
  - state 插入 -> 同 key 更新 -> 全量读取 -> stats 插入 -> 覆盖更新 -> 再读取 的最小闭环
- `room_summary_update_queue` / `room_children` 样板当前补齐了：
  - `get_pending_updates` 的优先级排序契约
  - `queue_update`、`mark_update_processed`、`mark_update_failed` 的状态迁移语义
  - child relation 插入 -> 同键 upsert -> 最终 child 内容与唯一性校验 的最小闭环
- `space_children` / hierarchy 样板当前补齐了：
  - `get_space_children`、`get_child_spaces`、`get_recursive_hierarchy`、`get_space_hierarchy_paginated` 查询契约
  - root space -> child space -> leaf room 的递归深度、`is_space` 判定、分页输出与 `children_state` 构造语义
  - 顺带修正了 hierarchy 构造中“子关系误读为父关系”的存储层问题
- `room_summary queue processor` 样板当前补齐了：
  - `queue_update` 的优先级落库语义与 `process_pending_updates` 的消费驱动契约
  - processed/failed 分流、失败错误文案落库、state event 写入 `room_summary_state`
  - 普通 event 消费后 `room_summaries.last_event_id/last_event_ts` 更新、`m.room.message` 写入 `last_message_ts`、`limit` 分批消费与 failed 不重复消费 的最小闭环
- 对应目标测试当前已验证通过：
  - `cargo test --locked --test unit schema_contract_p0 -- --test-threads=1`
  - `cargo test --locked --test unit room_summary_storage -- --test-threads=1`

### Phase 3: migration gate 设计

- 定义 gate 顺序：创建库、执行迁移、检查 schema、执行关键查询、运行集成样例
- 设计阻断条件和输出格式
- 定义本地复现命令和排障路径

### Phase 4: CI 接线建议

- 规定在哪个阶段执行 gate
- 规定失败时保留哪些日志与产物
- 规定如何标记迁移问题、查询问题和映射问题
- 当前已新增独立 integration target：
  - `schema_contract_room_summary_tests`
  - `schema_contract_space_tests`
  - `schema_contract_account_data_tests`
  - `schema_contract_search_tests`
  - `schema_contract_e2ee_verification_tests`
- 五条 integration target 已接入 `db-migration-gate.yml` 的 `sqlx Migrate Run` 阻断链路，作为从 `unit` 聚合入口向能力域拆分的首批落地样板
- 后续执行面已补充 DB 级确认收口：
  - `database_integrity_tests` 自动解析可用测试数据库 URL，不再依赖手工导出 `TEST_DATABASE_URL`
  - 新增 `verification_requests` 完整迁移链索引回归断言
  - 新增 public schema contract repair 可应用性检查
  - 已通过补偿迁移清理 room summary / retention / deleted_events_index 的 orphan room 引用，并恢复对应外键约束

## 6. 技术约束

- 必须对齐项目现有字段标准和命名规范
- 契约测试必须基于真实数据库
- gate 需要同时覆盖 migration、schema、查询三层
- 输出信息必须能定位到具体迁移或具体查询

## 7. 里程碑与时限

- D1-D2：完成关键表与关键查询清单
- D3-D4：完成 contract test 和 gate 设计
- D5：完成 CI 接线与失败分类说明

## 8. 质量指标

- P0/P1 模块全部纳入
- 关键能力域覆盖率不低于 80%
- 每类失败都有可定位、可复现、可归因说明
- 至少形成 1 条端到端闭环样例

## 9. 测试与验证

- Schema 验证：检查表、列、索引、默认值、可空性
- 查询验证：执行关键 SQL/SQLx 并校验返回结构
- 集成验证：覆盖写入、读取、更新和错误分支
- 门禁验证：模拟字段缺失或命名漂移并确认 gate 阻断

## 10. 风险与缓解

- 风险：关键查询清单遗漏
- 缓解：按能力域负责人或模块目录双重复核

- 风险：gate 成本过高导致 CI 变慢
- 缓解：先定义最小阻断集，再扩展全量校验

- 风险：错误定位过于粗糙
- 缓解：按迁移、schema、查询、映射四类输出结果

## 11. 验收标准

- 已形成关键表/关键查询矩阵
- 已形成可执行的 schema contract test 基线
- 已形成 migration gate 设计和 CI 接线建议
- 已明确本地复现与问题定位路径
- 已将 `room summary`、`space`、`account_data`、`search`、`e2ee` 五个独立 integration target 接入 migration gate 实际阻断链路

## 12. 后续关联交付物

- `task15_schema_dependency_inventory.md`
- `task15_schema_contract_test_plan.md`
- `task15_migration_gate_design.md`

## 13. 后续扩展建议

- 下一优先级建议补 worker 调度、批量消费和重试退避策略，把 queue processor 从单次消费推进到持续运行语义。
- 若继续扩展 Space 域，可再补搜索可见性、分页 token 语义与 parent path 相关 contract。
- 若 CI 成本允许，可把 `room_summaries` contract 从 `unit` 入口进一步拆到独立 integration target，便于按能力域并行扩展。

## 14. 2026-04-06 收口摘要

### 已落地修复

- 新增 4 条补偿迁移：
  - `20260406000001_restore_verification_requests_pending_index.sql`
  - `20260406000002_restore_schema_contract_foreign_keys.sql`
  - `20260406000003_restore_public_schema_contract_foreign_keys.sql`
  - `20260406000004_cleanup_schema_contract_room_orphans.sql`
- `database_integrity_tests` 已补齐：
  - 自动解析测试数据库 URL
  - `DatabaseInitMode::Strict` 严格迁移初始化
  - public schema contract repair
  - orphan data 细粒度诊断（count + samples）
- `db-migration-gate.yml` 的 `sqlx Migrate Run` 已新增 5 条 DB 级阻断测试：
  - `test_audit_critical_indexes_exist`
  - `test_audit_critical_constraints_exist`
  - `test_verification_requests_pending_index_survives_full_migration_chain`
  - `test_public_schema_contract_repairs_apply_cleanly`
  - `test_orphan_data_diagnostics_query_executes`

### 治理口径对齐

- `scripts/ci/critical_migrations.txt` 已纳入 `20260406000001-00004`
- `migrations/README.md` 与 `migrations/MIGRATION_INDEX.md` 已同步声明：
  - unified schema 之后必须补跑 `critical_migrations.txt`
  - 影响 schema contract、public repair、关键索引或关键外键恢复的迁移，必须同步登记

### 本地复现命令

```bash
cargo test --locked --test integration database_integrity_tests::tests::test_audit_critical_indexes_exist -- --exact --nocapture
cargo test --locked --test integration database_integrity_tests::tests::test_audit_critical_constraints_exist -- --exact --nocapture
cargo test --locked --test integration database_integrity_tests::tests::test_verification_requests_pending_index_survives_full_migration_chain -- --exact --nocapture
cargo test --locked --test integration database_integrity_tests::tests::test_public_schema_contract_repairs_apply_cleanly -- --exact --nocapture
cargo test --locked --test integration database_integrity_tests::tests::test_orphan_data_diagnostics_query_executes -- --exact --nocapture
```

### 当前结论

- 本轮阻塞点已从“测试接线或 skip”收敛为“真实数据问题 + 迁移资产缺口”，并已完成修复。
- 当前 DB migration gate 已同时覆盖：
  - 迁移链索引回归
  - public schema repair 可应用性
  - 关键约束/索引存在性
  - room-derived orphan 诊断可执行性
