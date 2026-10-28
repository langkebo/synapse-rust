# Changelog

> 所有项目的显著变更都会记录在此文件。
>
> 格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
> 版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。
>
> **包版本**: v6.3.0（以 `Cargo.toml` 为准）｜**迁移/数据库基线**: v12（迁移 `00000000_unified_schema_v12.sql` 单一基线）
>
> 说明：`v12` 指 **数据库迁移/schema 基线**（当前为 `migrations/00000000_unified_schema_v12.sql` 单一 forward 基线），与包版本号（SemVer，见 `Cargo.toml`）是两套独立口径，二者无需相等。
>
> 说明：2026-09 模块化拆分**前**的历史条目沿用当时的单 crate 路径（`src/web/`、`src/services/`、`src/e2ee/`、`src/cache/`）；拆分后现行路径为 `synapse-web/src/`、`synapse-services/src/`、`synapse-e2ee/src/` 等。历史条目按其时布局保留、不回改，以免曲解变更史。

---

## 版本速查

| 版本 | 发布日期 | 性质 | 主要内容 |
|------|----------|------|----------|
| [v6.3.0](#v630---2026-10-28) | 2026-10-28 | 功能+安全 | worker 多进程拓扑 W-A…W-I / 私有端点前缀治理 / 隐私与 E2EE 加固 B-6…B-9 / Synapse v1.162 对齐 |
| [v6.2.0](#v620---2026-07-22) | 2026-07-22 | 重构 | 架构优化 Round 2（收口）：test_utils 类型修复 / clippy expect-used 门禁 / 1291 文件 trait 迁移 |
| [v6.1.0](#v610---2026-07-14) | 2026-07-14 | 重构+安全 | 架构优化 Round 2：Auth trait 分层 / EventStore 读写分离 / 安全加固 / Bench+Snapshot |
| [v6.0.5](#v605---2026-07-09) | 2026-07-09 | 重构 | 架构优化 Round 2：trait 提取 / glob re-export 收敛 / 中间件迁移 / storage 拆分 |
| [v10.0.0](#v10000---2026-06-12) | 2026-06-12 | 迁移基线（DB） | P0/P1 全部修复 / v10 迁移基线 / clippy 门禁修复 / 文档同步 / P2 #7 L1 unwrap 治理 |
| [v8.0.0](#v8000---2026-06-06) | 2026-06-06 | 历史 | P0 全部修复 / v8 迁移基线 / E2EE vodozemac 收敛 Phase 1+2 / Step 10-12 工程门禁 |
| [v7.x](#v7x---2026-05-28-前) | 2026-05-28 前 | 历史 | 旧 `Cargo.toml` 版本基线，包含 v7 迁移文件（已被 v8 收敛） |

---

## [v6.3.0] - 2026-10-28

> 自 v6.2.0（2026-07-22）以来的累计发布：**2450 个提交**（非合并 2372 个：fix 776 /
> docs 557 / feat 284 / test 195 / refactor 155 / chore 157 / perf 99 / ci 25 / security 6 等）。
> 三条主线：**worker 多进程拓扑（W-A…W-I）**、**私有端点前缀治理（ISSUE-13 / A 族清零）**、
> **隐私与端到端加密加固（B-6…B-9）**，另有 Synapse v1.162 行为对齐与门禁体系可执行化。

### Added（新增）
- **worker 多进程拓扑 W-A…W-I**：`stream_writers` 八路均可转发（六路真转发、两路硬拒）/
  根二进制按角色起实例 / 心跳自声明身份并自查注册 / 负载均衡接入生产（默认关）/
  拓扑校验从"只记日志"变为可强制 / 参考多进程部署与端到端烟测 /
  `worker_write_forward_total` 指标、`/_health` 判据与告警
- **MSC 面**：MSC3266 room summary（含 `query_ignored` 登记）、MSC4262 LEFT_ROOM、
  MSC4190 注册面、MSC4242 孤儿查询删除、MSC4354 sticky TTL、MSC4140 延迟事件全套端点、
  MSC4210 legacy 提及默认隐藏、MSC3981 递归与拓扑序、MSC4222 `state_after`、
  MSC4311 宽限开关与 `rooms.knock`、MSC3861/MAS 接线、MSC3720 account status、
  MSC3814 dehydrated device events、MSC4297 v2.1 交叉验证、MSC4452 `m.preview_url` 稳定化
- **关系 API**：补 4 段 `/{relType}/{eventType}` 路由，`event_type` 过滤落到存储与服务层
- **三方事件准入钩子** `check_event_allowed`：含内容改写、触发侧配置，覆盖房间创建序列与
  burn-after-read
- **搜索**：`event_search` 物化表（表 + 触发器 + GIN 索引），房间/消息搜索改命中 trgm 索引
- **新表**：`event_receipts_stream`（stream 位点日志）、`user_pending_deactivations`（注销队列）
- **运维与观测**：PostgreSQL Grafana 20 面板、exporter 与 6 条 PG 告警、备份新鲜度闭环、
  WAL 归档（PITR）与恢复演练 runbook、alert-handler 格式适配层（钉钉/企业微信原生）、
  SpikeInAuthFailures 与 MessageQueueBacklog 两条告警从失效栈迁入
- **媒体**：`preview_url` 实现真实抓取（钉扎 IP + 逐跳 SSRF 校验 + 硬上限）
- **Admin**：per-user 邀请名单端点、媒体端点族补齐、读面 RBAC 登记

### Changed（变更）
- **破坏性**：删除 `/_matrix/client/*` 下的私有端点死别名（**77 + 78 条**迁至
  `/_matrix/vendor/v1`），退役 **22 条** `client/v1` 双注册（A 族清零，并加"只减不增"守卫）
- **破坏性**：`stream_writers` 从配置项变成可执行的写归属；无 Redis 时 `events` /
  `to_device` 转发改为启动即报错
- `/_matrix/client/versions` 宣告延到 **v1.17**，并以"证据表 + 守卫"锁住声明
- 数据库统一收敛到容器（5432 仅发布到宿主回环），部署时生成强随机口令并做口令校验
- 请求超时与第三方规则调用超时改为配置项（超时按 fail-closed 拒绝）
- 清理 4 张废弃表并折入迁移基线；三条 CHECK 只在定义变化时重建

### Fixed（修复）
- 联邦入站完整性校验从 join 推广到 leave/knock（FED-02）
- 注销补齐账户级数据清理与待处理邀请/敲击拒绝；`/requestToken` 防账号枚举
- to-device 积压按龄回收；stream 位点改为 per-stream 多标签 gauge
- Admin 读面三处判定不自洽（集合端点 / 路径别名 / 无覆盖只读）

### Security（安全）
- **B-6** 私密房间短 Megolm 轮换（1 天 / 50 条）
- **B-7** 服务端持有房间密钥必须可见且默认关闭（`/_health` 与部署期断言）
- **B-8** to-device 反滥用限流
- **B-9** 一次性邀请 + 私密 DM 默认 retention + 测试门禁闭环
- CAS SSO 同源校验、默认口令治理、`/metrics` 三态暴露控制、监控栈反向代理的
  Basic Auth 与 HTTPS

### Performance（性能）
- 事件准入改用共享 `room_state` 缓存；状态决议结果按冲突集合缓存并提为进程内共享
- 缓存前缀扫描按命名空间收窄；指标直方图内存有界化
- N+1 查询整改（PERF-06…14、CODE-04/07）

### Release / 工程
- 包版本 `[workspace.package].version` 6.2.0 → **6.3.0**（9 个成员 crate 继承）
- `build-and-push.sh` 不再硬编码镜像版本，改为读 `[workspace.package].version`；
  `scripts/api_test/generate_openapi.py` 的 OpenAPI `info.version` 同口径读取
- CI 增加 `concurrency`（PR 新推送取消旧 run）、门禁可达性元门禁（TEST-04）、
  `client/v1` 退役面棘轮、契约 `query_unknown` 裁决表、逐文件覆盖率台账与"达成即删"流程

---

## [v6.2.0] - 2026-07-22

> 架构优化 Round 2（收口）：test_utils 类型修复 / clippy expect-used 门禁 /
> TestContext schema pool 集成 / lib + unit 测试全绿 (1362 tests)

### Fixed（修复）
- test_utils `get_template_schema_name` 返回类型 mismatch（`?` 解包后缺少 `Ok(...)` wrapper）
- clippy `expect_used` 门禁：test schema cleanup runtime 添加 `#[allow(clippy::expect_used)]`
- Integration `TestContext` 接入 `LeasedSchema` schema pool（TRUNCATE 复用替代 DROP 重建）

### Changed（变更）
- Cargo.toml 版本号 6.1.0 → 6.2.0
- CI 测试命令统一：lib 用 `--all-features`，unit 用 `--features test-utils`

---

## [v6.1.0] - 2026-07-14

> 架构优化 Round 2（续）：Auth trait 分层 / EventStore 读写分离 / UserService 便利层 /
> 存储 trait 迁移 / god-file 拆分 / 安全 fail-closed 加固 / Bench + Snapshot 补齐

### Added（新增）
- **MembershipTransition 状态机**：`synapse_common::membership_transition::is_legal(from, to, ctx)`
  纯函数表，集中定义所有合法的成员资格转换；联邦入站 m.room.member 事件经此校验
- **Membership Benchmark**：14 个成员资格转换 case（7 fail-closed + 7 allowed/idempotent）
- **Seed Script**：`scripts/seed_bench_data.sh` 性能测试数据播种（用户/房间/设备密钥）
- **联邦 Send_Transaction 集成测试**：5 个 case 覆盖签名认证/PUD验证/签名PDU处理
- **Login + Sync Snapshot 测试**：鉴权成功响应 shape 锁定（insta），动态字段自动 redact
- **CanonicalEvent newtype**：缓存规范 JSON 字节，避免联邦签名验证重复序列化
- **Auth trait 分层**：`TokenAuth`（9 方法）、`CredentialAuth`（10 方法）、`RoomAuth`（10 方法）
- **EventReader / EventWriter traits**：读写分离，35 读方法 + 11 写方法
- **UserService 便利层**：封装 UserStore，减少重复调用
- **InMemory mocks**：6 个存储模块的 mock 实现，支持单元测试

### Changed（变更）
- **Auth 调用点迁移**：所有 web 层调用从旧 `Auth` trait 迁移到 `TokenAuth`/`CredentialAuth`/`RoomAuth`
- **EventStore 消费者迁移**：读写分离为 `EventReader`/`EventWriter`，消除只读路径的写权限
- **UserService 迁移**：12 个 service 从直接 `UserStore` 访问迁移到 `UserService`
- **存储 → 服务层路由**：`context.rs` 移除已弃用的 `user_storage`/`room_storage` 字段
- **God-file 拆分**：application_service / sliding_sync / friend_room / registration_token / openclaw / saml / cas / event_report / server_notification / media_quota 等模块从单文件拆为子模块
- **Glob re-export 收敛**：根 crate + synapse-storage + synapse-common 的 glob re-export 替换为显式导出
- **中间件类型化上下文**：federation / admin / core 中间件迁移到具体 `State<T>` 类型

### Fixed（修复）
- **P1 Token 校验 fail-closed**：`account_compat.rs` 区分 DB 错误（500）与认证失败（匿名降级）
- **P2 错误日志加固**：federation auth 密钥获取、widget 权限检查、transaction PDU 校验等 `.ok()` 吞没点添加诊断日志
- **P2 可见性 fail-closed**：`search.rs` 用户搜索可见性查询失败时默认隐藏（`unwrap_or(false)`）
- **C-5 Phase 4 协议层边界冻结**：AES/Ed25519 接口可见性收窄，减少公开暴露面

### Security
- 联邦端点对"存在但未授权"与"不存在"统一返回 404（防存在性泄漏）
- 成员资格状态机拒绝非法转换（banned re-join、invite-of-banned、self-ban、knock 非法房间）
- Token 刷新并发安全：区分并发刷新重试与重放攻击（M2/OPT-026）
- Health check 仅信任 `/health`，不再 fallback `/versions`（防 DB 故障掩盖）

---

## [v6.0.5] - 2026-07-09

> 架构优化 Round 2：纯重构，无行为变更。197 个文件，净增约 1,700 行。

### Changed（变更）

#### C2 — Service Trait 提取（11 个 service）
- `AdminUserService`、`AdminFederationService`、`ClientPushService`、`AccountDataService`、
  `DehydratedDeviceService` 等 11 个 service 的存储依赖从 `Arc<ConcreteType>` 迁移到
  `Arc<dyn StoreApi>`，支持 mock 注入和单元测试
- 新增 6 个存储模块的 `InMemory` mock 实现，用于测试

#### Phase 2 — Glob Re-export 收敛
- `synapse-common` 和 `synapse-storage` 的 glob re-export 缩小为显式导出列表
- 根 crate 的 glob re-export 替换为显式导出

#### Phase 3 — 中间件类型化上下文迁移
- 新增 `CoreContext`、`AdminContext`、`FederationContext` 三个类型化上下文
- 认证、shadow-ban、CSRF、限流中间件从 `AppState` 迁移到 `State<CoreContext>`
- admin 认证中间件迁移到 `State<AdminContext>`
- federation 限流中间件迁移到 `State<FederationContext>`
- replication HTTP 认证中间件迁移到 `State<CoreContext>`

#### Storage God-File 拆分
- `room_summary.rs`（1,827 行）拆分为 7 个子模块
- `worker.rs`（1,200+ 行）拆分为 5 个子模块
- `space.rs`（800+ 行）拆分为 4 个子模块
- `test_mocks.rs` 拆分为按域组织的子模块

#### Room 服务重构
- 线性化 DAG 构造流程，消除 post-construction setter
- 统一 room events 批量镜像方法签名（`SinceFilter`）

### Removed（移除）
- 无新增。

---

## [v8.0.0] - 2026-06-06

> 🚧 **预发布基线**。CI 全绿、`cargo clippy -D warnings` 0 错误 0 警告、
> `cargo check --all-features --locked` 0 错误。
> 关联基线：[`COMPREHENSIVE_AUDIT_REPORT_2026-06-03.md`](./docs/synapse-rust/COMPREHENSIVE_AUDIT_REPORT_2026-06-03.md)。

### Added（新增）

#### P0 安全/正确性（9 项 C-* 全部完成 / C-5 进入收敛期）
- **C-1 联邦 X-Matrix 时间戳新鲜度校验**
  - 新增 `src/common/nonce_cache.rs::FederationNonceCache`（moka，TTL=60s，容量=1M）
  - 新增 `src/web/middleware/federation_auth.rs::verify_freshness`（±30s 滑动窗口）
- **C-2 Canonical JSON 字符转义**
  - `src/e2ee/signed_json.rs::escape_canonical_string` 处理 U+2028/2029/FFFD
- **C-3 Sync since token 单次解析**
  - `src/services/sync_service/mod.rs`：`since_token` 单次解析贯穿 `sync_with_request`
  - 同一 `Option<SyncToken>` 贯穿 `delete_messages_up_to` 和 `is_incremental`
- **C-4 CI 路由分层门禁**
  - 新增 `scripts/quality/check_route_layering.sh`
  - 集成到 Makefile 和 CI 流程
- **C-5 E2EE vodozemac 收敛**（Phase 1 ✅ + Phase 2 ✅ + Phase 3 🚧）
  - **Phase 1（2026-06-05）**：`MegolmProvider` 双路径抽象（`MegolmBackend::{Legacy, Vodozemac}`）
  - **Phase 2（2026-06-06）**：
    - 新增 `PickleFormat::{Legacy, Vodozemac, Dual}` 枚举
    - 新增 `megolm_sessions.pickle_format` / `vodozemac_pickle` 列（迁移文件
      `migrations/20260605120000_megolm_vodozemac_dual_write_v8.sql`）
    - 懒迁移 API：`promote_to_dual` / `list_legacy_sessions` / `count_by_pickle_format`
    - 7 个新 metrics + 3 个记录方法
  - **Phase 3（2026-06-06 启动）**：本地 vodozemac 互操作 19 个 case 已就位
- **C-6 JWT 旧 token 默认放行修复**
  - `src/auth/token.rs::is_legacy_token_window_open` 默认返回 `false`
- **C-7 TOTP 恒时比较**
  - `src/web/utils/admin_auth.rs`：TOTP 改用 `subtle::ConstantTimeEq::ct_eq`
- **C-8/C-9 迁移文件 Schema 冲突**
  - v8 统一基线（`00000000_unified_schema_v8.sql` + `00000001_extensions_v8.sql`）
  - 迁移目录从 50 个文件收敛至 4 个
  - 17 处 `_ts`/`_at` 后缀统一
  - 8 处 `#[sqlx(rename)]` 桥接消除
- **C-10 SAML 模块 `NOW()` 残留**
  - 修复 `src/services/saml_service.rs:332, 580, 778` 共 3 处
  - 改为 `EXTRACT(EPOCH FROM NOW())::BIGINT * 1000`

#### P1 架构/质量（11 项全部完成）
- **M-1 ServiceContainer 分层**
  - 拆为 4 个子结构体（`E2ee` / `RoomSync` / `Federation` / `Admin`）
  - 48 核心字段（实际，2.2 报告为 35）
  - 文件行数 1201（vs 2.2 报告的 1408）
- **M-2 `common/config/mod.rs` 拆分**
  - 拆为 18 子模块（`auth` / `builtin_oidc` / `database` / `error` / `experimental` /
    `federation` / `identity` / `logging` / `manager` / `performance` / `push` /
    `rate_limit` / `retention` / `search` / `security` / `server` / `smtp` /
    `translate` / `voip` / `worker`）
  - 聚合文件 1976 行
- **M-3 `sqlx::query!` 全量迁移**（**已搁置**）
  - 替代方案：CI 强制 `schema_health_check` 门禁
  - 详见 `docs/synapse-rust/M3_PROGRESS.md`（**stale**）与
    `scripts/ci_schema_health_check.sh`
- **M-4 路由层强制使用 service**
  - `scripts/ci/check_route_storage_boundary.sh` 门禁部署
  - `scripts/quality/check_route_layering.sh` 配套
- **M-5 N+1/无限流硬性 `LIMIT`**
  - `src/storage/membership.rs::get_room_members` / `get_shared_room_users` 添加 `LIMIT 200`
  - `src/storage/event.rs::get_room_events_by_type` / `get_sender_events` 添加 `limit.min(200)`
  - `src/storage/room.rs::get_rooms_batch` 输入数组 `take(200)` 上限
- **M-6 联邦签名缓存 key 失效广播**
  - `KeyRotationManager`（`src/federation/key_rotation.rs`）
  - `FederationSignatureCache`（`src/cache/federation_signature_cache.rs`）
- **M-7 Typing/Presence 强制 Redis**
  - `CacheManager` L1+L2（`src/cache/mod.rs`）
- **M-8/M-9 `ApiError` 结构化 + TraceContext 透传**
  - `src/common/error.rs` 结构化日志（`tracing::error!(errcode, error, context)` 模式）
  - 7 个关键方法添加 `#[instrument]`
  - `tracing` crate 启用 `attributes` feature
- **M-10 巨型文件拆分**
  - 8 个文件已全部拆分
  - 仅剩 `common/config/mod.rs` 聚合文件（1976 行）

#### 工程门禁与 CI（Step 10）
- **`deny.toml`**：`cargo-deny` 配置（advisories/bans/licenses/sources）
- **`cargo-audit.toml` + `audit.toml`**：`cargo-audit` 阻断执行
  （`--deny warnings --deny unsound --deny yanked`）
- **`scripts/ci/supply_chain_gate.sh`**：Step 10 主门禁，集成
  `cargo-deny check` + `cargo-audit`；CI 中 `ci.yml:93, 318` 已在两个 job 中调用
- **`.github/workflows/mutation-testing.yml`**：cargo-mutants nightly
  （非阻塞，timeout 120min）
- **`.tarpaulin.toml`**：覆盖率门槛 `range = "70..90"`
- **`docs/INDEX.md`**：文档治理中枢，区分 `archive/` 与现行，纳入 PR 门禁

#### 测试整改（Step 8）
- 删除 `error.rs` 中 4 个套套逻辑测试，共 ~200 行
- 删除 `benches/` 中 7 个无 IO 伪性能测试，共 ~400 行
- 引入 `cargo-mutants` 接入 CI（`.github/workflows/mutation-testing.yml`）
- `tarpaulin.toml` 覆盖率门槛 `fail-under = 70`

#### 性能与可观测性（Step 9）
- `ApiError` 100% 结构化
- 7 个关键方法添加 `#[instrument]`
- `LIMIT 200` 在 3 个查询路径落地

#### 文档与发布基线（Step 12）
- `docs/synapse-rust/API_COVERAGE_REPORT.md`（vs Synapse v1.149.1）
- `docs/synapse-rust/SUPPORTED_MATRIX_SURFACE.md`（Matrix v1.18 + Synapse v1.153）
- `docs/synapse-rust/E2EE_VODOZEMAC_MIGRATION.md`（C-5 完整迁移设计 + 状态报告）
- `docs/INDEX.md`（**新增**，2026-06-06）

### Changed
- 评估口径：审计报告 v2.4 → v2.5（2026-06-06）
- Matrix spec baseline: v1.13 → v1.18
- Synapse behavioral reference: v1.149.1 → v1.153.0

### Fixed
- DB Schema 漂移风险：`schema_health_check` CI 门禁
- 联邦 nonce 重放：✅ C-1
- Canonical JSON 跨语言兼容性：✅ C-2
- Sync token 解析竞态：✅ C-3
- TOTP 计时侧信道：✅ C-7
- SAML `NOW()` 运行时错误：✅ C-10
- ApiError 内部错误泄漏（`internal_with_log` / `database_with_log`）：~1200 处
  跨 119 个文件
- N+1 查询：M-5 落地
- 路由层直查 DB：M-4 门禁部署
- format drift：`.github/workflows/format-drift-tracking.yml` + `format-governance.yml`

### Security
- `cargo-deny` + `cargo-audit` 阻断执行
- 已豁免 2 条 RUSTSEC（均带 Review-by 期限）：
  - **RUSTSEC-2023-0071**（rsa 0.9.10 Marvin Attack）：仅用于 OIDC 签名，
    无解密路径；目标 2026-06-30 迁 ES256
  - **RUSTSEC-2024-0436**（paste 1.0.15 unmaintained）：传递依赖，
    仅编译期宏使用

### Deprecated
- 自研 Olm/Megolm crypto（`src/e2ee/olm/session.rs` /
  `src/e2ee/crypto/{aes,x25519}.rs`）— 已进入 C-5 Phase 4 边界冻结阶段，进一步删除需待跨端矩阵全绿后评估
  （**必须 Phase 3 跨 Element 客户端矩阵全绿**）

### Removed
- 50 个迁移文件 → 收敛至 4 个（`migrations/00000000_unified_schema_v8.sql` /
  `migrations/00000001_extensions_v8.sql` / `migrations/extension_map.conf` /
  `migrations/README.md`）
- 12+ 重复索引定义
- 17 处 `expires_ts` / `consumed_ts` / `logout_sent_ts` 命名（→ `_at` 后缀）

---

## [v7.x] - 2026-05-28 前

> 历史版本，迁移文件已并入 v8.0.0。保留作历史溯源。

### 已知问题（迁移至 v8.0.0 修复）
- 迁移文件 v7 Schema 冲突（`voice_usage_stats` 三重定义等）
- 30+ 内部表定义重复
- 69+ 跨文件表定义重复
- 18 张冗余表（已在 Batch-03 DROP）
- 63 处 `SELECT *` 脆弱查询
- 5+ 处布尔字段缺 `is_` 前缀
- 3 处 `NOW()` 赋值 BIGINT 列（**C-10**）

### Deprecated
- 旧基线 `00000000_unified_schema_v7.sql` → 已被 v8 替代

---

## 变更分类约定

每条变更归入以下类别之一：

- **Added**：新增功能、API、迁移、文档
- **Changed**：既有功能的行为/接口变更
- **Fixed**：Bug 修复、性能退化修复
- **Security**：CVE 修复、安全策略变更
- **Deprecated**：即将移除（保留过渡期）
- **Removed**：本版本已移除

跨多个类别的变更按"对用户最显著的类别"归类，
次要类别在条目末尾以"（另：...）"标注。

---

## 版本号约定

- **MAJOR**（如 v8 → v9）：数据库 schema 大版本升级、协议不兼容变更
- **MINOR**（如 v8.0 → v8.1）：新 API、新 MSC、稳定功能新增
- **PATCH**（如 v8.0.0 → v8.0.1）：Bug 修复、安全补丁、性能优化

预发布标签：`-alpha.N` / `-beta.N` / `-rc.N`（如 `v8.0.0-beta.1`）

---

## 与其他文档的关系

| 文档 | 用途 |
|------|------|
| [`docs/synapse-rust/COMPREHENSIVE_AUDIT_REPORT_2026-06-03.md`](./docs/synapse-rust/COMPREHENSIVE_AUDIT_REPORT_2026-06-03.md) | 全面审查报告（本 changelog 的依据） |
| [`docs/INDEX.md`](./docs/INDEX.md) | 文档导航中枢 |
| [`README.md`](./README.md) | 项目门面 |
| `Cargo.toml` | 当前版本号机器源 |
| `git tag` | 实际发版 tag 序列 |
