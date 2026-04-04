# 数据库版本管理标准与规范

> 日期：2026-04-04  
> 版本：v1.0  
> 状态：正式发布  
> 适用范围：synapse-rust 项目所有数据库变更

---

## 一、概述

### 1.1 目的

本文档定义 synapse-rust 项目数据库版本管理的标准流程、命名规范、编写规范和审查标准，确保数据库变更的安全性、可追溯性和可维护性。

### 1.2 适用范围

- 所有数据库 schema 变更
- 所有数据迁移脚本
- 所有索引和约束变更
- 所有性能优化相关的数据库操作

### 1.3 核心原则

1. **幂等性**：所有迁移必须可重复执行
2. **可回滚**：所有迁移必须有对应的回滚脚本
3. **向前兼容**：新版本必须兼容旧版本数据
4. **性能优先**：避免长时间锁表和阻塞
5. **文档完整**：变更必须有清晰的说明和测试证据

---

## 二、迁移命名规范

### 2.1 标准格式

#### 格式 A：时间戳格式（当前使用）

```
YYYYMMDDHHMMSS_description.sql
YYYYMMDDHHMMSS_description.undo.sql
```

**示例**：
```
20260404120000_add_user_preferences.sql
20260404120000_add_user_preferences.undo.sql
```

**适用场景**：
- 常规功能迁移
- 性能优化
- Schema 修复

#### 格式 B：版本化格式（推荐用于新迁移）

```
V{YYMMDD}_{seq}__{ticket}_{description}.sql
V{YYMMDD}_{seq}__{ticket}_{description}.undo.sql
```

**示例**：
```
V260404_001__DB-123__add_user_preferences.sql
V260404_001__DB-123__add_user_preferences.undo.sql
```

**适用场景**：
- 重大功能变更
- 需要跟踪工单的变更
- 需要版本控制的变更

### 2.2 特殊文件命名

#### 基线文件

```
00000000_unified_schema_v{major}.sql
```

**示例**：`00000000_unified_schema_v6.sql`

#### 综合迁移文件

```
99999999_unified_incremental_migration.sql
```

#### 性能优化文件

```
YYYYMMDD_p{priority}_optimization.sql
```

**示例**：
- `20260328_p1_indexes.sql` - P1 优先级索引优化
- `20260329_p2_optimization.sql` - P2 优先级优化

### 2.3 描述命名规范

**动词选择**：

| 操作 | 动词 | 示例 |
|------|------|------|
| 创建新表 | add, create | add_user_preferences |
| 修改表结构 | alter, modify | alter_users_add_status |
| 删除表/字段 | drop, remove | remove_deprecated_columns |
| 重命名 | rename | rename_created_at_to_created_ts |
| 修复问题 | fix | fix_missing_indexes |
| 对齐 schema | align | align_thread_schema |
| 性能优化 | optimize | optimize_events_indexes |

**描述规则**：
- 使用小写字母和下划线
- 简洁明了，不超过 50 字符
- 描述变更内容，不描述原因
- 避免使用缩写（除非是通用缩写）

---

## 三、迁移脚本编写规范

### 3.1 文件结构

```sql
-- ============================================================================
-- 迁移标题
-- 日期：YYYY-MM-DD
-- 工单：TICKET-123
-- 说明：详细说明变更内容和原因
-- 影响：描述对现有系统的影响
-- 回滚：说明回滚策略
-- ============================================================================

SET TIME ZONE 'UTC';

-- 设置超时（可选，用于大表操作）
SET statement_timeout = '30s';

-- ============================================================================
-- Part 1: 表结构变更
-- ============================================================================

-- 创建新表
CREATE TABLE IF NOT EXISTS new_table (
    id BIGSERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT
);

-- 修改现有表
ALTER TABLE existing_table 
ADD COLUMN IF NOT EXISTS new_column TEXT;

-- ============================================================================
-- Part 2: 索引创建
-- ============================================================================

-- 创建索引（生产环境必须使用 CONCURRENTLY）
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_new_table_name 
ON new_table(name);

-- ============================================================================
-- Part 3: 数据迁移（如果需要）
-- ============================================================================

-- 批量更新数据
UPDATE existing_table 
SET new_column = 'default_value'
WHERE new_column IS NULL;

-- ============================================================================
-- Part 4: 约束添加
-- ============================================================================

-- 添加外键约束
ALTER TABLE new_table 
ADD CONSTRAINT fk_new_table_user 
FOREIGN KEY (user_id) REFERENCES users(user_id) 
ON DELETE CASCADE;

-- ============================================================================
-- 验证
-- ============================================================================

-- 验证表存在
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.tables 
        WHERE table_name = 'new_table'
    ) THEN
        RAISE EXCEPTION 'Table new_table was not created';
    END IF;
END $$;
```

