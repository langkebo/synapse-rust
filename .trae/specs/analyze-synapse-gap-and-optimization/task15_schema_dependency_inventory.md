# Task 15 - Schema 依赖清单

## 1. 目标

明确高风险能力域对应的关键表、关键字段、关键索引和关键查询，形成 schema contract 与 migration gate 的输入。

## 2. 能力域矩阵

| 能力域 | 关键表 | 高风险字段/约束 | 关键查询/行为 |
| --- | --- | --- | --- |
| 基础身份与访问 | `users`, `devices`, `access_tokens`, `refresh_tokens` | `user_id`, `device_id`, `is_revoked`, `expires_at` | 登录、token 校验、设备查询 |
| 房间主链 | `rooms`, `events`, `room_memberships` | `room_id`, `membership`, `origin_server_ts` | members、messages、search、sync、权限判断 |
| Account Data | `account_data`, `push_rules` | `user_id`, `room_id`, `data_type` | account data 读写、push rules 读取 |
| Space | `spaces`, `space_members`, `space_summaries`, `space_statistics`, `space_events` | `space_id`, `room_id` | hierarchy、space summary、成员查询 |
| Thread | `thread_roots`, `thread_participants`, `thread_receipts`, `thread_subscriptions`, `thread_notifications` | `thread_id`, `root_event_id`, `last_reply_ts` | thread 列表、回复、订阅与通知 |
| Room Summary | `room_summaries`, `room_summary_members` | `room_id`, `join_rules`, unread 统计列 | unread、summary members、摘要刷新 |
| Retention | `room_retention_policies`, `server_retention_policy` | `min_lifetime`, `max_lifetime`, `room_id` | retention upsert/get/delete |
| E2EE / Verification | `device_verification_request` 及设备列表流相关表 | `request_id`, `user_id`, `device_id` | 设备验证状态、key 变化流 |
| Search | `search_index` 以及事件/成员权限相关表 | 文本索引列、权限过滤依赖 | 搜索索引重建、全文检索、权限裁剪 |
| 邀请治理 | `room_invite_state`, `room_invite_rooms`, `room_invites` | invite 关联键与状态列 | 邀请快照与邀请状态渲染 |
| 媒体配额 | `media_quota_config`, `user_media_quota`, `media_usage_log`, `media_quota_alerts`, `server_media_quota` | quota 配置列、用户使用量、告警状态与服务器总量 | 配额创建、上传/删除计量、告警读取与服务器阈值更新 |
| 回执与读标记 | `read_markers`, `event_receipts` | marker / receipt 唯一键、事件关联、时间戳与 JSON 数据 | fully-read / m.read 写入、回执查询、sync unread 统计输入 |

## 3. 高优先级 contract 对象

- `rooms`
- `events`
- `room_memberships`
- `account_data`, `push_rules`
- `spaces*`, `thread_*`, `room_summaries*`, `room_retention_policies`
- `device_verification_request`
- `read_markers`, `event_receipts`

## 4. 契约条目清单（P0 最小集）

说明：
- 本清单面向“可执行契约”：字段存在性、类型/可空性、默认值、唯一约束/关键索引。
- 列名与类型以 `migrations/00000000_unified_schema_v6.sql` 为准；后续若迁移变更，必须同步更新清单与 contract tests。

### 4.0 `rooms`

| 条目 | 类型 | 可空 | 默认值/约束 | 索引/唯一性 |
| --- | --- | --- | --- | --- |
| `rooms.room_id` | TEXT | 否 | PK | PK |
| `rooms.creator` | TEXT | 是 | n/a | `idx_rooms_creator`（部分索引） |
| `rooms.is_public` | BOOLEAN | 是 | DEFAULT FALSE | `idx_rooms_is_public`（部分索引） |
| `rooms.room_version` | TEXT | 是 | DEFAULT `'6'` | n/a |
| `rooms.join_rules` | TEXT | 是 | DEFAULT `'invite'` | n/a |
| `rooms.history_visibility` | TEXT | 是 | DEFAULT `'shared'` | n/a |
| `rooms.created_ts` | BIGINT | 否 | n/a | n/a |

### 4.1 `room_memberships`

