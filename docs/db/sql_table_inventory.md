# SQL 表结构统计文档

> **项目**: synapse-rust 数据库全面排查与优化
> **版本**: v1.0.0
> **生成日期**: 2026-03-20
> **源文件**: `migrations/00000000_unified_schema_v6.sql`

---

## 统计概览

| 指标 | 数量 |
|------|------|
| 总表数 | 114+ |
| 总索引数 | 150+ |
| 总外键数 | 30+ |

---

## 第一部分：核心用户表

### 1.1 users (用户表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| user_id | TEXT | NOT NULL PRIMARY KEY | 用户ID |
| username | TEXT | NOT NULL UNIQUE | 用户名 |
| password_hash | TEXT | | 密码哈希 |
| is_admin | BOOLEAN | DEFAULT FALSE | 是否管理员 |
| is_guest | BOOLEAN | DEFAULT FALSE | 是否访客 |
| is_shadow_banned | BOOLEAN | DEFAULT FALSE | 是否影子封禁 |
| is_deactivated | BOOLEAN | DEFAULT FALSE | 是否已停用 |
| created_ts | BIGINT | NOT NULL | 创建时间戳 |
| updated_ts | BIGINT | | 更新时间戳 |
| displayname | TEXT | | 显示名称 |
| avatar_url | TEXT | | 头像URL |
| email | TEXT | | 邮箱 |
| phone | TEXT | | 电话 |
| generation | BIGINT | DEFAULT 0 | 代际 |
| consent_version | TEXT | | 同意版本 |
| appservice_id | TEXT | | 应用服务ID |
| user_type | TEXT | | 用户类型 |
| invalid_update_at | BIGINT | | 无效更新时间 |
| migration_state | TEXT | | 迁移状态 |
| password_changed_ts | BIGINT | | 密码修改时间 |
| must_change_password | BOOLEAN | DEFAULT FALSE | 必须修改密码 |
| password_expires_at | BIGINT | | 密码过期时间 |
| failed_login_attempts | INTEGER | DEFAULT 0 | 登录失败次数 |
| locked_until | BIGINT | | 锁定截止时间 |

**索引**:
- `idx_users_email` ON email
- `idx_users_is_admin` ON is_admin
- `idx_users_must_change_password` ON must_change_password WHERE must_change_password = TRUE
- `idx_users_password_expires` ON password_expires_at WHERE password_expires_at IS NOT NULL
- `idx_users_locked` ON locked_until WHERE locked_until IS NOT NULL

**外键**: 无

---

### 1.2 user_threepids (用户第三方身份表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| user_id | TEXT | NOT NULL | 用户ID |
| medium | TEXT | NOT NULL | 身份类型(邮箱/电话) |
| address | TEXT | NOT NULL | 身份地址 |
| validated_at | BIGINT | | 验证时间 |
| added_ts | BIGINT | NOT NULL | 添加时间 |
| is_verified | BOOLEAN | DEFAULT FALSE | 是否已验证 |
| verification_token | TEXT | | 验证令牌 |
| verification_expires_at | BIGINT | | 验证过期时间 |

**索引**:
- `idx_user_threepids_user` ON user_id
- UNIQUE: (medium, address)

**外键**: user_id REFERENCES users(user_id) ON DELETE CASCADE

---

### 1.3 devices (设备表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| device_id | TEXT | NOT NULL PRIMARY KEY | 设备ID |
| user_id | TEXT | NOT NULL | 用户ID |
| display_name | TEXT | | 设备显示名 |
| device_key | JSONB | | 设备密钥 |
| last_seen_ts | BIGINT | | 最后活跃时间 |
| last_seen_ip | TEXT | | 最后活跃IP |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| first_seen_ts | BIGINT | NOT NULL | 首次活跃时间 |
| user_agent | TEXT | | 用户代理 |
| appservice_id | TEXT | | 应用服务ID |
| ignored_user_list | TEXT | | 忽略用户列表 |

**索引**:
- `idx_devices_user_id` ON user_id
- `idx_devices_last_seen` ON last_seen_ts DESC

**外键**: user_id REFERENCES users(user_id) ON DELETE CASCADE

---

### 1.4 access_tokens (访问令牌表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| token | TEXT | NOT NULL UNIQUE | 令牌 |
| user_id | TEXT | NOT NULL | 用户ID |
| device_id | TEXT | | 设备ID |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| expires_at | BIGINT | | 过期时间 |
| last_used_ts | BIGINT | | 最后使用时间 |
| user_agent | TEXT | | 用户代理 |
| ip_address | TEXT | | IP地址 |
| is_revoked | BOOLEAN | DEFAULT FALSE | 是否已撤销 |
| revoked_at | BIGINT | | 撤销时间 |

**索引**:
- `idx_access_tokens_user_id` ON user_id
- `idx_access_tokens_valid` ON is_revoked WHERE is_revoked = FALSE

**外键**: user_id REFERENCES users(user_id) ON DELETE CASCADE

---

### 1.5 refresh_tokens (刷新令牌表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| token_hash | TEXT | NOT NULL UNIQUE | 令牌哈希 |
| user_id | TEXT | NOT NULL | 用户ID |
| device_id | TEXT | | 设备ID |
| access_token_id | TEXT | | 访问令牌ID |
| scope | TEXT | | 作用域 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| expires_at | BIGINT | | 过期时间 |
| last_used_ts | BIGINT | | 最后使用时间 |
| use_count | INTEGER | DEFAULT 0 | 使用次数 |
| is_revoked | BOOLEAN | DEFAULT FALSE | 是否已撤销 |
| revoked_at | BIGINT | | 撤销时间 |
| revoked_reason | TEXT | | 撤销原因 |
| client_info | JSONB | | 客户端信息 |
| ip_address | TEXT | | IP地址 |
| user_agent | TEXT | | 用户代理 |

**索引**:
- `idx_refresh_tokens_user_id` ON user_id
- `idx_refresh_tokens_revoked` ON is_revoked WHERE is_revoked = FALSE

**外键**: user_id REFERENCES users(user_id) ON DELETE CASCADE

---

### 1.6 token_blacklist (Token 黑名单表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| token_hash | TEXT | NOT NULL UNIQUE | 令牌哈希 |
| token | TEXT | | 令牌 |
| token_type | TEXT | DEFAULT 'access' | 令牌类型 |
| user_id | TEXT | | 用户ID |
| revoked_at | BIGINT | NOT NULL | 撤销时间 |
| reason | TEXT | | 原因 |
| expires_at | BIGINT | | 过期时间 |

**索引**:
- `idx_token_blacklist_hash` ON token_hash

**外键**: 无

---

## 第二部分：房间相关表

### 2.1 rooms (房间表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| room_id | TEXT | NOT NULL PRIMARY KEY | 房间ID |
| creator | TEXT | | 创建者 |
| is_public | BOOLEAN | DEFAULT FALSE | 是否公开 |
| room_version | TEXT | DEFAULT '6' | 房间版本 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| last_activity_ts | BIGINT | | 最后活动时间 |
| is_federated | BOOLEAN | DEFAULT TRUE | 是否联邦 |
| has_guest_access | BOOLEAN | DEFAULT FALSE | 是否有访客访问 |
| join_rules | TEXT | DEFAULT 'invite' | 加入规则 |
| history_visibility | TEXT | DEFAULT 'shared' | 历史可见性 |
| name | TEXT | | 房间名称 |
| topic | TEXT | | 房间主题 |
| avatar_url | TEXT | | 房间头像 |
| canonical_alias | TEXT | | 规范别名 |
| visibility | TEXT | DEFAULT 'private' | 可见性 |

**索引**:
- `idx_rooms_creator` ON creator WHERE creator IS NOT NULL
- `idx_rooms_is_public` ON is_public WHERE is_public = TRUE
- `idx_rooms_last_activity` ON last_activity_ts DESC WHERE last_activity_ts IS NOT NULL

**外键**: 无

---

### 2.2 room_memberships (房间成员表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| room_id | TEXT | NOT NULL | 房间ID |
| user_id | TEXT | NOT NULL | 用户ID |
| membership | TEXT | NOT NULL | 成员关系 |
| joined_ts | BIGINT | | 加入时间 |
| invited_ts | BIGINT | | 邀请时间 |
| left_ts | BIGINT | | 离开时间 |
| banned_ts | BIGINT | | 封禁时间 |
| sender | TEXT | | 发送者 |
| reason | TEXT | | 原因 |
| event_id | TEXT | | 事件ID |
| event_type | TEXT | | 事件类型 |
| display_name | TEXT | | 显示名称 |
| avatar_url | TEXT | | 头像URL |
| is_banned | BOOLEAN | DEFAULT FALSE | 是否已封禁 |
| invite_token | TEXT | | 邀请令牌 |
| updated_ts | BIGINT | | 更新时间 |
| join_reason | TEXT | | 加入原因 |
| banned_by | TEXT | | 封禁者 |
| ban_reason | TEXT | | 封禁原因 |

**索引**:
- `idx_room_memberships_room` ON room_id
- `idx_room_memberships_user` ON user_id
- `idx_room_memberships_membership` ON membership
- `idx_room_memberships_user_membership` ON (user_id, membership)
- `idx_room_memberships_room_membership` ON (room_id, membership)
- `idx_room_memberships_joined` ON (user_id, room_id) WHERE membership = 'join'
- UNIQUE: (room_id, user_id)

**外键**:
- room_id REFERENCES rooms(room_id) ON DELETE CASCADE
- user_id REFERENCES users(user_id) ON DELETE CASCADE

---

### 2.3 events (事件表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| event_id | TEXT | NOT NULL PRIMARY KEY | 事件ID |
| room_id | TEXT | NOT NULL | 房间ID |
| sender | TEXT | NOT NULL | 发送者 |
| event_type | TEXT | NOT NULL | 事件类型 |
| content | JSONB | NOT NULL | 内容 |
| origin_server_ts | BIGINT | NOT NULL | 原始服务器时间 |
| state_key | TEXT | | 状态键 |
| is_redacted | BOOLEAN | DEFAULT FALSE | 是否已删除 |
| redacted_at | BIGINT | | 删除时间 |
| redacted_by | TEXT | | 删除者 |
| transaction_id | TEXT | | 事务ID |
| depth | BIGINT | | 深度 |
| prev_events | JSONB | | 前一事件 |
| auth_events | JSONB | | 认证事件 |
| signatures | JSONB | | 签名 |
| hashes | JSONB | | 哈希 |
| unsigned | JSONB | DEFAULT '{}' | 未签名数据 |
| processed_at | BIGINT | | 处理时间 |
| not_before | BIGINT | DEFAULT 0 | 不早于 |
| status | TEXT | | 状态 |
| reference_image | TEXT | | 引用图片 |
| origin | TEXT | | 来源 |
| user_id | TEXT | | 用户ID |

