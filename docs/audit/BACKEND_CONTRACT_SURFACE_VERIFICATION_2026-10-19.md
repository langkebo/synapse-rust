# 后端契约面问题复核与优化方案（2026-10-19）

> **复核对象**：`matrix-js-sdk/artifacts/backend-issues-contract-surface-2026-10-18.md`（下称"原清单"）
> **后端基线**：`synapse-rust @ 9d2f152bb`（`git log -1`，工作区干净）
> **复核方式**：直接跑原清单给出的复现命令 + 逐条读后端源码 + 用后端自己的提取器
> `scripts/contract/extract_registered.py` 做独立交叉验证（不依赖 SDK 侧的推断）。
> **结论**：5 条里 **2 条归因错误**（症状真实、根因判反，按原方案改**不会修好**）、
> **2 条成立但被高估**、**1 条成立且低优先**。
> 真正该做的那件事（导出 handler 归属）原清单**没有提到**，而它是 B-1/B-4 的共同根因。

---

## 0. 判定总表

| 编号 | 原清单结论 | 复核判定 | 关键证据 |
| --- | --- | --- | --- |
| B-1 | 同一路径被两个模块**重复注册**（114 条） | ❌ **归因错误**（症状真实） | 全表 1043 条绝对路径里**重复注册 0 条**（仅 2 条 OIDC 泳道孪生，feature 互斥）。114 = **同名 handler 撞车**：42 个函数名各有 2–3 份定义 |
| B-1 影响 | 30 调用点拿不到契约，含 ban/kick/unban/invite/account_data 主路径 | ✅ **证实** | 这 5 条在 `byRoute` 里确为 MISS（见 §2.1） |
| B-2 | 145 条路由请求体不可知 | ⚠️ **数字吻合、定性过度** | 后端侧 unique 路由 141 条；其中 **59 条协议上就是任意 JSON**（account_data / filter / 事件 content / UIA / 联邦 PDU），加 struct 会**破坏规范** |
| B-3 | Query 类型解析不出（11 条） | ✅ **证实** | 逐条签名核对一致：`Query<HashMap>` ×3、`Query<Value>` ×6、`SearchQuery` ×2（后端已具名，是 SDK 侧解析不到） |
| B-4 | 派生表不记录 `.nest()` 挂载关系 | ❌ **机制误判** | 表里存的是**绝对路径** `/_matrix/vendor/v1/invites/one-time`，挂载前缀已烘焙。真实缺口是 `registered_by` 落到兜底值 `vendor`，**全表仅 2 条** |
| B-5 | 转发壳让请求体静态不可判 | ✅ **证实** | `admin/room/management.rs:180-196` 与原文逐字一致 |
| B-6 | 10 条契约变好、SDK 无需改 | ✅ **证实** | `check-wire-format` 违规 0、豁免 0 |

**新增 3 条原清单漏掉的问题**：见 §3（N-1 deny_unknown 覆盖不足、N-2 同名 handler 是**双实现**、N-3 提取器已能解析 94% 的 handler 却从未导出）。

---

## 1. 复核基线

```
后端  synapse-rust @ 9d2f152bb  (main, origin/main, 工作区干净)
SDK   matrix-js-sdk  artifacts/backend-issues-contract-surface-2026-10-18.md
门禁  node scripts/quality/check-wire-format.mjs  → EXIT=0
      967 调用点 / 可比对 87 / 违规 0 / 豁免 0
      覆盖桶 { backend-body-unknown:145, backend-query-unknown:11,
               route-contract-unavailable:30, route-not-resolved:27, ... }
```

**必须先纠正的框架**：这些桶**不是红灯**，门禁是绿的（`EXIT=0`）。它们是"没被机器校验的调用点记账"。
原清单用 🔴🟡 标级容易被读成"服务端坏了"，实际是**校验盲区**。真正会咬人的是 N-2（双实现的副作用差异）。

---

## 2. 逐条判定

### 2.1 B-1 —— 症状真、根因反

**实测 A：全表有没有重复注册？没有。**

