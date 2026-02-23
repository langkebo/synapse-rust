# 数据库脚本优化规范文档

## 一、项目概述

### 1.1 项目背景
Synapse-Rust 项目的数据库迁移脚本存在多个问题，导致服务启动失败和API测试失败。需要对数据库初始化脚本与迁移脚本进行全面系统性分析和优化。

### 1.2 优化目标
1. 解决数据库架构与代码不匹配问题
2. 消除迁移脚本中的冗余和冲突
3. 提升迁移脚本执行效率
4. 确保数据库结构的一致性、完整性和可维护性

## 二、问题分析

### 2.1 已识别问题清单

| 问题ID | 问题描述 | 影响范围 | 严重程度 |
|--------|----------|----------|----------|
| DB-001 | 迁移脚本中引用不存在的列 | 多个API | 高 |
| DB-002 | 表结构与代码期望不匹配 | 语音消息、推送通知、搜索API | 高 |
| DB-003 | 缺少必要的数据库表 | pushers, push_rules, room_members | 高 |
| DB-004 | 迁移脚本执行顺序问题 | 数据库初始化 | 中 |
| DB-005 | 迁移脚本中存在冗余定义 | 性能 | 低 |

### 2.2 具体问题详情

#### DB-001: 迁移脚本中引用不存在的列

**问题位置**:
- `20260213000007_create_refresh_tokens_tables.sql` - 引用 `token_hash`, `expires_at`, `is_revoked`
- `20260213000009_create_event_reports_tables.sql` - 引用 `reporter_user_id`, `reported_user_id`, `status`, `received_ts`
- `20260213000005_create_room_summaries_tables.sql` - 引用 `stream_id`

**错误信息**:
```
error returned from database: column "token_hash" does not exist
error returned from database: column "reporter_user_id" does not exist
error returned from database: column "stream_id" does not exist
```

#### DB-002: 表结构与代码期望不匹配

**voice_messages 表**:
- 代码期望: `processed_ts`, `mime_type`, `encryption`
- 实际存在: `processed_at` (名称不匹配)

**rooms 表**:
- 代码期望: `join_rules`
- 实际: 缺少此列

**events 表**:
- 代码期望: `type`
- 实际: 只有 `event_type`

#### DB-003: 缺少必要的数据库表

| 表名 | 用途 | 影响API |
|------|------|---------|
| `pushers` | 推送器注册 | 推送通知API |
| `push_rules` | 推送规则 | 推送通知API |
| `room_members` | 房间成员 | 搜索API |

#### DB-004: 迁移脚本执行顺序问题

当前迁移脚本按文件名排序执行，但存在依赖关系问题：
- 部分脚本在表创建前尝试创建索引
- 部分脚本引用尚未创建的表

#### DB-005: 迁移脚本中存在冗余定义

- `20260206000000_master_unified_schema.sql` 已定义大部分表
- 后续迁移脚本重复定义相同表结构

## 三、优化方案

### 3.1 脚本合并策略

创建统一的迁移脚本，按以下顺序执行：

1. **核心表** (Phase 1)
   - users, devices, access_tokens, refresh_tokens

2. **房间和成员表** (Phase 2)
   - rooms, room_memberships, room_members, room_aliases

3. **消息和事件表** (Phase 3)
   - events, event_contents, event_signatures

4. **E2EE加密表** (Phase 4)
   - device_keys, cross_signing_keys, megolm_sessions

5. **媒体和语音表** (Phase 5)
   - media_repository, voice_messages

6. **推送通知表** (Phase 6)
   - pushers, push_rules, push_notifications

7. **扩展功能表** (Phase 7)
   - spaces, threads, application_services

### 3.2 表结构修复

#### voice_messages 表修复

```sql
-- 添加缺失列
ALTER TABLE voice_messages ADD COLUMN IF NOT EXISTS processed_ts BIGINT;
ALTER TABLE voice_messages ADD COLUMN IF NOT EXISTS mime_type VARCHAR(100);
ALTER TABLE voice_messages ADD COLUMN IF NOT EXISTS encryption JSONB;

-- 数据迁移
UPDATE voice_messages SET processed_ts = processed_at WHERE processed_ts IS NULL AND processed_at IS NOT NULL;
```

#### rooms 表修复

```sql
ALTER TABLE rooms ADD COLUMN IF NOT EXISTS join_rules VARCHAR(50) DEFAULT 'invite';
ALTER TABLE rooms ADD COLUMN IF NOT EXISTS join_rules_event_id VARCHAR(255);
```

