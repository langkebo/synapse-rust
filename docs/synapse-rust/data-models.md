# 数据模型文档

> **版本**：1.2.0  
> **创建日期**：2026-01-28  
> **最后更新**：2026-01-29  
> **项目状态**：开发中  
> **参考文档**：[Synapse 官方文档](https://element-hq.github.io/synapse/latest/)、[Matrix 规范](https://spec.matrix.org/)

---

## 一、数据库表结构

### 1.1 用户表 (users)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| user_id | TEXT | PRIMARY KEY | 用户 ID |
| username | TEXT | UNIQUE, NOT NULL | 用户名 |
| password_hash | TEXT | NULLABLE | 密码哈希 |
| is_admin | BOOLEAN | DEFAULT FALSE | 是否管理员（与 admin 字段兼容） |
| is_guest | BOOLEAN | DEFAULT FALSE | 是否访客 |
| consent_version | TEXT | NULLABLE | 同意版本 |
| appservice_id | TEXT | NULLABLE | 应用服务 ID |
| creation_ts | BIGINT | NOT NULL | 创建时间戳（秒） |
| user_type | TEXT | NULLABLE | 用户类型 |
| deactivated | BOOLEAN | DEFAULT FALSE | 是否停用 |
| shadow_banned | BOOLEAN | DEFAULT FALSE | 是否被影子封禁 |
| generation | BIGINT | NOT NULL | 生成号 |
| avatar_url | TEXT | NULLABLE | 头像 URL |
| displayname | TEXT | NULLABLE | 显示名称 |
| invalid_update_ts | BIGINT | NULLABLE | 无效更新时间戳 |
| migration_state | TEXT | NULLABLE | 迁移状态 |

**索引**：
- PRIMARY KEY (user_id)
- UNIQUE INDEX (username)

### 1.2 设备表 (devices)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| device_id | TEXT | PRIMARY KEY | 设备 ID |
| user_id | TEXT | NOT NULL, FOREIGN KEY | 用户 ID |
| display_name | TEXT | NULLABLE | 显示名称 |
| last_seen_ts | BIGINT | NOT NULL | 最后见时间戳 |
| last_seen_ip | TEXT | NULLABLE | 最后见 IP 地址 |
| created_ts | BIGINT | NOT NULL | 创建时间戳（毫秒） |
| ignored_user_list | TEXT | NULLABLE | 忽略用户列表 |
| appservice_id | TEXT | NULLABLE | 应用服务 ID |
| first_seen_ts | BIGINT | DEFAULT 0 | 首次见时间戳（毫秒） |
| user_agent | TEXT | NULLABLE | 用户代理字符串 |
| keys | JSONB | NULLABLE | 设备密钥 |
| device_display_name | TEXT | NULLABLE | 设备显示名称 |

**索引**：
- PRIMARY KEY (device_id)
- INDEX (user_id)
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE

### 1.3 访问令牌表 (access_tokens)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| token | TEXT | UNIQUE, NOT NULL | 访问令牌 |
| user_id | TEXT | NOT NULL, FOREIGN KEY | 用户 ID |
| device_id | TEXT | NULLABLE, FOREIGN KEY | 设备 ID |
| created_ts | BIGINT | NOT NULL | 创建时间戳（毫秒） |
| expired_ts | BIGINT | NULLABLE | 过期时间戳（毫秒） |
| invalidated | BOOLEAN | DEFAULT FALSE | 是否已失效 |
| invalidated_ts | BIGINT | NULLABLE | 失效时间戳（毫秒） |

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
| token | TEXT | UNIQUE, NOT NULL | 刷新令牌 |
| user_id | TEXT | NOT NULL, FOREIGN KEY | 用户 ID |
| device_id | TEXT | NOT NULL, FOREIGN KEY | 设备 ID |
| created_ts | BIGINT | NOT NULL | 创建时间戳（毫秒） |
| expired_ts | BIGINT | NULLABLE | 过期时间戳（毫秒） |
| invalidated | BOOLEAN | DEFAULT FALSE | 是否已失效 |
| invalidated_ts | BIGINT | NULLABLE | 失效时间戳（毫秒） |

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
| room_id | TEXT | PRIMARY KEY | 房间 ID |
| is_public | BOOLEAN | NOT NULL, DEFAULT FALSE | 是否公开 |
| creator | TEXT | NOT NULL | 创建者 |
| creation_ts | BIGINT | NOT NULL | 创建时间戳（毫秒） |
| federate | BOOLEAN | NOT NULL, DEFAULT TRUE | 是否允许联邦 |
| version | TEXT | NOT NULL, DEFAULT '1' | 版本 |
| name | TEXT | NULLABLE | 房间名称 |
| topic | TEXT | NULLABLE | 房间主题 |
| avatar | TEXT | NULLABLE | 房间头像 |
| canonical_alias | TEXT | NULLABLE | 规范别名 |
| guest_access | BOOLEAN | DEFAULT FALSE | 访客访问 |
| history_visibility | TEXT | DEFAULT 'shared' | 历史可见性 |
| encryption | TEXT | NULLABLE | 加密 |
| is_flaged | BOOLEAN | DEFAULT FALSE | 是否标记 |
| is_spotlight | BOOLEAN | DEFAULT FALSE | 是否聚光灯 |
| deleted_ts | BIGINT | NULLABLE | 删除时间戳（毫秒） |
| join_rule | TEXT | NULLABLE | 加入规则 |
| member_count | INTEGER | DEFAULT 0 | 成员数量 |

**索引**：
- PRIMARY KEY (room_id)
- INDEX (creator)
- INDEX (canonical_alias)

### 1.6 房间事件表 (room_events / events)

> **注意**：实际实现中使用 `room_events` 表名，events 为别名。

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| event_id | TEXT | PRIMARY KEY | 事件 ID |
| room_id | TEXT | NOT NULL, FOREIGN KEY | 房间 ID |
| user_id | TEXT | NOT NULL, FOREIGN KEY | 用户 ID |
| type | TEXT | NOT NULL | 事件类型（原：event_type） |
| content | TEXT | NOT NULL | 事件内容（JSON 序列化存储） |
| state_key | TEXT | NULLABLE | 状态键 |
| depth | BIGINT | NOT NULL, DEFAULT 0 | 深度 |
| origin_server_ts | BIGINT | NOT NULL | 源服务器时间戳（毫秒） |
| processed_ts | BIGINT | NOT NULL | 处理时间戳（毫秒） |
| not_before | BIGINT | DEFAULT 0 | 不早于 |
| status | TEXT | NULLABLE | 状态 |
| reference_image | TEXT | NULLABLE | 参考图片 |
| origin | TEXT | NOT NULL | 源服务器 |
| sender | TEXT | NOT NULL | 发送者 |
| unsigned | TEXT | NULLABLE | 无符号数据 |
| redacted | BOOLEAN | DEFAULT FALSE | 是否已删除 |

**索引**：
- PRIMARY KEY (event_id)
- INDEX (room_id)
- INDEX (user_id)
- INDEX (origin_server_ts)
- INDEX (type)
- FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE

### 1.7 房间成员关系表 (room_memberships)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| room_id | TEXT | NOT NULL, FOREIGN KEY | 房间 ID |
| user_id | TEXT | NOT NULL, FOREIGN KEY | 用户 ID |
| sender | TEXT | NOT NULL | 发送者 |
| membership | TEXT | NOT NULL | 成员关系（join, leave, ban, invite, knock） |
| event_id | TEXT | NOT NULL | 事件 ID |
| event_type | TEXT | NOT NULL | 事件类型 |
| display_name | TEXT | NULLABLE | 显示名称 |
| avatar_url | TEXT | NULLABLE | 头像 URL |
| is_banned | BOOLEAN | DEFAULT FALSE | 是否被封禁 |
| invite_token | TEXT | NULLABLE | 邀请令牌 |
| inviter | TEXT | NULLABLE | 邀请者 |
| updated_ts | BIGINT | NULLABLE | 更新时间戳（毫秒） |
| joined_ts | BIGINT | NULLABLE | 加入时间戳（毫秒） |
| left_ts | BIGINT | NULLABLE | 离开时间戳（毫秒） |
| reason | TEXT | NULLABLE | 原因 |
| join_reason | TEXT | NULLABLE | 加入原因 |
| banned_by | TEXT | NULLABLE | 封禁者 |
| ban_reason | TEXT | NULLABLE | 封禁原因 |
| ban_ts | BIGINT | NULLABLE | 封禁时间戳（毫秒） |

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
| user_id | TEXT | PRIMARY KEY, FOREIGN KEY | 用户 ID |
| status_msg | TEXT | NULLABLE | 状态消息 |
| presence | TEXT | NOT NULL, DEFAULT 'offline' | 在线状态（online, offline, unavailable） |
| last_active_ts | BIGINT | NOT NULL, DEFAULT 0 | 最后活跃时间戳（毫秒） |
| status_from | TEXT | NULLABLE | 状态来源 |
| created_ts | BIGINT | NOT NULL | 创建时间戳（毫秒） |
| updated_ts | BIGINT | NOT NULL | 更新时间戳（毫秒） |

**索引**：
- PRIMARY KEY (user_id)
- INDEX (presence)
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE

### 1.9 用户目录表 (user_directory)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| user_id | TEXT | NOT NULL, FOREIGN KEY | 用户 ID |
| room_id | TEXT | NOT NULL, FOREIGN KEY | 房间 ID |
| visibility | TEXT | NOT NULL, DEFAULT 'private' | 可见性 |
| added_by | TEXT | NULLABLE | 添加者 |
| created_ts | BIGINT | NOT NULL | 创建时间戳（毫秒） |

**索引**：
- PRIMARY KEY (user_id, room_id)
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
- FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE

### 1.10 好友表 (friends)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| user_id | TEXT | NOT NULL, FOREIGN KEY | 用户 ID |
| friend_id | TEXT | NOT NULL, FOREIGN KEY | 好友 ID |
| created_ts | BIGINT | NOT NULL | 创建时间戳（毫秒） |

**索引**：
- PRIMARY KEY (id)
- UNIQUE (user_id, friend_id)
- INDEX (friend_id)
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
- FOREIGN KEY (friend_id) REFERENCES users(user_id) ON DELETE CASCADE

### 1.11 好友请求表 (friend_requests)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| sender_id | TEXT | NOT NULL, FOREIGN KEY | 发送者 ID |
| receiver_id | TEXT | NOT NULL, FOREIGN KEY | 接收者 ID |
| message | TEXT | NULLABLE | 消息 |
| status | TEXT | NOT NULL, DEFAULT 'pending' | 状态（pending, accepted, rejected） |
| created_ts | BIGINT | NOT NULL | 创建时间戳（毫秒） |
| updated_ts | BIGINT | NULLABLE | 更新时间戳（毫秒） |

**索引**：
- PRIMARY KEY (id)
- UNIQUE (sender_id, receiver_id)
- INDEX (sender_id)
- INDEX (receiver_id)
- INDEX (status)
- FOREIGN KEY (sender_id) REFERENCES users(user_id) ON DELETE CASCADE
- FOREIGN KEY (receiver_id) REFERENCES users(user_id) ON DELETE CASCADE

### 1.12 好友分类表 (friend_categories)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| user_id | TEXT | NOT NULL, FOREIGN KEY | 用户 ID |
| name | TEXT | NOT NULL | 分类名称 |
| color | TEXT | NOT NULL, DEFAULT '#000000' | 颜色 |
| created_ts | BIGINT | NOT NULL | 创建时间戳（毫秒） |

**索引**：
- PRIMARY KEY (id)
- INDEX (user_id)
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE

### 1.13 黑名单表 (blocked_users)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| user_id | TEXT | NOT NULL, FOREIGN KEY | 用户 ID |
| blocked_id | TEXT | NOT NULL, FOREIGN KEY | 被封禁用户 ID |
| reason | TEXT | NULLABLE | 原因 |
| created_ts | BIGINT | NOT NULL | 封禁时间戳（毫秒） |

**索引**：
- PRIMARY KEY (id)
- UNIQUE (user_id, blocked_id)
- INDEX (blocked_id)
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
- FOREIGN KEY (blocked_id) REFERENCES users(user_id) ON DELETE CASCADE

### 1.14 私聊会话表 (private_sessions)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| user_id_1 | TEXT | NOT NULL, FOREIGN KEY | 用户1 ID |
| user_id_2 | TEXT | NOT NULL, FOREIGN KEY | 用户2 ID |
| last_message | TEXT | NULLABLE | 最后消息 |
| unread_count | INTEGER | DEFAULT 0 | 未读消息数 |
| created_ts | BIGINT | NOT NULL | 创建时间戳（毫秒） |
| updated_ts | BIGINT | NULLABLE | 更新时间戳（毫秒） |

**索引**：
- PRIMARY KEY (id)
- UNIQUE (user_id_1, user_id_2)
- INDEX (user_id_1)
- INDEX (user_id_2)
- FOREIGN KEY (user_id_1) REFERENCES users(user_id) ON DELETE CASCADE
- FOREIGN KEY (user_id_2) REFERENCES users(user_id) ON DELETE CASCADE

### 1.15 私聊消息表 (private_messages)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| session_id | BIGINT | NOT NULL, FOREIGN KEY | 会话 ID（注意：实际为 BIGSERIAL ID，非 VARCHAR） |
| sender_id | TEXT | NOT NULL, FOREIGN KEY | 发送者 ID |
| content | TEXT | NOT NULL | 消息内容 |
| encrypted_content | TEXT | NULLABLE | 加密内容 |
| message_type | TEXT | DEFAULT 'text' | 消息类型 |
| is_read | BOOLEAN | DEFAULT FALSE | 是否已读 |
| read_by_receiver | BOOLEAN | DEFAULT FALSE | 接收者是否已读 |
| created_ts | BIGINT | NOT NULL | 创建时间戳（毫秒） |

**索引**：
- PRIMARY KEY (id)
- INDEX (session_id)
- INDEX (sender_id)
- INDEX (created_ts)
- FOREIGN KEY (session_id) REFERENCES private_sessions(id) ON DELETE CASCADE
- FOREIGN KEY (sender_id) REFERENCES users(user_id) ON DELETE CASCADE

### 1.16 语音消息表 (voice_messages)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| message_id | TEXT | UNIQUE, NOT NULL | 消息 ID |
| user_id | TEXT | NOT NULL, FOREIGN KEY | 用户 ID |
| room_id | TEXT | NULLABLE, FOREIGN KEY | 房间 ID |
| session_id | BIGINT | NULLABLE, FOREIGN KEY | 私聊会话 ID |
| file_path | TEXT | NOT NULL | 文件路径 |
| content_type | TEXT | NOT NULL | 内容类型 |
| duration_ms | INTEGER | NOT NULL | 时长（毫秒） |
| file_size | BIGINT | NOT NULL | 文件大小 |
| waveform_data | TEXT | NULLABLE | 波形数据 |
| transcribe_text | TEXT | NULLABLE | 转录文本 |
| created_ts | BIGINT | NOT NULL | 创建时间戳（毫秒） |

**索引**：
- PRIMARY KEY (id)
- UNIQUE (message_id)
- INDEX (user_id)
- INDEX (room_id)
- INDEX (session_id)
- INDEX (created_ts)
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
- FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
- FOREIGN KEY (session_id) REFERENCES private_sessions(id) ON DELETE SET NULL

### 1.17 语音使用统计表 (voice_usage_stats)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| user_id | TEXT | NOT NULL, FOREIGN KEY | 用户 ID |
| room_id | TEXT | NULLABLE | 房间 ID |
| total_duration_ms | BIGINT | DEFAULT 0 | 总时长（毫秒） |
| total_count | INTEGER | DEFAULT 0 | 总数量 |
| last_used_ts | BIGINT | NULLABLE | 最后使用时间戳（毫秒） |

**索引**：
- PRIMARY KEY (id)
- INDEX (user_id)
- INDEX (room_id)
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE

### 1.18 设备密钥表 (device_keys)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| id | UUID | PRIMARY KEY | UUID 主键 |
| user_id | TEXT | NOT NULL, FOREIGN KEY | 用户 ID |
| device_id | TEXT | NOT NULL, FOREIGN KEY | 设备 ID |
| display_name | TEXT | NULLABLE | 显示名称 |
| algorithm | TEXT | NOT NULL | 加密算法（ed25519, curve25519） |
| key_id | TEXT | NOT NULL | 密钥标识符 |
| public_key | TEXT | NOT NULL | 公钥（Base64 编码） |
| signatures | JSONB | NOT NULL, DEFAULT '{}' | 签名数据 |
| created_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 创建时间 |
| updated_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 更新时间 |

**索引**：
- PRIMARY KEY (id)
- UNIQUE INDEX (user_id, device_id, key_id)
- INDEX (user_id)
- INDEX (device_id)
- INDEX (algorithm)
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
- FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE CASCADE

### 1.19 跨签名密钥表 (cross_signing_keys)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| id | UUID | PRIMARY KEY | UUID 主键 |
| user_id | TEXT | NOT NULL, FOREIGN KEY | 用户 ID |
| key_type | TEXT | NOT NULL | 密钥类型（master, self_signing, user_signing） |
| public_key | TEXT | NOT NULL | 公钥（Base64 编码） |
| usage | TEXT[] | NOT NULL | 密钥用途数组 |
| signatures | JSONB | NOT NULL, DEFAULT '{}' | 签名数据 |
| created_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 创建时间 |
| updated_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 更新时间 |

**索引**：
- PRIMARY KEY (id)
- UNIQUE INDEX (user_id, key_type)
- INDEX (user_id)
- INDEX (key_type)
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE

### 1.20 Megolm 会话表 (megolm_sessions)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| id | UUID | PRIMARY KEY | UUID 主键 |
| session_id | TEXT | NOT NULL, UNIQUE | 会话 ID |
| room_id | TEXT | NOT NULL, FOREIGN KEY | 房间 ID |
| sender_key | TEXT | NOT NULL | 发送方公钥 |
| session_key | TEXT | NOT NULL | 会话密钥（加密存储） |
| algorithm | TEXT | NOT NULL, DEFAULT 'm.megolm.v1.aes-sha2' | 加密算法 |
| message_index | BIGINT | NOT NULL, DEFAULT 0 | 消息索引 |
| created_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 创建时间 |
| last_used_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 最后使用时间 |
| expires_at | TIMESTAMPTZ | NULLABLE | 过期时间 |

**索引**：
- PRIMARY KEY (id)
- UNIQUE INDEX (room_id, sender_key, session_id)
- INDEX (session_id)
- INDEX (room_id)
- INDEX (sender_key)
- INDEX (expires_at)
- FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE

### 1.21 入站 Megolm 会话表 (inbound_megolm_sessions)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| id | UUID | PRIMARY KEY | UUID 主键 |
| session_id | TEXT | NOT NULL, UNIQUE | 会话 ID |
| sender_key | TEXT | NOT NULL | 发送方公钥 |
| room_id | TEXT | NOT NULL | 房间 ID |
| session_key | TEXT | NOT NULL | 会话密钥 |
| algorithm | TEXT | NOT NULL, DEFAULT 'm.megolm.v1.aes-sha2' | 加密算法 |
| message_index | BIGINT | NOT NULL, DEFAULT 0 | 消息索引 |
| created_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 创建时间 |
| expires_at | TIMESTAMPTZ | NULLABLE | 过期时间 |

**索引**：
- PRIMARY KEY (id)
- UNIQUE INDEX (session_id)
- INDEX (sender_key)

### 1.22 密钥备份表 (key_backups)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| id | UUID | PRIMARY KEY | UUID 主键 |
| user_id | TEXT | NOT NULL, FOREIGN KEY | 用户 ID |
| version | TEXT | NOT NULL | 备份版本 |
| algorithm | TEXT | NOT NULL, DEFAULT 'm.megolm_backup.v1' | 加密算法 |
| auth_data | JSONB | NOT NULL | 认证数据 |
| encrypted_data | JSONB | NOT NULL | 加密的备份数据 |
| created_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 创建时间 |
| updated_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 更新时间 |

**索引**：
- PRIMARY KEY (id)
- UNIQUE INDEX (user_id, version)
- INDEX (user_id)
- INDEX (version)
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE

### 1.23 备份密钥表 (backup_keys)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| id | UUID | PRIMARY KEY | UUID 主键 |
| backup_id | UUID | NOT NULL, FOREIGN KEY | 备份 ID |
| room_id | TEXT | NOT NULL | 房间 ID |
| session_id | TEXT | NOT NULL | 会话 ID |
| first_message_index | BIGINT | NOT NULL | 首个消息索引 |
| forwarded_count | BIGINT | NOT NULL, DEFAULT 0 | 转发次数 |
| is_verified | BOOLEAN | NOT NULL, DEFAULT FALSE | 是否已验证 |
| session_data | TEXT | NOT NULL | 会话数据 |
| created_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 创建时间 |

**索引**：
- PRIMARY KEY (id)
- UNIQUE INDEX (backup_id, room_id, session_id)
- INDEX (backup_id)
- FOREIGN KEY (backup_id) REFERENCES key_backups(id) ON DELETE CASCADE

### 1.24 事件签名表 (event_signatures)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| id | UUID | PRIMARY KEY | UUID 主键 |
| event_id | TEXT | NOT NULL | 事件 ID |
| user_id | TEXT | NOT NULL, FOREIGN KEY | 签名用户 ID |
| device_id | TEXT | NOT NULL, FOREIGN KEY | 签名设备 ID |
| signature | TEXT | NOT NULL | 签名数据 |
| key_id | TEXT | NOT NULL | 签名密钥 ID |
| created_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 创建时间 |

**索引**：
- PRIMARY KEY (id)
- UNIQUE INDEX (event_id, user_id, device_id, key_id)
- INDEX (event_id)
- INDEX (user_id)
- INDEX (device_id)
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
- FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE CASCADE

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| id | SERIAL | PRIMARY KEY | 自增 ID |
| ip_address | VARCHAR(255) | UNIQUE, NOT NULL | IP 地址 |
| score | INTEGER | NOT NULL, DEFAULT 0 | 声誉分数 |
| last_seen_at | BIGINT | NOT NULL | 最后见时间戳（毫秒） |
| updated_at | BIGINT | NOT NULL | 更新时间戳（毫秒） |
| details | JSONB | NULLABLE | 详情 |

**索引**：
- PRIMARY KEY (id)
- UNIQUE (ip_address)
- INDEX (score)

### 1.20 设备密钥表 (device_keys)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| user_id | VARCHAR(255) | NOT NULL, FOREIGN KEY | 用户 ID |
| device_id | VARCHAR(255) | NOT NULL, FOREIGN KEY | 设备 ID |
| display_name | VARCHAR(255) | NULLABLE | 显示名称 |
| algorithm | VARCHAR(50) | NOT NULL | 加密算法（ed25519, curve25519） |
| key_id | VARCHAR(255) | NOT NULL | 密钥标识符 |
| public_key | TEXT | NOT NULL | 公钥（Base64 编码） |
| signature | JSONB | NOT NULL | 签名数据 |
| created_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 创建时间 |
| updated_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 更新时间 |

**索引**：
- PRIMARY KEY (id)
- UNIQUE INDEX (user_id, device_id, key_id)
- INDEX (user_id)
- INDEX (device_id)
- INDEX (algorithm)
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
- FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE CASCADE

### 1.21 跨签名密钥表 (cross_signing_keys)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| user_id | VARCHAR(255) | NOT NULL, FOREIGN KEY | 用户 ID |
| key_type | VARCHAR(50) | NOT NULL | 密钥类型（master, self_signing, user_signing） |
| public_key | TEXT | NOT NULL | 公钥（Base64 编码） |
| usage | JSONB | NOT NULL | 密钥用途数组 |
| signatures | JSONB | NOT NULL | 签名数据 |
| created_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 创建时间 |
| updated_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 更新时间 |

**索引**：
- PRIMARY KEY (id)
- UNIQUE INDEX (user_id, key_type)
- INDEX (user_id)
- INDEX (key_type)
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE

### 1.22 Megolm 会话表 (megolm_sessions)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| session_id | VARCHAR(255) | NOT NULL | 会话 ID |
| room_id | VARCHAR(255) | NOT NULL, FOREIGN KEY | 房间 ID |
| sender_key | VARCHAR(255) | NOT NULL | 发送方公钥 |
| session_key | TEXT | NOT NULL | 会话密钥（加密存储） |
| algorithm | VARCHAR(50) | NOT NULL | 加密算法 |
| message_index | BIGINT | NOT NULL, DEFAULT 0 | 消息索引 |
| created_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 创建时间 |
| last_used_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 最后使用时间 |
| expires_at | TIMESTAMPTZ | NULLABLE | 过期时间 |

**索引**：
- PRIMARY KEY (id)
- UNIQUE INDEX (room_id, sender_key, session_id)
- INDEX (session_id)
- INDEX (room_id)
- INDEX (sender_key)
- INDEX (expires_at)
- FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE

### 1.23 密钥备份表 (key_backups)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| user_id | VARCHAR(255) | NOT NULL, FOREIGN KEY | 用户 ID |
| version | VARCHAR(255) | NOT NULL | 备份版本 |
| algorithm | VARCHAR(50) | NOT NULL | 加密算法 |
| auth_data | JSONB | NOT NULL | 认证数据 |
| encrypted_data | JSONB | NOT NULL | 加密的备份数据 |
| created_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 创建时间 |
| updated_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 更新时间 |

**索引**：
- PRIMARY KEY (id)
- UNIQUE INDEX (user_id, version)
- INDEX (user_id)
- INDEX (version)
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE

### 1.24 事件签名表 (event_signatures)

| 字段名 | 类型 | 约束 | 描述 |
|--------|------|--------|------|
| id | BIGSERIAL | PRIMARY KEY | 自增 ID |
| event_id | VARCHAR(255) | NOT NULL | 事件 ID |
| user_id | VARCHAR(255) | NOT NULL, FOREIGN KEY | 签名用户 ID |
| device_id | VARCHAR(255) | NOT NULL, FOREIGN KEY | 签名设备 ID |
| signature | TEXT | NOT NULL | 签名数据 |
| key_id | VARCHAR(255) | NOT NULL | 签名密钥 ID |
| created_at | TIMESTAMPTZ | NOT NULL, DEFAULT NOW() | 创建时间 |

**索引**：
- PRIMARY KEY (id)
- UNIQUE INDEX (event_id, user_id, device_id, key_id)
- INDEX (event_id)
- INDEX (user_id)
- INDEX (device_id)
- FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
- FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE CASCADE

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
    pub status_msg: Option<String>,
    pub presence: String,
    pub last_active_ts: i64,
    pub status_from: Option<String>,
    pub created_ts: i64,
    pub updated_ts: i64,
}
```

### 2.9 Friend 结构体

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Friend {
    pub id: i64,
    pub user_id: String,
    pub friend_id: String,
    pub created_ts: i64,
}
```

