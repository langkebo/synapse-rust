# Synapse Rust 数据库迁移指南

## 概述

本文档提供 Synapse Rust 项目的数据库迁移管理指南，包括迁移文件组织、执行流程和最佳实践。

## 迁移文件组织

### 目录结构

```
migrations/
├── 00000000_unified_schema_v6.sql  # 统一 Schema 基线文件
├── DATABASE_FIELD_STANDARDS.md     # 字段命名规范
├── MIGRATION_HISTORY.md            # 迁移历史记录
├── MIGRATION_INDEX.md              # 迁移索引
├── README.md                       # 迁移说明
└── archive/                        # 归档的旧迁移文件
    ├── 00000000_unified_schema_v5.sql
    ├── 20260301_add_notifications_ts_column.sql
    └── ... (37个归档文件)
```

### 迁移文件命名规范

| 类型 | 命名格式 | 示例 |
|------|----------|------|
| Schema 基线 | `00000000_unified_schema_v{N}.sql` | `00000000_unified_schema_v6.sql` |
| 功能迁移 | `YYYYMMDDHHMMSS_description.sql` | `20260313000003_fix_sync_stream_id_type.sql` |
| 修复迁移 | `YYYYMMDDHHMMSS_fix_description.sql` | `20260308000001_fix_field_naming_inconsistencies.sql` |

## 迁移执行流程

### 1. 首次部署

```bash
# 1. 启动数据库容器
cd docker
docker compose -f docker-compose.local.yml up -d db

# 2. 等待数据库就绪
docker compose -f docker-compose.local.yml exec db pg_isready -U synapse

# 3. 运行迁移
cargo run --bin run_migrations

# 4. 验证 Schema
docker compose -f docker-compose.local.yml exec db psql -U synapse -d synapse_test -c "\dt"
```

### 2. 增量迁移

```bash
# 1. 创建新迁移文件
# 文件名格式: YYYYMMDDHHMMSS_description.sql

# 2. 测试迁移
cargo run --bin run_migrations -- --dry-run

# 3. 执行迁移
cargo run --bin run_migrations

# 4. 验证迁移结果
docker compose -f docker-compose.local.yml exec db psql -U synapse -d synapse_test -c "SELECT * FROM schema_migrations ORDER BY version DESC LIMIT 10;"
```

### 3. Docker 环境迁移

```bash
# 使用 Docker 入口脚本
docker compose -f docker-compose.local.yml up -d

# 查看迁移日志
docker compose -f docker-compose.local.yml logs -f synapse-rust | grep migration

# 手动执行迁移
docker compose -f docker-compose.local.yml exec synapse-rust /app/scripts/run-migrations.sh
```

## 迁移最佳实践

### 1. Schema 基线管理

**原则**: 使用单一 Schema 基线文件，避免分散的迁移文件

```sql
-- ✅ 推荐：统一 Schema 基线
-- 00000000_unified_schema_v6.sql
-- 包含所有表定义、索引、约束

-- ❌ 避免：多个分散的迁移文件
-- 20260301000001_add_table_a.sql
-- 20260301000002_add_table_b.sql
-- 20260301000003_add_column_c.sql
```

### 2. 字段命名规范

| 字段类型 | 命名规范 | 示例 |
|----------|----------|------|
| 布尔字段 | `is_` 或 `has_` 前缀 | `is_admin`, `is_revoked`, `has_avatar` |
| NOT NULL 时间戳 | `_ts` 后缀 | `created_ts`, `updated_ts` |
| 可空时间戳 | `_at` 后缀 | `expires_at`, `revoked_at` |
| 外键 | `{table}_id` 格式 | `user_id`, `room_id`, `device_id` |

### 3. SQL 查询规范

```rust
// ✅ 推荐：明确列出所有字段
sqlx::query_as!(
    User,
    r#"SELECT user_id, username, is_admin, is_guest, created_ts, updated_ts 
       FROM users WHERE user_id = $1"#,
    user_id
)

// ❌ 避免：使用 SELECT *
sqlx::query_as!(
    User,
    r#"SELECT * FROM users WHERE user_id = $1"#,
    user_id
)
```

### 4. 类型安全

```rust
// ✅ 推荐：使用正确的类型
pub struct AccessToken {
    pub id: i64,
    pub token: String,
    pub user_id: String,
    pub device_id: Option<String>,
    pub created_ts: i64,           // NOT NULL -> i64
    pub expires_at: Option<i64>,   // NULLABLE -> Option<i64>
    pub last_used_at: Option<i64>, // NULLABLE -> Option<i64>
    pub is_revoked: bool,          // BOOLEAN -> bool
    pub revoked_at: Option<i64>,   // NULLABLE -> Option<i64>
}

// ❌ 避免：类型不匹配
pub struct AccessToken {
    pub created_ts: Option<i64>,   // 错误：NOT NULL 字段不应为 Option
    pub is_revoked: Option<bool>,  // 错误：有默认值的布尔字段不应为 Option
}
```

## 常见问题解决

### 1. 迁移失败回滚

```bash
# 查看迁移历史
docker compose exec db psql -U synapse -d synapse_test -c "SELECT * FROM schema_migrations ORDER BY version DESC;"

# 手动回滚（谨慎操作）
docker compose exec db psql -U synapse -d synapse_test -c "DELETE FROM schema_migrations WHERE version = 'failed_version';"
```

### 2. 字段类型不匹配

**问题**: `sync_stream_id.id` 类型为 INT4，但代码期望 BIGINT

**解决方案**:
```sql
-- 创建修复迁移
-- 20260313000003_fix_sync_stream_id_type.sql
ALTER TABLE sync_stream_id ALTER COLUMN id TYPE BIGINT;
```

### 3. 缺失表/字段

**问题**: 编译时错误提示字段不存在

**解决方案**:
1. 检查数据库 Schema 是否最新
2. 运行 `cargo run --bin run_migrations`
3. 验证字段存在：`\d table_name`

## 迁移验证清单

### 执行前检查

- [ ] 备份数据库
- [ ] 检查迁移文件语法
- [ ] 确认迁移顺序正确
- [ ] 验证依赖关系

### 执行后验证

- [ ] 检查 `schema_migrations` 表
- [ ] 验证表结构 (`\d table_name`)
- [ ] 验证索引 (`\di`)
- [ ] 验证外键约束 (`\d+ table_name`)
- [ ] 运行单元测试 (`cargo test`)
- [ ] 运行集成测试

## 环境配置

### 开发环境

```bash
# .env
DATABASE_URL=postgres://synapse:synapse@localhost:5432/synapse_test
REDIS_URL=redis://localhost:6379
SQLX_OFFLINE=false
```

### 生产环境

```bash
# .env
DATABASE_URL=postgres://synapse:${DB_PASSWORD}@db:5432/synapse_test
REDIS_URL=redis://redis:6379
SQLX_OFFLINE=true
RUN_MIGRATIONS=true
VERIFY_SCHEMA=true
```

## 版本历史

| 版本 | 日期 | 变更说明 |
|------|------|----------|
| 1.0.0 | 2026-03-09 | 初始版本，创建迁移指南 |

## 相关文档

- [数据库字段命名规范](./DATABASE_FIELD_STANDARDS.md)
- [迁移历史记录](./MIGRATION_HISTORY.md)
- [API 参考文档](../docs/synapse-rust/api-reference.md)
