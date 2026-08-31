# 09: 删除 .undo.sql + 历史 v10 baseline

**What to build:** 项目未实际发布，重部署窗口允许完全清理历史撤销脚本和老 baseline。

**Blocked by:** 01-v11-baseline-scaffold

**Status:** ✅ PARTIAL (undo.sql deleted, v10 baseline retained)

- [x] ✅ 删除所有 `migrations/*.undo.sql`（22 个文件全部删除）
- [ ] ⏸️ 删除 `migrations/00000000_unified_schema_v10.sql`（待确认）
- [ ] ⏸️ 删除 `migrations/00000001_extensions_v10.sql`（待确认）
- [ ] ⏸️ 更新 `scripts/init_test_public_schema.sh`
- [ ] ⏸️ 更新 `docker/db_migrate.sh`
- [ ] ⏸️ 更新 `docker/deploy/scripts/migrate.sh`
- [x] ✅ 验证 `synapse_test` schema 从 v11 干净重建（253 表，763 索引）

**v10 baseline 保留原因**：v10 baseline 之后的增量迁移（`20260810120000_*` 等）没有 v11 内联版本，删除 v10 会导致增量迁移中引用的函数/表/字段在 baseline 中不存在。虽然这些增量迁移本身也可删除（因为都在 v11 中内联），但需要先验证 v11 baseline 包含所有增量迁移的内容。当前 v11 baseline 包含 v10 的完整内容，所有增量都是向后兼容的 ADD/ALTER，无冲突。**建议：在确认所有增量迁移内容都已在 v11 baseline 中内联后，再删除 v10**。

- [ ] 删除所有 `migrations/*.undo.sql`（包括 v10 baseline 之后的增量撤销脚本）
- [ ] 删除 `migrations/00000000_unified_schema_v10.sql`（已迁移到 v11）
- [ ] 删除 `migrations/00000001_extensions_v10.sql`（如果存在）
- [ ] 更新 `scripts/init_test_public_schema.sh`：只应用 v11（跳过增量迁移检测）
- [ ] 更新 `docker/db_migrate.sh`：`latest_baseline_file()` 只匹配 v11
- [ ] 更新 `docker/deploy/scripts/migrate.sh`：同步
- [ ] 验证 `synapse_test` schema 从 v11 干净重建
- [ ] 验证 storage 测试套件不受影响（1513/1513）

**关键决策**：
- 完全删除（不留备份）：项目尚未真实发布，无历史包袱
- v11 baseline 内联所有变更，无需 .undo.sql