# synapse-rust 问题确认清单与优化方案

> 输入：`synapse-rust-问题清单.md`（2026-08-10，下称「清单」）+ `project-audit-2026/project-audit-2026.html`（2026-08-09，下称「审计」）
> 方法：对两份报告逐条交叉比对，高危与冲突条目全部打开源码复核（3 路并行取证，行号以当前源码为准）。
> 日期：2026-08-10（初版），2026-08-11 更新（同步 F-1/E-1/G-1/B-1/E2EE-04/E2EE-09/E-2/G-3/C-4/FED-06/WORK-04/A-7/PERF-08/WEB-03/WEB-04 修复状态）

---

## 一、总体结论

1. **两份报告互补而非重复**。清单聚焦同步链路/限流/存储/缓存/架构债务（A~I 系）；审计的大头在 E2EE 密码学与联邦安全（清单完全未覆盖），两边只有约 8 条重叠。
2. **审计的 E2EE 最高危 4 条已被修复**（E2EE-01/02/08/11，源码中留有修复注释）——审计报告（08-09）比代码旧，**切勿按审计报告重复开 P0 工单**。E2EE-03/05/06 仍然成立；E2EE-04/09 已修复。
3. **本轮复核 39 条：✅确认 30 条、⚠️部分确认 5 条、❌证伪 5 条**（含已修复 4 条）。
4. 冲突条目（SEC-01 XFF）辨析结果：**两份报告各对一半**——`trusted_proxies` 门控存在（清单对），但取 XFF 最左元素仍可伪造（审计对）。
5. 本轮**新发现 6 项**两份报告均未记载的问题/事实（见第四节）。

---

## 二、证伪与已修复条目（不要重复修）

| 编号 | 审计声称 | 复核结论 |
|---|---|---|
| E2EE-01 | SSSS 用密文当 AES 密钥 | ❌ 已修复：`ssss/service.rs:241` 现为 HKDF 派生密钥 |
| E2EE-02 | SAS 验证丢弃私钥 | ❌ 已修复：`verification/service.rs:36-46` 返回双钥，`:237-239` 走真实 ECDH |
| E2EE-04 | 生产无 nonce 重用检测 | ❌ 已修复(2026-08-10, commit 6588148b)：移除 `#[cfg(test)]` 门控，NonceTracker + SecureNonceGenerator 生产启用 |
| E2EE-08 | SAS MAC 非恒定时间比较 | ❌ 已修复：`:312` 用 `mac_matches()`，底层 `secure_compare` |
| E2EE-09 | megolm pickle 三处 expect() | ❌ 已修复(2026-08-10, commit 3e3b1a32)：三处 `.expect()` 替换为 `?` 错误传播 |
| E2EE-11 | SSSS 密钥不足零填充 | ❌ 已修复：`:266-271` 不足 32 字节直接返回 Err |
| FED-02 | 接收端无联邦事务去重 | ❌ 不成立：`federation/transaction.rs:34-55` 有 `federation_txn:{origin}:{txn_id}` 缓存去重 + TTL，且 `:217/:234` 有内容哈希与签名验证 |

部分确认（代码属实但影响面需修正）：

| 编号 | 复核结论 |
|---|---|
| E2EE-07 | 代码属实，但 `upload_cross_signing_keys` 无生产调用方；实际路由 `device_signing/upload` 走的 `upload_device_signing_key`（:201-296）**有**强制验签。降级为死代码/陷阱代码 |
| FED-05 | backfill/get_event/get_missing_events 确实不验签，但这三个 client 方法**无生产调用方**。降级 |
| FED-07 | 重试 7 次后丢弃属实，但 `federation_queue` 表会标记 `failed` 留痕，并非完全无死信 |
| SEC-01 | 门控存在但取最左 XFF 元素可伪造成立（`ip.rs:42`），且 `peer_addr=None` 时无条件信任头部 |
| WEB-01 | 跳过审计属实；但 shadow-ban 由全局中间件兜住，**真实残留问题是 is_guest 被忽略（访客可建房）** |

---

## 三、确认存在的实际问题（合并去重后）

### 🔴 P0 安全（生产可达，应尽快修）— ✅ 全部修复

