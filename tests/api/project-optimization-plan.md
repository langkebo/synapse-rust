# Synapse-Rust 项目优化方案

> **文档版本**: 1.0  
> **创建时间**: 2026-02-12  
> **项目状态**: 测试完成度 50.8% (90/177)  
> **待处理问题**: 66 个端点待测试/实现，21 个问题需修复

---

## 目录

1. [项目现状分析](#1-项目现状分析)
2. [优化目标与验收标准](#2-优化目标与验收标准)
3. [功能模块实现计划](#3-功能模块实现计划)
4. [数据库迁移优化方案](#4-数据库迁移优化方案)
5. [权限控制与边界条件处理](#5-权限控制与边界条件处理)
6. [测试计划](#6-测试计划)
7. [实施时间表](#7-实施时间表)
8. [风险评估与应对策略](#8-风险评估与应对策略)

---

## 1. 项目现状分析

### 1.1 测试统计概览

| 模块 | 总端点数 | 已测试 | 通过 | 需修复 | 待测试 |
|------|----------|--------|------|--------|--------|
| 核心客户端 API | 62 | 62 | 50 | 12 | 0 |
| 管理员 API | 27 | 5 | 1 | 4 | 22 |
| 联邦通信 API | 39 | 0 | 0 | 0 | 39 |
| 好友系统 API | 11 | 0 | 0 | 0 | 11 |
| 端到端加密 API | 6 | 0 | 0 | 0 | 6 |
| 媒体文件 API | 8 | 0 | 0 | 0 | 8 |
| 语音消息 API | 10 | 10 | 7 | 3 | 0 |
| VoIP API | 3 | 3 | 1 | 2 | 0 |
| 密钥备份 API | 11 | 10 | 10 | 0 | 1 |
| **总计** | **177** | **90** | **69** | **21** | **66** |

### 1.2 问题分类统计

| 问题类型 | 数量 | 优先级分布 |
|----------|------|------------|
| 端点未实现 | 49 | 高: 15, 中: 25, 低: 9 |
| 数据库约束/结构问题 | 3 | 高: 3 |
| 文件系统权限问题 | 1 | 高: 1 |
| 认证问题 | 2 | 中: 2 |
| 事件类型错误 | 1 | 高: 1 |
| 状态事件查询问题 | 1 | 高: 1 |
| 服务未配置 | 2 | 中: 2 |
| **总计** | **59** | **高: 21, 中: 31, 低: 7** |

### 1.3 关键问题清单

#### 高优先级问题 (需立即处理)

| 序号 | 问题 | 端点 | 影响 |
|------|------|------|------|
| 1 | 数据库约束缺失 | `/_matrix/client/r0/keys/upload` | E2EE 密钥上传失败 |
| 2 | 文件系统权限错误 | `/_matrix/media/v3/upload` | 媒体上传失败 |
| 3 | 数据库列缺失 | `voice_messages` 表 | 语音消息功能不可用 |
| 4 | 状态事件存储错误 | `state/{event_type}/{state_key}` | 房间状态查询失败 |
| 5 | 事件类型前缀错误 | `state/{event_type}` POST | 状态事件类型错误 |
| 6 | 通过别名加入房间未实现 | `join/{room_id_or_alias}` | 核心功能缺失 |
| 7 | 联邦服务器发现未实现 | `/.well-known/matrix/server` | 联邦通信不可用 |

---

## 2. 优化目标与验收标准

### 2.1 总体目标

1. **功能完整性**: 实现 100% 的 API 端点 (177/177)
2. **测试覆盖率**: 所有端点测试通过率 ≥ 95%
3. **代码质量**: 无编译错误、无警告、无安全漏洞
4. **文档一致性**: API 文档与实现完全一致

### 2.2 验收标准

| 阶段 | 验收标准 | 指标 |
|------|----------|------|
| 第一阶段 | 高优先级问题全部修复 | 21 个问题解决 |
| 第二阶段 | 核心功能端点全部实现 | 62 个端点可用 |
| 第三阶段 | 管理员 API 全部实现 | 27 个端点可用 |
| 第四阶段 | 联邦通信 API 全部实现 | 39 个端点可用 |
| 第五阶段 | 扩展功能 API 全部实现 | 49 个端点可用 |
| 最终验收 | 所有测试通过 | 通过率 ≥ 95% |

---

## 3. 功能模块实现计划

### 3.1 第一阶段: 高优先级问题修复 (预计 5 天)

#### 3.1.1 数据库问题修复

**任务 1: 修复 E2EE 密钥上传约束**

| 项目 | 内容 |
|------|------|
| **问题描述** | `keys/upload` 端点报错: `there is no unique or exclusion constraint matching the ON CONFLICT specification` |
| **影响范围** | 端到端加密功能 |
| **解决方案** | 添加唯一约束到 `e2ee_device_keys` 表 |
| **实施步骤** | 1. 分析现有表结构<br>2. 创建迁移脚本添加约束<br>3. 更新 Rust 代码使用正确的 UPSERT 语法 |
| **验收标准** | 密钥上传返回 200，数据正确存储 |
| **预计时间** | 1 天 |

**迁移脚本示例**:
```sql
-- 20260213000000_fix_e2ee_keys_constraint.sql
ALTER TABLE e2ee_device_keys 
ADD CONSTRAINT e2ee_device_keys_user_device_unique 
UNIQUE (user_id, device_id);

-- 更新现有数据
UPDATE e2ee_device_keys 
SET updated_ts = EXTRACT(EPOCH FROM NOW()) * 1000 
WHERE updated_ts IS NULL;
```

**任务 2: 修复语音消息数据库列缺失**

| 项目 | 内容 |
|------|------|
| **问题描述** | `voice_messages` 表缺少 `processed` 列 |
| **影响范围** | 语音消息获取功能 |
| **解决方案** | 添加缺失列并设置默认值 |
| **实施步骤** | 1. 创建迁移脚本<br>2. 更新 Rust 模型定义<br>3. 更新查询逻辑 |
| **验收标准** | 语音消息获取返回正确数据 |
| **预计时间** | 0.5 天 |

**迁移脚本示例**:
```sql
-- 20260213000001_fix_voice_messages_column.sql
ALTER TABLE voice_messages 
ADD COLUMN IF NOT EXISTS processed BOOLEAN DEFAULT FALSE;

ALTER TABLE voice_messages 
ADD COLUMN IF NOT EXISTS processed_at BIGINT;

UPDATE voice_messages 
SET processed = TRUE, processed_at = created_at 
WHERE processed IS NULL;
```

#### 3.1.2 文件系统权限修复

**任务 3: 修复媒体上传权限**

| 项目 | 内容 |
|------|------|
| **问题描述** | 媒体上传失败: `Permission denied (os error 13)` |
| **影响范围** | 所有媒体上传功能 |
| **解决方案** | 配置正确的目录权限和所有权 |
| **实施步骤** | 1. 检查媒体存储目录配置<br>2. 创建目录并设置权限<br>3. 更新 Docker 配置 |
| **验收标准** | 媒体上传返回 200，文件正确存储 |
| **预计时间** | 0.5 天 |

**修复命令**:
```bash
# 创建媒体目录
mkdir -p /var/lib/synapse/media
mkdir -p /var/lib/synapse/media/thumbnails
mkdir -p /var/lib/synapse/media/quarantine

# 设置权限
chown -R synapse:synapse /var/lib/synapse/media
chmod -R 755 /var/lib/synapse/media
```

#### 3.1.3 核心功能端点实现

**任务 4: 实现通过别名加入房间**

| 项目 | 内容 |
|------|------|
| **端点** | `POST /_matrix/client/r0/join/{room_id_or_alias}` |
| **规范参考** | [Matrix Client-Server API - Join](https://spec.matrix.org/v1.2/client-server-api/#post_matrixclientr0joinroomidoralias) |
| **实施步骤** | 1. 创建路由处理函数<br>2. 实现房间别名解析<br>3. 实现加入房间逻辑<br>4. 添加权限检查 |
| **验收标准** | 用户可通过别名加入房间 |
| **预计时间** | 1 天 |

**Rust 实现框架**:
```rust
pub async fn join_room_by_alias(
    State(state): State<AppState>,
    Path(room_id_or_alias): Path<String>,
    Extension(user_id): Extension<String>,
    Json(body): Json<JoinRoomRequest>,
) -> Result<Json<JoinRoomResponse>, ApiError> {
    let room_id = if room_id_or_alias.starts_with('!') {
        room_id_or_alias
    } else if room_id_or_alias.starts_with('#') {
        resolve_room_alias(&state, &room_id_or_alias).await?
    } else {
        return Err(ApiError::bad_json("Invalid room ID or alias"));
    };
    
    let server_names = body.via.unwrap_or_default();
    join_room(&state, &user_id, &room_id, server_names).await?;
    
    Ok(Json(JoinRoomResponse { room_id }))
}
```

**任务 5: 实现忘记房间端点**

| 项目 | 内容 |
|------|------|
| **端点** | `POST /_matrix/client/r0/rooms/{room_id}/forget` |
| **规范参考** | [Matrix Client-Server API - Forget](https://spec.matrix.org/v1.2/client-server-api/#post_matrixclientr0roomsroomidforget) |
| **实施步骤** | 1. 验证用户已离开房间<br>2. 删除房间成员记录<br>3. 清理本地缓存 |
| **验收标准** | 用户可忘记已离开的房间 |
| **预计时间** | 0.5 天 |

#### 3.1.4 状态事件处理修复

**任务 6: 修复状态事件存储**

| 项目 | 内容 |
|------|------|
| **问题描述** | GET `state/{event_type}/{state_key}` 返回 404 |
| **根本原因** | 状态事件存储时 `state_key` 字段未正确设置 |
| **解决方案** | 修复事件存储逻辑，确保 `state_key` 正确保存 |
| **验收标准** | 状态事件查询返回正确数据 |
| **预计时间** | 1 天 |

**任务 7: 修复事件类型前缀错误**

| 项目 | 内容 |
|------|------|
| **问题描述** | POST 设置状态事件时类型被错误添加 "m.room." 前缀 |
| **示例** | 发送 "member" 被存储为 "m.room.member" |
| **解决方案** | 修改事件类型处理逻辑，避免重复添加前缀 |
| **验收标准** | 事件类型与请求一致 |
| **预计时间** | 0.5 天 |

### 3.2 第二阶段: 管理员 API 实现 (预计 7 天)

#### 3.2.1 用户管理端点

| 序号 | 端点 | 方法 | 功能 | 优先级 | 预计时间 |
|------|------|------|------|--------|----------|
| 1 | `/_synapse/admin/v2/users` | GET | v2 用户列表 | 低 | 0.5 天 |
| 2 | `/_synapse/admin/v2/users/{user_id}` | GET | v2 用户信息 | 低 | 0.5 天 |
| 3 | `/_synapse/admin/v2/users/{user_id}` | PUT | 创建/更新用户 | 中 | 1 天 |
| 4 | `/_synapse/admin/v1/users/{user_id}/login` | POST | 登录为用户 | 中 | 1 天 |
| 5 | `/_synapse/admin/v1/users/{user_id}/logout` | POST | 登出用户设备 | 中 | 0.5 天 |
| 6 | `/_synapse/admin/v1/users/{user_id}/devices` | GET | 获取用户设备 | 中 | 0.5 天 |
| 7 | `/_synapse/admin/v1/users/{user_id}/devices/{device_id}` | DELETE | 删除用户设备 | 中 | 0.5 天 |
| 8 | `/_synapse/admin/v1/users/{user_id}/media` | GET | 获取用户媒体 | 低 | 0.5 天 |
| 9 | `/_synapse/admin/v1/users/{user_id}/media` | DELETE | 删除用户媒体 | 低 | 0.5 天 |

**实现参考 (官方文档)**:
- 用户管理 API: `GET /_synapse/admin/v2/users/<user_id>`
- 创建用户: `PUT /_synapse/admin/v2/users/<user_id>`
- 重置密码: `POST /_synapse/admin/v1/reset_password/<user_id>`

#### 3.2.2 房间管理端点

| 序号 | 端点 | 方法 | 功能 | 优先级 | 预计时间 |
|------|------|------|------|--------|----------|
| 1 | `/_synapse/admin/v1/rooms/{room_id}/members` | GET | 获取房间成员 | 中 | 0.5 天 |
| 2 | `/_synapse/admin/v1/rooms/{room_id}/state` | GET | 获取房间状态 | 中 | 0.5 天 |
| 3 | `/_synapse/admin/v1/rooms/{room_id}/messages` | GET | 获取房间消息 | 中 | 0.5 天 |
| 4 | `/_synapse/admin/v1/rooms/{room_id}/join` | POST | 管理员加入房间 | 中 | 0.5 天 |
| 5 | `/_synapse/admin/v1/join/{room_id_or_alias}` | POST | 加入房间 | 中 | 0.5 天 |
| 6 | `/_synapse/admin/v1/rooms/{room_id}/block` | POST | 封锁房间 | 中 | 0.5 天 |
| 7 | `/_synapse/admin/v1/rooms/{room_id}/block` | GET | 获取封锁状态 | 低 | 0.5 天 |
| 8 | `/_synapse/admin/v1/rooms/{room_id}/make_admin` | POST | 设置房间管理员 | 中 | 0.5 天 |

#### 3.2.3 服务器管理端点

| 序号 | 端点 | 方法 | 功能 | 优先级 | 预计时间 |
|------|------|------|------|--------|----------|
| 1 | `/_synapse/admin/v1/purge_media_cache` | POST | 清理媒体缓存 | 低 | 0.5 天 |
| 2 | `/_synapse/admin/v1/statistics` | GET | 获取统计信息 | 中 | 0.5 天 |
| 3 | `/_synapse/admin/v1/background_updates` | GET | 后台更新状态 | 低 | 0.5 天 |
| 4 | `/_synapse/admin/v1/background_updates/{job_name}` | POST | 执行后台更新 | 低 | 0.5 天 |

### 3.3 第三阶段: 联邦通信 API 实现 (预计 10 天)

#### 3.3.1 服务器发现端点

| 序号 | 端点 | 方法 | 功能 | 优先级 | 预计时间 |
|------|------|------|------|--------|----------|
| 1 | `/.well-known/matrix/server` | GET | 获取服务器信息 | 高 | 1 天 |
| 2 | `/_matrix/key/v2/server/{key_id}` | GET | 获取指定密钥 | 中 | 1 天 |

**实现要点**:
- 服务器发现是联邦通信的基础，必须优先实现
- 需要正确配置 `server_name` 和签名密钥
- 参考官方文档: [Delegation](https://element-hq.github.io/synapse/latest/delegate.html)

#### 3.3.2 事件查询端点

| 序号 | 端点 | 方法 | 功能 | 优先级 | 预计时间 |
|------|------|------|------|--------|----------|
| 1 | `/_matrix/federation/v1/event/{event_id}` | GET | 获取事件 | 高 | 1 天 |
| 2 | `/_matrix/federation/v1/state/{room_id}` | GET | 获取房间状态 | 高 | 1 天 |
| 3 | `/_matrix/federation/v1/state_ids/{room_id}` | GET | 获取状态ID列表 | 中 | 0.5 天 |
| 4 | `/_matrix/federation/v1/backfill/{room_id}` | GET | 回填事件 | 中 | 1 天 |

#### 3.3.3 其他联邦端点

| 序号 | 端点 | 方法 | 功能 | 优先级 | 预计时间 |
|------|------|------|------|--------|----------|
| 1 | `/_matrix/federation/v1/exchange_third_party_invite/{room_id}` | POST | 交换第三方邀请 | 低 | 0.5 天 |
| 2 | `/_matrix/federation/v1/on_bind_third_party_invite/{room_id}` | POST | 绑定第三方邀请 | 低 | 0.5 天 |
| 3 | `/_matrix/federation/v1/3pid/onbind` | POST | 第三方ID绑定 | 低 | 0.5 天 |
| 4 | `/_matrix/federation/v1/sendToDevice/{txn_id}` | PUT | 发送到设备 | 中 | 1 天 |

### 3.4 第四阶段: 扩展功能 API 实现 (预计 8 天)

#### 3.4.1 好友系统 API

| 序号 | 端点 | 方法 | 功能 | 优先级 | 预计时间 |
|------|------|------|------|--------|----------|
| 1 | `/_matrix/client/v1/friends` | GET | 获取好友列表 | 中 | 0.5 天 |
| 2 | `/_matrix/client/v1/friends` | POST | 添加好友 | 中 | 0.5 天 |
| 3 | `/_matrix/client/v1/friends/{user_id}` | DELETE | 删除好友 | 中 | 0.5 天 |
| 4 | `/_matrix/client/v1/friends/requests` | GET | 获取好友请求 | 中 | 0.5 天 |
| 5 | `/_matrix/client/v1/friends/requests/{user_id}` | PUT | 处理好友请求 | 中 | 0.5 天 |
| 6 | `/_matrix/client/v1/friends/blocked` | GET | 获取黑名单 | 低 | 0.5 天 |

#### 3.4.2 端到端加密 API

| 序号 | 端点 | 方法 | 功能 | 优先级 | 预计时间 |
|------|------|------|------|--------|----------|
| 1 | `/_matrix/client/r0/keys/upload` | POST | 上传密钥 | 高 | 1 天 |
| 2 | `/_matrix/client/r0/keys/query` | POST | 查询密钥 | 高 | 1 天 |
| 3 | `/_matrix/client/r0/keys/claim` | POST | 声明密钥 | 高 | 1 天 |
| 4 | `/_matrix/client/r0/keys/changes` | GET | 获取密钥变更 | 中 | 0.5 天 |
| 5 | `/_matrix/client/r0/deviceoneTimeKeys/sign:{user_id}:{device_id}:{key_id}` | POST | 上传签名密钥 | 中 | 1 天 |

#### 3.4.3 媒体文件 API

| 序号 | 端点 | 方法 | 功能 | 优先级 | 预计时间 |
|------|------|------|------|--------|----------|
| 1 | `/_matrix/media/v3/download/{server_name}/{media_id}/{filename}` | GET | 指定文件名下载 | 低 | 0.5 天 |
| 2 | `/_matrix/media/r0/preview_url` | GET | URL预览 | 低 | 1 天 |
| 3 | `/_matrix/media/r0/unstable/info/{server_name}/{media_id}` | GET | 媒体信息 | 低 | 0.5 天 |
| 4 | `/_matrix/media/r0/unstable/config` | GET | 不稳定配置 | 低 | 0.5 天 |

---

## 4. 数据库迁移优化方案

### 4.1 现有迁移脚本分析

| 脚本名称 | 状态 | 问题 |
|----------|------|------|
| `20260206000000_master_unified_schema.sql` | ✅ 已执行 | 基础表结构完整 |
| `20260209100000_add_performance_indexes.sql` | ✅ 已执行 | 性能索引已添加 |
| `20260211000000_cleanup_legacy_friends.sql` | ⚠️ 需验证 | 清理旧数据 |
| `20260211000001_migrate_friends_to_rooms.sql` | ⚠️ 需验证 | 好友迁移 |
| `20260211000002_validate_friend_migration.sql` | ⚠️ 需验证 | 迁移验证 |
| `20260212000000_emergency_fix.sql` | ✅ 已执行 | 紧急修复 |

### 4.2 新增迁移脚本计划

#### 4.2.1 E2EE 密钥约束修复

```sql
-- 文件: migrations/20260213000000_fix_e2ee_keys_constraint.sql
-- 版本: 1.0
-- 描述: 修复端到端加密密钥表约束问题

BEGIN;

-- 检查表是否存在
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'e2ee_device_keys') THEN
        CREATE TABLE e2ee_device_keys (
            id BIGSERIAL PRIMARY KEY,
            user_id VARCHAR(255) NOT NULL,
            device_id VARCHAR(255) NOT NULL,
            key_data JSONB NOT NULL,
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT,
            UNIQUE(user_id, device_id)
        );
    END IF;
END $$;

-- 添加唯一约束（如果不存在）
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint 
        WHERE conname = 'e2ee_device_keys_user_device_unique'
    ) THEN
        ALTER TABLE e2ee_device_keys 
        ADD CONSTRAINT e2ee_device_keys_user_device_unique 
        UNIQUE (user_id, device_id);
    END IF;
END $$;

-- 创建索引
CREATE INDEX IF NOT EXISTS idx_e2ee_keys_user ON e2ee_device_keys(user_id);
CREATE INDEX IF NOT EXISTS idx_e2ee_keys_device ON e2ee_device_keys(device_id);

COMMIT;
```

#### 4.2.2 语音消息表修复

```sql
-- 文件: migrations/20260213000001_fix_voice_messages_table.sql
-- 版本: 1.0
-- 描述: 修复语音消息表缺失列问题

BEGIN;

-- 添加缺失列
ALTER TABLE voice_messages 
ADD COLUMN IF NOT EXISTS processed BOOLEAN DEFAULT FALSE;

ALTER TABLE voice_messages 
ADD COLUMN IF NOT EXISTS processed_at BIGINT;

ALTER TABLE voice_messages 
ADD COLUMN IF NOT EXISTS duration_seconds INTEGER;

ALTER TABLE voice_messages 
ADD COLUMN IF NOT EXISTS sample_rate INTEGER;

ALTER TABLE voice_messages 
ADD COLUMN IF NOT EXISTS channels INTEGER DEFAULT 1;

-- 更新现有数据
UPDATE voice_messages 
SET 
    processed = TRUE,
    processed_at = created_at,
    duration_seconds = EXTRACT(EPOCH FROM INTERVAL '1 minute')::INTEGER
WHERE processed IS NULL;

-- 创建索引
CREATE INDEX IF NOT EXISTS idx_voice_processed ON voice_messages(processed);
CREATE INDEX IF NOT EXISTS idx_voice_user_processed ON voice_messages(user_id, processed);

COMMIT;
```

#### 4.2.3 状态事件表优化

```sql
-- 文件: migrations/20260213000002_fix_state_events_table.sql
-- 版本: 1.0
-- 描述: 修复状态事件存储问题

BEGIN;

-- 确保状态事件表结构正确
ALTER TABLE room_events 
ADD COLUMN IF NOT EXISTS state_key TEXT DEFAULT '';

-- 创建状态事件索引
CREATE INDEX IF NOT EXISTS idx_room_events_state_key 
ON room_events(room_id, event_type, state_key);

-- 创建复合索引用于快速查询
CREATE INDEX IF NOT EXISTS idx_room_events_type_state 
ON room_events(room_id, event_type, state_key) 
WHERE state_key IS NOT NULL AND state_key != '';

COMMIT;
```

### 4.3 迁移执行策略

#### 4.3.1 执行前检查

```bash
#!/bin/bash
# 迁移前检查脚本

echo "=== 数据库迁移前检查 ==="

# 1. 检查数据库连接
echo "检查数据库连接..."
docker exec synapse-postgres pg_isready -U synapse

# 2. 备份当前数据库
echo "创建数据库备份..."
docker exec synapse-postgres pg_dump -U synapse synapse_test > backup_$(date +%Y%m%d_%H%M%S).sql

# 3. 检查磁盘空间
echo "检查磁盘空间..."
df -h /var/lib/docker

# 4. 检查表状态
echo "检查关键表状态..."
docker exec synapse-postgres psql -U synapse -d synapse_test -c "
SELECT table_name, 
       pg_size_pretty(pg_total_relation_size(table_name::text)) as size
FROM information_schema.tables 
WHERE table_schema = 'public' 
ORDER BY pg_total_relation_size(table_name::text) DESC 
LIMIT 10;
"
```

#### 4.3.2 迁移执行流程

```bash
#!/bin/bash
# 迁移执行脚本

MIGRATIONS_DIR="/home/hula/synapse_rust/synapse/migrations"
DB_CONTAINER="synapse-postgres"
DB_NAME="synapse_test"
DB_USER="synapse"

# 按顺序执行迁移
for migration in $(ls -1 $MIGRATIONS_DIR/*.sql | sort); do
    filename=$(basename "$migration")
    echo "执行迁移: $filename"
    
    # 复制迁移文件到容器
    docker cp "$migration" "$DB_CONTAINER:/tmp/$filename"
    
    # 执行迁移
    docker exec "$DB_CONTAINER" psql -U "$DB_USER" -d "$DB_NAME" -f "/tmp/$filename"
    
    if [ $? -eq 0 ]; then
        echo "✅ $filename 执行成功"
    else
        echo "❌ $filename 执行失败"
        exit 1
    fi
done

echo "=== 迁移完成 ==="
```

#### 4.3.3 迁移后验证

```sql
-- 验证脚本: verify_migration.sql

-- 1. 验证 E2EE 密钥表约束
SELECT conname, contype 
FROM pg_constraint 
WHERE conrelid = 'e2ee_device_keys'::regclass;

-- 2. 验证语音消息表结构
SELECT column_name, data_type, is_nullable, column_default
FROM information_schema.columns 
WHERE table_name = 'voice_messages' 
ORDER BY ordinal_position;

-- 3. 验证状态事件索引
SELECT indexname, indexdef 
FROM pg_indexes 
WHERE tablename = 'room_events';

-- 4. 验证数据完整性
SELECT 
    (SELECT COUNT(*) FROM users) as users_count,
    (SELECT COUNT(*) FROM devices) as devices_count,
    (SELECT COUNT(*) FROM access_tokens WHERE invalidated_ts IS NULL) as active_tokens,
    (SELECT COUNT(*) FROM room_events) as events_count;
```

---

## 5. 权限控制与边界条件处理

### 5.1 认证中间件优化

#### 5.1.1 当前问题

| 问题 | 端点 | 影响 |
|------|------|------|
| 无认证可访问 | `/_matrix/client/r0/rooms/{room_id}/state` | 安全风险 |
| 需要认证但不应需要 | `/_matrix/client/r0/publicRooms` | 用户体验问题 |

#### 5.1.2 解决方案

```rust
// 文件: src/web/middleware/auth.rs

pub async fn auth_middleware(
    State(config): State<AuthConfig>,
    mut req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let path = req.uri().path();
    
    // 公开端点列表（无需认证）
    let public_paths = [
        "/_matrix/client/versions",
        "/_matrix/client/r0/publicRooms",
        "/.well-known/matrix/server",
        "/.well-known/matrix/client",
        "/health",
        "/_synapse/admin/v1/server_version",
    ];
    
    // 检查是否为公开路径
    if public_paths.iter().any(|p| path.starts_with(p)) {
        return Ok(next.run(req).await);
    }
    
    // 检查是否为联邦请求
    if path.starts_with("/_matrix/federation/") {
        return verify_federation_signature(req, next).await;
    }
    
    // 检查是否为管理员端点
    if path.starts_with("/_synapse/admin/") {
        return verify_admin_auth(req, next).await;
    }
    
    // 标准用户认证
    verify_user_auth(req, next).await
}

async fn verify_admin_auth(
    mut req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let user_id = req.extensions().get::<String>()
        .ok_or(ApiError::unauthorized("Missing user context"))?;
    
    let is_admin = check_admin_status(&user_id).await?;
    
    if !is_admin {
        return Err(ApiError::forbidden("Admin access required"));
    }
    
    Ok(next.run(req).await)
}
```

### 5.2 边界条件处理

#### 5.2.1 输入验证

```rust
// 文件: src/web/validators/mod.rs

pub fn validate_room_id(room_id: &str) -> Result<(), ApiError> {
    if room_id.is_empty() {
        return Err(ApiError::bad_json("Room ID cannot be empty"));
    }
    
    if !room_id.starts_with('!') {
        return Err(ApiError::bad_json("Room ID must start with '!'"));
    }
    
    if room_id.len() > 255 {
        return Err(ApiError::bad_json("Room ID too long (max 255 characters)"));
    }
    
    let parts: Vec<&str> = room_id[1..].splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(ApiError::bad_json("Invalid room ID format"));
    }
    
    Ok(())
}

pub fn validate_user_id(user_id: &str) -> Result<(), ApiError> {
    if user_id.is_empty() {
        return Err(ApiError::bad_json("User ID cannot be empty"));
    }
    
    if !user_id.starts_with('@') {
        return Err(ApiError::bad_json("User ID must start with '@'"));
    }
    
    if user_id.len() > 255 {
        return Err(ApiError::bad_json("User ID too long (max 255 characters)"));
    }
    
    let parts: Vec<&str> = user_id[1..].splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(ApiError::bad_json("Invalid user ID format"));
    }
    
    Ok(())
}

pub fn validate_event_type(event_type: &str) -> Result<(), ApiError> {
    if event_type.is_empty() {
        return Err(ApiError::bad_json("Event type cannot be empty"));
    }
    
    if event_type.len() > 255 {
        return Err(ApiError::bad_json("Event type too long (max 255 characters)"));
    }
    
    // 验证事件类型格式
    let valid_pattern = regex::Regex::new(r"^[a-zA-Z0-9._-]+(\.[a-zA-Z0-9._-]+)*$").unwrap();
    if !valid_pattern.is_match(event_type) {
        return Err(ApiError::bad_json("Invalid event type format"));
    }
    
    Ok(())
}
```

#### 5.2.2 速率限制

```rust
// 文件: src/web/middleware/rate_limit.rs

use std::time::{Duration, Instant};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct RateLimiter {
    requests: Arc<RwLock<HashMap<String, Vec<Instant>>>>,
    max_requests: usize,
    window: Duration,
}

impl RateLimiter {
    pub fn new(max_requests: usize, window: Duration) -> Self {
        Self {
            requests: Arc::new(RwLock::new(HashMap::new())),
            max_requests,
            window,
        }
    }
    
    pub async fn check(&self, key: &str) -> Result<(), ApiError> {
        let mut requests = self.requests.write().await;
        let now = Instant::now();
        
        let entry = requests.entry(key.to_string()).or_insert_with(Vec::new);
        
        // 清理过期请求
        entry.retain(|&t| now.duration_since(t) < self.window);
        
        if entry.len() >= self.max_requests {
            return Err(ApiError::rate_limited(
                "Too many requests. Please wait before trying again.",
                self.window.as_secs() as u64
            ));
        }
        
        entry.push(now);
        Ok(())
    }
}

// 配置示例
pub fn configure_rate_limits() -> HashMap<String, RateLimiter> {
    let mut limits = HashMap::new();
    
    // 登录限制: 每分钟 5 次
    limits.insert(
        "login".to_string(),
        RateLimiter::new(5, Duration::from_secs(60))
    );
    
    // 注册限制: 每小时 3 次
    limits.insert(
        "register".to_string(),
        RateLimiter::new(3, Duration::from_secs(3600))
    );
    
    // 消息发送限制: 每秒 10 条
    limits.insert(
        "message".to_string(),
        RateLimiter::new(10, Duration::from_secs(1))
    );
    
    limits
}
```

### 5.3 错误处理标准化

```rust
// 文件: src/web/error.rs

use serde_json::json;
use axum::http::StatusCode;

#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    Unauthorized(String),
    Forbidden(String),
    NotFound(String),
    Conflict(String),
    RateLimited { message: String, retry_after: u64 },
    InternalError(String),
}

impl ApiError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ApiError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            ApiError::Forbidden(_) => StatusCode::FORBIDDEN,
            ApiError::NotFound(_) => StatusCode::NOT_FOUND,
            ApiError::Conflict(_) => StatusCode::CONFLICT,
            ApiError::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
            ApiError::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
    
    pub fn error_code(&self) -> &str {
        match self {
            ApiError::BadRequest(_) => "M_BAD_JSON",
            ApiError::Unauthorized(_) => "M_UNKNOWN_TOKEN",
            ApiError::Forbidden(_) => "M_FORBIDDEN",
            ApiError::NotFound(_) => "M_NOT_FOUND",
            ApiError::Conflict(_) => "M_USER_IN_USE",
            ApiError::RateLimited { .. } => "M_LIMIT_EXCEEDED",
            ApiError::InternalError(_) => "M_UNKNOWN",
        }
    }
    
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            ApiError::RateLimited { message, retry_after } => json!({
                "errcode": self.error_code(),
                "error": message,
                "retry_after_ms": retry_after * 1000
            }),
            _ => json!({
                "errcode": self.error_code(),
                "error": self.message()
            })
        }
    }
    
    fn message(&self) -> &str {
        match self {
            ApiError::BadRequest(msg) => msg,
            ApiError::Unauthorized(msg) => msg,
            ApiError::Forbidden(msg) => msg,
            ApiError::NotFound(msg) => msg,
            ApiError::Conflict(msg) => msg,
            ApiError::RateLimited { message, .. } => message,
            ApiError::InternalError(msg) => msg,
        }
    }
}
```

---

## 6. 测试计划

### 6.1 单元测试

#### 6.1.1 测试框架配置

```rust
// 文件: src/tests/mod.rs

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;
    
    // 初始化测试环境
    async fn setup_test_db() -> PgPool {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect("postgres://test:test@localhost/synapse_test")
            .await
            .expect("Failed to connect to test database");
        
        // 运行迁移
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("Failed to run migrations");
        
        pool
    }
    
    // 清理测试数据
    async fn cleanup_test_db(pool: &PgPool) {
        sqlx::query("TRUNCATE users, devices, access_tokens CASCADE")
            .execute(pool)
            .await
            .expect("Failed to cleanup test database");
    }
}
```

#### 6.1.2 核心功能测试用例

```rust
// 文件: src/tests/api/user_test.rs

#[cfg(test)]
mod user_tests {
    use super::*;
    
    #[tokio::test]
    async fn test_user_registration() {
        let pool = setup_test_db().await;
        let app = create_test_app(pool.clone()).await;
        
        // 测试正常注册
        let response = app
            .post("/_matrix/client/r0/register")
            .json(&json!({
                "username": "testuser",
                "password": "Test@123",
                "auth": {"type": "m.login.dummy"}
            }))
            .send()
            .await;
        
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await;
        assert!(body.get("access_token").is_some());
        assert!(body.get("user_id").is_some());
        
        // 测试重复注册
        let response = app
            .post("/_matrix/client/r0/register")
            .json(&json!({
                "username": "testuser",
                "password": "Test@123",
                "auth": {"type": "m.login.dummy"}
            }))
            .send()
            .await;
        
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        
        cleanup_test_db(&pool).await;
    }
    
    #[tokio::test]
    async fn test_user_login() {
        let pool = setup_test_db().await;
        let app = create_test_app(pool.clone()).await;
        
        // 创建测试用户
        create_test_user(&app, "testuser", "Test@123").await;
        
        // 测试正确密码登录
        let response = app
            .post("/_matrix/client/r0/login")
            .json(&json!({
                "type": "m.login.password",
                "user": "testuser",
                "password": "Test@123"
            }))
            .send()
            .await;
        
        assert_eq!(response.status(), StatusCode::OK);
        
        // 测试错误密码登录
        let response = app
            .post("/_matrix/client/r0/login")
            .json(&json!({
                "type": "m.login.password",
                "user": "testuser",
                "password": "wrongpassword"
            }))
            .send()
            .await;
        
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        
        cleanup_test_db(&pool).await;
    }
    
    #[tokio::test]
    async fn test_user_authentication_required() {
        let pool = setup_test_db().await;
        let app = create_test_app(pool.clone()).await;
        
        // 测试无认证访问受保护端点
        let response = app
            .get("/_matrix/client/r0/account/whoami")
            .send()
            .await;
        
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        
        cleanup_test_db(&pool).await;
    }
}
```

#### 6.1.3 房间管理测试用例

```rust
// 文件: src/tests/api/room_test.rs

#[cfg(test)]
mod room_tests {
    use super::*;
    
    #[tokio::test]
    async fn test_create_room() {
        let pool = setup_test_db().await;
        let app = create_test_app(pool.clone()).await;
        let token = create_test_user_and_login(&app, "testuser", "Test@123").await;
        
        let response = app
            .post("/_matrix/client/r0/createRoom")
            .header("Authorization", format!("Bearer {}", token))
            .json(&json!({
                "name": "Test Room",
                "topic": "A test room",
                "preset": "private_chat"
            }))
            .send()
            .await;
        
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await;
        assert!(body.get("room_id").is_some());
        
        cleanup_test_db(&pool).await;
    }
    
    #[tokio::test]
    async fn test_join_room_by_alias() {
        let pool = setup_test_db().await;
        let app = create_test_app(pool.clone()).await;
        let token = create_test_user_and_login(&app, "testuser", "Test@123").await;
        
        // 创建带别名的房间
        let room_id = create_room_with_alias(&app, &token, "test-room").await;
        
        // 通过别名加入房间
        let response = app
            .post("/_matrix/client/r0/join/%23test-room:cjystx.top")
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await;
        
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await;
        assert_eq!(body["room_id"], room_id);
        
        cleanup_test_db(&pool).await;
    }
    
    #[tokio::test]
    async fn test_room_state_permissions() {
        let pool = setup_test_db().await;
        let app = create_test_app(pool.clone()).await;
        let token = create_test_user_and_login(&app, "testuser", "Test@123").await;
        let room_id = create_test_room(&app, &token).await;
        
        // 测试无认证访问房间状态
        let response = app
            .get(&format!("/_matrix/client/r0/rooms/{}/state", room_id))
            .send()
            .await;
        
        // 应该返回 401 Unauthorized
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        
        cleanup_test_db(&pool).await;
    }
}
```

### 6.2 集成测试

#### 6.2.1 测试脚本

```bash
#!/bin/bash
# 文件: tests/integration/run_all_tests.sh

set -e

echo "=== 开始集成测试 ==="

# 配置
SERVER="http://localhost:8008"
ADMIN_USER="admin"
ADMIN_PASS="admin123"
TEST_USER="testuser_$(date +%s)"
TEST_PASS="Test@123"

# 1. 健康检查
echo "1. 健康检查测试..."
curl -s "$SERVER/health" | grep -q "OK" && echo "✅ 健康检查通过" || echo "❌ 健康检查失败"

# 2. 用户注册测试
echo "2. 用户注册测试..."
REGISTER_RESPONSE=$(curl -s -X POST "$SERVER/_matrix/client/r0/register" \
    -H "Content-Type: application/json" \
    -d "{\"username\":\"$TEST_USER\",\"password\":\"$TEST_PASS\",\"auth\":{\"type\":\"m.login.dummy\"}}")

if echo "$REGISTER_RESPONSE" | grep -q "access_token"; then
    echo "✅ 用户注册成功"
    TOKEN=$(echo "$REGISTER_RESPONSE" | jq -r '.access_token')
else
    echo "❌ 用户注册失败: $REGISTER_RESPONSE"
    exit 1
fi

# 3. 用户登录测试
echo "3. 用户登录测试..."
LOGIN_RESPONSE=$(curl -s -X POST "$SERVER/_matrix/client/r0/login" \
    -H "Content-Type: application/json" \
    -d "{\"type\":\"m.login.password\",\"user\":\"$TEST_USER\",\"password\":\"$TEST_PASS\"}")

if echo "$LOGIN_RESPONSE" | grep -q "access_token"; then
    echo "✅ 用户登录成功"
else
    echo "❌ 用户登录失败: $LOGIN_RESPONSE"
    exit 1
fi

# 4. 创建房间测试
echo "4. 创建房间测试..."
ROOM_RESPONSE=$(curl -s -X POST "$SERVER/_matrix/client/r0/createRoom" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"name":"Test Room","preset":"private_chat"}')

if echo "$ROOM_RESPONSE" | grep -q "room_id"; then
    echo "✅ 创建房间成功"
    ROOM_ID=$(echo "$ROOM_RESPONSE" | jq -r '.room_id')
else
    echo "❌ 创建房间失败: $ROOM_RESPONSE"
    exit 1
fi

# 5. 发送消息测试
echo "5. 发送消息测试..."
MESSAGE_RESPONSE=$(curl -s -X PUT "$SERVER/_matrix/client/r0/rooms/$ROOM_ID/send/m.room.message/$(date +%s)" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"msgtype":"m.text","body":"Hello, World!"}')

if echo "$MESSAGE_RESPONSE" | grep -q "event_id"; then
    echo "✅ 发送消息成功"
else
    echo "❌ 发送消息失败: $MESSAGE_RESPONSE"
    exit 1
fi

# 6. 房间状态测试
echo "6. 房间状态测试..."
STATE_RESPONSE=$(curl -s -X GET "$SERVER/_matrix/client/r0/rooms/$ROOM_ID/state" \
    -H "Authorization: Bearer $TOKEN")

if echo "$STATE_RESPONSE" | grep -q "m.room.create"; then
    echo "✅ 获取房间状态成功"
else
    echo "❌ 获取房间状态失败: $STATE_RESPONSE"
    exit 1
fi

# 7. 管理员 API 测试
echo "7. 管理员 API 测试..."
ADMIN_TOKEN=$(get_admin_token "$SERVER" "$ADMIN_USER" "$ADMIN_PASS")

USERS_RESPONSE=$(curl -s -X GET "$SERVER/_synapse/admin/v1/users" \
    -H "Authorization: Bearer $ADMIN_TOKEN")

if echo "$USERS_RESPONSE" | grep -q "users"; then
    echo "✅ 管理员 API 正常"
else
    echo "❌ 管理员 API 失败: $USERS_RESPONSE"
    exit 1
fi

echo "=== 集成测试完成 ==="
```

### 6.3 端到端测试

#### 6.3.1 测试场景

| 场景 | 测试步骤 | 预期结果 |
|------|----------|----------|
| 完整用户流程 | 注册 → 登录 → 创建房间 → 发送消息 → 获取消息 | 所有操作成功 |
| 多用户交互 | 用户A创建房间 → 邀请用户B → 用户B加入 → 互相发送消息 | 消息正确传递 |
| 权限控制 | 普通用户访问管理员端点 | 返回 403 Forbidden |
| 联邦通信 | 服务器A发送消息到服务器B | 消息正确同步 |

#### 6.3.2 自动化测试脚本

```python
#!/usr/bin/env python3
# 文件: tests/e2e/test_full_flow.py

import requests
import json
import time
import uuid

class SynapseE2ETest:
    def __init__(self, server_url):
        self.server_url = server_url
        self.session = requests.Session()
        
    def test_full_user_flow(self):
        """测试完整的用户流程"""
        print("=== 测试完整用户流程 ===")
        
        # 1. 注册用户
        username = f"testuser_{uuid.uuid4().hex[:8]}"
        password = "Test@123"
        
        register_data = {
            "username": username,
            "password": password,
            "auth": {"type": "m.login.dummy"}
        }
        
        response = self.session.post(
            f"{self.server_url}/_matrix/client/r0/register",
            json=register_data
        )
        assert response.status_code == 200, f"注册失败: {response.text}"
        
        token = response.json()["access_token"]
        user_id = response.json()["user_id"]
        print(f"✅ 用户注册成功: {user_id}")
        
        # 设置认证头
        self.session.headers.update({"Authorization": f"Bearer {token}"})
        
        # 2. 创建房间
        room_data = {
            "name": "Test Room",
            "preset": "private_chat"
        }
        
        response = self.session.post(
            f"{self.server_url}/_matrix/client/r0/createRoom",
            json=room_data
        )
        assert response.status_code == 200, f"创建房间失败: {response.text}"
        
        room_id = response.json()["room_id"]
        print(f"✅ 创建房间成功: {room_id}")
        
        # 3. 发送消息
        message_data = {
            "msgtype": "m.text",
            "body": "Hello, World!"
        }
        
        txn_id = str(int(time.time() * 1000))
        response = self.session.put(
            f"{self.server_url}/_matrix/client/r0/rooms/{room_id}/send/m.room.message/{txn_id}",
            json=message_data
        )
        assert response.status_code == 200, f"发送消息失败: {response.text}"
        
        event_id = response.json()["event_id"]
        print(f"✅ 发送消息成功: {event_id}")
        
        # 4. 获取消息
        response = self.session.get(
            f"{self.server_url}/_matrix/client/r0/rooms/{room_id}/messages",
            params={"dir": "b", "limit": 10}
        )
        assert response.status_code == 200, f"获取消息失败: {response.text}"
        
        messages = response.json()["chunk"]
        assert len(messages) > 0, "没有找到消息"
        print(f"✅ 获取消息成功: {len(messages)} 条消息")
        
        # 5. 同步测试
        response = self.session.get(
            f"{self.server_url}/_matrix/client/r0/sync",
            params={"timeout": 0}
        )
        assert response.status_code == 200, f"同步失败: {response.text}"
        
        sync_data = response.json()
        assert room_id in sync_data["rooms"]["join"], "房间不在同步数据中"
        print(f"✅ 同步测试成功")
        
        print("=== 完整用户流程测试通过 ===\n")
        return True

if __name__ == "__main__":
    test = SynapseE2ETest("http://localhost:8008")
    test.test_full_user_flow()
```

---

## 7. 实施时间表

### 7.1 总体时间规划

| 阶段 | 任务 | 预计时间 | 开始日期 | 结束日期 | 责任人 |
|------|------|----------|----------|----------|--------|
| 第一阶段 | 高优先级问题修复 | 5 天 | 2026-02-13 | 2026-02-17 | 后端开发 |
| 第二阶段 | 管理员 API 实现 | 7 天 | 2026-02-18 | 2026-02-24 | 后端开发 |
| 第三阶段 | 联邦通信 API 实现 | 10 天 | 2026-02-25 | 2026-03-06 | 后端开发 |
| 第四阶段 | 扩展功能 API 实现 | 8 天 | 2026-03-07 | 2026-03-14 | 后端开发 |
| 第五阶段 | 测试与修复 | 5 天 | 2026-03-15 | 2026-03-19 | QA团队 |
| 最终验收 | 文档更新与交付 | 2 天 | 2026-03-20 | 2026-03-21 | 全体 |

### 7.2 详细任务分解

#### 第一阶段详细计划

| 日期 | 任务 | 产出物 | 验收标准 |
|------|------|--------|----------|
| 2026-02-13 | 数据库约束修复 | 迁移脚本 | 密钥上传成功 |
| 2026-02-13 | 语音消息表修复 | 迁移脚本 | 语音消息获取成功 |
| 2026-02-14 | 文件系统权限修复 | 配置文档 | 媒体上传成功 |
| 2026-02-14 | 通过别名加入房间实现 | Rust 代码 | 端点返回 200 |
| 2026-02-15 | 忘记房间端点实现 | Rust 代码 | 端点返回 200 |
| 2026-02-15 | 状态事件存储修复 | Rust 代码 | 状态查询成功 |
| 2026-02-16 | 事件类型前缀修复 | Rust 代码 | 类型正确存储 |
| 2026-02-17 | 第一阶段测试 | 测试报告 | 所有测试通过 |

---

## 8. 风险评估与应对策略

### 8.1 技术风险

| 风险 | 影响 | 概率 | 应对策略 |
|------|------|------|----------|
| 数据库迁移失败 | 高 | 中 | 执行前完整备份，准备回滚脚本 |
| 联邦通信实现复杂 | 高 | 高 | 参考官方实现，分阶段实现 |
| 性能问题 | 中 | 中 | 添加性能测试，优化查询 |
| 安全漏洞 | 高 | 低 | 代码审查，安全测试 |

### 8.2 项目风险

| 风险 | 影响 | 概率 | 应对策略 |
|------|------|------|----------|
| 时间延期 | 中 | 中 | 预留缓冲时间，优先高优先级任务 |
| 需求变更 | 中 | 低 | 变更评审流程，影响分析 |
| 资源不足 | 高 | 低 | 提前规划，外部支持 |

### 8.3 应急预案

```bash
#!/bin/bash
# 文件: scripts/rollback.sh
# 数据库回滚脚本

BACKUP_FILE=$1

if [ -z "$BACKUP_FILE" ]; then
    echo "用法: ./rollback.sh <backup_file.sql>"
    exit 1
fi

echo "=== 开始数据库回滚 ==="

# 停止服务
docker-compose stop synapse

# 恢复数据库
docker exec -i synapse-postgres psql -U synapse -d synapse_test < "$BACKUP_FILE"

if [ $? -eq 0 ]; then
    echo "✅ 数据库回滚成功"
    # 重启服务
    docker-compose start synapse
else
    echo "❌ 数据库回滚失败"
    exit 1
fi
```

---

## 附录

### A. 参考文档

1. [Matrix Client-Server API 规范](https://spec.matrix.org/v1.2/client-server-api/)
2. [Synapse 官方文档](https://element-hq.github.io/synapse/latest/)
3. [Synapse Admin API](https://element-hq.github.io/synapse/latest/admin_api/)
4. [Matrix Federation API](https://spec.matrix.org/v1.2/server-server-api/)

### B. 工具与资源

| 工具 | 用途 | 链接 |
|------|------|------|
| Postman | API 测试 | https://www.postman.com/ |
| pgAdmin | 数据库管理 | https://www.pgadmin.org/ |
| Grafana | 监控面板 | https://grafana.com/ |
| Prometheus | 指标收集 | https://prometheus.io/ |

### C. 联系方式

| 角色 | 负责人 | 职责 |
|------|--------|------|
| 项目负责人 | TBD | 整体协调与决策 |
| 后端开发 | TBD | API 实现与修复 |
| 数据库管理 | TBD | 迁移脚本与优化 |
| QA 测试 | TBD | 测试用例与执行 |
| 运维支持 | TBD | 环境配置与部署 |

---

> **文档维护**: 本文档应随项目进展持续更新，确保与实际实施情况保持一致。