**索引**:
- `idx_events_room_id` ON room_id
- `idx_events_sender` ON sender
- `idx_events_type` ON event_type
- `idx_events_origin_server_ts` ON origin_server_ts DESC
- `idx_events_not_redacted` ON (room_id, origin_server_ts DESC) WHERE is_redacted = FALSE

**外键**: room_id REFERENCES rooms(room_id) ON DELETE CASCADE

---

### 2.4 room_summaries (房间摘要表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| room_id | TEXT | NOT NULL PRIMARY KEY | 房间ID |
| name | TEXT | | 名称 |
| topic | TEXT | | 主题 |
| canonical_alias | TEXT | | 规范别名 |
| member_count | BIGINT | DEFAULT 0 | 成员数量 |
| joined_members | BIGINT | DEFAULT 0 | 已加入成员 |
| invited_members | BIGINT | DEFAULT 0 | 已邀请成员 |
| hero_users | JSONB | | 核心用户 |
| is_world_readable | BOOLEAN | DEFAULT FALSE | 是否世界可读 |
| can_guest_join | BOOLEAN | DEFAULT FALSE | 能否访客加入 |
| is_federated | BOOLEAN | DEFAULT TRUE | 是否联邦 |
| encryption_state | TEXT | | 加密状态 |
| updated_ts | BIGINT | | 更新时间 |

**索引**: 无

**外键**: 无

---

### 2.5 room_directory (房间目录表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| room_id | TEXT | NOT NULL UNIQUE | 房间ID |
| is_public | BOOLEAN | DEFAULT TRUE | 是否公开 |
| is_searchable | BOOLEAN | DEFAULT TRUE | 是否可搜索 |
| app_service_id | TEXT | | 应用服务ID |
| added_ts | BIGINT | NOT NULL | 添加时间 |

**索引**:
- `idx_room_directory_public` ON is_public WHERE is_public = TRUE

**外键**: room_id REFERENCES rooms(room_id) ON DELETE CASCADE

---

### 2.6 room_aliases (房间别名表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| room_alias | TEXT | NOT NULL PRIMARY KEY | 房间别名 |
| room_id | TEXT | NOT NULL | 房间ID |
| server_name | TEXT | NOT NULL | 服务器名 |
| created_ts | BIGINT | NOT NULL | 创建时间 |

**索引**:
- `idx_room_aliases_room_id` ON room_id

**外键**: room_id REFERENCES rooms(room_id) ON DELETE CASCADE

---

### 2.7 thread_roots (线程根消息表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| room_id | TEXT | NOT NULL | 房间ID |
| event_id | TEXT | NOT NULL | 事件ID |
| sender | TEXT | NOT NULL | 发送者 |
| thread_id | TEXT | | 线程ID |
| reply_count | BIGINT | DEFAULT 0 | 回复数量 |
| last_reply_event_id | TEXT | | 最后回复事件ID |
| last_reply_sender | TEXT | | 最后回复发送者 |
| last_reply_ts | BIGINT | | 最后回复时间 |
| participants | JSONB | DEFAULT '[]' | 参与者 |
| is_fetched | BOOLEAN | DEFAULT FALSE | 是否已获取 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| updated_ts | BIGINT | | 更新时间 |

**索引**:
- `idx_thread_roots_room` ON room_id
- `idx_thread_roots_event` ON event_id
- `idx_thread_roots_thread` ON thread_id
- `idx_thread_roots_last_reply` ON last_reply_ts DESC WHERE last_reply_ts IS NOT NULL
- UNIQUE: (room_id, event_id)

**外键**: room_id REFERENCES rooms(room_id) ON DELETE CASCADE

---

### 2.8 room_parents (房间父关系表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| room_id | TEXT | NOT NULL | 房间ID |
| parent_room_id | TEXT | NOT NULL | 父房间ID |
| sender | TEXT | NOT NULL | 发送者 |
| is_suggested | BOOLEAN | DEFAULT FALSE | 是否建议 |
| via_servers | JSONB | DEFAULT '[]' | 经由服务器 |
| added_ts | BIGINT | NOT NULL | 添加时间 |

**索引**:
- `idx_room_parents_room` ON room_id
- `idx_room_parents_parent` ON parent_room_id
- UNIQUE: (room_id, parent_room_id)

**外键**:
- room_id REFERENCES rooms(room_id) ON DELETE CASCADE
- parent_room_id REFERENCES rooms(room_id) ON DELETE CASCADE

---

## 第三部分：E2EE 加密相关表

### 3.1 device_keys (设备密钥表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| user_id | TEXT | NOT NULL | 用户ID |
| device_id | TEXT | NOT NULL | 设备ID |
| algorithm | TEXT | NOT NULL | 算法 |
| key_id | TEXT | NOT NULL | 密钥ID |
| public_key | TEXT | NOT NULL | 公钥 |
| key_data | TEXT | | 密钥数据 |
| signatures | JSONB | | 签名 |
| added_ts | BIGINT | NOT NULL | 添加时间 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| updated_ts | BIGINT | | 更新时间 |
| ts_updated_ms | BIGINT | | 更新时间(毫秒) |
| is_verified | BOOLEAN | DEFAULT FALSE | 是否已验证 |
| is_blocked | BOOLEAN | DEFAULT FALSE | 是否已阻止 |
| display_name | TEXT | | 显示名称 |

**索引**:
- `idx_device_keys_user_device` ON (user_id, device_id)
- UNIQUE: (user_id, device_id, key_id)

**外键**: 无

---

### 3.2 cross_signing_keys (跨签名密钥表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| user_id | TEXT | NOT NULL | 用户ID |
| key_type | TEXT | NOT NULL | 密钥类型 |
| key_data | TEXT | NOT NULL | 密钥数据 |
| signatures | JSONB | | 签名 |
| added_ts | BIGINT | NOT NULL | 添加时间 |

**索引**:
- `idx_cross_signing_keys_user` ON user_id
- UNIQUE: (user_id, key_type)

**外键**: 无

---

### 3.3 megolm_sessions (Megolm 会话表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | UUID | PRIMARY KEY DEFAULT gen_random_uuid() | 主键 |
| session_id | TEXT | NOT NULL UNIQUE | 会话ID |
| room_id | TEXT | NOT NULL | 房间ID |
| sender_key | TEXT | NOT NULL | 发送者密钥 |
| session_key | TEXT | NOT NULL | 会话密钥 |
| algorithm | TEXT | NOT NULL | 算法 |
| message_index | BIGINT | DEFAULT 0 | 消息索引 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| last_used_ts | BIGINT | | 最后使用时间 |
| expires_at | BIGINT | | 过期时间 |

**索引**:
- `idx_megolm_sessions_room` ON room_id
- `idx_megolm_sessions_session` ON session_id

**外键**: 无

---

### 3.4 event_signatures (事件签名表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | UUID | PRIMARY KEY DEFAULT gen_random_uuid() | 主键 |
| event_id | TEXT | NOT NULL | 事件ID |
| user_id | TEXT | NOT NULL | 用户ID |
| device_id | TEXT | NOT NULL | 设备ID |
| signature | TEXT | NOT NULL | 签名 |
| key_id | TEXT | NOT NULL | 密钥ID |
| algorithm | TEXT | NOT NULL | 算法 |
| created_ts | BIGINT | NOT NULL | 创建时间 |

**索引**:
- `idx_event_signatures_event` ON event_id
- UNIQUE: (event_id, user_id, device_id, key_id)

**外键**: 无

---

### 3.5 device_signatures (设备签名表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| user_id | TEXT | NOT NULL | 用户ID |
| device_id | TEXT | NOT NULL | 设备ID |
| target_user_id | TEXT | NOT NULL | 目标用户ID |
| target_device_id | TEXT | NOT NULL | 目标设备ID |
| algorithm | TEXT | NOT NULL | 算法 |
| signature | TEXT | NOT NULL | 签名 |
| created_ts | BIGINT | NOT NULL | 创建时间 |

**索引**: UNIQUE: (user_id, device_id, target_user_id, target_device_id, algorithm)

**外键**: 无

---

### 3.6 key_backups (密钥备份表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| backup_id | BIGSERIAL | PRIMARY KEY | 备份ID |
| user_id | TEXT | NOT NULL | 用户ID |
| algorithm | TEXT | NOT NULL | 算法 |
| auth_data | JSONB | | 认证数据 |
| auth_key | TEXT | | 认证密钥 |
| version | BIGINT | DEFAULT 1 | 版本 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| updated_ts | BIGINT | | 更新时间 |

**索引**:
- `idx_key_backups_user` ON user_id
- UNIQUE: (user_id, version)

**外键**: 无

---

### 3.7 backup_keys (密钥备份数据表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| backup_id | BIGINT | NOT NULL | 备份ID |
| room_id | TEXT | NOT NULL | 房间ID |
| session_id | TEXT | NOT NULL | 会话ID |
| session_data | JSONB | NOT NULL | 会话数据 |
| created_ts | BIGINT | NOT NULL | 创建时间 |

**索引**:
- `idx_backup_keys_backup` ON backup_id
- `idx_backup_keys_room` ON room_id

**外键**: backup_id REFERENCES key_backups(backup_id) ON DELETE CASCADE

---

### 3.8 olm_accounts (Olm 账户表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| user_id | TEXT | NOT NULL | 用户ID |
| device_id | TEXT | NOT NULL | 设备ID |
| identity_key | TEXT | NOT NULL | 身份密钥 |
| serialized_account | TEXT | NOT NULL | 序列化账户 |
| is_one_time_keys_published | BOOLEAN | DEFAULT FALSE | 已发布一次性密钥 |
| is_fallback_key_published | BOOLEAN | DEFAULT FALSE | 已发布回退密钥 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| updated_ts | BIGINT | NOT NULL | 更新时间 |

**索引**:
- `idx_olm_accounts_user` ON user_id
- `idx_olm_accounts_device` ON device_id
- UNIQUE: (user_id, device_id)

**外键**: 无

---

### 3.9 olm_sessions (Olm 会话表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| user_id | TEXT | NOT NULL | 用户ID |
| device_id | TEXT | NOT NULL | 设备ID |
| session_id | TEXT | NOT NULL UNIQUE | 会话ID |
| sender_key | TEXT | NOT NULL | 发送者密钥 |
| receiver_key | TEXT | NOT NULL | 接收者密钥 |
| serialized_state | TEXT | NOT NULL | 序列化状态 |
| message_index | INTEGER | DEFAULT 0 | 消息索引 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| last_used_ts | BIGINT | NOT NULL | 最后使用时间 |
| expires_at | BIGINT | | 过期时间 |