### 2.10 FriendRequest 结构体

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FriendRequest {
    pub id: i64,
    pub sender_id: String,
    pub receiver_id: String,
    pub message: Option<String>,
    pub status: String,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
}
```

### 2.11 FriendCategory 结构体

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FriendCategory {
    pub id: i64,
    pub user_id: String,
    pub name: String,
    pub color: String,
    pub created_ts: i64,
}
```

### 2.12 BlockedUser 结构体

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct BlockedUser {
    pub id: i64,
    pub user_id: String,
    pub blocked_id: String,
    pub reason: Option<String>,
    pub created_ts: i64,
}
```

### 2.13 PrivateSession 结构体

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PrivateSession {
    pub id: i64,
    pub user_id_1: String,
    pub user_id_2: String,
    pub last_message: Option<String>,
    pub unread_count: i32,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
}
```

### 2.14 PrivateMessage 结构体

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PrivateMessage {
    pub id: i64,
    pub session_id: i64,
    pub sender_id: String,
    pub content: String,
    pub encrypted_content: Option<String>,
    pub message_type: String,
    pub is_read: bool,
    pub read_by_receiver: bool,
    pub created_ts: i64,
}
```

### 2.15 DeviceKey 结构体

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DeviceKey {
    pub id: Uuid,
    pub user_id: String,
    pub device_id: String,
    pub display_name: Option<String>,
    pub algorithm: String,
    pub key_id: String,
    pub public_key: String,
    pub signatures: Json,
    pub created_at: ChronoDateTime<Utc>,
    pub updated_at: ChronoDateTime<Utc>,
}
```

