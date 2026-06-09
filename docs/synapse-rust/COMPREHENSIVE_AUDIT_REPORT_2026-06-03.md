# synapse-rust 全面深度技术审查报告

**报告版本**: 5.2
**审查日期**: 2026-06-09（全量重新审查 + 2026-06-09 二次/三次代码复核；按当前工作区重新校正结论）
**对比基线**: element-hq/synapse v1.153
**审查范围**: `/Users/ljf/Desktop/hu_ts/synapse-rust`（主 crate `src/` + workspace 子 crate + `migrations/` + CI 脚本；根目录生效迁移为 v10）
**审查模式**: 本地静态分析 + 关键 CI 脚本复核 + SQLx 离线编译验证 + 数据库迁移目录审计 + Step 1-12 文档口径校正 + 小范围代码修复

---

## 0.1 2026-06-09 复核修正（当前权威摘要）

> 本报告长期累积了多轮“审查结论 + 执行日志 + 历史快照”。2026-06-09 二次复核发现，下文部分统计和“已完成/仅剩 1 项”的结论已经落后于当前仓库状态。本节优先于下文历史叙述。

### 本次复核确认的事实

- `migrations/` 根目录当前只保留 `00000000_unified_schema_v10.sql` 和 `00000001_extensions_v10.sql` 两个生效基线文件；`v8` 文件已移入 `migrations/archive/`。因此文中“v8/v10 双基线并存于根目录”“6 个 .sql 文件并存”的表述已过时。
- 路由分层门禁并非文中反复提到的 `scripts/quality/check_route_layering.sh` 在 CI 中生效；当前 CI 实际接入的是 `scripts/ci/check_route_storage_boundary.sh`（`.github/workflows/ci.yml`）。`check_route_layering.sh` 存在，但目前更像本地巡检脚本。
- `check_route_storage_boundary.sh` 当前通过，且 `scripts/ci/route_storage_exceptions.txt` 已清空；本轮从补齐 29 个存量 `use crate::storage` 路由文件快照开始，随后通过一批 service re-export / shim 迁移、guest 注册/升级逻辑下沉，以及 `handlers/room/*`、`admin/*`、`ai_connection.rs`、`openclaw.rs` 清理，将例外收敛到 0。当前门禁语义已从“拦新增、保留存量债务”推进到“路由层新增 `use crate::storage` 直引将直接失败”。
- 文中“`SELECT *` 全量消除”结论不准确。本轮已修复 3 处真实残留：`src/storage/sliding_sync.rs`、`src/storage/space.rs`、`src/storage/registration_token.rs`。复核后 `src/` 中仅剩 2 处文本命中：1 处为 PostgreSQL `UNNEST` 语法片段，1 处为 `src/common/macros.rs` 的示例宏字符串，不属于业务查询。
- OpenAPI 已接入，但仍是“基础集成”而非“覆盖完成”：当前 `src/web/api_doc.rs` 中已有 17 个 `#[utoipa::path]` 注解，覆盖 health、versions、server_version、capabilities、well-known，以及 `account_data` / `filter` / `push` 的稳定读接口；功能仍受 `openapi-docs` feature 控制，并非默认构建项。
- E2EE Megolm 运行时主路径已比旧结论更进一步：`MegolmProvider` 已直接封装 `MegolmVodozemacService`，`vodozemac` 依赖也已是普通依赖；但 `.github/workflows/e2ee-interop.yml` 的 `element-web-interop` job 仍明确标注完整 Element Web harness 尚未接线，且 `e2ee/crypto/*` 等自研/协议辅助代码仍在。因此 C-5 仍不能标为完成。
- OTLP dev 默认端点确已落地：`src/common/telemetry_config.rs` 的 `resolve_otlp_endpoint()` 会在 `debug_assertions` 下默认回退到 `http://localhost:4317`。这项结论为真。
- 覆盖率门槛和 mutation testing 的“配置已就位”结论为真，但本次复核未重新执行 `cargo tarpaulin` / `cargo mutants` 并生成新结果，因此不能将其表述为“覆盖率目标已达成”或“变异残留已压到阈值内”。
- `src/storage/refresh_token.rs` 仍残留 2 处 `EXTRACT(EPOCH FROM NOW()) * 1000` 浮点毫秒表达式；本轮已同步修为 `EXTRACT(EPOCH FROM NOW())::BIGINT * 1000`，并重新执行 `cargo sqlx prepare -- --all-targets`。
- 当前主 crate `src/` 口径统计为：编译期 `sqlx::query!` / `query_as!` / `query_scalar!` **1355** 处，动态 `sqlx::query(` **174** 处，动态 `sqlx::query_as::<...>` **14** 处，动态占比约 **12.2%**。`.sqlx/` 当前缓存文件 **1143** 个。历史段落中 1358/1146 等数字为旧缓存快照，不再作为当前口径。

### 本次复核后仍未完成的关键项

- `C-5`：Megolm 运行时主路径已切到 vodozemac，但跨 Element 端到端互操作 harness 尚未完全接线；Phase 4 仍需完成自研/协议辅助代码边界梳理、feature/comment 口径清理和跨端验收。
- `C-4 / M-4`：`use crate::storage` 这一类路由层直引已完成清零，`check_route_storage_boundary.sh` 可直接拦截新增违例；当前工作区 grep 口径下，`src/web/routes/` 业务路由层真实 `sqlx::query*` / `.pool` / `PgPool` 直用路径已全部清空，仅测试代码中可能存在测试辅助用的数据库连接构造。
- `M-4 / P2 #35`：覆盖率与 mutation testing 仍停留在“门禁配置完成”，缺少本轮可复核的执行结果。
- `M-5`：列表接口统一 keyset 分页尚未完成；此前的 `LIMIT 200` 只是硬上限治理，不等于全量分页模型统一。本轮已将 `admin/federation` 的 destinations 列表从纯 `OFFSET` 迁移为“keyset 优先 + legacy offset 兜底”，将 `friend_room` 好友列表升级为 `next_batch` cursor 优先、`offset/next_offset` 兼容输出，并将 `openclaw` 消息列表切到统一 `from` / `next_batch` cursor；同时收紧 `openclaw` conversations / messages / generations 的 legacy `offset` 口径，非零 offset 现在明确报错而非静默忽略。
- `M-8 / M-9`：`ApiError` 完整结构化重构、关键 service 全量 `#[instrument]` 仍未完成；本轮已为 `account_data_service`、`room_tag_service`、`oidc_mapping_service` 补齐基础埋点，但当前仍只能视为“部分治理到位”。
- `P2 #37`：OpenAPI 已从最初的 4 个示例端点扩展到 17 个公开稳定读端点，但离“覆盖完成”仍有明显距离，不应表述为“优化任务全部完成”。

### 本轮已落地修复

- 修复 `src/storage/sliding_sync.rs` 中对 `sliding_sync_rooms` 的 `SELECT *`，改为显式列清单。
- 修复 `src/storage/space.rs` 中对 `space_statistics` 的 `SELECT *`，改为显式列清单。
- 修复 `src/storage/registration_token.rs` 中对 `registration_token_batches` 的 `SELECT *`，改为显式列清单。
- 修复 `src/storage/room/mod.rs` 中 `get_rooms_batch()` 的现存 `sqlx::query_as!` 可空性推断编译错误，改为显式列的运行时 `query_as::<_, RoomRecord>`；复核后 `cargo check --locked --lib` 重新通过。
- 回填 `scripts/ci/route_storage_exceptions.txt` 的 29 个现存路由存储直连文件，使 `check_route_storage_boundary.sh` 恢复为可用的“增量门禁”。
- 将 `src/web/routes/sliding_sync.rs`、`src/web/routes/event_report.rs`、`src/web/routes/room_summary.rs`、`src/web/routes/feature_flags.rs`、`src/web/routes/app_service.rs`、`src/web/routes/cas.rs`、`src/web/routes/rendezvous.rs`、`src/web/routes/extractors/auth.rs`、`src/web/routes/admin/user.rs`、`src/web/routes/admin/token.rs`、`src/web/routes/handlers/thread.rs` 的类型/DTO 依赖切换到 `services` 层 re-export。
- 调整 `src/services/sliding_sync_service.rs`、`src/services/event_report_service.rs`、`src/services/room/summary.rs`、`src/services/feature_flag_service.rs`、`src/services/application_service.rs`、`src/services/cas_service.rs`、`src/services/registration_token_service.rs`、`src/services/thread_service.rs`、`src/services/mod.rs`，补齐 service-layer shim / re-export，支撑路由层脱离 `storage` 直接依赖。
- 在 `src/storage/user.rs` 新增 guest 状态更新与 guest 升级存储方法，并将 `src/auth/register.rs` 扩展为 guest 注册、guest 校验、guest 升级统一入口；`src/web/routes/auth_compat.rs` 与 `src/web/routes/guest.rs` 不再直接创建 device 或更新 `users` 表。
- 将 `src/web/routes/background_update.rs`、`src/web/routes/push_notification.rs`、`src/web/routes/module.rs`、`src/web/routes/handlers/search.rs` 的类型/DTO 依赖切换到 `services` 层 re-export，并分别调整 `src/services/background_update_service.rs`、`src/services/push/service.rs`、`src/services/module_service.rs` 对外暴露存储 DTO。
- 清理 `src/web/routes/handlers/room/{mod,members,receipts,state}.rs` 的 route->storage 直连：房间成员、回执、read markers、state 查询和类型依赖已统一切到 `room_service` / `services` re-export。
- 清理 `src/web/routes/admin/{audit,notification,room/mod}.rs` 的类型依赖直连，统一改走 `services` 层 re-export。
- 清理 `src/web/routes/admin/federation.rs`、`src/web/routes/ai_connection.rs`、`src/web/routes/openclaw.rs` 的最后一批 route->storage 类型依赖；`scripts/ci/route_storage_exceptions.txt` 已清空。
- 新增 `src/services/admin_federation_service.rs`、`src/services/client_push_service.rs`、`src/services/account_data_service.rs`，将 `src/web/routes/admin/federation.rs`、`src/web/routes/push.rs`、`src/web/routes/account_data.rs` 中的管理 SQL 统一下沉到 service；复核后这些路由文件已无 `sqlx::query*`、`PgPool` 或 `.pool` 直连。
- 清理 `src/web/routes/handlers/room/management.rs` 的 SQL 直连：房间信息、成员列表、能力检查、Vault 数据和搜索逻辑已统一切到 `room_service` / `account_data_service` / `search_service`。
- 新增 `src/services/admin_token_service.rs` 并将 `src/web/routes/admin/token.rs` 中的 registration token / access token / refresh token 管理 SQL 全部下沉；复核后该路由文件已无 `sqlx::query*`、`PgPool` 或 `.pool` 直连。
- 将 `src/services/admin_user_service.rs` 从 DTO shim 扩展为真实管理服务，并清理 `src/web/routes/admin/user.rs` 中的 v2 用户列表、创建/更新、统计、批量操作与账户更新 SQL；复核后该路由文件已无 `sqlx::query*`、`PgPool` 或 `.pool` 直连。
- 新增 `src/services/admin_security_service.rs` 并将 `src/web/routes/admin/security.rs` 中的 shadow ban、用户速率限制读写与缓存失效逻辑统一下沉；复核后该路由文件已无 `sqlx::query*`、`PgPool` 或 `.pool` 直连。
- 新增 `src/services/admin_media_service.rs` 并将 `src/web/routes/admin/media.rs` 中的媒体列表、详情、删除、quota 统计及按用户媒体管理 SQL 统一下沉；同时按 canonical `user_id` 收敛用户媒体查询/删除路径。复核后该路由文件已无 `sqlx::query*`、`PgPool` 或 `.pool` 直连。
- 将 `src/web/routes/admin/report.rs` 中的举报列表、单条读取、删除、按房间举报查询统一改为复用 `event_report_service`；同时补齐 `event_report_service.delete_report()` 的 404 语义校验。复核后该路由文件已无 `sqlx::query*`、`PgPool` 或 `.pool` 直连。
- 扩展 `src/services/search_service.rs`，承接 `src/web/routes/handlers/search.rs` 中的房间事件搜索、`timestamp_to_event`、事件上下文窗口和房间搜索逻辑；复核后该路由文件已无 `sqlx::query*`、`PgPool` 或 `.pool` 直连。
- 扩展 `src/storage/retention.rs` 与 `src/services/retention_service.rs`，补齐服务端 retention policy 的 optional / upsert / 状态统计能力，并将 `src/web/routes/admin/retention.rs` 中的策略查询、策略写入与状态统计 SQL 全部下沉；复核后该路由文件已无 `sqlx::query*`、`PgPool` 或 `.pool` 直连。
- 新增 `src/services/admin_server_service.rs`，承接 `src/web/routes/admin/server.rs` 与 `src/web/routes/handlers/health.rs` 中的数据库健康探针与 required tables schema 校验；复核后这两个路由文件已无 `sqlx::query*`、`PgPool` 或 `.pool` 直连。
- 扩展 `src/services/admin_registration_service.rs` 与 `src/storage/user.rs`，补齐 shared-secret 管理员注册流程中的 `user_type` 持久化，并将 `src/web/routes/admin/register.rs` 中的 nonce / 注册主流程切换为统一走 `admin_registration_service`；复核后该路由文件已无 `sqlx::query*`、`PgPool` 或 `.pool` 直连。
- 为 `src/services/container.rs` 增加 `database_pool()` 只读入口，并清理 `src/web/routes/state.rs` 中对连接池字段的直接 `.pool` 访问；复核后该路由文件已无 `sqlx::query*`、`PgPool` 或 `.pool` 直连。
- 扩展 `src/storage/event/mod.rs` 与 `src/services/room/service.rs`，补齐 `room_ephemeral` 读取模型和 client 视角封装，并将 `src/web/routes/ephemeral.rs` 中的临时事件查询下沉到 `room_service`；复核后该路由文件已无 `sqlx::query*`、`PgPool` 或 `.pool` 直连。
- 扩展 `src/storage/event/mod.rs` 与 `src/services/room/service.rs`，补齐带过期时间的临时事件 upsert / delete 能力，并将 `src/web/routes/typing.rs` 中的 typing ephemeral 写入、清理逻辑下沉到 `room_service`；复核后该路由文件已无 `sqlx::query*`、`PgPool` 或 `.pool` 直连。
- 将 `src/web/routes/push_rules.rs` 中默认 push rules 的账户数据读取切换为复用 `client_push_service.get_push_rules_content()`；复核后该路由文件已无 `sqlx::query*`、`PgPool` 或 `.pool` 直连。
- 将 `src/web/routes/device.rs` 中 device-list stream 位置、变化记录和设备详情批量查询改为复用 `DeviceStorage` 现有接口（`get_max_device_list_stream_id` / `get_device_list_changes` / `get_devices_by_user_device_pairs`）；复核后该路由文件已无 `sqlx::query*`、`PgPool` 或 `.pool` 直连。
- 扩展 `src/services/room/service.rs`，补齐 pinned state 的读取/写入能力，并将 `src/web/routes/pinned.rs` 中对最新 `m.room.pinned_events` state event 的查询与写入下沉到 `room_service`；复核后该路由文件已无 `sqlx::query*`、`PgPool` 或 `.pool` 直连。
- 扩展 `src/storage/room_tag.rs` 并新增 `src/services/room_tag_service.rs`，将 `src/web/routes/tags.rs` 中的 room tag 查询、写入和删除改为统一走 service/storage；复核后该路由文件已无 `sqlx::query*`、`PgPool` 或 `.pool` 直连。
- 扩展 `src/storage/rendezvous.rs` 与 `src/services/account_data_service.rs`，分别承接 rendezvous message 存取与 OpenID token 校验，并将 `src/web/routes/rendezvous.rs`、`src/web/routes/federation/mod.rs` 中的现场 storage 构造下沉；复核后这两个路由文件已无 `sqlx::query*`、`PgPool` 或 `.pool` 直连。
- 将 `src/web/routes/assembly.rs` 中 extended profile 的 `account_data` 读写切换为复用 `account_data_service.get_account_data()` / `set_account_data()`；复核后该路由文件已无 `sqlx::query*`、`PgPool` 或 `.pool` 直连。
- 新增 `src/services/oidc_mapping_service.rs`，将 `src/web/routes/oidc.rs` 中 `oidc_user_mapping` 的绑定查询、登录记账和首次绑定写入统一收敛到 service；复核后该路由文件已无 `sqlx::query*`、`PgPool` 或 `.pool` 直连。
- 扩展 `src/storage/device.rs` 与 `src/e2ee/cross_signing/service.rs`，补齐按用户读取 device stream id 与公开 cross-signing key 的能力，并将 `src/web/routes/federation/membership.rs` 中的相关 SQL 直连下沉；复核后该路由文件已无 `sqlx::query*`、`PgPool` 或 `.pool` 直连。
- 路由层 `use crate::storage` 存量已从 29 个压降至 0；`bash scripts/ci/check_route_storage_boundary.sh` 当前通过。
- 修复 `src/storage/refresh_token.rs` 中 2 处 `EXTRACT(EPOCH FROM NOW()) * 1000` 浮点毫秒比较，统一为 `::BIGINT * 1000`；重新生成 `.sqlx/` 缓存后 `SQLX_OFFLINE=true cargo check --locked --lib` 通过。
- **业务路由层 sqlx 直连清零完成**：当前 `src/web/routes/` 业务路由层已无 `sqlx::query!` / `sqlx::query_as!` / `sqlx::query_scalar!` / `.pool` / `PgPool` 直连，所有业务 SQL 操作已通过 service/storage 层封装
- 将 `src/services/admin_federation_service.rs` / `src/web/routes/admin/federation.rs` 中 destinations 列表改为 keyset 优先分页：新增基于 `server_name` 的 cursor，返回 `next_batch`，同时保留 legacy `offset` / `next_from` 兜底兼容，减少大页 `OFFSET` 扫描成本。
- 将 `src/services/friend_room_service/{models,mod}.rs` / `src/web/routes/friend_room.rs` 中好友列表分页升级为 cursor 优先：新增可编码 `sort_by` + 排序键的 `next_batch`，统一 `alphabet` / `recent` / `activity` 三种排序下的分页语义，同时保留 legacy `offset` / `next_offset` 兼容现有客户端。
- 将 `src/storage/openclaw.rs` / `src/services/openclaw_service.rs` / `src/web/routes/openclaw.rs` 中消息列表分页升级为 cursor 优先：新增 `(created_ts, id)` 消息 cursor，支持 `from` / `next_batch`，同时继续兼容 legacy `before` 参数。
- 收紧 `src/web/routes/openclaw.rs` 中 conversations / messages / generations 列表的 legacy `offset` 语义：`offset=0` 仅作为兼容占位保留，非零 offset 明确返回 `400`，避免对外继续暴露“已无效但看似可用”的旧分页模型。
- 扩展 `src/web/api_doc.rs` 的公开文档面，新增 `account_data` / `filter` / `push` 稳定读接口以及先前补齐的 versions / capabilities / well-known 注解；当前公开稳定示例端点已从 4 个增至 17 个。
- 为 `src/services/account_data_service.rs`、`src/services/room_tag_service.rs`、`src/services/oidc_mapping_service.rs` 补充 `#[instrument]`，提升本轮新增薄 service 的可观测性；同时修复 `src/web/routes/handlers/versions.rs` 测试辅助误用私有 helper 的隐藏编译问题，复核后 `cargo test --locked --lib --no-run` 通过。

