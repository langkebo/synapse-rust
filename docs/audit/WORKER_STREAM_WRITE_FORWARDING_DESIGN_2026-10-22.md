# `worker.stream_writers` 写转发设计（W-I）

> 状态：**第 0–4 阶段全部落地**（八路真转发 + 联邦面 + 指标 + 客户端 IP 传递），提交
> `f31c0a422`；**第 5 阶段（收口）部分完成** —— Redis 前置条件已校验（§4.5），
> admin 面 14 条、`write_forwarding` 响应段、两进程 e2e 仍未做（§9）。
> 三条结构性边界记在 §5.3：`/_synapse/admin/*` 的 14 条真写者、delayed-event 派发
> （已改由进程归属决定）、"既有 handler 后来开始写事件"这类行为漂移（棘轮盲区）。
> 相关：`WORKER_TOPOLOGY_VERIFICATION_AUDIT_2026-10-16.md`（W-A…W-H）、
> `synapse-services/src/worker/runtime_topology.rs`、`synapse-services/src/worker/write_streams.rs`。

## 1. 这个配置今天是什么状态

`worker.stream_writers` 有 8 个字段（`events`/`typing`/`to_device`/`account_data`/
`receipts`/`presence`/`push_rules`/`device_lists`），全部默认 `["master"]`。
它被解析、被校验基数、被 `/_synapse/worker/v1/topology/validate` 暴露，
`validate_worker_config` 还会在非默认时打一条 warning 说明"没有消费者"。

也就是说：**配了它，什么都不会发生**，只有一条日志提醒你别当真。
这与 `worker.surface_gate` 在 W-F 之前的处境完全一样，处理方式也应该一样 ——
把声明变成可执行的事实，而不是继续加注释。

## 2. 先纠正一个很容易照搬上游的理由

上游 Synapse 要求每路只有一个 writer，理由是**流位置由进程内的
`StreamIdGenerator` 分配**，多个进程同时写会让 `stream_ordering` 互相撞车。

**本仓不是这样**：`events.stream_ordering` 的默认值是
`nextval('events_stream_ordering_seq')`（`migrations/00000000_unified_schema_v12.sql:91`、
`:352`）。序列在数据库里，任何进程取到的都是全局单调递增的值
⇒ **多进程写事件不会破坏流顺序**。

所以在本仓，"单写者"真正防的不是顺序，而是**重复的提交后副作用**：

| 副作用 | 位置 | 多写者时的后果 |
|---|---|---|
| 联邦广播（签名 + 发往远端） | `messages.rs::run_post_commit_fan_out` → `sign_and_broadcast_event` | 同一条事件可能被**两个实例各广播一次** |
| appservice 事务派发 | 同上 → `dispatch_appservice_event` | AS 收到重复事件 |
| 房间摘要刷新 | 同上 → `room_summary_service` | 重复计算，非致命但浪费 |

这是**必须写进文档的一句**，否则下一个人会把上游的推理照抄过来，
然后发现"顺序本来就是对的"，进而怀疑整个设计没必要。

## 3. 为什么选择"服务端透明写转发"

有三条路：

| 方案 | 做法 | 问题 |
|---|---|---|
| A. 只改反代 | nginx 按 method/path 把写请求分给 writer | nginx 不知道"哪个路径属于哪一路流"，这份语义映射只有服务端有；而且 method 不等于写（`POST /sync` 是读） |
| B. 服务层转发 | 在 `MessagingService::send_message…` 等 8~30 个服务方法里各自转发 | 每个方法要一份 envelope；鉴权上下文、幂等键、错误码都要逐个复制；漏一个方法就悄悄破了单写者 |
| C. **路由层透明转发**（本方案） | 中间件判定"这次写属于哪一路流"，非 writer 实例原样转发给 writer | 需要一份 `(method, path) → stream` 映射，且必须可棘轮 |

选 C，理由：

1. **写路径只有一份实现**。writer 收到转发后跑的是它自己的 handler，包括它自己的鉴权、
   校验、限流、txn 去重 —— 转发层不复制任何业务逻辑（AGENTS.md 铁律 2）。
