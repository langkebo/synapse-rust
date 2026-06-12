# synapse-rust 全面深度技术审计报告

**版本**: 8.5.5（2026-06-12 Matrix capability 真值源收口版）
**审计基线**: `/Users/ljf/Desktop/hu_ts/synapse-rust` 当前工作区状态（含未提交改动）
**对标基线**: Matrix Spec v1.18；element-hq/synapse v1.153.x 文档与架构实践
**审计对象**:
- `docs/synapse-rust/COMPREHENSIVE_AUDIT_REPORT_2026-06-03.md`
- `docs/synapse-rust/LAYER_MIGRATION_OPTIMIZATION_PLAN_2026-06-12.md`
- `docs/synapse-rust/SUPPORTED_MATRIX_SURFACE.md`
- `docs/synapse-rust/TECHNICAL_DEBT_OPTIMIZATION_PLAN_2026-06-11.md`

---

## 一、执行摘要

本次复核结论与旧版综合审计报告相比有两个关键变化：

1. **一批旧问题已经被修复，原文档存在明显状态漂移**。
   - `application_service` 早期记录的 `processed` / `transaction_id` SQL 致命错误，当前 canonical 实现已经修复。
   - `migrations/README.md` 与 `CHANGELOG.md` 已同步到 v10，不再支持“仍引用 v8”的旧结论。
   - `route_ledger` 当前为完整实现，不再是 4 行 re-export 壳文件。

2. **当前项目仍存在会影响持续演进和后续门禁收敛的真实问题**。
   - `feature_flags` 的 `CacheManager` 类型边界已修复，`cargo check --workspace --all-features --locked` 已恢复通过。
   - `test-utils` 集成测试编译门禁已恢复通过，但 root/canonical 双轨冗余本身仍未根治。
  - 应用服务能力与上游 Synapse 相比仍存在结构性缺口：`app_service_config_files` 的 YAML 加载、本地房间事件的 namespace 自动 enqueue、自动 sender，以及基础 backoff/recoverer 与第二层失败治理已落地；但事务模型、bridge 端到端验证和更完整的调度组件仍未补齐。
   - 根 crate 与 canonical crate 的镜像模块冗余仍然显著，但 `src/services/mod.rs` 已移除 `pub use crate::storage::*`，服务层开始改为显式依赖 storage。
   - `admin_user_service` 的 canonical shim 已解除，root 侧已收口为 facade；但 canonical 实现中仍保留部分 direct SQL，root/canonical 双轨分层债仍未根治。
   - 协议面文档与代码存在漂移，尤其是 room versions 与静态 capability 声明策略。

结论：**当前 synapse-rust 不是“历史问题基本清零”的状态，而是“旧缺陷部分闭环、新的分层与门禁问题成为主矛盾”的状态。**

---

## 二、审计范围与方法

### 2.1 审计方法

本轮采用以下方式交叉验证：

- 完整阅读用户指定的 4 份文档。
- 对文档中涉及的关键代码文件逐一静态取证。
- 定向执行门禁命令验证当前工作区状态。
- 直接研读上游 Synapse 文档：`architecture.md`、`workers.md`、`application_services.md`、`replication.md`。
- 对代码冗余、配置冗余、依赖冗余、模块冗余 4 类问题做补充盘点。

### 2.2 已执行验证

