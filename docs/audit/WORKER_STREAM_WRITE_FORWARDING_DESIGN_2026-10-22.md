# `worker.stream_writers` 写转发设计（W-I）

> 状态：**第 1 阶段已实施**（决策层 + 中间件 + 映射棘轮 + 校验收紧）；
> `events` / `device_lists` 两路**故意不转发**，配置非默认值时启动校验直接报错。
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

转发保持**原方法、原路径、原 query、原 body、原 `Authorization`**，只加两个头：

| 头 | 值 | 作用 |
|---|---|---|
| `x-synapse-write-stream` | 流名（如 `receipts`） | 目标实例据此判断"这是一次转发，别再转" |
| `x-synapse-worker-secret` | `worker.replication.http.secret` | 与 `/_synapse/worker/v1/*` **同一把**共享密钥；同时是上面那个头的可信凭据 |

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

### 4.4 已知的语义损失

- **客户端 IP**：writer 看到的是 reader 的地址，不是客户端的。按 IP 的限流因此在
  多进程下按"reader"聚合。修法（未做）：转发时追加 `X-Forwarded-For`，
  并只在 secret 校验通过时采信它 —— 这条要改限流取 IP 的地方，属独立改动。
- **超时叠加**：转发层用 120 s 的 client 超时，比外层
  `request_timeout_middleware` 长，所以客户端看到的仍是原有超时预算。

## 5. 哪几路能转发（本阶段）

八路里**只开六路**。这不是偷懒，是诚实的边界：映射必须穷尽，穷尽不了的一路就不该假装支持。

| 流 | 本阶段 | 覆盖的路由（规范化后） | 说明 |
|---|---|---|---|
| `typing` | ✅ | `rooms/{id}/typing/{user}`、`rooms/typing` | 状态在 Redis，写入本身不重 |
| `to_device` | ✅ | `sendToDevice/{type}/{txn}` | 队列写，天然带 txn |
| `account_data` | ✅ | `user/{u}/account_data/*`、`user/{u}/rooms/{r}/account_data/*`、`user/{u}/rooms/{r}/tags/*`、`rooms/{r}/account_data/*` | 含 tags（规范上是 account data 的一种） |
| `receipts` | ✅ | `rooms/{r}/receipt/{type}/{event}`、`rooms/{r}/read_markers`、`rooms/{r}/threads/{t}/read` | 会话级已读属收据 |
| `presence` | ✅ | `presence/{u}/status`、`presence/list` | |
| `push_rules` | ✅ | `pushrules/{scope}/…` | |
| `events` | ⛔ **本阶段不支持** | `rooms/*/{send,state,redact,invite,join,leave,…}`、`createRoom`、`join/*`、`knock/*`、`spaces/*/{join,leave,invite}`、`admin/room/*/redact`、`unstable/rooms/*/delayed_event/*` … | 见 §5.1 |
| `device_lists` | ⛔ **本阶段不支持** | 设备列表变化写在 `keys/upload`、`devices/*`、`logout*`、`delete_devices` 里面 | 见 §5.2 |

**配置成非默认值就直接报错**（`validate_worker_config` 的 error，`strict` 下拒绝启动）：

```text
worker.stream_writers.events 指向了 'event_persister'，但本版本的写转发不支持 events
（原因见 docs/audit/WORKER_STREAM_WRITE_FORWARDING_DESIGN_2026-10-22.md §5.1）。
把它设回 ["master"]，或等该阶段落地。
```

这条 error 是这个设计里最重要的部分：**宁可启动失败，也不要再出现"配了但没生效"**。

### 5.1 为什么 `events` 不能只转客户端那一半

事件写入有两个入口族：

1. **客户端面**：`PUT /rooms/{id}/send|state/…`、`/redact`、`/invite|join|leave|kick|ban`、
   `createRoom`、`/upgrade`、delayed event、sticky events、admin redact …
2. **联邦面**：`PUT /_matrix/federation/v1/send/{txn}`（事务）、`/send_join`、`/send_leave`、
   `/invite`、`/state/{id}`、`/backfill` 的响应处理、`/3pid/onbind` …

只转客户端那一半，会得到"客户端写都在 event_persister、联邦写散落在各 reader"
的混合态 —— 而 §2 那三个副作用**恰恰在联邦入口也会发生**。
这种"半个单写者"比不做更糟：它看起来生效了，重复广播却还在。

要支持 `events`，必须同时把联邦写面纳入映射，并处理：

- 转发后 `X-Matrix` 签名里的 `destination` 仍是本服务器，writer 会正常校验（可行）；
- delayed event 的调度器（`src/server/mod.rs` 的后台派发）在**哪个实例**跑 ——
  它绕过 HTTP 直接调用服务，因此不在转发覆盖范围内；
- 联邦事务体可能很大，转发上限要单独评估。

这些是下一阶段的工作量，不是"再补几条 match arm"。

### 5.2 为什么 `device_lists` 更麻烦

设备列表变化不是一个端点，而是**设备相关写操作的副作用**：
`POST /keys/upload`、`PUT|DELETE /devices/*`、`/delete_devices`、`/logout`、`/logout/all`、
`/account/password` 都会改 `device_lists` 的 outbound 队列。要把它们全部交给单写者，
等于把"设备管理面"整体搬到 writer 上，而设备管理又是 E2EE 的关键路径。
在 `events` 都没落地之前动这条，风险不成比例。

## 6. 可观测性

| 面 | 内容 |
|---|---|
| `GET /_synapse/worker/v1/topology/validate` | 已经会用 `errors` / `warnings` 逐条点名哪一路、哪个实例、为什么不可用（本阶段**未**新增 `write_forwarding` 响应段；要做需同时改响应结构与前端约定） |
| 日志 | 每次转发 `debug`（流、目标、耗时）；`unavailable` 时 `error` + 一次启动期告警 |
| 指标（本阶段未接） | 计划 `worker_write_forward_total{stream,outcome}`；`outcome ∈ ok / upstream_error / unreachable / refused` |
| 启动日志 | `write_forwarding = true` 时打印被转发的流清单；每路 `Unavailable` 各打一条 `error` |

## 7. 阶段划分

| 阶段 | 内容 | 状态 |
|---|---|---|
| 0 | 映射表 + 决策层（纯函数）+ 棘轮守卫 + 校验收紧 | ✅ 本阶段 |
| 1 | 中间件 + 装配 + 六路转发 + 桩上游测试 | ✅ 本阶段 |
| 2 | `events`：客户端面 + 联邦面映射、delayed-event 归属、体积上限评估 | 待做 |
| 3 | `device_lists`：设备管理面整体归属 | 待做 |
| 4 | 指标 + 客户端 IP 传递（`X-Forwarded-For` 的采信条件） | 待做 |

## 8. 与既有守卫的关系

- `worker.surface_gate`（W-F）**只管读面收窄**；本中间件装在其内层，
  所以"不归这个角色"的路径仍然 404，不会被转发掩盖。
- `worker.topology.strict`（W-B）此前只让拓扑错误变致命；本阶段的
  `events`/`device_lists` 报错走同一通道 ⇒ `strict: true` 的部署启动即失败。
- `tests/unit/worker_write_stream_tests.rs` 用 `declared_ledger_all()` 比对真实路由表：
  每个客户端写路由要么映射到某一路流，要么落在"明确不归流管"的清单里；
  新增写路由而不做决定 ⇒ 测试红。