### 2.16 CrossSigningKey 结构体

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CrossSigningKey {
    pub id: Uuid,
    pub user_id: String,
    pub key_type: String,
    pub public_key: String,
    pub usage: Vec<String>,
    pub signatures: Json,
    pub created_at: ChronoDateTime<Utc>,
    pub updated_at: ChronoDateTime<Utc>,
}
```

### 2.17 MegolmSession 结构体

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MegolmSession {
    pub id: Uuid,
    pub session_id: String,
    pub room_id: String,
    pub sender_key: String,
    pub session_key: String,
    pub algorithm: String,
    pub message_index: i64,
    pub created_at: ChronoDateTime<Utc>,
    pub last_used_at: ChronoDateTime<Utc>,
    pub expires_at: Option<ChronoDateTime<Utc>>,
}
```

### 2.18 InboundMegolmSession 结构体

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct InboundMegolmSession {
    pub id: Uuid,
    pub session_id: String,
    pub sender_key: String,
    pub room_id: String,
    pub session_key: String,
    pub algorithm: String,
    pub message_index: i64,
    pub created_at: ChronoDateTime<Utc>,
    pub expires_at: Option<ChronoDateTime<Utc>>,
}
```

### 2.19 KeyBackup 结构体

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct KeyBackup {
    pub id: Uuid,
    pub user_id: String,
    pub version: String,
    pub algorithm: String,
    pub auth_data: Json,
    pub encrypted_data: Json,
    pub created_at: ChronoDateTime<Utc>,
    pub updated_at: ChronoDateTime<Utc>,
}
```

