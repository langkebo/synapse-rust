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
| Retention | `room_retention_policies`, `server_retention_policy` | `min_lifetime_ms`, `max_lifetime_ms`, `room_id` | retention upsert/get/delete |
| E2EE / Verification | `device_verification_request` 及设备列表流相关表 | `request_id`, `user_id`, `device_id` | 设备验证状态、key 变化流 |
| Search | `search_index` 以及事件/成员权限相关表 | 文本索引列、权限过滤依赖 | 搜索索引重建、全文检索、权限裁剪 |
| 邀请治理 | `room_invite_state`, `room_invite_rooms`, `room_invites` | invite 关联键与状态列 | 邀请快照与邀请状态渲染 |

## 3. 高优先级 contract 对象

- `events`
- `room_memberships`
- `account_data`, `push_rules`
- `spaces*`, `thread_*`, `room_summaries*`, `room_retention_policies`
- `device_verification_request`

## 4. 契约条目清单（P0 最小集）

说明：
- 本清单面向“可执行契约”：字段存在性、类型/可空性、默认值、唯一约束/关键索引。
- 列名与类型以 `migrations/00000000_unified_schema_v6.sql` 为准；后续若迁移变更，必须同步更新清单与 contract tests。

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
