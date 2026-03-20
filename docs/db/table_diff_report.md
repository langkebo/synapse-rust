# SQL 与 Rust 表结构差异分析报告

> **项目**: synapse-rust 数据库全面排查与优化
> **版本**: v1.0.0
> **生成日期**: 2026-03-20
> **分析范围**: SQL Schema vs Rust Table Definitions vs Rust Models

---

## 统计概览

| 指标 | 数量 |
|------|------|
| SQL 表总数 | 114+ |
| Rust 动态表数 | 23+ |
| 一致性检查数 | 35+ |
| 发现差异数 | 3 |
| 已修复严重问题 (P0/P1) | 2 ✅ |
| 中等问题 (P2) | 3 |

---

## 第一部分：已修复的严重问题 (P0/P1) ✅

### 问题 1: threepids 表 - 废弃表已在 Rust 代码中清理 [P1] ✅ 已修复

| 项目 | 详情 |
|------|------|
| 问题类型 | 废弃表定义 |
| 严重程度 | P1 |
| SQL 状态 | 已废弃 (功能合并到 user_threepids) |
| Rust 状态 | ✅ 已在 `database_initializer.rs` 中移除 |

**修复操作**:
- 从 `database_initializer.rs` 中移除了废弃的 `threepids` 表创建代码
- 确认业务代码使用 `user_threepids` 表

**验证**:
- 代码编译通过 ✅
- 业务逻辑使用正确的表 ✅

---

### 问题 2: reports 表 - 废弃表已在 Rust 代码中清理 [P1] ✅ 已修复

| 项目 | 详情 |
|------|------|
| 问题类型 | 废弃表定义 |
| 严重程度 | P1 |
| SQL 状态 | 已废弃 (功能合并到 event_reports) |
| Rust 状态 | ✅ 已在 `database_initializer.rs` 中移除 |

**修复操作**:
- 从 `database_initializer.rs` 中移除了废弃的 `reports` 表创建代码
- 确认业务代码使用 `event_reports` 表

**验证**:
- 代码编译通过 ✅
- 业务逻辑使用正确的表 ✅

---

## 第二部分：中等差异 (P2)

### 问题 3: rooms.guest_access 类型不一致 [P2]

| 项目 | 详情 |
|------|------|
| 问题类型 | 数据类型不一致 |
| 严重程度 | P2 |
| SQL 定义 | `has_guest_access BOOLEAN DEFAULT FALSE` |
| Rust 动态添加 | `guest_access VARCHAR(50) DEFAULT 'forbidden'` |

**SQL 定义**:
```sql
-- unified_schema_v6.sql
has_guest_access BOOLEAN DEFAULT FALSE
```

**Rust 动态添加**:
```sql
-- database_initializer.rs - step_ensure_additional_tables()
ALTER TABLE rooms ADD COLUMN IF NOT EXISTS guest_access VARCHAR(50) DEFAULT 'forbidden'
```

**差异对比**:
| 属性 | SQL | Rust |
|------|-----|------|
| 列名 | has_guest_access | guest_access |
| 类型 | BOOLEAN | VARCHAR(50) |
| 默认值 | FALSE | 'forbidden' |

**影响分析**:
- 同一功能使用不同的列名
- 数据类型不兼容 (BOOLEAN vs VARCHAR)
- 可能导致查询逻辑混乱

**修复建议**:
1. 统一使用 `has_guest_access BOOLEAN` 类型
2. 或统一使用 `guest_access VARCHAR(50)` 类型
3. 清理多余的列定义
4. 确保代码中只使用一个列

---

### 问题 4: search_index.updated_ts 列定义一致性 [P2]

| 项目 | 详情 |
|------|------|
| 问题类型 | 列定义一致性问题 |
| 严重程度 | P2 |
| SQL 定义 | `updated_ts BIGINT` |
| Rust 定义 | `updated_ts BIGINT` |
| 状态 | 已一致 |

**说明**: 此问题在审计中发现 SQL 和 Rust 定义已经一致，无需修复。但需要持续监控。

---

### 问题 5: user_directory.updated_ts 模型缺失 [P2]

| 项目 | 详情 |
|------|------|
| 问题类型 | 模型字段缺失 |
| 严重程度 | P2 |
| SQL 表 | `user_directory` 有 `updated_ts` 列 |
| Rust 模型 | `UserDirectory` 缺少 `updated_ts` 字段 |

**SQL 定义**:
```sql
-- unified_schema_v6.sql
CREATE TABLE user_directory (
    user_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    visibility TEXT NOT NULL DEFAULT 'private',
    added_by TEXT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,  -- 此列存在于 SQL
);
```

**Rust 模型定义**:
```rust
// src/storage/models/user.rs
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct UserDirectory {
    pub user_id: String,
    pub room_id: String,
    pub visibility: String,
    pub added_by: Option<String>,
    pub created_ts: i64,
    // 缺少 updated_ts 字段
}
```

**影响分析**:
- 无法正确读取 `updated_ts` 列数据
- 可能导致用户目录更新时数据丢失
- ORM 映射不完整

