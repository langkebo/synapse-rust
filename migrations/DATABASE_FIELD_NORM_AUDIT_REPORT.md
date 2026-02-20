# 数据库字段命名规范合规性审查报告

## 1. 审查概述

本报告基于 `DATABASE_FIELD_STANDARDS.md` 规范，对项目数据库脚本和代码进行全面合规性审查。

**审查日期**: 2026-02-20
**审查范围**: 
- 数据库迁移脚本 (`migrations/`)
- 数据库架构文档 (`db_schema_columns.txt`)
- Rust 源代码中的数据库相关代码

## 2. 发现的冲突与问题

### 2.1 布尔字段命名问题 (缺少 `is_` 前缀)

| 表名 | 当前字段名 | 规范字段名 | 严重程度 | 状态 |
|------|-----------|-----------|---------|------|
| users | `deactivated` | `is_deactivated` | 高 | 已修复 |
| users | `shadow_banned` | `is_shadow_banned` | 高 | 已修复 |
| application_services | `is_active` | `is_enabled` | 中 | 已修复 |
| application_service_statistics | `is_active` | `is_enabled` | 中 | 待修复 |
| federation_blacklist | `is_active` | `is_enabled` | 中 | 待修复 |
| media_quota_config | `is_active` | `is_enabled` | 中 | 已修复 |
| notification_templates | `is_active` | `is_enabled` | 中 | 待修复 |
| server_notifications | `is_active` | `is_enabled` | 中 | 已修复 |
| server_notifications | `is_dismissible` | `is_dismissable` | 低 | 已修复 |
| refresh_tokens | `invalidated` | `is_revoked` | 高 | 待修复 |
| account_data_callbacks | `enabled` | `is_enabled` | 中 | 待修复 |
| captcha_template | `enabled` | `is_enabled` | 中 | 待修复 |
| cross_signing_keys | `blocked` | `is_blocked` | 中 | 待修复 |
| device_keys | `blocked` | `is_blocked` | 中 | 待修复 |
| federation_blacklist_rule | `enabled` | `is_enabled` | 中 | 待修复 |
| media_callbacks | `enabled` | `is_enabled` | 中 | 待修复 |
| modules | `enabled` | `is_enabled` | 中 | 待修复 |
| password_auth_providers | `enabled` | `is_enabled` | 中 | 待修复 |
| presence_routes | `enabled` | `is_enabled` | 中 | 待修复 |
| push_rules | `enabled` | `is_enabled` | 中 | 待修复 |
| pushers | `enabled` | `is_enabled` | 中 | 待修复 |
| rate_limit_callbacks | `enabled` | `is_enabled` | 中 | 待修复 |
| saml_identity_providers | `enabled` | `is_enabled` | 中 | 待修复 |
| media_repository | `quarantined` | `is_quarantined` | 中 | 待修复 |
| space_children | `suggested` | `is_suggested` | 低 | 待修复 |
| registration_token | `is_active` | `is_enabled` | 中 | 待修复 |

### 2.2 时间字段命名问题

| 表名 | 当前字段名 | 规范字段名 | 严重程度 | 状态 |
|------|-----------|-----------|---------|------|
| access_tokens | `invalidated_ts` | `revoked_ts` | 高 | 已修复 |
| refresh_tokens | `expires_ts` | `expires_at` | 中 | 待修复 |
| account_data | `created_at` | `created_ts` | 低 | 待修复 |
| account_data | `updated_at` | `updated_ts` | 低 | 待修复 |
| devices | `created_at` | `created_ts` | 低 | 待修复 |
| federation_signing_keys | `created_at` | `created_ts` | 低 | 待修复 |
| blocked_rooms | `created_at` | `created_ts` | 低 | 待修复 |
| captcha_config | `created_at`/`updated_at` | `created_ts`/`updated_ts` | 低 | 待修复 |
| cas_services | `created_at`/`updated_at` | `created_ts`/`updated_ts` | 低 | 待修复 |
| device_keys | `created_at`/`updated_at` | `created_ts`/`updated_ts` | 低 | 待修复 |
| federation_access_stats | `created_at`/`updated_at` | `created_ts`/`updated_ts` | 低 | 待修复 |
| federation_blacklist | `created_at`/`updated_at` | `created_ts`/`updated_ts` | 低 | 待修复 |
| media_quota_alerts | `created_at` | `created_ts` | 低 | 待修复 |
| media_quota_config | `created_at`/`updated_at` | `created_ts`/`updated_ts` | 低 | 待修复 |
| media_repository | `created_at` | `created_ts` | 低 | 待修复 |
| media_thumbnails | `created_at` | `created_ts` | 低 | 待修复 |
| notification_templates | `created_at`/`updated_at` | `created_ts`/`updated_ts` | 低 | 待修复 |
| room_account_data | `created_at`/`updated_at` | `created_ts`/`updated_ts` | 低 | 待修复 |
| saml_identity_providers | `created_at`/`updated_at` | `created_ts`/`updated_ts` | 低 | 待修复 |
| server_notifications | `created_at`/`updated_at` | `created_ts`/`updated_ts` | 低 | 待修复 |
| user_media_quota | `created_at`/`updated_at` | `created_ts`/`updated_ts` | 低 | 待修复 |
| user_notification_status | `created_at` | `created_ts` | 低 | 待修复 |

