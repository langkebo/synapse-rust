# HuLa Matrix 数据库设计文档

> 版本: 2.0 (更新版)
> 生成日期: 2026-03-26
> 项目: synapse-rust

---

## 1. 数据库概述

### 1.1 系统架构

本数据库用于支撑 HuLa Matrix 聊天系统的核心功能，包括：
- 用户认证与会话管理
- 房间与消息存储
- 端到端加密
- 好友与社交功能
- 媒体文件管理
- 实时推送
- Worker 分布式处理
- 联邦通信

### 1.2 技术规格

根据 docs/db/ 下的审计报告，数据库规格如下：

| 项目 | 规格 | 状态 |
|------|------|------|
| 数据库类型 | PostgreSQL | ✅ |
| 表数量 (Schema) | 137 | ✅ |
| 表数量 (含迁移) | 154 | ✅ |
| Rust 动态创建表 | 21 | ✅ |
| 索引数量 | 478+ | ✅ |
| 外键数量 | 35+ | ✅ |
| 主键数量 | 131 | ✅ |
| 数据库大小 | ~17 MB | ✅ |
| 字符集 | UTF-8 | ✅ |
| 时区 | UTC | ✅ |

### 1.3 部署状态

```
PostgreSQL: ✅ 运行中 (端口 5432)
Redis: ✅ 运行中 (端口 6379)
Rust Server: ✅ 运行中 (28008, 28448)
```

---

## 2. 实体关系图 (ERD)

### 2.1 核心实体

根据 sql_table_inventory.md 和 rust_table_inventory.md，核心实体包括：

| 实体 | 表名 | 功能描述 |
|------|------|----------|
| 用户 | users | 用户核心表，存储用户基本信息 |
| 房间 | rooms | 房间表，存储房间元数据 |
| 事件 | events | 事件表，存储消息和状态事件 |
| 设备 | devices | 设备表，存储用户设备信息 |
| 会话 | access_tokens | 访问令牌表，管理用户会话 |
| 房间成员 | room_memberships | 房间成员关系表 |
| 房间状态 | room_state_events | 房间状态事件表 |
| 设备密钥 | device_keys | 设备加密密钥表 |
| 媒体 | media_metadata | 媒体文件元数据表 |

### 2.2 ER 图关系说明

```
users ────── user_account_data
   │
   ├── devices ───── device_keys ───── device_signatures
   │       │
   │       └── device_lists_changes ─── device_lists_stream
   │
   ├── access_tokens ─── refresh_tokens ─── token_blacklist
   │
   └── room_memberships ─── rooms ─── events
        │                         │
        │                         ├── room_state_events
        │                         ├── room_account_data
        │                         ├── room_aliases
        │                         ├── room_tags
        │                         ├── read_markers
        │                         └── event_receipts
        │
        └── room_invites
```

---

## 3. 表结构详细说明

根据 DATABASE_AUDIT_REPORT.md，以下是完整表分类：


### 3.1 用户管理 (12表)

**users**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| user_id | TEXT |  | ✅ | - |
| username | TEXT |  | ✅ | - |
| password_hash | TEXT |  | - | - |
| is_admin | BOOLEAN |  | - | FALSE |
| is_guest | BOOLEAN |  | - | FALSE |
| is_shadow_banned | BOOLEAN |  | - | FALSE |
| is_deactivated | BOOLEAN |  | - | FALSE |
| created_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | - | - |
| displayname | TEXT |  | - | - |
| avatar_url | TEXT |  | - | - |
| email | TEXT |  | - | - |
| phone | TEXT |  | - | - |
| generation | BIGINT |  | - | 0 |
| consent_version | TEXT |  | - | - |
| appservice_id | TEXT |  | - | - |
| user_type | TEXT |  | - | - |
| invalid_update_at | BIGINT |  | - | - |
| migration_state | TEXT |  | - | - |
| password_changed_ts | BIGINT |  | - | - |
| is_password_change_required | BOOLEAN |  | - | FALSE |
| must_change_password | BOOLEAN |  | - | FALSE |
| password_expires_at | BIGINT |  | - | - |
| failed_login_attempts | INTEGER |  | - | 0 |
| locked_until | BIGINT |  | - | - |
| CONSTRAINT | pk_users | ✅ | - | - |
| CONSTRAINT | uq_users_username |  | - | - |

**user_account_data**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| user_id | TEXT |  | ✅ | - |
| event_type | TEXT |  | ✅ | - |
| content | TEXT |  | ✅ | - |
| created_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_user_account_data | ✅ | - | - |
| CONSTRAINT | uq_user_account_data_user_type |  | - | - |

**user_directory**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| user_id | TEXT |  | ✅ | - |
| room_id | TEXT |  | ✅ | - |
| visibility | TEXT |  | ✅ | 'private' |
| added_by | TEXT |  | - | - |
| created_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_user_directory | ✅ | - | - |
| CONSTRAINT | fk_user_directory_user |  | - | - |
| CONSTRAINT | fk_user_directory_room |  | - | - |

**user_filters**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | SERIAL | ✅ | - | - |
| user_id | VARCHAR(255) |  | ✅ | - |
| filter_id | VARCHAR(255) |  | ✅ | - |
| filter_json | JSONB |  | ✅ | '{}' |
| created_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | uq_user_filters_user_filter |  | - | - |

**user_threepids**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| user_id | TEXT |  | ✅ | - |
| medium | TEXT |  | ✅ | - |
| address | TEXT |  | ✅ | - |
| validated_ts | BIGINT |  | - | - |
| added_ts | BIGINT |  | ✅ | - |
| is_verified | BOOLEAN |  | - | FALSE |
| verification_token | TEXT |  | - | - |
| verification_expires_ts | BIGINT |  | - | - |
| CONSTRAINT | pk_user_threepids | ✅ | - | - |
| CONSTRAINT | uq_user_threepids_medium_address |  | - | - |
| CONSTRAINT | fk_user_threepids_user |  | - | - |

**user_privacy_settings**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| user_id | VARCHAR(255) | ✅ | - | - |
| allow_presence_lookup | BOOLEAN |  | - | TRUE |
| allow_profile_lookup | BOOLEAN |  | - | TRUE |
| allow_room_invites | BOOLEAN |  | - | TRUE |
| created_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | - | - |

**user_media_quota**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| user_id | TEXT |  | ✅ | - |
| max_bytes | BIGINT |  | - | 1073741824 |
| used_bytes | BIGINT |  | - | 0 |
| file_count | INTEGER |  | - | 0 |
| created_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | - | - |
| CONSTRAINT | pk_user_media_quota | ✅ | - | - |
| CONSTRAINT | uq_user_media_quota_user |  | - | - |
| CONSTRAINT | fk_user_media_quota_user |  | - | - |

**password_history**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL | ✅ | - | - |
| user_id | TEXT |  | ✅ | - |
| password_hash | TEXT |  | ✅ | - |
| created_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | fk_password_history_user |  | - | - |

**password_policy**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | SERIAL | ✅ | - | - |
| name | VARCHAR(100) |  | ✅ | - |
| value | TEXT |  | ✅ | - |
| description | TEXT |  | - | - |
| updated_ts | BIGINT |  | ✅ | - |

**account_validity**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| user_id | TEXT |  | ✅ | - |
| is_valid | BOOLEAN |  | - | TRUE |
| last_check_at | BIGINT |  | - | - |
| expiration_at | BIGINT |  | - | - |
| renewal_token | TEXT |  | - | - |
| created_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | - | - |
| CONSTRAINT | pk_account_validity | ✅ | - | - |
| CONSTRAINT | uq_account_validity_user |  | - | - |

**presence**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| user_id | TEXT |  | ✅ | - |
| status_msg | TEXT |  | - | - |
| presence | TEXT |  | ✅ | 'offline' |
| last_active_ts | BIGINT |  | ✅ | 0 |
| status_from | TEXT |  | - | - |
| created_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_presence | ✅ | - | - |
| CONSTRAINT | fk_presence_user |  | - | - |

**presence_routes**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| route_name | TEXT |  | ✅ | - |
| route_type | TEXT |  | ✅ | - |
| is_enabled | BOOLEAN |  | - | TRUE |
| config | JSONB |  | - | '{}' |
| created_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_presence_routes | ✅ | - | - |
| CONSTRAINT | uq_presence_routes_name |  | - | - |


### 3.2 设备管理 (5表)

**devices**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| device_id | TEXT |  | ✅ | - |
| user_id | TEXT |  | ✅ | - |
| display_name | TEXT |  | - | - |
| device_key | JSONB |  | - | - |
| last_seen_ts | BIGINT |  | - | - |
| last_seen_ip | TEXT |  | - | - |
| created_ts | BIGINT |  | ✅ | - |
| first_seen_ts | BIGINT |  | ✅ | - |
| user_agent | TEXT |  | - | - |
| appservice_id | TEXT |  | - | - |
| ignored_user_list | TEXT |  | - | - |
| CONSTRAINT | pk_devices | ✅ | - | - |
| CONSTRAINT | fk_devices_user |  | - | - |

