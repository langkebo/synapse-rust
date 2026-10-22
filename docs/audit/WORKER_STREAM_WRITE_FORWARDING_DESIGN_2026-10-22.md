# `worker.stream_writers` 写转发设计（W-I）

> 状态：**第 0–5 阶段全部落地**（八路真转发 + 联邦面 + 管理面 + 指标 + 客户端 IP 传递 +
> 观测面/健康/告警 + 两实例 e2e），提交 `f31c0a422` / `18b676444` / `8bf1a191b` /
> `64b62cdb2`。§9 只剩"流式转发"一条 P3（**等压测证据**，见该行理由）。
> 两处结构性边界记在 §5.3：delayed-event 派发（已改由进程归属决定）、
> "既有 handler 后来开始写事件"这类行为漂移（棘轮盲区）。
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

**这一跳不走代理**（`no_proxy_client_with_timeout`，2026-10-22 补）。两个理由：

- 语义上 `worker.instance_map` 指的是**同一部署里的另一个进程**，这一跳不该出网。环境里有
  `HTTP_PROXY` 时，请求会被交给代理、代理再连它自己的"上游"，目标实例根本收不到 ——
  表现出来是 502 而不是 503，于是"writer 挂了"和"没人在那个地址上"变得一样。
- 安全上更不能走：请求带着 `x-synapse-worker-secret`，它同时是 `/_synapse/worker/v1/*` 的
  认证凭据。让一个环境变量把集群内部凭据送到部署之外的进程里，等于把内部认证交给装机默认值。

这个坑是两实例 e2e 测出来的（测试机上 `HTTP_PROXY` 指向本地代理，代理能连回环，
所以"端口开着就成功、关着就 502"）。单实例 + 桩上游的测试永远看不见它：桩上游在同一进程里
用 `tower::oneshot` 调用，根本没有网络。

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

**八路全部可转发**，覆盖**四个**请求命名空间。归属判据只有两条（`write_streams.rs` 的
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

管理面（`/_synapse/admin/`，156 条写路由里只有这 16 条归流，逐条明细见 §5.3）：

| 流 | 覆盖的路由（规范化后，版本段已去掉） |
|---|---|
| `events` | `rooms/{id}/{ban,kick,ban/{u},kick/{u},unban/{u},make_admin,backfill}`、`rooms/{id}/members/{u}`（PUT/DELETE）、`join/{id_or_alias}`、`send_server_notice` |
| `device_lists` | `users/{u}/devices/{d}`（DELETE）、`users/{u}/devices/{d}/delete`、`users/{u}/logout`、`user_sessions/{u}/invalidate` |

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

### 5.3 `/_synapse/admin/*` 已纳入（阶段 5）

原先这里写的是"admin 面不在覆盖范围"，理由是"不在 `declared_ledger_all()` 里"。**那个理由
是错的**：admin 路由**本来就在**账本里（`derived_route_table_always.inc.rs` 有
`/_synapse/admin/…`），只是 `in_mapped_namespace()` 把它过滤掉了。所以补法不需要换账本来源，
只需要：`mapped_path` 认 `/_synapse/admin/{v1,v2}`、新增 `WriteSurface::Admin`、把 156 条写
路由逐条冻进清单。

结果：**156 条里 16 条真的写受管流**，其余 140 条记 `-`。

| 流 | 条数 | 路由 |
|---|---|---|
| `events` | 12 | `POST …/rooms/{id}/ban`、`POST …/ban/{u}`、`POST …/kick`、`POST …/kick/{u}`、`POST …/unban/{u}`、`PUT`/`POST …/make_admin`、`PUT …/members/{u}`、`DELETE …/members/{u}`、`POST …/rooms/{id}/backfill`、`POST /_synapse/admin/v1/join/{room_id_or_alias}`、`POST /_synapse/admin/v1/send_server_notice` |
| `device_lists` | 4 | `DELETE …/users/{u}/devices/{d}`、`POST …/devices/{d}/delete`（同一 handler 的兼容别名）、`POST …/users/{u}/logout`、`POST …/user_sessions/{u}/invalidate` |

落点（全部读 handler 得出，不是按路径名猜）：

