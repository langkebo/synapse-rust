# 迁移操作指南

> 日期：2026-04-04  
> 版本：v1.0  
> 适用范围：生产环境、预发布环境、开发环境

---

## 一、概述

本指南提供数据库迁移的完整操作流程，包括创建、测试、验证、应用和回滚迁移脚本。

### 1.1 迁移管理原则

1. **单一事实来源**：所有 Schema 变更必须通过迁移脚本
2. **可回滚性**：每个迁移必须有对应的回滚脚本
3. **幂等性**：迁移可以安全地重复执行
4. **零停机**：生产环境迁移应支持在线执行
5. **可审计性**：所有迁移操作必须记录和可追溯

### 1.2 迁移工具链

| 工具 | 用途 | 环境 |
|------|------|------|
| `scripts/migration_manager.sh` | 迁移生命周期管理 | 开发/测试 |
| `docker/db_migrate.sh` | 容器化迁移执行 | 所有环境 |
| `scripts/validate_schema_all.sh` | Schema 验证 | 所有环境 |
| `sqlx migrate` | Rust 原生迁移工具 | 开发环境 |

---

## 二、迁移创建流程

### 2.1 创建新迁移

```bash
# 使用迁移管理器创建
bash scripts/migration_manager.sh create "add_user_preferences_table"

# 输出示例
Created migration: migrations/20260404142530_add_user_preferences_table.sql
Created rollback: migrations/20260404142530_add_user_preferences_table.undo.sql
```

生成的文件结构：
```
migrations/
├── 20260404142530_add_user_preferences_table.sql
└── 20260404142530_add_user_preferences_table.undo.sql
```

### 2.2 编写迁移脚本

#### 正向迁移 (*.sql)

```sql
-- Migration: Add user preferences table
-- Date: 2026-04-04
-- Author: Database Team
-- Jira: SYS-1234

-- Create table with IF NOT EXISTS for idempotency
CREATE TABLE IF NOT EXISTS user_preferences (
    user_id TEXT NOT NULL,
    preference_key TEXT NOT NULL,
    preference_value JSONB NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_user_preferences PRIMARY KEY (user_id, preference_key),
    CONSTRAINT fk_user_preferences_user FOREIGN KEY (user_id) 
        REFERENCES users(user_id) ON DELETE CASCADE
);

-- Create indexes with CONCURRENTLY for online execution
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_user_preferences_user 
    ON user_preferences(user_id);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_user_preferences_updated 
    ON user_preferences(updated_ts DESC);

-- Add comments for documentation
COMMENT ON TABLE user_preferences IS 'User-specific preference storage';
COMMENT ON COLUMN user_preferences.preference_value IS 'JSON-encoded preference data';
```

#### 回滚脚本 (*.undo.sql)

```sql
-- Rollback: Remove user preferences table
-- Date: 2026-04-04

-- Drop indexes first
DROP INDEX IF EXISTS idx_user_preferences_updated;
DROP INDEX IF EXISTS idx_user_preferences_user;

-- Drop table
DROP TABLE IF EXISTS user_preferences;
```

### 2.3 迁移脚本最佳实践

#### ✅ 推荐做法

1. **使用 IF NOT EXISTS / IF EXISTS**
   ```sql
   CREATE TABLE IF NOT EXISTS my_table (...);
   CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_name ON my_table(col);
   DROP TABLE IF EXISTS my_table;
   ```

2. **在线索引创建**
   ```sql
   -- 生产环境必须使用 CONCURRENTLY
   CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_name ON table_name(column);
   ```

3. **外键约束命名**
   ```sql
   CONSTRAINT fk_child_parent FOREIGN KEY (parent_id) 
       REFERENCES parent(id) ON DELETE CASCADE
   ```

4. **添加注释**
   ```sql
   COMMENT ON TABLE my_table IS 'Purpose and usage';
   COMMENT ON COLUMN my_table.col IS 'Column description';
   ```

5. **批量数据迁移分批处理**
   ```sql
   -- 避免长时间锁表
   DO $$
   DECLARE
       batch_size INT := 1000;
       processed INT := 0;
   BEGIN
       LOOP
           UPDATE my_table SET new_col = old_col
           WHERE id IN (
               SELECT id FROM my_table 
               WHERE new_col IS NULL 
               LIMIT batch_size
           );
           
           GET DIAGNOSTICS processed = ROW_COUNT;
           EXIT WHEN processed = 0;
           
           PERFORM pg_sleep(0.1); -- 避免过载
       END LOOP;
   END $$;
   ```

#### ❌ 避免做法

