# 数据库 Schema 同步指南

## 概述

本文档记录了 synapse-rust 项目中数据库 Schema 与代码不同步的问题及解决方案。

## 常见问题

### 1. ban_ts 列缺失

**问题**: 创建房间时失败，提示 `column "ban_ts" does not exist`

**原因**: Docker 镜像中的数据库 Schema 与代码不同步

**解决方案**:

```bash
# 添加缺失的列
docker exec synapse-postgres-local psql -U synapse -d synapse_test -c "ALTER TABLE room_memberships ADD COLUMN ban_ts BIGINT;"
```

### 2. is_redacted 列问题

**问题**: events 表使用 `is_redacted` 但数据库只有 `redacted`

**解决方案**:

```bash
# 重命名列
docker exec synapse-postgres-local psql -U synapse -d synapse_test -c "ALTER TABLE events RENAME COLUMN redacted TO is_redacted;"
```

### 3. device_keys 列问题

**问题**: `created_ts` 应为 `added_ts`, `updated_at` 应为 `ts_updated_ms`

**解决方案**:

```bash
# 修复列名
docker exec synapse-postgres-local psql -U synapse -d synapse_test -c "ALTER TABLE device_keys RENAME COLUMN created_ts TO added_ts;"
docker exec synapse-postgres-local psql -U synapse -d synapse_test -c "ALTER TABLE device_keys RENAME COLUMN updated_at TO ts_updated_ms;"
```

## 快速修复脚本

运行以下命令自动修复所有 Schema 问题：

```bash
docker exec synapse-postgres-local psql -U synapse -d synapse_test -c "
-- 添加 room_memberships 缺失列
ALTER TABLE room_memberships ADD COLUMN IF NOT EXISTS ban_ts BIGINT;
ALTER TABLE room_memberships ADD COLUMN IF NOT EXISTS join_reason TEXT;

-- 修复 events 表
ALTER TABLE events ADD COLUMN IF NOT EXISTS is_redacted BOOLEAN DEFAULT FALSE;

-- 修复 device_keys 表  
ALTER TABLE device_keys ADD COLUMN IF NOT EXISTS added_ts BIGINT;
ALTER TABLE device_keys ADD COLUMN IF NOT EXISTS ts_updated_ms BIGINT;
"
```

## 验证修复

```bash
# 验证 room_memberships 表
docker exec synapse-postgres-local psql -U synapse -d synapse_test -c "SELECT column_name FROM information_schema.columns WHERE table_name = 'room_memberships' AND column_name IN ('ban_ts', 'join_reason');"

# 验证 events 表
docker exec synapse-postgres-local psql -U synapse -d synapse_test -c "SELECT column_name FROM information_schema.columns WHERE table_name = 'events' AND column_name = 'is_redacted';"

# 验证 device_keys 表
docker exec synapse-postgres-local psql -U synapse -d synapse_test -c "SELECT column_name FROM information_schema.columns WHERE table_name = 'device_keys' AND column_name IN ('added_ts', 'ts_updated_ms');"
```

## 预防措施

1. **始终在部署前运行迁移脚本**
2. **使用版本化的迁移文件** (`migrations/` 目录)
3. **在 CI/CD 中添加 Schema 验证步骤**
4. **定期同步 Docker 镜像与代码**

## 相关文件

- `migrations/20260307000001_fix_field_names_to_match_standards.sql`
- `migrations/20260307000003_fix_schema_sync.sql`
- `migrations/DATABASE_FIELD_STANDARDS.md`
