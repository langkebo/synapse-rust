# 迁移工具使用指南

> 日期：2026-04-04  
> 工具：`scripts/migration_manager.sh`  
> 版本：v2.0

---

## 一、概述

`migration_manager.sh` 是数据库迁移管理工具，提供迁移的完整生命周期管理：创建、测试、验证、应用、回滚。

### 功能列表

| 命令 | 功能 | 用途 |
|------|------|------|
| `status` | 查看迁移状态 | 查看已应用和待应用的迁移 |
| `apply` | 应用所有待处理迁移 | 执行数据库升级 |
| `verify` | 验证迁移完整性 | 检查关键表是否存在 |
| `rollback` | 回滚指定迁移 | 撤销迁移变更 |
| `create` | 创建新迁移 | 生成迁移文件模板 |
| `test` | 测试迁移 | 在隔离环境测试迁移 |
| `validate` | 验证迁移文件 | 检查迁移文件质量 |

---

## 二、基础命令

### 2.1 查看迁移状态

```bash
bash scripts/migration_manager.sh status
```

**输出示例**：
```
[INFO] Migration Status:

Applied Migrations:
  ✓ 00000000
  ✓ 20260328

Pending Migrations:
  ○ 20260404000001_consolidated_schema_alignment.sql
  ○ 20260404000002_consolidated_minor_features.sql
```

### 2.2 应用所有待处理迁移

```bash
bash scripts/migration_manager.sh apply
```

**功能**：
- 自动检测待应用的迁移
- 按版本号顺序应用
- 记录应用状态到 `schema_migrations` 表

**注意**：
- 确保数据库已备份
- 在生产环境建议使用维护窗口
- 应用前先在测试环境验证

### 2.3 验证迁移完整性

```bash
bash scripts/migration_manager.sh verify
```

**功能**：
- 检查关键表是否存在
- 统计已应用的迁移数量
- 验证数据库结构完整性

---

## 三、高级命令

### 3.1 创建新迁移

#### 基本用法

```bash
bash scripts/migration_manager.sh create <name> [description]
```

#### 示例

```bash
bash scripts/migration_manager.sh create add_user_settings "Add user settings table"
```

#### 输出

工具会自动生成：
1. **迁移文件**：`migrations/YYYYMMDDHHMMSS_add_user_settings.sql`
2. **回滚文件**：`migrations/undo/YYYYMMDDHHMMSS_add_user_settings_undo.sql`
3. **更新索引**：自动更新 `MIGRATION_INDEX.md`

#### 生成的迁移模板

```sql
-- Migration: add_user_settings
-- Version: 20260404120000
-- Description: Add user settings table
-- Created: 2026-04-04

-- ============================================================================
-- IMPORTANT: Use CREATE INDEX CONCURRENTLY for production safety
-- ============================================================================

BEGIN;

-- Add your migration SQL here
-- Example:
-- CREATE TABLE IF NOT EXISTS example_table (
--     id BIGSERIAL PRIMARY KEY,
--     name VARCHAR(255) NOT NULL,
--     created_ts BIGINT NOT NULL
-- );

-- CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_example_name
--     ON example_table(name);

COMMIT;
```

#### 生成的回滚模板

```sql
-- Undo Migration: add_user_settings
-- Version: 20260404120000
-- Description: Rollback for 20260404120000_add_user_settings
-- Created: 2026-04-04

BEGIN;

-- Add your rollback SQL here
-- Example:
-- DROP INDEX IF EXISTS idx_example_name;
-- DROP TABLE IF EXISTS example_table;

COMMIT;
```

#### 下一步

1. 编辑迁移文件，添加实际的 SQL
2. 编辑回滚文件，添加对应的回滚 SQL
3. 使用 `validate` 命令验证
4. 使用 `test` 命令测试

---

### 3.2 测试迁移

#### 基本用法

```bash
bash scripts/migration_manager.sh test <migration_file>
```

#### 示例

```bash
bash scripts/migration_manager.sh test 20260404120000_add_user_settings.sql
```

#### 测试流程