用派生路由表（1043 行，等于 SDK 侧 `generatedRouteCount`）做 `(method, path)` 去重：

```
table entries: 1043
duplicate (method,path): 2
  2x  GET /.well-known/jwks.json            owners=oidc_fallback,oidc
  2x  GET /.well-known/openid-configuration owners=oidc_fallback,oidc
```

这 2 条是**设计内**的泳道孪生（`derived_routes.rs` 文件头写明了：default 泳道只有 `oidc_fallback`，
`builtin-oidc` 特性下才有 `oidc`，两个永远不会同时挂载）。**除此之外一条重复都没有。**

再补两个独立判据：

- axum 0.8 的 `.route()` / `.merge()` 对同
  `(method, path)` 会 **panic**。服务能起，就是不重复注册的直接证据。
- SDK 收集器的 `contractConflicts.count`（"同一 key 拿到契约不一致的多份定义"）= **0**。
  若真有双模块注册，这个数不可能为 0。

**实测 B：114 到底是什么？**

`collisionRefused` 的判据在 `admin-contract.mjs:2237`：

```js
if (defsOfName.length > 1) {                      // ← 同名 handler 有 >1 份定义
    const owner = routeOwner.get(rawRoute);
    if (!handlerAttributionAllowed({ definitionCount, owner, definitionModuleKey }))
        collisionRefused++;                       // ← 归属不了，fail-closed
}
```

复制这段逻辑（用 SDK 导出的 `parseRouteTable` / `resolveRoutePath` / `handlerAttributionAllowed`）
把 114 条全量摊开：

```
total refused = 114    distinct handlers = 42
get_statistics      [routes=6]  defs=3  def: admin/server.rs | app_service.rs | worker.rs
get_status          [routes=6]  defs=3  def: admin/server.rs | background_update.rs | telemetry.rs
get_stats           [routes=6]  defs=3  def: background_update.rs | handlers/thread.rs | room_summary.rs
ban_user            [routes=3]  defs=2  def: admin/room/management.rs | handlers/room/members.rs
kick_user           [routes=3]  defs=2  def: admin/room/management.rs | handlers/room/members.rs
unban_user          [routes=3]  defs=2  def: admin/room/management.rs | handlers/room/members.rs
register            [routes=2]  defs=2  def: admin/register.rs | auth_compat.rs
...
```

**114 条 = 42 个函数名 × 每条路由的"两个方向都拒绝"**。它们对应的路由**各不相同**：
`ban_user` 的两份定义分别服务
`POST /_matrix/client/v3/rooms/{room_id}/ban`（body 取 `user_id`）与
`POST /_synapse/admin/v1/rooms/{room_id}/ban/{user_id}`（path 取 `user_id`）——
**是两个本来就该不同的端点**，不是"同一路径注册两次"。

**真正的根因**：契约导出面**只有 `registered_by` 一个归属列**，而它的语义是
「SDK codegen 目录」（见 `extract_registered.py:941` 的 docstring），**不是**「handler 定义文件」。
SDK 被逼着用一个为别的目的造的字符串去反推 handler 归属；同名时推不出来，只能 fail-closed。

> 原清单的建议 1（"admin 侧改走 `/_synapse/admin/...` 从根上消除路径撞车"）
> **前提不成立**：那些 admin 端点**已经**在 `/_synapse/admin/...` 下
> （`admin/room/mod.rs:191/195`）。按它改不会有任何变化。

**实测 C：真实损害范围**

```
MISS  POST /_matrix/client/v3/rooms/{room_id}/ban
MISS  POST /_matrix/client/v3/rooms/{room_id}/kick
MISS  POST /_matrix/client/v3/rooms/{room_id}/unban
MISS  POST /_matrix/client/v3/rooms/{room_id}/invite
MISS  PUT  /_matrix/client/v3/rooms/{room_id}/account_data/{type}
HAS   POST /_matrix/client/v3/register
HAS   GET  /_matrix/vendor/v1/spaces/{space_id}
```