#### events 表修复

```sql
ALTER TABLE events ADD COLUMN IF NOT EXISTS type VARCHAR(255);
UPDATE events SET type = event_type WHERE type IS NULL;
```

### 3.3 缺失表创建

#### pushers 表

```sql
CREATE TABLE IF NOT EXISTS pushers (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    pushkey TEXT NOT NULL,
    kind VARCHAR(50) NOT NULL DEFAULT 'http',
    app_id VARCHAR(255) NOT NULL,
    app_display_name VARCHAR(255),
    device_display_name VARCHAR(255),
    profile_tag VARCHAR(255),
    lang VARCHAR(20) DEFAULT 'en',
    data JSONB DEFAULT '{}',
    enabled BOOLEAN DEFAULT true,
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    last_updated_ts BIGINT,
    CONSTRAINT pushers_user_pushkey_unique UNIQUE(user_id, pushkey)
);
```

#### push_rules 表

```sql
CREATE TABLE IF NOT EXISTS push_rules (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    rule_id VARCHAR(255) NOT NULL,
    scope VARCHAR(50) NOT NULL DEFAULT 'global',
    kind VARCHAR(50) NOT NULL,
    priority INTEGER DEFAULT 0,
    conditions JSONB DEFAULT '[]',
    actions JSONB DEFAULT '[]',
    enabled BOOLEAN DEFAULT true,
    is_default BOOLEAN DEFAULT false,
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    CONSTRAINT push_rules_user_rule_unique UNIQUE(user_id, scope, kind, rule_id)
);
```

#### room_members 表

```sql
CREATE TABLE IF NOT EXISTS room_members (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    membership VARCHAR(50) NOT NULL DEFAULT 'join',
    displayname VARCHAR(255),
    avatar_url VARCHAR(512),
    event_id VARCHAR(255),
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    CONSTRAINT room_members_room_user_unique UNIQUE(room_id, user_id)
);
```

### 3.4 迁移脚本优化

#### 优化原则

1. **幂等性**: 所有迁移脚本可重复执行
2. **原子性**: 每个迁移在事务中执行
3. **依赖管理**: 确保表创建在索引创建之前
4. **错误处理**: 使用 `IF NOT EXISTS` 和 `IF EXISTS` 避免重复创建错误

#### 新迁移脚本结构

```
migrations/
├── 00000000000000_initial_schema.sql        # 核心表
├── 00000000000001_rooms_and_members.sql     # 房间和成员
├── 00000000000002_events.sql                # 事件
├── 00000000000003_e2ee_keys.sql            # E2EE密钥
├── 00000000000004_media_and_voice.sql       # 媒体和语音
├── 00000000000005_push_notifications.sql    # 推送通知
├── 00000000000006_extensions.sql            # 扩展功能
└── 00000000000007_indexes.sql               # 性能索引
```

## 四、实施步骤

### 4.1 准备阶段

1. 备份现有数据库
2. 分析现有表结构
3. 识别所有缺失和冲突

### 4.2 实施阶段

1. 创建新的统一迁移脚本
2. 删除旧的冲突迁移脚本
3. 执行新迁移脚本
4. 验证表结构完整性

### 4.3 验证阶段

1. 执行所有API测试
2. 验证服务启动
3. 检查日志无错误

## 五、回滚机制

### 5.1 数据库备份

```bash
# 备份命令
docker compose exec db pg_dump -U synapse synapse_test > backup_$(date +%Y%m%d_%H%M%S).sql

# 恢复命令
cat backup_YYYYMMDD_HHMMSS.sql | docker compose exec -T db psql -U synapse synapse_test
```

### 5.2 回滚步骤

1. 停止服务
2. 恢复数据库备份
3. 恢复旧迁移脚本
4. 重启服务

## 六、测试验证策略

### 6.1 单元测试

- 验证每个表的列完整性
- 验证外键约束
- 验证索引创建

### 6.2 集成测试

- 执行所有API测试用例
- 验证服务启动无错误
- 验证日志无警告

### 6.3 性能测试

- 迁移脚本执行时间
- 数据库查询性能
- 服务响应时间

## 七、预期成果

1. 服务正常启动，无健康检查失败
2. 所有API测试通过
3. 迁移脚本执行无错误和警告
4. 数据库结构完整且一致