1. **创建测试数据库**：`synapse_migration_test`
2. **应用基线 Schema**：`00000000_unified_schema_v6.sql`
3. **应用目标迁移**：测试迁移文件
4. **验证迁移成功**：检查执行结果
5. **测试回滚**：应用对应的 undo 文件（如果存在）
6. **清理测试数据库**：删除测试数据库

#### 输出示例

```
[INFO] Testing migration: 20260404120000_add_user_settings.sql
[INFO] Creating test database: synapse_migration_test
[INFO] Applying baseline schema...
[SUCCESS] Baseline schema applied
[INFO] Applying migration...
[SUCCESS] Migration applied successfully
[INFO] Testing undo migration: 20260404120000_add_user_settings_undo.sql
[SUCCESS] Undo migration applied successfully
[INFO] Cleaning up test database...
[SUCCESS] Migration test completed
```

#### 注意事项

- 测试在隔离环境进行，不影响主数据库
- 需要 Docker 容器运行
- 测试会自动清理，不留痕迹

---

### 3.3 验证迁移文件

#### 基本用法

```bash
bash scripts/migration_manager.sh validate <migration_file>
```

#### 示例

```bash
bash scripts/migration_manager.sh validate 20260404120000_add_user_settings.sql
```

#### 验证项

1. **SQL 语法检查**
   - 使用 `psql --dry-run` 检查语法
   - 检测语法错误

2. **索引创建检查**
   - 检查是否使用 `CREATE INDEX CONCURRENTLY`
   - 警告非并发索引创建

3. **危险操作检查**
   - 检测 `DROP TABLE`、`TRUNCATE`、`DELETE FROM ... WHERE 1=1`
   - 警告潜在的数据丢失风险

4. **命名规范检查**
   - 验证文件名格式：`YYYYMMDDHHMMSS_name.sql`
   - 确保符合项目规范

#### 输出示例

```
[INFO] Validating migration: 20260404120000_add_user_settings.sql
[INFO] Checking SQL syntax...
  ✓ SQL syntax valid
[INFO] Checking index creation...
  ✓ All indexes use CONCURRENTLY or no indexes created
[INFO] Checking for dangerous operations...
  ✓ No dangerous operations detected
[INFO] Checking naming convention...
  ✓ Filename follows naming convention

[SUCCESS] Validation passed: no issues found
```

#### 验证失败示例

```
[INFO] Validating migration: bad_migration.sql
[INFO] Checking SQL syntax...
  ✗ SQL syntax error detected
[INFO] Checking index creation...
  ✗ Non-concurrent index creation found:
    CREATE INDEX idx_user_email ON users(email);
[WARNING] Consider using CREATE INDEX CONCURRENTLY for production safety
[INFO] Checking for dangerous operations...
  ✗ Dangerous operations found:
    DROP TABLE old_users;
[WARNING] Review these operations carefully
[INFO] Checking naming convention...
  ✗ Filename does not follow convention: YYYYMMDDHHMMSS_name.sql

[WARNING] Validation completed with 4 issue(s)
```

---

### 3.4 回滚迁移

#### 基本用法

```bash
bash scripts/migration_manager.sh rollback <version>
```

#### 示例

```bash
bash scripts/migration_manager.sh rollback 20260404120000
```

#### 注意

- 当前版本回滚功能需要手动执行 undo 文件
- 建议使用 `test` 命令先验证回滚脚本
- 生产环境回滚前务必备份

---

## 四、完整工作流程

### 4.1 创建新迁移的标准流程