### 3.2 幂等性规范

**必须使用的幂等性语法**：

```sql
-- 表创建
CREATE TABLE IF NOT EXISTS table_name (...);

-- 索引创建
CREATE INDEX IF NOT EXISTS idx_name ON table(column);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_name ON table(column);

-- 列添加
ALTER TABLE table_name ADD COLUMN IF NOT EXISTS column_name TYPE;

-- 约束添加（需要先检查）
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'constraint_name'
    ) THEN
        ALTER TABLE table_name ADD CONSTRAINT constraint_name ...;
    END IF;
END $$;

-- 列删除（需要先检查）
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'table_name' AND column_name = 'column_name'
    ) THEN
        ALTER TABLE table_name DROP COLUMN column_name;
    END IF;
END $$;
```

### 3.3 性能优化规范

#### 规则 1：索引创建必须使用 CONCURRENTLY

```sql
-- ✅ 正确 - 不锁表
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_events_room_time 
ON events(room_id, origin_server_ts DESC);

-- ❌ 错误 - 会锁表
CREATE INDEX idx_events_room_time 
ON events(room_id, origin_server_ts DESC);
```

**例外**：
- 新创建的空表可以不使用 CONCURRENTLY
- 测试环境可以不使用 CONCURRENTLY

#### 规则 2：大表操作必须设置超时

```sql
-- 设置语句超时
SET statement_timeout = '30s';

-- 执行操作
ALTER TABLE large_table ADD COLUMN new_column TEXT;

-- 重置超时
RESET statement_timeout;
```

#### 规则 3：批量数据更新必须分批

```sql
-- ❌ 错误 - 一次更新所有数据
UPDATE large_table SET column = 'value';

-- ✅ 正确 - 分批更新
DO $$
DECLARE
    batch_size INT := 10000;
    affected_rows INT;
BEGIN
    LOOP
        UPDATE large_table 
        SET column = 'value'
        WHERE id IN (
            SELECT id FROM large_table 
            WHERE column IS NULL 
            LIMIT batch_size
        );
        
        GET DIAGNOSTICS affected_rows = ROW_COUNT;
        EXIT WHEN affected_rows = 0;
        
        RAISE NOTICE 'Updated % rows', affected_rows;
        COMMIT;
    END LOOP;
END $$;
```

#### 规则 4：避免全表扫描

```sql
-- ❌ 错误 - 全表扫描
ALTER TABLE large_table ADD COLUMN new_column TEXT DEFAULT 'value';

-- ✅ 正确 - 先添加列，再批量更新
ALTER TABLE large_table ADD COLUMN new_column TEXT;
-- 然后分批更新（见规则 3）
```

### 3.4 事务控制规范

#### 规则 1：DDL 操作使用事务

```sql
BEGIN;
    CREATE TABLE new_table (...);
    CREATE INDEX idx_new_table ON new_table(column);
    ALTER TABLE existing_table ADD COLUMN new_column TEXT;
COMMIT;
```

#### 规则 2：CONCURRENTLY 操作不能在事务中

```sql
-- ❌ 错误
BEGIN;
    CREATE INDEX CONCURRENTLY idx_name ON table(column);
COMMIT;

-- ✅ 正确
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_name ON table(column);
```

#### 规则 3：大数据量操作避免长事务

```sql
-- 分批提交，避免长事务
DO $$
BEGIN
    FOR i IN 1..100 LOOP
        UPDATE table SET column = value WHERE id BETWEEN i*1000 AND (i+1)*1000;
        COMMIT;
    END LOOP;
END $$;
```

