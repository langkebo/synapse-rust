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

| 维度 | synapse-rust 当前值 | 标准 Synapse 参考值 | 差距 |
|------|---------------------|---------------------|------|
| 数据库表数量 | 236 张 (unified schema 185 CREATE TABLE) | 60-80 张 | **3-4 倍** |
| 迁移脚本 (.sql) | 51 个文件 + 大量 .undo.sql | 按版本管理，约 80 个增量 | 数量不算多但结构混乱 |
| Rust 源码 | ~175,000 行 | N/A (Python ~200k 行) | 非标功能占 ~17,500 行 |
| 测试代码 | ~80,000 行 | N/A | 含大量冗余集成测试 |
| 辅助脚本 (sh+py) | 83+31=114 个 | ~20 个 | **5-6 倍** |
| 文档文件 (md) | 96+ 个 | ~30 个 | **3 倍** |
| 仓库根目录临时文件 | 15 个 (token、log、py) | 0 | 全部应清理 |

### 1.2 核心问题总结

1. **数据库表过多** — 236 张表中约 80 张为非标准功能专用表，约 30 张为过度设计的辅助/统计表
2. **迁移脚本结构混乱** — unified schema (3610 行) 与 20+ 个增量迁移并存，部分增量迁移的内容已被合并到 unified schema，形成双源
3. **非标准功能过度膨胀** — OpenClaw、好友系统、语音消息、CAS、SAML、外部服务集成、Beacon 等非 Matrix 标准功能占代码 ~17,500 行
4. **辅助脚本过度工程化** — 114 个辅助脚本中至少 60 个可以合并或删除
5. **文档膨胀严重** — 96 个 md 文件中至少 40 个是过渡性报告（日期后缀文档），内容高度重叠
6. **仓库卫生差** — 根目录残留 token 文件、日志文件、临时 Python 脚本

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

### 5.1 过度工程化的基础设施

| 模块 | 文件 | 行数 | 问题 | 处置 |
|------|------|------|------|------|
| DB 监控 | `storage/monitoring.rs` | 763 行 | 自建监控系统，应用 Prometheus | 精简为 Prometheus 指标导出 |
| DB 性能评估 | `storage/performance_evaluation.rs` | 487 行 | 过度设计的评估框架 | 删除，用 pg_stat_statements |
| DB 连接池监控 | `storage/pool_monitor.rs` | 267 行 | 与 sqlx 自带指标重复 | 精简为 sqlx metrics 桥接 |
| DB 连接监控 | `storage/connection_monitor.rs` | 439 行 | 过度设计 | 合并到 pool_monitor |
| Schema 校验器 | `storage/schema_validator.rs` | 832 行 | 运行时 schema 校验过重 | 精简为启动时关键表检查 |
| 完整性检查器 | `storage/integrity_checker.rs` | 551 行 | 运行时完整性检查 | 改为离线脚本 |
| 维护计划 | `storage/maintenance_plan.rs` | 455 行 | 自建维护调度 | 删除，用 pg_cron |
| DB 编译时验证 | `storage/compile_time_validation.rs` | 257 行 | 编译时检查 | 保留但精简 |
| 遥测告警 | `services/telemetry_alert_service.rs` | ~300 行 | 自建告警 | 精简为 Prometheus 告警规则 |

预估可精简 **~3,500 行** 存储层基础设施代码。

### 5.2 test_utils 清理

`src/test_utils.rs` (336 行) 提供测试连接池管理，合理保留。但以下应清理:

- 仓库根目录的临时测试脚本 (`analyze_results.py`, `prepare_test_accounts.py`, `register_test_accounts.py`, `.tmp_probe.py`)
- 根目录 token 文件 (`admin_token.txt`, `super_admin_token.txt`, `user_token.txt`, `admin2_token.txt`, `test_tokens.json`)

---

## 六、脚本与工具精简方案

### 6.1 当前状态

`scripts/` 目录下 83 个文件 + 31 个 Python 文件，大量功能重叠。

### 6.2 合并/删除计划

#### 保留（核心，~15 个）

