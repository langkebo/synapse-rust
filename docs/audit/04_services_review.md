# synapse-rust 业务逻辑层结构性代码审查

- 日期：2026-07-10
- 范围：`synapse-services/src/`（194 个 `.rs` 文件，真实业务逻辑）。`src/services/` 仅 `pub use` facade，无逻辑。
- 依据规则：`.trae/rules/project_rules.md`（已核对原文，非凭关键词猜测）
- 方法：机械扫描（raw SQL / unwrap / 时间戳 / 错误类型）+ 3 个 Explore agent 深挖循环依赖、后台任务停机、热路径缓存，均带 file:line 证据
- 一句话结论：**没有循环依赖、没有裸 SQL、错误处理高度统一（777 处 ApiError / 0 处 anyhow）、生产路径零 unwrap——真正的债是两类：①后台任务缺统一停机信号（5 个 `loop` 无 shutdown 分支，违反硬约束，P1）；②SyncService 完全没有共享缓存字段，`/sync` 每次命中 10 处未缓存的稳定读（P1）。另有一处 `account_data` 秒/毫秒时间戳混用的真 bug（P2）。**

## 总览（按严重度）

| 级别 | 数量 | 类别 |
|------|------|------|
| P0 | 0 | 无 |
| P1 | 2 类 | 后台任务无停机信号、SyncService 热路径全程未缓存 |
| P2 | 3 类 | 时间戳秒/毫秒混用、sync/sliding 读逻辑重复、3 处 `Box<dyn Error>` 未用 ApiError |
| ✅ 合规 | 4 项 | 无裸 SQL、无循环依赖、错误处理统一、生产零 unwrap/unsafe |

---

## 1. 业务逻辑直建 SQL（关注点 1）—— ✅ 合规

结论：**业务服务不碰 SQL，全部委托 storage。** 全目录仅 `media/mod.rs` 出现 `sqlx::query`，且都在 `#[cfg(test)]` 内的测试建库代码（`media/mod.rs:553` `CREATE SCHEMA`、`:566` `search_path`），生产方法（`:46-430` `quarantine_media`/`upload_media`/`delete_media_for_user` 等）全部调用注入的 storage/domain service。`database_initializer/` 内有 DDL，但那是该模块的**正当职责**（运行时建表），非业务逻辑越权。

## 2. 服务间循环依赖（关注点 2）—— ✅ 合规

结论：**严格 DAG，无环。** `container.rs:94-98` 明确文档化："依赖图是一个 DAG，因此线性构建就足够了，不存在构后回填连线（no post-construction wiring）"。跨服务持有（`Arc<XxxService>`）均单向：

| 源服务 | 依赖 | 文件:行 |
|--------|------|---------|
| `SlidingSyncService` | `TypingService` | `sliding_sync_service/mod.rs:39` |
| `FeatureFlagService` | `AdminAuditService` | `feature_flag_service.rs:13` |
| `AdminFederationService` | `FederationBlacklistService` | `admin_federation_service.rs:98` |
| `DeviceTrustService` | `Verification/CrossSigning/DeviceKey` | `wiring/e2ee.rs:86-91` |
| `MediaDomainService` | `MediaQuotaService` | `media/mod.rs:40` |
| `VoiceService` | `MediaService` | `voice_service.rs:23` |

叶子服务（`PresenceService`/`SyncService`/`ClientPushService`/`AccountDataService`/`Auth`）只持有 storage，不持有任何业务服务。**全库零 `Weak<Service>`**——不需要打破环，因为压根没有环。构建阶段拓扑序：infra → storage → e2ee → admin → federation → room → sso → core → media → extensions → account。

## 3. 错误处理统一 ApiError（关注点 3 / 规则五.4）—— ✅ 合规（3 处小瑕疵）

结论：**高度统一。** 生产代码 **777 处返回 `ApiError`，0 处 `anyhow`**，仅 3 处 `Box<dyn std::error::Error>`：

| 文件:行 | 级别 | 问题 | 修复建议 |
|---------|------|------|---------|
| `auth/trait.rs:75`、`auth/mod.rs:204`、`auth/account.rs:264` `generate_email_verification_token` | P2 | 同一个方法（trait + 2 实现）返回 `Result<String, Box<dyn Error>>`，与全库 `ApiError` 惯例不一致 | 改 `Result<String, ApiError>`，token 生成失败走 `ApiError::internal_with_log` |

## 4. 时间戳统一毫秒（关注点 4 / 规则五.3）—— P2（1 处真 bug）