| 条目 | 类型 | 可空 | 默认值/约束 | 索引/唯一性 |
| --- | --- | --- | --- | --- |
| `room_memberships.id` | BIGSERIAL | 否 | PK | PK |
| `room_memberships.room_id` | TEXT | 否 | FK -> `rooms(room_id)` | `idx_room_memberships_room` |
| `room_memberships.user_id` | TEXT | 否 | FK -> `users(user_id)` | `idx_room_memberships_user` |
| `room_memberships.membership` | TEXT | 否 | n/a | `idx_room_memberships_room_membership`, `idx_room_memberships_user_membership` |
| `room_memberships.updated_ts` | BIGINT | 是 | n/a | n/a |
| `uq_room_memberships_room_user` | n/a | n/a | UNIQUE(`room_id`,`user_id`) | UNIQUE |

### 4.2 `events`

| 条目 | 类型 | 可空 | 默认值/约束 | 索引/唯一性 |
| --- | --- | --- | --- | --- |
| `events.event_id` | TEXT | 否 | PK | PK |
| `events.room_id` | TEXT | 否 | FK -> `rooms(room_id)` | `idx_events_room_id`, `idx_events_not_redacted` |
| `events.sender` | TEXT | 否 | n/a | `idx_events_sender` |
| `events.event_type` | TEXT | 否 | n/a | `idx_events_type` |
| `events.content` | JSONB | 否 | n/a | n/a |
| `events.origin_server_ts` | BIGINT | 否 | n/a | `idx_events_origin_server_ts`, `idx_events_not_redacted` |
| `events.is_redacted` | BOOLEAN | 是 | DEFAULT FALSE | `idx_events_not_redacted`（部分索引） |
| `events.unsigned` | JSONB | 是 | DEFAULT `'{}'` | n/a |

### 4.3 `account_data` / `room_account_data`

| 条目 | 类型 | 可空 | 默认值/约束 | 索引/唯一性 |
| --- | --- | --- | --- | --- |
| `account_data.user_id` | TEXT | 否 | n/a | `idx_account_data_user` |
| `account_data.data_type` | TEXT | 否 | n/a | UNIQUE(`user_id`,`data_type`) |
| `account_data.content` | JSONB | 否 | n/a | n/a |
| `account_data.created_ts` | BIGINT | 否 | n/a | n/a |
| `account_data.updated_ts` | BIGINT | 否 | n/a | n/a |
| `uq_account_data_user_type` | n/a | n/a | UNIQUE(`user_id`,`data_type`) | UNIQUE |
| `room_account_data.user_id` | TEXT | 否 | n/a | UNIQUE(`user_id`,`room_id`,`data_type`) |
| `room_account_data.room_id` | TEXT | 否 | n/a | UNIQUE(`user_id`,`room_id`,`data_type`) |
| `room_account_data.data_type` | TEXT | 否 | n/a | UNIQUE(`user_id`,`room_id`,`data_type`) |
| `room_account_data.data` | JSONB | 否 | n/a | n/a |
| `uq_room_account_data_user_room_type` | n/a | n/a | UNIQUE(`user_id`,`room_id`,`data_type`) | UNIQUE |

### 4.4 `push_rules`

| 条目 | 类型 | 可空 | 默认值/约束 | 索引/唯一性 |
| --- | --- | --- | --- | --- |
| `push_rules.user_id` | TEXT | 否 | n/a | `idx_push_rules_user` |
| `push_rules.scope` | TEXT | 否 | n/a | UNIQUE(`user_id`,`scope`,`kind`,`rule_id`) |
| `push_rules.rule_id` | TEXT | 否 | n/a | UNIQUE(`user_id`,`scope`,`kind`,`rule_id`) |
| `push_rules.kind` | TEXT | 否 | n/a | n/a |
| `push_rules.priority_class` | INTEGER | 否 | n/a | n/a |
| `push_rules.priority` | INTEGER | 是 | DEFAULT `0` | `idx_push_rules_user_priority` |
| `push_rules.conditions` | JSONB | 是 | DEFAULT `'[]'` | n/a |
| `push_rules.actions` | JSONB | 是 | DEFAULT `'[]'` | n/a |
| `push_rules.is_enabled` | BOOLEAN | 是 | DEFAULT TRUE | n/a |
| `push_rules.created_ts` | BIGINT | 否 | n/a | n/a |
| `uq_push_rules_user_scope_kind_rule` | n/a | n/a | UNIQUE(`user_id`,`scope`,`kind`,`rule_id`) | UNIQUE |

### 4.5 `room_retention_policies`