```
run_ci_tests.sh          — CI 测试入口
ci_backend_validation.sh  — CI 后端校验
run_benchmarks.sh          — 基准测试
run_cargo_audit.sh         — 安全审计
backup_database.sh         — 数据库备份
check_schema_table_coverage.py — Schema 门禁
check_schema_contract_coverage.py — 契约门禁
generate_logical_checksum_report.py — 逻辑校验
run_pg_amcheck.py          — PG 完整性
```

#### 合并（功能重叠，~20 个 → ~5 个）

| 合并前 | 合并为 |
|--------|--------|
| `db_consistency_check.sh`, `db_schema_check.sh`, `schema_sync_check.sh`, `schema_validator.sh`, `validate_schema_all.sh`, `foreign_key_check.sh`, `index_check.sh` | `scripts/db/validate.sh` |
| `field_naming_check.sh`, `db_field_audit.py`, `check_field_consistency.sql` | `scripts/db/field_check.sh` |
| `test_migrations.sh`, `verify_migration.sh`, `sqlx_migrate.sh`, `migration_manager.sh` | `scripts/db/migrate.sh` |
| `code_quality_check.sh`, `compile_time_check.sh`, `check_doc_spelling.sh` | `scripts/quality/check.sh` |
| `detect_shell_routes.sh`, `detect_unwired_route_candidates.sh`, `extract_routes.sh` | `scripts/routes/check.sh` |

#### 删除（无明确使用场景或一次性脚本，~30 个）

```
convert_indexes_to_concurrent.sh  — 一次性操作
fix_duplicate_indexes.sh           — 一次性操作
optimize_database.sh               — 应由 DBA 手动执行
clean_cache.sh                     — 运维脚本不应在源码中
sliding_sync_tables.sql            — 应在 migrations/ 中
generate_benchmark_data.sh         — 测试数据生成
build_sqlx_migration_source.py     — 内部工具
generate_migration_manifest.py     — 迁移治理过重
verify_migration_manifest.py       — 同上
audit_migration_layout.py          — 同上
check_external_evidence_complete.py — 企业流程工具
cleanup_test_results.py            — 应在 CI 中处理
analyze_api_matrix.py              — 一次性分析
permission_matrix_probe.py         — 一次性分析
matrix_api_smoke.py                — 被 Rust 测试覆盖
register_admin_correct.py          — 临时工具
```

### 6.3 Allowlist 文件清理

```
scripts/schema_table_coverage_exceptions.txt  — 应随着 schema 收口逐步清空
scripts/shell_routes_allowlist.txt             — 保留但精简
scripts/unwired_route_candidates_allowlist.txt — 保留但精简
scripts/logical_checksum_tables.txt            — 保留
```

---

## 七、文档精简方案

### 7.1 问题

docs/ 下 96 个 md 文件，其中大量是带日期后缀的过渡性报告（如 `*_2026-04-15.md` 出现 15 次），内容高度重叠。

### 7.2 目标结构

```
docs/
├── README.md                        # 文档索引
├── api-error.md                     # API 镜像审查（已有，保留）
├── API_STABILITY.md                 # API 稳定性（保留）
├── MSC_DIFFERENCE_MATRIX.md         # MSC 差异矩阵（保留）
├── PERFORMANCE_BASELINE.md          # 性能基线（保留）
├── db/
│   ├── DATABASE_AUDIT_REPORT.md     # 数据库审计（保留，合并）
│   ├── DIAGNOSIS_REPORT.md          # 诊断报告（保留，合并）
│   ├── MIGRATION_GOVERNANCE.md      # 迁移治理（保留）
│   ├── SCHEMA_VALIDATION_GUIDE.md   # Schema 校验（保留）
│   └── FIELD_MAPPING_REPORT.md      # 字段映射（保留）
├── sdk/                             # SDK 文档（保留全部）
│   ├── README.md
│   ├── admin.md
│   ├── authentication.md
│   └── ...
├── synapse-rust/
│   ├── API_COVERAGE_REPORT.md       # API 覆盖率（保留）
│   ├── permission_matrix.csv        # 权限矩阵（保留）
│   └── OPTIMIZATION_AND_DEDUPLICATION_PLAN_2026-04-21.md  # 本文
├── quality/
│   └── defects_api_integration.md   # 缺陷报告（保留）
└── API-OPTION/
    └── README.md                    # API 优化选项索引（保留，精简子文件）
```