规则五.3：时间戳统一毫秒级 `chrono::Utc::now().timestamp_millis()`（BIGINT）。全库 **151 处用 `timestamp_millis()`**，15 处用秒级 `timestamp()`。**绝大多数秒级用法合规**（JWT `exp`/`iat` 依规范就是秒）：

- ✅ 合规秒级（JWT/JSON，非 DB 毫秒列）：`auth/token.rs:39`（JWT exp 校验）、`oidc_service.rs:457`、`push/providers/webpush.rs:105`、`push/providers/apns.rs:116`、`rtc/sfu.rs:281,332`（LiveKit JWT `exp=now+3600`）、`friend_room_service/mod.rs:1043`（JSON `"since"` 字段）、`admin_registration_service.rs:79,115`（内部 nonce 比较，自洽）。

| 文件:行 | 级别 | 问题 | 修复建议 |
|---------|------|------|---------|
| `account_data_service.rs:100` | **P2** | **真 bug**：写 room account data 用 `Utc::now().timestamp()`（**秒**）传给 `upsert_room_account_data(...)`，而**同一文件** `:163` 用 `timestamp_millis()`（毫秒）。房间账户数据的时间戳比其余路径小 1000 倍，跨端排序/增量同步会错位 | 改 `:100` 为 `timestamp_millis()`，与 `:163` 及全库一致 |
| `auth/session.rs:94` | P2（信息） | logout marker 写秒级到 cache 字符串（非 DB 列，仅比较用） | 统一毫秒，低风险 |

## 5. 热路径查询缓存（关注点 5 / 规则十二.3）—— P1

结论：**结构性缺陷——`SyncService` 根本没有共享 cache 字段。** 它只有一个本地 `lazy_loaded_members_cache: Arc<RwLock<HashMap>>`（`sync_service/mod.rs:45`）,不接 `SharedInfra.cache`(L1 moka + L2 Redis)。因此 `/sync` 每次请求以下**稳定、可重复读全部直打 PostgreSQL、零缓存**：

| 文件:行 | 每次 /sync 读取 | 可缓存性 |
|---------|----------------|----------|
| `sync_service/filter.rs:24` `get_filter` | 按 filter_id 查 filters 表 | **P1** 极少变，key `filter:{user}:{id}` 长 TTL |
| `sync_service/data_fetch.rs:223` `list_account_data` | 拉全部 account data | **P1** key `account_data:{user}`，set 时失效 |
| `sync_service/data_fetch.rs:308` `get_max_device_list_stream_id` | 全局 `MAX(stream_id)` 单行 | **P1** 短 TTL(5s) 全局 key |
| `sync_service/data_fetch.rs:302` `get_device_lists_since_with_shared_rooms` | 多表 join | P2 |
| `sync_service/response.rs:207` `get_one_time_keys_count_by_algorithm` | 每设备 OTK 计数 | P2 per-(user,device) 30s TTL |
| `sync_service/response.rs:226` `get_rooms_needing_key_rotation` | 每用户 | P2 短 TTL |
| `sync_service/response.rs:254` `get_device_counts_batch` | 变更用户设备数 | P2 |
| `sliding_sync_service/state.rs:18` `get_state_events(room_id)` | 每房间全量 state | **P1** key `room_state:{room}`，state 变更失效 |
| `sliding_sync_service/timeline.rs:16` `get_room_events_paginated` | 每房间时间线 | P2 分页，短 TTL |
| `sliding_sync_service/extensions.rs:38` `get_global_account_data` | 直打 DB | P2 |

修复建议：给 `SyncService` 注入 `SharedInfra.cache`，优先缓存 filter / account_data / max_device_list_stream_id / sliding 的 room state（这 4 个变更率最低、命中率最高）。

**✅ 已缓存（给予肯定）**：presence（`synapse-storage/presence/mod.rs:185` 内置缓存）、lazy-load 成员（`sync_service/lazy_load.rs:14`）、sliding sync 窗口快照与 E2EE 流位（`sliding_sync_service/filters.rs:89`、`extensions.rs:211`，用 `cache.set_raw/get_raw`）。`SlidingSyncService` **有** `self.cache` 且用得好——问题集中在 `SyncService`。

## 6. SyncService / SlidingSyncService 逻辑重复（关注点 6 / 规则七.3）—— P2

两者读同一批 storage（event / account_data / device），但编排各写一套：`sync_service/`（12 文件：data_fetch/response/filter/push_rules…）vs `sliding_sync_service/`（6 文件：state/timeline/extensions/filters）。协议不同（长轮询 vs 滑窗），**不宜整体合并**，但重复的读逻辑可下沉：