| 条目 | 类型 | 可空 | 默认值/约束 | 索引/唯一性 |
| --- | --- | --- | --- | --- |
| `room_retention_policies.room_id` | TEXT | 否 | FK -> `rooms(room_id)` | UNIQUE(`room_id`) |
| `room_retention_policies.min_lifetime` | BIGINT | 否 | DEFAULT 0 | n/a |
| `room_retention_policies.max_lifetime` | BIGINT | 是 | n/a | n/a |
| `room_retention_policies.expire_on_clients` | BOOLEAN | 否 | DEFAULT FALSE | n/a |
| `room_retention_policies.is_server_default` | BOOLEAN | 否 | DEFAULT FALSE | `idx_room_retention_policies_server_default`（部分索引） |
| `room_retention_policies.created_ts` | BIGINT | 否 | n/a | n/a |
| `room_retention_policies.updated_ts` | BIGINT | 否 | n/a | n/a |

### 4.6 `device_verification_request`

| 条目 | 类型 | 可空 | 默认值/约束 | 索引/唯一性 |
| --- | --- | --- | --- | --- |
| `device_verification_request.user_id` | TEXT | 否 | FK -> `users(user_id)` | `idx_device_verification_request_user_device_pending` |
| `device_verification_request.new_device_id` | TEXT | 否 | n/a | `idx_device_verification_request_user_device_pending` |
| `device_verification_request.status` | TEXT | 否 | n/a | 部分索引 `WHERE status='pending'` |
| `device_verification_request.request_token` | TEXT | 否 | UNIQUE | UNIQUE |
| `device_verification_request.expires_at` | TIMESTAMPTZ | 否 | n/a | `idx_device_verification_request_expires_pending` |
| `device_verification_request.completed_at` | TIMESTAMPTZ | 是 | n/a | n/a |

说明：
- 本表的 `*_at` 字段按 `TIMESTAMPTZ` 存储（当前 schema 事实），属于时间字段口径的例外项；相关 contract tests 应固定断言该类型，避免误改为 BIGINT 导致查询/映射漂移。

### 4.7 `search_index`（最小契约）

| 条目 | 类型 | 可空 | 默认值/约束 | 索引/唯一性 |
| --- | --- | --- | --- | --- |
| `search_index.event_id` | VARCHAR(255) | 否 | UNIQUE | UNIQUE |
| `search_index.room_id` | VARCHAR(255) | 否 | n/a | `idx_search_index_room` |
| `search_index.user_id` | VARCHAR(255) | 否 | n/a | `idx_search_index_user` |
| `search_index.event_type` | VARCHAR(255) | 否 | n/a | `idx_search_index_type` |
| `search_index.content` | TEXT | 否 | n/a | n/a |
| `search_index.created_ts` | BIGINT | 否 | n/a | n/a |
| `search_index.updated_ts` | BIGINT | 是 | n/a | n/a |
| `uq_search_index_event` | n/a | n/a | UNIQUE(`event_id`) | UNIQUE |

### 4.8 `room_summaries` / `room_summary_members`

| 条目 | 类型 | 可空 | 默认值/约束 | 索引/唯一性 |
| --- | --- | --- | --- | --- |
| `room_summaries.room_id` | TEXT | 否 | PK, FK -> `rooms(room_id)` | PK |
| `room_summaries.join_rules` | TEXT | 否 | DEFAULT `'invite'` | n/a |
| `room_summaries.hero_users` | JSONB | 否 | DEFAULT `'[]'` | n/a |
| `room_summaries.last_event_ts` | BIGINT | 是 | n/a | `idx_room_summaries_last_event_ts` |
| `room_summaries.is_space` | BOOLEAN | 否 | DEFAULT FALSE | `idx_room_summaries_space`（部分索引） |
| `room_summaries.updated_ts` | BIGINT | 否 | n/a | n/a |
| `room_summary_members.room_id` | TEXT | 否 | FK -> `rooms(room_id)` | UNIQUE(`room_id`,`user_id`) |
| `room_summary_members.user_id` | TEXT | 否 | FK -> `users(user_id)` | UNIQUE(`room_id`,`user_id`), `idx_room_summary_members_user_membership_room` |
| `room_summary_members.membership` | TEXT | 否 | n/a | `idx_room_summary_members_user_membership_room`, `idx_room_summary_members_room_membership_hero_active` |
| `room_summary_members.is_hero` | BOOLEAN | 否 | DEFAULT FALSE | `idx_room_summary_members_room_membership_hero_active`, `idx_room_summary_members_room_hero_user` |
| `room_summary_members.last_active_ts` | BIGINT | 是 | n/a | `idx_room_summary_members_room_membership_hero_active` |
| `uq_room_summary_members_room_user` | n/a | n/a | UNIQUE(`room_id`,`user_id`) | UNIQUE |

### 4.9 `space_summaries` / `space_members` / `space_events` / `space_statistics`

