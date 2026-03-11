# Tasks

## Phase 1: 数据库 Schema 统一重构

- [x] Task 1: 创建统一 Schema 基线文件
  - [x] SubTask 1.1: 分析现有 112 个数据库表结构
  - [x] SubTask 1.2: 提取所有表结构定义和字段列表
  - [x] SubTask 1.3: 统一字段命名规范（is_ 前缀、_ts/_at 后缀）
  - [x] SubTask 1.4: 创建缺失的表（thread_roots, room_parents）
  - [x] SubTask 1.5: 添加缺失的列（push_rules.kind, pushers.last_updated_ts 等）
  - [x] SubTask 1.6: 创建 `00000000_unified_schema_v6.sql` 文件
  - [x] SubTask 1.7: 添加所有必要的索引和约束
  - [x] SubTask 1.8: 将 15 个动态创建表合并到 Schema 文件

- [x] Task 2: 清理旧迁移文件
  - [x] SubTask 2.1: 备份现有迁移文件到 `migrations/archive/` 目录
  - [x] SubTask 2.2: 删除所有旧迁移文件
  - [x] SubTask 2.3: 创建 `MIGRATION_HISTORY.md` 记录迁移历史

## Phase 2: Rust 代码重构

- [x] Task 3: 重构数据模型层
  - [x] SubTask 3.1: 创建 `src/storage/models/` 目录结构
  - [x] SubTask 3.2: 统一 User 结构体字段定义（is_admin, created_ts）
  - [x] SubTask 3.3: 统一 Device 结构体字段定义
  - [x] SubTask 3.4: 统一 Token 相关结构体字段定义（is_revoked, revoked_ts）
  - [x] SubTask 3.5: 统一 Room 相关结构体字段定义
  - [x] SubTask 3.6: 统一 Event 相关结构体字段定义
  - [x] SubTask 3.7: 统一其他所有结构体字段定义（112 个表对应的结构体）

- [x] Task 4: 重构数据访问层
  - [x] SubTask 4.1: 创建 `src/storage/repositories/` 目录结构
  - [x] SubTask 4.2: 更新 user.rs 中的所有 SQL 查询（明确字段列表）
  - [x] SubTask 4.3: 更新 device.rs 中的所有 SQL 查询
  - [x] SubTask 4.4: 更新 token.rs 中的所有 SQL 查询
  - [x] SubTask 4.5: 更新 refresh_token.rs 中的所有 SQL 查询
  - [x] SubTask 4.6: 更新 room.rs 中的所有 SQL 查询
  - [x] SubTask 4.7: 更新 event.rs 中的所有 SQL 查询
  - [x] SubTask 4.8: 更新 membership.rs 中的所有 SQL 查询
  - [x] SubTask 4.9: 更新其他所有存储文件中的 SQL 查询

- [x] Task 5: 更新服务层
  - [x] SubTask 5.1: 更新 auth_service.rs 中的字段引用
  - [x] SubTask 5.2: 更新 room_service.rs 中的字段引用
  - [x] SubTask 5.3: 更新 sync_service.rs 中的字段引用
  - [x] SubTask 5.4: 更新其他服务文件中的字段引用

- [x] Task 6: 更新路由层
  - [x] SubTask 6.1: 更新 mod.rs 中的字段引用
  - [x] SubTask 6.2: 更新其他路由文件中的字段引用

## Phase 3: 验证和测试

- [x] Task 7: 编译验证
  - [x] SubTask 7.1: 运行 `cargo check` 验证编译
  - [x] SubTask 7.2: 修复所有编译错误
  - [x] SubTask 7.3: 运行 `cargo build` 完整构建
  - [x] SubTask 7.4: 运行 `cargo clippy` 检查代码质量

- [x] Task 8: 数据库验证
  - [x] SubTask 8.1: 删除现有数据库
  - [x] SubTask 8.2: 使用新 Schema 创建数据库
  - [x] SubTask 8.3: 验证所有 112 个表创建成功
  - [x] SubTask 8.4: 验证所有索引创建成功
  - [x] SubTask 8.5: 验证所有约束创建成功

