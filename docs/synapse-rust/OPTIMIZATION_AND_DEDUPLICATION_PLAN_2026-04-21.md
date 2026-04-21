# synapse-rust 项目优化精简总方案

> **日期**: 2026-04-21
> **范围**: 全项目（代码、数据库、迁移、脚本、文档、仓库卫生）
> **目标**: 消除过度开发与冗余，保留 Matrix 标准功能与必要扩展，降低维护成本
> **参考基线**: [element-hq/synapse](https://github.com/element-hq/synapse) (Python 参考实现)

---

## 目录

- [一、现状总览与核心问题](#一现状总览与核心问题)
- [二、数据库表精简方案](#二数据库表精简方案)
- [三、迁移脚本治理方案](#三迁移脚本治理方案)
- [四、非标准功能模块化与裁剪方案](#四非标准功能模块化与裁剪方案)
- [五、代码层面精简方案](#五代码层面精简方案)
- [六、脚本与工具精简方案](#六脚本与工具精简方案)
- [七、文档精简方案](#七文档精简方案)
- [八、仓库卫生清理方案](#八仓库卫生清理方案)
- [九、执行路线图](#九执行路线图)
- [十、风险与回滚策略](#十风险与回滚策略)

---

## 一、现状总览与核心问题

### 1.1 数量级对比

| 维度 | 优化前 | 优化后 | 变化 |
|------|--------|--------|------|
| 数据库表数量 | 236 张 | 236 张 (feature-gate 隔离，待后续按需裁剪) | 结构化隔离 |
| Rust 源码 | ~175,840 行 | ~173,067 行 | -2,773 行 |
| 辅助脚本 | 73 个 | 25 个 | **-66%** |
| 文档文件 (活跃) | 96+ 个 | 28 个 | **-71%** |
| 文档文件 (归档) | 0 | 82 个 | 仅归档不删除 |
| 根目录临时文件 | 15 个 | 0 | **-100%** |
| Feature flags | 4 个 | 16 个 | +12 个扩展 feature |
| Benchmark 文件 (孤立) | 7 | 0 | **-100%** |
| 测试文件 (孤立) | 7 | 0 | **-100%** |

### 1.2 核心问题总结与处置状态

| 问题 | 状态 | 处置 |
|------|------|------|
| **数据库表过多** (236 张) | 🟡 结构化隔离 | 非核心表已通过 feature-gate 隔离，物理删除需后续逐表验证 |
| **迁移脚本结构混乱** | ✅ 已治理 | 冗余迁移归档，空目录删除，文档外迁 |
| **非标准功能过度膨胀** (~17,500 行) | ✅ Feature-gate | 12 个 feature flag 隔离非核心模块，默认全启用保持兼容 |
| **辅助脚本过度工程化** (73 个) | ✅ 精简 66% | 48 个冗余脚本已删除 |
| **文档膨胀严重** (96+ 个) | ✅ 精简 71% | 82 个文档归档，28 个活跃 |
| **仓库卫生差** (15 个临时文件) | ✅ 全部清理 | .gitignore 防止再次提交 |
| **过度工程化基础设施** (~3,800 行) | ✅ 大部分完成 | 已删除 4,664 行（含 storage/models 死代码目录），剩余 ~2,191 行有活跃消费者 |

---

## 二、数据库表精简方案

### 2.1 原则

- **保留**: Matrix 规范要求的核心表、Matrix MSC 实验性但已在客户端广泛使用的表
- **Feature-gate**: 非 Matrix 标准但项目需要的扩展功能表（好友、AI、隐私等）
- **删除**: 无代码引用的孤立表、过度设计的统计/审计表、可由其他表替代的冗余表

### 2.2 表分层分类

#### 第一层：Matrix 核心表（保留，约 65 张）

这些表对应 Matrix Client-Server API / Federation API 的核心功能，必须保留：

```
users, access_tokens, refresh_tokens, refresh_token_families,
refresh_token_rotations, refresh_token_usage, token_blacklist,
password_history, password_reset_tokens, registration_tokens,
registration_token_usage, login_tokens, account_validity,
user_external_ids, openid_tokens,
devices, device_keys, device_signatures, device_trust_status,
cross_signing_keys, cross_signing_trust, key_backups, backup_keys,
olm_accounts, olm_sessions, one_time_keys, megolm_sessions,
e2ee_key_requests, e2ee_security_events,
rooms, room_aliases, room_depth, room_state_events, room_events,
room_memberships, room_invites, room_account_data, room_directory,
room_tags, room_ephemeral, room_parents, room_summaries,
events, event_auth, event_receipts, event_signatures,
event_relations, redactions, event_reports,
notifications, pushers, push_devices, push_rules,
push_notification_queue, push_notification_log,
typing, read_markers, to_device_messages,
user_directory, presence, user_account_data, account_data,
user_threepids, user_filters,
sliding_sync_rooms, sliding_sync_lists, sliding_sync_tokens,
sync_stream_id,
application_services, application_service_rooms,
application_service_room_alias_namespaces,
application_service_room_namespaces,
application_service_user_namespaces,
application_service_state, application_service_transactions,
application_service_events,
federation_servers, federation_signing_keys, federation_queue,
federation_blacklist,
media_metadata, thumbnails,
schema_migrations, db_metadata, background_updates
```

#### 第二层：已使用的扩展功能表 — Feature-gate 化（约 50 张）

这些表对应项目自定义扩展功能，应通过 feature flag 控制建表：

| 功能域 | 表名 | 处置 |
|--------|------|------|
| **好友系统** | friends, friend_requests, friend_categories, invitation_blocks | feature-gate `friends` |
| **OpenClaw/AI** | ai_chat_roles, ai_connections, ai_conversations, ai_generations, ai_messages, openclaw_connections | feature-gate `openclaw` |
| **语音消息** | voice_messages, voice_usage_stats | feature-gate `voice` (或合并到标准 media) |
| **SAML SSO** | saml_identity_providers, saml_sessions, saml_logout_requests, saml_auth_events, saml_user_mapping | feature-gate `saml` |
| **CAS SSO** | cas_services, cas_proxy_tickets, cas_proxy_granting_tickets, cas_slo_sessions, cas_tickets, cas_user_attributes | feature-gate `cas` |
| **Beacon/位置** | beacon_info, beacon_locations | feature-gate `beacons` |
| **VoIP 会话** | call_sessions, call_candidates, matrixrtc_sessions, matrixrtc_memberships, matrixrtc_encryption_keys | feature-gate `voip-tracking` |
| **阅后即焚** | (无持久化表，纯内存) | 保持现状 |
| **隐私扩展** | user_privacy_settings | feature-gate `privacy-ext` |
| **功能开关** | feature_flags, feature_flag_targets | 保留（运维必需） |
| **审计日志** | audit_events | 保留（安全必需） |
| **Widget** | widgets, widget_permissions, widget_sessions | feature-gate `widgets` |
| **Server Notification** | server_notifications, user_notification_status, notification_templates, notification_delivery_log, scheduled_notifications | feature-gate `server-notifications` |

#### 第三层：建议删除的冗余表（约 40 张）

| 表名 | 删除理由 |
|------|----------|
| `room_summary_members` | 可由 room_memberships + room_summaries 运行时计算 |
| `room_summary_state` | 与 room_state_events 数据重复 |
| `room_summary_stats` | 可由 room_summaries 运行时聚合 |
| `room_summary_update_queue` | 后台更新队列可用通用 background_updates 替代 |
| `retention_cleanup_queue` | 可合并到 background_updates |
| `retention_cleanup_logs` | 过度审计，可用日志替代 |
| `retention_stats` | 运行时统计可用 SQL 聚合替代 |
| `event_report_history` | 与 event_reports 数据重复 |
| `event_report_stats` | 运行时聚合替代 |
| `room_children` | 与 space_children 功能重复 |
| `space_events` | 可合并到 events 表（用 event_type 过滤） |
| `space_statistics` | 运行时聚合替代 |
| `space_summaries` | 与 room_summaries 概念重叠 |
| `space_members` | 与 room_memberships 概念重叠 |
| `thread_replies` | 可通过 event_relations (rel_type='m.thread') 查询 |
| `thread_read_receipts` | 可合并到 event_receipts (增加 thread_id 列) |
| `thread_relations` | 与 event_relations 功能重复 |
| `thread_statistics` (重复定义) | unified schema 中定义了两次 |
| `ip_reputation` (重复定义) | 同一表在 unified schema 中出现两次 |
| `reports` (重复定义) | unified schema 中定义了两次 |
| `threepids` (重复定义) | 与 user_threepids 重复 |
| `user_directory_profiles` | 可合并到 user_directory |
| `presence_routes` | 无明确使用场景 |
| `presence_subscriptions` | Synapse 已弃用此功能 |
| `user_stats` | 运行时聚合替代 |
| `group_memberships` | Matrix Groups (communities) 已弃用，被 Spaces 替代 |
| `private_messages` | 非标概念，Matrix 用 DM room |
| `private_sessions` | 同上 |
| `registration_captcha` | 与 captcha_config/captcha_send_log 功能重叠 |
| `password_auth_providers` | 非标准，可用 OIDC 替代 |
| `password_policy` | 可内嵌到配置文件 |
| `push_config` | 可内嵌到配置文件 |
| `third_party_rule_results` | 无明确使用场景 |
| `invites` | 与 room_invites 重复 |
| `rate_limit_callbacks` | 过度设计，限流回调可在代码中处理 |
| `spam_check_results` | 可用日志替代 |
| `deleted_events_index` | 可通过 events 表 + 状态字段实现 |
| `key_signatures` | 与 device_signatures 功能重叠 |
| `key_rotation_history` | 可由 key_rotation_log 替代 |
| `worker_load_stats` | 过度设计，可由 Prometheus 指标替代 |
| `worker_task_assignments` | 过度设计 |
| `worker_connections` | 过度设计 |
| `lazy_loaded_members` | 可由运行时状态管理 |

### 2.3 执行策略

**不是一步删除，而是分三步收敛：**

1. **Phase 1 — 合并重复定义**: 修复 unified schema 中的重复 CREATE TABLE（thread_statistics, ip_reputation, reports, threepids），这是纯 bug 修复
2. **Phase 2 — Feature-gate 隔离**: 将非核心表移入条件建表脚本（按 feature flag 控制），unified schema 只保留 Matrix 核心表
3. **Phase 3 — 删除冗余表**: 验证无代码引用后删除冗余表，同步清理对应的 storage/service 代码

---

## 三、迁移脚本治理方案

### 3.1 当前问题

| 问题 | 严重程度 | 说明 |
|------|----------|------|
| Unified schema 3610 行过于庞大 | P1 | 难以审查和维护 |
| 增量迁移与 unified schema 双源 | P1 | 部分增量迁移的 DDL 已合入 unified schema，但增量脚本仍保留 |
| .undo.sql 与迁移脚本 1:1 配套但缺乏验证 | P2 | 回滚脚本是否真正可用未经测试 |
| migrations/ 目录混入文档 (.md) | P2 | CONSOLIDATION_PLAN.md, DUPLICATE_INDEX_FIX_PLAN.md 等应在 docs/ |
| 迁移命名不统一 | P2 | 有 `20260328_p1_indexes.sql` 也有 `20260328000003_xxx.sql` |
| hotfix/ rollback/ undo/ 三种撤回目录并存 | P2 | 应统一为一种 |

### 3.2 目标架构

```
migrations/
├── 00000000_core_schema.sql              # 核心 Matrix 表（拆分自 unified_schema_v6）
├── 00000001_extensions_friends.sql       # Feature-gated: 好友系统表
├── 00000001_extensions_openclaw.sql      # Feature-gated: OpenClaw 表
├── 00000001_extensions_saml.sql          # Feature-gated: SAML 表
├── 00000001_extensions_cas.sql           # Feature-gated: CAS 表
├── 00000001_extensions_voice.sql         # Feature-gated: 语音消息表
├── 00000001_extensions_beacon.sql        # Feature-gated: Beacon 表
├── 00000001_extensions_voip.sql          # Feature-gated: VoIP 会话跟踪表
├── 00000001_extensions_widgets.sql       # Feature-gated: Widget 表
├── 00000001_extensions_notifications.sql # Feature-gated: 通知扩展表
├── YYYYMMDD_HHMMSS_description.sql       # 增量迁移（仅未合入核心的变更）
├── YYYYMMDD_HHMMSS_description.undo.sql  # 配套回滚
├── archive/                              # 已合入 core_schema 的历史迁移
│   └── ...
└── README.md                             # 迁移说明（唯一允许的 .md 文件）
```

### 3.3 执行步骤

1. **拆分 unified schema**:
   - 将 `00000000_unified_schema_v6.sql` 拆为 `00000000_core_schema.sql`（仅核心 Matrix 表，预估 ~1500 行）
   - 各 feature 的扩展表独立为 `00000001_extensions_*.sql`
   - 所有 extension schema 带 `IF NOT EXISTS` 保证幂等

2. **归档已合入的增量迁移**:
   - 以下迁移已被 unified schema 吸收，移入 `archive/`:
     - `20260328_p1_indexes.sql`
     - `20260328000003_add_invite_restrictions_and_device_verification_request.sql`
     - `20260329_p2_optimization.sql`
     - `20260329000100_add_missing_schema_tables.sql`
     - `99999999_unified_incremental_migration.sql`
   - 在 `schema_migrations` 表中保留已执行记录，确保已部署环境不会重复执行

3. **统一回滚目录**:
   - 保留 `.undo.sql` 后缀约定
   - 删除 `rollback/` `hotfix/` `undo/` 空目录
   - 所有回滚脚本与迁移脚本同目录

4. **迁移文档外迁**:
   - 将 `CONSOLIDATION_PLAN.md`, `DUPLICATE_INDEX_FIX_PLAN.md`, `MIGRATION_INDEX.md`, `DATABASE_FIELD_STANDARDS.md`, `SCHEMA_OPTIMIZATION_REPORT.md`, `MANIFEST-template.txt`, `migration_layout_audit.json` 移入 `docs/db/`
   - migrations/ 只保留 `.sql` 文件和 `README.md`

5. **统一命名规范**:
   - 增量迁移统一为 `YYYYMMDDHHMMSS_description.sql`
   - 不再使用 `_p1_`, `_p2_` 等临时标签

### 3.4 新环境建库流程

```bash
# 核心表（必选）
sqlx migrate run  # 自动执行 00000000_core_schema.sql

# 扩展表（按需）
psql -f migrations/00000001_extensions_friends.sql     # 如果需要好友功能
psql -f migrations/00000001_extensions_openclaw.sql     # 如果需要 AI 功能
# ... 按需选择
```

---

## 四、非标准功能模块化与裁剪方案

### 4.1 功能分类与处置决策

| 功能 | 代码量 | 与 Matrix 标准关系 | 处置决策 | 理由 |
|------|--------|---------------------|----------|------|
| **OpenClaw AI 平台** | ~2,634 行 | 完全无关 | **Feature-gate + 独立模块** | 完整的 AI SaaS 产品不应耦合到 homeserver |
| **好友系统** | ~3,267 行 | 完全无关 | **Feature-gate + 独立模块** | 微信式好友图谱是社交网络功能，非 Matrix 范畴 |
| **语音消息** | ~2,285 行 | 重复 | **精简为标准 media 适配层** | Matrix 用 m.audio + org.matrix.msc3245.voice，不需要独立子系统 |
| **SAML SSO** | ~2,642 行 | 合理扩展 | **Feature-gate，修复手写 XML parser** | 企业场景有需求，但应使用正规 SAML 库 |
| **CAS SSO** | ~1,347 行 | 已弃用协议 | **Feature-gate，标记弃用** | CAS 已被 OIDC 取代，原 Synapse 也已弃用 |
| **外部服务集成** | ~1,271 行 | 完全无关 | **Feature-gate + 独立模块** | TrendRadar/webhook 是独立产品 |
| **Beacon/位置** | ~1,073 行 | MSC3489 扩展 | **精简为 MSC3489 基本实现** | 移除超出 MSC 的附加计算（距离、统计） |
| **AI Connection/MCP** | ~689 行 | 完全无关 | **合并到 OpenClaw 模块** | MCP proxy 不应内嵌 homeserver |
| **阅后即焚** | ~642 行 | 完全无关 | **Feature-gate** | 纯内存实现，可选功能 |
| **隐私扩展** | ~285 行 | 轻度扩展 | **保留，Feature-gate** | 合理的隐私增强 |
| **Feature Flags** | ~729 行 | 运维扩展 | **保留** | 运维必需能力 |
| **E2EE 泄露检测** | ~281 行 | 安全扩展 | **保留** | 安全增强能力 |

### 4.2 模块化实施方案

使用 Cargo features 控制非核心功能的编译：

```toml
# Cargo.toml
[features]
default = []
friends = []
openclaw = ["openclaw-routes"]
openclaw-routes = []
voice-extended = []
saml = []
cas = []
beacons = []
voip-tracking = []
widgets = []
server-notifications = []
burn-after-read = []
privacy-ext = []
all-extensions = ["friends", "openclaw", "voice-extended", "saml", "cas",
                  "beacons", "voip-tracking", "widgets", "server-notifications",
                  "burn-after-read", "privacy-ext"]
```

### 4.3 ServiceContainer 瘦身

当前 `ServiceContainer` 有 **100+ 个字段**，每个 feature 的 storage + service 都被直接挂载。应改为：

```rust
// 当前：100+ 个 pub 字段全部无条件初始化
pub struct ServiceContainer {
    // ... 100+ fields
}

// 目标：核心字段 + Extension Registry
pub struct ServiceContainer {
    // Matrix 核心 (~30 fields)
    pub user_storage: UserStorage,
    pub room_storage: RoomStorage,
    pub event_storage: EventStorage,
    // ...

    // 扩展模块注册表
    extensions: ExtensionRegistry,
}

impl ServiceContainer {
    pub fn get_extension<T: Extension>(&self) -> Option<&T> { ... }
}
```

这样非核心功能的初始化仅在对应 feature 启用时发生，不再膨胀核心启动路径。

### 4.4 语音消息具体精简方案

**当前**: 完整子系统（文件存储 + DB tracking + 波形 + 转写 + 统计 + Redis 缓存）

**目标**: 薄适配层，使用标准 media 基础设施

```
删除:
  - src/storage/voice.rs (531 行) — 独立存储层
  - src/services/voice_service.rs (1,162 行) — 独立服务层
  - voice_messages, voice_usage_stats 表

保留/改造:
  - src/web/routes/voice.rs → 改为 media 上传 + m.audio content type 的适配器 (~100 行)
  - 语音消息作为标准 media 上传，元数据存入 event content
```

---

## 五、代码层面精简方案

### 5.1 过度工程化的基础设施 (部分完成)

| 模块 | 文件 | 行数 | 问题 | 处置 | 状态 |
|------|------|------|------|------|------|
| DB 性能评估 | `storage/performance_evaluation.rs` | 407 行 | 过度设计的评估框架 | 删除 | ✅ 已删除 |
| DB 连接监控 | `storage/connection_monitor.rs` | 394 行 | 过度设计 | 删除 | ✅ 已删除 |
| 完整性检查器 | `storage/integrity_checker.rs` | 504 行 | 运行时完整性检查 | 删除 | ✅ 已删除 |
| 维护计划 | `storage/maintenance_plan.rs` | 408 行 | 自建维护调度 | 删除 | ✅ 已删除 |
| DB 编译时验证 | `storage/compile_time_validation.rs` | 273 行 | 无外部引用 | 删除 | ✅ 已删除 |
| 死代码文件 | `batch.rs`, `connection_pool.rs`, `query_utils.rs` | 784 行 | 未在 mod.rs 声明 | 删除 | ✅ 已删除 |
| DB 监控 | `storage/monitoring.rs` | 743 行 | 自建监控系统 | 精简为 Prometheus 指标导出 | 🔲 后续优化 |
| DB 连接池监控 | `storage/pool_monitor.rs` | 268 行 | 与 sqlx 自带指标重复 | 精简为 sqlx metrics 桥接 | 🔲 后续优化 |
| Schema 校验器 | `storage/schema_validator.rs` | 819 行 | 运行时 schema 校验过重 | 精简为启动时关键表检查 | 🔲 后续优化 |
| 遥测告警 | `services/telemetry_alert_service.rs` | ~361 行 | 自建告警 | 精简为 Prometheus 告警规则 | 🔲 后续优化 |

已精简 **~2,770 行** 存储层基础设施代码，剩余 ~2,191 行待后续优化。

### 5.2 test_utils 清理 ✅ 已完成

`src/test_utils.rs` (336 行) 提供测试连接池管理，合理保留。以下已清理:

- ✅ 仓库根目录的临时测试脚本 — 已从 git 移除并加入 .gitignore
- ✅ 根目录 token 文件 — 已从 git 移除并加入 .gitignore
- ✅ 孤立测试文件 (tests/ 根目录) — 6 个文件已删除
- ✅ 孤立 benchmark 文件 (benches/) — 7 个文件已删除
- ✅ test-artifacts/ — 已删除并加入 .gitignore

---

## 六、脚本与工具精简方案 ✅ 已完成

### 6.1 精简结果

脚本从 73 个 (git tracked) 精简到 **25 个**，减少 66%。

### 6.2 保留的核心脚本 (25 个)

| 类别 | 脚本 | 用途 |
|------|------|------|
| CI | `run_ci_tests.sh` | CI 测试入口 |
| CI | `ci_backend_validation.sh` | CI 后端校验 |
| CI | `ci/critical_migrations.txt` | 关键迁移清单 |
| 安全 | `run_cargo_audit.sh` | 安全审计 |
| 数据库 | `backup_database.sh` | 数据库备份 |
| 数据库 | `check_schema_table_coverage.py` | Schema 门禁 |
| 数据库 | `check_schema_contract_coverage.py` | 契约门禁 |
| 数据库 | `generate_logical_checksum_report.py` | 逻辑校验 |
| 数据库 | `run_pg_amcheck.py` | PG 完整性 |
| 数据库 | `logical_checksum_tables.txt` | 关键表清单 |
| 数据库 | `schema_table_coverage_exceptions.txt` | 例外清单 |
| 质量 | `check_doc_spelling.sh` | 文档拼写检查 |
| 质量 | `api_schema_verify.sh` | API Schema 验证 |
| 路由 | `shell_routes_allowlist.txt` | Shell 路由白名单 |
| 路由 | `unwired_route_candidates_allowlist.txt` | 未挂载路由白名单 |
| 基准 | `run_benchmarks.sh` | 基准测试 |
| 测试 | `test/api-integration_test.sh` | API 集成测试 |
| 测试 | `test/api-integration-core.sh` | 核心 API 测试 |
| 测试 | `test/api-integration-full.sh` | 全量 API 测试 |
| 测试 | `test/api-integration-optional.sh` | 可选 API 测试 |
| 测试 | `test/run_sdk_verification_real_backend.sh` | SDK 后端验证 |
| 性能 | `test/perf/api_matrix_core.js` | 性能测试脚本 |
| 性能 | `test/perf/api_matrix_core.sh` | 性能测试入口 |
| 性能 | `test/perf/guardrail.py` | 性能护栏 |
| 性能 | `test/perf/run_tests.sh` | 性能测试运行 |

### 6.3 已删除的脚本 (48 个)

包括: 重复 DB 校验脚本(10 个)、迁移管理脚本(4 个)、一次性修复脚本(5 个)、
路由/代码检测脚本(5 个)、质量检查脚本(6 个)、DB 工具子目录(8 个)、
分析/探测脚本(6 个)、其他冗余脚本(4 个)。

---

## 七、文档精简方案 ✅ 已完成

### 7.1 精简结果

活跃文档从 96+ 个精简到 **28 个**，减少 71%。82 个过渡性/冗余文档归入 `docs/archive/`。

### 7.2 当前活跃文档结构

```
docs/
├── api-error.md                     # API 镜像审查报告
├── API_STABILITY.md                 # API 稳定性规范
├── MSC_DIFFERENCE_MATRIX.md         # MSC 差异矩阵
├── PERFORMANCE_BASELINE.md          # 性能基线
├── API-OPTION/
│   └── README.md                    # API 优化选项索引
├── db/
│   ├── DATABASE_AUDIT_REPORT.md     # 数据库审计报告
│   ├── DATABASE_FIELD_STANDARDS.md  # 字段命名标准
│   ├── DIAGNOSIS_REPORT.md          # 数据库诊断报告
│   ├── FIELD_MAPPING_REPORT.md      # 字段映射报告
│   ├── MIGRATION_GOVERNANCE.md      # 迁移治理方案
│   ├── MIGRATION_INDEX.md           # 迁移索引
│   └── SCHEMA_VALIDATION_GUIDE.md   # Schema 校验指南
├── quality/
│   └── defects_api_integration.md   # API 集成缺陷
├── sdk/                             # SDK 文档 (9 个)
│   ├── README.md, admin.md, authentication.md,
│   │   e2ee.md, errors.md, friends.md,
│   │   media.md, messages.md, rooms.md
├── synapse-rust/
│   ├── admin-registration-guide.md  # 管理员注册指南
│   ├── api (文件)                    # API 状态
│   ├── API_COVERAGE_REPORT.md       # API 覆盖率
│   ├── API_SECURITY_VERIFICATION_REPORT.md # API 安全验证
│   ├── OPTIMIZATION_AND_DEDUPLICATION_PLAN_2026-04-21.md # 本文
│   └── permission_matrix.csv        # 权限矩阵
└── archive/                         # 归档 (82 个文件)
```

### 7.3 已归档的文档分类

- 2026-04-15 日期后缀过渡性报告: 19 个
- 中间过程/分析文档: 7 个
- 已完结的优化计划: 9 个 (API-OPTION 子文件)
- 已合入主报告的 db/ 子文档: 12 个
- 已完结的安全/质量审计: 6 个
- 其他过渡性文档: 29 个

---

## 八、仓库卫生清理方案 ✅ 已完成

### 8.1 根目录临时文件 ✅

已从 git tracking 移除并加入 .gitignore:
- `admin_token.txt`, `admin2_token.txt`, `super_admin_token.txt`, `user_token.txt`
- `test_tokens.json`
- `.tmp_probe.py`, `analyze_results.py`, `prepare_test_accounts.py`, `register_test_accounts.py`

### 8.2 测试产物 ✅

- `test-results-fixed/` — 已从 git 移除，加入 .gitignore
- `docker/deploy/test-results-matrix/` — 已从 git 移除，加入 .gitignore
- `test-artifacts/` — 已删除，加入 .gitignore
- `.DS_Store` 文件 — 已移除，加入 .gitignore

### 8.3 根目录文件整理 ✅

- `DELIVERY_REPORT.md`, `DB_REVIEW_REPORT.md`, `OPTIMIZATION.md` → `docs/archive/`
- `CHANGELOG_REFACTOR.md`, `CHANGELOG-SECURITY.md` → `docs/archive/`
- `TESTING_STANDARD.md` → `docs/archive/`
- `docker-compose.federation-test.yml` → `docker/`

---

## 九、执行路线图与完成状态

> 最后更新: 2026-04-21

### Phase 1: 仓库卫生与文档 ✅ 已完成

- [x] 清理根目录临时文件（token/py/log 共 15 个文件从 git 移除）
- [x] 更新 .gitignore（防止凭证、测试产物再次入库）
- [x] 归档过渡性文档到 docs/archive/（82 个文件已归档）
- [x] 迁移 migrations/ 下的 .md 文件到 docs/db/ 或 docs/archive/
- [x] 清理 test-results-fixed/ 和 docker/deploy/test-results-matrix/ 测试产物
- [x] 清理 test-artifacts/ 目录
- [x] 移动根目录 docker-compose.federation-test.yml 到 docker/
- [x] 清理 .DS_Store 文件

### Phase 2: 迁移脚本治理 ✅ 已完成

- [x] 验证 unified schema 中的重复 CREATE TABLE 定义（已确认均为注释状态，无需修复）
- [x] 将已合入 unified schema 的增量迁移归档（4 个迁移到 migrations/archive/）
- [x] 删除空 rollback/hotfix 目录及配套回滚脚本

### Phase 3: Feature-gate 拆分 ✅ 已完成

- [x] 在 Cargo.toml 中定义 12 个 feature flags + all-extensions 元特征
- [x] 将非核心 storage 模块加上 `#[cfg(feature = "...")]`
- [x] 将非核心 service 模块加上 `#[cfg(feature = "...")]`
- [x] 将非核心 route 模块加上 `#[cfg(feature = "...")]`
- [x] ServiceContainer 字段按 feature 条件编译
- [x] ServiceContainer::new() 初始化逻辑按 feature 条件编译
- [x] 路由装配 assembly.rs 按 feature 条件挂载
- [x] 默认启用 all-extensions 保持向后兼容
- [x] `cargo check --all-features` 通过
- [x] `cargo clippy --all-features -- -D warnings` 通过

### Phase 4: 冗余代码与数据库表删除 ✅ 已完成

- [x] 删除过度工程化的存储基础设施（5 个文件 ~2,000 行）:
  - maintenance_plan.rs — 自建维护调度（应用 pg_cron）
  - performance_evaluation.rs — 过度设计的评估框架（应用 pg_stat_statements）
  - integrity_checker.rs — 运行时完整性检查（改为离线脚本）
  - connection_monitor.rs — 与 pool_monitor 功能重叠
  - compile_time_validation.rs — 无外部引用的编译时检查
- [x] 删除死代码存储文件（batch.rs, connection_pool.rs, query_utils.rs ~784 行）
- [x] 删除死代码模型目录（storage/models/ 13 个文件 ~1,857 行 — 未在 mod.rs 声明）
- [x] 删除孤立测试文件（6 个文件 ~1,175 行）
- [x] 删除孤立 benchmark 文件（7 个文件）
- [x] 删除 Docker 测试/性能产物（9 个文件）
- [x] 删除 docker/deploy 冗余文档与脚本（8 个文件）
- [x] 移除 .trae/ IDE 配置目录 (47 个文件) 并加入 .gitignore
- [x] 验证并移除 3 张零引用数据库死表:
  - private_sessions — 零 DML 引用
  - private_messages — 仅 schema_health_check 列存在性检查（已移除）
  - room_children — 仅 schema_validator 契约定义（已移除）
- [x] 从 unified schema 移除 3 张死表定义 (-61 行)
- [x] 新增 20260421000001_drop_unused_tables.sql 迁移 + undo
- [x] 清理 schema_validator.rs 和 schema_health_check.rs 中的死表引用

### Phase 5: 脚本精简 ✅ 已完成

- [x] 删除 48 个冗余/一次性/功能重叠脚本（从 73 → 25）
- [x] 合并后保留的核心脚本：CI 入口、安全审计、schema 门禁、逻辑校验、测试脚本

### Phase 6: 验证 ✅ 已完成

- [x] `cargo check --all-features --locked` 通过
- [x] `cargo clippy --all-features --locked -- -D warnings` 通过（零警告）

---

### 后续可选优化（不阻塞当前交付）

| 项目 | 风险 | 说明 |
|------|------|------|
| 拆分 unified schema 为 core + extensions | 高 | 需逐表验证依赖，影响已部署环境迁移链 |
| 修复最小构建（--no-default-features --features server） | 中 | 需逐一处理 ~100 处跨模块引用 |
| 继续删除冗余数据库表 | 高 | 剩余候选表均有活跃代码引用，需重构后才能删除 |
| 精简 monitoring.rs / pool_monitor.rs / schema_validator.rs | 中 | 有活跃消费者，需接入 Prometheus 后替换 |
| 精简语音消息子系统为标准 media 适配器 | 中 | 涉及 API 契约变更 |
| 精简 Beacon 为 MSC3489 基本实现 | 中 | 需移除距离计算/统计附加功能 |

---

## 十、风险与回滚策略

### 10.1 风险矩阵

| 风险 | 概率 | 影响 | 缓解措施 |
|------|------|------|----------|
| Feature-gate 后遗漏条件编译 | 高 | 编译失败 | CI 矩阵测试 default + all-extensions |
| 删除冗余表影响运行中功能 | 中 | 功能不可用 | 先标记 deprecated 再删除，预留一个版本 |
| 迁移脚本重组影响已部署环境 | 低 | 部署失败 | schema_migrations 表保留所有历史记录 |
| 文档归档丢失必要信息 | 低 | 知识损失 | 只归档不删除，archive/ 可随时查阅 |

### 10.2 关键原则

1. **先 gate 后删** — 非核心功能先用 feature flag 隔离，确认无影响后再考虑删除
2. **先测后动** — 每个 phase 完成后跑完整 CI，不跨 phase 积累变更
3. **只归档不删除** — 文档和迁移脚本移到 archive/ 而非直接删除
4. **保留迁移历史** — schema_migrations 表中的执行记录不可删除

---

## 附录：精简前后实际对比

> 最后更新: 2026-04-21 (第三轮优化后)

| 维度 | 精简前 | 精简后 (当前) | 变化 |
|------|--------|---------------|------|
| Rust 代码行数 | ~175,840 | ~171,176 | **-4,664 行** |
| 辅助脚本数 | 73 (git tracked) | 25 | **-66%** |
| 文档文件数 (活跃) | 96+ | 28 | **-71%** |
| 文档文件数 (归档) | 0 | 82 | 仅归档不删除 |
| 迁移文件数 (活跃) | 51 | 44+2 | +drop migration |
| Unified schema 行数 | 3,610 | 3,549 | -61 行 (移除 3 张死表) |
| 测试文件 (孤立) | 7 | 0 | -7 文件 |
| Benchmark 文件 (孤立) | 7 | 0 | -7 文件 |
| 根目录临时文件 | 15 | 0 | 全部清理 |
| IDE 配置文件 (.trae/) | 47 tracked | 0 tracked | gitignored |
| Docker 测试/报告产物 | 12+ | 0 | 已清理 |
| 死代码模型目录 (storage/models/) | 1,857 行 | 0 | **已删除** |
| 数据库死表 (schema 移除) | 3 张 | 0 | private_sessions/private_messages/room_children |
| Feature flags | 4 | 16 | +12 个扩展 feature |
| ServiceContainer 条件字段 | 0 | 18 | 18 个字段按 feature 编译 |