**索引**:
- `idx_olm_sessions_user_device` ON (user_id, device_id)
- `idx_olm_sessions_sender_key` ON sender_key
- `idx_olm_sessions_expires` ON expires_at WHERE expires_at IS NOT NULL

**外键**: 无

---

### 3.10 e2ee_key_requests (E2EE 密钥请求表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| request_id | TEXT | NOT NULL UNIQUE | 请求ID |
| user_id | TEXT | NOT NULL | 用户ID |
| device_id | TEXT | NOT NULL | 设备ID |
| room_id | TEXT | NOT NULL | 房间ID |
| session_id | TEXT | NOT NULL | 会话ID |
| algorithm | TEXT | NOT NULL | 算法 |
| action | TEXT | NOT NULL | 动作 |
| is_fulfilled | BOOLEAN | DEFAULT FALSE | 是否已满足 |
| fulfilled_by_device | TEXT | | 满足设备 |
| fulfilled_ts | BIGINT | | 满足时间 |
| created_ts | BIGINT | NOT NULL | 创建时间 |

**索引**:
- `idx_e2ee_key_requests_user` ON user_id
- `idx_e2ee_key_requests_session` ON session_id
- `idx_e2ee_key_requests_pending` ON is_fulfilled WHERE is_fulfilled = FALSE

**外键**: 无

---

## 第四部分：媒体存储表

### 4.1 media_metadata (媒体元数据表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| media_id | TEXT | NOT NULL PRIMARY KEY | 媒体ID |
| server_name | TEXT | NOT NULL | 服务器名 |
| content_type | TEXT | NOT NULL | 内容类型 |
| file_name | TEXT | | 文件名 |
| size | BIGINT | NOT NULL | 大小 |
| uploader_user_id | TEXT | | 上传者用户ID |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| last_accessed_at | BIGINT | | 最后访问时间 |
| quarantine_status | TEXT | | 隔离状态 |

**索引**:
- `idx_media_uploader` ON uploader_user_id
- `idx_media_server` ON server_name

**外键**: 无

---

### 4.2 thumbnails (缩略图表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| media_id | TEXT | NOT NULL | 媒体ID |
| width | INTEGER | NOT NULL | 宽度 |
| height | INTEGER | NOT NULL | 高度 |
| method | TEXT | NOT NULL | 方法 |
| content_type | TEXT | NOT NULL | 内容类型 |
| size | BIGINT | NOT NULL | 大小 |
| created_ts | BIGINT | NOT NULL | 创建时间 |

**索引**:
- `idx_thumbnails_media` ON media_id

**外键**: media_id REFERENCES media_metadata(media_id) ON DELETE CASCADE

---

### 4.3 media_quota (媒体配额表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| user_id | TEXT | NOT NULL UNIQUE | 用户ID |
| max_bytes | BIGINT | DEFAULT 1073741824 | 最大字节 |
| used_bytes | BIGINT | DEFAULT 0 | 已用字节 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| updated_ts | BIGINT | | 更新时间 |

**索引**: 无

**外键**: user_id REFERENCES users(user_id) ON DELETE CASCADE

---

## 第五部分：认证相关表 (CAS/SAML)

### 5.1 cas_tickets (CAS 票据表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| ticket_id | TEXT | NOT NULL UNIQUE | 票据ID |
| user_id | TEXT | NOT NULL | 用户ID |
| service_url | TEXT | NOT NULL | 服务URL |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| expires_ts | BIGINT | NOT NULL | 过期时间 |
| consumed_at | BIGINT | | 消费时间 |
| consumed_by | TEXT | | 消费者 |
| is_valid | BOOLEAN | DEFAULT TRUE | 是否有效 |

**索引**:
- `idx_cas_tickets_user` ON user_id

**外键**: 无

---

### 5.2 saml_sessions (SAML 会话表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| session_id | TEXT | NOT NULL UNIQUE | 会话ID |
| user_id | TEXT | NOT NULL | 用户ID |
| name_id | TEXT | | NameID |
| issuer | TEXT | | 发行者 |
| session_index | TEXT | | 会话索引 |
| attributes | JSONB | DEFAULT '{}' | 属性 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| expires_ts | BIGINT | NOT NULL | 过期时间 |
| last_used_ts | BIGINT | NOT NULL | 最后使用时间 |
| status | TEXT | DEFAULT 'active' | 状态 |

**索引**:
- `idx_saml_sessions_user` ON user_id

**外键**: 无

---

## 第六部分：推送通知表

### 6.1 push_devices (推送设备表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| user_id | TEXT | NOT NULL | 用户ID |
| device_id | TEXT | NOT NULL | 设备ID |
| push_kind | TEXT | NOT NULL | 推送类型 |
| app_id | TEXT | NOT NULL | 应用ID |
| app_display_name | TEXT | | 应用显示名 |
| device_display_name | TEXT | | 设备显示名 |
| profile_tag | TEXT | | 配置标签 |
| pushkey | TEXT | NOT NULL | 推送键 |
| lang | TEXT | DEFAULT 'en' | 语言 |
| data | JSONB | DEFAULT '{}' | 数据 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| updated_ts | BIGINT | | 更新时间 |
| is_enabled | BOOLEAN | DEFAULT TRUE | 是否启用 |

**索引**:
- `idx_push_devices_user` ON user_id
- UNIQUE: (user_id, device_id, pushkey)

**外键**: 无

---

### 6.2 push_rules (推送规则表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| user_id | TEXT | NOT NULL | 用户ID |
| scope | TEXT | NOT NULL | 范围 |
| rule_id | TEXT | NOT NULL | 规则ID |
| kind | TEXT | NOT NULL | 类型 |
| priority_class | INTEGER | NOT NULL | 优先级类 |
| priority | INTEGER | DEFAULT 0 | 优先级 |
| conditions | JSONB | DEFAULT '[]' | 条件 |
| actions | JSONB | DEFAULT '[]' | 动作 |
| pattern | TEXT | | 模式 |
| is_default | BOOLEAN | DEFAULT FALSE | 是否默认 |
| is_enabled | BOOLEAN | DEFAULT TRUE | 是否启用 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| updated_ts | BIGINT | | 更新时间 |

**索引**:
- `idx_push_rules_user` ON user_id
- UNIQUE: (user_id, scope, rule_id)

**外键**: 无

---

### 6.3 pushers (推送器表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| user_id | TEXT | NOT NULL | 用户ID |
| device_id | TEXT | NOT NULL | 设备ID |
| pushkey | TEXT | NOT NULL | 推送键 |
| pushkey_ts | BIGINT | NOT NULL | 推送键时间 |
| kind | TEXT | NOT NULL | 类型 |
| app_id | TEXT | NOT NULL | 应用ID |
| app_display_name | TEXT | NOT NULL | 应用显示名 |
| device_display_name | TEXT | NOT NULL | 设备显示名 |
| profile_tag | TEXT | | 配置标签 |
| lang | TEXT | DEFAULT 'en' | 语言 |
| data | JSONB | DEFAULT '{}' | 数据 |
| updated_ts | BIGINT | | 更新时间 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| is_enabled | BOOLEAN | DEFAULT TRUE | 是否启用 |

**索引**:
- `idx_pushers_user` ON user_id
- `idx_pushers_enabled` ON is_enabled WHERE is_enabled = TRUE
- UNIQUE: (user_id, device_id, pushkey)

**外键**: 无

---

## 第七部分：Space 相关表

### 7.1 space_children (Space 子房间表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| space_id | TEXT | NOT NULL | Space ID |
| room_id | TEXT | NOT NULL | 房间ID |
| sender | TEXT | NOT NULL | 发送者 |
| is_suggested | BOOLEAN | DEFAULT FALSE | 是否建议 |
| via_servers | JSONB | DEFAULT '[]' | 经由服务器 |
| added_ts | BIGINT | NOT NULL | 添加时间 |

**索引**:
- `idx_space_children_space` ON space_id
- `idx_space_children_room` ON room_id
- UNIQUE: (space_id, room_id)

**外键**: 无

---

### 7.2 spaces (Spaces 表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| space_id | TEXT | NOT NULL PRIMARY KEY | Space ID |
| name | TEXT | | 名称 |
| creator | TEXT | NOT NULL | 创建者 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| is_public | BOOLEAN | DEFAULT FALSE | 是否公开 |
| is_private | BOOLEAN | DEFAULT TRUE | 是否私有 |
| member_count | BIGINT | DEFAULT 0 | 成员数量 |
| topic | TEXT | | 主题 |
| avatar_url | TEXT | | 头像URL |
| canonical_alias | TEXT | | 规范别名 |
| history_visibility | TEXT | DEFAULT 'shared' | 历史可见性 |
| join_rules | TEXT | DEFAULT 'invite' | 加入规则 |
| room_type | TEXT | DEFAULT 'm.space' | 房间类型 |
| updated_ts | BIGINT | | 更新时间 |

**索引**:
- `idx_spaces_creator` ON creator
- `idx_spaces_public` ON is_public WHERE is_public = TRUE

**外键**: 无

---

## 第八部分：联邦相关表

### 8.1 federation_servers (联邦服务器表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| server_name | TEXT | NOT NULL UNIQUE | 服务器名 |
| is_blocked | BOOLEAN | DEFAULT FALSE | 是否阻止 |
| blocked_at | BIGINT | | 阻止时间 |
| blocked_reason | TEXT | | 阻止原因 |
| last_successful_connect_at | BIGINT | | 最后成功连接时间 |
| last_failed_connect_at | BIGINT | | 最后失败连接时间 |
| failure_count | INTEGER | DEFAULT 0 | 失败次数 |

**外键**: 无

---

### 8.2 federation_blacklist (联邦黑名单表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| server_name | TEXT | NOT NULL UNIQUE | 服务器名 |
| reason | TEXT | | 原因 |
| added_ts | BIGINT | NOT NULL | 添加时间 |
| added_by | TEXT | | 添加者 |
| updated_ts | BIGINT | | 更新时间 |

**索引**:
- `idx_federation_blacklist_server` ON server_name

**外键**: 无

---

### 8.3 federation_queue (联邦队列表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| destination | TEXT | NOT NULL | 目的地 |
| event_id | TEXT | NOT NULL | 事件ID |
| event_type | TEXT | NOT NULL | 事件类型 |
| room_id | TEXT | | 房间ID |
| content | JSONB | NOT NULL | 内容 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| sent_at | BIGINT | | 发送时间 |
| retry_count | INTEGER | DEFAULT 0 | 重试次数 |
| status | TEXT | DEFAULT 'pending' | 状态 |

**索引**:
- `idx_federation_queue_destination` ON destination
- `idx_federation_queue_status` ON status

**外键**: 无

---

## 第九部分：账户数据表