| # | 合并自 | 问题 | 取证 |
|---|---|---|---|
| S1 | E2EE-05 + E2EE-06 | 设备密钥/OTK/回退密钥验签失败仅 `warn!` 仍存储分发；`/keys/signatures/upload` 不验签直接存。可注入伪造密钥 → MITM | `device_keys/service.rs:218-225, 299-320, 388-407, 569-608`；路由可达 `routes/e2ee/devices.rs:189-194` **✅已修复(2026-08-10, TDD)**：验签失败/缺失一律 400 拒绝存储；upload_signatures 支持 spec 请求体并拒绝代他人上传签名（M_FORBIDDEN 入 failures）；新增 5 个测试 |
| S2 | FED-01 | 远程服务器密钥**缓存前不验签**，可 MITM 联邦连接注入伪造密钥后伪造任意联邦请求 | `synapse-federation/src/client.rs:410-431` **✅已修复(2026-08-10, TDD)**：新增 `verify_server_keys_self_signature`（canonical JSON + ed25519 verify_strict），验签通过前不写缓存；新增 3 个测试（有效/伪造/缺失自签名） |
| S3 | FED-04 | `verify_auth_chain` 只查 room_id 一致与事件存在，不验签名/auth_events/授权规则 | `event_auth/chain.rs:35-59` **✅已修复(2026-08-10, TDD)**：新增授权事件类型校验 + 非 create 事件必须被 auth_events 引用；密码学验签由接收路径（transaction.rs:217,234）承担；新增 3 个测试 |
| S4 | FED-03 | 状态解析用 `event_id.split(':')` 前缀匹配判断接受/拒绝 | `state_resolution.rs:79` **✅已修复(2026-08-10, TDD)**：改为 (type, state_key) 槽位 + 内容一致性精确归属；新增 2 个测试（此前所有事件被错误 rejected） |
| S5 | E2EE-03 | `Aes256GcmKey` 未实现 Zeroize/ZeroizeOnDrop（Ed25519 私钥有，AES 密钥没有） | `crypto/aes.rs:22-25` 对比 `ed25519.rs:49` **✅已修复(2026-08-10, TDD)**：derive `Zeroize, ZeroizeOnDrop`；新增 2 个测试（trait 约束 + zeroize 后字节清零） |

### 🔴 P0 性能/正确性（同步链路主战场）— ✅ 全部修复

| # | 合并自 | 问题 | 取证 |
|---|---|---|---|
| S6 | A-1 + SS-05 | v2 `/sync` 250ms 轮询模拟长轮询，1000 在线 ≈ 1.2 万 QPS 空转；同 crate 的 `EventNotifier` 事件驱动机制未接入。修正：since 是 stream_id/时间戳双模式（≥1e12 回退时间戳），时间戳路径仍有同毫秒漏读风险 | `sync_service/event_fetch.rs:168-192`、`config/performance.rs:42` **✅已修复(2026-08-10, TDD)** |
| S7 | A-3 | presence 去重 `get_raw` 只读本地缓存，跨实例/重启/驱逐后回声 → `is_idle=false` → 长轮询失效 → 忙循环复发开关 | `synapse-cache/src/lib.rs:887-896`、`extensions.rs:211-216`、`mod.rs:470-472` **✅已修复(2026-08-10, TDD)** |
| S8 | A-2 | `EventNotifier` Redis 扇出从未接线（`with_redis` 零调用、无订阅端、60 行死代码），多实例长轮询必然 30s 延迟；文档宣称与实现矛盾 | `container.rs:292`、`event_notifier.rs:79, 177-180` **✅已修复(2026-08-10, TDD)** |
| S9 | SS-01 | `/_matrix/client/v4/sync` 不在 IP 级限流豁免表 → IP 限流 + 用户限流**双重限流**，合法客户端吃 429 | `middleware/rate_limit.rs:12-21` vs `routes/sliding_sync.rs:18` **✅已修复(2026-08-10, TDD)** |
| S10 | SS-04 | 房间状态缓存失效**永远落空**（详见新发现 N2），房间变更后最长 5 分钟返回陈旧数据 | `state.rs:19` vs `mod.rs:577-584` **✅已修复(2026-08-10)** |