| 条目 | 类型 | 可空 | 默认值/约束 | 索引/唯一性 |
| --- | --- | --- | --- | --- |
| `space_members.space_id` | TEXT | 否 | n/a | UNIQUE(`space_id`,`user_id`), `idx_space_members_space` |
| `space_members.user_id` | TEXT | 否 | n/a | UNIQUE(`space_id`,`user_id`), `idx_space_members_user` |
| `space_members.membership` | TEXT | 否 | DEFAULT `'join'` | `idx_space_members_membership` |
| `space_members.joined_ts` | BIGINT | 否 | n/a | n/a |
| `space_members.updated_ts` | BIGINT | 是 | n/a | n/a |
| `space_members.left_ts` | BIGINT | 是 | n/a | n/a |
| `uq_space_members_space_user` | n/a | n/a | UNIQUE(`space_id`,`user_id`) | UNIQUE |
| `space_summaries.space_id` | TEXT | 否 | UNIQUE, FK -> `spaces(space_id)` | UNIQUE, `idx_space_summary_space` |
| `space_summaries.summary` | JSONB | 是 | DEFAULT `'{}'` | n/a |
| `space_summaries.children_count` | BIGINT | 是 | DEFAULT `0` | n/a |
| `space_summaries.member_count` | BIGINT | 是 | DEFAULT `0` | n/a |
| `space_summaries.updated_ts` | BIGINT | 否 | n/a | n/a |
| `space_events.event_id` | TEXT | 否 | PK | PK |
| `space_events.space_id` | TEXT | 否 | FK -> `spaces(space_id)` | `idx_space_events_space`, `idx_space_events_space_type_ts`, `idx_space_events_space_ts` |
| `space_events.event_type` | TEXT | 否 | n/a | `idx_space_events_space_type_ts` |
| `space_events.sender` | TEXT | 否 | n/a | n/a |
| `space_events.content` | JSONB | 否 | n/a | n/a |
| `space_events.origin_server_ts` | BIGINT | 否 | n/a | `idx_space_events_space_type_ts`, `idx_space_events_space_ts` |
| `space_events.processed_ts` | BIGINT | 是 | n/a | n/a |
| `space_statistics.space_id` | TEXT | 否 | PK | PK |
| `space_statistics.is_public` | BOOLEAN | 否 | DEFAULT FALSE | n/a |
| `space_statistics.child_room_count` | BIGINT | 是 | DEFAULT `0` | n/a |
| `space_statistics.member_count` | BIGINT | 是 | DEFAULT `0` | `idx_space_statistics_member_count` |
| `space_statistics.created_ts` | BIGINT | 否 | n/a | n/a |
| `space_statistics.updated_ts` | BIGINT | 是 | n/a | n/a |

### 4.10 `room_summary_state` / `room_summary_stats`

| 条目 | 类型 | 可空 | 默认值/约束 | 索引/唯一性 |
| --- | --- | --- | --- | --- |
| `room_summary_state.id` | BIGSERIAL | 否 | PK | PK |
| `room_summary_state.room_id` | TEXT | 否 | FK -> `rooms(room_id)` | UNIQUE(`room_id`,`event_type`,`state_key`), `idx_room_summary_state_room` |
| `room_summary_state.event_type` | TEXT | 否 | n/a | UNIQUE(`room_id`,`event_type`,`state_key`) |
| `room_summary_state.state_key` | TEXT | 否 | n/a | UNIQUE(`room_id`,`event_type`,`state_key`) |
| `room_summary_state.event_id` | TEXT | 是 | n/a | n/a |
| `room_summary_state.content` | JSONB | 否 | DEFAULT `'{}'` | n/a |
| `room_summary_state.updated_ts` | BIGINT | 否 | n/a | n/a |
| `uq_room_summary_state_room_type_state` | n/a | n/a | UNIQUE(`room_id`,`event_type`,`state_key`) | UNIQUE |
| `room_summary_stats.id` | BIGSERIAL | 否 | PK | PK |
| `room_summary_stats.room_id` | TEXT | 否 | UNIQUE, FK -> `rooms(room_id)` | UNIQUE |
| `room_summary_stats.total_events` | BIGINT | 否 | DEFAULT `0` | n/a |
| `room_summary_stats.total_state_events` | BIGINT | 否 | DEFAULT `0` | n/a |
| `room_summary_stats.total_messages` | BIGINT | 否 | DEFAULT `0` | n/a |
| `room_summary_stats.total_media` | BIGINT | 否 | DEFAULT `0` | n/a |
| `room_summary_stats.storage_size` | BIGINT | 否 | DEFAULT `0` | n/a |
| `room_summary_stats.last_updated_ts` | BIGINT | 否 | n/a | n/a |