2. **反代可以继续"不做分流"**。谁接到请求都行，服务端自己把写送到 writer。
   这比"必须把 nginx 规则写对"可靠得多，也让 W-F 的 `surface_gate` 保持"只收窄读面"的语义。
3. 映射表可以用仓里既有的棘轮手段（`declared_ledger_all()`）盯住，不会静默腐烂。

代价（必须承认）：**多一跳网络**。写请求的延迟 += reader↔writer 的往返。
单进程部署（默认 `stream_writers = ["master"]`、`write_forwarding = false`）零变化。

## 4. 协议

### 4.1 请求

转发保持**原方法、原路径、原 query、原 body、原 `Authorization`**，只加三个头：

| 头 | 值 | 作用 |
|---|---|---|
| `x-synapse-write-stream` | 流名（如 `receipts`） | 目标实例据此判断"这是一次转发，别再转" |
| `x-synapse-worker-secret` | `worker.replication.http.secret` | 与 `/_synapse/worker/v1/*` **同一把**共享密钥；同时是上面那个头的可信凭据 |
| `x-synapse-client-ip` | 裸客户端地址（可选） | 让 writer 的按 IP 限流跟着客户端走，而不是按 reader 聚合；只在 secret 匹配时被采信（§4.4） |

**不剥 `Authorization`** 是刻意的：writer 用自己的 DB 重新校验客户端令牌，
所以转发不引入新的信任关系，也不需要"代表用户"的内部凭据。

### 4.2 防环

writer 收到带 `x-synapse-write-stream` **且** secret 正确的请求 ⇒ **就地执行，不再转发**。
若两个头都在但 secret 不对 ⇒ **忽略这两个头**（当作普通请求）并剥掉它们，
避免外部请求伪造标记去影响路由。

> 伪造这个头能绕过什么？什么也绕不过 —— 它的唯一作用是"别转发"，
> 而"不转发"就是 `write_forwarding = false` 时的默认行为。写它只是为了让
> 循环不可能发生，不是授权机制。

### 4.3 失败矩阵

| 情况 | 行为 |
|---|---|
| writer 是**本实例** | 本地执行（不转发） |
| `write_forwarding = false` | 本地执行，并每启动一次告警"配置未生效" |
| 配置的 writer 不在 `instance_map` | **启动校验报错**；运行期若仍发生 ⇒ `503 M_UNKNOWN` |
| `replication.http.enabled = false` 或无 secret | 同上（没有密钥就无法防环） |
| 连接失败 / 超时 / 非 2xx | 把 writer 的响应**原样**返回（含 4xx/5xx body）；连接层失败 ⇒ `503`，**不自动重试** |
| body 超过 `MAX_FORWARD_BODY` | `413 M_TOO_LARGE`，与 `RequestBodyLimitLayer` 同语义 |

**不自动重试**是设计选择：写请求的幂等由 `room_event_txn_dedup`（事件）和
各流自身的语义保证，转发层重试会在"writer 已提交、响应丢失"时放大成重复写。
客户端重试 + 现有去重才是正确的组合。

### 4.4 客户端 IP（已实现）

转发时追加**第三个头** `x-synapse-client-ip`（裸地址，不是列表），并且**只在与
`x-synapse-worker-secret` 同时出现且 secret 匹配时**才被采信
（`write_forwarding::forwarded_client_ip`，全仓唯一判定点）：

| 情况 | 行为 |
|---|---|
| 三个头齐全 + secret 匹配 | 用头里的地址作为限流 key（限流中间件在转发层**之外**，读不到 policy 对象，因此读进程级的 `process_secret()`） |
| 缺 secret / secret 不对 | 当作没这个头：用对端地址（= 转发前的行为），且三个头在放行前被一并剥掉 |
| 值不是裸 IP（列表、主机名、空） | 同样当没有 —— 这个值会进限流 key，不能让垃圾值变成 key |
| 拿不到 `ConnectInfo`（无 connect-info 的嵌入/测试） | 不发这个头，writer 退化成按 reader 聚合 |

