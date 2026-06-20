# synapse-rust 全面深度技术审计报告

**版本**: 8.8.0（2026-06-20 对标 Synapse v1.153-v1.155 优化：canonical JSON 规范向量门禁、to-device 大小限制、access token 缓存失效修复）
**审计基线**: `/Users/ljf/Desktop/hu_ts/synapse-rust` 当前工作区状态
**对标基线**: Matrix Spec v1.18；element-hq/synapse v1.155.x 文档与架构实践
**最后验证时间**: 2026-06-20
**证据来源**:
  - `cargo check --workspace --all-features --locked`: 通过
  - `cargo test --features test-utils --test unit`: 862 passed, 0 failed
  - `cargo test --lib -p synapse-common`: 298 passed, 0 failed（含 38 个 canonical JSON 测试，22 个为 Matrix 规范向量）
  - `cargo fmt --all -- --check`: 通过
  - `cargo clippy --all-features --locked -- -D warnings`: 通过
  - `python3 scripts/ci/check_root_canonical_ledger.py`: services=2 (facade=2, full_impl=0), storage=55 (facade=55, full_impl=0)
  - `bash scripts/ci/check_sqlx_offline_cache.sh`: OK (296 缓存文件)
  - `cargo tree -d --workspace`: 9 组深层重复依赖
  - `cargo test --features test-utils --test unit -- test_capability --nocapture`: 全部 17 个 capability 已归入 RouteSurface/ConfigControlled，无遗留 StaticStable
  - P1-12 部署工件：`docker/docker-compose.split-minimal.yml`、`docker/nginx/split-minimal.conf`、`docker/run_split_minimal_smoke.sh`、`src/worker/topology_validator.rs`、`docs/synapse-rust/WORKER_TOPOLOGY_BASELINE_2026-06-14.md` 全部就位
  - P1-06 surface 一致性：`SUPPORTED_MATRIX_SURFACE.md` 与 `versions.rs` 代码 capability 声明一致（10 public + 8 authenticated-only）
  - `cargo test --features test-utils --test integration api_sliding_sync_contract_tests -- --nocapture --test-threads=4`: **通过**
  - `cargo test --features test-utils --test integration api_federation_signature_auth_tests -- --nocapture --test-threads=4`: **通过**
  - `cargo test --features test-utils --test integration --no-run --locked`: **再次通过**
  - `bash scripts/ci/check_release_doc_spotcheck.sh`: PASS
  - 文档归档：6 个历史文档已迁入 `docs/synapse-rust/archive/`，PR 模板 `.github/pull_request_template.md` 已创建
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
  - 协议面文档与代码漂移已进一步收敛；capability 治理已完成首轮闭环，当前 17 个 capability 已统一归入 `RouteSurface / ConfigControlled`，无遗留 `StaticStable`。
  - 注册验证码链路的运行时 panic stub 已消除，focused integration 已补齐“未配置 provider 显式失败 / 已配置 email 入后台任务 / 已配置 SMS provider 成功投递”三条运行时证据；本轮又补入 `synapse_worker` 针对 fake SMTP server 的真实握手/投递冒烟测试，当前剩余缺口已进一步收敛为生产级 SMS provider 对接与回执校验。
  - SQLX 校验基线已统一为 live-schema primary、`.sqlx/` optional accelerator；当前剩余工作是长期维护文档与脚本一致，而不是继续把空 `.sqlx/` 视为失败。
  - worker/replication 方向已从“静态基线可见化”推进到“topology validator + route owner/header 校验 + split_minimal 可执行部署工件 + deployment smoke 联调链路”，并新增 worker task claim/ownership focused 单测、`HealthChecker` 的 `Unhealthy -> Degraded -> Healthy` 恢复状态机 focused 单测，以及“worker 注销后 running 任务自动回退到 `pending` 并可被其他 worker 重新 claim”、“多 worker 下活动实例可见性与 replication position 按 worker 隔离”、“fallback 选择优先最新 heartbeat 的兼容 worker、而非最 stale 实例”、“load balancer 命中不健康实例后 fallback 优先转向健康候选而非再次回退到坏实例”的 focused 证据；同时新增 integration 级验证，直接覆盖 `select_worker_for_task()` 在 `load balancer -> health_checker -> fallback` 链路下从不健康首选实例回退到健康候选，以及实例恢复为 `Healthy` 后重新被选回的行为，进一步固定任务归属与基础恢复边界；但相较上游 Synapse `workers.md` 中围绕 `instance_map`、HTTP replication listener、worker 配置分工与反向代理路由的可运营模型，当前剩余差距仍收敛于长时间窗多实例 heartbeat/replication position 一致性与运维手册闭环。

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
- `cargo check --workspace --all-features --locked`：**再次通过**
  - 验证点：`friend_room_service` 已在 canonical 侧引入 `FriendRoomRoomOps`、`FriendFederationSender` 与 `FriendRoomCreateRoomConfig`，root 侧 `src/services/friend_room_service/mod.rs` 已收口为薄包装；`synapse-web` 的 direct-DM 路由调用点也已切换到中性 DTO
- `python3 scripts/ci/check_root_canonical_ledger.py`：**再次通过**
  - 验证点：删除 root 侧已不再参与编译的 `services/friend_room_service/groups.rs`、`services/friend_room_service/models.rs` 后，services overlap 统计已进一步收敛到 `102` 个重叠文件，其中 `69 thin_facade / 33 full_impl`
- `cargo check --workspace --all-features --locked`：**再次通过**
  - 验证点：root `src/web/routes/dm.rs` 与 `src/web/routes/friend_room.rs` 也已切换到 `FriendRoomCreateRoomConfig`，`src/services/friend_room_service/mod.rs` 删除了 `ensure_direct_room(...)` / `create_or_reuse_direct_message_room(...)` 的兼容转发，仅保留构造与依赖适配
- `python3 scripts/ci/check_root_canonical_ledger.py`：**再次通过**
  - 验证点：`friend_room_service` 的 root 包装进一步压薄后，ledger 统计仍保持 `services=102 / thin_facade=69 / full_impl=33`，说明本轮收益主要体现在调用面与边界简化，而非重叠文件数量变化
- `cargo check --workspace --all-features --locked`：**再次通过**
  - 验证点：root `src/services/media/mod.rs` 已删除多余的 `chunked_upload` 包装模块，直接收口为 `pub use synapse_services::media::*;` 纯 facade；现有 `crate::services::media::chunked_upload::*` 调用路径保持兼容
- `python3 scripts/ci/check_root_canonical_ledger.py`：**再次通过**
  - 验证点：`services/media/mod.rs` 收口为纯 facade 后，services overlap 统计由 `69 thin_facade / 33 full_impl` 进一步收敛到 `70 thin_facade / 32 full_impl`
- `cargo check --workspace --all-features --locked`：**再次通过**
  - 验证点：`synapse-storage/src/media/chunked_upload.rs` 已新增 `ChunkedUploadStorage` 与相关 DTO，canonical `synapse-services/src/media/chunked_upload.rs` 改为复用 storage 层，`upload_progress` / `upload_chunks` 的 direct SQL 已从 service 层移除
- `python3 scripts/ci/check_root_canonical_ledger.py`：**再次通过**
  - 验证点：在继续把 canonical `chunked_upload` 逻辑切换到 storage 后，root/canonical services overlap 统计进一步收敛到 `services=102 / thin_facade=71 / full_impl=31`
- `cargo check --workspace --all-features --locked`：**再次通过**
  - 验证点：`src/services/account_data_service.rs` 已移除对 `PgPool` 直接查询 `account_data` / `room_account_data` 的依赖，改为复用 `src/storage/account_data.rs`、`src/storage/room_account_data.rs` facade 以及 `synapse-storage` 中新增的 typed storage 能力
- `python3 scripts/ci/check_root_canonical_ledger.py`：**再次通过**
  - 验证点：当前工作区 ledger 已稳定在 `services=83 / thin_facade=69 / full_impl=14`，`storage=60 / thin_facade=60 / full_impl=0`
- `cargo check --workspace --all-features --locked`：**再次通过**
  - 验证点：`src/services/oidc_mapping_service.rs` 已不再直接执行 `oidc_user_mapping` SQL，而是复用 `src/storage/oidc_user_mapping.rs` facade 与 `synapse-storage/src/oidc_user_mapping.rs`；同时 `synapse-web/src/routes/oidc.rs` 的登录回调已切换为调用 `OidcMappingService`
- `python3 scripts/ci/check_root_canonical_ledger.py`：**再次通过**
  - 验证点：新增 root `oidc_user_mapping` facade 后，当前工作区 ledger 更新为 `services=83 / thin_facade=69 / full_impl=14`，`storage=61 / thin_facade=61 / full_impl=0`
- `cargo check --workspace --all-features --locked`：**再次通过**
  - 验证点：`src/services/client_push_service.rs` 已将 `pushers` / `push_rules` / `notifications` / `m.push_rules` 相关 direct SQL 切换为复用 `src/storage/push.rs` 与 `src/storage/account_data.rs` facade；同时补齐了 `synapse-storage/src/push.rs` 的设备过滤、通知 ID 读取与布尔返回类型，root/canonical 的 `EventBroadcaster`、`FriendFederationClient`、space 分页接口等兼容点也已恢复全量编译
- `python3 scripts/ci/check_root_canonical_ledger.py`：**再次通过**
  - 验证点：新增 root `push` facade 后，当前工作区 ledger 更新为 `services=83 / thin_facade=69 / full_impl=14`，`storage=62 / thin_facade=62 / full_impl=0`
- `cargo check --workspace --all-features --locked`：**再次通过**
  - 验证点：`src/services/admin_security_service.rs` 已移除对 `users.is_shadow_banned` 与 `rate_limits` 的 direct SQL，改为复用 `UserStorage::set_shadow_ban(...)` 与新增的 `src/storage/rate_limit.rs` facade / `synapse-storage/src/rate_limit.rs`；同时 `synapse-web/src/routes/admin/security.rs` 也已切换到同一 storage owner，避免 canonical web 路由继续复制相同 SQL
- `python3 scripts/ci/check_root_canonical_ledger.py`：**再次通过**
  - 验证点：新增 root `rate_limit` facade 后，当前工作区 ledger 更新为 `services=83 / thin_facade=69 / full_impl=14`，`storage=63 / thin_facade=63 / full_impl=0`
- `cargo check --workspace --all-features --locked`：**再次通过**
  - 验证点：`src/services/admin_media_service.rs` 已移除对 `media_metadata` 的 direct SQL，改为复用新增的 `src/storage/admin_media.rs` facade / `synapse-storage/src/admin_media.rs`；同时 canonical `synapse-web/src/routes/admin/media.rs` 也已切换到同一 storage owner，避免 admin media 路由继续复制 `media_metadata` 查询/删除逻辑
- `python3 scripts/ci/check_root_canonical_ledger.py`：**再次通过**
  - 验证点：新增 root `admin_media` facade 后，当前工作区 ledger 更新为 `services=83 / thin_facade=69 / full_impl=14`，`storage=64 / thin_facade=64 / full_impl=0`
- `cargo check --workspace --all-features --locked`：**再次通过**
  - 验证点：canonical `synapse-services/src/sync_service/data_fetch.rs` 与 root `src/services/sync_service/data_fetch.rs` 已不再直接查询 `account_data` / `room_account_data`；其中 `account_data` 读取改为复用既有 `AccountDataStorage`，`room_account_data` 读取改为复用 `synapse-storage/src/room_account_data.rs` 新增的 list/batch list helper，`m.direct` 过滤、`m.push_rules` 补齐与 sync 响应拼装语义保持不变
- `python3 scripts/ci/check_root_canonical_ledger.py`：**再次通过**
  - 验证点：本轮未新增新的 root storage facade，ledger 统计保持 `services=83 / thin_facade=69 / full_impl=14`，`storage=64 / thin_facade=64 / full_impl=0`；收益主要体现在 `sync_service/data_fetch.rs` 内部 owner 进一步收敛，而非重叠文件数量变化
- `cargo check --workspace --all-features --locked`：**再次通过**
  - 验证点：canonical/root `sync_service/data_fetch.rs` 中针对 `to_device_messages` 与 `device_lists_stream` 的 direct SQL 已切换为复用 `synapse-e2ee/src/to_device/storage.rs` 新增的 `get_messages_since(...)` / `has_messages_since(...)` 以及 `synapse-storage/src/device.rs` 新增的 `get_device_list_changed_users_since(...)` / `get_device_list_left_users_since(...)` / `has_device_list_updates_since(...)`；同时 `sync_service/event_fetch.rs` 的增量轮询也已同步切换到同一 owner helper
- `python3 scripts/ci/check_root_canonical_ledger.py`：**再次通过**
  - 验证点：本轮继续保持 `services=83 / thin_facade=69 / full_impl=14`，`storage=64 / thin_facade=64 / full_impl=0`；收益仍集中在 `sync_service` 内部 direct SQL 缩减与 owner 收敛，不涉及新的 facade 文件
- `cargo test --test to_device_sync_tests_migrated -- --nocapture`：**通过**
  - 验证点：现有 `to_device` 增量 token 与 ACK 删除链路在切换到 owner helper 后仍保持通过，说明 sync token 前进与 `delete_messages_up_to(...)` 的既有语义未被回归破坏
- `cargo check --workspace --all-features --locked`：**再次通过**
  - 验证点：canonical/root `sync_service/data_fetch.rs` 中针对 `room_ephemeral` 的单房间读取与 batch 读取 SQL 已切换为复用 `synapse-storage/src/event/mod.rs` 新增的 `get_ephemeral_events_batch(...)` 以及既有 `get_ephemeral_events(...)`；`m.receipt` 聚合与 typing/其它 ephemeral 事件拼装仍留在 sync service，storage 只承担读取 owner
- `python3 scripts/ci/check_root_canonical_ledger.py`：**再次通过**
  - 验证点：本轮继续保持 `services=83 / thin_facade=69 / full_impl=14`，`storage=64 / thin_facade=64 / full_impl=0`；收益仍集中在 `sync_service/data_fetch.rs` 的 direct SQL 缩减，不涉及新的 facade 文件
- `cargo check --workspace --all-features --locked`：**再次通过**
  - 验证点：canonical/root `sync_service/data_fetch.rs` 中针对 `read_markers + events` 的 unread counts 单房间与 batch 聚合 SQL 已切换为复用 `synapse-storage/src/room/mod.rs` 新增的 `get_unread_counts(...)` / `get_unread_counts_batch(...)`；`sync_service` 保留的职责只剩结果转为 sync response 所需 `(highlight_count, notification_count)` 元组
- `python3 scripts/ci/check_root_canonical_ledger.py`：**再次通过**
  - 验证点：本轮继续保持 `services=83 / thin_facade=69 / full_impl=14`，`storage=64 / thin_facade=64 / full_impl=0`；收益仍集中在 `sync_service` 内部 direct SQL 继续减少，而非新增 facade 文件
- `cargo test --test api_room_tests test_room_unread_count_route_returns_counts_from_summary -- --exact --nocapture`：**通过**
  - 验证点：现有 `/_matrix/client/v3/rooms/{room_id}/unread_count` 路由仍能返回 `notification_count=1` / `highlight_count=1`，说明 `sync_service.room_unread_counts()` 在切换到 `RoomStorage` owner helper 后语义保持不变
