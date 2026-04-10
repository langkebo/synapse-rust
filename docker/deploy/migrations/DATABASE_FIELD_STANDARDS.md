# Synapse Rust 数据库字段使用规范

> **版本**: v3.1.0
> **更新日期**: 2026-03-20
> **审核状态**: 已通过全面排查 (db-comprehensive-audit-v1)

---

## 1. 命名规范

### 1.1 通用命名规则

| 规则 | 说明 | 示例 |
|------|------|------|
| 使用snake_case | 所有字段名使用小写字母和下划线 | `user_id`, `created_ts` |
| 避免缩写 | 除非是广泛认知的缩写 | `access_token` 而非 `acc_tok` |
| 布尔字段使用is_/has_前缀 | 明确表示布尔类型 | `is_revoked`, `is_admin`, `has_published_keys` |

### 1.2 时间字段规范 ⭐ 核心规范

#### 统一标准

| 字段类型 | 推荐字段名 | 数据类型 | 可空性 | 说明 |
|----------|------------|----------|--------|------|
| **创建时间** | `created_ts` | BIGINT | NOT NULL | 毫秒级时间戳 |
| **更新时间** | `updated_ts` | BIGINT | 可空 | 毫秒级时间戳 |
| **过期时间** | `expires_at` | BIGINT | 可空 | 毫秒级时间戳 |
| **撤销时间** | `revoked_at` | BIGINT | 可空 | 毫秒级时间戳 |
| **最后使用时间** | `last_used_ts` | BIGINT | 可空 | 毫秒级时间戳 |
| **验证时间** | `validated_at` | BIGINT | 可空 | 毫秒级时间戳 |

#### 命名规则说明

| 后缀 | 用途 | 可空性 | 示例 |
|------|------|--------|------|
| `_ts` | 必须存在的时间戳 | NOT NULL 或 可空 | `created_ts`, `updated_ts`, `added_ts` |
| `_at` | 可选操作的时间戳 | 可空 | `expires_at`, `revoked_at`, `validated_at`, `last_used_at` |

#### ⚠️ 重要：必须一致的字段名

以下字段在代码和数据库中必须完全一致：

| 表名 | 字段名 | 正确写法 | 错误写法 |
|------|--------|----------|----------|
| users | 密码过期时间 | `password_expires_at` | `password_expires_ts` |
| user_threepids | 验证时间 | `validated_at` | `validated_ts` |
| refresh_tokens | 最后使用时间 | `last_used_ts` | `last_used_at` |
| registration_tokens | 最后使用时间 | `last_used_ts` | `last_used_at` |

### 1.3 布尔字段命名规范

| 前缀 | 用途 | 示例 |
|------|------|------|
| `is_` | 是否...状态 | `is_admin`, `is_revoked`, `is_enabled` |
| `has_` | 拥有...属性 | `has_published_keys`, `has_avatar` |

**建议优化（非强制）**：

| 当前命名 | 建议命名 | 原因 |
|----------|----------|------|
| `must_change_password` | `is_password_change_required` | 更符合 `is_` 前缀规范 |
| `is_one_time_keys_published` | `has_published_one_time_keys` | 更符合 `has_` 前缀规范 |
| `is_fallback_key_published` | `has_published_fallback_key` | 更符合 `has_` 前缀规范 |

### 1.4 禁止使用的字段

| 禁止字段 | 替代字段 | 原因 |
|----------|----------|------|
| `created_at` | `created_ts` | 统一使用 `_ts` 后缀 |
| `updated_at` | `updated_ts` | 统一使用 `_ts` 后缀 |
| `invalidated` | `is_revoked` | 布尔字段需 `is_` 前缀 |
| `invalidated_ts` | `revoked_at` | 命名不一致 |
| `expires_ts` | `expires_at` | 可选过期时间用 `_at` |
| `revoked_ts` | `revoked_at` | 可选撤销时间用 `_at` |
| `validated_ts` | `validated_at` | 验证时间用 `_at` |
| `enabled` | `is_enabled` | 布尔字段需 `is_` 前缀 |
| `last_used_at` | `last_used_ts` | 活跃时间用 `_ts` |

---

## 2. 核心表字段规范

### 2.1 users 表

```sql
CREATE TABLE users (
    user_id VARCHAR(255) PRIMARY KEY,
    username VARCHAR(255) UNIQUE NOT NULL,
    password_hash TEXT,
    is_admin BOOLEAN DEFAULT FALSE,
    is_guest BOOLEAN DEFAULT FALSE,
    is_shadow_banned BOOLEAN DEFAULT FALSE,
    is_deactivated BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    displayname VARCHAR(255),
    avatar_url TEXT,
    email TEXT,
    phone TEXT,
    generation BIGINT DEFAULT 0,
    consent_version TEXT,
    appservice_id TEXT,
    user_type TEXT,
    invalid_update_at BIGINT,
    migration_state TEXT,
    password_changed_ts BIGINT,
    must_change_password BOOLEAN DEFAULT FALSE,
    password_expires_at BIGINT,           -- ⚠️ 必须是 _at
    failed_login_attempts INTEGER DEFAULT 0,
    locked_until BIGINT
);
```

### 2.2 user_threepids 表

