# synapse-rust 数据库设计需求文档

> **项目**: synapse-rust (Matrix Homeserver)
> **版本**: v6.0.4
> **文档日期**: 2026-03-26
> **基于API**: 656个端点, 48个模块

---

## 目录

1. [数据库概述](#1-数据库概述)
2. [实体关系图(ERD)](#2-实体关系图erd)
3. [表结构详细说明](#3-表结构详细说明)
4. [索引设计](#4-索引设计)
5. [数据字典](#5-数据字典)
6. [安全考虑](#6-安全考虑)
7. [维护建议](#7-维护建议)

---

## 1. 数据库概述

### 1.1 项目背景

synapse-rust是基于Matrix协议的Rust实现 homeserver，需要支持以下核心功能：

| 功能类别 | 端点数量 | 说明 |
|----------|----------|------|
| 用户认证 | 57+ | 登录、登出、令牌管理 |
| 房间管理 | 120+ | 创建房间、加入/离开、消息发送 |
| 设备管理 | 8+ | 设备注册、更新、删除 |
| 端到端加密 | 27+ | 密钥管理、会话加密 |
| 媒体服务 | 21+ | 媒体上传、下载、缩略图 |
| 联邦协议 | 47+ | 跨服务器通信 |
| 推送通知 | 18+ | 推送规则、设备推送 |
| Space功能 | 21+ | 空间层级结构 |

### 1.2 数据库统计

| 指标 | 数值 |
|------|------|
| 数据表总数 | **135+** |
| 索引总数 | **200+** |
| 外键约束 | **50+** |
| 核心模块 | **48** |
| API端点 | **656** |

### 1.3 技术选型

| 组件 | 选择 | 说明 |
|------|------|------|
| 数据库 | PostgreSQL | 主数据存储 |
| 缓存 | Redis | 会话缓存、令牌缓存 |
| 字段命名 | snake_case | 遵循项目规范 |
| 时间戳格式 | 毫秒级BIGINT | `created_ts`, `updated_ts` |
| 可空时间戳 | `_at`后缀 | `expires_at`, `revoked_at` |

### 1.4 字段命名规范

#### 时间字段规范

| 后缀 | 用途 | 可空性 | 示例 |
|------|------|--------|------|
| `_ts` | 必须存在的时间戳 | NOT NULL | `created_ts`, `updated_ts`, `added_ts` |
| `_at` | 可选操作的时间戳 | 可空 | `expires_at`, `revoked_at`, `validated_at`, `last_used_at` |

#### 布尔字段规范

| 前缀 | 用途 | 示例 |
|------|------|------|
| `is_` | 是否...状态 | `is_admin`, `is_revoked`, `is_enabled` |
| `has_` | 拥有...属性 | `has_published_keys`, `has_avatar` |

---

## 2. 实体关系图(ERD)

### 2.1 核心实体关系

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           用户认证模块                                        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   ┌──────────┐     ┌───────────────┐     ┌─────────────┐                   │
│   │  users   │────▶│  devices      │     │ access_tokens│                   │
│   └──────────┘     └───────────────┘     └─────────────┘                   │
│        │                  │                      │                         │
│        │                  │                      │                         │
│        ▼                  ▼                      ▼                         │
│   ┌──────────┐     ┌───────────────┐     ┌─────────────┐                   │
│   │user_threepids│ │device_keys   │     │refresh_tokens│                   │
│   └──────────┘     └───────────────┘     └─────────────┘                   │
│        │                  │                      │                         │
│        │                  ▼                      │                         │
│        │           ┌───────────────┐            │                         │
│        │           │cross_signing_keys│          │                         │
│        │           └───────────────┘            │                         │
│        │                                          │                         │
│        └──────────────────────────────────────────┘                         │
│                          │                                                  │
└──────────────────────────│──────────────────────────────────────────────────┘
                           │
┌──────────────────────────│──────────────────────────────────────────────────┐
│                          ▼           房间消息模块                           │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   ┌──────────┐     ┌───────────────┐     ┌─────────────┐                   │
│   │  rooms   │────▶│room_memberships│────▶│   events    │                   │
│   └──────────┘     └───────────────┘     └─────────────┘                   │
│        │                  │                      │                         │
│        ▼                  ▼                      ▼                         │
│   ┌──────────┐     ┌───────────────┐     ┌─────────────┐                   │
│   │room_summaries│ │room_parents   │     │event_receipts│                  │
│   └──────────┘     └───────────────┘     └─────────────┘                   │
│        │                  │                      │                         │
│        ▼                  ▼                      ▼                         │
│   ┌──────────┐     ┌───────────────┐     ┌─────────────┐                   │
│   │room_directory│ │ space_children │     │read_markers │                   │
│   └──────────┘     └───────────────┘     └─────────────┘                   │
│        │                                                               │
│        ▼                                                               │
│   ┌──────────┐     ┌───────────────┐                                   │
│   │room_aliases│    │thread_roots  │                                   │
│   └──────────┘     └───────────────┘                                   │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
                           │
┌──────────────────────────│──────────────────────────────────────────────────┐
│                          ▼           E2EE加密模块                          │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   ┌──────────────┐     ┌───────────────┐     ┌─────────────┐               │
│   │ device_keys  │────▶│ megolm_sessions│    │olm_accounts │               │
│   └──────────────┘     └───────────────┘     └─────────────┘               │
│        │                      │                     │                      │
│        ▼                      ▼                     ▼                      │
│   ┌──────────────┐     ┌───────────────┐     ┌─────────────┐               │
│   │event_signatures│    │backup_keys   │     │olm_sessions │               │
│   └──────────────┘     └───────────────┘     └─────────────┘               │
│        │                      │                                           │
│        ▼                      ▼                                           │
│   ┌──────────────┐     ┌───────────────┐                                 │
│   │device_signatures│   │ key_backups  │                                 │
│   └──────────────┘     └───────────────┘                                 │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
                           │
┌──────────────────────────│──────────────────────────────────────────────────┐
│                          ▼           媒体存储模块                          │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   ┌──────────────┐     ┌───────────────┐                                 │
│   │media_metadata│────▶│  thumbnails   │                                 │
│   └──────────────┘     └───────────────┘                                 │
│        │                                                             │
│        ▼                                                             │
│   ┌──────────────┐     ┌───────────────┐                                 │
│   │ media_quota  │     │user_media_quota│                                │
│   └──────────────┘     └───────────────┘                                 │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### 2.2 实体关系说明

| 关系类型 | 说明 | 示例 |
|----------|------|------|
| 1:N | 一个用户拥有多个设备 | users → devices |
| 1:N | 一个房间有多个成员 | rooms → room_memberships |
| 1:N | 一个用户有多个令牌 | users → access_tokens |
| N:M | 用户间好友关系 | users ↔ users (via friends) |
| 1:1 | 用户与账户数据 | users → account_data |
| 1:N | 媒体与缩略图 | media_metadata → thumbnails |

---

## 3. 表结构详细说明

### 3.1 用户与认证模块

#### 3.1.1 users - 用户表

**功能定位**: 存储Matrix用户的基本信息，是整个系统的核心表之一。

| 字段名 | 数据类型 | 长度/精度 | 主键 | 外键 | 可空 | 默认值 | 说明 |
|--------|----------|-----------|------|------|------|--------|------|
| user_id | TEXT | - | ✓ | - | ✗ | - | Matrix用户ID，格式: @user:server |
| username | TEXT | - | - | - | ✗ | - | 用户名，唯一 |
| password_hash | TEXT | - | - | - | ✓ | - | Argon2id哈希密码 |
| is_admin | BOOLEAN | - | - | - | ✗ | FALSE | 是否管理员 |
| is_guest | BOOLEAN | - | - | - | ✗ | FALSE | 是否访客用户 |
| is_shadow_banned | BOOLEAN | - | - | - | ✗ | FALSE | 是否被影子封禁 |
| is_deactivated | BOOLEAN | - | - | - | ✗ | FALSE | 是否已停用 |
| created_ts | BIGINT | - | - | - | ✗ | - | 创建时间戳(毫秒) |
| updated_ts | BIGINT | - | - | - | ✓ | - | 更新时间戳 |
| displayname | TEXT | - | - | - | ✓ | - | 显示名称 |
| avatar_url | TEXT | - | - | - | ✓ | - | 头像URL |
| email | TEXT | - | - | - | ✓ | - | 邮箱地址 |
| phone | TEXT | - | - | - | ✓ | - | 电话号码 |
| generation | BIGINT | - | - | - | ✗ | 0 | 用户代际 |
| consent_version | TEXT | - | - | - | ✓ | - | 同意版本 |
| appservice_id | TEXT | - | - | - | ✓ | - | 应用服务ID |
| user_type | TEXT | - | - | - | ✓ | - | 用户类型 |
| invalid_update_at | BIGINT | - | - | - | ✓ | - | 无效更新时间 |
| migration_state | TEXT | - | - | - | ✓ | - | 迁移状态 |
| password_changed_ts | BIGINT | - | - | - | ✓ | - | 密码修改时间 |
| is_password_change_required | BOOLEAN | - | - | - | ✗ | FALSE | 是否需要修改密码 |
| password_expires_at | BIGINT | - | - | - | ✓ | - | 密码过期时间 |
| failed_login_attempts | INTEGER | - | - | - | ✗ | 0 | 登录失败次数 |
| locked_until | BIGINT | - | - | - | ✓ | - | 锁定截止时间 |

**约束**:
- PRIMARY KEY (user_id)
- UNIQUE (username)

**索引**:
- `idx_users_email` ON (email)
- `idx_users_is_admin` ON (is_admin)
- `idx_users_must_change_password` ON (must_change_password) WHERE must_change_password = TRUE
- `idx_users_password_expires` ON (password_expires_at) WHERE password_expires_at IS NOT NULL
- `idx_users_locked` ON (locked_until) WHERE locked_until IS NOT NULL

#### 3.1.2 devices - 设备表

**功能定位**: 存储用户的登录设备信息，支持多设备管理。

| 字段名 | 数据类型 | 长度/精度 | 主键 | 外键 | 可空 | 默认值 | 说明 |
|--------|----------|-----------|------|------|------|--------|------|
| device_id | TEXT | - | ✓ | - | ✗ | - | 设备唯一ID |
| user_id | TEXT | - | - | ✓ | ✗ | - | 所属用户ID |
| display_name | TEXT | - | - | - | ✓ | - | 设备显示名称 |
| device_key | JSONB | - | - | - | ✓ | - | 设备加密公钥 |
| last_seen_ts | BIGINT | - | - | - | ✓ | - | 最后活跃时间 |
| last_seen_ip | TEXT | - | - | - | ✓ | - | 最后登录IP |
| created_ts | BIGINT | - | - | - | ✗ | - | 创建时间 |
| first_seen_ts | BIGINT | - | - | - | ✗ | - | 首次活跃时间 |
| user_agent | TEXT | - | - | - | ✓ | - | 用户代理字符串 |
| appservice_id | TEXT | - | - | - | ✓ | - | 应用服务ID |
| ignored_user_list | TEXT | - | - | - | ✓ | - | 忽略用户列表 |

**约束**:
- PRIMARY KEY (device_id)
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE

**索引**:
- `idx_devices_user_id` ON (user_id)
- `idx_devices_last_seen` ON (last_seen_ts DESC)

#### 3.1.3 access_tokens - 访问令牌表

**功能定位**: 存储用户的访问令牌，用于API认证。

| 字段名 | 数据类型 | 长度/精度 | 主键 | 外键 | 可空 | 默认值 | 说明 |
|--------|----------|-----------|------|------|------|--------|------|
| id | BIGSERIAL | - | ✓ | - | - | - | 自增主键 |
| token | TEXT | - | - | - | ✗ | - | 访问令牌(唯一) |
| user_id | TEXT | - | - | ✓ | ✗ | - | 用户ID |
| device_id | TEXT | - | - | - | ✓ | - | 设备ID |
| created_ts | BIGINT | - | - | - | ✗ | - | 创建时间 |
| expires_at | BIGINT | - | - | - | ✓ | - | 过期时间 |
| last_used_ts | BIGINT | - | - | - | ✓ | - | 最后使用时间 |
| user_agent | TEXT | - | - | - | ✓ | - | 用户代理 |
| ip_address | TEXT | - | - | - | ✓ | - | IP地址 |
| is_revoked | BOOLEAN | - | - | - | ✗ | FALSE | 是否已撤销 |

**约束**:
- PRIMARY KEY (id)
- UNIQUE (token)
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE

**索引**:
- `idx_access_tokens_user_id` ON (user_id)
- `idx_access_tokens_valid` ON (is_revoked) WHERE is_revoked = FALSE

#### 3.1.4 refresh_tokens - 刷新令牌表

**功能定位**: 存储用于刷新访问令牌的刷新令牌。

| 字段名 | 数据类型 | 长度/精度 | 主键 | 外键 | 可空 | 默认值 | 说明 |
|--------|----------|-----------|------|------|------|--------|------|
| id | BIGSERIAL | - | ✓ | - | - | - | 自增主键 |
| token_hash | TEXT | - | - | - | ✗ | - | 令牌哈希(唯一) |
| user_id | TEXT | - | - | ✓ | ✗ | - | 用户ID |
| device_id | TEXT | - | - | - | ✓ | - | 设备ID |
| access_token_id | TEXT | - | - | - | ✓ | - | 关联访问令牌ID |
| scope | TEXT | - | - | - | ✓ | - | 权限范围 |
| created_ts | BIGINT | - | - | - | ✗ | - | 创建时间 |
| expires_at | BIGINT | - | - | - | ✓ | - | 过期时间 |
| last_used_ts | BIGINT | - | - | - | ✓ | - | 最后使用时间 |
| use_count | INTEGER | - | - | - | ✗ | 0 | 使用次数 |
| is_revoked | BOOLEAN | - | - | - | ✗ | FALSE | 是否已撤销 |
| revoked_reason | TEXT | - | - | - | ✓ | - | 撤销原因 |
| client_info | JSONB | - | - | - | ✓ | - | 客户端信息 |
| ip_address | TEXT | - | - | - | ✓ | - | IP地址 |
| user_agent | TEXT | - | - | - | ✓ | - | 用户代理 |

**约束**:
- PRIMARY KEY (id)
- UNIQUE (token_hash)
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE

**索引**:
- `idx_refresh_tokens_user_id` ON (user_id)
- `idx_refresh_tokens_revoked` ON (is_revoked) WHERE is_revoked = FALSE

#### 3.1.5 user_threepids - 用户第三方身份表

**功能定位**: 存储用户的邮箱、手机等第三方身份验证信息。

| 字段名 | 数据类型 | 长度/精度 | 主键 | 外键 | 可空 | 默认值 | 说明 |
|--------|----------|-----------|------|------|------|--------|------|
| id | BIGSERIAL | - | ✓ | - | - | - | 自增主键 |
| user_id | TEXT | - | - | ✓ | ✗ | - | 用户ID |
| medium | TEXT | - | - | - | ✗ | - | 验证类型(email/phone) |
| address | TEXT | - | - | - | ✗ | - | 验证地址 |
| validated_at | BIGINT | - | - | - | ✓ | - | 验证时间 |
| added_ts | BIGINT | - | - | - | ✗ | - | 添加时间 |
| is_verified | BOOLEAN | - | - | - | ✗ | FALSE | 是否已验证 |
| verification_token | TEXT | - | - | - | ✓ | - | 验证令牌 |
| verification_expires_at | BIGINT | - | - | - | ✓ | - | 验证过期时间 |

**约束**:
- PRIMARY KEY (id)
- UNIQUE (medium, address)
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE

**索引**:
- `idx_user_threepids_user` ON (user_id)

---

### 3.2 房间与消息模块

#### 3.2.1 rooms - 房间表

**功能定位**: 存储房间的基本元数据信息。

| 字段名 | 数据类型 | 长度/精度 | 主键 | 外键 | 可空 | 默认值 | 说明 |
|--------|----------|-----------|------|------|------|--------|------|
| room_id | TEXT | - | ✓ | - | ✗ | - | 房间唯一ID |
| creator | TEXT | - | - | - | ✓ | - | 创建者用户ID |
| is_public | BOOLEAN | - | - | - | ✗ | FALSE | 是否公开房间 |
| room_version | TEXT | - | - | - | ✗ | '6' | 房间协议版本 |
| created_ts | BIGINT | - | - | - | ✗ | - | 创建时间 |
| last_activity_ts | BIGINT | - | - | - | ✓ | - | 最后活动时间 |
| is_federated | BOOLEAN | - | - | - | ✗ | TRUE | 是否支持联邦 |
| has_guest_access | BOOLEAN | - | - | - | ✗ | FALSE | 访客是否有访问权 |
| join_rules | TEXT | - | - | - | ✗ | 'invite' | 加入规则 |
| history_visibility | TEXT | - | - | - | ✗ | 'shared' | 历史可见性 |
| name | TEXT | - | - | - | ✓ | - | 房间名称 |
| topic | TEXT | - | - | - | ✓ | - | 房间主题 |
| avatar_url | TEXT | - | - | - | ✓ | - | 房间头像 |
| canonical_alias | TEXT | - | - | - | ✓ | - | 规范别名 |
| visibility | TEXT | - | - | - | ✗ | 'private' | 可见性 |

**约束**:
- PRIMARY KEY (room_id)

**索引**:
- `idx_rooms_creator` ON (creator) WHERE creator IS NOT NULL
- `idx_rooms_is_public` ON (is_public) WHERE is_public = TRUE
- `idx_rooms_last_activity` ON (last_activity_ts DESC) WHERE last_activity_ts IS NOT NULL

#### 3.2.2 room_memberships - 房间成员表

**功能定位**: 存储房间与用户的成员关系及状态。

| 字段名 | 数据类型 | 长度/精度 | 主键 | 外键 | 可空 | 默认值 | 说明 |
|--------|----------|-----------|------|------|------|--------|------|
| id | BIGSERIAL | - | ✓ | - | - | - | 自增主键 |
| room_id | TEXT | - | - | ✓ | ✗ | - | 房间ID |
| user_id | TEXT | - | - | ✓ | ✗ | - | 用户ID |
| membership | TEXT | - | - | - | ✗ | - | 成员状态(join/leave/ban/invite/knock) |
| joined_ts | BIGINT | - | - | - | ✓ | - | 加入时间 |
| invited_ts | BIGINT | - | - | - | ✓ | - | 被邀请时间 |
| left_ts | BIGINT | - | - | - | ✓ | - | 离开时间 |
| banned_ts | BIGINT | - | - | - | ✓ | - | 被禁时间 |
| sender | TEXT | - | - | - | ✓ | - | 操作发送者 |
| reason | TEXT | - | - | - | ✓ | - | 操作原因 |
| event_id | TEXT | - | - | - | ✓ | - | 相关事件ID |
| event_type | TEXT | - | - | - | ✓ | - | 相关事件类型 |
| display_name | TEXT | - | - | - | ✓ | - | 显示名称 |
| avatar_url | TEXT | - | - | - | ✓ | - | 头像URL |
| is_banned | BOOLEAN | - | - | - | ✗ | FALSE | 是否被禁 |
| invite_token | TEXT | - | - | - | ✓ | - | 邀请令牌 |
| updated_ts | BIGINT | - | - | - | ✓ | - | 更新时间 |
| join_reason | TEXT | - | - | - | ✓ | - | 加入原因 |
| banned_by | TEXT | - | - | - | ✓ | - | 禁言者 |
| ban_reason | TEXT | - | - | - | ✓ | - | 禁言原因 |

**约束**:
- PRIMARY KEY (id)
- UNIQUE (room_id, user_id)
- FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE

**索引**:
- `idx_room_memberships_room` ON (room_id)
- `idx_room_memberships_user` ON (user_id)
- `idx_room_memberships_membership` ON (membership)
- `idx_room_memberships_user_membership` ON (user_id, membership)
- `idx_room_memberships_room_membership` ON (room_id, membership)
- `idx_room_memberships_joined` ON (user_id, room_id) WHERE membership = 'join'

#### 3.2.3 events - 事件表

**功能定位**: 存储房间内的所有事件(消息、状态变更等)。

| 字段名 | 数据类型 | 长度/精度 | 主键 | 外键 | 可空 | 默认值 | 说明 |
|--------|----------|-----------|------|------|------|--------|------|
| event_id | TEXT | - | ✓ | - | ✗ | - | 事件唯一ID |
| room_id | TEXT | - | - | ✓ | ✗ | - | 所属房间ID |
| sender | TEXT | - | - | - | ✗ | - | 发送者用户ID |
| event_type | TEXT | - | - | - | ✗ | - | 事件类型 |
| content | JSONB | - | - | - | ✗ | - | 事件内容 |
| origin_server_ts | BIGINT | - | - | - | ✗ | - | 源服务器时间戳 |
| state_key | TEXT | - | - | - | ✓ | - | 状态键(状态事件) |
| is_redacted | BOOLEAN | - | - | - | ✗ | FALSE | 是否已删除 |
| redacted_at | BIGINT | - | - | - | ✓ | - | 删除时间 |
| redacted_by | TEXT | - | - | - | ✓ | - | 删除操作者 |
| transaction_id | TEXT | - | - | - | ✓ | - | 事务ID |
| depth | BIGINT | - | - | - | ✓ | - | 事件深度 |
| prev_events | JSONB | - | - | - | ✓ | - | 前置事件 |
| auth_events | JSONB | - | - | - | ✓ | - | 认证事件 |
| signatures | JSONB | - | - | - | ✓ | - | 事件签名 |
| hashes | JSONB | - | - | - | ✓ | - | 事件哈希 |
| unsigned | JSONB | - | - | - | ✗ | '{}' | 未签名数据 |
| processed_at | BIGINT | - | - | - | ✓ | - | 处理时间 |
| not_before | BIGINT | - | - | - | ✗ | 0 | 生效时间 |
| status | TEXT | - | - | - | ✓ | - | 事件状态 |
| reference_image | TEXT | - | - | - | ✓ | - | 引用图片 |
| origin | TEXT | - | - | - | ✓ | - | 源服务器名 |
| user_id | TEXT | - | - | - | ✓ | - | 用户ID |

**约束**:
- PRIMARY KEY (event_id)
- FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE

**索引**:
- `idx_events_room_id` ON (room_id)
- `idx_events_sender` ON (sender)
- `idx_events_type` ON (event_type)
- `idx_events_origin_server_ts` ON (origin_server_ts DESC)
- `idx_events_not_redacted` ON (room_id, origin_server_ts DESC) WHERE is_redacted = FALSE

#### 3.2.4 room_summaries - 房间摘要表

**功能定位**: 存储房间的摘要信息，用于快速查询。

| 字段名 | 数据类型 | 长度/精度 | 主键 | 外键 | 可空 | 默认值 | 说明 |
|--------|----------|-----------|------|------|------|--------|------|
| room_id | TEXT | - | ✓ | - | ✗ | - | 房间ID |
| name | TEXT | - | - | - | ✓ | - | 房间名称 |
| topic | TEXT | - | - | - | ✓ | - | 房间主题 |
| canonical_alias | TEXT | - | - | - | ✓ | - | 规范别名 |
| member_count | BIGINT | - | - | - | ✗ | 0 | 成员数量 |
| joined_members | BIGINT | - | - | - | ✗ | 0 | 已加入成员数 |
| invited_members | BIGINT | - | - | - | ✗ | 0 | 已邀请成员数 |
| hero_users | JSONB | - | - | - | ✓ | - | 核心用户列表 |
| is_world_readable | BOOLEAN | - | - | - | ✗ | FALSE | 是否世界可读 |
| can_guest_join | BOOLEAN | - | - | - | ✗ | FALSE | 访客是否能加入 |
| is_federated | BOOLEAN | - | - | - | ✗ | TRUE | 是否支持联邦 |
| encryption_state | TEXT | - | - | - | ✓ | - | 加密状态 |
| updated_ts | BIGINT | - | - | - | ✓ | - | 更新时间 |

**约束**:
- PRIMARY KEY (room_id)

#### 3.2.5 thread_roots - 线程根消息表

**功能定位**: 存储线程的根消息及统计信息。

| 字段名 | 数据类型 | 长度/精度 | 主键 | 外键 | 可空 | 默认值 | 说明 |
|--------|----------|-----------|------|------|------|--------|------|
| id | BIGSERIAL | - | ✓ | - | - | - | 自增主键 |
| room_id | TEXT | - | - | ✓ | ✗ | - | 房间ID |
| event_id | TEXT | - | - | - | ✗ | - | 根事件ID |
| sender | TEXT | - | - | - | ✗ | - | 发送者 |
| thread_id | TEXT | - | - | - | ✓ | - | 线程ID |
| reply_count | BIGINT | - | - | - | ✗ | 0 | 回复数量 |
| last_reply_event_id | TEXT | - | - | - | ✓ | - | 最后回复事件ID |
| last_reply_sender | TEXT | - | - | - | ✓ | - | 最后回复发送者 |
| last_reply_ts | BIGINT | - | - | - | ✓ | - | 最后回复时间 |
| participants | JSONB | - | - | - | ✗ | '[]' | 参与者列表 |
| is_fetched | BOOLEAN | - | - | - | ✗ | FALSE | 是否已获取 |
| created_ts | BIGINT | - | - | - | ✗ | - | 创建时间 |
| updated_ts | BIGINT | - | - | - | ✓ | - | 更新时间 |

**约束**:
- PRIMARY KEY (id)
- UNIQUE (room_id, event_id)
- FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE

**索引**:
- `idx_thread_roots_room` ON (room_id)
- `idx_thread_roots_event` ON (event_id)
- `idx_thread_roots_thread` ON (thread_id)
- `idx_thread_roots_last_reply` ON (last_reply_ts DESC) WHERE last_reply_ts IS NOT NULL

---

### 3.3 E2EE加密模块

#### 3.3.1 device_keys - 设备密钥表

**功能定位**: 存储设备的加密公钥信息。

| 字段名 | 数据类型 | 长度/精度 | 主键 | 外键 | 可空 | 默认值 | 说明 |
|--------|----------|-----------|------|------|------|--------|------|
| id | BIGSERIAL | - | ✓ | - | - | - | 自增主键 |
| user_id | TEXT | - | - | - | ✗ | - | 用户ID |
| device_id | TEXT | - | - | - | ✗ | - | 设备ID |
| algorithm | TEXT | - | - | - | ✗ | - | 加密算法 |
| key_id | TEXT | - | - | - | ✗ | - | 密钥ID |
| public_key | TEXT | - | - | - | ✗ | - | 公钥数据 |
| key_data | TEXT | - | - | - | ✓ | - | 完整密钥数据 |
| signatures | JSONB | - | - | - | ✓ | - | 密钥签名 |
| added_ts | BIGINT | - | - | - | ✗ | - | 添加时间 |
| created_ts | BIGINT | - | - | - | ✗ | - | 创建时间 |
| updated_ts | BIGINT | - | - | - | ✓ | - | 更新时间 |
| ts_updated_ms | BIGINT | - | - | - | ✓ | - | 毫秒级更新时间 |
| is_verified | BOOLEAN | - | - | - | ✗ | FALSE | 是否已验证 |
| is_blocked | BOOLEAN | - | - | - | ✗ | FALSE | 是否已阻止 |
| display_name | TEXT | - | - | - | ✓ | - | 显示名称 |

**约束**:
- PRIMARY KEY (id)
- UNIQUE (user_id, device_id, key_id)

**索引**:
- `idx_device_keys_user_device` ON (user_id, device_id)

#### 3.3.2 megolm_sessions - Megolm会话表

**功能定位**: 存储Megolm加密会话信息。

| 字段名 | 数据类型 | 长度/精度 | 主键 | 外键 | 可空 | 默认值 | 说明 |
|--------|----------|-----------|------|------|------|--------|------|
| id | UUID | - | ✓ | - | - | gen_random_uuid() | 主键 |
| session_id | TEXT | - | - | - | ✗ | - | 会话ID(唯一) |
| room_id | TEXT | - | - | - | ✗ | - | 房间ID |
| sender_key | TEXT | - | - | - | ✗ | - | 发送者密钥 |
| session_key | TEXT | - | - | - | ✗ | - | 会话密钥 |
| algorithm | TEXT | - | - | - | ✗ | - | 加密算法 |
| message_index | BIGINT | - | - | - | ✗ | 0 | 消息索引 |
| created_ts | BIGINT | - | - | - | ✗ | - | 创建时间 |
| last_used_ts | BIGINT | - | - | - | ✓ | - | 最后使用时间 |
| expires_at | BIGINT | - | - | - | ✓ | - | 过期时间 |

**约束**:
- PRIMARY KEY (id)
- UNIQUE (session_id)

**索引**:
- `idx_megolm_sessions_room` ON (room_id)
- `idx_megolm_sessions_session` ON (session_id)

#### 3.3.3 key_backups - 密钥备份表

**功能定位**: 存储用户的密钥备份元数据。

| 字段名 | 数据类型 | 长度/精度 | 主键 | 外键 | 可空 | 默认值 | 说明 |
|--------|----------|-----------|------|------|------|--------|------|
| backup_id | BIGSERIAL | - | ✓ | - | - | - | 备份ID |
| user_id | TEXT | - | - | - | ✗ | - | 用户ID |
| algorithm | TEXT | - | - | - | ✗ | - | 备份算法 |
| auth_data | JSONB | - | - | - | ✓ | - | 认证数据 |
| auth_key | TEXT | - | - | - | ✓ | - | 认证密钥 |
| mgmt_key | TEXT | - | - | - | ✓ | - | 管理密钥 |
| version | BIGINT | - | - | - | ✗ | 1 | 备份版本 |
| created_ts | BIGINT | - | - | - | ✗ | - | 创建时间 |
| updated_ts | BIGINT | - | - | - | ✓ | - | 更新时间 |

**约束**:
- PRIMARY KEY (backup_id)
- UNIQUE (user_id, version)

**索引**:
- `idx_key_backups_user` ON (user_id)

#### 3.3.4 backup_keys - 密钥备份数据表

**功能定位**: 存储密钥备份的具体数据。

| 字段名 | 数据类型 | 长度/精度 | 主键 | 外键 | 可空 | 默认值 | 说明 |
|--------|----------|-----------|------|------|------|--------|------|
| id | BIGSERIAL | - | ✓ | - | - | - | 自增主键 |
| backup_id | BIGINT | - | - | ✓ | ✗ | - | 备份ID |
| room_id | TEXT | - | - | - | ✗ | - | 房间ID |
| session_id | TEXT | - | - | - | ✗ | - | 会话ID |
| session_data | JSONB | - | - | - | ✗ | - | 会话数据 |
| created_ts | BIGINT | - | - | - | ✗ | - | 创建时间 |

**约束**:
- PRIMARY KEY (id)
- FOREIGN KEY (backup_id) REFERENCES key_backups(backup_id) ON DELETE CASCADE

**索引**:
- `idx_backup_keys_backup` ON (backup_id)
- `idx_backup_keys_room` ON (room_id)

#### 3.3.5 olm_accounts - Olm账户表

**功能定位**: 存储Olm加密账户信息。

| 字段名 | 数据类型 | 长度/精度 | 主键 | 外键 | 可空 | 默认值 | 说明 |
|--------|----------|-----------|------|------|------|--------|------|
| id | BIGSERIAL | - | ✓ | - | - | - | 自增主键 |
| user_id | TEXT | - | - | - | ✗ | - | 用户ID |
| device_id | TEXT | - | - | - | ✗ | - | 设备ID |
| identity_key | TEXT | - | - | - | ✗ | - | 身份密钥 |
| serialized_account | TEXT | - | - | - | ✗ | - | 序列化账户 |
| is_one_time_keys_published | BOOLEAN | - | - | - | ✗ | FALSE | 是否已发布一次性密钥 |
| is_fallback_key_published | BOOLEAN | - | - | - | ✗ | FALSE | 是否已发布回退密钥 |
| created_ts | BIGINT | - | - | - | ✗ | - | 创建时间 |
| updated_ts | BIGINT | - | - | - | ✗ | - | 更新时间 |

**约束**:
- PRIMARY KEY (id)
- UNIQUE (user_id, device_id)

**索引**:
- `idx_olm_accounts_user` ON (user_id)
- `idx_olm_accounts_device` ON (device_id)

#### 3.3.6 olm_sessions - Olm会话表

**功能定位**: 存储Olm加密会话信息。

| 字段名 | 数据类型 | 长度/精度 | 主键 | 外键 | 可空 | 默认值 | 说明 |
|--------|----------|-----------|------|------|------|--------|------|
| id | BIGSERIAL | - | ✓ | - | - | - | 自增主键 |
| user_id | TEXT | - | - | - | ✗ | - | 用户ID |
| device_id | TEXT | - | - | - | ✗ | - | 设备ID |
| session_id | TEXT | - | - | - | ✗ | - | 会话ID(唯一) |
| sender_key | TEXT | - | - | - | ✗ | - | 发送者密钥 |
| receiver_key | TEXT | - | - | - | ✗ | - | 接收者密钥 |
| serialized_state | TEXT | - | - | - | ✗ | - | 序列化状态 |
| message_index | INTEGER | - | - | - | ✗ | 0 | 消息索引 |
| created_ts | BIGINT | - | - | - | ✗ | - | 创建时间 |
| last_used_ts | BIGINT | - | - | - | ✗ | - | 最后使用时间 |
| expires_at | BIGINT | - | - | - | ✓ | - | 过期时间 |

**约束**:
- UNIQUE (session_id)

**索引**:
- `idx_olm_sessions_user_device` ON (user_id, device_id)
- `idx_olm_sessions_sender_key` ON (sender_key)
- `idx_olm_sessions_expires` ON (expires_at) WHERE expires_at IS NOT NULL

---

### 3.4 媒体存储模块

#### 3.4.1 media_metadata - 媒体元数据表

**功能定位**: 存储上传媒体的元数据信息。

| 字段名 | 数据类型 | 长度/精度 | 主键 | 外键 | 可空 | 默认值 | 说明 |
|--------|----------|-----------|------|------|------|--------|------|
| media_id | TEXT | - | ✓ | - | ✗ | - | 媒体唯一ID |
| server_name | TEXT | - | - | - | ✗ | - | 服务器名称 |
| content_type | TEXT | - | - | - | ✗ | - | MIME类型 |
| file_name | TEXT | - | - | - | ✓ | - | 文件名 |
| size | BIGINT | - | - | - | ✗ | - | 文件大小(字节) |
| uploader_user_id | TEXT | - | - | - | ✓ | - | 上传者用户ID |
| created_ts | BIGINT | - | - | - | ✗ | - | 创建时间 |
| last_accessed_at | BIGINT | - | - | - | ✓ | - | 最后访问时间 |
| quarantine_status | TEXT | - | - | - | ✓ | - | 隔离状态 |

**约束**:
- PRIMARY KEY (media_id)

**索引**:
- `idx_media_uploader` ON (uploader_user_id)
- `idx_media_server` ON (server_name)

#### 3.4.2 thumbnails - 缩略图表

**功能定位**: 存储媒体缩略图信息。

| 字段名 | 数据类型 | 长度/精度 | 主键 | 外键 | 可空 | 默认值 | 说明 |
|--------|----------|-----------|------|------|------|--------|------|
| id | BIGSERIAL | - | ✓ | - | - | - | 自增主键 |
| media_id | TEXT | - | - | ✓ | ✗ | - | 关联媒体ID |
| width | INTEGER | - | - | - | ✗ | - | 宽度(像素) |
| height | INTEGER | - | - | - | ✗ | - | 高度(像素) |
| method | TEXT | - | - | - | ✗ | - | 生成方法(crop/scale) |
| content_type | TEXT | - | - | - | ✗ | - | MIME类型 |
| size | BIGINT | - | - | - | ✗ | - | 文件大小(字节) |
| created_ts | BIGINT | - | - | - | ✗ | - | 创建时间 |

**约束**:
- PRIMARY KEY (id)
- FOREIGN KEY (media_id) REFERENCES media_metadata(media_id) ON DELETE CASCADE

**索引**:
- `idx_thumbnails_media` ON (media_id)

#### 3.4.3 media_quota - 媒体配额表

**功能定位**: 存储用户的媒体存储配额信息。

| 字段名 | 数据类型 | 长度/精度 | 主键 | 外键 | 可空 | 默认值 | 说明 |
|--------|----------|-----------|------|------|------|--------|------|
| id | BIGSERIAL | - | ✓ | - | - | - | 自增主键 |
| user_id | TEXT | - | - | ✓ | ✗ | - | 用户ID |
| max_bytes | BIGINT | - | - | - | ✗ | 1073741824 | 最大配额(1GB) |
| used_bytes | BIGINT | - | - | - | ✗ | 0 | 已使用大小 |
| created_ts | BIGINT | - | - | - | ✗ | - | 创建时间 |
| updated_ts | BIGINT | - | - | - | ✓ | - | 更新时间 |

**约束**:
- PRIMARY KEY (id)
- UNIQUE (user_id)
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE

---

### 3.5 推送通知模块

#### 3.5.1 pushers - 推送器表

**功能定位**: 存储推送器配置信息。

| 字段名 | 数据类型 | 长度/精度 | 主键 | 外键 | 可空 | 默认值 | 说明 |
|--------|----------|-----------|------|------|------|--------|------|
| id | BIGSERIAL | - | ✓ | - | - | - | 自增主键 |
| user_id | TEXT | - | - | - | ✗ | - | 用户ID |
| device_id | TEXT | - | - | - | ✗ | - | 设备ID |
| pushkey | TEXT | - | - | - | ✗ | - | 推送密钥 |
| pushkey_ts | BIGINT | - | - | - | ✗ | - | 推送密钥时间戳 |
| kind | TEXT | - | - | - | ✗ | - | 推送类型(http/email) |
| app_id | TEXT | - | - | - | ✗ | - | 应用ID |
| app_display_name | TEXT | - | - | - | ✗ | - | 应用显示名 |
| device_display_name | TEXT | - | - | - | ✗ | - | 设备显示名 |
| profile_tag | TEXT | - | - | - | ✓ | - | 配置标签 |
| lang | TEXT | - | - | - | ✗ | 'en' | 语言代码 |
| data | JSONB | - | - | - | ✗ | '{}' | 推送数据 |
| updated_ts | BIGINT | - | - | - | ✓ | - | 更新时间 |
| created_ts | BIGINT | - | - | - | ✗ | - | 创建时间 |
| is_enabled | BOOLEAN | - | - | - | ✗ | TRUE | 是否启用 |

**约束**:
- PRIMARY KEY (id)
- UNIQUE (user_id, device_id, pushkey)

**索引**:
- `idx_pushers_user` ON (user_id)
- `idx_pushers_enabled` ON (is_enabled) WHERE is_enabled = TRUE

#### 3.5.2 push_rules - 推送规则表

**功能定位**: 存储用户的推送规则配置。

| 字段名 | 数据类型 | 长度/精度 | 主键 | 外键 | 可空 | 默认值 | 说明 |
|--------|----------|-----------|------|------|------|--------|------|
| id | BIGSERIAL | - | ✓ | - | - | - | 自增主键 |
| user_id | TEXT | - | - | - | ✗ | - | 用户ID |
| scope | TEXT | - | - | - | ✗ | - | 规则范围(global/device) |
| rule_id | TEXT | - | - | - | ✗ | - | 规则ID |
| kind | TEXT | - | - | - | ✗ | - | 规则类型(override/underride/content/room/sender) |
| priority_class | INTEGER | - | - | - | ✗ | - | 优先级类(1-5) |
| priority | INTEGER | - | - | - | ✗ | 0 | 优先级 |
| conditions | JSONB | - | - | - | ✗ | '[]' | 触发条件 |
| actions | JSONB | - | - | - | ✗ | '[]' | 执行动作 |
| pattern | TEXT | - | - | - | ✓ | - | 匹配模式 |
| is_default | BOOLEAN | - | - | - | ✗ | FALSE | 是否默认规则 |
| is_enabled | BOOLEAN | - | - | - | ✗ | TRUE | 是否启用 |
| created_ts | BIGINT | - | - | - | ✗ | - | 创建时间 |
| updated_ts | BIGINT | - | - | - | ✓ | - | 更新时间 |

**约束**:
- PRIMARY KEY (id)
- UNIQUE (user_id, scope, rule_id)

**索引**:
- `idx_push_rules_user` ON (user_id)

---

### 3.6 好友关系模块

#### 3.6.1 friends - 好友表

**功能定位**: 存储用户的好友关系。

| 字段名 | 数据类型 | 长度/精度 | 主键 | 外键 | 可空 | 默认值 | 说明 |
|--------|----------|-----------|------|------|------|--------|------|
| id | BIGSERIAL | - | ✓ | - | - | - | 自增主键 |
| user_id | TEXT | - | - | ✓ | ✗ | - | 用户ID |
| friend_id | TEXT | - | - | ✓ | ✗ | - | 好友用户ID |
| created_ts | BIGINT | - | - | - | ✗ | - | 建立时间 |

**约束**:
- PRIMARY KEY (id)
- UNIQUE (user_id, friend_id)
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
- FOREIGN KEY (friend_id) REFERENCES users(user_id) ON DELETE CASCADE

**索引**:
- `idx_friends_user_id` ON (user_id)

#### 3.6.2 friend_requests - 好友请求表

**功能定位**: 存储好友请求信息。

| 字段名 | 数据类型 | 长度/精度 | 主键 | 外键 | 可空 | 默认值 | 说明 |
|--------|----------|-----------|------|------|------|--------|------|
| id | BIGSERIAL | - | ✓ | - | - | - | 自增主键 |
| sender_id | TEXT | - | - | ✓ | ✗ | - | 发送者ID |
| receiver_id | TEXT | - | - | ✓ | ✗ | - | 接收者ID |
| message | TEXT | - | - | - | ✓ | - | 请求消息 |
| status | TEXT | - | - | - | ✗ | 'pending' | 请求状态 |
| created_ts | BIGINT | - | - | - | ✗ | - | 创建时间 |
| updated_ts | BIGINT | - | - | - | ✓ | - | 更新时间 |

**约束**:
- PRIMARY KEY (id)
- UNIQUE (sender_id, receiver_id)
- FOREIGN KEY (sender_id) REFERENCES users(user_id) ON DELETE CASCADE
- FOREIGN KEY (receiver_id) REFERENCES users(user_id) ON DELETE CASCADE

**索引**:
- `idx_friend_requests_sender` ON (sender_id)
- `idx_friend_requests_receiver` ON (receiver_id)

#### 3.6.3 friend_categories - 好友分组表

**功能定位**: 存储好友分组信息。

| 字段名 | 数据类型 | 长度/精度 | 主键 | 外键 | 可空 | 默认值 | 说明 |
|--------|----------|-----------|------|------|------|--------|------|
| id | BIGSERIAL | - | ✓ | - | - | - | 自增主键 |
| user_id | TEXT | - | - | ✓ | ✗ | - | 用户ID |
| name | TEXT | - | - | - | ✗ | - | 分组名称 |
| color | TEXT | - | - | - | ✗ | '#000000' | 分组颜色 |
| created_ts | BIGINT | - | - | - | ✗ | - | 创建时间 |

**约束**:
- PRIMARY KEY (id)
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE

---

### 3.7 Space与层级模块

#### 3.7.1 spaces - Space表

**功能定位**: 存储Space(空间)的基本信息。

| 字段名 | 数据类型 | 长度/精度 | 主键 | 外键 | 可空 | 默认值 | 说明 |
|--------|----------|-----------|------|------|------|--------|------|
| space_id | TEXT | - | ✓ | - | ✗ | - | Space ID(主键) |
| name | TEXT | - | - | - | ✓ | - | Space名称 |
| creator | TEXT | - | - | - | ✗ | - | 创建者 |
| created_ts | BIGINT | - | - | - | ✗ | - | 创建时间 |
| is_public | BOOLEAN | - | - | - | ✗ | FALSE | 是否公开 |
| is_private | BOOLEAN | - | - | - | ✗ | TRUE | 是否私有 |
| member_count | BIGINT | - | - | - | ✗ | 0 | 成员数量 |
| topic | TEXT | - | - | - | ✓ | - | 主题 |
| avatar_url | TEXT | - | - | - | ✓ | - | 头像URL |
| canonical_alias | TEXT | - | - | - | ✓ | - | 规范别名 |
| history_visibility | TEXT | - | - | - | ✗ | 'shared' | 历史可见性 |
| join_rules | TEXT | - | - | - | ✗ | 'invite' | 加入规则 |
| room_type | TEXT | - | - | - | ✗ | 'm.space' | 房间类型 |
| updated_ts | BIGINT | - | - | - | ✓ | - | 更新时间 |

**约束**:
- PRIMARY KEY (space_id)

**索引**:
- `idx_spaces_creator` ON (creator)
- `idx_spaces_public` ON (is_public) WHERE is_public = TRUE

#### 3.7.2 space_children - Space子房间表

**功能定位**: 存储Space与子房间的关联关系。

| 字段名 | 数据类型 | 长度/精度 | 主键 | 外键 | 可空 | 默认值 | 说明 |
|--------|----------|-----------|------|------|------|--------|------|
| id | BIGSERIAL | - | ✓ | - | - | - | 自增主键 |
| space_id | TEXT | - | - | - | ✗ | - | Space ID |
| room_id | TEXT | - | - | - | ✗ | - | 房间ID |
| sender | TEXT | - | - | - | ✗ | - | 添加者 |
| is_suggested | BOOLEAN | - | - | - | ✗ | FALSE | 是否建议 |
| via_servers | JSONB | - | - | - | ✗ | '[]' | 中转服务器列表 |
| added_ts | BIGINT | - | - | - | ✗ | - | 添加时间 |

**约束**:
- PRIMARY KEY (id)
- UNIQUE (space_id, room_id)

**索引**:
- `idx_space_children_space` ON (space_id)
- `idx_space_children_room` ON (room_id)

#### 3.7.3 room_parents - 房间父子关系表

**功能定位**: 存储房间与Space的父子关系。

| 字段名 | 数据类型 | 长度/精度 | 主键 | 外键 | 可空 | 默认值 | 说明 |
|--------|----------|-----------|------|------|------|--------|------|
| id | BIGSERIAL | - | ✓ | - | - | - | 自增主键 |
| room_id | TEXT | - | - | ✓ | ✗ | - | 房间ID |
| parent_room_id | TEXT | - | - | ✓ | ✗ | - | 父Space ID |
| sender | TEXT | - | - | - | ✗ | - | 操作发送者 |
| is_suggested | BOOLEAN | - | - | - | ✗ | FALSE | 是否建议 |
| via_servers | JSONB | - | - | - | ✗ | '[]' | 中转服务器列表 |
| added_ts | BIGINT | - | - | - | ✗ | - | 添加时间 |

**约束**:
- PRIMARY KEY (id)
- UNIQUE (room_id, parent_room_id)
- FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
- FOREIGN KEY (parent_room_id) REFERENCES rooms(room_id) ON DELETE CASCADE

**索引**:
- `idx_room_parents_room` ON (room_id)
- `idx_room_parents_parent` ON (parent_room_id)

---

### 3.8 联邦模块

#### 3.8.1 federation_servers - 联邦服务器表

**功能定位**: 存储联邦服务器状态信息。

| 字段名 | 数据类型 | 长度/精度 | 主键 | 外键 | 可空 | 默认值 | 说明 |
|--------|----------|-----------|------|------|------|--------|------|
| id | BIGSERIAL | - | ✓ | - | - | - | 自增主键 |
| server_name | TEXT | - | - | - | ✗ | - | 服务器名称(唯一) |
| is_blocked | BOOLEAN | - | - | - | ✗ | FALSE | 是否被阻止 |
| blocked_at | BIGINT | - | - | - | ✓ | - | 阻止时间 |
| blocked_reason | TEXT | - | - | - | ✓ | - | 阻止原因 |
| last_successful_connect_at | BIGINT | - | - | - | ✓ | - | 最后成功连接时间 |
| last_failed_connect_at | BIGINT | - | - | - | ✓ | - | 最后失败连接时间 |
| failure_count | INTEGER | - | - | - | ✗ | 0 | 连续失败次数 |

**约束**:
- PRIMARY KEY (id)
- UNIQUE (server_name)

#### 3.8.2 federation_blacklist - 联邦黑名单表

**功能定位**: 存储联邦黑名单服务器。

| 字段名 | 数据类型 | 长度/精度 | 主键 | 外键 | 可空 | 默认值 | 说明 |
|--------|----------|-----------|------|------|------|--------|------|
| id | BIGSERIAL | - | ✓ | - | - | - | 自增主键 |
| server_name | TEXT | - | - | - | ✗ | - | 服务器名称(唯一) |
| reason | TEXT | - | - | - | ✓ | - | 加入原因 |
| added_ts | BIGINT | - | - | - | ✗ | - | 添加时间 |
| added_by | TEXT | - | - | - | ✓ | - | 添加者 |
| updated_ts | BIGINT | - | - | - | ✓ | - | 更新时间 |

**约束**:
- PRIMARY KEY (id)
- UNIQUE (server_name)

**索引**:
- `idx_federation_blacklist_server` ON (server_name)

#### 3.8.3 federation_queue - 联邦队列表

**功能定位**: 存储待发送的联邦事件。

| 字段名 | 数据类型 | 长度/精度 | 主键 | 外键 | 可空 | 默认值 | 说明 |
|--------|----------|-----------|------|------|------|--------|------|
| id | BIGSERIAL | - | ✓ | - | - | - | 自增主键 |
| destination | TEXT | - | - | - | ✗ | - | 目标服务器 |
| event_id | TEXT | - | - | - | ✗ | - | 事件ID |
| event_type | TEXT | - | - | - | ✗ | - | 事件类型 |
| room_id | TEXT | - | - | - | ✓ | - | 房间ID |
| content | JSONB | - | - | - | ✗ | - | 事件内容 |
| created_ts | BIGINT | - | - | - | ✗ | - | 创建时间 |
| sent_at | BIGINT | - | - | - | ✓ | - | 发送时间 |
| retry_count | INTEGER | - | - | - | ✗ | 0 | 重试次数 |
| status | TEXT | - | - | - | ✗ | 'pending' | 状态 |

**约束**:
- PRIMARY KEY (id)

**索引**:
- `idx_federation_queue_destination` ON (destination)
- `idx_federation_queue_status` ON (status)

---

### 3.9 应用服务模块

#### 3.9.1 application_services - 应用服务表

**功能定位**: 存储应用服务配置信息。

| 字段名 | 数据类型 | 长度/精度 | 主键 | 外键 | 可空 | 默认值 | 说明 |
|--------|----------|-----------|------|------|------|--------|------|
| id | BIGSERIAL | - | ✓ | - | - | - | 自增主键 |
| as_id | TEXT | - | - | - | ✗ | - | 应用服务ID(唯一) |
| url | TEXT | - | - | - | ✗ | - | 服务URL |
| as_token | TEXT | - | - | - | ✗ | - | AS令牌 |
| hs_token | TEXT | - | - | - | ✗ | - | HS令牌 |
| sender_localpart | TEXT | - | - | - | ✗ | - | 发送者本地名 |
| is_enabled | BOOLEAN | - | - | - | ✗ | FALSE | 是否启用 |
| rate_limited | BOOLEAN | - | - | - | ✗ | TRUE | 是否限速 |
| protocols | TEXT[] | - | - | - | ✗ | '{}' | 支持的协议 |
| namespaces | JSONB | - | - | - | ✗ | '{}' | 命名空间配置 |
| created_ts | BIGINT | - | - | - | ✗ | - | 创建时间 |
| updated_ts | BIGINT | - | - | - | ✓ | - | 更新时间 |
| description | TEXT | - | - | - | ✓ | - | 描述信息 |

**约束**:
- PRIMARY KEY (id)
- UNIQUE (as_id)

**索引**:
- `idx_application_services_enabled` ON (is_enabled) WHERE is_enabled = TRUE

---

### 3.10 系统配置模块

#### 3.10.1 registration_tokens - 注册令牌表

**功能定位**: 存储注册邀请令牌。

| 字段名 | 数据类型 | 长度/精度 | 主键 | 外键 | 可空 | 默认值 | 说明 |
|--------|----------|-----------|------|------|------|--------|------|
| id | BIGSERIAL | - | ✓ | - | - | - | 自增主键 |
| token | TEXT | - | - | - | ✗ | - | 令牌(唯一) |
| token_type | TEXT | - | - | - | ✗ | 'single_use' | 令牌类型 |
| description | TEXT | - | - | - | ✓ | - | 描述 |
| max_uses | INTEGER | - | - | - | ✗ | 0 | 最大使用次数 |
| uses_count | INTEGER | - | - | - | ✗ | 0 | 已使用次数 |
| is_used | BOOLEAN | - | - | - | ✗ | FALSE | 是否已使用 |
| is_enabled | BOOLEAN | - | - | - | ✗ | TRUE | 是否启用 |
| created_ts | BIGINT | - | - | - | ✗ | - | 创建时间 |
| updated_ts | BIGINT | - | - | - | ✓ | - | 更新时间 |
| expires_at | BIGINT | - | - | - | ✓ | - | 过期时间 |
| last_used_ts | BIGINT | - | - | - | ✓ | - | 最后使用时间 |
| created_by | TEXT | - | - | - | ✗ | - | 创建者 |
| allowed_email_domains | TEXT[] | - | - | - | ✓ | - | 允许的邮箱域名 |
| allowed_user_ids | TEXT[] | - | - | - | ✓ | - | 允许的用户ID列表 |
| auto_join_rooms | TEXT[] | - | - | - | ✓ | - | 自动加入的房间 |
| display_name | TEXT | - | - | - | ✓ | - | 显示名称 |
| email | TEXT | - | - | - | ✓ | - | 邮箱 |

**约束**:
- PRIMARY KEY (id)
- UNIQUE (token)

**索引**:
- `idx_registration_tokens_type` ON (token_type)
- `idx_registration_tokens_expires` ON (expires_at) WHERE expires_at IS NOT NULL
- `idx_registration_tokens_enabled` ON (is_enabled) WHERE is_enabled = TRUE

#### 3.10.2 password_policy - 密码策略表

**功能定位**: 存储系统密码策略配置。

| 字段名 | 数据类型 | 长度/精度 | 主键 | 外键 | 可空 | 默认值 | 说明 |
|--------|----------|-----------|------|------|------|--------|------|
| id | SERIAL | - | ✓ | - | - | - | 自增主键 |
| name | VARCHAR | 100 | - | - | ✗ | - | 策略名称(唯一) |
| value | TEXT | - | - | - | ✗ | - | 策略值 |
| description | TEXT | - | - | - | ✓ | - | 描述 |
| updated_ts | BIGINT | - | - | - | ✗ | - | 更新时间 |

**约束**:
- PRIMARY KEY (id)
- UNIQUE (name)

---

## 4. 索引设计

### 4.1 索引分类

| 索引类型 | 数量 | 用途 |
|----------|------|------|
| 主键索引 | 100+ | 唯一标识记录 |
| 外键索引 | 50+ | 加速表关联查询 |
| 唯一索引 | 80+ | 保障数据唯一性 |
| 复合索引 | 40+ | 优化多字段查询 |
| 部分索引 | 30+ | 减少索引存储 |
| GIN索引 | 10+ | JSONB内容搜索 |

### 4.2 关键查询优化索引

#### 用户认证查询
```sql
-- 用户登录查询
CREATE INDEX idx_access_tokens_user_valid 
ON access_tokens(user_id, is_revoked) WHERE is_revoked = FALSE;

-- 设备列表查询
CREATE INDEX idx_devices_user_last_seen 
ON devices(user_id, last_seen_ts DESC);
```

#### 房间消息查询
```sql
-- 房间消息历史
CREATE INDEX idx_events_room_time 
ON events(room_id, origin_server_ts DESC);

-- 用户参与的房间
CREATE INDEX idx_room_memberships_joined 
ON room_memberships(user_id, room_id) WHERE membership = 'join';
```

#### E2EE密钥查询
```sql
-- 设备密钥查询
CREATE INDEX idx_device_keys_user_device 
ON device_keys(user_id, device_id);

-- Megolm会话查询
CREATE INDEX idx_megolm_sessions_room_time 
ON megolm_sessions(room_id, last_used_ts DESC);
```

### 4.3 JSONB内容搜索索引

```sql
-- 事件内容搜索
CREATE INDEX idx_events_content_gin ON events USING GIN (content);

-- 账户数据搜索
CREATE INDEX idx_account_data_content_gin ON account_data USING GIN (content);

-- 推送规则条件搜索
CREATE INDEX idx_push_rules_conditions_gin ON push_rules USING GIN (conditions);
```

---

## 5. 数据字典

### 5.1 常用数据类型

| 数据类型 | PostgreSQL | Rust | 说明 |
|----------|------------|------|------|
| 用户ID | TEXT | String | Matrix用户ID格式 |
| 房间ID | TEXT | String | Matrix房间ID格式 |
| 事件ID | TEXT | String | Matrix事件ID格式 |
| 毫秒时间戳 | BIGINT | i64 | Unix毫秒时间戳 |
| JSON数据 | JSONB | Value | JSON对象 |
| 布尔值 | BOOLEAN | bool | true/false |
| 自动递增ID | BIGSERIAL | i64 | 自增主键 |
| UUID | UUID | Uuid | 全局唯一标识 |

### 5.2 枚举值说明

#### membership - 房间成员状态
| 值 | 说明 |
|----|------|
| join | 已加入 |
| leave | 已离开 |
| ban | 被禁言 |
| invite | 被邀请 |
| knock | 敲门请求 |

#### presence - 用户在线状态
| 值 | 说明 |
|----|------|
| online | 在线 |
| offline | 离线 |
| unavailable | 不可用 |

#### status - 通用状态
| 值 | 说明 |
|----|------|
| pending | 待处理 |
| processing | 处理中 |
| completed | 已完成 |
| failed | 失败 |
| cancelled | 已取消 |

### 5.3 字段前缀说明

| 前缀 | 用途 | 示例 |
|------|------|------|
| user_ | 用户相关 | user_id, username |
| room_ | 房间相关 | room_id, room_name |
| device_ | 设备相关 | device_id, device_key |
| event_ | 事件相关 | event_id, event_type |
| created_ | 创建相关 | created_ts, created_by |
| updated_ | 更新相关 | updated_ts, updated_by |
| last_ | 最后相关 | last_seen, last_used |
| is_ | 布尔状态 | is_admin, is_verified |
| has_ | 布尔属性 | has_avatar, has_keys |

---

## 6. 安全考虑

### 6.1 数据加密

| 数据类型 | 加密方式 | 说明 |
|----------|----------|------|
| 密码 | Argon2id | 高强度密码哈希 |
| 令牌 | SHA256哈希 | 令牌不存储原文 |
| 私密消息 | E2EE | 端到端加密 |
| 敏感字段 | 应用层加密 | 如identity_key |

### 6.2 访问控制

| 控制类型 | 实现方式 |
|----------|----------|
| 用户隔离 | 外键约束 + 应用层检查 |
| 设备隔离 | 设备ID + 用户验证 |
| 房间权限 | membership状态检查 |
| 管理操作 | is_admin标志验证 |

### 6.3 审计日志

| 事件类型 | 记录表 |
|----------|--------|
| 登录失败 | security_events |
| 密码修改 | password_history |
| 权限变更 | security_events |
| 敏感操作 | module_execution_logs |

### 6.4 数据保护

```sql
-- 密码历史记录防重复
CREATE TABLE password_history (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    password_hash TEXT NOT NULL,
    created_ts BIGINT NOT NULL
);

-- 令牌黑名单防重用
CREATE TABLE token_blacklist (
    id BIGSERIAL PRIMARY KEY,
    token_hash TEXT NOT NULL UNIQUE,
    is_revoked BOOLEAN DEFAULT TRUE,
    expires_at BIGINT
);
```

### 6.5 SQL注入防护

- 所有用户输入通过参数化查询
- 使用sqlx的query_as和query方法
- 禁止字符串拼接SQL
- 输入验证和白名单过滤

---

## 7. 维护建议

### 7.1 定期维护任务

| 任务 | 频率 | 说明 |
|------|------|------|
| VACUUM | 每日 | 清理死元组 |
| ANALYZE | 每日 | 更新统计信息 |
| 索引重建 | 每周 | 修复索引膨胀 |
| 旧数据归档 | 每月 | 归档历史数据 |
| 连接池检查 | 每日 | 检查连接泄漏 |

### 7.2 性能监控指标

| 指标 | 告警阈值 |
|------|----------|
| 查询延迟P99 | > 100ms |
| 连接数使用率 | > 80% |
| 死元组比例 | > 20% |
| 缓存命中率 | < 80% |
| 复制延迟 | > 1s |

### 7.3 容量规划

| 资源 | 估算方式 |
|------|----------|
| 存储空间 | 用户数 × 平均消息数 × 消息大小 |
| 内存 | 连接数 × 连接缓存大小 |
| 索引空间 | 表空间 × 30% |

### 7.4 备份策略

| 备份类型 | 频率 | 保留时间 |
|----------|------|-----------|
| 全量备份 | 每日 | 30天 |
| 增量备份 | 每小时 | 7天 |
| WAL归档 | 实时 | 14天 |

---

## 附录A: 表清单汇总

### A.1 用户与认证 (8表)
1. users - 用户表
2. devices - 设备表
3. access_tokens - 访问令牌表
4. refresh_tokens - 刷新令牌表
5. token_blacklist - 令牌黑名单表
6. user_threepids - 用户第三方身份表
7. openid_tokens - OpenID令牌表
8. password_history - 密码历史表

### A.2 房间与消息 (15表)
9. rooms - 房间表
10. room_memberships - 房间成员表
11. events - 事件表
12. room_summaries - 房间摘要表
13. room_directory - 房间目录表
14. room_aliases - 房间别名表
15. thread_roots - 线程根消息表
16. room_parents - 房间父子关系表
17. event_receipts - 事件回执表
18. read_markers - 读标记表
19. room_state_events - 房间状态事件表
20. room_tags - 房间标签表
21. room_account_data - 房间账户数据表
22. user_account_data - 用户账户数据表
23. room_invites - 房间邀请表

### A.3 E2EE加密 (12表)
24. device_keys - 设备密钥表
25. cross_signing_keys - 跨签名密钥表
26. megolm_sessions - Megolm会话表
27. olm_accounts - Olm账户表
28. olm_sessions - Olm会话表
29. event_signatures - 事件签名表
30. device_signatures - 设备签名表
31. key_backups - 密钥备份表
32. backup_keys - 备份密钥表
33. e2ee_key_requests - E2EE密钥请求表
34. one_time_keys - 一次性密钥表
35. key_rotation_history - 密钥轮转历史表

### A.4 媒体存储 (6表)
36. media_metadata - 媒体元数据表
37. thumbnails - 缩略图表
38. media_quota - 媒体配额表
39. user_media_quota - 用户媒体配额表
40. media_quota_config - 媒体配额配置表
41. remote_media_cache - 远程媒体缓存表

### A.5 推送通知 (8表)
42. pushers - 推送器表
43. push_rules - 推送规则表
44. push_devices - 推送设备表
45. push_notification_queue - 推送通知队列表
46. push_notification_log - 推送通知日志表
47. push_config - 推送配置表
48. notifications - 通知表
49. notification_templates - 通知模板表

### A.6 好友关系 (6表)
50. friends - 好友表
51. friend_requests - 好友请求表
52. friend_categories - 好友分组表
53. blocked_users - 屏蔽用户表
54. private_sessions - 私密会话表
55. private_messages - 私密消息表

### A.7 Space与层级 (6表)
56. spaces - Space表
57. space_children - Space子房间表
58. space_hierarchy - Space层级表
59. thread_subscriptions - 线程订阅表
60. room_ephemeral - 房间临时数据表
61. sliding_sync_rooms - 滑动同步房间缓存表

### A.8 联邦协议 (5表)
62. federation_servers - 联邦服务器表
63. federation_blacklist - 联邦黑名单表
64. federation_queue - 联邦队列表
65. federation_cache - 联邦缓存表
66. application_service_state - 应用服务状态表

### A.9 应用服务 (8表)
67. application_services - 应用服务表
68. application_service_transactions - 应用服务事务表
69. application_service_events - 应用服务事件表
70. application_service_user_namespaces - 应用服务用户命名空间表
71. application_service_room_alias_namespaces - 应用服务房间别名命名空间表
72. application_service_room_namespaces - 应用服务房间命名空间表
73. to_device_messages - To-Device消息表
74. device_lists_changes - 设备列表变更表

### A.10 系统配置 (15表)
75. registration_tokens - 注册令牌表
76. registration_token_usage - 注册令牌使用记录表
77. filters - 用户过滤器表
78. account_data - 账户数据表
79. presence - 用户在线状态表
80. user_directory - 用户目录表
81. password_policy - 密码策略表
82. captcha - 验证码表
83. captcha_send_log - 验证码发送日志表
84. captcha_template - 验证码模板表
85. captcha_config - 验证码配置表
86. background_updates - 后台更新表
87. workers - 工作进程表
88. worker_commands - 工作进程命令表
89. worker_events - 工作进程事件表

### A.11 安全与审计 (10表)
90. event_reports - 事件举报表
91. event_report_history - 事件举报历史表
92. report_rate_limits - 举报速率限制表
93. event_report_stats - 举报统计表
94. security_events - 安全事件表
95. ip_blocks - IP封禁表
96. ip_reputation - IP信誉表
97. spam_check_results - 垃圾检查结果表
98. third_party_rule_results - 第三方规则结果表
99. rendezvous_session - Rendezvous会话表

### A.12 其他功能 (15表)
100. modules - 模块表
101. module_execution_logs - 模块执行日志表
102. account_validity - 账户有效性表
103. password_auth_providers - 密码认证提供者表
104. presence_routes - 在线状态路由表
105. media_callbacks - 媒体回调表
106. rate_limit_callbacks - 速率限制回调表
107. account_data_callbacks - 账户数据回调表
108. search_index - 搜索索引表
109. user_privacy_settings - 用户隐私设置表
110. schema_migrations - Schema迁移记录表
111. db_metadata - 数据库元数据表
112. sync_stream_id - 同步流ID表
113. blocked_rooms - 封禁房间表
114. worker_statistics - 工作进程统计表
115. account_validity - 账户有效性表
116. user_filters - 用户过滤器表
117. voice_messages - 语音消息表
118. voice_usage_stats - 语音使用统计表
119. delayed_events - 延迟事件表
120. user_directory - 用户目录表
121. notification_queue - 通知队列表
122. presence_list - 在线状态列表表
123. user_presence_summary - 用户在线状态摘要表
124. room_categories - 房间分类表
125. room_category_rooms - 房间分类关系表
126. notification_settings - 通知设置表
127. read_receipts - 已读回执表
128. typing_notifications - 打字通知表
129. presence_state - 在线状态表
130. sent_fts - 全文搜索表
131. notification_engines - 通知引擎表
132. notification_channels - 通知渠道表
133. notification_templates - 通知模板表
134. rate_limits - 速率限制表
135. rate_limit_buckets - 速率限制桶表

---

## 附录B: ERD简化图

```
users (1) ──────< (N) devices
  │
  │
  ├─────< (N) access_tokens
  │
  ├─────< (N) refresh_tokens
  │
  ├─────< (N) user_threepids
  │
  ├─────< (N) room_memberships >───── (1) rooms
  │
  ├─────< (N) account_data
  │
  ├─────< (N) pushers
  │
  ├─────< (N) push_rules
  │
  ├─────< (N) presence
  │
  ├─────< (N) friends >───── (1) users
  │
  └─────< (N) device_keys

rooms (1) ──────< (N) room_memberships
  │
  ├─────< (N) events
  │
  ├─────< (1) room_summaries
  │
  ├─────< (N) room_aliases
  │
  ├─────< (N) thread_roots
  │
  ├─────< (N) room_parents >───── (1) rooms (parent)
  │
  └─────< (N) space_children >───── (1) spaces

spaces (1) ──────< (N) space_children
  │
  └─────< (N) room_parents

key_backups (1) ──────< (N) backup_keys
```

---

*文档生成完成 - synapse-rust v6.0.4*
*基于API端点: 656个, 数据库表: 135+*