- ban/kick/unban → `admin_*_user_membership` → `apply_moderation_transition`，后者在
  `moderation.rs:470` **真的** `create_event` 一条 `m.room.member` 并 `sign_and_broadcast_event`
  （前半段只动 `member_storage`，容易只看到那半段就以为不写事件）；
- `members` → `join_room` / `leave_room`；`make_admin` → `grant_room_admin` →
  `upsert_power_levels_event`（一条真的 `m.room.power_levels`）；
- `backfill` → `backfill_room_history` → **每个取回的 PDU 都 `create_event_with_graph`**，且
  对端列表来自 `get_joined_servers_in_room` —— 这条是最容易漏的；
- `send_server_notice` → `create_room` + `add_member` + `send_message`；
- 设备那 4 条 → `token_auth.revoke_device` / `logout_all`。

**三条看着像、核实后是 `-`**（这类"看着像"的判断必须落到具体行号才算证据）：

| 路由 | 实际做的事 |
|---|---|
| `POST /_synapse/admin/v1/shutdown_room` | `UPDATE rooms SET is_public = false, name = …` + 清成员行（`synapse-storage/src/room/mod.rs:1023`）。不发事件、不广播 |
| `POST /_synapse/admin/v1/users/{u}/evict` | 逐房间调**存储层** `remove_member`，不是 `leave_room`（`admin_user_service.rs:321`）⇒ 不产生 `m.room.member`。⚠️ 这是一处与上游的语义差异：上游按正常 leave 路径退房 |
| `PUT\|DELETE …/rooms/{id}/listings/public` | 只改 `rooms.is_public` + `room_directory`（`synapse-storage/src/room/admin.rs:417`），没有 `m.room.join_rules` |

还有两处结构性边界（与 admin 无关，仍然是有意不覆盖）：

| 不覆盖的东西 | 为什么 | 现状 |
|---|---|---|
| delayed-event 派发 | 绕过 HTTP，没有可映射的路由 | 已用进程归属关闭（§5.1） |
| "既有路由**开始**写事件" | 棘轮比的是 `(method, path)` 集合，不是 handler 行为。`v1/invite` 将来补落库、`m.call.hangup` 将来补发事件，清单都不会变红 | **棘轮盲区**，已写进 `write_stream_map.txt` 头部，改这类 handler 时必须回来重看那一列 |

> ⚠️ 一条值得留下的教训：这张表的条数**每扫一遍就变一点**（正则扫 11 → 传递闭包扫 14 →
> 逐 handler 读出来 16）。手工枚举不可靠，所以补法必须是"账本 + 棘轮"：现在新增一条 admin 写
> 路由而没在清单里做归属决定，`the_frozen_map_covers_exactly_the_real_client_write_surface`
> 会红。反过来，清单里那 140 个 `-` 也不是免检的 —— 一旦有 handler 从"只改表"变成"发事件"，
> 棘轮看不见（见上面那条盲区）。

> ⚠️ 一个容易搞错的对照：`POST /_matrix/client/v3/admin/room/{room_id}/redact` 虽然在
> 名字里带 admin，但它住在**客户端命名空间**、在账本里，所以**已经**归 `events`（§5 表）；
> 而 `/_synapse/admin/v1/rooms/{id}/cascade_redact` **不**持久化 `m.room.redaction`
> （只回填 `events.redacted_by`），所以是 `-`。

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
| `GET /_synapse/worker/v1/topology/validate` | **已有 `write_forwarding` 段**（§6.1）：逐流给出解析到的 `base_url`、`Unavailable` 的原因、目标实例是否在注册表里活着、以及最近一次转发的结局 |
| `GET /_health` | **已反映**（§6.2）：`checks.write_forwarding` 在"目标不可用 / 目标没在跑 / 最近一次转发失败 / 通知绑定流无 Redis"时把整体判成 `degraded` |
| 告警 | **已加**（§6.3）：`WorkerWriteForwardFailed`（critical）+ `WorkerWriteForwardStalled`（warning） |
| 启动日志 | 非 `events` 属主的实例会打一条 "Skipping the MSC4140 delayed-event dispatcher…"，这也是"这台不派发"的唯一可见证据 |

### 6.1 `topology/validate` 的 `write_forwarding` 段