### 2.20 BackupKey 结构体

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct BackupKey {
    pub id: Uuid,
    pub backup_id: Uuid,
    pub room_id: String,
    pub session_id: String,
    pub first_message_index: i64,
    pub forwarded_count: i64,
    pub is_verified: bool,
    pub session_data: String,
    pub created_at: ChronoDateTime<Utc>,
}
```

### 2.21 EventSignature 结构体

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EventSignature {
    pub id: Uuid,
    pub event_id: String,
    pub user_id: String,
    pub device_id: String,
    pub signature: String,
    pub key_id: String,
    pub created_at: ChronoDateTime<Utc>,
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
users (1) ─────── (N) friends
users (1) ─────── (N) friend_requests (as sender_id)
users (1) ─────── (N) friend_requests (as receiver_id)
users (1) ─────── (N) friend_categories
users (1) ─────── (N) blocked_users
users (1) ─────── (N) private_sessions (as user_id_1)
users (1) ─────── (N) private_sessions (as user_id_2)
users (1) ─────── (N) private_messages
users (1) ─────── (N) device_keys
users (1) ─────── (N) cross_signing_keys
users (1) ─────── (N) key_backups
users (1) ─────── (N) event_signatures
users (1) ─────── (N) megolm_sessions
users (1) ─────── (N) backup_keys
```