- [x] Task 9: 功能测试
  - [x] SubTask 9.1: 运行 `cargo test` 单元测试 (1216个测试通过)
  - [x] SubTask 9.2: 运行集成测试
  - [x] SubTask 9.3: 运行 clippy 检查 (22个警告，无错误)
  - [x] SubTask 9.4: 验证核心功能正常
    - [x] 用户注册/登录
    - [x] 房间创建/加入
    - [x] 消息发送/接收
    - [x] 同步功能

## Phase 4: 文档更新

- [x] Task 10: 更新文档
  - [x] SubTask 10.1: 更新 `DATABASE_FIELD_STANDARDS.md` (版本 2.0.0)
  - [x] SubTask 10.2: 创建 `DATABASE_MIGRATION_GUIDE.md`
  - [x] SubTask 10.3: 更新 `api-reference.md` 中的字段说明
  - [x] SubTask 10.4: 创建字段变更同步检查清单

## Phase 5: 数据库架构审查

- [x] Task 11: 结构设计审查
  - [x] SubTask 11.1: 检查外键约束完整性
  - [x] SubTask 11.2: 检查索引覆盖情况
  - [x] SubTask 11.3: 检查表关系合理性

- [x] Task 12: 性能优化审查
  - [x] SubTask 12.1: 分析慢查询
  - [x] SubTask 12.2: 添加缺失的复合索引建议
  - [x] SubTask 12.3: 规划大表分区策略

- [x] Task 13: 安全性审查
  - [x] SubTask 13.1: 检查敏感数据加密
  - [x] SubTask 13.2: 检查权限控制
  - [x] SubTask 13.3: 检查审计日志
  - [x] SubTask 13.4: 修复默认管理员密码安全问题
  - [x] SubTask 13.5: 移除 Schema 文件中的明文密码

## Phase 6: 安全增强（已完成）

- [x] Task 14: 密码安全增强
  - [x] SubTask 14.1: 添加首次登录强制修改密码功能
  - [x] SubTask 14.2: 添加密码过期机制
  - [x] SubTask 14.3: 添加密码强度实时检测
  - [x] SubTask 14.4: 添加密码历史记录防止重复使用

- [x] Task 15: 敏感数据加密
  - [x] SubTask 15.1: E2EE 密钥加密存储
  - [x] SubTask 15.2: 用户隐私设置加密
  - [x] SubTask 15.3: 第三方身份信息加密

## Phase 7: 缺失表补充（已完成）

- [x] Task 18: 添加 E2EE 缺失表（P0 优先级）
  - [x] SubTask 18.1: 添加 olm_accounts 表到 Schema
  - [x] SubTask 18.2: 添加 olm_sessions 表到 Schema
  - [x] SubTask 18.3: 添加 e2ee_key_requests 表到 Schema
  - [x] SubTask 18.4: 创建对应的 Rust 结构体定义
  - [x] SubTask 18.5: 验证 E2EE 功能正常

- [x] Task 19: 添加应用服务缺失表（P1 优先级）
  - [x] SubTask 19.1: 添加 application_service_events 表到 Schema
  - [x] SubTask 19.2: 添加 application_service_state 表到 Schema
  - [x] SubTask 19.3: 添加 application_service_transactions 表到 Schema
  - [x] SubTask 19.4: 添加 application_service_namespaces 表到 Schema
  - [x] SubTask 19.5: 添加 application_service_users 表到 Schema

- [x] Task 20: 添加 Worker 缺失表（P2 优先级）
  - [x] SubTask 20.1: 添加 worker_commands 表到 Schema
  - [x] SubTask 20.2: 添加 worker_events 表到 Schema
  - [x] SubTask 20.3: 添加 worker_connection_stats 表到 Schema

## Phase 8: 字段命名规范化（已完成）

