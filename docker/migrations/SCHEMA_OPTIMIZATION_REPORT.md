# 数据库 Schema 排查报告 v6.0.0 → v6.1.0

## 状态: ✅ 已完成

## 问题排查结果

### 1. 数据冗余问题（重复表）

| 重复表 1 | 重复表 2 | 解决方案 | 状态 |
|---------|---------|---------|------|
| `user_threepids` | `threepids` | 保留 `user_threepids`，删除 `threepids` | ✅ |
| `event_reports` | `reports` | 保留 `event_reports`（功能更完整），删除 `reports` | ✅ |
| `thread_roots` | `thread_statistics` | 合并到 `thread_roots`（添加 participants 字段） | ✅ |

### 2. 字段冗余/重复

| 表 | 冗余字段 | 问题 | 解决方案 | 状态 |
|---|---------|------|---------|------|
| `notifications` | `is_read` 和 `read` | 语义完全重复 | 保留 `is_read`，删除 `read` | ✅ |
| `voice_usage_stats` | `last_activity_at` 和 `last_active_at` | 重复字段 | 保留 `last_active_ts` | ✅ |
| `rooms` | `member_count` | 与 `room_summaries.member_count` 冗余 | 删除，使用 room_summaries | ✅ |

### 3. 命名不一致问题

| 问题 | 原命名 | 统一后 | 状态 |
|-----|-------|-------|------|
| 时间字段混用 | 有的 `_ts`，有的 `_at`，有的 `ts` | NOT NULL: `_ts`，可空: `_at` | ✅ |
| 字段类型混用 | `VARCHAR(n)` / `TEXT` 混用 | 统一为 `TEXT` | ✅ |
| JSON 类型 | `TEXT` (JSON字符串) / `JSONB` | 统一为 `JSONB`（性能更好） | ✅ |
| 外键命名 | 不统一 | `{table}_id` 格式 | ✅ |

### 4. 修复的具体字段

| 原字段名 | 新字段名 | 所在表 | 状态 |
|---------|---------|--------|------|
| `password_changed_at` | `password_changed_ts` | users | ✅ |
| `password_expires_at` | `password_expires_ts` | users | ✅ |
| `joined_at` | `joined_ts` | room_memberships | ✅ |
| `invited_at` | `invited_ts` | room_memberships | ✅ |
| `left_at` | `left_ts` | room_memberships | ✅ |
| `banned_at` | `banned_ts` | room_memberships | ✅ |
| `validated_at` | `validated_ts` | user_threepids | ✅ |
| `verification_expires_at` | `verification_expires_ts` | user_threepids | ✅ |
| `last_activity_at` | `last_activity_ts` | rooms | ✅ |
| `last_activity_at` | `last_active_ts` | private_sessions | ✅ |
| `last_activity_at` | `last_active_ts` | voice_usage_stats | ✅ |

### 5. 外键约束问题

**修复**: 所有外键约束在 CREATE TABLE 时一并创建，确保数据一致性。

### 6. 索引优化

| 表 | 索引优化 | 状态 |
|---|---------|------|
| `users` | 条件索引 (WHERE email IS NOT NULL) | ✅ |
| `rooms` | 条件索引 + last_activity_ts 索引 | ✅ |
| `room_memberships` | 复合索引 (user_id, membership), (room_id, membership) | ✅ |
| `thread_roots` | 条件索引 (last_reply_ts) | ✅ |
| `token_blacklist` | 条件索引 (expires_ts) | ✅ |

## 代码修复汇总

### 修改的文件

1. **SQL Schema**
   - `migrations/00000000_unified_schema_v6.sql`

2. **Rust 模型**
   - `src/storage/models/user.rs` - UserThreepid 字段
   - `src/storage/models/membership.rs` - RoomMembership, PrivateSession 字段
   - `src/storage/models/push.rs` - Notification 字段
   - `src/storage/models/room.rs` - Room 字段

3. **Rust 存储层**
   - `src/storage/user.rs` - SQL 查询字段
   - `src/storage/threepid.rs` - SQL 查询和模型字段
   - `src/storage/mod.rs` - 测试代码

## 迁移说明

### v6.0.0 → v6.1.0 迁移需要执行以下 SQL:

```sql
-- 1. 移除重复表（如果存在）
DROP TABLE IF EXISTS threepids;
DROP TABLE IF EXISTS reports;
DROP TABLE IF EXISTS thread_statistics;

-- 2. 字段重命名（使用 ALTER TABLE）
ALTER TABLE users RENAME COLUMN password_changed_at TO password_changed_ts;
ALTER TABLE users RENAME COLUMN password_expires_at TO password_expires_ts;
ALTER TABLE room_memberships RENAME COLUMN joined_at TO joined_ts;
ALTER TABLE room_memberships RENAME COLUMN invited_at TO invited_ts;
ALTER TABLE room_memberships RENAME COLUMN left_at TO left_ts;
ALTER TABLE room_memberships RENAME COLUMN banned_at TO banned_ts;
ALTER TABLE user_threepids RENAME COLUMN validated_at TO validated_ts;
ALTER TABLE user_threepids RENAME COLUMN verification_expires_at TO verification_expires_ts;
ALTER TABLE rooms RENAME COLUMN last_activity_at TO last_activity_ts;
ALTER TABLE private_sessions RENAME COLUMN last_activity_at TO last_activity_ts;
ALTER TABLE voice_usage_stats RENAME COLUMN last_active_at TO last_active_ts;

-- 3. 移除冗余字段
ALTER TABLE rooms DROP COLUMN IF EXISTS member_count;
ALTER TABLE notifications DROP COLUMN IF EXISTS read;
ALTER TABLE voice_usage_stats DROP COLUMN IF EXISTS last_activity_at;

-- 4. 添加 participants 字段到 thread_roots
ALTER TABLE thread_roots ADD COLUMN IF NOT EXISTS participants JSONB DEFAULT '[]';

-- 5. 添加条件索引
CREATE INDEX IF NOT EXISTS idx_rooms_last_activity ON rooms(last_activity_ts DESC) WHERE last_activity_ts IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_room_memberships_joined ON room_memberships(user_id, room_id) WHERE membership = 'join';
CREATE INDEX IF NOT EXISTS idx_thread_roots_last_reply ON thread_roots(last_reply_ts DESC) WHERE last_reply_ts IS NOT NULL;
```

## 注意事项

1. **向后兼容**: 优化版本保持主要表结构兼容
2. **性能提升**: 
   - 条件索引减少存储空间
   - JSONB 统一提升 JSON 查询性能
   - 外键约束保障数据一致性
3. **代码一致性**: Rust 代码字段名与数据库字段名保持一致