**为什么不用 `X-Forwarded-For`**：限流侧已有 `effective_client_ip(...)` 会在
`trust_forwarded = true` 且对端命中 `trusted_proxies` 时采信 `X-Forwarded-For`。
把 reader 实例加进 `trusted_proxies` 等于让"reader 可达"变成"任何转发头都可信"，
而这一层需要的判定是"带 worker secret 的那一个头"，两者不是同一件事。共用一个头
会让两个信任模型互相污染。

其余语义损失：

- **超时叠加**：转发层用 120 s 的 client 超时，比外层
  `request_timeout_middleware` 长，所以客户端看到的仍是原有超时预算 —— **预算不变，但
  见 §4.5：跨实例唤醒需要 Redis，否则"多久拿到数据"会退化到等满这个预算**。
- **响应体上限**：writer 的响应被整段读回（<= `server.max_upload_size`）后才回给
  客户端，超过则 `503` 而不是半截输出。

### 4.5 跨实例 `/sync` 唤醒需要 Redis（`events` / `to_device` 的隐含前置条件）

`events` 与 `to_device` 转发之后，写发生在 writer 上，而"把长轮询叫醒"这一步仍然是
**进程内 `Notify` + Redis pub/sub**：

- 落事件后 `notifying_event_writer` 调 `notify_room`（`synapse-services/src/event_notifier.rs:242`）；
  `sendToDevice` 处理器对每个收件人调 `notify_user`（`synapse-web/src/routes/e2ee/devices.rs:191`）；
- 两者都是「本地 `notify_waiters()` + `publish_redis()`」，而 `publish_redis` 在没有
  Redis pool 时**直接 return**（`event_notifier.rs:463-466`），pool 只在
  `container.rs:432` 的 `if config.redis.enabled` 分支里装配；
- `/sync` 的等待端（`sync_service/event_fetch.rs:172-192`）只有在**没有** notifier 时才退化
  成周期轮询；有 notifier 时只等 `Notify` 或 `remaining` 超时，**没有兜底轮询**。

⇒ `write_forwarding: true` + `redis.enabled: false` 时，writer 上的写**不会**唤醒 reader 上的
长轮询，客户端要等自己的 `timeout`（典型 30 s 量级）才拿到数据。**数据不丢**，但延迟从
"立刻"退化到"等满一个 timeout"。

W-I 之前不存在这个问题：每个实例自己写、自己唤醒。这是"把写集中到一台"必然带出来的新
前置条件 —— 它不改变响应内容，只改变**什么时候**响应，所以 §4.4 那句"超时预算不变"单独看
是正确的、合起来看会误导。

`device_lists` 不在此列：设备列表变更本来就只发 `m.device_list_update` EDU、没有本地
`notify_user`，`/sync` 侧一直是"下次醒来时带走"，与是否转发无关。

**现状：已校验（error）。** `write_streams::missing_notify_fan_out(config, redis_enabled)`
给出消息，三个判定入口共用它，所以不会出现"启动拒绝、`--validate-topology` 放行"的分歧：

| 入口 | 位置 |
|---|---|
| 启动 | `topology_gate::check_startup_topology(&config.worker, config.redis.enabled)`（`worker.topology.strict` 下直接拒绝启动） |
| 预检 CLI | `cli::topology_exit_code(&config.worker, config.redis.enabled)` ⇒ `--validate-topology` 退出码 |
| 运维视图 | `GET /_synapse/worker/v1/topology/validate` 的 `validation.errors` |

只对 `events` / `to_device` 报告，且只在这条流**真的被转发**（`WriteRoute::Forward`）时报：
本实例就是 writer 时两半都在进程内做完；目标不可解析时写请求直接失败，另有自己的 error。

## 5. 哪几路能转发

**八路全部可转发**，覆盖三个请求命名空间。归属判据只有两条（`write_streams.rs` 的
模块文档有完整版）：