### 7.3 建议删除/归档的文档

以下文档内容已过时或被后续文档取代，建议移入 `docs/archive/`：

```
# 2026-04-15 日期后缀系列（10+ 个，内容高度重叠）
docs/API_CONTRACT_ACTUAL_SITUATION_2026-04-15.md
docs/API_CONTRACT_EXECUTION_RECOMMENDATION_2026-04-15.md
docs/API_CONTRACT_FINAL_EXECUTION_REPORT_2026-04-15.md
docs/API_CONTRACT_FINAL_STATUS_2026-04-15.md
docs/API_CONTRACT_FINAL_SUMMARY_2026-04-15.md
docs/API_CONTRACT_PROJECT_DELIVERY_2026-04-15.md
docs/API_CONTRACT_UPDATE_GUIDE_2026-04-15.md
docs/API_CONTRACT_UPDATE_PLAN_2026-04-15.md
docs/API_CONTRACT_UPDATE_SUMMARY_2026-04-15.md
docs/CURRENT_STATUS_2026-04-15.md
docs/DAILY_SUMMARY_2026-04-15.md
docs/OPTIMIZATION_COMPLETE_2026-04-15.md
docs/OPTIMIZATION_PROGRESS_2026-04-15-EVENING.md
docs/OPTIMIZATION_STATUS_2026-04-15.md
docs/OPTIMIZATION_SUMMARY_2026-04-15.md
docs/FINAL_OPTIMIZATION_SUMMARY_2026-04-15.md
docs/FINAL_WORK_SUMMARY_2026-04-15.md
docs/ULTIMATE_FINAL_SUMMARY_2026-04-15.md
docs/SYSTEMATIC_OPTIMIZATION_EXECUTION_PLAN_2026-04-15.md

# 中间过程文档
docs/AUTH_MD_UPDATE_EXAMPLE_2026-04-15.md
docs/CLONE_ANALYSIS_2026-04-15.md
docs/CLONE_UNWRAP_OPTIMIZATION_2026-04-15.md
docs/CODE_QUALITY_IMPROVEMENTS_2026-04-04.md
docs/DEPENDENCY_ANALYSIS_2026-04-15.md
docs/UNWRAP_PANIC_ANALYSIS_2026-04-15.md
docs/TEST_FAILURES_2026-04-05.md

# db/ 下可合并的文档
docs/db/COMPLETION_REPORT.md          → 合入 DIAGNOSIS_REPORT.md
docs/db/P2_COMPLETION_SUMMARY.md      → 合入 DIAGNOSIS_REPORT.md
docs/db/DB_OPTIMIZATION_TASKS_2026-03-28.csv → 归档
docs/db/DISASTER_RECOVERY_GUIDE.md    → 保留但从 db/ 提升到 docs/
docs/db/MONITORING_GUIDE.md           → 保留但从 db/ 提升到 docs/
docs/db/PERFORMANCE_OPTIMIZATION_GUIDE.md → 合入 docs/PERFORMANCE_BASELINE.md
docs/db/MIGRATION_OPERATIONS_GUIDE.md → 合入 MIGRATION_GOVERNANCE.md
docs/db/MIGRATION_TOOLS_GUIDE.md      → 合入 MIGRATION_GOVERNANCE.md
docs/db/DIAGNOSIS_EXTERNAL_EVIDENCE_TEMPLATE.md → 归档

# synapse-rust/ 下可归档的文档
docs/synapse-rust/ADMIN_OPTIMIZATION_SUMMARY_2026-04-04.md → 归档
docs/synapse-rust/ADMIN_VERIFICATION_MAPPING_2026-04-03.md → 归档
docs/synapse-rust/BACKEND_API_CONTRACT_REMEDIATION_PLAN_2026-04-14.md → 归档
docs/synapse-rust/OPENCLAW_ENABLEMENT_DESIGN_2026-04-16.md → 归档
```

