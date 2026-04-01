# 数据库迁移治理与门禁落地方案

## 1. 目标

- 建立迁移脚本单一代码源
- 降低 unified schema 与增量迁移不一致风险
- 将数据库完整性检查纳入 Merge Request 门禁

## 2. 当前现状

| 类别 | 当前状态 | 风险 |
|---|---|---|
| unified schema | 存在 `00000000_unified_schema_v6.sql` | thread/retention/room summary/space 主链已收口，仍需持续约束后续增量同步 |
| 根目录迁移 | `migrations/*.sql` 仍保留 sqlx 历史链与兼容入口 | 历史迁移与治理目录并存，需避免口径分叉 |
| rollback 目录 | `migrations/rollback/` 已建立并为新增迁移配套回滚脚本 | 仍需持续补齐“新增迁移必有回滚/声明不可逆”的门禁 |
| incremental 目录 | `migrations/incremental/` 已建立并采用版本化治理命名 | 迁移治理按批次渐进推进，需与 manifest 和 layout audit 统一扫描 |
| hotfix 目录 | `migrations/hotfix/` 已建立并采用版本化治理命名 | 仍需约束 hotfix 在下一次常规发布前收敛 |
| archive 目录 | `migrations/archive/` 已建立 | 历史脚本归档需按批次推进，避免影响现有迁移链 |

## 3. 目标目录模型

```text
migrations/
  00000000_unified_schema_v6.sql
  20260330000010_add_audit_events.sql
  20260330000010_add_audit_events.undo.sql
  incremental/
    V{version}__{Jira编号}_{简短描述}.sql
  rollback/
    YYYYMMDDHHMMSS_description.rollback.sql
    V{version}__{Jira编号}_{简短描述}.down.sql
    V{version}__{Jira编号}_{简短描述}.undo.sql
  hotfix/
    V{version}__{Jira编号}_{简短描述}.sql
  archive/
    ...
```

## 4. 治理原则

1. 所有结构变更必须同时更新 unified schema 与增量迁移。
2. 迁移治理命名以 `MIGRATION_INDEX.md` 为唯一规范源，治理目录使用 `V{version}__...` 版本化格式。
3. 每个增量迁移必须配套 rollback 脚本或明确声明不可逆。
4. hotfix 在下一次常规发布前必须并入正式迁移。
5. 所有新增表必须补齐索引, 约束, 验证 SQL, 以及最小回归测试。
6. `db-migration-gate.yml` 是唯一迁移治理门禁，`ci.yml` 只承担通用构建、测试与基础迁移初始化。
7. Rust 运行时数据库初始化默认视为兼容入口，只有显式设置 `SYNAPSE_ENABLE_RUNTIME_DB_INIT=true` 时才允许执行。

## 5. 门禁清单

| 门禁 ID | 检查项 | 工具/方式 | 失败策略 |
|---|---|---|---|
| GATE-DB-001 | SQL 迁移语法有效 | `psql -v ON_ERROR_STOP=1 -f` | 阻止合并 |
| GATE-DB-002 | unified schema 可建库 | PostgreSQL 15 容器 | 阻止合并 |
| GATE-DB-003 | 增量迁移可连续执行 | `sqlx migrate run` | 阻止合并 |
| GATE-DB-004 | 代码引用表集合有 schema 对应 | 自定义扫描脚本 | 阻止合并 |
| GATE-DB-005 | 文档质量通过 | markdownlint + lychee + 拼写检查 | 阻止合并 |
| GATE-DB-006 | 数据库完整性通过 | `pg_amcheck` | 阻止合并 |
| GATE-DB-007 | 主从复制一致性通过 | 逻辑校验 / checksum 报告 | 阻止发布 |
| GATE-DB-008 | 外部证据已补齐 | 外部证据文件占位词扫描 | 阻止合并 |

当前落地状态:

- GATE-DB-004 已接入 `scripts/check_schema_table_coverage.py`
- GATE-DB-004/目录治理/manifest 校验统一归口到 `db-migration-gate.yml`
- GATE-DB-006 已接入 `scripts/run_pg_amcheck.py`
- GATE-DB-007 已接入 `scripts/generate_logical_checksum_report.py`
- GATE-DB-007 在 MR/CI 中为“报告框架”模式；主从对比通过 `db-replica-consistency.yml` 定时/手动执行（依赖 secrets 提供主从连接）
- GATE-DB-008 已接入 `scripts/check_external_evidence_complete.py`，要求提交 `docs/db/DIAGNOSIS_EXTERNAL_EVIDENCE_*.md`
- `db-migration-gate.yml` 当前已串联 retention / room summary / thread / db schema smoke tests，用于验证关键缺表与 unified schema 闭环
- `ci.yml` 保留 `sqlx migrate run` 作为通用测试前置，不承担迁移治理口径定义
- Rust 运行时初始化默认关闭，生产与 CI 应以 `docker/db_migrate.sh` 作为迁移执行入口

## 6. PostgreSQL 等价检查说明

用户原始要求中的 `mysqlcheck --all-databases --check-upgrade` 与 `pt-table-checksum` 属于 MySQL 工具链。本项目实际数据库为 PostgreSQL, 需使用以下等价方案:

| 原要求 | PostgreSQL 等价方案 | 备注 |
|---|---|---|
| mysqlcheck | pg_amcheck | 检查索引与系统目录一致性 |
| pt-table-checksum | 逻辑分片 checksum / 行数对账脚本 | 适配主从复制场景 |

官方文档:

- https://www.postgresql.org/docs/15/app-pgamcheck.html

现有脚本:

- `python3 scripts/run_pg_amcheck.py`
- `python3 scripts/generate_logical_checksum_report.py`
- 关键表清单: `scripts/logical_checksum_tables.txt`

## 7. 建议流水线阶段

```text
lint-docs
  -> build
  -> test
  -> db-schema-apply
  -> db-migrate-run
  -> db-amcheck
  -> db-logical-checksum
  -> release-approval
```

## 8. 执行步骤

| 步骤 | 动作 | 产出 |
|---|---|---|
| 1 | 清点 migrations 根目录脚本 | 迁移资产清单 |
| 2 | 标记重复/废弃/被 unified schema 吸收的文件 | 去重候选表 |
| 3 | 建立 incremental/rollback/hotfix 目录 | 标准目录结构 |
| 4 | 增加 schema 对应性扫描 | GATE-DB-004 |
| 5 | 增加 PostgreSQL 完整性检查 | GATE-DB-006 |
| 6 | 增加主从复制逻辑校验 | GATE-DB-007 |

## 9. 发布准入

以下条件全部满足后才允许进入生产发布审批:

- 所有 CI 工作流通过
- 所有数据库门禁通过
- MR 至少 2 名领域专家 approve
- 所有 review 对话 resolved
- 回滚脚本完成演练并留档