### 2.3 冗余字段问题

| 表名 | 冗余字段 | 保留字段 | 严重程度 | 状态 |
|------|---------|---------|---------|------|
| access_tokens | `ip` | `ip_address` (需添加) | 中 | 待修复 |

### 2.4 数据类型不一致问题

| 表名 | 字段名 | 当前类型 | 规范类型 | 严重程度 | 状态 |
|------|-------|---------|---------|---------|------|
| captcha_config | `created_at` | `timestamp with time zone` | `BIGINT` | 中 | 待修复 |
| cas_services | `created_at` | `timestamp with time zone` | `BIGINT` | 中 | 待修复 |
| device_keys | `created_at` | `timestamp with time zone` | `BIGINT` | 中 | 待修复 |
| federation_access_stats | `created_at` | `timestamp with time zone` | `BIGINT` | 中 | 待修复 |
| media_quota_alerts | `created_at` | `timestamp with time zone` | `BIGINT` | 中 | 待修复 |
| media_quota_config | `created_at` | `timestamp with time zone` | `BIGINT` | 中 | 待修复 |
| notification_templates | `created_at` | `timestamp with time zone` | `BIGINT` | 中 | 待修复 |
| saml_identity_providers | `created_at` | `timestamp with time zone` | `BIGINT` | 中 | 待修复 |
| server_notifications | `created_at` | `timestamp with time zone` | `BIGINT` | 中 | 待修复 |
| user_media_quota | `created_at` | `timestamp with time zone` | `BIGINT` | 中 | 待修复 |

## 3. 数据库结构完整性审查

### 3.1 缺失的索引

| 表名 | 建议索引 | 原因 |
|------|---------|------|
| access_tokens | `idx_access_tokens_user_id` | 高频查询 user_id |
| access_tokens | `idx_access_tokens_expires_ts` | 过期令牌清理 |
| refresh_tokens | `idx_refresh_tokens_user_id` | 高频查询 user_id |
| refresh_tokens | `idx_refresh_tokens_expires_at` | 过期令牌清理 |
| events | `idx_events_room_id_origin_server_ts` | 房间事件查询 |
| events | `idx_events_sender` | 发送者事件查询 |
| room_memberships | `idx_room_memberships_user_id` | 用户房间列表查询 |
| devices | `idx_devices_user_id` | 用户设备查询 |
| pushers | `idx_pushers_user_id` | 用户推送配置查询 |

### 3.2 缺失的外键约束

| 表名 | 字段 | 参照表 | 参照字段 | 状态 |
|------|------|-------|---------|------|
| access_tokens | `user_id` | `users` | `user_id` | 待添加 |
| refresh_tokens | `user_id` | `users` | `user_id` | 待添加 |
| devices | `user_id` | `users` | `user_id` | 待添加 |
| room_memberships | `user_id` | `users` | `user_id` | 待添加 |
| room_memberships | `room_id` | `rooms` | `room_id` | 待添加 |
| events | `room_id` | `rooms` | `room_id` | 待添加 |
| events | `sender` | `users` | `user_id` | 待添加 |
| user_media_quota | `user_id` | `users` | `user_id` | 待添加 |
| user_media_quota | `quota_config_id` | `media_quota_config` | `id` | 待添加 |
| server_notifications | `created_by` | `users` | `user_id` | 待添加 |
| user_notification_status | `user_id` | `users` | `user_id` | 待添加 |
| user_notification_status | `notification_id` | `server_notifications` | `id` | 待添加 |

### 3.3 潜在性能问题

1. **大表缺少分区**: `events` 表可能增长到数百万行，建议按 `room_id` 或时间分区
2. **缺少复合索引**: 多个高频查询场景缺少复合索引
3. **JSONB 字段未优化**: 多个 JSONB 字段缺少 GIN 索引

## 4. 代码与数据库一致性审查

### 4.1 Rust 结构体字段与数据库不一致