### 3.5 回滚脚本规范

每个迁移必须有对应的回滚脚本（.undo.sql 或 .rollback.sql）：

```sql
-- ============================================================================
-- 回滚脚本：add_user_preferences
-- 日期：YYYY-MM-DD
-- 说明：回滚 add_user_preferences 迁移
-- 警告：此操作将删除 user_preferences 表及其所有数据
-- ============================================================================

SET TIME ZONE 'UTC';

-- ============================================================================
-- Part 1: 删除约束
-- ============================================================================

ALTER TABLE user_preferences 
DROP CONSTRAINT IF EXISTS fk_user_preferences_user;

-- ============================================================================
-- Part 2: 删除索引
-- ============================================================================

DROP INDEX IF EXISTS idx_user_preferences_user_id;

-- ============================================================================
-- Part 3: 删除表
-- ============================================================================

DROP TABLE IF EXISTS user_preferences;

-- ============================================================================
-- 验证
-- ============================================================================

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.tables 
        WHERE table_name = 'user_preferences'
    ) THEN
        RAISE EXCEPTION 'Table user_preferences still exists';
    END IF;
END $$;
```

---

## 四、迁移审查标准

### 4.1 代码审查清单

提交迁移脚本前必须检查：

#### 基础检查

- [ ] 文件命名符合规范
- [ ] 包含完整的文件头注释
- [ ] 使用 `SET TIME ZONE 'UTC'`
- [ ] 所有操作使用幂等性语法
- [ ] 包含对应的 undo/rollback 脚本

#### 性能检查

- [ ] 索引创建使用 `CONCURRENTLY`（生产环境）
- [ ] 大表操作设置超时
- [ ] 批量数据更新使用分批处理
- [ ] 避免全表扫描操作

#### 安全检查

- [ ] 不包含敏感数据
- [ ] 不删除重要数据（或有明确警告）
- [ ] 外键约束使用适当的 ON DELETE 策略
- [ ] 数据类型选择合理

#### 兼容性检查

- [ ] 向前兼容旧版本数据
- [ ] 不破坏现有功能
- [ ] 考虑了并发执行的情况

#### 测试检查

- [ ] 在空数据库测试通过
- [ ] 在现有数据库测试通过
- [ ] 回滚脚本测试通过
- [ ] 性能测试通过（如果是大表操作）

### 4.2 审查流程

```
开发者 → 自检 → 提交 PR → 自动化测试 → DBA 审查 → 批准 → 合并
```

#### 阶段 1：开发者自检

- 运行本地测试
- 检查代码规范
- 编写测试用例

#### 阶段 2：自动化测试

- CI 执行迁移测试
- Schema 验证
- 性能基准测试

#### 阶段 3：DBA 审查

- 审查迁移脚本
- 评估性能影响
- 确认回滚策略

#### 阶段 4：批准与合并

- 至少 1 个 DBA 批准
- 所有测试通过
- 文档更新完整

---

## 五、迁移执行流程

### 5.1 开发环境

```bash
# 1. 创建迁移
./scripts/create_migration.sh "add_user_preferences"

# 2. 编写迁移 SQL
vim migrations/20260404120000_add_user_preferences.sql
vim migrations/20260404120000_add_user_preferences.undo.sql

# 3. 本地测试
./scripts/test_migration.sh migrations/20260404120000_add_user_preferences.sql

# 4. 执行迁移
./scripts/db_migrate.sh migrate

# 5. 验证
./scripts/db_migrate.sh validate

# 6. 测试回滚
./scripts/db_migrate.sh rollback 20260404120000
```

### 5.2 测试环境

```bash
# 1. 部署代码
git pull origin develop

# 2. 备份数据库
./scripts/backup_database.sh

# 3. 执行迁移
./scripts/db_migrate.sh migrate

# 4. 运行测试
./scripts/run_integration_tests.sh

# 5. 验证功能
./scripts/smoke_test.sh
```

### 5.3 预发布环境

