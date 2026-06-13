# synapse-rust 全面深度技术审计报告

**版本**: 8.6.55（2026-06-13 P1-03 facade 化 webhook_notification(3) + rtc(6)，full_impl 由 51 降至 42）
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
  - 应用服务能力与上游 Synapse 相比仍存在结构性缺口：`app_service_config_files` 的 YAML 加载、本地房间事件的 namespace 自动 enqueue、自动 sender，以及基础 backoff/recoverer 与第二层失败治理已落地；其中 fatal 连续失败自动禁用已补上 focused integration 验证，appservice 存储层已对齐 v10 schema 的 `txn_id` / `value` 双列现实，真实 `RoomService::send_message()` 的 message-path bridge e2e 已落地，`m.room.member` 的 membership bridge e2e、virtual user 必须命中本地 exclusive user namespace 的边界校验、exclusive namespace 冲突校验，以及管理面显式 `push_event` 的 namespace 所有权约束也已落地；前几轮已补入 transaction controller / per-AS 调度策略首版、每轮活跃 AS 限流、backlog 阈值识别、transaction 聚合状态机与 scheduler 状态/计数器写回；最近几轮又进一步把 scheduler 聚合状态接入 `/_synapse/admin/v1/appservices/statistics`、`/_synapse/admin/v1/telemetry/metrics` 与独立 Prometheus `/metrics` 文本出口，修复了 `ApplicationServiceStorage::get_statistics()` 过去依赖未持续维护的 `application_service_statistics` 表而导致统计空集/失真的产品缺口，并补齐了 recovery flow、mixed contention 计数关系与 Prometheus 恢复摘要的 focused 运行时证据，P0-02 主缺口已进一步收敛到阈值调优与更细的高负载策略治理。
  - 根 crate 与 canonical crate 的镜像模块冗余仍然显著，但 `src/services/mod.rs` 已移除 `pub use crate::storage::*`，服务层开始改为显式依赖 storage。
  - `admin_user_service` 的 canonical shim 已解除，root 侧已收口为 facade；canonical 实现中的 direct SQL 已清零，但 root/canonical 双轨分层债仍未根治。
  - 协议面文档与代码漂移已开始收敛，但 capability 仍需继续按“静态稳定 / 配置控制 / 路由存在性”细分治理。
  - 注册验证码链路与上游 Synapse 的可运营能力仍有缺口：`captcha_service` 的 email/sms provider 发送函数当前仍是 `todo!()` stub，启用对应能力后会直接命中运行时 panic。
  - SQLX 离线缓存门禁与当前工作区状态出现反向漂移：`bash scripts/ci/check_sqlx_offline_cache.sh` 当前失败，说明 `.sqlx/` 入仓基线、M3 文档和真实仓库状态尚未重新统一。
  - worker/replication 方向虽已具备 Redis、任务队列、若干 replication/worker 入口，但相较上游 Synapse `workers.md` 中围绕 `instance_map`、HTTP replication listener、worker 配置分工与反向代理路由的可运营模型，当前实现仍偏“通用后台 worker”，尚未形成可规模化部署闭环。

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
- `cargo test --features test-utils --test integration invite_user_enqueues_appservice_membership_event -- --nocapture`：**通过**
  - 验证点：修正 `tests/integration/mod.rs` 中 integration setup 外层超时默认值，使其不再早于 `configured_test_db_init_timeout()`；同时把 `room_service_tests_migrated.rs` 的建表夹具改为幂等后，`invite_user()` 的 appservice membership enqueue 回归测试已完成断言级验证
- `cargo test --features test-utils --test integration upgrade_room_enqueues_tombstone_and_replacement_create_events -- --nocapture`：**通过**
  - 验证点：在同一组环境收口后，`upgrade_room()` 的 tombstone / replacement room `m.room.create` enqueue 回归测试已完成断言级验证
- `python3 scripts/ci/check_root_canonical_ledger.py`：**通过**
  - 验证点：当前 `src/services` 与 `synapse-services/src` 的递归重叠文件数为 `119`，其中 root 侧初版分类为 `43 thin_facade / 76 full_impl`；`src/storage` 与 `synapse-storage/src` 的递归重叠文件数为 `58`，当前均被识别为 thin facade；同时再次确认 service 层未恢复 `pub use crate::storage::*`
- `cargo test -p synapse-services --lib beacon_service::tests::test_parse_geo_uri --features beacons -- --exact --nocapture`：**通过**
- `cargo test -p synapse-services --lib burn_after_read_service::tests::test_burn_stats_default --features burn-after-read -- --exact --nocapture`：**通过**
- `cargo test -p synapse-services --lib matrix_ai_connection_service::tests::test_create_connection_request --features openclaw-routes -- --exact --nocapture`：**通过**
  - 验证点：root 侧 `beacon_service.rs`、`burn_after_read_service.rs`、`matrix_ai_connection_service.rs` 已删除重复测试模块并收口为纯 facade；对应 canonical 单测仍可直接在 `synapse-services` 内通过。过程中顺手修复 `synapse-services/Cargo.toml` 中 `openclaw-routes` 未透传 `synapse-storage/openclaw-routes` 的既有 feature 缺口，使 `matrix_ai_connection_service` 的 canonical 单测恢复可执行
- `cargo check --locked`：**再次通过**
  - 验证点：root 侧 `application_service.rs`、`admin_audit_service.rs`、`registration_token_service.rs`、`relations_service.rs`、`server_notification_service.rs` 已继续删除重复测试模块并收口为纯 facade；由于这 5 个文件在 ledger 中本就已归类为 `thin_facade`，本轮不会改变 `services` 的 `46/73` 统计，但能继续减少 root 侧重复测试噪音与 facade 偏离
- `cargo check --locked`：**再次通过**
  - 验证点：root 侧 `admin_federation_service.rs`、`federation_blacklist_service.rs`、`media_quota_service.rs` 已删除重复测试模块并收口为纯 facade；复扫 `src/services/*.rs` 后，“顶层单文件 facade + root `#[cfg(test)]` 重复测试”这一条 lane 已基本清空
- `cargo test -p synapse-services --lib federation_blacklist_service::tests::test_check_result_serialization -- --exact --nocapture`：**通过**
- `cargo test -p synapse-services --lib media_quota_service::tests::test_user_quota_info_serialization -- --exact --nocapture`：**通过**
- `cargo check --locked`：**再次通过**
  - 验证点：`synapse-services/src/auth/mod.rs` 已正式接通 `password_policy` 模块并对外 re-export；`src/services/auth/password_policy.rs` 已从 root 整文件实现收口为 facade，root/canonical overlap ledger 统计同步从 `46 thin_facade / 73 full_impl` 变为 `47 thin_facade / 72 full_impl`
- `cargo test -p synapse-services --lib auth::password_policy::tests::test_password_validation_valid -- --exact --nocapture`：**通过**
  - 验证点：`auth/password_policy.rs` 的 canonical 模块路径已真正纳入 `synapse-services::auth` 模块树，密码策略验证测试可直接从 canonical 路径执行
- `python3 scripts/ci/check_root_canonical_ledger.py`：**通过**
  - 验证点：`src/services/database_initializer/mod.rs` 已收口为 `synapse_services::database_initializer::*` facade，root 侧原 `database_initializer/models.rs` 与 `tables.rs` 已删除，ledger 统计同步从 `47 thin_facade / 72 full_impl` 变为 `48 thin_facade / 69 full_impl`
- `cargo check --locked`：**通过**
  - 验证点：`database_initializer` 已完成模块级 canonical 收口后，root crate 与 canonical crate 仍保持编译通过；该场景因 `mod.rs/tables.rs` 依赖 `DatabaseInitService` 的 inherent impl，实际采用的是整组模块收口而非孤立单文件 facade
- `cargo test -p synapse-services --lib database_initializer::models::tests::test_initialization_report_empty -- --exact --nocapture`：**通过**
- `cargo test -p synapse-services --lib database_initializer::models::tests::test_environment_from_string_production -- --exact --nocapture`：**通过**
  - 验证点：原 root 侧 `database_initializer/models.rs` 的基础单测已迁入 canonical，`InitializationReport` 与 `Environment` 相关覆盖仍可直接从 `synapse-services` 路径执行
- `python3 scripts/ci/check_root_canonical_ledger.py`：**通过**
  - 验证点：`src/services/geo_ip/models.rs` 已收口为 `synapse_services::geo_ip::models::*` facade，ledger 统计同步从 `48 thin_facade / 69 full_impl` 变为 `49 thin_facade / 68 full_impl`
- `cargo check --locked`：**通过**
  - 验证点：`geo_ip/models.rs` 收口后，root 侧 `geo_ip/service.rs` 继续通过 `super::models::*` 消费 canonical DTO，root/canonical 编译链路未回归
- `cargo test -p synapse-services --lib geo_ip::models::tests::test_geo_ip_provider_serialization --features geo-ip -- --exact --nocapture`：**通过**
- `cargo test -p synapse-services --lib geo_ip::models::tests::test_geo_ip_config_default --features geo-ip -- --exact --nocapture`：**通过**
  - 验证点：原 root 侧 `geo_ip/models.rs` 的默认值与 serde 行为测试已迁入 canonical，`GeoIpProvider` 与 `GeoIpConfig` 相关覆盖仍可直接从 `synapse-services` 路径执行
- `python3 scripts/ci/check_root_canonical_ledger.py`：**通过**
  - 验证点：`src/services/content_scanner/models.rs` 已收口为 `synapse_services::content_scanner::models::*` facade，ledger 统计同步从 `49 thin_facade / 68 full_impl` 变为 `50 thin_facade / 67 full_impl`
- `cargo check --locked`：**通过**
  - 验证点：`content_scanner/models.rs` 收口后，root 侧 `content_scanner/service.rs` 继续通过 `super::models::*` 消费 canonical DTO，root/canonical 编译链路未回归
- `cargo test -p synapse-services --lib content_scanner::models::tests::test_scanner_type_serialization -- --exact --nocapture`：**通过**
- `cargo test -p synapse-services --lib content_scanner::models::tests::test_content_scanner_config_default -- --exact --nocapture`：**通过**
  - 验证点：原 root 侧 `content_scanner/models.rs` 的枚举 serde 与默认配置测试已迁入 canonical，`ScannerType` 与 `ContentScannerConfig` 相关覆盖仍可直接从 `synapse-services` 路径执行
- `python3 scripts/ci/check_root_canonical_ledger.py`：**通过**
  - 验证点：`src/services/identity/models.rs` 已收口为 `synapse_services::identity::models::*` facade，ledger 统计同步从 `50 thin_facade / 67 full_impl` 变为 `51 thin_facade / 66 full_impl`
- `cargo check --locked`：**通过**
  - 验证点：`identity/models.rs` 收口后，root 侧 `identity/service.rs` 与 `identity/storage.rs` 继续通过 `super::models::*` 消费 canonical DTO，root/canonical 编译链路未回归
- `cargo test -p synapse-services --lib identity::models::tests::test_third_party_id_new -- --exact --nocapture`：**通过**
- `cargo test -p synapse-services --lib identity::models::tests::test_lookup_response -- --exact --nocapture`：**通过**
  - 验证点：原 root 侧 `identity/models.rs` 的 `ThirdPartyId::new()` 与 lookup response 基础覆盖已迁入 canonical，相关 DTO 行为仍可直接从 `synapse-services` 路径执行
- `python3 scripts/ci/check_root_canonical_ledger.py`：**通过**
  - 验证点：`src/services/identity/storage.rs` 已收口为 `synapse_services::identity::storage::*` facade，ledger 统计同步从 `51 thin_facade / 66 full_impl` 变为 `52 thin_facade / 65 full_impl`
- `cargo check --locked`：**通过**
  - 验证点：`identity/storage.rs` 收口后，root 侧 `identity/service.rs` 与 `services/container.rs` 继续通过 canonical `IdentityStorage` 装配身份服务，root/canonical 编译链路未回归；同时修正了 canonical `identity/storage.rs` 对 `user_threepids.validated_at` 的列名对齐，消除了残留的 v8 `validated_ts` 漂移
- `cargo test --features test-utils --test integration --no-run --locked`：**通过**
  - 验证点：integration 目标在 `identity/storage.rs` facade 化后仍可完成整体编译，`test-utils` 特性下的 root/canonical 类型边界未新增编译回归
- `cargo test --features test-utils --test unit identity_service_tests::tests::test_identity_validate_id_server_trusted -- --exact --nocapture`：**通过**
  - 验证点：identity 相关 unit target 在本轮收口后仍可正常执行；另一次尝试运行 `threepid_storage_tests_migrated::test_add_threepid` 时暴露出现有测试夹具重复建表的既有非幂等问题，属于独立测试债，不计为本轮 `identity/storage.rs` 行为回归
- `python3 scripts/ci/check_root_canonical_ledger.py`：**通过**
  - 验证点：`src/services/identity/service.rs` 已收口为 `synapse_services::identity::service::*` facade，ledger 统计同步从 `52 thin_facade / 65 full_impl` 变为 `53 thin_facade / 64 full_impl`
- `cargo check --locked`：**通过**
  - 验证点：`identity/service.rs` 收口后，root 侧 `services/container.rs` 继续通过 canonical `IdentityService::new(...)` 装配身份服务，root/canonical 编译链路未回归
- `cargo test --features test-utils --test integration --no-run --locked`：**通过**
  - 验证点：integration 目标在 `identity/service.rs` facade 化后仍可完成整体编译，`test-utils` 特性下的 root/canonical 类型边界未新增编译回归
- `cargo test --features test-utils --test unit identity_service_tests::tests::test_identity_validate_id_server_trusted -- --exact --nocapture`：**通过**
  - 验证点：identity 相关 unit target 在 service facade 化后仍可正常执行；unit 目标中出现的 `tests/unit/common/mod.rs` dead_code warnings 为既有测试警告，未因本轮收口新增
- `python3 scripts/ci/check_root_canonical_ledger.py`：**通过**
  - 验证点：`src/services/identity/mod.rs` 已收口为 `synapse_services::identity::*` 整模块 facade，ledger 统计同步从 `53 thin_facade / 64 full_impl` 变为 `54 thin_facade / 63 full_impl`
- `cargo check --locked`：**通过**
  - 验证点：`identity/mod.rs` 收口后，root 侧 `services/mod.rs` 继续导出 `identity` 模块，`container.rs` 与外部使用面未出现模块路径断裂
- `cargo test --features test-utils --test integration --no-run --locked`：**通过**
  - 验证点：integration 目标在 `identity` 整模块 facade 化后仍可完成整体编译，`services::identity::*` 导出面未新增编译回归
- `cargo test --features test-utils --test unit identity_service_tests::tests::test_identity_validate_id_server_trusted -- --exact --nocapture`：**通过**
  - 验证点：root 侧仍可通过 `services::identity::models::*` 路径消费 canonical 导出；unit 目标中出现的 `tests/unit/common/mod.rs` dead_code warnings 仍为既有测试警告，未因本轮 `mod.rs` 收口新增
- `python3 scripts/ci/check_root_canonical_ledger.py`：**通过**
  - 验证点：`src/services/content_scanner/mod.rs` 已收口为 `synapse_services::content_scanner::*` 整模块 facade，ledger 统计同步从 `54 thin_facade / 63 full_impl` 变为 `55 thin_facade / 62 full_impl`
- `cargo check --locked`：**通过**
  - 验证点：`content_scanner/mod.rs` 收口后，root 侧 `services/mod.rs` 继续导出 `content_scanner` 模块，编译链路未出现模块路径断裂
- `cargo test --features test-utils --test integration --no-run --locked`：**通过**
  - 验证点：integration 目标在 `content_scanner` 整模块 facade 化后仍可完成整体编译，`services::content_scanner::*` 导出面未新增编译回归
- `python3 scripts/ci/check_root_canonical_ledger.py`：**通过**
  - 验证点：`src/services/geo_ip/mod.rs` 已收口为 `synapse_services::geo_ip::*` 整模块 facade，ledger 统计同步从 `55 thin_facade / 62 full_impl` 变为 `56 thin_facade / 61 full_impl`
- `cargo check --locked`：**通过**
  - 验证点：`geo_ip/mod.rs` 收口后，root 侧 `services/mod.rs` 继续导出 `geo_ip` 模块，编译链路未出现模块路径断裂
- `cargo test --features test-utils --test integration --no-run --locked`：**通过**
  - 验证点：integration 目标在 `geo_ip` 整模块 facade 化后仍可完成整体编译，`services::geo_ip::*` 导出面未新增编译回归
- `python3 scripts/ci/check_root_canonical_ledger.py`：**通过**
  - 验证点：`src/services/content_scanner/service.rs` 已收口为 `synapse_services::content_scanner::service::*` facade，ledger 统计同步从 `56 thin_facade / 61 full_impl` 变为 `57 thin_facade / 60 full_impl`
- `cargo check --locked`：**通过**
  - 验证点：`content_scanner/service.rs` 收口后，root 侧 `content_scanner/mod.rs` 继续通过 canonical `ContentScanner` 导出对外暴露模块能力，编译链路未出现路径断裂
- `cargo test --features test-utils --test integration --no-run --locked`：**通过**
  - 验证点：integration 目标在 `content_scanner/service.rs` facade 化后仍可完成整体编译，`services::content_scanner::*` 导出面未新增编译回归
- `cargo test -p synapse-services --lib content_scanner::models::tests::test_content_type_roundtrip -- --exact --nocapture`：**通过**
  - 验证点：`content_scanner` 组 canonical 测试仍可直接执行，说明本轮 service facade 化未破坏模块内 `models/service/mod` 的导出关系
- `python3 scripts/ci/check_root_canonical_ledger.py`：**通过**
  - 验证点：`src/services/geo_ip/service.rs` 已收口为 `synapse_services::geo_ip::service::*` facade，ledger 统计同步从 `57 thin_facade / 60 full_impl` 变为 `58 thin_facade / 59 full_impl`
- `cargo check --locked`：**通过**
  - 验证点：先对齐 canonical `geo_ip/service.rs` 中 `lookup_maxmind()` 的既有 `todo!()`，再收口 root facade 后，`geo_ip/mod.rs` 继续通过 canonical `GeoIpService` 对外导出，编译链路未出现路径断裂
- `cargo test --features test-utils --test integration --no-run --locked`：**通过**
  - 验证点：integration 目标在 `geo_ip/service.rs` facade 化后仍可完成整体编译，`services::geo_ip::*` 导出面未新增编译回归
- `cargo test -p synapse-services --features geo-ip --lib geo_ip::service::tests::test_lookup_maxmind_falls_back_to_default_country -- --exact --nocapture`：**通过**
- `cargo test -p synapse-services --features geo-ip --lib geo_ip::service::tests::test_lookup_disabled_returns_default_country -- --exact --nocapture`：**通过**
  - 验证点：canonical `geo_ip/service.rs` 新增的 MaxMind fallback 行为与 disabled fallback 行为测试均通过，说明本轮对齐消除了 canonical 侧残留 `todo!()`，且未破坏默认国家回退语义