### 9.1 filters (用户过滤器表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| user_id | TEXT | NOT NULL | 用户ID |
| filter_id | TEXT | NOT NULL | 过滤器ID |
| content | JSONB | NOT NULL DEFAULT '{}' | 内容 |
| created_ts | BIGINT | NOT NULL | 创建时间 |

**索引**:
- `idx_filters_user` ON user_id
- `idx_filters_filter_id` ON filter_id
- UNIQUE: (user_id, filter_id)

**外键**: 无

---

### 9.2 openid_tokens (OpenID 令牌表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| token | TEXT | NOT NULL UNIQUE | 令牌 |
| user_id | TEXT | NOT NULL | 用户ID |
| device_id | TEXT | | 设备ID |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| expires_ts | BIGINT | NOT NULL | 过期时间 |
| is_valid | BOOLEAN | DEFAULT TRUE | 是否有效 |

**索引**:
- `idx_openid_tokens_user` ON user_id

**外键**: user_id REFERENCES users(user_id) ON DELETE CASCADE

---

### 9.3 account_data (账户数据表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| user_id | TEXT | NOT NULL | 用户ID |
| data_type | TEXT | NOT NULL | 数据类型 |
| content | JSONB | NOT NULL | 内容 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| updated_ts | BIGINT | NOT NULL | 更新时间 |

**索引**:
- `idx_account_data_user` ON user_id
- UNIQUE: (user_id, data_type)

**外键**: 无

---

## 第十部分：后台任务表

### 10.1 background_updates (后台更新表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| update_name | TEXT | NOT NULL UNIQUE | 更新名称 |
| job_name | TEXT | | 作业名称 |
| job_type | TEXT | | 作业类型 |
| description | TEXT | | 描述 |
| table_name | TEXT | | 表名 |
| column_name | TEXT | | 列名 |
| is_running | BOOLEAN | DEFAULT FALSE | 是否运行中 |
| status | TEXT | DEFAULT 'pending' | 状态 |
| progress | JSONB | DEFAULT '{}' | 进度 |
| total_items | INTEGER | DEFAULT 0 | 总项目数 |
| processed_items | INTEGER | DEFAULT 0 | 已处理项目数 |
| created_ts | BIGINT | | 创建时间 |
| started_ts | BIGINT | | 开始时间 |
| completed_ts | BIGINT | | 完成时间 |
| updated_ts | BIGINT | | 更新时间 |
| error_message | TEXT | | 错误信息 |
| retry_count | INTEGER | DEFAULT 0 | 重试次数 |
| max_retries | INTEGER | DEFAULT 3 | 最大重试次数 |
| batch_size | INTEGER | DEFAULT 100 | 批大小 |
| sleep_ms | INTEGER | DEFAULT 100 | 睡眠毫秒 |
| depends_on | JSONB | DEFAULT '[]' | 依赖 |
| metadata | JSONB | DEFAULT '{}' | 元数据 |

**索引**:
- `idx_background_updates_status` ON status
- `idx_background_updates_running` ON is_running WHERE is_running = TRUE

**外键**: 无

---

### 10.2 workers (工作进程表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| worker_id | TEXT | NOT NULL UNIQUE | 工作进程ID |
| worker_name | TEXT | NOT NULL | 工作进程名称 |
| worker_type | TEXT | NOT NULL | 工作进程类型 |
| host | TEXT | NOT NULL DEFAULT 'localhost' | 主机 |
| port | INTEGER | NOT NULL DEFAULT 8080 | 端口 |
| status | TEXT | NOT NULL DEFAULT 'starting' | 状态 |
| last_heartbeat_ts | BIGINT | | 最后心跳时间 |
| started_ts | BIGINT | NOT NULL | 开始时间 |
| stopped_ts | BIGINT | | 停止时间 |
| config | JSONB | DEFAULT '{}' | 配置 |
| metadata | JSONB | DEFAULT '{}' | 元数据 |
| version | TEXT | | 版本 |
| is_enabled | BOOLEAN | DEFAULT TRUE | 是否启用 |

**索引**:
- `idx_workers_type` ON worker_type
- `idx_workers_status` ON status
- `idx_workers_heartbeat` ON last_heartbeat_ts WHERE last_heartbeat_ts IS NOT NULL

**外键**: 无

---

### 10.3 sync_stream_id (同步流 ID 表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| stream_type | TEXT | | 流类型 |
| last_id | BIGINT | DEFAULT 0 | 最后ID |
| updated_ts | BIGINT | | 更新时间 |

**索引**: UNIQUE: (stream_type)

**外键**: 无

---

## 第十一部分：其他重要表

### 11.1 presence (Presence 表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| user_id | TEXT | NOT NULL PRIMARY KEY | 用户ID |
| status_msg | TEXT | | 状态消息 |
| presence | TEXT | NOT NULL DEFAULT 'offline' | 在线状态 |
| last_active_ts | BIGINT | NOT NULL DEFAULT 0 | 最后活跃时间 |
| status_from | TEXT | | 状态来源 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| updated_ts | BIGINT | NOT NULL | 更新时间 |

**外键**: user_id REFERENCES users(user_id) ON DELETE CASCADE

---

### 11.2 user_directory (用户目录表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| user_id | TEXT | NOT NULL | 用户ID |
| room_id | TEXT | NOT NULL | 房间ID |
| visibility | TEXT | NOT NULL DEFAULT 'private' | 可见性 |
| added_by | TEXT | | 添加者 |
| created_ts | BIGINT | NOT NULL | 创建时间 |

**索引**: 无

**外键**:
- user_id REFERENCES users(user_id) ON DELETE CASCADE
- room_id REFERENCES rooms(room_id) ON DELETE CASCADE

---

### 11.3 read_markers (读标记表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| room_id | TEXT | NOT NULL | 房间ID |
| user_id | TEXT | NOT NULL | 用户ID |
| event_id | TEXT | NOT NULL | 事件ID |
| marker_type | TEXT | NOT NULL | 标记类型 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| updated_ts | BIGINT | NOT NULL | 更新时间 |

**索引**:
- `idx_read_markers_room_user` ON (room_id, user_id)
- UNIQUE: (room_id, user_id, marker_type)

**外键**: 无

---

### 11.4 event_receipts (事件接收表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| event_id | TEXT | NOT NULL | 事件ID |
| room_id | TEXT | NOT NULL | 房间ID |
| user_id | TEXT | NOT NULL | 用户ID |
| receipt_type | TEXT | NOT NULL | 接收类型 |
| ts | BIGINT | NOT NULL | 时间戳 |
| data | JSONB | DEFAULT '{}' | 数据 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| updated_ts | BIGINT | NOT NULL | 更新时间 |

**索引**:
- `idx_event_receipts_event` ON event_id
- `idx_event_receipts_room` ON room_id
- UNIQUE: (event_id, room_id, user_id, receipt_type)

**外键**: 无

---

### 11.5 notifications (通知表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| user_id | TEXT | NOT NULL | 用户ID |
| event_id | TEXT | | 事件ID |
| room_id | TEXT | | 房间ID |
| ts | BIGINT | NOT NULL | 时间戳 |
| notification_type | VARCHAR(50) | DEFAULT 'message' | 通知类型 |
| profile_tag | VARCHAR(255) | | 配置标签 |
| is_read | BOOLEAN | DEFAULT FALSE | 是否已读 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| updated_ts | BIGINT | | 更新时间 |

**索引**:
- `idx_notifications_user_id` ON user_id
- `idx_notifications_ts` ON ts DESC
- `idx_notifications_room` ON room_id

**外键**: 无

---

### 11.6 registration_tokens (注册令牌表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| token | TEXT | NOT NULL UNIQUE | 令牌 |
| token_type | TEXT | DEFAULT 'single_use' | 令牌类型 |
| description | TEXT | | 描述 |
| max_uses | INTEGER | DEFAULT 0 | 最大使用次数 |
| uses_count | INTEGER | DEFAULT 0 | 已使用次数 |
| is_used | BOOLEAN | DEFAULT FALSE | 是否已使用 |
| is_enabled | BOOLEAN | DEFAULT TRUE | 是否启用 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| updated_ts | BIGINT | | 更新时间 |
| expires_at | BIGINT | | 过期时间 |
| last_used_ts | BIGINT | | 最后使用时间 |
| created_by | TEXT | NOT NULL | 创建者 |
| allowed_email_domains | TEXT[] | | 允许的邮箱域 |
| allowed_user_ids | TEXT[] | | 允许的用户ID |
| auto_join_rooms | TEXT[] | | 自动加入房间 |
| display_name | TEXT | | 显示名称 |
| email | TEXT | | 邮箱 |

**索引**:
- `idx_registration_tokens_type` ON token_type
- `idx_registration_tokens_expires` ON expires_at WHERE expires_at IS NOT NULL
- `idx_registration_tokens_enabled` ON is_enabled WHERE is_enabled = TRUE

**外键**: 无

---

### 11.7 event_reports (事件举报表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| event_id | TEXT | NOT NULL | 事件ID |
| room_id | TEXT | NOT NULL | 房间ID |
| reporter_user_id | TEXT | NOT NULL | 举报用户ID |
| reported_user_id | TEXT | | 被举报用户ID |
| event_json | JSONB | | 事件JSON |
| reason | TEXT | | 原因 |
| description | TEXT | | 描述 |
| status | TEXT | DEFAULT 'open' | 状态 |
| score | INTEGER | DEFAULT 0 | 分数 |
| received_ts | BIGINT | NOT NULL | 接收时间 |
| resolved_at | BIGINT | | 解决时间 |
| resolved_by | TEXT | | 解决者 |
| resolution_reason | TEXT | | 解决原因 |

**索引**:
- `idx_event_reports_event` ON event_id
- `idx_event_reports_room` ON room_id
- `idx_event_reports_reporter` ON reporter_user_id
- `idx_event_reports_status` ON status
- `idx_event_reports_received` ON received_ts DESC

**外键**: 无

---

## 第十二部分：密码安全表

### 12.1 password_history (密码历史记录表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| user_id | TEXT | NOT NULL | 用户ID |
| password_hash | TEXT | NOT NULL | 密码哈希 |
| created_ts | BIGINT | NOT NULL | 创建时间 |

**索引**:
- `idx_password_history_user` ON user_id
- `idx_password_history_created` ON created_ts DESC

**外键**: user_id REFERENCES users(user_id) ON DELETE CASCADE

---

### 12.2 password_policy (密码策略配置表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | SERIAL | PRIMARY KEY | 主键 |
| name | VARCHAR(100) | NOT NULL UNIQUE | 名称 |
| value | TEXT | NOT NULL | 值 |
| description | TEXT | | 描述 |
| updated_ts | BIGINT | NOT NULL | 更新时间 |

**外键**: 无

---

## 第十三部分：迁移版本控制表

