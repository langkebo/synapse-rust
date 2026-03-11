# Synapse-Rust 数据字典文档

## 一、文档概述

### 1.1 文档信息

| 项目 | 值 |
|------|-----|
| 数据库类型 | PostgreSQL 16 |
| Schema 版本 | v6.0.0 |
| 表总数 | 114+ |
| 创建日期 | 2026-03-10 |
| 最后更新 | 2026-03-10 |

### 1.2 命名规范

| 规范 | 说明 | 示例 |
|------|------|------|
| 表名 | 小写字母，下划线分隔，复数形式 | `users`, `room_memberships` |
| 主键 | `pk_{table_name}` | `pk_users` |
| 唯一约束 | `uq_{table_name}_{columns}` | `uq_users_username` |
| 外键 | `fk_{table_name}_{columns}` | `fk_devices_user` |
| 索引 | `idx_{table_name}_{columns}` | `idx_users_email` |
| 布尔字段 | `is_` 或 `has_` 前缀 | `is_admin`, `has_guest_access` |
| NOT NULL 时间戳 | `_ts` 后缀 (BIGINT 毫秒) | `created_ts`, `updated_ts` |
| 可空时间戳 | `_at` 后缀 (BIGINT) | `expires_at`, `revoked_at` |
| 外键字段 | `{table}_id` 格式 | `user_id`, `room_id` |

---

## 二、核心用户模块

### 2.1 users (用户表)

存储 Matrix 用户的基本信息。