### 4.11 `room_summary_update_queue` / `room_children`

| 条目 | 类型 | 可空 | 默认值/约束 | 索引/唯一性 |
| --- | --- | --- | --- | --- |
| `room_summary_update_queue.id` | BIGSERIAL | 否 | PK | PK |
| `room_summary_update_queue.room_id` | TEXT | 否 | FK -> `rooms(room_id)` | `idx_room_summary_update_queue_status_priority_created` |
| `room_summary_update_queue.event_id` | TEXT | 否 | n/a | n/a |
| `room_summary_update_queue.event_type` | TEXT | 否 | n/a | n/a |
| `room_summary_update_queue.state_key` | TEXT | 是 | n/a | n/a |
| `room_summary_update_queue.priority` | INTEGER | 否 | DEFAULT `0` | `idx_room_summary_update_queue_status_priority_created` |
| `room_summary_update_queue.status` | TEXT | 否 | DEFAULT `'pending'` | `idx_room_summary_update_queue_status_priority_created` |
| `room_summary_update_queue.created_ts` | BIGINT | 否 | n/a | `idx_room_summary_update_queue_status_priority_created` |
| `room_summary_update_queue.processed_ts` | BIGINT | 是 | n/a | n/a |
| `room_summary_update_queue.error_message` | TEXT | 是 | n/a | n/a |
| `room_summary_update_queue.retry_count` | INTEGER | 否 | DEFAULT `0` | n/a |
| `room_children.id` | BIGSERIAL | 否 | PK | PK |
| `room_children.parent_room_id` | TEXT | 否 | FK -> `rooms(room_id)` | UNIQUE(`parent_room_id`,`child_room_id`), `idx_room_children_parent_suggested` |
| `room_children.child_room_id` | TEXT | 否 | FK -> `rooms(room_id)` | UNIQUE(`parent_room_id`,`child_room_id`), `idx_room_children_child` |
| `room_children.state_key` | TEXT | 是 | n/a | n/a |
| `room_children.content` | JSONB | 否 | DEFAULT `'{}'` | n/a |
| `room_children.suggested` | BOOLEAN | 否 | DEFAULT FALSE | `idx_room_children_parent_suggested` |
| `room_children.created_ts` | BIGINT | 否 | DEFAULT `0` | n/a |
| `room_children.updated_ts` | BIGINT | 是 | n/a | n/a |
| `uq_room_children_parent_child` | n/a | n/a | UNIQUE(`parent_room_id`,`child_room_id`) | UNIQUE |

### 4.12 `space_children` / hierarchy

| 条目 | 类型 | 可空 | 默认值/约束 | 索引/唯一性 |
| --- | --- | --- | --- | --- |
| `space_children.id` | BIGSERIAL | 否 | PK | PK |
| `space_children.space_id` | TEXT | 否 | n/a | UNIQUE(`space_id`,`room_id`), `idx_space_children_space` |
| `space_children.room_id` | TEXT | 否 | n/a | UNIQUE(`space_id`,`room_id`), `idx_space_children_room` |
| `space_children.sender` | TEXT | 否 | n/a | n/a |
| `space_children.is_suggested` | BOOLEAN | 是 | DEFAULT FALSE | n/a |
| `space_children.via_servers` | JSONB | 是 | DEFAULT `'[]'` | n/a |
| `space_children.added_ts` | BIGINT | 否 | n/a | n/a |
| `uq_space_children_space_room` | n/a | n/a | UNIQUE(`space_id`,`room_id`) | UNIQUE |

### 4.13 `thread_roots` / `thread_replies` / `thread_relations`

