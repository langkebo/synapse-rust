# 数据库迁移系统文档

## 概述

本项目使用结构化的数据库迁移系统来管理数据库架构变更。迁移系统确保所有数据库修改都经过版本控制、测试和审核。

## 迁移文件命名规范

迁移文件使用时间戳命名格式：

```
YYYYMMDDHHMMSS_description.sql
```

示例：
- `20260311000001_add_space_members_table.sql`
- `20260311000002_fix_table_structures.sql`
- `20260311000006_add_e2ee_tables.sql`

## 迁移文件结构

每个迁移文件应包含以下部分：

```sql
-- ============================================================================
-- 迁移脚本: YYYYMMDDHHMMSS_description.sql
-- 创建日期: YYYY-MM-DD
-- 作者: 作者名
-- 描述: 简要描述此迁移的目的
-- 版本: vX.Y.Z
-- ============================================================================

-- 1. 数据库变更
CREATE TABLE IF NOT EXISTS example_table (
    id BIGSERIAL PRIMARY KEY,
    name TEXT NOT NULL
);

-- 2. 索引创建
CREATE INDEX IF NOT EXISTS idx_example_name ON example_table(name);

-- 3. 数据迁移（如需要）
-- INSERT INTO ...

-- 4. 验证
DO $$
BEGIN
    -- 验证逻辑
END $$;

-- 5. 记录迁移
INSERT INTO schema_migrations (version, name, applied_ts, description)
VALUES ('vX.Y.Z', 'migration_name', EXTRACT(EPOCH FROM NOW()) * 1000, 'Description')
ON CONFLICT (version) DO UPDATE SET applied_ts = EXCLUDED.applied_ts;
```

## 使用迁移管理工具

### 查看迁移状态

```bash
./scripts/migration_manager.sh status
```

输出示例：
```
Applied Migrations:
  ✓ 20260311000001
  ✓ 20260311000002

Pending Migrations:
  ○ 20260311000006_add_e2ee_tables.sql
```

### 应用所有待处理的迁移

```bash
./scripts/migration_manager.sh apply
```

### 验证迁移完整性

```bash
./scripts/migration_manager.sh verify
```

### 回滚迁移

```bash
./scripts/migration_manager.sh rollback <version>
```

## 迁移最佳实践

### 1. 幂等性

所有迁移脚本应该是幂等的，可以安全地多次执行：

```sql
-- 使用 IF NOT EXISTS
CREATE TABLE IF NOT EXISTS my_table (...);
CREATE INDEX IF NOT EXISTS idx_my_field ON my_table(field);

-- 使用 DO 块检查条件
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'my_table' AND column_name = 'new_column') THEN
        ALTER TABLE my_table ADD COLUMN new_column TEXT;
    END IF;
END $$;
```

### 2. 向后兼容

迁移应该保持向后兼容，避免破坏现有功能：

```sql
-- 添加可空列而不是非空列
ALTER TABLE my_table ADD COLUMN new_column TEXT;

-- 稍后通过数据迁移填充默认值
UPDATE my_table SET new_column = 'default' WHERE new_column IS NULL;

-- 最后添加非空约束
ALTER TABLE my_table ALTER COLUMN new_column SET NOT NULL;
```

### 3. 数据安全

始终备份重要数据：

```sql
-- 创建备份表
CREATE TABLE my_table_backup_YYYYMMDD AS SELECT * FROM my_table;

-- 执行破坏性操作
DROP TABLE my_table;
```

### 4. 性能考虑

大型表迁移应考虑性能影响：

```sql
-- 分批更新
DO $$
DECLARE
    batch_size INTEGER := 10000;
    updated_count INTEGER;
BEGIN
    LOOP
        UPDATE large_table SET field = 'value'
        WHERE field IS NULL
        LIMIT batch_size;
        
        GET DIAGNOSTICS updated_count = ROW_COUNT;
        EXIT WHEN updated_count = 0;
        
        COMMIT;
    END LOOP;
END $$;
```

## CI/CD 集成

迁移脚本会在以下场景自动执行：

1. **容器启动时**：通过 `docker-entrypoint.sh` 自动应用待处理的迁移
2. **部署前**：CI/CD 流水线中验证迁移完整性
3. **测试环境**：每次测试运行前重置数据库

### GitHub Actions 配置示例

```yaml
name: Database Migration

on:
  push:
    paths:
      - 'migrations/**'

jobs:
  migrate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Start PostgreSQL
        run: docker compose -f docker/docker-compose.local.yml up -d postgres-local
        
      - name: Wait for database
        run: sleep 10
        
      - name: Run migrations
        run: ./scripts/migration_manager.sh apply
        
      - name: Verify migrations
        run: ./scripts/migration_manager.sh verify
```

## 故障排除

### 迁移失败

如果迁移失败：

1. 检查错误日志
2. 手动修复数据库状态
3. 从 `schema_migrations` 表中删除失败的迁移记录
4. 重新应用迁移

```sql
-- 删除失败的迁移记录
DELETE FROM schema_migrations WHERE version = 'vX.Y.Z';

-- 手动修复后重新应用
```

### 版本冲突

如果多个开发者创建了相同时间戳的迁移：

1. 重命名迁移文件以使用不同的时间戳
2. 更新迁移文件中的版本号
3. 重新应用迁移

## 迁移历史

| 版本 | 日期 | 描述 |
|------|------|------|
| v6.0.1 | 2026-03-09 | 密码安全增强 |
| v6.0.2 | 2026-03-10 | 添加 E2EE 表 |
| v6.0.3 | 2026-03-10 | 字段标准化 |
| v6.0.4 | 2026-03-11 | 修复 API 测试问题 |
| v6.0.5 | 2026-03-11 | 修复媒体配额表 |
| v6.0.6 | 2026-03-11 | 添加 E2EE 完整表结构 |

## 相关文档

- [DATABASE_FIELD_STANDARDS.md](./DATABASE_FIELD_STANDARDS.md) - 数据库字段命名规范
- [MIGRATION_HISTORY.md](./MIGRATION_HISTORY.md) - 完整迁移历史
- [api-error.md](../docs/synapse-rust/api-error.md) - API 测试错误记录
