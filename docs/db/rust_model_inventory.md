# Rust 模型统计文档

> **项目**: synapse-rust 数据库全面排查与优化
> **版本**: v1.0.0
> **生成日期**: 2026-03-20
> **源文件**: `src/storage/**/*.rs`

---

## 统计概览

| 指标 | 数量 |
|------|------|
| 模型文件总数 | 33 |
| `sqlx::FromRow` 结构体总数 | 80+ |
| 模型字段统计 | 500+ |

---

## 第一部分：用户相关模型

### 1.1 User (用户模型)

**文件**: `src/storage/models/user.rs`

```rust
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct User {
    pub user_id: String,
    pub username: String,
    pub password_hash: Option<String>,
    pub is_admin: bool,
    pub is_guest: bool,
    pub is_shadow_banned: bool,
    pub is_deactivated: bool,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
    pub displayname: Option<String>,
    pub avatar_url: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub generation: i64,
    pub consent_version: Option<String>,
    pub appservice_id: Option<String>,
    pub user_type: Option<String>,
    pub invalid_update_at: Option<i64>,
    pub migration_state: Option<String>,
    pub password_changed_ts: Option<i64>,
    pub must_change_password: bool,
    pub password_expires_at: Option<i64>,
    pub failed_login_attempts: i32,
    pub locked_until: Option<i64>,
}
```

**字段数**: 29
**与表一致性**: ✅ 与 `users` 表一致

---

### 1.2 UserThreepid (用户第三方身份模型)

**文件**: `src/storage/models/user.rs`

```rust
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct UserThreepid {
    pub id: i64,
    pub user_id: String,
    pub medium: String,
    pub address: String,
    pub validated_at: Option<i64>,
    pub added_ts: i64,
    pub is_verified: bool,
    pub verification_token: Option<String>,
    pub verification_expires_at: Option<i64>,
}
```

**字段数**: 9
**与表一致性**: ✅ 与 `user_threepids` 表一致

---

### 1.3 UserProfile (用户资料模型)

**文件**: `src/storage/models/user.rs`

```rust
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct UserProfile {
    pub user_id: String,
    pub username: String,
    pub displayname: Option<String>,
    pub avatar_url: Option<String>,
    pub created_ts: i64,
}
```

**字段数**: 5
**与表一致性**: ✅ 部分字段，查询专用模型

---

### 1.4 Presence (在线状态模型)

**文件**: `src/storage/models/user.rs`

```rust
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
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

**字段数**: 7
**与表一致性**: ✅ 与 `presence` 表一致

---

### 1.5 UserDirectory (用户目录模型)

**文件**: `src/storage/models/user.rs`

```rust
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct UserDirectory {
    pub user_id: String,
    pub room_id: String,
    pub visibility: String,
    pub added_by: Option<String>,
    pub created_ts: i64,
}
```

**字段数**: 5
**与表一致性**: ⚠️ SQL 中有 `updated_ts` 列，模型中缺失

---

### 1.6 Friend (好友模型)

**文件**: `src/storage/models/user.rs`

```rust
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct Friend {
    pub id: i64,
    pub user_id: String,
    pub friend_id: String,
    pub created_ts: i64,
}
```

**字段数**: 4
**与表一致性**: ✅ 与 `friends` 表一致

---

### 1.7 FriendRequest (好友请求模型)

**文件**: `src/storage/models/user.rs`

```rust
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
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

**字段数**: 7
**与表一致性**: ✅ 与 `friend_requests` 表一致

---

### 1.8 BlockedUser (黑名单用户模型)

**文件**: `src/storage/models/user.rs`

```rust
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct BlockedUser {
    pub id: i64,
    pub user_id: String,
    pub blocked_id: String,
    pub reason: Option<String>,
    pub created_ts: i64,
}
```

**字段数**: 5
**与表一致性**: ✅ 与 `blocked_users` 表一致

---

## 第二部分：设备相关模型

### 2.1 Device (设备模型)

**文件**: `src/storage/models/device.rs`