| 判据 | 归属 |
|---|---|
| 写房间 DAG（`create_event` / send / state / redact / 成员变更），**或**向联邦对端广播房间级 PDU/EDU | `events` |
| 写 `device_lists_stream` / `device_lists_changes`（建设备、改设备名、删设备、改设备密钥或交叉签名），随后给同房间的远端发 `m.device_list_update` EDU | `device_lists` |
| 其余（收据、typing、账号数据、push rules、presence，以及本服务自建的 space/thread/friend/delayed 表） | 不归流，或归六路轻流之一 |

三条硬事实支撑这张表，都是逐条读 handler 得出的，不是按路径名字猜的：

1. **`m.call.hangup` / `m.call.candidates` 不写事件**。它们只动本地
   `call_sessions` 表，`call_invite` / `call_answer` 才 `create_event`。所以这两条
   明确**不**归 `events`（多一跳没有收益）。这同时是一处规范偏差：合规服务器会把
   `m.call.*` 作为房间事件发出去 —— 修 handler 时这一行必须一起删。
2. **sticky events 归 `events`**，尽管它写的是元数据表：`set_is_sticky_event` /
   `clear_is_sticky_event` 都会向房间内每个联邦对端广播 EDU，两个 writer 就是两份广播。
3. **`v1/invite` 不归 `events`，`v2/invite` 归**。v1 只校验并回签（不落库），v2 才
   `create_event_with_graph` 持久化 PDU。两者段形状相同，所以 `mapped_path` 在联邦
   面**保留版本段**才分得开。

覆盖的客户端 / vendor 族（`scripts/contract/write_stream_map.txt` 是逐条冻结版）：

| 流 | 覆盖的路由（规范化后） |
|---|---|
| `events` | `createRoom`、`rooms/create_private`、`create_dm`、`friends/dm/{u}`、`invites/claim`、`join/{id}`、`knock/{id}`、`rooms/*/{send,state,redact,invite,join,leave,kick,ban,unban,upgrade}`、`rooms/*/send/m.call.{invite,answer}/*`、`rooms/*/sticky_events`、`pinned_events`、`relations/*`、`anti_screenshot`、`admin/room/*/redact` |
| `device_lists` | `devices/{id}`（PUT/DELETE）、`delete_devices`、`logout/all`、`account/deactivate`、`keys/upload`、`keys/upload/{id}`、`keys/device_signing/upload` |
| `typing` | `rooms/{id}/typing/{user}`、`rooms/typing` |
| `to_device` | `sendToDevice/{type}/{txn}` |
| `account_data` | `user/{u}/account_data/*`、`user/{u}/rooms/{r}/account_data/*`、`user/{u}/rooms/{r}/tags/*`、`rooms/{r}/account_data/*` |
| `receipts` | `rooms/{r}/receipt/{type}/{event}`、`rooms/{r}/read_markers`、`rooms/{r}/threads/{t}/read` |
| `presence` | `presence/{u}/status`、`presence/list` |
| `push_rules` | `pushrules/{scope}/…` |

联邦面（`/_matrix/federation/`）：

| 路由 | 归属 | 为什么 |
|---|---|---|
| `v1/send/{txn}` | `events`（**同时**写 `device_lists`） | PDU 落 DAG；`m.signing_key_update` EDU 会记设备列表变更 |
| `v1\|v2/send_join`、`v1\|v2/send_leave` | `events` | 落成员 PDU |
| `v2/invite` | `events` | 持久化被邀请的 PDU |
| `v1/knock`、`v1/thirdparty/invite` | `events` | `create_event` |
| `v1/invite`、`v1/get_missing_events`、`v1/exchange_third_party_invite`、`v*/publicRooms`、`v*/user/keys/*` | 不归流 | 只校验/读/回签，不落 DAG |

### 5.1 `events` 必须连联邦面一起转（已按此实施）

只转客户端那一半，会得到"客户端写都在 writer、联邦写散落在各 reader"的混合态 ——
而 §2 那三个提交后副作用**恰恰在联邦入口也会发生**。这种"半个单写者"比不做更糟：
它看起来生效了，重复广播却还在。

联邦面纳入后仍要处理的三件事，处理方式：