```bash
# 1. 创建备份
./scripts/backup_database.sh --full

# 2. 干运行（检查但不执行）
./scripts/db_migrate.sh migrate --dry-run

# 3. 执行迁移
./scripts/db_migrate.sh migrate

# 4. 验证
./scripts/db_migrate.sh validate
./scripts/check_field_consistency.sql

# 5. 性能测试
./scripts/performance_test.sh

# 6. 监控
./scripts/monitor_database.sh
```

### 5.4 生产环境

```bash
# 1. 维护窗口通知
./scripts/notify_maintenance.sh

# 2. 完整备份
./scripts/backup_database.sh --full --verify

# 3. 准备回滚脚本
./scripts/prepare_rollback.sh

# 4. 执行迁移（只读模式）
./scripts/db_migrate.sh migrate --read-only-check

# 5. 执行迁移（实际执行）
./scripts/db_migrate.sh migrate --production

# 6. 验证
./scripts/db_migrate.sh validate
./scripts/check_field_consistency.sql

# 7. 监控
./scripts/monitor_database.sh --alert

# 8. 恢复服务
./scripts/end_maintenance.sh
```

---

## 六、回滚策略

### 6.1 回滚触发条件

**立即回滚**：
- 迁移执行失败
- 数据完整性问题
- 严重性能下降（>50%）
- 应用功能完全不可用

**计划回滚**：
- 发现逻辑错误
- 性能下降（20-50%）
- 部分功能异常

### 6.2 回滚执行

```bash
# 1. 停止应用（如果需要）
systemctl stop synapse-rust

# 2. 执行回滚脚本
psql -d synapse -f migrations/20260404120000_add_user_preferences.undo.sql

# 3. 验证回滚
./scripts/db_migrate.sh validate

# 4. 检查数据完整性
./scripts/check_field_consistency.sql

# 5. 重启应用
systemctl start synapse-rust

# 6. 验证功能
./scripts/smoke_test.sh
```

### 6.3 紧急恢复

如果回滚失败，使用备份恢复：

```bash
# 1. 停止所有服务
systemctl stop synapse-rust

# 2. 从备份恢复
pg_restore -d synapse -c backup_20260404.dump

# 3. 验证数据
./scripts/validate_database.sh

# 4. 重启服务
systemctl start synapse-rust

# 5. 通知相关人员
./scripts/notify_recovery.sh
```

---

## 七、监控与告警

### 7.1 迁移执行监控

监控指标：
- 执行时间
- 锁等待时间
- 磁盘空间使用
- CPU 和内存使用
- 活跃连接数

### 7.2 告警阈值

| 指标 | 警告阈值 | 严重阈值 |
|------|---------|---------|
| 执行时间 | > 5 分钟 | > 10 分钟 |
| 锁等待 | > 10 秒 | > 30 秒 |
| 磁盘使用 | > 80% | > 90% |
| CPU 使用 | > 70% | > 90% |
| 连接数 | > 80% | > 95% |

### 7.3 监控脚本

```sql
-- 监控迁移执行
SELECT 
    pid,
    usename,
    application_name,
    state,
    query,
    now() - query_start as duration
FROM pg_stat_activity
WHERE query LIKE '%CREATE%' OR query LIKE '%ALTER%'
ORDER BY query_start;

-- 监控锁等待
SELECT 
    blocked_locks.pid AS blocked_pid,
    blocked_activity.usename AS blocked_user,
    blocking_locks.pid AS blocking_pid,
    blocking_activity.usename AS blocking_user,
    blocked_activity.query AS blocked_statement,
    blocking_activity.query AS blocking_statement
FROM pg_catalog.pg_locks blocked_locks
JOIN pg_catalog.pg_stat_activity blocked_activity ON blocked_activity.pid = blocked_locks.pid
JOIN pg_catalog.pg_locks blocking_locks 
    ON blocking_locks.locktype = blocked_locks.locktype
    AND blocking_locks.database IS NOT DISTINCT FROM blocked_locks.database
    AND blocking_locks.relation IS NOT DISTINCT FROM blocked_locks.relation
    AND blocking_locks.page IS NOT DISTINCT FROM blocked_locks.page
    AND blocking_locks.tuple IS NOT DISTINCT FROM blocked_locks.tuple
    AND blocking_locks.virtualxid IS NOT DISTINCT FROM blocked_locks.virtualxid
    AND blocking_locks.transactionid IS NOT DISTINCT FROM blocked_locks.transactionid
    AND blocking_locks.classid IS NOT DISTINCT FROM blocked_locks.classid
    AND blocking_locks.objid IS NOT DISTINCT FROM blocked_locks.objid
    AND blocking_locks.objsubid IS NOT DISTINCT FROM blocked_locks.objsubid
    AND blocking_locks.pid != blocked_locks.pid
JOIN pg_catalog.pg_stat_activity blocking_activity ON blocking_activity.pid = blocking_locks.pid
WHERE NOT blocked_locks.granted;
```