**修复建议**:
1. 在 `UserDirectory` 模型中添加 `updated_ts: Option<i64>` 字段
2. 确保与 SQL 表结构完全一致

---

## 第三部分：字段命名规范问题

### 问题 6: users.password_expires_at 命名检查 [P2]

| 表名 | 字段 | 当前状态 | 规范要求 | 结论 |
|------|------|----------|----------|------|
| users | password_expires_at | ✅ 正确 | `_at` 后缀用于可选过期时间 | 符合规范 |

**说明**: 根据 `DATABASE_FIELD_STANDARDS.md` 规范，`_at` 后缀用于过期、撤销、验证等可选操作的时间戳，`password_expires_at` 正确使用了 `_at` 后缀。

---

### 问题 7: user_threepids.validated_at 命名检查 [P2]

| 表名 | 字段 | 当前状态 | 规范要求 | 结论 |
|------|------|----------|----------|------|
| user_threepids | validated_at | ✅ 正确 | `_at` 后缀用于可选验证时间 | 符合规范 |

**说明**: `validated_at` 正确使用了 `_at` 后缀，因为验证时间是可选操作。

---

### 问题 8: refresh_tokens.last_used_ts 命名检查 [P2]

| 表名 | 字段 | 当前状态 | 规范要求 | 结论 |
|------|------|----------|----------|------|
| refresh_tokens | last_used_ts | ✅ 正确 | `_ts` 后缀用于活跃时间 | 符合规范 |

**说明**: `last_used_ts` 正确使用了 `_ts` 后缀，因为最后使用时间是活跃时间戳。

---

### 问题 9: registration_tokens.last_used_ts 命名检查 [P2]

| 表名 | 字段 | 当前状态 | 规范要求 | 结论 |
|------|------|----------|----------|------|
| registration_tokens | last_used_ts | ✅ 正确 | `_ts` 后缀用于活跃时间 | 符合规范 |

**说明**: `last_used_ts` 正确使用了 `_ts` 后缀。

---

## 第四部分：索引差异分析

### 索引完整性检查

| 表名 | SQL 索引数 | Rust 定义索引数 | 一致性 |
|------|-----------|-----------------|--------|
| device_keys | 2 | 1 | ⚠️ 差异 |
| pushers | 3 | 1 | ⚠️ 差异 |
| user_threepids | 2 | 1 | ⚠️ 差异 |
| refresh_tokens | 2 | 1 | ⚠️ 差异 |
| access_tokens | 2 | 1 | ⚠️ 差异 |

**说明**: Rust 动态表创建时只创建了唯一约束索引，SQL 中的额外索引未在 Rust 中创建。

**影响分析**:
- 可能影响查询性能
- 部分索引在动态创建时缺失

**修复建议**:
1. 确保 Rust 动态表创建时包含所有必要的索引
2. 或在数据库初始化后运行额外的索引创建脚本

---

## 第五部分：差异汇总表

| 序号 | 问题类型 | 严重程度 | 表名 | 问题描述 | 修复优先级 |
|------|----------|----------|------|----------|------------|
| 1 | 废弃表 | P1 | threepids | SQL 已废弃，Rust 仍在使用 | 高 |
| 2 | 废弃表 | P1 | reports | SQL 已废弃，Rust 仍在使用 | 高 |
| 3 | 类型不一致 | P2 | rooms | guest_access vs has_guest_access | 中 |
| 4 | 模型缺失 | P2 | user_directory | UserDirectory 模型缺少 updated_ts | 中 |
| 5 | 索引缺失 | P2 | 多个表 | Rust 动态创建时缺少部分索引 | 中 |

---

## 第六部分：修复计划

### 紧急修复 (P1) - 立即处理

| 序号 | 任务 | 负责人 | 截止日期 |
|------|------|--------|----------|
| 1 | 清理 threepids 表相关代码 | - | - |
| 2 | 清理 reports 表相关代码 | - | - |

### 中期修复 (P2) - 计划处理

| 序号 | 任务 | 负责人 | 截止日期 |
|------|------|--------|----------|
| 3 | 统一 rooms.guest_access 定义 | - | - |
| 4 | 添加 UserDirectory.updated_ts 字段 | - | - |
| 5 | 补充缺失的索引定义 | - | - |

---

## 附录：相关文件列表

| 文件路径 | 说明 |
|----------|------|
| `migrations/00000000_unified_schema_v6.sql` | 主 SQL Schema 定义 |
| `src/services/database_initializer.rs` | Rust 表初始化定义 |
| `src/storage/models/user.rs` | 用户相关模型 |
| `src/storage/models/device.rs` | 设备相关模型 |
| `src/storage/token.rs` | Token 相关模型 |
| `src/storage/refresh_token.rs` | 刷新令牌模型 |

---

## 文档更新记录

| 日期 | 版本 | 更新内容 |
|------|------|----------|
| 2026-03-20 | v1.0.0 | 初始版本，基于 SQL vs Rust 对比分析生成 |