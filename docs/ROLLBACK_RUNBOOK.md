# Database Migration Rollback Runbook

> 版本: v1.0
> 更新日期: 2026-03-29
> 目标环境: synapse-rust

---

## 1. 回滚策略概述

> 当前项目口径以 `docker/db_migrate.sh`、`migrations/rollback/`、`.undo.sql` / `.down.sql` 迁移资产为准，Flyway 仅作为兼容路径保留。

### 1.1 回滚时间目标

| 环境 | 回滚耗时目标 | 实际演练 |
|------|--------------|----------|
| Staging | < 3 min | 待验证 |
| Production | < 5 min | 待验证 |

### 1.2 回滚策略类型

| 类型 | 适用场景 | 工具 |
|------|----------|------|
| 迁移资产回滚 | 单个迁移脚本回滚 | `rollback/*.rollback.sql`、`.undo.sql`、`psql` |
| Flyway Undo | 兼容场景下的单个迁移回滚 | Flyway |
| 物理回滚 | 迁移资产不可用时的紧急回滚 | psql |
| 应用层回滚 | 数据修复而非结构变更 | Rust 代码 |

---

## 2. 回滚前检查

### 2.1 前置条件

- [ ] 确认需要回滚的迁移版本
- [ ] 确认回滚脚本存在
- [ ] 确认没有依赖该迁移的其他迁移
- [ ] 备份当前数据库

### 2.2 备份命令

```bash
# 完整数据库备份
pg_dump -h localhost -U synapse -d synapse -F custom -f backup_before_rollback_$(date +%Y%m%d_%H%M%S).dump

# 仅结构备份
pg_dump -h localhost -U synapse -d synapse -F plain -f schema_backup_$(date +%Y%m%d_%H%M%S).sql

# 仅数据备份（关键表）
pg_dump -h localhost -U synapse -d synapse -t room_summaries -t room_summary_members -F custom -f critical_data_backup.dump
```

---

## 3. 项目主回滚流程

### 3.1 检查迁移状态

```bash
# 查看当前版本
psql -h localhost -U synapse -d synapse -c "SELECT version, name, success, applied_ts, executed_at FROM schema_migrations ORDER BY COALESCE(applied_ts, FLOOR(EXTRACT(EPOCH FROM executed_at) * 1000)::BIGINT) DESC NULLS LAST, id DESC LIMIT 5;"

# 查看可用的回滚脚本
ls migrations/rollback/
find migrations -maxdepth 2 \( -name "*.undo.sql" -o -name "*.down.sql" \)
```

### 3.2 执行回滚

```bash
# 时间戳迁移回滚
psql -h localhost -U synapse -d synapse -v ON_ERROR_STOP=1 -f migrations/rollback/20260330000009_align_beacon_and_call_exceptions.rollback.sql

# 版本化迁移回滚
psql -h localhost -U synapse -d synapse -v ON_ERROR_STOP=1 -f migrations/V260330_001__MIG-XXX__add_missing_schema_tables.undo.sql
```

### 3.3 验证回滚

```bash
# 检查表结构
psql -h localhost -U synapse -d synapse -c "\d table_name"

# 检查索引
psql -h localhost -U synapse -d synapse -c "SELECT indexname FROM pg_indexes WHERE tablename = 'table_name';"

# 检查数据完整性
psql -h localhost -U synapse -d synapse -c "SELECT COUNT(*) FROM table_name;"
```

### 3.4 Flyway 兼容路径

```bash
# 仅在兼容场景下使用
flyway -configFiles=scripts/db/flyway.conf info
flyway -configFiles=scripts/db/flyway.conf undo -targetVersion=V260330_001
```

---

## 4. 紧急物理回滚流程

### 4.1 识别需要回滚的变更

```sql
-- 查看最近的 DDL 变更
SELECT * FROM schema_migrations
WHERE executed_at > NOW() - INTERVAL '1 hour'
ORDER BY COALESCE(applied_ts, 0) DESC, version DESC;
```

### 4.2 手动回滚脚本模板

```sql
-- VXXXXXXXX_XXXXXX_description.rollback.sql
-- 注意：必须是幂等的

BEGIN;

-- 检查是否存在（幂等性保证）
DO $$
BEGIN
    -- 回滚逻辑
END $$;

COMMIT;
```

