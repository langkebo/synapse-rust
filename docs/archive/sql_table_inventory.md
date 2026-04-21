# SQL 表清单报告

> **项目**: synapse-rust 数据库全面排查
> **版本**: v1.0.0
> **生成日期**: 2026-03-20
> **数据来源**: PostgreSQL information_schema

---

## 一、执行摘要

| 指标 | 值 |
|------|-----|
| 总表数量 | 154 |
| 总索引数量 | 478 |
| 总外键数量 | 35+ |
| 数据库大小 | ~20 MB |

---

## 二、完整表清单

### 2.1 用户认证相关表

| 序号 | 表名 | 说明 |
|------|------|------|
| 1 | users | 用户主表 |
| 2 | access_tokens | 访问令牌 |
| 3 | refresh_tokens | 刷新令牌 |
| 4 | refresh_token_rotations | 刷新令牌轮换记录 |
| 5 | refresh_token_usage | 刷新令牌使用记录 |
| 6 | refresh_token_families | 刷新令牌家族 |
| 7 | token_blacklist | 令牌黑名单 |
| 8 | password_history | 密码历史 |
| 9 | password_reset_tokens | 密码重置令牌 |
| 10 | registration_tokens | 注册令牌 |
| 11 | registration_token_usage | 注册令牌使用记录 |
| 12 | login_tokens | 登录令牌 |
| 13 | account_validity | 账户有效期 |
| 14 | user_external_ids | 用户外部ID |
| 15 | openid_tokens | OpenID令牌 |

### 2.2 设备管理相关表

| 序号 | 表名 | 说明 |
|------|------|------|
| 16 | devices | 设备表 |
| 17 | device_keys | 设备密钥 |
| 18 | device_signatures | 设备签名 |
| 19 | device_trust_status | 设备信任状态 |
| 20 | device_verification_request | 设备验证请求 |
| 21 | e2ee_key_requests | E2EE密钥请求 |
| 22 | e2ee_security_events | E2EE安全事件 |

### 2.3 密钥管理相关表

| 序号 | 表名 | 说明 |
|------|------|------|
| 23 | cross_signing_keys | 交叉签名密钥 |
| 24 | cross_signing_trust | 交叉签名信任 |
| 25 | key_backups | 密钥备份 |
| 26 | secure_key_backups | 安全密钥备份 |
| 27 | secure_backup_session_keys | 安全备份会话密钥 |
| 28 | key_rotation_log | 密钥轮换日志 |
| 29 | backup_keys | 备份密钥 |
| 30 | olm_accounts | Olm账户 |
| 31 | olm_sessions | Olm会话 |
| 32 | one_time_keys | 一次性密钥 |
| 33 | megolm_sessions | Megolm会话 |

### 2.4 房间相关表

| 序号 | 表名 | 说明 |
|------|------|------|
| 34 | rooms | 房间主表 |
| 35 | room_aliases | 房间别名 |
| 36 | room_depth | 房间深度 |
| 37 | room_state_events | 房间状态事件 |
| 38 | room_events | 房间事件 |
| 39 | room_memberships | 房间成员 |
| 40 | room_invites | 房间邀请 |
| 41 | room_account_data | 房间账户数据 |
| 42 | room_ephemeral | 房间临时数据 |
| 43 | room_parents | 房间父子关系 |
| 44 | room_summaries | 房间摘要 |
| 45 | room_summary_members | 房间摘要成员 |
| 46 | room_tags | 房间标签 |
| 47 | room_directory | 房间目录 |
| 48 | event_auth | 事件认证 |
| 49 | event_receipts | 事件回执 |
| 50 | event_reports | 事件报告 |
| 51 | event_report_history | 事件报告历史 |
| 52 | event_report_stats | 事件报告统计 |
| 53 | event_signatures | 事件签名 |
| 54 | events | 事件主表 |
| 55 | redactions | 修订记录 |

### 2.5 消息相关表

