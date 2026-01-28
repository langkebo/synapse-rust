# 数据模型文档

> **版本**：1.0.0  
> **创建日期**：2026-01-28  
> **项目状态**：开发中  
> **参考文档**：[Synapse 官方文档](https://element-hq.github.io/synapse/latest/)、[Matrix 规范](https://spec.matrix.org/)

---

## 一、数据库表结构

### 1.1 用户表 (users)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| user_id | VARCHAR(255) | PRIMARY KEY | 用户 ID |
| username | VARCHAR(255) | UNIQUE, NOT NULL | 用户名 |
| password_hash | VARCHAR(255) | NULLABLE | 密码哈希 |
| displayname | VARCHAR(255) | NULLABLE | 显示名称 |
| avatar_url | VARCHAR(255) | NULLABLE | 头像 URL |
| admin | BOOLEAN | NOT NULL, DEFAULT false | 是否管理员 |
| deactivated | BOOLEAN | NOT NULL, DEFAULT false | 是否停用 |
| is_guest | BOOLEAN | NOT NULL, DEFAULT false | 是否访客 |
| consent_version | VARCHAR(255) | NULLABLE | 同意版本 |
| appservice_id | VARCHAR(255) | NULLABLE | 应用服务 ID |
| user_type | VARCHAR(255) | NULLABLE | 用户类型 |
| shadow_banned | BOOLEAN | NOT NULL, DEFAULT false | 是否被影子封禁 |
| generation | BIGINT | NOT NULL, DEFAULT 0 | 生成号 |
| invalid_update_ts | BIGINT | NULLABLE | 无效更新时间戳 |
| migration_state | VARCHAR(255) | NULLABLE | 迁移状态 |
| creation_ts | BIGINT | NOT NULL | 创建时间戳（毫秒） |

**索引**：
- PRIMARY KEY (user_id)
- UNIQUE INDEX (username)

### 1.2 设备表 (devices)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| device_id | VARCHAR(255) | PRIMARY KEY | 设备 ID |
| user_id | VARCHAR(255) | NOT NULL, FOREIGN KEY | 用户 ID |
| display_name | VARCHAR(255) | NULLABLE | 显示名称 |
| last_seen_ts | BIGINT | NOT NULL | 最后见时间戳（毫秒） |
| last_seen_ip | VARCHAR(255) | NULLABLE | 最后见 IP 地址 |
| created_ts | BIGINT | NOT NULL | 创建时间戳（毫秒） |
| ignored_user_list | VARCHAR(255) | NULLABLE | 忽略用户列表 |
| appservice_id | VARCHAR(255) | NULLABLE | 应用服务 ID |
| first_seen_ts | BIGINT | NOT NULL | 首次见时间戳（毫秒） |

**索引**：
- PRIMARY KEY (device_id)
- INDEX (user_id)
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE

### 1.3 访问令牌表 (access_tokens)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| token | VARCHAR(255) | UNIQUE, NOT NULL | 访问令牌 |
| user_id | VARCHAR(255) | NOT NULL, FOREIGN KEY | 用户 ID |
| device_id | VARCHAR(255) | NULLABLE, FOREIGN KEY | 设备 ID |
| created_ts | BIGINT | NOT NULL | 创建时间戳（毫秒） |
| expires_ts | BIGINT | NULLABLE | 过期时间戳（毫秒） |
| invalidated_ts | BIGINT | NULLABLE | 失效时间戳（毫秒） |
| expired_ts | BIGINT | NULLABLE | 过期时间戳（毫秒） |

**索引**：
- PRIMARY KEY (id)
- UNIQUE INDEX (token)
- INDEX (user_id)
- INDEX (device_id)
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
- FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE CASCADE

### 1.4 刷新令牌表 (refresh_tokens)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| token | VARCHAR(255) | UNIQUE, NOT NULL | 刷新令牌 |
| user_id | VARCHAR(255) | NOT NULL, FOREIGN KEY | 用户 ID |
| device_id | VARCHAR(255) | NOT NULL, FOREIGN KEY | 设备 ID |
| created_ts | BIGINT | NOT NULL | 创建时间戳（毫秒） |
| expires_ts | BIGINT | NULLABLE | 过期时间戳（毫秒） |
| invalidated_ts | BIGINT | NULLABLE | 失效时间戳（毫秒） |
| expired_ts | BIGINT | NULLABLE | 过期时间戳（毫秒） |

**索引**：
- PRIMARY KEY (id)
- UNIQUE INDEX (token)
- INDEX (user_id)
- INDEX (device_id)
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
- FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE CASCADE

### 1.5 房间表 (rooms)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| room_id | VARCHAR(255) | PRIMARY KEY | 房间 ID |
| is_public | BOOLEAN | NOT NULL, DEFAULT false | 是否公开 |
| creator | VARCHAR(255) | NOT NULL | 创建者 |
| creation_ts | BIGINT | NOT NULL | 创建时间戳（毫秒） |
| federate | BOOLEAN | NOT NULL, DEFAULT true | 是否允许联邦 |
| version | VARCHAR(255) | NOT NULL | 版本 |
| name | VARCHAR(255) | NULLABLE | 房间名称 |
| topic | VARCHAR(255) | NULLABLE | 房间主题 |
| avatar | VARCHAR(255) | NULLABLE | 房间头像 |
| canonical_alias | VARCHAR(255) | NULLABLE | 规范别名 |
| guest_access | BOOLEAN | NOT NULL, DEFAULT false | 访客访问 |
| history_visibility | VARCHAR(255) | NOT NULL, DEFAULT 'shared' | 历史可见性 |
| encryption | VARCHAR(255) | NULLABLE | 加密 |
| is_flaged | BOOLEAN | NOT NULL, DEFAULT false | 是否标记 |
| is_spotlight | BOOLEAN | NOT NULL, DEFAULT false | 是否聚光灯 |
| deleted_ts | BIGINT | NULLABLE | 删除时间戳（毫秒） |
| join_rule | VARCHAR(255) | NULLABLE | 加入规则 |
| member_count | INTEGER | NOT NULL, DEFAULT 0 | 成员数量 |

**索引**：
- PRIMARY KEY (room_id)
- INDEX (creator)
- INDEX (canonical_alias)

### 1.6 事件表 (events)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| event_id | VARCHAR(255) | PRIMARY KEY | 事件 ID |
| room_id | VARCHAR(255) | NOT NULL, FOREIGN KEY | 房间 ID |
| user_id | VARCHAR(255) | NOT NULL, FOREIGN KEY | 用户 ID |
| event_type | VARCHAR(255) | NOT NULL | 事件类型 |
| content | JSONB | NOT NULL | 事件内容 |
| state_key | VARCHAR(255) | NULLABLE | 状态键 |
| depth | BIGINT | NOT NULL | 深度 |
| origin_server_ts | BIGINT | NOT NULL | 源服务器时间戳（毫秒） |
| processed_ts | BIGINT | NOT NULL | 处理时间戳（毫秒） |
| not_before | BIGINT | NULLABLE | 不早于 |
| status | VARCHAR(255) | NULLABLE | 状态 |
| reference_image | VARCHAR(255) | NULLABLE | 参考图片 |
| origin | VARCHAR(255) | NOT NULL | 源服务器 |

**索引**：
- PRIMARY KEY (event_id)
- INDEX (room_id)
- INDEX (user_id)
- INDEX (origin_server_ts)
- INDEX (event_type)
- FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE

### 1.7 成员关系表 (room_memberships)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| room_id | VARCHAR(255) | NOT NULL, FOREIGN KEY | 房间 ID |
| user_id | VARCHAR(255) | NOT NULL, FOREIGN KEY | 用户 ID |
| sender | VARCHAR(255) | NOT NULL | 发送者 |
| membership | VARCHAR(255) | NOT NULL | 成员关系（join, leave, ban, invite, knock） |
| event_id | VARCHAR(255) | NOT NULL | 事件 ID |
| event_type | VARCHAR(255) | NOT NULL | 事件类型 |
| display_name | VARCHAR(255) | NULLABLE | 显示名称 |
| avatar_url | VARCHAR(255) | NULLABLE | 头像 URL |
| is_banned | BOOLEAN | NULLABLE | 是否被封禁 |
| invite_token | VARCHAR(255) | NULLABLE | 邀请令牌 |
| inviter | VARCHAR(255) | NULLABLE | 邀请者 |
| updated_ts | BIGINT | NULLABLE | 更新时间戳（毫秒） |
| joined_ts | BIGINT | NULLABLE | 加入时间戳（毫秒） |
| left_ts | BIGINT | NULLABLE | 离开时间戳（毫秒） |
| reason | VARCHAR(255) | NULLABLE | 原因 |
| join_reason | VARCHAR(255) | NULLABLE | 加入原因 |
| banned_by | VARCHAR(255) | NULLABLE | 封禁者 |

**索引**：
- PRIMARY KEY (room_id, user_id)
- INDEX (user_id)
- INDEX (membership)
- INDEX (event_id)
- FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE

### 1.8 在线状态表 (presence)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| user_id | VARCHAR(255) | PRIMARY KEY, FOREIGN KEY | 用户 ID |
| presence | VARCHAR(255) | NOT NULL | 在线状态（online, offline, unavailable） |
| status_msg | VARCHAR(255) | NULLABLE | 状态消息 |
| last_active_ts | BIGINT | NOT NULL | 最后活跃时间戳（毫秒） |
| currently_active | BOOLEAN | NOT NULL, DEFAULT false | 当前活跃 |

**索引**：
- PRIMARY KEY (user_id)
- INDEX (presence)
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE

### 1.9 好友表 (friends)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| user_id | VARCHAR(255) | NOT NULL, FOREIGN KEY | 用户 ID |
| friend_id | VARCHAR(255) | NOT NULL, FOREIGN KEY | 好友 ID |
| category | VARCHAR(255) | NULLABLE | 分类 |
| added_at | BIGINT | NOT NULL | 添加时间戳（毫秒） |

**索引**：
- PRIMARY KEY (user_id, friend_id)
- INDEX (friend_id)
- INDEX (category)
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
- FOREIGN KEY (friend_id) REFERENCES users(user_id) ON DELETE CASCADE

### 1.10 好友请求表 (friend_requests)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| request_id | VARCHAR(255) | PRIMARY KEY | 请求 ID |
| from_user_id | VARCHAR(255) | NOT NULL, FOREIGN KEY | 发送者 ID |
| to_user_id | VARCHAR(255) | NOT NULL, FOREIGN KEY | 接收者 ID |
| message | VARCHAR(255) | NULLABLE | 消息 |
| status | VARCHAR(255) | NOT NULL | 状态（pending, accepted, rejected） |
| created_at | BIGINT | NOT NULL | 创建时间戳（毫秒） |
| responded_at | BIGINT | NULLABLE | 响应时间戳（毫秒） |

**索引**：
- PRIMARY KEY (request_id)
- INDEX (from_user_id)
- INDEX (to_user_id)
- INDEX (status)
- FOREIGN KEY (from_user_id) REFERENCES users(user_id) ON DELETE CASCADE
- FOREIGN KEY (to_user_id) REFERENCES users(user_id) ON DELETE CASCADE

### 1.11 好友分类表 (friend_categories)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| user_id | VARCHAR(255) | NOT NULL, FOREIGN KEY | 用户 ID |
| category_name | VARCHAR(255) | NOT NULL | 分类名称 |
| display_name | VARCHAR(255) | NULLABLE | 显示名称 |
| color | VARCHAR(255) | NULLABLE | 颜色 |
| icon | VARCHAR(255) | NULLABLE | 图标 |
| created_at | BIGINT | NOT NULL | 创建时间戳（毫秒） |

**索引**：
- PRIMARY KEY (user_id, category_name)
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE

### 1.12 黑名单表 (blocked_users)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| user_id | VARCHAR(255) | NOT NULL, FOREIGN KEY | 用户 ID |
| blocked_user_id | VARCHAR(255) | NOT NULL, FOREIGN KEY | 被封禁用户 ID |
| reason | VARCHAR(255) | NULLABLE | 原因 |
| blocked_at | BIGINT | NOT NULL | 封禁时间戳（毫秒） |

**索引**：
- PRIMARY KEY (user_id, blocked_user_id)
- INDEX (blocked_user_id)
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
- FOREIGN KEY (blocked_user_id) REFERENCES users(user_id) ON DELETE CASCADE

### 1.13 私聊会话表 (private_sessions)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| session_id | VARCHAR(255) | UNIQUE, NOT NULL | 会话 ID |
| creator_id | VARCHAR(255) | NOT NULL, FOREIGN KEY | 创建者 ID |
| participant_id | VARCHAR(255) | NOT NULL, FOREIGN KEY | 参与者 ID |
| session_name | VARCHAR(255) | NULLABLE | 会话名称 |
| ttl_seconds | INTEGER | NULLABLE | TTL（秒） |
| auto_delete | BOOLEAN | NULLABLE | 自动删除 |
| created_at | TIMESTAMP | NOT NULL | 创建时间 |

**索引**：
- PRIMARY KEY (id)
- UNIQUE INDEX (session_id)
- INDEX (creator_id)
- INDEX (participant_id)
- FOREIGN KEY (creator_id) REFERENCES users(user_id) ON DELETE CASCADE
- FOREIGN KEY (participant_id) REFERENCES users(user_id) ON DELETE CASCADE

### 1.14 私聊消息表 (private_messages)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| message_id | VARCHAR(255) | UNIQUE, NOT NULL | 消息 ID |
| session_id | VARCHAR(255) | NOT NULL, FOREIGN KEY | 会话 ID |
| sender_id | VARCHAR(255) | NOT NULL, FOREIGN KEY | 发送者 ID |
| content | TEXT | NOT NULL | 消息内容 |
| encrypted | BOOLEAN | NOT NULL, DEFAULT false | 是否加密 |
| ttl_seconds | INTEGER | NULLABLE | TTL（秒） |
| created_at | TIMESTAMP | NOT NULL | 创建时间 |
| read_at | TIMESTAMP | NULLABLE | 已读时间 |

**索引**：
- PRIMARY KEY (id)
- UNIQUE INDEX (message_id)
- INDEX (session_id)
- INDEX (sender_id)
- INDEX (created_at)
- FOREIGN KEY (session_id) REFERENCES private_sessions(session_id) ON DELETE CASCADE
- FOREIGN KEY (sender_id) REFERENCES users(user_id) ON DELETE CASCADE

### 1.15 会话密钥表 (session_keys)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| session_id | VARCHAR(255) | NOT NULL, FOREIGN KEY | 会话 ID |
| key_data | TEXT | NOT NULL | 密钥数据 |
| created_at | TIMESTAMP | NOT NULL | 创建时间 |
| expires_at | TIMESTAMP | NULLABLE | 过期时间 |

**索引**：
- PRIMARY KEY (id)
- INDEX (session_id)
- FOREIGN KEY (session_id) REFERENCES private_sessions(session_id) ON DELETE CASCADE

### 1.16 语音消息表 (voice_messages)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| message_id | VARCHAR(255) | UNIQUE, NOT NULL | 消息 ID |
| user_id | VARCHAR(255) | NOT NULL, FOREIGN KEY | 用户 ID |
| room_id | VARCHAR(255) | NULLABLE, FOREIGN KEY | 房间 ID |
| file_format | VARCHAR(255) | NOT NULL | 文件格式 |
| file_size | BIGINT | NOT NULL | 文件大小 |
| duration | INTEGER | NOT NULL | 时长（秒） |
| file_url | VARCHAR(255) | NOT NULL | 文件 URL |
| created_at | TIMESTAMP | NOT NULL | 创建时间 |

**索引**：
- PRIMARY KEY (id)
- UNIQUE INDEX (message_id)
- INDEX (user_id)
- INDEX (room_id)
- INDEX (created_at)
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
- FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE

### 1.17 安全事件表 (security_events)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| event_type | VARCHAR(255) | NOT NULL | 事件类型 |
| user_id | VARCHAR(255) | NULLABLE, FOREIGN KEY | 用户 ID |
| ip_address | VARCHAR(255) | NULLABLE | IP 地址 |
| user_agent | VARCHAR(255) | NULLABLE | 用户代理 |
| details | JSONB | NULLABLE | 详情 |
| created_at | TIMESTAMP | NOT NULL | 创建时间 |

**索引**：
- PRIMARY KEY (id)
- INDEX (event_type)
- INDEX (user_id)
- INDEX (ip_address)
- INDEX (created_at)
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE

### 1.18 IP 阻止表 (ip_blocks)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| ip_address | VARCHAR(255) | UNIQUE, NOT NULL | IP 地址 |
| reason | VARCHAR(255) | NULLABLE | 原因 |
| blocked_at | TIMESTAMP | NOT NULL | 封禁时间 |
| expires_at | TIMESTAMP | NULLABLE | 过期时间 |

**索引**：
- PRIMARY KEY (id)
- UNIQUE INDEX (ip_address)
- INDEX (blocked_at)

### 1.19 IP 声誉表 (ip_reputation)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| ip_address | VARCHAR(255) | UNIQUE, NOT NULL | IP 地址 |
| score | INTEGER | NOT NULL, DEFAULT 0 | 声誉分数 |
| last_seen_at | TIMESTAMP | NOT NULL | 最后见时间 |
| updated_at | TIMESTAMP | NOT NULL | 更新时间 |

**索引**：
- PRIMARY KEY (id)
- UNIQUE INDEX (ip_address)
- INDEX (score)

---

## 二、Rust 结构体定义

### 2.1 User 结构体

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct User {
    pub user_id: String,
    pub username: String,
    pub password_hash: Option<String>,
    pub displayname: Option<String>,
    pub avatar_url: Option<String>,
    pub admin: bool,
    pub deactivated: bool,
    pub is_guest: bool,
    pub consent_version: Option<String>,
    pub appservice_id: Option<String>,
    pub user_type: Option<String>,
    pub shadow_banned: bool,
    pub generation: i64,
    pub invalid_update_ts: Option<i64>,
    pub migration_state: Option<String>,
    pub creation_ts: i64,
}
```

### 2.2 Device 结构体

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Device {
    pub device_id: String,
    pub user_id: String,
    pub display_name: Option<String>,
    pub last_seen_ts: i64,
    pub last_seen_ip: Option<String>,
    pub created_ts: i64,
    pub ignored_user_list: Option<String>,
    pub appservice_id: Option<String>,
    pub first_seen_ts: i64,
}
```

### 2.3 AccessToken 结构体

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AccessToken {
    pub id: i64,
    pub token: String,
    pub user_id: String,
    pub device_id: Option<String>,
    pub created_ts: i64,
    pub expires_ts: Option<i64>,
    pub invalidated_ts: Option<i64>,
    pub expired_ts: Option<i64>,
}
```

### 2.4 RefreshToken 结构体

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RefreshToken {
    pub id: i64,
    pub token: String,
    pub user_id: String,
    pub device_id: String,
    pub created_ts: i64,
    pub expires_ts: Option<i64>,
    pub invalidated_ts: Option<i64>,
    pub expired_ts: Option<i64>,
}
```

### 2.5 Room 结构体

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Room {
    pub room_id: String,
    pub is_public: bool,
    pub creator: String,
    pub creation_ts: i64,
    pub federate: bool,
    pub version: String,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar: Option<String>,
    pub canonical_alias: Option<String>,
    pub guest_access: bool,
    pub history_visibility: String,
    pub encryption: Option<String>,
    pub is_flaged: bool,
    pub is_spotlight: bool,
    pub deleted_ts: Option<i64>,
    pub join_rule: Option<String>,
    pub member_count: i32,
}
```

### 2.6 RoomEvent 结构体

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RoomEvent {
    pub event_id: String,
    pub room_id: String,
    pub user_id: String,
    pub event_type: String,
    pub content: serde_json::Value,
    pub state_key: Option<String>,
    pub depth: i64,
    pub origin_server_ts: i64,
    pub processed_ts: i64,
    pub not_before: Option<i64>,
    pub status: Option<String>,
    pub reference_image: Option<String>,
    pub origin: String,
}
```

### 2.7 RoomMember 结构体

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RoomMember {
    pub room_id: String,
    pub user_id: String,
    pub sender: String,
    pub membership: String,
    pub event_id: String,
    pub event_type: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub is_banned: Option<bool>,
    pub invite_token: Option<String>,
    pub inviter: Option<String>,
    pub updated_ts: Option<i64>,
    pub joined_ts: Option<i64>,
    pub left_ts: Option<i64>,
    pub reason: Option<String>,
    pub join_reason: Option<String>,
    pub banned_by: Option<String>,
}
```

### 2.8 Presence 结构体

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Presence {
    pub user_id: String,
    pub presence: String,
    pub status_msg: Option<String>,
    pub last_active_ts: i64,
    pub currently_active: bool,
}
```

### 2.9 Friend 结构体

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Friend {
    pub user_id: String,
    pub friend_id: String,
    pub category: Option<String>,
    pub added_at: i64,
}
```

### 2.10 FriendRequest 结构体

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FriendRequest {
    pub request_id: String,
    pub from_user_id: String,
    pub to_user_id: String,
    pub message: Option<String>,
    pub status: String,
    pub created_at: i64,
    pub responded_at: Option<i64>,
}
```

### 2.11 FriendCategory 结构体

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FriendCategory {
    pub user_id: String,
    pub category_name: String,
    pub display_name: Option<String>,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub created_at: i64,
}
```

### 2.12 BlockedUser 结构体

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct BlockedUser {
    pub user_id: String,
    pub blocked_user_id: String,
    pub reason: Option<String>,
    pub blocked_at: i64,
}
```

### 2.13 PrivateSession 结构体

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PrivateSession {
    pub id: i64,
    pub session_id: String,
    pub creator_id: String,
    pub participant_id: String,
    pub session_name: Option<String>,
    pub ttl_seconds: Option<i32>,
    pub auto_delete: Option<bool>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
```

### 2.14 PrivateMessage 结构体

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PrivateMessage {
    pub id: i64,
    pub message_id: String,
    pub session_id: String,
    pub sender_id: String,
    pub content: String,
    pub encrypted: bool,
    pub ttl_seconds: Option<i32>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub read_at: Option<chrono::DateTime<chrono::Utc>>,
}
```

### 2.15 VoiceMessage 结构体

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct VoiceMessage {
    pub id: i64,
    pub message_id: String,
    pub user_id: String,
    pub room_id: Option<String>,
    pub file_format: String,
    pub file_size: i64,
    pub duration: i32,
    pub file_url: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
```

---

## 三、关系映射

### 3.1 用户关系

```
users (1) ─────── (N) devices
users (1) ─────── (N) access_tokens
users (1) ─────── (N) refresh_tokens
users (1) ─────── (N) room_memberships
users (1) ─────── (1) presence
users (1) ─────── (N) friends (as user_id)
users (1) ─────── (N) friends (as friend_id)
users (1) ─────── (N) friend_requests (as from_user_id)
users (1) ─────── (N) friend_requests (as to_user_id)
users (1) ─────── (N) friend_categories
users (1) ─────── (N) blocked_users (as user_id)
users (1) ─────── (N) blocked_users (as blocked_user_id)
users (1) ─────── (N) private_sessions (as creator_id)
users (1) ─────── (N) private_sessions (as participant_id)
users (1) ─────── (N) private_messages
users (1) ─────── (N) voice_messages
```

### 3.2 房间关系

```
rooms (1) ─────── (N) events
rooms (1) ─────── (N) room_memberships
rooms (1) ─────── (N) voice_messages
```

### 3.3 事件关系

```
events (N) ─────── (1) rooms
events (N) ─────── (1) users
```

### 3.4 私聊关系

```
private_sessions (1) ─────── (N) private_messages
private_sessions (1) ─────── (N) session_keys
```

---

## 四、索引策略

### 4.1 主键索引

所有表都有主键索引，用于快速查找和唯一性约束。

### 4.2 外键索引

所有外键字段都有索引，用于加速关联查询。

### 4.3 唯一索引

- `users.username`：用户名唯一索引
- `access_tokens.token`：访问令牌唯一索引
- `refresh_tokens.token`：刷新令牌唯一索引
- `private_sessions.session_id`：会话 ID 唯一索引
- `private_messages.message_id`：消息 ID 唯一索引
- `voice_messages.message_id`：语音消息 ID 唯一索引
- `ip_blocks.ip_address`：IP 地址唯一索引
- `ip_reputation.ip_address`：IP 地址唯一索引

### 4.4 复合索引

- `room_memberships(room_id, user_id)`：房间成员复合索引
- `friends(user_id, friend_id)`：好友关系复合索引
- `friend_categories(user_id, category_name)`：好友分类复合索引
- `blocked_users(user_id, blocked_user_id)`：黑名单复合索引

### 4.5 查询优化索引

- `events.origin_server_ts`：事件时间戳索引，用于时间范围查询
- `events.event_type`：事件类型索引，用于类型过滤
- `events.room_id`：房间 ID 索引，用于房间事件查询
- `private_messages.created_at`：创建时间索引，用于时间排序
- `voice_messages.created_at`：创建时间索引，用于时间排序
- `security_events.created_at`：创建时间索引，用于时间排序
- `ip_reputation.score`：声誉分数索引，用于排序

---

## 五、参考资料

- [Synapse 官方文档](https://element-hq.github.io/synapse/latest/)
- [Matrix 规范](https://spec.matrix.org/)
- [PostgreSQL 文档](https://www.postgresql.org/docs/)
- [SQLx 文档](https://docs.rs/sqlx/latest/sqlx/)

---

## 六、变更日志

| 版本 | 日期 | 变更说明 |
|------|------|----------|
| 1.0.0 | 2026-01-28 | 初始版本，定义数据模型文档 |
