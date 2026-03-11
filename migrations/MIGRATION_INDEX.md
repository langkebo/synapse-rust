# 数据库迁移文件索引

> 版本: v6.0.4
> 更新日期: 2026-03-11
> 状态: ✅ 活跃

## 概述

本文档记录 synapse-rust 项目的数据库迁移文件，包括版本号、描述和执行顺序。

---

## 迁移文件列表

### 核心架构迁移

| 版本号 | 文件名 | 描述 | 状态 |
|--------|--------|------|------|
| v6.0.0 | `00000000_unified_schema_v6.sql` | 统一数据库架构文件 | ✅ 核心 |
| v5 | `archive/00000000_unified_schema_v5.sql` | 旧版统一架构 | 📦 归档 |

### 增量迁移 (v6.0.0+)

| 版本号 | 文件名 | 描述 | 状态 |
|--------|--------|------|------|
| v6.0.1 | `20260309000001_password_security_enhancement.sql` | 密码安全增强 | ✅ |
| v6.0.2 | `20260310000001_add_missing_e2ee_tables.sql` | E2EE 缺失表 | ✅ |
| v6.0.2 | `20260310000002_normalize_fields_and_add_tables.sql` | 字段规范化 | ✅ |
| v6.0.2 | `20260310000003_fix_api_test_issues.sql` | API 测试问题修复 | ✅ |
| v6.0.2 | `20260310000004_create_federation_signing_keys.sql` | 联邦签名密钥 | ✅ |
| v6.0.3 | `20260311000001_add_space_members_table.sql` | Space 成员表 | ✅ |
| v6.0.3 | `20260311000002_fix_table_structures.sql` | 表结构修复 | ✅ |
| v6.0.4 | `20260311000003_optimize_database_structure.sql` | 数据库结构优化 | ✅ |

---

## 字段命名规范

### 时间戳字段

| 后缀 | 数据类型 | 说明 | 示例 |
|------|----------|------|------|
| `_ts` | BIGINT | NOT NULL 毫秒级时间戳 | `created_ts`, `updated_ts` |
| `_at` | BIGINT | 可选毫秒级时间戳 | `expires_at`, `revoked_at` |

### 布尔字段

| 前缀 | 说明 | 示例 |
|------|------|------|
| `is_*` | 是否... | `is_admin`, `is_enabled`, `is_revoked` |
| `has_*` | 拥有... | `has_avatar`, `has_displayname` |

### 外键字段

| 格式 | 说明 | 示例 |
|------|------|------|
| `{table}_id` | 外键引用 | `user_id`, `room_id`, `device_id` |

---

## 核心表字段要求

### users 表
```sql
user_id TEXT PRIMARY KEY,
username TEXT UNIQUE NOT NULL,
is_admin BOOLEAN DEFAULT FALSE,
is_guest BOOLEAN DEFAULT FALSE,
is_shadow_banned BOOLEAN DEFAULT FALSE,
is_deactivated BOOLEAN DEFAULT FALSE,
created_ts BIGINT NOT NULL,
updated_ts BIGINT
```

### rooms 表
```sql
room_id TEXT PRIMARY KEY,
creator_user_id TEXT,
room_version TEXT DEFAULT '6',
join_rules TEXT DEFAULT 'invite',
is_public BOOLEAN DEFAULT FALSE,
is_federatable BOOLEAN DEFAULT TRUE,
created_ts BIGINT NOT NULL
```

### events 表
```sql
event_id TEXT PRIMARY KEY,
room_id TEXT NOT NULL,
sender TEXT NOT NULL,
event_type TEXT NOT NULL,
content JSONB DEFAULT '{}',
origin_server_ts BIGINT NOT NULL,
created_ts BIGINT NOT NULL
```

### devices 表
```sql
device_id TEXT PRIMARY KEY,
user_id TEXT NOT NULL,
display_name TEXT,
last_seen_ts BIGINT,
created_ts BIGINT NOT NULL,
first_seen_ts BIGINT NOT NULL
```

### access_tokens 表
```sql
id BIGSERIAL PRIMARY KEY,
token TEXT UNIQUE NOT NULL,
user_id TEXT NOT NULL,
device_id TEXT,
created_ts BIGINT NOT NULL,
expires_ts BIGINT NOT NULL,
is_valid BOOLEAN DEFAULT TRUE
```

### refresh_tokens 表
```sql
id BIGSERIAL PRIMARY KEY,
token_hash TEXT UNIQUE NOT NULL,
user_id TEXT NOT NULL,
device_id TEXT,
created_ts BIGINT NOT NULL,
expires_at BIGINT,
is_revoked BOOLEAN DEFAULT FALSE,
revoked_ts BIGINT
```

---

## 迁移执行顺序

### 新环境初始化

```bash
# 1. 执行核心架构
psql -U synapse -d synapse -f migrations/00000000_unified_schema_v6.sql

# 2. 按顺序执行增量迁移
for f in migrations/202603*.sql; do
    psql -U synapse -d synapse -f "$f"
done
```

### 从 v5 升级

```bash
# 1. 备份数据
pg_dump -U synapse synapse > backup_$(date +%Y%m%d).sql

# 2. 执行增量迁移
for f in migrations/202603*.sql; do
    psql -U synapse -d synapse -f "$f"
done
```

---

## 兼容字段映射

为保持向后兼容，以下字段对同时存在：

| 主字段 | 兼容字段 | 表 |
|--------|----------|------|
| `creator_user_id` | `creator` | rooms |
| `join_rules` | `join_rule` | rooms |
| `room_version` | `version` | rooms |
| `avatar_url` | `avatar` | rooms, users |
| `is_federatable` | `federate` | rooms |
| `created_ts` | `creation_ts` | rooms |
| `event_type` | `type` | events |

---

## 禁止使用的字段

| 禁止字段 | 替代字段 | 原因 |
|----------|----------|------|
| `invalidated` | `is_revoked` | 语义重复 |
| `invalidated_ts` | `revoked_ts` | 命名不一致 |
| `created_at` | `created_ts` | 统一使用 `_ts` 后缀 |
| `updated_at` | `updated_ts` | 统一使用 `_ts` 后缀 |
| `enabled` | `is_enabled` | 布尔字段需 `is_` 前缀 |

---

## 当前状态

| 指标 | 数值 |
|------|------|
| 核心迁移文件 | 1 个 |
| 增量迁移文件 | 8 个 |
| 归档迁移文件 | 37 个 |
| 数据库表 | 99+ 个 |
| 迁移版本 | v6.0.4 |

---

## 相关文档

- [DATABASE_FIELD_STANDARDS.md](./DATABASE_FIELD_STANDARDS.md) - 字段命名标准
- [MIGRATION_HISTORY.md](./MIGRATION_HISTORY.md) - 迁移历史记录
- [SCHEMA_OPTIMIZATION_REPORT.md](./SCHEMA_OPTIMIZATION_REPORT.md) - Schema 优化报告
- [README.md](./README.md) - 迁移使用说明

---

## 变更日志

### 2026-03-11
- 添加 `20260311000003_optimize_database_structure.sql` 迁移
- 更新文档结构，添加字段规范说明
- 更新迁移版本至 v6.0.4

### 2026-03-09
- 创建 `00000000_unified_schema_v6.sql` 统一架构
- 归档 37 个旧迁移文件到 `archive/` 目录