- `python3 scripts/ci/check_root_canonical_ledger.py`：**通过**
  - 验证点：`src/services/push/providers/mod.rs` 已收口为 `synapse_services::push::providers::*` facade，ledger 统计同步从 `58 thin_facade / 59 full_impl` 变为 `59 thin_facade / 58 full_impl`
- `cargo check --locked`：**通过**
  - 验证点：先将 root 侧公共 helper `send_with_retry` / `is_retryable_error` 与其基础测试迁入 canonical `push/providers/mod.rs`，再收口 root facade 后，`push/service.rs` 与各 provider 子模块仍可通过同一路径消费公共导出，编译链路未出现路径断裂
- `cargo test --features test-utils --test integration --no-run --locked`：**通过**
  - 验证点：integration 目标在 `push/providers/mod.rs` facade 化后仍可完成整体编译，`services::push::*` 导出面未新增编译回归
- `cargo test -p synapse-services --lib push::providers::tests::test_is_retryable_error_http_status_codes -- --exact --nocapture`：**通过**
- `cargo test -p synapse-services --lib push::providers::tests::test_is_retryable_error_non_retryable -- --exact --nocapture`：**通过**
  - 验证点：canonical `push/providers/mod.rs` 新增的 retry helper 基础测试均通过，说明公共 helper 已成为 canonical 单一事实来源，未破坏 push provider 模块内导出关系
- `python3 scripts/ci/check_root_canonical_ledger.py`：**通过**
  - 验证点：先将 canonical `push/service.rs` 对齐到 `send_with_retry(...)` 公共发送路径，再将 `src/services/push/service.rs` 收口为 `synapse_services::push::service::*` facade，ledger 统计同步从 `59 thin_facade / 58 full_impl` 变为 `60 thin_facade / 57 full_impl`
- `cargo check --locked`：**通过**
  - 验证点：`push/service.rs` facade 化后，root 侧 `services::push::service::*` 继续经由 canonical 暴露 `PushNotificationService`、`NotificationPayload`、`SendNotificationRequest` 与 push rule 相关类型，未引入编译回归
- `cargo test --features test-utils --test integration --no-run --locked`：**通过**
  - 验证点：integration 目标在 `push/service.rs` 收口后仍可完成整体编译，`services::push::*` 导出面未出现路径断裂
- `cargo test -p synapse-services --lib push::providers::tests::test_is_retryable_error_http_status_codes -- --exact --nocapture`：**通过**
- `cargo test -p synapse-services --lib push::providers::tests::test_is_retryable_error_non_retryable -- --exact --nocapture`：**通过**
  - 验证点：本轮 `push/service.rs` 对齐复用的 retry helper 基础测试继续通过，说明 provider 重试公共路径仍保持可用，`push/service.rs` 改为 facade 后未破坏 canonical 单一事实来源
- `python3 scripts/ci/check_root_canonical_ledger.py`：**通过**
  - 验证点：先对齐 canonical `push/mod.rs` 的 `send_with_retry` re-export，再将 `src/services/push/mod.rs` 收口为 `synapse_services::push::*` facade，ledger 统计同步从 `60 thin_facade / 57 full_impl` 变为 `61 thin_facade / 56 full_impl`
- `cargo check --locked`：**通过**
  - 验证点：`push/mod.rs` facade 化后，root 侧 `services::push::*` 继续经由 canonical 暴露 gateway/provider/queue/service 全部公共导出，未引入编译回归
- `cargo test --features test-utils --test integration --no-run --locked`：**通过**
  - 验证点：integration 目标在 `push/mod.rs` 收口后仍可完成整体编译，`services::push::*` 模块边界未出现路径断裂
- `cargo test -p synapse-services --lib push::providers::tests::test_is_retryable_error_http_status_codes -- --exact --nocapture`：**通过**
- `cargo test -p synapse-services --lib push::providers::tests::test_is_retryable_error_non_retryable -- --exact --nocapture`：**通过**
  - 验证点：本轮 `push/mod.rs` 收口继续复用 canonical retry helper 基础测试，说明整组 `push` facade 化后公共发送辅助路径仍保持可用
- `python3 scripts/ci/check_root_canonical_ledger.py`：**通过**
  - 验证点：`src/services/push/gateway.rs` 已收口为 `synapse_services::push::gateway::*` facade，ledger 统计同步从 `61 thin_facade / 56 full_impl` 变为 `62 thin_facade / 55 full_impl`
- `cargo check --locked`：**通过**
  - 验证点：`push/gateway.rs` facade 化后，root 侧 `services::push::gateway::*` 继续经由 canonical 暴露 push gateway 请求/响应 DTO、配置与发送逻辑，未引入编译回归
- `cargo test --features test-utils --test integration --no-run --locked`：**通过**
  - 验证点：integration 目标在 `push/gateway.rs` 收口后仍可完成整体编译，`services::push::*` 边界未出现路径断裂
- `cargo test -p synapse-services --lib push::gateway::tests::test_push_gateway_config_default -- --exact --nocapture`：**通过**
- `cargo test -p synapse-services --lib push::gateway::tests::test_build_notification -- --exact --nocapture`：**通过**
  - 验证点：canonical `push/gateway.rs` 的基础配置与 notification builder 测试继续通过，说明 gateway 公共导出转交给 canonical 后未破坏现有行为
- `cargo check --locked`：**通过**
  - 验证点：`src/web/routes/state.rs` 已切换到 `services.core.config` grouped view；`synapse-services/src/admin_user_service.rs` 已继续把用户停用状态与 `user_type` 更新收口到 `UserStorage`
- `cargo test --features test-utils --test integration super_admin_can_update_user_v2_deactivation_and_role_fields -- --nocapture`：**通过**
  - 验证点：管理员 `v2` 用户更新接口在 super admin 场景下可正确写入 `deactivated` 与 `user_type`，覆盖本轮 `P1-04` 下沉后的行为一致性
- `cargo check --locked`：**再次通过**
  - 验证点：`synapse-services/src/admin_user_service.rs` 中 direct SQL 已清零，`get_user_stats()` 与 `get_single_user_stats()` 也已改为调用 `UserStorage`，本轮 `P1-04` direct SQL 下沉已完成
- `cargo test --lib web::routes::handlers::versions::tests --locked`：**通过**
- `cargo test --features test-utils --test integration versions_and_public_capabilities_match_declared_room_version_surface -- --nocapture`：**通过**
  - 验证点：`/_matrix/client/versions` 仍保守声明到 `v1.13`，公开 `/_matrix/client/v3/capabilities` 中的 `m.room_versions` 已与 `SUPPORTED_ROOM_VERSIONS` 常量保持一致，并继续对未认证请求隐藏 `m.sso` / `io.hula.*` 私有扩展能力
- `cargo test --features test-utils --test integration federation_query_destination_returns_minimal_payload -- --nocapture`：**通过**
  - 验证点：`/_matrix/federation/v1/query/destination` 返回的 `m.change_password` 与 `m.room_versions.default` 已与 client 能力面共用同一真值来源，不再单独手写漂移值
- `cargo tree -d --workspace | head -n 120`：**确认存在重复依赖版本**
  - 已确认案例：`base64 v0.21.7` 与 `base64 v0.22.1`
- 本地 shared-template-schema 冷启动时间优化：**已落地**
  - 优化范围：`src/test_utils.rs` 中 `template_schema_is_ready()` / `mark_template_schema_ready()` 改为文件系统 marker，避免模板 schema 随每次测试数据库清理被删除后重复 `Strict` 迁移初始化
  - 初步效果：新增 appservice 回归测试已可在 ~6-9秒内完成，不再被 240秒+ 级冷启动阻塞
- `cargo test --features test-utils --test integration test_invite_user_enqueues_appservice_membership_event -- --nocapture`：**通过**
  - 验证点：`invite_user()` 的 appservice membership enqueue 回归测试已完成断言级验证
- `cargo test --features test-utils --test integration test_upgrade_room_enqueues_tombstone_and_replacement_create_events -- --nocapture`：**通过**
  - 验证点：`upgrade_room()` 的 tombstone / replacement room `m.room.create` enqueue 回归测试已完成断言级验证
- `TEST_ISOLATED_SCHEMAS=1 cargo test --features test-utils --test integration room_service_tests_migrated::test_appservice_fatal_delivery_failures_disable_service_and_persist_state -- --exact --nocapture`：**通过**
  - 验证点：`ApplicationServiceStorage` 已对齐统一 schema 中 `application_service_transactions.txn_id` / `transaction_id` 与 `application_service_state.value` / `state_value` 的双列现实；fatal HTTP 失败在连续 3 次后会写入 delivery state、自动禁用对应 AS，并被 `get_all_active()` 正确过滤
- `TEST_ISOLATED_SCHEMAS=1 cargo test --features test-utils --test integration room_service_tests_migrated::test_appservice_ -- --nocapture`：**通过**
  - 验证点：focused appservice sender 集成测试当前已同时覆盖手工触发成功投递、后台 `start_sender()` 轮询 flush pending queue，以及 fatal 自动禁用三条链路；成功路径会真实发出 `PUT /transactions/{txn_id}`、完成 transaction、清空 pending events，并把 `delivery_status` 写回 `up`
- `TEST_ISOLATED_SCHEMAS=1 cargo test --features test-utils --test integration room_service_tests_migrated::test_bridge_e2e_send_message_delivers_real_room_event_payload -- --exact --nocapture`：**通过**
  - 验证点：真实 `RoomService::send_message()` 已可经由 `dispatch_appservice_event()` / `enqueue_matching_event()` 进入 appservice queue，并由后台 sender 发往 mock bridge；桥端收到的 payload 来自真实房间事件存储回填，断言了 `event_id`、`room_id`、`sender`、`content.body`、`queue_event_id`、`delivery_status=up` 以及 pending queue 清空
- `cargo check --locked`：**通过**
  - 验证点：appservice 存储层 schema 兼容修复与 focused failure-disable 回归测试收口后，工作区编译门禁仍保持通过
- `cargo test -p synapse-services --lib application_service::tests::test_namespace_matches_can_require_exclusive_rules --locked`：**通过**
- `cargo test -p synapse-services --lib application_service::tests::test_exclusive_namespace_patterns_extracts_only_exclusive_rules --locked`：**通过**
- `cargo test -p synapse-services --lib application_service::tests::test_is_local_user_id_requires_matching_server_name --locked`：**通过**
  - 验证点：新增 unit tests 已覆盖“exclusive-only namespace 匹配”、“exclusive namespace pattern 提取”与“virtual user 必须属于本地 server_name”三类边界辅助逻辑
- `cargo test --features test-utils --test integration room_service_tests_migrated::test_bridge_e2e_membership_events_deliver_real_room_member_payloads -- --exact --nocapture`：**通过**
  - 验证点：真实 `invite_user()` + `join_room()` 产生的 `m.room.member` 事件已可回填真实房间事件 payload，并经 appservice transaction 一次性投递到 mock bridge；断言了 `room_id`、`state_key`、`sender`、`content.membership`、`queue_event_id` 与 `delivery_status=up`
- `cargo test --features test-utils --test integration api_appservice_p1_tests::test_appservice_namespace_exclusivity -- --exact --nocapture`：**通过**
  - 验证点：exclusive user namespace 当前已从“仅记录”升级为“实际冲突校验”；第二个 AppService 复用同一 exclusive regex 会返回冲突，且 namespace 查询与 virtual user 归属仍保持一致
- `cargo test --features test-utils --test integration api_appservice_p1_tests::test_appservice_virtual_user_requires_exclusive_local_namespace -- --exact --nocapture`：**通过**
  - 验证点：管理面注册 virtual user 时，当前必须命中对应 AppService 拥有的本地 exclusive user namespace；越权 namespace 与外域 user_id 已分别返回 `403` / `400`
- `cargo test --features test-utils --test integration api_appservice_p1_tests::test_appservice_admin_push_event_requires_namespace_ownership -- --exact --nocapture`：**通过**
  - 验证点：管理面显式 `push_event` 当前已要求 `room_id` / `sender` / `state_key` 至少一项落在目标 AppService 的 namespace 内；越权写入返回 `403`，合法 namespace 写入继续允许
- `cargo test --features test-utils --test integration api_appservice_p1_tests::test_appservice_transaction_push -- --exact --nocapture`：**通过**
  - 验证点：在新增 namespace guard 后，管理面显式事件写入的合法路径仍可成功入队，不引入兼容性回归
- `cargo test -p synapse-services --lib scheduler::tests::test_transaction_controller_prioritizes_pending_transactions --locked`：**通过**
- `cargo test -p synapse-services --lib scheduler::tests::test_transaction_controller_rotates_ready_services --locked`：**通过**
  - 验证点：新增 unit tests 已覆盖 transaction controller 的“pending transaction 优先级”与“per-AS round-robin 轮转”两类核心调度策略
- `cargo test --features test-utils --test integration room_service_tests_migrated::test_appservice_scheduler_does_not_block_healthy_service_during_retry_backoff -- --exact --nocapture`：**通过**
  - 验证点：某个 AS 进入 retry backoff 后，scheduler 仍会继续推进健康 AS 的 pending queue，不再被单一失败服务阻塞
- `cargo test --features test-utils --test integration room_service_tests_migrated::test_appservice_scheduler_keeps_single_pending_transaction_per_service -- --exact --nocapture`：**通过**
  - 验证点：同一 AS 在已有未完成 transaction 时，scheduler 不会再为该服务并发创建第二个 transaction，而是保留单 transaction 重试语义
- `cargo test --features test-utils --test integration room_service_tests_migrated::test_appservice_background_sender_flushes_pending_queue -- --exact --nocapture`：**通过**
  - 验证点：`start_sender()` 当前已降级为 scheduler 兼容入口；在新增 per-AS 调度策略后，后台 flush pending queue 的既有语义仍保持成立
- `cargo check --locked`：**通过**
  - 验证点：移除 `server.run()` 中重复 sender 启动入口，并将 sender 收口到统一 scheduler/controller 实现后，工作区编译门禁仍保持通过
- `cargo test -p synapse-services --lib scheduler::tests::test_backlog_state_uses_thresholds --locked`：**通过**
  - 验证点：新增 unit test 已覆盖 scheduler backlog 状态分类逻辑，确认 `idle / normal / high` 会随 pending events / transactions 阈值变化
- `cargo test --features test-utils --test integration room_service_tests_migrated::test_appservice_scheduler_capacity_limit_persists_state -- --exact --nocapture`：**通过**
  - 验证点：当 `max_services_per_tick=1` 时，scheduler 仅放行一个 AS；未被本轮处理的服务会写入 `scheduler_last_result=capacity_limited`、`scheduler_pending_event_count`、`scheduler_backlog_state=high` 与 `scheduler_last_tick_ts`
- `cargo check --locked`：**再次通过**
  - 验证点：在补入 pending 计数接口、capacity limit 与 scheduler state 持久化后，工作区编译门禁仍保持通过
- `cargo test -p synapse-services --lib scheduler::tests::test_transaction_state_reflects_scheduler_result_and_pending_work --locked`：**通过**
  - 验证点：新增 unit test 已覆盖 transaction 聚合状态机，确认 `idle / pending_events / pending_transaction / retry_backoff / capacity_limited` 会随 scheduler result 和待处理事务状态切换
- `cargo test --features test-utils --test integration room_service_tests_migrated::test_appservice_scheduler_does_not_block_healthy_service_during_retry_backoff -- --exact --nocapture`：**再次通过**
  - 验证点：失败 AS 当前会写入 `scheduler_last_result=backoff`、`scheduler_transaction_state=retry_backoff` 与 `scheduler_total_backoff_count`；健康 AS 的 `scheduler_total_success_count` 会持续增长
- `cargo test --features test-utils --test integration room_service_tests_migrated::test_appservice_scheduler_keeps_single_pending_transaction_per_service -- --exact --nocapture`：**再次通过**
  - 验证点：单 AS 单 pending transaction 语义下，服务当前会写入 `scheduler_transaction_state=retry_backoff` 与 `scheduler_total_failure_count=1`，验证失败后状态机/计数器与 transaction 控制语义一致
- `cargo test --features test-utils --test integration room_service_tests_migrated::test_appservice_scheduler_capacity_limit_persists_state -- --exact --nocapture`：**再次通过**
  - 验证点：capacity-limited 场景下，未被处理服务当前会写入 `scheduler_transaction_state=capacity_limited` 与 `scheduler_total_capacity_limited_count=1`，被放行服务会写入 `scheduler_total_success_count=1`
- `cargo test --features test-utils --test integration sustained_backlog -- --nocapture`：**通过**
  - 验证点：在 3 个 AS 持续积压、`max_events_per_txn=1`、`max_services_per_tick=2` 的高负载场景下，被首轮 `capacity_limited` 的服务可在后续 tick 中重新轮转获得 dispatch，不会被持续饿死
- `cargo test --features test-utils --test integration custom_backlog_thresholds -- --nocapture`：**通过**
  - 验证点：在 `max_services_per_tick=1`、`high_pending_event_threshold=3` 的自定义阈值场景下，被限流服务虽保留 `scheduler_transaction_state=capacity_limited`，但 `scheduler_backlog_state` 会随阈值变化保持为 `normal`，证明阈值配置会真实影响持久化调度观测
- `cargo test -p synapse-services --lib default_constants -- --nocapture`：**通过**
  - 验证点：`ApplicationServiceScheduler` 默认常量当前已显式固定为 `MAX_SERVICES_PER_TICK=8`、`HIGH_PENDING_EVENT_THRESHOLD=50`、`HIGH_PENDING_TRANSACTION_THRESHOLD=2`
- `cargo test -p synapse-services --lib default_threshold_boundaries -- --nocapture`：**通过**
  - 验证点：默认 backlog 阈值边界当前已具备精确单测证据，确认 `49 events -> normal`、`50 events -> high`、`1 pending transaction -> normal`、`2 pending transactions -> high`
- `cargo test -p synapse-services --lib aggressive_thresholds_escalate_same_load -- --nocapture`：**通过**
  - 验证点：同一负载在更保守阈值下会被提前提升到 `high`，当前已明确验证 `25 events` 在默认 `50` 下仍为 `normal`、在阈值 `25` 下变为 `high`；`1 pending transaction` 在默认 `2` 下仍为 `normal`、在阈值 `1` 下变为 `high`
- `DATABASE_URL=postgresql://synapse:***@localhost:5432/synapse_test TEST_DATABASE_URL=postgresql://synapse:***@localhost:5432/synapse_test TEST_DB_TEMPLATE_SCHEMA=public cargo test --features test-utils --test integration room_service_tests_migrated::test_appservice_scheduler_persists_different_backlog_state_for_default_vs_aggressive_event_thresholds -- --exact --nocapture`：**通过**
  - 验证点：在本地 `synapse_test` 完成 `db_migrate.sh migrate` / `validate`，并同时补齐 `DATABASE_URL` 与 `TEST_DATABASE_URL` 后，focused integration 已真正进入运行时；共享模板 schema 下两次 scenario 复用固定 exclusive room namespace 的冲突也已修复，当前默认 `50` vs 更保守 `25` 的持久化 backlog 状态对比已恢复为断言级证据
