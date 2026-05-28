# synapse-rust 优化方案与重复实现审计

## 1. 文档目标

本文基于对上游 `element-hq/synapse` 的调研，以及对本地 `synapse-rust` 代码结构的排查，形成一份可落地的优化蓝图，覆盖以下内容:

- 本地重复实现、重复逻辑与依赖整合问题审计
- 优化目标与优先级排序
- 具体实施步骤
- 技术选型建议
- 性能验收指标
- 风险预判与应对措施

本文重点不是泛泛而谈，而是直接对应本地真实代码结构和运行时装配情况。

## 2. 审计方法与边界

### 2.1 重点检查对象

本次重点审计以下几个实际参与运行时的模块域:

- 缓存
- 私聊 / 好友
- 媒体上传
- 队列 / 后台任务 / worker
- RTC / 语音 / 通话
- OIDC / SSO
- 命名与模型边界

### 2.2 审计原则

- 只认实际代码和运行时装配，不凭模块名称猜测
- 优先检查 `src/services/container.rs` 中同时注入的能力
- 同时关注:
  - 功能重叠
  - 状态源不一致
  - 第三方依赖重复整合
  - 命名歧义带来的维护风险

## 3. 本地项目现状判断

### 3.1 总体结论

`synapse-rust` 当前不是“功能不够多”，而是“领域能力已经很多，但边界尚未收敛”。这会带来四类问题:

1. 同一个业务域存在多套入口
2. 同一类状态被内存、Redis、PostgreSQL 多处持有
3. feature gate 增长快于治理规则
4. 后续性能优化和问题定位成本明显偏高

### 3.2 已有基础是正向的

需要强调的是，本地项目已经具备很多优化前提:

- 明确的 `ServiceContainer` 依赖装配中心
- PostgreSQL + Redis 的基础设施能力
- feature gate 机制
- 已建立一定程度的 Clippy 约束
- 已有历史优化文档和部分领域收敛实践
- `room/` 域已经做过模块合并，是后续继续收敛的正面样板

因此，本轮方案不是推倒重来，而是做“第二阶段架构收敛”。

## 4. 重复实现与整合问题审计结果

## 4.1 高风险: 缓存双轨并存

### 4.1.1 现状

当前至少存在两套通用或准通用缓存实现:

- `src/cache/mod.rs` 下的 `CacheManager`
- `src/services/cache/service.rs` 下的 `CacheService`

此外还有专项缓存:

- `src/services/cache/room_cache.rs` 下的 `RoomSummaryCache`

### 4.1.2 证据

- `CacheManager` 具备 Redis、本地缓存、熔断、失效广播、查询缓存等能力。
- `CacheService` 单独实现了内存 LRU、TTL、命名空间清理、pattern 失效和统计。
- `RoomSummaryCache` 又单独维护房间摘要、成员和 presence 缓存。

### 4.1.3 问题

- 通用缓存语义重复
- TTL、命名空间、失效策略容易分叉
- 命中率、退化率、失效广播口径难统一
- 未来 Redis 退化、热 key、内存上限治理无法在一个入口完成

### 4.1.4 影响

- 同一数据可能在多个缓存栈中失效不一致
- 开发者很难判断新功能该接哪一套缓存
- 性能调优时无法快速确定瓶颈

### 4.1.5 进展 (2026-05-28)

- ✅ `CacheService` 及 `src/services/cache/` 目录已彻底移除，所有调用已迁至 `CacheManager`
- ✅ `RoomSummaryCache` 已随 CacheService 一并移除
- ✅ `MessageQueue` (`src/services/message_queue/`) 已彻底移除，已被 `RedisTaskQueue` 取代
- 当前状态：缓存双轨已收敛为 `CacheManager` 单轨

## 4.2 高风险: DMService 与 FriendRoomService 双轨维护用户关系

### 4.2.1 现状

以下两套服务同时存在且参与运行时:

- `src/services/dm_service.rs`
- `src/services/friend_room_service.rs`

`ServiceContainer` 也同时注入了 `dm_service` 与 `friend_room_service`。

### 4.2.2 证据

