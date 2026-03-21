# synapse-rust 数据库字段映射一致性检查报告

> **检查日期**: 2026-03-21
> **项目**: synapse-rust (Matrix Homeserver)
> **状态**: 🟡 已修复，待验证

---

## 一、执行摘要

本次全面检查旨在发现数据库表字段名称与代码中引用字段名称的不一致问题。

### 1.1 问题统计

| 严重程度 | 数量 | 说明 |
|----------|------|------|
| 🔴 关键 | 1 | 导致运行时错误，测试失败 |
| 🔴 高 | 2 | 可能导致数据丢失或功能异常 |
| 🟡 中 | 2 | 可能导致潜在问题 |
| ✅ 已修复 | 1 | saml.rs processed_at NOW() 已修复 |
| 🟡 待应用 | 1 | 统一迁移脚本待执行 |

---

## 二、字段不一致问题详情

### 问题 1: events 表 - processed_at vs processed_ts 🔴 关键

**状态**: 🟡 待迁移

**描述**:
代码中 SQL 查询引用了 `events.processed_ts` 列，但该列在数据库中不存在（实际列为 `processed_at`）。

**修复方案**:
创建统一迁移脚本 `20260321000006_field_consistency_fix.sql`：
1. 将 `events.processed_ts` 数据迁移到 `processed_at`
2. 删除冗余的 `processed_ts` 列

**迁移文件**:
`migrations/20260321000006_field_consistency_fix.sql`

---

### 问题 2: saml_logout_requests 表 - processed_at vs processed_ts 🔴 高

**状态**: ✅ 已修复代码

**描述**:
代码中引用 `saml_logout_requests.processed_at`，但尝试使用 `NOW()` 函数赋值。

**已修复**:
修改 `src/storage/saml.rs:684-693`:
```rust
// 修复前
"UPDATE saml_logout_requests SET status = 'processed', processed_at = NOW() WHERE request_id = $1"

// 修复后
let now_ts = chrono::Utc::now().timestamp_millis();
sqlx::query(
    "UPDATE saml_logout_requests SET status = 'processed', processed_ts = $2 WHERE request_id = $1"
)
.bind(request_id)
.bind(now_ts)
```

**迁移配合**:
统一迁移脚本会将 `processed_at` 重命名为 `processed_ts` 以匹配代码。

---

### 问题 3: auth/mod.rs - access_token 未存储到数据库 🔴 高

**状态**: 🔴 待修复

**描述**:
`generate_access_token` 函数只生成了 JWT token，但没有调用 `token_storage.create_token()` 将 token 存储到数据库。

**相关文件**:
- `src/auth/mod.rs:698-725`

**修复方案**:
在 `generate_access_token` 函数中添加 token 存储调用

---

### 问题 4: storage/mod.rs - 测试代码引用不存在的字段 🟡 中

**状态**: 🟡 待确认

**描述**:
测试代码创建 `RoomEvent` 实体时引用了 `processed_ts` 字段，但 `RoomEvent` 结构体定义中不存在该字段。

**相关文件**:
- `src/storage/mod.rs:330-344`

---

### 问题 5: 缺失的表 🟡 中

**状态**: 🟡 待迁移

**描述**:
代码引用了多个数据库中不存在的表。

**缺失的表**:
1. `room_summary_update_queue`
2. `retention_cleanup_queue`
3. `space_events`

**修复方案**:
统一迁移脚本会创建这些缺失的表。

---

## 三、统一迁移脚本

### 3.1 迁移文件

**文件名**: `migrations/20260321000006_field_consistency_fix.sql`

**功能**:
1. 删除冗余的 `events.processed_ts` 列（数据迁移到 `processed_at`）
2. 重命名 `saml_logout_requests.processed_at` → `processed_ts`（匹配代码）
3. 创建缺失的表：
   - `room_summary_update_queue`
   - `retention_cleanup_queue`
   - `space_events`

### 3.2 执行迁移

```bash
# 复制迁移文件到容器
docker cp migrations/20260321000006_field_consistency_fix.sql docker-postgres:/tmp/

# 执行迁移
docker exec docker-postgres psql -U synapse -d synapse -f /tmp/20260321000006_field_consistency_fix.sql
```

### 3.3 迁移验证

```bash
# 验证 events 表
docker exec docker-postgres psql -U synapse -d synapse -c "SELECT column_name FROM information_schema.columns WHERE table_name = 'events' AND column_name LIKE '%processed%'"

# 验证 saml_logout_requests 表
docker exec docker-postgres psql -U synapse -d synapse -c "SELECT column_name FROM information_schema.columns WHERE table_name = 'saml_logout_requests' AND column_name LIKE '%processed%'"

# 验证新表
docker exec docker-postgres psql -U synapse -d synapse -c "SELECT table_name FROM information_schema.tables WHERE table_name IN ('room_summary_update_queue', 'retention_cleanup_queue', 'space_events')"
```

---

## 四、字段命名规范（参考）

### 4.1 时间戳字段

| 后缀 | 数据类型 | 说明 | 示例 |
|------|----------|------|------|
| `_ts` | BIGINT | NOT NULL 毫秒级时间戳 | `created_ts`, `updated_ts`, `last_seen_ts` |
| `_at` | BIGINT | 可选毫秒级时间戳 | `expires_at`, `revoked_at`, `validated_at` |

**注意**: `processed_at` 和 `processed_ts` 功能相同，统一使用 `processed_ts`

### 4.2 布尔字段

| 前缀 | 说明 | 示例 |
|------|------|------|
| `is_*` | 是否... | `is_admin`, `is_enabled`, `is_revoked` |
| `has_*` | 拥有... | `has_avatar`, `has_displayname` |

---

## 五、修复优先级建议

### 5.1 立即执行 (P0)

| # | 操作 | 说明 |
|---|------|------|
| 1 | 执行迁移脚本 | 添加缺失表，重命名列 |
| 2 | 验证迁移结果 | 确认所有表和列正确 |

### 5.2 近期修复 (P1)

| # | 问题 | 说明 |
|---|------|------|
| 1 | auth/mod.rs access_token 未存储 | 添加 token 存储调用 |
| 2 | storage/mod.rs 测试代码 | 修复测试代码 |

---

## 六、附录

### A. 相关文件列表

**迁移文件**:
- `migrations/20260321000006_field_consistency_fix.sql`

**修改的文件**:
- `src/storage/saml.rs` (已修复)

**待修改的文件**:
- `src/auth/mod.rs`
- `src/storage/mod.rs`

### B. 测试验证

```bash
# 运行数据库完整性测试
cd /Users/ljf/Desktop/hu/matrix-js-sdk
npx vitest run spec/integ/real-backend/database-integrity.test.ts --config vitest.real-backend.config.ts

# 运行 SDK 集成测试
npx tsx spec/integ/real-backend/step2-room.test.ts
npx tsx spec/integ/real-backend/step3-message.test.ts
```

---

*报告生成时间: 2026-03-21*
*检查工具: 自研脚本 + 人工审查*
*维护者: HuLa Team*