- `DATABASE_URL=postgresql://synapse:***@localhost:5432/synapse_test TEST_DATABASE_URL=postgresql://synapse:***@localhost:5432/synapse_test TEST_DB_TEMPLATE_SCHEMA=public cargo test --features test-utils --test integration room_service_tests_migrated::test_appservice_scheduler_default_capacity_limit_handles_ninth_service -- --exact --nocapture`：**通过**
  - 验证点：在默认 `MAX_SERVICES_PER_TICK=8`、9 个活跃 AS 同时各自有 1 个 pending event 的边界场景下，scheduler 单 tick 仅放行 8 个服务，第 9 个服务会写入 `scheduler_last_result=capacity_limited`、`scheduler_transaction_state=capacity_limited`、`scheduler_pending_event_count=1`、`scheduler_backlog_state=normal`；该默认容量边界在本地恢复环境后已再次完成 focused 复核
- `DATABASE_URL=postgresql://synapse:***@localhost:5432/synapse_test bash docker/db_migrate.sh migrate`：**本轮已恢复通过**
  - 复核结果：本轮在本地空库链路上收口了 v10 基线与迁移脚本的多处 drift，包括 `rooms` 创建顺序、`users.name` / `key_rotation_config.room_id` / `registration_captcha.session_id` 无效索引、`rendezvous_sessions` / `qr_login_codes` 错误表名，以及 `docker/db_migrate.sh` 的 `schema_migrations.executed_at` / `is_success` 元数据漂移；修正后本地 `synapse_test` 已能完成 `db_migrate.sh migrate` 与 `validate`
- `DATABASE_URL=postgresql://synapse:***@localhost:5432/synapse_test TEST_DATABASE_URL=postgresql://synapse:***@localhost:5432/synapse_test TEST_DB_TEMPLATE_SCHEMA=public cargo test --features test-utils --test integration room_service_tests_migrated::test_appservice_scheduler_persists_different_backlog_state_for_default_vs_aggressive_transaction_thresholds -- --exact --nocapture`：**通过**
  - 验证点：默认 `HIGH_PENDING_TRANSACTION_THRESHOLD=2` 场景下，单个被限流服务持有 `1 pending transaction` 时会写回 `scheduler_pending_transaction_count=1` 且 `scheduler_backlog_state=normal`；切到更保守阈值 `1` 后，同一负载会写回 `scheduler_backlog_state=high`，说明当前默认 `2` 与更保守 `1` 的运行时分界已完成持久化复核
- `DATABASE_URL=postgresql://synapse:***@localhost:5432/synapse_test TEST_DATABASE_URL=postgresql://synapse:***@localhost:5432/synapse_test TEST_DB_TEMPLATE_SCHEMA=public cargo test --features test-utils --test integration room_service_tests_migrated::test_appservice_scheduler_prioritizes_pending_transactions_over_pending_events -- --exact --nocapture`：**通过**
  - 验证点：当 `application_service_statistics` 不提供可用 pending 计数时，scheduler 现会在排序前回退到 live pending counts 重新观测并重排，确保“已有 pending transaction 的服务”优先于“仅有 pending events 的服务”获得单 tick 唯一 dispatch 槽位；被延后的事件服务会写回 `capacity_limited`
- `DATABASE_URL=postgresql://synapse:***@localhost:5432/synapse_test TEST_DATABASE_URL=postgresql://synapse:***@localhost:5432/synapse_test TEST_DB_TEMPLATE_SCHEMA=public cargo test --features test-utils --test integration room_service_tests_migrated::test_appservice_scheduler_rotates_capacity_limited_service_under_sustained_transaction_backlog -- --exact --nocapture`：**通过**
  - 验证点：在 3 个 AS 各持有 `2 pending transactions`、`max_services_per_tick=2` 的持续 transaction backlog 场景下，首轮被限流服务会写回 `scheduler_pending_transaction_count=2`、`scheduler_backlog_state=high`、`scheduler_transaction_state=capacity_limited`，并会在后续 tick 中重新轮转获得 dispatch；4 tick 内可清空全部 6 个预置 transaction，说明当前调度在 transaction backlog 下仍具备公平性且默认 transaction 阈值 `2` 的高压分界与实际行为一致
- `DATABASE_URL=postgresql://synapse:***@localhost:5432/synapse_test TEST_DATABASE_URL=postgresql://synapse:***@localhost:5432/synapse_test TEST_DB_TEMPLATE_SCHEMA=public cargo test --features test-utils --test integration room_service_tests_migrated::test_appservice_scheduler_handles_mixed_event_and_transaction_backlog_under_capacity_limit -- --exact --nocapture`：**通过**
  - 验证点：在 `2` 个 transaction-backlog AS 与 `1` 个 event-heavy AS 共同竞争 `max_services_per_tick=2` 的 mixed 场景下，scheduler 首轮会优先放行两个 pending-transaction 服务，并将 event-heavy 服务写回 `scheduler_last_result=capacity_limited`、`scheduler_pending_event_count=60`、`scheduler_backlog_state=high`、`scheduler_transaction_state=capacity_limited`；当 transaction 压力缓解后，event-heavy 服务会在下一 tick 重新获得 dispatch 且 backlog 被清空，说明默认 `8/2` 在 mixed 负载下仍保持优先级正确性与基本公平性
- `DATABASE_URL=postgresql://synapse:***@localhost:5432/synapse_test TEST_DATABASE_URL=postgresql://synapse:***@localhost:5432/synapse_test TEST_DB_TEMPLATE_SCHEMA=public cargo test --features test-utils --test integration room_service_tests_migrated::test_appservice_scheduler_mixed_backlog_does_not_block_healthy_services_during_retry_backoff -- --exact --nocapture`：**通过**
  - 验证点：在 `1` 个失败并进入 `retry_backoff` 的 pending-transaction AS、`1` 个健康 pending-transaction AS 与 `1` 个 event-heavy AS 共同竞争 `max_services_per_tick=2` 的 mixed 场景下，scheduler 首轮仍会放行失败事务与健康事务，次轮会抑制失败 AS 的立即重试，同时让 event-heavy 健康服务获得 dispatch 并清空 backlog；说明 mixed 负载叠加 backoff 时，失败 AS 不会拖慢健康 transaction/event 服务的推进
- `cargo check --locked`：**受无关编译错误阻塞，待环境收口后复核**
  - 阻塞原因：当前工作区的 `synapse-storage/src/federation_blacklist.rs`、`refresh_token.rs`、`token.rs` 存在一批与本轮 scheduler 改动无关的 `sqlx` 推断错误（`error[E0282]: type annotations needed`），导致全仓 `cargo check --locked` 无法完成
- `GetDiagnostics`（`synapse-services/src/application_service/mod.rs`、`synapse-services/src/application_service/scheduler.rs`、`tests/integration/api_appservice_p1_tests.rs`）：**通过**
  - 验证点：本轮新增的 admin scheduler 聚合输出与 focused test 文件在 IDE 诊断层无新增错误
- `cargo check --locked`：**通过**
  - 验证点：此前阻塞 appservice scheduler 多出口验证的 `sqlx::query!` 编译期连接耗尽已消失，当前工作区可再次完成本轮增量编译复核
- `cargo test --features test-utils --test integration test_appservice_statistics_expose_scheduler_summary -- --exact --nocapture`：**通过**
  - 验证点：`/_synapse/admin/v1/appservices/statistics` 已稳定输出 `scheduler.available`、`last_result`、`transaction_state` 与聚合计数/耗时字段
- `GetDiagnostics`（`src/web/routes/telemetry.rs`、`tests/integration/api_telemetry_alerts_tests.rs`）：**通过**
  - 验证点：本轮新增的 telemetry scheduler 汇总响应与 focused integration 文件在 IDE 诊断层无新增错误
- `cargo test --features test-utils --test integration test_telemetry_metrics_alerts_and_ack -- --exact --nocapture`：**通过**
  - 验证点：`/_synapse/admin/v1/telemetry/metrics` 已可聚合输出 appservice scheduler 的 backoff/pending/计数器汇总视图，且不影响原有 telemetry alerts 流程
- `GetDiagnostics`（`src/server.rs`）：**通过**
  - 验证点：Prometheus listener 状态装配切换到 appservice-aware 渲染后，server 侧无新增诊断错误
- `cargo test --lib summarize_appservice_scheduler_metrics_aggregates_scheduler_state -- --exact --nocapture`：**通过**
  - 验证点：telemetry 侧 scheduler 汇总函数可正确聚合 per-AS scheduler 状态
- `cargo test --lib render_appservice_scheduler_prometheus_metrics_includes_expected_series -- --exact --nocapture`：**通过**
  - 验证点：独立 `/metrics` 文本出口已稳定追加 `synapse_appservice_scheduler_*` 系列 gauge 骨架
- `cargo test --features test-utils --test integration test_telemetry_metrics_reflect_real_scheduler_recovery_flow -- --exact --nocapture`：**通过**
  - 验证点：真实 scheduler 驱动的“失败一次后恢复成功”路径，已能稳定反映到 `/_synapse/admin/v1/telemetry/metrics` 聚合出口；最终聚合态表现为 pending 清零、backoff 清零、成功计数恢复，且单服务 `scheduler_last_result` 与 `scheduler_transaction_state` 已回到健康状态
- `cargo test --features test-utils --test integration test_telemetry_metrics_preserve_explainable_mixed_contention_counts -- --exact --nocapture`：**通过**
  - 验证点：在“retry_backoff 服务 + 健康 pending-transaction 服务 + 多个 event-heavy 服务”的 mixed contention 场景下，telemetry 聚合出口会给出可解释的 `failure_count / backoff_count / capacity_limited_count` 关系，证明运维计数面与真实调度语义一致
- `cargo test --lib render_appservice_scheduler_prometheus_metrics_reflects_recovery_summary -- --exact --nocapture`：**通过**
  - 验证点：Prometheus 文本渲染链已补齐 recovery summary 单测，确认 `/metrics` 中的 `synapse_appservice_scheduler_*` 指标会跟随恢复后的聚合结果更新

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
| `AdminUserDetails` 直接暴露 `User` 存储类型 | **已证伪** | 当前 `AdminUserDetails.user` 与 `AdminSingleUserStats.user` 已统一收口为 service DTO `AdminUserProfile` | 分层隔离、序列化边界 | 管理员查询单个用户 |
| `admin_user_service` 直接 SQL 绕过 storage | **已修复** | root facade 不再承载实现；canonical `synapse-services/src/admin_user_service.rs` 中用户列表、停用状态更新、`user_type` 更新、批量停用、用户统计与单用户消息统计均已改为调用 `UserStorage` | 管理后台服务层 | 用户管理接口变更、审计、测试 |
| `application_service` root 与 canonical 为双全量实现 | **已证伪** | root `src/services/application_service.rs` 和 `src/storage/application_service.rs` 都已 facade 化 | 服务/存储迁移状态判断 | 分层迁移评估 |
| `application_service` 仍存在 `processed` / `transaction_id` SQL 错误 | **已证伪** | 当前 canonical 存储实现已修复相关 SQL | Appservice 事务流 | 事件投递、事务完成 |
| 文档中的模块对数/行数统计可直接作为当前规模判断 | **部分失真** | 当前递归统计显示 `services` 同名重叠 119 个、`storage` 同名重叠 58 个，且已补入脚本化 overlap ledger；原文数字已不适合作为现状统计 | 冗余规模评估 | 制定迁移排期 |
| `src/services/mod.rs` 存在全量 storage 泄漏 | **已证伪** | `pub use crate::storage::*` 已移除；当前遗留问题转为“部分服务内部曾依赖隐式 storage 导出，现已开始显式收口” | 全部 service 使用面 | 新功能接入、跨模块引用 |

### 3.3 `SUPPORTED_MATRIX_SURFACE.md` 复核

| 原文问题/结论 | 当前验证结论 | 复现情况 | 影响范围 | 发生场景 |
|---|---|---|---|---|
| Client API versions 声明 `r0.5.0`、`r0.6.x`、`v1.1..v1.13` | **真实存在** | `CLIENT_API_VERSION_SUPPORT` 与文档一致 | 协议声明 | `/_matrix/client/versions` |
| 默认 room version 为 `10` | **真实存在** | `DEFAULT_ROOM_VERSION` 仍为 `10` | 建房行为 | 创建房间 |
| 文档称只支持 `1..11`，不声明 `12` | **已证伪** | 当前 `SUPPORTED_ROOM_VERSIONS` 明确包含 `12`、`13`，测试也断言 `resolve_room_version(Some("12")) == Some("12")` | 协议面文档、客户端兼容预期 | `/capabilities`、联邦协商 |
| federation membership 已检查 federation 维度 | **真实存在** | `src/web/routes/federation/membership.rs` 中已调用 `can_federate_room_version()` | 联邦安全与兼容性 | join/leave/invite/knock |
| Focused gate 当前不可运行 | **部分失真** | 文档里的目标测试至少可通过 `--no-run` 编译，不再被立即卡死 | 协议面验证流程 | 本地协议面回归检查 |
| `m.change_password` / `m.set_displayname` / `m.set_avatar_url` / `m.3pid_changes` 需后续从静态 true 收敛 | **部分失真** | 当前这四项已收敛到 `versions.rs` 中的集中具名真值函数，不再直接裸写 `insert_enabled_capability(..., true)`；更大范围 capability 现已开始细分到“配置控制 / 路由存在性 / 静态稳定”，但仍未全部完成 | 协议兼容面准确性 | 客户端依据 capability 判定功能时 |

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

- **仍真实存在的核心问题**：protocol surface 文档漂移、appservice 架构缺口、root/canonical 双轨冗余。
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
| Appservice 事件推送 | 自动 namespace 匹配 + 调度 + 失败恢复 | 已在本地房间事件、联邦 membership/transaction、以及 join/leave/invite/ban/unban/kick/upgrade/friend-room state event/部分建房事件等旁路入口接入 appservice enqueue 或提交后统一分发，并新增自动 sender、基础 backoff/recoverer 与失败分类/自动隔离坏 AS；真实 `send_message()` 的 message-path bridge e2e 与 `m.room.member` 的 membership bridge e2e 已补齐，virtual user / exclusive namespace / admin 显式写入边界也已收口；前几轮已补入 transaction controller 首版、per-AS round-robin 调度策略、每轮活跃 AS 限流、backlog 阈值识别、transaction 聚合状态机与 scheduler 结果计数/最近一次指标；最近几轮进一步修复了 statistics 读面依赖未维护统计表的问题，使 admin statistics、telemetry 与 Prometheus 三个出口统一基于实时聚合工作，并补齐 recovery flow、mixed contention 运维计数关系与 Prometheus 恢复摘要证据；当前主缺口已收敛到运行时阈值调优与更细的高负载策略治理 | **部分补齐，主缺口转向运行时阈值调优与高负载策略治理** |
| 分层隔离 | REST/handler/storage 边界清晰 | nominal 上仍是 `route -> service -> storage`；`services/mod.rs` 的全量 storage 泄漏已移除，但 root/canonical 双轨与少量 service 直连 SQL 仍削弱边界治理 | **架构短板** |
| Worker/replication | 围绕单写多读与缓存失效设计 | 已有 worker/replication 雏形，但根/canonical 双轨与编译门禁仍拖慢收敛 | **部分具备，未完全成熟** |
| 验证码/辅助服务可运营性 | 文档、配置、投递链路完整闭环 | `captcha_service` 已有模板与存储层，但 email/sms 发送仍是 `todo!()` stub，配置打开后无法形成真实投递闭环 | **运行时缺口** |
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

- 递归统计显示，`src/services` 与 `synapse-services/src` 之间存在 **119 个同名 `.rs` 文件重叠**。
- `src/storage` 与 `synapse-storage/src` 之间存在 **58 个同名 `.rs` 文件重叠**。
- 这意味着当前仍是“root 实现 + canonical 实现/壳文件”并存，而不是单一事实来源。

### 5.2 配置冗余

- `app_service_config_files` 已从“死配置面”收敛为“启动期可消费配置面”；本地房间事件、联邦 membership/transaction，以及 join/leave/invite/ban/unban/kick/upgrade/friend-room state event/部分建房事件等旁路入口会自动 enqueue 到 appservice pending queue，其中建房事务内事件会在提交成功后统一 dispatch；sender 会周期性优先重试未完成 transaction、再组批发送 pending events，并已加入基础 backoff/recoverer 以及失败分类/自动隔离坏 AS 的第二层治理；前几轮已补齐 message-path 与 membership 两条 bridge e2e，并将 virtual user、exclusive namespace、管理面显式 `push_event` 的 ownership guard 收口到真实代码路径，同时把 `start_sender()` 收敛为 scheduler 兼容入口、补入 transaction controller 首版、per-AS round-robin 调度策略、容量治理、transaction 聚合状态机与 scheduler 状态/计数器；最近几轮继续将 scheduler 聚合状态显式暴露到 `/_synapse/admin/v1/appservices/statistics`、`/_synapse/admin/v1/telemetry/metrics` 与独立 Prometheus `/metrics` 文本出口，且已修复 statistics 读面依赖未维护统计表的产品缺口，补齐 telemetry recovery flow、mixed contention 运维计数关系与 Prometheus recovery summary 证据；剩余主缺口已转向运行时阈值调优与更细的策略调优。
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