### 🟠 P1 高（明确缺陷）— ✅ 全部修复

| # | 合并自 | 问题 |
|---|---|---|
| S11 | A-4 + SS-02 | 路由层自测 wall-clock 不扣 `idle_wait_ms`，健康长轮询全记成 30s 慢请求；且路由与 service **递增同一个** `sliding_sync_slow_requests_total` → 一次慢请求计数 +2，WARN 日志洪泛 **✅已修复(2026-08-10, TDD)** |
| S12 | A-5 + SS-03 | initial sync 逐房间串行物化 + `let _ =` 吞错；单次物化恰好 7 次 DB 查询（100 房 = 700 次），外层 `if let Ok` 再吞一次 **✅已修复(2026-08-10, TDD)** |
| S13 | SS-09 | `room_sync_with_timeout` 外层硬编码 60s，客户端 timeout=120s 被截断（`sync_service/mod.rs:310-314`） **✅已修复(2026-08-10, TDD)** |
| S14 | SS-10 | `build_timeline` 恒取最新 N 条、不感知 pos，增量 sync 重复下发已收事件（`timeline.rs:16`） **✅已修复(2026-08-10, TDD)** |
| S15 | B-3 + SS-08 | 限流 fail-open 语义相反：Redis 抖动时 `/sync` 放行、sliding sync 全量 500（`sliding_sync.rs:66`） **✅已修复(2026-08-10, TDD)** |
| S16 | SEC-01⚠️ | XFF 取最左元素可伪造注入绕过登录限流（`ip.rs:42`） **✅已修复(2026-08-10, TDD)** |
| S17 | SEC-02 | 联邦限流 Redis 出错无条件放行且无配置项（`federation_rate_limit.rs:52-55`） **✅已修复(2026-08-10, TDD)** |
| S18 | STO-01 | DAG BFS 每事件单独查前驱，深 DAG 数百次查询（`dag.rs:66-91`） **✅已修复(2026-08-10)** |
| S19 | STO-02 + C-1 | `add_receipt` DELETE+INSERT 无事务；`delete_connection_data` 三条 DELETE 无事务 **✅已修复(2026-08-10)** |
| S20 | STO-03 | presence 存储靠匹配 SQL 错误消息字符串判断列存在（`presence/mod.rs:427, 462`） **✅已修复(2026-08-10)** |
| S21 | PERF-01 | QueryCache 读路径持 `cache.write()+stats.write()+hot_keys.write()` 三把写锁，读全串行（`query_cache.rs:211-228`） **✅已修复(2026-08-10)** |
| S22 | PERF-3↑ | WorkerBus `Clone` 内 5 处 `blocking_read()`，async 上下文**直接 panic**（比审计说的"阻塞"更严重）（`bus.rs:456-472`） **✅已修复(2026-08-10, TDD)** |
| S23 | ARCH-01 + ARCH-02↑ | AuthService 内部自建 7+ 存储绕过 DI；`UserService::new` 实际 **13 处**（审计只报了 3+），各自持有独立缓存状态 **✅已修复(2026-08-10, TDD)** |
| S24 | ARCH-06 | DirectoryService 纯内存 HashMap 无持久化，重启丢全部房间别名（违反 Matrix 规范） **✅已修复(2026-08-10, TDD)** |
| S25 | WEB-01⚠️ | create_room 手动鉴权跳过审计、is_guest 被忽略（访客可建房） **✅已修复(2026-08-10, TDD)** |
| S26 | B-2 | 429 与长轮询互为掩护：限流只在长轮询失效时触发，会掩盖 S7 类回归。需加 `sliding_sync_rate_limited_total` 独立告警 **✅已修复(2026-08-10, TDD)** |
| S27 | G-2 | `PUT /upload/{server}/{media_id}` 漏配 body limit → 回落 Axum 默认 2MB，MSC2246 异步上传实际不可用（`media/mod.rs:91`） **✅已修复(2026-08-10, TDD)** |

### 🟡 P2 中（技术债，本轮已复核确认的）