- `DMService` 使用内存 `HashMap` 维护 DM 房间关系。
- `FriendRoomService` 使用 `FriendRoomStorage`、`RoomService`、`EventStorage`、`PresenceStorage` 和 `CacheManager`，并在 `FriendListEntry` 中维护 `dm_room_id`、`dm_room_active`、`dm_room_state` 等字段。

### 4.2.3 问题

- 两套服务都在表达“用户间私聊/关系房间”的一部分事实
- 一个是内存态，一个是数据库+缓存态
- 语义上并不完全相同，但重叠已经足够大

### 4.2.4 影响

- 重启后 `DMService` 状态消失，和持久化好友域可能失配
- 后续要做私聊治理、未读数、关系变更审计时，边界会持续混乱

### 4.2.5 进展 (2026-05-28)

- ✅ `DMService` 已收缩为 `pub(crate)` + `#[cfg(any(test, feature = "test-utils"))]`，移出公开 re-export
- ✅ `dm.rs` 路由层已收敛：`update_dm_room`/`create_dm_room`/`get_dm_rooms` 均委托给服务层
- ✅ 路由层辅助函数已添加 `#[cfg(not(feature = "friends"))]` 隔离
- 当前状态：DMService 已降级为测试/兼容层，FriendRoomService 为持久化主实现

## 4.3 高风险: 媒体上传双轨

### 4.3.1 现状

媒体域当前至少包含两条并行上传路径:

- `src/services/media_service.rs` 的 `MediaService`
- `src/services/media/chunked_upload.rs` 的 `ChunkedUploadService`

路由层 `src/web/routes/media.rs` 也同时暴露:

- 标准 `/upload`
- `/upload/chunk/*`

### 4.3.2 证据

- `MediaService` 负责文件系统路径、缩略图目录、普通上传和带指定 `media_id` 上传。
- `ChunkedUploadService` 负责分块上传、进度跟踪、分块持久化、完成合并。
- `create_upload_provider_router` 又为后续外部存储提供了第三条扩展入口雏形。

### 4.3.3 问题

- 媒体元数据、配额、审计、生命周期清理没有统一抽象
- 上传策略已经分裂成“普通上传 / 分块上传 / 外部 provider 预留”
- 将来接对象存储会进一步扩大分叉

### 4.3.4 影响

- 配额统计和失败重试逻辑难统一
- 文件系统与数据库状态可能出现悬空记录
- 用户体验和管理 API 难做到一致

## 4.4 高风险: 队列与后台执行模型多轨并存

### 4.4.1 现状

当前至少存在以下并行抽象:

- `TaskQueue`
- `BackgroundTaskManager`
- `RedisTaskQueue`
- `services/message_queue::MessageQueue`
- `worker/*` 子系统

### 4.4.2 证据

- `TaskQueue` 是基于 `tokio::mpsc` 的内存异步任务队列。
- `BackgroundTaskManager` 基于 `TaskQueue` 再包一层 task id。
- `RedisTaskQueue` 基于 Redis 处理后台 job。
- `MessageQueue` 又是一个独立内存队列，带 publish/consume/ack/nack 语义。
- `worker/manager.rs` 则已经是完整的 worker 注册、连接、总线、负载均衡和健康检查系统。

### 4.4.3 问题

- “任务执行”和“消息排队”的抽象层次没有清晰分层
- 测试环境、单机环境、生产环境缺少标准映射关系
- 运维侧难以知道哪个队列是正式生产路径

### 4.4.4 影响

- 后台任务丢失、幂等、重试、优先级、监控指标难统一
- worker 扩容价值会被多套旁路队列稀释

### 4.4.5 进展 (2026-05-28)

- ✅ `MessageQueue` 已彻底移除，已被 `RedisTaskQueue` 取代
- ✅ `TaskQueue`/`BackgroundTaskManager` 已限缩至 `#[cfg(test)]`，生产环境统一使用 `RedisTaskQueue`
- 当前状态：生产任务主链路已统一为 `RedisTaskQueue + worker/*`

## 4.5 中高风险: RTC / 语音 / 通话领域边界过碎

### 4.5.1 现状

相关模块包括:

- `VoipService`
- `VoiceService`
- `CallService`
- `MatrixRTCService`

### 4.5.2 证据

