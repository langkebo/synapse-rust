# synapse-rust vs Element Synapse 对比审计报告

> 审计日期：2026-05-06
> 更新日期：2026-05-07（最终审计同步 + 联邦出站差距分析 + 未解决问题汇总）
> 对标项目：[element-hq/synapse](https://github.com/element-hq/synapse) (Python 官方实现)
> 审计对象：/Users/ljf/Desktop/hu_ts/synapse-rust
> 审计范围：API 兼容性、联邦协议、安全架构、数据库设计、媒体/推送/其他模块、状态解析、后台任务、配置

---

## 一、总体评估

| 维度 | 完成度 | 评价 |
|------|--------|------|
| Client-Server API | ~90% | 核心端点已实现，UIA 框架已补全，缺少房间升级等少量端点 |
| Federation API | ~75% | 入站完整，EventBroadcaster 出站+重试已实现，EDU 广播已集成，缺少持久化出站队列和批处理 |
| 安全架构 | ~95% | 限流/CSRF/CORS/Shadow Ban/SSRF/UIA 框架均已实现 |
| 数据库设计 | ~92% | Schema 基本完整，stream_ordering 已添加，State Groups 表已创建，流式数据表已创建，room_stats_current 已创建 |
| 媒体 | ~90% | 上传下载缩略图可用，保留策略和缓存清理已实现，Content-Disposition 已支持，MIME 类型已使用 infer magic bytes，上传配额强制检查 |
| 推送 | ~95% | 规则管理完整，推送网关已实现（FCM/APNs/WebPush），推送队列带重试 |
| 状态解析 | ~60% | MSC1442 State Resolution v2 核心算法已实装（auth_difference/mainline/reverse_topological_power/resolve_state_v2），待接入联邦层 |
| 后台任务 | ~80% | 框架完整，自动调度器已实现（含媒体清理/保留策略/锁清理） |
| 配置 | ~75% | 核心配置已有，docker/deploy 已全面优化，新增 homeserver.yaml media 段等，Prometheus 监控已实现 |

---

## 二、P0 严重问题（联邦基本不可用）

### 问题 1：缺少出站联邦发送器 (Outbound Federation Sender)

- **Synapse 参考**：`synapse.app.federation_sender` Worker，`TransactionQueue` 循环处理出站队列
- **当前状态**：基础广播已实现（`EventBroadcaster`），但缺少持久化、批处理和独立发送循环
- **已实现部分**：
  - ✅ `EventBroadcaster::broadcast_event()` — 向远端服务器发送 PDU
  - ✅ `EventBroadcaster::broadcast_edu_to_room()` — 向房间参与者广播 EDU（typing/receipt）
  - ✅ `PendingTransaction` 内存队列 + `enqueue_for_retry()` — 失败入队
  - ✅ `retry_pending_transactions()` — 指数退避重试（1s→5s→15s→30s→60s→5min→15min，最多 7 次）
  - ✅ 后台调度器每 5 分钟调用重试（server.rs:508-516）
  - ✅ `federation_queue` 表已创建（但未在 EventBroadcaster 中使用）
- **仍缺失部分**：
  - ❌ **事务持久化**：`PendingTransaction` 仅存内存 `Vec`，重启后全部丢失。`federation_queue` 表存在但未接入
  - ❌ **按目标批处理**：当前每个 PDU 发送独立事务。Synapse 将同一目标的多个 PDU/EDU 合并为一个事务
  - ❌ **按目标服务器状态追踪**：Synapse 追踪 `destination_retry_timings` 含持久化失败计数，当前无此机制
  - ❌ **新事件唤醒**：当前收到新事件直接同步发送（阻塞），缺少异步发送循环
  - ❌ **独立 Worker 进程**：Synapse 有专用 `federation_sender` Worker，当前集成在主进程中
- **影响**：
  1. 服务器重启导致所有未发送的事务永久丢失（无持久化）
  2. 发送新事件时阻塞请求线程，影响延迟
  3. 无法针对不同目标服务器独立管理重试策略
- **位置**：[src/federation/event_broadcaster.rs](src/federation/event_broadcaster.rs) — 已有基础架构
- **修复建议**（按优先级）：
  1. 🔴 将 `PendingTransaction` 持久化到 `federation_queue` 表，启动时恢复
  2. 🟡 实现按目标的事务批处理（收集 N 个 PDU 或等待 T 秒后发送）
  3. 🟡 添加异步发送循环（后台 task 而非同步阻塞）
  4. 🟢 添加 `destination_retry_timings` 表追踪每服务器状态

### 问题 2：缺少 State Resolution v2 算法 ✅ 已修复 (2026-05-07)

- **当前状态**：已实现 MSC1442 State Resolution v2 核心算法，位于 `src/federation/event_auth.rs`
- **已实装方法**：
  - `calculate_auth_difference()` — 计算两组 auth chain 的对称差异（含 transitive closure）
  - `compute_mainline()` — 从 room create 事件构建 mainline 排序链
  - `sort_by_reverse_topological_power()` — 逆拓扑权力排序（power DESC → ts ASC → mainline ASC → event_id ASC）
  - `resolve_state_v2()` — 完整 MSC1442 v2 算法（区分 unconflicted/conflicted keys）
- **待完成**：将 v2 算法接入联邦层 `send_transaction` 处理流程
- **位置**：[src/federation/event_auth.rs](src/federation/event_auth.rs)

### 问题 3：房间版本硬编码为 v1 ✅ 已修复

- **Synapse 参考**：支持 v1-v10 房间版本，默认 v10
- **当前状态**：已从数据库读取 room_version，默认 v10（`federation.rs` 已修改）
- **修复内容**：`make_join` 现在从数据库获取房间版本，不存在时默认为 "10"
- **修复日期**：2026-05-06

### 问题 4：缺少收据 (Receipt) 端点 ✅ 已实现

- **Synapse 参考**：`PUT /_matrix/client/v3/rooms/{room_id}/receipt/{receipt_type}/{event_id}`
- **当前状态**：已完整实现收据端点
- **已实现功能**：
  - `POST /rooms/{room_id}/receipt/{receipt_type}/{event_id}` — 发送已读回执
  - `GET /rooms/{room_id}/receipts/{receipt_type}/{event_id}` — 获取已读回执
  - `POST|PUT /rooms/{room_id}/read_markers` — 设置已读标记
  - 支持 `m.read`、`m.read.private` 回执类型
  - 支持 `m.fully_read`、`m.private_read`、`m.marked_unread` 标记类型
  - Sliding Sync 已集成 receipts 扩展

---

## 三、P1 高优先级问题（影响用户体验）

### 问题 5：缺少 UIA (User-Interactive Authentication) 框架 ✅ 已修复

- **Synapse 参考**：`synapse/handlers/ui_auth.py` — 完整的 UIA 会话管理
- **当前状态**：UIA 框架已完整实现
- **修复内容**：
  - `UiaService` — 完整的 UIA 会话管理服务
  - 会话创建/验证/完成流程（基于 Redis 缓存）
  - `m.login.password` UIA 阶段验证
  - `m.login.token` UIA 阶段验证
  - 多阶段认证流程支持（flows/stages）
  - 会话超时配置（`ui_auth_session_timeout`，默认 15 分钟）
  - 规范的 401 响应（errcode/flows/params/session/completed）
  - 设备删除/密码修改等端点已集成 UIA 框架
- **修复日期**：2026-05-07

### 问题 6：缺少 Typing/Receipt 联邦 EDU 出站广播 ✅ 已修复

- **Synapse 参考**：`m.typing` / `m.receipt` EDU 通过联邦事务广播
- **当前状态**：Typing 和 Receipt 联邦 EDU 出站广播已实现
- **修复内容**：
  - `EventBroadcaster` 添加 `broadcast_edu_to_room()` 方法
  - `get_eligible_destinations()` 基于房间成员列表推导目标服务器
  - typing 路由集成 EDU 广播（set/clear typing 时发送 `m.typing` EDU）
  - receipt 路由集成 EDU 广播（发送 receipt 时发送 `m.receipt` EDU）
  - Typing 状态写入 `room_ephemeral` 表，/sync 响应包含 typing 通知
- **修复日期**：2026-05-07

### 问题 7：缺少设备列表出站推送 ✅ 已修复

- **Synapse 参考**：`m.device_list_update` EDU 发送到共享房间的联合服务器
- **当前状态**：设备列表出站推送已实现
- **修复内容**：
  - `update_device` / `delete_device` / `delete_devices` 端点集成 `m.device_list_update` EDU
  - 自动查找用户共享房间的所有联合服务器
  - 去重发送（同一服务器只发送一次）
- **修复日期**：2026-05-07

### 问题 8：推送网关仅有配置，缺少实际实现 ✅ 已实现

- **Synapse 参考**：`HttpPusher` 向已注册的 push gateway 发送 HTTP 推送请求
- **当前状态**：推送网关已完整实现
- **已实现功能**：
  - `PushGateway` — HTTP 推送网关客户端
  - FCM (Firebase Cloud Messaging) 提供者
  - APNs (Apple Push Notification Service) 提供者
  - WebPush 提供者
  - 推送规则评估（event_match/contains_display_name/room_member_count/sender_notification_permission）
  - PushQueue 带重试机制
  - 设备注册/注销端点

### 问题 9：缺少房间成员管理端点 ✅ 已实现

- **Synapse 参考**：`invite/kick/ban/unban/join/leave` 端点
- **当前状态**：所有房间成员管理端点已完整实现
- **已实现功能**：
  - `POST /rooms/{room_id}/join` — 加入房间
  - `POST /rooms/{room_id}/leave` — 离开房间
  - `POST /rooms/{room_id}/invite` — 邀请用户
  - `POST /rooms/{room_id}/kick` — 踢出用户
  - `POST /rooms/{room_id}/ban` — 封禁用户
  - `POST /rooms/{room_id}/unban` — 解封用户
  - `POST /join/{room_id_or_alias}` — 通过别名加入
  - `POST /knock/{room_id_or_alias}` — 敲门请求

### 问题 10：缺少 to-device 消息端点 ✅ 已实现

- **Synapse 参考**：`PUT /_matrix/client/v3/sendToDevice/{event_type}/{txn_id}`
- **当前状态**：to-device 消息端点已完整实现
- **已实现功能**：
  - `PUT /sendToDevice/{event_type}/{transaction_id}` — 发送 to-device 消息
  - 消息去重（`to_device_transactions` 表）
  - 流式 ID（`to_device_stream_id_seq` 序列）
  - Sync 集成（sync_service 和 sliding_sync_service）
  - 消息清理（消费后自动删除）

---

## 四、P2 中优先级问题（影响运维和可靠性）

### 问题 11：缺少联邦事务去重 ✅ 已修复

- **Synapse 参考**：记录已处理的事务 ID 防止重复处理
- **当前状态**：联邦事务去重已实现
- **修复内容**：
  - `send_transaction` 入口添加 `(origin, txn_id)` 去重检查
  - 基于 Redis 缓存标记已处理事务（24 小时 TTL）
  - 重复事务直接返回空结果，不重复处理 PDU
  - 添加 `federation_inbound_txn_dedup_total` 监控指标
- **修复日期**：2026-05-07

### 问题 12：缺少联邦重试/退避调度器 ✅ 已修复

- **Synapse 参考**：完整的目的地重试调度器（指数退避、持久化重试状态）
- **当前状态**：联邦重试/退避调度器已实现
- **修复内容**：
  - `EventBroadcaster` 添加 `PendingTransaction` 队列
  - 退避调度：1s → 5s → 15s → 30s → 60s → 5min → 15min
  - 最多重试 7 次后丢弃
  - `retry_pending_transactions()` 方法由后台调度器每 5 分钟调用
  - 发送失败的事务自动入队重试
- **修复日期**：2026-05-07

### 问题 13：缺少后台任务自动调度器 ✅ 已修复

- **Synapse 参考**：`BackgroundUpdateController` 循环执行待处理的更新任务
- **当前状态**：后台任务调度器已在 server.rs 中实现
- **修复内容**：每 60 秒执行一次后台任务循环，包括：
  - 后台更新重试（retry_failed）
  - 过期锁清理（cleanup_expired_locks）
  - 保留策略清理（run_scheduled_cleanups）
  - 媒体缓存清理（每小时执行一次）
- **修复日期**：2026-05-06

### 问题 14：缺少媒体保留/清理机制 ✅ 已修复

- **Synapse 参考**：`media_retention.local_media_lifetime` / `remote_media_lifetime` + 定期清理
- **当前状态**：媒体保留和清理机制已实现
- **修复内容**：
  - 配置项 `remote_media_lifetime`（默认 30 天）和 `local_media_lifetime`（0=永不过期）
  - 后台调度器每小时清理过期媒体
  - `purge_media_cache()` 方法按时间戳清理
- **修复日期**：2026-05-06

### 问题 15：缺少流式 Token 机制 ✅ 已修复

- **Synapse 参考**：`stream_ordering` 单调递增流 ID，sync 基于 stream token
- **当前状态**：stream_ordering 流式令牌机制已实现
- **修复内容**：
  - `events` 表添加 `stream_ordering BIGSERIAL` 列
  - 数据库迁移自动回填现有事件的 stream_ordering
  - `RoomEvent`/`StateEvent` 结构体添加 `stream_ordering` 字段
  - 新增 `get_max_stream_ordering()`/`get_events_since_stream_ordering()`/`get_room_events_by_stream_range()` 方法
  - `next_event_stream_id()` 优先使用 stream_ordering 生成 sync token
  - 新增索引 `idx_events_stream_ordering`/`idx_events_room_stream_ordering`
- **修复日期**：2026-05-07

### 问题 16：缺少 State Groups 架构 ✅ 表已创建 (2026-05-07)

- **Synapse 参考**：`state_groups` + `state_group_edges` 实现高效状态解析和缓存
- **当前状态**：State Groups 四张表已添加到 schema，待接入存储层和服务层
- **已创建表**：
  - `state_groups` — 房间状态快照
  - `state_group_edges` — DAG 边
  - `event_to_state_groups` — 事件映射
  - `state_group_state` — 状态条目存储
- **位置**：[migrations/00000000_unified_schema_v6.sql](migrations/00000000_unified_schema_v6.sql)

### 问题 17：缺少 Presence/Typing 同步集成 ✅ 已修复

- **Synapse 参考**：`/sync` 响应包含 `presence` 和 `ephemeral` 数据
- **当前状态**：Presence 和 Typing 已集成到 /sync 响应
- **修复内容**：
  - Presence 事件已在 sync 响应中返回
  - Typing 状态写入 `room_ephemeral` 表
  - /sync 通过 `get_room_ephemeral_events` 获取 ephemeral 事件
  - Receipt 事件也通过 ephemeral 通道同步
- **修复日期**：2026-05-07

---

## 五、P3 低优先级问题（影响完整性）

### 问题 18：缺少 40+ Synapse 配置选项 ✅ 部分修复

对比 Synapse 的 `config_documentation`，以下关键配置已补全：

| 配置类别 | 已补全配置 |
|----------|----------|
| 房间 | `autocreate_auto_join_rooms`, `auto_join_rooms` |
| 在线状态 | `presence_enabled` |
| 应用服务 | `app_service_config_files` |
| 安全 | `allow_public_rooms_without_auth`, `allow_public_rooms_over_federation` |
| 加密 | `encryption_enabled_by_default_for_room_type` |

仍缺少的配置：
| 配置类别 | 缺失配置 |
|----------|----------|
| 注册 | `registration_requires_token`, `registrations_require_3pid` |
| 密码 | `password_config.pepper` |
| 联邦 | `federation_domain_whitelist`, `federation_client_timeout` |
| 媒体 | `thumbnail_sizes`, `max_image_pixels` |
| 安全 | `max_avatar_size`, `allowed_avatar_mimetypes`, `redaction_retention_period` |
| 后台 | `background_updates.min_batch_size` |

### 问题 19：缺少 3PID 验证流程

- 邮箱/手机验证流程不完整（发送验证邮件、验证 token、绑定 3PID）

### 问题 20：缺少房间升级端点

- `POST /_matrix/client/v3/rooms/{room_id}/upgrade` 缺失

### 问题 21：缺少应用服务事务推送 ✅ 已验证已实现

- HS -> AS 的 `PUT /transactions/{txn_id}` 回调已在 `src/services/application_service.rs` 中完整实现
- 位置：[src/services/application_service.rs](src/services/application_service.rs) — `send_transaction()` 方法

### 问题 22：缺少 SSRF 黑名单实际执行 ✅ 已修复

- **Synapse 参考**：`ip_range_blacklist` 配置 + DNS 解析后二次校验
- **当前状态**：SSRF 黑名单已在 URL preview 端点强制执行
- **修复内容**：
  - `is_ip_in_blacklist()` — CIDR 和精确 IP 匹配
  - `check_url_against_blacklist()` — URL 解析 + DNS 解析后 IP 校验
  - URL preview 端点集成黑名单检查
- **修复日期**：2026-05-06

### 问题 23：缺少 Content-Disposition 支持 ✅ 已实现

- **Synapse 参考**：媒体下载端点包含 Content-Disposition 头
- **当前状态**：Content-Disposition 已完整实现并安全加固
- **已实现功能**：
  - `build_media_headers()` — 完整的媒体响应头构建
  - 安全类型白名单 `SAFE_INLINE_MEDIA_TYPES`
  - 白名单类型使用 `inline`，非白名单强制 `attachment`
  - `sanitize_attachment_filename()` — 文件名清洗
  - 下载支持带文件名参数
  - 额外安全头（X-Content-Type-Options/CSP/CORP/Referrer-Policy）

### 问题 24：缩略图 MIME 类型猜测不准确 ✅ 已修复 (2026-05-07)

- 已使用 `infer` crate 的 magic bytes 检测替代文件扩展名猜测
- 联邦缩略图（federation.rs）和本地媒体（media.rs）均已修复
- 检测失败时回退到扩展名匹配
- 位置：[src/web/routes/federation.rs](src/web/routes/federation.rs) — `federation_guess_content_type()`
          [src/web/routes/media.rs](src/web/routes/media.rs) — `guess_content_type()`

### 问题 25：缺少媒体配额强制执行 ✅ 已修复 (2026-05-07)

- 上传时自动检查配额：`current + new > max` → 拒绝上传
- 位置：[src/web/routes/media.rs](src/web/routes/media.rs) — `upload_media_common()`

---

## 六、数据库 Schema 差距

### 缺失的关键表

| 表名 | 用途 | Synapse 对应 | 状态 |
|------|------|-------------|------|
| `state_groups` | 状态组缓存 | `state_groups` | ✅ 已创建 |
| `state_group_edges` | 状态组边 | `state_group_edges` | ✅ 已创建 |
| `federation_inbound_events` | 联邦事件去重 | `federation_inbound_events` | ✅ 已创建 |
| `event_to_state_groups` | 事件→状态组映射 | `event_to_state_groups` | ✅ 已创建 |
| `state_group_state` | 状态组条目 | `state_group_state` | ✅ 已创建 |
| `event_search` | 全文搜索索引 | `event_search` | ✅ 由 search_index 表替代 |
| `event_edges` | 事件 DAG 边 | `event_edges` | ✅ 已创建 |
| `event_forward_extremities` | 前向极值 | `event_forward_extremities` | ✅ 已创建 |
| `room_stats_current` | 房间统计 | `room_stats_current` | ✅ 已创建 |
| `device_lists_outbound_pokes` | 设备列表出站推送 | `device_lists_outbound_pokes` | ✅ 已创建 |
| `receipts_linearized` | 收据存储 | `receipts_linearized` | ✅ 已创建 |
| `presence_stream` | 在线状态流 | `presence_stream` | ✅ 已创建 |
| `typing_stream` | 输入状态流 | `typing_stream` | ✅ 已创建 |

### 缺失的关键索引 ✅ 大部分已添加

| 索引 | 用途 | 状态 |
|------|------|------|
| `events(room_id, topological_ordering)` | 房间消息排序 | 业务逻辑用 depth 替代 |
| `events(room_id, stream_ordering)` | 房间流式查询 | ✅ `idx_events_room_stream_ordering` |
| `events(stream_ordering)` | 全局流式查询 | ✅ `idx_events_stream_ordering` |
| `access_tokens(user_id, is_revoked)` | Token 验证 | ✅ `idx_access_tokens_user_revoked` |
| `event_to_state_groups(event_id)` | 状态组映射 | ✅ 主键 PK |

---

## 七、缺失的 Client-Server API 端点

| 端点 | 用途 | 优先级 | 状态 |
|------|------|--------|------|
| `PUT /rooms/{room_id}/receipt/{receipt_type}/{event_id}` | 发送已读回执 | P0 | ✅ 已实现 |
| `PUT /rooms/{room_id}/read_markers` | 设置读标记 | P1 | ✅ 已实现 |
| `POST /rooms/{room_id}/invite` | 邀请用户 | P1 | ✅ 已实现 |
| `POST /rooms/{room_id}/join` | 加入房间 | P1 | ✅ 已实现 |
| `POST /rooms/{room_id}/leave` | 离开房间 | P1 | ✅ 已实现 |
| `POST /rooms/{room_id}/kick` | 踢出用户 | P1 | ✅ 已实现 |
| `POST /rooms/{room_id}/ban` | 封禁用户 | P1 | ✅ 已实现 |
| `POST /rooms/{room_id}/unban` | 解封用户 | P1 | ✅ 已实现 |
| `PUT /rooms/{room_id}/redact/{event_id}/{txn_id}` | 删除消息 | P1 | ✅ 已实现 |
| `PUT /sendToDevice/{event_type}/{txn_id}` | 发送 to-device 消息 | P1 | ✅ 已实现 |
| `GET /rooms/{room_id}/members` | 获取成员列表 | P1 | ✅ 已实现 |
| `GET /rooms/{room_id}/event/{event_id}` | 获取单个事件 | P1 | ✅ 已实现 |
| `GET /joined_rooms` | 获取已加入房间 | P2 | ✅ 已实现 |
| `POST /rooms/{room_id}/forget` | 忘记房间 | P2 | ✅ 已实现 |
| `POST /rooms/{room_id}/upgrade` | 升级房间版本 | P2 | ✅ 已实现 |
| `GET /rooms/{room_id}/timestamp_to_event` | 按时间戳查找事件 | P2 | ✅ 已实现 |
| `GET /rooms/{room_id}/hierarchy` | 客户端空间层级 | P2 | ✅ 已实现 |
| `GET /notifications` | 获取通知列表 | P3 | ✅ 已实现 |

---

## 八、修复优先级与工作量估算

### 第一阶段：联邦可用性（部分完成）

| 问题 | 工作量 | 状态 |
|------|--------|------|
| #1 出站联邦发送器 | 部分完成（60%） | 广播+重试已实现，持久化+批处理+异步循环缺失 |
| #2 State Resolution v2 | ✅ 已实装 | MSC1442 算法已实装，待接入联邦层 |
| #3 房间版本支持 | ✅ 已修复 | 动态读取 DB，默认 v10 |

### 第二阶段：核心功能补全（已全部完成）

| 问题 | 状态 |
|------|------|
| #5 UIA 框架 | ✅ 已验证已实现 |
| #6 Typing/Receipt 联邦 | ✅ 已修复 |
| #7 设备列表出站推送 | ✅ 已修复 |
| #8 推送网关实现 | ✅ 已实现 | FCM/APNs/WebPush |
| #9 房间成员管理端点 | ✅ 已实现 | invite/kick/ban/unban/join/leave |
| #10 to-device 消息 | ✅ 已实现 | 路由 + 存储 + 联邦 |

### 第三阶段：运维可靠性（已全部完成）

| 问题 | 状态 |
|------|------|
| #11 联邦事务去重 | ✅ 已修复 |
| #12 联邦重试调度器 | ✅ 已修复 |
| #13 后台任务调度器 | ✅ 已修复 |
| #14 媒体保留/清理 | ✅ 已修复 |
| #15 流式 Token | ✅ 已修复 |
| #16 State Groups | ✅ 表已创建 |

### 第四阶段：完善与合规（大部分完成）

| 问题 | 状态 |
|------|------|
| #17 Presence/Typing 同步 | ✅ 已修复 |
| #18 配置选项补全 | ✅ 部分修复（homeserver.yaml + media 段） |
| #19 3PID 验证 | 待实现 |
| #20 房间升级 | ✅ 已实现 | upgrade_room() in handlers/room.rs |
| #21 AS 事务推送 | ✅ 已验证已实现 |
| #22 SSRF 黑名单 | ✅ 已修复 |
| #23 Content-Disposition | ✅ 已实现 |
| #24 缩略图 MIME | ✅ 已修复 |
| #25 媒体配额强制 | ✅ 已修复 |

---

## 九、Synapse 最佳实践对照清单

| 最佳实践 | Synapse 实现 | synapse-rust 状态 | 差距 |
|----------|-------------|-------------------|------|
| PostgreSQL 首选 | asyncpg + 连接池 | sqlx + 连接池 | ✅ 已实现 |
| Worker 架构 | 多进程 + Redis IPC | 单进程，含 background_updates worker 表支持 | ⚠️ 单进程 |
| 令牌桶限流 | 多维度（IP/用户/房间/端点） | 三维度限流（用户+IP+端点），含令牌桶算法 | ✅ 已实现 |
| IP 黑名单 | 出站请求 CIDR 检查 | 已在 URL preview 执行 | ✅ 已实现 |
| SSRF 防护 | DNS 解析后二次校验 | 已在 URL preview 执行 | ✅ 已实现 |
| 事件签名 | Ed25519 + canonical JSON | 完整实现 | ✅ 已实现 |
| 密钥轮换 | 定期轮换 + grace period | 完整实现 | ✅ 已实现 |
| 流式 Token | stream_ordering 单调递增 | stream_ordering 已添加 | ✅ 已修复 |
| 媒体保留 | 可配置生命周期 + 自动清理 | 已实现 | ✅ 已实现 |
| 推送网关 | HTTP/WebPush/FCM/APNs | 完整实现 | ✅ 已实现 |
| 后台任务 | 自动调度 + 进度追踪 | 调度器已实现 | ✅ 已实现 |
| 房间版本 | v1-v10 | 数据库读取+默认v10 | ✅ 已修复 |
| State Groups | 高效状态缓存 | 表已创建 | ✅ 表已创建 |
| State Resolution v2 | MSC1442 完整实现 | MSC1442 算法已实装 | ✅ 已实装 |
| UIA 框架 | 多阶段会话管理 | UiaService 已实现 | ✅ 已修复 |
| 联邦出站 | 事务队列 + 重试 + 持久化 | 内存队列+指数退避重试，缺少持久化和批处理 | ⚠️ 缺少持久化 |
| 联邦 EDU 广播 | typing/receipt EDU 出站 | 已实现 | ✅ 已修复 |
| 设备列表推送 | device_list_update EDU | 已实现 | ✅ 已修复 |
| 联邦重试 | 指数退避 + 持久化 | 内存队列+退避 | ✅ 已修复 |
| Presence 同步 | /sync 返回 presence | 已集成 | ✅ 已修复 |
| 配置选项 | 覆盖 Synapse 主要配置 | 部分补全 | ⚠️ 部分修复 |
| Prometheus 监控 | metrics 端点 | 已实现（server.rs render_prometheus_metrics） | ✅ 已实现 |
| 审计日志 | 管理员操作审计 | 已实现 | ✅ 已实现 |
| Shadow Banning | 静默丢弃写操作 | 已实现 | ✅ 已实现 |
| 密码安全 | Argon2 + 策略验证 | 已实现 | ✅ 已实现 |

---

## 十、剩余未解决问题汇总

> 更新日期：2026-05-07（最终审计同步）

### 🔴 高优先级（影响可靠性和联邦可用性）

| # | 问题 | 现状 | 影响 | 工作量 |
|---|------|------|------|--------|
| 1 | **出站联邦事务持久化** | `federation_queue` 表已创建，但 `EventBroadcaster` 仅用内存 Vec | 重启丢失所有未发送事务 | 3-5 天 |
| 2 | **按目标事务批处理** | 当前每个 PDU 单独发送一个事务 | 网络效率低，远端压力大 | 2-3 天 |
| 3 | **异步发送循环** | 当前同步阻塞在请求线程中发送 | 影响消息发送延迟 | 1-2 天 |
| 4 | **State Resolution v2 接入** | 算法已实装（event_auth.rs），未接入联邦事务处理 | 状态冲突时使用旧算法 | 2-3 天 |

### 🟡 中优先级（影响功能完整性）

| # | 问题 | 现状 | 影响 |
|---|------|------|------|
| 5 | **State Groups 存储层接入** | 4 张表已创建，无 Rust 存储层代码 | 状态计算效率低 |
| 6 | **3PID 验证流程** | 邮箱/手机验证完全缺失 | 无法验证第三方身份 |
| 7 | **P3#18 剩余配置项** | 6 类配置缺失（详见 P3 #18） | 运维灵活性受限 |

### 🟢 低优先级（优化项）

| # | 问题 | 现状 |
|---|------|------|
| 8 | Worker 多进程架构 | 单进程设计，如需横向扩展需改造 |
| 9 | `destination_retry_timings` 表 | Synapse 用于追踪每目标服务器健康状态 |
| 10 | 联邦出站事务压缩/签名优化 | 当前基础实现，Synapse 有更多优化 |

---

## 十一、联邦出站现状详细分析

### 当前架构（EventBroadcaster）

```
EventBroadcaster
├── broadcast_event()       ✅ 同步发送 PDU，失败入队
├── broadcast_edu_to_room() ✅ 推导目标 → 逐服务器发送 EDU
├── pending_queue           ⚠️ Vec<PendingTransaction> 仅内存，重启丢失
├── retry_pending()         ✅ 指数退避 1s→15min，最多 7 次 → 丢弃
└── 后台调度器              ✅ 每 5 分钟 tick 执行重试
```

### Synapse 架构（对标）

```
FederationSender Worker (独立进程)
├── TransactionQueue (每目标一个)
│   ├── 持久化到数据库 (events_to_send + federation_queue)
│   ├── 启动时恢复未完成事务
│   └── 按目标批处理（收集多 PDU 后一起发送）
├── PerDestinationQueue
│   ├── destination_retry_timings（持久化）
│   ├── 指数退避（含 jitter）
│   └── 成功后重置失败计数
├── PresenceRouter（presence EDU 出站）
└── DeviceListUpdater（device_list EDU 出站）
```

### 差距总结

| 维度 | EventBroadcaster (当前) | Synapse FederationSender |
|------|------------------------|-------------------------|
| 队列存储 | 内存 Vec | PostgreSQL 持久化 |
| 重启恢复 | ❌ 全部丢失 | ✅ 从 DB 恢复 |
| 批处理 | ❌ 1 PDU/事务 | ✅ N PDU + M EDU/事务 |
| 发送方式 | 同步阻塞 | 异步循环 |
| 目标追踪 | ❌ 无 | ✅ destination_retry_timings |
| 进程隔离 | ❌ 主进程内 | ✅ 独立 Worker |