- device-list-change 计算（`sync_service/response.rs:254` vs `sliding_sync_service/extensions.rs:211`）
- account data 拉取（`sync_service/data_fetch.rs:223` vs `sliding_sync_service/extensions.rs:38`）

修复建议：抽一个共享 `SyncReadHelpers`（或给 storage 加带缓存的读方法），两个 service 复用，顺带解决 §5 的缓存问题（一处加缓存，两处受益）。架构审查（`02_architecture_review.md`）已从依赖角度标注此点。

## 7. 后台任务停机信号（关注点 7 / 硬约束）—— P1

结论：**违反硬约束——无统一停机协调器。** 全库 **零 `CancellationToken`**，无中央 `TaskManager::shutdown()`。5 个生产后台 `loop` **无任何停机分支，进程只能被 SIGKILL 杀掉**：

| 文件:行 | 任务 | 停机处理 |
|---------|------|---------|
| `event_notifier.rs:152` | Redis 事件订阅（错误 1s 重连） | ❌ **无** |
| `burn_after_read_service.rs:301` | 阅后即焚处理器（5s tick） | ❌ **无** |
| `room/service.rs:184` | 房间任务清理（60s tick） | ❌ **无**（`RoomService::shutdown()` 只 abort 延迟任务，不含此 loop） |
| `application_service/scheduler.rs:404` | AS 事件调度器 | ❌ **无** |
| `worker/tcp.rs:39` | TCP 复制 accept 循环 | ❌ **无** |
| `worker/bus.rs:161` | Redis pubsub 订阅 | ✅ `disconnect()` `handle.abort()`（:243） |
| `worker/health.rs:219` | 周期健康检查 | ✅ `select!` on `shutdown_rx.recv()` |

另发现死代码：`admin_registration_service.rs:61` `start_nonce_cleanup_task` 定义了但**全库无调用方**。

修复建议：引入统一 `CancellationToken`（`tokio_util::sync::CancellationToken`），在 `container` 层创建并注入所有后台服务，每个 `loop` 改 `tokio::select!` 加 `_ = token.cancelled() => break` 分支；进程收 SIGTERM 时 `token.cancel()` 后 join 所有 handle。参考已合规的 `worker/health.rs:219` 模式。

## 8. 生产路径 unsafe / unwrap（关注点 8）—— ✅ 合规

结论：**生产路径零 unwrap、零 unsafe、零 panic。**
- 2 处 `unsafe`（`worker/topology_validator.rs:654,666`）均在 `#[test]` 内设置环境变量——测试专用。
- 4 处 `panic!`（`cas_service.rs:424,435`、`worker/protocol.rs:395,406`）均是测试断言（`_ => panic!("Expected …")`）。
- 522 处 `unwrap()` 全部落在 `*/tests.rs`、`test_mocks.rs` 或 `//!` doc-comment 示例中；逐文件核对（unwrap 出现在 `#[cfg(test)]` 之前的生产段）仅 `typing_service.rs`/`directory_service.rs` 各 2 处，实为 doc-comment 与测试。**无生产 unwrap。**

---

## 修复优先级建议

1. **P1 后台任务停机**（§7）——引入 `CancellationToken` 统一注入，5 个裸 loop 加 shutdown 分支。违反硬约束，且影响优雅重启/滚动发布时的数据一致性（阅后即焚、AS 调度中途被杀）。
2. **P1 SyncService 缓存**（§5）——给 SyncService 注入 `SharedInfra.cache`，优先 filter/account_data/max_stream_id/room_state 四项。`/sync` 是最高频端点，收益最大。
3. **P2 时间戳 bug**（§4）——`account_data_service.rs:100` 秒改毫秒，一行修复消除跨端排序错位。
4. P2 其余（sync/sliding 读逻辑下沉、3 处 `Box<dyn Error>` 改 ApiError）可与 §5 缓存改造合并做。

## 合规亮点（避免过度否定）

- ✅ 无循环依赖：严格 DAG，container.rs 文档化，零 `Weak`。
- ✅ 无裸 SQL：业务层全委托 storage，仅测试建库用 `sqlx::query`。
- ✅ 错误处理统一：777 处 ApiError，0 处 anyhow，仅 3 处历史 `Box<dyn Error>`。
- ✅ 生产零 unwrap/unsafe/panic：全部落测试与 doc-comment。

产出：docs/audit/04_services_review.md
