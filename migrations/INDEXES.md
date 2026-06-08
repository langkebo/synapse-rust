# 索引治理文档

> 版本: v1.0.0
> 更新日期: 2026-06-07
> 数据源: `migrations/00000000_unified_schema_v10.sql` 及 `src/services/database_initializer/tables.rs`

---

## 一、Partial Index 列表

Partial Index（部分索引）通过 `WHERE` 子句仅索引满足条件的行，可显著减少索引体积并提升特定查询性能。

| 表名 | 索引名 | 索引字段 | WHERE 条件 | 用途说明 |
|------|--------|----------|------------|----------|
| users | idx_users_must_change_password | must_change_password | must_change_password = TRUE | 查找需要修改密码的用户 |
| users | idx_users_password_expires | password_expires_at | password_expires_at IS NOT NULL | 查找密码即将过期的用户 |
| users | idx_users_locked | locked_until | locked_until IS NOT NULL | 查找被锁定的用户 |
| users | idx_users_name_trgm | name (GIN) | EXISTS (SELECT 1 FROM information_schema.columns WHERE ...) | 条件创建的 trigram 搜索索引 |
| access_tokens | idx_access_tokens_valid | is_revoked | is_revoked = FALSE | 查找有效的 access token |
| access_tokens | idx_access_tokens_user_revoked | user_id, is_revoked | is_revoked = FALSE | 按用户查找有效 token |
| access_tokens | idx_access_tokens_device_id | device_id | device_id IS NOT NULL | 按设备查找 token（排除空设备） |
| access_tokens | idx_access_tokens_expires | expires_at | is_revoked = FALSE | 查找未撤销且有过期时间的 token |
| refresh_tokens | idx_refresh_tokens_revoked | is_revoked | is_revoked = FALSE | 查找有效的 refresh token |
| refresh_tokens | idx_refresh_tokens_device_id | device_id | device_id IS NOT NULL | 按设备查找 refresh token |
| token_blacklist | idx_token_blacklist_user_id | user_id | user_id IS NOT NULL | 按用户查找黑名单 token |
| rooms | idx_rooms_creator | creator | creator IS NOT NULL | 按创建者查找房间 |
| rooms | idx_rooms_is_public | is_public | is_public = TRUE | 查找公开房间 |
| rooms | idx_rooms_last_activity | last_activity_ts DESC | last_activity_ts IS NOT NULL | 按最近活跃时间排序房间 |
| room_memberships | idx_room_memberships_joined | user_id, room_id | membership = 'join' | 查找已加入的成员关系 |
| events | idx_events_not_redacted | room_id, origin_server_ts DESC | is_redacted = FALSE | 查找未删除事件（按房间+时间） |
| events | idx_events_type_state | room_id, event_type, state_key | event_type LIKE 'm.room.%' | 查找房间状态事件 |
| events | idx_events_room_stream_ordering_not_redacted | room_id, stream_ordering DESC | is_redacted = FALSE | 按流序号查找未删除事件 |
| events | idx_events_friend_room | sender, room_id, origin_server_ts DESC | event_type = 'm.room.create' AND content->>'type' = 'm.friends' | 查找好友房间创建事件 |
| events | idx_events_friend_list | room_id, origin_server_ts DESC | event_type = 'm.friends.list' AND state_key = '' | 查找好友列表事件 |
| room_summaries | idx_room_summaries_space | is_space | is_space = TRUE | 查找 Space 类型的房间摘要 |
| room_directory | idx_room_directory_public | is_public | is_public = TRUE | 查找公开房间目录 |
| room_invites | uq_room_invites_invite_code | invite_code (UNIQUE) | invite_code IS NOT NULL | 邀请码唯一约束（排除空值） |
| room_retention_policies | idx_room_retention_policies_server_default | is_server_default | is_server_default = TRUE | 查找服务器默认保留策略 |
| device_keys | idx_device_keys_fallback | user_id, device_id | is_fallback = TRUE | 查找回退设备密钥 |
| megolm_sessions | idx_megolm_sessions_pickle_format | pickle_format | pickle_format = 'legacy' | 查找旧格式 Megolm 会话（懒迁移） |
| olm_sessions | idx_olm_sessions_expires | expires_at | expires_at IS NOT NULL | 查找有过期时间的 Olm 会话 |
| e2ee_key_requests | idx_e2ee_key_requests_pending | is_fulfilled | is_fulfilled = FALSE | 查找未完成的密钥请求 |
| device_verification_request | idx_device_verification_request_user_device_pending | user_id, new_device_id | status = 'pending' | 查找待处理的设备验证请求 |
| device_verification_request | idx_device_verification_request_expires_pending | expires_at | status = 'pending' | 查找待处理且有过期时间的验证请求 |
| one_time_keys | idx_one_time_keys_used | is_used | is_used = FALSE | 查找未使用的 OTK |
| dehydrated_devices | idx_dehydrated_devices_expires | expires_at | expires_at IS NOT NULL | 查找有过期时间的脱水设备 |
| leak_alerts | idx_leak_alerts_acknowledged | is_acknowledged | is_acknowledged = FALSE | 查找未确认的泄露告警 |
| user_media_quota | idx_user_media_quota_used | used_bytes DESC | used_bytes > 0 | 查找有使用量的媒体配额 |
| media_quota_config | idx_media_quota_config_enabled | is_enabled | is_enabled = TRUE | 查找启用的配额配置 |
| media_quota_alerts | idx_media_quota_alerts_user | user_id | is_read = FALSE | 查找未读的配额告警 |
| media_quota_alerts | idx_media_quota_alerts_is_read | is_read | is_read = FALSE | 查找未读告警 |
| upload_progress | idx_upload_progress_user_created_active | user_id, created_ts DESC | status <> 'finalized' | 查找进行中的上传任务 |
| push_device | idx_push_device_user_enabled | user_id | is_enabled = TRUE | 查找启用的推送设备 |
| pushers | idx_pushers_enabled | is_enabled | is_enabled = TRUE | 查找启用的推送器 |
| spaces | idx_spaces_public | is_public | is_public = TRUE | 查找公开 Space |
| spaces | idx_spaces_parent | parent_space_id | parent_space_id IS NOT NULL | 查找有父级 Space 的条目 |
| federation_blacklist_config | idx_federation_blacklist_config_enabled | is_enabled | is_enabled = TRUE | 查找启用的联邦黑名单配置 |
| federation_blacklist_rule | idx_federation_blacklist_rule_enabled | is_enabled | is_enabled = TRUE | 查找启用的联邦黑名单规则 |
| federation_queue | idx_federation_queue_pending | destination, created_ts | status = 'pending' | 查找待发送的联邦消息 |
| threepid_validation_session | idx_threepid_session_expires | expires_at | is_validated = FALSE | 查找未验证且有过期时间的会话 |
| threepid_validation_session | idx_threepid_session_expires_v8 | expires_at | is_validated = FALSE | 同上（v8 兼容） |
| background_updates | idx_background_updates_running | is_running | is_running = TRUE | 查找正在运行的后台更新 |
| background_updates | idx_background_updates_running_job | job_name, started_ts | status = 'running' | 查找运行中的后台任务 |
| background_updates | idx_background_updates_pending | status, job_type, created_ts | status IN ('pending', 'scheduled') | 查找待处理的后台更新 |
| workers | idx_workers_heartbeat | last_heartbeat_ts | last_heartbeat_ts IS NOT NULL | 查找有心跳的 Worker |
| burn_after_read_pending | idx_burn_pending_delete_ts | delete_ts | is_processed = FALSE | 查找未处理的阅后即焚删除任务 |
| megolm_session_keys | idx_megolm_session_keys_expiry | expires_at | expires_at IS NOT NULL | 查找有过期时间的 Megolm 会话密钥 |
| feature_flags | idx_feature_flags_expires_at | expires_at | expires_at IS NOT NULL | 查找有过期时间的特性标志 |
| refresh_token_families | idx_refresh_token_families_device | device_id | device_id IS NOT NULL | 按设备查找 token 族 |
| registration_tokens | idx_registration_tokens_expires | expires_at | expires_at IS NOT NULL | 查找有过期时间的注册令牌 |
| registration_tokens | idx_registration_tokens_enabled | is_enabled | is_enabled = TRUE | 查找启用的注册令牌 |
| registration_token_batches | idx_registration_token_batches_enabled_created | created_ts DESC | is_enabled = TRUE | 查找启用的令牌批次 |
| application_services | idx_application_services_enabled | is_enabled | is_enabled = TRUE | 查找启用的应用服务 |
| application_service_transactions | idx_application_service_transactions_processed | is_processed | is_processed = FALSE | 查找未处理的应用服务事务 |
| presence | idx_presence_last_active_ts | last_active_ts | last_active_ts IS NOT NULL | 查找有活跃时间的在线状态 |
| rendezvous_sessions | idx_rendezvous_sessions_expires | expires_at | expires_at IS NOT NULL | 查找有过期时间的 Rendezvous 会话 |
| qr_login_codes | idx_qr_login_codes_expires | expires_at | expires_at IS NOT NULL | 查找有过期时间的二维码登录码 |
| typing_stream | idx_typing_stream_active | room_id, is_typing | is_typing = TRUE | 查找正在输入的房间 |
| user_locks | idx_user_locks_user_active | user_id, is_active (UNIQUE) | is_active = TRUE | 用户活跃锁定唯一约束 |
| user_locks | idx_user_locks_active | is_active, created_ts DESC | is_active = TRUE | 查找活跃锁定记录 |
| rooms_summaries_mv | idx_rooms_summaries_mv_public_activity | is_public, joined_members DESC, last_activity_ts DESC | is_public = TRUE | 物化视图：公开房间排序 |
| key_rotation_pending | idx_key_rotation_pending_unprocessed | user_id | processed = FALSE | 查找未处理的密钥轮换任务（Rust 代码定义） |