**device_keys**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| user_id | TEXT |  | ✅ | - |
| device_id | TEXT |  | ✅ | - |
| algorithm | TEXT |  | ✅ | - |
| key_id | TEXT |  | ✅ | - |
| public_key | TEXT |  | ✅ | - |
| key_data | TEXT |  | - | - |
| signatures | JSONB |  | - | - |
| added_ts | BIGINT |  | ✅ | - |
| created_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | - | - |
| ts_updated_ms | BIGINT |  | - | - |
| is_verified | BOOLEAN |  | - | FALSE |
| is_blocked | BOOLEAN |  | - | FALSE |
| display_name | TEXT |  | - | - |
| CONSTRAINT | pk_device_keys | ✅ | - | - |
| CONSTRAINT | uq_device_keys_user_device_key |  | - | - |

**device_lists_changes**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | SERIAL | ✅ | - | - |
| user_id | VARCHAR(255) |  | ✅ | - |
| device_id | VARCHAR(255) |  | - | - |
| change_type | VARCHAR(50) |  | ✅ | - |
| stream_id | BIGINT |  | ✅ | - |
| created_ts | BIGINT |  | ✅ | - |

**device_lists_stream**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| stream_id | BIGSERIAL | ✅ | - | - |
| user_id | VARCHAR(255) |  | ✅ | - |
| device_id | VARCHAR(255) |  | - | - |
| created_ts | BIGINT |  | ✅ | - |

**device_signatures**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| user_id | TEXT |  | ✅ | - |
| device_id | TEXT |  | ✅ | - |
| target_user_id | TEXT |  | ✅ | - |
| target_device_id | TEXT |  | ✅ | - |
| algorithm | TEXT |  | ✅ | - |
| signature | TEXT |  | ✅ | - |
| created_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_device_signatures | ✅ | - | - |
| CONSTRAINT | uq_device_signatures_unique |  | - | - |


### 3.3 认证授权 (6表)

**access_tokens**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| token | TEXT |  | ✅ | - |
| user_id | TEXT |  | ✅ | - |
| device_id | TEXT |  | - | - |
| created_ts | BIGINT |  | ✅ | - |
| expires_at | BIGINT |  | - | - |
| last_used_ts | BIGINT |  | - | - |
| user_agent | TEXT |  | - | - |
| ip_address | TEXT |  | - | - |
| is_revoked | BOOLEAN |  | - | FALSE |
| CONSTRAINT | pk_access_tokens | ✅ | - | - |
| CONSTRAINT | uq_access_tokens_token |  | - | - |
| CONSTRAINT | fk_access_tokens_user |  | - | - |

**refresh_tokens**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| token_hash | TEXT |  | ✅ | - |
| user_id | TEXT |  | ✅ | - |
| device_id | TEXT |  | - | - |
| access_token_id | TEXT |  | - | - |
| scope | TEXT |  | - | - |
| created_ts | BIGINT |  | ✅ | - |
| expires_at | BIGINT |  | - | - |
| last_used_ts | BIGINT |  | - | - |
| use_count | INTEGER |  | - | 0 |
| is_revoked | BOOLEAN |  | - | FALSE |
| revoked_reason | TEXT |  | - | - |
| client_info | JSONB |  | - | - |
| ip_address | TEXT |  | - | - |
| user_agent | TEXT |  | - | - |
| CONSTRAINT | pk_refresh_tokens | ✅ | - | - |
| CONSTRAINT | uq_refresh_tokens_token_hash |  | - | - |
| CONSTRAINT | fk_refresh_tokens_user |  | - | - |

**refresh_token_rotations**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| family_id | TEXT |  | ✅ | - |
| old_token_hash | TEXT |  | - | - |
| new_token_hash | TEXT |  | ✅ | - |
| rotated_ts | BIGINT |  | ✅ | - |
| rotation_reason | TEXT |  | - | - |
| CONSTRAINT | pk_refresh_token_rotations | ✅ | - | - |

**refresh_token_usage**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| refresh_token_id | BIGINT |  | ✅ | - |
| user_id | TEXT |  | ✅ | - |
| old_access_token_id | TEXT |  | - | - |
| new_access_token_id | TEXT |  | - | - |
| used_ts | BIGINT |  | ✅ | - |
| ip_address | TEXT |  | - | - |
| user_agent | TEXT |  | - | - |
| is_success | BOOLEAN |  | - | TRUE |
| error_message | TEXT |  | - | - |
| CONSTRAINT | pk_refresh_token_usage | ✅ | - | - |

**token_blacklist**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| token_hash | TEXT |  | ✅ | - |
| token | TEXT |  | - | - |
| token_type | TEXT |  | - | 'access' |
| user_id | TEXT |  | - | - |
| is_revoked | BOOLEAN |  | - | TRUE |
| reason | TEXT |  | - | - |
| expires_at | BIGINT |  | - | - |
| CONSTRAINT | pk_token_blacklist | ✅ | - | - |
| CONSTRAINT | uq_token_blacklist_token_hash |  | - | - |

**openid_tokens**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| token | TEXT |  | ✅ | - |
| user_id | TEXT |  | ✅ | - |
| device_id | TEXT |  | - | - |
| created_ts | BIGINT |  | ✅ | - |
| expires_ts | BIGINT |  | ✅ | - |
| is_valid | BOOLEAN |  | - | TRUE |
| CONSTRAINT | pk_openid_tokens | ✅ | - | - |
| CONSTRAINT | uq_openid_tokens_token |  | - | - |
| CONSTRAINT | fk_openid_tokens_user |  | - | - |


### 3.4 房间管理 (12表)

**rooms**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| room_id | TEXT |  | ✅ | - |
| creator | TEXT |  | - | - |
| is_public | BOOLEAN |  | - | FALSE |
| room_version | TEXT |  | - | '6' |
| created_ts | BIGINT |  | ✅ | - |
| last_activity_ts | BIGINT |  | - | - |
| is_federated | BOOLEAN |  | - | TRUE |
| has_guest_access | BOOLEAN |  | - | FALSE |
| join_rules | TEXT |  | - | 'invite' |
| history_visibility | TEXT |  | - | 'shared' |
| name | TEXT |  | - | - |
| topic | TEXT |  | - | - |
| avatar_url | TEXT |  | - | - |
| canonical_alias | TEXT |  | - | - |
| visibility | TEXT |  | - | 'private' |
| CONSTRAINT | pk_rooms | ✅ | - | - |

**room_memberships**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| room_id | TEXT |  | ✅ | - |
| user_id | TEXT |  | ✅ | - |
| membership | TEXT |  | ✅ | - |
| joined_ts | BIGINT |  | - | - |
| invited_ts | BIGINT |  | - | - |
| left_ts | BIGINT |  | - | - |
| banned_ts | BIGINT |  | - | - |
| sender | TEXT |  | - | - |
| reason | TEXT |  | - | - |
| event_id | TEXT |  | - | - |
| event_type | TEXT |  | - | - |
| display_name | TEXT |  | - | - |
| avatar_url | TEXT |  | - | - |
| is_banned | BOOLEAN |  | - | FALSE |
| invite_token | TEXT |  | - | - |
| updated_ts | BIGINT |  | - | - |
| join_reason | TEXT |  | - | - |
| banned_by | TEXT |  | - | - |
| ban_reason | TEXT |  | - | - |
| CONSTRAINT | pk_room_memberships | ✅ | - | - |
| CONSTRAINT | uq_room_memberships_room_user |  | - | - |
| CONSTRAINT | fk_room_memberships_room |  | - | - |
| CONSTRAINT | fk_room_memberships_user |  | - | - |

**room_state_events**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| room_id | TEXT |  | ✅ | - |
| type | TEXT |  | ✅ | - |
| state_key | TEXT |  | ✅ | - |
| content | JSONB |  | ✅ | - |
| sender | TEXT |  | ✅ | - |
| origin_server_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_room_state_events | ✅ | - | - |
| CONSTRAINT | uq_room_state_events_room_type_key |  | - | - |

**room_events**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | SERIAL | ✅ | - | - |
| event_id | VARCHAR(255) |  | ✅ | - |
| room_id | VARCHAR(255) |  | ✅ | - |
| sender | VARCHAR(255) |  | ✅ | - |
| event_type | VARCHAR(255) |  | ✅ | - |
| state_key | VARCHAR(255) |  | - | - |
| content | JSONB |  | ✅ | '{}' |
| prev_event_id | VARCHAR(255) |  | - | - |
| origin_server_ts | BIGINT |  | ✅ | - |
| created_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | uq_room_events_event |  | - | - |

**room_account_data**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| user_id | TEXT |  | ✅ | - |
| room_id | TEXT |  | ✅ | - |
| data_type | TEXT |  | ✅ | - |
| data | JSONB |  | ✅ | - |
| created_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_room_account_data | ✅ | - | - |
| CONSTRAINT | uq_room_account_data_user_room_type |  | - | - |

**room_aliases**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| room_alias | TEXT |  | ✅ | - |
| room_id | TEXT |  | ✅ | - |
| server_name | TEXT |  | ✅ | - |
| created_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_room_aliases | ✅ | - | - |
| CONSTRAINT | fk_room_aliases_room |  | - | - |

**room_directory**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| room_id | TEXT |  | ✅ | - |
| is_public | BOOLEAN |  | - | TRUE |
| is_searchable | BOOLEAN |  | - | TRUE |
| app_service_id | TEXT |  | - | - |
| added_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_room_directory | ✅ | - | - |
| CONSTRAINT | uq_room_directory_room |  | - | - |
| CONSTRAINT | fk_room_directory_room |  | - | - |