---

## 八、最佳实践

### 8.1 DO

✅ **使用幂等性语法**
```sql
CREATE TABLE IF NOT EXISTS ...
CREATE INDEX IF NOT EXISTS ...
ALTER TABLE ... ADD COLUMN IF NOT EXISTS ...
```

✅ **使用并发索引创建**
```sql
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_name ON table(column);
```

✅ **添加详细注释**
```sql
-- 说明：添加用户偏好设置表
-- 原因：支持用户自定义界面设置
-- 影响：无，新表不影响现有功能
```

✅ **分批处理大数据量**
```sql
-- 分批更新，每批 10000 行
UPDATE table SET column = value 
WHERE id IN (SELECT id FROM table WHERE condition LIMIT 10000);
```

✅ **设置超时**
```sql
SET statement_timeout = '30s';
```

✅ **验证执行结果**
```sql
DO $$
BEGIN
    IF NOT EXISTS (...) THEN
        RAISE EXCEPTION 'Validation failed';
    END IF;
END $$;
```

### 8.2 DON'T

❌ **不要在生产环境使用非并发索引**
```sql
-- 错误：会锁表
CREATE INDEX idx_name ON large_table(column);
```

❌ **不要一次性更新大量数据**
```sql
-- 错误：可能导致长时间锁表
UPDATE large_table SET column = value;
```

❌ **不要删除数据而不备份**
```sql
-- 错误：没有备份就删除
DROP TABLE important_table;
```

❌ **不要使用硬编码值**
```sql
-- 错误：硬编码
UPDATE users SET server_id = 'server1';

-- 正确：使用变量或配置
UPDATE users SET server_id = current_setting('app.server_id');
```

❌ **不要忽略错误处理**
```sql
-- 错误：没有错误处理
ALTER TABLE table ADD COLUMN column TYPE;

-- 正确：有错误处理
DO $$
BEGIN
    ALTER TABLE table ADD COLUMN IF NOT EXISTS column TYPE;
EXCEPTION
    WHEN duplicate_column THEN
        RAISE NOTICE 'Column already exists';
END $$;
```

---

## 九、工具与脚本

### 9.1 迁移创建工具

```bash
#!/bin/bash
# scripts/create_migration.sh

DESCRIPTION="$1"
TIMESTAMP=$(date +%Y%m%d%H%M%S)
FILENAME="${TIMESTAMP}_${DESCRIPTION}.sql"
UNDO_FILENAME="${TIMESTAMP}_${DESCRIPTION}.undo.sql"

# 创建迁移文件
cat > "migrations/$FILENAME" << EOF
-- ============================================================================
-- $DESCRIPTION
-- 日期：$(date +%Y-%m-%d)
-- 工单：TODO
-- 说明：TODO
-- ============================================================================

SET TIME ZONE 'UTC';

-- TODO: 添加迁移内容

EOF

# 创建回滚文件
cat > "migrations/$UNDO_FILENAME" << EOF
-- ============================================================================
-- 回滚：$DESCRIPTION
-- 日期：$(date +%Y-%m-%d)
-- 警告：TODO
-- ============================================================================

SET TIME ZONE 'UTC';

-- TODO: 添加回滚内容

EOF

echo "Created: migrations/$FILENAME"
echo "Created: migrations/$UNDO_FILENAME"
```

### 9.2 迁移测试工具