- `VoipService` 实际是 TURN/STUN 凭证服务。
- `CallService` 管理通话邀请、应答、候选和挂断。
- `MatrixRTCService` 管理 RTC session、membership、encryption key。
- `VoiceService` 建立在 `MediaService` 之上，处理语音消息上传。

### 4.5.3 问题

- 四者职责并不完全重复，但都落在“实时通信”大域中
- 命名与接口层面缺少统一门面
- 监控口径、审计口径和路由归属不清晰

### 4.5.4 影响

- 未来接 Element Call / MSC4143 相关能力时会持续扩张
- 开发者容易误把“语音消息”“TURN 配置”“实时会话”“通话状态机”混为一谈

## 4.6 中风险: OIDC 双模式并存，边界缺少制度化说明

### 4.6.1 现状

以下两套能力并存:

- `src/services/oidc_service.rs`
- `src/services/builtin_oidc_provider.rs`

### 4.6.2 证据

- `OidcService` 面向外部 IdP，包含 discovery、JWKS、PKCE、authorization URL 等客户端能力。
- `BuiltinOidcProvider` 实现内置 OIDC Provider，包含 RSA key、token 签发、auth session、refresh token 等完整服务端能力。

### 4.6.3 问题

- 两者不是重复实现，但都占据 OIDC 领域
- 缺少一份明确的“何时启用哪种模式”的架构说明
- 测试矩阵、文档、管理入口可能继续分化

## 4.7 低到中风险: CAS 命名歧义

### 4.7.1 现状

同时存在:

- `src/services/cas_service.rs` 中的 `CasService`
- `src/storage/cas.rs` 中的数据模型 `CasService`

### 4.7.2 问题

- 编码时易混淆
- IDE 搜索和代码评审的认知成本偏高

## 4.8 第三方依赖整合问题

### 4.8.1 当前特征

本地依赖结构本身不算失控，但已经呈现出“基础设施重复抽象”的风险，例如:

- Redis 相关能力分散到缓存、队列、worker
- RTC/媒体域的第三方能力入口开始增多
- SSO 域同时包含 OIDC / Builtin OIDC / SAML / CAS

### 4.8.2 风险

- 新依赖引入时容易绕过既有抽象
- 运维和配置复杂度被模块扩张带动上涨

## 5. 优化目标

## 5.1 总体目标

在不破坏现有主功能的前提下，将 `synapse-rust` 从“模块丰富但边界分散”优化为“领域清晰、入口统一、指标可观测、可渐进扩展”的工程结构。

## 5.2 量化目标

### P0-P1 阶段目标

- 将高风险重复实现从 7 组降到 3 组以内
- 形成统一缓存接口与统一媒体入口
- 明确生产级异步执行主链路
- 为 RTC、SSO、私聊关系域建立标准边界文档

### P2 阶段目标

- 建立关键路径性能基线与回归门禁
- 将关键服务指标接入统一监控
- 完成主要重复模块的“兼容层保留 + 旧路径冻结”

## 6. 优先级排序

## 6.1 P0: 必须优先处理

1. 缓存双轨收敛
2. 队列 / 后台执行模型收敛
3. DM / Friend 关系域收敛

## 6.2 P1: 高价值尽快处理

4. 媒体上传入口统一
5. RTC 域统一门面
6. OIDC 双模式制度化

## 6.3 P2: 中期治理

7. 命名歧义清理
8. feature gate 生命周期治理
9. 文档、CI、指标的系统化固化

## 7. 详细实施方案

## 7.1 P0-1 缓存收敛方案

### 7.1.1 目标

保留一套正式缓存基础设施，把其他缓存实现降为适配层或专项策略层。

### 7.1.2 建议方案

- 以 `CacheManager` 作为唯一通用缓存底座
- `CacheService` 停止承载新业务
- `RoomSummaryCache` 改造成基于统一缓存接口的领域缓存适配器

### 7.1.3 实施步骤

1. 定义统一缓存 trait，例如 `CacheBackend` / `TypedCache`.
2. 为 `CacheManager` 提供 typed API、namespace API、metrics API。
3. 把 `services/cache/service.rs` 的调用点逐步迁到统一接口。
4. 将 `CacheService` 标记为 deprecated internal path，不再新增依赖方。
5. 把 `RoomSummaryCache` 只保留领域 key 组装和 TTL 策略，不再自建底层存储。