### 13.1 schema_migrations (迁移记录表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| version | TEXT | NOT NULL UNIQUE | 版本 |
| name | TEXT | | 名称 |
| checksum | TEXT | | 校验和 |
| applied_ts | BIGINT | | 应用时间 |
| execution_time_ms | BIGINT | | 执行时间(毫秒) |
| success | BOOLEAN | NOT NULL DEFAULT TRUE | 是否成功 |
| description | TEXT | | 描述 |
| executed_at | TIMESTAMPTZ | DEFAULT NOW() | 执行时间 |

**索引**:
- `idx_schema_migrations_version` ON version

**外键**: 无

---

## 第十四部分：动态创建的附加表

### 14.1 typing (输入状态表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| user_id | TEXT | NOT NULL | 用户ID |
| room_id | TEXT | NOT NULL | 房间ID |
| typing | BOOLEAN | DEFAULT FALSE | 是否正在输入 |
| last_active_ts | BIGINT | NOT NULL | 最后活跃时间 |

**索引**: UNIQUE: (user_id, room_id)

**外键**: 无

---

### 14.2 search_index (消息搜索索引表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | SERIAL | PRIMARY KEY | 主键 |
| event_id | VARCHAR(255) | NOT NULL UNIQUE | 事件ID |
| room_id | VARCHAR(255) | NOT NULL | 房间ID |
| user_id | VARCHAR(255) | NOT NULL | 用户ID |
| event_type | VARCHAR(255) | NOT NULL | 事件类型 |
| type | VARCHAR(255) | NOT NULL | 类型 |
| content | TEXT | NOT NULL | 内容 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| updated_ts | BIGINT | | 更新时间 |

**索引**:
- `idx_search_index_room` ON room_id
- `idx_search_index_user` ON user_id
- `idx_search_index_type` ON event_type

**外键**: 无

---

### 14.3 room_tags (房间标签表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | SERIAL | PRIMARY KEY | 主键 |
| user_id | VARCHAR(255) | NOT NULL | 用户ID |
| room_id | VARCHAR(255) | NOT NULL | 房间ID |
| tag | VARCHAR(255) | NOT NULL | 标签 |
| order_value | DOUBLE PRECISION | | 排序值 |
| created_ts | BIGINT | NOT NULL | 创建时间 |

**索引**: UNIQUE: (user_id, room_id, tag)

**外键**: 无

---

### 14.4 room_events (房间事件缓存表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | SERIAL | PRIMARY KEY | 主键 |
| event_id | VARCHAR(255) | NOT NULL UNIQUE | 事件ID |
| room_id | VARCHAR(255) | NOT NULL | 房间ID |
| sender | VARCHAR(255) | NOT NULL | 发送者 |
| event_type | VARCHAR(255) | NOT NULL | 事件类型 |
| state_key | VARCHAR(255) | | 状态键 |
| content | JSONB | NOT NULL DEFAULT '{}' | 内容 |
| prev_event_id | VARCHAR(255) | | 前一事件ID |
| origin_server_ts | BIGINT | NOT NULL | 原始服务器时间 |
| created_ts | BIGINT | NOT NULL | 创建时间 |

**索引**:
- `idx_room_events_room` ON room_id
- `idx_room_events_event` ON event_id

**外键**: 无

---

### 14.5 to_device_messages (E2EE To-Device 消息表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | SERIAL | PRIMARY KEY | 主键 |
| sender_user_id | VARCHAR(255) | NOT NULL | 发送用户ID |
| sender_device_id | VARCHAR(255) | NOT NULL | 发送设备ID |
| recipient_user_id | VARCHAR(255) | NOT NULL | 接收用户ID |
| recipient_device_id | VARCHAR(255) | NOT NULL | 接收设备ID |
| event_type | VARCHAR(255) | NOT NULL | 事件类型 |
| content | JSONB | NOT NULL DEFAULT '{}' | 内容 |
| message_id | VARCHAR(255) | | 消息ID |
| stream_id | BIGINT | NOT NULL | 流ID |
| created_ts | BIGINT | NOT NULL | 创建时间 |

**索引**:
- `idx_to_device_recipient` ON (recipient_user_id, recipient_device_id)
- `idx_to_device_stream` ON (recipient_user_id, stream_id)

**外键**: 无

---

### 14.6 device_lists_changes (设备列表变更跟踪表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | SERIAL | PRIMARY KEY | 主键 |
| user_id | VARCHAR(255) | NOT NULL | 用户ID |
| device_id | VARCHAR(255) | | 设备ID |
| change_type | VARCHAR(50) | NOT NULL | 变更类型 |
| stream_id | BIGINT | NOT NULL | 流ID |
| created_ts | BIGINT | NOT NULL | 创建时间 |

**索引**:
- `idx_device_lists_user` ON user_id
- `idx_device_lists_stream` ON stream_id

**外键**: 无

---

### 14.7 room_ephemeral (房间临时数据表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | SERIAL | PRIMARY KEY | 主键 |
| room_id | VARCHAR(255) | NOT NULL | 房间ID |
| event_type | VARCHAR(255) | NOT NULL | 事件类型 |
| user_id | VARCHAR(255) | NOT NULL | 用户ID |
| content | JSONB | NOT NULL DEFAULT '{}' | 内容 |
| stream_id | BIGINT | NOT NULL | 流ID |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| expires_ts | BIGINT | | 过期时间 |

**索引**:
- `idx_room_ephemeral_room` ON room_id

**外键**: 无

---

### 14.8 device_lists_stream (设备列表流位置表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| stream_id | BIGSERIAL | PRIMARY KEY | 流ID |
| user_id | VARCHAR(255) | NOT NULL | 用户ID |
| device_id | VARCHAR(255) | | 设备ID |
| created_ts | BIGINT | NOT NULL | 创建时间 |

**索引**:
- `idx_device_lists_stream_user` ON user_id

**外键**: 无

---

### 14.9 user_filters (用户过滤器持久化表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | SERIAL | PRIMARY KEY | 主键 |
| user_id | VARCHAR(255) | NOT NULL | 用户ID |
| filter_id | VARCHAR(255) | NOT NULL | 过滤器ID |
| filter_json | JSONB | NOT NULL DEFAULT '{}' | 过滤器JSON |
| created_ts | BIGINT | NOT NULL | 创建时间 |

**索引**: UNIQUE: (user_id, filter_id)

**外键**: 无

---

### 14.10 sliding_sync_rooms (Sliding Sync 房间状态缓存表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| user_id | TEXT | NOT NULL | 用户ID |
| device_id | TEXT | NOT NULL | 设备ID |
| room_id | TEXT | NOT NULL | 房间ID |
| conn_id | TEXT | | 连接ID |
| list_key | TEXT | | 列表键 |
| bump_stamp | BIGINT | DEFAULT 0 | 碰撞戳 |
| highlight_count | INTEGER | DEFAULT 0 | 高亮计数 |
| notification_count | INTEGER | DEFAULT 0 | 通知计数 |
| is_dm | BOOLEAN | DEFAULT FALSE | 是否私信 |
| is_encrypted | BOOLEAN | DEFAULT FALSE | 是否加密 |
| is_tombstoned | BOOLEAN | DEFAULT FALSE | 是否已删除 |
| invited | BOOLEAN | DEFAULT FALSE | 是否已邀请 |
| name | TEXT | | 名称 |
| avatar | TEXT | | 头像 |
| timestamp | BIGINT | DEFAULT 0 | 时间戳 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| updated_ts | BIGINT | NOT NULL | 更新时间 |

**索引**:
- UNIQUE: (user_id, device_id, room_id, COALESCE(conn_id, ''))
- `idx_sliding_sync_rooms_user_device` ON (user_id, device_id)
- `idx_sliding_sync_rooms_bump_stamp` ON bump_stamp DESC

**外键**: 无

---

### 14.11 thread_subscriptions (线程订阅表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| room_id | TEXT | NOT NULL | 房间ID |
| thread_id | TEXT | NOT NULL | 线程ID |
| user_id | TEXT | NOT NULL | 用户ID |
| notification_level | TEXT | DEFAULT 'all' | 通知级别 |
| is_muted | BOOLEAN | DEFAULT FALSE | 是否静音 |
| is_pinned | BOOLEAN | DEFAULT FALSE | 是否置顶 |
| subscribed_ts | BIGINT | NOT NULL | 订阅时间 |
| updated_ts | BIGINT | NOT NULL | 更新时间 |

**索引**:
- `idx_thread_subscriptions_room_thread` ON (room_id, thread_id)
- UNIQUE: (room_id, thread_id, user_id)

**外键**: 无

---

### 14.12 space_hierarchy (Space 层级结构表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| space_id | TEXT | NOT NULL | Space ID |
| room_id | TEXT | NOT NULL | 房间ID |
| parent_space_id | TEXT | | 父Space ID |
| depth | INTEGER | DEFAULT 0 | 深度 |
| children | TEXT[] | | 子房间 |
| via_servers | TEXT[] | | 经由服务器 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| updated_ts | BIGINT | NOT NULL | 更新时间 |

**索引**:
- `idx_space_hierarchy_space` ON space_id
- UNIQUE: (space_id, room_id)

**外键**: 无

---

## 第十五部分：其他业务表

### 15.1 server_retention_policy (保留策略表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| policy_name | TEXT | NOT NULL UNIQUE | 策略名称 |
| min_lifetime_days | INTEGER | DEFAULT 90 | 最小生命周期(天) |
| max_lifetime_days | INTEGER | DEFAULT 365 | 最大生命周期(天) |
| allow_per_room_override | BOOLEAN | DEFAULT TRUE | 允许每个房间覆盖 |
| is_default | BOOLEAN | DEFAULT FALSE | 是否默认 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| updated_ts | BIGINT | | 更新时间 |

**索引**:
- `idx_server_retention_policy_default` ON is_default WHERE is_default = TRUE

**外键**: 无

---

### 15.2 user_media_quota (用户媒体配额表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| user_id | TEXT | NOT NULL UNIQUE | 用户ID |
| max_bytes | BIGINT | DEFAULT 1073741824 | 最大字节 |
| used_bytes | BIGINT | DEFAULT 0 | 已用字节 |
| file_count | INTEGER | DEFAULT 0 | 文件数量 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| updated_ts | BIGINT | | 更新时间 |

**索引**:
- `idx_user_media_quota_used` ON used_bytes DESC WHERE used_bytes > 0

**外键**: user_id REFERENCES users(user_id) ON DELETE CASCADE

---

### 15.3 media_quota_config (媒体配额配置表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| config_name | TEXT | NOT NULL UNIQUE | 配置名称 |
| max_file_size | BIGINT | DEFAULT 10485760 | 最大文件大小 |
| max_upload_rate | BIGINT | | 最大上传速率 |
| allowed_content_types | TEXT[] | DEFAULT ARRAY['image/jpeg', 'image/png', 'image/gif', 'video/mp4', 'audio/ogg'] | 允许的内容类型 |
| retention_days | INTEGER | DEFAULT 90 | 保留天数 |
| is_enabled | BOOLEAN | DEFAULT TRUE | 是否启用 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| updated_ts | BIGINT | | 更新时间 |