✅ 原清单对"影响面"的判断是对的：**恰好是房间管理/账户数据这条最常改的主路径**。
原因是这些 handler 的名字（`ban_user`/`kick_user`/`unban_user`/`invite_user`/`set_room_account_data`）
在 admin 面或兄弟模块里各有一份同名定义，撞车后两侧都被拒。

### 2.2 B-2 —— 数字对，但"都是缺陷"不成立

后端侧（unique 路由，v1/v3 孪生合并后）：

```
unique routes=1008  typed(struct)=159  manualRead(body.get)=41  unknown=141  noBody=667
```

**141 条得先分桶再动手**。按"协议是否允许任意键"分类（规则见附录 C）：

| 桶 | 条数 | 性质 | 该怎么办 |
| --- | --- | --- | --- |
| **A** | **59** | 协议上 body 就是任意 JSON 对象 | **保持 `Value`**，登记豁免，不算债 |
| **B** | **82** | 字段可枚举，当前只是没写 struct | 分批加具名 struct |

A 桶构成：UIA（`auth` dict 任意）18、megolm 会话键 9、联邦 PDU/事务体 9、状态事件 content 6、
account data content 4、megolm 备份会话 4、filter 定义体 2、消息事件 content 2、redact 体 2、其余 3。

> ⚠️ 给 `set_account_data` / `send_state_event` / `filter` 这类端点加
> `#[serde(deny_unknown_fields)]` 是**规范违规**：Matrix 的 account data、事件 content、
> filter 定义体按设计就是开放对象（自定义事件类型、厂商扩展字段全靠它）。
> 改成闭合 struct 会直接拒掉客户端合法的自定义内容。
> 原清单"给这批端点定义 struct 并加 `deny_unknown_fields`"只对 B 桶的 **admin 面**成立。

B 桶高价值批次（按 SDK 调用频次 + 字段稳定性）：`e2ee/devices.rs`(10)、
`directory_reporting.rs`(9)、`federation/keys.rs`(8)、`handlers/presence.rs`(5)、
`e2ee/keys.rs`(4)、`push.rs`(4)、`media/upload.rs`(4)。

### 2.3 B-3 —— 成立，且是最容易收口的一条

逐条核对签名（`io.queryType`）：

| 路由 | handler | 实参 |
| --- | --- | --- |
| `GET /_matrix/client/v3/rooms/{x}/initialSync` | `room_initial_sync` | `Query<HashMap>` |
| `GET /_matrix/vendor/v1/keys/rotation/check` | `check_needs_rotation` | `Query<HashMap>` |
| `POST /_matrix/vendor/v1/keys/rotation/check` | `check_needs_rotation_post` | `Query<HashMap>` |
| `GET /_matrix/client/v3/register/available` | `check_username_availability` | `Query<Value>` |
| `GET /_matrix/client/{v1,v3}/keys/changes` | `key_changes` | `Query<Value>` |
| `GET /_matrix/federation/v1/query/directory` | `query_directory` | `Query<Value>` |
| `GET /_matrix/client/v3/events` | `get_events` | `Query<Value>` |
| `GET /_matrix/media/{v1,v3}/preview_url` | `preview_url` | `Query<Value>` |
| `GET .../dehydrated_device/{x}/events` | `get_dehydrated_device_events` | `Query<Value>` |
| `GET /_matrix/vendor/v1/rooms/{x}/threads/search` | `search_threads` | `SearchQuery`（已具名） |
| `GET /_matrix/vendor/v1/spaces/search` | `search_spaces` | `SearchQuery`（已具名） |

后 2 条**后端没问题**——`SearchQuery` 是具名 struct，是 SDK 侧 struct 索引解析不到。
前 9 条是后端真缺口。

**已有的守卫**：`scripts/contract/ledger_annotations.txt` 的 `query_params=` 由
`check_annotation_fidelity` **双向**守卫（声明≠解析即红；解析出但没声明也红）。
但 `handler_query_fields()` 对 `manual`（`Value`/`HashMap`）状态**故意跳过**——
所以这 9 条是"守卫明知不可判而放行"的既知盲区，不是漏网。

### 2.4 B-4 —— 机制判反，影响面 2 条