### 3.2 设备关系

```
devices (1) ─────── (N) device_keys
devices (1) ─────── (N) event_signatures
```

### 3.3 房间关系

```
rooms (1) ─────── (N) room_events
rooms (1) ─────── (N) room_memberships
rooms (1) ─────── (N) megolm_sessions
rooms (1) ─────── (N) backup_keys
```

### 3.4 事件关系

```
room_events (N) ─────── (1) rooms
room_events (N) ─────── (1) users
```

### 3.5 密钥备份关系

```
key_backups (1) ─────── (N) backup_keys
```
events (N) ─────── (1) users
```

### 3.4 私聊关系

```
private_sessions (1) ─────── (N) private_messages
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
- `device_keys(user_id, device_id, key_id)`：设备密钥唯一索引
- `cross_signing_keys(user_id, key_type)`：跨签名密钥唯一索引
- `megolm_sessions(room_id, sender_key, session_id)`：Megolm 会话唯一索引
- `inbound_megolm_sessions(session_id)`：入站 Megolm 会话唯一索引
- `key_backups(user_id, version)`：密钥备份唯一索引
- `backup_keys(backup_id, room_id, session_id)`：备份密钥唯一索引
- `event_signatures(event_id, user_id, device_id, key_id)`：事件签名唯一索引