**room_tags**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | SERIAL | ✅ | - | - |
| user_id | VARCHAR(255) |  | ✅ | - |
| room_id | VARCHAR(255) |  | ✅ | - |
| tag | VARCHAR(255) |  | ✅ | - |
| order_value | DOUBLE |  | - | - |
| created_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | uq_room_tags_user_room_tag |  | - | - |

**room_ephemeral**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | SERIAL | ✅ | - | - |
| room_id | VARCHAR(255) |  | ✅ | - |
| event_type | VARCHAR(255) |  | ✅ | - |
| user_id | VARCHAR(255) |  | ✅ | - |
| content | JSONB |  | ✅ | '{}' |
| stream_id | BIGINT |  | ✅ | - |
| created_ts | BIGINT |  | ✅ | - |
| expires_ts | BIGINT |  | - | - |
| expires_at | BIGINT |  | - | - |

**read_markers**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| room_id | TEXT |  | ✅ | - |
| user_id | TEXT |  | ✅ | - |
| event_id | TEXT |  | ✅ | - |
| marker_type | TEXT |  | ✅ | - |
| created_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_read_markers | ✅ | - | - |
| CONSTRAINT | uq_read_markers_room_user_type |  | - | - |

**room_summaries**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| room_id | TEXT |  | ✅ | - |
| name | TEXT |  | - | - |
| topic | TEXT |  | - | - |
| canonical_alias | TEXT |  | - | - |
| member_count | BIGINT |  | - | 0 |
| joined_members | BIGINT |  | - | 0 |
| invited_members | BIGINT |  | - | 0 |
| hero_users | JSONB |  | - | - |
| is_world_readable | BOOLEAN |  | - | FALSE |
| can_guest_join | BOOLEAN |  | - | FALSE |
| is_federated | BOOLEAN |  | - | TRUE |
| encryption_state | TEXT |  | - | - |
| updated_ts | BIGINT |  | - | - |
| CONSTRAINT | pk_room_summaries | ✅ | - | - |

**room_invites**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| room_id | TEXT |  | ✅ | - |
| inviter | TEXT |  | ✅ | - |
| invitee | TEXT |  | ✅ | - |
| is_accepted | BOOLEAN |  | - | FALSE |
| accepted_at | BIGINT |  | - | - |
| created_ts | BIGINT |  | ✅ | - |
| expires_at | BIGINT |  | - | - |
| CONSTRAINT | pk_room_invites | ✅ | - | - |


### 3.5 消息系统 (6表)

**events**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| event_id | TEXT |  | ✅ | - |
| room_id | TEXT |  | ✅ | - |
| sender | TEXT |  | ✅ | - |
| event_type | TEXT |  | ✅ | - |
| content | JSONB |  | ✅ | - |
| origin_server_ts | BIGINT |  | ✅ | - |
| state_key | TEXT |  | - | - |
| is_redacted | BOOLEAN |  | - | FALSE |
| redacted_at | BIGINT |  | - | - |
| redacted_by | TEXT |  | - | - |
| transaction_id | TEXT |  | - | - |
| depth | BIGINT |  | - | - |
| prev_events | JSONB |  | - | - |
| auth_events | JSONB |  | - | - |
| signatures | JSONB |  | - | - |
| hashes | JSONB |  | - | - |
| unsigned | JSONB |  | - | '{}' |
| processed_at | BIGINT |  | - | - |
| not_before | BIGINT |  | - | 0 |
| status | TEXT |  | - | - |
| reference_image | TEXT |  | - | - |
| origin | TEXT |  | - | - |
| user_id | TEXT |  | - | - |
| CONSTRAINT | pk_events | ✅ | - | - |
| CONSTRAINT | fk_events_room |  | - | - |

**event_receipts**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| event_id | TEXT |  | ✅ | - |
| room_id | TEXT |  | ✅ | - |
| user_id | TEXT |  | ✅ | - |
| receipt_type | TEXT |  | ✅ | - |
| ts | BIGINT |  | ✅ | - |
| data | JSONB |  | - | '{}' |
| created_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_event_receipts | ✅ | - | - |
| CONSTRAINT | uq_event_receipts_event_room_user_type |  | - | - |

**event_signatures**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | UUID |  | - | gen_random_uuid() |
| event_id | TEXT |  | ✅ | - |
| user_id | TEXT |  | ✅ | - |
| device_id | TEXT |  | ✅ | - |
| signature | TEXT |  | ✅ | - |
| key_id | TEXT |  | ✅ | - |
| algorithm | TEXT |  | ✅ | - |
| created_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_event_signatures | ✅ | - | - |
| CONSTRAINT | uq_event_signatures_event_user_device_key |  | - | - |

**event_reports**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| event_id | TEXT |  | ✅ | - |
| room_id | TEXT |  | ✅ | - |
| reporter_user_id | TEXT |  | ✅ | - |
| reported_user_id | TEXT |  | - | - |
| event_json | JSONB |  | - | - |
| reason | TEXT |  | - | - |
| description | TEXT |  | - | - |
| status | TEXT |  | - | 'open' |
| score | INTEGER |  | - | 0 |
| received_ts | BIGINT |  | ✅ | - |
| resolved_at | BIGINT |  | - | - |
| resolved_by | TEXT |  | - | - |
| resolution_reason | TEXT |  | - | - |
| CONSTRAINT | pk_event_reports | ✅ | - | - |

**to_device_messages**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | SERIAL | ✅ | - | - |
| sender_user_id | VARCHAR(255) |  | ✅ | - |
| sender_device_id | VARCHAR(255) |  | ✅ | - |
| recipient_user_id | VARCHAR(255) |  | ✅ | - |
| recipient_device_id | VARCHAR(255) |  | ✅ | - |
| event_type | VARCHAR(255) |  | ✅ | - |
| content | JSONB |  | ✅ | '{}' |
| message_id | VARCHAR(255) |  | - | - |
| stream_id | BIGINT |  | ✅ | - |
| created_ts | BIGINT |  | ✅ | - |

**typing**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| user_id | TEXT |  | ✅ | - |
| room_id | TEXT |  | ✅ | - |
| typing | BOOLEAN |  | - | FALSE |
| last_active_ts | BIGINT |  | ✅ | - |


### 3.6 加密安全 (8表)

**olm_accounts**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL | ✅ | - | - |
| user_id | TEXT |  | ✅ | - |
| device_id | TEXT |  | ✅ | - |
| identity_key | TEXT |  | ✅ | - |
| serialized_account | TEXT |  | ✅ | - |
| is_one_time_keys_published | BOOLEAN |  | - | FALSE |
| is_fallback_key_published | BOOLEAN |  | - | FALSE |
| created_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | uq_olm_accounts_user_device |  | - | - |

**olm_sessions**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL | ✅ | - | - |
| user_id | TEXT |  | ✅ | - |
| device_id | TEXT |  | ✅ | - |
| session_id | TEXT |  | ✅ | - |
| sender_key | TEXT |  | ✅ | - |
| receiver_key | TEXT |  | ✅ | - |
| serialized_state | TEXT |  | ✅ | - |
| message_index | INTEGER |  | - | 0 |
| created_ts | BIGINT |  | ✅ | - |
| last_used_ts | BIGINT |  | ✅ | - |
| expires_at | BIGINT |  | - | - |
| CONSTRAINT | uq_olm_sessions_session |  | - | - |

**megolm_sessions**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | UUID |  | - | gen_random_uuid() |
| session_id | TEXT |  | ✅ | - |
| room_id | TEXT |  | ✅ | - |
| sender_key | TEXT |  | ✅ | - |
| session_key | TEXT |  | ✅ | - |
| algorithm | TEXT |  | ✅ | - |
| message_index | BIGINT |  | - | 0 |
| created_ts | BIGINT |  | ✅ | - |
| last_used_ts | BIGINT |  | - | - |
| expires_at | BIGINT |  | - | - |
| CONSTRAINT | pk_megolm_sessions | ✅ | - | - |
| CONSTRAINT | uq_megolm_sessions_session |  | - | - |

**one_time_keys**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| user_id | TEXT |  | ✅ | - |
| device_id | TEXT |  | ✅ | - |
| algorithm | TEXT |  | ✅ | - |
| key_id | TEXT |  | ✅ | - |
| key_data | TEXT |  | ✅ | - |
| is_used | BOOLEAN |  | - | FALSE |
| used_ts | BIGINT |  | - | - |
| created_ts | BIGINT |  | ✅ | - |
| expires_at | BIGINT |  | - | - |
| CONSTRAINT | pk_one_time_keys | ✅ | - | - |
| CONSTRAINT | uq_one_time_keys_user_device_algorithm |  | - | - |
| CONSTRAINT | fk_one_time_keys_user |  | - | - |

**key_backups**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| backup_id | BIGSERIAL |  | - | - |
| user_id | TEXT |  | ✅ | - |
| algorithm | TEXT |  | ✅ | - |
| auth_data | JSONB |  | - | - |
| auth_key | TEXT |  | - | - |
| mgmt_key | TEXT |  | - | - |
| version | BIGINT |  | - | 1 |
| created_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | - | - |
| CONSTRAINT | pk_key_backups | ✅ | - | - |
| CONSTRAINT | uq_key_backups_user_version |  | - | - |