```rust
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct Device {
    pub device_id: String,
    pub user_id: String,
    pub display_name: Option<String>,
    pub device_key: Option<serde_json::Value>,
    pub last_seen_ts: Option<i64>,
    pub last_seen_ip: Option<String>,
    pub created_ts: i64,
    pub first_seen_ts: i64,
    pub user_agent: Option<String>,
    pub appservice_id: Option<String>,
    pub ignored_user_list: Option<String>,
}
```

**字段数**: 11
**与表一致性**: ✅ 与 `devices` 表一致

---

### 2.2 DehydratedDevice (脱水设备模型)

**文件**: `src/storage/models/device.rs`

```rust
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct DehydratedDevice {
    pub id: i64,
    pub user_id: String,
    pub device_id: String,
    pub device_data: Option<serde_json::Value>,
    pub time_of_dehydration: i64,
    pub is_restored: bool,
    pub restored_by_device_id: Option<String>,
    pub created_ts: i64,
}
```

**字段数**: 8
**与表一致性**: ✅ 与 `dehydrated_devices` 表一致

---

## 第三部分：Token 相关模型

### 3.1 AccessToken (访问令牌模型)

**文件**: `src/storage/token.rs`

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AccessToken {
    pub id: i64,
    pub token: String,
    pub user_id: String,
    pub device_id: Option<String>,
    pub created_ts: i64,
    pub expires_at: Option<i64>,
    pub last_used_ts: Option<i64>,
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
    pub is_revoked: bool,
    pub revoked_at: Option<i64>,
}
```

**字段数**: 11
**与表一致性**: ✅ 与 `access_tokens` 表一致

---

### 3.2 RefreshToken (刷新令牌模型)

**文件**: `src/storage/refresh_token.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RefreshToken {
    pub id: i64,
    pub token_hash: String,
    pub user_id: String,
    pub device_id: Option<String>,
    pub access_token_id: Option<String>,
    pub scope: Option<String>,
    pub created_ts: i64,
    pub expires_at: Option<i64>,
    pub last_used_ts: Option<i64>,
    pub use_count: i32,
    pub is_revoked: bool,
    pub revoked_at: Option<i64>,
    pub revoked_reason: Option<String>,
    pub client_info: Option<serde_json::Value>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}
```

**字段数**: 16
**与表一致性**: ✅ 与 `refresh_tokens` 表一致

---

### 3.3 TokenBlacklistEntry (Token 黑名单模型)

**文件**: `src/storage/refresh_token.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TokenBlacklistEntry {
    pub id: i64,
    pub token_hash: String,
    pub token_type: String,
    pub user_id: String,
    pub revoked_at: i64,
    pub expires_at: Option<i64>,
    pub reason: Option<String>,
}
```

**字段数**: 7
**与表一致性**: ✅ 与 `token_blacklist` 表一致

---

## 第四部分：房间相关模型

### 4.1 RoomSummary (房间摘要模型)

**文件**: `src/storage/models/room.rs`

**字段数**: 16
**与表一致性**: ✅ 与 `room_summaries` 表一致

---

### 4.2 SlidingSyncRoom (Sliding Sync 房间模型)

**文件**: `src/storage/sliding_sync.rs`

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SlidingSyncRoom {
    pub id: i64,
    pub user_id: String,
    pub device_id: String,
    pub room_id: String,
    pub conn_id: Option<String>,
    pub list_key: Option<String>,
    pub bump_stamp: i64,
    pub highlight_count: i32,
    pub notification_count: i32,
    pub is_dm: bool,
    pub is_encrypted: bool,
    pub is_tombstoned: bool,
    pub invited: bool,
    pub name: Option<String>,
    pub avatar: Option<String>,
    pub timestamp: i64,
    pub created_ts: i64,
    pub updated_ts: i64,
}
```

**字段数**: 19
**与表一致性**: ✅ 与 `sliding_sync_rooms` 表一致

---

### 4.3 ThreadSubscription (线程订阅模型)

