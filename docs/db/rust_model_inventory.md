# Rust 模型清单报告

> **项目**: synapse-rust 数据库全面排查
> **版本**: v1.0.0
> **生成日期**: 2026-03-20
> **数据来源**: src/storage/models/*.rs

---

## 一、执行摘要

| 指标 | 值 |
|------|-----|
| 模型文件数量 | 7 |
| 模型结构体数量 | 51 |
| 对应 SQL 表数量 | 50+ |

---

## 二、模型文件清单

### 2.1 user.rs (11 个模型)

| 序号 | 模型名 | 行号 | 对应表 |
|------|--------|------|--------|
| 1 | User | 3 | users |
| 2 | UserDirectory | 37 | user_directory |
| 3 | UserDirectoryProfile | 50 | user_directory_profiles |
| 4 | UserPrivacySettings | 59 | user_privacy_settings |
| 5 | UserFilter | 68 | user_filters |
| 6 | UserStats | 79 | user_stats |
| 7 | UserExternalId | 87 | user_external_ids |
| 8 | UserThreepid | 98 | user_threepids |
| 9 | AccountData | 107 | account_data |
| 10 | UserMediaQuota | 116 | user_media_quota |
| 11 | Presence | 127 | presence |

### 2.2 token.rs (7 个模型)

| 序号 | 模型名 | 行号 | 对应表 |
|------|--------|------|--------|
| 12 | AccessToken | 3 | access_tokens |
| 13 | RefreshToken | 18 | refresh_tokens |
| 14 | TokenBlacklist | 38 | token_blacklist |
| 15 | PasswordResetToken | 50 | password_reset_tokens |
| 16 | LoginToken | 61 | login_tokens |
| 17 | RegistrationToken | 74 | registration_tokens |
| 18 | OpenIdToken | 84 | openid_tokens |

### 2.3 crypto.rs (7 个模型)

| 序号 | 模型名 | 行号 | 对应表 |
|------|--------|------|--------|
| 19 | DeviceKey | 3 | device_keys |
| 20 | DeviceSignature | 21 | device_signatures |
| 21 | CrossSigningKey | 31 | cross_signing_keys |
| 22 | CrossSigningTrust | 45 | cross_signing_trust |
| 23 | KeyBackup | 57 | key_backups |
| 24 | SecureKeyBackup | 69 | secure_key_backups |
| 25 | DeviceListChange | 81 | device_lists_changes |

### 2.4 room.rs (13 个模型)

| 序号 | 模型名 | 行号 | 对应表 |
|------|--------|------|--------|
| 26 | Room | 3 | rooms |
| 27 | RoomAlias | 23 | room_aliases |
| 28 | RoomDepth | 39 | room_depth |
| 29 | RoomStateEvent | 49 | room_state_events |
| 30 | RoomAccountData | 57 | room_account_data |
| 31 | RoomTag | 74 | room_tags |
| 32 | RoomSummary | 88 | room_summaries |
| 33 | RoomSummaryMember | 99 | room_summary_members |
| 34 | RoomParent | 110 | room_parents |
| 35 | RoomEphemeral | 122 | room_ephemeral |
| 36 | SlidingSyncRoom | 132 | sliding_sync_rooms |
| 37 | SpaceChild | 143 | space_children |
| 38 | SpaceHierarchy | 143+ | space_hierarchy |

### 2.5 push.rs (7 个模型)

| 序号 | 模型名 | 行号 | 对应表 |
|------|--------|------|--------|
| 39 | Pusher | 3 | pushers |
| 40 | PushRule | 21 | push_rules |
| 41 | PushDevice | 39 | push_devices |
| 42 | Notification | 58 | notifications |
| 43 | PushConfig | 72 | push_config |
| 44 | PushNotificationQueue | 85 | push_notification_queue |
| 45 | NotificationLog | 96 | push_notification_log |

### 2.6 membership.rs (3 个模型)

| 序号 | 模型名 | 行号 | 对应表 |
|------|--------|------|--------|
| 46 | RoomMembership | 3 | room_memberships |
| 47 | RoomInvite | 27 | room_invites |
| 48 | EventReceipt | 41 | event_receipts |

### 2.7 federation.rs (3 个模型)

| 序号 | 模型名 | 行号 | 对应表 |
|------|--------|------|--------|
| 49 | FederationSigningKey | 3 | federation_signing_keys |
| 50 | FederationServer | 15 | federation_servers |
| 51 | DeviceListStream | 25 | device_lists_stream |

---

## 三、模型字段类型映射

### 3.1 常用类型映射表

| PostgreSQL 类型 | Rust 类型 | 说明 |
|-----------------|-----------|------|
| BIGINT NOT NULL | i64 | 非空整数 |
| BIGINT | Option\<i64\> | 可空整数 |
| VARCHAR(n) NOT NULL | String | 非空字符串 |
| VARCHAR(n) | Option\<String\> | 可空字符串 |
| TEXT NOT NULL | String | 非空文本 |
| TEXT | Option\<String\> | 可空文本 |
| BOOLEAN NOT NULL | bool | 非空布尔 |
| BOOLEAN | Option\<bool\> | 可空布尔 |
| JSONB | serde_json::Value | JSON 数据 |
| TIMESTAMPTZ | chrono::DateTime\<chrono::Utc\> | 时间戳 |
| BIGSERIAL | i64 | 自增主键 |

### 3.2 时间戳字段规范

| 后缀 | 用途 | 类型 | 示例 |
|------|------|------|------|
| `_ts` | 创建/更新时间 | BIGINT | created_ts, updated_ts |
| `_at` | 过期/撤销时间 | BIGINT | expires_at, revoked_at |

---

## 四、模型派生宏

所有模型都使用以下派生宏：

```rust
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
```

| 派生宏 | 用途 |
|--------|------|
| Debug | 调试输出 |
| Clone | 深拷贝 |
| sqlx::FromRow | SQLx 行映射 |
| Serialize | JSON 序列化 |
| Deserialize | JSON 反序列化 |

---

## 五、文档更新记录

| 日期 | 版本 | 更新内容 |
|------|------|----------|
| 2026-03-20 | v1.0.0 | 初始版本，基于 storage/models 目录扫描生成 |