| 条目 | 类型 | 可空 | 默认值/约束 | 索引/唯一性 |
| --- | --- | --- | --- | --- |
| `thread_roots.id` | BIGSERIAL | 否 | PK | PK |
| `thread_roots.room_id` | TEXT | 否 | FK -> `rooms(room_id)` | `idx_thread_roots_room` |
| `thread_roots.root_event_id` | TEXT | 否 | UNIQUE(`room_id`,`root_event_id`) | `idx_thread_roots_root_event` |
| `thread_roots.thread_id` | TEXT | 是 | n/a | `idx_thread_roots_thread`, `idx_thread_roots_room_thread_unique`（partial unique） |
| `thread_roots.reply_count` | BIGINT | 是 | DEFAULT `0` | n/a |
| `thread_roots.participants` | JSONB | 是 | DEFAULT `'[]'` | n/a |
| `thread_roots.last_reply_ts` | BIGINT | 是 | n/a | `idx_thread_roots_room_last_reply_created`, `idx_thread_roots_last_reply` |
| `thread_replies.id` | BIGSERIAL | 否 | PK | PK |
| `thread_replies.room_id` | TEXT | 否 | FK -> `rooms(room_id)` | `idx_thread_replies_room_event` |
| `thread_replies.thread_id` | TEXT | 否 | n/a | `idx_thread_replies_room_thread_ts`, `idx_thread_replies_room_thread_event` |
| `thread_replies.event_id` | TEXT | 否 | UNIQUE(`room_id`,`event_id`) | UNIQUE |
| `thread_replies.content` | JSONB | 否 | DEFAULT `'{}'` | n/a |
| `thread_relations.id` | BIGSERIAL | 否 | PK | PK |
| `thread_relations.room_id` | TEXT | 否 | FK -> `rooms(room_id)` | `idx_thread_relations_room_event` |
| `thread_relations.event_id` | TEXT | 否 | UNIQUE(`room_id`,`event_id`,`relation_type`) | UNIQUE |
| `thread_relations.relates_to_event_id` | TEXT | 否 | n/a | `idx_thread_relations_room_relates_to` |

### 4.14 Retention (`server_retention_policy` / `room_retention_policies` / `retention_cleanup_*` / `deleted_events_index`)

| 条目 | 类型 | 可空 | 默认值/约束 | 索引/唯一性 |
| --- | --- | --- | --- | --- |
| `server_retention_policy.id` | BIGSERIAL | 否 | PK | PK |
| `server_retention_policy.min_lifetime` | BIGINT | 否 | DEFAULT `0` | n/a |
| `server_retention_policy.expire_on_clients` | BOOLEAN | 否 | DEFAULT FALSE | n/a |
| `room_retention_policies.room_id` | TEXT | 否 | UNIQUE, FK -> `rooms(room_id)` | UNIQUE |
| `room_retention_policies.is_server_default` | BOOLEAN | 否 | DEFAULT FALSE | `idx_room_retention_policies_server_default`（部分索引） |
| `retention_cleanup_queue.id` | BIGSERIAL | 否 | PK | PK |
| `retention_cleanup_queue.status` | TEXT | 否 | DEFAULT `'pending'` | `idx_retention_cleanup_queue_status_origin` |
| `retention_cleanup_queue.retry_count` | INTEGER | 否 | DEFAULT `0` | n/a |
| `retention_cleanup_queue.uq_retention_cleanup_queue_room_event` | n/a | n/a | UNIQUE(`room_id`,`event_id`) | UNIQUE |
| `retention_cleanup_logs.id` | BIGSERIAL | 否 | PK | PK |
| `retention_cleanup_logs.room_id` | TEXT | 否 | FK -> `rooms(room_id)` | `idx_retention_cleanup_logs_room_started` |
| `deleted_events_index.id` | BIGSERIAL | 否 | PK | PK |
| `deleted_events_index.uq_deleted_events_index_room_event` | n/a | n/a | UNIQUE(`room_id`,`event_id`) | UNIQUE |
| `deleted_events_index.idx_deleted_events_index_room_ts` | n/a | n/a | INDEX(`room_id`,`deletion_ts`) | INDEX |

### 4.15 Invite restrictions (`room_invite_blocklist` / `room_invite_allowlist`)

| 条目 | 类型 | 可空 | 默认值/约束 | 索引/唯一性 |
| --- | --- | --- | --- | --- |
| `room_invite_blocklist.id` | BIGSERIAL | 否 | PK | PK |
| `room_invite_blocklist.room_id` | TEXT | 否 | FK -> `rooms(room_id)` | `idx_room_invite_blocklist_room` |
| `room_invite_blocklist.user_id` | TEXT | 否 | UNIQUE(`room_id`,`user_id`) | `idx_room_invite_blocklist_user` |
| `room_invite_allowlist.id` | BIGSERIAL | 否 | PK | PK |
| `room_invite_allowlist.room_id` | TEXT | 否 | FK -> `rooms(room_id)` | `idx_room_invite_allowlist_room` |
| `room_invite_allowlist.user_id` | TEXT | 否 | UNIQUE(`room_id`,`user_id`) | `idx_room_invite_allowlist_user` |

### 4.16 Auth tokens (`access_tokens` / `refresh_tokens` / `token_blacklist`)