- **当前验证**：`app_service_config_files` 已有启动期 YAML 装载、regex/URL/`sender` 基本校验与幂等导入，运行时会在服务容器装配后自动消费配置；`RoomService::create_event()` 已接入 `app_service_manager.enqueue_matching_event(...)`，本地房间消息、状态事件和 pinned state 更新会按 `room_id` / `sender` / `state_key` 做 namespace 匹配并写入 appservice pending queue；联邦 `transaction` / `membership` 路由与 `voip` / `room redaction` / `com.hula.privacy` 等路由旁路事件入口在持久化成功后会 best-effort 进入 appservice queue；`join_room()` / `leave_room()` / `invite_user()` / `ban_user()` / `unban_user()` / `kick_user()` / `upgrade_room()` / `FriendRoomService::send_state_event()` 等 service 旁路入口已经切回统一事件链路；继续按调用面复核后，当前运行时入口中的 `dispatch_appservice_event()` 仅作为 `enqueue_matching_event()` 的统一包装存在，生产代码里对 `ApplicationServiceManager::push_event()` 的直接调用已基本收敛到管理面 appservice API，而不再散落于业务路由；建房链路中的 `m.room.create`、`m.room.member`、`m.room.power_levels`、`m.room.join_rules`、`m.room.history_visibility`、`m.room.guest_access`、`m.room.encryption`、`com.hula.privacy`、`initial_state`、metadata 与邀请事件会先在事务内持久化，再在 commit 成功后统一 dispatch 到 appservice queue；`room_service_tests_migrated.rs` 已新增回归测试，覆盖 `create_room()` 提交后 enqueue、`join_room()` / `invite_user()` 旁路 membership enqueue、`upgrade_room()` 的 `m.room.tombstone` 与 replacement room `m.room.create` enqueue、`test_appservice_successful_delivery_completes_transaction_and_marks_event_processed` 的 sender 成功投递链路、`test_appservice_background_sender_flushes_pending_queue` 的后台 sender 轮询链路、`test_appservice_fatal_delivery_failures_disable_service_and_persist_state` 的 fatal 连续失败自动禁用链路、`test_bridge_e2e_send_message_delivers_real_room_event_payload` 的真实 `RoomService::send_message()` -> appservice queue -> 后台 sender -> mock bridge 收包链路，以及前两轮新增的 scheduler focused integration；`tests/integration/mod.rs` 的外层 integration setup timeout 已与 `configured_test_db_init_timeout()` 对齐，不再在 `shared-template-schema` 冷启动时于 120s 提前截断；`room_service_tests_migrated.rs` 的 appservice 建表夹具本轮继续对齐统一 schema 中 `application_service_transactions.txn_id` / `transaction_id` 与 `application_service_state.value` / `state_value` 的双列现实，避免测试夹具与生产 schema 再次漂移；当前 `ApplicationServiceScheduler` 已接管 per-AS 调度主链路，新增 transaction controller 首版、pending transaction 优先级与 round-robin 调度策略，并通过 `started` guard 与移除 `server.run()` 中重复 `start_sender()` 入口消除双 sender 并发启动；前一轮已补入 `max_services_per_tick` 限流、backlog 阈值识别、真实 pending 计数回退，以及 `scheduler_last_result` / `scheduler_pending_event_count` / `scheduler_pending_transaction_count` / `scheduler_backlog_state` / `scheduler_last_tick_ts` 状态写回；本轮继续补入 `scheduler_transaction_state` 聚合状态机、`scheduler_total_success_count` / `scheduler_total_failure_count` / `scheduler_total_backoff_count` / `scheduler_total_capacity_limited_count` / `scheduler_total_in_flight_count` 计数器，以及 `scheduler_last_dispatched_events` / `scheduler_last_elapsed_ms` 最近一次调度指标；`ApplicationServiceManager::start_sender(...)` 现仅保留为 scheduler 兼容入口；新增 `retry_backoff_ms(...)`、`is_transaction_ready_to_retry(...)`、HTTP 失败分类与自动禁用阈值后，单 AS 在存在未完成 transaction 时已按 `retry_count` 执行基础退避重试，并已通过 focused integration test 同时验证“成功 HTTP 投递 -> 真实发出 `PUT /transactions/{txn_id}` -> 完成 transaction -> 清空 pending events -> `delivery_status=up`”、“后台 `start_sender()` 兼容入口 -> 自动 flush pending queue -> 完成 transaction -> 清空 pending events”、“fatal HTTP 失败连续 3 次 -> 写入 delivery state -> 自动禁用 AS -> `get_all_active()` 过滤生效”、“失败 AS 进入 retry backoff 后不阻塞健康 AS 继续出队”、“同一 AS 存在未完成 transaction 时不会并发创建第二个 transaction”，以及“命中每轮活跃 AS 上限时会将剩余服务标记为 `capacity_limited` 并持久化 pending/backlog 状态”的完整链路；最近几轮又补入 `test_appservice_scheduler_recovers_multiple_retry_backoff_services_without_restarving_event_bucket`、`test_appservice_scheduler_continuous_event_ingress_does_not_starve_event_bucket_after_transaction_backlog_drains` 与 `test_appservice_scheduler_super_event_heavy_service_begins_dispatch_within_two_ticks_under_light_transaction_bursts` 三条 focused integration，把“多个 retry_backoff 服务共享恢复窗口后再次争用”、“多个 pending-transaction 长时间 backlog 叠加 event-heavy 持续追加事件”以及“单个超大 event-heavy 与多个轻量 transaction 混跑”的下一阶段高负载场景补成了真实代码侧证据。与上游 Synapse 的自动事件推送、scheduler、transaction controller、recoverer 相比，当前实现已具备基础调度闭环、联邦/更广的旁路覆盖、第二层 recoverer 治理、per-AS 调度首版、容量治理首版和 transaction 聚合状态/指标首版，剩余缺口已进一步收敛到生产压测与阈值调优，而不再是代码侧 focused 场景缺证。
- **复现步骤**：检索 `enqueue_matching_event(`、`dispatch_appservice_event(` 与 `RoomService::create_event()` 调用面，可见本地房间事件已开始自动挂接 appservice；检索 `src/web/routes/federation/transaction.rs`、`src/web/routes/federation/membership.rs`、`src/web/routes/voip.rs`、`src/web/routes/room.rs` 与 `src/web/routes/handlers/room/events.rs` 中的 `dispatch_appservice_event(` 调用，可见联邦/路由旁路入口已补齐一批 best-effort enqueue；检索 `src/services/room/membership_actions.rs`、`src/services/room/membership_moderation.rs`、`src/services/room/upgrade.rs`、`src/services/friend_room_service/mod.rs` 中对 `self.create_event(` 或 `room_service.create_event(` 的调用，以及 `src/services/room/create.rs` 中 commit 后统一 `dispatch_appservice_event(` 的循环，可见 service 旁路与建房事务后分发已落地；继续检索 `retry_backoff_ms(`、`is_transaction_ready_to_retry(`、`classify_http_failure(`、`should_disable_service(`、`ApplicationServiceScheduler`、`plan_dispatch_order(`、`with_capacity_options(` 与 `scheduler_last_result` / `scheduler_transaction_state` / `scheduler_total_success_count` 等 state key，可见 recoverer 已扩展到失败分类与坏 AS 自动隔离，而 sender 主链路已统一到带容量治理与状态机的 scheduler/controller。
- **影响范围**：桥接类 AS（IRC/Slack/Discord 等）、第三方服务集成、从 Synapse 迁移的配置兼容性。
- **发生场景**：部署 bridge、期待 namespace 事件自动推送、使用 Synapse YAML appservice 配置迁移时。
- **优化方案**：补齐 AS 配置装载、namespace 匹配、排队发送、事务重试恢复全链路；以 canonical service/storage 为主线实现，root 侧仅保留兼容 facade。
- **本轮新增落地点**：`ApplicationServiceStorage` 已对 user/room/alias namespace 查询优先选择 `is_exclusive=TRUE` 规则，并新增 exclusive namespace 冲突探测；`ApplicationServiceManager::register_virtual_user(...)` 当前要求 user_id 属于本地 server 且命中对应 AppService 的 exclusive user namespace；`ApplicationServiceManager::push_event(...)` 当前要求显式写入至少命中目标 AppService 的 `room_id` / `sender` / `state_key` namespace；`room_service_tests_migrated.rs` 已补入 `test_bridge_e2e_membership_events_deliver_real_room_member_payloads`，覆盖 invite/join membership 到 mock bridge 的真实 payload 投递。
- **本轮新增落地点**：`ApplicationServiceScheduler` 已新增 transaction controller 首版、pending transaction 优先级与 per-AS round-robin 调度策略，并通过 `started` guard 防止重复启动；`ApplicationServiceManager::start_sender(...)` 已改为 scheduler 兼容入口；`server.run()` 中重复的 sender 启动入口已删除；`room_service_tests_migrated.rs` 已补入“失败 AS retry backoff 不阻塞健康 AS”与“同一 AS 仅保持单 pending transaction”两条 focused integration，覆盖 per-AS 调度与 transaction controller 的核心行为。
- **本轮新增落地点**：`ApplicationServiceStorage` / `ApplicationServiceManager` 已补入真实 pending events / transactions 计数接口；`ApplicationServiceScheduler` 已新增 `max_services_per_tick` 限流、backlog 阈值识别、统计缺失时的真实 pending 计数回退，以及 `scheduler_last_result` / `scheduler_pending_event_count` / `scheduler_pending_transaction_count` / `scheduler_backlog_state` / `scheduler_last_tick_ts` 状态写回；`room_service_tests_migrated.rs` 已补入 `test_appservice_scheduler_capacity_limit_persists_state`，覆盖容量限流与调度状态观测行为，并新增 `test_appservice_scheduler_rotates_capacity_limited_service_under_sustained_backlog`，证明持续积压下被首轮限流的服务仍会在后续 tick 中轮转获得出队机会；随后又补入 `test_appservice_scheduler_uses_custom_backlog_thresholds_for_limited_service`、`test_appservice_scheduler_default_capacity_limit_handles_ninth_service`、`test_appservice_scheduler_persists_different_backlog_state_for_default_vs_aggressive_event_thresholds`、`test_appservice_scheduler_persists_different_backlog_state_for_default_vs_aggressive_transaction_thresholds`、`test_appservice_scheduler_prioritizes_pending_transactions_over_pending_events`、`test_appservice_scheduler_rotates_capacity_limited_service_under_sustained_transaction_backlog`、`test_appservice_scheduler_handles_mixed_event_and_transaction_backlog_under_capacity_limit` 与 `test_appservice_scheduler_mixed_backlog_does_not_block_healthy_services_during_retry_backoff`，分别证明自定义阈值会真实改变持久化的 `scheduler_backlog_state`、默认 `MAX_SERVICES_PER_TICK=8` 的第 9 个活跃服务会被单 tick 容量边界正确限流、同一组积压在默认/更保守事件阈值下会写回不同的 `scheduler_backlog_state`、同一组 `1 pending transaction` 负载在默认 `HIGH_PENDING_TRANSACTION_THRESHOLD=2` 与更保守 `1` 下会写回不同的 `scheduler_backlog_state`、在 `application_service_statistics` 缺失有效 pending 计数时 scheduler 仍会先用 live counts 重排并优先调度已有 pending transaction 的服务、持续 transaction backlog 下首轮被限流服务仍会在后续 tick 中轮转获得 dispatch、mixed event/transaction backlog 下 transaction 优先级与 event-heavy 服务后续回补 dispatch 的实际行为，以及 mixed backlog 叠加 retry backoff 时失败 AS 不会拖慢健康 transaction/event 服务的推进；在本地恢复数据库、修复统一基线与迁移脚本 drift 并补齐完整测试环境变量后，上述七条 focused integration 已重新通过。当前 P0-02 的运行时主缺口已从“恢复默认值证据”进一步收敛为“更细的高负载策略治理验证”，而非继续立即调整默认值。
- **本轮新增落地点**：`ApplicationServiceScheduler` 已新增 `scheduler_transaction_state` 聚合状态机，以及 success/failure/backoff/capacity/in-flight 计数器和最近一次调度指标写回；`room_service_tests_migrated.rs` 已把 backoff、单 pending transaction、capacity-limited 三条 focused integration 继续扩展为状态机/计数器断言。
- **本轮新增落地点**：`ApplicationServiceManager::get_statistics()` 当前会把 `scheduler_last_result`、`scheduler_transaction_state`、pending/backlog 计数、success/failure/backoff/capacity/in-flight 计数器与最近一次调度指标聚合进 `/_synapse/admin/v1/appservices/statistics` 的 `scheduler` 字段，形成稳定的 admin 读取面；`api_appservice_p1_tests.rs` 已新增 `test_appservice_statistics_expose_scheduler_summary`，命令行 focused integration 已完成通过，admin 读取面的外部行为已有运行时证据。
- **本轮新增落地点**：`src/web/routes/telemetry.rs::get_metrics_summary()` 当前会继续读取 appservice statistics 中的 `scheduler` 视图，并把 `services_in_backoff`、`services_capacity_limited`、`services_with_pending_transactions`、`total_pending_events`、`total_pending_transactions` 与 success/failure/backoff/capacity/in-flight 聚合计数汇总到 `/_synapse/admin/v1/telemetry/metrics`；`tests/integration/api_telemetry_alerts_tests.rs` 已扩展 telemetry 集成测试，且 focused integration 已通过，telemetry 聚合面的运行时证据已补齐。
- **本轮新增落地点**：`src/server.rs::render_prometheus_metrics()` 当前会在原有 collector 文本后，以 best-effort 方式追加 `synapse_appservice_scheduler_*` 系列 gauge，包括服务总数、available/backoff/capacity-limited 服务数、pending events/transactions，以及 success/failure/backoff/capacity/in-flight 聚合计数；当 appservice statistics 读取失败时仅记录 warning，不阻断 `/metrics` 整体输出。`src/server.rs` 已补入文本渲染单测覆盖该输出骨架。
- **本轮新增落地点**：`synapse-storage/src/application_service.rs::get_statistics()` 已不再把 `application_service_statistics` 视为唯一事实来源，而是从 `application_services`、`application_service_users`、`application_service_events` 与 `application_service_transactions` 做实时聚合，修复了注册成功但 statistics/telemetry/Prometheus 读面返回空集或失真的产品缺口；`update_last_seen()` 也已改为缺行 `upsert`，避免观测链因统计表缺记录而断裂。
- **本轮新增落地点**：`tests/integration/api_telemetry_alerts_tests.rs` 已补入 `test_telemetry_metrics_reflect_real_scheduler_recovery_flow` 与 `test_telemetry_metrics_preserve_explainable_mixed_contention_counts`，分别覆盖“失败一次后恢复成功”的真实 scheduler 恢复路径，以及“retry_backoff 服务 + 健康 pending-transaction 服务 + 多个 event-heavy 服务”在较长时间窗 mixed contention 下的运维计数关系；telemetry 侧当前不仅能输出聚合值，还能给出与实际调度语义一致、可向运维解释的计数面。
- **本轮新增落地点**：`src/server.rs` 已补入 `render_appservice_scheduler_prometheus_metrics_reflects_recovery_summary`，证明 Prometheus 文本出口会跟随 recovery flow 聚合结果更新；配合 telemetry recovery flow 与 mixed contention 测试，当前 admin statistics / telemetry / Prometheus 三个外部观测出口已形成一致证据链。
- **本轮新增落地点**：`tests/integration/room_service_tests_migrated.rs` 已继续补入 `test_appservice_scheduler_recovers_multiple_retry_backoff_services_without_restarving_event_bucket`、`test_appservice_scheduler_continuous_event_ingress_does_not_starve_event_bucket_after_transaction_backlog_drains` 与 `test_appservice_scheduler_super_event_heavy_service_begins_dispatch_within_two_ticks_under_light_transaction_bursts`，分别验证“多个 retry_backoff 服务在共享恢复窗口结束后重新争用时不会重新饿死 event bucket”、“多个 pending-transaction 服务长期 backlog 下 event-heavy 持续追加事件后仍能在 transaction 压力释放后完整回补 dispatch”，以及“单个超大 event-heavy 服务在轻量 transaction burst 压力下会先被首轮 `capacity_limited`，但仍能在第二个 tick 开始 dispatch 并持续排空 backlog”；这三条 focused integration 已补齐代码侧下一阶段验证闭环。
- **实施步骤**：
  1. 已完成：落地 `app_service_config_files` YAML 加载与校验，并通过 `upsert_registration()` 同步写入统一存储。
  2. 已完成首版：在 `RoomService::create_event()` 本地房间事件链路中引入 namespace 匹配与 enqueue。
  3. 已完成首版：实现周期 sender，优先重试未完成 transaction，再从 pending queue 组批发送。
  4. 已完成首版：补齐基础 backoff/recoverer，失败 transaction 会更新时间基线，sender 会按 `retry_count` 退避重试最早未完成 transaction。
  5. 已完成增强：补齐联邦 `transaction` / `membership` 与部分旁路事件入口的 best-effort enqueue，并为 recoverer 增加失败分类、delivery state 与坏 AS 自动禁用阈值。
  6. 已完成增强：补充 `create_room()` 提交后 enqueue、`join_room()` / `invite_user()` membership enqueue，以及 `upgrade_room()` 的 tombstone / replacement room create enqueue 回归测试；同时继续收口 integration 夹具中的 `PresenceStorage` / canonical cache 漂移，修正 integration setup 外层 timeout 与 shared template marker（从数据库改为文件系统），确保新增 4 条 appservice 回归测试已完成断言级本地验证。
  7. 已完成增强：补齐 `m.room.member` 的 membership bridge e2e、virtual user 必须命中本地 exclusive user namespace 的边界校验、exclusive namespace 冲突校验，以及管理面显式 `push_event` 的 namespace 所有权约束；当前 message-path 与 membership 两条最短路径均已具备 focused integration 证据。
  8. 已完成增强：补齐 transaction controller 首版、pending transaction 优先级、per-AS round-robin 调度策略，并移除运行时双 sender 启动入口；当前调度主链路已统一到 scheduler/controller。
  9. 已完成增强：补齐每轮活跃 AS 限流、backlog 阈值识别、真实 pending 计数回退与 scheduler 状态写回；当前容量治理首版已落地。
  10. 已完成增强：补齐 transaction 聚合状态机与 scheduler 结果计数/最近一次指标写回；当前状态面已能表达 `retry_backoff` / `capacity_limited` 等关键调度语义。
  11. 已完成首版：补齐 admin statistics 侧的 scheduler 聚合状态显式暴露。
  12. 已完成首版：补齐 telemetry metrics 侧的 appservice scheduler 汇总输出。
  13. 已完成首版并补齐 focused 复核：补齐独立 Prometheus `/metrics` 文本出口的 appservice scheduler 聚合指标，并补入持续积压下的 capacity-limited 轮转证据、自定义阈值持久化基线、默认 `MAX_SERVICES_PER_TICK=8` 的边界命令证据、默认 backlog 阈值的精确单测边界、默认阈值与更保守阈值的对比单测基线，以及对应的 integration 持久化证据；本轮已将默认事件阈值从 `100` 收紧到 `50`，且默认事件阈值 `50`、默认容量边界 `8`、默认 transaction 阈值 `2`、“无统计面时 pending transaction 仍优先”、“持续 transaction backlog 下首轮被限流服务仍会在后续 tick 中轮转获得 dispatch”、“mixed event/transaction backlog 下 transaction 优先级与后续回补 dispatch 仍成立”以及“mixed backlog 叠加 retry backoff 时失败 AS 不会拖慢健康服务”的七条 focused integration 已在恢复后的本地环境中重新通过。
  14. 已完成增强：将 appservice statistics 读面改为实时聚合，并对 `update_last_seen()` 做缺行 `upsert`，修复 `application_service_statistics` 未被持续维护时 admin statistics / telemetry / Prometheus 返回空集或失真的产品缺口；当前三条观测出口已共享同一份更可信的事实来源。
  15. 已完成增强：补齐 telemetry recovery flow、mixed contention 运维计数关系与 Prometheus recovery summary 三条 focused 证据，明确验证“失败一次后恢复成功”不会在外部观测面留下伪 backoff/pending 状态，同时 `failure_count / backoff_count / capacity_limited_count` 在 mixed contention 场景下保持可解释关系。
  16. 已完成增强：补齐“多个 retry_backoff 服务共享恢复窗口后再次争用”、“多个 pending-transaction 服务长期 backlog 且 event-heavy 持续追加事件”与“单个超大 event-heavy 服务混跑轻量 transaction burst”三条下一阶段 focused integration；当前代码侧高负载验证已补齐，基于 event-only、transaction-only、mixed、mixed+backoff、recovery、continuous-ingress 与 super-event-heavy 七类负载证据，暂不继续收紧 `MAX_SERVICES_PER_TICK` / `HIGH_PENDING_TRANSACTION_THRESHOLD` / `HIGH_PENDING_EVENT_THRESHOLD`，后续仅在更长时间窗生产压测或线上反例出现时再调整默认值。