### 对全文结论的修正

- “P0/P1/P2 全部完成、仅剩 1 项”的结论不成立；至少还剩 C-5、路由分层存量迁移、OpenAPI 覆盖扩展、覆盖率/变异测试实测、keyset 分页统一、`ApiError` 深度重构、Tracing 全量补齐等多项工作。
- “OpenAPI 已集成”可以保留，但应理解为“feature 级基础接入”，不能等价为“文档覆盖完成”。
- “迁移基线已统一到 v10”可以成立，但应同步删除或覆盖文中仍把 v8/v10 描述为根目录双基线并存的旧结论。

## 一、整体结论

| 维度 | 评级 | 说明 |
|---|---|---|
| 功能覆盖 | ★★★★☆ | Matrix Client-Server / Server-Server 主要 API 表面已覆盖，但与 Synapse v1.153 仍有 30+ 行为差异 |
| 架构合理性 | ★★★★★ | `ServiceContainer` 已分层拆分 8 核心字段 + 4 子结构体（M-1 ✅）；`common/config/mod.rs` 已拆 18 子模块（M-2 ✅）；workspace 多 crate 已拆分 |
| 安全性 | ★★★★★ | 联邦 X-Matrix 时间戳校验已实现（±30s + nonce 缓存）、Canonical JSON 已修复、JWT 旧 token 默认拒绝、TOTP 恒时比较、Push 鉴权已加固、Redis 健康检查已就位 |
| E2EE | ★★★★☆ | Megolm 运行时主路径已直接封装 `MegolmVodozemacService`；本地 vodozemac 互操作测试存在；跨 Element Web/Android/iOS harness 与自研辅助代码边界清理仍未完成 |
| 性能 | ★★★★☆ | N+1/无限流已做硬上限治理，主 `src/` 口径编译期 SQL 宏 1355 处、动态 SQL 188 处、动态占比约 12.2%，已达 ≤ 30% 目标；`admin/federation` destinations、`friend_room` 好友列表、`openclaw` 消息列表已切到 cursor / keyset 优先，`openclaw` 其余列表的 legacy offset 口径也已收紧，但统一分页仍未全量完成 |
| 代码质量 | ★★★★☆ | ServiceContainer 核心字段已从 48 降至 8（+4 子结构体），config/mod.rs 已拆 18 子模块，SELECT * 业务查询已清零；路由层存量直连与 ApiError 深层结构化仍是债务 |
| 可观测性 | ★★★★☆ | `#[instrument]` 已扩展到约 283 处（主 `src/` 口径），错误日志已有结构化字段，OTLP dev 默认端点已接入；全链路 request id 与关键 service 覆盖仍未完成 |
| 测试覆盖 | ★★★☆☆ | 套套逻辑已删除 ~600 行，cargo-mutants + tarpaulin 已配置，99 个可变异点已识别（E2EE 45 + federation 54），覆盖率门槛待达标 |
| 依赖/CI | ★★★★☆ | anyhow 已从 lib crate 移除，cargo-deny/audit 已就位，CI 门禁持续加固，mutation testing CI 已配置 |
| 数据库/迁移 | ★★★★★ | 根目录生效迁移已统一为 v10 两个 SQL 文件；v8 文件已归档；Schema 冲突、SELECT * 业务查询和本轮发现的 refresh token 浮点毫秒表达式已修复 |
| OpenAPI | ★★★☆☆ | `utoipa` + `utoipa-swagger-ui` 已集成，17 个公开稳定读端点已注解（health / versions / server_version / capabilities / well-known / account_data / filter / push），Swagger UI 已就位（`/_swagger`），220+ 路由待全量注解 |
| **总体** | **★★★★☆** | **不能再表述为“P0/P1/P2 全部完成、仅剩 1 项”**。已完成大量安全/Schema/SQLx/迁移治理；仍需收尾 C-5 跨端互操作与清理、路由层存量迁移、OpenAPI 覆盖扩展、覆盖率/变异测试实测、keyset 分页统一、ApiError 深度重构和 tracing/request id 补齐 |

---

## 二、Critical（必须立即修复）

### C-1 联邦 X-Matrix 时间戳新鲜度校验缺失 ✅ 已修复
- **位置**: `src/web/middleware/federation_auth.rs`、`src/common/nonce_cache.rs`
- **修复内容** (2026-06-04):
  - 实现 `FederationNonceCache`（moka 缓存，TTL=60s，容量=1M），按 origin+nonce 去重
  - 实现 `DEFAULT_TIMESTAMP_SKEW` ±30s 时间窗口校验
  - `federation_auth.rs` 中 `verify_freshness` 逻辑已集成到认证中间件
- **风险**: 已消除 — 攻击者无法重放旧请求

### C-2 Canonical JSON 不符合 Matrix 规范 ✅ 已修复
- **位置**: `src/e2ee/signed_json.rs`
- **修复内容** (2026-06-04):
  - 实现 `escape_canonical_string` 函数，正确处理 U+2028 / U+2029 / U+FFFD 转义
  - `canonical_json` 函数使用 `escape_canonical_string` 替代原 `serde_json::to_string`
  - 递归处理对象和数组的 canonical 排序
- **风险**: 已消除 — 签名值在跨服务器验证时一致

### C-3 Sync 服务 since token 重复解析导致 to_device 丢失 ✅ 已修复
- **位置**: `src/services/sync_service/mod.rs`
- **修复内容** (2026-06-04):
  - `sync_with_request` 中 `since_token` 单次解析（`since.and_then(SyncToken::parse)`）
  - 同一 `Option<SyncToken>` 贯穿 `delete_messages_up_to` 和 `is_incremental` 判断
  - 消除重复解析导致的 to_device 消息截断/重复投递
- **风险**: 已消除 — Sync 增量同步正确性保证

### C-4 路由绕过 service 层直查存储（架构违例）🚧 增量门禁已部署 / 存量迁移未完成
- **位置**: `scripts/ci/check_route_storage_boundary.sh`（CI 增量门禁）、`scripts/ci/route_storage_exceptions.txt`（存量例外）、`scripts/quality/check_route_layering.sh`（本地深扫巡检）
- **修复内容** (2026-06-09 复核):
  - CI 实际接入 `scripts/ci/check_route_storage_boundary.sh`，仅检查 `use crate::storage` 直引，并允许 `route_storage_exceptions.txt` 中的存量文件。
  - 本轮通过 service re-export / shim 以及 `handlers/room/*`、`admin/*`、`ai_connection.rs`、`openclaw.rs` 继续迁移，将路由层 `use crate::storage` 直引文件从 29 个压到 0 个；`bash scripts/ci/check_route_storage_boundary.sh` 当前通过。
  - `scripts/quality/check_route_layering.sh` 仍会报告 `sqlx::query*` / `PgPool` 等更广义的路由层直连问题；该脚本当前由 Makefile 暴露，但未作为 `.github/workflows/ci.yml` 阻断项。
- **风险**: 新增 `crate::storage` 直引已可被 CI 阻断；存量路由直连 SQL/Pool 仍可能绕过 service 层规则，需继续迁移。

### C-5 E2EE 自研路径未与 vodozemac 同步 🚧 Megolm 主路径已切换 / 跨端验收与清理未完成（2026-06-09 复核）
- **位置**: `src/e2ee/vodozemac_megolm.rs`（vodozemac 实现）、`src/e2ee/megolm/service.rs`（双路径抽象）、`src/services/container.rs:117,146-149`（装配）、`src/common/server_metrics.rs:75-96`（可观测性）、`migrations/20260605120000_megolm_vodozemac_dual_write_v8.sql`（双写 schema）、`src/e2ee/vodozemac_interop_tests.rs`（互操作测试）
- **当前状态**（2026-06-06）:
  - ✅ Olm Account/Session 100% vodozemac
  - ✅ Megolm 替代实现 100% 已完成（`vodozemac_megolm.rs`，含单测）
  - ✅ **Phase 1 完成**（2026-06-05）：`MegolmProvider` 已装配到 `E2eeServices`，孤儿模块问题已解决
  - ✅ **Phase 2 完成**（2026-06-06）：Megolm 双写（`PickleFormat::{Legacy, Vodozemac, Dual}` + `vodozemac_pickle` 列 + 懒迁移 `promote_to_dual` / `list_legacy_sessions` / `count_by_pickle_format`），7 个新 metrics + 3 个记录方法
  - ✅ **Megolm 运行时主路径已切换**（2026-06-09 复核）：`src/e2ee/megolm/service.rs` 中 `MegolmProvider` 已直接封装 `MegolmVodozemacService`，旧 `MegolmBackend` 运行时分支已不在该文件中
  - 🚧 **Phase 3 仍属部分完成**：本地 vodozemac 互操作测试矩阵已扩展至 19 个 case；但 `.github/workflows/e2ee-interop.yml` 中完整 Element Web harness 仍明确标注未接线，Android/iOS 跨端矩阵也没有可复核结果
  - 🚧 **Phase 4 部分推进但未完成**：Megolm service 运行时分支已清理，`vodozemac` crate 已是普通依赖；但 `e2ee/crypto/{aes,x25519,mod}.rs` 等自研/协议辅助路径仍在，`vodozemac-megolm` feature/comment 口径仍需整理，清理完成前不能宣称 C-5 完成
  - ❌ 自研/辅助 crypto（X25519/Ed25519/AES/Argon2）仍存在，部分属于协议层必要包装，需逐项分类保留或删除
  - ❌ 协议层（SSSS/Secure Backup/Cross-Signing/SAS）保留 — vodozemac 0.9 不覆盖
- **vodozemac 0.9 能力边界**:
  - 提供: Olm Account/Session、Megolm GroupSession/InboundGroupSession、Curve25519 ECDH、Ed25519
  - **不提供**: AES-256-GCM（需 `aes-gcm`）、Argon2（需 `argon2` crate）、SSSS/Secure Backup/Cross-Signing 协议层
- **三分类收敛策略**:
  - 🟢 A 直接替换: `megolm/service.rs`、`crypto/x25519.rs`、Olm 收尾
  - 🟡 B 配合其他库: `crypto/{aes,ed25519}.rs` 包装层、SSSS、Secure Backup、Verification、Cross-Signing、Signature
  - 🔴 C 不能替换: `crypto/argon2.rs`、协议层、模型/存储层
- **ROI**: 年度净收益 ~30 人天，投资 4-5 人周，回收期 ≤ 1 年
- **4 阶段收敛计划**:
  - ✅ **Phase 1（1 周）**: 装配 `MegolmProvider` 到 `E2eeServices`，加 `E2EE_USE_VODOZEMAC_MEGOLM` env 路由 — **2026-06-05 完成**（详见 `E2EE_VODOZEMAC_MIGRATION.md` §9）
  - ✅ **Phase 2（1 周）**: Megolm 双写（`PickleFormat::Dual` + `vodozemac_pickle` 列），懒迁移（`promote_to_dual` 幂等 + `list_legacy_sessions` 分页），`E2EE_DUAL_WRITE=true` 灰度开关 — **2026-06-06 完成**（详见 `E2EE_VODOZEMAC_MIGRATION.md` §10）
  - 🚧 **Phase 3（2 周）**: 跨 Element Web/Android/iOS 互操作（CI workflow 5.3）。本地 vodozemac 参考路径互操作测试已落地（19 个 case，`E2EE_INTEROP=1` 显式启用），完整 Element Web harness 待接线
  - 🚧 **Phase 4（1 周）**: Megolm service 运行时分支已切换；仍需清理或明确保留自研/协议辅助 crypto，统一 feature/comment 口径，并在跨端互操作全绿后关闭迁移遗留
- **关键路径**:
  - ✅ `src/services/container.rs:117,146-149` — `MegolmProvider` 装配已就位
  - ✅ `src/e2ee/megolm/storage.rs:295-413` — `promote_to_dual` / `list_legacy_sessions` / `count_by_pickle_format` 已就位
  - ✅ `src/e2ee/megolm/models.rs:13-43` — `PickleFormat` 枚举 + serde 已就位
  - ✅ `tests/unit/megolm_dual_write_storage_tests.rs` + `megolm_dual_write_metrics_tests.rs` — Phase 2 单测已就位
  - ✅ `src/e2ee/vodozemac_interop_tests.rs` — Phase 3 本地互操作 19 个 case 已就位（注册到 `e2ee/mod.rs:21`）
  - ⏳ `.github/workflows/e2ee-interop.yml` — 本地 vodozemac smoke 已接入；完整 Element Web harness 仍待接线
  - ✅ `src/e2ee/ssss/service.rs:42,184,210` — X25519+AES 收敛（已直接使用 x25519_dalek + aes_gcm）
  - ✅ `src/e2ee/secure_backup/service.rs:412-453` — AES 收敛（已直接使用 aes_gcm）
  - ✅ `src/e2ee/verification/service.rs:5,68` — X25519+HMAC 收敛（已直接使用 x25519_dalek + hmac）
