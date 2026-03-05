# 数据库字段审计报告

## 概览

- 表数量: 171
- 字段数量: 1658
- 约束数量: 448
- 外键数量: 104
- 审计问题总数: 190

## 关键问题统计

- snake_case 命名违规: 0
- 布尔前缀违规: 36
- 同名 ID 类型不一致: 16
- 冗余字段对: 5
- 疑似缺失外键: 133

## 问题分类

- missing_fk: 133
- boolean_naming: 36
- id_type_inconsistent: 16
- redundant_columns: 5

## 高优先级问题样例

| 严重级别 | 类别 | 表 | 字段 | 当前类型 | 建议 | 说明 |
|---|---|---|---|---|---|---|
| high | id_type_inconsistent | * | app_id | text|varchar | 统一为 varchar(255) 或 text 并在同语义域保持一致 | 同名 ID 字段存在多种数据类型 |
| high | id_type_inconsistent | * | as_id | text|varchar | 统一为 varchar(255) 或 text 并在同语义域保持一致 | 同名 ID 字段存在多种数据类型 |
| high | id_type_inconsistent | * | device_id | text|varchar | 统一为 varchar(255) 或 text 并在同语义域保持一致 | 同名 ID 字段存在多种数据类型 |
| high | id_type_inconsistent | * | event_id | text|varchar | 统一为 varchar(255) 或 text 并在同语义域保持一致 | 同名 ID 字段存在多种数据类型 |
| high | id_type_inconsistent | * | id | int4|int8|uuid|varchar | 统一为 varchar(255) 或 text 并在同语义域保持一致 | 同名 ID 字段存在多种数据类型 |
| high | id_type_inconsistent | * | key_id | text|varchar | 统一为 varchar(255) 或 text 并在同语义域保持一致 | 同名 ID 字段存在多种数据类型 |
| high | id_type_inconsistent | * | last_reply_event_id | text|varchar | 统一为 varchar(255) 或 text 并在同语义域保持一致 | 同名 ID 字段存在多种数据类型 |
| high | id_type_inconsistent | * | media_id | text|varchar | 统一为 varchar(255) 或 text 并在同语义域保持一致 | 同名 ID 字段存在多种数据类型 |
| high | id_type_inconsistent | * | name_id | text|varchar | 统一为 varchar(255) 或 text 并在同语义域保持一致 | 同名 ID 字段存在多种数据类型 |
| high | id_type_inconsistent | * | request_id | text|varchar | 统一为 varchar(255) 或 text 并在同语义域保持一致 | 同名 ID 字段存在多种数据类型 |
| high | id_type_inconsistent | * | room_id | text|varchar | 统一为 varchar(255) 或 text 并在同语义域保持一致 | 同名 ID 字段存在多种数据类型 |
| high | id_type_inconsistent | * | rule_id | int4|varchar | 统一为 varchar(255) 或 text 并在同语义域保持一致 | 同名 ID 字段存在多种数据类型 |
| high | id_type_inconsistent | * | sender_id | text|varchar | 统一为 varchar(255) 或 text 并在同语义域保持一致 | 同名 ID 字段存在多种数据类型 |
| high | id_type_inconsistent | * | session_id | text|varchar | 统一为 varchar(255) 或 text 并在同语义域保持一致 | 同名 ID 字段存在多种数据类型 |
| high | id_type_inconsistent | * | target_worker_id | text|varchar | 统一为 varchar(255) 或 text 并在同语义域保持一致 | 同名 ID 字段存在多种数据类型 |
| high | id_type_inconsistent | * | user_id | text|varchar | 统一为 varchar(255) 或 text 并在同语义域保持一致 | 同名 ID 字段存在多种数据类型 |
| high | redundant_columns | application_service_events | as_id|appservice_id | - | 保留单一规范字段并建立兼容迁移 | 存在语义重叠字段 |
| high | redundant_columns | application_service_state | as_id|appservice_id | - | 保留单一规范字段并建立兼容迁移 | 存在语义重叠字段 |
| high | redundant_columns | application_service_user_namespaces | as_id|appservice_id | - | 保留单一规范字段并建立兼容迁移 | 存在语义重叠字段 |
| high | redundant_columns | application_service_users | as_id|appservice_id | - | 保留单一规范字段并建立兼容迁移 | 存在语义重叠字段 |
| high | redundant_columns | password_auth_providers | enabled|is_enabled | - | 保留单一规范字段并建立兼容迁移 | 存在语义重叠字段 |
| medium | boolean_naming | account_validity | allow_renewal | bool | is_renewal | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | application_service_events | processed | bool | is_processed | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | application_service_room_alias_namespaces | exclusive | bool | is_exclusive | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | application_service_room_namespaces | exclusive | bool | is_exclusive | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | application_service_statistics | rate_limited | bool | is_rate_limited | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | application_service_user_namespaces | exclusive | bool | is_exclusive | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | application_services | rate_limited | bool | is_rate_limited | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | captcha_send_log | success | bool | is_success | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | device_keys | blocked | bool | is_blocked | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | device_keys | verified | bool | is_verified | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | e2ee_key_requests | fulfilled | bool | is_fulfilled | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | email_verification_tokens | used | bool | is_used | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | events | contains_url | bool | is_contains_url | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | events | redacted | bool | is_redacted | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | media_quota_alerts_backup_20260301 | acknowledged | bool | is_acknowledged | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | module_execution_logs | success | bool | is_success | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | notifications | read | bool | is_read | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | olm_accounts | fallback_key_published | bool | is_fallback_key_published | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | olm_accounts | one_time_keys_published | bool | is_one_time_keys_published | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | password_auth_providers | enabled | bool | is_enabled | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | private_messages | read_by_receiver | bool | is_read_by_receiver | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | push_notification_log | success | bool | is_success | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | push_provider_configs | enabled | bool | is_enabled | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | refresh_token_usage | success | bool | is_success | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | room_directory | searchable | bool | is_searchable | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | room_retention_policies | expire_on_clients | bool | is_expire_on_clients | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | room_summaries | federation_allowed | bool | is_federation_allowed | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | room_summaries | guest_can_join | bool | is_guest_can_join | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | room_summaries | world_readable | bool | is_world_readable | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | schema_migrations | success | bool | is_success | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | server_retention_policy | allow_default | bool | is_default | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | server_retention_policy | expire_on_clients | bool | is_expire_on_clients | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | space_children | suggested | bool | is_suggested | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | to_device_messages | delivered | bool | is_delivered | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | typing | typing | bool | is_typing | 布尔字段未使用 is_/has_ 前缀 |
| medium | boolean_naming | users | must_change_password | bool | is_must_change_password | 布尔字段未使用 is_/has_ 前缀 |
| medium | missing_fk | access_tokens | appservice_id | varchar | 评估并补充外键或显式记录无外键原因 | 疑似关联字段缺少主外键约束 |
| medium | missing_fk | access_tokens | device_id | varchar | 评估并补充外键或显式记录无外键原因 | 疑似关联字段缺少主外键约束 |
| medium | missing_fk | account_data | user_id | varchar | 评估并补充外键或显式记录无外键原因 | 疑似关联字段缺少主外键约束 |
| medium | missing_fk | account_data_callbacks | user_id | varchar | 评估并补充外键或显式记录无外键原因 | 疑似关联字段缺少主外键约束 |
| medium | missing_fk | active_tokens | appservice_id | varchar | 评估并补充外键或显式记录无外键原因 | 疑似关联字段缺少主外键约束 |
| medium | missing_fk | active_tokens | device_id | varchar | 评估并补充外键或显式记录无外键原因 | 疑似关联字段缺少主外键约束 |
| medium | missing_fk | active_tokens | user_id | varchar | 评估并补充外键或显式记录无外键原因 | 疑似关联字段缺少主外键约束 |
| medium | missing_fk | application_service_events | appservice_id | varchar | 评估并补充外键或显式记录无外键原因 | 疑似关联字段缺少主外键约束 |
| medium | missing_fk | application_service_events | as_id | text | 评估并补充外键或显式记录无外键原因 | 疑似关联字段缺少主外键约束 |
| medium | missing_fk | application_service_events | room_id | varchar | 评估并补充外键或显式记录无外键原因 | 疑似关联字段缺少主外键约束 |
| medium | missing_fk | application_service_state | appservice_id | varchar | 评估并补充外键或显式记录无外键原因 | 疑似关联字段缺少主外键约束 |
| medium | missing_fk | application_service_state | as_id | varchar | 评估并补充外键或显式记录无外键原因 | 疑似关联字段缺少主外键约束 |
| medium | missing_fk | application_service_statistics | as_id | varchar | 评估并补充外键或显式记录无外键原因 | 疑似关联字段缺少主外键约束 |
| medium | missing_fk | application_service_user_namespaces | appservice_id | varchar | 评估并补充外键或显式记录无外键原因 | 疑似关联字段缺少主外键约束 |
| medium | missing_fk | application_service_users | appservice_id | varchar | 评估并补充外键或显式记录无外键原因 | 疑似关联字段缺少主外键约束 |
| medium | missing_fk | application_service_users | as_id | text | 评估并补充外键或显式记录无外键原因 | 疑似关联字段缺少主外键约束 |
| medium | missing_fk | application_service_users | user_id | varchar | 评估并补充外键或显式记录无外键原因 | 疑似关联字段缺少主外键约束 |
| medium | missing_fk | application_services | as_id | varchar | 评估并补充外键或显式记录无外键原因 | 疑似关联字段缺少主外键约束 |
| medium | missing_fk | backup_keys | room_id | varchar | 评估并补充外键或显式记录无外键原因 | 疑似关联字段缺少主外键约束 |
| medium | missing_fk | blocked_rooms | room_id | varchar | 评估并补充外键或显式记录无外键原因 | 疑似关联字段缺少主外键约束 |
| medium | missing_fk | captcha_send_log | captcha_id | varchar | 评估并补充外键或显式记录无外键原因 | 疑似关联字段缺少主外键约束 |
| medium | missing_fk | cas_proxy_granting_tickets | user_id | text | 评估并补充外键或显式记录无外键原因 | 疑似关联字段缺少主外键约束 |
| medium | missing_fk | cas_services | service_id | varchar | 评估并补充外键或显式记录无外键原因 | 疑似关联字段缺少主外键约束 |