- **责任节点**：协议兼容负责人、应用服务负责人、测试负责人、运维负责人。
- **资源投入**：后端 2~3 人周，QA 1 人周，SRE 0.5 人周。
- **验收标准**：
  - 功能可用性：当前已满足“AS YAML 配置可加载并在启动期导入”、“本地房间事件、联邦入口与多数已识别旁路事件可按 namespace 自动进入 pending queue”、“建房事务内事件可在 commit 成功后统一进入 appservice 分发链路”、“周期 sender/scheduler 可自动尝试发送/重试未完成 transaction”、“真实 `send_message()` message-path 可端到端投递到 mock bridge”、“真实 `m.room.member` invite/join membership 可端到端投递到 mock bridge”、“失败 AS 进入 backoff 时健康 AS 仍可继续出队”、“同一 AS 在未完成 transaction 存在时不会并发创建第二个 transaction”、“命中每轮活跃 AS 上限时可将剩余服务标记为 `capacity_limited` 并持久化 pending/backlog 状态”，以及“scheduler 可持久化 `retry_backoff` / `capacity_limited` 等 transaction 聚合状态与 success/failure/backoff/capacity 计数器，并通过 `/_synapse/admin/v1/appservices/statistics`、`/_synapse/admin/v1/telemetry/metrics` 与独立 `/metrics` 文本出口显式输出聚合 scheduler 视图”；此外，statistics 读面已改为实时聚合，recovery flow、mixed contention、multiple-recovery、continuous-ingress 与 super-event-heavy 的代码侧 focused 证据也已补齐。当前默认 `MAX_SERVICES_PER_TICK=8` / `HIGH_PENDING_TRANSACTION_THRESHOLD=2` / `HIGH_PENDING_EVENT_THRESHOLD=50` 暂无继续收紧或放宽证据，后续主任务正式转为生产压测与阈值观察。
  - 性能指标：单 AS 1000 events/min 压测下，入队到首次发送 p95 < 200ms，1 万事件积压在 5 分钟内消化完毕。
  - 代码质量：新增 route/service/storage/integration 四层测试；关键调度路径具备失败场景测试。
  - 资源利用率：AS 调度器 CPU 常态占用 < 1 核；队列积压时内存增长可控，恢复后 10 分钟内回落到基线 ±15%。

#### P0-02 下一步调优计划

- **目标边界**：后续不再把重点放在“是否具备 scheduler 能力”或“是否补齐代码侧 focused 场景”，而是验证默认 `MAX_SERVICES_PER_TICK=8`、`HIGH_PENDING_EVENT_THRESHOLD=50`、`HIGH_PENDING_TRANSACTION_THRESHOLD=2` 在更贴近生产的长时间窗、burst 恢复和大 backlog 场景下是否仍是合适默认值。
- **已完成的代码侧场景**：本轮已完成三类 focused 证据，分别是“多个 retry_backoff 服务周期性恢复后再次争用 dispatch 槽位”、“多个 pending-transaction 服务长期维持 backlog 且 event-heavy 服务持续追加事件”、“单个超大 event-heavy 服务与多个轻量 transaction 服务混跑时的首次 dispatch 边界”；因此下一步不再优先补同类型代码用例，而改为生产口径复核与压测记录。
- **核心指标**：统一以 `pending_events`、`pending_transactions`、`services_in_backoff`、`services_capacity_limited`、`success/failure/backoff/capacity_limited` 聚合计数，以及 event-heavy 服务的首次重新获得 dispatch 的 tick 数作为调优判断依据；若 Prometheus/telemetry/admin statistics 三个出口出现显著不一致，应先视为观测链问题而非直接调阈值。
- **调参顺序**：默认先观察 `max_services_per_tick` 是否导致长期 `capacity_limited`，再观察 `HIGH_PENDING_TRANSACTION_THRESHOLD` 是否过早把轻量 transaction backlog 提升为 `high`，最后才考虑 `HIGH_PENDING_EVENT_THRESHOLD`；除非出现明确 starvation 反例，否则不建议一次同时改动多个默认值。
- **退出条件**：当 event-only、transaction-only、mixed、mixed+backoff、recovery、continuous-ingress 与 super-event-heavy 七类场景都能在更长时间窗下保持“无长期饥饿、无伪 backoff 残留、聚合出口一致、恢复后 pending 清零”，即可把 `P0-02` 从“实现/验证阶段”转入“生产参数观察阶段”，后续只保留运维监控和偶发反例复盘。

| 后续场景 | 当前关注点 | 建议动作 | 主要观测指标 | 责任节点 | 退出判据 |
|---|---|---|---|---|---|
| 多个 `retry_backoff` 服务周期性恢复后再次争用 | 代码侧已验证恢复服务不会重新饿死 event bucket，但仍缺生产压测下的长窗口恢复曲线 | 保留当前 focused integration 作为基线；后续补压测脚本，记录恢复前后 3~5 个调度窗口 | `services_in_backoff`、`total_backoff_count`、恢复后 `pending_transactions` 归零时间、单服务 `scheduler_transaction_state` | 应用服务负责人 + 测试负责人 | 恢复后健康服务无明显饥饿，失败服务回到 `idle/success/dispatched`，三条观测出口一致 |
| 多个 `pending-transaction` 服务长期 backlog，且 `event-heavy` 服务持续追加事件 | 代码侧已验证 event-heavy 会在 transaction 压力释放后完整回补，但仍缺更长时间窗的生产口径记录 | 保留当前 continuous-ingress focused integration；仅在出现长期 `capacity_limited` 时评估上调 `max_services_per_tick` | `services_capacity_limited`、`total_capacity_limited_count`、`pending_events`、event-heavy 服务首次回补 dispatch 的 tick 数 | 应用服务负责人 + 运维负责人 | event-heavy 服务能稳定回补 dispatch，无长期积压，`capacity_limited` 仅表现为短时竞争而非持续状态 |
| 单个超大 `event-heavy` 服务与多个轻量 transaction 服务混跑 | 代码侧已验证超大 backlog 在首轮 `capacity_limited` 后可于第二个 tick 开始 dispatch，但仍缺尾延迟分布数据 | 以当前 focused integration 作为默认阈值基线；后续在固定 transaction 负载下逐步提升 event-heavy backlog，观察 `normal -> high` 边界是否与真实尾延迟恶化点一致 | `scheduler_backlog_state`、`pending_events`、入队到首次 dispatch 的 p95/p99、`total_success_count` 增长斜率 | 应用服务负责人 + SRE | backlog state 的提升点与真实尾延迟恶化点基本一致，不再出现明显“过早告警”或“过晚告警” |
| 多出口观测一致性复核 | admin statistics / telemetry / Prometheus 是否在相同场景下给出一致聚合结果 | 每次新增高负载或恢复场景时，固定抽样比对三条出口；若不一致，优先修观测链，不先调阈值 | `total_services`、`services_in_backoff`、`services_with_pending_transactions`、`total_pending_events`、`total_pending_transactions` | 运维负责人 + 测试负责人 | 三条出口关键聚合值稳定一致，且与单服务 state 读面可互相印证 |

#### P0-02 生产压测方案

- **目标**：在不改动默认 `MAX_SERVICES_PER_TICK=8`、`HIGH_PENDING_EVENT_THRESHOLD=50`、`HIGH_PENDING_TRANSACTION_THRESHOLD=2` 的前提下，用生产口径压测确认当前默认值是否足以覆盖 event-only、transaction-only、mixed、mixed+backoff、recovery、continuous-ingress 与 super-event-heavy 七类运行时场景。
- **压测环境**：优先使用与生产同版本二进制、同 schema、同 Prometheus 抓取周期和同 appservice scheduler 配置；压测期间固定 `appservice` 数量、bridge 响应模型、房间/成员规模与事件类型配比，避免同时变更多个自变量。
- **执行节奏**：每个场景按 `10 min` 预热、`30 min` 稳态、`10 min` 恢复观察执行；所有场景至少重复 `3` 次，按中位数和最差一次同时记录，避免偶发样本掩盖 starvation/backoff 尾部问题。
- **采样口径**：统一从 `/_synapse/admin/v1/appservices/statistics`、`/_synapse/admin/v1/telemetry/metrics`、Prometheus `/metrics` 三条出口取样，并对至少 `2` 个重点 AS 保留单服务 state 抽样，确保聚合面与单服务状态可互相校验。

| 压测场景 | 负载模型 | 主要目的 | 建议负载级别 | 关键风险 |
|---|---|---|---|---|
| Baseline event-only | `8` 个健康 AS，持续消息/状态事件，无 transaction backlog、无失败重试 | 验证默认 `max_services_per_tick=8` 下常态事件分发吞吐与尾延迟 | `1x` 基线、`2x` 峰值、`4x` 突刺 | 正常负载下提前进入 `capacity_limited` 或 backlog state 过早升高 |
| Transaction-only | `8` 个 AS 均已有 pending transaction，持续重试且无额外事件注入 | 验证默认 `HIGH_PENDING_TRANSACTION_THRESHOLD=2` 的高压分界是否合理 | 每 AS `1/2/4` 个 pending transaction 梯度 | 轻量 transaction backlog 被过早标记 `high`，导致误告警或调度抖动 |
| Mixed steady-state | `4` 个 transaction-heavy AS + `4` 个 event-heavy AS 持续并发 | 验证 transaction 优先级与 event bucket 公平性 | transaction 侧 `1x`，event 侧 `1x/2x/4x` | event-heavy 长时间拿不到 dispatch，形成隐性饥饿 |
| Mixed + backoff | `2` 个失败 AS 周期性进入 backoff，其他健康 AS 持续有事件/transaction | 验证失败 AS 不拖慢健康 AS，且 backoff 状态可恢复 | 失败 AS 注入 `5xx/timeout`，恢复窗口按当前 backoff 策略 | 健康 AS 被失败 AS 连带拖慢，或恢复后出现伪 backoff 残留 |
| Recovery burst | 先积压 `10k` pending events 或 `100+` pending transactions，再恢复 bridge | 验证恢复期 drain 速度与尾延迟回落速度 | 先阻断 `5 min`，后恢复并观察 `10 min` | 恢复后长时间清不掉 backlog，或观测出口未同步回落 |
| Continuous ingress | transaction backlog 未清空期间持续追加 event-heavy 流量 | 验证持续写入下 event bucket 不被永久饿死 | event-only 基线的 `2x/4x` 持续追加 | pending_events 只升不降，event-heavy 首次回补 dispatch 明显超出预期 |
| Super event-heavy | `1` 个超大 event-heavy AS + 多个轻量 transaction AS | 验证默认 `HIGH_PENDING_EVENT_THRESHOLD=50` 是否贴近真实尾延迟恶化点 | 单 AS backlog 逐步抬升到 `50/100/500/1000` | backlog 已显著恶化但仍停留 `normal`，或过早进入 `high` 造成噪声 |

| 观测维度 | 指标来源 | 重点指标 | 采样频率 | 判断用途 |
|---|---|---|---|---|
| 调度聚合 | admin statistics / telemetry / Prometheus | `total_pending_events`、`total_pending_transactions`、`services_in_backoff`、`services_capacity_limited`、`services_with_pending_transactions` | `15s` | 识别 backlog 是否持续积压，判断是否存在长期限流或失败扩散 |
| 调度结果 | admin statistics / Prometheus | `total_success_count`、`total_failure_count`、`total_backoff_count`、`total_capacity_limited_count`、`total_in_flight_count` | `15s` | 判断成功/失败/限流是否符合场景预期，确认是否出现异常抖动 |
| 单服务状态 | admin statistics 单 AS 视图 | `scheduler_transaction_state`、`scheduler_last_result`、`scheduler_backlog_state`、`scheduler_pending_event_count`、`scheduler_pending_transaction_count` | `15s` | 识别单个 AS 是否长期卡在 `retry_backoff`、`capacity_limited` 或错误 backlog state |
| 端到端时延 | 压测客户端 + bridge mock 日志 | 入队到首次 dispatch p50/p95/p99、恢复后 backlog 清零时间、event-heavy 首次重新获得 dispatch 的 tick 数 | `1 min` 汇总 | 判断默认阈值是否贴近真实尾延迟恶化点，是否已出现 starvation |
| 资源利用率 | 主机监控 / 容器监控 | scheduler 所在进程 CPU、RSS、连接池占用、数据库慢查询数、bridge 侧处理耗时 | `15s` | 排除“阈值正常但资源已透支”的假阳性，避免仅看逻辑指标做错误结论 |
| 观测一致性 | 三出口交叉比对 | 同时刻 `total_services`、pending/backoff/capacity 聚合值差异率 | 每轮场景固定抽样 `3` 次 | 确认观测链可信，避免以错误监控数据指导调参 |

| 阈值判断项 | 通过阈值 | 预警阈值 | 失败阈值 | 解释 |
|---|---|---|---|---|
| 入队到首次 dispatch p95 | `<= 200ms` | `200ms - 500ms` | `> 500ms` | 直接沿用当前 `P0-02` 验收口径，并扩展到生产压测分档 |
| 入队到首次 dispatch p99 | `<= 1s` | `1s - 3s` | `> 3s` | 用于识别少量 event-heavy 或恢复窗口下的尾延迟失控 |
| backlog 清零时间 | `10k events <= 5 min` | `5 - 10 min` | `> 10 min` | 用于验证恢复期 drain 能力是否满足当前默认值预期 |
| `services_capacity_limited` 持续时长 | 单次 `< 2 min` | `2 - 10 min` | `> 10 min` | 短时竞争可接受，长时间保持说明 `max_services_per_tick` 偏紧或公平性退化 |
| `services_in_backoff` 恢复后残留 | `0` 残留 | `<= 1` 个采样点残留 | 连续 `> 1 min` 非零 | 用于识别 recovery flow 是否留下伪 backoff/pending 状态 |
| event-heavy 首次重新获得 dispatch | `<= 2 tick` | `3 - 5 tick` | `> 5 tick` | 直接映射当前 focused integration 的公平性边界到生产压测口径 |
| 三出口关键聚合差异率 | `< 5%` | `5% - 10%` | `> 10%` | 超过阈值先判定为观测链问题，不直接做参数调整 |
| scheduler CPU 占用 | `< 1` 核 | `1 - 2` 核 | `> 2` 核或持续升高 | 防止阈值看似合理但调度成本已经不可接受 |
| 内存回落 | 恢复后 `10 min` 回到基线 `+-15%` | `+-15% - +-30%` | 超过 `+-30%` 或不回落 | 识别 backlog drain 后对象/缓存未释放或存在泄漏倾向 |

- **判断顺序**：先看三出口是否一致，再看 `services_capacity_limited` 是否长期非零，再看 `pending_transactions` 与 `pending_events` 是否在恢复窗口内回落，最后才结合 p95/p99 和资源指标判断是否需要调阈值；观测链不一致时，本轮结果不用于调参。
- **调参触发条件**：仅当同一失败模式在至少 `2` 个不同场景、`3` 轮重复中稳定复现，且三出口与单服务 state 均能互证时，才允许提出默认值变更；否则维持当前默认值并记录为待观察现象。
- **建议退出条件**：
  1. 七类场景均完成至少 `3` 轮可重复压测，且观测数据已归档。
  2. 所有场景的三出口关键聚合差异率均小于 `5%`，无观测链阻塞问题。
  3. 无任何场景出现连续 `10 min` 以上的 `capacity_limited` 或 `retry_backoff` 残留。
  4. `10k events` backlog 可在 `5 min` 内清空，恢复后 `10 min` 内内存回到基线 `+-15%`。
  5. event-heavy 服务在 mixed、continuous-ingress、super-event-heavy 三类场景下均能在 `2 tick` 内重新获得 dispatch，未出现稳定 starvation 反例。
  6. 未出现需要同时调整 `max_services_per_tick`、`HIGH_PENDING_TRANSACTION_THRESHOLD`、`HIGH_PENDING_EVENT_THRESHOLD` 的复合故障；若必须联动调参，则 `P0-02` 不退出，继续保留在高优先级治理列表。
- **压测产出物**：每轮压测至少沉淀一份记录，包含场景参数、执行时间窗、三出口截图或导出、单服务状态样本、是否命中阈值、是否建议调参，以及“结论是否可复现”的一句话判断；所有结论必须能回溯到指标图和原始取样。

#### P0-02 压测执行清单

- **执行前准备**：
  1. 固定压测版本：确认 server 二进制、数据库 schema、bridge mock、Prometheus 抓取配置与目标生产版本一致，并记录 git SHA / 镜像 tag / 配置版本。
  2. 固定测试拓扑：记录 AS 数量、exclusive namespace 划分、房间数量、平均房间成员数、消息/状态/transaction 事件配比，避免同一轮压测中途改拓扑。
  3. 固定观察窗口：提前创建 dashboard 或采样脚本，确保 `admin statistics`、`telemetry`、Prometheus 三出口都能按 `15s` 节奏取样。
  4. 固定重点样本：为至少 `2` 个健康 AS、`1` 个 event-heavy AS、`1` 个 transaction-heavy AS、`1` 个故障 AS 预先分配服务编号，后续所有场景都复用同一命名。
  5. 固定保护阈值：预先声明“立即停压”条件，包括数据库连接池耗尽、主进程 OOM 风险、Prometheus 持续抓空、bridge mock 全量不可用、业务主路径 5xx 明显外溢。
- **统一执行步骤**：
  1. 基线采样 `5 min`：先在零或轻负载下采集 CPU、RSS、pending/backoff/capacity 基线，作为后续回落对照。
  2. 预热 `10 min`：逐步把流量抬到目标场景的 `1x` 级别，确认三出口开始稳定出数且关键 AS 状态可读。
  3. 稳态 `30 min`：把流量提升到目标等级并保持稳定，不在稳态窗口内调整任何 scheduler 参数。
  4. 恢复观察 `10 min`：停止新增压测流量或恢复被阻断 bridge，观察 backlog、backoff、capacity 与内存是否按预期回落。
  5. 结果复核 `5 min`：导出三出口样本，抽样核对单服务状态，确认本轮是否存在观测链异常。
- **场景执行提示**：
  1. `Baseline event-only`：只注入消息/状态事件，不人为制造 transaction backlog，用于拿吞吐和尾延迟基线。
  2. `Transaction-only`：先预置 pending transaction，再持续触发重试窗口，避免混入新的 event-heavy 干扰。
  3. `Mixed steady-state`：固定 `4:4` 的 transaction-heavy 与 event-heavy 服务比例，不在场景中途切换角色。
  4. `Mixed + backoff`：失败 AS 只注入可控 `5xx/timeout`，不要同时引入随机网络抖动，否则难以解释 backoff 计数。
  5. `Recovery burst`：阻断和恢复动作必须有明确时间点，确保 backlog 清零时间可精确计算。
  6. `Continuous ingress`：持续追加流量时不要一次性打满，按 `2x -> 4x` 阶梯增加，便于识别 starvation 起点。
  7. `Super event-heavy`：单 AS backlog 按 `50/100/500/1000` 梯度上升，每个台阶至少保持一个完整稳态窗口。