```
registered_by = vendor : 2 条
  POST /_matrix/vendor/v1/invites/one-time
  POST /_matrix/vendor/v1/invites/claim
```

- **表里有绝对路径**：`/_matrix/vendor/v1/invites/one-time`，`.nest("/_matrix/vendor/v1", …)`
  的前缀已经烘焙进路径。所以"派生表不记录 `.nest()` 挂载关系"**是错的**。
- 真实缺口：这两条是**内联注册在 `assembly.rs::create_vendor_router()` 里**的
  （`assembly.rs:116-117`），`ledger_origins.txt` 只给了 `assembly.rs::create_vendor_router → vendor`
  这条**兜底规则**，没人给它们加按域规则（对比 `my_rooms → room`、`search_rooms → search`
  都有显式规则）。而 `ledger_origins.txt` 自己的注释写着：留这条兜底是**故意**的——
  "新增了 vendor 路由却忘了归域"要表现为落在 `vendor` 而不是被静默改判。
- 影响面：**2 条**，且 SDK 已用硬编码别名兜住了
  （`scripts/contract-module-map.mjs:40-42`：`"invite-link": "vendor"`，注释还引了行号）。

> 原清单说不改就是"SDK 侧在补偿后端信息缺失"。准确说：这是在**补偿一个刻意的兜底设计**，
> 代价是 1 行别名。为了 2 条路由去改 `registered_by` 会让 SDK 的 `src/invite-link/`
> 生成目录**静默搬家**（`ledger_origins.txt` 原话），风险大于收益。

### 2.5 B-5 —— 成立，1 条，低优先

`admin/room/management.rs:180-196` 与原文逐字一致。SDK 侧保持 fail-closed 是正确行为，不是缺陷。

### 2.6 B-6 —— 成立

`check-wire-format` 违规 0、豁免 0；`backendCommit` 已冻结到 `9d2f152bb`。

---

## 3. 原清单漏掉的三条

### N-1 🔴 有 struct ≠ 有校验：159 条 typed 里 51 条没有 `deny_unknown_fields`

```
typed struct routes: 159   deny_unknown_fields: 108   无 deny: 51
```

典型：`POST /_matrix/client/v3/search`（`SearchRequest`）、
`POST /_matrix/client/v3/rooms/{x}/upgrade`（`UpgradeRoomRequest`）、
`POST /_synapse/admin/v1/register`（`RegisterRequest`）、
`POST /_synapse/admin/v1/feature-flags`。

**这正是原清单 B-2 想消灭的那个病**（"多发一个字段不报错"）——加了 struct 也没治，
因为 serde 默认就是忽略未知字段。所以 B-2 的验收判据不能是"requestKnown 上升"，
必须是"`deny_unknown_fields` 覆盖数上升"。

⚠️ 但**不能一刀切**：客户端 API 加 `deny_unknown_fields` 会造成
「新 SDK + 旧服务端 = 400」。私有部署下 SDK 与服务端可共同发版，才可以用；
对外兼容的端点应当用"未知字段记 warning"而非拒绝。

### N-2 🔴 同名 handler 不只是命名冲突，是**双实现**（且副作用不同）

```
handlers/room/members.rs:625  ban_user   body 取 user_id → membership().ban_user()
                                        → 之后 sync_dm_room_membership_change("banned")
admin/room/management.rs:303  ban_user   path 取 user_id → ban_user_internal()
                                        → 无 friend-DM 同步
```

两者都叫 `ban_user`，都做"把某人踢出房间"，但**一个会同步好友私聊房间状态、一个不会**。
`kick_user` / `unban_user` 同样。

这不是契约问题，是**行为不一致**——比"契约不可判"严重得多：
同一操作走 admin 面和走 C-S 面，客户端看到的好友关系状态会不一样。
原清单完全没有提到。

### N-3 🟢 后端提取器**已经能**解析 handler 归属，只是从没导出过

用后端自己的 `Resolver` 解析 1041 条路由：

```
routes: 1041   single-handler: 981   multi: 17   unresolved: 43
handler names with >1 defining file: 39
```