```sql
CREATE TABLE user_threepids (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    medium VARCHAR(255) NOT NULL,
    address VARCHAR(255) NOT NULL,
    validated_at BIGINT,                  -- ⚠️ 必须是 _at (代码使用 validated_at)
    added_ts BIGINT NOT NULL,
    is_verified BOOLEAN DEFAULT FALSE,
    verification_token TEXT,
    verification_expires_at BIGINT
);
```

### 2.3 refresh_tokens 表

```sql
CREATE TABLE refresh_tokens (
    id BIGSERIAL PRIMARY KEY,
    token_hash TEXT UNIQUE NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255),
    access_token_id VARCHAR(255),
    scope VARCHAR(255),
    created_ts BIGINT NOT NULL,
    expires_at BIGINT,
    last_used_ts BIGINT,                  -- ⚠️ 必须是 _ts (代码使用 last_used_ts)
    use_count INTEGER DEFAULT 0,
    is_revoked BOOLEAN DEFAULT FALSE,
    revoked_at BIGINT,
    revoked_reason TEXT,
    client_info JSONB,
    ip_address VARCHAR(45),
    user_agent TEXT
);
```

### 2.4 registration_tokens 表

```sql
CREATE TABLE registration_tokens (
    id BIGSERIAL PRIMARY KEY,
    token TEXT UNIQUE NOT NULL,
    token_type VARCHAR(50) DEFAULT 'single_use',
    description TEXT,
    max_uses INTEGER DEFAULT 0,
    uses_count INTEGER DEFAULT 0,
    is_used BOOLEAN DEFAULT FALSE,
    is_enabled BOOLEAN DEFAULT TRUE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    expires_at BIGINT,
    last_used_ts BIGINT,                  -- ⚠️ 必须是 _ts (代码使用 last_used_ts)
    created_by TEXT,
    allowed_email_domains TEXT[],
    allowed_user_ids TEXT[],
    auto_join_rooms TEXT[],
    display_name TEXT,
    email TEXT
);
```

---

## 3. 代码规范

### 3.1 结构体字段定义

```rust
// ✅ 正确示例
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct User {
    pub password_expires_at: Option<i64>,  // 与 schema 一致
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserThreepid {
    pub validated_at: Option<i64>,        // 与 schema 一致
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RefreshToken {
    pub last_used_ts: Option<i64>,        // 与 schema 一致
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RegistrationToken {
    pub last_used_ts: Option<i64>,        // 与 schema 一致
}

// ❌ 错误示例
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct User {
    pub password_expires_ts: Option<i64>, // 错误：应为 password_expires_at
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserThreepid {
    pub validated_ts: Option<i64>,        // 错误：应为 validated_at
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RefreshToken {
    pub last_used_at: Option<i64>,        // 错误：应为 last_used_ts
}
```

---

## 4. 版本历史

| 版本 | 日期 | 变更说明 |
|------|------|----------|
| 1.0.0 | 2026-02-19 | 初始版本，统一字段命名规范 |
| 2.0.0 | 2026-03-09 | 创建统一 Schema 基线文件 |
| 2.1.0 | 2026-03-12 | 完成 22 个模块审核验证 |
| **3.0.0** | **2026-03-14** | **全面排查修复：password_expires_at, validated_at, last_used_ts** |
| **3.1.0** | **2026-03-20** | **db-comprehensive-audit-v1 全面排查更新** |

### v3.1.0 详细变更

#### 新增规范

| 变更类型 | 内容 |
|----------|------|
| 新增布尔字段规范 | 添加 `is_`/`has_` 前缀使用指南 |
| 新增建议优化 | 添加布尔字段命名建议（非强制） |
| 更新禁止字段列表 | 添加 `last_used_at` 禁止项 |

#### 发现的规范问题（建议优化，非强制）

| 表名 | 字段 | 当前命名 | 建议命名 |
|------|------|----------|----------|
| users | must_change_password | `must_change_password` | `is_password_change_required` |
| olm_accounts | is_one_time_keys_published | `is_one_time_keys_published` | `has_published_one_time_keys` |
| olm_accounts | is_fallback_key_published | `is_fallback_key_published` | `has_published_fallback_key` |

### v3.0.0 详细变更

#### 修复的字段不一致问题

| 表名 | 问题字段 | 修复方案 |
|------|----------|----------|
| users | `password_expires_at` vs `password_expires_ts` | Schema 改为 `password_expires_at` |
| user_threepids | `validated_at` vs `validated_ts` | Schema 改为 `validated_at` |
| refresh_tokens | `last_used_ts` vs `last_used_at` | Schema 改为 `last_used_ts` |
| registration_tokens | `last_used_at` vs `last_used_ts` | Schema 改为 `last_used_ts` |

---

## 5. 快速检查命令

### 5.1 检查字段不一致

```bash
# 检查 users 表
psql -U synapse -d synapse -c "
SELECT column_name FROM information_schema.columns 
WHERE table_name = 'users' AND column_name LIKE '%expires%';
"

# 检查 user_threepids 表  
psql -U synapse -d synapse -c "
SELECT column_name FROM information_schema.columns 
WHERE table_name = 'user_threepids' AND column_name LIKE '%validat%';
"

# 检查 refresh_tokens 表
psql -U synapse -d synapse -c "
SELECT column_name FROM information_schema.columns 
WHERE table_name = 'refresh_tokens' AND column_name LIKE '%last_used%';
"
```

### 5.2 代码检查

```bash
# 检查不一致的字段名
grep -r "password_expires_ts\|validated_ts\|last_used_at" src/storage/models/
```
