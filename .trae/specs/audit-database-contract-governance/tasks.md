# Tasks

- [x] Task 1: 统一迁移治理文档口径
  - [x] 以 `MIGRATION_INDEX.md` 的命名规则和目录模型作为唯一规范源
  - [x] 回写 `MIGRATION_GOVERNANCE.md` 中仍保留旧时间戳格式的描述
  - [x] 明确 `db-migration-gate.yml` 是唯一迁移治理门禁
  - [x] 标记暂不纳入最小实现的后续优化项
  - [x] 数据库契约问题已排查和修复

- [x] Task 2: 补齐 manifest 扫描覆盖
  - [x] `generate_migration_manifest.py` 已支持扫描根目录、`incremental/`、`hotfix/`
  - [x] `incremental/` 和 `hotfix/` 目录已存在
  - [x] `MANIFEST-template.txt` 继续可直接复用

- [x] Task 3: 补齐布局审计目录检查
  - [x] `audit_migration_layout.py` 已审计 `incremental/`、`hotfix/`
  - [x] 为新增目录中的正向脚本执行 rollback 配套检查

- [x] Task 4: 修正运维入口的错误列名
  - [x] `rollback_drill.py` - `installed_rank` → `COALESCE(applied_ts, 0) DESC, version DESC`
  - [x] `rollback_drill.py` - `installed_on` → `executed_at`
  - [x] `Makefile` - `installed_rank` → `COALESCE(applied_ts, 0) DESC, version DESC`
  - [x] `Makefile` - `installed_on` → `executed_at`
  - [x] `ROLLBACK_RUNBOOK.md` - 更新过时字段引用

- [x] Task 5: 固化最小治理闭环
  - [x] `db-migration-gate.yml` 定义为唯一迁移治理门禁
  - [x] `ci.yml` 保留通用测试与基础迁移初始化
  - [x] 硬编码 critical increment 列表的维护要求已记录

- [x] Task 6: 同步部署与环境说明口径
  - [x] 部署文档迁移入口收敛到 `docker/db_migrate.sh`
  - [x] 补充 `SYNAPSE_ENABLE_RUNTIME_DB_INIT` 的兼容定位与相关环境变量说明

## 已完成的数据库契约修复

| 修复项 | 类型 | 状态 |
|--------|------|------|
| `space_children.order` 列缺失 | SQL 迁移 | ✅ |
| `space_children.suggested` 列缺失 | SQL 迁移 | ✅ |
| `space_children.added_by` 列缺失 | SQL 迁移 | ✅ |
| `space_children.removed_ts` 列缺失 | SQL 迁移 | ✅ |
| `rooms.guest_access` 列缺失 | SQL 迁移 | ✅ |
| `search.rs` parent_id → parent_room_id | 代码修正 | ✅ |
| `access_tokens.token_hash` → token | 代码修正 | ✅ |

# Task Dependencies

- Task 2 依赖 Task 1 的统一规范源
- Task 3 依赖 Task 1 的目录模型与命名口径
- Task 4 依赖 Task 1 的运维口径与版本记录规则
- Task 5 依赖全部前置任务完成后形成最小闭环