### 4.3 索引回滚示例

```sql
-- 回滚添加的索引
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'idx_room_summary_room_id') THEN
        DROP INDEX IF EXISTS idx_room_summary_room_id;
        RAISE NOTICE 'Index idx_room_summary_room_id dropped successfully';
    ELSE
        RAISE NOTICE 'Index idx_room_summary_room_id does not exist, skipping';
    END IF;
END $$;
```

### 4.4 列回滚示例

```sql
-- 回滚添加的列
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'room_summaries' AND column_name = 'new_column'
    ) THEN
        ALTER TABLE room_summaries DROP COLUMN IF EXISTS new_column;
        RAISE NOTICE 'Column new_column dropped successfully';
    ELSE
        RAISE NOTICE 'Column new_column does not exist, skipping';
    END IF;
END $$;
```

### 4.5 表回滚示例

```sql
-- 回滚创建的表
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'new_table') THEN
        DROP TABLE IF EXISTS new_table CASCADE;
        RAISE NOTICE 'Table new_table dropped successfully';
    ELSE
        RAISE NOTICE 'Table new_table does not exist, skipping';
    END IF;
END $$;
```

---

## 5. 回滚验证清单

### 5.1 结构验证

- [ ] 表结构与预期一致
- [ ] 列类型与预期一致
- [ ] 索引与预期一致
- [ ] 外键约束完整

### 5.2 数据验证

- [ ] 数据行数一致
- [ ] 关键业务数据完整
- [ ] 关联数据一致

### 5.3 应用验证

- [ ] 应用启动正常
- [ ] 核心功能测试通过
- [ ] 无异常日志

---

## 6. 回滚后处理

### 6.1 更新文档

```markdown
## 回滚记录 YYYY-MM-DD

| 项目 | 内容 |
|------|------|
| 回滚版本 | VXXXXXXXX |
| 原因 | ... |
| 执行人 | ... |
| 耗时 | X 分钟 |
| 验证结果 | 通过 |
```

### 6.2 通知相关方

- [ ] 通知开发团队
- [ ] 通知 DBA
- [ ] 通知 SRE
- [ ] 更新故障报告（如有）

### 6.3 根因分析

- [ ] 分析迁移失败原因
- [ ] 更新迁移脚本
- [ ] 添加回归测试

---

## 7. 常见回滚场景

### 7.1 场景 1: 索引添加超时

**症状**: 添加索引超时或锁定表

**回滚**:
```sql
-- 检查索引是否部分创建
SELECT * FROM pg_indexes WHERE indexname = 'idx_slow_index';

-- 删除未完成的索引
DROP INDEX IF EXISTS idx_slow_index;
```

### 7.2 场景 2: 列类型变更失败

**症状**: ALTER TABLE 超时或数据丢失

**回滚**:
```sql
-- 检查当前列状态
SELECT column_name, data_type FROM information_schema.columns
WHERE table_name = 'target_table';

-- 如果需要，回滚到备份数据
BEGIN;
ALTER TABLE target_table ALTER COLUMN col TYPE original_type;
COMMIT;
```

### 7.3 场景 3: 新表创建失败

**症状**: 表创建一半，残留不完整表

**回滚**:
```sql
-- 删除不完整的表
DROP TABLE IF EXISTS incomplete_table CASCADE;

-- 检查是否有残留索引
DROP INDEX IF EXISTS idx_incomplete_table_col;
```

---

## 8. 紧急联系人

| 角色 | 职责 | 联系方式 |
|------|------|----------|
| DBA Lead | 数据库相关问题 | ... |
| SRE Lead | 基础设施问题 | ... |
| Backend Lead | 应用层问题 | ... |

---

## 9. 参考资料

| 文档 | 路径 |
|------|------|
| Flyway 文档 | https://flywaydb.org/documentation/ |
| PostgreSQL 回滚 | https://www.postgresql.org/docs/current/sql-rollback.html |
| 项目迁移索引 | migrations/MIGRATION_INDEX.md |
| 迁移治理文档 | docs/db/MIGRATION_GOVERNANCE.md |