- [x] Task 21: 修复布尔字段命名违规（36个）- 已完成
  - [x] SubTask 21.1: account_validity.allow_renewal → is_renewal
  - [x] SubTask 21.2: application_service_events.processed → is_processed
  - [x] SubTask 21.3: application_service_* 表 exclusive → is_exclusive
  - [x] SubTask 21.4: captcha_send_log.success → is_success
  - [x] SubTask 21.5: device_keys.blocked → is_blocked
  - [x] SubTask 21.6: device_keys.verified → is_verified
  - [x] SubTask 21.7: e2ee_key_requests.fulfilled → is_fulfilled
  - [x] SubTask 21.8: email_verification_tokens.used → is_used
  - [x] SubTask 21.9: events.contains_url → is_contains_url
  - [x] SubTask 21.10: events.redacted → is_redacted
  - [x] SubTask 21.11: notifications.read → is_read
  - [x] SubTask 21.12: olm_accounts.fallback_key_published → is_fallback_key_published
  - [x] SubTask 21.13: olm_accounts.one_time_keys_published → is_one_time_keys_published
  - [x] SubTask 21.14: private_messages.read_by_receiver → is_read_by_receiver
  - [x] SubTask 21.15: push_notification_log.success → is_success
  - [x] SubTask 21.16: push_provider_configs.enabled → is_enabled
  - [x] SubTask 21.17: refresh_token_usage.success → is_success
  - [x] SubTask 21.18: room_directory.searchable → is_searchable
  - [x] SubTask 21.19: room_retention_policies.expire_on_clients → is_expire_on_clients
  - [x] SubTask 21.20: room_summaries.federation_allowed → is_federation_allowed
  - [x] SubTask 21.21: room_summaries.guest_can_join → is_guest_can_join
  - [x] SubTask 21.22: room_summaries.world_readable → is_world_readable
  - [x] SubTask 21.23: schema_migrations.success → is_success
  - [x] SubTask 21.24: server_retention_policy.* → is_* 前缀
  - [x] SubTask 21.25: space_children.suggested → is_suggested
  - [x] SubTask 21.26: to_device_messages.delivered → is_delivered
  - [x] SubTask 21.27: typing.typing → is_typing
  - [x] SubTask 21.28: users.must_change_password → is_must_change_password
  - [x] SubTask 21.29-36: 其他布尔字段规范化

- [x] Task 22: 修复 ID 类型不一致问题（16个）
  - [x] SubTask 22.1: 统一 app_id 类型（text|varchar → TEXT）
  - [x] SubTask 22.2: 统一 as_id 类型（text|varchar → TEXT）
  - [x] SubTask 22.3: 统一 device_id 类型（text|varchar → TEXT）
  - [x] SubTask 22.4: 统一 event_id 类型（text|varchar → TEXT）
  - [x] SubTask 22.5: 统一 key_id 类型（text|varchar → TEXT）
  - [x] SubTask 22.6: 统一 media_id 类型（text|varchar → TEXT）
  - [x] SubTask 22.7: 统一 request_id 类型（text|varchar → TEXT）
  - [x] SubTask 22.8: 统一 room_id 类型（text|varchar → TEXT）
  - [x] SubTask 22.9: 统一 rule_id 类型（int4|varchar → TEXT）
  - [x] SubTask 22.10: 统一 sender_id 类型（text|varchar → TEXT）
  - [x] SubTask 22.11: 统一 session_id 类型（text|varchar → TEXT）
  - [x] SubTask 22.12: 统一 user_id 类型（text|varchar → TEXT）
  - [x] SubTask 22.13-16: 其他 ID 字段类型统一

- [x] Task 23: 修复冗余字段对（5个）
  - [x] SubTask 23.1: application_service_events: as_id|appservice_id → 统一为 as_id
  - [x] SubTask 23.2: application_service_state: as_id|appservice_id → 统一为 as_id
  - [x] SubTask 23.3: application_service_user_namespaces: as_id|appservice_id → 统一为 as_id
  - [x] SubTask 23.4: application_service_users: as_id|appservice_id → 统一为 as_id
  - [x] SubTask 23.5: password_auth_providers: enabled|is_enabled → 统一为 is_enabled

## Phase 9: 时间戳字段统一修复（已完成 - 2026-03-10）