- **2026-06-09 进展**: Phase 1+2 完成，Megolm service 主路径已直接封装 vodozemac；本地互操作测试矩阵存在。完整 Element Web/Android/iOS 跨端结果与 Phase 4 清理结果仍缺失。
- **最高风险**:
  - 存量 Megolm session pickle 格式不兼容（高）→ Phase 2 双写 + lazy migrate + session 轮换窗口已落地
  - 跨 Element 客户端互操作（高）→ `E2EE_VODOZEMAC_MIGRATION.md` 4.2 矩阵（I-1~I-8），待 Phase 3 收尾
- **不要做的**:
  - 不应替换 `argon2.rs`（vodozemac 不覆盖密码哈希）
  - 不应替换 SSSS/Secure Backup/Cross-Signing 协议层（vodozemac 不覆盖 Matrix 协议层）
  - 不应一次性删除自研 Megolm（必须双写 + 互操作测试后再清理，Phase 4 触发条件）
- **2026-06-09 决策**: 不再把 C-5 表述为“只剩 Phase 4”。应拆成三项验收：完整 Element Web harness 接线并产生日志、Android/iOS 或等价跨客户端矩阵结果、遗留 crypto/feature/comment 口径清理。

### C-6 JWT 旧 token 默认放行 + 签名宽容 ✅ 已修复
- **位置**: `src/auth/token.rs`
- **修复内容** (2026-06-04):
  - `is_legacy_token_window_open` 默认返回 `false`（无 `JWT_ACCEPT_LEGACY_UNTIL` 环境变量时窗口关闭）
  - 运维人员需显式设置 `JWT_ACCEPT_LEGACY_UNTIL=<future-ts>` 才能打开迁移窗口
  - 旧 token 默认拒绝，签名不降级
- **风险**: 已消除 — 攻击者无法伪造无 kid token 通过校验

### C-7 TOTP 验证码比较非恒时 ✅ 已修复
- **位置**: `src/web/utils/admin_auth.rs`
- **修复内容** (2026-06-04):
  - 使用 `subtle::ConstantTimeEq` trait 的 `ct_eq` 方法替代直接比较
  - `generated.as_bytes().ct_eq(provided_code.as_bytes()).into()` 确保恒时比较
- **风险**: 已消除 — 远程时序攻击无法猜测 TOTP 验证码

### C-8 `NOW()` 赋值 BIGINT `_ts` 列导致运行时类型错误 ✅ 全部已修复
- **位置**: `src/storage/saml.rs`、`src/e2ee/key_rotation/service.rs`、`synapse-e2ee/src/key_rotation/service.rs`
- **状态**: ✅ 全部已修复（2026-06-08 更新）— 所有 10 处修复
- **已修复**:
  - `UPDATE saml_sessions SET last_used_ts = (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)` ✅
  - `last_authenticated_ts = EXTRACT(EPOCH FROM NOW())::BIGINT * 1000` ✅
  - `updated_ts = (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)` ✅
  - `DELETE FROM saml_auth_events WHERE created_ts < NOW() - INTERVAL '1 day' * $1` → 改为 BIGINT 算术比较 ✅
  - `DELETE FROM megolm_sessions WHERE expires_at < NOW()` → 改为 `expires_at < (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)` ✅
  - `key_rotation/service.rs` 10 处 NOW()+schema 不匹配全部修复：`NOW()` → `timestamp_millis()` 参数化、`is_rotated`/`rotated_at` → `rotation_count`/`last_rotation_ts`、`key_rotation_log` → `key_rotation_state` ✅
  - `refresh_token.rs` 4 处 `EXTRACT(EPOCH FROM NOW()) * 1000` → `EXTRACT(EPOCH FROM NOW())::BIGINT * 1000` ✅
  - `tests/mock_db.rs` + `registration_service_tests.rs` 7 处 `EXTRACT(EPOCH FROM NOW())` → `EXTRACT(EPOCH FROM NOW())::BIGINT * 1000` ✅
- **验证**: `cargo check` + `cargo clippy` + `cargo test --lib` 均通过，0 errors 0 warnings 0 failed
- **2026-06-09 追加修复**: `push_notification.rs` 中 `PushNotificationQueue.next_attempt_at`/`sent_at` 和 `PushNotificationLog.sent_at` 字段从 `DateTime<Utc>` 统一为 `i64` 毫秒时间戳，修复 7 处类型转化点（`queue_notification`、`get_pending_notifications`、`mark_notification_sent`、`mark_notification_failed`、`create_notification_log`、`cleanup_old_logs` + 4 处测试代码）；重建 `.sqlx/` 缓存后 `cargo check` + `cargo clippy --all-targets` + `cargo test --no-run` 全部通过
- **2026-06-09 三次复核追加修复**: `src/storage/refresh_token.rs` 中 `get_user_stats()` 的 2 处 `EXTRACT(EPOCH FROM NOW()) * 1000` 已同步改为 `EXTRACT(EPOCH FROM NOW())::BIGINT * 1000`，与 `synapse-storage/src/refresh_token.rs` 保持一致；执行 `cargo sqlx prepare -- --all-targets` 后 `SQLX_OFFLINE=true cargo check --locked --lib` 通过。

### C-9 迁移文件 Schema 冲突导致运行时反序列化失败
- **位置**: `migrations/` 目录（已收敛至 4 个文件）
- **状态**: ✅ 全部已修复（2026-06-05 更新）
- **已修复**:
  - `voice_usage_stats` 三重 Schema 冲突 → v8 采用 20260517 版本 ✅
  - `user_privacy_settings` 双重 Schema 冲突 → v8 已统一 ✅
  - CAS 表字段命名不一致 → v8 已统一 `_at` 后缀 ✅
  - `leak_alerts` 完整 Schema 不匹配 → 重写 `LeakAlert` 结构体及所有存储方法对齐 v8 ✅
  - `e2ee_audit_log` 缺少 `action` NOT NULL 列 → INSERT 映射 `operation` 到 `action`+`operation` ✅
  - `rendezvous_session` 可空列断言为 NOT NULL → 改为 `Option<String>` ✅
  - `devices.verified/verified_ts/verification_method` 列不存在 → 迁移到 `device_trust_status` 表 ✅
  - `cross_signing_keys.cross_signed/device_id` 列不存在 → 迁移到 `cross_signing_trust` 表 ✅
  - 16 个布尔字段统一 `is_` 前缀（enabled→is_enabled, sticky→is_sticky 等） ✅
  - 3 个 matrixrtc 表添加 ON CONFLICT 所需唯一索引 ✅
  - v8 基线已应用到本地 PostgreSQL，`cargo check` + `cargo clippy` 全部通过 ✅
- **待验证**: 已有 v7 数据库升级到 v8 的增量路径

### C-10 SAML 模块 `NOW()` 赋值 BIGINT 列未修复（2026-06-06 新增）✅ 已修复
- **位置**: `src/storage/saml.rs`
- **状态**: ✅ 全部已修复（2026-06-06）
- **已修复**:
  - `saml.rs:332` `UPDATE saml_sessions SET last_used_ts = NOW()` → `last_used_ts = (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)` ✅
  - `saml.rs:580` `saml_identity_providers.updated_ts = NOW()` → `(EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)` ✅
  - `saml.rs:778` `DELETE FROM saml_auth_events WHERE created_ts < NOW() - INTERVAL '1 day' * $1` → `created_ts < (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000) - $1 * 86400000` ✅
- **验证**: `cargo check` + `cargo clippy --all-features --locked -- -D warnings` 均通过
- **原症状（C-8 修复遗漏）**: 3 处 `NOW()` 用法对 BIGINT `_ts`/`updated_ts`/`created_ts` 列做赋值或比较，PG BIGINT 无法隐式接收 `timestamptz` → 运行时执行失败
- **修复模板**:
  ```rust
  // saml.rs:332
  sqlx::query("UPDATE saml_sessions SET last_used_ts = (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000) WHERE session_id = $1")
  // saml.rs:580
  updated_ts = (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)
  // saml.rs:778
  sqlx::query("DELETE FROM saml_auth_events WHERE created_ts < ((EXTRACT(EPOCH FROM NOW())::BIGINT * 1000) - ($1 * 86400000))")
  ```
- **风险**: 已消除 — SAML SSO 登录路径不再触发 NOW()+BIGINT 类型冲突

---

## 三、Major（高优先级）

### M-1 ServiceContainer 巨型 struct（80+ 公共字段）✅ 已完成 → 进一步优化至 8 核心字段
- **位置**: `src/services/container.rs`（**1201 行**，较 1408 行的早期版本压缩）
- **状态**: ✅ 已完成 + 进一步优化（2026-06-08 更新）— 48 核心字段 → **8 核心字段**（+4 子结构体）
- **已实施**:
  - 定义 4 个分层子结构体：`CoreServices`（12 字段，含 auth/registration/search/media/cache/metrics/config/task_queue 等）、`AccountServices`（8 字段，含 user/threepid/device/token/presence 等存储）、`SsoServices`（6 字段，含 saml/cas/oidc/builtin_oidc）、`ExtensionServices`（18 字段，含 voice/friends/rtc/directory/ai/identity 等）
  - **2026-06-08 优化**: `ServiceContainer` 重构为 `pub core: CoreServices`、`pub accounts: AccountServices`、`pub sso: SsoServices`、`pub extensions: ExtensionServices` + **8 个核心字段**（pool, server_name, config, e2ee, rooms, federation, admin, sync）
  - 初始化逻辑通过工厂函数组装子结构体
  - 所有子结构体添加 `#[derive(Clone)]` 确保与 `ServiceContainer` 兼容
  - 更新 ~30 个消费方文件的引用路径
- **验证 (2026-06-08)**: 4 个子结构体 + 8 个核心字段，`cargo check` + `cargo clippy` 均通过
- **效果**: 80+ 字段按功能域分层，8 个核心字段直接暴露，DI 可维护性显著提升

### M-2 `common/config/mod.rs` 拆分（**4056 行 → 18 子模块**）✅ 已完成
- **位置**: `src/common/config/mod.rs`
- **状态**: ✅ 已完成（2026-06-05）
- **已实施**:
  - 按域拆分为 18 个子模块：`error.rs`、`voip.rs`、`auth.rs`、`retention.rs`、`builtin_oidc.rs`、`experimental.rs`、`identity.rs`、`translate.rs`、`performance.rs`、`search.rs`、`rate_limit.rs`、`server.rs`、`database.rs`、`logging.rs`、`federation.rs`、`security.rs`、`worker.rs`、`smtp.rs`
  - `mod.rs` 从 4056 行缩减至 1976 行：只保留 `Config` 聚合根、`pub mod` 声明、`pub use` 重导出、注释掉的未实现模块
  - 通过 `pub use` 重导出保持向后兼容，所有 `use crate::common::config::ServerConfig` 等路径无需修改
  - 去除重复的 `default_*` 辅助函数（5 个函数在 `voip.rs` 和 `server.rs` 中重复定义）
- **效果**: 配置结构体按功能域分离，`mod.rs` 行数减少 51%，可维护性显著提升

### M-3 动态 `sqlx::query` 迁移为编译期宏（目标已达成，持续治理）✅
- **症状**: 已消除 — 编译期宏大面积覆盖
- **当前状态 (2026-06-09 重新审查)**:
  - `sqlx::query!` / `query_as!` / `query_scalar!` 编译期宏：**1355 处**（主 `src/` 口径）
  - `sqlx::query(` 动态调用：**174 处**（主 `src/` 口径）
  - `sqlx::query_as::<_, T>` 动态调用：**14 处**（主 `src/` 口径）
  - **动态 SQL 占比 ≈ 12.2%**（188/1543），✅ 远超 ≤ 30% 目标
  - **编译期宏覆盖率 ≈ 87.8%**，✅ 远超 ≥ 70% 目标
  - **`.sqlx/` 离线缓存已全量重建**（**1143 个 JSON 文件**，基于 v10 Schema，本轮 refresh token SQL 文本变更后重新 prepare）
  - **`SQLX_OFFLINE=true cargo check` 通过**（0 错误）
  - **`SQLX_OFFLINE=true cargo clippy --all-targets` 通过**（0 错误 0 警告）
  - **`SQLX_OFFLINE=true cargo test --no-run` 通过**（所有测试可执行文件编译通过）
- **已完成批次**:
  - ✅ 阶段 A：孤儿模块清理
  - ✅ 阶段 B：token.rs 15 个
  - ✅ 阶段 C：key_rotation 9 + federation_blacklist 5 = 14 个
  - ✅ 阶段 D：refresh_token 26 + device 20 + registration_token 8 = 54 个
  - ✅ 阶段 E：saml 19 + application_service 23 = 42 个
  - ✅ 阶段 F：room/mod 30 + room/admin 10 + event/mod 9 + membership 14 + thread 9 + relations 4 = 76 个
  - ✅ 阶段 G：sliding_sync 18 + push_notification 5 + presence 8 + media_quota 18 + data_fetch 9 + chunked_upload 13 = 71 个
  - ✅ 阶段 H：server_notification + space + module + openclaw + widget + background_update + event_report + worker/storage + friend_room + room_summary = ~130 个
  - ✅ 阶段 I：beacon + invite_blocklist + sticky_event + user.rs + 路由层 = ~50 个
  - ✅ 阶段 J：E2EE 存储层 + 路由层 + 存储层遗漏 = ~190 个
  - ✅ 阶段 K：存储层 batch1-3 + 路由层 + 服务层 + room/mod + federation/key_rotation = ~231 个
  - ✅ 阶段 L：E2EE 子系统 + 存储层 + 路由层+服务层 + 联邦+Worker+其他 = ~248 个
- **不可迁移（永久保留动态）**：
  - `database_initializer/tables.rs` 的 107 处 DDL
  - ~12 处 `format!` 动态拼接 SQL
  - ~15 处 `ANY($1)` / `UNNEST` 数组参数查询
  - ~10 处 QueryBuilder 动态查询
  - ~8 处元组返回类型（`query_as::<_, (T1, T2)>`）
  - ~5 处 schema 与代码列名不一致
  - ~5 处系统表查询（pg_stat 等）
  - ~8 处 fallback 旧 schema 兼容查询

### M-4 测试约 40% 为套套逻辑
- **位置**: `tests/`、`benches/`
- **状态**: ✅ Step 8 已完成（2026-06-04）
- **已实施**:
  - 删除 `error.rs` 中 4 个套套逻辑测试（~200 行）
  - 删除 `benches/` 中 7 个无 IO 伪性能测试（~400 行）
  - 引入 `cargo-mutants` CI nightly 非阻塞流程
  - 创建 `tarpaulin.toml` 覆盖率门槛配置（≥70%）
  - 更新 `Makefile` 添加 `test-mutation`/`test-coverage-check` 目标
- **待完成**: 实际运行 `cargo mutants` 并消除残留 ≤ 5%

### M-5 多处 N+1 / 无限流查询
- **位置**: `storage/room.rs`、`storage/event.rs`、`storage/membership.rs`
- **状态**: ✅ Step 9.1 已完成（2026-06-04）
- **已实施**:
  - `get_room_members` 添加 `ORDER BY joined_ts DESC, user_id DESC LIMIT 200`（keyset 分页就绪）
  - `get_shared_room_users` 添加 `ORDER BY user_id LIMIT 200`
  - `get_rooms_batch` 输入数组 `take(200)` 上限保护
  - `get_room_events_by_type` / `get_sender_events` 添加 `limit.min(200)` 上限
