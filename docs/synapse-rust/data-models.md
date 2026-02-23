# 数据模型文档

> **版本**：2.0.0  
> **创建日期**：2026-01-28  
> **最后更新**：2026-01-30  
> **项目状态**：开发中  
> **参考文档**：[Synapse 官方文档](https://element-hq.github.io/synapse/latest/)、[Matrix 规范](https://spec.matrix.org/)

---

## 一、数据库表结构总览

本文档描述了 synapse-rust 项目的完整数据模型，涵盖用户认证、设备管理、房间通信、好友关系、私聊功能、端到端加密等核心模块。数据库设计遵循 Matrix 协议规范，同时针对 Rust 语言特性进行了优化适配。所有表结构均已在代码中通过 `initialize_database` 函数定义，确保运行时自动创建所需的数据库结构。

### 1.1 表分类概览

| 类别 | 包含表 | 说明 |
|------|--------|------|
| 用户认证 | users、devices、access_tokens、refresh_tokens | 用户身份验证与设备管理 |
| 房间通信 | rooms、room_memberships、room_events、current_state_events | 房间创建、成员管理与消息传递 |
| 实时状态 | presence、typing | 用户在线状态与输入指示 |
| 用户目录 | user_directory、user_ips、user_filters | 用户信息索引与查询优化 |
| 好友关系 | friends、friend_requests、friend_categories、blocked_users | 社交关系管理 |
| 私聊功能 | private_sessions、private_messages | 点对点加密通信 |
| 语音消息 | voice_messages、voice_usage_stats | 语音消息存储与统计 |
| 推送通知 | pushers、push_rules、push_rules_user_sent_rules、pusher_throttle | 消息推送机制 |
| 端到端加密 | device_keys、cross_signing_keys、one_time_keys、key_backups、backup_keys、event_signatures、megolm_sessions、inbound_megolm_sessions、session_keys | 加密密钥管理 |
| 房间扩展 | room_state、room_aliases、room_key_distributions、receipts | 房间状态与别名管理 |
| 系统维护 | ip_blocks、ip_reputation、security_events、db_metadata、key_changes、ratelimit_shard | 系统运行与安全监控 |
| 关联表 | user_rooms | 用户与房间关联 |

---

## 二、核心表结构详解

### 2.1 用户表（users）

用户表是系统的核心表之一，存储所有用户的基本信息。每个用户由唯一的 `user_id` 标识，该标识符遵循 Matrix 协议规范，格式为 `@username:servername`。表结构设计兼顾了用户信息的完整性和查询效率，通过合理的索引配置确保常用查询模式的性能。

```sql
CREATE TABLE IF NOT EXISTS users (
    user_id TEXT NOT NULL PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT,
    is_admin BOOLEAN DEFAULT FALSE,
    is_guest BOOLEAN DEFAULT FALSE,
    consent_version TEXT,
    appservice_id TEXT,
    creation_ts BIGINT NOT NULL,
    user_type TEXT,
    deactivated BOOLEAN DEFAULT FALSE,
    shadow_banned BOOLEAN DEFAULT FALSE,
    generation BIGINT NOT NULL,
    avatar_url TEXT,
    displayname TEXT,
    invalid_update_ts BIGINT,
    migration_state TEXT
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| user_id | TEXT | PRIMARY KEY | 用户唯一标识符，遵循 Matrix 格式 `@username:domain` |
| username | TEXT | UNIQUE, NOT NULL | 用户名，唯一且不能为空 |
| password_hash | TEXT | NULLABLE | 密码哈希值，支持多种哈希算法 |
| is_admin | BOOLEAN | DEFAULT FALSE | 是否为管理员账户 |
| is_guest | BOOLEAN | DEFAULT FALSE | 是否为访客用户 |
| consent_version | TEXT | NULLABLE | 用户同意的条款版本号 |
| appservice_id | TEXT | NULLABLE | 应用服务标识符，用于机器人账户 |
| creation_ts | BIGINT | NOT NULL | 账户创建时间戳，精确到毫秒 |
| user_type | TEXT | NULLABLE | 用户类型标识 |
| deactivated | BOOLEAN | DEFAULT FALSE | 是否已停用账户 |
| shadow_banned | BOOLEAN | DEFAULT FALSE | 是否被影子封禁（用户无感知） |
| generation | BIGINT | NOT NULL | 内部生成号，用于同步排序 |
| avatar_url | TEXT | NULLABLE | 用户头像 URL |
| displayname | TEXT | NULLABLE | 用户显示名称 |
| invalid_update_ts | BIGINT | NULLABLE | 无效更新时间戳 |
| migration_state | TEXT | NULLABLE | 数据迁移状态标识 |

**索引配置**：
- PRIMARY KEY (user_id)
- UNIQUE INDEX (username)
- 建议添加 INDEX (creation_ts) 用于按时间排序查询
- 建议添加 INDEX (is_admin) 用于管理员查询

**查询模式分析**：最频繁的查询包括按 user_id 查询、按用户名查询、按创建时间排序等。建议在 username 字段上创建 GIN 索引以支持模糊搜索功能。

### 2.2 设备表（devices）

设备表记录用户的登录设备信息，支持多设备登录场景。每个设备有唯一的 `device_id`，与用户关联实现设备管理功能。设备表设计考虑了安全审计需求，记录最后活跃时间和 IP 地址，便于账户安全监控。

```sql
CREATE TABLE IF NOT EXISTS devices (
    device_id TEXT NOT NULL PRIMARY KEY,
    user_id TEXT NOT NULL,
    display_name TEXT,
    last_seen_ts TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    last_seen_ip TEXT,
    created_ts BIGINT NOT NULL,
    user_agent TEXT,
    keys JSONB,
    device_display_name TEXT,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| device_id | TEXT | PRIMARY KEY | 设备唯一标识符 |
| user_id | TEXT | NOT NULL, FOREIGN KEY | 所属用户标识符 |
| display_name | TEXT | NULLABLE | 设备显示名称 |
| last_seen_ts | TIMESTAMP WITH TIME ZONE | DEFAULT NOW() | 最后活跃时间 |
| last_seen_ip | TEXT | NULLABLE | 最后活跃 IP 地址 |
| created_ts | BIGINT | NOT NULL | 设备创建时间戳 |
| user_agent | TEXT | NULLABLE | 客户端 User-Agent 字符串 |
| keys | JSONB | NULLABLE | 设备加密密钥信息 |
| device_display_name | TEXT | NULLABLE | 设备显示名称备用字段 |

**索引配置**：
- PRIMARY KEY (device_id)
- INDEX (user_id)
- INDEX (last_seen_ts DESC) 用于查询最近活跃设备
- 建议添加 INDEX (last_seen_ip) 用于安全审计

**级联删除策略**：当用户被删除时，关联的设备记录将自动删除，这是通过 `ON DELETE CASCADE` 外键约束实现的，确保数据一致性。

### 2.3 访问令牌表（access_tokens）

访问令牌表实现用户认证功能，记录登录会话的访问令牌。每个令牌关联特定用户和设备，支持令牌失效管理和过期机制。表设计考虑了安全性要求，支持令牌失效状态追踪。

```sql
CREATE TABLE IF NOT EXISTS access_tokens (
    id BIGSERIAL PRIMARY KEY,
    token TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    device_id TEXT,
    created_ts BIGINT NOT NULL,
    expired_ts BIGINT,
    invalidated BOOLEAN DEFAULT FALSE,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE CASCADE
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增主键 |
| token | TEXT | UNIQUE, NOT NULL | 访问令牌字符串 |
| user_id | TEXT | NOT NULL, FOREIGN KEY | 所属用户 |
| device_id | TEXT | NULLABLE, FOREIGN KEY | 关联设备 |
| created_ts | BIGINT | NOT NULL | 创建时间戳 |
| expired_ts | BIGINT | NULLABLE | 过期时间戳 |
| invalidated | BOOLEAN | DEFAULT FALSE | 是否已失效 |

**索引配置**：
- PRIMARY KEY (id)
- UNIQUE INDEX (token)
- INDEX (user_id)
- INDEX (device_id)
- 建议添加 INDEX (expired_ts) 用于过期令牌清理

### 2.4 刷新令牌表（refresh_tokens）

刷新令牌表配合访问令牌使用，实现令牌续签机制。刷新令牌的存活时间通常较长，用于获取新的访问令牌。

```sql
CREATE TABLE IF NOT EXISTS refresh_tokens (
    id BIGSERIAL PRIMARY KEY,
    token TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    expired_ts BIGINT,
    invalidated BOOLEAN DEFAULT FALSE,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE CASCADE
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增主键 |
| token | TEXT | UNIQUE, NOT NULL | 刷新令牌字符串 |
| user_id | TEXT | NOT NULL, FOREIGN KEY | 所属用户 |
| device_id | TEXT | NOT NULL, FOREIGN KEY | 关联设备 |
| created_ts | BIGINT | NOT NULL | 创建时间戳 |
| expired_ts | BIGINT | NULLABLE | 过期时间戳 |
| invalidated | BOOLEAN | DEFAULT FALSE | 是否已失效 |

**索引配置**：
- PRIMARY KEY (id)
- UNIQUE INDEX (token)
- INDEX (user_id)
- INDEX (device_id)

### 2.5 房间表（rooms）

房间表存储房间的元数据信息，是 Matrix 通信的核心数据结构。每个房间有唯一的 `room_id`，记录创建者、创建时间、房间配置等基本信息。房间支持多种配置选项，包括公开/私密、联邦开关、加密设置等。

```sql
CREATE TABLE IF NOT EXISTS rooms (
    room_id TEXT NOT NULL PRIMARY KEY,
    is_public BOOLEAN NOT NULL DEFAULT FALSE,
    creator TEXT NOT NULL,
    creation_ts BIGINT NOT NULL,
    federate BOOLEAN NOT NULL DEFAULT TRUE,
    version TEXT NOT NULL DEFAULT '1',
    name TEXT,
    topic TEXT,
    avatar TEXT,
    canonical_alias TEXT,
    guest_access BOOLEAN DEFAULT FALSE,
    history_visibility TEXT DEFAULT 'shared',
    encryption TEXT,
    is_flaged BOOLEAN DEFAULT FALSE,
    is_spotlight BOOLEAN DEFAULT FALSE,
    deleted_ts BIGINT,
    join_rule TEXT,
    visibility TEXT
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| room_id | TEXT | PRIMARY KEY | 房间唯一标识符 |
| is_public | BOOLEAN | NOT NULL, DEFAULT FALSE | 是否为公开房间 |
| creator | TEXT | NOT NULL | 创建者用户 ID |
| creation_ts | BIGINT | NOT NULL | 创建时间戳 |
| federate | BOOLEAN | NOT NULL, DEFAULT TRUE | 是否允许联邦 |
| version | TEXT | NOT NULL, DEFAULT '1' | 房间协议版本 |
| name | TEXT | NULLABLE | 房间名称 |
| topic | TEXT | NULLABLE | 房间主题描述 |
| avatar | TEXT | NULLABLE | 房间头像 URL |
| canonical_alias | TEXT | NULLABLE | 规范别名 |
| guest_access | BOOLEAN | DEFAULT FALSE | 访客访问权限 |
| history_visibility | TEXT | DEFAULT 'shared' | 历史消息可见性 |
| encryption | TEXT | NULLABLE | 端到端加密算法 |
| is_flaged | BOOLEAN | DEFAULT FALSE | 是否标记 |
| is_spotlight | BOOLEAN | DEFAULT FALSE | 是否推荐显示 |
| deleted_ts | BIGINT | NULLABLE | 软删除时间戳 |
| join_rule | TEXT | NULLABLE | 加入规则 |
| visibility | TEXT | NULLABLE | 房间可见性 |

**索引配置**：
- PRIMARY KEY (room_id)
- INDEX (creator)
- INDEX (is_public)
- INDEX (creation_ts DESC)
- 建议添加 INDEX (canonical_alias) 用于别名查询

### 2.6 房间成员关系表（room_memberships）

房间成员关系表记录用户与房间的关联信息，是房间访问控制的核心。每个成员关系由 `(room_id, user_id)` 复合主键唯一标识，支持多种成员状态（加入、离开、封禁、邀请、敲击）。

```sql
CREATE TABLE IF NOT EXISTS room_memberships (
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    sender TEXT NOT NULL,
    membership TEXT NOT NULL,
    event_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    display_name TEXT,
    avatar_url TEXT,
    is_banned BOOLEAN DEFAULT FALSE,
    invite_token TEXT,
    inviter TEXT,
    updated_ts BIGINT,
    joined_ts BIGINT,
    left_ts BIGINT,
    reason TEXT,
    banned_by TEXT,
    ban_reason TEXT,
    ban_ts BIGINT,
    join_reason TEXT,
    PRIMARY KEY (room_id, user_id),
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| room_id | TEXT | PRIMARY KEY, FOREIGN KEY | 房间 ID |
| user_id | TEXT | PRIMARY KEY, FOREIGN KEY | 用户 ID |
| sender | TEXT | NOT NULL | 操作发起者 |
| membership | TEXT | NOT NULL | 成员状态（join、leave、ban、invite、knock） |
| event_id | TEXT | NOT NULL | 关联事件 ID |
| event_type | TEXT | NOT NULL | 事件类型 |
| display_name | TEXT | NULLABLE | 显示名称 |
| avatar_url | TEXT | NULLABLE | 头像 URL |
| is_banned | BOOLEAN | DEFAULT FALSE | 是否被封禁 |
| invite_token | TEXT | NULLABLE | 邀请令牌 |
| inviter | TEXT | NULLABLE | 邀请者 |
| updated_ts | BIGINT | NULLABLE | 更新时间戳 |
| joined_ts | BIGINT | NULLABLE | 加入时间戳 |
| left_ts | BIGINT | NULLABLE | 离开时间戳 |
| reason | TEXT | NULLABLE | 操作原因 |
| banned_by | TEXT | NULLABLE | 封禁执行者 |
| ban_reason | TEXT | NULLABLE | 封禁原因 |
| ban_ts | BIGINT | NULLABLE | 封禁时间戳 |
| join_reason | TEXT | NULLABLE | 加入原因 |

**索引配置**：
- PRIMARY KEY (room_id, user_id)
- INDEX (user_id)
- INDEX (membership)
- INDEX (event_id)
- 建议添加 INDEX (room_id, membership, joined_ts DESC) 用于成员列表查询
- 建议添加 INDEX (room_id, joined_ts DESC) 用于活跃成员查询

### 2.7 房间事件表（room_events）

房间事件表存储房间内发生的所有事件，是消息传递的核心数据结构。事件按照 `origin_server_ts` 时间戳排序，支持消息历史追溯和状态同步。每个事件属于特定房间和用户，通过 `event_id` 唯一标识。

```sql
CREATE TABLE IF NOT EXISTS room_events (
    event_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    content TEXT NOT NULL,
    state_key TEXT,
    depth BIGINT NOT NULL DEFAULT 0,
    origin_server_ts BIGINT NOT NULL,
    processed_ts BIGINT NOT NULL,
    not_before BIGINT DEFAULT 0,
    status TEXT DEFAULT NULL,
    reference_image TEXT,
    origin TEXT NOT NULL,
    sender TEXT NOT NULL,
    unsigned TEXT,
    redacted BOOLEAN DEFAULT FALSE,
    PRIMARY KEY (event_id),
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| event_id | TEXT | PRIMARY KEY | 事件唯一标识符 |
| room_id | TEXT | NOT NULL, FOREIGN KEY | 所属房间 |
| user_id | TEXT | NOT NULL, FOREIGN KEY | 发送用户 |
| event_type | TEXT | NOT NULL | 事件类型 |
| content | TEXT | NOT NULL | 事件内容（JSON 序列化） |
| state_key | TEXT | NULLABLE | 状态键（用于状态事件） |
| depth | BIGINT | NOT NULL, DEFAULT 0 | 事件深度 |
| origin_server_ts | BIGINT | NOT NULL | 源服务器时间戳 |
| processed_ts | BIGINT | NOT NULL | 处理时间戳 |
| not_before | BIGINT | DEFAULT 0 | 最早可处理时间 |
| status | TEXT | DEFAULT NULL | 发送状态 |
| reference_image | TEXT | NULLABLE | 引用图片 |
| origin | TEXT | NOT NULL | 源服务器标识 |
| sender | TEXT | NOT NULL | 发送者标识 |
| unsigned | TEXT | NULLABLE | 无符号数据 |
| redacted | BOOLEAN | DEFAULT FALSE | 是否已删除 |

**索引配置**：
- PRIMARY KEY (event_id)
- INDEX (room_id)
- INDEX (user_id)
- INDEX (origin_server_ts DESC)
- INDEX (event_type)
- 建议添加 INDEX (room_id, event_type, origin_server_ts DESC) 用于类型过滤查询
- 建议添加 INDEX (room_id, origin_server_ts DESC, redacted) 用于消息历史查询

### 2.8 当前状态事件表（current_state_events）

当前状态事件表存储房间的当前状态快照，用于快速查询房间当前配置。与 room_events 表的历史记录不同，此表只保留最新状态。

```sql
CREATE TABLE IF NOT EXISTS current_state_events (
    room_id TEXT NOT NULL,
    type TEXT NOT NULL,
    state_key TEXT NOT NULL,
    event_id TEXT NOT NULL,
    membership TEXT,
    depth BIGINT NOT NULL,
    stream_ordering BIGINT,
    PRIMARY KEY (room_id, type, state_key),
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| room_id | TEXT | PRIMARY KEY, FOREIGN KEY | 房间 ID |
| type | TEXT | PRIMARY KEY | 事件类型 |
| state_key | TEXT | PRIMARY KEY | 状态键 |
| event_id | TEXT | NOT NULL | 事件 ID |
| membership | TEXT | NULLABLE | 成员状态（用于成员事件） |
| depth | BIGINT | NOT NULL | 事件深度 |
| stream_ordering | BIGINT | NULLABLE | 流排序 |

**索引配置**：
- PRIMARY KEY (room_id, type, state_key)
- INDEX (event_id)
- 建议添加 INDEX (room_id, type) 用于按类型查询状态

### 2.9 在线状态表（presence）

在线状态表记录用户的实时在线状态，支持状态消息和最后活跃时间追踪。这是实现用户在线状态显示功能的核心表。

```sql
CREATE TABLE IF NOT EXISTS presence (
    user_id TEXT NOT NULL PRIMARY KEY,
    status_msg TEXT,
    presence TEXT NOT NULL DEFAULT 'offline',
    last_active_ts BIGINT NOT NULL DEFAULT 0,
    status_from TEXT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| user_id | TEXT | PRIMARY KEY, FOREIGN KEY | 用户 ID |
| status_msg | TEXT | NULLABLE | 状态消息文本 |
| presence | TEXT | NOT NULL, DEFAULT 'offline' | 在线状态（online、offline、unavailable） |
| last_active_ts | BIGINT | NOT NULL, DEFAULT 0 | 最后活跃时间戳 |
| status_from | TEXT | NULLABLE | 状态来源 |
| created_ts | BIGINT | NOT NULL | 创建时间戳 |
| updated_ts | BIGINT | NOT NULL | 更新时间戳 |

**索引配置**：
- PRIMARY KEY (user_id)
- INDEX (presence)
- 建议添加 INDEX (last_active_ts DESC) 用于活跃用户查询

---

## 三、社交功能表结构

### 3.1 好友表（friends）

好友表记录用户之间的双向好友关系，支持好友备注和星标功能。每条好友关系由 `(user_id, friend_id)` 复合主键唯一标识。

```sql
CREATE TABLE IF NOT EXISTS friends (
    user_id TEXT NOT NULL,
    friend_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    note TEXT,
    is_favorite BOOLEAN DEFAULT FALSE,
    PRIMARY KEY (user_id, friend_id),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (friend_id) REFERENCES users(user_id) ON DELETE CASCADE
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| user_id | TEXT | PRIMARY KEY, FOREIGN KEY | 用户 ID |
| friend_id | TEXT | PRIMARY KEY, FOREIGN KEY | 好友用户 ID |
| created_ts | BIGINT | NOT NULL | 建立好友关系时间戳 |
| note | TEXT | NULLABLE | 好友备注 |
| is_favorite | BOOLEAN | DEFAULT FALSE | 是否星标好友 |

**索引配置**：
- PRIMARY KEY (user_id, friend_id)
- INDEX (friend_id)
- 建议添加 INDEX (user_id, is_favorite DESC, created_ts DESC) 用于好友列表查询

### 3.2 好友请求表（friend_requests）

好友请求表处理好友申请流程，记录请求者、目标用户、请求消息和当前状态。支持请求的接受、拒绝和忽略操作。

```sql
CREATE TABLE IF NOT EXISTS friend_requests (
    id BIGSERIAL PRIMARY KEY,
    requester_id TEXT NOT NULL,
    target_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    message TEXT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    FOREIGN KEY (requester_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (target_id) REFERENCES users(user_id) ON DELETE CASCADE
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| requester_id | TEXT | NOT NULL, FOREIGN KEY | 请求者 ID |
| target_id | TEXT | NOT NULL, FOREIGN KEY | 目标用户 ID |
| status | TEXT | NOT NULL, DEFAULT 'pending' | 请求状态（pending、accepted、rejected、ignored） |
| message | TEXT | NULLABLE | 请求消息 |
| created_ts | BIGINT | NOT NULL | 创建时间戳 |
| updated_ts | BIGINT | NOT NULL | 更新时间戳 |

**索引配置**：
- PRIMARY KEY (id)
- UNIQUE INDEX (requester_id, target_id)
- INDEX (target_id)
- INDEX (status)
- 建议添加 INDEX (target_id, status) 用于待处理请求查询

### 3.3 好友分类表（friend_categories）

好友分类表支持用户自定义好友分组功能，每个分类属于特定用户，提供颜色标识以支持 UI 显示。

```sql
CREATE TABLE IF NOT EXISTS friend_categories (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    name TEXT NOT NULL,
    color TEXT NOT NULL DEFAULT '#000000',
    created_ts BIGINT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| user_id | TEXT | NOT NULL, FOREIGN KEY | 所属用户 |
| name | TEXT | NOT NULL | 分类名称 |
| color | TEXT | NOT NULL, DEFAULT '#000000' | 分类颜色 |
| created_ts | BIGINT | NOT NULL | 创建时间戳 |

**索引配置**：
- PRIMARY KEY (id)
- INDEX (user_id)
- 建议添加 UNIQUE INDEX (user_id, name) 防止同名分类

### 3.4 黑名单表（blocked_users）

黑名单表实现用户屏蔽功能，记录用户屏蔽的其他用户及其原因。

```sql
CREATE TABLE IF NOT EXISTS blocked_users (
    user_id TEXT NOT NULL,
    blocked_id TEXT NOT NULL,
    reason TEXT,
    created_ts BIGINT NOT NULL,
    PRIMARY KEY (user_id, blocked_id),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (blocked_id) REFERENCES users(user_id) ON DELETE CASCADE
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| user_id | TEXT | PRIMARY KEY, FOREIGN KEY | 用户 ID |
| blocked_id | TEXT | PRIMARY KEY, FOREIGN KEY | 被屏蔽用户 ID |
| reason | TEXT | NULLABLE | 屏蔽原因 |
| created_ts | BIGINT | NOT NULL | 屏蔽时间戳 |

**索引配置**：
- PRIMARY KEY (user_id, blocked_id)
- INDEX (blocked_id)

---

## 四、私聊功能表结构

### 4.1 私聊会话表（private_sessions）

私聊会话表管理点对点加密通信的会话信息，每个会话关联两个用户，记录最后消息和未读计数。

```sql
CREATE TABLE IF NOT EXISTS private_sessions (
    id BIGSERIAL PRIMARY KEY,
    user_id_1 TEXT NOT NULL,
    user_id_2 TEXT NOT NULL,
    last_message TEXT,
    unread_count INTEGER DEFAULT 0,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    FOREIGN KEY (user_id_1) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (user_id_2) REFERENCES users(user_id) ON DELETE CASCADE
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| user_id_1 | TEXT | NOT NULL, FOREIGN KEY | 用户 1 |
| user_id_2 | TEXT | NOT NULL, FOREIGN KEY | 用户 2 |
| last_message | TEXT | NULLABLE | 最后消息预览 |
| unread_count | INTEGER | DEFAULT 0 | 未读消息数 |
| created_ts | BIGINT | NOT NULL | 创建时间戳 |
| updated_ts | BIGINT | NOT NULL | 更新时间戳 |

**索引配置**：
- PRIMARY KEY (id)
- UNIQUE INDEX (user_id_1, user_id_2)
- INDEX (user_id_1)
- INDEX (user_id_2)
- 建议添加 INDEX (user_id_1, updated_ts DESC) 用于会话列表查询
- 建议添加 INDEX (user_id_2, updated_ts DESC) 用于会话列表查询

### 4.2 私聊消息表（private_messages）

私聊消息表存储私聊会话中的消息内容，支持加密内容和已读状态追踪。

```sql
CREATE TABLE IF NOT EXISTS private_messages (
    id BIGSERIAL PRIMARY KEY,
    session_id BIGINT NOT NULL,
    sender_id TEXT NOT NULL,
    content TEXT NOT NULL,
    encrypted_content TEXT,
    message_type TEXT DEFAULT 'text',
    is_read BOOLEAN DEFAULT FALSE,
    read_by_receiver BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    FOREIGN KEY (session_id) REFERENCES private_sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (sender_id) REFERENCES users(user_id) ON DELETE CASCADE
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| session_id | BIGINT | NOT NULL, FOREIGN KEY | 会话 ID |
| sender_id | TEXT | NOT NULL, FOREIGN KEY | 发送者 ID |
| content | TEXT | NOT NULL | 消息内容（明文） |
| encrypted_content | TEXT | NULLABLE | 加密内容 |
| message_type | TEXT | DEFAULT 'text' | 消息类型 |
| is_read | BOOLEAN | DEFAULT FALSE | 发送者视角的已读状态 |
| read_by_receiver | BOOLEAN | DEFAULT FALSE | 接收者视角的已读状态 |
| created_ts | BIGINT | NOT NULL | 创建时间戳 |

**索引配置**：
- PRIMARY KEY (id)
- INDEX (session_id)
- INDEX (sender_id)
- INDEX (created_ts DESC)
- 建议添加 INDEX (session_id, created_ts DESC) 用于消息历史查询
- 建议添加 INDEX (session_id, is_read) 用于未读消息查询

---

## 五、语音消息表结构

### 5.1 语音消息表（voice_messages）

语音消息表存储用户发送的语音消息元数据，支持在房间和私聊中使用。记录文件路径、时长、波形数据等信息。

```sql
CREATE TABLE IF NOT EXISTS voice_messages (
    id BIGSERIAL PRIMARY KEY,
    message_id TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    room_id TEXT,
    session_id BIGINT,
    file_path TEXT NOT NULL,
    content_type TEXT NOT NULL,
    duration_ms INTEGER NOT NULL,
    file_size BIGINT NOT NULL,
    waveform_data TEXT,
    transcribe_text TEXT,
    created_ts BIGINT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    FOREIGN KEY (session_id) REFERENCES private_sessions(id) ON DELETE SET NULL
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| message_id | TEXT | UNIQUE, NOT NULL | 消息唯一标识 |
| user_id | TEXT | NOT NULL, FOREIGN KEY | 发送用户 |
| room_id | TEXT | NULLABLE, FOREIGN KEY | 房间 ID（房间消息） |
| session_id | BIGINT | NULLABLE, FOREIGN KEY | 私聊会话 ID（私聊消息） |
| file_path | TEXT | NOT NULL | 文件存储路径 |
| content_type | TEXT | NOT NULL | MIME 类型 |
| duration_ms | INTEGER | NOT NULL | 语音时长（毫秒） |
| file_size | BIGINT | NOT NULL | 文件大小（字节） |
| waveform_data | TEXT | NULLABLE | 波形数据 |
| transcribe_text | TEXT | NULLABLE | 语音转文字 |
| created_ts | BIGINT | NOT NULL | 创建时间戳 |

**索引配置**：
- PRIMARY KEY (id)
- UNIQUE INDEX (message_id)
- INDEX (user_id)
- INDEX (room_id)
- INDEX (session_id)
- INDEX (created_ts DESC)

### 5.2 语音使用统计表（voice_usage_stats）

语音使用统计表记录用户的语音消息使用情况，用于统计和计费功能。

```sql
CREATE TABLE IF NOT EXISTS voice_usage_stats (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    room_id TEXT,
    total_duration_ms BIGINT DEFAULT 0,
    total_count INTEGER DEFAULT 0,
    last_used_ts BIGINT,
    updated_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| user_id | TEXT | NOT NULL, FOREIGN KEY | 用户 ID |
| room_id | TEXT | NULLABLE | 房间 ID |
| total_duration_ms | BIGINT | DEFAULT 0 | 总使用时长（毫秒） |
| total_count | INTEGER | DEFAULT 0 | 总发送数量 |
| last_used_ts | BIGINT | NULLABLE | 最后使用时间戳 |
| updated_ts | BIGINT | NOT NULL | 更新时间戳 |

**索引配置**：
- PRIMARY KEY (id)
- INDEX (user_id)
- INDEX (room_id)
- 建议添加 UNIQUE INDEX (user_id, room_id) 用于按房间统计

---

## 六、端到端加密表结构

### 6.1 设备密钥表（device_keys）

设备密钥表存储用户的设备加密密钥，用于端到端加密的消息传输。每个设备可以有多个密钥，支持不同的加密算法。

```sql
CREATE TABLE IF NOT EXISTS device_keys (
    id UUID PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    display_name TEXT,
    algorithm TEXT NOT NULL,
    key_id TEXT NOT NULL,
    public_key TEXT NOT NULL,
    signatures JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE CASCADE
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| id | UUID | PRIMARY KEY | UUID 主键 |
| user_id | TEXT | NOT NULL, FOREIGN KEY | 用户 ID |
| device_id | TEXT | NOT NULL, FOREIGN KEY | 设备 ID |
| display_name | TEXT | NULLABLE | 显示名称 |
| algorithm | TEXT | NOT NULL | 加密算法（ed25519、curve25519） |
| key_id | TEXT | NOT NULL | 密钥标识符 |
| public_key | TEXT | NOT NULL | 公钥（Base64 编码） |
| signatures | JSONB | NOT NULL, DEFAULT '{}' | 签名数据 |
| created_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 创建时间 |
| updated_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 更新时间 |

**索引配置**：
- PRIMARY KEY (id)
- UNIQUE INDEX (user_id, device_id, key_id)
- INDEX (user_id)
- INDEX (device_id)
- INDEX (algorithm)

### 6.2 跨签名密钥表（cross_signing_keys）

跨签名密钥表存储用户的跨签名密钥，用于验证用户身份和设备信任关系。

```sql
CREATE TABLE IF NOT EXISTS cross_signing_keys (
    id UUID PRIMARY KEY,
    user_id TEXT NOT NULL,
    key_type TEXT NOT NULL,
    public_key TEXT NOT NULL,
    usage TEXT[] NOT NULL,
    signatures JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| id | UUID | PRIMARY KEY | UUID 主键 |
| user_id | TEXT | NOT NULL, FOREIGN KEY | 用户 ID |
| key_type | TEXT | NOT NULL | 密钥类型（master、self_signing、user_signing） |
| public_key | TEXT | NOT NULL | 公钥 |
| usage | TEXT[] | NOT NULL | 密钥用途数组 |
| signatures | JSONB | NOT NULL, DEFAULT '{}' | 签名数据 |
| created_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 创建时间 |
| updated_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 更新时间 |

**索引配置**：
- PRIMARY KEY (id)
- UNIQUE INDEX (user_id, key_type)
- INDEX (user_id)

### 6.3 一次性密钥表（one_time_keys）

一次性密钥表存储一次性密钥，用于建立加密会话。每个密钥只能使用一次，使用后立即删除。

```sql
CREATE TABLE IF NOT EXISTS one_time_keys (
    id UUID PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    key_id TEXT NOT NULL,
    public_key TEXT NOT NULL,
    signatures JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    used BOOLEAN NOT NULL DEFAULT FALSE,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE CASCADE
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| id | UUID | PRIMARY KEY | UUID 主键 |
| user_id | TEXT | NOT NULL, FOREIGN KEY | 用户 ID |
| device_id | TEXT | NOT NULL, FOREIGN KEY | 设备 ID |
| algorithm | TEXT | NOT NULL | 加密算法 |
| key_id | TEXT | NOT NULL | 密钥标识符 |
| public_key | TEXT | NOT NULL | 公钥 |
| signatures | JSONB | NOT NULL, DEFAULT '{}' | 签名数据 |
| created_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 创建时间 |
| used | BOOLEAN | NOT NULL, DEFAULT FALSE | 是否已使用 |

**索引配置**：
- PRIMARY KEY (id)
- UNIQUE INDEX (user_id, device_id, algorithm, key_id)
- INDEX (user_id)
- INDEX (used)

### 6.4 Megolm 会话表（megolm_sessions）

Megolm 会话表存储出站 Megolm 加密会话，用于群组消息加密。

```sql
CREATE TABLE IF NOT EXISTS megolm_sessions (
    id UUID PRIMARY KEY,
    session_id TEXT NOT NULL UNIQUE,
    room_id TEXT NOT NULL,
    sender_key TEXT NOT NULL,
    session_key TEXT NOT NULL,
    algorithm TEXT NOT NULL DEFAULT 'm.megolm.v1.aes-sha2',
    message_index BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NULLABLE,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| id | UUID | PRIMARY KEY | UUID 主键 |
| session_id | TEXT | UNIQUE, NOT NULL | 会话 ID |
| room_id | TEXT | NOT NULL, FOREIGN KEY | 房间 ID |
| sender_key | TEXT | NOT NULL | 发送方公钥 |
| session_key | TEXT | NOT NULL | 会话密钥 |
| algorithm | TEXT | NOT NULL, DEFAULT 'm.megolm.v1.aes-sha2' | 加密算法 |
| message_index | BIGINT | NOT NULL, DEFAULT 0 | 消息索引 |
| created_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 创建时间 |
| last_used_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 最后使用时间 |
| expires_at | TIMESTAMPTZ | NULLABLE | 过期时间 |

**索引配置**：
- PRIMARY KEY (id)
- UNIQUE INDEX (room_id, sender_key, session_id)
- INDEX (session_id)
- INDEX (room_id)
- INDEX (sender_key)
- INDEX (expires_at)

### 6.5 入站 Megolm 会话表（inbound_megolm_sessions）

入站 Megolm 会话表存储入站 Megolm 加密会话，用于解密群组消息。

```sql
CREATE TABLE IF NOT EXISTS inbound_megolm_sessions (
    id UUID PRIMARY KEY,
    session_id TEXT NOT NULL UNIQUE,
    sender_key TEXT NOT NULL,
    room_id TEXT NOT NULL,
    session_key TEXT NOT NULL,
    algorithm TEXT NOT NULL DEFAULT 'm.megolm.v1.aes-sha2',
    message_index BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NULLABLE
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| id | UUID | PRIMARY KEY | UUID 主键 |
| session_id | TEXT | UNIQUE, NOT NULL | 会话 ID |
| sender_key | TEXT | NOT NULL | 发送方公钥 |
| room_id | TEXT | NOT NULL | 房间 ID |
| session_key | TEXT | NOT NULL | 会话密钥 |
| algorithm | TEXT | NOT NULL, DEFAULT 'm.megolm.v1.aes-sha2' | 加密算法 |
| message_index | BIGINT | NOT NULL, DEFAULT 0 | 消息索引 |
| created_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 创建时间 |
| expires_at | TIMESTAMPTZ | NULLABLE | 过期时间 |

**索引配置**：
- PRIMARY KEY (id)
- UNIQUE INDEX (session_id)
- INDEX (sender_key)
- INDEX (room_id)

### 6.6 密钥备份表（key_backups）

密钥备份表存储用户的密钥备份信息，支持密钥恢复功能。

```sql
CREATE TABLE IF NOT EXISTS key_backups (
    id UUID PRIMARY KEY,
    user_id TEXT NOT NULL,
    version TEXT NOT NULL,
    algorithm TEXT NOT NULL DEFAULT 'm.megolm_backup.v1',
    auth_data JSONB NOT NULL,
    encrypted_data JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| id | UUID | PRIMARY KEY | UUID 主键 |
| user_id | TEXT | NOT NULL, FOREIGN KEY | 用户 ID |
| version | TEXT | NOT NULL | 备份版本 |
| algorithm | TEXT | NOT NULL, DEFAULT 'm.megolm_backup.v1' | 加密算法 |
| auth_data | JSONB | NOT NULL | 认证数据 |
| encrypted_data | JSONB | NOT NULL | 加密数据 |
| created_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 创建时间 |
| updated_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 更新时间 |

**索引配置**：
- PRIMARY KEY (id)
- UNIQUE INDEX (user_id, version)
- INDEX (user_id)
- INDEX (version)

### 6.7 备份密钥表（backup_keys）

备份密钥表存储备份中的密钥信息，与 key_backups 表关联。

```sql
CREATE TABLE IF NOT EXISTS backup_keys (
    id UUID PRIMARY KEY,
    backup_id UUID NOT NULL,
    room_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    first_message_index BIGINT NOT NULL DEFAULT 0,
    forwarded_count BIGINT NOT NULL DEFAULT 0,
    is_verified BOOLEAN NOT NULL DEFAULT FALSE,
    session_data TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    FOREIGN KEY (backup_id) REFERENCES key_backups(id) ON DELETE CASCADE
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| id | UUID | PRIMARY KEY | UUID 主键 |
| backup_id | UUID | NOT NULL, FOREIGN KEY | 备份 ID |
| room_id | TEXT | NOT NULL | 房间 ID |
| session_id | TEXT | NOT NULL | 会话 ID |
| first_message_index | BIGINT | NOT NULL, DEFAULT 0 | 首个消息索引 |
| forwarded_count | BIGINT | NOT NULL, DEFAULT 0 | 转发次数 |
| is_verified | BOOLEAN | NOT NULL, DEFAULT FALSE | 是否已验证 |
| session_data | TEXT | NOT NULL | 会话数据 |
| created_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 创建时间 |

**索引配置**：
- PRIMARY KEY (id)
- UNIQUE INDEX (backup_id, room_id, session_id)
- INDEX (backup_id)
- INDEX (room_id)
- INDEX (session_id)

### 6.8 事件签名表（event_signatures）

事件签名表存储事件的加密签名，用于验证消息来源。

```sql
CREATE TABLE IF NOT EXISTS event_signatures (
    id UUID PRIMARY KEY,
    event_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    signature TEXT NOT NULL,
    key_id TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE CASCADE
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| id | UUID | PRIMARY KEY | UUID 主键 |
| event_id | TEXT | NOT NULL | 事件 ID |
| user_id | TEXT | NOT NULL, FOREIGN KEY | 签名用户 |
| device_id | TEXT | NOT NULL, FOREIGN KEY | 签名设备 |
| signature | TEXT | NOT NULL | 签名数据 |
| key_id | TEXT | NOT NULL | 密钥 ID |
| created_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 创建时间 |

**索引配置**：
- PRIMARY KEY (id)
- INDEX (event_id)
- INDEX (user_id)
- INDEX (device_id)

### 6.9 会话密钥表（session_keys）

会话密钥表存储端到端加密的会话密钥信息。

```sql
CREATE TABLE IF NOT EXISTS session_keys (
    session_key TEXT NOT NULL PRIMARY KEY,
    room_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    first_message_index BIGINT NOT NULL,
    forwarded_count BIGINT NOT NULL DEFAULT 0,
    is_verified BOOLEAN NOT NULL DEFAULT FALSE,
    session_data TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| session_key | TEXT | PRIMARY KEY | 会话密钥 |
| room_id | TEXT | NOT NULL | 房间 ID |
| session_id | TEXT | NOT NULL | 会话 ID |
| first_message_index | BIGINT | NOT NULL | 首个消息索引 |
| forwarded_count | BIGINT | NOT NULL, DEFAULT 0 | 转发次数 |
| is_verified | BOOLEAN | NOT NULL, DEFAULT FALSE | 是否已验证 |
| session_data | TEXT | NOT NULL | 会话数据 |
| created_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 创建时间 |

**索引配置**：
- PRIMARY KEY (session_key)
- INDEX (room_id)
- INDEX (session_id)

---

## 七、推送通知表结构

### 7.1 推送器表（pushers）

推送器表管理消息推送配置，每个用户可以有多个推送器关联不同的推送服务。

```sql
CREATE TABLE IF NOT EXISTS pushers (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    access_token TEXT NOT NULL,
    profile_tag TEXT,
    kind TEXT NOT NULL,
    app_id TEXT NOT NULL,
    app_display_name TEXT,
    device_name TEXT,
    pushkey TEXT NOT NULL,
    ts BIGINT NOT NULL,
    language TEXT,
    data TEXT,
    expiry_ts BIGINT,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| user_id | TEXT | NOT NULL, FOREIGN KEY | 用户 ID |
| access_token | TEXT | NOT NULL | 关联访问令牌 |
| profile_tag | TEXT | NULLABLE | 配置标签 |
| kind | TEXT | NOT NULL | 推送器类型 |
| app_id | TEXT | NOT NULL | 应用 ID |
| app_display_name | TEXT | NULLABLE | 应用显示名称 |
| device_name | TEXT | NULLABLE | 设备名称 |
| pushkey | TEXT | NOT NULL | 推送密钥 |
| ts | BIGINT | NOT NULL | 时间戳 |
| language | TEXT | NULLABLE | 语言设置 |
| data | TEXT | NULLABLE | 附加数据 |
| expiry_ts | BIGINT | NULLABLE | 过期时间戳 |

**索引配置**：
- PRIMARY KEY (id)
- INDEX (user_id)
- INDEX (pushkey)
- 建议添加 INDEX (user_id, kind) 用于按类型查询推送器

### 7.2 推送规则表（push_rules）

推送规则表定义消息推送的过滤规则，支持用户自定义推送策略。

```sql
CREATE TABLE IF NOT EXISTS push_rules (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    rule_id TEXT NOT NULL,
    priority_class INTEGER NOT NULL DEFAULT 0,
    priority INTEGER NOT NULL DEFAULT 0,
    conditions TEXT,
    actions TEXT,
    is_default_rule BOOLEAN DEFAULT FALSE,
    is_enabled BOOLEAN DEFAULT TRUE,
    is_user_created BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| user_id | TEXT | NOT NULL, FOREIGN KEY | 用户 ID |
| rule_id | TEXT | NOT NULL | 规则 ID |
| priority_class | INTEGER | NOT NULL, DEFAULT 0 | 优先级类别 |
| priority | INTEGER | NOT NULL, DEFAULT 0 | 优先级 |
| conditions | TEXT | NULLABLE | 触发条件（JSON） |
| actions | TEXT | NULLABLE | 执行动作（JSON） |
| is_default_rule | BOOLEAN | DEFAULT FALSE | 是否默认规则 |
| is_enabled | BOOLEAN | DEFAULT TRUE | 是否启用 |
| is_user_created | BOOLEAN | DEFAULT FALSE | 是否用户创建 |
| created_ts | BIGINT | NOT NULL | 创建时间戳 |

**索引配置**：
- PRIMARY KEY (id)
- INDEX (user_id)
- INDEX (priority_class, priority)
- 建议添加 UNIQUE INDEX (user_id, rule_id)

### 7.3 用户推送规则表（push_rules_user_sent_rules）

用户推送规则表记录用户对系统默认规则的个性化设置。

```sql
CREATE TABLE IF NOT EXISTS push_rules_user_sent_rules (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    rule_id TEXT NOT NULL,
    enable BOOLEAN DEFAULT TRUE,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| user_id | TEXT | NOT NULL, FOREIGN KEY | 用户 ID |
| rule_id | TEXT | NOT NULL | 规则 ID |
| enable | BOOLEAN | DEFAULT TRUE | 是否启用 |

**索引配置**：
- PRIMARY KEY (id)
- INDEX (user_id)

### 7.4 推送节流表（pusher_throttle）

推送节流表实现推送频率控制，防止推送风暴。

```sql
CREATE TABLE IF NOT EXISTS pusher_throttle (
    pusher TEXT NOT NULL PRIMARY KEY,
    last_sent_ts BIGINT NOT NULL,
    throttle_ms INTEGER NOT NULL DEFAULT 0
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| pusher | TEXT | PRIMARY KEY | 推送器标识 |
| last_sent_ts | BIGINT | NOT NULL | 最后发送时间戳 |
| throttle_ms | INTEGER | NOT NULL, DEFAULT 0 | 节流时间（毫秒） |

---

## 八、房间扩展表结构

### 8.1 房间别名表（room_aliases）

房间别名表存储房间的替代别名，支持用户使用友好的别名加入房间。

```sql
CREATE TABLE IF NOT EXISTS room_aliases (
    room_id TEXT NOT NULL,
    alias TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    PRIMARY KEY (room_id, alias),
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| room_id | TEXT | PRIMARY KEY, FOREIGN KEY | 房间 ID |
| alias | TEXT | PRIMARY KEY | 别名 |
| created_ts | BIGINT | NOT NULL | 创建时间戳 |

**索引配置**：
- PRIMARY KEY (room_id, alias)
- UNIQUE INDEX (alias)

### 8.2 房间状态表（room_state）

房间状态表记录房间的状态事件，用于状态追踪和管理。

```sql
CREATE TABLE IF NOT EXISTS room_state (
    room_id TEXT NOT NULL,
    type TEXT NOT NULL,
    state_key TEXT NOT NULL,
    event_id TEXT NOT NULL,
    depth BIGINT NOT NULL,
    stream_ordering BIGINT,
    PRIMARY KEY (room_id, type, state_key),
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| room_id | TEXT | PRIMARY KEY, FOREIGN KEY | 房间 ID |
| type | TEXT | PRIMARY KEY | 状态类型 |
| state_key | TEXT | PRIMARY KEY | 状态键 |
| event_id | TEXT | NOT NULL | 事件 ID |
| depth | BIGINT | NOT NULL | 深度 |
| stream_ordering | BIGINT | NULLABLE | 流排序 |

**索引配置**：
- PRIMARY KEY (room_id, type, state_key)
- INDEX (event_id)

### 8.3 房间密钥分发表（room_key_distributions）

房间密钥分发表追踪密钥分发历史，用于安全审计。

```sql
CREATE TABLE IF NOT EXISTS room_key_distributions (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    session_key TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| room_id | TEXT | NOT NULL, FOREIGN KEY | 房间 ID |
| event_id | TEXT | NOT NULL | 事件 ID |
| user_id | TEXT | NOT NULL, FOREIGN KEY | 目标用户 |
| session_key | TEXT | NOT NULL | 会话密钥 |
| created_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 创建时间 |

**索引配置**：
- PRIMARY KEY (id)
- INDEX (room_id)
- INDEX (user_id)

### 8.4 回执表（receipts）

回执表存储消息已读回执信息，用于标记消息阅读状态。

```sql
CREATE TABLE IF NOT EXISTS receipts (
    sender TEXT NOT NULL,
    sent_to TEXT NOT NULL,
    room_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    sent_ts BIGINT NOT NULL,
    receipt_type TEXT NOT NULL,
    PRIMARY KEY (sent_to, sender, room_id),
    FOREIGN KEY (sender) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (sent_to) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| sender | TEXT | PRIMARY KEY, FOREIGN KEY | 发送者 |
| sent_to | TEXT | PRIMARY KEY, FOREIGN KEY | 接收者 |
| room_id | TEXT | PRIMARY KEY, FOREIGN KEY | 房间 ID |
| event_id | TEXT | NOT NULL | 已读事件 ID |
| sent_ts | BIGINT | NOT NULL | 回执时间戳 |
| receipt_type | TEXT | NOT NULL | 回执类型 |

**索引配置**：
- PRIMARY KEY (sent_to, sender, room_id)
- INDEX (room_id)
- INDEX (sent_ts)

---

## 九、系统维护表结构

### 9.1 IP 黑名单表（ip_blocks）

IP 黑名单表记录被封禁的 IP 地址，用于访问控制。

```sql
CREATE TABLE IF NOT EXISTS ip_blocks (
    id BIGSERIAL PRIMARY KEY,
    ip TEXT NOT NULL,
    reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NULLABLE
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| ip | TEXT | NOT NULL | IP 地址 |
| reason | TEXT | NULLABLE | 封禁原因 |
| created_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 创建时间 |
| expires_at | TIMESTAMPTZ | NULLABLE | 过期时间 |

**索引配置**：
- PRIMARY KEY (id)
- INDEX (ip)
- INDEX (expires_at)

### 9.2 IP 信誉表（ip_reputation）

IP 信誉表记录 IP 地址的信誉评分，用于风险评估。

```sql
CREATE TABLE IF NOT EXISTS ip_reputation (
    ip TEXT NOT NULL PRIMARY KEY,
    reputation_score INTEGER NOT NULL DEFAULT 50,
    last_updated TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| ip | TEXT | PRIMARY KEY | IP 地址 |
| reputation_score | INTEGER | NOT NULL, DEFAULT 50 | 信誉评分 |
| last_updated | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 最后更新 |

### 9.3 安全事件表（security_events）

安全事件表记录系统安全相关事件，用于安全审计。

```sql
CREATE TABLE IF NOT EXISTS security_events (
    id BIGSERIAL PRIMARY KEY,
    event_type TEXT NOT NULL,
    user_id TEXT,
    ip TEXT,
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| event_type | TEXT | NOT NULL | 事件类型 |
| user_id | TEXT | NULLABLE | 关联用户 |
| ip | TEXT | NULLABLE | IP 地址 |
| description | TEXT | NULLABLE | 事件描述 |
| created_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 创建时间 |

**索引配置**：
- PRIMARY KEY (id)
- INDEX (event_type)
- INDEX (user_id)
- INDEX (created_at DESC)

### 9.4 数据库元数据表（db_metadata）

数据库元数据表存储数据库版本和迁移状态信息。

```sql
CREATE TABLE IF NOT EXISTS db_metadata (
    id BIGSERIAL PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    value TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| key | TEXT | UNIQUE, NOT NULL | 键 |
| value | TEXT | NULLABLE | 值 |
| created_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 创建时间 |
| updated_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 更新时间 |

**索引配置**：
- PRIMARY KEY (id)
- UNIQUE INDEX (key)

### 9.5 密钥变更表（key_changes）

密钥变更表记录用户密钥的变更历史，用于密钥追踪。

```sql
CREATE TABLE IF NOT EXISTS key_changes (
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    changed_at BIGINT NOT NULL,
    change_type TEXT NOT NULL,
    PRIMARY KEY (user_id, device_id, changed_at)
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| user_id | TEXT | PRIMARY KEY | 用户 ID |
| device_id | TEXT | PRIMARY KEY | 设备 ID |
| changed_at | BIGINT | PRIMARY KEY | 变更时间戳 |
| change_type | TEXT | NOT NULL | 变更类型 |

**索引配置**：
- PRIMARY KEY (user_id, device_id, changed_at)
- INDEX (changed_at DESC)

### 9.6 速率限制分片表（ratelimit_shard）

速率限制分片表实现分布式速率限制的分片管理。

```sql
CREATE TABLE IF NOT EXISTS ratelimit_shard (
    user_id TEXT NOT NULL PRIMARY KEY,
    shard_id INTEGER NOT NULL
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| user_id | TEXT | PRIMARY KEY | 用户 ID |
| shard_id | INTEGER | NOT NULL | 分片 ID |

### 9.7 用户房间关联表（user_rooms）

用户房间关联表记录用户参与的房间列表，用于快速查询。

```sql
CREATE TABLE IF NOT EXISTS user_rooms (
    user_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    PRIMARY KEY (user_id, room_id),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| user_id | TEXT | PRIMARY KEY, FOREIGN KEY | 用户 ID |
| room_id | TEXT | PRIMARY KEY, FOREIGN KEY | 房间 ID |
| created_ts | BIGINT | NOT NULL | 加入时间戳 |

**索引配置**：
- PRIMARY KEY (user_id, room_id)
- INDEX (room_id)

### 9.8 用户过滤器表（user_filters）

用户过滤器表存储用户的过滤器定义，用于消息过滤。

```sql
CREATE TABLE IF NOT EXISTS user_filters (
    user_id TEXT NOT NULL,
    filter_id BIGINT NOT NULL,
    filter_definition TEXT NOT NULL,
    PRIMARY KEY (user_id, filter_id),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| user_id | TEXT | PRIMARY KEY, FOREIGN KEY | 用户 ID |
| filter_id | BIGINT | PRIMARY KEY | 过滤器 ID |
| filter_definition | TEXT | NOT NULL | 过滤器定义（JSON） |

**索引配置**：
- PRIMARY KEY (user_id, filter_id)

### 9.9 用户 IP 表（user_ips）

用户 IP 表记录用户的 IP 地址历史，用于安全审计。

```sql
CREATE TABLE IF NOT EXISTS user_ips (
    user_id TEXT NOT NULL,
    access_token TEXT NOT NULL,
    ip TEXT NOT NULL,
    user_agent TEXT,
    device_id TEXT NOT NULL,
    last_seen BIGINT NOT NULL,
    first_seen BIGINT NOT NULL DEFAULT 0
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| user_id | TEXT | NOT NULL | 用户 ID |
| access_token | TEXT | NOT NULL | 访问令牌 |
| ip | TEXT | NOT NULL | IP 地址 |
| user_agent | TEXT | NULLABLE | 用户代理 |
| device_id | TEXT | NOT NULL | 设备 ID |
| last_seen | BIGINT | NOT NULL | 最后访问时间戳 |
| first_seen | BIGINT | NOT NULL, DEFAULT 0 | 首次访问时间戳 |

**索引配置**：
- INDEX (user_id)
- INDEX (last_seen DESC)
- 建议添加 INDEX (ip) 用于 IP 查询

### 9.10 用户目录表（user_directory）

用户目录表管理用户的房间可见性设置。

```sql
CREATE TABLE IF NOT EXISTS user_directory (
    user_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    visibility TEXT NOT NULL DEFAULT 'private',
    added_by TEXT,
    created_ts BIGINT NOT NULL,
    PRIMARY KEY (user_id, room_id),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| user_id | TEXT | PRIMARY KEY, FOREIGN KEY | 用户 ID |
| room_id | TEXT | PRIMARY KEY, FOREIGN KEY | 房间 ID |
| visibility | TEXT | NOT NULL, DEFAULT 'private' | 可见性 |
| added_by | TEXT | NULLABLE | 添加者 |
| created_ts | BIGINT | NOT NULL | 创建时间戳 |

**索引配置**：
- PRIMARY KEY (user_id, room_id)

### 9.11 正在输入表（typing）

正在输入表记录房间中用户的正在输入状态。

```sql
CREATE TABLE IF NOT EXISTS typing (
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    last_updated TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (room_id, user_id),
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);
```

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|------|------|
| room_id | TEXT | PRIMARY KEY, FOREIGN KEY | 房间 ID |
| user_id | TEXT | PRIMARY KEY, FOREIGN KEY | 用户 ID |
| last_updated | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 最后更新 |

**索引配置**：
- PRIMARY KEY (room_id, user_id)

---

## 十、表关系总览

### 10.1 实体关系图概述

系统中的表遵循以下主要关系模式：

**用户核心关系**：users 表作为核心，所有与用户相关的表都通过外键关联。devices、access_tokens、refresh_tokens 等表通过 user_id 关联到 users 表，实现用户为中心的星型结构。

**房间关系**：rooms 表与 room_memberships、room_events、current_state_events 形成房间为中心的层级结构。room_memberships 表的复合主键 `(room_id, user_id)` 确保每个用户在每个房间只有一条成员记录。

**加密密钥关系**：device_keys、cross_signing_keys、one_time_keys 等加密相关表形成以用户和设备为中心的层级结构，支持端到端加密的完整密钥管理流程。

**消息关系**：private_sessions 作为会话中心，private_messages 关联到会话。用户通过 friend_requests 建立社交连接，通过 friends 维护好友关系。

### 10.2 外键约束链

| 父表 | 子表 | 删除规则 | 说明 |
|------|------|----------|------|
| users | devices | CASCADE | 删除用户时删除所有设备 |
| users | access_tokens | CASCADE | 删除用户时删除所有访问令牌 |
| users | refresh_tokens | CASCADE | 删除用户时删除所有刷新令牌 |
| users | room_memberships | CASCADE | 删除用户时删除所有成员关系 |
| users | private_messages | CASCADE | 删除用户时删除所有私聊消息 |
| devices | access_tokens | CASCADE | 删除设备时删除关联令牌 |
| devices | device_keys | CASCADE | 删除设备时删除设备密钥 |
| devices | refresh_tokens | CASCADE | 删除设备时删除刷新令牌 |
| rooms | room_memberships | CASCADE | 删除房间时删除所有成员关系 |
| rooms | room_events | CASCADE | 删除房间时删除所有事件 |
| rooms | current_state_events | CASCADE | 删除房间时删除状态事件 |
| private_sessions | private_messages | CASCADE | 删除会话时删除所有消息 |

---

## 十一、索引优化策略

### 11.1 核心查询索引

根据系统的查询模式，建议创建以下核心索引以优化性能：

```sql
-- 房间成员列表查询优化
CREATE INDEX IF NOT EXISTS idx_room_memberships_room_membership_joined 
ON room_memberships(room_id, membership, joined_ts DESC);

-- 房间消息历史查询优化
CREATE INDEX IF NOT EXISTS idx_room_events_room_type_ts 
ON room_events(room_id, event_type, origin_server_ts DESC);

-- 私聊消息列表查询优化
CREATE INDEX IF NOT EXISTS idx_private_messages_session_read 
ON private_messages(session_id, created_ts DESC, is_read);

-- 用户会话列表查询优化
CREATE INDEX IF NOT EXISTS idx_private_sessions_user1_updated 
ON private_sessions(user_id_1, updated_ts DESC);

CREATE INDEX IF NOT EXISTS idx_private_sessions_user2_updated 
ON private_sessions(user_id_2, updated_ts DESC);

-- 好友列表查询优化
CREATE INDEX IF NOT EXISTS idx_friends_user_favorite 
ON friends(user_id, is_favorite DESC, created_ts DESC);

-- 好友请求处理查询优化
CREATE INDEX IF NOT EXISTS idx_friend_requests_target_status 
ON friend_requests(target_id, status);

-- 设备最后活跃查询优化
CREATE INDEX IF NOT EXISTS idx_devices_user_last_seen 
ON devices(user_id, last_seen_ts DESC);

-- 事件类型聚合查询优化
CREATE INDEX IF NOT EXISTS idx_room_events_type_count 
ON room_events(room_id, event_type) WHERE redacted = FALSE;
```

### 11.2 部分索引

针对特定查询模式创建部分索引，减少索引大小并提高查询效率：

```sql
-- 仅索引未删除事件
CREATE INDEX IF NOT EXISTS idx_room_events_not_redacted 
ON room_events(room_id, origin_server_ts DESC) 
WHERE redacted = FALSE;

-- 仅索引未读消息
CREATE INDEX IF NOT EXISTS idx_private_messages_unread 
ON private_messages(session_id, created_ts DESC) 
WHERE is_read = FALSE AND read_by_receiver = FALSE;

-- 仅索引有效会话
CREATE INDEX IF NOT EXISTS idx_private_sessions_valid 
ON private_sessions(user_id_1, updated_ts DESC) 
WHERE unread_count > 0;

-- 仅索引活跃的推送器
CREATE INDEX IF NOT EXISTS idx_pushers_active 
ON pushers(user_id, kind) 
WHERE expiry_ts IS NULL OR expiry_ts > NOW();
```

### 11.3 复合索引优先级

在设计索引时，应优先考虑以下查询模式的复合索引：

| 优先级 | 查询模式 | 推荐索引 |
|--------|----------|----------|
| 高 | 按房间查询成员列表 | `(room_id, membership, joined_ts DESC)` |
| 高 | 按房间查询消息历史 | `(room_id, origin_server_ts DESC)` |
| 高 | 按用户查询会话列表 | `(user_id_1, updated_ts DESC)` |
| 高 | 按用户查询好友列表 | `(user_id, is_favorite DESC)` |
| 中 | 按房间查询状态事件 | `(room_id, type)` |
| 中 | 按设备查询密钥 | `(user_id, device_id)` |
| 低 | 按时间范围查询事件 | `(room_id, origin_server_ts)` |

---

## 十二、变更日志

| 版本 | 日期 | 变更说明 |
|------|------|----------|
| 1.0.0 | 2026-01-28 | 初始版本，包含基础表结构定义 |
| 1.2.0 | 2026-01-29 | 补充端到端加密和推送通知相关表 |
| 2.0.0 | 2026-01-30 | 全面重构，与代码实现完全一致，添加完整索引策略 |