| 条目 | 类型 | 可空 | 默认值/约束 | 索引/唯一性 |
| --- | --- | --- | --- | --- |
| `access_tokens.id` | BIGSERIAL | 否 | PK | PK |
| `access_tokens.token` | TEXT | 否 | UNIQUE | UNIQUE |
| `access_tokens.user_id` | TEXT | 否 | FK -> `users(user_id)` | `idx_access_tokens_user_id` |
| `access_tokens.is_revoked` | BOOLEAN | 是 | DEFAULT FALSE | `idx_access_tokens_valid`（部分索引） |
| `refresh_tokens.id` | BIGSERIAL | 否 | PK | PK |
| `refresh_tokens.token_hash` | TEXT | 否 | UNIQUE | UNIQUE |
| `refresh_tokens.user_id` | TEXT | 否 | FK -> `users(user_id)` | `idx_refresh_tokens_user_id` |
| `refresh_tokens.use_count` | INTEGER | 是 | DEFAULT `0` | n/a |
| `refresh_tokens.is_revoked` | BOOLEAN | 是 | DEFAULT FALSE | `idx_refresh_tokens_revoked`（部分索引） |
| `token_blacklist.id` | BIGSERIAL | 否 | PK | PK |
| `token_blacklist.token_hash` | TEXT | 否 | UNIQUE | `idx_token_blacklist_hash` |
| `token_blacklist.token_type` | TEXT | 是 | DEFAULT `'access'` | n/a |
| `token_blacklist.is_revoked` | BOOLEAN | 是 | DEFAULT TRUE | n/a |

### 4.17 Presence (`presence` / `presence_subscriptions`)

| 条目 | 类型 | 可空 | 默认值/约束 | 索引/唯一性 |
| --- | --- | --- | --- | --- |
| `presence.user_id` | TEXT | 否 | PK, FK -> `users(user_id)` | PK |
| `presence.presence` | TEXT | 否 | DEFAULT `'offline'` | `idx_presence_user_status`（复合索引） |
| `presence.last_active_ts` | BIGINT | 否 | DEFAULT `0` | n/a |
| `presence_subscriptions.subscriber_id` | TEXT | 否 | UNIQUE(`subscriber_id`,`target_id`) | `idx_presence_subscriptions_subscriber` |
| `presence_subscriptions.target_id` | TEXT | 否 | UNIQUE(`subscriber_id`,`target_id`) | `idx_presence_subscriptions_target` |

### 4.18 OpenID token (`openid_tokens`)

| 条目 | 类型 | 可空 | 默认值/约束 | 索引/唯一性 |
| --- | --- | --- | --- | --- |
| `openid_tokens.id` | BIGSERIAL | 否 | PK | PK |
| `openid_tokens.token` | TEXT | 否 | UNIQUE | UNIQUE |
| `openid_tokens.user_id` | TEXT | 否 | FK -> `users(user_id)` | `idx_openid_tokens_user` |
| `openid_tokens.expires_at` | BIGINT | 否 | n/a | n/a |
| `openid_tokens.is_valid` | BOOLEAN | 是 | DEFAULT TRUE | n/a |

### 4.19 Media quota (`media_quota_config` / `user_media_quota` / `media_usage_log` / `media_quota_alerts` / `server_media_quota`)

| 条目 | 类型 | 可空 | 默认值/约束 | 索引/唯一性 |
| --- | --- | --- | --- | --- |
| `media_quota_config.id` | BIGSERIAL | 否 | PK | PK |
| `media_quota_config.name` | TEXT | 否 | DEFAULT `'default'` | n/a |
| `media_quota_config.max_storage_bytes` | BIGINT | 否 | DEFAULT `10737418240` | n/a |
| `media_quota_config.max_file_size_bytes` | BIGINT | 否 | DEFAULT `10485760` | n/a |
| `media_quota_config.max_files_count` | INTEGER | 否 | DEFAULT `10000` | n/a |
| `media_quota_config.allowed_mime_types` | JSONB | 否 | DEFAULT `'[]'` | n/a |
| `media_quota_config.blocked_mime_types` | JSONB | 否 | DEFAULT `'[]'` | n/a |
| `media_quota_config.is_default` | BOOLEAN | 否 | DEFAULT FALSE | n/a |
| `user_media_quota.id` | BIGSERIAL | 否 | PK | PK |
| `user_media_quota.user_id` | TEXT | 否 | UNIQUE(`user_id`) | UNIQUE(`user_id`) |
| `user_media_quota.current_storage_bytes` | BIGINT | 否 | DEFAULT `0` | n/a |
| `user_media_quota.current_files_count` | INTEGER | 否 | DEFAULT `0` | n/a |
| `media_usage_log.id` | BIGSERIAL | 否 | PK | PK |
| `media_usage_log.user_id` | TEXT | 否 | n/a | `idx_media_usage_log_user` |
| `media_usage_log.timestamp` | BIGINT | 否 | n/a | `idx_media_usage_log_timestamp` |
| `media_quota_alerts.id` | BIGSERIAL | 否 | PK | PK |
| `media_quota_alerts.user_id` | TEXT | 否 | n/a | `idx_media_quota_alerts_user`（未读部分索引） |
| `media_quota_alerts.is_read` | BOOLEAN | 否 | DEFAULT FALSE | `idx_media_quota_alerts_user`（未读部分索引） |
| `server_media_quota.id` | BIGSERIAL | 否 | PK | PK |
| `server_media_quota.current_storage_bytes` | BIGINT | 否 | DEFAULT `0` | n/a |
| `server_media_quota.current_files_count` | INTEGER | 否 | DEFAULT `0` | n/a |
| `server_media_quota.alert_threshold_percent` | INTEGER | 否 | DEFAULT `80` | n/a |