1. **不要在生产环境使用非并发索引创建**
   ```sql
   -- ❌ 会锁表
   CREATE INDEX idx_name ON large_table(column);
   
   -- ✅ 在线创建
   CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_name ON large_table(column);
   ```

2. **不要使用 ALTER TABLE 修改大表列类型**
   ```sql
   -- ❌ 会长时间锁表
   ALTER TABLE large_table ALTER COLUMN col TYPE BIGINT;
   
   -- ✅ 使用多步骤迁移
   -- Step 1: 添加新列
   ALTER TABLE large_table ADD COLUMN col_new BIGINT;
   -- Step 2: 分批迁移数据
   -- Step 3: 切换应用代码
   -- Step 4: 删除旧列
   ```

3. **不要在迁移中使用 TRUNCATE 或 DELETE 大量数据**
   ```sql
   -- ❌ 危险操作
   TRUNCATE TABLE important_data;
   
   -- ✅ 使用软删除或归档
   UPDATE important_data SET deleted = true WHERE condition;
   ```

4. **不要忘记回滚脚本**
   ```bash
   # 每个 .sql 必须有对应的 .undo.sql
   ```

---

## 三、迁移测试流程

### 3.1 本地测试

```bash
# 1. 在隔离测试数据库中测试
bash scripts/migration_manager.sh test migrations/20260404142530_add_user_preferences_table.sql

# 输出示例
Testing migration in isolated database: synapse_migration_test
✓ Migration applied successfully
✓ Rollback executed successfully
✓ Migration test passed
```

### 3.2 验证迁移

```bash
# 2. 验证 SQL 语法和安全性
bash scripts/migration_manager.sh validate migrations/20260404142530_add_user_preferences_table.sql

# 检查项：
# - SQL 语法正确性
# - 是否使用 CONCURRENTLY 创建索引
# - 是否包含危险操作（TRUNCATE, DROP DATABASE）
# - 是否有回滚脚本
```

### 3.3 Schema 验证

```bash
# 3. 运行完整 Schema 验证
bash scripts/validate_schema_all.sh

# 验证项：
# - 表覆盖检查
# - 合约覆盖检查（90% 阈值）
# - 迁移布局审计
# - 清单完整性
```

### 3.4 性能测试

```bash
# 4. 在大数据集上测试性能
# 生成测试数据
bash scripts/generate_benchmark_data.sh preset medium

# 应用迁移并测量时间
time bash docker/db_migrate.sh migrate

# 检查索引效率
psql $DATABASE_URL -c "EXPLAIN ANALYZE SELECT * FROM user_preferences WHERE user_id = '@test:example.com';"
```

---

## 四、迁移应用流程

### 4.1 开发环境

```bash
# 方式 1: 使用 Docker 脚本
bash docker/db_migrate.sh migrate

# 方式 2: 使用 sqlx
export DATABASE_URL="postgresql://synapse:synapse@localhost:5432/synapse_dev"
sqlx migrate run

# 方式 3: 直接应用
psql $DATABASE_URL -f migrations/20260404142530_add_user_preferences_table.sql
```

### 4.2 预发布环境

```bash
# 1. 备份数据库
pg_dump -h staging-db -U synapse -d synapse > backup_$(date +%Y%m%d_%H%M%S).sql

# 2. 应用迁移
export DATABASE_URL="postgresql://synapse:password@staging-db:5432/synapse"
bash docker/db_migrate.sh migrate

# 3. 验证 Schema
bash docker/db_migrate.sh validate

# 4. 运行集成测试
cargo test --test integration -- --test-threads=1

# 5. 验证应用启动
cargo run --release
```

### 4.3 生产环境

#### 准备阶段

```bash
# 1. 创建迁移清单
python3 scripts/generate_migration_manifest.py \
    --release v1.2.0 \
    --jira SYS-1234 \
    --owner db-team \
    --output artifacts/MANIFEST-prod-v1.2.0.txt

# 2. 审查清单
cat artifacts/MANIFEST-prod-v1.2.0.txt

# 3. 备份生产数据库
pg_dump -h prod-db -U synapse -d synapse -Fc > backup_prod_$(date +%Y%m%d_%H%M%S).dump

# 4. 验证备份
pg_restore --list backup_prod_*.dump | head -20
```

#### 执行阶段

```bash
# 5. 设置维护窗口（如需要）
# 通知用户，启用只读模式

# 6. 应用迁移
export DATABASE_URL="postgresql://synapse:password@prod-db:5432/synapse"
bash docker/db_migrate.sh migrate 2>&1 | tee migration_$(date +%Y%m%d_%H%M%S).log

# 7. 验证迁移
bash docker/db_migrate.sh validate

# 8. 运行 Schema 验证
bash scripts/validate_schema_all.sh

# 9. 检查数据完整性
python3 scripts/run_pg_amcheck.py
```