---

## 二、复合索引列表

复合索引（多字段索引）用于优化多条件查询，字段顺序按查询频率和选择性排列。

| 表名 | 索引名 | 索引字段 | 是否唯一 | 用途说明 |
|------|--------|----------|----------|----------|
| matrixrtc_sessions | idx_matrixrtc_sessions_room_session | room_id, session_id | UNIQUE | MatrixRTC 会话唯一标识 |
| matrixrtc_memberships | idx_matrixrtc_memberships_room_session_user_device | room_id, session_id, user_id, device_id | UNIQUE | MatrixRTC 成员唯一标识 |
| matrixrtc_encryption_keys | idx_matrixrtc_encryption_keys_room_session_idx | room_id, session_id, key_index | UNIQUE | MatrixRTC 加密密钥唯一标识 |
| user_threepids | idx_user_threepids_medium_address | medium, address | 否 | 按媒介类型和地址查找三方 ID |
| access_tokens | idx_access_tokens_user_id | user_id | 否 | 按用户查找 access token |
| access_tokens | idx_access_tokens_user_revoked | user_id, is_revoked | 否 | 按用户和撤销状态查找 token（Partial） |
| refresh_tokens | idx_refresh_tokens_user_id | user_id | 否 | 按用户查找 refresh token |
| room_memberships | idx_room_memberships_user_membership | user_id, membership | 否 | 按用户和成员状态查询 |
| room_memberships | idx_room_memberships_room_membership | room_id, membership | 否 | 按房间和成员状态查询 |
| room_memberships | idx_room_memberships_user_status | user_id, membership, joined_ts DESC | 否 | 按用户状态和时间查询成员 |
| room_memberships | idx_room_memberships_room_status | room_id, membership | 否 | 按房间和状态查询成员 |
| room_memberships | idx_room_memberships_room_user | room_id, user_id | 否 | 按房间和用户查询成员 |
| room_memberships | idx_memberships_user_room | user_id, room_id | 否 | 按用户和房间查询成员 |
| room_memberships | idx_room_memberships_joined | user_id, room_id | 否 | 查找已加入的成员（Partial） |
| events | idx_events_room_time | room_id, origin_server_ts DESC | 否 | 按房间和时间范围查询事件 |
| events | idx_events_sender_type | sender, event_type | 否 | 按发送者和事件类型查询 |
| events | idx_events_sender_time | sender, origin_server_ts DESC | 否 | 按发送者和时间查询事件 |
| events | idx_events_room_stream_ordering | room_id, stream_ordering DESC | 否 | 按房间和流序号查询事件 |
| events | idx_events_sync_covering | room_id, stream_ordering DESC INCLUDE (event_id, sender, event_type, content, origin_server_ts) | 否 | 覆盖索引：同步查询优化 |
| events | idx_events_not_redacted | room_id, origin_server_ts DESC | 否 | 未删除事件查询（Partial） |
| events | idx_events_type_state | room_id, event_type, state_key | 否 | 房间状态事件查询（Partial） |
| events | idx_events_room_stream_ordering_not_redacted | room_id, stream_ordering DESC | 否 | 未删除事件流序号查询（Partial） |
| events | idx_events_friend_room | sender, room_id, origin_server_ts DESC | 否 | 好友房间事件查询（Partial） |
| events | idx_events_friend_list | room_id, origin_server_ts DESC | 否 | 好友列表事件查询（Partial） |
| event_relations | idx_event_relations_unique | event_id, relation_type, sender | UNIQUE | 事件关系唯一约束 |
| event_relations | idx_event_relations_room_event | room_id, relates_to_event_id, relation_type | 否 | 按房间和关联事件查询关系 |
| event_relations | idx_event_relations_sender | sender, relation_type | 否 | 按发送者和关系类型查询 |
| event_relations | idx_event_relations_origin_ts | room_id, origin_server_ts DESC | 否 | 按房间和时间查询关系 |
| room_summary_members | idx_room_summary_members_user_membership_room | user_id, membership, room_id | 否 | 按用户成员状态查询摘要 |
| room_summary_members | idx_room_summary_members_room_membership_hero_active | room_id, membership, is_hero DESC, last_active_ts DESC | 否 | 按房间和活跃状态查询摘要成员 |
| room_summary_members | idx_room_summary_members_room_hero_user | room_id, is_hero DESC, user_id | 否 | 按房间和 Hero 用户查询 |
| room_summary_members | idx_room_summary_members_room_user | room_id, user_id | 否 | 按房间和用户查询摘要成员 |
| room_summary_update_queue | idx_room_summary_update_queue_status_priority_created | status, priority DESC, created_ts ASC | 否 | 按状态和优先级处理更新队列 |
| room_invite_blocklist | idx_room_invite_blocklist_room_user | room_id, user_id | 否 | 按房间和用户查询邀请黑名单 |
| room_invite_allowlist | idx_room_invite_allowlist_room_user | room_id, user_id | 否 | 按房间和用户查询邀请白名单 |
| room_sticky_events | idx_room_sticky_events_user_sticky | user_id, is_sticky, room_id | 否 | 按用户和置顶状态查询事件 |
| device_keys | idx_device_keys_user_device | user_id, device_id | 否 | 按用户和设备查找密钥 |
| device_trust_status | idx_device_trust_status_user_level | user_id, trust_level | 否 | 按用户和信任级别查询 |
| cross_signing_trust | idx_cross_signing_trust_user_trusted | user_id, is_trusted | 否 | 按用户和信任状态查询 |
| key_signatures | idx_key_signatures_target | target_user_id, target_key_id | 否 | 按目标用户和密钥 ID 查询签名 |
| key_rotation_log | idx_key_rotation_log_user_rotated | user_id, rotated_at DESC | 否 | 按用户和轮换时间查询日志 |
| e2ee_security_events | idx_e2ee_security_events_user_created | user_id, created_ts DESC | 否 | 按用户和时间查询安全事件 |
| verification_requests | idx_verification_requests_to_user_state | to_user, state, updated_ts DESC | 否 | 按目标用户和状态查询验证请求 |
| olm_sessions | idx_olm_sessions_user_device | user_id, device_id | 否 | 按用户和设备查找 Olm 会话 |
| device_verification_request | idx_device_verification_request_user_device_pending | user_id, new_device_id | 否 | 按用户和设备查找待处理验证（Partial） |
| one_time_keys | idx_one_time_keys_user_device | user_id, device_id | 否 | 按用户和设备查找 OTK |
| e2ee_audit_log | idx_e2ee_audit_log_user_created | user_id, created_ts DESC | 否 | 按用户和时间查询审计日志 |
| e2ee_stored_secrets | idx_e2ee_stored_secrets_user_name | user_id, secret_name | UNIQUE | 存储密钥唯一约束 |
| voice_messages | idx_voice_messages_room_ts | room_id, created_ts DESC | 否 | 按房间和时间查询语音消息 |
| voice_messages | idx_voice_messages_user_ts | user_id, created_ts DESC | 否 | 按用户和时间查询语音消息 |
| voice_usage_stats | idx_voice_usage_stats_user | user_id, created_ts DESC | 否 | 按用户和时间查询语音统计 |
| voice_usage_stats | idx_voice_usage_stats_room | room_id, created_ts DESC | 否 | 按房间和时间查询语音统计 |
| push_rules | idx_push_rules_user_priority | user_id, priority | 否 | 按用户和优先级查询推送规则 |
| upload_chunks | idx_upload_chunks_upload_order | upload_id, chunk_index ASC | 否 | 按上传 ID 和分片顺序查询 |
| space_events | idx_space_events_space_type_ts | space_id, event_type, origin_server_ts DESC | 否 | 按 Space、类型和时间查询事件 |
| space_events | idx_space_events_space_ts | space_id, origin_server_ts DESC | 否 | 按 Space 和时间查询事件 |
| federation_queue | idx_federation_queue_dest_status | destination, status, created_ts | 否 | 按目标和状态查询联邦队列 |
| federation_queue | idx_federation_queue_pending | destination, created_ts | 否 | 待发送联邦消息查询（Partial） |
| destination_retry_timings | idx_destination_retry_next | retry_last_ts, failure_count | 否 | 重试时间查询 |
| account_data | idx_account_data_user_type | user_id, data_type | 否 | 按用户和数据类型查询账户数据 |
| background_updates | idx_background_updates_running_job | job_name, started_ts | 否 | 运行中任务查询（Partial） |
| background_updates | idx_background_updates_pending | status, job_type, created_ts | 否 | 待处理任务查询（Partial） |
| background_update_history | idx_background_update_history_job_start | job_name, execution_start_ts DESC | 否 | 按任务名和执行时间查询历史 |
| worker_task_assignments | idx_worker_task_assignments_status_priority_created | status, priority DESC, created_ts ASC | 否 | 按状态和优先级查询任务分配 |
| worker_task_assignments | idx_worker_task_assignments_worker_status | assigned_worker_id, status | 否 | 按 Worker 和状态查询任务 |
| modules | idx_modules_type_enabled_priority | module_type, is_enabled, priority | 否 | 按类型和启用状态查询模块 |
| module_execution_logs | idx_module_execution_logs_module_name_executed | module_name, executed_ts DESC | 否 | 按模块名和执行时间查询日志 |
| spam_check_results | idx_spam_results_sender_checked | sender, checked_ts DESC | 否 | 按发送者和检查时间查询 |
| third_party_rule_results | idx_third_party_results_event_checked | event_id, checked_ts DESC | 否 | 按事件和检查时间查询 |
| media_callbacks | idx_media_callbacks_type_enabled | callback_type, is_enabled | 否 | 按回调和启用状态查询 |
| audit_events | idx_audit_events_actor_created | actor_id, created_ts DESC | 否 | 按操作者和时间查询审计 |
| audit_events | idx_audit_events_resource_created | resource_type, resource_id, created_ts DESC | 否 | 按资源和时间查询审计 |
| audit_events | idx_audit_events_request_created | request_id, created_ts DESC | 否 | 按请求 ID 和时间查询审计 |
| feature_flags | idx_feature_flags_scope_status | target_scope, status, updated_ts DESC | 否 | 按范围和状态查询特性标志 |
| feature_flag_targets | idx_feature_flag_targets_lookup | flag_key, subject_type, subject_id | 否 | 按标志和主体查询目标 |
| state_group_state | idx_state_group_state_group_type_key | state_group_id, event_type, state_key | 否 | 按状态组和类型查询状态 |
| friend_requests | idx_friend_requests_receiver_status | receiver_id, status, created_ts DESC | 否 | 按接收者和状态查询好友请求 |
| friend_requests | idx_friend_requests_sender_status | sender_id, status, created_ts DESC | 否 | 按发送者和状态查询好友请求 |
| sliding_sync_lists | idx_sliding_sync_lists_unique | user_id, device_id, COALESCE(conn_id, ''), list_key | UNIQUE | Sliding Sync 列表唯一约束 |
| sliding_sync_lists | idx_sliding_sync_lists_user_device | user_id, device_id | 否 | 按用户和设备查询同步列表 |
| sliding_sync_tokens | idx_sliding_sync_tokens_unique | user_id, device_id, COALESCE(conn_id, '') | UNIQUE | Sliding Sync Token 唯一约束 |
| sliding_sync_tokens | idx_sliding_sync_tokens_user | user_id, device_id | 否 | 按用户和设备查询同步 Token |
| sliding_sync_rooms | idx_sliding_sync_rooms_unique | user_id, device_id, room_id, COALESCE(conn_id, '') | UNIQUE | Sliding Sync 房间唯一约束 |
| sliding_sync_rooms | idx_sliding_sync_rooms_user_device | user_id, device_id | 否 | 按用户和设备查询同步房间 |
| to_device_messages | idx_to_device_recipient | recipient_user_id, recipient_device_id | 否 | 按收件人和设备查询 To-Device 消息 |
| to_device_messages | idx_to_device_stream | recipient_user_id, stream_id | 否 | 按收件人和流 ID 查询 To-Device 消息 |
| room_account_data | idx_room_account_data_user_room | user_id, room_id | 否 | 按用户和房间查询账户数据 |
| read_markers | idx_read_markers_user_room | user_id, room_id | 否 | 按用户和房间查询已读标记 |
| lazy_loaded_members | idx_lazy_loaded_members_user_room | user_id, room_id | 否 | 按用户和房间查询懒加载成员 |
| thread_subscriptions | idx_thread_subscriptions_room_thread | room_id, thread_id | 否 | 按房间和线程查询订阅 |
| thread_read_receipts | idx_thread_read_receipts_user_room | user_id, room_id | 否 | 按用户和房间查询线程已读回执 |
| megolm_session_keys | idx_megolm_session_keys_lookup | user_id, session_id | 否 | 按用户和会话查找 Megolm 密钥 |
| quarantined_media_changes | idx_quarantined_media_changes_media | media_id, server_name | 否 | 按媒体和服务器查询隔离变更 |
| rooms_summaries_mv | idx_rooms_summaries_mv_creator | creator, created_ts DESC | 否 | 物化视图：按创建者查询 |
| rooms_summaries_mv | idx_rooms_summaries_mv_members | joined_members DESC, last_activity_ts DESC | 否 | 物化视图：按成员数排序 |
| public_room_directory | idx_public_room_directory_members | joined_members DESC, last_event_ts DESC | 否 | 物化视图：公开房间按成员排序 |

