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

| 阶段 | 目的 | 当前落点 | 失败阻断 |
| --- | --- | --- | --- |
| Gate 0 | 迁移布局审计 | `db-migration-gate.yml` + migration audit 脚本 | 阻断 |
| Gate 1 | 表覆盖率 | `check_schema_table_coverage.py` | 阻断 |
| Gate 2 | 列/索引/约束 contract | `check_schema_contract_coverage.py` | 阻断 |
| Gate 3 | 统一 schema apply + amcheck | workflow + `run_pg_amcheck.py` | 阻断 |
| Gate 4 | 关键 smoke test | thread / retention / room summary / db schema smoke | 阻断 |
| Gate 5 | 领域 contract test | Task 15 新增 contract 包 | 阻断 |
| Gate 6 | 逻辑校验与报告 | logical checksum / artifacts | 先非阻断 |

## 3. 失败输出规范

- `migration_id`
- `domain`
- `table`
- `column_or_index`
- `query_or_test_name`
- `failure_class`
- `reproduce_command`

## 4. 当前已知治理缺口

- workflow 中仍存在迁移文件名引用失配风险，需在实施阶段单独修正。
- `DatabaseInitService` 仍保留兼容入口，应继续维持默认关闭，避免第二迁移口径。
- 领域 contract test 目前还未像 schema smoke 那样形成完整目录结构，需要按本方案补齐。

## 5. CI 接线建议

- PR 默认阻断 Gate 0-5。
- Gate 6 先作为报告产物保留，后续按稳定度决定是否升级为阻断。
- 失败时上传：schema dump、contract diff、amcheck 输出、对应测试日志。
- 成功时只保留精简 summary，避免产物噪音。