| 文件 | 结构体 | 问题字段 | 状态 |
|------|-------|---------|------|
| storage/user.rs | `User` | `deactivated` -> `is_deactivated` | 已修复 |
| storage/user.rs | `User` | `shadow_banned` -> `is_shadow_banned` | 已修复 |
| storage/token.rs | `AccessToken` | `invalidated_ts` -> `revoked_ts` | 已修复 |
| storage/server_notification.rs | `ServerNotification` | `is_active` -> `is_enabled` | 已修复 |
| storage/server_notification.rs | `ServerNotification` | `is_dismissible` -> `is_dismissable` | 已修复 |
| storage/media_quota.rs | `MediaQuotaConfig` | `is_active` -> `is_enabled` | 已修复 |
| storage/application_service.rs | `ApplicationService` | `is_active` -> `is_enabled` | 已修复 |
| storage/registration_token.rs | `RegistrationToken` | `is_active` -> `is_enabled` | 待修复 |
| storage/federation_blacklist.rs | `FederationBlacklist` | `is_active` -> `is_enabled` | 待修复 |
| storage/mod.rs | 测试代码 | `deactivated`/`shadow_banned` | 已修复 |

### 4.2 SQL 查询字段名不一致

| 文件 | 函数 | 问题 | 状态 |
|------|------|------|------|
| storage/user.rs | 多个查询 | `deactivated` -> `is_deactivated` | 已修复 |
| storage/token.rs | 多个查询 | `invalidated_ts` -> `revoked_ts` | 已修复 |
| storage/server_notification.rs | 多个查询 | `is_active` -> `is_enabled` | 已修复 |
| storage/media_quota.rs | 多个查询 | `is_active` -> `is_enabled` | 已修复 |
| storage/application_service.rs | 多个查询 | `is_active` -> `is_enabled` | 已修复 |

## 5. 改进建议

### 5.1 高优先级 (立即处理)

1. **完成布尔字段规范化**: 将所有 `enabled`、`blocked`、`quarantined` 等字段添加 `is_` 前缀
2. **添加缺失的外键约束**: 确保数据完整性
3. **添加关键索引**: 提升查询性能

### 5.2 中优先级 (近期处理)

1. **统一时间字段类型**: 将所有 `timestamp with time zone` 转换为 `BIGINT` 毫秒时间戳
2. **统一时间字段后缀**: 将 `created_at` 改为 `created_ts`，`updated_at` 改为 `updated_ts`
3. **移除冗余字段**: 删除 `access_tokens.ip` 等冗余字段

### 5.3 低优先级 (长期优化)

1. **大表分区**: 对 `events` 等大表实施分区策略
2. **JSONB 索引优化**: 为频繁查询的 JSONB 字段添加 GIN 索引
3. **添加数据库文档**: 完善数据库设计文档

## 6. 迁移脚本建议

建议创建以下迁移脚本：

1. `20260220000001_normalize_boolean_fields.sql` - 布尔字段规范化
2. `20260220000002_normalize_time_fields.sql` - 时间字段规范化
3. `20260220000003_add_missing_indexes.sql` - 添加缺失索引
4. `20260220000004_add_foreign_keys.sql` - 添加外键约束
5. `20260220000005_remove_redundant_fields.sql` - 移除冗余字段

## 7. 审查结论

### 7.1 已完成修复

- ✅ 用户表 (`users`) 布尔字段规范化
- ✅ 访问令牌表 (`access_tokens`) 时间字段规范化
- ✅ 服务器通知表 (`server_notifications`) 字段规范化
- ✅ 媒体配额表 (`media_quota_config`) 字段规范化
- ✅ 应用服务表 (`application_services`) 字段规范化
- ✅ Rust 代码结构体字段更新

### 7.2 待处理项

- ⏳ 其他表的布尔字段规范化 (约 20+ 个表)
- ⏳ 时间字段类型统一 (约 20+ 个表)
- ⏳ 添加缺失索引 (约 10+ 个索引)
- ⏳ 添加外键约束 (约 12+ 个约束)
- ⏳ 移除冗余字段

### 7.3 风险评估

| 风险项 | 风险等级 | 说明 |
|-------|---------|------|
| 数据迁移失败 | 中 | 大量字段重命名可能导致迁移失败 |
| 应用兼容性 | 中 | 字段名变更需要同步更新所有应用代码 |
| 性能影响 | 低 | 索引添加可能短暂影响写入性能 |
| 数据完整性 | 低 | 外键约束添加需要确保现有数据一致性 |

## 8. 附录

### 8.1 规范参考

详见 `DATABASE_FIELD_STANDARDS.md`

### 8.2 相关文件

- `/home/hula/synapse_rust/synapse/migrations/DATABASE_FIELD_STANDARDS.md`
- `/home/hula/synapse_rust/synapse/migrations/db_schema_columns.txt`
- `/home/hula/synapse_rust/synapse/migrations/00000000_unified_schema.sql`
- `/home/hula/synapse_rust/synapse/migrations/20260220000000_normalize_field_names.sql`