- `cargo check --workspace --all-features --locked`：**再次通过**
  - 验证点：canonical `synapse-services/src/e2ee_audit/audit_service.rs` 与 root `src/services/e2ee/audit_service.rs` 中针对 `e2ee_audit_log` 的写入、历史读取、分页读取、按操作读取、按设备读取与清理 SQL 已切换为复用新增的 `synapse-storage/src/e2ee_audit.rs`；service 层保留的职责只剩日志语义包装与 `CrossSigningVerificationService` 组合使用
- `python3 scripts/ci/check_root_canonical_ledger.py`：**再次通过**
  - 验证点：新增 root `src/storage/e2ee_audit.rs` facade 后，当前工作区 ledger 更新为 `services=83 / thin_facade=69 / full_impl=14`，`storage=65 / thin_facade=65 / full_impl=0`
- `测试覆盖说明`：**无新增 focused test**
  - 原因与风险：当前工作区没有现成的 `e2ee_audit` 专项运行时回归用例；本轮主要依赖编译门禁与 owner 收口静态证据。后续若继续扩到 `CrossSigningVerificationService` 或 admin/key-history 路由，建议补一条 focused integration 覆盖 `log_key_operation()` 与 `get_key_history()` 主链路
- `cargo check --workspace --all-features --locked`：**再次通过**
  - 验证点：删除 root `src/services/room/utils.rs` 后，`src/services/room/mod.rs` 继续通过 `pub use synapse_services::room::*;` 暴露 canonical `room` 模块，说明该文件仅为未进入编译链的镜像残留，不承担运行时代码职责
- `python3 scripts/ci/check_root_canonical_ledger.py`：**再次通过**
  - 验证点：删除未编译的 root `services/room/utils.rs` 后，ledger 从 `services=83 / thin_facade=69 / full_impl=14` 进一步收敛到 `services=82 / thin_facade=69 / full_impl=13`；`storage` 保持 `65 / thin_facade=65 / full_impl=0`
- `cargo test --features test-utils --test integration test_delete_user_token_by_id -- --exact --nocapture`：**通过**
  - 验证点：`AccessTokenStorage::delete_user_token_by_id(...)` 已覆盖“按 `user_id + token_id` 删除 access token” 的 storage owner 能力，避免 root service 与 canonical admin route 继续各自保留一份 `DELETE FROM access_tokens ...`
- `cargo check --workspace --all-features --locked`：**再次通过**
  - 验证点：root `src/services/admin_token_service.rs` 的 access token 删除逻辑已改走 `AccessTokenStorage::delete_user_token_by_id(...)`，canonical `synapse-web/src/routes/admin/token.rs` 的 access/refresh token 列表与删除路径也已切到 storage owner
- `python3 scripts/ci/check_root_canonical_ledger.py`：**再次通过**
  - 验证点：本轮 admin token owner 收口未新增 root/canonical overlap，ledger 继续保持 `services=82 / thin_facade=69 / full_impl=13`、`storage=65 / thin_facade=65 / full_impl=0`
- `cargo test --features test-utils --test integration test_admin_user_token_routes_require_existing_user -- --exact --nocapture`：**当前被无关集成测试编译回归阻断**
  - 阻断点：`tests/integration/room_service_tests_migrated.rs` 与 `tests/integration/sync_service_tests_migrated.rs` 仍存在 `synapse_rust::EventBroadcaster` / `synapse_services::EventBroadcaster` 类型错位，导致整个 `integration` test target 编译失败；该问题与本轮 token owner 收口无直接关系，暂未并入当前 low-risk lane
- `cargo test -p synapse-web --lib routes::account_data::tests::test_account_data_routes_structure -- --exact --nocapture`：**通过**
- `cargo test -p synapse-web --lib routes::account_data::tests::test_openid_token_response -- --exact --nocapture`：**通过**
  - 验证点：canonical `synapse-web/src/routes/account_data.rs` 在切换 `account_data` / `room_account_data` / `filters` / `openid_tokens` 到 storage owner 后，路由模块自身编译与基础响应结构测试保持通过
- `cargo check --workspace --all-features --locked`：**再次通过**
  - 验证点：canonical `synapse-web/src/routes/account_data.rs` 已移除对 `account_data`、`room_account_data`、`filters` 与 `openid_tokens` 的 direct SQL，改为复用 `AccountDataStorage`、`RoomAccountDataStorage`、`FilterStorage`、`OpenIdTokenStorage` 以及既有 `UserStorage` / `RoomStorage` owner helper
- `python3 scripts/ci/check_root_canonical_ledger.py`：**再次通过**
  - 验证点：本轮 `account_data` canonical route owner 收口未新增 overlap，ledger 继续保持 `services=82 / thin_facade=69 / full_impl=13`、`storage=65 / thin_facade=65 / full_impl=0`
- `cargo test --features test-utils --test integration test_account_data_round_trip_across_v3_and_r0 -- --exact --nocapture`：**当前仍被无关集成测试编译回归阻断**
  - 阻断点：`tests/integration/room_service_tests_migrated.rs` 与 `tests/integration/sync_service_tests_migrated.rs` 里的 `synapse_rust::EventBroadcaster` / `synapse_services::EventBroadcaster` 类型错位仍会先于 `api_account_data_routes_tests` 编译失败；该问题与本轮 canonical `account_data` route 收口无直接关系
- `cargo test -p synapse-web --lib routes::tags::tests::test_tags_routes_structure -- --exact --nocapture`：**通过**
- `cargo test -p synapse-web --lib routes::tags::tests::test_tags_router_keeps_scope_limited_to_r0_and_v3 -- --exact --nocapture`：**通过**
  - 验证点：canonical `synapse-web/src/routes/tags.rs` 在切换到 `RoomTagStorage` 后，路由模块自身编译与基础路径结构测试保持通过
- `cargo check --workspace --all-features --locked`：**再次通过**
  - 验证点：canonical `synapse-web/src/routes/tags.rs` 已移除本地 `room_tags` 查询/upsert/delete helper，统一改为复用 `synapse-storage/src/room_tag.rs` 的 `RoomTagStorage`
- `python3 scripts/ci/check_root_canonical_ledger.py`：**再次通过**
  - 验证点：本轮 `tags` canonical route owner 收口未新增 overlap，ledger 继续保持 `services=82 / thin_facade=69 / full_impl=13`、`storage=65 / thin_facade=65 / full_impl=0`
- `cargo test -p synapse-web --lib routes::push_rules::tests::default_rules_have_required_top_level_keys -- --exact --nocapture`：**通过**
- `cargo test -p synapse-web --lib routes::push_rules::tests::merge_adds_missing_rules_without_clobbering -- --exact --nocapture`：**通过**
  - 验证点：canonical `synapse-web/src/routes/push_rules.rs` 在移除 `account_data` 直连 SQL 后，默认规则生成与 merge 语义保持通过
- `cargo check --workspace --all-features --locked`：**再次通过**
  - 验证点：canonical `synapse-web/src/routes/push_rules.rs` 的 `m.push_rules` 默认读取已切换到既有 `UserStorage::get_account_data_content(...)` owner
- `python3 scripts/ci/check_root_canonical_ledger.py`：**再次通过**
  - 验证点：本轮 `push_rules` canonical route owner 收口未新增 overlap，ledger 继续保持 `services=82 / thin_facade=69 / full_impl=13`、`storage=65 / thin_facade=65 / full_impl=0`
- `cargo test -p synapse-web --lib routes::push::tests::test_push_routes_structure -- --exact --nocapture`：**通过**
- `cargo test -p synapse-web --lib routes::push::tests::test_push_router_keeps_rule_mutation_extras_limited_to_v3 -- --exact --nocapture`：**当前被无关默认特性编译阻断**
  - 阻断点：`synapse-services/src/room/service.rs` 在未开启 `friends` feature 的默认特性编译下仍引用了被 `#[cfg(feature = "friends")]` 门控的 `crate::friend_room_service`；该问题与本轮 canonical `push.rs` 中 `push_rules` 子组收口无直接关系
- `cargo check --workspace --all-features --locked`：**再次通过**
  - 验证点：canonical `synapse-web/src/routes/push.rs` 中 `m.push_rules` 读取、`push_rules` CRUD、`actions/enabled` 更新与规则列表读取已切换到既有 `UserStorage::get_account_data_content(...)` 与 `synapse-storage/src/push.rs` 的 `PushStorage`
- `python3 scripts/ci/check_root_canonical_ledger.py`：**再次通过**
  - 验证点：本轮 `push.rs` 中 `push_rules` 子组 owner 收口未新增 overlap，ledger 继续保持 `services=82 / thin_facade=69 / full_impl=13`、`storage=65 / thin_facade=65 / full_impl=0`
- `cargo test -p synapse-web --lib routes::push::tests::test_notification_response_structure -- --exact --nocapture`：**当前被无关默认特性编译阻断**
- `cargo test -p synapse-web --lib routes::push::tests::test_push_route_examples_still_match_expected_prefixes -- --exact --nocapture`：**当前被无关默认特性编译阻断**
  - 阻断点：`synapse-services/src/room/service.rs` 在未开启 `friends` feature 的默认特性编译下仍引用了被 `#[cfg(feature = "friends")]` 门控的 `crate::friend_room_service`；该问题与本轮 canonical `push.rs` 中 `notifications` 子组收口无直接关系
- `cargo check --workspace --all-features --locked`：**再次通过**
  - 验证点：canonical `synapse-web/src/routes/push.rs` 中 `notifications` 列表读取与 ack 更新已切换到既有 `synapse-storage/src/push.rs` 的 `PushStorage`
- `python3 scripts/ci/check_root_canonical_ledger.py`：**再次通过**
  - 验证点：本轮 `push.rs` 中 `notifications` 子组 owner 收口未新增 overlap，ledger 继续保持 `services=82 / thin_facade=69 / full_impl=13`、`storage=65 / thin_facade=65 / full_impl=0`
- `cargo test -p synapse-web --lib --all-features routes::push::tests::test_push_routes_structure -- --exact --nocapture`：**当前被无关 `synapse-web` lib test 编译阻断**
- `cargo test -p synapse-web --lib --all-features routes::push::tests::test_set_pusher_request -- --exact --nocapture`：**当前被无关 `synapse-web` lib test 编译阻断**
  - 阻断点：`synapse-web/src/routes/admin/federation.rs` 仍存在 `super::PendingFederationCursor` unresolved import；该问题与本轮 canonical `push.rs` 中 `pushers` 子组收口无直接关系
- `cargo check --workspace --all-features --locked`：**再次通过**
  - 验证点：canonical `synapse-web/src/routes/push.rs` 中 `pushers` 列表、upsert 与 delete 已切换到既有 `synapse-storage/src/push.rs` 的 `PushStorage`，并对齐 root `P2 #32` 的当前设备可见性与 `device_id` 必需语义
- `python3 scripts/ci/check_root_canonical_ledger.py`：**再次通过**
  - 验证点：本轮 `push.rs` 中 `pushers` 子组 owner 收口未新增 overlap，ledger 继续保持 `services=82 / thin_facade=69 / full_impl=13`、`storage=65 / thin_facade=65 / full_impl=0`
- `cargo test -p synapse-web --lib --all-features routes::pinned::tests -- --nocapture`：**当前被无关 `synapse-web` lib test 编译阻断**
  - 阻断点：`synapse-web/src/routes/admin/federation.rs` 仍存在 `super::PendingFederationCursor` unresolved import；该问题与本轮 canonical `pinned.rs` 收口无直接关系
- `cargo check --workspace --all-features --locked`：**再次通过**
  - 验证点：canonical `synapse-web/src/routes/pinned.rs` 已移除对 `events` 表的最新 pinned state 读取与 pinned state event 写入 SQL，统一改为复用 canonical `RoomService::get_pinned_event_ids()` / `set_pinned_event_ids()`
- `python3 scripts/ci/check_root_canonical_ledger.py`：**再次通过**
  - 验证点：本轮 `pinned.rs` owner 收口未新增 overlap，ledger 继续保持 `services=82 / thin_facade=69 / full_impl=13`、`storage=65 / thin_facade=65 / full_impl=0`
- `cargo check --workspace --all-features --locked`：**再次通过**
  - 验证点：canonical `synapse-web/src/routes/guest.rs` 与 `synapse-web/src/routes/auth_compat.rs` 中 guest 注册路径的 `UPDATE users SET is_guest = TRUE ...` 已统一改为复用既有 `UserStorage::set_guest_status(...)`
- `python3 scripts/ci/check_root_canonical_ledger.py`：**再次通过**
  - 验证点：本轮 guest 注册路径 owner 收口未新增 overlap，ledger 继续保持 `services=82 / thin_facade=69 / full_impl=13`、`storage=65 / thin_facade=65 / full_impl=0`
- `cargo check --workspace --all-features --locked`：**再次通过**
  - 验证点：canonical `synapse-web/src/routes/guest.rs` 中 guest upgrade 路径的 `UPDATE users SET username = ..., is_guest = FALSE, password_hash = ...` 已统一改为复用既有 `UserStorage::upgrade_guest_account(...)`
- `python3 scripts/ci/check_root_canonical_ledger.py`：**再次通过**
  - 验证点：本轮 canonical guest upgrade 路径 owner 收口同样未新增 overlap，ledger 继续保持 `services=82 / thin_facade=69 / full_impl=13`、`storage=65 / thin_facade=65 / full_impl=0`
- `cargo check --workspace --all-features --locked`：**再次通过**
  - 验证点：canonical `synapse-web/src/routes/ephemeral.rs`、`synapse-web/src/routes/handlers/room/receipts.rs` 与 `synapse-web/src/routes/typing.rs` 中对 `room_ephemeral` 的 route 内 direct SQL 访问，已统一切回既有 `RoomService::get_ephemeral_events_for_client(...)`、`send_receipt(...)`、`set_read_markers(...)`、`set_typing_ephemeral_event(...)` 与 `clear_typing_ephemeral_event(...)`
- `python3 scripts/ci/check_root_canonical_ledger.py`：**再次通过**
  - 验证点：本轮 canonical `room_ephemeral` 读写面 owner 收口未新增 overlap，ledger 继续保持 `services=82 / thin_facade=69 / full_impl=13`、`storage=65 / thin_facade=65 / full_impl=0`
- `cargo check --workspace --all-features --locked`：**再次通过**
  - 验证点：canonical `synapse-web/src/routes/admin/register.rs` 中 shared-secret admin registration 成功后的 `UPDATE users SET user_type = ...` 已统一改为复用既有 `UserStorage::set_user_type(...)`
- `python3 scripts/ci/check_root_canonical_ledger.py`：**再次通过**
  - 验证点：本轮 canonical `admin/register` 单字段 owner 收口同样未新增 overlap，ledger 继续保持 `services=82 / thin_facade=69 / full_impl=13`、`storage=65 / thin_facade=65 / full_impl=0`
- `cargo check --workspace --all-features --locked`：**再次通过**
  - 验证点：canonical `synapse-web/src/routes/admin/retention.rs` 中 server/room retention policy 读取、upsert 与 status 汇总的 route 内 direct SQL 已统一切回既有 `RetentionService::get_server_policy_optional()`、`upsert_server_policy(...)`、`get_room_policy(...)`、`set_room_policy(...)` 与 `get_status_summary()`