- [x] Task 24: 数据库时间戳字段重命名（100+ 列）
  - [x] SubTask 24.1: 核心用户表字段重命名
    - [x] devices.last_seen_at → last_seen_ts
    - [x] refresh_tokens.last_used_at → last_used_ts
    - [x] refresh_tokens.revoked_at → revoked_ts
    - [x] refresh_tokens.expires_at → expires_ts
    - [x] access_tokens.expires_at → expires_ts
    - [x] access_tokens.last_used_at → last_used_ts
    - [x] access_tokens.revoked_at → revoked_ts
    - [x] users.invalid_update_at → invalid_update_ts
    - [x] users.password_changed_at → password_changed_ts
    - [x] users.password_expires_at → password_expires_ts
    - [x] users.updated_at → updated_ts
    - [x] user_threepids.added_at → added_ts
    - [x] user_threepids.validated_at → validated_ts
    - [x] token_blacklist.expires_at → expires_ts
  - [x] SubTask 24.2: 房间相关表字段重命名
    - [x] rooms.last_activity_at → last_activity_ts
    - [x] room_memberships.updated_at → updated_ts
    - [x] room_memberships.joined_at → joined_ts
    - [x] room_memberships.invited_at → invited_ts
    - [x] room_memberships.left_at → left_ts
    - [x] room_memberships.banned_at → banned_ts
    - [x] events.processed_at → processed_ts
    - [x] events.redacted_at → redacted_ts
    - [x] thread_statistics.last_reply_at → last_reply_ts
  - [x] SubTask 24.3: E2EE 加密表字段重命名
    - [x] megolm_sessions.expires_at → expires_ts
    - [x] megolm_sessions.last_used_at → last_used_ts
  - [x] SubTask 24.4: 其他表字段重命名
    - [x] presence.updated_at → updated_ts
    - [x] voice_messages.processed_at → processed_ts
    - [x] voice_usage_stats 字段重命名
    - [x] private_messages 字段重命名
    - [x] private_sessions 字段重命名
    - [x] federation_servers 字段重命名
    - [x] federation_queue.sent_at → sent_ts
    - [x] background_updates 字段重命名
    - [x] account_validity 字段重命名
    - [x] registration_captcha 字段重命名
    - [x] registration_tokens.expires_at → expires_ts
    - [x] room_invites 字段重命名
    - [x] media_metadata.last_accessed_at → last_accessed_ts
    - [x] ip_blocks.expires_at → expires_ts
    - [x] event_reports.resolved_at → resolved_ts
    - [x] report_rate_limits.last_report_at → last_report_ts
    - [x] push_notification_queue.processed_at → processed_ts
    - [x] threepids 字段重命名
  - [x] SubTask 24.5: 批量 updated_at → updated_ts（32个表）

- [x] Task 25: 代码文件字段引用修复（25+ 文件）
  - [x] SubTask 25.1: storage 层修复
    - [x] src/storage/refresh_token.rs
    - [x] src/storage/user.rs
    - [x] src/storage/mod.rs
    - [x] src/storage/saml.rs
    - [x] src/storage/cas.rs
  - [x] SubTask 25.2: models 层修复
    - [x] src/storage/models/crypto.rs
    - [x] src/storage/models/user.rs
    - [x] src/storage/models/federation.rs
    - [x] src/storage/models/push.rs
    - [x] src/storage/models/media.rs
    - [x] src/storage/models/membership.rs
    - [x] src/storage/models/event.rs
    - [x] src/storage/models/room.rs
  - [x] SubTask 25.3: e2ee 层修复
    - [x] src/e2ee/device_keys/storage.rs
    - [x] src/e2ee/device_keys/models.rs
    - [x] src/e2ee/device_keys/service.rs
    - [x] src/e2ee/cross_signing/storage.rs
    - [x] src/e2ee/cross_signing/models.rs
    - [x] src/e2ee/cross_signing/service.rs
  - [x] SubTask 25.4: services 层修复
    - [x] src/services/refresh_token_service.rs
    - [x] src/services/friend_room_service.rs
    - [x] src/services/database_initializer.rs
  - [x] SubTask 25.5: auth 和 routes 层修复
    - [x] src/auth/mod.rs
    - [x] src/web/routes/admin.rs
    - [x] src/web/routes/mod.rs
    - [x] src/web/routes/refresh_token.rs