**索引**:
- `idx_media_quota_config_enabled` ON is_enabled WHERE is_enabled = TRUE

**外键**: 无

---

### 15.4 one_time_keys (一次性密钥表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| user_id | TEXT | NOT NULL | 用户ID |
| device_id | TEXT | NOT NULL | 设备ID |
| algorithm | TEXT | NOT NULL | 算法 |
| key_id | TEXT | NOT NULL | 密钥ID |
| key_data | TEXT | NOT NULL | 密钥数据 |
| is_used | BOOLEAN | DEFAULT FALSE | 是否已使用 |
| used_ts | BIGINT | | 使用时间 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| expires_at | BIGINT | | 过期时间 |

**索引**:
- `idx_one_time_keys_user_device` ON (user_id, device_id)
- `idx_one_time_keys_used` ON is_used WHERE is_used = FALSE
- UNIQUE: (user_id, device_id, algorithm, key_id)

**外键**: user_id REFERENCES users(user_id) ON DELETE CASCADE

---

### 15.5 rendezvous_session (Rendezvous 会话表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| session_id | TEXT | NOT NULL UNIQUE | 会话ID |
| user_id | TEXT | | 用户ID |
| device_id | TEXT | | 设备ID |
| status | TEXT | DEFAULT 'pending' | 状态 |
| content | JSONB | DEFAULT '{}' | 内容 |
| expires_ts | BIGINT | NOT NULL | 过期时间 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| updated_ts | BIGINT | | 更新时间 |

**索引**:
- `idx_rendezvous_session_user` ON user_id WHERE user_id IS NOT NULL
- `idx_rendezvous_session_expires` ON expires_ts
- `idx_rendezvous_session_status` ON status

**外键**: 无

---

### 15.6 application_services (应用服务表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| as_id | TEXT | NOT NULL UNIQUE | 应用服务ID |
| url | TEXT | NOT NULL | URL |
| as_token | TEXT | NOT NULL | 应用服务令牌 |
| hs_token | TEXT | NOT NULL | Homeserver令牌 |
| sender_localpart | TEXT | NOT NULL | 发送者本地部分 |
| is_enabled | BOOLEAN | DEFAULT FALSE | 是否启用 |
| rate_limited | BOOLEAN | DEFAULT TRUE | 是否限流 |
| protocols | TEXT[] | DEFAULT '{}' | 协议 |
| namespaces | JSONB | DEFAULT '{}' | 命名空间 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| updated_ts | BIGINT | | 更新时间 |
| description | TEXT | | 描述 |

**索引**:
- `idx_application_services_enabled` ON is_enabled WHERE is_enabled = TRUE

**外键**: 无

---

### 15.7 application_service_state (应用服务状态表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| as_id | TEXT | NOT NULL | 应用服务ID |
| state_key | TEXT | NOT NULL | 状态键 |
| value | JSONB | NOT NULL | 值 |
| updated_ts | BIGINT | NOT NULL | 更新时间 |

**索引**:
- `idx_application_service_state_as` ON as_id
- UNIQUE: (as_id, state_key)

**外键**: as_id REFERENCES application_services(as_id) ON DELETE CASCADE

---

### 15.8 application_service_transactions (应用服务事务表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| as_id | TEXT | NOT NULL | 应用服务ID |
| txn_id | TEXT | NOT NULL | 事务ID |
| data | JSONB | DEFAULT '{}' | 数据 |
| processed | BOOLEAN | DEFAULT FALSE | 是否已处理 |
| processed_ts | BIGINT | | 处理时间 |
| created_ts | BIGINT | NOT NULL | 创建时间 |

**索引**:
- `idx_application_service_transactions_as` ON as_id
- `idx_application_service_transactions_processed` ON processed WHERE processed = FALSE
- UNIQUE: (as_id, txn_id)

**外键**: 无

---

### 15.9 application_service_events (应用服务事件表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| as_id | TEXT | NOT NULL | 应用服务ID |
| event_id | TEXT | NOT NULL UNIQUE | 事件ID |
| room_id | TEXT | | 房间ID |
| event_type | TEXT | | 事件类型 |
| processed | BOOLEAN | DEFAULT FALSE | 是否已处理 |
| processed_ts | BIGINT | | 处理时间 |
| created_ts | BIGINT | NOT NULL | 创建时间 |

**索引**:
- `idx_application_service_events_as` ON as_id
- `idx_application_service_events_room` ON room_id

**外键**: 无

---

### 15.10 application_service_user_namespaces (应用服务用户命名空间表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| as_id | TEXT | NOT NULL | 应用服务ID |
| namespace | TEXT | NOT NULL | 命名空间 |
| is_exclusive | BOOLEAN | DEFAULT TRUE | 是否独占 |
| created_ts | BIGINT | NOT NULL | 创建时间 |

**索引**:
- `idx_application_service_user_namespaces_as` ON as_id

**外键**: as_id REFERENCES application_services(as_id) ON DELETE CASCADE

---

### 15.11 application_service_room_alias_namespaces (应用服务房间别名命名空间表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| as_id | TEXT | NOT NULL | 应用服务ID |
| namespace | TEXT | NOT NULL | 命名空间 |
| is_exclusive | BOOLEAN | DEFAULT TRUE | 是否独占 |
| created_ts | BIGINT | NOT NULL | 创建时间 |

**外键**: as_id REFERENCES application_services(as_id) ON DELETE CASCADE

---

### 15.12 application_service_room_namespaces (应用服务房间命名空间表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| as_id | TEXT | NOT NULL | 应用服务ID |
| namespace | TEXT | NOT NULL | 命名空间 |
| is_exclusive | BOOLEAN | DEFAULT TRUE | 是否独占 |
| created_ts | BIGINT | NOT NULL | 创建时间 |

**外键**: as_id REFERENCES application_services(as_id) ON DELETE CASCADE

---

## 第十六部分：私有会话表

### 16.1 private_sessions (私密会话表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | VARCHAR(255) | NOT NULL PRIMARY KEY | 会话ID |
| user_id_1 | VARCHAR(255) | NOT NULL | 用户1 ID |
| user_id_2 | VARCHAR(255) | NOT NULL | 用户2 ID |
| session_type | VARCHAR(50) | DEFAULT 'direct' | 会话类型 |
| encryption_key | VARCHAR(255) | | 加密密钥 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| last_activity_ts | BIGINT | NOT NULL | 最后活动时间 |
| updated_ts | BIGINT | | 更新时间 |
| unread_count | INTEGER | DEFAULT 0 | 未读计数 |
| encrypted_content | TEXT | | 加密内容 |

**索引**: UNIQUE: (user_id_1, user_id_2)

**外键**:
- user_id_1 REFERENCES users(user_id) ON DELETE CASCADE
- user_id_2 REFERENCES users(user_id) ON DELETE CASCADE

---

### 16.2 private_messages (私密消息表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| session_id | VARCHAR(255) | NOT NULL | 会话ID |
| sender_id | VARCHAR(255) | NOT NULL | 发送者ID |
| content | TEXT | NOT NULL | 内容 |
| encrypted_content | TEXT | | 加密内容 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| message_type | VARCHAR(50) | DEFAULT 'm.text' | 消息类型 |
| is_read | BOOLEAN | DEFAULT FALSE | 是否已读 |
| read_by_receiver | BOOLEAN | DEFAULT FALSE | 接收者已读 |
| read_at | BIGINT | | 阅读时间 |
| edit_history | JSONB | | 编辑历史 |
| is_deleted | BOOLEAN | DEFAULT FALSE | 是否已删除 |
| deleted_at | BIGINT | | 删除时间 |
| is_edited | BOOLEAN | DEFAULT FALSE | 是否已编辑 |
| unread_count | INTEGER | DEFAULT 0 | 未读计数 |

**索引**:
- `idx_private_messages_session` ON session_id

**外键**:
- session_id REFERENCES private_sessions(id) ON DELETE CASCADE
- sender_id REFERENCES users(user_id) ON DELETE CASCADE

---

## 第十七部分：好友相关表

### 17.1 friends (好友表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| user_id | TEXT | NOT NULL | 用户ID |
| friend_id | TEXT | NOT NULL | 好友ID |
| created_ts | BIGINT | NOT NULL | 创建时间 |

**索引**:
- `idx_friends_user_id` ON user_id
- UNIQUE: (user_id, friend_id)

**外键**:
- user_id REFERENCES users(user_id) ON DELETE CASCADE
- friend_id REFERENCES users(user_id) ON DELETE CASCADE

---

### 17.2 friend_requests (好友请求表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| sender_id | TEXT | NOT NULL | 发送者ID |
| receiver_id | TEXT | NOT NULL | 接收者ID |
| message | TEXT | | 消息 |
| status | TEXT | NOT NULL DEFAULT 'pending' | 状态 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| updated_ts | BIGINT | | 更新时间 |

**索引**:
- `idx_friend_requests_sender` ON sender_id
- `idx_friend_requests_receiver` ON receiver_id
- UNIQUE: (sender_id, receiver_id)

**外键**:
- sender_id REFERENCES users(user_id) ON DELETE CASCADE
- receiver_id REFERENCES users(user_id) ON DELETE CASCADE

---

### 17.3 friend_categories (好友分类表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| user_id | TEXT | NOT NULL | 用户ID |
| name | TEXT | NOT NULL | 名称 |
| color | TEXT | NOT NULL DEFAULT '#000000' | 颜色 |
| created_ts | BIGINT | NOT NULL | 创建时间 |

**外键**: user_id REFERENCES users(user_id) ON DELETE CASCADE

---

### 17.4 blocked_users (黑名单用户表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| user_id | TEXT | NOT NULL | 用户ID |
| blocked_id | TEXT | NOT NULL | 黑名单用户ID |
| reason | TEXT | | 原因 |
| created_ts | BIGINT | NOT NULL | 创建时间 |

**索引**:
- `idx_blocked_users_user_id` ON user_id
- UNIQUE: (user_id, blocked_id)

**外键**:
- user_id REFERENCES users(user_id) ON DELETE CASCADE
- blocked_id REFERENCES users(user_id) ON DELETE CASCADE

---

## 第十八部分：安全事件表

### 18.1 security_events (安全事件表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| event_type | TEXT | NOT NULL | 事件类型 |
| user_id | TEXT | | 用户ID |
| ip_address | TEXT | | IP地址 |
| user_agent | TEXT | | 用户代理 |
| details | JSONB | | 详情 |
| created_ts | BIGINT | NOT NULL | 创建时间 |