#### 验证阶段

```bash
# 10. 验证关键查询性能
psql $DATABASE_URL <<EOF
EXPLAIN ANALYZE SELECT * FROM user_preferences WHERE user_id = '@user:example.com';
EXPLAIN ANALYZE SELECT * FROM rooms WHERE is_public = true LIMIT 20;
EOF

# 11. 启动应用
systemctl start synapse-rust

# 12. 健康检查
curl http://localhost:8008/_matrix/client/versions

# 13. 监控日志
tail -f /var/log/synapse-rust/synapse.log

# 14. 关闭维护窗口
```

---

## 五、迁移回滚流程

### 5.1 何时回滚

立即回滚的情况：
- 迁移执行失败
- 应用启动失败
- 关键功能不可用
- 性能严重下降
- 数据完整性问题

### 5.2 回滚步骤

```bash
# 1. 停止应用
systemctl stop synapse-rust

# 2. 执行回滚脚本
psql $DATABASE_URL -f migrations/20260404142530_add_user_preferences_table.undo.sql

# 3. 验证回滚
psql $DATABASE_URL -c "\d user_preferences"
# 应该显示 "Did not find any relation named..."

# 4. 恢复应用到之前版本
git checkout v1.1.0
cargo build --release

# 5. 启动应用
systemctl start synapse-rust

# 6. 验证功能
curl http://localhost:8008/_matrix/client/versions
```

### 5.3 从备份恢复

如果回滚脚本失败：

```bash
# 1. 停止应用
systemctl stop synapse-rust

# 2. 删除当前数据库
psql -h prod-db -U postgres -c "DROP DATABASE synapse;"
psql -h prod-db -U postgres -c "CREATE DATABASE synapse OWNER synapse;"

# 3. 恢复备份
pg_restore -h prod-db -U synapse -d synapse backup_prod_20260404_120000.dump

# 4. 验证恢复
psql -h prod-db -U synapse -d synapse -c "SELECT COUNT(*) FROM users;"

# 5. 启动应用
systemctl start synapse-rust
```

---

## 六、常见场景操作

### 6.1 添加新表

```sql
-- 正向迁移
CREATE TABLE IF NOT EXISTS new_feature (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    data JSONB NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT fk_new_feature_user FOREIGN KEY (user_id) 
        REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_new_feature_user 
    ON new_feature(user_id);

-- 回滚
DROP INDEX IF EXISTS idx_new_feature_user;
DROP TABLE IF EXISTS new_feature;
```

### 6.2 添加列

```sql
-- 正向迁移
ALTER TABLE users ADD COLUMN IF NOT EXISTS avatar_url TEXT;
ALTER TABLE users ADD COLUMN IF NOT EXISTS bio TEXT;

-- 回滚
ALTER TABLE users DROP COLUMN IF EXISTS bio;
ALTER TABLE users DROP COLUMN IF EXISTS avatar_url;
```

### 6.3 修改列类型（大表）

```sql
-- Step 1: 添加新列
ALTER TABLE events ADD COLUMN origin_server_ts_new BIGINT;

-- Step 2: 分批迁移数据
DO $$
DECLARE
    batch_size INT := 10000;
    processed INT;
BEGIN
    LOOP
        UPDATE events 
        SET origin_server_ts_new = origin_server_ts::BIGINT
        WHERE origin_server_ts_new IS NULL
        AND id IN (
            SELECT id FROM events 
            WHERE origin_server_ts_new IS NULL 
            LIMIT batch_size
        );
        
        GET DIAGNOSTICS processed = ROW_COUNT;
        EXIT WHEN processed = 0;
        
        PERFORM pg_sleep(0.1);
    END LOOP;
END $$;

-- Step 3: 在应用代码中切换到新列
-- Step 4: 删除旧列（单独的迁移）
-- ALTER TABLE events DROP COLUMN origin_server_ts;
-- ALTER TABLE events RENAME COLUMN origin_server_ts_new TO origin_server_ts;
```

### 6.4 创建索引（大表）

```sql
-- 正向迁移 - 必须使用 CONCURRENTLY
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_events_room_time_covering
    ON events(room_id, origin_server_ts DESC) 
    INCLUDE (event_id, sender, event_type);

-- 回滚
DROP INDEX CONCURRENTLY IF EXISTS idx_events_room_time_covering;
```

### 6.5 数据迁移