- **`X-Matrix` 签名**：`destination` 仍是本服务器，`uri` 是路径+query（不含 host），
  转发不改这两者，所以 writer 侧的签名校验照旧通过 —— 不引入新的信任关系。
- **体积**：转发上限**跟随** `server.max_upload_size`（`max_forward_body`），
  不再用固定的 16 MiB。比外层 `RequestBodyLimitLayer` 小的转发上限会变成一条只在
  转发时出现的隐式写限制：一条本地能接的联邦事务，转发就 413。
- **delayed-event 派发**：它绕过 HTTP 直接调服务，**没有路由可映射**。改由
  `owns_events_dispatch(config)` 让**进程**归属：转发关着（默认）时每个进程都能派发
  （Redis 锁仲裁，行为零变化）；转发打开时只有 `events` 的 writer 启动派发器，其余
  实例打一条 info 后不启动。队列在共享库里，所以排到 reader 上的延迟事件照样会被
  writer 取走。

### 5.2 `device_lists` 不是端点，而是整个设备管理面的副作用（已按此实施）

设备列表变更由 `DeviceStorage` 的六个方法产生（`update_*_display_name`、
`delete_device`、`delete_user_devices`、`delete_devices_batch`、
`delete_user_devices_batch`、`record_device_list_change`），再往上追到 HTTP 入口就是
§5 表里那八条（外加交叉签名上传）。`broadcast_device_list_update` 随后会给**每个
同房间的远端服务器**发 `m.device_list_update` EDU —— 所以多写者不是"多几行 stream
记录"这种无害重复，而是重复联邦广播。

两个由此产生的约束：

- **`logout` 不归 `device_lists`**：本仓的 `logout` 只拉黑 token 并删 `device_tokens`，
  设备行还在，不产生设备列表变更。`logout/all` 才 `delete_all_devices`。
- **`events` 与 `device_lists` 必须指向同一实例**。`PUT /_matrix/federation/v1/send/{txn}`
  一个请求写两路流（PDU + EDU），它按 `events` 归属执行 ⇒ 那一台也会写
  `device_lists`。两边配不同实例的话 `device_lists` 会有两个写者（events writer 拿
  全部联邦 EDU + 配置的 writer 拿全部客户端设备变更），`m.device_list_update` 重复
  广播正好回来。`validate_worker_config` 用 `coupled_stream_conflicts` 把这条做成
  **启动错误**，`topology_validator::allowed_stream_writer_types` 也相应允许
  `device_lists` 挂在 `event_persister` 上（此前只允许 `master`，即该配置在它存在的
  那个部署里根本不可表达）。

### 5.3 三处**有意**不覆盖

| 不覆盖的东西 | 为什么 | 现状 |
|---|---|---|
| `/_synapse/admin/*` 的写 | 不在 `declared_ledger_all()` 冻结的范围里。该命名空间有 **156 条写路由**，纳入就得逐条做归属决定，而这套棘轮只比对"客户端 + vendor + 联邦"三段 | **已知缺口，且不是无害的**：其中至少 **14 条真的写受管流**（见下表）⇒ 这些路径上"单写者"目前**不成立** |
| delayed-event 派发 | 见 §5.1 | 已用进程归属关闭 |
| "既有路由**开始**写事件" | 棘轮比的是 `(method, path)` 集合，不是 handler 行为。`v1/invite` 将来补落库、`m.call.hangup` 将来补发事件，清单都不会变红 | **棘轮盲区**，已写进 `write_stream_map.txt` 头部，改这类 handler 时必须回来重看那一列 |

admin 面的这 14 条（逐 handler 追到 `create_event` / `upsert_power_levels_event` /
`delete_user_device(s)` 得出，不是按路径名猜；`/rooms/*/cascade_redact` 之类**不**在此列 ——
它明确不持久化 `m.room.redaction`，只写自己的表）：