---

## 三、索引设计原则

### 3.1 主键索引

- 自动创建，使用 `BIGSERIAL` 或 `TEXT PRIMARY KEY`
- 无需手动创建主键索引

### 3.2 唯一索引

- 用于业务唯一约束字段（如 `token_hash`、`room_alias`、`invite_code`）
- 对可为空的唯一字段，使用 Partial Index 排除 NULL 值：`WHERE column IS NOT NULL`

### 3.3 Partial Index（部分索引）

- **核心原则**：仅索引满足特定条件的行，减少索引体积
- **适用场景**：
  - 布尔字段筛选：`WHERE is_revoked = FALSE`、`WHERE is_enabled = TRUE`
  - 可空字段非空筛选：`WHERE expires_at IS NOT NULL`、`WHERE device_id IS NOT NULL`
  - 状态筛选：`WHERE status = 'pending'`、`WHERE is_processed = FALSE`
  - 特定事件类型：`WHERE event_type = 'm.room.create' AND content->>'type' = 'm.friends'`
- **命名约定**：索引名应体现筛选条件语义，如 `idx_xxx_enabled`、`idx_xxx_pending`

### 3.4 复合索引

- **字段顺序**：按查询频率和选择性排列，高选择性字段在前
- **排序方向**：与查询 `ORDER BY` 方向一致
- **覆盖索引**：对高频查询使用 `INCLUDE` 子句避免回表
  ```sql
  CREATE INDEX idx_events_sync_covering ON events(room_id, stream_ordering DESC)
      INCLUDE (event_id, sender, event_type, content, origin_server_ts);
  ```