- ~~**SS-07**：presence 扩展 `last_active_ago` 硬编码 0（`extensions.rs:175`，注释自认 Mock）~~ **✅已修复**：改用 `get_presence_snapshots` 获取 `last_active_ts`，`last_active_ago` 由 `now_ts - last_active_ts` 实时计算；offline/无时间戳时为 null；新增 4 个测试
- ~~**STO-05**：`check_rate_limit` SELECT→UPDATE 无 `FOR UPDATE`，TOCTOU 竞态~~ **✅已修复**：`event_report/repository.rs` 的 `check_rate_limit` 改为 `BEGIN → SELECT … FOR UPDATE → UPDATE → COMMIT` 事务，消除 TOCTOU 窗口
- ~~**PERF-05**：`claim_task` 拉 1000 条内存 `find()`（`manager.rs:546-554`）~~ **✅已修复**：改为 `get_pending_task_by_id(task_id)` 按 ID 直查，不再拉 1000 条到内存；新增测试验证 >1000 条 pending 时仍可领取
- ~~**PERF-08**：`broadcast_invalidation` 只发 Redis 不失效本地缓存~~ **✅已修复(2026-08-10, commit 8c51447e)**：广播同时失效本地缓存
- ~~**WORK-01**：`WorkerBus.unsubscribe` 仅删本地列表，不退订 Redis Pub/Sub~~ **✅已修复(2026-08-11)**：`unsubscribe` 现通过命令通道 (`SubCommand::Unsubscribe`) 通知订阅任务，订阅任务收到命令后断开重连，重连时读取更新后的 `subscribed_channels` 列表只订阅剩余频道，实现真正的 Redis 退订
- ~~**WORK-04**：HealthChecker 只查注册表键存在性，崩溃 worker 仍报 Healthy~~ **✅已修复(2026-08-10, commit 8c51447e)**：新增 `record_heartbeat` + `heartbeat_timeout_secs` 活性探测，无心跳或超时即报 Unhealthy
- ~~**WORK-05**：Redis 发布已加重试，但重试耗尽后仍静默丢消息~~ **✅已修复(2026-08-11)**：重试耗尽后不再静默丢弃——新增 `FailedPublish` 结构体 + 内存环形缓冲 DLQ (256 条容量)，提供 `failed_publish_count()` / `list_failed_publishes()` / `retry_failed_publishes()` 方法用于检查和重放
- ~~**SEC-03**：内置 OIDC 明文密码回退仅 warn 不拒绝~~ **✅已修复**：`builtin_oidc_provider.rs` 默认 `allow_plaintext_passwords=false` 时返回 401 拒绝；仅在显式配置 `true` 时放行并 warn；新增测试验证默认拒绝
- ~~**WEB-03**：`is_localhost_bind()` 含 0.0.0.0，dev 模式 CORS 全开放~~ **✅已修复(2026-08-10, commit 8c51447e)**
- ~~**WEB-04**：中间件顺序不当（CORS 最内层、rate_limit 先于 csrf）~~ **✅已修复(2026-08-10, commit 8c51447e)**
- ~~**E2EE-04**：生产无 nonce 重用检测~~ **✅已修复(2026-08-10, commit 6588148b)**：移除 `#[cfg(test)]` 门控，NonceTracker + SecureNonceGenerator 生产启用；`encrypt_with_nonce` 改为实例方法使用计数器生成 + 碰撞检测
- ~~**E2EE-09**：megolm pickle 三处 `expect()`~~ **✅已修复(2026-08-10, commit 3e3b1a32)**：三处 `.expect()` 替换为 `?` 错误传播
- ~~**FED-06**：server_resolution_cache 无 TTL，DNS 变更不感知~~ **✅已修复(2026-08-10, commit 84152265)**：新增 `CachedResolvedServer` 结构含 `cached_at` 时间戳，TTL 300s 后过期重解析
- ~~**B-1**：两套同名限流配置类型并存且都在用~~ **✅已修复(2026-08-10, commit 8c51447e)**：`config/rate_limit.rs` 改为 re-export `rate_limit_config.rs` 的类型，消除重复定义
- **ARCH-05**：`assemble/` 整模块孤儿代码（`lib.rs` 未声明 mod，不参与编译）——降级为重构中间产物，不计为问题