- `python3 scripts/ci/check_root_canonical_ledger.py`：**再次通过**
  - 验证点：本轮 canonical `admin/retention` owner 收口未新增 overlap，ledger 继续保持 `services=82 / thin_facade=69 / full_impl=13`、`storage=65 / thin_facade=65 / full_impl=0`
- `cargo check --workspace --all-features --locked`：**再次通过**
  - 验证点：canonical `synapse-web/src/routes/admin/server.rs` 中 `status/health` 两条 DB 探活路径已统一改为复用既有 `state.health_checker.check_readiness()`；canonical `synapse-web/src/routes/handlers/health.rs` 中 database 检查也改为复用 `health_checker`，schema required tables 检查改为复用既有 `SchemaValidator::validate_required_tables(...)`
- `python3 scripts/ci/check_root_canonical_ledger.py`：**再次通过**
  - 验证点：本轮 canonical `admin/server` / `handlers/health` 基础探活路径 owner 收口未新增 overlap，ledger 继续保持 `services=82 / thin_facade=69 / full_impl=13`、`storage=65 / thin_facade=65 / full_impl=0`
- `cargo check --workspace --all-features --locked`：**再次通过**
  - 验证点：canonical `synapse-web/src/routes/admin/report.rs` 中 `event_reports` 的列表、详情、按房间列表、按房间详情与删除路径，已统一切回既有 `EventReportService::get_all_reports(...)`、`get_report(...)`、`get_reports_by_room(...)` 与 `delete_report(...)`
- `python3 scripts/ci/check_root_canonical_ledger.py`：**再次通过**
  - 验证点：本轮 canonical `admin/report` owner 收口未新增 overlap，ledger 继续保持 `services=82 / thin_facade=69 / full_impl=13`、`storage=65 / thin_facade=65 / full_impl=0`
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
- **2026-06-18 运行时门禁回写**：`api_sliding_sync_contract_tests` 与 `api_federation_signature_auth_tests` 已分别通过 `--test-threads=4` 的模块级复跑，当前测试侧主阻断已从这两组协议回归收敛为更长期的压测/运营闭环问题，而非继续卡在同类 integration 失败。

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
   -致性”。

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
| 验证码/辅助服务可运营性 | 文档、配置、投递链路完整闭环 | `captcha_service` 已具备模板、存储层、email 后台任务入队与 SMS provider 抽象；`synapse_worker` 也已补入 fake SMTP server 的真实握手/投递冒烟测试。当前剩余缺口主要收敛为生产级 SMS provider 接入与回执校验 | **部分补齐，仍有外部联调缺口** |
| 协议声明治理 | 保守、以实现/测试为依据 | room version 文档与 capability 声明已基本对齐；当前 17 个 capability 已统一归入 `RouteSurface / ConfigControlled`，并有 contract/snapshot 测试防漂移 | **已完成首轮闭环，后续以防回归为主** |
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
- `SUPPORTED_MATRIX_SURFACE` 中要求收敛的 capability 已完成首轮治理收口；当前代码中的 17 个 capability 已统一归入 `RouteSurface / ConfigControlled`，并由 `versions.rs` 集中 helper 与 contract/snapshot 测试共同约束，当前主工作转为维持事实驱动边界、防止声明回归。

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
- **修复方式**：已将 root [mod.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/cache/mod.rs) 直接收口为 `synapse-cache` facade，并同步清理 root `container.rs` / 状态装配中遗留的 cache 兼容转换逻辑，避免继续维护 root/canonical 双份 `CacheManager` 类型边界。
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
- **P0-02.1 本地 live 压测结果（2026-06-18）**：已在本地 `target/debug/synapse-rust`、`docker/config/homeserver.local.yaml`、5 个 `stress_as_*` YAML、admin API 与 `scripts/mock_appservice_bridge.py` 组合下完成 7 类 live 场景清单，并在每轮关键场景前清空 `application_service_state` / `application_service_events` / `application_service_transactions` / `application_service_statistics` 运行态表以获得干净基线。已完成场景包括 `event-only`、`transaction-only`、`mixed`、`mixed-backoff`、`recovery`、`continuous-ingress` 与 `super-event-heavy`；其中 `transaction-only` 在 `stress_as_1` 持续失败配置下观测到 `pending_transaction=1`、`total_backoff_count=12` 与单服务 `retry_backoff`，`mixed` 在 20 秒 steady-state 下收敛到 `pending_event_count=3` / `pending_transaction_count=0` / `total_success_count=115`，`mixed-backoff` 证明健康 `stress_as_1` 持续推进而失败 `stress_as_2` 维持 `pending_transaction=1` 且 `total_backoff_count=24`，`recovery` 在 5 个服务各失败一次后恢复到 `scheduler_available_services=5`、`pending=0`、`total_failure_count=5`、`total_backoff_count=40`，`continuous-ingress` 在 20 秒内注入 `270` 条事件后仅残留 `pending=3`，`super-event-heavy` 下重 AS 成功 `5` 且轻 AS 合计成功 `5`，未观察到长期饿死；三条外部观测出口 `/_synapse/admin/v1/appservices/statistics`、`/_synapse/admin/v1/telemetry/metrics` 与 Prometheus `/metrics` 在关键聚合值上保持一致。当前默认 `MAX_SERVICES_PER_TICK=8`、`HIGH_PENDING_TRANSACTION_THRESHOLD=2`、`HIGH_PENDING_EVENT_THRESHOLD=50` 在这组本地 live 清单中未打出持续 `capacity_limited`，P0-02.1 可视为完成，后续主任务从“验证默认值是否明显失衡”转入“更接近生产的更长 soak 和线上反例观察”。
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
  16. 已完成增强：补齐“多个 retry_backoff 服务共享恢复窗口后再次争用”、“多个 pending-transaction 服务长期 backlog 且 event-heavy 持续追加事件”与“单个超大 event-heavy 服务混跑轻量 transaction burst”三条下一阶段 focused integration。
  17. 已完成本地 live 验证：通过 admin API、mock bridge、真实 YAML appservice 配置与三出口快照，完成 event-only、transaction-only、mixed、mixed+backoff、recovery、continuous-ingress、super-event-heavy 七类场景的本地 live 清单；在本轮本地窗口内未观察到需要继续调节默认 `8/2/50` 阈值的直接证据，后续仅在更长 soak 或线上反例出现时再调整默认值。
- **责任节点**：协议兼容负责人、应用服务负责人、测试负责人、运维负责人。
- **资源投入**：后端 2~3 人周，QA 1 人周，SRE 0.5 人周。
- **验收标准**：
  - 功能可用性：当前已满足“AS YAML 配置可加载并在启动期导入”、“本地房间事件、联邦入口与多数已识别旁路事件可按 namespace 自动进入 pending queue”、“建房事务内事件可在 commit 成功后统一进入 appservice 分发链路”、“周期 sender/scheduler 可自动尝试发送/重试未完成 transaction”、“真实 `send_message()` message-path 可端到端投递到 mock bridge”、“真实 `m.room.member` invite/join membership 可端到端投递到 mock bridge”、“失败 AS 进入 backoff 时健康 AS 仍可继续出队”、“同一 AS 在未完成 transaction 存在时不会并发创建第二个 transaction”、“命中每轮活跃 AS 上限时可将剩余服务标记为 `capacity_limited` 并持久化 pending/backlog 状态”，以及“scheduler 可持久化 `retry_backoff` / `capacity_limited` 等 transaction 聚合状态与 success/failure/backoff/capacity 计数器，并通过 `/_synapse/admin/v1/appservices/statistics`、`/_synapse/admin/v1/telemetry/metrics` 与独立 `/metrics` 文本出口显式输出聚合 scheduler 视图”；此外，statistics 读面已改为实时聚合，recovery flow、mixed contention、multiple-recovery、continuous-ingress 与 super-event-heavy 的代码侧 focused 证据也已补齐。当前默认 `MAX_SERVICES_PER_TICK=8` / `HIGH_PENDING_TRANSACTION_THRESHOLD=2` / `HIGH_PENDING_EVENT_THRESHOLD=50` 暂无继续收紧或放宽证据，后续主任务正式转为生产压测与阈值观察。
  - 性能指标：单 AS 1000 events/min 压测下，入队到首次发送 p95 < 200ms，1 万事件积压在 5 分钟内消化完毕。
  - 代码质量：新增 route/service/storage/integration 四层测试；关键调度路径具备失败场景测试。
  - 资源利用率：AS 调度器 CPU 常态占用 < 1 核；队列积压时内存增长可控，恢复后 10 分钟内回落到基线 ±15%。

#### P0-02 下一步调优计划

- **目标边界**：`P0-02.1` 的本地 live 清单已完成，后续不再把重点放在“是否具备 scheduler 能力”或“本地短窗口 checklist 是否补齐”，而是验证默认 `MAX_SERVICES_PER_TICK=8`、`HIGH_PENDING_EVENT_THRESHOLD=50`、`HIGH_PENDING_TRANSACTION_THRESHOLD=2` 在更贴近生产的更长 soak、burst 恢复和大 backlog 场景下是否仍是合适默认值。
- **已完成的场景基线**：代码侧 focused 证据与本地 live 七场景清单都已完成，下一步不再优先补同类型测试，而改为保留现有场景作为回归基线，并记录更接近生产的长时间窗样本。
- **核心指标**：统一以 `pending_events`、`pending_transactions`、`services_in_backoff`、`services_capacity_limited`、`success/failure/backoff/capacity_limited` 聚合计数，以及 event-heavy 服务的首次重新获得 dispatch 的 tick 数作为调优判断依据；若 Prometheus/telemetry/admin statistics 三个出口出现显著不一致，应先视为观测链问题而非直接调阈值。
- **调参顺序**：默认先观察 `max_services_per_tick` 是否导致长期 `capacity_limited`，再观察 `HIGH_PENDING_TRANSACTION_THRESHOLD` 是否过早把轻量 transaction backlog 提升为 `high`，最后才考虑 `HIGH_PENDING_EVENT_THRESHOLD`；除非出现明确 starvation 反例，否则不建议一次同时改动多个默认值。
- **退出条件**：本地 live 七类场景已满足“无长期饥饿、无明显伪 backoff 残留、聚合出口一致、恢复后 pending 清零”的 checklist；后续只需在更接近生产的更长 soak 下重复抽样，若仍无反例，即可把 `P0-02` 长驻为“生产参数观察阶段”，只保留运维监控和偶发反例复盘。

| 后续场景 | 当前关注点 | 建议动作 | 主要观测指标 | 责任节点 | 退出判据 |
|---|---|---|---|---|---|
| 多个 `retry_backoff` 服务周期性恢复后再次争用 | 本地 live `recovery` 已确认恢复后 `pending=0` 且 `total_backoff_count` 可解释；仍缺更长 soak 曲线 | 保留当前 focused integration 和本地 live 结果作为基线；后续只补更长时间窗抽样 | `services_in_backoff`、`total_backoff_count`、恢复后 `pending_transactions` 归零时间、单服务 `scheduler_transaction_state` | 应用服务负责人 + 测试负责人 | 恢复后健康服务无明显饥饿，失败服务回到 `idle/success/dispatched`，三条观测出口一致 |
| 多个 `pending-transaction` 服务长期 backlog，且 `event-heavy` 服务持续追加事件 | 本地 live `continuous-ingress` 已确认积压不无限增长；仍缺更长时间窗样本 | 保留当前 continuous-ingress focused integration 与 live 结果；仅在出现长期 `capacity_limited` 时评估上调 `max_services_per_tick` | `services_capacity_limited`、`total_capacity_limited_count`、`pending_events`、event-heavy 服务首次回补 dispatch 的 tick 数 | 应用服务负责人 + 运维负责人 | event-heavy 服务能稳定回补 dispatch，无长期积压，`capacity_limited` 仅表现为短时竞争而非持续状态 |
| 单个超大 `event-heavy` 服务与多个轻量 transaction 服务混跑 | 本地 live `super-event-heavy` 已确认轻量服务未被饿死；仍缺尾延迟分布数据 | 以当前 focused integration 和 live 结果作为默认阈值基线；后续在固定 transaction 负载下逐步提升 event-heavy backlog | `scheduler_backlog_state`、`pending_events`、入队到首次 dispatch 的 p95/p99、`total_success_count` 增长斜率 | 应用服务负责人 + SRE | backlog state 的提升点与真实尾延迟恶化点基本一致，不再出现明显“过早告警”或“过晚告警” |
| 多出口观测一致性复核 | 本地 live 七场景均已抽样对齐三出口；后续只需在更长 soak 下重复核对 | 每次新增高负载或恢复场景时，固定抽样比对三条出口；若不一致，优先修观测链，不先调阈值 | `total_services`、`services_in_backoff`、`services_with_pending_transactions`、`total_pending_events`、`total_pending_transactions` | 运维负责人 + 测试负责人 | 三条出口关键聚合值稳定一致，且与单服务 state 读面可互相印证 |

#### P0-02 生产压测方案

- **目标**：在本地 live 七场景已经完成的基础上，不改动默认 `MAX_SERVICES_PER_TICK=8`、`HIGH_PENDING_EVENT_THRESHOLD=50`、`HIGH_PENDING_TRANSACTION_THRESHOLD=2`，进一步用更接近生产的长 soak 压测确认当前默认值是否仍足以覆盖 event-only、transaction-only、mixed、mixed+backoff、recovery、continuous-ingress 与 super-event-heavy 七类运行时场景。
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

### P1-03 root/canonical 双轨镜像已完成主 lane 收口，Phase 4 结构性瓶颈已清零