响应里新增一段（与 `stream_writers`、`runtime` 并列）：

```json
"write_forwarding": {
  "enabled": true,
  "instance_name": "client_reader",
  "notify_without_redis": [],
  "streams": [
    { "stream": "events", "owners": ["event_persister"],
      "route": "forward", "target_instance": "event_persister",
      "target_base_url": "http://event_persister:8008", "reason": null,
      "requires_redis": true, "last_outcome": "ok", "last_outcome_age_secs": 3,
      "writer_live": true },
    { "stream": "typing", "owners": ["client_reader"],
      "route": "local", "target_instance": null, "target_base_url": null,
      "reason": null, "requires_redis": false,
      "last_outcome": null, "last_outcome_age_secs": null, "writer_live": null }
  ]
}
```

三条设计取舍：

1. **路由来源是装配时发布的那一份，不是从配置重算的**。`WriteForwarding::from_config`
   读密钥文件失败时会把每一路折叠成 `Unavailable`，而 `write_route()`（纯配置函数）
   仍会说 `Forward` —— 从配置重算就恰好在最该说实话的那个场景里撒谎。所以装配调用
   `write_forwarding.publish_routes()`，报告读那一份；没跑过装配（测试）才回落到配置。
2. **`writer_live` 来自注册表**（心跳），不是网络探测。它回答的是"配置里点名的那个
   实例还在不在跑"，配置本身回答不了这个问题。
3. **`last_outcome` 是"最近一次真实转发"**，不是计数器累计值：累计值在"启动时错一次、
   之后一直好"的场景里会永久显示失败。

### 6.2 `/_health` 的判据

`checks.write_forwarding.status` 取四个值，规则都在 `WriteForwardingReport::status()` 里：

| status | 条件 | 对整体 |
|---|---|---|
| `disabled` | `worker.write_forwarding` / `worker.enabled` 关着 | 不影响 |
| `degraded` | 任一路 `route == "unavailable"`（配了但用不了） | 整体降为 `degraded` |
| `degraded` | 任一路 `writer_live == false`（注册表里没在跑） | 同上 |
| `degraded` | 任一路 `last_outcome ∈ {unreachable, refused}` | 同上 |
| `degraded` | `notify_without_redis` 非空（§4.5） | 同上 |
| `healthy` | 以上都不成立 | 不影响 |

**为什么是 `degraded` 而不是 `unhealthy`**：本实例的读路径完全正常。把整台 reader 判死
会让它被摘出负载均衡，而"重启一个 writer 挂掉的 reader"修不好任何东西 —— 这与本文件里
redis / schema 的口径一致（只有数据库不可用才是 `unhealthy`）。

**`upstream_error` 不降级**：writer 答了 403/429/500，说明路径通着，那是它对**某一次写**
的裁决。把 403 洪峰当 outage 会把这条告警训练成噪音。

`/health`（容器探针）只带配置级判据（不读注册表）：它是几秒一次的探针，为了一条
"writer 进程还在不在"多打一次数据库不值得；那一半留给 `/_health`。HTTP 状态码仍只由
数据库决定 —— 探针不该因为写路径坏了而重启容器。

### 6.3 告警

`docker/deploy/prometheus/alerting-rules.yml` 的 `worker-write-forwarding` 组：

| 告警 | 表达式 | 严重度 | 说什么 |
|---|---|---|---|
| `WorkerWriteForwardFailed` | `sum by (stream,outcome) (rate(worker_write_forward_total{outcome=~"unreachable\|refused"}[5m])) > 0`，`for: 5m` | critical | 写**没送到** writer，这些路径上的写全 503、数据没落库 |
| `WorkerWriteForwardStalled` | `sum by (stream) (rate(...[5m])) > 0 and sum by (stream) (rate(...{outcome="ok"}[5m])) == 0`，`for: 10m` | warning | 送到了但一次都没成功（典型：writer 活着但在拒流量） |

两条都不含 `upstream_error`（理由同上）。序列只在 `write_forwarding: true` 时才存在 ⇒
默认部署不会误报。

## 7. 阶段划分