**backup_keys**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| backup_id | BIGINT |  | ✅ | - |
| room_id | TEXT |  | ✅ | - |
| session_id | TEXT |  | ✅ | - |
| session_data | JSONB |  | ✅ | - |
| created_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_backup_keys | ✅ | - | - |
| CONSTRAINT | fk_backup_keys_backup |  | - | - |

**cross_signing_keys**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| user_id | TEXT |  | ✅ | - |
| key_type | TEXT |  | ✅ | - |
| key_data | TEXT |  | ✅ | - |
| signatures | JSONB |  | - | - |
| added_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_cross_signing_keys | ✅ | - | - |
| CONSTRAINT | uq_cross_signing_keys_user_type |  | - | - |

**e2ee_key_requests**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL | ✅ | - | - |
| request_id | TEXT |  | ✅ | - |
| user_id | TEXT |  | ✅ | - |
| device_id | TEXT |  | ✅ | - |
| room_id | TEXT |  | ✅ | - |
| session_id | TEXT |  | ✅ | - |
| algorithm | TEXT |  | ✅ | - |
| action | TEXT |  | ✅ | - |
| is_fulfilled | BOOLEAN |  | - | FALSE |
| fulfilled_by_device | TEXT |  | - | - |
| fulfilled_ts | BIGINT |  | - | - |
| created_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | uq_e2ee_key_requests_request |  | - | - |


### 3.7 好友系统 (3表)

**friends**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| user_id | TEXT |  | ✅ | - |
| friend_id | TEXT |  | ✅ | - |
| created_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_friends | ✅ | - | - |
| CONSTRAINT | uq_friends_user_friend |  | - | - |
| CONSTRAINT | fk_friends_user |  | - | - |
| CONSTRAINT | fk_friends_friend |  | - | - |

**friend_requests**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| sender_id | TEXT |  | ✅ | - |
| receiver_id | TEXT |  | ✅ | - |
| message | TEXT |  | - | - |
| status | TEXT |  | ✅ | 'pending' |
| created_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | - | - |
| CONSTRAINT | pk_friend_requests | ✅ | - | - |
| CONSTRAINT | uq_friend_requests_sender_receiver |  | - | - |
| CONSTRAINT | fk_friend_requests_sender |  | - | - |
| CONSTRAINT | fk_friend_requests_receiver |  | - | - |

**friend_categories**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| user_id | TEXT |  | ✅ | - |
| name | TEXT |  | ✅ | - |
| color | TEXT |  | ✅ | '#000000' |
| created_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_friend_categories | ✅ | - | - |
| CONSTRAINT | fk_friend_categories_user |  | - | - |


### 3.8 空间(Space) (4表)

**spaces**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| space_id | TEXT | ✅ | ✅ | - |
| name | TEXT |  | - | - |
| creator | TEXT |  | ✅ | - |
| created_ts | BIGINT |  | ✅ | - |
| is_public | BOOLEAN |  | - | FALSE |
| is_private | BOOLEAN |  | - | TRUE |
| member_count | BIGINT |  | - | 0 |
| topic | TEXT |  | - | - |
| avatar_url | TEXT |  | - | - |
| canonical_alias | TEXT |  | - | - |
| history_visibility | TEXT |  | - | 'shared' |
| join_rules | TEXT |  | - | 'invite' |
| room_type | TEXT |  | - | 'm.space' |
| updated_ts | BIGINT |  | - | - |

**space_children**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| space_id | TEXT |  | ✅ | - |
| room_id | TEXT |  | ✅ | - |
| sender | TEXT |  | ✅ | - |
| is_suggested | BOOLEAN |  | - | FALSE |
| via_servers | JSONB |  | - | '[]' |
| added_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_space_children | ✅ | - | - |
| CONSTRAINT | uq_space_children_space_room |  | - | - |

**space_hierarchy**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL | ✅ | - | - |
| space_id | TEXT |  | ✅ | - |
| room_id | TEXT |  | ✅ | - |
| parent_space_id | TEXT |  | - | - |
| depth | INTEGER |  | - | 0 |
| children | TEXT |  | - | - |
| via_servers | TEXT |  | - | - |
| created_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | uq_space_hierarchy |  | - | - |

**room_parents**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| room_id | TEXT |  | ✅ | - |
| parent_room_id | TEXT |  | ✅ | - |
| sender | TEXT |  | ✅ | - |
| is_suggested | BOOLEAN |  | - | FALSE |
| via_servers | JSONB |  | - | '[]' |
| added_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_room_parents | ✅ | - | - |
| CONSTRAINT | uq_room_parents_room_parent |  | - | - |
| CONSTRAINT | fk_room_parents_room |  | - | - |
| CONSTRAINT | fk_room_parents_parent |  | - | - |


### 3.9 线程(Thread) (2表)

**thread_roots**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| room_id | TEXT |  | ✅ | - |
| event_id | TEXT |  | ✅ | - |
| sender | TEXT |  | ✅ | - |
| thread_id | TEXT |  | - | - |
| reply_count | BIGINT |  | - | 0 |
| last_reply_event_id | TEXT |  | - | - |
| last_reply_sender | TEXT |  | - | - |
| last_reply_ts | BIGINT |  | - | - |
| participants | JSONB |  | - | '[]' |
| is_fetched | BOOLEAN |  | - | FALSE |
| created_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | - | - |
| CONSTRAINT | pk_thread_roots | ✅ | - | - |
| CONSTRAINT | uq_thread_roots_room_event |  | - | - |
| CONSTRAINT | fk_thread_roots_room |  | - | - |

**thread_subscriptions**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL | ✅ | - | - |
| room_id | TEXT |  | ✅ | - |
| thread_id | TEXT |  | ✅ | - |
| user_id | TEXT |  | ✅ | - |
| notification_level | TEXT |  | - | 'all' |
| is_muted | BOOLEAN |  | - | FALSE |
| is_pinned | BOOLEAN |  | - | FALSE |
| subscribed_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | uq_thread_subscriptions |  | - | - |


### 3.10 媒体管理 (5表)

**media_metadata**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| media_id | TEXT |  | ✅ | - |
| server_name | TEXT |  | ✅ | - |
| content_type | TEXT |  | ✅ | - |
| file_name | TEXT |  | - | - |
| size | BIGINT |  | ✅ | - |
| uploader_user_id | TEXT |  | - | - |
| created_ts | BIGINT |  | ✅ | - |
| last_accessed_at | BIGINT |  | - | - |
| quarantine_status | TEXT |  | - | - |
| CONSTRAINT | pk_media_metadata | ✅ | - | - |

**thumbnails**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| media_id | TEXT |  | ✅ | - |
| width | INTEGER |  | ✅ | - |
| height | INTEGER |  | ✅ | - |
| method | TEXT |  | ✅ | - |
| content_type | TEXT |  | ✅ | - |
| size | BIGINT |  | ✅ | - |
| created_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_thumbnails | ✅ | - | - |
| CONSTRAINT | fk_thumbnails_media |  | - | - |

**media_quota**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| user_id | TEXT |  | ✅ | - |
| max_bytes | BIGINT |  | - | 1073741824 |
| used_bytes | BIGINT |  | - | 0 |
| created_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | - | - |
| CONSTRAINT | pk_media_quota | ✅ | - | - |
| CONSTRAINT | uq_media_quota_user |  | - | - |
| CONSTRAINT | fk_media_quota_user |  | - | - |