- **当前状态**：**facade 收口主 lane 已完成，且 Phase 4 已落地收口**。`python3 scripts/ci/check_root_canonical_ledger.py` 当前报告 services=2（facade=2, full_impl=0），storage=55（facade=55, full_impl=0）。此前 P1-03.1 已完成 4 阶段优化中的前 3 阶段：(1) `test_config.rs` 收口为 thin facade，re-export `synapse_services::test_config`；(2) `worker/topology_validator.rs` 和 `worker/types.rs` 收口为 thin facade，re-export `synapse_services::worker::*`；(3) canonical `container.rs` 已补齐全部 9 个缺失字段（`account_data_service`、`client_push_service`、`oidc_mapping_service`、`room_tag_service`、5 个 admin 服务）、`database_pool()` 方法、以及 worker topology 检查（`should_run_global_maintenance`、`current_instance_worker_type`、`global_maintenance_owner`），使 canonical 成为完整的组合根实现。**本轮已完成 Phase 4：先收口低风险的 e2ee/federation import-path-only 差异，再对齐高风险的 `CrossSigningService`、`SecretStorageService`、`FriendFederation` trait object 构造边界，最终将 root `container.rs` 收口为 thin facade。**`scripts/ci_backend_validation.sh` 已在 Rust 检查入口先执行该脚本。
- **复现步骤**：执行 `python3 scripts/ci/check_root_canonical_ledger.py`；查看 `src/services/mod.rs` 与 `scripts/ci_backend_validation.sh`。
- **影响范围**：分层治理、编译速度、IDE 索引、API 边界稳定性、代码评审成本。
- **发生场景**：迁移 facade、修改公共类型、排查编译错误、查找唯一实现来源时。
- **优化方案**：建立"单一事实来源"治理账本，把 root 明确定位为 facade/兼容层；禁止重新引入 storage glob re-export；按模块批次减少双实现文件与隐式依赖；并将 facade/canonical 间的 DTO、构造参数与责任矩阵收口到同一套治理口径和门禁，避免模块级迁移完成后又在边界层重新分叉。
- **实施步骤**：
  1. 已完成首版：固化"禁止恢复 `pub use crate::storage::*`"规则，`scripts/ci/check_root_canonical_ledger.py` 会对该禁线做硬校验。
  2. 已完成首版：为重叠文件建立脚本化 ledger，当前至少能稳定输出 `thin_facade` / `full_impl` 两类初版分类与重叠清单样本。
  3. ✅ 已完成（2/2 facade）：当前 ledger 已收敛至 `services=2 (facade=2, full_impl=0)`，`storage=55 (facade=55, full_impl=0)`。在完成 `test_config.rs` thin facade 收口与 canonical `container.rs` 补齐（9 个缺失字段 + `database_pool()` + worker topology 检查）后，本轮继续完成独立拆出的 Phase 4：root `cache` 已收口为 `synapse-cache` facade，root `e2ee/federation` 的低风险 import-path-only 差异已优先收口，`CrossSigningService`、`SecretStorageService`、`FriendFederation` 的 trait object 构造边界也已对齐，最终 root `container.rs` 已转换为 thin facade。
  4. 已完成首版：`scripts/ci_backend_validation.sh` 已接入 overlap ledger 脚本，Rust 检查前会先输出当前重叠文件数与 facade/full_impl 统计。
- **责任节点**：架构负责人、各域模块 owner、CI 负责人。
- **资源投入**：后端 2 人周，CI/工具 0.5 人周。
- **验收标准**：
  - 功能可用性：迁移后原有 API/route 行为无回归。
  - 性能指标：`cargo check` 增量构建时间较当前下降 15% 以上。
  - 代码质量：`src/services/mod.rs` 中不再出现 `pub use crate::storage::*`，且不再新增 `crate::services::*` 间接消费 storage 的调用；当前 `services` 重叠已收敛至 `2`（全部为 thin_facade），`full_impl=0`；`storage` 重叠 `55`（全部为 thin_facade），`full_impl=0`；新增或迁移模块不得重新引入 root/canonical 双轨 DTO、构造参数漂移或责任归属不清的问题。
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

### P1-06 Matrix surface 文档与能力声明治理已闭环

- **当前状态**：**已完成首轮闭环**。全部 17 个 capability 已归入 RouteSurface/ConfigControlled 两类治理，5 个 contract/snapshot 测试覆盖声明漂移防护，`SUPPORTED_MATRIX_SURFACE.md` 与 `versions.rs` 代码 capability 声明一致（10 public + 8 authenticated-only）。

- **当前验证**：`SUPPORTED_MATRIX_SURFACE.md` 的 room version 矩阵已修正为与代码一致的 `1..13`，并补入当前证据来源；`tests/integration/api_auth_routes_tests.rs` 已新增 `/versions` 与公开 `/capabilities` 的合约测试，校验 room version 常量、默认版本与未认证公开能力边界；前几轮已把 `m.change_password` / `m.set_displayname` / `m.set_avatar_url` / `m.3pid_changes`、`m.room.summary` / `m.room.suggested` / `m.voice` / `m.thread` / `io.hula.sliding_sync` 以及 `io.hula.widget`、`io.hula.friends`、`io.hula.burn_after_read`、`external_services` 等能力收敛到共享 helper 真值源。本轮复核确认：全部 17 个 capability 已统一归入 `RouteSurface` / `ConfigControlled` 两类治理，无遗留 `StaticStable`；`SUPPORTED_MATRIX_SURFACE.md`、`versions.rs` 与 5 条 contract/snapshot 测试已形成一致证据链。
- **复现步骤**：查看 `src/common/room_versions.rs` 与 `src/web/routes/handlers/versions.rs`。
- **影响范围**：客户端兼容预期、联邦行为说明、协议声明可信度。
- **发生场景**：客户端依据 `/versions` 与 `/capabilities` 判定支持面、对外对标 Synapse 时。
- **优化方案**：把协议声明治理收敛到“代码常量 + route ledger + contract test + 文档生成”闭环。
- **实施步骤**：
  1. 已完成：修正文档中的 room version 支持矩阵，并补入 `SUPPORTED_ROOM_VERSIONS` / `client_room_versions_capability()` / integration contract test 的证据链。
  2. 已完成首轮：对一组稳定 capability 做集中具名真值收口，并让 federation discovery 与 API 文档示例复用同一声明事实。
  3. 已完成第二轮：将 room summary / suggested / voice / thread / sliding sync 与相关 unstable features 收敛为共享 helper 真值源，并补入 integration 合约断言。
  4. 已完成第三轮：把 capability helper 统一收敛到 `RouteSurface` / `ConfigControlled` 两类治理，并新增 contract/snapshot 测试直接断言 governance 分类，确保 `m.sso`、`openclaw`、`ai_connection` 与 `external_services` / `widget` / `friends` / `burn_after_read` 等能力的声明方式可审计。
  5. 已完成：全部 17 个 capability 已归入 `RouteSurface` / `ConfigControlled` 两类治理，无遗留 `StaticStable` 或裸 `bool`；后续重点转为把部分 `RouteSurface` 能力继续升级为更强的事实驱动。
- **责任节点**：协议兼容负责人、文档负责人、测试负责人。
- **资源投入**：后端 1 人周，QA 0.5 人周。
- **验收标准**：
  - 功能可用性：`/versions`、`/capabilities`、federation room version 响应互相一致。
  - 性能指标：协议面接口 p95 不高于当前；不因动态生成能力而增加明显 CPU/分配开销。
  - 代码质量：新增 contract test 覆盖 room versions 与 capability matrix；文档与代码差异清零。
  - 资源利用率：协议面验证脚本总执行时间 < 2 分钟，可纳入 CI。

### P1-10 注册验证码 email/sms 投递链路已关闭 panic stub，并补齐 focused integration 闭环

- **当前状态**：**已完成第三轮闭环，SMTP worker 冒烟、SMS provider 抽象、配置与 focused integration 已落地**
- **当前验证**：`synapse-services/src/captcha_service.rs` 已将 `email` 路径接入现有 `BackgroundJob::SendEmail` 后台任务提交；当 SMTP 或任务队列未配置时，会返回显式 `ApiError::not_implemented(...)`，不再触发 panic。`sms` 路径也已从 `todo!()` 改为显式 `not_implemented` 错误。本轮 `src/bin/synapse_worker.rs` 新增 fake SMTP server 冒烟测试，验证 worker 真实执行 SMTP 握手、投递 `BackgroundJob::SendEmail` 邮件并生成完整 SMTP payload；同时 `synapse-services/src/sms_provider.rs`（`SmsProvider` trait + `NoopSmsProvider` + `create_sms_provider` 工厂）、`synapse-common/src/config/sms.rs`（`SmsConfig`）已集成到 `CaptchaService::with_sms_config` 与 `ServiceContainer` 装配；`tests/integration/captcha_tests_migrated.rs` 已补齐“未配置 email provider 显式报错”“已配置 email 成功入后台任务”“已配置 SMS provider 成功投递并记录 provider”三条 focused integration。`cargo test --bin synapse_worker smtp_smoke_tests::test_process_send_email_job_smoke_against_fake_smtp_server -- --exact --nocapture` 与 `cargo check --workspace --all-features --locked` 已通过。
- **影响范围**：原“配置打开即 panic”的高风险缺口已关闭；真实 SMTP worker 冒烟证据也已补齐，剩余缺口进一步收敛为生产级 SMS provider 接入与回执校验。
- **发生场景**：开启 email captcha 但未配置 SMTP/worker、开启 sms captcha 但尚未接入短信 provider、灰度环境联调时。
- **优化方案**：保持当前“显式失败优于 panic”的边界，后续继续把 email worker 冒烟和 SMS provider 接入补齐为真正可运营路径。
- **实施步骤**：
  1. ✅ 已完成：`send_email(...)` / `send_sms(...)` 从 `todo!()` 改为可预期失败路径，确保未配置 provider 时返回 `not_implemented` 而非进程 panic。
  2. ✅ 已完成：`CaptchaService` 构造参数已扩展为可接收 `task_queue` 与 `smtp_enabled`，root/canonical container 装配同步对齐。
  3. ✅ 已完成：focused integration 已覆盖“未配置 provider 时显式报错”“已配置 email 时成功入后台邮件任务”与“已配置 SMS provider 时成功投递并记录 provider”三条链路。
  4. ✅ 已完成：`synapse_worker` 已补入 fake SMTP server 冒烟测试，验证 worker 真实完成 SMTP 握手与邮件投递。
  5. 剩余未完成项：生产级 SMS provider 联调与回执校验。
- **责任节点**：账号注册负责人、通知/基础设施负责人、测试负责人。
- **资源投入**：后端 1 人周，测试 0.5 人周。
- **验收标准**：
  - 功能可用性：启用 email/sms captcha 时不发生 panic，未配置 provider 时返回可解释错误。
  - 性能指标：验证码发送链路 p95 不高于当前同类通知链路 10%。
  - 代码质量：生产代码中不再存在 `captcha_service` 相关 `todo!()` 发送 stub。
  - 资源利用率：发送失败不导致无界重试或任务堆积。

### P1-11 SQLX 离线缓存门禁与仓库基线已统一，OIDC 配置文档与回调安全已补齐

- **当前状态**：**已基本完成**。SQLX 基线已统一为 live-schema primary，`check_sqlx_offline_cache.sh` 门禁已落地（离线缓存为空时跳过）。OIDC 侧：external OIDC（PKCE + state 管理 + 回调 URL 安全校验）与 builtin OIDC（discovery + JWKS + authorize/token/login）已明确区分，`homeserver.yaml` 已补充三种模式说明、回调安全校验清单与运维说明。剩余待完成：生产级 IdP 对接压测与多 IdP 并发验证。
- **当前验证**：当前仓库 `.sqlx/` 为空，但代码内仍存在大量 `query!` 宏；因此继续强制“`.sqlx/` 必须非空”会与真实仓库基线长期冲突。本轮已将 `scripts/ci/check_sqlx_offline_cache.sh` 调整为“有缓存则校验 offline compile；无缓存则明确 `SKIP/0` 退出”，把主校验基线正式收口到 live-schema / DB-enabled 编译门禁。`bash scripts/ci/check_sqlx_offline_cache.sh` 当前已按该策略返回成功。
- **影响范围**：开发者对 SQLX 校验路径的理解恢复一致，避免脚本和文档继续互相打架；后续若需要重启 `.sqlx/`，可作为可选加速层单独恢复。
- **发生场景**：运行 SQLX 离线检查、准备发布、回归迁移脚本、继续推进 M3 SQL 宏迁移时。
- **优化方案**：明确“live-schema 为主、`.sqlx/` 可选增强”的当前基线，并把 M3 文档中的现行说明同步改成这一口径。
- **实施步骤**：
  1. ✅ 已完成：确认当前仓库不再以“`.sqlx/` 必须非空入仓”为正式基线。
  2. ✅ 已完成：`check_sqlx_offline_cache.sh` 已降级为 advisory gate；无缓存时显式跳过，有缓存时继续校验 offline compile。
  3. ✅ 已完成：将 M3 文档与综合审计报告中的现行策略回写为“live-schema primary / `.sqlx/` optional accelerator”。
- **责任节点**：数据库负责人、平台/CI 负责人、文档负责人。
- **资源投入**：后端/平台 1 人周。
- **验收标准**：
  - 功能可用性：开发者可以明确知道当前 SQLX 校验依赖的是 live schema，而不是强制离线缓存。
  - 性能指标：对应门禁在 CI 中稳定可复现，不出现“文档要求存在、仓库实际为空”的反向漂移。
  - 代码质量：M3 文档、CI 脚本、仓库 `.sqlx/` 状态三者一致。
  - 资源利用率：不引入重复门禁或无效缓存维护成本。

### P1-12 worker/replication 部署工件与 smoke/soak test 已全覆盖