### 7.1.4 验收指标

- 缓存命中率可按 namespace 统计
- Redis hit/miss、本地 hit/miss、fallback 次数统一出指标
- 新增功能不再直接依赖 `CacheService`

## 7.2 P0-2 队列与后台执行模型收敛方案

### 7.2.1 目标

形成一条清晰的主链路:

- 单进程轻量任务: 本地适配层
- 生产异步任务: Redis 队列
- 分布式执行与复制: worker 子系统

### 7.2.2 建议方案

- 将 `RedisTaskQueue + worker/*` 定义为生产主路径
- `TaskQueue` 仅保留为测试/本地 fallback
- `MessageQueue` 如无明确外部契约，停止作为独立通用消息系统扩张

### 7.2.3 实施步骤

1. 编写异步执行架构文档，定义四类职责:
   - fire-and-forget 本地任务
   - durable background job
   - stream replication
   - business message queue
2. 给 `MessageQueue` 做使用点普查。
3. 若仅少量内部使用，则合并到 `BackgroundJob` / `RedisTaskQueue` 语义中。
4. 将 `TaskQueue` 明确标注为 `test/local-only`.
5. 建立统一 job metadata:
   - job_id
   - job_type
   - trace_id
   - retry_count
   - created_ts
   - last_error
6. 在 `worker` 层接入任务指标和 lag 指标。

### 7.2.4 验收指标

- 生产任务 90% 以上走统一 durable queue
- 后台任务失败率、重试次数、队列长度、消费延迟可观测
- 不再出现三套以上并行新增的通用队列入口

## 7.3 P0-3 私聊 / 好友关系域收敛方案

### 7.3.1 目标

统一“用户关系 + 私聊房间映射 + 好友状态 + 私聊元信息”的事实来源。

### 7.3.2 建议方案

- 以 `FriendRoomService + FriendRoomStorage` 作为持久化主实现
- `DMService` 改为轻量 facade 或 compatibility cache

### 7.3.3 实施步骤

1. 梳理所有 `DMService` 调用点。
2. 在 `FriendRoomService` 中补齐必要的 `get_existing_dm`、`mark_room_as_dm` 兼容接口。
3. 将 `DMService` 改造成:
   - 读穿透到持久化层
   - 仅做短期缓存
   - 或直接删除并提供迁移 shim
4. 为私聊关系引入明确的数据模型文档:
   - relation_status
   - dm_room_id
   - dm_room_state
   - source_of_truth

### 7.3.4 验收指标

- 重启后 DM 状态不丢失
- 私聊房间查询统一由持久化域返回
- 不再同时存在两套“主语义实现”

## 7.4 P1-1 媒体域统一方案

### 7.4.1 目标

保留多种上传策略，但只保留一个媒体领域入口。

### 7.4.2 建议方案

建立 `MediaDomainService`:

- 普通上传策略
- 分块上传策略
- 外部 provider 策略
- 缩略图、配额、审计、元数据清理统一挂接

### 7.4.3 实施步骤

1. 提炼统一接口:
   - `start_upload`
   - `upload_part`
   - `complete_upload`
   - `abort_upload`
   - `store_metadata`
2. `MediaService` 负责媒体存储与缩略图
3. `ChunkedUploadService` 降级为一种上传策略实现
4. `upload provider` 路由接入统一策略选择器
5. 建立统一 media metadata 表和清理任务

### 7.4.4 验收指标

- 普通上传与分块上传共享统一配额校验
- 所有上传流程都能产出统一审计日志
- 媒体孤儿记录和孤儿文件可定期清理

## 7.5 P1-2 RTC 域统一方案

### 7.5.1 目标

将 `VoipService`、`CallService`、`MatrixRTCService`、`VoiceService` 组织为一个清晰的域模型。

### 7.5.2 建议拆分

- `RtcInfraService`: TURN/STUN、凭证、基础配置
- `RtcSessionService`: MatrixRTC session/membership/encryption
- `CallOrchestrationService`: invite/answer/candidates/hangup
- `VoiceMessageService`: 语音消息上传