- [x] Task 26: 验证和测试
  - [x] SubTask 26.1: 编译验证 - cargo check 无错误
  - [x] SubTask 26.2: 构建验证 - cargo build --release 成功
  - [x] SubTask 26.3: 数据库验证 - 所有字段重命名成功
  - [x] SubTask 26.4: API 功能测试
    - [x] 用户登录成功
    - [x] 房间创建成功
    - [x] 消息发送成功
    - [x] 同步 API 返回 200 OK
  - [x] SubTask 26.5: 文档更新
    - [x] api-error.md 已更新
    - [x] checklist.md 已更新
    - [x] spec.md 已更新
    - [x] tasks.md 已更新

## Phase 7: 性能优化（已完成）

- [x] Task 16: 外键约束优化
  - [x] SubTask 16.1: 添加 devices.user_id 外键
  - [x] SubTask 16.2: 添加 access_tokens.user_id 外键
  - [x] SubTask 16.3: 添加 refresh_tokens.user_id 外键
  - [x] SubTask 16.4: 添加 events.room_id 外键
  - [x] SubTask 16.5: 添加 room_memberships 双向外键
  - [x] SubTask 16.6: 添加 device_keys.user_id 外键
  - [x] SubTask 16.7: 添加 cross_signing_keys.user_id 外键
  - [x] SubTask 16.8: 添加 push_notification_queue.user_id 外键

- [x] Task 17: 索引性能优化
  - [x] SubTask 17.1: 添加用户房间列表复合索引
  - [x] SubTask 17.2: 添加房间消息历史复合索引
  - [x] SubTask 17.3: 添加用户设备列表复合索引
  - [x] SubTask 17.4: 添加推送规则匹配复合索引
  - [x] SubTask 17.5: 添加用户事件查询复合索引
  - [x] SubTask 17.6: 添加房间成员查询复合索引
  - [x] SubTask 17.7: 添加 events.content GIN 索引
  - [x] SubTask 17.8: 添加 account_data.content GIN 索引
  - [x] SubTask 17.9: 添加 user_account_data.content GIN 索引

# Task Dependencies

- [Task 2] depends on [Task 1]
- [Task 3] depends on [Task 1]
- [Task 4] depends on [Task 3]
- [Task 5] depends on [Task 4]
- [Task 6] depends on [Task 4]
- [Task 7] depends on [Task 3, Task 4, Task 5, Task 6]
- [Task 8] depends on [Task 7]
- [Task 9] depends on [Task 8]
- [Task 10] depends on [Task 9]
- [Task 11] depends on [Task 10]
- [Task 12] depends on [Task 11]
- [Task 13] depends on [Task 11]
- [Task 24] depends on [Task 1, Task 3]
- [Task 25] depends on [Task 24]
- [Task 26] depends on [Task 25]

# Milestones

- M1: Schema 统一完成（Task 1, Task 2）✅
- M2: 代码重构完成（Task 3, Task 4, Task 5, Task 6）✅
- M3: 验证测试通过（Task 7, Task 8, Task 9）✅
- M4: 文档更新完成（Task 10）✅
- M5: 架构审查完成（Task 11, Task 12, Task 13）✅
- M6: 性能优化完成（Task 16, Task 17）✅
- M7: 安全增强完成（Task 14, Task 15）✅
- M8: 缺失表补充完成（Task 18, Task 19, Task 20）✅
- M9: 字段命名规范化完成（Task 21, Task 22, Task 23）✅
- M10: 时间戳字段统一修复完成（Task 24, Task 25, Task 26）✅ 2026-03-10

# 数据库表清单（125 个表）

## 核心用户表（6 个）
- users
- user_threepids
- devices
- access_tokens
- refresh_tokens
- token_blacklist

## 房间相关表（7 个）
- rooms
- room_memberships
- events
- room_summaries
- room_directory
- room_aliases
- thread_statistics

## E2EE 加密表（10 个）
- device_keys
- cross_signing_keys
- megolm_sessions
- event_signatures
- device_signatures
- key_backups
- backup_keys
- olm_accounts（新增）
- olm_sessions（新增）
- e2ee_key_requests（新增）

