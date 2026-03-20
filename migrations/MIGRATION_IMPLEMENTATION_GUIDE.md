# 数据库迁移脚本合并实施指南

> **项目**: synapse-rust 数据库迁移脚本优化
> **版本**: v1.0.0
> **创建日期**: 2026-03-20
> **状态**: 待执行

---

## 一、执行前准备

### 1.1 环境检查清单

```bash
#!/bin/bash
# 环境检查脚本

echo "=== 环境检查 ==="

# 1. 检查 PostgreSQL 连接
psql -U synapse -d synapse -c "SELECT version();"

# 2. 检查当前迁移状态
psql -U synapse -d synapse -c "SELECT version, description, applied_at FROM schema_migrations ORDER BY applied_at DESC;"

# 3. 检查磁盘空间 (需要至少 10GB)
df -h /

# 4. 检查内存
free -h

# 5. 检查 PostgreSQL 配置
psql -U synapse -d synapse -c "SHOW max_connections;"
psql -U synapse -d synapse -c "SHOW work_mem;"

echo "=== 检查完成 ==="
```

### 1.2 备份清单

| 备份项 | 命令 | 验证命令 |
|--------|------|----------|
| 完整备份 | `pg_dump -U synapse -d synapse -F c -b -v -f backup_full_$(date +%Y%m%d).dump` | `pg_restore --list backup_full_*.dump \| wc -l` |
| Schema 备份 | `pg_dump -U synapse -d synapse --schema-only -b -v -f backup_schema_$(date +%Y%m%d).dump` | `pg_restore --list backup_schema_*.dump \| wc -l` |
| 关键表备份 | `pg_dump -U synapse -d synapse -t users -t devices -t access_tokens -F c -b -v -f backup_critical_*.dump` | `pg_restore --list backup_critical_*.dump \| grep TABLE` |

### 1.3 测试环境准备

```bash
#!/bin/bash
# 创建测试环境

# 1. 创建测试数据库
psql -U postgres -c "CREATE DATABASE synapse_test;"

# 2. 执行基础 schema
psql -U synapse -d synapse_test -f migrations/00000000_unified_schema_v6.sql

# 3. 执行综合迁移脚本
psql -U synapse -d synapse_test -f migrations/UNIFIED_MIGRATION_v1.sql

# 4. 验证
psql -U synapse -d synapse_test -c "SELECT COUNT(*) as table_count FROM information_schema.tables WHERE table_schema = 'public';"
```

---

## 二、综合迁移脚本结构

### 2.1 脚本结构模板