- **94% 的路由可以精确解析出 handler 定义文件**；
- 39 个同名跨文件（与 SDK 侧 42 个高度重合，差值是解析粒度不同）；
- `RouteRow.__new__(cls, method, path, owner, handler=None)` 早就留了 `.handler` 字段
  （docstring 说是给 query 反向守卫用的），但**全仓没有任何地方给它赋值**——是个死字段。

**结论：SDK 要的那份数据，后端 94% 现成，缺的只是一行 JSON 导出。**
这是 B-1 与 B-4 的共同解，也是原清单没看到的。

---

## 4. 可执行方案

原则：**先补导出面（零风险、无 Rust 改动），再收口 query（单仓、低风险），最后才动 82 条请求体。**
不动 `RouteEntry`、不动 1043 行 `.inc.rs`、不动 6 个 fixture、不 bump `SCHEMA_VERSION`——
那是"改路由字面量 ⇒ 整条链"的雷区，收益与风险不成比例。

### P0 导出 handler 归属（**做这个，B-1 与 B-4 一起消失**）

#### P0-1 后端：新增 sidecar 产物

| 项 | 内容 |
| --- | --- |
| 新脚本 | `scripts/contract/gen_route_handlers.py` |
| 产出 | `docs/synapse-rust/ROUTE_HANDLERS.json` |
| 实现 | 复用 `extract_registered.Resolver`：`res.handlers[(method,path)]` + `res.resolve_handler_def()`；**不重写解析器** |
| 内容 | `METHOD path → { "file": "handlers/room/members.rs", "fn": "ban_user", "resolution": "exact" \| "unresolved" \| "ambiguous" }` |
| 门禁 | `gen_derived_routes.py --check` 增加"sidecar 与当前源码一致"一项；`check_route_contract.sh` 纳入 |
| 规模 | 981 exact / 43 unresolved / 17 ambiguous |

- 43 条 unresolved 一律标 `unresolved`，**不猜**（多集中在 `assembly.rs` 的
  `use crate::routes::handlers::…` 重导出，如 `handlers::extended_profile::get_extended_profile`）。
  SDK 对这些继续 fail-closed，与现状完全等价 ⇒ 改动**不可能让门禁变红**。
- 顺手可修（可选，同批）：`resolve_handler_def` 对 `assembly.rs` 的带 `use` 前缀路径解析失败，
  补上可把 981 提到约 1024。值不大，可留 P3。

**验收**
```bash
python3 scripts/contract/gen_route_handlers.py --check   # EXIT=0
python3 scripts/contract/gen_derived_routes.py --check   # EXIT=0（既有 fixture 一字未动）
bash scripts/contract/check_route_contract.sh            # EXIT=0
```

#### P0-2 SDK：按导出文件精确归属，删掉两条启发式兜底

`scripts/quality/lib/admin-contract.mjs`：

1. 读 `ROUTE_HANDLERS.json`（**缺席时退化为现状**，不硬失败——跨仓 pin 滞后是常态）；
2. `handlerRoutes` 改双索引：`Map<"file::fn", keys[]>` + 保留 `Map<fn, keys[]>` 作 legacy；
3. `handlerAttributionAllowed` 增最高优先规则
   **sidecar 里该路由的 `file` === 本定义文件 ⇒ 允许**（不再走 `sameRouteModuleFamily`）；
4. 删掉 B-4 那两条兜底（"同 handler 已在绝对路径登记时跳过裸路径"、"全表唯一后缀时采纳"）——
   `/_matrix/vendor/v1/invites/one-time` 有了 sidecar 归属后不再需要猜。

**验收（全部要跑）**

| 指标 | 现状 | 期望 |
| --- | --- | --- |
| `collisionRefused` | 114 | ≤ 60（余下是 sidecar 也 unresolved 的，正确 fail-closed） |
| ban/kick/unban/invite/account_data 5 条 | MISS | **全部 HAS** |
| `route-contract-unavailable` | 30 | ≤ 25 且不含上述 5 条 |
| `contractConflicts.count` | 0 | **必须仍为 0**（守卫没失效的判据） |
| `check-wire-format` 违规 | 0 | **仍为 0**（不能引入假违规） |