```sql
-- 正向迁移 - 分批处理
DO $$
DECLARE
    batch_size INT := 5000;
    processed INT;
    total INT := 0;
BEGIN
    LOOP
        -- 更新一批数据
        UPDATE room_memberships 
        SET membership_state = 'join'
        WHERE membership_state IS NULL 
        AND membership = 'join'
        AND id IN (
            SELECT id FROM room_memberships 
            WHERE membership_state IS NULL 
            LIMIT batch_size
        );
        
        GET DIAGNOSTICS processed = ROW_COUNT;
        total := total + processed;
        
        EXIT WHEN processed = 0;
        
        -- 记录进度
        RAISE NOTICE 'Processed % rows, total: %', processed, total;
        
        -- 短暂休眠避免过载
        PERFORM pg_sleep(0.05);
    END LOOP;
    
    RAISE NOTICE 'Migration complete. Total rows: %', total;
END $$;

-- 回滚 - 恢复原始状态
UPDATE room_memberships 
SET membership_state = NULL 
WHERE membership_state = 'join';
```

---

## 七、故障排查

### 7.1 迁移执行失败

**症状**：
```
ERROR: relation "user_preferences" already exists
```

**原因**：迁移已部分执行

**解决**：
```bash
# 1. 检查当前状态
psql $DATABASE_URL -c "\d user_preferences"

# 2. 手动清理
psql $DATABASE_URL -f migrations/20260404142530_add_user_preferences_table.undo.sql

# 3. 重新执行
bash docker/db_migrate.sh migrate
```

### 7.2 索引创建超时

**症状**：
```
ERROR: canceling statement due to statement timeout
```

**原因**：大表索引创建时间过长

**解决**：
```sql
-- 增加超时时间
SET statement_timeout = '1h';
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_name ON large_table(column);
RESET statement_timeout;
```

### 7.3 外键约束冲突

**症状**：
```
ERROR: insert or update on table violates foreign key constraint
```

**原因**：数据不一致

**解决**：
```sql
-- 1. 找出孤立记录
SELECT child.* 
FROM child_table child
LEFT JOIN parent_table parent ON child.parent_id = parent.id
WHERE parent.id IS NULL;

-- 2. 清理或修复数据
DELETE FROM child_table 
WHERE parent_id NOT IN (SELECT id FROM parent_table);

-- 3. 重新添加约束
ALTER TABLE child_table 
ADD CONSTRAINT fk_child_parent 
FOREIGN KEY (parent_id) REFERENCES parent_table(id);
```

### 7.4 锁等待超时

**症状**：
```
ERROR: could not obtain lock on relation "events"
```

**原因**：表被其他事务锁定

**解决**：
```sql
-- 1. 查看锁信息
SELECT 
    pid, 
    usename, 
    application_name, 
    state, 
    query 
FROM pg_stat_activity 
WHERE datname = 'synapse' 
AND state != 'idle';

-- 2. 终止阻塞查询（谨慎）
SELECT pg_terminate_backend(pid) 
FROM pg_stat_activity 
WHERE pid = <blocking_pid>;

-- 3. 在维护窗口重试
```

---

## 八、监控和审计

### 8.1 迁移执行日志

```bash
# 记录所有迁移操作
bash docker/db_migrate.sh migrate 2>&1 | tee -a /var/log/synapse/migrations.log

# 日志应包含：
# - 时间戳
# - 执行的迁移文件
# - 执行结果
# - 错误信息（如有）
```

### 8.2 Schema 版本追踪

```sql
-- 查看已应用的迁移
SELECT * FROM _sqlx_migrations ORDER BY installed_on DESC LIMIT 10;

-- 查看迁移审计记录
SELECT * FROM migration_audit ORDER BY applied_at DESC LIMIT 10;
```

### 8.3 性能监控

```sql
-- 监控索引使用情况
SELECT 
    schemaname,
    tablename,
    indexname,
    idx_scan,
    idx_tup_read,
    idx_tup_fetch
FROM pg_stat_user_indexes
WHERE schemaname = 'public'
ORDER BY idx_scan DESC;

-- 监控表大小
SELECT 
    tablename,
    pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename)) AS size
FROM pg_tables
WHERE schemaname = 'public'
ORDER BY pg_total_relation_size(schemaname||'.'||tablename) DESC
LIMIT 20;
```

---

## 九、参考资料

- [迁移工具指南](MIGRATION_TOOLS_GUIDE.md)
- [Schema 验证指南](SCHEMA_VALIDATION_GUIDE.md)
- [性能优化指南](PERFORMANCE_OPTIMIZATION_GUIDE.md)
- [灾难恢复指南](DISASTER_RECOVERY_GUIDE.md)
- [P2 长期改进计划](../synapse-rust/P2_LONG_TERM_IMPROVEMENT_PLAN.md)

---

**文档版本**：v1.0  
**创建日期**：2026-04-04  
**维护者**：数据库团队  
**审核者**：运维团队