- **当前状态**：**已完成**。部署工件（`docker-compose` / `nginx` / `smoke.sh`）、topology validator、deployment smoke test（827 行）、deployment soak test（408 行）均已落地。剩余待完成：生产环境多实例 soak 运行结果收集。
- **最新补充证据**：`synapse-services/src/worker/storage.rs` 与 `src/worker/storage.rs` 已在 `unregister_worker()` 中将被 stopped worker 持有的 `pending/running` 任务原子回退到 `pending`，并进一步把 `update_worker_status()` 收口为“状态更新 + 终态任务释放”的同一事务：当 worker 通过 heartbeat 显式进入 `stopped/error` 时，其持有任务也会自动回退；同时将 `get_active_workers()` 收口为直接按 `workers.status in ('running','starting')` 查询，避免活动实例判断继续依赖测试/运行环境中的 `active_workers` 视图状态。`src/worker/manager.rs` 与 `synapse-services/src/worker/manager.rs` 的 fallback 选择逻辑已改为优先挑选最新 heartbeat 的兼容 worker，并在 `health_checker` 存在时优先转向健康候选、仅在没有健康候选时才退回全部兼容实例；本轮还把 heartbeat 与内存运行时状态对齐，确保 worker 进入 `running` 时刷新 load balancer/health checker 注册，进入 `stopping/stopped/error` 时移出候选集，避免 DB 已停止但 load balancer 仍继续挑选旧实例。对应 focused 单测已覆盖 freshest heartbeat、missing heartbeat 以及 healthy-over-unhealthy fallback 三个边界。另新增 `tests/integration/worker_task_recovery_tests.rs` 中的五条 `select_worker_for_task()` / task recovery integration 场景，分别验证 LeastConnections 先命中不健康实例时最终会回退到健康 frontend worker、recovering worker 从未通过健康检查恢复到 `Healthy` 后会重新被选回、worker 发送 `stopped` heartbeat 后会释放运行中任务并从 load balancer 候选集中移除、worker 发送 `error` heartbeat 后也会触发与 `stopped` 等价的任务回退与候选剔除，以及 worker 发送 `stopping` heartbeat 后会进入优雅 drain：保留已在途任务，但拒绝新的候选选择与任务 claim；同文件也继续验证“worker 注销后任务不会卡死，而是可被另一台 running worker 重新 claim”以及“多 worker 下 active worker 可见性与 replication position 按 worker 隔离”。与此同时，`scripts/deployment_smoke_test.sh` 已把 `stopping` 与 `error` heartbeat 的最小运行时 contract 纳入 smoke baseline：前者要求 worker 从 active/LB 候选中退出但允许在途任务完成，后者要求已占有任务回退为 pending 并允许 peer worker 重新 claim。为避免共享 `OnceCell` 测试池在多条 `#[tokio::test]` 之间复用时出现连接获取超时，这组 worker focused integration 现已改为每条用例使用独立 schema 的 isolated pool，整文件串行与默认线程回归均已稳定通过。
- **当前验证**：上游 `element-hq/synapse` 在 `docs/workers.md` 中明确把 worker 扩展建立在 `instance_map`、HTTP replication listener、Redis replication/pub-sub、worker-specific config 与反向代理路由分工之上；当前仓库虽已有 `src/bin/synapse_worker.rs`、Redis task queue、若干 worker/replication 入口与 metrics，但 `synapse_worker` 仍主要表现为通用后台作业消费者，没有形成按路由/职责切分的 worker ownership、实例映射与运维配置闭环。本轮已将 `WorkerType` 的 `responsibility_domains`、`instance_map_keys`、`owned_route_prefixes`、`replication_streams` 与 `WorkerCapabilities` 显式透出到 admin worker API，并新增 `/_synapse/worker/v1/topology` 作为最小 topology baseline 输出，先把 worker ownership / instance_map 从“隐式代码知识”提升为“运行时可见基线”；随后又补入 `split_minimal` 的 listener 规划、Nginx 反向代理样例与 smoke test 基线。最近几轮进一步新增 `/_synapse/worker/v1/topology/validate`，把启动期 validator 的真实计算结果以结构化 JSON 暴露给运维面，并让 `scripts/deployment_smoke_test.sh` 显式校验 `valid=true` 与 `validation.errors`；同时将 `stream_writers` 从“仅检查 owner 是否存在”升级为“校验 owner 是否符合当前 topology baseline 的写流职责”（例如 `events` 仅允许 `master/event_persister`，其它当前未建模流默认仅允许 `master`），并将 `assign_task(preferred_worker_id=...)`、显式 `claim_task(...)` 与 `claim_next_pending_task(...)` 三条任务分配路径统一收口到 worker capability 校验，避免把 `event_processing`、`sync`、`background` 等任务错误分配给不具备能力的 worker；前一轮已补入 `src/worker/manager.rs` focused 单测，直接断言“运行中且能力兼容的 worker 可以通过 ownership 校验”和“即使 task type 兼容，非 running worker 也不得 claim/own 任务”，把 claim 边界从实现约束提升为可回归验证的证据；本轮又补入 `src/worker/health.rs` focused 单测，并让 `recovery_threshold` 真正参与状态迁移，直接验证 worker 健康状态会从 `Unhealthy` 经 `Degraded` 逐步恢复到 `Healthy`；进一步把 `/sync`、`/_matrix/media/v3/config` 与 `/_matrix/federation/v1/version` 三条高价值 route probe 的 `X-Synapse-Route-Owner` 从“topology 期望值回显”收口为“当前实际服务实例的 worker type”，并在 `/_synapse/worker/v1/topology/validate` 中继续输出 `route_owner_expectations` 作为期望值来源，使 deployment smoke test 能够真正发现 reverse proxy 把请求打到错误实例的情况；与此同时，仓库新增 `docker/config/homeserver.split-minimal.yaml`、`docker/docker-compose.split-minimal.yml`、`docker/nginx/split-minimal.conf`、`docker/config/.env.split-minimal.example`、`docker/config/split-minimal.smoke.env` 与 `docker/run_split_minimal_smoke.sh`，首次把 listener / reverse proxy / smoke 样板与 `up -> 获取 admin token -> smoke -> down` 串成一条可直接执行的联调链路，并补足首次运行所需的环境样板与失败时的 `compose ps` 观测。
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
  1. ✅ 已完成：为审计/技术债文档增加"最后验证时间"和"证据来源"字段（本报告已具备，`M3_PROGRESS.md` 已同步更新）。
  2. ✅ 已完成：PR 模板 `.github/pull_request_template.md` 已增加"文档状态同步"检查项（覆盖 `SUPPORTED_MATRIX_SURFACE.md`、`COMPREHENSIVE_AUDIT_REPORT`、`route_ledger`、分层 ledger 与依赖管理）。
  3. ✅ 已完成：新增 `scripts/ci/check_release_doc_spotcheck.sh`，对 current 文档的 freshness/baseline 标记和 PR 模板检查项做 release 前 spot-check；脚本同时会对已知 stale 断言输出 advisory warning，并支持通过 `--strict` 或 `STRICT_WARNINGS=1` 将告警升级为失败；在发现告警/失败时还会直接输出下一步处置建议，PR 模板也已加入该脚本执行项。
  4. ✅ 已完成：6 个历史文档已归档至 `docs/synapse-rust/archive/`（`OPTIMIZATION_AND_DEDUPLICATION_PLAN_2026-04-21.md`、`REDUNDANCY_CLEANUP_LOG_2026-05-28.md`、`REDUNDANT_TABLE_DELETION_PLAN.md`、`SYNAPSE_RUST_OPTIMIZATION_BLUEPRINT_2026-05-27.md`、`SYNAPSE_UPSTREAM_RESEARCH_2026-05-27.md`、`E2EE_VODOZEMAC_MIGRATION.md`），附归档 README 说明原因。
- **责任节点**：文档负责人、架构负责人、发布负责人。
- **资源投入**：文档 0.5 人周，发布流程 0.5 人周。
- **验收标准**：
  - 功能可用性：团队能基于 current 文档直接完成一次排期评审而不需二次纠偏。
  - 性能指标：文档 spot-check 不超过每次发布前 0.5 人日。
  - 代码质量：current 文档中的关键结论抽样 100% 可被命令或代码定位证实。
  - 资源利用率：归档后 current 文档数量下降，阅读成本明显下降；发布前文档审查工时可控。

### P2-08 重复依赖版本需要专项清理，避免长期积累成供应链负担

- **当前验证**：`cargo tree -d --workspace` 已确认重复依赖。本轮已消除 `base64 v0.21.7`（通过禁用 `config` 的 `ron` feature，同时连带消除 `toml_datetime`/`toml_edit`/`winnow` 重复）。2026-06-17 再次 `cargo update` 统一 6 个兼容依赖（h2 v0.4.14→v0.4.15, smawk v0.3.2→v0.3.3, syn v2.0.117→v2.0.118, time v0.3.47→v0.3.49, time-core v0.1.8→v0.1.9, time-macros v0.2.27→v0.2.29）。剩余重复版本：`getrandom`（3 版本）、`hashbrown`（3 版本）、`itertools`（3 版本）、`rand`（3 版本）、`prost`（2 版本）、`core-foundation`（2 版本）、`hashlink`（2 版本）、`nom`（2 版本）、`socket2`（2 版本），均为深层传递依赖，需等待上游 crate 版本升级。
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
3. **功能缺陷修复：P1-03 root/canonical 双轨账本与显式依赖治理**
  - 依赖 P0-02 的 appservice 主链路先稳定；当前已补齐 overlap ledger、CI 入口、`pub use crate::storage::*` 禁线硬校验，并完成多批顶层单文件 service facade 净化、`auth/password_policy.rs` 收口、`database_initializer` 模块级 canonical 收口，以及 `geo_ip/`、`content_scanner/`、`identity/`、`push/`、`webhook_notification/`、`rtc/` 等多组连续收口，`services` 的 full_impl 已显著下降。本轮进一步把 `friend_room_service` 从“root/canonical 双份大实现”推进到“canonical 主实现 + root 薄包装”模型：canonical 侧已落地 `FriendRoomRoomOps`、`FriendFederationSender` 与 `FriendRoomCreateRoomConfig`，root 侧 `RoomService` / `FriendFederationClient` 仅实现最小 trait 并承担最小构造适配，`synapse-web` 与 root `src/web/routes/{dm,friend_room}.rs` 的 direct-DM 调用点都已切换到中性 DTO；随后又将 `media/chunked_upload` 的 root `media/mod.rs` 收口为纯 facade，并继续在 `synapse-storage/src/media/chunked_upload.rs` 新建 `ChunkedUploadStorage`，把 canonical `synapse-services/src/media/chunked_upload.rs` 中对 `upload_progress` / `upload_chunks` 的 direct SQL 下沉到 storage 层；之后又为 `account_data_service` 新增 `AccountDataStorage` facade 与 `RoomAccountDataStorage` 的 typed get/delete 能力，把 service 层对 `account_data` / `room_account_data` 的直接 SQL 回收到 storage；再继续将 `oidc_mapping_service` 切换为复用既有 `OidcUserMappingStorage`，并把 `synapse-web/src/routes/oidc.rs` 中重复的 OIDC 绑定 SQL 收口为统一 service 调用；随后将 `client_push_service` 中 `pushers` / `push_rules` / `notifications` / `m.push_rules` 这组 direct SQL 统一切换到 `PushStorage` 与 `AccountDataStorage` facade，同时补回 root/canonical `EventBroadcaster`、`FriendFederationClient` 与 space 分页接口的最小兼容层；继续新增 `RateLimitStorage` 并为 `UserStorage` 补入 `set_shadow_ban(...)`，把 root `admin_security_service` 以及 canonical `synapse-web/src/routes/admin/security.rs` 中针对 `users.is_shadow_banned` / `rate_limits` 的重复 SQL 一并收回到 storage owner；前一轮再新增 `AdminMediaStorage`，把 root `admin_media_service` 与 canonical `synapse-web/src/routes/admin/media.rs` 中围绕 `media_metadata` 的分页、详情、删除、配额与按用户删除逻辑统一收口到 storage owner；随后把 `sync_service` 中 `account_data` / `room_account_data`、`to_device_messages` / `device_lists_stream`、`room_ephemeral` 与 unread counts 聚合切换到 storage owner；之后又新增 `E2eeAuditStorage`，把 root/canonical `audit_service.rs` 中围绕 `e2ee_audit_log` 的写入、历史读取和清理逻辑统一收口到 storage owner，并删除未进入 root `room/mod.rs` 编译链的镜像残留 `src/services/room/utils.rs`；前一轮再把 root `admin_token_service` 与 canonical `synapse-web/src/routes/admin/token.rs` 中 access token / refresh token 的列表与删除路径继续回收到 storage owner；本轮继续将 canonical `synapse-web/src/routes/account_data.rs` 中 `account_data` / `room_account_data` / `filters` / `openid_tokens` 的 direct SQL 切换为复用现有 storage owner。当前工作区 ledger 维持 `services=82 / thin_facade=69 / full_impl=13`，`storage=65 / thin_facade=65 / full_impl=0`。这意味着 P1-03 已从“逐个 facade 收口”推进到“同时回收 canonical/root service 与 web 路由中的直接存储职责，并清理未编译的镜像残留文件”。下一步优先继续处理 `sync_service/*` 之外仍保留 owner 漂移但不需要 `P4 types.rs` 先行统一的模块。
4. **架构演进项：P1-12 worker/replication 可运营模型闭环**
  - 依赖 P1-03 的分层边界继续收口；当前已先把 worker type 的 `responsibility_domains`、`instance_map_keys`、`owned_route_prefixes`、`replication_streams` 与 `capabilities` 透出到 admin worker API，并新增 `/_synapse/worker/v1/topology` 返回最小 deployment preset，形成最小 responsibility matrix / instance map 基线；同时已补入 `docs/synapse-rust/WORKER_TOPOLOGY_BASELINE_2026-06-14.md`，把当前 worker 类型矩阵、安全边界与 `monolith` / `split_minimal` 两套预设落成文档，并继续补齐 `split_minimal` 的 listener 规划、Nginx 反向代理样例与 deployment smoke test 基线。下一步继续把这些文档样板转成真实部署工件与 topology validator，而不再停留在静态概念说明。
5. **功能缺陷修复：P1-06 Matrix surface 治理闭环**
   - ✅ 已完成：全部 17 个 capability 已归入 RouteSurface/ConfigControlled，`SUPPORTED_MATRIX_SURFACE.md` 与代码一致，5 个 contract/snapshot 测试覆盖声明漂移防护。
6. **性能优化项：P2-08 重复依赖版本清理**
   - 依赖前述功能与架构面先稳定；生成完整重复依赖清单，优先处理低风险叶子依赖，避免在主链路仍波动时放大回归面。
7. **代码规范整改项：P2-07 文档证据基线治理**
   - 已完成 current 文档 evidence/freshness 基线、历史文档归档、PR 模板文档同步检查项，以及 `scripts/ci/check_release_doc_spotcheck.sh` 的 release 前 spot-check 流程化；当前脚本除硬性 metadata/baseline 检查外，也会对已知 stale wording 输出 advisory warning，并支持通过 `--strict` 或 `STRICT_WARNINGS=1` 在 release cut/CI 中提升为硬失败；脚本末尾还会输出针对告警/失败的处置建议，后续只需在发布流程中持续执行并按提示处理。
8. **通用运行时复核项**
   - 在上述核心整改稳定后，重新建立覆盖率、`cargo test --lib` 失败数、`tests/unit/` DB 依赖迁移完成度等基线；P0-02 appservice scheduler 多出口与优先级链路的 focused 运行时复核已在本轮补齐。

| 阶段 | 时间 | 目标 | 对应问题 | 负责人节点 |
|---|---|---|---|---|
| Phase A | 已完成 | 恢复 all-features 编译门禁，关闭 `feature_flags` blocker | P0-01 | 架构 + 平台 + CI |
| Phase B | 已完成 | 恢复 `test-utils` 集成测试编译门禁，关闭当前 P1-09 阻断项 | P1-09 | 架构 + 测试 + 模块 owner |
| Phase C | 已完成首版，后续剩余 1~2 周 | 补齐 appservice 配置装载、自动推送、关键桥接/边界验证、scheduler/controller 首版、容量治理首版、transaction 状态/内部指标首版与 admin statistics/telemetry/Prometheus 聚合出口，并补齐 focused 运行时复核；本轮已进一步修复 statistics 实时聚合事实来源与 recovery/mixed contention 观测证据，后续仅剩阈值调优与高负载策略治理 | P0-02 | 协议兼容 + appservice |
| Phase D | 已完成第二轮 | 关闭 `captcha_service` 运行时 panic stub，并补齐 email queue / SMS provider focused integration 基线 | P1-10 | 账号 + 通知 |
| Phase E | 已完成 | 统一 SQLX 当前基线为 live-schema primary，offline cache 改为可选增强 | P1-11 | DB + 平台 + CI |
| Phase F | 2 周 | 继续清理 root/canonical 边界债，收口 `services/mod.rs` 分层泄漏 | P1-03 | 服务层 + 存储层 |
| Phase G | 已完成部署工件与 smoke 基线，剩余 soak/recovery 验证 | 明确 worker responsibility matrix，补 worker/replication 最小可运营拓扑 | P1-12 | 架构 + 平台 + 运维 |
| Phase H | 已完成首轮（capability 归口 + surface 文档对齐） | 收敛 `ServiceContainer` 双访问面，修复 Matrix surface 文档漂移 | P1-05 / P1-06 | 架构 + Web + 文档 |
| Phase I | 持续（2026-06-17 文档归档+PR 模板+spot-check 脚本 CLI/严格模式已完成；2026-06-18 P1-13 OPERATIONS.md 监控/告警/故障定位补齐+PRODUCTION_DEPLOYMENT_GUIDE stub 归档已完成） | 文档治理与重复依赖治理常态化；运维文档基线对齐上游 | P1-13 / P2-07 / P2-08 | 文档 + 平台 + 发布 |

---

## 八、当前项目状态总评

### 8.1 可确认的当前真实问题