> ⚠️ 改完必须验 `contractConflicts=0`。它非 0 就说明归属判据放得太松，
> 把两份契约挂到同一个 key 上了——这是这套守卫唯一的回归判据。

### P1 收口 B-3（单仓、5 个 struct + 注解，1 天）

按 §2.3 的 9 条，把 `Query<HashMap>` / `Query<Value>` 换成具名 struct：

| 文件 | 新 struct |
| --- | --- |
| `handlers/room/management/metadata.rs` | `RoomInitialSyncParams` |
| `key_rotation.rs` | `KeyRotationCheckParams`（两个 handler 共用） |
| `auth_compat.rs` | `RegisterAvailableParams` |
| `e2ee/keys.rs` | `KeyChangesParams` |
| `federation/events.rs` | `DirectoryQueryParams` |
| `handlers/sync.rs` | `EventsParams` |
| `media/preview.rs` | `PreviewUrlParams` |
| `handlers/dehydrated_device.rs` | `DehydratedDeviceEventsParams` |

然后 `ledger_annotations.txt` 为这 9 条补 `query_params=`。

**硬约束**：Query struct **不加** `#[serde(deny_unknown_fields)]`，也**不要**把未知 query
参数变成 400。现状 `Query<HashMap>` 本来就是"多传就忽略"，收成 struct 后 serde 的默认行为
（忽略未知字段）必须保持——否则 SDK 多带一个 `?foo=1` 就 400。

**验收**
```bash
python3 scripts/contract/extract_registered.py            # check_annotation_fidelity 绿
python3 scripts/contract/gen_derived_routes.py --check     # 绿（query_params 进路由表不换行）
node scripts/quality/check-query-param-coverage.mjs        # SDK 侧 backend-query-unknown 11 → 0
```
（后 2 条 `SearchQuery` 属 SDK 侧 struct 索引问题，改 SDK，不动后端。）

### P2 B-2 分桶收口（分批，每批一次提交）

**Step 1（不动业务代码，先让债务可见）**：把 A 桶 59 条登记进
`scripts/quality/wire-format-waivers.json`，理由写"协议即任意 JSON（account data / filter /
事件 content / UIA / 联邦 PDU / megolm 会话键）"。
效果：`backend-body-unknown` 145 → **≈86**，剩下的才是真债。
加一条比对门禁：waiver 集合必须等于 sidecar 里 A 桶集合（防止豁免表被当垃圾桶）。

**Step 2（改 B 桶 82 条，按文件分批）**，每批一个提交，顺序按调用频次：

1. `e2ee/devices.rs`(10) —— keys/query、keys/claim、sendToDevice、signatures/upload、device_signing/upload
2. `federation/keys.rs`(8) —— user/keys 的 query/claim/upload（含 legacy 前缀）
3. `directory_reporting.rs`(9) —— report_event/room/user、publicRooms、user_directory
4. `handlers/presence.rs`(5) —— set_presence、presence/list
5. `e2ee/keys.rs`(4)、`push.rs`(4)、`media/upload.rs`(4)、其余零散 39 条

每批只做「定义 struct + 改签名」，**deny 策略按面区分**：

- **admin / vendor 面**（`/_synapse/*`、`/_matrix/vendor/*`）：加 `deny_unknown_fields`。
  SDK 与服务端同发版，多传字段打成 400 是可接受且想要的。验收判据就是 N-1 那个数：
  `deny_unknown_fields` 覆盖从 108 上升到 159。
- **`/_matrix/client/*` 与 `/_matrix/federation/*` 面**：**不加**。保持 serde 默认。
  这些端点要对"新 SDK / 新联邦对端 + 旧服务端"兼容。

**验收（每批）**
```bash
cargo clippy --workspace --all-targets --features test-utils --all-features --locked -- -D warnings
cargo nextest run --profile ci --all-features --test integration <related>
node scripts/quality/check-wire-format.mjs        # 违规仍为 0
python3 scripts/contract/gen_derived_routes.py --check
```

