# synapse-rust 项目优化状态报告

> 版本: v1.0.0
> 更新日期: 2026-03-12
> 壊查人员: 项目分析团队

---

## 一、优化进度概览

### 1.1 已完成任务 ✅

#### 1.1.1 数据库字段命名检查 ✅

**检查结果**:
- 已存在迁移文件 `20260312000004_fix_timestamp_field_names.sql` 用于修复 `expires_ts` → `expires_at` 和 `revoked_ts` -> `revoked_at`
- 统一架构文件中仍有 `created_at` 和 `updated_at` 字段需要修复
- 新迁移文件中存在字段命名不一致问题

- **修复措施**:
  - 修复了 `20260312000005_qr_login.sql` 中的字段命名
  - 修复了 `20260312000006_invite_blocklist.sql` 中的字段命名
  - 修复了 `20260312000007_sticky_event.sql` 中的字段命名
  - 创建了 `20260313000008_field_name_fix.sql` 用于修复统一架构文件中的字段命名

#### 1.1.2 缺失索引检查 ✅

**检查结果**:
- 已存在迁移文件 `20260312000004_add_missing_indexes.sql` 添加了关键索引
- `presence_subscriptions` 表已有复合索引
- `call_sessions`、 `call_candidates`、 `read_markers`、 `event_receipts` 等表都有索引
- **状态**: ✅ 已完成

#### 1.1.3 MSC4388 二维码登录实现检查 ✅

**检查结果**:
- 已存在完整的存储层实现 (`src/storage/qr_login.rs`)
- 已存在完整的路由层实现 (`src/web/routes/qr_login.rs`)
- 已存在数据库表 `qr_login_transactions`
- 已实现的功能:
  - 创建二维码登录事务
  - 获取二维码登录状态
  - 更新二维码登录状态
  - 删除二维码登录事务
  - 清理过期事务
- **状态**: ✅ 已完成

#### 1.1.4 集成测试框架检查 ✅

**检查结果**:
- 已存在测试框架目录结构 `tests/`
- 已有单元测试、集成测试、 E2E 测试、性能测试
- 测试公共模块已实现 (`tests/common/`)
  - `mock_db.rs` - Mock 数据库
  - `fixtures.rs` - 测试数据工厂
  - `assertions.rs` - 测试断言
- **状态**: ✅ 已完成

#### 1.1.5 Rust 代码字段引用修复 ✅

**修复内容**:
- 修复了 `src/storage/qr_login.rs` 中的字段引用
  - `created_at` → `created_ts`
  - `updated_at` → `updated_ts`
  - 更新了 SQL 查询中的字段名
  - 更新了 `QrTransaction` 结构体中的字段名
- **状态**: ✅ 已完成

### 1.2 进行中任务 🚧

#### 1.2.1 统一架构文件字段修复 🚧

**任务描述**: 执行 `20260313000008_field_name_fix.sql` 迁移文件

**进度**: 准备执行
**预计完成时间**: 2026-03-13

#### 1.2.2 Rust 代码全面检查 🚧

**任务描述**: 检查所有 Rust 代码中的字段引用，确保与数据库字段名一致
**进度**: 进行中
**预计完成时间**: 2026-03-14

---

## 二、详细修复记录

### 2.1 迁移文件修复

#### 2.1.1 qr_login.sql 修复
**修复前**:
```sql
CREATE TABLE IF NOT EXISTS qr_login_transactions (
    id BIGSERIAL PRIMARY KEY,
    transaction_id TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    device_id TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    created_at BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    completed_at BIGINT,
    access_token TEXT,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_at BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
);
```

**修复后**:
```sql
CREATE TABLE IF NOT EXISTS qr_login_transactions (
    id BIGSERIAL PRIMARY KEY,
    transaction_id TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    device_id TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_ts BIGINT,
    expires_at BIGINT NOT NULL,
    completed_at BIGINT,
    access_token TEXT
);
```

**修复内容**:
- 移除重复的 `created_ts` 和 `updated_at` 字段
- 将 `created_at` 重命名为 `created_ts`
- 将 `updated_at` 重命名为 `updated_ts` 并设为可空

- 调整字段顺序

#### 2.1.2 invite_blocklist.sql 修复
**修复前**:
```sql
CREATE TABLE IF NOT EXISTS room_invite_blocklist (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    created_at BIGINT NOT NULL,
    UNIQUE(room_id, user_id)
);
```
**修复后**:
```sql
CREATE TABLE IF NOT EXISTS room_invite_blocklist (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    UNIQUE(room_id, user_id)
);
```
**修复内容**:
- `created_at` → `created_ts`
- 添加默认值