```bash
# 1. 创建迁移
bash scripts/migration_manager.sh create add_feature "Add new feature"

# 2. 编辑迁移文件
vim migrations/YYYYMMDDHHMMSS_add_feature.sql

# 3. 编辑回滚文件
vim migrations/undo/YYYYMMDDHHMMSS_add_feature_undo.sql

# 4. 验证迁移
bash scripts/migration_manager.sh validate YYYYMMDDHHMMSS_add_feature.sql

# 5. 测试迁移
bash scripts/migration_manager.sh test YYYYMMDDHHMMSS_add_feature.sql

# 6. 提交代码
git add migrations/YYYYMMDDHHMMSS_add_feature.sql
git add migrations/undo/YYYYMMDDHHMMSS_add_feature_undo.sql
git add migrations/MIGRATION_INDEX.md
git commit -m "feat: add feature migration"

# 7. 在测试环境应用
bash scripts/migration_manager.sh apply

# 8. 验证测试环境
bash scripts/migration_manager.sh verify

# 9. 在生产环境应用（维护窗口）
bash docker/db_migrate.sh migrate
```

### 4.2 迁移失败恢复流程

```bash
# 1. 查看迁移状态
bash scripts/migration_manager.sh status

# 2. 查看错误日志
docker compose -f docker/docker-compose.yml logs db

# 3. 回滚失败的迁移
bash scripts/migration_manager.sh rollback <version>

# 4. 修复迁移文件
vim migrations/<version>_*.sql

# 5. 重新测试
bash scripts/migration_manager.sh test <version>_*.sql

# 6. 重新应用
bash scripts/migration_manager.sh apply
```

---

## 五、最佳实践

### 5.1 迁移编写规范

1. **使用事务**
   ```sql
   BEGIN;
   -- 迁移 SQL
   COMMIT;
   ```

2. **使用并发索引**
   ```sql
   CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_name
       ON table_name(column_name);
   ```

3. **使用幂等操作**
   ```sql
   CREATE TABLE IF NOT EXISTS ...
   ALTER TABLE ... ADD COLUMN IF NOT EXISTS ...
   ```

4. **避免危险操作**
   - 不使用 `DROP TABLE`（除非确实需要）
   - 不使用 `TRUNCATE`
   - 不使用 `DELETE FROM ... WHERE 1=1`

5. **添加注释**
   ```sql
   -- 说明迁移目的
   -- 说明业务背景
   -- 说明注意事项
   ```

### 5.2 测试规范

1. **本地测试**
   - 使用 `test` 命令在隔离环境测试
   - 验证迁移和回滚都能成功

2. **测试环境验证**
   - 在测试环境完整应用
   - 验证应用功能正常

3. **性能测试**
   - 对大表操作进行性能测试
   - 评估锁表时间

### 5.3 部署规范

1. **备份优先**
   ```bash
   bash scripts/backup_database.sh
   ```

2. **维护窗口**
   - 在低峰期执行
   - 预留足够时间

3. **监控告警**
   - 监控数据库连接数
   - 监控慢查询
   - 监控锁等待

4. **回滚准备**
   - 准备回滚脚本
   - 验证回滚脚本可用

---

## 六、故障排查

### 6.1 常见问题

#### 问题 1：容器未运行

**错误**：
```
[ERROR] Service db is not running
```

**解决**：
```bash
cd docker
docker compose up -d db
```

#### 问题 2：SQL 语法错误

**错误**：
```
✗ SQL syntax error detected
```

**解决**：
1. 检查 SQL 语法
2. 使用 `psql` 手动测试
3. 查看详细错误信息

#### 问题 3：迁移文件未找到

**错误**：
```
[ERROR] Migration file not found
```

**解决**：
1. 检查文件路径
2. 确认文件在 `migrations/` 目录
3. 使用完整文件名

#### 问题 4：测试数据库创建失败

**错误**：
```
ERROR: database "synapse_migration_test" already exists
```

**解决**：
```bash
docker compose -f docker/docker-compose.yml exec db \
    psql -U synapse -d postgres -c "DROP DATABASE IF EXISTS synapse_migration_test;"
```

---

## 七、参考资料

- [迁移索引](../../migrations/MIGRATION_INDEX.md)
- [迁移使用说明](../../migrations/README.md)
- [数据库审计报告](../db/DATABASE_AUDIT_REPORT_2026-04-04.md)
- [版本管理标准](../db/DATABASE_VERSION_MANAGEMENT_STANDARDS.md)

---

**文档版本**：v1.0  
**创建日期**：2026-04-04  
**维护者**：数据库团队