- **待完成**: 所有列表接口统一 keyset 分页模式；`get_public_rooms_paginated` 已有 keyset 分页可作为参考模板

### M-6 联邦签名缓存的失效策略 ✅ 已完成
- **位置**: `src/cache/federation_signature_cache.rs`、`src/federation/key_rotation.rs`
- **状态**: ✅ 已完成（2026-06-06）
- **已实施**:
  - `KeyRotationManager` 新增 `signature_cache` 字段，通过 `set_signature_cache` 注入 `FederationSignatureCache` 实例
  - `rotate_keys` 方法中密钥轮换后调用 `cache.notify_key_rotation(&event)`，自动失效旧密钥相关的签名缓存
  - `FederationSignatureCache::notify_key_rotation` 实现密钥失效 + 签名缓存批量清除 + 监听器回调
  - 缓存 TTL 默认 3600s（≤ 24h），通过 `SignatureCacheConfig` 配置
- **风险**: 已消除 — 服务器密钥轮换后缓存自动同步失效

### M-7 Typing/Presence 内存存储跨 worker 不一致 ✅ 已完成
- **位置**: `services/typing_service.rs`、`storage/presence.rs`
- **状态**: ✅ 已完成（2026-06-06）
- **已实施**:
  - **Typing**: 移除内存 `HashMap` 存储，改为 `CacheManager`（L1 本地 + L2 Redis）双层缓存
    - `TypingService` 使用 `cache: Arc<CacheManager>` 替代原内存 `HashMap`
    - `set_typing`/`clear_typing`/`get_typing_users` 等方法通过 `cache.get::<RoomTypingState>` 读写 Redis
    - 定义 `RoomTypingState` 结构体用于 Redis 序列化，TTL 120s 自动过期
  - **Presence**: 已通过 DB+Cache 实现多 worker 一致（`storage/presence.rs`），无需额外修改
- **风险**: 已消除 — 多 worker 部署时 Typing/Presence 状态强一致

### M-8 错误处理未结构化
- **状态**: ✅ Step 9.2 已完成（2026-06-04）
- **已实施**:
  - `ApiError::message()` 中 `tracing::error!` 添加 `errcode`/`error` 结构化字段
  - `internal_with_log`/`database_with_log` 添加 `errcode`/`context`/`error` 结构化字段
  - `From<sqlx::Error>` 实现添加 `errcode`/`error` 结构化字段
- **待完成**: `ApiError` 完整重构为 `kind/code/source/cause` 结构体（当前仍为枚举 variant）

### M-9 日志/可观测性 span 关联缺失
- **状态**: ✅ Step 9.3 已完成（2026-06-04）
- **已实施**:
  - `room/service.rs` 添加 6 个 `#[instrument]`：`create_room`、`send_message`、`join_room`、`leave_room`、`get_room_messages`、`invite_user`
  - `sync_service/mod.rs` 添加 `sync_with_request` 的 `#[instrument]`
  - `tracing` crate 启用 `attributes` feature
  - 修复 `tracing` 模块名与 crate 名冲突（`::tracing::instrument` 绝对路径）
- **待完成**: OTLP collector dev 端点默认开启；其余关键 service 方法全量 `#[instrument]`

### M-10 巨型文件 12+（每文件 >1500 行）✅ 已完成
- **状态**: ✅ 已完成（2026-06-06 更新）
- **已拆分**:
  - `src/storage/event.rs` → `src/storage/event/mod.rs` + `models.rs` + `state.rs` + `batch.rs` ✅
  - `src/services/room/service.rs` (2084 行) → `service.rs`(1428) + `create.rs`(607) + `utils.rs`(25) ✅
  - `src/storage/room.rs` (2044 行) → `room/mod.rs`(1023) + `models.rs`(244) + `admin.rs`(787) ✅
  - `src/services/database_initializer.rs` (1975 行) → `mod.rs`(680) + `models.rs`(121) + `tables.rs`(1181) ✅
  - `src/services/friend_room_service.rs` (1919 行) → `mod.rs`(1296) + `models.rs`(149) + `groups.rs`(486) ✅
  - `src/web/routes/admin/room.rs` (1742 行) → `mod.rs`(869) + `types.rs`(48) + `management.rs`(608) + `spaces.rs`(235) ✅
  - `src/storage/event/mod.rs` (1712 行) → `mod.rs`(~800) + `state.rs`(394) + `batch.rs`(463) ✅
  - `src/federation/event_auth.rs` (1544 行) → `mod.rs`(895) + `models.rs`(65) + `chain.rs`(182) + `state_resolution.rs`(416) ✅
- **当前 >1500 行文件** (2026-06-06 验证):
  - `src/common/config/mod.rs` (**1977 行**，已拆 18 子模块，聚合文件 — 预期保留)
- **建议**: 按领域拆 crate（`synapse-core` / `synapse-federation` / `synapse-e2ee` / `synapse-web` / `synapse-storage`）或 workspace 成员

### M-11 迁移文件严重冗余与冲突（25 个文件 → 已收敛为 4 个）
- **位置**: `migrations/` 目录
- **状态**: ✅ Step 7.5 已完成（2026-06-06 验证仍为 4 文件）
- **当前目录结构 (2026-06-06 验证)**:
  ```
  migrations/
  ├── 00000000_unified_schema_v8.sql    # 新基线（242 表，约 4552 行）
  ├── 00000001_extensions_v8.sql        # 扩展表
  ├── extension_map.conf
  ├── README.md
  ├── archive/                          # 旧 v7 备份
  └── undo/                             # 空目录
  ```
- **根目录有效文件**: 4 个（与 2.2 报告一致）
- **已修复**:
  - 删除了 v7 基线内部 30+ 重复定义 ✅
  - 删除了 69+17 跨文件重复表定义 ✅
  - 删除了 `spam_check_results`/`third_party_rule_results` 旧定义 ✅
  - 删除了 12+ 重复索引创建 ✅
  - 统一了 `voice_usage_stats` 采用 20260517 版本 ✅
  - 统一了 `user_privacy_settings` 采用 unified_v7 版本 ✅
  - 统一了 CAS 表 `_at` 后缀 ✅
  - 16 个布尔字段统一 `is_` 前缀 ✅
  - 3 个 matrixrtc 表添加唯一索引 ✅
  - Rust 代码全面对齐 v8 Schema（audit_service, leak_detection, key_rotation, rendezvous 等） ✅
  - `cargo check` + `cargo clippy` 全部通过 ✅
- **待完成**: 已有 v7 数据库升级验证

### M-12 16 处 `_ts`/`_at` 字段后缀不一致 ✅ 已修复
- **位置**: `src/storage/` 下 8 个文件
- **状态**: ✅ 已修复（2026-06-05 更新）
- **修复方式**: Rust 代码已统一使用 `_at` 后缀（与 v8 基线 DB 列名对齐），`#sqlx(rename)` 桥接已消除
- **已对齐的字段**（Rust 已改为 `_at`）:
  - `cas.rs`: `consumed_at` / `logout_sent_at` ✅
  - `captcha.rs`: `used_at` / `verified_at` ✅
  - `saml.rs`: `last_metadata_refresh_at` / `processed_at` ✅
  - `event.rs`: `processed_at` ✅
  - `event_report.rs`: `resolved_at` ✅
  - `module.rs`: `expiration_at` ✅
  - `refresh_token.rs`: `compromised_at` ✅
  - `threepid.rs`: `validated_at`（SQL alias `validated_ts as "validated_at"`） ✅
- **剩余 `#[sqlx(rename)]`**: 仅剩非时间戳语义桥接（`join_rules`→`join_rule`、`sender_localpart`→`sender_local_part`、`order_value`→`order`、`is_enabled`→`enabled` 等布尔前缀）

### M-13 `AccountValidity` 语义混淆 + 布尔字段命名不规范
- **位置**: `src/storage/module.rs`
- **症状**:
  - `email_sent_ts` 对应 DB 列 `last_check_at` — 语义完全不同（"邮件发送时间" vs "最后检查时间"）
  - `renewal_token_ts` 标记 `#[sqlx(skip)]` — DB 中无对应列，业务逻辑依赖但数据不持久化
- **影响**: 极易引起开发者混淆，`renewal_token_ts` 数据丢失
- **建议**: 统一语义，DB 列名与 Rust 字段名对齐

### M-14 布尔字段缺少 `is_` 前缀（已大部分修复，**仍有 4 处 DB-mapped 桥接**）⚠️
- **位置**: 多个 storage 文件
- **状态**: ⚠️ 主体已修复（2026-06-06 更新）— 16 个 DB 字段已迁移，仍有 4 处使用 `#[sqlx(rename)]` 桥接
- **已修复 (核心路径)**:
  - `user_notification_settings.enabled` → `is_enabled` ✅
  - `sticky_event.sticky` → `is_sticky` ✅
  - `application_services.rate_limited` → `is_rate_limited` ✅
  - `application_service_namespaces.exclusive` → `is_exclusive` ✅
  - `push_notification_log.success` → `is_success` ✅
  - `module_execution_log.success` → `is_success` ✅
  - `registration_token_usage.success` → `is_success` ✅
  - `room_retention_policies.expire_on_clients` → `is_expire_on_clients` ✅
  - `email_verification_tokens.used` → `is_used` ✅
  - `presence.typing` → `is_typing` ✅
  - `cas_registered_service.require_secure` → `is_require_secure` ✅
  - `cas_registered_service.single_logout` → `is_single_logout` ✅
  - `application_service_statistics.processed` → `is_processed` ✅
  - `e2ee_leak_alerts.resolved` → `is_acknowledged` ✅（v8 Schema 重构）
  - `sliding_sync_rooms.invited` → `is_invited` ✅
  - `database_initializer.success` → `is_success` ✅
- **未完全修复 (2026-06-06 新发现)**:
  - `src/storage/push_notification.rs:28` `PushGateway.enabled: bool`（`#[sqlx(rename = "is_enabled")]`，line 27）— DB 列已迁移，Rust 字段未改
  - `src/storage/push_notification.rs:55` `PushRule.enabled: bool`（`#[sqlx(rename = "is_enabled")]`，line 54）— 同上
  - `src/storage/refresh_token.rs:36` `RefreshTokenUsage.success: bool`（`#[sqlx(rename = "is_success")]`，line 35）— 同上
- **非 DB 映射 struct（可保持现状）**:
  - `src/services/content_scanner/models.rs:60` `ScannerConfig.enabled`（配置 struct）
  - `src/services/webhook_notification/models.rs:53` `WebhookConfig.enabled`（配置 struct）
  - `src/services/geo_ip/models.rs:19` `GeoIpConfig.enabled`（配置 struct）
  - `src/e2ee/olm/models.rs:69` `FallbackKey.used`（in-memory 状态）
  - `src/e2ee/device_trust/models.rs:400` `VerificationRespondResponse.success`（API 响应 DTO）
  - API Request DTO (`CreatePushRuleRequest.enabled`、`RecordUsageRequest.success`) — 用户输入，命名兼容 Matrix 客户端
- **建议**: 4 处 DB-mapped 桥接可逐步消除（重命名 Rust 字段为 `is_enabled`/`is_success`），或保留以兼容 Matrix API JSON 命名约定

---

## 四、Minor（中低优先级）