### 7.5.3 实施步骤

1. 新建 `services/rtc/` 模块。
2. 保留旧类型名一段时间，改为 re-export。
3. 统一 metrics:
   - active_sessions
   - active_memberships
   - call_setup_success_rate
   - turn_credential_issued_total
4. 统一 route/service/storage 文档。

### 7.5.4 验收指标

- 新增 RTC 功能都进入 `rtc/` 域
- 文档中可清楚区分 TURN、Call、RTC session、Voice message

## 7.6 P1-3 OIDC 双模式治理方案

### 7.6.1 目标

不是删除其中一个模式，而是把模式边界制度化。

### 7.6.2 建议方案

- `OidcService`: 外部 IdP 客户端模式
- `BuiltinOidcProvider`: 开发 / 测试 / 内部部署模式

### 7.6.3 实施步骤

1. 补充配置文档，明确二者适用场景。
2. 在启动时给出模式冲突检查与警告。
3. 为两条链路补齐独立指标和 smoke test。
4. 明确是否允许生产同时启用两套入口。

### 7.6.4 验收指标

- 配置说明明确
- 测试覆盖两种模式的最小闭环
- 运维不会误把 builtin provider 当成正式公网 IdP

## 7.7 P2-1 命名与模块卫生治理

### 7.7.1 重点

- 将 `storage/cas.rs` 的 `CasService` 模型更名为 `CasRegisteredService` 或等价名称
- 对 domain/service/storage/model 命名规则做统一

### 7.7.2 验收指标

- 搜索结果不再大面积混淆
- 新模块命名遵循统一规范

## 8. 技术选型建议

## 8.1 缓存

- 主缓存: 继续使用 Redis + 本地缓存二级模型
- 推荐基座: `CacheManager`
- 不建议继续演化第二套通用内存缓存框架

## 8.2 异步任务

- 生产 durable queue: Redis
- 单机短任务: Tokio 本地队列，仅限 fallback / test
- 分布式复制与协调: 继续使用现有 worker 子系统

## 8.3 数据存储

- 继续坚持 PostgreSQL first
- 减少关键业务状态仅存内存的实现

## 8.4 指标

- 指标出口: Prometheus
- 推荐统一标签:
  - service
  - operation
  - result
  - backend
  - route

## 8.5 配置治理

- 对 feature gate 建立三级状态:
  - stable
  - experimental
  - internal
- 为每个 feature 标注 owner、默认值、退出条件

## 9. 性能验收指标

以下指标建议作为本地优化方案的首轮验收门槛。

## 9.1 API 与同步

- `/sync` p95 延迟下降 20% 以上
- Sliding Sync 热路径 p95 延迟下降 15% 以上
- 初始同步与增量同步能分别出指标

## 9.2 缓存

- 核心 namespace cache hit ratio >= 85%
- Redis fallback rate < 5%
- 缓存失效广播延迟 p95 < 500ms

## 9.3 媒体

- 普通上传成功率 >= 99.9%
- 分块上传成功率 >= 99.5%
- 媒体孤儿文件/记录清理任务成功率 >= 99%

## 9.4 队列与 worker

- durable queue 消费延迟 p95 < 2s
- job retry after first failure < 3 次
- worker replication lag p95 < 1s

## 9.5 RTC

- 通话建立成功率 >= 98%
- RTC session 创建失败率 < 1%
- TURN 凭证签发失败率 < 0.1%

## 10. 风险预判与应对措施

## 10.1 风险: 收敛过程中引发行为回归

### 应对

- 先建 facade，再切调用点
- 保留兼容层一个迭代周期
- 为关键路径增加 focused integration tests

## 10.2 风险: 多模块同时调整导致节奏失控

### 应对

- 分域推进，不做全局大爆改
- 每一阶段只收敛一个主领域
- 每阶段结束必须补文档和指标

## 10.3 风险: feature gate 与配置组合过多

### 应对

- 对每个域建立“支持矩阵”
- 禁止未声明 owner 的新 feature 进入默认功能集

## 10.4 风险: 运维链路不清晰

### 应对

- 增加 upgrade notes
- 增加 deployment checklist
- 对 worker、Redis、媒体、SSO 给出单独运行手册