- `cargo check --workspace --all-features --locked`、`test-utils` integration `--no-run` 与 `cargo clippy --all-features --locked -- -D warnings` 均已恢复通过。
- appservice 已从“管理接口化”推进到“本地房间事件自动分发 + 联邦/多数已识别旁路入口覆盖 + 建房事务提交后统一分发 + 自动 sender + 基础 backoff/recoverer + 失败分类/自动隔离坏 AS + message-path / membership bridge e2e + virtual user / exclusive namespace / 管理面显式写入边界约束 + transaction controller / per-AS 调度策略首版 + 容量限流与 scheduler 状态观测首版 + transaction 聚合状态/内部指标首版 + admin statistics/telemetry/Prometheus 聚合出口首版 + statistics 实时聚合事实来源修复 + recovery/mixed contention 观测证据补齐”，但仍未达到上游 Synapse 的完整事件分发系统能力。
- root/canonical 双轨冗余仍然显著；但 `friend_room_service` 已不再停留在“仅完成语义对齐”的阶段，而是完成了 canonical 主实现 + root 薄包装的第一阶段收口。
- `captcha_service` 的 panic stub 已消除，focused integration 也已覆盖 email queue/SMS provider 两条最短运行时链路；本轮又补入 `synapse_worker` fake SMTP server 冒烟测试，因此当前剩余缺口已收敛为生产级 provider/回执校验。
- worker/replication 已有最小 instance_map / route ownership / topology preset 可见基线，`split_minimal` 的 listener / 反向代理 / smoke test 样板与部署工件（`docker-compose.split-minimal.yml`、`nginx/split-minimal.conf`、`run_split_minimal_smoke.sh`）已落地，topology validator 启动期校验、stream writer 强校验、heartbeat/replication position/task claim 的 deployment smoke 基线如今也已覆盖 stopping/error heartbeat 的最小运行时 contract；再加上 stopped/error 终态下的任务自动回退与候选剔除、stopping heartbeat 的优雅 drain 语义、多 worker active visibility / replication position 隔离、fallback 优先 freshest heartbeat 且优先健康候选、`select_worker_for_task()` 从不健康 LB 首选实例回退到健康候选并在恢复后重新选回的 focused 证据，仓内可直接验证的边界已进一步收紧，但仍缺少更长时间窗多实例下的生产级一致性验证。
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
- `captcha_service` email/sms 路径的 `todo!()` 运行时 panic。
- `.sqlx/` 离线缓存门禁失败且与仓库基线漂移。
- appservice 的 membership bridge e2e 缺口。
- virtual user 未受 exclusive user namespace 约束。
- exclusive namespace 仅记录未做真实冲突校验。
- 管理面显式 `push_event` 缺少 namespace ownership 约束。
- appservice 缺少 transaction controller / per-AS 调度策略。
- appservice 缺少容量限流与 scheduler 状态观测。
- appservice 缺少 transaction 聚合状态与 scheduler 内部指标面。
- scheduler 在 `application_service_statistics` 缺失有效 pending 计数时，会把 pending transaction 优先级退化为按 `as_id` 排序。
- ledger 夹具测试因模块结构变更导致的 fixture 漂移（已重新生成 4 个 profile 夹具，`cargo test --features test-utils --test unit` 862 passed）。
- `cargo update` 依赖版本漂移（2026-06-17 统一 6 个兼容依赖）。
- P2-07 文档证据基线治理：6 个历史文档已归档至 `docs/synapse-rust/archive/`，PR 模板 `.github/pull_request_template.md` 已创建并包含文档状态同步检查项。
- P1-03.2 capability 声明治理：全部 17 个 capability 已归入 RouteSurface/ConfigControlled，5 个 contract/snapshot 测试覆盖声明漂移防护。
- P1-12 worker 部署工件：`docker-compose.split-minimal.yml`、`nginx/split-minimal.conf`、`run_split_minimal_smoke.sh`、`topology_validator.rs` 全部就位。
- P1-06 surface 一致性：`SUPPORTED_MATRIX_SURFACE.md` 与 `versions.rs` 代码 capability 声明一致。
- P1-03.1 facade 收口主 lane 已完成（2/2 facade）：`test_config.rs` 已收口为 thin facade；canonical `container.rs` 已补齐为完整实现（9 个缺失字段 + `database_pool()` + worker topology 检查）；本轮继续完成独立拆出的 Phase 4，已先收口 e2ee/federation 的 import-path-only 差异，再完成 `CrossSigningService`、`SecretStorageService`、`FriendFederation` 的 trait object 边界对齐，root `container.rs` 已收口为 thin facade。
- P1-11 OIDC 配置文档：`homeserver.yaml` 已补充三种模式说明、回调安全校验清单与运维说明。
- P1-12 部署 smoke test：`scripts/deployment_smoke_test.sh`（827 行）已覆盖 heartbeat/replication position/task claim/backlog drain/route owner/recovery 全链路。`scripts/deployment_soak_test.sh`（408 行）已实现持续运行 soak 流程。

### 8.3 本轮未做最终定论、需运行时再复核的项

- 当前真实覆盖率与 mutation baseline。
- ✅ `cargo test --lib` 当前失败测试数：**2026-06-19 已复核** — `cargo test --lib --all-features --locked` 结果为 `1248 passed; 0 failed; 1 ignored`，无失败用例。
- `tests/unit/` DB 依赖迁移是否已完全完成：**2026-06-19 已复核** — `tests/unit/` 共 70 个测试文件，其中 9 个文件引用 DB 相关类型（`PgPool`/`TestDatabase` 等），均为合法的 DB 集成测试（`new_features_tests`、`worker_tests`、`room_summary_tests`、`event_report_tests`、`background_update_tests`、`module_tests`、`retention_tests`、`refresh_token_tests`、`registration_token_tests`），不属于"待迁移"范畴。`cargo test --features test-utils --test unit --locked` 结果为 `862 passed; 0 failed`。
- ✅ 全仓生产代码 `unwrap/expect` 的最新精确分布：**2026-06-19 已复核** — 生产代码（排除 `#[cfg(test)]` 与 `tests.rs`/`_tests.rs`）中 `unwrap/expect` 分布为 `src=0, synapse-common=140, synapse-cache=15, synapse-storage=94, synapse-federation=54, synapse-e2ee=146, synapse-services=306, synapse-web=163`，总计约 918 处（含部分文件内嵌测试代码混入）。`todo!()`/`unimplemented!()` = 0；`panic!()` = 30 处，全部位于测试代码中（`#[cfg(test)]` 模块或 `*_tests.rs` 文件），无生产 panic。

---

## 九、结论

当前 synapse-rust 的核心矛盾已经从“若干早期致命 SQL/协议错误”转移为：

1. **`cargo check --workspace --all-features --locked`、`cargo test --features test-utils --test unit`（862 passed）、`cargo clippy --all-features --locked -- -D warnings` 均已通过，root/canonical 双轨 ledger 已收敛至 `services=2 (facade=2, full_impl=0)`，`storage=55 (facade=55, full_impl=0)`。facade 收口主 lane 已完成（2/2 facade）：此前已完成 `test_config.rs` thin facade 收口、`worker/topology_validator.rs` 和 `worker/types.rs` thin facade 收口，并将 canonical `container.rs` 补齐为完整实现；本轮继续完成独立拆出的 Phase 4，先收口低风险的 e2ee/federation import-path-only 差异，再处理高风险的 `CrossSigningService`、`SecretStorageService`、`FriendFederation` trait object 边界，最终将 root `container.rs` 收口为 thin facade，结构性瓶颈已清零**。
2. **appservice 已形成基础调度闭环，并补齐联邦/多数已识别旁路入口覆盖、建房事务提交后分发、第二层 recoverer 失败治理、实时聚合观测事实来源，以及 recovery/mixed contention 的多出口证据，但仍未达到上游 Synapse 的完整架构能力**。
3. **注册验证码运行时 stub 已消除，focused integration 已补齐 email queue/SMS provider 两条最短运行时闭环，SQLX 基线也已统一为 live-schema primary；当前剩余缺口是更贴近生产的 SMTP worker 与真实短信 provider 联调**。
4. **worker/replication 已从"纯隐式代码知识"推进到"instance_map / route ownership / topology preset 可见基线 + split_minimal 部署工件 + topology validator 强校验 + deployment smoke 基线 + deployment soak 持续验证"，已形成最小可执行闭环**。
5. **capability 声明治理已闭环：全部 17 个 capability 归入 RouteSurface/ConfigControlled，5 个 contract/snapshot 测试覆盖声明漂移防护，`SUPPORTED_MATRIX_SURFACE.md` 与代码一致**。
6. **文档治理已开始收敛：6 个历史文档归档，PR 模板已创建，审计报告证据基线已建立，release 前文档 spot-check 脚本也已落地；后续只需在发布流程里持续执行**。

因此，下一轮治理不应继续把重点放在已经修复的历史问题上，而应优先按本报告的 P0/P1 清单处理当前真实阻断项与结构性短板。

---

## 十、参考 `element-hq/synapse` 的追加复核（2026-06-14）

本轮额外参考上游 GitHub 仓库中的 `README.rst`、`docs/workers.md` 与 `docs/application_services.md`，对当前项目仍存在的问题做了一轮“面向可运营闭环”的补充复核。

### 10.1 Worker/Replication 与上游仍有可运营差距，但最小可执行闭环已形成

- **上游基线**：`docs/workers.md` 明确把多 worker 部署建立在 `instance_map`、HTTP replication listener、Redis pub/sub、worker-specific config 与 reverse proxy 路由分工之上。
- **当前实现**：`src/worker/types.rs` 与 `src/web/routes/worker.rs` 已把 `instance_map_keys`、`owned_route_prefixes`、`replication_streams` 与 `/_synapse/worker/v1/topology` 暴露出来；`src/worker/topology_validator.rs` 已提供启动期 topology validator 与 stream writer/route owner/background owner 强校验；`docs/synapse-rust/WORKER_TOPOLOGY_BASELINE_2026-06-14.md`、`docker/docker-compose.split-minimal.yml`、`docker/nginx/split-minimal.conf`、`docker/run_split_minimal_smoke.sh` 与 `scripts/deployment_smoke_test.sh` 也已把 `split_minimal` listener / reverse proxy / heartbeat / replication position / task claim / route owner probe 串成可执行的最小部署与联调链路。
- **剩余问题**：当前缺口已从“没有可执行基线”收敛为“缺少更长时间窗、多实例、带恢复/漂移观测的 soak test 与运维手册沉淀”；也就是说，主问题不再是静态样板缺失，而是生产级持续验证与故障手册仍不够完整。

### 10.2 Capability 声明治理已完成首轮保守口径收敛

- **上游基线**：Synapse 对 `/_matrix/client/versions` 与 `/_matrix/client/v3/capabilities` 的公开声明整体较保守，强调“仅声明已稳定、可验证、可审计”的 surface。
- **当前实现**：`src/web/routes/handlers/versions.rs` 已引入 `CapabilityGovernance`，并把全部 17 个 capability 收口到 `route_surface` / `config_controlled` 两类治理。
- **当前验证**：`SUPPORTED_MATRIX_SURFACE.md` 与 `versions.rs` 的 capability 声明一致（10 public + 8 authenticated-only），并由 `test_versions_response_snapshot_keys`、`test_capabilities_response_snapshot_public_surface`、`test_capabilities_response_snapshot_authenticated_surface`、`test_all_capabilities_have_governance_classification` 与 `test_no_residual_static_stable_governance` 五条 contract/snapshot 测试共同兜底。
- **剩余问题**：当前不再存在 `StaticStable` 残留；后续若继续对齐上游更保守口径，重点将是把部分 `RouteSurface` 能力升级为更强的事实驱动，而不是继续清理治理分类本身。

### 10.3 OIDC/MAS 生产基线仍弱于上游部署心智

- **上游基线**：`README.rst` 已把 Synapse 放进 ESS/MAS 的生产部署语境，强调文档化的安装、反向代理、配置与升级路径。
- **当前实现**：项目已有 external OIDC、builtin OIDC provider 与 SAML/CAS 分层入口，`src/web/routes/oidc.rs` 也已把 OIDC 路由是否启用收敛为显式条件。
- **剩余问题**：`src/services/container.rs` 仍明确警告 builtin OIDC 仅用于 development/testing，说明当前 OIDC 更接近“可运行能力 + 开发态 fallback”，尚未形成对标 MAS/生产 IdP 的完整运维与安全基线。

### 10.4 Captcha/通知交付链仍缺真实 provider 闭环，但 focused integration 已补齐

- **上游基线**：Synapse 文档与 ESS README 均把“可部署、可配置、可运维”作为默认前提，而不是只停留在 API 存在。
- **当前实现**：`synapse-services/src/captcha_service.rs` 已清除 panic stub，并把 email captcha 接到后台任务队列；`synapse-services/src/sms_provider.rs` 与 `synapse-common/src/config/sms.rs` 也已提供 SMS provider 抽象与配置入口；`tests/integration/captcha_tests_migrated.rs` 已补齐“未配置 email provider 显式失败”“已配置 email 成功入后台任务”“已配置 SMS provider 成功投递并记录 provider”三条 focused integration。
- **剩余问题**：当前缺口已从“发送边界会 panic / 没有运行时证据”进一步收敛为“生产级短信 provider 接入与回执校验仍未完成”；也就是说，代码侧闭环已基本建立，但运维级交付闭环仍需继续补齐外部 provider 联调。

### 10.5 AppService 已逼近生产闭环，但 worker 化与运维闭环仍落后于上游

- **上游基线**：`docs/application_services.md` 明确以 `app_service_config_files`、exclusive namespace、独立 AS 配置和明确 ownership 作为基线；结合 `docs/workers.md`，上游的 appservice 也天然位于多进程/运维模型中。
- **当前实现**：本项目已经完成 `app_service_config_files` 启动期加载、exclusive namespace 关键校验、pending queue / transaction / scheduler / statistics / telemetry 的一整轮补强。
- **剩余问题**：appservice 方向当前主要缺口已不在“单点代码逻辑”或“部署 smoke test 缺失”，而在 worker topology、生产压测阈值、故障恢复手册，以及更长时间窗多实例下的 owner/恢复一致性闭环。

---

## 十一、未完成优化任务清单

### 11.1 P0

1. **P0-02.1 appservice 生产压测与阈值调优**
   - 对 `MAX_SERVICES_PER_TICK`、`HIGH_PENDING_TRANSACTION_THRESHOLD`、retry backoff、mixed backlog 进行持续压测。
   - 输出明确的指标阈值、回退条件、容量建议与生产默认值。

2. **P0-02.2 appservice 运维闭环**
   - 为 appservice scheduler / transaction controller / recoverer 增加故障定位手册、告警阈值与真实 smoke test。
   - 补齐多实例/长时间积压场景下的恢复验证，而不只停留于 focused integration。

### 11.2 P1