精简后 docs/ 应从 96 个文件降至 **~25 个文件**。

---

## 八、仓库卫生清理方案

### 8.1 根目录临时文件（必须清理）

```bash
# 应添加到 .gitignore 并从版本控制删除
rm admin_token.txt admin2_token.txt super_admin_token.txt user_token.txt
rm test_tokens.json
rm test-results-*.log
rm analyze_results.py prepare_test_accounts.py register_test_accounts.py
rm .tmp_probe.py

# .gitignore 补充
*_token.txt
test_tokens.json
test-results*.log
.tmp_*.py
```

### 8.2 test-results 目录（313 个文件）

```bash
# 测试结果不应提交到版本控制
echo "test-results/" >> .gitignore
echo "test-results-fixed/" >> .gitignore
git rm -r --cached test-results/ test-results-fixed/
```

### 8.3 deploy 日志

```bash
echo "docker/deploy/logs/" >> .gitignore
echo "docker/deploy/test-results-matrix/" >> .gitignore
```

### 8.4 其他

- `DELIVERY_REPORT.md`, `DB_REVIEW_REPORT.md`, `OPTIMIZATION.md` 从仓库根目录移入 `docs/archive/`
- `CHANGELOG_REFACTOR.md`, `CHANGELOG-SECURITY.md` 合并为 `CHANGELOG.md`
- `TESTING_STANDARD.md` 合并到 `TESTING.md`

---

## 九、执行路线图

### Phase 1: 仓库卫生与文档（1-2 天，无风险）

- [ ] 清理根目录临时文件
- [ ] 更新 .gitignore
- [ ] 归档过渡性文档到 docs/archive/
- [ ] 迁移 migrations/ 下的 .md 文件到 docs/db/
- [ ] 合并重复文档

### Phase 2: 数据库 Schema 修复（2-3 天，低风险）

- [ ] 修复 unified schema 中的重复 CREATE TABLE 定义
- [ ] 将已合入 unified schema 的增量迁移归档
- [ ] 统一迁移脚本命名规范
- [ ] 合并回滚目录

### Phase 3: Feature-gate 拆分（3-5 天，中风险）

- [ ] 在 Cargo.toml 中定义 feature flags
- [ ] 将非核心 storage/service/route 代码加上 `#[cfg(feature = "...")]`
- [ ] 拆分 unified schema 为 core + extensions
- [ ] ServiceContainer 引入 ExtensionRegistry 模式
- [ ] 确保 `cargo build` (无 feature) 只编译核心 Matrix 功能

### Phase 4: 冗余表与代码删除（3-5 天，高风险）

- [ ] 验证第三层冗余表的代码引用情况
- [ ] 逐表删除并同步清理存储层
- [ ] 精简语音消息子系统
- [ ] 精简 Beacon 为 MSC3489 基本实现
- [ ] 精简 DB 监控/评估基础设施

### Phase 5: 脚本合并（1-2 天，低风险）

- [ ] 合并 DB 校验脚本
- [ ] 合并质量检查脚本
- [ ] 删除一次性脚本
- [ ] 更新 CI 引用

### Phase 6: 验证与稳定（2-3 天）

- [ ] 完整 CI 测试通过
- [ ] Docker 构建验证
- [ ] 迁移链完整性验证
- [ ] 性能回归测试

**总计预估: 12-20 个工作日**

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

## 附录：精简前后对比预估

| 维度 | 精简前 | 精简后 (仅核心) | 精简后 (全功能) |
|------|--------|-----------------|-----------------|
| 数据库表 | 236 | ~65 | ~115 |
| Rust 代码行数 | ~175,000 | ~145,000 | ~160,000 |
| 迁移文件数 | 51 | ~15 | ~25 |
| 辅助脚本数 | 114 | ~15 | ~25 |
| 文档文件数 | 96 | ~25 | ~25 |
| ServiceContainer 字段 | 100+ | ~35 | ~60 |
| 编译时间 (预估) | 基线 | -20% | -5% |
| Docker 镜像 | 202MB | ~180MB | ~190MB |