```bash
#!/bin/bash
# scripts/test_migration.sh

MIGRATION_FILE="$1"
TEST_DB="synapse_test_$(date +%s)"

echo "Testing migration: $MIGRATION_FILE"

# 创建测试数据库
createdb "$TEST_DB"

# 执行统一 schema
psql -d "$TEST_DB" -f migrations/00000000_unified_schema_v6.sql

# 执行迁移
psql -d "$TEST_DB" -f "$MIGRATION_FILE"

# 验证
psql -d "$TEST_DB" -f scripts/check_field_consistency.sql

# 测试回滚
UNDO_FILE="${MIGRATION_FILE%.sql}.undo.sql"
if [ -f "$UNDO_FILE" ]; then
    psql -d "$TEST_DB" -f "$UNDO_FILE"
    echo "Rollback test passed"
fi

# 清理
dropdb "$TEST_DB"

echo "Test completed successfully"
```

### 9.3 Schema 验证工具

```bash
#!/bin/bash
# scripts/validate_schema.sh

DB_NAME="${1:-synapse}"

echo "Validating schema for database: $DB_NAME"

# 检查表数量
TABLE_COUNT=$(psql -d "$DB_NAME" -t -c "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public'")
echo "Tables: $TABLE_COUNT"

# 检查索引数量
INDEX_COUNT=$(psql -d "$DB_NAME" -t -c "SELECT COUNT(*) FROM pg_indexes WHERE schemaname = 'public'")
echo "Indexes: $INDEX_COUNT"

# 检查外键数量
FK_COUNT=$(psql -d "$DB_NAME" -t -c "SELECT COUNT(*) FROM information_schema.table_constraints WHERE constraint_type = 'FOREIGN KEY'")
echo "Foreign Keys: $FK_COUNT"

# 运行一致性检查
psql -d "$DB_NAME" -f scripts/check_field_consistency.sql
```

---

## 十、常见问题

### Q1: 如何处理大表的列添加？

**A**: 使用以下策略：

```sql
-- 1. 添加列（不设置默认值）
ALTER TABLE large_table ADD COLUMN new_column TEXT;

-- 2. 分批更新数据
DO $$
DECLARE
    batch_size INT := 10000;
BEGIN
    LOOP
        UPDATE large_table 
        SET new_column = 'default_value'
        WHERE id IN (
            SELECT id FROM large_table 
            WHERE new_column IS NULL 
            LIMIT batch_size
        );
        EXIT WHEN NOT FOUND;
        COMMIT;
    END LOOP;
END $$;

-- 3. 添加 NOT NULL 约束（如果需要）
ALTER TABLE large_table ALTER COLUMN new_column SET NOT NULL;
```

### Q2: 如何安全地重命名列？

**A**: 使用以下步骤：

```sql
-- 1. 添加新列
ALTER TABLE table_name ADD COLUMN new_name TYPE;

-- 2. 复制数据
UPDATE table_name SET new_name = old_name;

-- 3. 更新应用代码使用新列名

-- 4. 删除旧列（在确认应用正常后）
ALTER TABLE table_name DROP COLUMN old_name;
```

### Q3: 如何处理迁移冲突？

**A**: 
1. 使用 `IF NOT EXISTS` / `IF EXISTS` 确保幂等性
2. 在迁移开始前检查状态
3. 使用事务确保原子性
4. 提供清晰的错误消息

### Q4: 如何测试迁移性能？

**A**:

```bash
# 1. 生成测试数据
./scripts/generate_test_data.sh 10000000  # 1000万行

# 2. 测试迁移执行时间
time psql -d test_db -f migrations/migration.sql

# 3. 监控资源使用
./scripts/monitor_migration.sh
```

---

## 十一、参考文档

- [PostgreSQL 官方文档](https://www.postgresql.org/docs/)
- [migrations/README.md](../migrations/README.md)
- [migrations/MIGRATION_INDEX.md](../migrations/MIGRATION_INDEX.md)
- [DATABASE_AUDIT_REPORT_2026-04-04.md](./DATABASE_AUDIT_REPORT_2026-04-04.md)
- [CONSOLIDATION_PLAN.md](../migrations/CONSOLIDATION_PLAN.md)

---

**文档版本**：v1.0  
**发布日期**：2026-04-04  
**维护团队**：数据库团队  
**下次审查**：2026-07-04