3. **P1-03.1 root/canonical 双轨继续收口**
   - `sync_service/data_fetch.rs` 的高价值 direct SQL 读面已基本收口，`e2ee_audit` 的 audit log owner 已切回 storage，admin token 路由/服务中的 registration token 列表/创建/详情/更新/删除与 access token / refresh token 列表/删除路径、canonical `account_data` 路由中的 `account_data` / `room_account_data` / `filters` / `openid_tokens` 访问、canonical `tags` 路由中的 `room_tags` 读写、canonical `push_rules` 默认读取路径中的 `m.push_rules` `account_data` 访问，以及 canonical `push.rs` 中 `push_rules` 子组的 `m.push_rules` 读取、规则列表/CRUD 与 `actions/enabled` 更新、`notifications` 子组的列表读取与 ack 更新、`pushers` 子组的列表/upsert/delete、canonical `pinned.rs` 中 pinned events 的读取与写入、canonical `guest.rs` / `auth_compat.rs` 中 guest 注册路径的 `is_guest` 标记与 canonical `guest.rs` 中 guest upgrade 路径的账号升级写入、canonical `ephemeral.rs` / `handlers/room/receipts.rs` / `typing.rs` 中 `room_ephemeral` 的读取、receipt 同步写入与 typing 临时事件写入/清理、canonical `admin/register.rs` 中 shared-secret 注册成功后的 `user_type` 持久化、canonical `admin/retention.rs` 中 server/room retention policy 与 retention status 汇总读写、canonical `admin/server.rs` / `handlers/health.rs` 中基础探活与 schema required tables 检查、canonical `admin/report.rs` 中 `event_reports` 的列表/详情/删除访问、canonical `admin/user.rs` 中 `v2 users` 列表/详情、`create_or_update_user_v2`、用户统计、批量创建/停用与账号更新路径，以及 canonical `device.rs` / `federation/membership.rs` 中设备列表 stream 位置与 cross-signing key 读取，也已继续回收到 owner，其中 `pinned.rs`、`ephemeral.rs`、`receipts.rs` 与 `typing.rs` 直接复用了 canonical `RoomService::get_pinned_event_ids()` / `set_pinned_event_ids()`、`get_ephemeral_events_for_client()`、`send_receipt()`、`get_receipts()`、`set_read_markers()`、`set_typing_ephemeral_event()` 与 `clear_typing_ephemeral_event()`，guest 注册、upgrade 与 admin register 的用户属性写入直接复用了既有 `UserStorage::set_guest_status(...)` / `upgrade_guest_account(...)` / `set_user_type(...)`，token 路由直接复用了既有 `RegistrationTokenService::get_all_tokens(...)` / `create_token(...)` / `get_token(...)` / `update_token(...)` / `delete_token(...)`，`device.rs` 与 `federation/membership.rs` 直接复用了既有 `DeviceStorage::get_max_device_list_stream_id()` / `get_max_device_list_stream_id_for_user()` 与 `CrossSigningStorage::get_cross_signing_key(...)`，retention 路由直接复用了既有 `RetentionService::get_server_policy_optional()` / `upsert_server_policy(...)` / `get_room_policy(...)` / `set_room_policy(...)` / `get_status_summary()`，基础探活路径直接复用了既有 `state.health_checker.check_readiness()` 与 `SchemaValidator::validate_required_tables(...)`，report 路由则直接复用了既有 `EventReportService::get_all_reports(...)` / `get_report(...)` / `get_reports_by_room(...)` / `delete_report(...)`，`admin/user.rs` 则直接复用了既有 `AdminUserService::list_users_v2(...)` / `get_user_v2(...)` / `create_or_update_user_v2(...)` / `get_user_stats()` / `get_single_user_stats(...)` / `batch_create_users(...)` / `batch_deactivate_users(...)` / `update_account(...)`，均无需新增最小 helper；并已顺手删除一处未编译的 root `room/utils.rs` 镜像残留；下一步继续筛选 `sync_service/*` 之外剩余 owner 漂移但不需要 `P4 types.rs` 先统一的模块，当前优先观察剩余 route 文件中的 `assembly`、`handlers/search` 与 `handlers/room/management`。
   - 本轮继续把 canonical `synapse-web/src/routes/handlers/search.rs` 中 room events search、timestamp-to-event、event context 与 room search 全部切到既有 `SearchService::search_room_events(...)` / `find_event_by_timestamp(...)` / `get_event_context_window(...)` / `search_rooms_for_user(...)`，并删除已失效的 route-local room events cursor helper；该文件中的 direct SQL 已清零。
   - 本轮继续把 canonical `synapse-web/src/routes/handlers/room/management.rs` 中 `get_room_info` / `get_room_version` / `get_joined_rooms` / `get_my_rooms` / `get_room_capabilities` 先切到既有 `RoomService::get_room_membership(...)` / `get_invited_members_count(...)` / `get_joined_rooms(...)` / `get_user_room_list(...)` / `get_room_encryption_status(...)`，随后再把 `search_room_messages(...)` 切到 `SearchService::search_room_messages(...)`，并将 `room_account_data` / `m.room.vault_data` 的读面切到 `RoomAccountDataStorage::get_room_account_data_content(...)` / `get_room_account_data_with_ts(...)`、写面切到既有 `room_storage.set_room_account_data(...)`；该文件中的 direct SQL 也已清零。
   - 本轮继续把 canonical `synapse-web/src/routes/device.rs` 中 `get_device_list_updates(...)` 剩余的 `device_lists_changes` / `devices` 读面切到既有 `DeviceStorage::get_device_list_changes(...)` 与 `get_devices_by_user_device_pairs(...)`，该文件中的 route direct SQL 也已清零。
   - 本轮继续把 canonical `synapse-web/src/routes/assembly.rs` 中 MSC4133 extended profile document 的 `account_data` 读写切到既有 `UserStorage::get_account_data_content(...)` 与 `upsert_account_data_content(...)`，从而清掉最后残留的 route direct SQL，同时保持原有字段名、大小限制与 visibility/ownership 校验不变。
   - 复核后当前 `synapse-web/src/routes` 已无 `sqlx::query` / `sqlx::query_scalar` / `QueryBuilder` 命中，canonical route direct SQL 已清零；后续若继续推进，将转向非 route 层 owner 漂移或 `P4 types.rs` 之前仍可独立收口的 lane。
   - 随后又做了一轮更广的 canonical 非 route low-risk 扫描，确认 `RoomService` 是当前最高收益的 owner 漂移收口线之一：`synapse-services/src/room/info.rs` 中 `get_room_encryption_status(...)` 已切到既有 `EventStorage::get_state_events_by_type(...)`，`synapse-services/src/room/membership.rs` 中 `get_invited_members_count(...)` 已切到既有 `RoomSummaryService::get_summary(...)`；同时又将 `synapse-services/src/auth/power_levels.rs` 中 `get_user_power_level(...)` / `get_room_power_levels_content(...)` 对 `m.room.power_levels` 的直连 SQL 切到基于现有 pool 轻量构造的 `EventStorage::get_state_events_by_type(...)`。在此基础上继续审 `media_service.rs` 后，先将 `get_media_metadata(...)` 的 `media_metadata` 读面切到既有 `AdminMediaStorage::get_media_info(...)`，随后再在 `synapse-storage/src/admin_media.rs` 中补入最小 `upsert_media_metadata(...)` owner，并将 `store_media_with_id(...)` 的 metadata upsert 写面也同步下沉，从而清掉 `synapse-services/src/media_service.rs` 中残留的 direct SQL。继续复核 `RoomService::get_user_room_list(...)` 后也确认：现有 `RoomMemberStorage::get_sync_rooms(...)` 仅覆盖 `join/leave`，`RoomSummaryService::get_summaries_for_user(...)` 仅覆盖 `join/invite` 且不直接带 membership，因此当前还不存在"不新增 owner 且保持语义不缩水"的低风险替代路径，这条 lane 暂继续保守保留。
   - **2026-06-19 federation_auth middleware 收口**：之前快扫认为 `federation_servers` / pending destinations 相关查询"尚未发现对应的 canonical storage owner"，本轮复核确认 `synapse-storage/src/admin_federation.rs` 中的 `AdminFederationStorage` 已是 canonical owner，只是缺少 middleware 所需的精确语义方法。本轮已补入 `get_server_admission_status(...)`（返回 `Option<Option<String>>`，区分"server 不存在"与"server 存在但 status 为 NULL"）与 `insert_pending_server(...)`（`ON CONFLICT DO NOTHING`）两个 storage 方法，并在 `AdminFederationService` 中补入 `check_admission(...)` 包装方法（NULL status 视为 active 以保持历史行为），随后将 root `src/web/middleware/federation_auth.rs` 与 canonical `synapse-web/src/middleware/federation_auth.rs` 中的 2 处 `sqlx::query_scalar` + 1 处 `sqlx::query` INSERT 全部切换到 `state.services.admin.admin_federation_service.check_admission(...)`。复核后 `synapse-web/src` 与 `src/web` 的 non-route direct SQL 均已清零（此前各 2 处，共 4 处）。同时修复了 `scripts/shell_routes_allowlist.txt` 中 `sticky_event.rs` / `voip.rs` 的行号漂移（144→147、164→168、233→226、311→297），恢复 `placeholder_scan_tests` 通过。
   - 目标是继续压低 `services full_impl`，并把剩余跨层 direct SQL / 重复 owner 收回 storage 或 canonical service。
   - **2026-06-19 状态分析**：当前 ledger 已收敛至 `services=2 (facade=2, full_impl=0)`，`storage=55 (facade=55, full_impl=0)`。此前唯一剩余的 services full_impl `container.rs` 已完成收口，ServiceContainer 组合根已完全回归 canonical 作为唯一事实来源。
   - **本轮 Phase 4 完成**：(1) root `cache` 已收口为 `synapse-cache` facade；(2) root `e2ee/federation` 中低风险的 import-path-only 差异已优先收口；(3) `CrossSigningService`、`SecretStorageService`、`FriendFederation` 的 trait object 构造边界已按 canonical 方式对齐；(4) root `container.rs` 已切换为 `pub use synapse_services::container::*;` thin facade。
   - **结论**：P1-03.1 的 facade 收口主 lane 已完成 4/4 阶段，Phase 4 独立任务已落地完成。当前 root/canonical 双轨在 services 层已不存在 `full_impl` 重叠文件，后续重点从“清除结构性瓶颈”转向“维持 facade 边界、避免回归”和继续处理非 facade 方向的架构治理项。

4. **P1-03.2 capability 声明治理**
   - ✅ 已完成：全部 17 个 capability 已归入 RouteSurface/ConfigControlled 两类治理，无遗留 StaticStable，由 `test_no_residual_static_stable_governance` 合约测试保障。后续可按需将 `m.change_password` 等 RouteSurface 能力从 route manifest 检查升级为更强的事实驱动（如 account compat 路由是否存在），但当前治理分类已完整闭环。
   - ✅ 已完成：`test_versions_response_snapshot_keys`、`test_capabilities_response_snapshot_public_surface`、`test_capabilities_response_snapshot_authenticated_surface`、`test_all_capabilities_have_governance_classification`、`test_no_residual_static_stable_governance` 共 5 个 contract/snapshot 测试已覆盖声明漂移防护。

5. **P1-06 Matrix surface 文档与实现一致性**
   - ✅ 本轮已确认：`SUPPORTED_MATRIX_SURFACE.md` 与 `versions.rs` 代码 capability 声明一致（10 public + 8 authenticated-only），治理规则、room version 矩阵、contract/snapshot 测试证据链均已同步。后续持续核对即可。

6. **P1-10 captcha/注册安全交付链闭环**
   - ✅ 已完成：focused integration 已覆盖"未配置 provider 显式报错""email captcha 成功入后台任务""SMS provider 成功投递并记录 provider"三条最短运行时链路。
   - ✅ 已完成（SMS 配置文档）：`homeserver.local.yaml` 已补齐 `sms:` 配置段（含 Aliyun/http provider 说明、安全建议、速率限制字段）。
   - ✅ 已完成（Aliyun 测试）：`synapse-services/src/sms_provider/aliyun.rs` 已补齐 3 组 wiremock 测试（send_success、send_failure、signature_deterministic），覆盖签名请求参数与错误响应处理。
   - ✅ 已完成（SMTP runbook）：`OPERATIONS.md` 已补齐"邮件投递 Smoke Test"runbook（worker 启动、SMTP 配置、触发 captcha、日志验证、MailHog 本地测试）。
   - 进行中（运行时验证）：真实 SMTP worker 冒烟已由 `synapse_worker` fake SMTP server 自动化测试覆盖；剩余生产级 SMS provider 回执校验仍需真实 SMS 基础设施。

7. **P1-11 OIDC/MAS 生产基线补齐**
   - ✅ 已完成（代码侧）：external OIDC（PKCE + state 管理 + 回调 URL 安全校验）与 builtin OIDC（discovery + JWKS + authorize/token/login）已明确区分，回调安全校验已实现（`is_safe_redirect_url`、`validate_state_pkce_binding`、one-time session、`security_audit` localpart 冲突追踪）。
   - ✅ 已完成（安全加固）：external OIDC `verify_pkce` 已从非恒定时间字符串比较（`==`）切换为 `synapse_common::crypto::secure_compare`，与 builtin OIDC PKCE 验证保持一致，消除时序攻击风险。
   - ✅ 已完成（文档侧）：`homeserver.yaml` 已补充 OIDC 三种模式说明、回调安全校验清单与运维说明（HTTPS 要求、issuer 一致性、环境变量注入、builtin 模式 `.well-known` 行为、单元测试覆盖）。
   - ✅ 已完成（测试覆盖）：`is_safe_redirect_url` 已补齐 7 组单元测试（相对路径、HTTPS hostname、危险 scheme、localhost/loopback、raw IP、protocol-relative、空/未知 scheme），覆盖 root 与 canonical 两份 `oidc.rs`。
   - ✅ 已完成（运维文档）：`OPERATIONS.md` §5.6 已补齐 OIDC/SSO 故障定位（5 步排查命令 + 常见问题表 + 生产 IdP 对接测试计划）。
   - 待开始（运行时验证）：生产级 IdP 对接压测与多 IdP 并发场景验证（需真实 IdP 基础设施，测试计划已文档化于 OPERATIONS.md §5.6）。

8. **P1-12 worker/replication 可运营模型闭环**
   - ✅ 已完成：`WORKER_TOPOLOGY_BASELINE_2026-06-14.md` 中的 listener / reverse proxy / smoke test 样板已转成可执行部署工件（`docker/docker-compose.split-minimal.yml`、`docker/nginx/split-minimal.conf`、`docker/run_split_minimal_smoke.sh`）。
   - ✅ 已完成：`src/worker/topology_validator.rs` 已实现 topology validator，启动时校验 route owner / stream writer / background owner。
   - ✅ 已完成：`scripts/deployment_smoke_test.sh`（827 行）已覆盖多实例下 heartbeat、replication position、task claim、claim_next_task、backlog drain、route owner probe、topology API 一致性、worker unregister/recovery 的 deployment smoke 基线。
   - ✅ 已完成：`scripts/deployment_soak_test.sh`（408 行）已实现持续运行 soak 流程：周期性验证（可配置间隔/时长）、拓扑漂移检测（跨周期快照比对）、worker 心跳连续性（>5min 视为 stale）、replication position 一致性、连续失败容忍与提前退出、SIGTERM/SIGINT 优雅关闭。
   - ✅ 已完成（结构化输出）：`scripts/deployment_soak_test.sh` 已补齐 `SOAK_OUTPUT_DIR` 环境变量，设置后生成 `soak_report_<timestamp>.json`（结构化结果）和 `soak_report_<timestamp>.md`（人类可读摘要），含 per-cycle checks/warnings/errors。
   - ✅ 已完成（缺失工件）：`docker/config/.env.split-minimal.example` 已创建，覆盖 split_minimal 部署所需全部环境变量（DB/Redis/安全密钥/worker replication/端口映射）。
   - ✅ 已完成（运维 runbook）：`OPERATIONS.md` 已补齐"Soak Test 运行手册"（前置条件、基本运行、自定义参数表、输出报告说明、漂移解读、优雅关闭）。
   - 待开始（运行时验证）：生产环境多实例 soak 运行结果收集（需真实多实例部署环境）。

