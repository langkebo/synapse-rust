- [x] 已明确本轮以"口径统一 + 少量脚本补齐"为最小实现边界
- [x] 已指定 `MIGRATION_INDEX.md` 为迁移命名与目录规范的唯一规范源
- [x] 已要求 `MIGRATION_GOVERNANCE.md` 与迁移索引文档使用同一命名规则和目录模型
- [x] 已要求 `generate_migration_manifest.py` 覆盖根目录、`incremental/` 与 `hotfix/`
- [x] 已要求 `audit_migration_layout.py` 对 `incremental/`、`hotfix/` 执行 rollback 配套检查
- [x] 已要求 `Makefile` 与 `rollback_drill.py` 改为查询 `schema_migrations` 实际列
- [x] 已明确 `db-migration-gate.yml` 是唯一迁移治理门禁且应设为 PR 必过检查
- [x] 已明确 `ci.yml` 仅保留通用测试与基础迁移初始化，不承担治理口径定义
- [x] 已记录 `db-migration-gate.yml` 中硬编码 critical increment 列表仍需人工维护
- [x] 已定义"可复用 / 待修改 / 暂不纳入最小实现"的交付清单范围
- [x] 已将自动生成关键增量列表、重写整套迁移流程、扩展第二套入口标记为后续优化
- [x] 已同步部署文档与环境变量说明，明确 `SYNAPSE_ENABLE_RUNTIME_DB_INIT` 的兼容定位

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