| 执行阶段 | 必做动作 | 必采样本 | 通过判定 | 立即停压条件 |
|---|---|---|---|---|
| 基线采样 | 记录空载或轻载 CPU、RSS、pending/backoff/capacity 基线 | 三出口基线值、重点 AS 初始 state、DB 连接池占用 | 三出口均有稳定输出，重点 AS state 可读 | 任一出口持续无数据 `> 1 min` |
| 预热 | 把流量平滑拉到 `1x`，确认无明显异常 | `pending_events`、`pending_transactions`、`total_in_flight_count`、进程 CPU/RSS | 未出现突发 `retry_backoff`、连接池和慢查询处于可控范围 | 业务主路径 5xx 外溢、连接池接近耗尽 |
| 稳态 | 固定负载运行，不变更配置 | p50/p95/p99、capacity/backoff 计数、重点 AS state、Prometheus 样本 | 指标连续可采且场景风险可解释 | OOM 风险、Prometheus 抓取失败、bridge mock 全量不可用 |
| 恢复观察 | 停止压流或恢复 bridge，观察回落 | backlog 清零时间、backoff 残留、内存回落、重点 AS 状态迁移 | pending/backoff/capacity 在目标窗口内回落 | 恢复后 `5 min` 仍继续恶化 |
| 结果复核 | 导出原始样本，核对三出口一致性 | 三出口导出、单服务 state 样本、压测侧日志摘要 | 可复盘、可解释、可复现 | 样本缺失导致结果不可解释 |

- **停机保护原则**：
  1. 任何一轮压测中，只要出现主进程 RSS 持续单向上升且超过基线 `+30%`、并在 `5 min` 内未回落趋势，即立即停压并保留 heap/监控证据。
  2. 任何一轮压测中，只要数据库连接池使用率长期贴近上限且慢查询持续放大，应先停止新增流量，再区分是 scheduler 问题还是存储瓶颈。
  3. 若 `admin statistics`、`telemetry`、Prometheus 任两条出口同时失真或停止更新，本轮结论直接作废，不再继续叠加更高负载。
  4. 若业务对外接口开始出现明显非 appservice 链路的连带错误，本轮立即终止，避免把 appservice 压测扩散为整站事故。

#### P0-02 压测记录模板

| 字段 | 填写要求 | 示例 |
|---|---|---|
| 场景名称 | 必填，使用矩阵中的标准名称 | `Mixed + backoff` |
| 轮次 | 必填，记录第几次重复 | `Round 2 / 3` |
| 版本信息 | 必填，记录 git SHA / 镜像 tag / 配置版本 | `server abc1234 / bridge mock v2 / config 2026-06-13` |
| 负载参数 | 必填，记录 AS 数、事件速率、pending transaction 梯度、阻断时长 | `8 AS, 4 event-heavy + 4 txn-heavy, 2x event ingress` |
| 时间窗口 | 必填，记录基线/预热/稳态/恢复起止时间 | `09:00-09:05 baseline, 09:05-09:15 warmup ...` |
| 核心结果 | 必填，写 p95/p99、backlog 清零时间、是否出现长期 capacity/backoff | `p95 180ms, p99 820ms, drain 4m20s, no long-lived capacity limit` |
| 三出口一致性 | 必填，写差异率和是否可信 | `max delta 3.2%, trusted` |
| 单服务抽样 | 必填，至少列 2 个健康 AS 和 1 个异常 AS 的 state 结论 | `as-a idle->dispatched, as-b capacity_limited<2tick, as-f retry_backoff recovered` |
| 资源结果 | 必填，写 CPU/RSS/连接池/慢查询摘要 | `CPU 0.7 core, RSS +11%, pool 62%, no slow query burst` |
| 判定结论 | 必填，只能写 `通过 / 预警 / 失败` 之一 | `通过` |
| 是否建议调参 | 必填，只能写 `否 / 观察 / 是` | `观察` |
| 一句话结论 | 必填，供周报直接复用 | `默认 8/2/50 在 mixed+backoff 场景下仍可恢复，无稳定 starvation` |

- **结论口径**：
  1. `通过`：所有关键指标在通过阈值内，且三出口一致、单服务状态可解释，无需调参。
  2. `预警`：出现单项预警阈值，但未跨入失败阈值，且现象可重复；此时只记录观察，不立即改默认值。
  3. `失败`：任一失败阈值命中，或观测链不可信导致无法得出结论；需进入故障复盘或参数评审。
- **周报复用建议**：每周仅汇总“新增完成场景数、通过/预警/失败分布、是否出现新 starvation 反例、是否触发参数评审”四项，避免在周报中重复展开原始样本。

#### P0-02 压测排期表

- **排期目标**：按 `D1 -> D2 -> D3` 从“先拿稳定基线、再验证核心竞争、最后验证恢复与极端场景”推进，避免第一天就把恢复类和 super-event-heavy 场景叠在一起，导致结果难解释。
- **责任分工**：默认由应用服务负责人主导场景执行与结果解释，运维负责人保障环境与监控，测试负责人负责压测流量编排与记录模板落档；若出现跨出口观测不一致，优先转给运维负责人排查观测链。
- **日切条件**：只有当前一天所有必跑场景都完成记录模板、三出口样本归档且没有未定性的 `失败` 结论时，才进入下一天；否则先完成补采样或复跑，不跳天推进。

| 日期 | 执行顺序 | 场景 | 责任人 | 前置条件 | 当日交付物 |
|---|---|---|---|---|---|
| `D1` | `1` | `Baseline event-only` | 应用服务负责人 + 测试负责人 | 压测环境已冻结；三出口 dashboard/采样脚本已验证；重点 AS 编号已固定 | 基线吞吐/时延报告 1 份，三出口基线导出 1 份，`通过/预警/失败` 结论 1 条 |
| `D1` | `2` | `Transaction-only` | 应用服务负责人 + 测试负责人 | `Baseline event-only` 已完成并确认无观测链异常；pending transaction 预置脚本可重复执行 | transaction 阈值观察记录 1 份，pending transaction 梯度样本 1 份，单服务 state 抽样 1 份 |
| `D1` | `3` | `Mixed steady-state` | 应用服务负责人 + 运维负责人 | 前两场已给出可信基线；`4 event-heavy + 4 txn-heavy` 拓扑已固定；资源监控正常 | mixed 常态公平性结论 1 份，capacity/backoff/pending 聚合图 1 组，是否进入 `D2` 的 go/no-go 结论 |
| `D2` | `1` | `Mixed + backoff` | 应用服务负责人 + 运维负责人 | `D1` 无未定性失败；故障 AS 的 `5xx/timeout` 注入方式已验证可控；恢复窗口已预演 | backoff 恢复曲线 1 份，健康 AS 是否被拖慢的结论 1 条，异常样本归档 1 份 |
| `D2` | `2` | `Recovery burst` | 运维负责人 + 测试负责人 | `Mixed + backoff` 已确认不会影响环境稳定性；bridge 阻断/恢复时间点可精确控制 | `10k events` 或 `100+ transactions` drain 报告 1 份，恢复期三出口回落样本 1 份 |
| `D2` | `3` | `Continuous ingress` | 应用服务负责人 + 测试负责人 | `Recovery burst` 后环境已回到基线 `+-15%`；持续追加流量脚本支持 `2x -> 4x` 阶梯抬升 | continuous-ingress starvation 观察记录 1 份，event-heavy 首次回补 dispatch tick 样本 1 份 |
| `D3` | `1` | `Super event-heavy` | 应用服务负责人 + 运维负责人 + 测试负责人 | `D1-D2` 已确认默认值无明显基础缺陷；单 AS backlog 梯度脚本已验证；停压阈值已复核 | `50/100/500/1000` 梯度对比结果 1 份，`HIGH_PENDING_EVENT_THRESHOLD=50` 贴合度判断 1 条 |
| `D3` | `2` | 七场景汇总复盘 | 应用服务负责人 + 运维负责人 + 测试负责人 | 7 个场景模板均已填写完毕；三出口样本与日志已归档；未完成项已标注 | 总结表 1 份，周报摘要 1 份，是否触发参数评审的结论 1 条 |

| 日期 | 通过门槛 | 不通过处理 | 备注 |
|---|---|---|---|
| `D1` | 三个基础场景都拿到可信基线，且无三出口失真、无主链路外溢错误 | 先补采样或复跑 `D1`，不进入 `D2` | `D1` 重点是确认“当前默认值是否具备可测性和可解释性” |
| `D2` | 恢复类和 backoff 类场景可稳定复现，且恢复后 backlog/backoff 能按目标窗口回落 | 暂停 `D3`，先做故障复盘或环境修复 | `D2` 重点是确认“恢复路径是否可信” |
| `D3` | 极端场景结论可解释，且能明确给出“保持默认值 / 进入参数评审”判断 | 保留 `P0-02` 为高优先级，继续追加专项压测 | `D3` 重点是确认“默认阈值是否仍可作为生产默认值” |

- **排期补充说明**：
  1. 若 `D1` 即出现长期 `capacity_limited`、三出口差异率 `> 10%` 或主链路外溢错误，则停止后续排期，优先修环境或观测链。
  2. 若 `D2` 出现 recovery 后 `retry_backoff` 残留或 backlog 无法在目标窗口内清空，则 `D3` 只保留复盘，不做 super-event-heavy 加压。
  3. 若 `D3` 仅出现预警而无失败，可维持默认 `8/2/50`，把现象记录为后续线上观察项；只有稳定复现失败时才触发参数评审。

#### P0-02 压测日报模板

- **使用方式**：`D1`、`D2`、`D3` 每天压测结束后固定产出一份日报；若当天有复跑，也并入同一日报，但要区分 `首轮结果` 与 `复跑结果`，避免只留下最终结论而丢失异常上下文。
- **填报责任**：默认由测试负责人整理原始样本，应用服务负责人给出结论和是否建议调参，运维负责人补充资源与观测链说明；日报发出前至少完成一次三方交叉确认。
- **发送时点**：建议在当日最后一个场景结束后 `30 min` 内发出；若命中停压或失败阈值，可先发 `异常快报`，再补完整日报。

| 字段 | 填写说明 | 示例 |
|---|---|---|
| 日期 | 必填，标注 `D1/D2/D3` 和自然日 | `D2 / 2026-06-14` |
| 今日目标 | 必填，写清当天计划验证的问题 | `验证恢复路径与 backoff 不拖慢健康 AS` |
| 今日场景 | 必填，列出实际执行的场景名和顺序 | `1. Mixed + backoff  2. Recovery burst  3. Continuous ingress` |
| 执行结果 | 必填，逐场景写 `通过/预警/失败` | `Mixed + backoff: 通过；Recovery burst: 预警；Continuous ingress: 通过` |
| 核心指标摘要 | 必填，汇总 p95/p99、drain 时间、capacity/backoff 残留、三出口差异率 | `p95 210ms, p99 1.1s, drain 5m40s, max delta 4.1%` |
| 单服务抽样 | 必填，至少写 2 个健康 AS 和 1 个异常 AS 的状态迁移 | `as-a dispatched stable, as-c capacity_limited<3tick, as-f retry_backoff recovered` |
| 资源与稳定性 | 必填，写 CPU、RSS、连接池、慢查询、是否有主链路外溢 | `CPU 0.9 core, RSS +13%, pool 68%, no slow-query burst, no external spill` |
| 观测链结论 | 必填，只能写 `可信 / 待确认 / 不可信` | `可信` |
| 结论判断 | 必填，只能写 `保持默认值 / 继续观察 / 进入参数评审` | `继续观察` |
| 风险与阻塞 | 选填，写当天未解决问题或需要次日先处理的事项 | `Recovery burst 的 drain 时间逼近预警阈值，需次日复跑确认` |
| 次日计划 | 必填，写明下一天是否继续排期、复跑或转复盘 | `进入 D3，但先复核 D2 的 recovery 样本` |

| 日报结论档位 | 触发条件 | 要求动作 |
|---|---|---|
| `绿灯` | 当天所有场景均为 `通过`，且观测链 `可信` | 直接进入下一天排期，日报中只保留摘要和样本链接 |
| `黄灯` | 存在 `预警`，但无 `失败`，且观测链 `可信` | 进入下一天前先补一轮 focused 复跑或补采样，并在日报中标记观察项 |
| `红灯` | 任一场景 `失败`，或观测链 `不可信` | 暂停后续排期，先发异常快报，再组织复盘或环境修复 |

- **异常快报模板**：
  1. 异常场景：填写具体场景名与时间点。
  2. 异常现象：只写最核心的一条，如“`services_capacity_limited` 连续 `12 min` 非零”或“三出口差异率达到 `18%`”。
  3. 当前影响：说明是否已停止后续压测、是否影响业务主路径。
  4. 临时结论：写“疑似阈值问题 / 疑似观测链问题 / 疑似环境问题”之一。
  5. 下一动作：写“复跑 / 修环境 / 修监控 / 发起参数评审”之一。
- **日报复用建议**：
  1. 飞书/邮件正文只保留“今日目标、执行结果、核心指标摘要、结论判断、次日计划”五块，原始样本放链接。
  2. 若当天只有复跑没有新场景，也要发日报，但标题需显式标注“复跑日”。
  3. `D3` 日报可直接扩展为阶段总结，只需在末尾追加“是否保持默认 `8/2/50`”的最终判断。

#### P0-02 已执行压测与本轮优化结论

- **已执行压测 1：appservice statistics 读面重载烟测**
  - 命令：`cargo test --features 'performance-tests test-utils' --test performance_manual appservice_statistics_load_smoke -- --ignored --nocapture`
  - 负载模型：`512` 个 AS，`event_only / transaction_only / mixed` 三类场景；每个 AS 预置 `256 pending events` 或 `8 pending transactions`；每轮统计读面采样 `20` 次。
  - 基线结果：在优化前，`event_only` 读面已明显高于其余场景，`p50=38ms`、`p95/p99=82ms`；`transaction_only` 为 `p50=7ms`、`p95=9ms`；`mixed` 为 `p50=13ms`、`p95=17ms`。
  - 结论：当前 `ApplicationServiceStorage::get_statistics()` 的热点主要集中在 `pending_event_count` 读面，说明“按 AS 逐条做 pending 计数相关子查询”的成本已经进入可观测区间。
- **本轮已完成优化：appservice statistics 聚合查询改写**
  - 改动点：`synapse-storage/src/application_service.rs::get_statistics()` 已从“对每个 AS 执行 `virtual_user_count` / `pending_event_count` / `pending_transaction_count` 相关子查询”改为 `WITH user_counts / pending_event_counts / pending_transaction_counts` 预聚合后统一 `LEFT JOIN`。
  - 优化结果：同一组压测样本复测后，`event_only` 降至 `p50=16ms`、`p95/p99=39ms`，尾延迟约下降一半；`mixed` 为 `p50=12ms`、`p95=14ms`；`transaction_only` 为 `p50=13ms`、`p95=16ms`，虽略有回升但绝对值仍低，未构成新的主瓶颈。
  - 影响判断：由于 admin statistics、telemetry metrics 与 Prometheus 三条观测出口共享该统计读面，这次优化直接降低了 `P0-02` 观测链在高 event backlog 下的读取成本。
- **已执行压测 2：scheduler mixed backlog 排空烟测**
  - 命令：`cargo test --features 'performance-tests test-utils' --test performance_manual appservice_scheduler_mixed_backlog_load_smoke -- --ignored --nocapture`
  - 负载模型：`16` 个 transaction-heavy AS + `16` 个 event-heavy AS，共 `32` 个 AS；每个 transaction-heavy AS 预置 `2 pending transactions`，每个 event-heavy AS 预置 `60 pending events`；scheduler 参数固定为 `max_events_per_txn=50`、`max_services_per_tick=8`、`HIGH_PENDING_EVENT_THRESHOLD=50`、`HIGH_PENDING_TRANSACTION_THRESHOLD=2`。
  - 结果：总共完成 `64` 次 dispatch，请求数与预期完全一致；总排空耗时 `696ms`，`tick_p50=61ms`、`tick_p95/p99=125ms`，总共在 `9 ticks` 内排空。
  - 调度节奏：累计请求数按 tick 分布为 `[8, 16, 24, 32, 40, 48, 56, 62, 64]`，前 `7` 个 tick 均保持满额放行，最后 `2` 个 tick 进入尾部批次收敛；当前更像是 mixed backlog 尾批次自然收敛现象，而非新的结构性阻塞。
- **当前结论**
  - 需要立即优化的热点已经确认并完成收敛：`get_statistics()` 的 pending 聚合读面。
  - `scheduler` 主链路在 mixed backlog 压测下没有出现异常饥饿或明显的容量塌陷，当前默认 `8/2/50` 暂无立即调整证据。
  - 下一步压测重点应从“继续优化 statistics 读面”转为“拉长时间窗，继续验证 recovery / mixed+backoff / continuous-ingress 的生产口径稳定性”。

#### P0-02 周报口径摘要

- 本周已完成 appservice scheduler 在 admin statistics、telemetry metrics 与 Prometheus 三条观测出口的统一接通，并补齐 focused 验证。
- 本周已修复 `ApplicationServiceStorage::get_statistics()` 依赖未持续维护统计表导致的空集/失真问题，当前三条观测出口统一基于实时聚合工作。
- 本周已补齐“失败一次后恢复成功”的 recovery flow 证据，确认恢复后不会在外部观测面残留伪 backoff/pending 状态。
- 本周已补齐“retry_backoff 服务 + 健康 pending-transaction 服务 + 多个 event-heavy 服务”的 mixed contention 运维计数关系验证，当前聚合计数与实际调度语义一致。
- 本周已补齐“多个 retry_backoff 服务共享恢复窗口后再次争用”、“多个 pending-transaction 长时间 backlog 叠加 event-heavy 持续追加事件”与“单个超大 event-heavy 混跑多个轻量 transaction burst”三条 focused integration，代码侧下一阶段验证已完成闭环。
- 当前 `P0-02` 已从“功能实现缺口”进一步收敛到“生产压测口径下的阈值调优与高负载治理”，默认 `MAX_SERVICES_PER_TICK=8`、`HIGH_PENDING_EVENT_THRESHOLD=50`、`HIGH_PENDING_TRANSACTION_THRESHOLD=2` 暂无立即调整证据。
- 下一步重点不再是继续补同类代码用例，而是围绕更长时间窗、生产压测和三出口一致性做参数观察与反例复核。

#### P0-02 一页式汇报稿

