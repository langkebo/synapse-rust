# 10: UNIQUE 约束与 UNIQUE INDEX 冗余清理

**What to build:** v11 baseline 中移除 `CONSTRAINT ... UNIQUE` 与 `CREATE UNIQUE INDEX` 重复定义（PG 中 UNIQUE 约束自动建索引，显式重复索引浪费存储）。

**Blocked by:** 01-v11-baseline-scaffold

**Status:** ✅ DONE (2026-08-31)

- [x] 在 v11 baseline 末尾追加 DO $$ 块，查找并删除与 CONSTRAINT 重复的显式 `CREATE UNIQUE INDEX`（索引名以 `uq_` 开头）
- [x] v11 baseline 重新应用 0 错误
- [x] v11 baseline 最终状态：253 表，763 索引，378 个显式索引

**实现方式**：简化的 DO $$ 块，只删除索引名以 `uq_` 开头的索引，检查是否在 `pg_constraint` 中有对应约束（通过 `conindid`）。

- [ ] 检查 v10 baseline，识别 `CONSTRAINT ... UNIQUE` 与 `CREATE UNIQUE INDEX` 重复定义（已知：`access_tokens.token_hash`、`refresh_tokens.token_hash`、`token_blacklist.token_hash` 等）
- [ ] v11 baseline 中：保留 `CONSTRAINT ... UNIQUE`（让 PG 自动建索引），删除显式 `CREATE UNIQUE INDEX`
- [ ] 同步删除 `CREATE INDEX ...` 与 `CREATE UNIQUE INDEX ...` 重复
- [ ] 验证 v11 重建后表上没有双重索引（`pg_indexes` 验证）
- [ ] storage 测试套件不因索引变更失败

**关键决策**：
- 保留 `CONSTRAINT`（约束语义清晰，索引自动建）
- 删除显式 `CREATE UNIQUE INDEX`（避免双重索引）