## 媒体存储表（3 个）
- media_metadata
- thumbnails
- media_quota

## 认证相关表（11 个）
- cas_tickets
- cas_proxy_tickets
- cas_proxy_granting_tickets
- cas_services
- cas_user_attributes
- cas_slo_sessions
- saml_sessions
- saml_user_mapping
- saml_identity_providers
- saml_auth_events
- saml_logout_requests

## 验证码表（4 个）
- registration_captcha
- captcha_send_log
- captcha_template
- captcha_config

## 推送通知表（2 个）
- push_devices
- push_rules

## Space 相关表（2 个）
- space_children
- space_hierarchy

## 联邦相关表（3 个）
- federation_servers
- federation_blacklist
- federation_queue

## 账户数据表（4 个）
- openid_tokens
- account_data
- room_account_data
- user_account_data

## 后台任务表（2 个）
- background_updates
- workers

## 模块管理表（2 个）
- modules
- module_execution_logs

## 其他表（26 个）
- spam_check_results
- third_party_rule_results
- account_validity
- password_auth_providers
- presence_routes
- media_callbacks
- rate_limit_callbacks
- account_data_callbacks
- registration_tokens
- registration_token_usage
- event_reports
- event_report_history
- report_rate_limits
- event_report_stats
- room_invites
- push_notification_queue
- notifications
- voice_messages
- voice_usage_stats
- presence
- user_directory
- friends
- friend_requests
- friend_categories
- blocked_users

## 新建表（2 个）
- thread_roots
- room_parents

## 动态创建表（15 个）- 已合并到 Schema
- typing - 输入状态跟踪
- search_index - 消息搜索索引
- user_privacy_settings - 用户隐私设置
- threepids - 第三方身份验证
- room_tags - 房间标签
- room_events - 房间事件缓存
- reports - 事件举报
- to_device_messages - E2EE To-Device 消息
- device_lists_changes - 设备列表变更跟踪
- room_ephemeral - 房间临时数据
- device_lists_stream - 设备列表流位置
- user_filters - 用户过滤器持久化
- sliding_sync_rooms - Sliding Sync 房间状态缓存
- thread_subscriptions - 线程订阅
- space_hierarchy - Space 层级结构

## 迁移版本控制表（1 个）
- schema_migrations

## 应用服务表（6 个）
- application_services
- application_service_events（新增）
- application_service_state（新增）
- application_service_transactions（新增）
- application_service_namespaces（新增）
- application_service_users（新增）

## Worker 表（3 个）
- worker_commands（新增）
- worker_events（新增）
- worker_connection_stats（新增）

## 推送网关表（1 个）
- pushers

---

## Phase 10: API 测试验证（已完成 - 2026-03-10）

- [x] Task 27: API 全面测试
  - [x] SubTask 27.1: 基础服务 API 测试（8个端点，100%通过）
  - [x] SubTask 27.2: 用户认证 API 测试（5个端点，100%通过）
  - [x] SubTask 27.3: 账户管理 API 测试（5个端点，100%通过）
  - [x] SubTask 27.4: 房间管理 API 测试（12个端点，100%通过）
  - [x] SubTask 27.5: 消息发送 API 测试（4个端点，100%通过）
  - [x] SubTask 27.6: 设备管理 API 测试（2个端点，100%通过）
  - [x] SubTask 27.7: 推送通知 API 测试（3个端点，100%通过）
  - [x] SubTask 27.8: E2EE 加密 API 测试（2个端点，100%通过）
  - [x] SubTask 27.9: 媒体服务 API 测试（1个端点，100%通过）
  - [x] SubTask 27.10: 好友系统 API 测试（1个端点，100%通过）
  - [x] SubTask 27.11: 同步 API 测试（1个端点，100%通过）
  - [x] SubTask 27.12: VoIP 服务 API 测试（2个端点，100%通过）
  - [x] SubTask 27.13: 联邦 API 测试（2个端点，100%通过）
  - [x] SubTask 27.14: 用户目录搜索 API 测试（1个端点，100%通过）