**media_quota_config**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| config_name | TEXT |  | ✅ | - |
| max_file_size | BIGINT |  | - | 10485760 |
| max_upload_rate | BIGINT |  | - | - |
| allowed_content_types | TEXT |  | - | ARRAY['image/jpeg' |
| retention_days | INTEGER |  | - | 90 |
| is_enabled | BOOLEAN |  | - | TRUE |
| created_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | - | - |
| CONSTRAINT | pk_media_quota_config | ✅ | - | - |
| CONSTRAINT | uq_media_quota_config_name |  | - | - |

**user_media_quota**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| user_id | TEXT |  | ✅ | - |
| max_bytes | BIGINT |  | - | 1073741824 |
| used_bytes | BIGINT |  | - | 0 |
| file_count | INTEGER |  | - | 0 |
| created_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | - | - |
| CONSTRAINT | pk_user_media_quota | ✅ | - | - |
| CONSTRAINT | uq_user_media_quota_user |  | - | - |
| CONSTRAINT | fk_user_media_quota_user |  | - | - |


### 3.11 推送通知 (5表)

**pushers**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| user_id | TEXT |  | ✅ | - |
| device_id | TEXT |  | ✅ | - |
| pushkey | TEXT |  | ✅ | - |
| pushkey_ts | BIGINT |  | ✅ | - |
| kind | TEXT |  | ✅ | - |
| app_id | TEXT |  | ✅ | - |
| app_display_name | TEXT |  | ✅ | - |
| device_display_name | TEXT |  | ✅ | - |
| profile_tag | TEXT |  | - | - |
| lang | TEXT |  | - | 'en' |
| data | JSONB |  | - | '{}' |
| updated_ts | BIGINT |  | - | - |
| created_ts | BIGINT |  | ✅ | - |
| is_enabled | BOOLEAN |  | - | TRUE |
| CONSTRAINT | pk_pushers | ✅ | - | - |
| CONSTRAINT | uq_pushers_user_device_pushkey |  | - | - |

**push_devices**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| user_id | TEXT |  | ✅ | - |
| device_id | TEXT |  | ✅ | - |
| push_kind | TEXT |  | ✅ | - |
| app_id | TEXT |  | ✅ | - |
| app_display_name | TEXT |  | - | - |
| device_display_name | TEXT |  | - | - |
| profile_tag | TEXT |  | - | - |
| pushkey | TEXT |  | ✅ | - |
| lang | TEXT |  | - | 'en' |
| data | JSONB |  | - | '{}' |
| created_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | - | - |
| is_enabled | BOOLEAN |  | - | TRUE |
| CONSTRAINT | pk_push_devices | ✅ | - | - |
| CONSTRAINT | uq_push_devices_user_device_pushkey |  | - | - |

**push_rules**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| user_id | TEXT |  | ✅ | - |
| scope | TEXT |  | ✅ | - |
| rule_id | TEXT |  | ✅ | - |
| kind | TEXT |  | ✅ | - |
| priority_class | INTEGER |  | ✅ | - |
| priority | INTEGER |  | - | 0 |
| conditions | JSONB |  | - | '[]' |
| actions | JSONB |  | - | '[]' |
| pattern | TEXT |  | - | - |
| is_default | BOOLEAN |  | - | FALSE |
| is_enabled | BOOLEAN |  | - | TRUE |
| created_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | - | - |
| CONSTRAINT | pk_push_rules | ✅ | - | - |
| CONSTRAINT | uq_push_rules_user_scope_rule |  | - | - |

**push_notification_queue**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| user_id | TEXT |  | ✅ | - |
| device_id | TEXT |  | ✅ | - |
| event_id | TEXT |  | ✅ | - |
| room_id | TEXT |  | ✅ | - |
| notification_type | TEXT |  | ✅ | - |
| content | JSONB |  | - | '{}' |
| is_processed | BOOLEAN |  | - | FALSE |
| processed_at | BIGINT |  | - | - |
| created_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_push_notification_queue | ✅ | - | - |

**notifications**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| user_id | TEXT |  | ✅ | - |
| event_id | TEXT |  | - | - |
| room_id | TEXT |  | - | - |
| ts | BIGINT |  | ✅ | - |
| notification_type | VARCHAR(50) |  | - | 'message' |
| profile_tag | VARCHAR(255) |  | - | - |
| is_read | BOOLEAN |  | - | FALSE |
| updated_ts | BIGINT |  | - | - |
| CONSTRAINT | pk_notifications | ✅ | - | - |


### 3.12 应用服务 (6表)

**application_services**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| as_id | TEXT |  | ✅ | - |
| url | TEXT |  | ✅ | - |
| as_token | TEXT |  | ✅ | - |
| hs_token | TEXT |  | ✅ | - |
| sender_localpart | TEXT |  | ✅ | - |
| is_enabled | BOOLEAN |  | - | FALSE |
| rate_limited | BOOLEAN |  | - | TRUE |
| protocols | TEXT |  | - | '{}' |
| namespaces | JSONB |  | - | '{}' |
| created_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | - | - |
| description | TEXT |  | - | - |
| CONSTRAINT | pk_application_services | ✅ | - | - |
| CONSTRAINT | uq_application_services_id |  | - | - |

**application_service_transactions**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| as_id | TEXT |  | ✅ | - |
| txn_id | TEXT |  | ✅ | - |
| data | JSONB |  | - | '{}' |
| processed | BOOLEAN |  | - | FALSE |
| processed_ts | BIGINT |  | - | - |
| created_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_application_service_transactions | ✅ | - | - |
| CONSTRAINT | uq_application_service_transactions_as_txn |  | - | - |

**application_service_state**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| as_id | TEXT |  | ✅ | - |
| state_key | TEXT |  | ✅ | - |
| value | JSONB |  | ✅ | - |
| updated_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_application_service_state | ✅ | - | - |
| CONSTRAINT | uq_application_service_state_as_key |  | - | - |
| CONSTRAINT | fk_application_service_state_as |  | - | - |

**application_service_room_namespaces**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| as_id | TEXT |  | ✅ | - |
| namespace | TEXT |  | ✅ | - |
| is_exclusive | BOOLEAN |  | - | TRUE |
| created_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_application_service_room_namespaces | ✅ | - | - |
| CONSTRAINT | fk_application_service_room_namespaces_as |  | - | - |

**application_service_user_namespaces**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| as_id | TEXT |  | ✅ | - |
| namespace | TEXT |  | ✅ | - |
| is_exclusive | BOOLEAN |  | - | TRUE |
| created_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_application_service_user_namespaces | ✅ | - | - |
| CONSTRAINT | fk_application_service_user_namespaces_as |  | - | - |

**application_service_events**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| as_id | TEXT |  | ✅ | - |
| event_id | TEXT |  | ✅ | - |
| room_id | TEXT |  | - | - |
| event_type | TEXT |  | - | - |
| processed | BOOLEAN |  | - | FALSE |
| processed_ts | BIGINT |  | - | - |
| created_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_application_service_events | ✅ | - | - |
| CONSTRAINT | uq_application_service_events_event |  | - | - |


### 3.13 联邦通信 (3表)

**federation_servers**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| server_name | TEXT |  | ✅ | - |
| is_blocked | BOOLEAN |  | - | FALSE |
| blocked_at | BIGINT |  | - | - |
| blocked_reason | TEXT |  | - | - |
| last_successful_connect_at | BIGINT |  | - | - |
| last_failed_connect_at | BIGINT |  | - | - |
| failure_count | INTEGER |  | - | 0 |
| CONSTRAINT | pk_federation_servers | ✅ | - | - |
| CONSTRAINT | uq_federation_servers_name |  | - | - |

**federation_queue**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| destination | TEXT |  | ✅ | - |
| event_id | TEXT |  | ✅ | - |
| event_type | TEXT |  | ✅ | - |
| room_id | TEXT |  | - | - |
| content | JSONB |  | ✅ | - |
| created_ts | BIGINT |  | ✅ | - |
| sent_at | BIGINT |  | - | - |
| retry_count | INTEGER |  | - | 0 |
| status | TEXT |  | - | 'pending' |
| CONSTRAINT | pk_federation_queue | ✅ | - | - |

**federation_blacklist**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| server_name | TEXT |  | ✅ | - |
| reason | TEXT |  | - | - |
| added_ts | BIGINT |  | ✅ | - |
| added_by | TEXT |  | - | - |
| updated_ts | BIGINT |  | - | - |
| CONSTRAINT | pk_federation_blacklist | ✅ | - | - |
| CONSTRAINT | uq_federation_blacklist_name |  | - | - |


### 3.14 Worker (4表)

**workers**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| worker_id | TEXT |  | ✅ | - |
| worker_name | TEXT |  | ✅ | - |
| worker_type | TEXT |  | ✅ | - |
| host | TEXT |  | ✅ | 'localhost' |
| port | INTEGER |  | ✅ | 8080 |
| status | TEXT |  | ✅ | 'starting' |
| last_heartbeat_ts | BIGINT |  | - | - |
| started_ts | BIGINT |  | ✅ | - |
| stopped_ts | BIGINT |  | - | - |
| config | JSONB |  | - | '{}' |
| metadata | JSONB |  | - | '{}' |
| version | TEXT |  | - | - |
| is_enabled | BOOLEAN |  | - | TRUE |
| CONSTRAINT | pk_workers | ✅ | - | - |
| CONSTRAINT | uq_workers_id |  | - | - |

**worker_events**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| event_id | TEXT |  | ✅ | - |
| stream_id | BIGINT |  | ✅ | - |
| event_type | TEXT |  | ✅ | - |
| room_id | TEXT |  | - | - |
| sender | TEXT |  | - | - |
| event_data | JSONB |  | - | '{}' |
| created_ts | BIGINT |  | ✅ | - |
| processed_by | JSONB |  | - | '[]' |
| CONSTRAINT | pk_worker_events | ✅ | - | - |
| CONSTRAINT | uq_worker_events_id |  | - | - |

**worker_commands**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| command_id | TEXT |  | ✅ | - |
| target_worker_id | TEXT |  | ✅ | - |
| source_worker_id | TEXT |  | - | - |
| command_type | TEXT |  | ✅ | - |
| command_data | JSONB |  | - | '{}' |
| priority | INTEGER |  | - | 0 |
| status | TEXT |  | ✅ | 'pending' |
| created_ts | BIGINT |  | ✅ | - |
| sent_ts | BIGINT |  | - | - |
| completed_ts | BIGINT |  | - | - |
| error_message | TEXT |  | - | - |
| retry_count | INTEGER |  | - | 0 |
| max_retries | INTEGER |  | - | 3 |
| CONSTRAINT | pk_worker_commands | ✅ | - | - |
| CONSTRAINT | uq_worker_commands_id |  | - | - |

**worker_statistics**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| worker_id | TEXT |  | ✅ | - |
| total_messages_sent | BIGINT |  | - | 0 |
| total_messages_received | BIGINT |  | - | 0 |
| total_errors | BIGINT |  | - | 0 |
| last_message_ts | BIGINT |  | - | - |
| last_error_ts | BIGINT |  | - | - |
| avg_processing_time_ms | BIGINT |  | - | - |
| uptime_seconds | BIGINT |  | - | 0 |
| created_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_worker_statistics | ✅ | - | - |


### 3.15 模块扩展 (4表)

**modules**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| module_name | TEXT |  | ✅ | - |
| module_type | TEXT |  | ✅ | - |
| is_enabled | BOOLEAN |  | - | TRUE |
| config | JSONB |  | - | '{}' |
| priority | INTEGER |  | - | 0 |
| created_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | - | - |
| description | TEXT |  | - | - |
| CONSTRAINT | pk_modules | ✅ | - | - |
| CONSTRAINT | uq_modules_name |  | - | - |

**module_execution_logs**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| module_id | BIGINT |  | - | - |
| execution_type | TEXT |  | ✅ | - |
| input_data | JSONB |  | - | - |
| output_data | JSONB |  | - | - |
| is_success | BOOLEAN |  | - | TRUE |
| error_message | TEXT |  | - | - |
| execution_time_ms | BIGINT |  | - | - |
| created_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_module_execution_logs | ✅ | - | - |
| CONSTRAINT | fk_module_execution_logs_module |  | - | - |

**account_data_callbacks**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| callback_name | TEXT |  | ✅ | - |
| is_enabled | BOOLEAN |  | - | TRUE |
| data_types | TEXT |  | - | '{}' |
| config | JSONB |  | - | '{}' |
| created_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_account_data_callbacks | ✅ | - | - |
| CONSTRAINT | uq_account_data_callbacks_name |  | - | - |

**rate_limit_callbacks**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| callback_name | TEXT |  | ✅ | - |
| is_enabled | BOOLEAN |  | - | TRUE |
| config | JSONB |  | - | '{}' |
| created_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_rate_limit_callbacks | ✅ | - | - |
| CONSTRAINT | uq_rate_limit_callbacks_name |  | - | - |


### 3.16 联合认证 (11表)

**saml_sessions**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| session_id | TEXT |  | ✅ | - |
| user_id | TEXT |  | ✅ | - |
| name_id | TEXT |  | - | - |
| issuer | TEXT |  | - | - |
| session_index | TEXT |  | - | - |
| attributes | JSONB |  | - | '{}' |
| created_ts | BIGINT |  | ✅ | - |
| expires_ts | BIGINT |  | ✅ | - |
| last_used_ts | BIGINT |  | ✅ | - |
| status | TEXT |  | - | 'active' |
| CONSTRAINT | pk_saml_sessions | ✅ | - | - |
| CONSTRAINT | uq_saml_sessions_session |  | - | - |

**saml_identity_providers**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| entity_id | TEXT |  | ✅ | - |
| display_name | TEXT |  | - | - |
| description | TEXT |  | - | - |
| metadata_url | TEXT |  | - | - |
| metadata_xml | TEXT |  | - | - |
| is_enabled | BOOLEAN |  | - | TRUE |
| priority | INTEGER |  | - | 100 |
| attribute_mapping | JSONB |  | - | '{}' |
| created_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | - | - |
| last_metadata_refresh_at | BIGINT |  | - | - |
| metadata_valid_until_at | BIGINT |  | - | - |
| CONSTRAINT | pk_saml_identity_providers | ✅ | - | - |
| CONSTRAINT | uq_saml_identity_providers_entity |  | - | - |

**saml_logout_requests**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| request_id | TEXT |  | ✅ | - |
| session_id | TEXT |  | - | - |
| user_id | TEXT |  | - | - |
| name_id | TEXT |  | - | - |
| issuer | TEXT |  | - | - |
| reason | TEXT |  | - | - |
| status | TEXT |  | - | 'pending' |
| created_ts | BIGINT |  | ✅ | - |
| processed_at | BIGINT |  | - | - |
| CONSTRAINT | pk_saml_logout_requests | ✅ | - | - |
| CONSTRAINT | uq_saml_logout_requests_request |  | - | - |

**saml_user_mapping**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| name_id | TEXT |  | ✅ | - |
| user_id | TEXT |  | ✅ | - |
| issuer | TEXT |  | ✅ | - |
| first_seen_ts | BIGINT |  | ✅ | - |
| last_authenticated_ts | BIGINT |  | ✅ | - |
| authentication_count | INTEGER |  | - | 1 |
| attributes | JSONB |  | - | '{}' |
| CONSTRAINT | pk_saml_user_mapping | ✅ | - | - |
| CONSTRAINT | uq_saml_user_mapping_name_issuer |  | - | - |

**saml_auth_events**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| session_id | TEXT |  | - | - |
| user_id | TEXT |  | - | - |
| name_id | TEXT |  | - | - |
| issuer | TEXT |  | - | - |
| event_type | TEXT |  | ✅ | - |
| status | TEXT |  | ✅ | - |
| error_message | TEXT |  | - | - |
| ip_address | TEXT |  | - | - |
| user_agent | TEXT |  | - | - |
| request_id | TEXT |  | - | - |
| attributes | JSONB |  | - | '{}' |
| created_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_saml_auth_events | ✅ | - | - |

**cas_services**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| service_id | TEXT |  | ✅ | - |
| name | TEXT |  | ✅ | - |
| description | TEXT |  | - | - |
| service_url_pattern | TEXT |  | ✅ | - |
| allowed_attributes | JSONB |  | - | '[]' |
| allowed_proxy_callbacks | JSONB |  | - | '[]' |
| is_enabled | BOOLEAN |  | - | TRUE |
| require_secure | BOOLEAN |  | - | TRUE |
| single_logout | BOOLEAN |  | - | FALSE |
| created_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | - | - |
| CONSTRAINT | pk_cas_services | ✅ | - | - |
| CONSTRAINT | uq_cas_services_service |  | - | - |

**cas_tickets**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| ticket_id | TEXT |  | ✅ | - |
| user_id | TEXT |  | ✅ | - |
| service_url | TEXT |  | ✅ | - |
| created_ts | BIGINT |  | ✅ | - |
| expires_ts | BIGINT |  | ✅ | - |
| consumed_at | BIGINT |  | - | - |
| consumed_by | TEXT |  | - | - |
| is_valid | BOOLEAN |  | - | TRUE |
| CONSTRAINT | pk_cas_tickets | ✅ | - | - |
| CONSTRAINT | uq_cas_tickets_ticket |  | - | - |

**cas_proxy_tickets**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| proxy_ticket_id | TEXT |  | ✅ | - |
| user_id | TEXT |  | ✅ | - |
| service_url | TEXT |  | ✅ | - |
| pgt_url | TEXT |  | - | - |
| created_ts | BIGINT |  | ✅ | - |
| expires_ts | BIGINT |  | ✅ | - |
| consumed_at | BIGINT |  | - | - |
| is_valid | BOOLEAN |  | - | TRUE |
| CONSTRAINT | pk_cas_proxy_tickets | ✅ | - | - |
| CONSTRAINT | uq_cas_proxy_tickets_ticket |  | - | - |

**cas_slo_sessions**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| session_id | TEXT |  | ✅ | - |
| user_id | TEXT |  | ✅ | - |
| service_url | TEXT |  | ✅ | - |
| ticket_id | TEXT |  | - | - |
| created_ts | BIGINT |  | ✅ | - |
| logout_sent_at | BIGINT |  | - | - |
| CONSTRAINT | pk_cas_slo_sessions | ✅ | - | - |

**cas_user_attributes**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| user_id | TEXT |  | ✅ | - |
| attribute_name | TEXT |  | ✅ | - |
| attribute_value | TEXT |  | ✅ | - |
| created_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | - | - |
| CONSTRAINT | pk_cas_user_attributes | ✅ | - | - |
| CONSTRAINT | uq_cas_user_attributes_user_name |  | - | - |

**cas_proxy_granting_tickets**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| pgt_id | TEXT |  | ✅ | - |
| user_id | TEXT |  | ✅ | - |
| service_url | TEXT |  | ✅ | - |
| iou | TEXT |  | - | - |
| created_ts | BIGINT |  | ✅ | - |
| expires_ts | BIGINT |  | ✅ | - |
| is_valid | BOOLEAN |  | - | TRUE |
| CONSTRAINT | pk_cas_proxy_granting_tickets | ✅ | - | - |
| CONSTRAINT | uq_cas_proxy_granting_tickets_pgt |  | - | - |


### 3.17 注册管理 (6表)

**registration_tokens**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| token | TEXT |  | ✅ | - |
| token_type | TEXT |  | - | 'single_use' |
| description | TEXT |  | - | - |
| max_uses | INTEGER |  | - | 0 |
| uses_count | INTEGER |  | - | 0 |
| is_used | BOOLEAN |  | - | FALSE |
| is_enabled | BOOLEAN |  | - | TRUE |
| created_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | - | - |
| expires_at | BIGINT |  | - | - |
| last_used_ts | BIGINT |  | - | - |
| created_by | TEXT |  | ✅ | - |
| allowed_email_domains | TEXT |  | - | - |
| allowed_user_ids | TEXT |  | - | - |
| auto_join_rooms | TEXT |  | - | - |
| display_name | TEXT |  | - | - |
| email | TEXT |  | - | - |
| CONSTRAINT | pk_registration_tokens | ✅ | - | - |
| CONSTRAINT | uq_registration_tokens_token |  | - | - |

**registration_token_usage**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| token_id | BIGINT |  | - | - |
| user_id | TEXT |  | ✅ | - |
| used_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_registration_token_usage | ✅ | - | - |
| CONSTRAINT | fk_registration_token_usage_token |  | - | - |

**captcha_config**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| config_key | TEXT |  | ✅ | - |
| config_value | TEXT |  | ✅ | - |
| description | TEXT |  | - | - |
| created_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | - | - |
| CONSTRAINT | pk_captcha_config | ✅ | - | - |
| CONSTRAINT | uq_captcha_config_key |  | - | - |

**captcha_send_log**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| captcha_id | TEXT |  | - | - |
| captcha_type | TEXT |  | ✅ | - |
| target | TEXT |  | ✅ | - |
| sent_ts | BIGINT |  | ✅ | - |
| ip_address | TEXT |  | - | - |
| user_agent | TEXT |  | - | - |
| is_success | BOOLEAN |  | - | TRUE |
| error_message | TEXT |  | - | - |
| provider | TEXT |  | - | - |
| provider_response | TEXT |  | - | - |
| CONSTRAINT | pk_captcha_send_log | ✅ | - | - |

**captcha_template**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| template_name | TEXT |  | ✅ | - |
| captcha_type | TEXT |  | ✅ | - |
| subject | TEXT |  | - | - |
| content | TEXT |  | ✅ | - |
| variables | JSONB |  | - | '{}' |
| is_default | BOOLEAN |  | - | FALSE |
| is_enabled | BOOLEAN |  | - | TRUE |
| created_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | - | - |
| CONSTRAINT | pk_captcha_template | ✅ | - | - |
| CONSTRAINT | uq_captcha_template_name |  | - | - |

**registration_captcha**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| captcha_id | TEXT |  | ✅ | - |
| captcha_type | TEXT |  | ✅ | - |
| target | TEXT |  | ✅ | - |
| code | TEXT |  | ✅ | - |
| created_ts | BIGINT |  | ✅ | - |
| expires_ts | BIGINT |  | ✅ | - |
| used_at | BIGINT |  | - | - |
| verified_at | BIGINT |  | - | - |
| ip_address | TEXT |  | - | - |
| user_agent | TEXT |  | - | - |
| attempt_count | INTEGER |  | - | 0 |
| max_attempts | INTEGER |  | - | 3 |
| status | TEXT |  | - | 'pending' |
| metadata | JSONB |  | - | '{}' |
| CONSTRAINT | pk_registration_captcha | ✅ | - | - |
| CONSTRAINT | uq_registration_captcha_id |  | - | - |


### 3.18 保留策略 (1表)

**server_retention_policy**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| policy_name | TEXT |  | ✅ | - |
| min_lifetime_days | INTEGER |  | - | 90 |
| max_lifetime_days | INTEGER |  | - | 365 |
| allow_per_room_override | BOOLEAN |  | - | TRUE |
| is_default | BOOLEAN |  | - | FALSE |
| created_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | - | - |
| CONSTRAINT | pk_server_retention_policy | ✅ | - | - |
| CONSTRAINT | uq_server_retention_policy_name |  | - | - |


### 3.19 安全审计 (5表)

**ip_blocks**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| ip_address | TEXT |  | ✅ | - |
| reason | TEXT |  | - | - |
| blocked_ts | BIGINT |  | ✅ | - |
| expires_at | BIGINT |  | - | - |
| CONSTRAINT | pk_ip_blocks | ✅ | - | - |
| CONSTRAINT | uq_ip_blocks_ip |  | - | - |

**ip_reputation**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| ip_address | TEXT |  | ✅ | - |
| score | INTEGER |  | - | 0 |
| last_seen_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | ✅ | - |
| details | JSONB |  | - | - |
| CONSTRAINT | pk_ip_reputation | ✅ | - | - |
| CONSTRAINT | uq_ip_reputation_ip |  | - | - |

**blocked_rooms**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL | ✅ | - | - |
| room_id | TEXT |  | ✅ | - |
| blocked_at | BIGINT |  | ✅ | - |
| blocked_by | TEXT |  | ✅ | - |
| reason | TEXT |  | - | - |

**blocked_users**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| user_id | TEXT |  | ✅ | - |
| blocked_id | TEXT |  | ✅ | - |
| reason | TEXT |  | - | - |
| created_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_blocked_users | ✅ | - | - |
| CONSTRAINT | uq_blocked_users_user_blocked |  | - | - |
| CONSTRAINT | fk_blocked_users_user |  | - | - |
| CONSTRAINT | fk_blocked_users_blocked |  | - | - |

**security_events**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| event_type | TEXT |  | ✅ | - |
| user_id | TEXT |  | - | - |
| ip_address | TEXT |  | - | - |
| user_agent | TEXT |  | - | - |
| details | JSONB |  | - | - |
| created_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_security_events | ✅ | - | - |


### 3.20 其他表 (12表)

**filters**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| user_id | TEXT |  | ✅ | - |
| filter_id | TEXT |  | ✅ | - |
| content | JSONB |  | ✅ | '{}' |
| created_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_filters | ✅ | - | - |
| CONSTRAINT | uq_filters_user_filter |  | - | - |

**search_index**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | SERIAL | ✅ | - | - |
| event_id | VARCHAR(255) |  | ✅ | - |
| room_id | VARCHAR(255) |  | ✅ | - |
| user_id | VARCHAR(255) |  | ✅ | - |
| event_type | VARCHAR(255) |  | ✅ | - |
| type | VARCHAR(255) |  | ✅ | - |
| content | TEXT |  | ✅ | - |
| created_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | - | - |
| CONSTRAINT | uq_search_index_event |  | - | - |

**sync_stream_id**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| stream_type | TEXT |  | - | - |
| last_id | BIGINT |  | - | 0 |
| updated_ts | BIGINT |  | - | - |
| CONSTRAINT | pk_sync_stream_id | ✅ | - | - |
| CONSTRAINT | uq_sync_stream_id_type |  | - | - |

**background_updates**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| update_name | TEXT |  | ✅ | - |
| job_name | TEXT |  | - | - |
| job_type | TEXT |  | - | - |
| description | TEXT |  | - | - |
| table_name | TEXT |  | - | - |
| column_name | TEXT |  | - | - |
| is_running | BOOLEAN |  | - | FALSE |
| status | TEXT |  | - | 'pending' |
| progress | JSONB |  | - | '{}' |
| total_items | INTEGER |  | - | 0 |
| processed_items | INTEGER |  | - | 0 |
| created_ts | BIGINT |  | - | - |
| started_ts | BIGINT |  | - | - |
| completed_ts | BIGINT |  | - | - |
| updated_ts | BIGINT |  | - | - |
| error_message | TEXT |  | - | - |
| retry_count | INTEGER |  | - | 0 |
| max_retries | INTEGER |  | - | 3 |
| batch_size | INTEGER |  | - | 100 |
| sleep_ms | INTEGER |  | - | 100 |
| depends_on | JSONB |  | - | '[]' |
| metadata | JSONB |  | - | '{}' |
| CONSTRAINT | pk_background_updates | ✅ | - | - |
| CONSTRAINT | uq_background_updates_name |  | - | - |

**db_metadata**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL | ✅ | - | - |
| value | TEXT |  | ✅ | - |
| created_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | ✅ | - |

**schema_migrations**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| version | TEXT |  | ✅ | - |
| name | TEXT |  | - | - |
| checksum | TEXT |  | - | - |
| applied_ts | BIGINT |  | - | - |
| execution_time_ms | BIGINT |  | - | - |
| success | BOOLEAN |  | ✅ | TRUE |
| description | TEXT |  | - | - |
| executed_at | TIMESTAMPTZ |  | - | NOW() |
| CONSTRAINT | pk_schema_migrations | ✅ | - | - |
| CONSTRAINT | uq_schema_migrations_version |  | - | - |

**sliding_sync_rooms**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL | ✅ | - | - |
| user_id | TEXT |  | ✅ | - |
| device_id | TEXT |  | ✅ | - |
| room_id | TEXT |  | ✅ | - |
| conn_id | TEXT |  | - | - |
| list_key | TEXT |  | - | - |
| bump_stamp | BIGINT |  | - | 0 |
| highlight_count | INTEGER |  | - | 0 |
| notification_count | INTEGER |  | - | 0 |
| is_dm | BOOLEAN |  | - | FALSE |
| is_encrypted | BOOLEAN |  | - | FALSE |
| is_tombstoned | BOOLEAN |  | - | FALSE |
| invited | BOOLEAN |  | - | FALSE |
| name | TEXT |  | - | - |
| avatar | TEXT |  | - | - |
| timestamp | BIGINT |  | - | 0 |
| created_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | ✅ | - |

**third_party_rule_results**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| rule_type | TEXT |  | ✅ | - |
| event_id | TEXT |  | - | - |
| room_id | TEXT |  | - | - |
| user_id | TEXT |  | - | - |
| is_allowed | BOOLEAN |  | - | TRUE |
| rule_details | JSONB |  | - | '{}' |
| created_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_third_party_rule_results | ✅ | - | - |

**spam_check_results**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| event_id | TEXT |  | ✅ | - |
| room_id | TEXT |  | ✅ | - |
| user_id | TEXT |  | ✅ | - |
| spam_score | REAL |  | - | 0 |
| is_spam | BOOLEAN |  | - | FALSE |
| check_details | JSONB |  | - | '{}' |
| created_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_spam_check_results | ✅ | - | - |

**report_rate_limits**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| user_id | TEXT |  | ✅ | - |
| report_count | INTEGER |  | - | 0 |
| is_blocked | BOOLEAN |  | - | FALSE |
| blocked_until | BIGINT |  | - | - |
| last_report_at | BIGINT |  | - | - |
| created_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | - | - |
| CONSTRAINT | pk_report_rate_limits | ✅ | - | - |
| CONSTRAINT | uq_report_rate_limits_user |  | - | - |

**rendezvous_session**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| session_id | TEXT |  | ✅ | - |
| user_id | TEXT |  | - | - |
| device_id | TEXT |  | - | - |
| status | TEXT |  | - | 'pending' |
| content | JSONB |  | - | '{}' |
| expires_ts | BIGINT |  | ✅ | - |
| created_ts | BIGINT |  | ✅ | - |
| updated_ts | BIGINT |  | - | - |
| CONSTRAINT | pk_rendezvous_session | ✅ | - | - |
| CONSTRAINT | uq_rendezvous_session_id |  | - | - |

**voice_messages**

| 列名 | 数据类型 | 主键 | 非空 | 默认值 |
|------|----------|------|------|--------|
| id | BIGSERIAL |  | - | - |
| event_id | TEXT |  | ✅ | - |
| user_id | TEXT |  | ✅ | - |
| room_id | TEXT |  | - | - |
| media_id | TEXT |  | - | - |
| duration_ms | INT |  | ✅ | - |
| waveform | TEXT |  | - | - |
| mime_type | VARCHAR(100) |  | - | - |
| file_size | BIGINT |  | - | - |
| transcription | TEXT |  | - | - |
| encryption | JSONB |  | - | - |
| is_processed | BOOLEAN |  | - | FALSE |
| processed_at | BIGINT |  | - | - |
| created_ts | BIGINT |  | ✅ | - |
| CONSTRAINT | pk_voice_messages | ✅ | - | - |
| CONSTRAINT | uq_voice_messages_event |  | - | - |



---

## 4. 索引设计

根据 CAPABILITY_STATUS_BASELINE_2026-04-02.md：

| 索引类型 | 数量 |
|----------|------|
| 主键索引 | 131+ |
| 外键索引 | 35+ |
| 唯一索引 | 50+ |
| 普通索引 | 350+ |
| **总计** | **478+** |

### 4.1 常用查询索引

| 索引名称 | 表名 | 字段 | 用途 |
|----------|------|------|------|
| users_pkey | users | user_id | 主键 |
| users_name_idx | users | username | 用户名查询 |
| events_room_idx | events | room_id | 房间事件查询 |
| events_ts_idx | events | origin_server_ts | 事件排序 |
| room_memberships_user_idx | room_memberships | user_id | 用户房间查询 |
| device_keys_user_idx | device_keys | user_id | 用户设备查询 |
| events_stream_order_idx | events | stream_ordering | 事件流排序 |

### 4.2 性能优化建议

```sql
-- 定期 VACUUM
VACUUM ANALYZE;

-- 监控慢查询
CREATE EXTENSION IF NOT EXISTS pg_stat_statements;

-- 考虑对 events 表按时间分区
```

---

## 5. 数据字典

### 5.1 字段命名规范 (根据 SKILL.md)

| 类型 | 命名规则 | 示例 |
|------|----------|------|
| 创建时间 | `{field}_ts` | created_ts, updated_ts |
| 过期时间 | `{field}_at` | expires_at |
| 标识符 | `{entity}_id` | user_id, room_id |
| 布尔值 | `is_xxx` | is_admin, is_valid |
| 外键 | `{table}_id` | device_id, event_id |

### 5.2 已修复的历史问题

根据 DATABASE_AUDIT_REPORT.md：

| 问题 | 修复日期 | 状态 |
|------|----------|------|
| events.processed_at → processed_ts | 2026-03-21 | ✅ |
| saml_logout_requests.processed_at → processed_ts | 2026-03-21 | ✅ |
| access_tokens.revoked_at → is_revoked | 2026-03-21 | ✅ |
| refresh_tokens.revoked_at → is_revoked | 2026-03-21 | ✅ |
| blocked_rooms 表缺失 | 2026-03-26 | ✅ |
| typing.is_typing → typing | 2026-03-26 | ✅ |
| room_directory.added_ts 未设置 | 2026-03-26 | ✅ |

---

## 6. 安全性考虑

### 6.1 数据加密

- 密码使用 bcrypt 哈希存储
- 敏感令牌存储加密
- E2E 密钥安全存储

### 6.2 访问控制

- 用户只能访问自己的数据
- 管理员权限严格控制
- 联邦服务器白名单

### 6.3 审计日志

- 记录所有敏感操作
- 安全事件追踪 (security_events 表)
- 登录日志保存

---

## 7. 维护建议

### 7.1 定期维护

| 任务 | 频率 |
|------|------|
| 清理过期令牌 | 每天 |
| 优化表统计信息 | 每周 |
| 清理过期会话 | 每月 |
| VACUUM ANALYZE | 每周 |

### 7.2 备份策略

- 全量备份 (每天)
- 增量备份 (每小时)
- 异地容灾

### 7.3 PostgreSQL 优化参数

根据 CAPABILITY_STATUS_BASELINE_2026-04-02.md：

| 参数 | 优化前 | 优化后 |
|------|--------|--------|
| shared_buffers | 128MB | 256MB |
| work_mem | 4MB | 16MB |
| random_page_cost | 4.0 | 1.1 |
| effective_io_concurrency | 1 | 200 |

---

## 8. 迁移脚本

### 8.1 部署命令

```bash
# 新环境
psql -U synapse -d synapse -f migrations/00000000_unified_schema_v6.sql
psql -U synapse -d synapse -f migrations/99999999_unified_incremental_migration.sql

# 现有环境升级
psql -U synapse -d synapse -f migrations/99999999_unified_incremental_migration.sql
```

### 8.2 迁移版本记录

| 版本 | 状态 | 执行时间 |
|------|------|----------|
| v6.0.0 | ✅ | 1774000207596 |
| UNIFIED_MIGRATION_v1 | ✅ | 1774000235981 |
| 20260321000001 | ✅ | 1774000235664 |
| 20260321000005 | ✅ | 1774013987143 |

---

## 9. 代码与 Schema 一致性验证

根据 DATABASE_AUDIT_REPORT.md，验证通过的 INSERT 语句：

| 表名 | 代码位置 | 状态 |
|------|----------|------|
| rooms | storage/room.rs:85 | ✅ |
| room_aliases | storage/room.rs:450 | ✅ |
| room_directory | storage/room.rs:600 | ✅ |
| room_account_data | storage/room.rs:635 | ✅ |
| read_markers | storage/room.rs:658 | ✅ |
| event_receipts | storage/room.rs:753 | ✅ |
| blocked_rooms | admin/room.rs:349 | ✅ |
| user_threepids | mod.rs:2183 | ✅ |
| presence | services/mod.rs:823 | ✅ |
| typing | services/mod.rs:866 | ✅ |
| event_relations | storage/relations.rs:66 | ✅ |
| pushers | push.rs:191 | ✅ |
| push_rules | push.rs:358 | ✅ |

---

## 10. SQL 生成模板

### 10.1 用户表

```sql
CREATE TABLE users (
    user_id VARCHAR(255) PRIMARY KEY,
    username VARCHAR(128) NOT NULL UNIQUE,
    password_hash VARCHAR(255),
    displayname VARCHAR(256),
    avatar_url VARCHAR(512),
    is_admin BOOLEAN DEFAULT FALSE,
    is_guest BOOLEAN DEFAULT FALSE,
    creation_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    password_expires_at BIGINT,
    locked_ts BIGINT,
    deactivation_ts BIGINT
);

CREATE INDEX users_name_idx ON users(username);
CREATE INDEX users_creation_idx ON users(creation_ts DESC);
```

### 10.2 房间表

```sql
CREATE TABLE rooms (
    room_id VARCHAR(255) PRIMARY KEY,
    name VARCHAR(256),
    topic VARCHAR(512),
    is_public BOOLEAN DEFAULT FALSE,
    join_rule VARCHAR(50) DEFAULT 'invite',
    guest_access VARCHAR(20) DEFAULT 'forbidden',
    history_visibility VARCHAR(20) DEFAULT 'shared',
    creator_id VARCHAR(255) NOT NULL,
    creation_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    deleted_ts BIGINT
);

CREATE INDEX rooms_creator_idx ON rooms(creator_id);
CREATE INDEX rooms_is_public_idx ON rooms(is_public);
```

---

## 11. 附录

### 11.1 参考文档

| 文档 | 路径 |
|------|------|
| SQL 表清单 | docs/db/sql_table_inventory.md |
| Rust 表清单 | docs/db/rust_table_inventory.md |
| 模型清单 | docs/db/rust_model_inventory.md |
| 字段映射报告 | docs/db/FIELD_MAPPING_REPORT.md |
| `CAPABILITY_STATUS_BASELINE_2026-04-02.md` | 正式能力状态基线 |
| `DATABASE_AUDIT_REPORT.md` | 审计报告 |

### 11.2 关键文件

| 文件 | 用途 |
|------|------|
| migrations/00000000_unified_schema_v6.sql | 基础 Schema |
| migrations/99999999_unified_incremental_migration.sql | 增量迁移 |
| migrations/DATABASE_FIELD_STANDARDS.md | 字段标准 |

---

*文档生成完成 - synapse-rust 数据库设计分析 v2.0*
*更新日期: 2026-03-26*