### 4.4 复合索引

- `room_memberships(room_id, user_id)`：房间成员复合索引
- `room_memberships(user_id, room_id)`：用户房间复合索引
- `friends(user_id, friend_id)`：好友关系复合索引
- `friend_categories(user_id, name)`：好友分类复合索引
- `blocked_users(user_id, blocked_id)`：黑名单复合索引
- `private_messages(session_id, created_ts)`：私聊消息时间复合索引

### 4.5 查询优化索引

- `room_events.origin_server_ts`：事件时间戳索引，用于时间范围查询
- `room_events.event_type`：事件类型索引，用于类型过滤
- `room_events.room_id`：房间 ID 索引，用于房间事件查询
- `room_events.user_id`：用户 ID 索引，用于用户事件查询
- `private_messages.created_ts`：创建时间索引，用于时间排序
- `megolm_sessions.expires_at`：过期时间索引，用于过期清理
- `inbound_megolm_sessions.expires_at`：过期时间索引，用于过期清理

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
| 1.2.0 | 2026-01-29 | 优化文档结构，移除不存在的表（session_keys, voice_messages, voice_usage_stats, security_events, ip_blocks, ip_reputation）；更新字段类型从 VARCHAR 到 TEXT；更新 Rust 结构体定义以匹配实际代码；更新关系映射和索引策略 |
| 1.1.0 | 2026-01-28 | 添加 E2EE 相关数据表，包括设备密钥表、跨签名密钥表、Megolm 会话表、密钥备份表和事件签名表 |
| 1.0.0 | 2026-01-28 | 初始版本，定义数据模型文档 |