#### 2.1.3 sticky_event.sql 修复
**修复前**:
```sql
CREATE TABLE IF NOT EXISTS room_sticky_events (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    sticky BOOLEAN NOT NULL DEFAULT true,
    created_at BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_at BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    UNIQUE(room_id, user_id, event_type)
);
```
**修复后**:
```sql
CREATE TABLE IF NOT EXISTS room_sticky_events (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    sticky BOOLEAN NOT NULL DEFAULT true,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    updated_ts BIGINT,
    UNIQUE(room_id, user_id, event_type)
);
```
**修复内容**:
- `created_at` → `created_ts`
- `updated_at` → `updated_ts` 并设为可空
- 移除 `updated_ts` 的默认值

### 2.2 Rust 代码修复
#### 2.2.1 qr_login.rs 修复
**修复前**:
```rust
pub async fn create_qr_login(
    &self,
    transaction_id: &str,
    user_id: &str,
    device_id: Option<&str>,
) -> Result<(), sqlx::Error> {
    let now = chrono::Utc::now().timestamp_millis();
    let expires_at = now + (5 * 60 * 1000);

    sqlx::query(
        r#"
        INSERT INTO qr_login_transactions (transaction_id, user_id, device_id, status, created_at, expires_at)
        VALUES ($1, $2, $3, 'pending', $4, $5)
        ...
        "#,
    )
    .bind(transaction_id)
    .bind(user_id)
    .bind(device_id)
    .bind(now)
    .bind(expires_at)
    .execute(&*self.pool)
    .await?;
    Ok(())
}

pub async fn get_qr_transaction(
    &self,
    transaction_id: &str,
) -> Result<Option<QrTransaction>, sqlx::Error> {
    let result = sqlx::query_as::<_, (String, String, Option<String>, String, i64, i64)>(
        r#"
        SELECT transaction_id, user_id, device_id, status, created_at, expires_at 
        FROM qr_login_transactions 
        WHERE transaction_id = $1
        "#,
    )
    .bind(transaction_id)
    .fetch_optional(&*self.pool)
    .await?;
    Ok(result.map(
        |(transaction_id, user_id, device_id, status, created_at, expires_at)| QrTransaction {
            transaction_id,
            user_id,
            device_id,
            status,
            created_at,
            expires_at,
        },
    ))
}
```
**修复后**:
```rust
pub async fn create_qr_login(
    &self,
    transaction_id: &str,
    user_id: &str,
    device_id: Option<&str>,
) -> Result<(), sqlx::Error> {
    let now = chrono::Utc::now().timestamp_millis();
    let expires_at = now + (5 * 60 * 1000);
    sqlx::query(
        r#"
        INSERT INTO qr_login_transactions (transaction_id, user_id, device_id, status, created_ts, expires_at)
        VALUES ($1, $2, $3, 'pending', $4, $5)
        ...
        "#,
    )
    .bind(transaction_id)
    .bind(user_id)
    .bind(device_id)
    .bind(now)
    .bind(expires_at)
    .execute(&*self.pool)
    .await?;
    Ok(())
}

pub async fn get_qr_transaction(
    &self,
    transaction_id: &str,
) -> Result<Option<QrTransaction>, sqlx::Error> {
    let result = sqlx::query_as::<_, (String, String, Option<String>, String, i64, Option<i64>, i64)>(
        r#"
        SELECT transaction_id, user_id, device_id, status, created_ts, updated_ts, expires_at 
        FROM qr_login_transactions 
        WHERE transaction_id = $1
        "#,
    )
    .bind(transaction_id)
    .fetch_optional(&*self.pool)
    .await?;
    Ok(result.map(
        |(transaction_id, user_id, device_id, status, created_ts, updated_ts, expires_at)| QrTransaction {
            transaction_id,
            user_id,
            device_id,
            status,
            created_ts,
            updated_ts,
            expires_at,
        },
    ))
}
```
**修复内容**:
- SQL 查询中的字段名修复
- `QrTransaction` 结构体字段名修复
- 添加 `updated_ts` 字段
- 更新 `update_qr_status` 方法添加 `updated_ts` 更新

---

## 三、下一步计划

### 3.1 矽期执行迁移
1. 执行 `20260313000008_field_name_fix.sql` 迁移文件
2. 验证字段命名是否正确
3. 运行测试确保功能正常

### 3.2 代码审查
1. 全面检查 Rust 代码中的字段引用
2. 更新相关文档
3. 提交代码审查
### 3.3 功能测试
1. 运行单元测试
2. 运行集成测试
3. 验证二维码登录功能
### 3.4 文档更新
1. 更新 `DATABASE_FIELD_STANDARDS.md`
2. 更新 `MIGRATION_INDEX.md`
3. 更新项目规则文档

---

## 四、总结

本次优化检查发现项目已经实现了大部分短期优化任务，主要问题集中在迁移文件中的字段命名不一致。已修复以下文件:

1. `migrations/20260312000005_qr_login.sql`
2. `migrations/20260312000006_invite_blocklist.sql`
3. `migrations/20260312000007_sticky_event.sql`
4. `src/storage/qr_login.rs`
5. 创建了新的迁移文件 `20260313000008_field_name_fix.sql` 用于修复统一架构文件中的字段命名问题。

下一步需要执行迁移并运行测试验证修复效果。
