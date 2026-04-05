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

1. `schema_contract_room_core`
2. `schema_contract_account_data`
3. `schema_contract_space`
4. `schema_contract_thread_retention_summary`
5. `schema_contract_e2ee_verification`
6. `schema_contract_search`

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

后续若需要按能力域拆分为独立“包”，再把 schema contract tests 迁移为可独立运行的 integration tests，建议落在：

```text
tests/integration/schema/
├── schema_contract_room_core_tests.rs
├── schema_contract_account_data_tests.rs
├── schema_contract_space_tests.rs
├── schema_contract_thread_retention_summary_tests.rs
├── schema_contract_e2ee_verification_tests.rs
└── schema_contract_search_tests.rs
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
- `Schema Table Coverage`：`scripts/check_schema_table_coverage.py`
- `Schema Contract Coverage`：`scripts/check_schema_contract_coverage.py --threshold 90`
- `Unified Schema Apply`：统一 schema apply + `scripts/run_pg_amcheck.py`
- `sqlx Migrate Run`：`sqlx migrate run` + `db_schema_smoke_tests`

Schema contract tests（本任务新增）建议接在 `sqlx Migrate Run` 之后作为阻断项，确保“迁移可跑”并不等于“查询仍可用”。