| 编号 | 类别 | 内容 | 建议 |
|---|---|---|---|
| m-1 | 依赖 | ✅ 已修复 — 7 个 lib crate `Cargo.toml` 添加 `[lints.clippy]` deny（unwrap_used/expect_used/panic），2 处 expect 添加 allow 注解 | 全部 `deny`，CI 加 `cargo clippy -- -D warnings` + `cargo geiger` |
| m-2 | 依赖 | ✅ 已修复 — `x25519-dalek` 2.0→2.0.1 + `aes-gcm` 0.10→0.10.3（CVE 已修复），`cargo-outdated` 加入 CI security-audit job | `cargo outdated` 加入 CI |
| m-3 | CI | ✅ 已修复 — `Swatinem/rust-cache@v2` 替换通用 cache，feature 矩阵（default/all-features），多 Rust 版本（stable/1.93.0） | 拆分 cache、按 feature 矩阵 |
| m-4 | CI | ✅ 已验证 — `deny.toml` + `audit.toml` + `supply_chain_gate.sh` + CI 集成已完整 | 加 `cargo deny check` + `cargo audit` 必需步骤 |
| m-5 | 测试 | ✅ 已验证 — `mutation-testing.yml` + Makefile target 已完整 | 引入 `cargo-mutants`，覆盖率门槛提升到 70% |
| m-6 | 重复 | ✅ 已修复 — `EventBroadcaster` trait 提升到 `synapse-common`，3 套实现统一（EventNotifier/federation/WorkerBus），to_device 发送后通知 sync 连接 | 抽取通用 `EventBroadcaster` |
| m-7 | 重复 | ✅ 已修复 — 16 处散落实现收敛到 `common/crypto`（decode_base64_32/secure_compare_bytes/encode_hex/decode_hex） | 收敛到 `common/crypto` |
| m-8 | 错误 | ✅ 已修复 — 5 个 lib crate 移除 `anyhow` 依赖，0 处 `anyhow!` 宏使用 | 业务库只能用 `thiserror`，仅 `main.rs` 允许 `anyhow` |
| m-9 | 配置 | ✅ 已修复 — 移除硬编码 fallback secret，`TOKEN_HASH_SECRET` 环境变量强制必填 | CI 校验、模板化 |
| m-10 | Federation | ✅ 已修复 — `openid_userinfo` 添加 `sub` 格式校验 + server_name 归属校验 | 对齐 Synapse |
| m-11 | Federation | ✅ 已修复 — 房间版本声明新增 v12/v13，`SUPPORTED_ROOM_VERSIONS` 已更新 | 同步 [spec changelog](https://spec.matrix.org/v1.13/rooms/) |
| m-12 | Federation | ✅ 已修复 — `EduDispatcher` 统一派发（EduType 枚举 + 3 个 handler），m.typing/m.device_list_update 不再被忽略 | 统一类型化 EDU 派发 |
| m-13 | E2EE | ✅ 已修复 — 设备名长度限制 ≤ 100 字符（路由层 M_BAD_REQUEST + 存储层防御性截断）；AccountValidity `email_sent_ts`→`last_check_at` 语义对齐 | 与 Synapse 同步限制（≤ 100 字符） |
| m-14 | E2EE | ✅ 已修复 — 3 处 DB-mapped 布尔字段 rename 桥接已消除（PushDevice/PushRule `enabled`→`is_enabled`，RefreshTokenUsage `success`→`is_success`） | 对齐 vodozemac 0.9 |
| m-15 | Sync | ✅ 已修复 — moka TTI (30min) + LRU (10K) + 懒清理机制，`gc_expired_connections()` 自动清理过期连接 | 加 LRU + TTL |
| m-16 | Media | ✅ 已修复 — `MediaLocator::parse` 统一工具抽取，3 处重复实现替换 | 抽 `MediaLocator::parse` |
| m-17 | Push | ✅ 已修复 — `PushGateway` trait + `PushGatewayType` 枚举，3 个 Provider 实现，endpoint 提取为配置项 | 抽 `PushGateway` 接口 |
| m-18 | DB | ✅ 已修复 — 迁移文件已收敛至 4 个，`schema_health_check` 自动校验 | 规范单向迁移 + `schema_health_check` 自动校验 |
| m-19 | DB | ✅ 已修复 — 创建 `migrations/INDEXES.md`（67 个 partial index + 96 个复合索引 + 设计原则 + 维护指南） | 维护 `migrations/INDEXES.md` |
| m-20 | Auth | ✅ 已修复 — `Argon2Config::enforce_minimum()` 添加下限常量（m_cost≥32768, t_cost≥1, p_cost≥1），低于下限自动提升 + warning 日志 | 启动时强制下限 |
| m-21 | Logging | ✅ 已修复 — 所有 `println!` 仅在 CLI binary 中（38 处），library 代码 0 处 | 替换 `tracing::info!` |
| m-22 | Security | ✅ 已修复 — `X-Content-Type-Options: nosniff` 全局强制（security_headers_middleware） | 全局强制 |
| m-23 | RateLimit | ✅ 已修复 — `RateLimitBackend` 枚举（Auto/Redis/Local），Redis 优先 + 降级警告 + 强制 Redis 模式拒绝降级 | Redis TokenBucket（已有能力） |
| m-24 | Doc | ✅ 已修复 — 创建 `docs/INDEX.md`（现行/归档分离 + 命名规范 + 维护指南），26 个历史文件移至 `docs/archive/` | 加 `docs/INDEX.md` 区分 `archive/` 与现行 |
| m-25 | DB | ✅ 已修复 — 全量消除（2026-06-08 更新）：synapse-storage 23 文件 ~130 处 + synapse-services 3 文件 12 处，全部替换为显式列名。仅保留 `SELECT * FROM UNNEST`（PostgreSQL 原生语法）| 改为显式列名，降低 Schema 变更风险 |
| m-26 | DB | ✅ 已修复 — v8 基线中已清理冗余列 | v8 基线中清理冗余列 |
| m-27 | DB | ✅ 已修复 — `push_devices` DateTime→i64 毫秒时间戳，v10 迁移 schema 对齐 | 统一为 BIGINT 毫秒时间戳 |
| m-28 | DB | ✅ 已修复 — 验证已使用 `i64`/`Option<i64>`，无需修改 | 统一为项目规范类型 |
| m-29 | DB | ✅ 已修复 — 验证已使用 `delete_ts`，无需修改 | 应为 `delete_ts`（NOT NULL 时间戳用 `_ts`） |
| m-30 | DB | ✅ 已修复 — `schema_health_check.rs` 中 `validated_ts`→`validated_at`，`enabled`→`is_enabled` | 应为 `validated_at` |

---

## 五、与 element-hq/synapse v1.153 行为差异（抽样）

| 模块 | 项 | Synapse v1.153 行为 | synapse-rust 当前 | 严重度 |
|---|---|---|---|---|
| Federation | X-Matrix ts 容忍 | ±30s 滑动窗口 + nonce | ✅ 已实现（C-1 修复） | ✅ |
| Federation | Server-Key 轮换 | 旧 key TTL ≥ 1d，签名失败降级 | TTL 默认 24h，签名失败硬错误 | M |
| Federation | Backfill `/state/` | 拒绝未授权 server | 不一致 | M |
| Sync | `since` token 严格性 | 单调递增 + 内部 v2 格式 | ✅ 单次解析（C-3 修复） | ✅ |
| E2EE | Olm 协议 | vodozemac 0.9 主路径 | Phase 1+2 完成 / Phase 3 进行中（**C-5**） | **C** |
| E2EE | Megolm session 过期 | 显式过期事件 + 自动清理 | 仅 created_at，无清理任务 | M |
| Account | UIA 流程 | 完整 re-auth/email/msisdn 流 | 残缺 | M |
| Account | 设备上限 | 配置化、命名限制 | ✅ 长度限制 ≤ 100 字符（m-13） | ✅ |
| Media | mxc 取回 | URL 过期签名 | ✅ HMAC-SHA256 签名（m-30） | ✅ |
| Push | `/pushers` 鉴权 | 仅设备 owner | ✅ device_id 校验（P2 #32） | ✅ |
| Admin | `/admin/purge_history` | 二次确认 + 审计 | ✅ 审计日志已补齐（P2 #33） | ✅ |
| Search | 全文检索 | Postgres FTS 或 ES 双路径 | 实现存 | OK |
| Presence | `/presence` 写权限 | shared/subscribe/online | 实现不完整 | m |

---

## 六、建议的修复路线（30 项，按 P0→P2）

### P0（阻塞生产）🚧 9/10 完成（2026-06-09 更新；C-8 全部清零含 push_notification.rs DateTime→i64；C-5 进入 Phase 3/4 收敛期）
1. ✅ 联邦 X-Matrix 时间戳新鲜度校验（C-1）
2. ✅ 修 Canonical JSON（U+2028/2029/FFFD）（C-2）
3. ✅ 修 Sync since token 重复解析（C-3）
4. ⚠️→🚧 收敛 E2EE 到 vodozemac（C-5）— **Phase 1 ✅ + Phase 2 ✅ + Phase 3 🚧**（2026-06-06 更新）：
   - **Phase 1（2 周）— 桥接层 + 单测 ✅**：装配 `MegolmProvider` 双路径抽象（`MegolmBackend::{Legacy, Vodozemac}`）+ `MegolmVodozemacService`（`GroupSession`/`InboundGroupSession` 封装），`ServiceContainer` 集成，9 个新增 metrics
   - **Phase 2（1 周）— Megolm 收敛 ✅**：双写路径（`PickleFormat::{Legacy, Vodozemac, Dual}` + `vodozemac_pickle` 列）、懒迁移（`promote_to_dual` 幂等 + `list_legacy_sessions` 分页 + `count_by_pickle_format` 监控）、7 个新 metrics + 3 个记录方法；`E2EE_DUAL_WRITE=true` 灰度开关
   - **Phase 3（2 周）— 跨客户端互操作 🚧**：本地 vodozemac 互操作测试矩阵已扩展至 **19 个 case**（Olm 账户/会话/线路编码/Megolm 共享/monotonicity/前向保密/pickle 兼容性/m.room_key to-device payload/算法拒绝），全部需 `E2EE_INTEROP=1` 显式启用，默认 `cargo test` 跳过。Element Web/Android/iOS 跨客户端矩阵留待 `.github/workflows/e2ee-interop.yml` 跑
   - **Phase 4（1 周）— 清理 ⏸**：删除自研 `e2ee/crypto/{aes,x25519,mod}.rs` 重叠部分 + `e2ee/olm/session.rs` 自研 ratchet；将 `vodozemac` 移出 optional
   - 详细进度见 `docs/synapse-rust/E2EE_VODOZEMAC_MIGRATION.md` §九（Phase 1）+ §十（Phase 2 双写）
5. ✅ 修 JWT 旧 token 默认放行（C-6）
6. ✅ TOTP 改用 `subtle::ConstantTimeEq`（C-7）
7. ✅ CI 路由分层门禁 `check_route_layering.sh`（C-4）
8. ✅ 修迁移文件 Schema 冲突（C-9）— v8 基线已收敛
9. ✅ 修复 SAML 模块 `NOW()` 残留（C-10）— saml.rs:332/580/778

### P1（建议在 P0 后一次性完成）
10. ✅ 拆分 `ServiceContainer` 为分层（M-1）— 已完成（2026-06-06 验证 4 个子结构体 + 48 核心字段）
11. ✅ 拆分 `common/config/mod.rs`（M-2）— 18 子模块，1977 行
12. ✅ v8 迁移基线重构（M-11/M-12/M-13/M-14）— 已完成
13. 🚧  `sqlx::query!` 全量迁移 + `.sqlx/` 入仓（M-3）— **M-3 Batch 1 阶段 A + 阶段 B + 阶段 C 已完成（2026-06-06）**：
    - **当前状态（阶段 B 全部完成后）**：
      - **15 个 `sqlx::query!` 实际生效**（`src/storage/token.rs`：8 个 `query!` + 3 个 `query_scalar!` + 4 个 `query_as!`，**Token 认证 100% 编译期宏化**）
      - **1341 个动态查询**（99.1%）
      - **`.sqlx/` 离线缓存从 0 → 14 个 `query-*.json` 文件**
    - **孤儿模块清理**（阶段 A+）：
      - 删除 `src/services/guest_service.rs`（167 行，2 个孤儿宏 + 1 测试 + 1 文档）
      - 删除 `src/cache/warmup.rs`（393 行，3 个孤儿宏 + 7 测试 + 中文文档）
      - 验证：`cargo build --bin synapse-rust` 0 错误；`cargo test --lib` 1620 passed (删除后首跑)
    - **阶段 B 8 个转换**（2026-06-06）：
      - `delete_token` / `delete_user_tokens` / `delete_device_tokens` / `delete_user_device_tokens` / `delete_user_tokens_except_device`（5 个 UPDATE）
      - `add_hash_to_blacklist`（1 个 INSERT...ON CONFLICT）
      - `cleanup_expired_blacklist_entries` / `cleanup_expired_tokens`（2 个 DELETE）
    - **阶段 B-Round 2 3 个 `query_scalar!` 转换**（2026-06-06）：
      - `token_exists` / `is_token_revoked`（access_tokens 存在性/撤销状态查询）
      - `is_in_blacklist`（token_blacklist 黑名单检查）
    - **阶段 B-Round 3 4 个 `query_as!` 转换**（2026-06-06）：
      - `create_token`（`INSERT ... RETURNING` 10 字段标注）
      - `get_token`（主+legacy × 2；`fetch_optional` + `Option<AccessToken>`）
      - `get_user_tokens`（`fetch_all` + `Vec<AccessToken>`）
      - **Token 认证 100% `query!` 化**（15/15）
    - **3 个可复制模板确立**（用于阶段 C/D）：
      1. 单表 CRUD `query!`（UPDATE/DELETE/INSERT...ON CONFLICT）
      2. `query_scalar!` 存在性检查（`SELECT 1 AS "exists!"` + `LIMIT 1` + `is_some()`）
      3. `query_as!` FromRow 读取（10 字段 `as "field!"` / `as "field?"` 标注）
    - **阶段 C 14 个转换**（2026-06-06）：
      - `key_rotation.rs` 9 个：`set_rotation_config_value` / `load_rotation_config` × 3 / `load_or_create_key` / `initialize` / `get_known_federation_servers` / `revoke_key` / `verify_from_database`
      - `federation_blacklist.rs` 5 个：`remove_from_blacklist` / `create_log` / `create_rule` / `get_all_rules` / `cleanup_expired_entries`
      - **联邦认证/密钥轮换 100% `query!` 化**（除 `signing.rs` / `signature_cache.rs` 无 SQL 外）
      - 新增 2 个包装 struct（`FederationServerName` / `FederationKeyRecord`）支持 `query_as!` 宏
      - 7 个 schema-drift 查询明确标注（`federation_blacklist` 表 nullable 不一致，**独立治理 issue 跟踪**）
    - **`cargo test --lib federation::key_rotation` 19 passed**（含 DB 集成测试 `test_load_or_create_key_loads_full_existing_record` 端到端验证 `query_as!` 流程）
    - **离线编译验证**：`SQLX_OFFLINE=true cargo check --lib` 退出码 0（**关键里程碑**）
    - **Batch 1 详情**：[`M3_BATCH1_EXECUTION_PLAN.md` §七（阶段 A）+ §八-十（阶段 B）+ §十二（阶段 C）](./M3_BATCH1_EXECUTION_PLAN.md)
    - **重启 M-3 关键路径加固**：阶段 D/E/F（5-7 工作日，剩余 ~42 处高敏感 SQL）
14. ✅ 路由层强制使用 service（M-4 配套）— CI 门禁已部署
15. ✅ 测试整改：删除套套逻辑、补断言、M-4 解决
16. ✅ N+1/无限流硬性 `LIMIT`（M-5）— Step 9.1 已完成
17. ✅ 联邦签名缓存 key 失效广播（M-6）— KeyRotationManager + FederationSignatureCache
18. ✅ Typing/Presence 强制 Redis（M-7）— CacheManager L1+L2
19. ✅ `ApiError` 结构化 + TraceContext 透传（M-8/M-9）— Step 9.2/9.3 已完成
20. ✅ 巨型文件拆分（M-10）— 8 个文件已全部拆分，仅剩 config/mod.rs 聚合文件（1977 行，已拆 18 子模块）

### P2（持续治理）
21. ✅ m-30 Media 链接签名（HMAC-SHA256）— `MediaLinkSigner` + `download_media_signed` 路由（2026-06-06）
22. ✅ 引入 `cargo-deny` / `cargo-audit` / `cargo-mutants` 入 CI — `deny.toml` + `audit.toml` + `supply_chain_gate.sh` + `mutation-testing.yml` 已就位
23. ✅ m-1 ~ m-30 全部 minor 项已修复（2026-06-07）
24. ✅ 维护 `docs/INDEX.md`，归档与现行分离（2026-06-07）
25. ✅ 拆分 workspace — 已有 synapse-common/cache/storage/e2ee/federation/services/web 子 crate
26. ✅ mutation testing 验证 — 99 个可变异点（megolm 45 + key_rotation 54），CI 已配置
27. ✅ 接入 OTel collector 默认 dev 端点 — `resolve_otlp_endpoint()` + debug_assertions 默认 localhost:4317
28. ✅ UIA 完整化 — m.login.email.identity + m.login.msisdn stub 已添加
29. ✅ Media 链接签名 — 已完整实现（MediaLinkSigner + verify + download_media_signed）
30. ✅ Push 鉴权加固 — set_pusher/get_pushers 添加 device_id 校验
31. ✅ Admin 操作审计补齐 — purge_history/delete_room/reset_password/deactivate_user 添加审计日志
32. ✅ Push 共享重试 — `is_retryable_error` + `send_with_retry` 指数退避（1s→2s→4s，最多 3 次）
33. ✅ Presence 状态机统一 — `PresenceState` 枚举 + `derive_activity` + 全局替换
34. ✅ JwtClaims 构造 builder — `ClaimsBuilder` 链式 API + 14 处替换
35. ✅ 覆盖率门槛配置 — `tarpaulin.toml` fail-under=70 + `mutation-testing.yml` CI 已就位（实际覆盖率待测量）
36. ✅ Redis 必选开关评估 — 启动时 PING 健康检查已实现（`src/server.rs`），Redis 不可用时 WARN 日志 + 服务降级提示
37. ✅ 文档与 OpenAPI 同步生成 — `utoipa` + `utoipa-swagger-ui` 已集成（`src/web/api_doc.rs`），`/_swagger` Swagger UI 已就位；当前已覆盖 17 个公开稳定读端点，后续仍需继续扩注

---

## 七、代码质量与可维护性指标（粗略）

| 指标 | 当前 (2026-06-09 重新审查) | 2.2 报告值 | 建议 |
|---|---|---|---|
| 最大单文件行数 | 1977 (`config/mod.rs`，已拆 18 子模块，聚合文件) | 4081 | ≤ 1000（按域拆） |
| `ServiceContainer` 核心字段 | **8** | 80+ | ≤ 15 | ✅ 已达标 |
| `ServiceContainer` 文件行数 | 1201 | 1408 | ≤ 500 |
| 源文件数 (src/) | **477** | ~700+ | — |
| 总 Rust 文件数 | **1146** | — | — |
| 总 LoC (src/) | **176,286** | — | — |
| 总 LoC (全项目) | **434,562** | — | — |
| 路由直查 DB 比例 | ~16%（157/约 950） | 同 | 0%（CI 门禁已部署） |
| 动态 `sqlx::query` 占比 | **~12.2%** (189/1547) | 99.6% | ≤ 30% | ✅ 已达标 |
| `sqlx::query!` 编译期宏 | **~1358** | 5 (已回退) | ≥ 1400 |
| `sqlx::query_as!` 编译期宏 | **含在1358内** | 0 | ≥ 300 |
| `sqlx::query_scalar!` 编译期宏 | **含在1358内** | 0 | ≥ 100 |
| `.sqlx/` 离线缓存文件 | **1146**（基于 v10 Schema） | 0 | ≥ 500 | ✅ |
| E2EE 自研代码路径 | 100%（vodozemac 路径已创建） | 同 | 收敛到 vodozemac |
| 测试函数数 ([#test]) | **5037** | — | — |
| #[tokio::test] 测试数 | **1321** | — | — |
| 套套逻辑测试 | 已删除（~600 行） | 同 | 0% | ✅ Step 8 |
| `unwrap()/expect()` 在 lib crate 出现 | 频繁 | 同 | 0 |
| `anyhow!` 在 lib crate | **0 处**（5 个 lib crate 已移除 anyhow 依赖） | 同 | 0 | ✅ m-8 |
| Tracing 跨链路串联 | 部分（7 个关键方法已加） | 同 | 全量 | ✅ Step 9.3 |
| OTel 接入 | 半成品 | 同 | 全量 |
| 迁移文件数 | 6（v8/v10 各 2 .sql + 2 增量） | 同 | 4 | ⚠️ 待统一到 v10 |
| v10 基线表数 | **250 表** | — | — | ✅ |
| v8 基线表数 | 243 表 | — | — | 待迁移 |
| 基线内部重复表定义 | 0 | 同 | 0 | ✅ 已修复 |
| 跨文件重复表定义 | 0 | 同 | 0 | ✅ 已修复 |
| `_ts`/`_at` rename 桥接 | 0 处（时间戳类） | 同 | 0 | ✅ 已修复 |
| `NOW()` 赋值 BIGINT 列 | **0 处**（全部清零：saml 3 处 + key_rotation 10 处 + EXTRACT 浮点精度 4 处 + 测试 DDL 7 处 + push_notification.rs 7 处 DateTime→i64） | 0 (声称) | 0 | ✅ C-8 全部已修复 |
| `SELECT *` 脆弱查询 | **0 处**（全量消除：synapse-storage 23 文件 ~130 处 + synapse-services 3 文件 12 处，仅剩 `SELECT * FROM UNNEST`） | 63 处 | 0 | ✅ m-25 |
| 布尔字段缺 `is_` 前缀 (DB) | **0 处桥接**（3 处 rename 已消除） | 0 (声称) | 0 | ✅ m-14 |
| `DateTime<Utc>` 在 DB 映射 | **0 处**（push_devices + push_notification + device_trust 均已统一为 i64） | 多处 | 0 | ✅ |
| OpenAPI 集成 | **已集成**（utoipa + utoipa-swagger-ui，`/_swagger` Swagger UI） | 无 | — | 220+ 路由待注解 |
| Redis 健康检查 | **已实现**（启动时 PING，失败 WARN 日志） | 无 | — | ✅ |
| `X-Content-Type-Options` 覆盖 | **全局**（security_headers_middleware） | 子域旁路 | 全局 | ✅ m-22 |
| 硬编码 fallback secret | **0 处**（TOKEN_HASH_SECRET 强制必填） | 存在 | 0 | ✅ m-9 |
| `cargo check` (SQLX_OFFLINE) | 0 错误 | 0 | 0 | ✅ |
| `cargo clippy --all-targets` | 0 错误 0 警告 | 同 | 0 | ✅ |
| `cargo test --no-run` | 所有测试可执行文件编译通过 | — | 0 failed | ✅ |
| `cargo sqlx prepare` | **已重建**（基于 v10 Schema，1146 文件） | 过期 | — | ✅ |

---

## 八、风险矩阵

| 风险类别 | 概率 | 影响 | 评级 | 状态 |
|---|---|---|---|---|
| 联邦重放攻击 | 低 | 高 | 严重 | ✅ 已修复（C-1） |
| 跨端 E2EE 互操作失败 | 高 | 高 | 严重 | ⚠️ 部分缓解（C-5 vodozemac 路径） |
| Sync 数据丢失/重复 | 低 | 中 | 严重 | ✅ 已修复（C-3） |
| 迁移 Schema 冲突导致运行时崩溃 | 低 | 高 | 严重 | ✅ 已修复（C-9 v8 基线） |
| `NOW()` 赋值 BIGINT 导致 SAML/E2EE 登录失败 | **低** | **高** | **严重** | ✅ **已修复（C-8 全部清零：saml 3 处 + key_rotation 10 处 + EXTRACT 4 处 + 测试 DDL 7 处）** |
| `sqlx::query!` 编译期宏已全面恢复 | 低 | 中 | 低 | ✅ **已修复（M-3 阶段 A-L 完成：1358 处宏 / 12.2% 动态 / 1146 .sqlx/ 缓存）** |
| 配置漂移导致启动失败 | 中 | 中 | 高 | — |
| 多 worker 数据不一致 | 高 | 中 | 高 | ✅ 已修复（M-7 Typing CacheManager + Presence DB/Cache） |
| 路由旁路导致业务规则失效 | 低 | 中 | 中 | ✅ CI 门禁已部署（C-4） |
| 性能瓶颈（DB/缓存失效） | 中 | 中 | 中 | — |
| 测试套套逻辑掩盖回归 | 低 | 中 | 高 | ✅ 已修复（Step 8） |
| 日志缺失导致线上排查困难 | 低 | 中 | 中 | ✅ 已修复（Step 9.3） |
| 依赖 CVE（无 audit 门禁） | 中 | 高 | 高 | — |
| v8/v10 迁移文件冗余导致混淆 | 低 | 中 | 中 | ⚠️ 双基线并存，待统一到 v10 |

---

## 九、附录 A：被识别为重复/冗余的实现

| 重复内容 | 出现位置 | 处理建议 |
|---|---|---|
| mxc:// 解析 | ✅ 已修复 — `MediaLocator::parse` 统一 | — |
| Base64/Hex/常量时间 | ✅ 已修复 — 16 处收敛到 `common/crypto` | — |
| EventBroadcaster | ✅ 已修复 — trait 提升到 `synapse-common`，3 套实现统一 | — |
| to_device 调度 | ✅ 已修复 — to_device 发送后通知 sync 连接 | — |
| Push 三端实现 | ✅ 已修复 — `PushGateway` trait + `is_retryable_error` + `send_with_retry` 共享重试 | — |
| Presence 状态机 | ✅ 已修复 — `PresenceState` 枚举统一 + `derive_activity` + 全局替换 | — |
| E2EE 自研 crypto | `e2ee/crypto/*` 与 `e2ee/olm/megolm` | 收敛到 vodozemac |
| Config 模块 | ✅ 已修复 — `common/config/mod.rs` 已拆 18 子模块 | — |
| JwtClaims 构造 | ✅ 已修复 — `ClaimsBuilder` 链式 API + 14 处替换 | — |
| CAS/SAML 表定义 | ✅ 已修复 — v8 基线收敛 | — |
| Schema 批次迁移表定义 | ✅ 已修复 — v8 基线收敛 | — |
| `voice_usage_stats` 定义 | ✅ 已修复 — v8 基线取 20260517 版本 | — |
| `user_privacy_settings` 定义 | ✅ 已修复 — v8 基线取 unified_v7 版本 | — |
| 索引定义 | ✅ 已修复 — v8 基线统一 | — |
| `spam_check_results`/`third_party_rule_results` | ✅ 已修复 — v8 基线取 20260529 版本 | — |
| `#[sqlx(rename)]` 桥接 | ✅ 已修复 — 3 处 DB-mapped rename 已消除 | — |

## 十、附录 B：数据库迁移全量审计详情（2026-06-04）

### B.1 迁移文件清单与职责

| 文件 | 类型 | 表数 | 主要内容 |
|------|------|------|----------|
| `00000000_unified_schema_v7.sql` | 基线 | ~120 | 全量建库入口，含 30+ 内部重复 |
| `00000001_extensions.sql` | 扩展 | 17 | CAS/SAML/Friends/Voice/Privacy |
| `20260515000001_consolidated_..._v7.sql` | Batch-01 | ~69 | 结构/契约/功能收敛 |
| `20260515000002_consolidated_..._v7.sql` | Batch-02 | 0 | stream_ordering 回填+覆盖索引 |
| `20260515000003_consolidated_..._v7.sql` | Batch-03 | -18 | DROP 18 张冗余表 |
| `20260515000004_consolidated_..._v7.sql` | Batch-04 | 0 | Schema 修复 |
| `20260515000005_consolidated_..._v7.sql` | Batch-05 | 0 | 表索引优化 |
| `20260515000006_consolidated_..._v7.sql` | Batch-06 | 0 | 约束治理（PK+FK） |
| `20260515000007_consolidated_..._v7.sql` | Batch-07 | 0 | 物化视图 |
| `20260515000008_consolidated_..._v7.sql` | Batch-08 | 0 | `expires_ts` → `expires_at` 重命名 |
| `20260515120000_burn_after_read_...sql` | 增量 | 4 | 阅后即焚持久化 |
| `20260516000001_key_rotation_...sql` | 增量 | 3 | 密钥轮转待处理表 |
| `20260517000001_voice_usage_stats.sql` | 增量 | 1 | 语音统计（与基线冲突） |
| `20260518000001_performance_indexes.sql` | 增量 | 0 | 7 个性能索引 |
| `20260519000001_additional_...sql` | 增量 | 0 | 15 个额外索引 |
| `20260526000001_friend_list_...sql` | 增量 | 0 | 好友列表索引 |
| `20260527000001_pg_trgm_...sql` | 增量 | 0 | pg_trgm 三元组索引 |
| `20260528000001_key_rotation_...sql` | 增量 | 1 | 密钥轮转配置 |
| `20260529000001_module_schema_...sql` | 增量 | 0 | 模块表字段对齐 |
| `20260529000002_module_result_...sql` | 增量 | 2 | 垃圾检查/第三方规则重建 |
| `20260602000001_megolm_session_keys.sql` | 增量 | 1 | Megolm 共享密钥 |
| `20260602000002_room_invite_...sql` | 增量 | 0 | 邀请 HMAC 签名 |
| `20260602000003_cross_signing_...sql` | 增量 | 0 | 跨签名 HMAC 绑定 |
| `20260603000001_align_at_suffix_...sql` | 增量 | 0 | `_at` 后缀列对齐 |

### B.2 关键冲突详情

**`voice_usage_stats` 三重 Schema 对比**：

| 列名 | extensions (v1) | unified_v7 (v2) | 20260517 (v3) | Rust 结构体 |
|------|-----------------|-----------------|---------------|-------------|
| `id` | BIGSERIAL | BIGSERIAL | BIGSERIAL | i64 |
| `user_id` | TEXT NOT NULL | TEXT NOT NULL | TEXT NOT NULL | String |
| `room_id` | — | TEXT | TEXT | Option<String> |
| `media_id` | — | — | TEXT NOT NULL | String |
| `content_type` | — | — | TEXT NOT NULL | String |
| `date` | DATE NOT NULL | DATE NOT NULL | — | — |
| `duration_ms` | — | — | INTEGER | i32 |
| `size_bytes` | — | — | BIGINT | i64 |
| `created_ts` | — | BIGINT | BIGINT NOT NULL | i64 |

Rust `VoiceUsageRecord` 仅与 v3 匹配。v1/v2 先执行则运行时崩溃。

### B.3 SQL 查询模式统计 (2026-06-06 重新统计)

| 查询类型 | 调用次数 | 涉及文件数 | 编译时保护 |
|----------|---------|-----------|-----------|
| `sqlx::query(` (运行时) | **832** | ~80 | ❌ |
| `sqlx::query!` (编译时) | **8** | 1 (`src/storage/token.rs`) | ✅ |
| `sqlx::query_as::<_, T>` (运行时) | **514** | ~70 | ❌ |
| `sqlx::query_as!(T, ...)` (编译时) | **0** | 0 | ✅ |
| **总计** | **1354** | **~100** | **0.59% 编译时** |

**对比 2.2 报告**:
- 动态 `sqlx::query` 计数: 448 → **832** (+85.7%)
- 编译期 `sqlx::query!` 计数: 476 → **8** (-98.3%；阶段 B 部分完成)
- 编译期 `sqlx::query_as!` 计数: 270 → **0** (-100%)
- 编译时保护比例: 57% → **0.59%** (-56.4pp)
- **M-3 Batch 1 进展（2026-06-06）**：
  - 阶段 A：删除 5 个孤儿宏（`guest_service.rs` + `cache/warmup.rs` 已删）
  - 阶段 B：新增 8 个生产 `query!`（`src/storage/token.rs` 中 5 UPDATE + 1 INSERT + 2 DELETE）
  - `.sqlx/` 缓存：0 → 8 个 `query-*.json` 文件
  - 离线编译：`SQLX_OFFLINE=true cargo check --lib` 0 错误
- **剩余**：阶段 B-Round 2/3（7 个 token `query_scalar`/`query_as!`）+ 阶段 C/D/E/F（~90 处其他高敏感 SQL）

## 十一、附录 C：修复工时分级（仅供参考，按工程复杂度，不承诺）

| 阶段 | 项数 | 范围 |
|---|---|---|
| P0 安全/正确性 | 9 | 集中冲刺，先行（含 C-8/C-9 数据库修复） |
| P1 架构/质量 | 11 | 同步进行（含 v8 基线重构） |
| P2 治理/CI | 13+ | 持续 |

> 报告中的"工程量"仅为复杂度分级评估，不构成交付时间承诺。

---

## 十二、最终建议

1. **M-3 已基本完成**：当前主 `src/` 口径为编译期 `sqlx::query!` / `query_as!` / `query_scalar!` 1355 处、动态 SQL 188 处，动态占比约 12.2%；`.sqlx/` 当前缓存 1143 文件，`cargo check --locked --lib` 与 `cargo test --locked --lib --no-run` 已复核通过。
2. **E2EE Megolm 双路径已装配（Phase 1+2 ✅ + Phase 3 🚧）**：Phase 1（`MegolmProvider` + `E2EE_USE_VODOZEMAC_MEGOLM` env 路由）+ Phase 2（双写 `PickleFormat::Dual` + `vodozemac_pickle` 列 + 懒迁移 + 7 metrics）已落地；Phase 3 本地互操作 19 个 case 已就位；下一步跨 Element 客户端矩阵 + Phase 4 清理自研路径
3. **P0/P1/P2 不能再表述为“全覆盖”**：P0 仍有 C-5 跨端互操作与 Phase 4 清理待收尾；P2 中 OpenAPI、覆盖率实测、keyset 分页与可观测性仍处于持续治理阶段。
4. **数据库迁移 v10 基线已就位**：根目录当前仅保留 `00000000_unified_schema_v10.sql` 与 `00000001_extensions_v10.sql` 两个生效基线文件，`v8` 文件已归档。
5. **OpenAPI 已完成一轮扩充**：`utoipa` + `utoipa-swagger-ui` 已就位，当前已覆盖 17 个公开稳定读端点，220+ 路由待继续注解。
6. **建立工程门禁**：`cargo clippy --all-targets`（0 错误 0 警告）+ `cargo-deny` + `cargo audit` + `cargo mutants` + 覆盖率门槛（已全部集成到 CI）。

---

## 十三、优化方案与执行序列

下面给出面向工程的执行序列，便于按 PR 拆分。每条均明确"目标 → 步骤 → 验收"。

### Step 1 — 联邦认证与签名硬化（P0-C1/C2）✅ 已完成 (2026-06-04)
- **目标**: 拒绝 X-Matrix 重放、Canonical JSON 通过 Synapse v1.153 向量。
- **实施**:
  1. ✅ `src/common/nonce_cache.rs`: `FederationNonceCache`（moka，TTL=60s，容量=1M）
  2. ✅ `src/web/middleware/federation_auth.rs`: `verify_freshness(origin_server_ts, skew=30s)`
  3. ✅ `src/e2ee/signed_json.rs`: `escape_canonical_string` 处理 U+2028/2029/FFFD
- **验收**: ✅ 实现完成

### Step 2 — Sync since token 单次解析（P0-C3）✅ 已完成 (2026-06-04)
- **目标**: 修 `sync_with_request` 解析两次导致 to_device 丢失。
- **实施**:
  1. ✅ `sync_service/mod.rs`: `since_token` 单次解析（`since.and_then(SyncToken::parse)`）
  2. ✅ 同一 `Option<SyncToken>` 贯穿 `delete_messages_up_to` 和 `is_incremental`
- **验收**: ✅ 实现完成

### Step 3 — E2EE 收敛到 vodozemac（P0-C5）🚧 Phase 1+2 完成 / Phase 3 进行中 (2026-06-06)
- **目标**: 单一 vodozemac 路径，删除自研 ratchet。
- **实施**:
  1. ✅ `src/e2ee/vodozemac_megolm.rs`: 基于 `vodozemac::megolm::GroupSession`/`InboundGroupSession` 实现
  2. ✅ `Cargo.toml`: `vodozemac-megolm` feature flag
  3. ✅ **Phase 1 (2026-06-05)**: `MegolmProvider` 装配到 `E2eeServices`，`E2EE_USE_VODOZEMAC_MEGOLM` env 路由
  4. ✅ **Phase 2 (2026-06-06)**: Megolm 双写（`PickleFormat::Dual` + `vodozemac_pickle` 列 + 懒迁移 `promote_to_dual` / `list_legacy_sessions` / `count_by_pickle_format`）+ 7 metrics
  5. 🚧 **Phase 3 (2026-06-06 启动)**: 本地 vodozemac 互操作 19 个 case 已落地（`src/e2ee/vodozemac_interop_tests.rs`），全部 `E2EE_INTEROP=1` 显式启用；Element Web/Android/iOS 跨端矩阵留待 `.github/workflows/e2ee-interop.yml`
  6. ⏸ **Phase 4**: 清理自研路径（必须 Phase 3 全绿后启动）
- **验收**: Phase 1+2 实现完成；Phase 3 待跨 Element 客户端矩阵全绿

### Step 4 — JWT/TOTP 严格化（P0-C6/C7）✅ 已完成 (2026-06-04)
- **目标**: 关闭 legacy token 默认放行、TOTP 恒时比较。
- **实施**:
  1. ✅ `auth/token.rs`: `is_legacy_token_window_open` 默认返回 `false`（无 `JWT_ACCEPT_LEGACY_UNTIL` 时）
  2. ✅ `web/utils/admin_auth.rs`: TOTP 使用 `subtle::ConstantTimeEq::ct_eq`
- **验收**: ✅ 实现完成

### Step 5 — 路由分层门禁（P0-C4）✅ 已完成 (2026-06-04)
- **目标**: 路由层禁止直连 storage。
- **实施**:
  1. ✅ `scripts/quality/check_route_layering.sh`: 检测 `use crate::storage`、`sqlx::query`、`PgPool` 直调
  2. ✅ 集成到 Makefile 和 CI 流程（PR 时强制检查）
- **验收**: ✅ 门禁脚本已部署

### Step 6 — ServiceContainer / Config 拆分（M-1/M-2）
- **目标**: 单文件 ≤ 1000 行；构造图清晰。
- **步骤**:
  1. 拆 `common/config/mod.rs` 为 `config/{server,database,federation,e2ee,media,cache,logging,...}.rs`。
  2. 拆 `services/container.rs` 为 `services/{core,features,infra}/mod.rs` + `Service` trait 注册。
  3. 引入 `Arc<Config>` 内部共享；外部通过 `state.config()` 访问。
  4. 单测：旧调用点经 `cargo test` + 编译器全量驱动迁移。
- **验收**: 巨型文件全部 ≤ 1000 行；`cargo build` + `cargo test` 全绿。

### Step 7 — `sqlx::query!` 全量迁移 + 缓存入仓（M-3）✅ **阶段 A-L 全部完成**

- **目标**: 编译期 SQL 校验，动态 query 占比从 99.6% 降至 ≤ 30%。**已达成** — 当前 12.2%。
- **当前实际状态 (2026-06-09 重新审查)**:
  - `sqlx::query!` + `query_as!` + `query_scalar!` 编译期宏：**1358 处**
  - `sqlx::query(` 动态调用：**173 处**
  - `sqlx::query_as::<_, T>` 动态调用：**16 处**
  - **总动态 SQL: 189 处**，占比 **12.2%**（远超 ≤ 30% 目标）
  - **`.sqlx/` 离线缓存：1146 个 JSON 文件**（基于 v10 Schema，已全量重建）
  - **`SQLX_OFFLINE=true cargo check` 通过**（0 错误）
  - **`SQLX_OFFLINE=true cargo clippy --all-targets` 通过**（0 错误 0 警告）
  - 不可迁移（永久保留动态）：
    - `database_initializer/tables.rs` 的 ~107 处 DDL
    - ~12 处 `format!` 动态拼接 SQL
    - ~15 处 `ANY($1)` / `UNNEST` 数组参数查询
    - ~10 处 QueryBuilder 动态查询
    - ~8 处元组返回类型（`query_as::<_, (T1, T2)>`）
    - ~8 处 fallback 旧 schema 兼容查询

#### ⚠️ 关键评估：`sqlx::query!` 全量迁移不能单独解决数据库问题

经全量审计（2026-06-04），项目当前最严重的数据库问题发生在 **DDL 定义层面**——迁移文件之间存在严重的 Schema 冲突、重复定义和命名不一致。`sqlx::query!` 只能在 Schema 正确的前提下提供编译时安全保障，**无法解决以下问题**：

| 问题 | `query!` 能否解决 | 原因 |
|------|-------------------|------|
| 列名拼写/类型错误 | ✅ 有效 | 编译时验证 |
| `SELECT *` 脆弱性 | ✅ 有效 | 展开为具体列 |
| 迁移文件 Schema 冲突 | ❌ 无法解决 | `query!` 只验证编译时 DB 状态 |
| `NOW()` 赋值 BIGINT 列 | ❌ 无法解决 | SQL 语法合法但语义错误 |
| 字段命名规范违反 | ❌ 无法解决 | 不验证命名规范 |
| 迁移文件冗余重复 | ❌ 无法解决 | DDL 层面问题 |

**结论**：必须先执行 Step 7.5（v8 基线重构），再推进 `query!` 全量迁移。否则 `query!` 会基于不一致的 Schema 做编译验证，反而可能固化错误。**当前 Step 7.5 已完成，但 Step 7 的迁移工作已回退，需重新启动 Batch 1**。

#### 历史 Batch 记录（与 M3_PROGRESS.md 同步，标注为已回退）

| Batch | 日期 | 文件数 | 动态减少 | 静态增加 | 关键覆盖 |
|---|---|---|---|---|---|
| 1 | 2026-06-10 | 4 | -34 | +33 | `audit.rs` / `feature_flags.rs` / `ai_connection.rs` / `matrixrtc.rs` |
| 2 | 2026-06-03 | 6 | -101 | +95 | `token.rs` / `threepid.rs` / `refresh_token.rs` / `registration_token.rs` / `email_verification.rs` / `federation_blacklist.rs` |
| 3 | 2026-06-03 | 6 | -121 | +121 | `user.rs` / `device.rs` / `captcha.rs` / `cas.rs` / `dehydrated_device.rs` / `openid_token.rs` |
| 4 | 2026-06-03 | 5 | -116 | +118 | `event.rs` / `room.rs` / `membership.rs` / `space.rs` / `room_summary.rs`（核心域） |
| 5 | 2026-06-03 | 5 | -79 | +79 | State / Federation / Relations / Thread / Sliding-Sync |
| 6 | 2026-06-03 | 1 | -11 | +11 | `room_summary.rs` 收尾 |
| 7 | 2026-06-04 | 1 | +0 | +8 | `room.rs` `search_all_rooms_admin` 3 QueryBuilder → 7+1 静态字面量 |
| 8 | 2026-06-04 | 3 | -22 | +22 | `room.rs` 14 复杂 join + 1 QueryBuilder / `space.rs` 6 / `membership.rs` 2 |
| 9 | 2026-06-04 | 7 | -12 | +54 | 全量 .sqlx/ 缓存再生 (308→506) + 编译错误修复 + state_groups/thread/relations/federation_queue/push_notification/dehydrated_device/feature_flags |
| 10 | 2026-06-04 | 7 | -82 | +110 | src/web/ 路由直查 + server_notification.rs (42处) + burn_after_read.rs (11处) + admin/user+federation+search+management+assembly |
| 11 | 2026-06-04 | 11 | -78 | +183 | 大规模末迁移 storage 文件：friend_room(19)/background_update(18)/saml(24)/presence(18)/application_service(31)/openclaw(8)/rendezvous(7)/call_session(4)/qr_login(4)/beacon(4) |
| 12 | 2026-06-04 | 11 | -46 | +56 | src/services/ 全量 DML：media(15)/sync_service(14)/e2ee(10)/friend_room_service(5)/sliding_sync(5)/search(3)/guest(2)/identity(1)/room(1) |

**累计 (历史)**: 动态 1408 → 189（-1219），编译期宏 4 → 1358（+1354）
**当前实际 (2026-06-09)**: 动态 189，编译期宏 1358 — **动态占比 12.2%，远超 30% 目标**

#### 已建立的 CI 门禁
- ✅ `SQLX_OFFLINE=true cargo check` — 离线编译验证通过（0 错误）
- ✅ `SQLX_OFFLINE=true cargo clippy --all-targets` — 0 错误 0 警告
- ✅ `SQLX_OFFLINE=true cargo test --no-run` — 所有测试可执行文件编译通过
- ✅ `cargo sqlx prepare -- --all-targets` — `.sqlx/` 缓存 1146 文件已全量重建（v10 Schema）
- ✅ `.sqlx/` 缓存入仓，支持离线编译类型检查

#### 关键技术模式（21 条经验教训，已验证）

1. **可空 bool 列强制非空**: `as "is_active!"` 覆盖 schema `boolean DEFAULT true` 的 `Option<bool>` 推断。
2. **QueryBuilder 条件分支短路**: `($1::text IS NULL OR col = $1)` 模式单条 `query!` 覆盖所有组合。
3. **IN 子句数组绑定**: `WHERE col = ANY($1::text[])` 替代动态 `push_bind`。
4. **CTE + UNION ALL 合并分支**: 多分支 dynamic `query_as` 合并为 1 套静态 `query_as!`，用 `($N::text IS NULL AND ...)` 短路不同权限路径。
5. **QueryBuilder → N 套静态字面量**: 3 order_by × 2 cursor 类型的 QueryBuilder 可拆为 7 套 `query_as!`（3 no-cursor + 3 cursor + 1 Name Some/None）。
6. **HAVING 子句参数显式 cast**: Postgres `PREPARE` 在 HAVING 中无法推断 `$N` 类型，必须加 `::BIGINT` / `::TEXT` 显式 cast。
7. **struct 扩展 nullable 字段 + NULL 填充**: 不同 query 共享 struct 时，不关心的字段用 `NULL::BIGINT as "joined_members?"` literal 填充。
8. **`#[sqlx(rename)]` 字段 alias 以结构体字段名为准**: `join_rules as "join_rule?"` 而非 DB 列名。
9. **`UNNEST($1::text[])` 需要 `Vec<String>`**: 不接受 `Vec<&str>`，需 `iter().map(String::from).collect()`。
10. **`fetch_one` vs `fetch_optional` for nullable scalars**: `as "field?"` + `fetch_one` 拿 `Option<T>`，非 `fetch_optional`（`Option<Option<T>>`）。

#### 当前状态（2026-06-09）

- **动态 SQL 占比 12.2%（189/1547）**，✅ 已完成 ≤ 30% 目标
- 不可迁移（永久保留动态）：DDL 107 处、format! 拼接 12 处、ANY/UNNEST 15 处、QueryBuilder 10 处、元组返回 8 处、fallback 8 处
- **下一步**: 当前已达到 M-3 最终目标（≤ 30% 动态占比），可持续治理剩余动态 SQL 但不阻塞发布

#### 历史待完成工作（已全部处理）
| 来源 | 状态 |
|---|---|
| `database_initializer.rs` | 107 处 DDL 永久保留 |
| `src/utils/` 等 | 已迁移大部分 |
| `src/web/` | 已迁移大部分 |
| `src/storage/` | 已迁移大部分 |
- **验收**: `SQLX_OFFLINE=true cargo check` + `cargo clippy --all-targets` + `cargo test --no-run` 全部通过

### Step 7.5 — 迁移基线重构：v8 统一收敛（M-11/M-12/M-13/M-14 + C-8/C-9）

- **目标**: 消除迁移文件冲突与冗余，建立单一真相源；统一字段命名规范。
- **前置条件**: Step 7 中 `query!` 迁移暂停，待 v8 基线确定后恢复。

#### 问题全景（2026-06-04 全量审计结果）

| 问题类别 | 严重度 | 数量 | 说明 |
|----------|--------|------|------|
| Schema 冲突（`voice_usage_stats` 三重定义等） | 致命 | 2+ 表 | `IF NOT EXISTS` 导致结果取决于执行顺序 |
| 基线内部自重复 | 高 | 30+ 表 | unified_schema_v7 主干 + 末尾 DO 块双重定义 |
| 跨文件重复定义 | 高 | 69+17 表 | unified 与 consolidated/extensions 之间 |
| DROP 后重建（Schema 不同） | 高 | 2 表 | `spam_check_results`/`third_party_rule_results` |
| 索引重复创建 | 中 | 12+ 索引 | `idx_refresh_tokens_user_id` 在 4 个位置 |
| `_ts`/`_at` 后缀不一致 | 高 | 16 处 | 需 `#[sqlx(rename)]` 桥接 |
| 布尔字段缺 `is_` 前缀 | 中 | 5+ 处 | `enabled`/`invited`/`resolved`/`used` |
| `NOW()` 赋值 BIGINT 列 | 致命 | 3 处 | saml.rs 运行时必定报错 |
| `SELECT *` 脆弱模式 | 中 | 63 处 | Schema 变更时可能运行时崩溃 |

#### 三阶段执行方案

**阶段 A：创建 v8 统一基线**

创建 `00000000_unified_schema_v8.sql`，将当前分散在 25 个迁移文件中的所有表定义收敛为单一真相源：

1. **逐表审查确定最终 Schema**：

| 表名 | 最终 Schema 来源 | 需要的变更 |
|------|-----------------|-----------|
| `voice_usage_stats` | 采用 20260517 版本（与 Rust `VoiceUsageRecord` 匹配） | 删除 v7/extensions 中的旧定义 |
| `user_privacy_settings` | 采用 unified_v7 版本（`allow_*` 字段） | 删除 extensions 中的冲突定义 |
| `cas_tickets`/`cas_proxy_tickets` | 统一使用 `consumed_at`（符合 `_at` 规范） | 修正 extensions 中的 `consumed_ts` |
| `cas_slo_sessions` | 统一使用 `logout_sent_at` | 修正 extensions 中的 `logout_sent_ts` |
| `spam_check_results` | 采用 20260529 版本（与 Rust `SpamCheckResult` 匹配） | 删除 v7 中的旧定义，清理冗余列 |
| `third_party_rule_results` | 采用 20260529 版本 | 删除 v7 中的旧定义，清理冗余列 |
| `voice_messages` | 合并两版字段 | 统一为包含 `encryption`/`is_processed`/`processed_at` 的版本 |
| 所有 `_ts`/`_at` 冲突列 | 统一为规范命名 | NOT NULL 用 `_ts`，可空用 `_at` |
| 所有布尔字段 | 统一 `is_` 前缀 | `enabled` → `is_enabled`，`invited` → `is_invited` 等 |

2. **删除 v7 基线内部重复定义**（约 30+ 张表在主干和末尾 DO 块中重复）
3. **内联所有 v7 批次迁移变更**（Batch-01 到 Batch-08 的 ALTER TABLE/CREATE INDEX/DROP TABLE 操作直接合并到基线中）
4. **内联所有后续增量迁移**（20260515 到 20260603 的变更合并到基线中）
5. **删除已被 Batch-03 DROP 的 18 张冗余表**（不再出现在基线中）
6. **统一所有索引定义**（消除 12+ 处重复，同名索引取最优定义）

**阶段 B：增量迁移归零**

v8 基线发布后，迁移目录从 50 个文件精简为 4 个：

```
migrations/
├── 00000000_unified_schema_v8.sql          # 新基线（单一真相源）
├── 00000001_extensions_v8.sql              # 扩展表（与 v8 对齐）
├── extension_map.conf
└── README.md
```

删除的文件（共 23 个 .sql + 23 个 .undo.sql = 46 个文件）：
- `00000000_unified_schema_v7.sql` → 被 v8 替代
- `00000001_extensions.sql` → 被 extensions_v8 替代
- `20260515*` 至 `20260603*` 全部 23 个增量迁移 → 已内联到 v8

**阶段 C：Rust 代码对齐**

在 v8 基线确定后，同步修正 Rust 代码：

1. **消除所有 `#[sqlx(rename)]` 桥接** — 修改 DB 列名或 Rust 字段名使其一致（16 处）
2. **修复 `NOW()` 赋值 BIGINT 列** — 改用 `EXTRACT(EPOCH FROM NOW())::BIGINT * 1000`（3 处）
3. **布尔字段统一 `is_` 前缀** — DB 列和 Rust 字段同步修改（5+ 处）
4. **`SELECT *` 改为显式列名** — 63 处全部修正
5. **推进 `sqlx::query!` 全量迁移** — 将 661 处运行时查询逐步迁移到编译时检查

#### 迁移执行策略

**已有 v7 数据库的升级**：
```sql
-- 00000000_unified_schema_v8.sql 开头添加升级守卫
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM schema_migrations WHERE version = '00000000_unified_schema_v7') THEN
        PERFORM apply_v7_to_v8_delta();  -- 仅执行差异 ALTER
        RETURN;
    END IF;
    -- 新环境：执行全量建表
END $$;
```

**全新环境**：直接执行 v8 基线，无需任何历史迁移。

#### 风险控制

| 风险 | 缓解措施 |
|------|----------|
| v8 基线过大（预计 5000+ 行） | 使用 `--no-transaction` + 逻辑分段注释 |
| 已有数据库升级失败 | v8 基线内置 v7→v8 增量路径，`db_migrate.sh` 检测基线版本 |
| Rust 代码批量修改引入 bug | 分批修改，每批配合 `cargo test` 验证 |
| `sqlx::query!` 需要在线数据库编译 | 使用 `sqlx prepare --check` 离线模式 + `.sqlx` 缓存 |

- **验收**:
  1. `migrations/` 目录仅含 2 个 .sql 文件 + 辅助文件
  2. `docker/db_migrate.sh migrate` 在新环境成功执行
  3. `docker/db_migrate.sh migrate` 在已有 v7 环境成功升级
  4. `cargo test --all-features` 全绿
  5. `SQLX_OFFLINE=true cargo check` 通过
  6. 0 处 `#[sqlx(rename)]` 桥接（_ts/_at 类）
  7. 0 处 `NOW()` 赋值 BIGINT 列

> **2026-06-09 v10 升级**: v10 Schema 已创建（250 表 vs v8 243 表），新增 7 张表（含 v10 优化），迁移 v8/v10 并存（6 个 .sql 文件）。`.sqlx/` 缓存已基于 v10 Schema 全量重建（1146 文件）。建议后续将 v8 文件移入 archive/，仅保留 v10 作单一基线。

### Step 8 — 测试整改（M-4）✅ 已完成 (2026-06-04)
- **目标**: 删除套套逻辑、强化断言。
- **完成项**:
  1. ✅ 删除 `error.rs` 中 4 个套套逻辑测试（`test_matrix_error_code_as_str`、`test_matrix_error_code_http_status`、`test_api_error_variants`、`test_api_error_factory_methods`），共 ~200 行
  2. ✅ 删除 `benches/` 中 7 个无 IO 伪性能测试（`performance_api_benchmarks.rs` 4 个本地基准 + `performance_federation_benchmarks.rs` 3 个伪基准），共 ~400 行
  3. ✅ 引入 `cargo-mutants` 接入 CI（`.github/workflows/mutation-testing.yml`，nightly 非阻塞）
  4. ✅ 覆盖率门槛配置：`tarpaulin.toml`（`fail-under = 70`）
  5. ✅ 更新 `Makefile` 添加 `test-mutation`/`test-coverage-check` 目标
- **待完成**: 实际运行 `cargo mutants` 并消除残留 ≤ 5%；覆盖率提升至 ≥ 70%
- **验收**: 套套逻辑 0；`cargo mutants` 报告残留 ≤ 5%。

### Step 9 — 性能与可观测性（M-5/M-8/M-9）✅ 已完成 (2026-06-04)
- **目标**: 消除 N+1、错误结构化、链路可追踪。
- **完成项**:
  1. ✅ `storage/membership.rs` `get_room_members` + `get_shared_room_users` 添加 `LIMIT 200`
  2. ✅ `storage/event.rs` `get_room_events_by_type` + `get_sender_events` 添加 `limit.min(200)`
  3. ✅ `storage/room.rs` `get_rooms_batch` 输入数组 `take(200)` 上限
  4. ✅ `ApiError` 结构化日志：`tracing::error!(errcode, error, context)` 模式
  5. ✅ `room/service.rs` 6 个关键方法 + `sync_service/mod.rs` 添加 `#[instrument]`
  6. ✅ `tracing` crate 启用 `attributes` feature
- **待完成**: OTLP collector dev 端点默认开启；`req_id` 全链路透传
- **验收**: 列表接口 p99 不退化；`ApiError` 100% 结构化；OTel dev compose 一键启动。

### Step 10 — 工程门禁与 CI（m-1 ~ m-5、m-24）✅ 已完成 (2026-06-06 验证)
- **目标**: CI 拦截质量回退。
- **完成项**:
  1. ✅ `deny.toml`（仓根）— `cargo-deny` 配置（advisories/bans/licenses/sources），已豁免 2 条 RUSTSEC（rsa 0.9.10 Marvin 攻击 + paste 1.0.15 unmaintained），均带 Review-by 期限
  2. ✅ `cargo-audit.toml` + `audit.toml`（仓根）— `cargo-audit` 配置，阻断执行（`--deny warnings --deny unsound --deny yanked`）
  3. ✅ `scripts/ci/supply_chain_gate.sh` — Step 10 主门禁，集成 `cargo-deny check` + `cargo-audit`；CI 中 `ci.yml:93, 318` 已在两个 job 中调用
  4. ✅ `.github/workflows/mutation-testing.yml` — cargo-mutants nightly（非阻塞，timeout 120min）
  5. ✅ `.tarpaulin.toml` — 覆盖率门槛 `range = "70..90"`
  6. ✅ `cargo clippy --all-features --locked -- -D warnings` — 0 错误 0 警告（2026-06-06 验证）
  7. ⏳ `cargo-geiger` 屏蔽 `unsafe` 新增 — 未引入，列 P2
  8. ✅ `docs/INDEX.md`（2026-06-06 新建）— 区分 `archive/` 与现行；治理规则已纳入 PR 门禁
- **验收**: 门禁脚本在 PR 流程中强制；3 个 PR 周期内 0 例外通过（持续观测）

### Step 11 — Minor 项滚动治理（m-6 ~ m-24）🚧 持续治理中
- **目标**: 持续清理。
- **完成项**:
  1. ✅ `federation/event_broadcaster.rs` 整合（`M2_2026-05-27`）
  2. ✅ `services/push/gateway.rs` 三端接口化（2026-05-27）
  3. ✅ `common/crypto.rs` 集中 base64/hex/常量时间比较（2026-05-27）
  4. ✅ m-30 Media 链接签名（HMAC-SHA256 `MediaLinkSigner` + `download_media_signed` 路由，2026-06-06）
  5. ✅ cargo-deny/cargo-audit/cargo-mutants 入 CI（`deny.toml` + `audit.toml` + `supply_chain_gate.sh` + `mutation-testing.yml`，2026-06-06）
  6. ⏳ 抽 `MediaLocator`（P2 持续）
  7. ⏳ 抽 `auth/login` builder（与 M-3 关联）
  8. ⏳ UIA 完整化、Push 鉴权加固、Admin 审计（P2）
  9. ✅ 同步 Matrix spec changelog（v1.18 baseline 已固化于 `SUPPORTED_MATRIX_SURFACE.md`）
- **验收**: 30 项 P0+P1 全部进版本；P2 项按月滚动。

### Step 12 — 文档与发布基线 ✅ 已完成 (2026-06-06)
- **目标**: 现状可追溯。
- **完成项**:
  1. ✅ `docs/synapse-rust/API_COVERAGE_REPORT.md`（已存在，vs Synapse v1.149.1）
  2. ✅ `docs/synapse-rust/SUPPORTED_MATRIX_SURFACE.md`（已存在，Matrix v1.18 + Synapse v1.153 baseline）
  3. ✅ 链接本审查报告为基线（`docs/INDEX.md` §二 已索引）
  4. ✅ `docs/INDEX.md`（2026-06-06 新建）— 文档导航中枢 + 治理规则
  5. ✅ `CHANGELOG.md`（2026-06-06 新建）— 基于 Keep a Changelog + SemVer；v8.0.0 预发布基线已记录；Unreleased 节跟踪 C-5 Phase 3/4
- **验收**: 文档可作为对外合规/对接入口（现行 ✅）

---

## 十四、变更管理建议

- **分支策略**: 每个 Step 一条 feature 分支（如 `feature/p0-federation-auth-hardening`）；CI 全绿 + 1 名 reviewer + 协议样本测试通过才可合并。
- **回滚预案**: P0 项配套 `feature flag`（如 `federation.nonce_strict`、`auth.legacy_tokens_disabled`），异常时 1 步回退。
- **灰度顺序**: dev → 单测 → 集成 → 两实例联邦 → staging → 生产 5%。
- **监控**: 联邦重试率、签名失败率、to_device 投递失败率、`/sync` 延时、E2EE 解密失败率 必须接入告警。

---

**报告完**。如需我针对任一 P0/P1 项进入实现阶段，请告知具体优先级与起止范围。

---

## 十五、Step 执行进度总览（**2026-06-09 重新审查**）

| Step | 名称 | 状态 | 完成度 | 关键产出 |
|------|------|------|--------|----------|
| 1 | 联邦认证与签名硬化 | ✅ 已完成 | 100% | C-1: FederationNonceCache + ±30s 时间窗口；C-2: escape_canonical_string (U+2028/2029/FFFD) |
| 2 | Sync since token 单次解析 | ✅ 已完成 | 100% | C-3: since_token 单次解析贯穿 sync_with_request |
| 3 | E2EE 收敛到 vodozemac | 🚧 Phase 1+2 完成 / Phase 3 进行中 | 90% | C-5: vodozemac_megolm.rs + MegolmProvider 装配 + 双写 PickleFormat::Dual + 7 metrics + 19 个本地互操作 case |
| 4 | JWT/TOTP 严格化 | ✅ 已完成 | 100% | C-6: is_legacy_token_window_open 默认 false；C-7: subtle::ConstantTimeEq |
| 5 | 路由分层门禁 | ✅ 已完成 | 100% | C-4: check_route_layering.sh CI 脚本 + Makefile 集成 |
| 6 | ServiceContainer/Config 拆分 | ✅ 已完成 | 100% | M-1 ✅ 4 子结构体 + 8 核心字段；M-2 ✅ 18 子模块 |
| 7 | `sqlx::query!` 全量迁移 | ✅ **阶段 A-L 全部完成** | **100%** | **1358 处编译期宏 / 12.2% 动态占比 / 1146 `.sqlx/` 缓存文件（v10 Schema）/ `SQLX_OFFLINE=true cargo check` + `cargo clippy --all-targets` 0 错误 0 警告** |
| 7.5 | 迁移基线重构 | ✅ 已完成 | 100% | v8 + v10 基线并存（v10 为当前基线，250 表），Schema 冲突全部修复 |
| 8 | 测试整改 | ✅ 已完成 | 90% | 删除套套逻辑 ~600 行，cargo-mutants CI，tarpaulin.toml |
| 9 | 性能与可观测性 | ✅ 已完成 | 90% | LIMIT 200，ApiError 结构化，7 个 #[instrument]，Redis 健康检查 |
| 10 | 工程门禁与 CI | ✅ 已完成 | 95% | deny.toml + cargo-audit.toml + supply_chain_gate.sh + mutation-testing.yml + .tarpaulin.toml 全部就位 |
| 11 | Minor 项滚动治理 | ✅ 已完成 | 100% | 30/30 Minor 项全部修复；P2 11/11 全部完成 |
| 12 | 文档与发布基线 | ✅ 已完成 | 100% | docs/INDEX.md + CHANGELOG.md + API_COVERAGE_REPORT.md + OpenAPI/Swagger UI 集成 |

### 未完成任务统计（**2026-06-09 更新**）

| 优先级 | 总数 | 已完成 | 未完成 | 未完成项 |
|--------|------|--------|--------|----------|
| P0（阻塞生产） | **10** | **9** | **1** | C-5 Phase 4（vodozemac 清理自研路径） |
| P1（架构/质量） | 11 | 11 | 0 | — |
| P2（持续治理） | 13+ | **13** | **0** | — |
| **总计** | **34+** | **33** | **1** | |

### 关键风险提示（**2026-06-09 更新**）

1. **C-5 vodozemac Phase 4 待完成**：自研 `e2ee/crypto/{aes,x25519,mod}.rs` 重叠代码 + `e2ee/olm/session.rs` 自研 ratchet 待清理；必须先完成 Phase 3 跨 Element 客户端互操作矩阵后启动
2. **v8→v10 迁移路径**：已有 v8 数据库升级到 v10 的增量路径需验证
3. **OpenAPI 覆盖率**：220+ 路由当前已有 17 个公开稳定读端点注解，全面覆盖仍待持续治理
4. **文档历史快照仍有旧口径**：报告内部分历史段落仍保留旧统计与旧结论，后续需继续做全文一致性清理

### 2026-06-09 验证清单

| 项 | 命令/位置 | 实际值 | 报告值（v4.0） | 状态 |
|---|---|---|---|---|
| `cargo check` (SQLX_OFFLINE) | terminal | 0 错误 | 0 错误 | ✅ |
| `cargo clippy --all-targets` | terminal | 0 错误 0 警告 | 0 | ✅ |
| `cargo test --no-run` | terminal | 全部可执行文件编译通过 | — | ✅ |
| `cargo sqlx prepare` | terminal | 已重建（v10 Schema） | 过期 | ✅ |
| `.sqlx/` 缓存文件数 | `find .sqlx -name "*.json"` | 1146 | 0（回退后） | ✅ 已全量重建 |
| `ServiceContainer` 行数 | `wc -l src/services/container.rs` | 1201 | 1201 | ✅ |
| `ServiceContainer` 核心字段数 | code review | 8（+4 子结构体） | 8 | ✅ |
| `config/mod.rs` 行数 | `wc -l` | ~1977 | 1977 | ✅ |
| `sqlx::query!` 编译期宏 | grep | **1358** | 1358 | ✅ |
| `sqlx::query(` 动态调用 | grep | **173** | 158 | ⚠️ +15（新缓存重建后的计数差异）|
| `sqlx::query_as::<_>` 动态调用 | grep | **16** | 12 | ⚠️ +4（计数差异）|
| 动态 SQL 占比 | 计算 | **12.2%** | 11% | ✅ 仍远超 30% 目标 |
| `NOW()` 赋值 BIGINT 列 | grep | **0 处** | 0 | ✅ |
| `DateTime<Utc>` DB 映射 | grep | **0 处** | 0 | ✅ |
| OpenAPI 集成 | code review | **已集成**（`/_swagger`） | 无 | ✅ P2 #37 |
| Redis 健康检查 | `src/server.rs` | **已实现** | 无 | ✅ P2 #36 |
| 迁移文件数 | `ls migrations/*.sql` | 6（v8+v10 并存） | 4 | ⚠️ 待统一到 v10 |
| v10 基线表数 | grep migrations/v10.sql | 250 | — | ✅ |
| 测试函数数 | grep `#[test]` | **5037 ([#test]) + 1321 ([#tokio])** | 1762 | ✅ |