```sql
-- ============================================================================
-- Synapse Rust 综合迁移脚本 v1.0.0
-- 创建日期: 2026-03-20
-- 描述: 整合所有增量迁移，实现一键部署
-- 幂等性: 完全幂等，可重复执行
-- ============================================================================

-- 配置
SET statement_timeout = '30min';
SET lock_timeout = '10s';

BEGIN;

-- ============================================================================
-- 辅助函数: 表存在检查
-- ============================================================================
CREATE OR REPLACE FUNCTION table_exists(table_name TEXT)
RETURNS BOOLEAN AS $$
BEGIN
    RETURN EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = 'public' AND table_name = table_exists.table_name
    );
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- 第一部分: 基础用户表
-- ============================================================================

-- 1.1 users 表 (已在 schema 中定义，此处仅验证)
DO $$
BEGIN
    IF NOT table_exists('users') THEN
        RAISE EXCEPTION 'users table does not exist. Please run unified_schema first.';
    END IF;
END $$;

-- 1.2 devices 表
CREATE TABLE IF NOT EXISTS devices (
    device_id TEXT NOT NULL PRIMARY KEY,
    user_id TEXT NOT NULL,
    display_name TEXT,
    device_key JSONB,
    last_seen_ts BIGINT,
    last_seen_ip TEXT,
    created_ts BIGINT NOT NULL,
    first_seen_ts BIGINT NOT NULL,
    user_agent TEXT,
    appservice_id TEXT,
    ignored_user_list TEXT
);

CREATE INDEX IF NOT EXISTS idx_devices_user_id ON devices(user_id);
CREATE INDEX IF NOT EXISTS idx_devices_last_seen ON devices(last_seen_ts DESC);

-- ============================================================================
-- 第二部分: 认证相关表
-- ============================================================================

-- 2.1 access_tokens 表
CREATE TABLE IF NOT EXISTS access_tokens (
    id BIGSERIAL PRIMARY KEY,
    token TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    device_id TEXT,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT,
    last_used_ts BIGINT,
    user_agent TEXT,
    ip_address TEXT,
    is_revoked BOOLEAN DEFAULT FALSE,
    revoked_at BIGINT
);

CREATE INDEX IF NOT EXISTS idx_access_tokens_user_id ON access_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_access_tokens_valid ON access_tokens(is_revoked) WHERE is_revoked = FALSE;

-- ============================================================================
-- 第三部分: E2EE 加密表
-- ============================================================================

-- 3.1 device_keys 表
CREATE TABLE IF NOT EXISTS device_keys (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    key_id TEXT NOT NULL,
    public_key TEXT NOT NULL,
    key_data TEXT,
    signatures JSONB,
    added_ts BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    ts_updated_ms BIGINT,
    is_verified BOOLEAN DEFAULT FALSE,
    is_blocked BOOLEAN DEFAULT FALSE,
    display_name TEXT,
    CONSTRAINT uq_device_keys_user_device_key UNIQUE (user_id, device_id, key_id)
);

CREATE INDEX IF NOT EXISTS idx_device_keys_user_device ON device_keys(user_id, device_id);

-- 3.2 key_backups 表
CREATE TABLE IF NOT EXISTS key_backups (
    backup_id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    auth_data JSONB,
    auth_key TEXT,
    version BIGINT DEFAULT 1,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT uq_key_backups_user_version UNIQUE (user_id, version)
);

-- 3.3 backup_keys 表
CREATE TABLE IF NOT EXISTS backup_keys (
    id BIGSERIAL PRIMARY KEY,
    backup_id BIGINT NOT NULL,
    room_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    session_data JSONB NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT fk_backup_keys_backup FOREIGN KEY (backup_id) REFERENCES key_backups(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_backup_keys_backup ON backup_keys(backup_id);
CREATE INDEX IF NOT EXISTS idx_backup_keys_room ON backup_keys(room_id);

-- 3.4 olm_accounts 表
CREATE TABLE IF NOT EXISTS olm_accounts (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    identity_key TEXT NOT NULL,
    serialized_account TEXT NOT NULL,
    has_published_one_time_keys BOOLEAN DEFAULT FALSE,
    has_published_fallback_key BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT uq_olm_accounts_user_device UNIQUE (user_id, device_id)
);

-- ============================================================================
-- 第四部分: 房间相关表
-- ============================================================================

-- 4.1 rooms 表
CREATE TABLE IF NOT EXISTS rooms (
    room_id TEXT NOT NULL PRIMARY KEY,
    creator TEXT,
    is_public BOOLEAN DEFAULT FALSE,
    room_version TEXT DEFAULT '6',
    created_ts BIGINT NOT NULL,
    last_activity_ts BIGINT,
    is_federated BOOLEAN DEFAULT TRUE,
    has_guest_access BOOLEAN DEFAULT FALSE,
    join_rules TEXT DEFAULT 'invite',
    history_visibility TEXT DEFAULT 'shared',
    name TEXT,
    topic TEXT,
    avatar_url TEXT,
    canonical_alias TEXT,
    visibility TEXT DEFAULT 'private',
    guest_access VARCHAR(50) DEFAULT 'forbidden'
);

CREATE INDEX IF NOT EXISTS idx_rooms_creator ON rooms(creator) WHERE creator IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_rooms_is_public ON rooms(is_public) WHERE is_public = TRUE;

-- 4.2 room_memberships 表
CREATE TABLE IF NOT EXISTS room_memberships (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    membership TEXT NOT NULL,
    joined_ts BIGINT,
    invited_ts BIGINT,
    left_ts BIGINT,
    banned_ts BIGINT,
    sender TEXT,
    reason TEXT,
    event_id TEXT,
    event_type TEXT,
    display_name TEXT,
    avatar_url TEXT,
    is_banned BOOLEAN DEFAULT FALSE,
    invite_token TEXT,
    updated_ts BIGINT,
    join_reason TEXT,
    banned_by TEXT,
    ban_reason TEXT,
    CONSTRAINT uq_room_memberships_room_user UNIQUE (room_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_room_memberships_room ON room_memberships(room_id);
CREATE INDEX IF NOT EXISTS idx_room_memberships_user ON room_memberships(user_id);
CREATE INDEX IF NOT EXISTS idx_room_memberships_user_membership ON room_memberships(user_id, membership);

-- 4.3 room_summaries 表
CREATE TABLE IF NOT EXISTS room_summaries (
    room_id TEXT NOT NULL PRIMARY KEY,
    name TEXT,
    topic TEXT,
    canonical_alias TEXT,
    member_count BIGINT DEFAULT 0,
    joined_members BIGINT DEFAULT 0,
    invited_members BIGINT DEFAULT 0,
    hero_users JSONB DEFAULT '[]',
    is_world_readable BOOLEAN DEFAULT FALSE,
    can_guest_join BOOLEAN DEFAULT FALSE,
    is_federated BOOLEAN DEFAULT TRUE,
    encryption_state TEXT,
    updated_ts BIGINT,
    guest_access VARCHAR(50) DEFAULT 'can_join'
);

-- ============================================================================
-- 第五部分: 字段修复 (幂等)
-- ============================================================================

-- 5.1 users 表字段修复
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'users' AND column_name = 'must_change_password'
    ) THEN
        ALTER TABLE users RENAME COLUMN must_change_password TO is_password_change_required;
    END IF;
END $$;

-- 5.2 user_threepids 表字段修复
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'user_threepids' AND column_name = 'validated_ts'
    ) THEN
        IF NOT EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_name = 'user_threepids' AND column_name = 'validated_at'
        ) THEN
            ALTER TABLE user_threepids RENAME COLUMN validated_ts TO validated_at;
        ELSE
            ALTER TABLE user_threepids DROP COLUMN IF EXISTS validated_ts;
        END IF;
    END IF;
END $$;

-- 5.3 registration_tokens 表字段修复
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'registration_tokens' AND column_name = 'last_used_at'
    ) THEN
        IF NOT EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_name = 'registration_tokens' AND column_name = 'last_used_ts'
        ) THEN
            ALTER TABLE registration_tokens RENAME COLUMN last_used_at TO last_used_ts;
        ELSE
            ALTER TABLE registration_tokens DROP COLUMN IF EXISTS last_used_at;
        END IF;
    END IF;
END $$;

-- ============================================================================
-- 第六部分: 索引优化 (幂等)
-- ============================================================================

-- 6.1 users 索引
CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);
CREATE INDEX IF NOT EXISTS idx_users_is_admin ON users(is_admin);
CREATE INDEX IF NOT EXISTS idx_users_password_expires ON users(password_expires_at) WHERE password_expires_at IS NOT NULL;

-- 6.2 pushers 索引
CREATE INDEX IF NOT EXISTS idx_pushers_user ON pushers(user_id);
CREATE INDEX IF NOT EXISTS idx_pushers_enabled ON pushers(is_enabled) WHERE is_enabled = TRUE;

-- 6.3 space_children 索引
CREATE INDEX IF NOT EXISTS idx_space_children_space ON space_children(space_id);
CREATE INDEX IF NOT EXISTS idx_space_children_room ON space_children(room_id);

-- ============================================================================
-- 第七部分: 数据验证
-- ============================================================================

DO $$
DECLARE
    error_count INTEGER := 0;
BEGIN
    -- 检查必需表
    FOR table_name IN SELECT unnest(ARRAY[
        'users', 'devices', 'rooms', 'events', 'room_memberships',
        'access_tokens', 'refresh_tokens', 'device_keys', 'key_backups'
    ]) LOOP
        IF NOT table_exists(table_name) THEN
            RAISE WARNING 'Missing required table: %', table_name;
            error_count := error_count + 1;
        END IF;
    END LOOP;

    -- 检查字段一致性
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'users' AND column_name = 'must_change_password'
    ) THEN
        RAISE WARNING 'users.must_change_password should be renamed';
        error_count := error_count + 1;
    END IF;

    IF error_count = 0 THEN
        RAISE NOTICE '✓ Database migration completed successfully';
    ELSE
        RAISE WARNING '⚠ Migration completed with % errors', error_count;
    END IF;
END $$;

-- ============================================================================
-- 第八部分: 记录迁移
-- ============================================================================

INSERT INTO schema_migrations (version, name, description, applied_at, success)
VALUES (
    'UNIFIED_v1.0.0',
    'unified_migration',
    'Consolidated migration combining all incremental migrations',
    (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    TRUE
) ON CONFLICT (version) DO UPDATE SET
    applied_at = (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    success = TRUE;

COMMIT;

-- ============================================================================
-- 回滚方案 (如需回滚)
-- ============================================================================
-- 注意: 回滚综合迁移非常危险，可能导致数据丢失
-- 建议使用备份恢复，而非回滚
--
-- BEGIN;
-- -- 1. 删除新增的列 (谨慎操作)
-- -- ALTER TABLE users DROP COLUMN IF EXISTS new_column;
--
-- -- 2. 删除新增的索引 (如果不再需要)
-- -- DROP INDEX IF EXISTS idx_new_index;
--
-- -- 3. 回滚字段重命名
-- -- ALTER TABLE users RENAME COLUMN is_password_change_required TO must_change_password;
--
-- -- 4. 删除迁移记录
-- -- DELETE FROM schema_migrations WHERE version = 'UNIFIED_v1.0.0';
--
-- COMMIT;
--
-- 推荐的回滚方式: 使用备份恢复
-- pg_restore -U synapse -d synapse -c backup_full_20260320.dump
```