| 字段名 | 类型 | 可空 | 默认值 | 说明 |
|--------|------|------|--------|------|
| user_id | TEXT | NO | - | 用户唯一标识符，格式: `@username:server` |
| username | TEXT | NO | - | 用户名（唯一） |
| password_hash | TEXT | YES | - | Argon2id 哈希后的密码 |
| is_admin | BOOLEAN | NO | FALSE | 是否为管理员 |
| is_guest | BOOLEAN | NO | FALSE | 是否为访客用户 |
| is_shadow_banned | BOOLEAN | NO | FALSE | 是否被影子封禁 |
| is_deactivated | BOOLEAN | NO | FALSE | 是否已停用 |
| created_ts | BIGINT | NO | - | 创建时间戳（毫秒） |
| updated_at | BIGINT | YES | - | 最后更新时间戳 |
| displayname | TEXT | YES | - | 显示名称 |
| avatar_url | TEXT | YES | - | 头像 URL (mxc://) |
| email | TEXT | YES | - | 电子邮箱 |
| phone | TEXT | YES | - | 手机号码 |
| generation | BIGINT | NO | 0 | 用户代数（用于刷新令牌） |
| consent_version | TEXT | YES | - | 同意协议版本 |
| appservice_id | TEXT | YES | - | 应用服务 ID |
| user_type | TEXT | YES | - | 用户类型 |
| invalid_update_at | BIGINT | YES | - | 无效更新时间 |
| migration_state | TEXT | YES | - | 迁移状态 |
| password_changed_at | BIGINT | YES | - | 密码最后修改时间 |
| must_change_password | BOOLEAN | NO | FALSE | 是否必须修改密码 |
| password_expires_at | BIGINT | YES | - | 密码过期时间 |
| failed_login_attempts | INTEGER | NO | 0 | 登录失败次数 |
| locked_until | BIGINT | YES | - | 账户锁定截止时间 |

**约束**:
- `pk_users`: PRIMARY KEY (user_id)
- `uq_users_username`: UNIQUE (username)

**索引**:
- `idx_users_email`: (email)
- `idx_users_is_admin`: (is_admin)
- `idx_users_must_change_password`: 条件索引 (must_change_password = TRUE)
- `idx_users_password_expires`: 条件索引 (password_expires_at IS NOT NULL)
- `idx_users_locked`: 条件索引 (locked_until IS NOT NULL)

---

### 2.2 user_threepids (用户第三方身份表)

存储用户的邮箱、手机等第三方身份验证信息。

| 字段名 | 类型 | 可空 | 默认值 | 说明 |
|--------|------|------|--------|------|
| id | BIGSERIAL | NO | - | 主键 |
| user_id | TEXT | NO | - | 用户 ID（外键 → users） |
| medium | TEXT | NO | - | 身份类型: `email`, `msisdn` |
| address | TEXT | NO | - | 身份地址 |
| validated_at | BIGINT | YES | - | 验证时间戳 |
| added_ts | BIGINT | NO | - | 添加时间戳 |
| is_verified | BOOLEAN | NO | FALSE | 是否已验证 |
| verification_token | TEXT | YES | - | 验证令牌 |
| verification_expires_at | BIGINT | YES | - | 验证令牌过期时间 |

**约束**:
- `pk_user_threepids`: PRIMARY KEY (id)
- `uq_user_threepids_medium_address`: UNIQUE (medium, address)
- `fk_user_threepids_user`: FOREIGN KEY (user_id) → users(user_id) ON DELETE CASCADE

---

### 2.3 devices (设备表)

存储用户的设备信息。

| 字段名 | 类型 | 可空 | 默认值 | 说明 |
|--------|------|------|--------|------|
| device_id | TEXT | NO | - | 设备唯一标识符 |
| user_id | TEXT | NO | - | 用户 ID（外键 → users） |
| display_name | TEXT | YES | - | 设备显示名称 |
| device_key | JSONB | YES | - | 设备密钥数据 |
| last_seen_at | BIGINT | YES | - | 最后活跃时间 |
| last_seen_ip | TEXT | YES | - | 最后活跃 IP |
| created_ts | BIGINT | NO | - | 创建时间戳 |
| first_seen_ts | BIGINT | NO | - | 首次活跃时间 |
| user_agent | TEXT | YES | - | 用户代理字符串 |
| appservice_id | TEXT | YES | - | 应用服务 ID |
| ignored_user_list | TEXT | YES | - | 忽略的用户列表 |

**约束**:
- `pk_devices`: PRIMARY KEY (device_id)
- `fk_devices_user`: FOREIGN KEY (user_id) → users(user_id) ON DELETE CASCADE

---

### 2.4 access_tokens (访问令牌表)

存储用户的访问令牌。

| 字段名 | 类型 | 可空 | 默认值 | 说明 |
|--------|------|------|--------|------|
| id | BIGSERIAL | NO | - | 主键 |
| token | TEXT | NO | - | 访问令牌（唯一） |
| user_id | TEXT | NO | - | 用户 ID（外键 → users） |
| device_id | TEXT | YES | - | 设备 ID |
| created_ts | BIGINT | NO | - | 创建时间戳 |
| expires_at | BIGINT | YES | - | 过期时间戳 |
| last_used_at | BIGINT | YES | - | 最后使用时间 |
| user_agent | TEXT | YES | - | 用户代理 |
| ip_address | TEXT | YES | - | IP 地址 |
| is_revoked | BOOLEAN | NO | FALSE | 是否已撤销 |
| revoked_at | BIGINT | YES | - | 撤销时间戳 |

**约束**:
- `pk_access_tokens`: PRIMARY KEY (id)
- `uq_access_tokens_token`: UNIQUE (token)
- `fk_access_tokens_user`: FOREIGN KEY (user_id) → users(user_id) ON DELETE CASCADE

---

### 2.5 refresh_tokens (刷新令牌表)

存储用于刷新访问令牌的令牌。

| 字段名 | 类型 | 可空 | 默认值 | 说明 |
|--------|------|------|--------|------|
| id | BIGSERIAL | NO | - | 主键 |
| token_hash | TEXT | NO | - | 令牌哈希（唯一） |
| user_id | TEXT | NO | - | 用户 ID（外键 → users） |
| device_id | TEXT | YES | - | 设备 ID |
| access_token_id | TEXT | YES | - | 关联的访问令牌 ID |
| scope | TEXT | YES | - | 权限范围 |
| created_ts | BIGINT | NO | - | 创建时间戳 |
| expires_at | BIGINT | YES | - | 过期时间戳 |
| last_used_at | BIGINT | YES | - | 最后使用时间 |
| use_count | INTEGER | NO | 0 | 使用次数 |
| is_revoked | BOOLEAN | NO | FALSE | 是否已撤销 |
| revoked_at | BIGINT | YES | - | 撤销时间戳 |
| revoked_reason | TEXT | YES | - | 撤销原因 |
| client_info | JSONB | YES | - | 客户端信息 |
| ip_address | TEXT | YES | - | IP 地址 |
| user_agent | TEXT | YES | - | 用户代理 |

**约束**:
- `pk_refresh_tokens`: PRIMARY KEY (id)
- `uq_refresh_tokens_token_hash`: UNIQUE (token_hash)
- `fk_refresh_tokens_user`: FOREIGN KEY (user_id) → users(user_id) ON DELETE CASCADE

---

## 三、房间模块

### 3.1 rooms (房间表)

存储房间的基本信息。

| 字段名 | 类型 | 可空 | 默认值 | 说明 |
|--------|------|------|--------|------|
| room_id | TEXT | NO | - | 房间唯一标识符 |
| creator | TEXT | YES | - | 创建者用户 ID |
| is_public | BOOLEAN | NO | FALSE | 是否公开房间 |
| room_version | TEXT | NO | '6' | 房间协议版本 |
| created_ts | BIGINT | NO | - | 创建时间戳 |
| last_activity_at | BIGINT | YES | - | 最后活动时间 |
| is_federated | BOOLEAN | NO | TRUE | 是否支持联邦 |
| has_guest_access | BOOLEAN | NO | FALSE | 是否允许访客访问 |
| join_rules | TEXT | NO | 'invite' | 加入规则: `public`, `invite`, `knock`, `private` |
| history_visibility | TEXT | NO | 'shared' | 历史可见性: `shared`, `joined`, `invited`, `world_readable` |
| name | TEXT | YES | - | 房间名称 |
| topic | TEXT | YES | - | 房间主题 |
| avatar_url | TEXT | YES | - | 房间头像 URL |
| canonical_alias | TEXT | YES | - | 规范别名 |
| member_count | INTEGER | NO | 0 | 成员数量 |
| visibility | TEXT | NO | 'private' | 可见性: `public`, `private` |

**约束**:
- `pk_rooms`: PRIMARY KEY (room_id)

---

### 3.2 room_memberships (房间成员表)

存储房间成员关系。

| 字段名 | 类型 | 可空 | 默认值 | 说明 |
|--------|------|------|--------|------|
| id | BIGSERIAL | NO | - | 主键 |
| room_id | TEXT | NO | - | 房间 ID（外键 → rooms） |
| user_id | TEXT | NO | - | 用户 ID（外键 → users） |
| membership | TEXT | NO | - | 成员状态: `join`, `invite`, `leave`, `ban`, `knock` |
| joined_at | BIGINT | YES | - | 加入时间戳 |
| invited_at | BIGINT | YES | - | 邀请时间戳 |
| left_at | BIGINT | YES | - | 离开时间戳 |
| banned_at | BIGINT | YES | - | 封禁时间戳 |
| sender | TEXT | YES | - | 操作发起者 |
| reason | TEXT | YES | - | 操作原因 |
| event_id | TEXT | YES | - | 关联事件 ID |
| event_type | TEXT | YES | - | 事件类型 |
| display_name | TEXT | YES | - | 显示名称 |
| avatar_url | TEXT | YES | - | 头像 URL |
| is_banned | BOOLEAN | NO | FALSE | 是否被封禁 |
| invite_token | TEXT | YES | - | 邀请令牌 |
| updated_at | BIGINT | YES | - | 更新时间戳 |
| join_reason | TEXT | YES | - | 加入原因 |
| banned_by | TEXT | YES | - | 封禁操作者 |
| ban_reason | TEXT | YES | - | 封禁原因 |

**约束**:
- `pk_room_memberships`: PRIMARY KEY (id)
- `uq_room_memberships_room_user`: UNIQUE (room_id, user_id)
- `fk_room_memberships_room`: FOREIGN KEY (room_id) → rooms(room_id) ON DELETE CASCADE
- `fk_room_memberships_user`: FOREIGN KEY (user_id) → users(user_id) ON DELETE CASCADE

---

### 3.3 events (事件表)

存储房间内的所有事件。

| 字段名 | 类型 | 可空 | 默认值 | 说明 |
|--------|------|------|--------|------|
| event_id | TEXT | NO | - | 事件唯一标识符 |
| room_id | TEXT | NO | - | 房间 ID（外键 → rooms） |
| sender | TEXT | NO | - | 发送者用户 ID |
| event_type | TEXT | NO | - | 事件类型: `m.room.message`, `m.room.member` 等 |
| content | JSONB | NO | - | 事件内容 |
| origin_server_ts | BIGINT | NO | - | 源服务器时间戳 |
| state_key | TEXT | YES | - | 状态键（状态事件） |
| is_redacted | BOOLEAN | NO | FALSE | 是否已被删除 |
| redacted_at | BIGINT | YES | - | 删除时间戳 |
| redacted_by | TEXT | YES | - | 删除操作者 |
| transaction_id | TEXT | YES | - | 客户端事务 ID |
| depth | BIGINT | YES | - | 事件深度 |
| prev_events | JSONB | YES | - | 前置事件列表 |
| auth_events | JSONB | YES | - | 授权事件列表 |
| signatures | JSONB | YES | - | 签名数据 |
| hashes | JSONB | YES | - | 哈希数据 |
| unsigned | JSONB | NO | '{}' | 未签名数据 |
| processed_at | BIGINT | YES | - | 处理时间戳 |
| not_before | BIGINT | NO | 0 | 最早处理时间 |
| status | TEXT | YES | - | 处理状态 |
| reference_image | TEXT | YES | - | 引用图片 |
| origin | TEXT | YES | - | 来源服务器 |
| user_id | TEXT | YES | - | 关联用户 ID |

**约束**:
- `pk_events`: PRIMARY KEY (event_id)
- `fk_events_room`: FOREIGN KEY (room_id) → rooms(room_id) ON DELETE CASCADE

---

## 四、E2EE 加密模块

### 4.1 device_keys (设备密钥表)

存储设备的加密密钥。

| 字段名 | 类型 | 可空 | 默认值 | 说明 |
|--------|------|------|--------|------|
| id | BIGSERIAL | NO | - | 主键 |
| user_id | TEXT | NO | - | 用户 ID（外键 → users） |
| device_id | TEXT | NO | - | 设备 ID |
| algorithm | TEXT | NO | - | 算法: `ed25519`, `curve25519` |
| key_id | TEXT | NO | - | 密钥标识符 |
| public_key | TEXT | NO | - | 公钥 |
| key_data | TEXT | YES | - | 密钥数据 |
| signatures | JSONB | YES | - | 签名数据 |
| added_ts | BIGINT | NO | - | 添加时间戳 |
| created_ts | BIGINT | NO | - | 创建时间戳 |
| updated_at | BIGINT | YES | - | 更新时间戳 |
| is_verified | BOOLEAN | NO | FALSE | 是否已验证 |
| is_blocked | BOOLEAN | NO | FALSE | 是否已阻止 |
| display_name | TEXT | YES | - | 显示名称 |

**约束**:
- `pk_device_keys`: PRIMARY KEY (id)
- `uq_device_keys_user_device_key`: UNIQUE (user_id, device_id, key_id)

---

### 4.2 cross_signing_keys (跨签名密钥表)

存储用户的跨签名密钥。

| 字段名 | 类型 | 可空 | 默认值 | 说明 |
|--------|------|------|--------|------|
| id | BIGSERIAL | NO | - | 主键 |
| user_id | TEXT | NO | - | 用户 ID（外键 → users） |
| key_type | TEXT | NO | - | 密钥类型: `master`, `self_signing`, `user_signing` |
| key_data | TEXT | NO | - | 密钥数据 |
| signatures | JSONB | YES | - | 签名数据 |
| added_ts | BIGINT | NO | - | 添加时间戳 |

**约束**:
- `pk_cross_signing_keys`: PRIMARY KEY (id)
- `uq_cross_signing_keys_user_type`: UNIQUE (user_id, key_type)

---

### 4.3 megolm_sessions (Megolm 会话表)

存储 Megolm 加密会话。

| 字段名 | 类型 | 可空 | 默认值 | 说明 |
|--------|------|------|--------|------|
| id | UUID | NO | gen_random_uuid() | 主键 |
| session_id | TEXT | NO | - | 会话标识符（唯一） |
| room_id | TEXT | NO | - | 房间 ID |
| sender_key | TEXT | NO | - | 发送者密钥 |
| session_key | TEXT | NO | - | 会话密钥 |
| algorithm | TEXT | NO | - | 算法: `m.megolm.v1.aes-sha2` |
| message_index | BIGINT | NO | 0 | 消息索引 |
| created_ts | BIGINT | NO | - | 创建时间戳 |
| last_used_at | BIGINT | YES | - | 最后使用时间 |
| expires_at | BIGINT | YES | - | 过期时间戳 |

**约束**:
- `pk_megolm_sessions`: PRIMARY KEY (id)
- `uq_megolm_sessions_session`: UNIQUE (session_id)

---

### 4.4 key_backups (密钥备份表)

存储用户的密钥备份元数据。

| 字段名 | 类型 | 可空 | 默认值 | 说明 |
|--------|------|------|--------|------|
| id | BIGSERIAL | NO | - | 主键 |
| user_id | TEXT | NO | - | 用户 ID（唯一） |
| algorithm | TEXT | NO | - | 备份算法 |
| auth_data | JSONB | NO | - | 认证数据 |
| auth_key | TEXT | YES | - | 认证密钥 |
| version | BIGINT | NO | 1 | 备份版本 |
| created_ts | BIGINT | NO | - | 创建时间戳 |
| updated_at | BIGINT | YES | - | 更新时间戳 |

**约束**:
- `pk_key_backups`: PRIMARY KEY (id)
- `uq_key_backups_user`: UNIQUE (user_id)

---

## 五、推送通知模块

### 5.1 push_devices (推送设备表)

存储用户的推送设备信息。

| 字段名 | 类型 | 可空 | 默认值 | 说明 |
|--------|------|------|--------|------|
| id | BIGSERIAL | NO | - | 主键 |
| user_id | TEXT | NO | - | 用户 ID |
| device_id | TEXT | NO | - | 设备 ID |
| push_kind | TEXT | NO | - | 推送类型: `http`, `email` |
| app_id | TEXT | NO | - | 应用标识符 |
| app_display_name | TEXT | YES | - | 应用显示名称 |
| device_display_name | TEXT | YES | - | 设备显示名称 |
| profile_tag | TEXT | YES | - | 配置文件标签 |
| pushkey | TEXT | NO | - | 推送密钥 |
| lang | TEXT | NO | 'en' | 语言代码 |
| data | JSONB | NO | '{}' | 额外数据 |
| created_ts | BIGINT | NO | - | 创建时间戳 |
| updated_at | BIGINT | YES | - | 更新时间戳 |
| is_enabled | BOOLEAN | NO | TRUE | 是否启用 |

**约束**:
- `pk_push_devices`: PRIMARY KEY (id)
- `uq_push_devices_user_device_pushkey`: UNIQUE (user_id, device_id, pushkey)

---

### 5.2 push_rules (推送规则表)

存储用户的推送规则。

| 字段名 | 类型 | 可空 | 默认值 | 说明 |
|--------|------|------|--------|------|
| id | BIGSERIAL | NO | - | 主键 |
| user_id | TEXT | NO | - | 用户 ID |
| scope | TEXT | NO | - | 规则范围: `global`, `device` |
| rule_id | TEXT | NO | - | 规则标识符 |
| kind | TEXT | NO | - | 规则类型: `override`, `underride`, `sender`, `room`, `content` |
| priority_class | INTEGER | NO | - | 优先级类别 |
| priority | INTEGER | NO | 0 | 优先级 |
| conditions | JSONB | NO | '[]' | 触发条件 |
| actions | JSONB | NO | '[]' | 执行动作 |
| pattern | TEXT | YES | - | 匹配模式 |
| is_default | BOOLEAN | NO | FALSE | 是否为默认规则 |
| is_enabled | BOOLEAN | NO | TRUE | 是否启用 |
| created_ts | BIGINT | NO | - | 创建时间戳 |
| updated_at | BIGINT | YES | - | 更新时间戳 |

**约束**:
- `pk_push_rules`: PRIMARY KEY (id)
- `uq_push_rules_user_scope_rule`: UNIQUE (user_id, scope, rule_id)

---

## 六、联邦模块

### 6.1 federation_servers (联邦服务器表)

存储联邦服务器状态。

| 字段名 | 类型 | 可空 | 默认值 | 说明 |
|--------|------|------|--------|------|
| id | BIGSERIAL | NO | - | 主键 |
| server_name | TEXT | NO | - | 服务器名称（唯一） |
| is_blocked | BOOLEAN | NO | FALSE | 是否被阻止 |
| blocked_at | BIGINT | YES | - | 阻止时间戳 |
| blocked_reason | TEXT | YES | - | 阻止原因 |
| last_successful_connect_at | BIGINT | YES | - | 最后成功连接时间 |
| last_failed_connect_at | BIGINT | YES | - | 最后失败连接时间 |
| failure_count | INTEGER | NO | 0 | 失败次数 |

**约束**:
- `pk_federation_servers`: PRIMARY KEY (id)
- `uq_federation_servers_name`: UNIQUE (server_name)

---

### 6.2 federation_blacklist (联邦黑名单表)

存储联邦黑名单。

| 字段名 | 类型 | 可空 | 默认值 | 说明 |
|--------|------|------|--------|------|
| id | BIGSERIAL | NO | - | 主键 |
| server_name | TEXT | NO | - | 服务器名称（唯一） |
| reason | TEXT | YES | - | 黑名单原因 |
| added_ts | BIGINT | NO | - | 添加时间戳 |
| added_by | TEXT | YES | - | 添加者 |
| updated_at | BIGINT | YES | - | 更新时间戳 |

**约束**:
- `pk_federation_blacklist`: PRIMARY KEY (id)
- `uq_federation_blacklist_name`: UNIQUE (server_name)

---

## 七、媒体存储模块

### 7.1 media_metadata (媒体元数据表)

存储上传媒体的元数据。

| 字段名 | 类型 | 可空 | 默认值 | 说明 |
|--------|------|------|--------|------|
| media_id | TEXT | NO | - | 媒体唯一标识符 |
| server_name | TEXT | NO | - | 来源服务器名称 |
| content_type | TEXT | NO | - | MIME 类型 |
| file_name | TEXT | YES | - | 原始文件名 |
| size | BIGINT | NO | - | 文件大小（字节） |
| uploader_user_id | TEXT | YES | - | 上传者用户 ID |
| created_ts | BIGINT | NO | - | 创建时间戳 |
| last_accessed_at | BIGINT | YES | - | 最后访问时间 |
| quarantine_status | TEXT | YES | - | 隔离状态 |

**约束**:
- `pk_media_metadata`: PRIMARY KEY (media_id)

---

## 八、密码安全模块

### 8.1 password_history (密码历史记录表)

存储用户的历史密码哈希，防止重复使用。

| 字段名 | 类型 | 可空 | 默认值 | 说明 |
|--------|------|------|--------|------|
| id | BIGSERIAL | NO | - | 主键 |
| user_id | TEXT | NO | - | 用户 ID（外键 → users） |
| password_hash | TEXT | NO | - | 密码哈希 |
| created_ts | BIGINT | NO | - | 创建时间戳 |

**约束**:
- `pk_password_history`: PRIMARY KEY (id)
- `fk_password_history_user`: FOREIGN KEY (user_id) → users(user_id) ON DELETE CASCADE

---

### 8.2 password_policy (密码策略配置表)

存储系统级密码策略配置。

| 字段名 | 类型 | 可空 | 默认值 | 说明 |
|--------|------|------|--------|------|
| id | SERIAL | NO | - | 主键 |
| name | VARCHAR(100) | NO | - | 配置项名称（唯一） |
| value | TEXT | NO | - | 配置值 |
| description | TEXT | YES | - | 配置说明 |
| updated_ts | BIGINT | NO | - | 更新时间戳 |

**默认配置**:

| name | value | description |
|------|-------|-------------|
| min_length | 8 | 最小密码长度 |
| max_length | 128 | 最大密码长度 |
| require_uppercase | true | 是否需要大写字母 |
| require_lowercase | true | 是否需要小写字母 |
| require_digit | true | 是否需要数字 |
| require_special | true | 是否需要特殊字符 |
| max_age_days | 90 | 密码最大有效期（天），0表示永不过期 |
| history_count | 5 | 密码历史记录数量，防止重复使用 |
| max_failed_attempts | 5 | 最大登录失败次数，超过后锁定账户 |
| lockout_duration_minutes | 30 | 账户锁定时长（分钟） |
| force_first_login_change | true | 是否强制首次登录修改密码 |

---

## 九、好友系统模块

### 9.1 friends (好友表)

| 字段名 | 类型 | 可空 | 默认值 | 说明 |
|--------|------|------|--------|------|
| id | BIGSERIAL | NO | - | 主键 |
| user_id | TEXT | NO | - | 用户 ID（外键 → users） |
| friend_id | TEXT | NO | - | 好友用户 ID（外键 → users） |
| created_ts | BIGINT | NO | - | 创建时间戳 |

**约束**:
- `pk_friends`: PRIMARY KEY (id)
- `uq_friends_user_friend`: UNIQUE (user_id, friend_id)

---

### 9.2 friend_requests (好友请求表)

| 字段名 | 类型 | 可空 | 默认值 | 说明 |
|--------|------|------|--------|------|
| id | BIGSERIAL | NO | - | 主键 |
| sender_id | TEXT | NO | - | 发送者 ID（外键 → users） |
| receiver_id | TEXT | NO | - | 接收者 ID（外键 → users） |
| message | TEXT | YES | - | 请求消息 |
| status | TEXT | NO | 'pending' | 状态: `pending`, `accepted`, `rejected` |
| created_ts | BIGINT | NO | - | 创建时间戳 |
| updated_at | BIGINT | YES | - | 更新时间戳 |

**约束**:
- `pk_friend_requests`: PRIMARY KEY (id)
- `uq_friend_requests_sender_receiver`: UNIQUE (sender_id, receiver_id)

---

## 十、表关系图

```
┌─────────────────────────────────────────────────────────────┐
│                        users                                 │
│  (用户核心表，存储所有用户信息)                               │
└─────────────────────────────────────────────────────────────┘
        │
        │ 1:N
        ▼
┌───────────────┬───────────────┬───────────────┬───────────────┐
│   devices    │ access_tokens │ refresh_tokens │ user_threepids │
│  (设备信息)   │  (访问令牌)   │  (刷新令牌)    │  (第三方身份)   │
└───────────────┴───────────────┴───────────────┴───────────────┘
        │
        │ N:M (via room_memberships)
        ▼
┌─────────────────────────────────────────────────────────────┐
│                        rooms                                 │
│  (房间核心表，存储所有房间信息)                               │
└─────────────────────────────────────────────────────────────┘
        │
        │ 1:N
        ▼
┌───────────────┬───────────────┬───────────────┬───────────────┐
│    events    │room_memberships│ room_summaries │ room_directory │
│  (房间事件)   │  (房间成员)    │  (房间摘要)    │  (房间目录)     │
└───────────────┴───────────────┴───────────────┴───────────────┘
```

---

## 十一、性能优化索引

### 11.1 复合索引

| 索引名 | 表 | 字段 | 用途 |
|--------|-----|------|------|
| idx_room_memberships_user_membership | room_memberships | (user_id, membership) | 用户房间列表查询 |
| idx_events_room_time | events | (room_id, origin_server_ts DESC) | 房间消息历史查询 |
| idx_device_keys_user_device | device_keys | (user_id, device_id) | 用户设备列表查询 |
| idx_push_rules_user_priority | push_rules | (user_id, priority) | 推送规则匹配 |
| idx_events_sender_type | events | (sender, event_type) | 用户事件查询 |
| idx_room_memberships_room_membership | room_memberships | (room_id, membership) | 房间成员查询 |

### 11.2 JSONB GIN 索引

| 索引名 | 表 | 字段 | 用途 |
|--------|-----|------|------|
| idx_events_content_gin | events | content | 消息内容搜索 |
| idx_account_data_content_gin | account_data | content | 账户数据查询 |
| idx_user_account_data_content_gin | user_account_data | content | 用户账户数据查询 |

---

## 十二、外键约束汇总

| 从表 | 从字段 | 引用表 | 引用字段 | 删除行为 |
|------|--------|--------|----------|----------|
| devices | user_id | users | user_id | CASCADE |
| access_tokens | user_id | users | user_id | CASCADE |
| refresh_tokens | user_id | users | user_id | CASCADE |
| user_threepids | user_id | users | user_id | CASCADE |
| events | room_id | rooms | room_id | CASCADE |
| room_memberships | room_id | rooms | room_id | CASCADE |
| room_memberships | user_id | users | user_id | CASCADE |
| device_keys | user_id | users | user_id | CASCADE |
| cross_signing_keys | user_id | users | user_id | CASCADE |
| push_notification_queue | user_id | users | user_id | CASCADE |
| password_history | user_id | users | user_id | CASCADE |
| friends | user_id | users | user_id | CASCADE |
| friends | friend_id | users | user_id | CASCADE |
| friend_requests | sender_id | users | user_id | CASCADE |
| friend_requests | receiver_id | users | user_id | CASCADE |

---

## 十三、版本历史

| 版本 | 日期 | 变更说明 |
|------|------|----------|
| v6.0.0 | 2026-03-09 | BREAKING CHANGE - 统一字段命名规范，添加缺失表和字段 |
| v5.0.0 | 2026-03-01 | 添加密码安全字段，添加 password_history/password_policy 表 |
| v4.0.0 | 2026-02-15 | 添加好友系统表，添加 E2EE 相关表 |
| v3.0.0 | 2026-02-01 | 添加推送通知表，添加 Space 相关表 |
| v2.0.0 | 2026-01-15 | 添加联邦相关表，添加媒体存储表 |
| v1.0.0 | 2026-01-01 | 初始版本，核心用户和房间表 |

---

*文档生成时间：2026-03-10*
*数据库版本：PostgreSQL 16*
*Schema 版本：v6.0.0*