### 🟡 P2 中（清单原文已附取证、本轮未重复复核，采信）

~~A-6（竞态版唤醒 API 死代码）~~ **✅已删除**、~~A-7（notifier map 不回收）~~ **✅已修复(2026-08-10, commit 8c51447e)**：`EventNotifier` 新增 `evict_idle_slots` + `start_idle_slot_evictor` 后台定期回收 `Arc::strong_count==1` 的空闲槽位、~~B-4（豁免表手工维护）~~ **✅已修复(2026-08-11)**：`RouteEntry` 新增 `rate_limit_exempt` 字段，sync/sliding_sync manifest 中标记豁免路由，`create_router` 启动时从 ledger 自动收集豁免路径并注入 `CoreContext`，中间件改为查询动态列表、~~C-2（`ensure_schema` 空壳 21 处调用）~~ **✅已修复**：`ensure_schema` 函数及 21 处空调用已删除，新增编译期测试防止重新引入、~~C-3（批量写 N+1，缓存层 presence 逐条 set）~~ **✅已修复(2026-08-11)**：`get_presence_snapshots` 改用 `cache.set_batch()` 批量写入；新增 `set_presence_batch()` SQL 方法使用 `UNNEST` 单次往返；全链路新增 service/storage/cache 三层批量 API + 8 个测试、~~C-4（启动检查 N+1）~~ **✅已修复(2026-08-11)**：`schema_health_check.rs` 表/列/索引检查全部改为 `ANY($1)` / `unnest` 批量查询，30+ 表 100+ 列从 130+ 次 DB 往返降为 3 次、~~D-1（`set_raw` 本地 TTL 被丢弃，两级 TTL 不一致）~~ **✅已修复**：`LocalCache` 新增 per-key TTL `deadlines` 旁路表，`set_raw` 传入 TTL，`get_raw` 检查过期、D-2（单一缓存实例混装，低优先级待处理）、~~E-1（联邦密钥逻辑两遍 + 每次新建 HTTP client）~~ **✅已修复(2026-08-10, commit 59f07134)**：联邦 client 改用 `synapse_common::http_client` 共享实例、~~E-2（`allow_http_key_fetch` 一开关关两样防护）~~ **✅已修复(2026-08-11)**：新增 `skip_ssrf_check` 配置项，`allow_http_key_fetch` 仅控制 HTTP/HTTPS 协议，SSRF IP 黑名单由 `skip_ssrf_check` 独立控制、~~F-1/F-2（5 处无超时 client + 静默退化）~~ **✅已修复(2026-08-10, commit 59f07134)**：新建 `synapse_common::http_client` 模块提供 `default_client()`/`client_with_timeout()`/`no_redirect_client_with_timeout()`，federation client 与 federation_auth middleware 全部切换、~~G-1（上传上限三处矛盾）~~ **✅已修复(2026-08-10, commit 59f07134)**：chunked_upload_start 硬编码 100MB 改为 `ctx.config.server.max_upload_size`、~~G-3（上传上限剩余矛盾点）~~ **✅已修复(2026-08-11)**：`upload.rs` 硬编码 chunk_size_limit/chunk_size 提取为命名常量 `CHUNK_SIZE_LIMIT_BYTES` / `ASYNC_CHUNK_SIZE_BYTES`、H-1~H-3（非默认 feature 死代码、模块碎裂、通知六套并存——低优先级待处理）、~~H-4（容器接线无 CI 检查）~~ **✅已修复(2026-08-11)**：新增 `scripts/check_container_wiring.sh` 脚本，遍历 `lib.rs` 的 `pub mod` 声明，验证每个模块在 `container.rs` 或 `wiring/` 中被引用，未引用的模块报错退出、~~I-1/I-2~~ **✅已复核(2026-08-11)**：I-1 的 36 处 TODO/FIXME 中 33 处在非默认 feature 模块 (openclaw/matrix_ai)，3 处在 test mocks，生产代码无待处理项；I-2 的 22 处 `#[allow]` 逃逸中 6 处在测试文件（合理），16 处在生产代码全部已复核为密码学初始化/配置加载等"不可失败"场景，注释充分。

---

## 四、本轮新发现（两份报告均未记载）

