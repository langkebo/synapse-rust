# 07: 时间戳字段统一命名 `*_ts` → `*_ts_ms`

**What to build:** v11 baseline 中所有 BIGINT 时间戳字段统一加 `_ms` 后缀（`created_ts` → `created_ts_ms`），消除与 PG `created_at` 命名混淆。

**Blocked by:** 01-v11-baseline-scaffold

**Status:** ready-for-agent

- [ ] v11 baseline 中所有 `created_ts BIGINT` → `created_ts_ms BIGINT`
- [ ] 所有 `updated_ts BIGINT` → `updated_ts_ms BIGINT`
- [ ] 所有 `origin_server_ts BIGINT` → `origin_server_ts_ms BIGINT`
- [ ] 所有 `last_activity_ts BIGINT` → `last_activity_ts_ms BIGINT`
- [ ] 所有 `applied_ts BIGINT` → `applied_ts_ms BIGINT`
- [ ] 所有 `executed_at BIGINT` → `executed_at_ms BIGINT`
- [ ] 检查并同步 `v10.sql` 中所有类似字段（README 文档同步更新）
- [ ] 不改 Rust struct 命名（避免破坏现有 sqlx 宏）→ 留给 ticket 08

**关键决策**：
- 重部署窗口：可直接在 baseline 改名，无需 ALTER TABLE RENAME 过渡
- 字段类型保持 BIGINT（毫秒时间戳）