### P3 N-2 同名双实现的行为一致性（**需先裁决**）

先回答一个问题：**admin 面 ban/kick/unban 是否应该同步 friend-DM 房间成员状态？**

- **若应该** ⇒ 把 `handlers/room/members.rs` 的 ban/kick/unban 抽成
  `pub(crate) async fn ban_member(ctx, room_id, target, actor, reason)`，
  `admin/room/management.rs` 的 `*_internal` 复用同一函数（副作用自然对齐）。
  一条集成测试：admin ban 后 DM 房间成员状态同步（**现在应当失败**，这是变异自证）。
- **若不应该**（admin 面刻意不做业务副作用）⇒ 在两处各写一条 rustdoc 说明差异，
  并加一条测试**钉住**这个差异，避免将来被"顺手统一"掉。

无论哪个方向，都要把"同名"消掉（`admin_ban_user` / `ban_user`），
顺带让 SDK 侧同名撞车从 42 降到 39 —— 但**不要指望它解决 B-1**，B-1 由 P0 解决。

### P4 不建议做

| 原清单建议 | 为什么不建议 |
| --- | --- |
| B-1「admin 侧改走 `/_synapse/admin/...`」 | 前提错。这些端点**已经**在 `/_synapse/admin/` 下，改了零效果 |
| B-1「在派生表里显式写 `owner` 字段」 | `owner` 已经有了，它叫 `registered_by`，问题是语义不同（codegen 目录 ≠ handler 定义文件）。真正要做的是 P0 的**新增** handler 归属列 |
| B-4「`registered_by` 改成定义文件同族」 | 只有 2 条路由，却会让 SDK `src/invite-link/` 生成目录静默搬家。**P0 落地后 B-4 自动消失**（SDK 不再依赖 `registered_by` 判归属），没必要动 |
| B-2「给这批端点加 `deny_unknown_fields`」 | 对 account_data / 事件 content / filter / UIA 是**规范违规**。只对 admin/vendor 面 A 桶之外的部分成立 |
| B-5 立即改 | 1 条路由、SDK 已 fail-closed、不是缺陷。列入 backlog 即可 |

---

## 5. 执行顺序与依赖

```
P0-1 后端 sidecar        ──┐
                          ├─→ P0-2 SDK 归属  ──→ (B-1 收口, B-4 自动消失)
P1 query structs ─────────┘                    ──→ (B-3 收口)

P2-A 豁免登记（独立，可并行）
P2-B 82 条分批（依赖 P2-A 完成，否则数字看不出进展）
P3  双实现（依赖用户裁决，独立）
```

**判绿只认门禁退出码**，不认报告文字：

```bash
bash scripts/contract/check_route_contract.sh                    # 后端契约链
python3 scripts/contract/gen_derived_routes.py --check           # 路由表新鲜度
node scripts/quality/check-wire-format.mjs                       # 报文契约（EXIT=0 + 违规 0）
node scripts/quality/check-query-param-coverage.mjs              # query 覆盖
node scripts/quality/check-route-set-parity.mjs                  # 路由集合对等
```

---

## 6. 本次复核的边界（未验证项）

- **没有跑后端 Rust 测试套件**。本文件全部结论来自：源码静态阅读 + SDK 门禁实跑 +
  后端 Python 提取器实跑。P1/P2 的改动必须按 CI 那套 clippy/nextest 验，本文不替代。
- **没有验证运行时行为**。N-2 的"admin 面不同步 friend-DM"是读代码得出的，
  未构造端到端用例实测；建议在裁决前先补一条测试确认现状。
- **`backend-body-unknown: 145` 是 SDK 调用点数，`141` 是后端 unique 路由数**，
  两者口径不同（一个调用点可能命中一条路由）。原清单把两者混用，本文分别标注。
- 分桶（A 59 / B 82）是**规则分类**，不是逐条人工裁决；边界上有 ±5 的调整空间
  （主要是 v1/v2 孪生与 `redact` 体的归属）。P2 Step 1 落地时应逐条过一遍。

---

## 附录 A：42 个撞车 handler（全量）