| 流 | 路由 |
|---|---|
| `events`（10） | `POST /_synapse/admin/v1/rooms/{id}/ban`、`POST …/ban/{u}`、`POST …/kick`、`POST …/kick/{u}`、`POST …/unban/{u}`、`PUT`/`POST …/make_admin`、`PUT /_synapse/admin/v1/rooms/{id}/members/{u}`、`DELETE …/members/{u}`、`POST /_synapse/admin/v1/join/{room_id_or_alias}` —— 落点 `membership::admin_{ban,kick,unban}_user_membership`、`join_room`、`leave_room`、`state::grant_room_admin`（后者 `upsert_power_levels_event`，写的是一条 `m.room.power_levels` 状态事件） |
| `device_lists`（4） | `DELETE /_synapse/admin/v1/users/{u}/devices/{d}`、`POST …/devices/{d}/delete`（同一 handler 的兼容别名）、`POST /_synapse/admin/v1/users/{u}/logout`、`POST /_synapse/admin/v1/user_sessions/{u}/invalidate` —— 落点 `token_auth.revoke_device` / `logout_all` → `delete_user_device(s)` |

> ⚠️ **这个数字本身是"还没穷尽"的证据**：第一遍按正则扫 handler 得到 11 条，第二遍按
> "helper 间接调用 + 传递闭包"扫出 14 条（补上 `make_admin` 的两种拼写与
> `/join/{room_id_or_alias}`）。手工清单每扫一遍就长一点 ⇒ 补法必须是**账本 + 棘轮**，
> 而不是再手写一份更长的表。

> ⚠️ 一个容易搞错的对照：`POST /_matrix/client/v3/admin/room/{room_id}/redact` 虽然在
> 名字里带 admin，但它住在**客户端命名空间**、在账本里，所以**已经**归 `events`（§5 表）。
> §5.3 说的"admin 面"专指 `/_synapse/admin/*`。

补法（未做）：把 `/_synapse/admin/` 加进 `mapped_path`（新增 `WriteSurface::Admin`），
再把这 156 条按"是否真写受管流"逐条冻进 `write_stream_map.txt` 的 admin 段 —— 其中 14 条
归流、其余 142 条填 `-`。代价是要把棘轮的账本来源从 `declared_ledger_all()` 扩到
admin 路由表；这是"要不要为 156 条路由扩账本"的决定，不是"捎带做掉"的改动。

### 5.4 与 `worker.surface_gate` 的冲突（重要）

两个特性单独都对，合起来会互相抵消：W-F 把实例收窄到本角色的前缀，W-I 把写转给配置
指定的 writer。对最要紧的那两路，两者正好对立 —— `event_persister` 只"拥有"
`/_synapse/worker/v1/replication/*`，所以**每一条转发到它的房间写都会被 surface gate
在 handler 之前 404 掉**。

修法：surface gate 多一个**窄授权**（`WorkerSurfaceGate::stream_writes`）——只放行
"`stream_for_write()` 判定属于某路流、且本实例就是那一路的 writer"的 `(method, path)`，
并且**只有转发打开时**才存在（转发关着时 `write_route` 对一切都是 `Local`，把那个交给
gate 等于让每个实例都能应答所有写）。授权集由 `WriteForwarding::locally_written_streams()`
给出，在 `build_router` 里装配。

## 6. 可观测性

| 面 | 内容 |
|---|---|
| `worker_write_forward_total{stream,outcome}` | **已接**，`outcome ∈ ok / upstream_error / unreachable / refused`。`upstream_error` = writer 用非 2xx 回了（拒绝），`unreachable` = 传输层失败（**不重试**），`refused` = 本地就拒了（writer 不可用），两者混在一起会让"writer 在拒流量"和"writer 挂了"看起来一样 |
| 日志 | 每次转发 `debug`（流、目标、耗时、状态）；`unreachable` / `refused` 各打一条 `error`；启动期对被转发的流清单打一条 `info`，每路 `Unavailable` 再各一条 `error` |
| `GET /_synapse/worker/v1/topology/validate` | 逐条点名哪一路、哪个实例、为什么不可用（本阶段未新增 `write_forwarding` 响应段；要做需同时改响应结构与前端约定） |
| 启动日志 | 非 `events` 属主的实例会打一条 "Skipping the MSC4140 delayed-event dispatcher…"，这也是"这台不派发"的唯一可见证据 |

