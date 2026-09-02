# 01: v11 baseline 脚手架

**What to build:** 建立 v11 baseline 文件骨架 + 重部署脚本。后续所有结构性优化都内联到 v11 CREATE TABLE 中。

**Blocked by:** None（可以立即开始）

**Status:** ready-for-agent

- [ ] 创建 `migrations/00000000_unified_schema_v11.sql` 空壳（注释说明本版本目标）
- [ ] 创建 `scripts/init_v11_database.sh`：DROP SCHEMA public CASCADE → 应用 v11.sql
- [ ] 修改 `docker/db_migrate.sh` 的 `latest_baseline_file()` 同时识别 v11
- [ ] 验证 v11 baseline 可独立应用（不依赖任何增量迁移）
- [ ] v11.sql 中至少包含 1 张测试表（如 `users`），确认能跑通

**关键决策**：
- v11 完整重写 schema 内容（不再依赖增量迁移）
- 删除 v10 baseline（重部署窗口不需要向后兼容）
- 保留 `00000000_unified_schema_v10.sql` 作为参考文档（read-only）