---

## 三、执行步骤

### 3.1 正式执行流程

```bash
#!/bin/bash
# 正式执行脚本

set -e

echo "=== 数据库迁移开始 ==="
echo "开始时间: $(date)"

# 1. 备份
echo "1. 创建备份..."
pg_dump -U synapse -d synapse -F c -b -v -f backup/synapse_full_$(date +%Y%m%d_%H%M%S).dump

# 2. 执行迁移
echo "2. 执行综合迁移..."
psql -U synapse -d synapse -f migrations/UNIFIED_MIGRATION_v1.sql

# 3. 验证
echo "3. 验证迁移..."
psql -U synapse -d synapse -c "SELECT COUNT(*) as tables FROM information_schema.tables WHERE table_schema = 'public';"
psql -U synapse -d synapse -c "SELECT version, success FROM schema_migrations ORDER BY applied_at DESC LIMIT 1;"

echo "=== 迁移完成 ==="
echo "完成时间: $(date)"
```

### 3.2 验证检查清单

```sql
-- 验证清单

-- 1. 表数量检查
SELECT COUNT(*) AS total_tables
FROM information_schema.tables
WHERE table_schema = 'public';

-- 2. 索引数量检查
SELECT COUNT(*) AS total_indexes
FROM pg_indexes
WHERE schemaname = 'public';

-- 3. 字段一致性检查
SELECT table_name, column_name
FROM information_schema.columns
WHERE table_name IN ('users', 'user_threepids', 'registration_tokens')
AND column_name IN ('must_change_password', 'validated_ts', 'last_used_at')
ORDER BY table_name, column_name;

-- 4. 外键完整性检查
SELECT
    tc.table_name,
    tc.constraint_name,
    tc.constraint_type
FROM information_schema.table_constraints tc
WHERE tc.constraint_type = 'FOREIGN KEY'
AND tc.table_schema = 'public';

-- 5. 迁移记录检查
SELECT * FROM schema_migrations ORDER BY applied_at DESC LIMIT 5;
```

---

## 四、常见问题处理

### 4.1 锁等待超时

```sql
-- 解决: 增加锁等待时间
SET lock_timeout = '60s';

-- 或使用非阻塞方式创建索引
CREATE INDEX CONCURRENTLY idx_new_index ON table_name(column_name);
```

### 4.2 表空间不足

```sql
-- 检查表空间使用
SELECT
    spcname,
    pg_size_pretty(pg_tablespace_size(spcname))
FROM pg_tablespace;

-- 扩展表空间
CREATE TABLESPACE new_space LOCATION '/path/to/new_space';
ALTER TABLE table_name SET TABLESPACE new_space;
```

### 4.3 外键约束失败

```sql
-- 临时禁用外键检查
SET CONSTRAINTS ALL DEFERRED;

-- 或使用 NOVALIDATE 添加约束
ALTER TABLE child_table
ADD CONSTRAINT fk_parent
FOREIGN KEY (parent_id) REFERENCES parent_table(id)
NOT VALID;
```

---

## 五、文档更新记录

| 日期 | 版本 | 更新内容 |
|------|------|----------|
| 2026-03-20 | v1.0.0 | 初始版本 |