## 7. 阶段划分

| 阶段 | 内容 | 状态 |
|---|---|---|
| 0 | 映射表 + 决策层（纯函数）+ 棘轮守卫 + 校验收紧 | ✅ |
| 1 | 中间件 + 装配 + 六路转发 + 桩上游测试 | ✅ |
| 2 | `events`：客户端面 + vendor 面 + 联邦面映射、delayed-event 归属、体积上限跟随上传上限 | ✅ |
| 3 | `device_lists`：设备管理面整体归属 + 与 `events` 的耦合约束 | ✅ |
| 4 | 指标 + 客户端 IP 传递（secret 门控的专用头） | ✅ |
| 5 | **收口**：Redis 前置条件校验（§4.5 ✅）、admin 面 14 条纳入映射（§5.3）、`topology/validate` 的 `write_forwarding` 响应段、两进程 e2e | 🚧 部分完成（§4.5 已做） |

## 8. 与既有守卫的关系

- `worker.surface_gate`（W-F）**只管读面收窄**；本中间件装在其内层，所以"不归这个角色"
  的路径仍然 404，不会被转发掩盖。唯一的例外是 §5.4 的窄授权，而它只对"本实例就是该
  流 writer"的写生效。
- `worker.topology.strict`（W-B）此前只让拓扑错误变致命；本阶段的 `Unavailable`、
  耦合冲突、`worker.enabled = false` 走同一通道 ⇒ `strict: true` 的部署启动即失败。
- `tests/unit/worker_write_stream_tests.rs` 用 `declared_ledger_all()` 比对真实路由表
  （客户端 + vendor + 联邦三段）：每个写路由要么映射到某一路流，要么落在"明确不归流管"
  的清单里；新增写路由而不做决定 ⇒ 测试红。第二个方向是第三列必须与
  `stream_for_write()` 一致，第三个方向是每条映射的命名空间必须被该流
  `mapped_surfaces()` 声明过。

## 9. 剩余待做（按严重度）

前两项是"配置开着、保证却不成立"，与文档早先"已全部落地"的说法冲突，故单列：

| 项 | 是什么 | 为什么必须做 | 代价 |
|---|---|---|---|
| ~~P1 Redis 前置条件~~ | ✅ **已做**：`missing_notify_fan_out` 接进启动 / CLI / admin 视图（§4.5） | — | — |
| P1 admin 面 14 条 | 把 `/_synapse/admin/*` 里真写受管流的 14 条纳入映射（§5.3 有清单 + 补法） | 否则这些路径上"单写者"不成立；且该清单每扫一遍就变长，说明手工枚举不可靠 | 要把账本来源从 `declared_ledger_all()` 扩到 admin 路由表（156 行），并新增 `WriteSurface::Admin` |
| P2 观测面 | `topology/validate` 增 `write_forwarding` 段（解析到哪个 `base_url`、为何 `Unavailable`、运行期是否可达）；`worker_write_forward_total` 进 dashboard + 告警 | writer 宕机时 reader 的 `/health` 仍绿，只有写请求 503；没有告警就只能等用户报障 | 响应结构 + 前端约定同步改，1–2 天 |
| P2 reader 健康 | 让 reader 的 `/_health` 反映其 writer 不可用 | 同上：现在的健康检查对"写路径已死"完全沉默 | 依赖上一项的数据 |
| P3 两进程 e2e | 真起两个进程 + 真 `instance_map` + 真 secret，覆盖 `events`/`federation send` 与 `surface_gate` 组合 | 现有测试全部用桩上游（`spawn_echo_upstream`），最容易互相抵消的 `surface_gate` × 转发组合从未真实联调 | 一个脚本 + CI job，1 天 |
| P3 流式转发 | 现在整段 buffer（上限 `server.max_upload_size`，50 MB），N 个并发联邦事务 = N×50 MB 峰值 | 只在联邦大事务压测下暴露；当前无证据说明会触发 | 需重写 framing，成本高，**等压测证据** |