| # | 发现 | 出处 |
|---|---|---|
| N1 | **UserService 实例化达 13 处**，远超审计报告的 3+：散落 container/wiring/admin_media/admin_security/account_identity/room_membership 等，DI 失控程度被严重低估 | `UserService::new` 全仓 grep |
| N2 | SS-04 比审计所述更深：全仓库**没有任何代码写入** `sliding_sync:room:{user}:{device}:{conn}:{room}` 这个键——`invalidate_room_cache` 删的是一个从不存在的键，是永落空的空操作，不只是"键不匹配" | 全仓 grep `sliding_sync:room:` 写路径零命中 |
| N3 | PERF-03 应升级：tokio `RwLock::blocking_read` 在 async 上下文**直接 panic**，不是"可能阻塞"。WorkerBus 一旦被 Clone（且被 .clone() 广泛持有）即为定时炸弹 | `worker/bus.rs:456-472` |
| N4 | 联邦接收端其实已有完整防护（txn 去重 + 哈希 + 签名验证，`transaction.rs:217, 234`）——审计 FED-02 证伪的同时说明**发送端 client.rs 与接收端防护水平不一致**，client 侧的 backfill 等无验证方法虽暂无调用方，但属于陷阱代码 | `transaction.rs` vs `client.rs:543-600` |
| N5 | A-1 的 since 语义是**双模式**：stream_id < 1e12 走 StreamOrdering，否则回退 OriginServerTs 时间戳——时间戳回退路径的同毫秒并发漏读风险仍在，清单"纯时间戳"的表述需修正 | `event_fetch.rs:33-50, 86-107` |
| N6 | `execute_sync` 外层超时已改为 `timeout+15s`（`handlers/sync.rs:148`），但 `room_sync_with_timeout` 仍硬编码 60s——**同一次同步有两个不一致的外层超时**，改 SS-09 时需两处对齐 | `handlers/sync.rs:148` vs `sync_service/mod.rs:310` |

---

## 五、优化方案（分批）

### 第 0 批：清理过期信息（立即，零代码风险）

1. ✅ 在审计报告 HTML 顶部标注 E2EE-01/02/08/11、FED-02 已修复/证伪，防止他人按旧报告开工。
2. ✅ 删除或接线不可达代码：`cross_signing::upload_cross_signing_keys`（E2EE-07，已删除）、federation client 的 `backfill/get_event/get_missing_events`（FED-05，已有调用方，不再是死代码）、`assemble/` 整模块（ARCH-05，已降级为重构中间产物）、`wait_for_room/wait_for_user`（A-6，已删除）。

### 第 1 批：安全止血 ✅ 全部完成

| 项 | 动作 | 状态 |
|---|---|---|
| S1 | 设备密钥/OTK/回退密钥验签失败**拒绝存储** | ✅ |
| S2 | `get_server_keys` 缓存前验证 signatures | ✅ |
| S3/S4 | `verify_auth_chain` 补校验；状态解析精确匹配 | ✅ |
| S5 | `Aes256GcmKey` derive `Zeroize, ZeroizeOnDrop` | ✅ |
| S27 | media PUT 端点补 `DefaultBodyLimit` | ✅ |
| S16 | XFF 改为从右往左跳过 trusted_proxies | ✅ |
| S25 | create_room 改用 `AuthenticatedUser` + 拒绝 guest | ✅ |

### 第 2 批：同步链路性能 ✅ 全部完成

| 项 | 动作 | 状态 |
|---|---|---|
| S6 | v2 sync 接入 EventNotifier 事件驱动 | ✅ |
| S7 | `get_raw` 加 Redis 回源 | ✅ |
| S9 | 豁免表补 v4/sync | ✅ |
| S10/N2 | 删掉 `invalidate_room_cache` 空操作 | ✅ |
| S11 | 删除路由层重复指标 | ✅ |
| S12 | materialize 查询 7→5 合并 | ✅ |
| S13/N6 | 外层超时统一为 `client_timeout + 15s` | ✅ |
| S14 | timeline 按 pos 水位线过滤 | ✅ |
| S15 | sliding sync 限流接入 fail_open_on_error | ✅ |
| S26 | 新增 sliding_sync_rate_limited_total 告警 | ✅ |
| S22/N3 | WorkerBus Clone 去掉 blocking_read | ✅ |
| S21 | QueryCache 读路径不持写锁 | ✅ |
| S17 | 联邦限流加 fail_open_on_error 配置 | ✅ |