| 已完成 | 当前判断 | 下一步 |
|---|---|---|
| 已完成 appservice scheduler 在 admin statistics、telemetry metrics、Prometheus 三条观测出口的统一接通，并补齐 focused 验证。 | `P0-02` 已不再是基础功能缺失问题，当前主矛盾已收敛为生产压测口径下的阈值调优与更细高负载治理。 | 继续围绕更长时间窗、生产压测与三出口一致性做参数观察，而非重复补同类代码用例。 |
| 已修复 `ApplicationServiceStorage::get_statistics()` 依赖未持续维护统计表导致的空集/失真问题，当前观测面统一基于实时聚合。 | 默认 `MAX_SERVICES_PER_TICK=8`、`HIGH_PENDING_EVENT_THRESHOLD=50`、`HIGH_PENDING_TRANSACTION_THRESHOLD=2` 目前没有立即调整证据。 | 优先观察长期 `capacity_limited` 是否持续存在，再决定是否上调 `max_services_per_tick`；除非出现明确 starvation 反例，不同时调整多个默认值。 |
| 已补齐 recovery flow、mixed contention，以及 multiple-recovery / continuous-ingress / super-event-heavy 三类下一阶段运行时证据，确认恢复后不会残留伪 backoff/pending 状态，且高负载代码侧边界已具备 focused 基线。 | 当前外部观测链已基本可信，后续若 admin statistics / telemetry / Prometheus 出现不一致，应优先视为观测链问题而非直接调阈值。 | 每次新增压测或线上反例时，固定抽样比对三条出口与单服务 state 读面，确保后续调优建立在可信观测基础上。 |

### P1-03 root/canonical 双轨镜像仍是主债务，overlap ledger 与 CI 入口已完成首版落地

- **当前验证**：`python3 scripts/ci/check_root_canonical_ledger.py` 当前报告 `src/services` 与 `synapse-services/src` 存在 110 个递归重叠文件，其中 root 侧最新分类为 `68 thin_facade / 42 full_impl`；`src/storage` 与 `synapse-storage/src` 存在 58 个递归重叠文件，当前均被识别为 thin facade；`src/services/mod.rs` 仍保留 `#![allow(ambiguous_glob_reexports)]`，但 `pub use crate::storage::*` 已移除，且 ledger 脚本会对该禁线做硬校验；`scripts/ci_backend_validation.sh` 已在 Rust 检查入口先执行该脚本。最近几轮已先后将 `src/services/beacon_service.rs`、`src/services/burn_after_read_service.rs`、`src/services/matrix_ai_connection_service.rs`、`src/services/application_service.rs`、`src/services/admin_audit_service.rs`、`src/services/registration_token_service.rs`、`src/services/relations_service.rs`、`src/services/server_notification_service.rs`、`src/services/admin_federation_service.rs`、`src/services/federation_blacklist_service.rs`、`src/services/media_quota_service.rs` 的 root 侧重复测试模块删除并收口为纯 facade，将 `src/services/auth/password_policy.rs` 从 root 整文件实现正式收口为 canonical facade，进一步把 `src/services/database_initializer/` 整组模块收口为 facade，并继续将 `src/services/geo_ip/models.rs`、`src/services/geo_ip/mod.rs`、`src/services/geo_ip/service.rs`、`src/services/content_scanner/models.rs`、`src/services/content_scanner/service.rs`、`src/services/content_scanner/mod.rs`、`src/services/identity/models.rs`、`src/services/identity/storage.rs`、`src/services/identity/service.rs`、`src/services/identity/mod.rs`、`src/services/push/providers/mod.rs`、`src/services/push/service.rs`、`src/services/push/mod.rs`、`src/services/push/gateway.rs`、`src/services/push/queue.rs`、`src/services/push/providers/apns.rs`、`src/services/push/providers/fcm.rs`、`src/services/push/providers/webpush.rs`、`src/services/webhook_notification/`（整组 3 文件）、`src/services/rtc/`（整组 6 文件）收口为 canonical facade；`push` 整组（mod、service、providers、gateway、queue、apns、fcm、webpush）已全部完成 facade 收口，`webhook_notification` 与 `rtc` 两组也已整体收口。按 `src/services/*.rs` 复扫后，顶层单文件 facade 的 root 重复测试 lane 已基本清空。
- **复现步骤**：执行 `python3 scripts/ci/check_root_canonical_ledger.py`；查看 `src/services/mod.rs` 与 `scripts/ci_backend_validation.sh`。
- **影响范围**：分层治理、编译速度、IDE 索引、API 边界稳定性、代码评审成本。
- **发生场景**：迁移 facade、修改公共类型、排查编译错误、查找唯一实现来源时。
- **优化方案**：建立“单一事实来源”治理账本，把 root 明确定位为 facade/兼容层；禁止重新引入 storage glob re-export；按模块批次减少双实现文件与隐式依赖；并将 facade/canonical 间的 DTO、构造参数与责任矩阵收口到同一套治理口径和门禁，避免模块级迁移完成后又在边界层重新分叉。
- **实施步骤**：
  1. 已完成首版：固化“禁止恢复 `pub use crate::storage::*`”规则，`scripts/ci/check_root_canonical_ledger.py` 会对该禁线做硬校验。
  2. 已完成首版：为重叠文件建立脚本化 ledger，当前至少能稳定输出 `thin_facade` / `full_impl` 两类初版分类与重叠清单样本。
  3. 进行中：继续把剩余服务内部对 storage 的隐式依赖改为显式依赖，并按模块收口真实实现文件；最近几轮已先后拿 14 个顶层单文件 service 模块做 facade 净化，将 `auth/password_policy.rs` 从 root 整文件实现正式收口为 canonical facade，进一步完成 `database_initializer` 模块级收口，并继续将 `geo_ip/models.rs`、`geo_ip/mod.rs`、`geo_ip/service.rs`、`content_scanner/models.rs`、`content_scanner/service.rs`、`content_scanner/mod.rs`、`identity/models.rs`、`identity/storage.rs`、`identity/service.rs`、`identity/mod.rs`、`push/providers/mod.rs`、`push/service.rs`、`push/mod.rs`、`push/gateway.rs`、`push/queue.rs`、`push/providers/apns.rs`、`push/providers/fcm.rs`、`push/providers/webpush.rs` 收口为 canonical facade，使 ledger 中的 `services full_impl` 由 `73` 下降到 `51`。其中 `identity/storage.rs` 这一步还顺手修正了 canonical 对 `user_threepids.validated_at` 的旧列名漂移，`geo_ip/service.rs` 这一步消除了 canonical `lookup_maxmind()` 的残留 `todo!()`，`push/providers/mod.rs` 这一步把公共 retry helper 与基础测试一起下沉到了 canonical 单一事实来源，`push/service.rs` 这一步进一步把实际 provider 发送路径统一到 canonical `send_with_retry(...)` 实现，`push/mod.rs` 这一步把整组公共导出也收束到了 canonical，`push/gateway.rs` 这一步继续把 gateway DTO 与发送逻辑转交给 canonical 单一实现，`push/queue.rs` 与 `push/providers/{apns,fcm,webpush}.rs` 这最后四步将 `push` 整组完全收口。最近一轮进一步将 `webhook_notification/`（mod、models、service 共 3 文件）和 `rtc/`（mod、call、infra、sfu、metrics、session 共 6 文件）两组整体收口为 canonical facade，并修正 `container.rs` 中 `RtcSessionService::new` 的 `CacheManager` 类型转换（`cache.to_synapse_cache_manager()`），使 `services full_impl` 由 `51` 进一步下降到 `42`。当前 `identity/`、`content_scanner/`、`geo_ip/` 三组都已整体转为 facade 导出，`push/`、`webhook_notification/`、`rtc/` 三组全部文件也已完成收口；本轮还明确将 `admin_user_service` 一类已完成 direct SQL 下沉的模块，其后续 DTO、构造参数与 root/canonical 责任矩阵治理并轨到 `P1-03` 统一账本。下一步继续优先复扫 `friend_room_service/` 或 `media/` 等相邻候选，并同步检查 facade/canonical 的导出 DTO、构造参数和 owner 边界是否仍存在双轨残留，判断是否还能沿同一低风险 lane 继续收口。
  4. 已完成首版：`scripts/ci_backend_validation.sh` 已接入 overlap ledger 脚本，Rust 检查前会先输出当前重叠文件数与 facade/full_impl 统计。
- **责任节点**：架构负责人、各域模块 owner、CI 负责人。
- **资源投入**：后端 2 人周，CI/工具 0.5 人周。
- **验收标准**：
  - 功能可用性：迁移后原有 API/route 行为无回归。
  - 性能指标：`cargo check` 增量构建时间较当前下降 15% 以上。
  - 代码质量：`src/services/mod.rs` 中不再出现 `pub use crate::storage::*`，且不再新增 `crate::services::*` 间接消费 storage 的调用；当前 `services` 递归重叠基线已被脚本化固定为 `110`，其中 `42` 个仍为 full_impl，后续迭代应持续下降；新增或迁移模块不得重新引入 root/canonical 双轨 DTO、构造参数漂移或责任归属不清的问题。
  - 资源利用率：IDE 索引时间与构建缓存体积较当前下降 10% 以上。

### P1-04 `admin_user_service` direct SQL 下沉已完成，后续仅剩边界固化类收尾

- **当前状态**：**已完成**
- **当前验证**：`synapse-services/src/admin_user_service.rs` 已升级为真实 canonical 实现，`src/services/admin_user_service.rs` 已收口为 facade re-export；其中 `list_users_v2` 的查询已下沉到 `synapse-storage/src/user.rs::list_users(...)`，随后又将 `create_or_update_user_v2` 中的停用状态更新、`user_type` 更新、`batch_deactivate_users` 的停用写入、`get_user_stats()` 与 `get_single_user_stats()` 的统计查询一并收口到 `UserStorage::set_deactivation_status(...)` / `set_user_type(...)` / `get_user_stats_summary(...)` / `count_sent_messages(...)`。当前 `AdminUserDetails.user` 与 `AdminSingleUserStats.user` 也已统一收口为 service DTO `AdminUserProfile`；`synapse-services/src/admin_user_service.rs` 已无 `sqlx::query` / `sqlx::query_scalar` / `QueryBuilder` 残留，并通过 `cargo check --locked` 与 `super_admin_can_update_user_v2_deactivation_and_role_fields` 集成测试验证。
- **复现步骤**：查看 `synapse-services/src/admin_user_service.rs` 中相关结构体与 SQL 调用点。
- **影响范围**：管理员用户管理接口、服务层测试、后续 DTO/字段变更。
- **发生场景**：批量创建/停用用户、用户列表分页、管理员查询单用户详情与统计。
- **优化方案**：在已完成 canonical 化与 direct SQL 下沉的基础上，将剩余 DTO/构造参数/责任矩阵治理正式并入 `P1-03` 的 root/canonical 统一账本与门禁，避免为单模块重复维护迁移账本。
- **实施步骤**：
  1. 已完成一轮：为 admin user API 增加回归测试，固定超级管理员场景下 `deactivated` / `user_type` 更新行为。
  2. 已完成两轮：用户列表查询、停用状态更新、`user_type` 更新、批量停用、用户统计与单用户消息统计均已下沉到 `synapse-storage`。
  3. 已并轨：root facade 与 canonical service 的 DTO/构造参数/责任矩阵治理已纳入 `P1-03` 统一推进，后续不再单列 `P1-04` 子账本。
- **责任节点**：管理后台负责人、存储层负责人、测试负责人。
- **资源投入**：后端 1.5 人周，QA 0.5 人周。
- **验收标准**：
  - 功能可用性：管理员用户 CRUD、分页、批量操作接口行为与当前兼容。
  - 性能指标：用户列表、用户统计接口 p95 不高于当前 10%；SQL 查询次数不增加。
  - 代码质量：root `admin_user_service.rs` 保持 facade；canonical `admin_user_service.rs` direct SQL 为 0；后续 DTO/构造参数/责任矩阵治理统一按 `P1-03` 的 ledger 与门禁推进。
  - 资源利用率：批量操作内存占用与连接池峰值不高于当前实现。

### P1-05 `ServiceContainer` 兼容层形成双访问面模块冗余

- **当前状态**：**已完成**。通过连续五轮迁移，并在本轮补齐 `src/web/routes/state.rs` 与 `src/web/middleware/csrf.rs` 中的最后残余兼容访问，已将 `src` 和 `tests` 目录中所有对 `services.config`、`services.server_name`、`services.device_storage` 等典型 `legacy` 扁平字段的引用全部切换到 `core` / `account` / `sso` / `extensions` grouped view。当前代码库中，这些 `legacy` 字段已无直接消费点，为下一步在 `ServiceContainer` 中移除这些字段、彻底消除双访问面冗余铺平了道路。
- **复现步骤**：查看 `src/services/container.rs` 的字段定义即可复现。
- **影响范围**：新代码接入路径、调用风格统一、后续容器收敛成本。
- **发生场景**：新增路由/服务注入、重构服务调用、从 root 迁移到 canonical 访问面时。
- **优化方案**：明确 grouped view 为正式访问面，legacy 扁平字段进入受控退役期；按路由域逐批迁移。
- **实施步骤**：
  1. 已完成首轮盘点：定位 `server.rs`、`admin/notification.rs`、`versions.rs`、`admin/retention.rs`、`sliding_sync.rs` 等仍消费 legacy 扁平字段的稳定入口。
  2. 已完成四轮迁移：新增收口代码优先使用 `core` / `account` / `sso` / `extensions` grouped view，并已覆盖 SSO、sync、media、admin register、verification、key rotation、联邦事务/成员/keys 与路由装配入口。
  3. 本轮验证：`cargo check --locked`、`cargo test --features test-utils --test integration --no-run --locked` 均通过；同时对 `src/web/routes` 运行针对 `services.config`、`services.server_name`、`services.registration_service`、`services.oidc_service`、`services.builtin_oidc_provider`、`services.saml_service`、`services.saml_storage`、`services.device_storage`、`services.event_broadcaster` 的 grep 检查，当前为 0 命中。
  4. 已完成补遗：`src/web/routes/state.rs` 已切换到 `services.core.config` grouped view，`src/web/middleware/csrf.rs` 测试中的 `services.server_name` 兼容赋值已移除。
  5. 本轮验证：对 `src` 与 `tests` 执行 `services.config`、`services.server_name`、`services.registration_service`、`services.oidc_service`、`services.builtin_oidc_provider`、`services.saml_service`、`services.saml_storage`、`services.device_storage`、`services.event_broadcaster` 的 grep 检查，当前均为 0 命中。
  6. 后续收口方向：在 `ServiceContainer` 兼容层正式退役前，为 legacy 扁平字段删除补 grep/编译门禁，并制定分阶段删除顺序。
- **责任节点**：架构负责人、web 路由负责人、服务层负责人。
- **资源投入**：后端 1~2 人周。
- **验收标准**：
  - 功能可用性：容器迁移后所有现有路由可启动、可回归。
  - 性能指标：服务装配时间与启动耗时不高于当前。
  - 代码质量：legacy 扁平字段消费面在两轮迭代内下降 80%；新增代码零扁平字段引用。
  - 资源利用率：不引入重复服务实例；启动后对象数量与内存占用保持稳定。

### P1-06 Matrix surface 文档与能力声明治理仍不闭环

- **当前验证**：`SUPPORTED_MATRIX_SURFACE.md` 的 room version 矩阵已修正为与代码一致的 `1..13`，并补入当前证据来源；`tests/integration/api_auth_routes_tests.rs` 已新增 `/versions` 与公开 `/capabilities` 的合约测试，校验 room version 常量、默认版本与未认证公开能力边界；`m.change_password` / `m.set_displayname` / `m.set_avatar_url` / `m.3pid_changes` 已从裸常量插入收敛为 `versions.rs` 中集中具名真值函数，`/_matrix/federation/v1/query/destination` 已复用同一 `m.change_password` 真值来源，`src/web/api_doc.rs` 的 capability 示例也已同步到当前 room version 矩阵；前一轮已把 `m.room.summary` / `m.room.suggested` / `m.voice` / `m.thread` / `io.hula.sliding_sync` 以及 `org.matrix.msc3245.voice` / `org.matrix.msc3983.thread` / `org.matrix.msc3886.sliding_sync`、`org.matrix.msc4261.widget`、`io.hula.friends`、`io.hula.burn_after_read` 收敛到共享 helper 真值源；本轮进一步把这些 helper 显式细分为“配置控制 / 路由存在性 / 静态稳定”三类：`m.sso`、`openclaw`、`ai_connection` 归入配置控制，`m.room.summary`、`m.voice`、`m.thread`、`io.hula.sliding_sync`、`external_services`、`io.hula.voice_extended`、`io.hula.widget`、`io.hula.burn_after_read`、`io.hula.friends` 等归入路由存在性，`m.room.suggested` 作为静态稳定能力显式保留；同时新增单元测试直接断言 capability governance 分类。本轮已将 `m.change_password` / `m.set_displayname` / `m.set_avatar_url` / `m.3pid_changes` 从裸 `bool` 收口为 `CapabilityFlag::static_stable(true).enabled()`，所有 17 个 capability 已全部归入三类治理，无遗留裸布尔值。
- **复现步骤**：查看 `src/common/room_versions.rs` 与 `src/web/routes/handlers/versions.rs`。
- **影响范围**：客户端兼容预期、联邦行为说明、协议声明可信度。
- **发生场景**：客户端依据 `/versions` 与 `/capabilities` 判定支持面、对外对标 Synapse 时。
- **优化方案**：把协议声明治理收敛到“代码常量 + route ledger + contract test + 文档生成”闭环。
- **实施步骤**：
  1. 已完成：修正文档中的 room version 支持矩阵，并补入 `SUPPORTED_ROOM_VERSIONS` / `client_room_versions_capability()` / integration contract test 的证据链。
  2. 已完成首轮：对一组稳定 capability 做集中具名真值收口，并让 federation discovery 与 API 文档示例复用同一声明事实。
  3. 已完成第二轮：将 room summary / suggested / voice / thread / sliding sync 与相关 unstable features 收敛为共享 helper 真值源，并补入 integration 合约断言。
  4. 已完成第三轮：把 capability helper 显式分类到“配置控制 / 路由存在性 / 静态稳定”三类，新增单元测试直接断言 governance 分类，确保 `m.sso`、`openclaw`、`ai_connection` 与 `external_services` / `widget` / `friends` / `burn_after_read` 等能力的声明方式可审计。
  5. 已完成：全部 17 个 capability 已归入三类治理（ConfigControlled: 3 / RouteSurface: 9 / StaticStable: 5），`m.change_password` 等 4 个旧裸 bool 已收口为 `CapabilityFlag::static_stable`；后续可将支持面文档收敛为由代码常量或验证脚本生成。
- **责任节点**：协议兼容负责人、文档负责人、测试负责人。
- **资源投入**：后端 1 人周，QA 0.5 人周。
- **验收标准**：
  - 功能可用性：`/versions`、`/capabilities`、federation room version 响应互相一致。
  - 性能指标：协议面接口 p95 不高于当前；不因动态生成能力而增加明显 CPU/分配开销。
  - 代码质量：新增 contract test 覆盖 room versions 与 capability matrix；文档与代码差异清零。
  - 资源利用率：协议面验证脚本总执行时间 < 2 分钟，可纳入 CI。