| 序号 | 表名 | 说明 |
|------|------|------|
| 56 | notifications | 通知 |
| 57 | pushers | 推送器 |
| 58 | push_devices | 推送设备 |
| 59 | push_rules | 推送规则 |
| 60 | push_notification_queue | 推送通知队列 |
| 61 | push_notification_log | 推送通知日志 |
| 62 | typing | 打字状态 |
| 63 | read_markers | 已读标记 |
| 64 | thread_roots | 线程根 |
| 65 | thread_subscriptions | 线程订阅 |
| 66 | delayed_events | 延迟事件 |
| 67 | private_messages | 私信 |
| 68 | private_sessions | 私信会话 |
| 69 | voice_messages | 语音消息 |
| 70 | thumbnails | 缩略图 |

### 2.6 用户目录相关表

| 序号 | 表名 | 说明 |
|------|------|------|
| 71 | user_directory | 用户目录 |
| 72 | user_directory_profiles | 用户目录资料 |
| 73 | presence | 在线状态 |
| 74 | presence_routes | 在线状态路由 |
| 75 | user_stats | 用户统计 |
| 76 | user_filters | 用户过滤器 |
| 77 | user_account_data | 用户账户数据 |
| 78 | user_privacy_settings | 用户隐私设置 |
| 79 | user_media_quota | 用户媒体配额 |
| 80 | user_threepids | 用户第三方ID |
| 81 | account_data | 账户数据 |
| 82 | account_data_callbacks | 账户数据回调 |

### 2.7 好友相关表

| 序号 | 表名 | 说明 |
|------|------|------|
| 83 | friends | 好友表 |
| 84 | friend_requests | 好友请求 |
| 85 | friend_categories | 好友分类 |
| 86 | invitation_blocks | 邀请屏蔽 |
| 87 | group_memberships | 群组 membership |

### 2.8 应用服务相关表

| 序号 | 表名 | 说明 |
|------|------|------|
| 88 | application_services | 应用服务 |
| 89 | application_service_users | 应用服务用户 |
| 90 | application_service_rooms | 应用服务房间 |
| 91 | application_service_room_alias_namespaces | 应用服务房间别名命名空间 |
| 92 | application_service_room_namespaces | 应用服务房间命名空间 |
| 93 | application_service_user_namespaces | 应用服务用户命名空间 |
| 94 | application_service_state | 应用服务状态 |
| 95 | application_service_transactions | 应用服务事务 |
| 96 | application_service_events | 应用服务事件 |

### 2.9 联邦相关表

| 序号 | 表名 | 说明 |
|------|------|------|
| 97 | federation_servers | 联邦服务器 |
| 98 | federation_signing_keys | 联邦签名密钥 |
| 99 | federation_queue | 联邦队列 |
| 100 | federation_blacklist | 联邦黑名单 |

### 2.10 媒体相关表

| 序号 | 表名 | 说明 |
|------|------|------|
| 101 | media_metadata | 媒体元数据 |
| 102 | media_quota | 媒体配额 |
| 103 | media_quota_config | 媒体配额配置 |
| 104 | media_callbacks | 媒体回调 |

### 2.11 安全相关表

| 序号 | 表名 | 说明 |
|------|------|------|
| 105 | blocked_users | 屏蔽用户 |
| 106 | ip_blocks | IP封禁 |
| 107 | ip_reputation | IP信誉 |
| 108 | captcha_config | 验证码配置 |
| 109 | captcha_send_log | 验证码发送日志 |
| 110 | captcha_template | 验证码模板 |
| 111 | security_events | 安全事件 |
| 112 | spam_check_results | 垃圾检查结果 |

### 2.12 认证相关表

| 序号 | 表名 | 说明 |
|------|------|------|
| 113 | password_policy | 密码策略 |
| 114 | password_auth_providers | 密码认证提供商 |
| 115 | registration_captcha | 注册验证码 |
| 116 | cas_services | CAS服务 |
| 117 | cas_proxy_tickets | CAS代理票据 |
| 118 | cas_proxy_granting_tickets | CAS代理授予票据 |
| 119 | cas_slo_sessions | CAS SLO会话 |
| 120 | cas_tickets | CAS票据 |
| 121 | cas_user_attributes | CAS用户属性 |
| 122 | saml_identity_providers | SAML身份提供商 |
| 123 | saml_sessions | SAML会话 |
| 124 | saml_logout_requests | SAML登出请求 |
| 125 | saml_auth_events | SAML认证事件 |
| 126 | saml_user_mapping | SAML用户映射 |