### 3.5 GIN 索引

- 用于全文搜索和 trigram 模糊匹配
- 使用 `jsonb_path_ops` 减少索引体积：`USING GIN (content jsonb_path_ops)`
- 使用 `gin_trgm_ops` 支持模糊搜索：`USING GIN (username gin_trgm_ops)`

### 3.6 必需索引

以下表必须创建复合索引以保证查询性能：

| 表名 | 索引字段 | 说明 |
|------|----------|------|
| presence_subscriptions | (subscriber_id, target_id) | 在线状态订阅查询（主键） |
| events | (room_id, origin_server_ts DESC) | 时间范围查询 |
| room_memberships | (user_id, membership) | 成员状态查询 |
| access_tokens | (user_id, is_revoked) | Token 验证查询 |

---

## 四、索引维护指南

### 4.1 何时添加索引

1. **新查询性能问题**：通过 `EXPLAIN ANALYZE` 确认存在全表扫描或低效查询
2. **新表创建**：在迁移文件中同步创建必要索引
3. **新功能上线**：评估新功能的查询模式，提前创建索引
4. **数据量增长**：当表数据量显著增长导致查询变慢时

### 4.2 添加索引的步骤

1. 在迁移文件中使用 `CREATE INDEX IF NOT EXISTS`
2. 更新本文档，记录索引信息
3. 在测试环境验证索引效果（使用 `EXPLAIN ANALYZE`）
4. 评估对写入性能的影响

