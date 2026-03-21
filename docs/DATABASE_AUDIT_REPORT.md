# 数据库 Schema 审核报告

> 审核日期: 2026-03-21
> 审核工具: postgres-expert Skill v1.2.0
> 最后更新: 2026-03-21 (v1.5 - 代码修复完成)

---

## 摘要

本次审核对 **synapse-rust** 项目的数据库 Schema 进行了现场验证和修复。所有 P0 和 P1 问题已修复完成，包括数据库迁移和代码同步。

---

## ✅ 已完成的修复

| # | 问题 | 类型 | 修复方案 | 状态 |
|---|------|------|----------|------|
| 1 | `token_blacklist` 缺少 `is_revoked` | 数据库 | 迁移脚本添加 ADD COLUMN | ✅ 已修复 |
| 2 | `user_threepids` 字段命名 | 数据库 | `validated_at` → `validated_ts`<br>`verification_expires_at` → `verification_expires_ts` | ✅ 已修复 |
| 3 | `private_messages` 字段命名 | 数据库 | `read_at` → `read_ts` | ✅ 已修复 |
| 4 | `schema_health_check.rs` 字段名错误 | 代码 | `creation_ts` → `created_ts`<br>`user_id` → `sender` (events表) | ✅ 已修复 |
| 5 | `compile_time_validation.rs` 字段名错误 | 代码 | `creation_ts` → `created_ts` | ✅ 已修复 |
| 6 | `web/routes/mod.rs` 字段引用 | 代码 | `validated_at` → `validated_ts` | ✅ 已修复 |
| 7 | `services/identity/storage.rs` 字段引用 | 代码 | `validated_at` → `validated_ts`<br>`added_at` → `added_ts` | ✅ 已修复 |
| 8 | `services/identity/models.rs` 字段定义 | 代码 | `ThirdPartyId.validated_at` → `validated_ts`<br>`ThirdPartyId.added_at` → `added_ts` | ✅ 已修复 |
| 9 | `storage/threepid.rs` SQL 查询 | 代码 | 所有 `validated_at` → `validated_ts`<br>所有 `verification_expires_at` → `verification_expires_ts` | ✅ 已修复 |
| 10 | `lock_timeout` 不足 | 配置 | 10s → 120s | ✅ 已修复 |

---

## 代码修复详情

### 修复 4 & 5: schema_health_check.rs & compile_time_validation.rs

**schema_health_check.rs**:
```rust
// 修改前                    // 修改后
("users", "creation_ts")  →  ("users", "created_ts")
("rooms", "creation_ts")  →  ("rooms", "created_ts")
("events", "user_id")     →  ("events", "sender")
("idx_users_creation_ts")  →  ("idx_users_created_ts")
```

**compile_time_validation.rs**:
```rust
// 修改前                    // 修改后
pub struct User {          pub struct User {
    pub creation_ts: i64       pub created_ts: i64,
}                           }

// 修改前                    // 修改后
pub struct Room {           pub struct Room {
    pub creation_ts: i64       pub created_ts: i64,
}                           }
```

### 修复 6: web/routes/mod.rs

```rust
// 修改前                          // 修改后
SELECT ... validated_at ...    →  SELECT ... validated_ts ...
"validated_at": ...           →  "validated_ts": ...
INSERT ... validated_at ...   →  INSERT ... validated_ts ...
```

### 修复 7: services/identity/storage.rs

```rust
// ThreePidRow 结构体
validated_at: Option<i64>  →  validated_ts: Option<i64>
added_at: Option<i64>      →  added_ts: Option<i64>

// SQL 查询
SELECT ... validated_at, added_at ...  →  SELECT ... validated_ts, added_ts ...
INSERT ... validated_at, added_at ... →  INSERT ... validated_ts, added_ts ...
```

### 修复 8: services/identity/models.rs

```rust
// ThirdPartyId 结构体
pub validated_at: i64  →  pub validated_ts: i64
pub added_at: i64     →  pub added_ts: i64

// impl ThirdPartyId::new()
validated_at: now     →  validated_ts: now
added_at: now         →  added_ts: now
```

### 修复 9: storage/threepid.rs

```rust
// CreateThreepidRequest
verification_expires_at: Option<i64>  →  verification_expires_ts: Option<i64>

// 所有 SQL 查询
validated_at               →  validated_ts
verification_expires_at     →  verification_expires_ts
```

---

## 数据库结构验证

### 当前数据库表数量
```sql
SELECT COUNT(*) FROM pg_tables WHERE schemaname = 'public';
-- 结果: 152 张表
```

### 关键表字段验证

| 表名 | is_revoked | validated_ts | verification_expires_ts | read_ts | 状态 |
|------|------------|--------------|-------------------------|---------|------|
| access_tokens | ✅ | N/A | N/A | N/A | ✅ |
| refresh_tokens | ✅ | N/A | N/A | N/A | ✅ |
| token_blacklist | ✅ | N/A | N/A | N/A | ✅ |
| user_threepids | N/A | ✅ | ✅ | N/A | ✅ |
| private_messages | N/A | N/A | N/A | ✅ | ✅ |

---

## 迁移脚本状态

| 迁移文件 | 版本 | 状态 |
|----------|------|------|
| `00000000_unified_schema_v6.sql` | v6 | ✅ 基础 schema |
| `UNIFIED_MIGRATION_v1.sql` | v1 | ✅ 已更新 (含所有修复) |
| Docker entrypoint | - | ✅ `lock_timeout=120s` |

---

## 编译验证

```bash
cd /Users/ljf/Desktop/hu/synapse-rust
cargo build
# 编译成功，无错误
```

---

## 待观察问题 (非阻塞)

### 1. 迁移锁超时

虽然 `lock_timeout` 已增加到 120s，但在高负载数据库上仍可能超时。

**建议**: 监控生产环境迁移执行时间，必要时进一步增加。

---

## 总结

| 类别 | 数量 | 已修复 | 状态 |
|------|------|--------|------|
| Token 表字段 | 3 | 3 | ✅ 完成 |
| 时间戳字段命名 | 3 | 3 | ✅ 完成 |
| Schema 健康检查 | 2 | 2 | ✅ 完成 |
| 代码字段引用 | 5 | 5 | ✅ 完成 |
| 迁移配置 | 1 | 1 | ✅ 完成 |

**结论**: 所有 P0 和 P1 问题已修复完成，数据库 schema 与代码一致，编译验证通过。

---

## 参考文档

- [SKILL.md](../../skills/postgres-expert/SKILL.md): PostgreSQL 字段命名规范
- [UNIFIED_MIGRATION_v1.sql](../../migrations/UNIFIED_MIGRATION_v1.sql): 综合迁移脚本
- [00000000_unified_schema_v6.sql](../../migrations/00000000_unified_schema_v6.sql): 基础 Schema 定义
- [schema_health_check.rs](../../src/storage/schema_health_check.rs): Schema 健康检查
- [compile_time_validation.rs](../../src/storage/compile_time_validation.rs): 编译时验证模型
- [web/routes/mod.rs](../../src/web/routes/mod.rs): API 路由
- [services/identity/storage.rs](../../src/services/identity/storage.rs): Identity 存储
- [services/identity/models.rs](../../src/services/identity/models.rs): Identity 模型
- [storage/threepid.rs](../../src/storage/threepid.rs): Threepid 存储