| 阶段 | 内容 | 状态 |
|---|---|---|
| 0 | 映射表 + 决策层（纯函数）+ 棘轮守卫 + 校验收紧 | ✅ |
| 1 | 中间件 + 装配 + 六路转发 + 桩上游测试 | ✅ |
| 2 | `events`：客户端面 + vendor 面 + 联邦面映射、delayed-event 归属、体积上限跟随上传上限 | ✅ |
| 3 | `device_lists`：设备管理面整体归属 + 与 `events` 的耦合约束 | ✅ |
| 4 | 指标 + 客户端 IP 传递（secret 门控的专用头） | ✅ |
| 5 | **收口**：Redis 前置条件校验（§4.5 ✅）、admin 面 156 条纳入映射（§5.3 ✅，16 条归流）、`topology/validate` 的 `write_forwarding` 响应段与 `/_health` 判据（§6.1/§6.2 ✅）、告警（§6.3 ✅）、两实例 e2e（§9 ✅） | ✅ 完成 |

## 8. 与既有守卫的关系

- `worker.surface_gate`（W-F）**只管读面收窄**；本中间件装在其内层，所以"不归这个角色"
  的路径仍然 404，不会被转发掩盖。唯一的例外是 §5.4 的窄授权，而它只对"本实例就是该
  流 writer"的写生效。
- `worker.topology.strict`（W-B）此前只让拓扑错误变致命；本阶段的 `Unavailable`、
  耦合冲突、`worker.enabled = false` 走同一通道 ⇒ `strict: true` 的部署启动即失败。
- `tests/unit/worker_write_stream_tests.rs` 用 `declared_ledger_all()` 比对真实路由表
  （客户端 + vendor + 联邦 + **管理面**四段）：每个写路由要么映射到某一路流，要么落在
  "明确不归流管"的清单里；新增写路由而不做决定 ⇒ 测试红。第二个方向是第三列必须与
  `stream_for_write()` 一致，第三个方向是每条映射的命名空间必须被该流
  `mapped_surfaces()` 声明过。第四个方向是 admin 段的条数被冻住（156 / 16 / 12 / 4），
  批量重新生成把某列翻面会在这里暴露。
- 分类器本身拆成三个文件（都压 600 行棘轮）：`write_streams.rs`（流/路由决策）、
  `write_streams/paths.rs`（命名空间与段规范化）、`write_streams/admin.rs`（管理面 16/156）。

## 9. 剩余待做（按严重度）

前两项"配置开着、保证却不成立"的已收口（§4.5 Redis 前置条件、§5.3 admin 面）；下面几项仍然没有：

| 项 | 是什么 | 为什么必须做 | 代价 |
|---|---|---|---|
| ~~P1 Redis 前置条件~~ | ✅ **已做**：`missing_notify_fan_out` 接进启动 / CLI / admin 视图（§4.5） | — | — |
| ~~P1 admin 面~~ | ✅ **已做**：156 条全部冻进清单，16 条归流（`events` 12 / `device_lists` 4），`WriteSurface::Admin` 独立分类（§5.3） | — | — |
| ~~P2 观测面~~ | ✅ **已做**：`topology/validate` 的 `write_forwarding` 段（§6.1，含 `base_url` / `reason` / `writer_live` / `last_outcome`）＋ 告警两条（§6.3） | — | — |
| ~~P2 reader 健康~~ | ✅ **已做**：`/_health` 的 `checks.write_forwarding`（§6.2，带注册表存活）；`/health` 带配置级片段，HTTP 状态码仍只由数据库决定 | — | — |
| ~~P3 两实例 e2e~~ | ✅ **已做**：`tests/unit/worker_write_forwarding_two_instance_tests.rs` 起**两个真 HTTP 服务**（各自 `WorkerConfig` + 真 `instance_map` + 同一把 secret，两个中间件按生产顺序叠），覆盖转发落地、§5.4 的 surface gate 组合、防环（含伪造信封）、死 writer 的 503 与报告。⚠️ 是**同进程两个实例**，不是两个 OS 进程 | — | — |
| P3 流式转发 | 现在整段 buffer（上限 `server.max_upload_size`，50 MB），N 个并发联邦事务 = N×50 MB 峰值 | 只在联邦大事务压测下暴露；当前无证据说明会触发 | 需重写 framing，成本高，**等压测证据** |