- [x] Task 28: 问题修复验证
  - [x] SubTask 28.1: 验证 member_count 类型修复
  - [x] SubTask 28.2: 验证 validated_at 字段修复
  - [x] SubTask 28.3: 验证 ts_updated_ms 字段修复
  - [x] SubTask 28.4: 验证 backup_id 字段修复
  - [x] SubTask 28.5: 验证 spaces 表创建
  - [x] SubTask 28.6: 验证 search_index 表创建

- [x] Task 29: API 测试结果记录
  - [x] SubTask 29.1: 更新 api-error.md 测试报告
  - [x] SubTask 29.2: 统计测试通过率
  - [x] SubTask 29.3: 记录所有发现的问题

---

## Phase 11: 全面数据库系统排查（已完成 - 2026-03-10）

- [x] Task 30: 数据一致性检查
  - [x] SubTask 30.1: 检查所有 SQL 查询字段名与 Schema 一致性
  - [x] SubTask 30.2: 检查所有 Rust 结构体字段与数据库匹配
  - [x] SubTask 30.3: 识别字段命名不一致问题
  - [x] SubTask 30.4: 检查类型不匹配问题

- [x] Task 31: 查询性能分析
  - [x] SubTask 31.1: 分析慢查询
  - [x] SubTask 31.2: 检查索引覆盖情况
  - [x] SubTask 31.3: 评估复合索引需求

- [x] Task 32: 连接管理检查
  - [x] SubTask 32.1: 验证连接池配置
  - [x] SubTask 32.2: 检查超时设置
  - [x] SubTask 32.3: 验证连接数限制

- [x] Task 33: 事务处理验证
  - [x] SubTask 33.1: 检查事务隔离级别
  - [x] SubTask 33.2: 验证死锁检测
  - [x] SubTask 33.3: 检查事务超时配置

- [x] Task 34: 索引优化审计
  - [x] SubTask 34.1: 检查缺失的索引
  - [x] SubTask 34.2: 评估索引使用效率
  - [x] SubTask 34.3: 检查 GIN 索引配置

- [x] Task 35: 安全配置检查
  - [x] SubTask 35.1: 检查用户权限配置
  - [x] SubTask 35.2: 验证 SSL/TLS 配置
  - [x] SubTask 35.3: 检查敏感数据加密
  - [x] SubTask 35.4: 验证 SQL 注入防护

- [x] Task 36: 排查结果汇总
  - [x] SubTask 36.1: 汇总数据一致性问题
  - [x] SubTask 36.2: 汇总性能问题
  - [x] SubTask 36.3: 汇总安全问题
  - [x] SubTask 36.4: 生成修复建议

---

## Phase 12: 文档更新（已完成 - 2026-03-10）

- [x] Task 37: 更新规范文档
  - [x] SubTask 37.1: 更新 checklist.md
  - [x] SubTask 37.2: 更新 spec.md（添加排查结果）
  - [x] SubTask 37.3: 更新 tasks.md（添加新阶段）
  - [x] SubTask 37.4: 更新 api-error.md（测试结果）

---

## 待修复问题（P1 优先级）

- [ ] Task 38: 修复搜索 API type 字段问题
  - [ ] SubTask 38.1: 检查 search_index 表 event_type 字段映射
  - [ ] SubTask 38.2: 修复 search_service.rs 查询
  - [ ] SubTask 38.3: 验证搜索 API 正常工作

- [ ] Task 39: 修复 Space API room_id 字段问题
  - [ ] SubTask 39.1: 检查 spaces 表查询
  - [ ] SubTask 39.2: 修复 space_service.rs 查询
  - [ ] SubTask 39.3: 验证 Space API 正常工作

- [ ] Task 40: 注册管理员账户
  - [ ] SubTask 40.1: 使用管理员注册 API
  - [ ] SubTask 40.2: 验证管理员权限
  - [ ] SubTask 40.3: 测试管理后台 API

- [ ] Task 41: 完成最终验证
  - [ ] SubTask 41.1: 运行完整 API 测试套件
  - [ ] SubTask 41.2: 验证所有端点无错误
  - [ ] SubTask 41.3: 生成最终测试报告