### P1-10 注册验证码 email/sms 投递链路仍是运行时 stub

- **当前状态**：**真实存在**
- **当前验证**：`synapse-services/src/captcha_service.rs` 中 `send_captcha_via_provider(...)` 会在 `email` / `sms` 分支分别调用 `send_email(...)` / `send_sms(...)`；这两个函数当前仅打 `warn!` 后直接 `todo!()`，一旦启用对应验证码通道会在运行时触发 panic。上游 Synapse 则至少提供了明确的 `CAPTCHA_SETUP.md` 配置与可运营文档，不会把配置面暴露为无实现 stub。
- **影响范围**：注册/找回密码验证码链路、运营接入、灰度环境验证、管理员误开配置后的稳定性。
- **发生场景**：开启 email captcha、sms captcha、联调模板与第三方提供商、测试环境冒烟时。
- **优化方案**：把验证码通道从“存储 + 模板已就绪、发送实现缺失”改为真正可运营的 provider 抽象；在 provider 未配置时返回显式 `ApiError`，而不是 panic。
- **实施步骤**：
  1. 补齐 `CaptchaDeliveryProvider` 抽象，至少支持 email provider，并为 sms provider 留出明确的 no-op/error 边界。
  2. 将 `send_email(...)` / `send_sms(...)` 从 `todo!()` 改为可预期失败路径，确保未配置 provider 时返回 `501/400` 风格错误而非进程 panic。
  3. 增加 focused integration，覆盖“未配置 provider 时显式报错”“已配置 provider 时成功写审计/发送记录”两条链路。
- **责任节点**：账号注册负责人、通知/基础设施负责人、测试负责人。
- **资源投入**：后端 1 人周，测试 0.5 人周。
- **验收标准**：
  - 功能可用性：启用 email/sms captcha 时不发生 panic，未配置 provider 时返回可解释错误。
  - 性能指标：验证码发送链路 p95 不高于当前同类通知链路 10%。
  - 代码质量：生产代码中不再存在 `captcha_service` 相关 `todo!()` 发送 stub。
  - 资源利用率：发送失败不导致无界重试或任务堆积。

### P1-11 SQLX 离线缓存门禁与仓库基线已漂移

- **当前状态**：**真实存在**
- **当前验证**：`bash scripts/ci/check_sqlx_offline_cache.sh` 当前直接失败，错误为“`.sqlx/ 没有任何 query 缓存`”；而 `docs/synapse-rust/M3_SQLX_MIGRATION_PLAN.md` 与 `M3_BATCH1_EXECUTION_PLAN.md` 仍把“.sqlx 缓存必须入仓”“`cargo sqlx prepare --check` 应纳入 CI”作为既定基线，说明门禁脚本、M3 文档与当前仓库状态存在明显漂移。
- **影响范围**：SQL 编译期校验可信度、CI/本地门禁、schema 漂移发现能力、后续 SQL 宏迁移排期。
- **发生场景**：运行 SQLX 离线检查、准备发布、回归迁移脚本、继续推进 M3 SQL 宏迁移时。
- **优化方案**：在“继续维护 `.sqlx/` 入仓基线”与“正式退役该门禁并同步文档”之间做一次明确决策，避免门禁永远处于半启用状态。
- **实施步骤**：
  1. 先确认当前仓库是否仍以 `.sqlx/` 入仓为正式策略；若是，则基于 v10 schema 重新生成缓存并恢复 CI 调用。
  2. 若已放弃 `.sqlx/` 入仓策略，则删除/降级 `check_sqlx_offline_cache.sh`，并同步修正文档与实施计划中的既有承诺。
  3. 无论选择哪条路线，都需要把“最后验证命令 + 验证日期 + 当前基线文件数/策略”回写到 M3 文档与综合审计报告。
- **责任节点**：数据库负责人、平台/CI 负责人、文档负责人。
- **资源投入**：后端/平台 1 人周。
- **验收标准**：
  - 功能可用性：开发者可以明确知道当前 SQLX 校验依赖的是离线缓存还是在线 schema。
  - 性能指标：对应门禁在 CI 中稳定可复现，不出现“文档要求存在、仓库实际为空”的反向漂移。
  - 代码质量：M3 文档、CI 脚本、仓库 `.sqlx/` 状态三者一致。
  - 资源利用率：不引入重复门禁或无效缓存维护成本。

### P1-12 worker/replication 与上游 Synapse 的可运营模型仍未闭环

- **当前状态**：**真实存在**
- **当前验证**：上游 `element-hq/synapse` 在 `docs/workers.md` 中明确把 worker 扩展建立在 `instance_map`、HTTP replication listener、Redis replication/pub-sub、worker-specific config 与反向代理路由分工之上；当前仓库虽已有 `src/bin/synapse_worker.rs`、Redis task queue、若干 worker/replication 入口与 metrics，但 `synapse_worker` 仍主要表现为通用后台作业消费者，没有形成按路由/职责切分的 worker ownership、实例映射与运维配置闭环。
- **影响范围**：大规模部署、读写分离/单写多读扩展、跨进程状态一致性、运维接入与故障定位。
- **发生场景**：尝试横向扩容、拆分 sync/federation/appservice/后台任务职责、设计高可用部署拓扑时。
- **优化方案**：不要把“存在 worker 二进制”视为已具备 Synapse 式 worker 架构；需要把 worker 类型、路由归属、replication 流、缓存失效传播与部署文档联成一套可执行模型。
- **实施步骤**：
  1. 先定义 worker responsibility matrix，明确哪些路由/后台流量可以独立下沉为 worker，哪些必须留在主进程。
  2. 补齐实例映射、listener/routing、replication stream 与缓存失效广播的配置/文档闭环，并给出最小可运行拓扑。
  3. 增加 focused integration 或部署级 smoke test，验证多实例下的状态同步、任务消费与 metrics 观测一致性。
- **责任节点**：架构负责人、平台负责人、运维负责人。
- **资源投入**：后端/平台 2 人周，运维 1 人周。
- **验收标准**：
  - 功能可用性：至少一类高价值 worker 流量可独立部署并保持行为一致。
  - 性能指标：多实例部署后目标路由/任务链路具备可观测的吞吐提升或主进程减压效果。
  - 代码质量：worker responsibility matrix、配置样例、运行时验证脚本形成闭环。
  - 资源利用率：横向扩展不引入不可解释的重复消费、缓存雪崩或状态漂移。

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

- **当前验证**：`cargo tree -d --workspace` 已确认重复依赖。本轮已消除 `base64 v0.21.7`（通过禁用 `config` 的 `ron` feature，同时连带消除 `toml_datetime`/`toml_edit`/`winnow` 重复）。剩余重复版本：`getrandom`（3 版本）、`hashbrown`（3 版本）、`itertools`（3 版本）、`rand`（3 版本）、`prost`（2 版本）、`core-foundation`（2 版本）、`hashlink`（2 版本）、`nom`（2 版本）、`socket2`（2 版本），均为深层传递依赖，需等待上游 crate 版本升级。
- **复现步骤**：执行 `cargo tree -d --workspace`。
- **影响范围**：构建体积、编译时间、安全升级、许可证治理。
- **发生场景**：依赖升级、供应链审计、二进制体积优化时。
- **优化方案**：建立重复依赖白名单/整改清单，优先清理低风险重复版本，无法统一时记录接受理由。
- **实施步骤**：
  1. ✅ 已完成：生成完整重复依赖清单并按可升级性分级；已消除 `base64 v0.21.7`（禁用 `config` 的 `ron` feature，同时消除 `toml_datetime`/`toml_edit`/`winnow` 重复）。
  2. ✅ 已完成：`cargo update` 统一 101 个兼容依赖（dashmap 6.1.0→6.2.1, axum 0.8.8→0.8.9, h2 0.4.13→0.4.14, hyper 1.8.1→1.10.1 等）。
  3. ✅ 已完成：剩余 9 组深层重复依赖（hashbrown/getrandom/rand/prost/itertools 等）均为 SemVer 不兼容版本，无法通过 `[patch.crates-io]` 强制统一，已建立 `DEPENDENCY_UPGRADE_TRACKER.md` 白名单与上游跟踪。
  4. ✅ 已完成：`cargo tree -d --workspace` 纳入定期供应链检查，记录于 DEPENDENCY_UPGRADE_TRACKER.md。
- **责任节点**：平台负责人、依赖治理负责人、安全负责人。
- **资源投入**：后端/平台 1 人周。
- **验收标准**：
  - 功能可用性：依赖清理不引入行为回归。
  - 性能指标：`cargo build` 或 `cargo check` 全量时间下降 5% 以上。
  - 代码质量：重复依赖清单中可统一项完成率 > 80%，剩余项有白名单说明。
  - 资源利用率：构建缓存体积或产物体积较当前下降 3% 以上。

---

## 七、实施路线图

### 7.1 当前未完成任务执行顺序（按优先级与依赖）

1. **核心安全风险修复**
   - 当前审计范围内已无新增“未闭合且可直接复现”的 appservice 核心安全阻断项；后续若触及 auth/token/admin 写入面，再按变更点补单元测试与 focused 回归，不额外并行开新安全整改支线。
2. **功能缺陷修复：P0-02 剩余 appservice 阈值调优与高负载治理缺口**
   - message-path / membership 两条 bridge e2e、virtual user / namespace exclusivity / 管理面显式写入边界、transaction controller / per-AS 调度策略首版、容量限流、transaction 聚合状态/内部指标面，以及 admin statistics/telemetry/Prometheus 三条聚合出口都已落地；本轮又补齐了“统计面缺失时回退 live counts 后，pending transaction 仍优先”、“持续 transaction backlog 下被限流服务可轮转回 dispatch”、“mixed event/transaction backlog 下 event-heavy 服务在 transaction 压力缓解后可回补 dispatch”、“mixed backlog 叠加 retry backoff 时失败 AS 不会拖慢健康服务”，以及“实时聚合 statistics 读面 + telemetry recovery flow + mixed contention 运维计数关系 + Prometheus recovery summary”的 focused 证据。基于当前证据，默认 `MAX_SERVICES_PER_TICK=8` / `HIGH_PENDING_TRANSACTION_THRESHOLD=2` 暂维持不变；当前剩余最高优先级已收敛为补更细的高负载治理验证与生产压测阈值调优，而不是继续立即调参。
3. **功能缺陷修复：P1-10 注册验证码 provider 运行时闭环**
   - `captcha_service` 的模板/存储层已存在，但 email/sms 发送函数仍为 `todo!()`；应优先消除“配置可开但一调用就 panic”的运行时缺口，再讨论更细的通知编排。
4. **门禁修复：P1-11 SQLX 离线缓存策略与脚本基线统一**
   - 当前 `check_sqlx_offline_cache.sh` 失败，说明 `.sqlx/` 入仓策略、门禁脚本与文档承诺三者不一致；需要尽快决定“恢复离线缓存”还是“正式退役该门禁”，避免数据库治理继续悬空。
5. **功能缺陷修复：P1-03 root/canonical 双轨账本与显式依赖治理**
   - 依赖 P0-02 的 appservice 主链路先稳定；当前已补齐 overlap ledger、CI 入口、`pub use crate::storage::*` 禁线硬校验，并完成多批顶层单文件 service facade 净化、`auth/password_policy.rs` 收口、`database_initializer` 模块级 canonical 收口，以及 `geo_ip/`、`content_scanner/`、`identity/` 三组与 `push/providers/mod.rs`、`push/service.rs`、`push/mod.rs` 的连续收口，`services` 的 full_impl 已从 `76` 降到 `56`。下一步继续优先处理 `push/gateway.rs`、`push/queue.rs` 等相邻非聚合壳层，再推进显式 storage 依赖治理，避免边改边重新引入分层泄漏。
6. **架构演进项：P1-12 worker/replication 可运营模型闭环**
   - 依赖 P1-03 的分层边界继续收口；把现有 worker/replication 雏形提升为可部署模型，补齐实例映射、路由归属、状态同步与最小拓扑文档。
7. **功能缺陷修复：P1-06 Matrix surface 治理闭环**
   - 依赖 P1-03 的服务边界再收口一轮；把剩余 capability 继续归类到“配置控制 / 路由存在性 / 静态稳定”，并推动文档/脚本生成闭环，防止协议面再次漂移。
8. **性能优化项：P2-08 重复依赖版本清理**
   - 依赖前述功能与架构面先稳定；生成完整重复依赖清单，优先处理低风险叶子依赖，避免在主链路仍波动时放大回归面。
9. **代码规范整改项：P2-07 文档证据基线治理**
   - 依赖 P1/P2 主链路状态稳定后进行；为 current 文档增加“最后验证时间 / 证据来源”，并把历史文档做 current/archive 分层，避免技术债文档再次反向失真。
10. **通用运行时复核项**
   - 在上述核心整改稳定后，重新建立覆盖率、`cargo test --lib` 失败数、`tests/unit/` DB 依赖迁移完成度等基线；P0-02 appservice scheduler 多出口与优先级链路的 focused 运行时复核已在本轮补齐。

| 阶段 | 时间 | 目标 | 对应问题 | 负责人节点 |
|---|---|---|---|---|
| Phase A | 已完成 | 恢复 all-features 编译门禁，关闭 `feature_flags` blocker | P0-01 | 架构 + 平台 + CI |
| Phase B | 已完成 | 恢复 `test-utils` 集成测试编译门禁，关闭当前 P1-09 阻断项 | P1-09 | 架构 + 测试 + 模块 owner |
| Phase C | 已完成首版，后续剩余 1~2 周 | 补齐 appservice 配置装载、自动推送、关键桥接/边界验证、scheduler/controller 首版、容量治理首版、transaction 状态/内部指标首版与 admin statistics/telemetry/Prometheus 聚合出口，并补齐 focused 运行时复核；本轮已进一步修复 statistics 实时聚合事实来源与 recovery/mixed contention 观测证据，后续仅剩阈值调优与高负载策略治理 | P0-02 | 协议兼容 + appservice |
| Phase D | 1 周 | 补齐验证码 provider 真实发送/失败路径，关闭 `captcha_service` 运行时 stub | P1-10 | 账号 + 通知 |
| Phase E | 1 周 | 统一 SQLX 离线缓存策略、门禁脚本与文档基线 | P1-11 | DB + 平台 + CI |
| Phase F | 2 周 | 继续清理 root/canonical 边界债，收口 `services/mod.rs` 分层泄漏 | P1-03 | 服务层 + 存储层 |
| Phase G | 2 周 | 明确 worker responsibility matrix，补 worker/replication 最小可运营拓扑 | P1-12 | 架构 + 平台 + 运维 |
| Phase H | 2 周 | 收敛 `ServiceContainer` 双访问面，修复 Matrix surface 文档漂移 | P1-05 / P1-06 | 架构 + Web + 文档 |
| Phase I | 持续 | 文档治理与重复依赖治理常态化 | P2-07 / P2-08 | 文档 + 平台 + 发布 |

---

## 八、当前项目状态总评

### 8.1 可确认的当前真实问题

- `cargo check --workspace --all-features --locked`、`test-utils` integration `--no-run` 与 `cargo clippy --all-features --locked -- -D warnings` 均已恢复通过。
- appservice 已从“管理接口化”推进到“本地房间事件自动分发 + 联邦/多数已识别旁路入口覆盖 + 建房事务提交后统一分发 + 自动 sender + 基础 backoff/recoverer + 失败分类/自动隔离坏 AS + message-path / membership bridge e2e + virtual user / exclusive namespace / 管理面显式写入边界约束 + transaction controller / per-AS 调度策略首版 + 容量限流与 scheduler 状态观测首版 + transaction 聚合状态/内部指标首版 + admin statistics/telemetry/Prometheus 聚合出口首版 + statistics 实时聚合事实来源修复 + recovery/mixed contention 观测证据补齐”，但仍未达到上游 Synapse 的完整事件分发系统能力。
- root/canonical 双轨冗余仍然显著。
- `captcha_service` 在 email/sms 通道下仍会命中 `todo!()`，属于配置打开即可触发的运行时 stub。
- `.sqlx/` 离线缓存门禁当前真实失败，M3 文档、脚本与仓库状态不一致。
- worker/replication 仍缺少上游 Synapse 式的实例映射、路由归属与可运营部署闭环。
- 协议面文档与真实代码状态存在漂移。
- 技术债文档自身已出现反向失真。

### 8.2 可确认已修复、无需继续当作当前问题的旧项

- `application_service` 早期 SQL 列名错误。
- `migrations/README.md` 仍引用 v8。
- `CHANGELOG.md` 仍引用 v8.0.0。
- `route_ledger` root 仅为 4 行 re-export 的结论。
- `feature_flags` 的 `CacheManager` 类型边界阻断 all-features 编译。
- `test-utils` 集成测试编译门禁仍被 root/canonical 类型边界阻断。
- `admin_user_service` direct SQL 绕过 storage。
- appservice 的 membership bridge e2e 缺口。
- virtual user 未受 exclusive user namespace 约束。
- exclusive namespace 仅记录未做真实冲突校验。
- 管理面显式 `push_event` 缺少 namespace ownership 约束。
- appservice 缺少 transaction controller / per-AS 调度策略。
- appservice 缺少容量限流与 scheduler 状态观测。
- appservice 缺少 transaction 聚合状态与 scheduler 内部指标面。
- scheduler 在 `application_service_statistics` 缺失有效 pending 计数时，会把 pending transaction 优先级退化为按 `as_id` 排序。

### 8.3 本轮未做最终定论、需运行时再复核的项

- 当前真实覆盖率与 mutation baseline。
- `cargo test --lib` 当前失败测试数。
- `tests/unit/` DB 依赖迁移是否已完全完成。
- 全仓生产代码 `unwrap/expect` 的最新精确分布。

---

## 九、结论

当前 synapse-rust 的核心矛盾已经从“若干早期致命 SQL/协议错误”转移为：

1. **`cargo check --workspace --all-features --locked`、测试特性下的 integration `--no-run`，以及 `cargo clippy --all-features --locked -- -D warnings` 均已恢复，但 root/canonical 双轨边界仍有系统性治理空间**。
2. **appservice 已形成基础调度闭环，并补齐联邦/多数已识别旁路入口覆盖、建房事务提交后分发、第二层 recoverer 失败治理、实时聚合观测事实来源，以及 recovery/mixed contention 的多出口证据，但仍未达到上游 Synapse 的完整架构能力**。
3. **注册验证码与 SQLX 离线缓存两条运维/门禁链路仍未闭环，前者存在运行时 stub，后者存在脚本与仓库状态漂移**。
4. **worker/replication 仍停留在“具备雏形”阶段，距离上游 Synapse 的可运营多实例模型还有明显差距**。
5. **分层迁移停留在 facade 与兼容层并存阶段，代码/模块冗余仍大**。
6. **文档治理落后于代码演进，导致团队对当前真实状态的判断失真**。

因此，下一轮治理不应继续把重点放在已经修复的历史问题上，而应优先按本报告的 P0/P1 清单处理当前真实阻断项与结构性短板。

---

**报告完。**