### 4.20 Receipts / Read markers (`read_markers` / `event_receipts`)

| 条目 | 类型 | 可空 | 默认值/约束 | 索引/唯一性 |
| --- | --- | --- | --- | --- |
| `read_markers.id` | BIGSERIAL | 否 | PK | PK |
| `read_markers.room_id` | TEXT | 否 | UNIQUE(`room_id`,`user_id`,`marker_type`) | `idx_read_markers_room_user` |
| `read_markers.user_id` | TEXT | 否 | UNIQUE(`room_id`,`user_id`,`marker_type`) | `idx_read_markers_room_user` |
| `read_markers.event_id` | TEXT | 否 | n/a | n/a |
| `read_markers.marker_type` | TEXT | 否 | UNIQUE(`room_id`,`user_id`,`marker_type`) | UNIQUE(`room_id`,`user_id`,`marker_type`) |
| `read_markers.created_ts` | BIGINT | 否 | n/a | n/a |
| `read_markers.updated_ts` | BIGINT | 否 | n/a | n/a |
| `event_receipts.id` | BIGSERIAL | 否 | PK | PK |
| `event_receipts.event_id` | TEXT | 否 | UNIQUE(`event_id`,`room_id`,`user_id`,`receipt_type`) | `idx_event_receipts_event` |
| `event_receipts.room_id` | TEXT | 否 | UNIQUE(`event_id`,`room_id`,`user_id`,`receipt_type`) | `idx_event_receipts_room`, `idx_event_receipts_room_type` |
| `event_receipts.user_id` | TEXT | 否 | UNIQUE(`event_id`,`room_id`,`user_id`,`receipt_type`) | n/a |
| `event_receipts.receipt_type` | TEXT | 否 | UNIQUE(`event_id`,`room_id`,`user_id`,`receipt_type`) | `idx_event_receipts_room_type` |
| `event_receipts.ts` | BIGINT | 否 | n/a | `idx_event_receipts_room_type` |
| `event_receipts.data` | JSONB | 是 | DEFAULT `'{}'` | n/a |
| `event_receipts.created_ts` | BIGINT | 否 | n/a | n/a |
| `event_receipts.updated_ts` | BIGINT | 否 | n/a | n/a |

## 5. 索引与契约重点

- `room_memberships(room_id, membership)` 与 `room_memberships(user_id, membership)`（必要时再评估三列复合索引）
- `events(room_id, origin_server_ts)` 及搜索相关文本索引
- `space_summaries(space_id)` 与 `space_children` / `space_members` 聚合一致性
- `space_events(space_id, event_type, origin_server_ts DESC)` 的过滤与排序契约
- `room_summary_state(room_id, event_type, state_key)` 的 upsert 唯一性与读取契约
- `room_summary_stats(room_id)` 的 upsert 覆盖更新契约
- `room_summary_update_queue(status, priority DESC, created_ts ASC)` 的调度排序契约
- `room_children(parent_room_id, child_room_id)` 的 upsert 唯一性与 child 关系读取契约
- `space_children(space_id, room_id)` 的 upsert 唯一性与 hierarchy 递归输入契约
- hierarchy 构造中 `children_state` 必须基于当前 room 对应的子关系而不是父关系
- `thread_roots(room_id, thread_id)` / `thread_roots(room_id, root_event_id)`
- `room_retention_policies(room_id)` 的 upsert 唯一性
- `room_summaries(room_id)` 与 summary members 关联一致性