**索引**:
- `idx_security_events_user_id` ON user_id
- `idx_security_events_created_ts` ON created_ts

**外键**: 无

---

### 18.2 ip_blocks (IP 封禁表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| ip_address | TEXT | NOT NULL UNIQUE | IP地址 |
| reason | TEXT | | 原因 |
| blocked_ts | BIGINT | NOT NULL | 封禁时间 |
| expires_at | BIGINT | | 过期时间 |

**索引**: 无

**外键**: 无

---

### 18.3 ip_reputation (IP 信誉表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| ip_address | TEXT | NOT NULL UNIQUE | IP地址 |
| score | INTEGER | DEFAULT 0 | 分数 |
| last_seen_ts | BIGINT | NOT NULL | 最后活跃时间 |
| updated_ts | BIGINT | NOT NULL | 更新时间 |
| details | JSONB | | 详情 |

**索引**:
- `idx_ip_reputation_score` ON score

**外键**: 无

---

## 第十九部分：语音消息表

### 19.1 voice_messages (语音消息表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| event_id | TEXT | NOT NULL UNIQUE | 事件ID |
| user_id | TEXT | NOT NULL | 用户ID |
| room_id | TEXT | | 房间ID |
| media_id | TEXT | | 媒体ID |
| duration_ms | INT | NOT NULL | 时长(毫秒) |
| waveform | TEXT | | 波形 |
| mime_type | VARCHAR(100) | | MIME类型 |
| file_size | BIGINT | | 文件大小 |
| transcription | TEXT | | 转录 |
| encryption | JSONB | | 加密 |
| is_processed | BOOLEAN | DEFAULT FALSE | 是否已处理 |
| processed_at | BIGINT | | 处理时间 |
| created_ts | BIGINT | NOT NULL | 创建时间 |

**索引**:
- `idx_voice_messages_room` ON room_id
- `idx_voice_messages_user` ON user_id
- `idx_voice_messages_processed` ON is_processed
- `idx_voice_messages_room_ts` ON (room_id, created_ts DESC)
- `idx_voice_messages_user_ts` ON (user_id, created_ts DESC)

**外键**: 无

---

### 19.2 voice_usage_stats (语音使用统计表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| user_id | TEXT | NOT NULL | 用户ID |
| room_id | TEXT | | 房间ID |
| date | DATE | NOT NULL | 日期 |
| period_start | TIMESTAMP | | 周期开始 |
| period_end | TIMESTAMP | | 周期结束 |
| total_duration_ms | BIGINT | DEFAULT 0 | 总时长(毫秒) |
| total_file_size | BIGINT | DEFAULT 0 | 总文件大小 |
| message_count | BIGINT | DEFAULT 0 | 消息数量 |
| last_active_ts | BIGINT | | 最后活跃时间 |
| created_ts | BIGINT | | 创建时间 |
| updated_ts | BIGINT | | 更新时间 |

**索引**:
- `idx_voice_usage_stats_user` ON user_id
- `idx_voice_usage_stats_date` ON date
- UNIQUE: (user_id, room_id, period_start)

**外键**: 无

---

## 第二十部分：模块管理表

### 20.1 modules (模块管理表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| module_name | TEXT | NOT NULL UNIQUE | 模块名称 |
| module_type | TEXT | NOT NULL | 模块类型 |
| is_enabled | BOOLEAN | DEFAULT TRUE | 是否启用 |
| config | JSONB | DEFAULT '{}' | 配置 |
| priority | INTEGER | DEFAULT 0 | 优先级 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| updated_ts | BIGINT | | 更新时间 |
| description | TEXT | | 描述 |

**索引**:
- `idx_modules_enabled` ON is_enabled

**外键**: 无

---

### 20.2 module_execution_logs (模块执行日志表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| module_id | BIGINT | | 模块ID |
| execution_type | TEXT | NOT NULL | 执行类型 |
| input_data | JSONB | | 输入数据 |
| output_data | JSONB | | 输出数据 |
| is_success | BOOLEAN | DEFAULT TRUE | 是否成功 |
| error_message | TEXT | | 错误信息 |
| execution_time_ms | BIGINT | | 执行时间(毫秒) |
| created_ts | BIGINT | NOT NULL | 创建时间 |

**索引**:
- `idx_module_logs_module` ON module_id
- `idx_module_logs_created` ON created_ts

**外键**: module_id REFERENCES modules(id) ON DELETE CASCADE

---

## 第二十一部分：其他系统表

### 21.1 account_validity (账户有效性检查表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| user_id | TEXT | NOT NULL UNIQUE | 用户ID |
| is_valid | BOOLEAN | DEFAULT TRUE | 是否有效 |
| last_check_at | BIGINT | | 最后检查时间 |
| expiration_at | BIGINT | | 过期时间 |
| renewal_token | TEXT | | 续期令牌 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| updated_ts | BIGINT | | 更新时间 |

**索引**:
- `idx_account_validity_user` ON user_id

**外键**: 无

---

### 21.2 password_auth_providers (密码认证提供者表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| provider_name | TEXT | NOT NULL UNIQUE | 提供者名称 |
| provider_type | TEXT | NOT NULL | 提供者类型 |
| is_enabled | BOOLEAN | DEFAULT TRUE | 是否启用 |
| config | JSONB | DEFAULT '{}' | 配置 |
| priority | INTEGER | DEFAULT 0 | 优先级 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| updated_ts | BIGINT | | 更新时间 |

**外键**: 无

---

### 21.3 presence_routes (Presence 路由表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| route_name | TEXT | NOT NULL UNIQUE | 路由名称 |
| route_type | TEXT | NOT NULL | 路由类型 |
| is_enabled | BOOLEAN | DEFAULT TRUE | 是否启用 |
| config | JSONB | DEFAULT '{}' | 配置 |
| created_ts | BIGINT | NOT NULL | 创建时间 |

**外键**: 无

---

### 21.4 media_callbacks (媒体回调表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| callback_name | TEXT | NOT NULL UNIQUE | 回调名称 |
| callback_type | TEXT | NOT NULL | 回调类型 |
| is_enabled | BOOLEAN | DEFAULT TRUE | 是否启用 |
| url | TEXT | NOT NULL | URL |
| headers | JSONB | DEFAULT '{}' | 请求头 |
| created_ts | BIGINT | NOT NULL | 创建时间 |

**外键**: 无

---

### 21.5 rate_limit_callbacks (速率限制回调表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| callback_name | TEXT | NOT NULL UNIQUE | 回调名称 |
| is_enabled | BOOLEAN | DEFAULT TRUE | 是否启用 |
| config | JSONB | DEFAULT '{}' | 配置 |
| created_ts | BIGINT | NOT NULL | 创建时间 |

**外键**: 无

---

### 21.6 account_data_callbacks (账户数据回调表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| callback_name | TEXT | NOT NULL UNIQUE | 回调名称 |
| is_enabled | BOOLEAN | DEFAULT TRUE | 是否启用 |
| data_types | TEXT[] | DEFAULT '{}' | 数据类型 |
| config | JSONB | DEFAULT '{}' | 配置 |
| created_ts | BIGINT | NOT NULL | 创建时间 |

**外键**: 无

---

### 21.7 registration_token_usage (注册令牌使用记录表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| token_id | BIGINT | | 令牌ID |
| user_id | TEXT | NOT NULL | 用户ID |
| used_ts | BIGINT | NOT NULL | 使用时间 |

**索引**:
- `idx_reg_token_usage_token` ON token_id
- `idx_reg_token_usage_user` ON user_id

**外键**: token_id REFERENCES registration_tokens(id) ON DELETE CASCADE

---

### 21.8 event_report_history (事件举报历史表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| report_id | BIGINT | NOT NULL | 举报ID |
| action | TEXT | NOT NULL | 动作 |
| actor_user_id | TEXT | | 操作用户ID |
| actor_role | TEXT | | 角色 |
| old_status | TEXT | | 旧状态 |
| new_status | TEXT | | 新状态 |
| reason | TEXT | | 原因 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| metadata | JSONB | | 元数据 |

**索引**:
- `idx_event_report_history_report` ON report_id

**外键**: report_id REFERENCES event_reports(id) ON DELETE CASCADE

---

### 21.9 report_rate_limits (举报速率限制表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| user_id | TEXT | NOT NULL UNIQUE | 用户ID |
| report_count | INTEGER | DEFAULT 0 | 举报计数 |
| is_blocked | BOOLEAN | DEFAULT FALSE | 是否被阻止 |
| blocked_until | BIGINT | | 阻止截止时间 |
| last_report_at | BIGINT | | 最后举报时间 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| updated_ts | BIGINT | | 更新时间 |

**索引**:
- `idx_report_rate_limits_user` ON user_id

**外键**: 无

---

### 21.10 event_report_stats (事件举报统计表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| stat_date | DATE | NOT NULL UNIQUE | 统计日期 |
| total_reports | INTEGER | DEFAULT 0 | 总举报数 |
| open_reports | INTEGER | DEFAULT 0 | 开放举报数 |
| resolved_reports | INTEGER | DEFAULT 0 | 已解决举报数 |
| dismissed_reports | INTEGER | DEFAULT 0 | 已驳回举报数 |
| escalated_reports | INTEGER | DEFAULT 0 | 已升级举报数 |
| avg_resolution_time_ms | BIGINT | | 平均解决时间(毫秒) |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| updated_ts | BIGINT | | 更新时间 |

**索引**:
- `idx_event_report_stats_date` ON stat_date

**外键**: 无

---

### 21.11 room_invites (房间邀请表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| room_id | TEXT | NOT NULL | 房间ID |
| inviter | TEXT | NOT NULL | 邀请者 |
| invitee | TEXT | NOT NULL | 被邀请者 |
| is_accepted | BOOLEAN | DEFAULT FALSE | 是否已接受 |
| accepted_at | BIGINT | | 接受时间 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| expires_at | BIGINT | | 过期时间 |

**索引**:
- `idx_room_invites_room` ON room_id
- `idx_room_invites_invitee` ON invitee

**外键**: 无

---

### 21.12 push_notification_queue (推送通知队列表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| user_id | TEXT | NOT NULL | 用户ID |
| device_id | TEXT | NOT NULL | 设备ID |
| event_id | TEXT | NOT NULL | 事件ID |
| room_id | TEXT | NOT NULL | 房间ID |
| notification_type | TEXT | NOT NULL | 通知类型 |
| content | JSONB | DEFAULT '{}' | 内容 |
| is_processed | BOOLEAN | DEFAULT FALSE | 是否已处理 |
| processed_at | BIGINT | | 处理时间 |
| created_ts | BIGINT | NOT NULL | 创建时间 |