- `cargo test --lib web::routes::handlers::versions::tests --no-run`：**通过**
- `cargo check --workspace --all-features --locked`：**通过**
  - 修复方式：在 [mod.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/cache/mod.rs#L702-L730) 增加 root cache 到 canonical cache 的状态转换，并在 [container.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/services/container.rs#L542-L545) 仅对 `feature_flag_storage` 构造使用该转换
- `cargo test --features test-utils --test integration --no-run --locked`：**通过**
  - 修复方式：收敛 `ai_connection` / `thread` / `burn_after_read` / `background_update` / `captcha` / `feature_flags` 等链路的 root/canonical 类型边界，并将集成测试夹具改为直接构造 canonical cache/storage 依赖
- `cargo test --features test-utils --test unit --no-run --locked`：**通过**
- `cargo check --workspace --all-features --locked`：**再次通过**
  - 验证点：`admin_user_service` 下沉部分 direct SQL 到 `UserStorage` 后，`src/services/mod.rs` 已去除 `pub use crate::storage::*`，`sync_service` / `room` / `admin_registration_service` 已改为显式依赖 storage
- `ApplicationServiceManager::load_from_config_files(...)` 启动期导入链路：**已落地**
  - 验证点：`synapse-services/src/application_service.rs` 已新增 YAML 解析、regex/URL/sender 校验与 `upsert_registration()` 幂等导入；`src/server.rs` 已在服务容器装配后消费 `config.server.app_service_config_files`
- `cargo test -p synapse-services --lib application_service::tests::test_retry_backoff_ms_grows_and_caps --locked`：**通过**
- `cargo test -p synapse-services --lib application_service::tests::test_is_transaction_ready_to_retry_respects_backoff_window --locked`：**通过**
- `cargo test -p synapse-services --lib application_service::tests::test_classify_http_failure_distinguishes_retryable_and_fatal_statuses --locked`：**通过**
- `cargo test -p synapse-services --lib application_service::tests::test_should_disable_service_uses_kind_specific_thresholds --locked`：**通过**
- `cargo check --workspace --all-features --locked`：**通过**
  - 验证点：在 appservice 联邦/旁路事件入口覆盖、建房事务提交后统一分发，以及 `openclaw`/`sync_service` 适配收尾后，工作区 check 仍可通过
- `cargo clippy --all-features --locked -- -D warnings`：**通过**
  - 修复范围：补齐 root `Cargo.toml` 对 `runtime-ddl` / `voip-tracking` / `privacy-ext` 的 feature 透传；收敛 root cache 注入 canonical `UserStorage` / `PresenceStorage` 的构造点；修正 `RendezvousMessageStorage` 的使用对象，并显式调用 `SlidingSyncStorage::delete_connection_data(...)` 与 `RoomStorage::get_user_rooms_paginated(...)`；同步修复 `openclaw` messages 分页适配、`sync_service` 的 `tracing` 宏歧义，以及 appservice 建房分发辅助结构的可见性/注入收尾
- `cargo test --features test-utils --test integration test_create_room_enqueues_appservice_events_after_commit -- --exact --nocapture`：**通过**
- `cargo test --features test-utils --test integration test_join_room_enqueues_appservice_membership_event -- --exact --nocapture`：**通过**
- `cargo test --features test-utils --test integration --no-run --locked`：**再次通过**
  - 验证点：继续收敛 `sync_service` / `presence_storage` / `api_device_presence` / `protocol_compliance` / `sliding_sync` / `to_device_sync` 等 integration 夹具中的 `PresenceStorage` 导出路径、canonical cache 注入和 `set_presence()` API 漂移后，新增 appservice 回归测试已不再被编译门禁阻塞
- `cargo test --features test-utils --test integration invite_user_enqueues_appservice_membership_event -- --nocapture`：**环境阻塞，未完成断言验证**
  - 阻塞点：本地 integration 数据库初始化在 `shared-template-schema` 阶段超时 120s，测试在进入断言前失败；当前已确认不是 appservice 逻辑或测试编译错误
- `cargo test --features test-utils --test integration upgrade_room_enqueues_tombstone_and_replacement_create_events -- --nocapture`：**环境阻塞，未完成断言验证**
  - 阻塞点：同样卡在 integration 数据库初始化超时，说明新增测试已可被目标正确发现并启动，但本地 PostgreSQL/模板 schema 准备仍需额外环境支持
- `cargo test --lib web::routes::handlers::versions::tests --locked`：**通过**
- `cargo test --features test-utils --test integration versions_and_public_capabilities_match_declared_room_version_surface -- --nocapture`：**通过**
  - 验证点：`/_matrix/client/versions` 仍保守声明到 `v1.13`，公开 `/_matrix/client/v3/capabilities` 中的 `m.room_versions` 已与 `SUPPORTED_ROOM_VERSIONS` 常量保持一致，并继续对未认证请求隐藏 `m.sso` / `io.hula.*` 私有扩展能力
- `cargo test --features test-utils --test integration federation_query_destination_returns_minimal_payload -- --nocapture`：**通过**
  - 验证点：`/_matrix/federation/v1/query/destination` 返回的 `m.change_password` 与 `m.room_versions.default` 已与 client 能力面共用同一真值来源，不再单独手写漂移值
- `cargo tree -d --workspace | head -n 120`：**确认存在重复依赖版本**
  - 已确认案例：`base64 v0.21.7` 与 `base64 v0.22.1`

### 2.3 证据边界说明

- 本报告以**当前磁盘工作区**为准，不以 Git 已提交历史为准。
- 对覆盖率、全量 test pass 数等指标，本轮未重复跑完整大门禁时，统一标注为“**待运行时复核**”，不沿用旧文档中的历史数值；`cargo clippy --all-features --locked -- -D warnings` 已在本轮重新恢复通过。

---

## 三、四份文档问题存在性验证清单

### 3.1 `COMPREHENSIVE_AUDIT_REPORT_2026-06-03.md` 复核

| 原文问题/结论 | 当前验证结论 | 复现情况 | 影响范围 | 发生场景 |
|---|---|---|---|---|
| `migrations/README.md` 仍引用 v8 | **已证伪** | 当前文件已更新为 `v10 baseline + 1 extension` | 文档一致性 | 新人阅读迁移说明 |
| `CHANGELOG.md` 基线仍为 v8.0.0 | **已证伪** | 当前文件头已更新为 `v10.0.0` | 文档一致性 | 发布/回溯版本时 |
| `application_service` 仍存在列名致命错误 | **已证伪** | `synapse-storage/src/application_service.rs` 已改为 `is_processed`，`mark_event_processed()` 不再写 `transaction_id` | Application Service 存储层 | appservice 事务完成 |
| “工程门禁基本恢复，剩余主要是 clippy/覆盖率” | **部分失真** | 当前 `cargo check --workspace --all-features --locked`、`cargo test --features test-utils --test integration --no-run --locked` 与 `cargo clippy --all-features --locked -- -D warnings` 均已恢复，但 root/canonical 分层债与协议面漂移仍是当前主问题 | 工作区门禁与分层治理 | 开启测试特性、本地 CI、回归验证 |
| 报告中的文档版本滞后问题仍是当前主问题 | **已证伪** | 相关文档已修复，但报告自身未同步 | 审计报告可信度 | 依据报告制定优先级时 |
| 覆盖率 20.11%、`cargo test --lib` 有 10 个失败 | **待运行时复核** | 本轮未重复执行全量 `cargo test --lib` 与 tarpaulin | 测试质量判断 | 需要重新建立最新基线时 |
| 当前真实遗留问题仅剩少量收尾项 | **已证伪** | 本轮新增确认 8 类当前问题，其中 2 类为 P0/P1 架构/门禁问题 | 全项目 | 架构治理、CI、协议兼容 |

### 3.2 `LAYER_MIGRATION_OPTIMIZATION_PLAN_2026-06-12.md` 复核

| 原文问题/结论 | 当前验证结论 | 复现情况 | 影响范围 | 发生场景 |
|---|---|---|---|---|
| `admin_user_service` root 侧为完整实现，canonical 侧仍是 shim | **已证伪** | 当前 `synapse-services/src/admin_user_service.rs` 已升级为真实 canonical 实现，`src/services/admin_user_service.rs` 已收口为 facade re-export | 管理员用户管理链路 | 用户列表、批量创建、停用 |
| `AdminUserListRow` / `AdminUserListItem` 字段重复 | **已证伪** | 当前已收敛为单一 `AdminUserListItem`，不再维护重复字段集 | 服务层 DTO 边界 | 用户列表分页 |
| `AdminUserDetails` 直接暴露 `User` 存储类型 | **已证伪** | 当前 `AdminUserDetails.user` 已收口为 service DTO `AdminUserProfile` | 分层隔离、序列化边界 | 管理员查询单个用户 |
| `admin_user_service` 直接 SQL 绕过 storage | **仍真实存在，但边界位置已变化** | root facade 不再承载实现；但 canonical `synapse-services/src/admin_user_service.rs` 仍保留列表、统计、用户类型/停用更新等 direct SQL | 管理后台服务层 | 用户管理接口变更、审计、测试 |
| `application_service` root 与 canonical 为双全量实现 | **已证伪** | root `src/services/application_service.rs` 和 `src/storage/application_service.rs` 都已 facade 化 | 服务/存储迁移状态判断 | 分层迁移评估 |
| `application_service` 仍存在 `processed` / `transaction_id` SQL 错误 | **已证伪** | 当前 canonical 存储实现已修复相关 SQL | Appservice 事务流 | 事件投递、事务完成 |
| 文档中的模块对数/行数统计可直接作为当前规模判断 | **部分失真** | 当前递归统计显示 `services` 同名重叠 120 个、`storage` 同名重叠 64 个，原文数字已不适合作为现状统计 | 冗余规模评估 | 制定迁移排期 |
| `src/services/mod.rs` 存在全量 storage 泄漏 | **已证伪** | `pub use crate::storage::*` 已移除；当前遗留问题转为“部分服务内部曾依赖隐式 storage 导出，现已开始显式收口” | 全部 service 使用面 | 新功能接入、跨模块引用 |

### 3.3 `SUPPORTED_MATRIX_SURFACE.md` 复核

| 原文问题/结论 | 当前验证结论 | 复现情况 | 影响范围 | 发生场景 |
|---|---|---|---|---|
| Client API versions 声明 `r0.5.0`、`r0.6.x`、`v1.1..v1.13` | **真实存在** | `CLIENT_API_VERSION_SUPPORT` 与文档一致 | 协议声明 | `/_matrix/client/versions` |
| 默认 room version 为 `10` | **真实存在** | `DEFAULT_ROOM_VERSION` 仍为 `10` | 建房行为 | 创建房间 |
| 文档称只支持 `1..11`，不声明 `12` | **已证伪** | 当前 `SUPPORTED_ROOM_VERSIONS` 明确包含 `12`、`13`，测试也断言 `resolve_room_version(Some("12")) == Some("12")` | 协议面文档、客户端兼容预期 | `/capabilities`、联邦协商 |
| federation membership 已检查 federation 维度 | **真实存在** | `src/web/routes/federation/membership.rs` 中已调用 `can_federate_room_version()` | 联邦安全与兼容性 | join/leave/invite/knock |
| Focused gate 当前不可运行 | **部分失真** | 文档里的目标测试至少可通过 `--no-run` 编译，不再被立即卡死 | 协议面验证流程 | 本地协议面回归检查 |
| `m.change_password` / `m.set_displayname` / `m.set_avatar_url` / `m.3pid_changes` 需后续从静态 true 收敛 | **仍真实存在** | 当前 capability 仍直接通过 `insert_enabled_capability(..., true)` 静态宣称 | 协议兼容面准确性 | 客户端依据 capability 判定功能时 |

### 3.4 `TECHNICAL_DEBT_OPTIMIZATION_PLAN_2026-06-11.md` 复核

| 原文问题/结论 | 当前验证结论 | 复现情况 | 影响范围 | 发生场景 |
|---|---|---|---|---|
| `route_ledger` root 侧只剩 4 行 re-export | **已证伪** | 当前 `src/web/routes/route_ledger.rs` 为完整实现，含 `RouteEntry`、`RouteLedger`、校验逻辑与说明 | 路由治理评估 | 路由账本维护 |
| `filter` 模块已收口为 thin re-export | **真实存在** | `src/storage/filter.rs` 当前仅 `pub use synapse_storage::filter::*;` | storage facade 进度 | 分层迁移 |
| `telemetry_config` 已收口为 thin re-export | **真实存在** | `src/common/telemetry_config.rs` 当前仅 `pub use synapse_common::telemetry_config::*;` | common facade 进度 | 公共配置迁移 |
| `application_service` 已完成 facade 收口 | **真实存在** | root service/storage 均已为 facade | 服务/存储迁移进度 | appservice 模块治理 |
| `feature_flags` 的 `CacheManager` 类型边界是 blocker | **已修复** | 当前已通过 root cache 到 canonical cache 的定向转换消除该阻断 | `feature_flags` 链路、全量构建门禁 | all-features 构建、container 装配 |
| `cargo check --workspace --all-features` 已恢复通过 | **当前为真** | 本轮实际命令复现通过 | 工作区编译门禁 | 发布前、合并前检查 |
| `unwrap/expect` 风险已从关键运行时路径大幅收敛 | **倾向为真** | 本轮已重跑 `cargo clippy --all-features --locked -- -D warnings` 并通过；未再发现当前门禁内的 `expect/unwrap` 阻断 | 代码质量治理 | 持续演进、clippy 门禁 |
| `tests/unit/` 中 DB 依赖测试迁移已完成 | **待运行时复核** | 本轮未重复审查全部 unit target 及其夹具依赖 | 测试分层 | 本地单测与 CI 结构治理 |

### 3.5 文档复核总判断

- **仍真实存在的核心问题**：`admin_user_service` 分层债、protocol surface 文档漂移、appservice 架构缺口、root/canonical 双轨冗余。
- **已被代码修复但文档未同步的旧问题**：`application_service` SQL 致命错误、`migrations/README.md` v8、`CHANGELOG.md` v8、`route_ledger` root re-export 说法。
- **需要运行时重新建基线的问题**：覆盖率、全量 `cargo test --lib` 失败数、全仓 `unwrap/expect` 精确分布、`tests/unit/` DB 依赖迁移完成度。

---

## 四、与上游 element-hq/synapse 的深度对标结论

### 4.1 上游 Synapse 的关键实践

根据 `architecture.md`、`workers.md`、`application_services.md`、`replication.md`，上游 Synapse 的几个关键特征是：

1. **清晰的业务边界**
   - HTTP/REST 作为边界层。
   - handlers 承担业务逻辑。
   - storage 作为统一持久化抽象。
   - notifier/distributor/replication 负责跨模块、跨进程通知。

2. **面向大规模部署的 worker 与 replication 设计**
   - 单数据库、按流复制、缓存失效广播、单写多读模型。
   - 重点不是“能起多个进程”，而是“跨进程状态一致性”。

3. **Application Service 设计更完整**
   - 通过 `app_service_config_files` 加载 YAML 注册。
   - 事件按 namespace 自动匹配与推送。
   - 具有 scheduler、transaction controller、recoverer 等完整事务链路。

4. **配置与协议声明相对保守**
   - 能力声明通常与真实实现、配置开关和集成测试一起治理。
   - 不轻易把未经验证的能力公开为稳定支持面。

### 4.2 当前项目的强项

当前 synapse-rust 并非全面落后，存在几项明显进步：

- 已建立 `route ledger` 与 manifest 验证机制，路由治理优于很多 Rust 同类实现。
- 已有 worker 子系统、TCP/HTTP replication 路径与位置同步接口。
- canonical crate 分层（`synapse-common` / `synapse-storage` / `synapse-services` / `synapse-web`）方向正确。
- `application_service` 早期 SQL 致命缺陷已经修复，说明迁移链条在推进。

### 4.3 当前项目相对上游的实质差距

| 对标维度 | 上游 Synapse | 当前 synapse-rust | 结论 |
|---|---|---|---|
| Appservice 注册 | YAML `app_service_config_files` + 运行时装载 | 已支持启动期 YAML 加载、基本校验与幂等导入；仍缺少与自动事件分发联动 | **部分补齐，主缺口转向调度链路** |
| Appservice 事件推送 | 自动 namespace 匹配 + 调度 + 失败恢复 | 已在本地房间事件、联邦 membership/transaction、以及 join/leave/invite/ban/unban/kick/upgrade/friend-room state event/部分建房事件等旁路入口接入 appservice enqueue 或提交后统一分发，并新增自动 sender、基础 backoff/recoverer 与失败分类/自动隔离坏 AS；但事务模型仍较简化，bridge 端到端验证与更完整调度组件仍未完成 | **部分补齐，仍有主链路缺口** |
| 分层隔离 | REST/handler/storage 边界清晰 | nominal 上仍是 `route -> service -> storage`；`services/mod.rs` 的全量 storage 泄漏已移除，但 root/canonical 双轨与少量 service 直连 SQL 仍削弱边界治理 | **架构短板** |
| Worker/replication | 围绕单写多读与缓存失效设计 | 已有 worker/replication 雏形，但根/canonical 双轨与编译门禁仍拖慢收敛 | **部分具备，未完全成熟** |
| 协议声明治理 | 保守、以实现/测试为依据 | room version 文档漂移，部分 capability 仍静态 `true` | **治理不足** |
| 运维配置面 | 配置项大多有明确消费路径 | `app_service_config_files` 已有启动期消费链路，自动 sender 与 recoverer 失败治理也已接入；但更完整的调度/恢复组件仍未闭环 | **部分缓解** |

### 4.4 对标后的总体判断

当前项目最需要向上游 Synapse 学的不是“照搬 Python 架构”，而是三件事：

1. **把能力声明、配置面和运行时代码真正闭环**。
2. **把 Application Service 从“接口集合”提升为“完整事件分发系统”**。
3. **把分层迁移从“模块存在”推进到“类型边界真的隔离”**。

---

## 五、冗余专项盘点

### 5.1 代码冗余

- 递归统计显示，`src/services` 与 `synapse-services/src` 之间存在 **120 个同名 `.rs` 文件重叠**。
- `src/storage` 与 `synapse-storage/src` 之间存在 **64 个同名 `.rs` 文件重叠**。
- 这意味着当前仍是“root 实现 + canonical 实现/壳文件”并存，而不是单一事实来源。

### 5.2 配置冗余

- `app_service_config_files` 已从“死配置面”收敛为“启动期可消费配置面”；本地房间事件、联邦 membership/transaction，以及 join/leave/invite/ban/unban/kick/upgrade/friend-room state event/部分建房事件等旁路入口会自动 enqueue 到 appservice pending queue，其中建房事务内事件会在提交成功后统一 dispatch；sender 会周期性优先重试未完成 transaction、再组批发送 pending events，并已加入基础 backoff/recoverer 以及失败分类/自动隔离坏 AS 的第二层治理，但事务模型和 bridge 端到端验证仍未闭环。
- `SUPPORTED_MATRIX_SURFACE` 中要求后续收敛的 capability，当前代码仍存在静态 `true` 声明，形成“可配置/可验证”与“硬编码公开能力”并存。

### 5.3 依赖冗余

- `cargo tree -d --workspace` 已确认至少存在一组重复依赖版本：`base64 v0.21.7` 与 `base64 v0.22.1`。
- 这类重复不会立刻造成功能错误，但会增加：
  - 构建体积
  - 编译时间
  - 安全升级与许可证审查成本

### 5.4 模块冗余

- `ServiceContainer` 同时暴露：
  - `e2ee/rooms/federation/admin/core/account/sso/extensions` 分组视图
  - 大量 legacy 扁平字段
- 这造成“新旧两套访问面”并存，属于典型的模块兼容层冗余。

---

## 六、当前完整问题清单、优化方案与量化验收标准

> 说明：以下仅保留本轮确认仍真实存在、或虽未在原文档中完整记录但已被本轮证实的当前问题。每项均给出实施步骤、责任节点、资源投入与四维验收标准。

### P0-01 `feature_flags` `CacheManager` 类型边界导致 all-features 工作区编译失败

- **当前状态**：**已修复**
- **修复结果**：`cargo check --workspace --all-features --locked` 已恢复通过。
- **修复方式**：在 root [mod.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/cache/mod.rs#L702-L730) 增加 `to_synapse_cache_manager()`，将 root cache 的本地/Redis/invalidation 状态重建为 canonical `synapse_cache::CacheManager`；在 [container.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/services/container.rs#L542-L545) 仅对 `FeatureFlagStorage::new(...)` 使用该转换。
- **影响范围**：`feature_flags` 链路、全工作区 all-features 编译门禁、CI 预检查。
- **发生场景**：发布前全量检查、启用扩展特性的本地开发、CI 合并门禁。
- **后续建议**：本次修复属于“定向兼容适配”，仍建议后续统一 root 与 canonical 的 cache 抽象边界，避免其他模块重复出现同类问题。
- **验收结果**：
  - 功能可用性：`feature_flag_storage` 构造可正常完成，服务装配恢复。
  - 性能指标：未引入额外 Redis 连接池或额外缓存广播链路，运行时行为保持与原配置一致。
  - 代码质量：`cargo check --workspace --all-features --locked` 通过；编辑文件诊断为空。
  - 资源利用率：仅为 `feature_flags` 链路构造 canonical cache 视图，不改变全局 cache 主实例数量。

### P1-09 `test-utils` 集成测试编译门禁的 root/canonical 类型边界问题

- **当前状态**：**已修复**
- **当前验证**：`cargo test --features test-utils --test integration --no-run --locked` 已恢复通过。
- **修复范围**：
  1. 生产代码侧收敛了 `ai_connection`、`thread`、`burn_after_read`、`background_update`、`typing_service`、`media_service` 及若干 service/storage 装配中的 root/canonical 类型边界。
  2. 测试夹具侧将 `feature_flags` 与 `captcha` 集成测试改为直接构造 canonical cache/storage 依赖，避免继续以 root 类型注入 canonical service/storage。
- **影响范围**：启用 `test-utils` 的集成测试目标、回归门禁、迁移后模块的测试可编译性。
- **发生场景**：运行 integration target、执行带测试特性的 CI、对外验证迁移后模块时。
- **后续建议**：本次关闭的是“编译门禁”而非“分层债”本身；后续仍应统一 root facade 与 canonical crate 的唯一事实来源，减少测试与装配层重复适配。
- **验收结果**：
  - 功能可用性：`integration` 测试目标可完成 `--no-run` 编译。
  - 性能指标：本次修复仅调整类型边界和测试夹具，不引入新的运行时链路。
  - 代码质量：相关 route/service/storage 与测试夹具已收敛到一致的 canonical 依赖注入方向。
  - 资源利用率：复用现有 canonical crate，不新增额外兼容层实例。

### P0-02 Application Service 架构与上游 Synapse 存在结构性缺口

- **当前验证**：`app_service_config_files` 已有启动期 YAML 装载、regex/URL/`sender` 基本校验与幂等导入，运行时会在服务容器装配后自动消费配置；`RoomService::create_event()` 已接入 `app_service_manager.enqueue_matching_event(...)`，本地房间消息、状态事件和 pinned state 更新会按 `room_id` / `sender` / `state_key` 做 namespace 匹配并写入 appservice pending queue；联邦 `transaction` / `membership` 路由与 `voip` / `room redaction` / `com.hula.privacy` 等路由旁路事件入口在持久化成功后会 best-effort 进入 appservice queue；`join_room()` / `leave_room()` / `invite_user()` / `ban_user()` / `unban_user()` / `kick_user()` / `upgrade_room()` / `FriendRoomService::send_state_event()` 等 service 旁路入口已经切回统一事件链路；建房链路中的 `m.room.create`、`m.room.member`、`m.room.power_levels`、`m.room.join_rules`、`m.room.history_visibility`、`m.room.guest_access`、`m.room.encryption`、`com.hula.privacy`、`initial_state`、metadata 与邀请事件会先在事务内持久化，再在 commit 成功后统一 dispatch 到 appservice queue；`room_service_tests_migrated.rs` 已新增回归测试，覆盖 `create_room()` 提交后 enqueue、`join_room()` / `invite_user()` 旁路 membership enqueue，以及 `upgrade_room()` 的 `m.room.tombstone` 与 replacement room `m.room.create` enqueue；当前 `integration --no-run` 已恢复通过，但 `invite_user` / `upgrade_room` 两条新增测试的本地运行仍受 integration 数据库初始化超时阻塞，尚未完成断言级验证；`ApplicationServiceManager::start_sender(...)` 已在 server 启动后开启周期 sender，会优先重试未完成 transaction，没有未完成 transaction 时再从 pending queue 组批回填原始 room event payload 并发送；新增 `retry_backoff_ms(...)`、`is_transaction_ready_to_retry(...)`、HTTP 失败分类与自动禁用阈值后，单 AS 在存在未完成 transaction 时已按 `retry_count` 执行基础退避重试，并会在连续失败超过阈值时写入 delivery state 且自动禁用对应 AS。与上游 Synapse 的自动事件推送、scheduler、transaction controller、recoverer 相比，当前实现已具备基础调度闭环、联邦/更广的旁路覆盖，以及第二层 recoverer 治理，但事务模型和完整调度组件仍偏首版。
- **复现步骤**：检索 `enqueue_matching_event(`、`dispatch_appservice_event(` 与 `RoomService::create_event()` 调用面，可见本地房间事件已开始自动挂接 appservice；检索 `src/web/routes/federation/transaction.rs`、`src/web/routes/federation/membership.rs`、`src/web/routes/voip.rs`、`src/web/routes/room.rs` 与 `src/web/routes/handlers/room/events.rs` 中的 `dispatch_appservice_event(` 调用，可见联邦/路由旁路入口已补齐一批 best-effort enqueue；检索 `src/services/room/membership_actions.rs`、`src/services/room/membership_moderation.rs`、`src/services/room/upgrade.rs`、`src/services/friend_room_service/mod.rs` 中对 `self.create_event(` 或 `room_service.create_event(` 的调用，以及 `src/services/room/create.rs` 中 commit 后统一 `dispatch_appservice_event(` 的循环，可见 service 旁路与建房事务后分发已落地；继续检索 `retry_backoff_ms(`、`is_transaction_ready_to_retry(`、`classify_http_failure(`、`should_disable_service(` 与 `delivery_status` / `delivery_last_error` / `delivery_disabled_reason` 等 state key，可见 recoverer 已扩展到失败分类与坏 AS 自动隔离。
- **影响范围**：桥接类 AS（IRC/Slack/Discord 等）、第三方服务集成、从 Synapse 迁移的配置兼容性。
- **发生场景**：部署 bridge、期待 namespace 事件自动推送、使用 Synapse YAML appservice 配置迁移时。
- **优化方案**：补齐 AS 配置装载、namespace 匹配、排队发送、事务重试恢复全链路；以 canonical service/storage 为主线实现，root 侧仅保留兼容 facade。
- **实施步骤**：
  1. 已完成：落地 `app_service_config_files` YAML 加载与校验，并通过 `upsert_registration()` 同步写入统一存储。
  2. 已完成首版：在 `RoomService::create_event()` 本地房间事件链路中引入 namespace 匹配与 enqueue。
  3. 已完成首版：实现周期 sender，优先重试未完成 transaction，再从 pending queue 组批发送。
  4. 已完成首版：补齐基础 backoff/recoverer，失败 transaction 会更新时间基线，sender 会按 `retry_count` 退避重试最早未完成 transaction。
  5. 已完成增强：补齐联邦 `transaction` / `membership` 与部分旁路事件入口的 best-effort enqueue，并为 recoverer 增加失败分类、delivery state 与坏 AS 自动禁用阈值。
  6. 已完成增强：补充 `create_room()` 提交后 enqueue、`join_room()` / `invite_user()` membership enqueue，以及 `upgrade_room()` 的 tombstone / replacement room create enqueue 回归测试；同时继续收口 integration 夹具中的 `PresenceStorage` / canonical cache 漂移，确保新增测试已能纳入编译门禁。
  7. 待完成：补齐更完整的 per-AS 调度治理、bridge 端到端集成测试与失败重试验证，并在具备可用 PostgreSQL/integration 模板 schema 环境后完成 `invite_user` / `upgrade_room` 两条新增回归测试的断言级运行验证；同时继续清点少量仍保留为显式直写+手动 dispatch 的入口是否需要进一步统一。
- **责任节点**：协议兼容负责人、应用服务负责人、测试负责人、运维负责人。
- **资源投入**：后端 2~3 人周，QA 1 人周，SRE 0.5 人周。
- **验收标准**：
  - 功能可用性：当前已满足“AS YAML 配置可加载并在启动期导入”、“本地房间事件、联邦入口与多数已识别旁路事件可按 namespace 自动进入 pending queue”、“建房事务内事件可在 commit 成功后统一进入 appservice 分发链路”、“周期 sender 可自动尝试发送/重试未完成 transaction”，以及“单 AS 未完成 transaction 可按 `retry_count` 做基础 backoff 重试，并在连续失败超阈值时自动隔离坏 AS”；后续仍需补齐 bridge 端到端验证和少量剩余入口核查。
  - 性能指标：单 AS 1000 events/min 压测下，入队到首次发送 p95 < 200ms，1 万事件积压在 5 分钟内消化完毕。
  - 代码质量：新增 route/service/storage/integration 四层测试；关键调度路径具备失败场景测试。
  - 资源利用率：AS 调度器 CPU 常态占用 < 1 核；队列积压时内存增长可控，恢复后 10 分钟内回落到基线 ±15%。

### P1-03 root/canonical 双轨镜像仍是主债务，`services/mod.rs` 全量 storage 泄漏已完成第一轮收口

- **当前验证**：`src/services` 与 `synapse-services/src` 存在 120 个同名文件重叠；`src/storage` 与 `synapse-storage/src` 存在 64 个同名文件重叠；`src/services/mod.rs` 仍保留 `#![allow(ambiguous_glob_reexports)]`，但 `pub use crate::storage::*` 已移除，`sync_service`、`room`、`admin_registration_service` 已开始显式从 `crate::storage` 引用依赖。
- **复现步骤**：执行文件重叠统计脚本；查看 `src/services/mod.rs`。
- **影响范围**：分层治理、编译速度、IDE 索引、API 边界稳定性、代码评审成本。
- **发生场景**：迁移 facade、修改公共类型、排查编译错误、查找唯一实现来源时。
- **优化方案**：建立“单一事实来源”治理账本，把 root 明确定位为 facade/兼容层；禁止重新引入 storage glob re-export；按模块批次减少双实现文件与隐式依赖。
- **实施步骤**：
  1. 固化“禁止恢复 `pub use crate::storage::*`”规则，并增加 lint/grep 门禁。
  2. 为重叠文件建立 ledger：thin facade、真实实现、待迁移、冻结四类。
  3. 继续把剩余服务内部对 storage 的隐式依赖改为显式依赖。
  4. 增加 CI 脚本统计重叠文件数与非 facade 文件数。
- **责任节点**：架构负责人、各域模块 owner、CI 负责人。
- **资源投入**：后端 2 人周，CI/工具 0.5 人周。
- **验收标准**：
  - 功能可用性：迁移后原有 API/route 行为无回归。
  - 性能指标：`cargo check` 增量构建时间较当前下降 15% 以上。
  - 代码质量：`src/services/mod.rs` 中不再出现 `pub use crate::storage::*`，且不再新增 `crate::services::*` 间接消费 storage 的调用；非 facade 型重叠文件数在一轮迭代内下降 50%。
  - 资源利用率：IDE 索引时间与构建缓存体积较当前下降 10% 以上。

### P1-04 `admin_user_service` 已完成 canonical 化，但 direct SQL 与分层收口仍未完成

- **当前验证**：`synapse-services/src/admin_user_service.rs` 已升级为真实 canonical 实现，`src/services/admin_user_service.rs` 已收口为 facade re-export；但 canonical 实现内仍保留 `list_users_v2`、`get_user_stats`、`get_single_user_stats`、批量停用/用户类型更新等 direct SQL，尚未全量下沉到 `synapse-storage`。
- **复现步骤**：查看 `synapse-services/src/admin_user_service.rs` 中相关结构体与 SQL 调用点。
- **影响范围**：管理员用户管理接口、服务层测试、后续 DTO/字段变更。
- **发生场景**：批量创建/停用用户、用户列表分页、管理员查询单用户详情与统计。
- **优化方案**：在已完成 canonical 化的基础上，继续把 direct SQL 下沉到 `synapse-storage`，并把 admin user 的 DTO/装配边界固定下来。
- **实施步骤**：
  1. 为 admin user API 增加回归测试，固定响应结构与分页行为。
  2. 继续把用户列表、统计、停用/用户类型更新等 SQL 下沉到 `synapse-storage`。
  3. 固化 root facade 与 canonical service 的公开 DTO、构造参数与依赖边界。
  4. 将 admin user 的 storage/service 责任矩阵纳入双轨迁移 ledger。
- **责任节点**：管理后台负责人、存储层负责人、测试负责人。
- **资源投入**：后端 1.5 人周，QA 0.5 人周。
- **验收标准**：
  - 功能可用性：管理员用户 CRUD、分页、批量操作接口行为与当前兼容。
  - 性能指标：用户列表、用户统计接口 p95 不高于当前 10%；SQL 查询次数不增加。
  - 代码质量：root `admin_user_service.rs` 保持 facade；canonical 与 root 边界职责明确；剩余 direct SQL 继续向 `synapse-storage` 收敛。
  - 资源利用率：批量操作内存占用与连接池峰值不高于当前实现。

### P1-05 `ServiceContainer` 兼容层形成双访问面模块冗余

- **当前验证**：`ServiceContainer` 同时保留 grouped view（`core/account/sso/extensions` 等）与大量 legacy 扁平字段。
- **复现步骤**：查看 `src/services/container.rs` 的字段定义即可复现。
- **影响范围**：新代码接入路径、调用风格统一、后续容器收敛成本。
- **发生场景**：新增路由/服务注入、重构服务调用、从 root 迁移到 canonical 访问面时。
- **优化方案**：明确 grouped view 为正式访问面，legacy 扁平字段进入受控退役期；按路由域逐批迁移。
- **实施步骤**：
  1. 统计仍依赖 legacy 扁平字段的调用点。
  2. 新代码一律使用 grouped view。
  3. 对稳定域先完成迁移，再批量删除 legacy 字段。
  4. 增加编译检查或 grep 门禁，防止新增扁平访问。
- **责任节点**：架构负责人、web 路由负责人、服务层负责人。
- **资源投入**：后端 1~2 人周。
- **验收标准**：
  - 功能可用性：容器迁移后所有现有路由可启动、可回归。
  - 性能指标：服务装配时间与启动耗时不高于当前。
  - 代码质量：legacy 扁平字段消费面在两轮迭代内下降 80%；新增代码零扁平字段引用。
  - 资源利用率：不引入重复服务实例；启动后对象数量与内存占用保持稳定。

### P1-06 Matrix surface 文档与能力声明治理仍不闭环

- **当前验证**：`SUPPORTED_MATRIX_SURFACE.md` 的 room version 矩阵已修正为与代码一致的 `1..13`，并补入当前证据来源；`tests/integration/api_auth_routes_tests.rs` 已新增 `/versions` 与公开 `/capabilities` 的合约测试，校验 room version 常量、默认版本与未认证公开能力边界；`m.change_password` / `m.set_displayname` / `m.set_avatar_url` / `m.3pid_changes` 已从裸常量插入收敛为 `versions.rs` 中集中具名真值函数，`/_matrix/federation/v1/query/destination` 已复用同一 `m.change_password` 真值来源，`src/web/api_doc.rs` 的 capability 示例也已同步到当前 room version 矩阵；但 capability 仍未完全收敛到“配置/路由存在性/实现证据”三层驱动。
- **复现步骤**：查看 `src/common/room_versions.rs` 与 `src/web/routes/handlers/versions.rs`。
- **影响范围**：客户端兼容预期、联邦行为说明、协议声明可信度。
- **发生场景**：客户端依据 `/versions` 与 `/capabilities` 判定支持面、对外对标 Synapse 时。
- **优化方案**：把协议声明治理收敛到“代码常量 + route ledger + contract test + 文档生成”闭环。
- **实施步骤**：
  1. 已完成：修正文档中的 room version 支持矩阵，并补入 `SUPPORTED_ROOM_VERSIONS` / `client_room_versions_capability()` / integration contract test 的证据链。
  2. 已完成首轮：对一组稳定 capability 做集中具名真值收口，并让 federation discovery 与 API 文档示例复用同一声明事实。
  3. 已完成首版：增加 `/versions` 与 `/capabilities` 合约测试，固定 room version surface 与未认证公开能力边界。
  4. 待完成：将支持面文档进一步收敛为由代码常量或验证脚本生成，并继续把剩余 capability 分类到“配置控制 / 路由存在性 / 静态稳定”三类。
- **责任节点**：协议兼容负责人、文档负责人、测试负责人。
- **资源投入**：后端 1 人周，QA 0.5 人周。
- **验收标准**：
  - 功能可用性：`/versions`、`/capabilities`、federation room version 响应互相一致。
  - 性能指标：协议面接口 p95 不高于当前；不因动态生成能力而增加明显 CPU/分配开销。
  - 代码质量：新增 contract test 覆盖 room versions 与 capability matrix；文档与代码差异清零。
  - 资源利用率：协议面验证脚本总执行时间 < 2 分钟，可纳入 CI。

### P2-07 审计/技术债文档自身已成为新的漂移源

- **当前验证**：`TECHNICAL_DEBT_OPTIMIZATION_PLAN_2026-06-11.md` 中 `route_ledger` re-export 与 `workspace cargo check 已恢复通过` 两项均与当前代码不符；旧综合报告仍保留多处已失效判断。
- **复现步骤**：对照文档描述与当前代码、当前门禁命令输出即可复现。
- **影响范围**：排期优先级、研发判断、审计结论可信度。
- **发生场景**：团队依据文档安排治理顺序、评审“是否已修复”时。
- **优化方案**：建立“审计文档也要有证据基线”的治理机制；所有审计/技术债文档必须附最后验证命令、验证日期、证据路径。
- **实施步骤**：
  1. 为审计/技术债文档增加“最后验证时间”和“证据来源”字段。
  2. PR 模板增加“文档状态同步”检查项。
  3. 每次 release 前执行一次文档 spot-check。
  4. 把历史文档分为 archive 与 current 两类，避免旧文档继续被误用为现状。
- **责任节点**：文档负责人、架构负责人、发布负责人。
- **资源投入**：文档 0.5 人周，发布流程 0.5 人周。
- **验收标准**：
  - 功能可用性：团队能基于 current 文档直接完成一次排期评审而不需二次纠偏。
  - 性能指标：文档 spot-check 不超过每次发布前 0.5 人日。
  - 代码质量：current 文档中的关键结论抽样 100% 可被命令或代码定位证实。
  - 资源利用率：归档后 current 文档数量下降，阅读成本明显下降；发布前文档审查工时可控。

### P2-08 重复依赖版本需要专项清理，避免长期积累成供应链负担

- **当前验证**：`cargo tree -d --workspace` 已确认至少存在 `base64 v0.21.7` 与 `base64 v0.22.1` 的重复版本。
- **复现步骤**：执行 `cargo tree -d --workspace`。
- **影响范围**：构建体积、编译时间、安全升级、许可证治理。
- **发生场景**：依赖升级、供应链审计、二进制体积优化时。
- **优化方案**：建立重复依赖白名单/整改清单，优先清理低风险重复版本，无法统一时记录接受理由。
- **实施步骤**：
  1. 生成完整重复依赖清单并按可升级性分级。
  2. 优先处理叶子依赖与小版本差异。
  3. 对无法统一的依赖建立白名单与解释。
  4. 将 `cargo tree -d --workspace` 纳入定期供应链检查。
- **责任节点**：平台负责人、依赖治理负责人、安全负责人。
- **资源投入**：后端/平台 1 人周。
- **验收标准**：
  - 功能可用性：依赖清理不引入行为回归。
  - 性能指标：`cargo build` 或 `cargo check` 全量时间下降 5% 以上。
  - 代码质量：重复依赖清单中可统一项完成率 > 80%，剩余项有白名单说明。
  - 资源利用率：构建缓存体积或产物体积较当前下降 3% 以上。

---

## 七、实施路线图

| 阶段 | 时间 | 目标 | 对应问题 | 负责人节点 |
|---|---|---|---|---|
| Phase A | 已完成 | 恢复 all-features 编译门禁，关闭 `feature_flags` blocker | P0-01 | 架构 + 平台 + CI |
| Phase B | 已完成 | 恢复 `test-utils` 集成测试编译门禁，关闭当前 P1-09 阻断项 | P1-09 | 架构 + 测试 + 模块 owner |
| Phase C | 2~3 周 | 补齐 appservice 配置装载与自动推送方案设计/首版实现 | P0-02 | 协议兼容 + appservice |
| Phase D | 2 周 | 清理 `admin_user_service` 边界债，收口 `services/mod.rs` 分层泄漏 | P1-03 / P1-04 | 服务层 + 存储层 |
| Phase E | 2 周 | 收敛 `ServiceContainer` 双访问面，修复 Matrix surface 文档漂移 | P1-05 / P1-06 | 架构 + Web + 文档 |
| Phase F | 持续 | 文档治理与重复依赖治理常态化 | P2-07 / P2-08 | 文档 + 平台 + 发布 |

---

## 八、当前项目状态总评

### 8.1 可确认的当前真实问题

- `cargo check --workspace --all-features --locked`、`test-utils` integration `--no-run` 与 `cargo clippy --all-features --locked -- -D warnings` 均已恢复通过。
- appservice 已从“管理接口化”推进到“本地房间事件自动分发 + 联邦/多数已识别旁路入口覆盖 + 建房事务提交后统一分发 + 自动 sender + 基础 backoff/recoverer + 失败分类/自动隔离坏 AS”，但仍未达到上游 Synapse 的完整事件分发系统能力。
- root/canonical 双轨冗余仍然显著。
- 管理员用户服务的分层隔离仍未收口。
- 协议面文档与真实代码状态存在漂移。
- 技术债文档自身已出现反向失真。

### 8.2 可确认已修复、无需继续当作当前问题的旧项

- `application_service` 早期 SQL 列名错误。
- `migrations/README.md` 仍引用 v8。
- `CHANGELOG.md` 仍引用 v8.0.0。
- `route_ledger` root 仅为 4 行 re-export 的结论。
- `feature_flags` 的 `CacheManager` 类型边界阻断 all-features 编译。
- `test-utils` 集成测试编译门禁仍被 root/canonical 类型边界阻断。

### 8.3 本轮未做最终定论、需运行时再复核的项

- 当前真实覆盖率与 mutation baseline。
- `cargo test --lib` 当前失败测试数。
- `tests/unit/` DB 依赖迁移是否已完全完成。
- 全仓生产代码 `unwrap/expect` 的最新精确分布。

---

## 九、结论

当前 synapse-rust 的核心矛盾已经从“若干早期致命 SQL/协议错误”转移为：

1. **`cargo check --workspace --all-features --locked`、测试特性下的 integration `--no-run`，以及 `cargo clippy --all-features --locked -- -D warnings` 均已恢复，但 root/canonical 双轨边界仍有系统性治理空间**。
2. **appservice 已形成基础调度闭环，并补齐联邦/多数已识别旁路入口覆盖、建房事务提交后分发及第二层 recoverer 失败治理，但仍未达到上游 Synapse 的完整架构能力**。
3. **分层迁移停留在 facade 与兼容层并存阶段，代码/模块冗余仍大**。
4. **文档治理落后于代码演进，导致团队对当前真实状态的判断失真**。

因此，下一轮治理不应继续把重点放在已经修复的历史问题上，而应优先按本报告的 P0/P1 清单处理当前真实阻断项与结构性短板。

---

**报告完。**