`get_statistics`(3)、`get_status`(3)、`get_stats`(3)、`ban_user`、`kick_user`、`unban_user`、
`get_retention_policy`、`set_retention_policy`、`get_server_version`、`get_public_rooms`、
`timestamp_to_event`、`knock_room`、`get_room_members`、`health_check`、`get_messages`、
`invite_user`、`get_room_hierarchy`、`send_message`、`get_room_version`、`get_room_event`、
`delete_media`、`get_space`、`get_space_rooms`、`delete_space`、`get_jitsi_config`、`get_user`、
`logout`、`count_by_status`、`count_all`、`get_devices`、`get_state`、`create_session`、
`get_session`、`update_session`、`delete_session`、`register`、`get_all_reports`、`get_report`、
`delete_report`、`get_events`、`get_room_account_data`、`set_room_account_data`

（完整 `def:` 文件对见 §2.1 复现脚本输出；后端侧独立解析得到 39 个同名跨文件，
差值来自 SDK 侧把 `ban_user`/`ban_user_by_body` 等细分粒度不同。）

## 附录 B：复现脚本

```bash
# ① 全表有无重复注册（§2.1 实测 A）
cd matrix-js-sdk && node -e "
import('./scripts/quality/lib/admin-contract.mjs').then(m=>{
  const fs=require('node:fs'), path=require('node:path');
  const dir='../synapse-rust/synapse-web/src/routes';
  const all=fs.readdirSync(dir).filter(f=>/^derived_route_table_.*\.inc\.rs$/.test(f))
    .flatMap(f=>m.parseGeneratedRouteTable(fs.readFileSync(path.join(dir,f),'utf8')));
  const c=new Map(); for(const g of all){const k=g.method+' '+g.path; c.set(k,(c.get(k)??0)+1);}
  console.log(all.length, [...c].filter(([,n])=>n>1));
})"

# ② 114 条的成因分布（§2.1 实测 B）
#    用 m.parseRouteTable / m.resolveRoutePath / m.handlerAttributionAllowed 复刻
#    collectRustAdminContract 的归属循环，按 handler 名聚合 —— 见正文代码块

# ③ 后端独立解析 handler 归属（§3 N-3）
cd synapse-rust && python3 - <<'PY'
import importlib.util
spec=importlib.util.spec_from_file_location("ex","scripts/contract/extract_registered.py")
ex=importlib.util.module_from_spec(spec); spec.loader.exec_module(ex)
res=ex.Resolver(ex.load_sources())
ok=un=multi=0
for meth,path in res.all_routes():
    ents=res.handlers.get((meth,path),set())
    if len(ents)!=1: multi+=1; continue
    owner,h=next(iter(ents))
    ok+= res.resolve_handler_def(h,owner) is not None; un+= res.resolve_handler_def(h,owner) is None
print(f"routes={len(res.all_routes())} exact={ok} unresolved={un} multi={multi}")
PY

# ④ 请求体分桶
#    /tmp/body_unknown.txt 由 collectRustAdminContract 的 byRoute 过滤
#    (io.hasJson && !bodyStruct) 得到，再套附录 C 规则
```

## 附录 C：A/B 分桶规则

```js
// A 桶（协议即任意 JSON ⇒ 保持 Value，登记豁免）
/\/account_data\//,            // account data content
/\/filter(\s|$)/,              // filter 定义体
/\/send\//,                    // 消息事件 content
/\/state\//,                   // 状态事件 content
/\/redact\//,                  // redact 体
/\/federation\/v\d\/(send|send_join|send_leave|invite|knock|exchange_third_party_invite|get_missing_events)\//,  // 联邦 PDU/事务
/\/federation\/v\d\/thirdparty\/invite/,
/\/key\/v2\/query/,
/\/account\/(password|deactivate)/,   // UIA auth dict
/\/account\/3pid/,                    // UIA auth dict
/\/register(\s|$)/,                   // UIA + 动态 flow
/\/keys\/backup\/secure/,             // megolm 备份会话
/\/room_keys\//                       // megolm 会话键
// 其余 = B 桶（82 条，可改具名 struct）
```