**索引**:
- `idx_push_queue_user` ON user_id
- `idx_push_queue_processed` ON is_processed

**外键**: 无

---

### 21.13 push_notification_log (推送通知日志表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| user_id | TEXT | NOT NULL | 用户ID |
| device_id | TEXT | NOT NULL | 设备ID |
| pushkey | TEXT | NOT NULL | 推送键 |
| status | TEXT | NOT NULL | 状态 |
| error_message | TEXT | | 错误信息 |
| retry_count | INTEGER | DEFAULT 0 | 重试次数 |
| last_attempt_at | BIGINT | | 最后尝试时间 |
| created_ts | BIGINT | NOT NULL | 创建时间 |

**索引**:
- `idx_push_log_user` ON user_id
- `idx_push_log_status` ON status

**外键**: 无

---

### 21.14 push_config (推送配置表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| user_id | TEXT | NOT NULL | 用户ID |
| device_id | TEXT | NOT NULL | 设备ID |
| config_type | TEXT | NOT NULL | 配置类型 |
| config_data | JSONB | DEFAULT '{}' | 配置数据 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| updated_ts | BIGINT | | 更新时间 |

**索引**:
- `idx_push_config_user` ON user_id
- UNIQUE: (user_id, device_id, config_type)

**外键**: 无

---

### 21.15 spam_check_results (垃圾信息检查结果表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| event_id | TEXT | NOT NULL | 事件ID |
| room_id | TEXT | NOT NULL | 房间ID |
| user_id | TEXT | NOT NULL | 用户ID |
| spam_score | REAL | DEFAULT 0 | 垃圾分数 |
| is_spam | BOOLEAN | DEFAULT FALSE | 是否垃圾 |
| check_details | JSONB | DEFAULT '{}' | 检查详情 |
| created_ts | BIGINT | NOT NULL | 创建时间 |

**索引**:
- `idx_spam_results_event` ON event_id
- `idx_spam_results_room` ON room_id

**外键**: 无

---

### 21.16 third_party_rule_results (第三方规则结果表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| rule_type | TEXT | NOT NULL | 规则类型 |
| event_id | TEXT | | 事件ID |
| room_id | TEXT | | 房间ID |
| user_id | TEXT | | 用户ID |
| is_allowed | BOOLEAN | DEFAULT TRUE | 是否允许 |
| rule_details | JSONB | DEFAULT '{}' | 规则详情 |
| created_ts | BIGINT | NOT NULL | 创建时间 |

**索引**:
- `idx_third_party_rule_type` ON rule_type

**外键**: 无

---

### 21.17 room_state_events (房间状态事件表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| room_id | TEXT | NOT NULL | 房间ID |
| type | TEXT | NOT NULL | 类型 |
| state_key | TEXT | NOT NULL | 状态键 |
| content | JSONB | NOT NULL | 内容 |
| sender | TEXT | NOT NULL | 发送者 |
| origin_server_ts | BIGINT | NOT NULL | 原始服务器时间 |

**索引**:
- `idx_room_state_events_room` ON room_id
- UNIQUE: (room_id, type, state_key)

**外键**: 无

---

### 21.18 refresh_token_usage (刷新令牌使用记录表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| refresh_token_id | BIGINT | NOT NULL | 刷新令牌ID |
| user_id | TEXT | NOT NULL | 用户ID |
| old_access_token_id | TEXT | | 旧访问令牌ID |
| new_access_token_id | TEXT | | 新访问令牌ID |
| used_ts | BIGINT | NOT NULL | 使用时间 |
| ip_address | TEXT | | IP地址 |
| user_agent | TEXT | | 用户代理 |
| is_success | BOOLEAN | DEFAULT TRUE | 是否成功 |
| error_message | TEXT | | 错误信息 |

**索引**:
- `idx_refresh_token_usage_token` ON refresh_token_id
- `idx_refresh_token_usage_user` ON user_id

**外键**: 无

---

### 21.19 refresh_token_families (刷新令牌家族表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| family_id | TEXT | NOT NULL UNIQUE | 家族ID |
| user_id | TEXT | NOT NULL | 用户ID |
| device_id | TEXT | | 设备ID |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| last_refresh_ts | BIGINT | | 最后刷新时间 |
| refresh_count | INTEGER | DEFAULT 0 | 刷新次数 |
| is_compromised | BOOLEAN | DEFAULT FALSE | 是否已泄露 |
| compromised_at | BIGINT | | 泄露时间 |

**索引**:
- `idx_refresh_token_families_user` ON user_id

**外键**: 无

---

### 21.20 refresh_token_rotations (刷新令牌轮换表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| family_id | TEXT | NOT NULL | 家族ID |
| old_token_hash | TEXT | | 旧令牌哈希 |
| new_token_hash | TEXT | NOT NULL | 新令牌哈希 |
| rotated_ts | BIGINT | NOT NULL | 轮换时间 |
| rotation_reason | TEXT | | 轮换原因 |

**索引**:
- `idx_refresh_token_rotations_family` ON family_id

**外键**: 无

---

### 21.21 worker_commands (工作进程命令表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| command_id | TEXT | NOT NULL UNIQUE | 命令ID |
| target_worker_id | TEXT | NOT NULL | 目标工作进程ID |
| source_worker_id | TEXT | | 源工作进程ID |
| command_type | TEXT | NOT NULL | 命令类型 |
| command_data | JSONB | DEFAULT '{}' | 命令数据 |
| priority | INTEGER | DEFAULT 0 | 优先级 |
| status | TEXT | NOT NULL DEFAULT 'pending' | 状态 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| sent_ts | BIGINT | | 发送时间 |
| completed_ts | BIGINT | | 完成时间 |
| error_message | TEXT | | 错误信息 |
| retry_count | INTEGER | DEFAULT 0 | 重试次数 |
| max_retries | INTEGER | DEFAULT 3 | 最大重试次数 |

**索引**:
- `idx_worker_commands_target` ON target_worker_id
- `idx_worker_commands_status` ON status

**外键**: 无

---

### 21.22 worker_events (工作进程事件表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| event_id | TEXT | NOT NULL UNIQUE | 事件ID |
| stream_id | BIGINT | NOT NULL | 流ID |
| event_type | TEXT | NOT NULL | 事件类型 |
| room_id | TEXT | | 房间ID |
| sender | TEXT | | 发送者 |
| event_data | JSONB | DEFAULT '{}' | 事件数据 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| processed_by | JSONB | DEFAULT '[]' | 已处理 |

**索引**:
- `idx_worker_events_stream` ON stream_id
- `idx_worker_events_type` ON event_type

**外键**: 无

---

### 21.23 worker_statistics (工作进程统计表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| worker_id | TEXT | NOT NULL | 工作进程ID |
| total_messages_sent | BIGINT | DEFAULT 0 | 发送消息总数 |
| total_messages_received | BIGINT | DEFAULT 0 | 接收消息总数 |
| total_errors | BIGINT | DEFAULT 0 | 错误总数 |
| last_message_ts | BIGINT | | 最后消息时间 |
| last_error_ts | BIGINT | | 最后错误时间 |
| avg_processing_time_ms | BIGINT | | 平均处理时间(毫秒) |
| uptime_seconds | BIGINT | DEFAULT 0 | 运行时间(秒) |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| updated_ts | BIGINT | NOT NULL | 更新时间 |

**外键**: 无

---

### 21.24 user_privacy_settings (用户隐私设置表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| user_id | VARCHAR(255) | PRIMARY KEY | 用户ID |
| allow_presence_lookup | BOOLEAN | DEFAULT TRUE | 允许查找在线状态 |
| allow_profile_lookup | BOOLEAN | DEFAULT TRUE | 允许查看资料 |
| allow_room_invites | BOOLEAN | DEFAULT TRUE | 允许房间邀请 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| updated_ts | BIGINT | | 更新时间 |

**外键**: 无

---

### 21.25 captcha_send_log (验证码发送日志表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| captcha_id | TEXT | | 验证码ID |
| captcha_type | TEXT | NOT NULL | 验证码类型 |
| target | TEXT | NOT NULL | 目标 |
| sent_ts | BIGINT | NOT NULL | 发送时间 |
| ip_address | TEXT | | IP地址 |
| user_agent | TEXT | | 用户代理 |
| is_success | BOOLEAN | DEFAULT TRUE | 是否成功 |
| error_message | TEXT | | 错误信息 |
| provider | TEXT | | 提供者 |
| provider_response | TEXT | | 提供者响应 |

**索引**:
- `idx_captcha_send_target` ON target

**外键**: 无

---

### 21.26 registration_captcha (注册验证码表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| captcha_id | TEXT | NOT NULL UNIQUE | 验证码ID |
| captcha_type | TEXT | NOT NULL | 验证码类型 |
| target | TEXT | NOT NULL | 目标 |
| code | TEXT | NOT NULL | 验证码 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| expires_ts | BIGINT | NOT NULL | 过期时间 |
| used_at | BIGINT | | 使用时间 |
| verified_at | BIGINT | | 验证时间 |
| ip_address | TEXT | | IP地址 |
| user_agent | TEXT | | 用户代理 |
| attempt_count | INTEGER | DEFAULT 0 | 尝试次数 |
| max_attempts | INTEGER | DEFAULT 3 | 最大尝试次数 |
| status | TEXT | DEFAULT 'pending' | 状态 |
| metadata | JSONB | DEFAULT '{}' | 元数据 |

**索引**:
- `idx_captcha_target` ON target
- `idx_captcha_status` ON status

**外键**: 无

---

### 21.27 captcha_template (验证码模板表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| template_name | TEXT | NOT NULL UNIQUE | 模板名称 |
| captcha_type | TEXT | NOT NULL | 验证码类型 |
| subject | TEXT | | 主题 |
| content | TEXT | NOT NULL | 内容 |
| variables | JSONB | DEFAULT '{}' | 变量 |
| is_default | BOOLEAN | DEFAULT FALSE | 是否默认 |
| is_enabled | BOOLEAN | DEFAULT TRUE | 是否启用 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| updated_ts | BIGINT | | 更新时间 |

**外键**: 无

---

### 21.28 captcha_config (验证码配置表)

| 列名 | 数据类型 | 约束 | 说明 |
|------|----------|------|------|
| id | BIGSERIAL | PRIMARY KEY | 主键 |
| config_key | TEXT | NOT NULL UNIQUE | 配置键 |
| config_value | TEXT | NOT NULL | 配置值 |
| description | TEXT | | 描述 |
| created_ts | BIGINT | NOT NULL | 创建时间 |
| updated_ts | BIGINT | | 更新时间 |

**外键**: 无

---

## 文档更新记录

| 日期 | 版本 | 更新内容 |
|------|------|----------|
| 2026-03-20 | v1.0.0 | 初始版本，基于 unified_schema_v6.sql 生成 |