## 10.5 2026-05-28 代码质量修复进展

本轮修复覆盖了优化蓝图中未涉及的深层代码质量问题：

### 10.5.1 错误响应泄露内部详情 (P0)

- **问题**: ~1200 处 `ApiError::internal(format!("...: {e}"))` 将数据库错误详情直接返回给客户端
- **修复**: 全量替换为 `ApiError::internal_with_log("Context", &e)` / `ApiError::database_with_log("Context", &e)` 辅助方法
- **效果**: 服务端日志保留完整错误详情，客户端仅返回通用消息 "An internal error occurred"
- **覆盖**: 119 个文件，0 残留

### 10.5.2 N+1 查询消除 (P0)

- **问题**: `query_keys_internal` 和 `get_verified_devices_batch` 存在 N+1 查询
- **修复**: 添加 `get_cross_signing_keys_batch` + `get_all_device_keys_batch` 批量方法
- **效果**: 2 次 SQL 替代 N 次 SQL

### 10.5.3 路由层直接 SQL 迁移 (P1)

- **问题**: `e2ee_routes.rs`、`dm.rs`、`account_compat.rs` 中存在 10+ 处直接 SQL 查询
- **修复**: 全部迁移到 Storage 层，新增 `get_device_list_changes`/`get_devices_by_user_device_pairs`/`get_account_data_content`/`batch_can_view_profile` 等方法
- **效果**: 路由层零直接 SQL

### 10.5.4 密钥轮转参数持久化 (P0)

- **问题**: `configure_key_rotation` 未持久化 `interval_ms`，3 个轮转常量硬编码
- **修复**: 全部 6 个轮转参数持久化到 `key_rotation_config` 表，添加迁移脚本
- **效果**: 轮转参数可运行时配置，重启后不丢失

### 10.5.5 管理员权限检查 (P0)

- **问题**: 6 个 key_rotation 路由缺少 `is_admin` 权限检查
- **修复**: 所有路由添加管理员权限验证

## 11. 推荐执行路线图

## 第一阶段: 2-3 周 → ✅ 已完成

- ✅ 完成缓存收敛设计 → CacheService/RoomSummaryCache/MessageQueue 已移除
- ✅ 完成队列/worker 主链路定义 → TaskQueue 限缩为 test-only，RedisTaskQueue 为生产主路径
- ✅ 完成 DM/Friend 关系域设计评审 → DMService 收缩为兼容层
- ✅ 建立首版指标清单 → 错误泄露/N+1/直接 SQL 等质量指标已建立基线

## 第二阶段: 3-4 周

- 落地缓存统一接口
- 下线或冻结 `CacheService` 新增使用
- 改造 `DMService` 为兼容层
- 梳理 `MessageQueue` 使用点

## 第三阶段: 3-4 周

- 建立统一媒体域入口
- 建立 `rtc/` 统一域
- 完成 OIDC 双模式文档和启动检查

## 第四阶段: 2 周

- 清理命名歧义
- 补充 CI 分层与性能回归门禁
- 完成升级说明与运维文档

## 12. 与上游 Synapse 的对照结论

如果把本地项目和上游 Synapse 的成熟实践对照，可以得到一个明确判断:

- 上游的核心优势在于“允许过渡，但强制收敛”
- 本地目前的主要短板在于“已经进入过渡期，但退出机制还不够明确”

因此本地优化的首要任务不是继续扩充功能，而是建立与上游类似的治理纪律:

- 一个领域只保留一个正式主入口
- 允许兼容层，但必须标注淘汰计划
- 每次优化都绑定指标和文档
- 有性能回归就回退，而不是硬扛

## 13. 最终建议

对 `synapse-rust` 的最优策略，不是立即做大规模重构，而是执行“收敛优先”的连续优化:

1. 先收敛缓存、任务、关系域三大高风险重复实现
2. 再统一媒体和 RTC 两个最容易继续扩张的领域
3. 最后用指标、CI 和升级文档把成果固化

只要这三步执行到位，`synapse-rust` 会从“功能复杂但边界分散”转向“能力丰富且可维护”，后续无论是继续做性能优化、安全加固还是 MSC 扩展，成本都会显著下降。