### 第 3 批：存储与缓存正确性 — ✅ 全部完成

| 项 | 状态 |
|---|---|
| S18：DAG BFS 改递归 CTE | ✅ |
| S19：add_receipt/delete_connection_data 包事务 | ✅ |
| S20：列存在性改 SQLSTATE 判断 | ✅ |
| STO-05：check_rate_limit FOR UPDATE | ✅ |
| C-2：删 ensure_schema 空壳 | ✅ |
| D-1：LocalCache per-key TTL | ✅ |
| PERF-05：claim_task 改直查 | ✅ |
| ~~PERF-08：广播同时本地失效~~ | ✅ (commit 8c51447e) |
| ~~C-4：启动检查批量查询~~ | ✅ (2026-08-11) |
| C-3：缓存层 presence 逐条 set | 待处理（低优先级） |
| D-2：拆分缓存实例 | 待处理（低优先级） |

### 第 4 批：架构与债务 — ✅ 全部完成（仅余 H-1~H-3/D-2 低优先级项）

| 项 | 状态 |
|---|---|
| S23/N1：UserService/AuthService 收敛单例 | ✅ |
| S24：DirectoryService 落库 | ✅ |
| B-1：限流配置类型二留一 | ✅ (commit 8c51447e) |
| E-1：联邦密钥拉取合一 + Client 全局复用 | ✅ (commit 59f07134) |
| F-1：五处 client 统一超时工厂 | ✅ (commit 59f07134) |
| G-1：上传上限统一读 config | ✅ (commit 59f07134) |
| E-2：SSRF 防护与 HTTP 协议开关分离 | ✅ (2026-08-11) |
| G-3：上传上限剩余硬编码提取为常量 | ✅ (2026-08-11) |
| FED-06：server_resolution_cache 加 TTL | ✅ (commit 84152265) |
| A-7：notifier map 空闲槽位回收 | ✅ (commit 8c51447e) |
| WORK-04：HealthChecker 心跳活性探测 | ✅ (commit 8c51447e) |
| SEC-03：OIDC 明文密码默认拒绝 | ✅ |
| WORK-01：unsubscribe 退订 Redis Pub/Sub | ✅ (2026-08-11) |
| WORK-05：Redis 发布 DLQ + 重放 | ✅ (2026-08-11) |
| B-4：豁免表从 route_ledger 自动派生 | ✅ (2026-08-11) |
| C-3：缓存层 presence 批量写入 | ✅ (2026-08-11) |
| H-4：容器接线 CI 检查脚本 | ✅ (2026-08-11) |
| I-1/I-2：TODO/FIXME + #[allow] 复核 | ✅ 已复核 (2026-08-11) |
| H-1：删除非默认 feature 死代码 | 待处理（低优先级，需产品边界决策） |
| H-2/H-3：模块合并 | 待处理（低优先级，重构性质） |
| D-2：单一缓存实例混装 | 待处理（低优先级） |

### 剩余待处理项汇总

| 优先级 | 项 | 描述 |
|---|---|---|
| P3 | D-2 | 单一缓存实例混装（低优先级，需拆分 moka 实例） |
| P3 | H-1 | 删除非默认 feature 死代码（需产品边界决策） |
| P3 | H-2/H-3 | 模块合并（重构性质，低风险） |

### 验收基线（呼应项目 TDD 规范）

- 每批完成后跑 230 个集成测试文件全量回归；
- 第 2 批验收：空闲长轮询 30s 内 DB 查询数 = 0（S6 ✅已修复——轮询循环替换为 EventNotifier 事件驱动）、`sliding_sync_slow_requests_total` 在健康服务器上不再增长（S11 ✅已修复——路由层不再计数，service 层扣除 idle_wait_ms）；
- 第 1 批验收：伪造签名上传返回 4xx（S1）、伪造 XFF 不改变限流桶（S16）。