### 2.13 空间相关表

| 序号 | 表名 | 说明 |
|------|------|------|
| 127 | spaces | 空间表 |
| 128 | space_hierarchy | 空间层级 |
| 129 | space_children | 空间子项 |

### 2.14 滑动同步相关表

| 序号 | 表名 | 说明 |
|------|------|------|
| 130 | sliding_sync_rooms | 滑动同步房间 |
| 131 | sync_stream_id | 同步流ID |

### 2.15 保留策略相关表

| 序号 | 表名 | 说明 |
|------|------|------|
| 132 | server_retention_policy | 服务器保留策略 |

### 2.16 推送配置相关表

| 序号 | 表名 | 说明 |
|------|------|------|
| 133 | push_config | 推送配置 |

### 2.17 三方规则相关表

| 序号 | 表名 | 说明 |
|------|------|------|
| 134 | third_party_rule_results | 三方规则结果 |

### 2.18 路由相关表

| 序号 | 表名 | 说明 |
|------|------|------|
| 135 | rendezvous_session | 约会会话 |

### 2.19 统计数据相关表

| 序号 | 表名 | 说明 |
|------|------|------|
| 136 | invites | 邀请统计 |

### 2.20 系统表

| 序号 | 表名 | 说明 |
|------|------|------|
| 137 | schema_migrations | Schema迁移记录 |
| 138 | db_metadata | 数据库元数据 |
| 139 | background_updates | 后台更新 |
| 140 | filters | 过滤器 |
| 141 | modules | 模块 |
| 142 | module_execution_logs | 模块执行日志 |
| 143 | workers | Worker信息 |
| 144 | active_workers | 活跃Worker |
| 145 | worker_events | Worker事件 |
| 146 | worker_commands | Worker命令 |
| 147 | worker_statistics | Worker统计 |
| 148 | rate_limit_callbacks | 速率限制回调 |
| 149 | report_rate_limits | 报告速率限制 |
| 150 | ip_reputation | IP信誉 |
| 151 | voice_usage_stats | 语音使用统计 |

### 2.21 统计扩展表

| 序号 | 表名 | 说明 |
|------|------|------|
| 152 | pg_stat_statements | 统计语句 |
| 153 | pg_stat_statements_info | 统计语句信息 |

---

## 三、索引统计

| 索引类型 | 数量 |
|----------|------|
| 主键索引 | 35+ |
| 外键索引 | 35+ |
| 唯一索引 | 50+ |
| 普通索引 | 350+ |
| **总计** | **478** |

---

## 四、迁移记录

| 版本 | 状态 | 执行时间 |
|------|------|----------|
| v6.0.0 | ✅ | 1774000207596 |
| 00000000_unified_schema_v6 | ✅ | 1774000235315 |
| 20260320000001_rename_must_change_password | ✅ | 1774000235421 |
| 20260320000002_rename_olm_boolean_fields | ✅ | 1774000235519 |
| 20260321000001_add_device_trust_tables | ✅ | 1774000235664 |
| 20260321000003_add_secure_backup_tables | ✅ | 1774000235783 |
| UNIFIED_MIGRATION_v1 | ✅ | 1774000235981 |
| 20260321000005_fix_timestamp_fields | ✅ | 1774013987143 |

---

## 五、PostgreSQL 配置状态

| 参数 | 值 | 状态 |
|------|-----|------|
| shared_buffers | 256MB | ✅ 已优化 |
| work_mem | 16MB | ✅ 已优化 |
| random_page_cost | 1.1 | ✅ SSD优化 |
| effective_io_concurrency | 200 | ✅ 并行I/O |

---

## 六、文档更新记录

| 日期 | 版本 | 更新内容 |
|------|------|----------|
| 2026-03-20 | v1.0.0 | 初始版本，基于数据库实际表结构生成 |