9. **P1-13 运维文档基线对齐上游部署心智**
   - ✅ 已完成：`docs/synapse-rust/OPERATIONS.md`（v1.0, 2026-06-18）已对标上游 `element-hq/synapse` README.rst / workers.md 结构，覆盖安装部署、升级、反向代理、监控告警、故障定位、备份恢复、运维脚本索引 7 大章节。
   - ✅ 已完成：监控告警章节已补齐 Prometheus metrics 端点、告警阈值参考表、OpenTelemetry 集成说明、日志配置（含 security_audit target）。
   - ✅ 已完成：故障定位章节已补齐快速定位表（7 类症状 × 可能原因 × 排查步骤 × 相关命令）。
   - ✅ 已完成：`docs/quality/PRODUCTION_DEPLOYMENT_GUIDE.md` stub 已归档至 `docs/synapse-rust/archive/`。
   - 后续持续：随部署工件演进同步更新 OPERATIONS.md。

10. **P4-01 `sync_service/types.rs` 前置条件清理**
   - ✅ 已完成：已单独比对 root/canonical `sync_service/types.rs`，确认当前不存在字段级或行为级漂移；root `src/services/sync_service/types.rs` 中原有的内联类型测试已迁出并去重到独立 `src/services/sync_service/tests.rs`，从而使 root/canonical 的测试承载形态一致。
   - ✅ 已完成：canonical `synapse-services/src/sync_service/types.rs` 中的 `SyncFilter` 已补齐 `PartialEq`，随后又将 root/canonical 两份 `types.rs` 的 `SyncServiceDeps` 内联路径类型统一收敛为通过 import 引入的 `ToDeviceStorage` / `PerformanceConfig` 写法；当前 `diff` 仅剩 crate 导入差异，正文已同构，不再存在显式语义差异。

---

## 十二、对标 Synapse v1.153-v1.155 的代码级优化（2026-06-20）

### 12.1 优化背景

基于对 element-hq/synapse v1.153.0、v1.154.0、v1.155.0rc1 的研究，识别出三类可立即落地的高价值代码级改进：

1. **Canonical JSON 规范向量门禁**（对应 Synapse #19739 Rust canonical JSON 序列化器）
2. **To-Device 消息大小限制**（对应 Synapse v1.155 #19617 to-device EDU 大小限制）
3. **Access Token 缓存失效修复**（对应 Synapse #19483 refresh token 会话缓存未失效）

### 12.2 已实施优化

#### OPT-01 Canonical JSON Matrix 规范测试向量门禁 ✅

| 项 | 内容 |
|---|---|
| **对标** | Synapse #19739（Rust canonical JSON 序列化器）、Matrix Spec v1.18 § Appendices |
| **问题** | canonical JSON 实现已有基础测试，但缺少 Matrix 规范官方测试向量，无法作为跨实现一致性门禁 |
| **位置** | [canonical_json.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/canonical_json.rs#L279-L493) |
| **改动** | 新增 22 个 `spec_vector_*` 测试，覆盖：空对象/数组、Unicode 码点排序、嵌套排序、数组顺序保持、无空白、整数范围边界（2^53±1）、浮点拒绝、控制字符转义（U+0000-U+001F）、U+2028/U+2029/U+FFFD 转义、signatures/unsigned 移除、深层嵌套混合类型、负数/零、高 Unicode 保持、反斜杠/引号转义、跨键序确定性 |
| **验证** | `cargo test --lib -p synapse-common canonical_json` → 38 passed（16 原有 + 22 新向量） |
| **意义** | canonical JSON 是事件签名、联邦请求签名、server keys 签名的共同根基。规范向量门禁确保本实现与 Synapse/Dendrite/Conduit 的签名验证互通 |

#### OPT-02 To-Device 消息大小限制 ✅

| 项 | 内容 |
|---|---|
| **对标** | Synapse v1.155 #19617（限制 to-device EDU 大小，防止过长队列阻塞外发联邦事务） |
| **问题** | `send_to_device` 路由无消息大小、收件人数量限制，过大的 to-device 负载可阻塞存储队列和联邦事务分发 |
| **位置** | [e2ee_routes.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/e2ee_routes.rs#L497-L523) `send_to_device()` |
| **改动** | 新增两层限制：(1) `MAX_TO_DEVICE_RECIPIENTS=5000`（单请求收件人总数上限，跨所有用户+设备）；(2) `MAX_TO_DEVICE_PAYLOAD_BYTES=65536`（单条 to-device 消息 64 KiB 上限）。超限返回 `M_BAD_JSON` |
| **验证** | `cargo check --locked` + `cargo clippy --lib --bins -D warnings` + `cargo test --test unit`（862 passed） |
| **意义** | 防止恶意或错误的客户端发送超大 to-device 负载，保护下游存储和联邦队列。与 Synapse v1.155 的 EDU 大小治理对齐 |

#### OPT-03 Access Token 缓存失效修复 ✅

| 项 | 内容 |
|---|---|
| **对标** | Synapse #19483（使用 refresh token 的会话 access token 缓存未失效） |
| **问题** | `logout()` 路径将 access token 加入黑名单并从 DB 软删除（`is_revoked=TRUE`），但未调用 `cache.delete_token()` 主动移除缓存条目。虽然 `validate_token()` 的黑名单/revoked 检查在缓存命中前执行（安全正确），但缓存条目会残留最长 5 分钟（TOKEN_CACHE_TTL_SECS），浪费内存且增加缓存不一致风险 |
| **位置** | [session.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/auth/session.rs#L20-L24) `logout()` |
| **改动** | 在 `delete_token()` DB 调用后新增 `self.cache.delete_token(access_token).await`，主动失效本地 + Redis + 跨实例广播 |
| **验证** | `cargo check --locked` + `cargo clippy --lib --bins -D warnings` + `cargo test --test unit`（862 passed） |
| **意义** | 确保 logout 后 access token 立即从所有缓存层移除，释放内存并保持缓存与 DB 一致。`change_password()` 和 `logout_all()` 不需要额外修改，因为 `validate_token()` 的 `is_token_revoked` DB 检查和 logout marker 机制已正确覆盖 |

### 12.3 已实施的第二轮优化（2026-06-20 第二批）

#### OPT-04 `m.direct_to_device` 联邦 EDU 处理 ✅

| 项 | 内容 |
|---|---|
| **对标** | Matrix Spec v1.18 § Federation API → EDUs |
| **问题** | `EduType` 枚举仅支持 `m.typing`/`m.presence`/`m.device_list_update`，`m.direct_to_device` EDU 被当作未知类型静默跳过，导致来自其他联邦服务器的 to-device 消息被直接丢弃，破坏跨服务器 E2EE 密钥交换 |
| **位置** | [edu.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/federation/edu.rs#L285-L426) |
| **改动** | (1) `EduType` 新增 `DirectToDevice` 变体 + `FromStr` 匹配；(2) 新增 `handle_direct_to_device_edu()` 处理函数，验证 sender 匹配 origin、解析 `content.messages` 映射、逐收件人调用 `to_device_service.send_messages()` 持久化；(3) 新增 `MAX_FEDERATION_TO_DEVICE_RECIPIENTS=5000` 和 `MAX_FEDERATION_TO_DEVICE_MSG_BYTES=65536` 两层大小限制；(4) 新增 3 个 metrics counter（processed/dropped/error） |
| **验证** | `cargo check --locked` + `cargo clippy --lib --bins -D warnings` + `cargo test --test unit`（862 passed） |
| **意义** | 修复跨服务器 E2EE 密钥交换的联邦互通缺口，使 room key sharing、verification 等 to-device 流程在联邦场景下正常工作 |

#### OPT-05 MSC4452 Preview URL Capability ✅

| 项 | 内容 |
|---|---|
| **对标** | Synapse v1.154 #19715（MSC4452 Preview URL capabilities API） |
| **问题** | 缺少 capability 驱动的功能开关，客户端无法通过 `/capabilities` 发现 preview_url 功能是否可用 |
| **位置** | [experimental.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/config/experimental.rs#L12-L22)（配置）、[versions.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/handlers/versions.rs#L428-L435)（capability 声明）、[media.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/media.rs#L628-L633)（端点） |
| **改动** | (1) `ExperimentalConfig` 新增 `msc4452_enabled: bool` 配置项（默认 false）；(2) `/capabilities` 响应新增 `io.element.msc4452.preview_url` capability，值由 `msc4452_enabled` 驱动；(3) `preview_url` 端点添加 MSC4452 说明注释 |
| **验证** | `cargo check --locked` + `cargo clippy --lib --bins -D warnings` |
| **意义** | 实现 capability 驱动的功能开关，客户端可通过 `/capabilities` 发现 preview_url 可用性，与 Synapse v1.154 行为对齐 |

#### OPT-07 后台剪枝 job 框架扩展 ✅

| 项 | 内容 |
|---|---|
| **对标** | Synapse v1.152+ `device_lists_changes_in_room` 剪枝策略 |
| **问题** | 现有剪枝框架仅覆盖 3 个表（device_lists_changes/presence/one_time_keys），`to_device_transactions`、`token_blacklist`、`federation_queue` 等 append-only 表无剪枝，长期运行实例磁盘膨胀 |
| **位置** | [pruning.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-storage/src/pruning.rs#L79-L127)（新增 3 个剪枝函数）、[server.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/server.rs#L651-L670)（定时任务调用） |
| **改动** | (1) 新增 `prune_old_to_device_transactions()`（24h 保留，匹配 `TRANSACTION_MAX_AGE_MS`）；(2) 新增 `prune_expired_token_blacklist()`（仅删除有 expires_at 且已过期的条目，保留永久撤销）；(3) 新增 `prune_old_federation_queue()`（7 天保留，仅删除 sent/failed 终态条目）；(4) 3 个新函数已接入 server.rs 每日定时任务；(5) 新增 2 个单元测试验证保留常量正确性 |
| **验证** | `cargo test --lib -p synapse-storage`（265 passed，含新剪枝测试） |
| **意义** | 扩展剪枝覆盖到 6 个 append-only 表，防止长期运行实例磁盘膨胀，与 Synapse v1.152+ 剪枝策略对齐 |

#### OPT-08 Sliding Sync 性能闸门 ✅

| 项 | 内容 |
|---|---|
| **对标** | Synapse v1.153.0rc3 MSC4186 回滚教训（性能回归未被及时发现） |
| **问题** | sliding sync 无性能监控，无法检测 p50/p95/p99 延迟变化和慢请求，性能回归可能被遗漏直到影响用户 |
| **位置** | [sliding_sync.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/sliding_sync.rs#L69-L117) |
| **改动** | (1) 在 `sync()` 调用前后测量耗时；(2) 记录到 `sliding_sync_duration_ms` histogram（p50/p95/p99 可观测）；(3) 递增 `sliding_sync_requests_total` counter；(4) 当耗时超过 `performance.sliding_sync_latency_threshold_ms`（默认 5000ms）时记录 warn 日志 + 递增 `sliding_sync_slow_requests_total` counter |
| **验证** | `cargo check --locked` + `cargo clippy --lib --bins -D warnings` + `cargo test --test unit`（862 passed） |
| **意义** | 提供 sliding sync 性能可观测性和慢请求告警，作为性能回滚闸门，避免重蹈 Synapse v1.153 MSC4186 回滚覆辙 |

#### OPT-09 Worker Lock 可配置重试 + Metrics ✅

| 项 | 内容 |
|---|---|
| **对标** | Synapse v1.153.0 `WORKER_LOCK_MAX_RETRY_INTERVAL` 降至 5 秒 |
| **问题** | `acquire_lock()` 单次尝试即返回，无重试机制，高并发 worker 场景下锁争用导致频繁失败；无锁等待时间 metrics |
| **位置** | [worker.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/config/worker.rs#L22-L35)（配置）、[background_update.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-storage/src/background_update.rs#L338-L376)（重试方法）、[background_update_service.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/background_update_service.rs#L127-L156)（服务层） |
| **改动** | (1) `WorkerConfig` 新增 `lock_max_retry_interval_ms`（默认 5000ms）和 `lock_max_retries`（默认 3）配置项；(2) `BackgroundUpdateStorage` 新增 `acquire_lock_with_retry()` 方法，指数退避（100ms→200ms→400ms...，上限 `max_retry_interval_ms`）；(3) `BackgroundUpdateService` 新增 `with_lock_retry_config()` builder 方法，`start_update()` 使用带重试的锁获取 + 记录 lock_wait_ms 日志；(4) 容器构造传入 WorkerConfig 参数 |
| **验证** | `cargo check --locked` + `cargo clippy --lib --bins -D warnings` + `cargo test --test unit`（862 passed） |
| **意义** | 防止 worker lock 争用下的 CPU starvation / DoS，提供锁等待时间可观测性，与 Synapse v1.153 锁退避策略对齐 |

### 12.4 研究发现但未实施的改进

| 编号 | 改进 | 原因 |
|------|------|------|
| OPT-06 | MSC3266 稳定化对齐（移除实验开关） | 当前已实现 MSC3266，需审查是否仍有实验开关残留 |
| OPT-10 | backfill 选点优化（绝对距离优先） | 需修改联邦回填算法，属于协议实现改动 |

### 12.4 Synapse Rust 化趋势对本项目的启示

Synapse v1.153-v1.155 正在将 `Event.signatures`、`Event.unsigned`、`Event.content`、canonical JSON 序列化器、`Requester` 类等核心类型逐步 Rust 化。这验证了 synapse-rust 的全 Rust 技术路线，但也意味着：

1. **协议正确性责任更大**：synapse-rust 没有 Python 参考实现兜底，canonical JSON 规范向量门禁（OPT-01）是必要的正确性保障
2. **类型设计可借鉴**：Synapse 在 Rust 化过程中整理了 `RoomVersion` 结构体、`TypeIs` helper 等，本项目可对照审查类型设计
3. **性能优势天然存在**：全 Rust 实现无需 PyO3 桥接开销，在事件签名、canonical JSON、状态解析等热点路径上有天然性能优势

### 12.5 验证结果（2026-06-20）

- `cargo fmt --all -- --check` ✅ 通过
- `cargo check --locked` ✅ 通过
- `cargo clippy --locked --lib --bins -- -D warnings` ✅ 通过（零警告）
- `cargo clippy --locked --test unit --features test-utils -- -D warnings` ✅ 通过（零警告）
- `cargo test --lib -p synapse-common` ✅ 298 passed（含 38 个 canonical JSON 测试）
- `cargo test --features test-utils --test unit --locked` ✅ 862 passed, 0 failed

---

**报告完。**