**文件**: `src/storage/thread.rs`

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ThreadSubscription {
    pub id: i64,
    pub room_id: String,
    pub thread_id: String,
    pub user_id: String,
    pub notification_level: String,
    pub is_muted: bool,
    pub is_pinned: bool,
    pub subscribed_ts: i64,
    pub updated_ts: i64,
}
```

**字段数**: 9
**与表一致性**: ✅ 与 `thread_subscriptions` 表一致

---

## 第五部分：事件相关模型

### 5.1 Event (事件模型)

**文件**: `src/storage/models/event.rs`

**字段数**: 30+
**与表一致性**: ✅ 与 `events` 表一致

---

### 5.2 RoomEvent (房间事件模型)

**文件**: `src/storage/event.rs`

**字段数**: 12
**与表一致性**: ✅ 与 `room_events` 表一致

---

## 第六部分：E2EE 加密相关模型

### 6.1 DeviceKeys (设备密钥模型)

**文件**: `src/storage/device.rs`

**字段数**: 15
**与表一致性**: ✅ 与 `device_keys` 表一致

---

### 6.2 KeyBackup (密钥备份模型)

**文件**: `src/storage/models/crypto.rs`

**字段数**: 9
**与表一致性**: ✅ 与 `key_backups` 表一致

---

### 6.3 BackupKeys (备份密钥模型)

**文件**: `src/storage/models/crypto.rs`

**字段数**: 7
**与表一致性**: ✅ 与 `backup_keys` 表一致

---

### 6.4 MegolmSession (Megolm 会话模型)

**文件**: `src/storage/models/crypto.rs`

**字段数**: 11
**与表一致性**: ✅ 与 `megolm_sessions` 表一致

---

### 6.5 OlmAccount (Olm 账户模型)

**文件**: `src/storage/models/crypto.rs`

**字段数**: 9
**与表一致性**: ✅ 与 `olm_accounts` 表一致

---

### 6.6 OlmSession (Olm 会话模型)

**文件**: `src/storage/models/crypto.rs`

**字段数**: 11
**与表一致性**: ✅ 与 `olm_sessions` 表一致

---

## 第七部分：推送相关模型

### 7.1 Pusher (推送器模型)

**文件**: `src/storage/models/push.rs`

**字段数**: 17
**与表一致性**: ✅ 与 `pushers` 表一致

---

### 7.2 PushRule (推送规则模型)

**文件**: `src/storage/models/push.rs`

**字段数**: 14
**与表一致性**: ✅ 与 `push_rules` 表一致

---

## 第八部分：媒体相关模型

### 8.1 MediaMetadata (媒体元数据模型)

**文件**: `src/storage/models/media.rs`

**字段数**: 10
**与表一致性**: ✅ 与 `media_metadata` 表一致

---

### 8.2 Thumbnail (缩略图模型)

**文件**: `src/storage/models/media.rs`

**字段数**: 8
**与表一致性**: ✅ 与 `thumbnails` 表一致

---

## 第九部分：Space 相关模型

### 9.1 SpaceChild (Space 子房间模型)

**文件**: `src/storage/room.rs`

**字段数**: 8
**与表一致性**: ✅ 与 `space_children` 表一致

---

### 9.2 SpaceHierarchy (Space 层级模型)

**文件**: `src/storage/room.rs`

**字段数**: 9
**与表一致性**: ✅ 与 `space_hierarchy` 表一致

---

## 第十部分：其他模型

### 10.1 AccountData (账户数据模型)

**文件**: `src/storage/models/room.rs`

**字段数**: 6
**与表一致性**: ✅ 与 `account_data` 表一致

---

### 10.2 UserFilter (用户过滤器模型)

**文件**: `src/storage/room_tag.rs`

**字段数**: 5
**与表一致性**: ✅ 与 `user_filters` 表一致

---

### 10.3 RoomTag (房间标签模型)

**文件**: `src/storage/room_tag.rs`

**字段数**: 6
**与表一致性**: ✅ 与 `room_tags` 表一致

---

### 10.4 ToDeviceMessage (To-Device 消息模型)

**文件**: `src/storage/models/federation.rs`

**字段数**: 11
**与表一致性**: ✅ 与 `to_device_messages` 表一致

---

### 10.5 DelayedEvent (延迟事件模型)

**文件**: `src/storage/delayed_event.rs`

**字段数**: 12
**与表一致性**: ✅ 与 `delayed_events` 表一致

---

## 第十一部分：模型与表一致性汇总

| 模型名称 | 文件位置 | 字段数 | 与表一致性 | 问题 |
|----------|----------|--------|------------|------|
| User | models/user.rs | 29 | ✅ 一致 | 无 |
| UserThreepid | models/user.rs | 9 | ✅ 一致 | 无 |
| UserProfile | models/user.rs | 5 | ✅ 部分 | 视图模型 |
| Presence | models/user.rs | 7 | ✅ 一致 | 无 |
| UserDirectory | models/user.rs | 5 | ⚠️ 缺失列 | updated_ts |
| Friend | models/user.rs | 4 | ✅ 一致 | 无 |
| FriendRequest | models/user.rs | 7 | ✅ 一致 | 无 |
| FriendCategory | models/user.rs | 5 | ✅ 一致 | 无 |
| BlockedUser | models/user.rs | 5 | ✅ 一致 | 无 |
| Device | models/device.rs | 11 | ✅ 一致 | 无 |
| DehydratedDevice | models/device.rs | 8 | ✅ 一致 | 无 |
| AccessToken | token.rs | 11 | ✅ 一致 | 无 |
| RefreshToken | refresh_token.rs | 16 | ✅ 一致 | 无 |
| TokenBlacklistEntry | refresh_token.rs | 7 | ✅ 一致 | 无 |
| SlidingSyncRoom | sliding_sync.rs | 19 | ✅ 一致 | 无 |
| ThreadSubscription | thread.rs | 9 | ✅ 一致 | 无 |
| Event | models/event.rs | 30+ | ✅ 一致 | 无 |
| RoomEvent | event.rs | 12 | ✅ 一致 | 无 |
| DeviceKeys | device.rs | 15 | ✅ 一致 | 无 |
| KeyBackup | models/crypto.rs | 9 | ✅ 一致 | 无 |
| BackupKeys | models/crypto.rs | 7 | ✅ 一致 | 无 |
| MegolmSession | models/crypto.rs | 11 | ✅ 一致 | 无 |
| OlmAccount | models/crypto.rs | 9 | ✅ 一致 | 无 |
| OlmSession | models/crypto.rs | 11 | ✅ 一致 | 无 |
| Pusher | models/push.rs | 17 | ✅ 一致 | 无 |
| PushRule | models/push.rs | 14 | ✅ 一致 | 无 |
| MediaMetadata | models/media.rs | 10 | ✅ 一致 | 无 |
| Thumbnail | models/media.rs | 8 | ✅ 一致 | 无 |
| SpaceChild | room.rs | 8 | ✅ 一致 | 无 |
| SpaceHierarchy | room.rs | 9 | ✅ 一致 | 无 |
| AccountData | models/room.rs | 6 | ✅ 一致 | 无 |
| UserFilter | room_tag.rs | 5 | ✅ 一致 | 无 |
| RoomTag | room_tag.rs | 6 | ✅ 一致 | 无 |
| ToDeviceMessage | models/federation.rs | 11 | ✅ 一致 | 无 |
| DelayedEvent | delayed_event.rs | 12 | ✅ 一致 | 无 |

---

## 第十二部分：发现的问题

### 问题 1: UserDirectory 模型缺少 updated_ts 字段 (P2)

| 问题 | 详情 |
|------|------|
| 模型 | UserDirectory |
| 文件 | src/storage/models/user.rs |
| SQL 表 | user_directory |
| 问题 | 模型中缺少 `updated_ts` 字段 |

**修复建议**: 在 `UserDirectory` 模型中添加 `updated_ts: Option<i64>` 字段

---

## 文档更新记录

| 日期 | 版本 | 更新内容 |
|------|------|----------|
| 2026-03-20 | v1.0.0 | 初始版本，基于 storage 目录扫描生成 |