### 4.3 何时删除索引

1. **索引未被使用**：通过 `pg_stat_user_indexes` 确认 idx_scan 长期为 0
2. **查询模式变更**：原查询不再使用该索引
3. **重复索引**：新索引完全覆盖旧索引的查询场景
4. **写入性能影响**：索引维护成本超过查询收益

### 4.4 删除索引的步骤

1. 确认索引无查询依赖（检查代码和查询日志）
2. 在迁移文件中使用 `DROP INDEX IF EXISTS`
3. 更新本文档，移除索引记录
4. 在测试环境验证无性能退化

### 4.5 索引健康检查

```sql
-- 查找未使用的索引
SELECT schemaname, relname AS table_name, indexrelname AS index_name,
       idx_scan AS index_scans, idx_tup_read, idx_tup_fetch
FROM pg_stat_user_indexes
WHERE idx_scan = 0
ORDER BY pg_relation_size(indexrelid) DESC;

-- 查找重复索引
SELECT pg_size_pretty(sum(pg_relation_size(idx))::bigint) AS size,
       (array_agg(idx))[1] as idx1, (array_agg(idx))[2] as idx2,
       (array_agg(idx))[3] as idx3, (array_agg(idx))[4] as idx4
FROM (
    SELECT indexrelid::regclass AS idx, indrelid::regclass AS table,
           array_to_string(indkey, ',') AS cols,
           array_to_string(indclass, ',') AS opclasses,
           indpred IS NOT NULL AS is_partial
    FROM pg_index
) sub
GROUP BY table, cols, opclasses, is_partial
HAVING count(*) > 1;

-- 查看索引大小
SELECT indexrelname AS index_name,
       pg_size_pretty(pg_relation_size(indexrelid)) AS index_size
FROM pg_stat_user_indexes
ORDER BY pg_relation_size(indexrelid) DESC
LIMIT 20;
```

### 4.6 索引创建最佳实践

1. **使用 `IF NOT EXISTS`**：避免重复创建导致错误
2. **避免在生产环境直接创建大表索引**：使用 `CONCURRENTLY` 选项
   ```sql
   CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_name ON table_name(column);
   ```
3. **迁移文件中使用安全模板**：
   ```sql
   CREATE INDEX IF NOT EXISTS idx_name ON table_name(column);
   ```
4. **定期审查**：每次版本发布前检查索引使用情况
