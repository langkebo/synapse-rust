# synapse-rust 路由契约（Route Contract）

> 自动生成于 2026-10-09，源 = `synapse-web/src/routes/**` 真实 `.route()` 注册面 + `derived_routes.rs`（含 `derived_route_table.inc.rs`）派生覆盖。
>
> 本文件是后端 HTTP 契约的**事实来源之一**（机器侧权威为 `derived_routes.rs` 生成的 `RouteLedger`，启动时校验、集成测试 PATCH 探测）。人工文档（INDEX.md / API_COVERAGE_REPORT.md）须与之保持一致。
>
> ⚠️ MSC 编号在本仓的**实际语义**以 [`MSC_SEMANTICS.md`](MSC_SEMANTICS.md) 为唯一真相源；若干编号（4155 / 4204 / 3967）被借用承载了与官方提案不同的功能，按编号推断语义前请先查表。

## 总览

- 注册路由条目（绝对 `(method, path)`，经 `.nest()` 前缀解析后去重）：**1030**
- 含路由注册的模块文件：**65**
- `derived_routes.rs` 中的 `registered_by` 标签：**73**
- 非默认 profile 门控的路由（`default` 构建不注册）：**19**（worker **11** · oidc **8**，明细见「运行时 Profile 门控」）
- 已被派生表覆盖的模块：**65**

> **路径为何是绝对的**：本清单由 `extract_registered.py` 从真实 router 构造解析得到，
> 已递归应用 `.nest("/prefix", ..)` 的前缀。
> 因此每一条都是客户端可直接拼接的 serve 路径，而不是子 router 内的相对字面量。

## 生成期自校验（不是自证）

提取器在生成时对两份事实来源做对账，任一项不达标即报错：

| 对照源 | 含义 | 结果 |
|---|---|---|
| `synapse-web/src/routes/derived_routes.rs` 派生表 | 由同一份 `.route()` 注册面机器生成，启动时按 `ProfileFlags` 过滤 | **本清单有而派生表缺 = 0** |
| `tests/unit/fixtures/ledger_export/*.json` | 由真实 Rust 装配（`synapse_ledger_export`）导出、golden 测试守护 | **ledger 有而本清单缺 = 0** |

第二条尤其关键：它保证本清单**不会漏掉任何一个真实对外服务的路由**。
反向差额（本清单多于 ledger）来自源码扫描会看到、而默认 feature 构建不注册的路由
（SAML / CAS / Voice / ExternalServices 等 gated 模块）。

## 运行时 Profile 门控（默认构建不注册的路由）

派生表按 `RouteProfile` **单调**分档：`always`（rank 0）⊂ `worker`（rank 1）⊂ `oidc`（rank 2）；`derived_route_manifest()` 仅在 `rank <= profile_rank(flags)` 时保留该行，`flags` 由运行时配置（`worker.enabled` / `oidc.enabled`）给出。

**下表的路由只有在对应 profile 打开时才注册**：默认构建里它们不在 `RouteLedger` 中，集成测试的 405 探测也不覆盖，照本文档拼接 URL 只会拿到 404 而不是 405；下游客户端的端点生成器若照单全收，会生成一批默认部署必然打不通的调用。名单由 `derived_route_table_{worker,oidc}.inc.rs` **机器反解**（不是手抄），随生成器一起更新；若提取布局变化导致反解失效，生成器直接报错而不产出缺标注的文档。

### 仅 `worker` profile（11 条，需 `worker.enabled = true`）

| Method | Path |
|---|---|
| `POST` | `/_synapse/worker/v1/commands/{command_id}/complete` |
| `POST` | `/_synapse/worker/v1/commands/{command_id}/fail` |
| `GET` | `/_synapse/worker/v1/events` |
| `GET` | `/_synapse/worker/v1/replication/{worker_id}/position` |
| `PUT` | `/_synapse/worker/v1/replication/{worker_id}/{stream_name}` |
| `POST` | `/_synapse/worker/v1/tasks/{task_id}/complete` |
| `POST` | `/_synapse/worker/v1/tasks/{task_id}/fail` |
| `GET` | `/_synapse/worker/v1/workers/{worker_id}/commands` |
| `POST` | `/_synapse/worker/v1/workers/{worker_id}/connect` |
| `POST` | `/_synapse/worker/v1/workers/{worker_id}/disconnect` |
| `POST` | `/_synapse/worker/v1/workers/{worker_id}/heartbeat` |

### 仅 `oidc` profile（8 条，需 `oidc.enabled = true`）

| Method | Path |
|---|---|
| `GET` | `/_matrix/client/v3/login/sso/redirect` |
| `GET` | `/_matrix/client/v3/login/sso/userinfo` |
| `GET` | `/_matrix/client/v3/oidc/authorize` |
| `GET` | `/_matrix/client/v3/oidc/callback` |
| `POST` | `/_matrix/client/v3/oidc/login` |
| `POST` | `/_matrix/client/v3/oidc/logout` |
| `POST` | `/_matrix/client/v3/oidc/token` |
| `GET` | `/_matrix/client/v3/oidc/userinfo` |

**逐模块清单里的两种标注**（都从派生表反解，不是人工维护）：

- 〔仅 `X` profile〕 —— 该路由**只**在 profile `X` 下注册，默认构建里不存在（即上表成员）；
- 〔`always` / `X` 双档注册〕 —— 同一 `(method, path)` 在 `always` 与 `X` 两档都注册，但两档的 `registered_by` 不同（默认档走回退实现，`X` 档走完整实现）。默认档可用，**不**计入上表；这类孪生行正是派生表 1032 行去重为 1030 条的来源。

当前共 **2** 条双档注册：

| Method | Path | 额外档位 |
|---|---|---|
| `GET` | `/.well-known/jwks.json` | `oidc` |
| `GET` | `/.well-known/openid-configuration` | `oidc` |

## 前缀之外 / 未装配的注册

以下注册不属于 `/_matrix/`、`/_synapse/`、`/.well-known/` 任一命名空间。B5-4 之后，此表只剩**有意为之的根级协议与探活端点**：孤儿 router（定义了却从未 merge 进任何路由树）已全部清除，因此这里出现任何新成员都必须先回答「是有意新增的根级端点，还是又一个没人装配的 router」。

| 模块 | Method | Path |
|---|---|---|
| `assembly.rs` | `GET` | `/` |
| `assembly.rs` | `GET` | `/_health` |
| `assembly.rs` | `GET` | `/health` |

## 契约覆盖（派生表一致性）

`RouteLedger` 在启动时校验派生表内 `(method,path)` 不重复，集成测试 `api_route_ledger_tests.rs` 对每个声明做 PATCH 探测（断言 405）。
B2-2 已删除全部 ~120 个手抄 `*_route_manifest()` 助手：路由元数据只剩 `derived_routes.rs` 一个来源，因此「模块是否声明了 manifest」不再是覆盖度指标，取而代之的是「该模块的路由是否进入派生表」。
**已知缺口 / 漂移**：

- 无未装配的孤儿路由。`synapse-web/src/routes/threepid.rs` 曾定义 `create_threepid_router()`（裸 `/requestToken`、`/submitToken`，**从未** merge 进任何路由树且路径非 Matrix 规范形状）—— B5-4 已删除该模块：真实 3PID 端点在 `account_compat.rs`（`/account/3pid/...`，已在 `assembly.rs` 装配），被删代码自引入起即无调用方，纯属死代码。
  **机器证据**：`test_extract_registered.py::check_non_namespace_bucket` 现在把「前缀之外」桶**精确**钉死为 3 条有意根级注册（3 条探活 + 0 条 CAS 根协议端点）。该桶出现任何新成员——无论是死灰复燃的未装配 router 还是新增非 Matrix 根端点——都会让守卫转红并要求显式裁定。
- `space/children_hierarchy.rs`、`space/lifecycle_query.rs`、`space/membership_state.rs`、`space/summary.rs`：路由在派生表中统一归入 `space` 标签（已覆盖）。

## 模块级路由清单（逐模块）

### CAS （12 条）

#### `cas.rs` — 12 条 ✅派生表

- `DELETE` `/_synapse/admin/v1/cas/services/{service_id}`
- `GET` `/_matrix/client/v3/login/sso/redirect/cas`
- `GET` `/_synapse/admin/v1/cas/services`
- `GET` `/_synapse/admin/v1/cas/users/{user_id}/attributes`
- `GET` `/_synapse/cas/login`
- `GET` `/_synapse/cas/logout`
- `GET` `/_synapse/cas/p3/serviceValidate`
- `GET` `/_synapse/cas/proxy`
- `GET` `/_synapse/cas/proxyValidate`
- `GET` `/_synapse/cas/serviceValidate`
- `POST` `/_synapse/admin/v1/cas/services`
- `POST` `/_synapse/admin/v1/cas/users/{user_id}/attributes`

### MSC4108 （4 条）

#### `msc4108_rendezvous.rs` — 4 条 ✅派生表

- `DELETE` `/_matrix/client/unstable/org.matrix.msc4108/rendezvous/{session_id}`
- `GET` `/_matrix/client/unstable/org.matrix.msc4108/rendezvous/{session_id}`
- `POST` `/_matrix/client/unstable/org.matrix.msc4108/rendezvous`
- `PUT` `/_matrix/client/unstable/org.matrix.msc4108/rendezvous/{session_id}`

### OIDC （10 条）

#### `oidc/mod.rs` — 10 条 ✅派生表（8 条仅 `oidc` profile）

- `GET` `/.well-known/jwks.json` 〔`always` / `oidc` 双档注册〕
- `GET` `/.well-known/openid-configuration` 〔`always` / `oidc` 双档注册〕
- `GET` `/_matrix/client/v3/login/sso/redirect` 〔仅 `oidc` profile〕
- `GET` `/_matrix/client/v3/login/sso/userinfo` 〔仅 `oidc` profile〕
- `GET` `/_matrix/client/v3/oidc/authorize` 〔仅 `oidc` profile〕
- `GET` `/_matrix/client/v3/oidc/callback` 〔仅 `oidc` profile〕
- `GET` `/_matrix/client/v3/oidc/userinfo` 〔仅 `oidc` profile〕
- `POST` `/_matrix/client/v3/oidc/login` 〔仅 `oidc` profile〕
- `POST` `/_matrix/client/v3/oidc/logout` 〔仅 `oidc` profile〕
- `POST` `/_matrix/client/v3/oidc/token` 〔仅 `oidc` profile〕

### Rendezvous （6 条）

#### `rendezvous.rs` — 6 条 ✅派生表

- `DELETE` `/_matrix/client/v1/rendezvous/{session_id}`
- `GET` `/_matrix/client/v1/rendezvous/{session_id}`
- `GET` `/_matrix/client/v1/rendezvous/{session_id}/messages`
- `POST` `/_matrix/client/v1/rendezvous`
- `POST` `/_matrix/client/v1/rendezvous/{session_id}/messages`
- `PUT` `/_matrix/client/v1/rendezvous/{session_id}`

### SAML （16 条）

#### `saml.rs` — 16 条 ✅派生表

- `DELETE` `/_synapse/admin/v1/saml/mapping/{name_id}`
- `GET` `/_matrix/client/v3/login/saml/callback`
- `GET` `/_matrix/client/v3/login/sso/redirect/saml`
- `GET` `/_matrix/client/v3/logout/saml`
- `GET` `/_matrix/client/v3/logout/saml/callback`
- `GET` `/_matrix/client/v3/saml/metadata`
- `GET` `/_matrix/client/v3/saml/sp_metadata`
- `GET` `/_synapse/admin/v1/saml/config`
- `GET` `/_synapse/admin/v1/saml/mapping/{name_id}`
- `GET` `/_synapse/admin/v1/saml/mappings`
- `POST` `/_matrix/client/v3/login/saml/callback`
- `POST` `/_matrix/client/v3/login/sso/redirect/saml`
- `POST` `/_synapse/admin/v1/saml/logout`
- `POST` `/_synapse/admin/v1/saml/metadata/refresh`
- `PUT` `/_synapse/admin/v1/saml/config`
- `PUT` `/_synapse/admin/v1/saml/mapping/{name_id}`

### Worker （26 条）

#### `worker.rs` — 26 条 ✅派生表（11 条仅 `worker` profile）

- `DELETE` `/_synapse/worker/v1/workers/{worker_id}`
- `GET` `/_synapse/worker/v1/events` 〔仅 `worker` profile〕
- `GET` `/_synapse/worker/v1/replication/{worker_id}/position` 〔仅 `worker` profile〕
- `GET` `/_synapse/worker/v1/select/{task_type}`
- `GET` `/_synapse/worker/v1/statistics`
- `GET` `/_synapse/worker/v1/statistics/types`
- `GET` `/_synapse/worker/v1/tasks`
- `GET` `/_synapse/worker/v1/topology`
- `GET` `/_synapse/worker/v1/topology/validate`
- `GET` `/_synapse/worker/v1/workers`
- `GET` `/_synapse/worker/v1/workers/type/{worker_type}`
- `GET` `/_synapse/worker/v1/workers/{worker_id}`
- `GET` `/_synapse/worker/v1/workers/{worker_id}/commands` 〔仅 `worker` profile〕
- `POST` `/_synapse/worker/v1/commands/{command_id}/complete` 〔仅 `worker` profile〕
- `POST` `/_synapse/worker/v1/commands/{command_id}/fail` 〔仅 `worker` profile〕
- `POST` `/_synapse/worker/v1/register`
- `POST` `/_synapse/worker/v1/tasks`
- `POST` `/_synapse/worker/v1/tasks/claim/{worker_id}`
- `POST` `/_synapse/worker/v1/tasks/{task_id}/claim/{worker_id}`
- `POST` `/_synapse/worker/v1/tasks/{task_id}/complete` 〔仅 `worker` profile〕
- `POST` `/_synapse/worker/v1/tasks/{task_id}/fail` 〔仅 `worker` profile〕
- `POST` `/_synapse/worker/v1/workers/{worker_id}/commands`
- `POST` `/_synapse/worker/v1/workers/{worker_id}/connect` 〔仅 `worker` profile〕
- `POST` `/_synapse/worker/v1/workers/{worker_id}/disconnect` 〔仅 `worker` profile〕
- `POST` `/_synapse/worker/v1/workers/{worker_id}/heartbeat` 〔仅 `worker` profile〕
- `PUT` `/_synapse/worker/v1/replication/{worker_id}/{stream_name}` 〔仅 `worker` profile〕

### 临时事件 （1 条）

#### `ephemeral.rs` — 1 条 ✅派生表

- `GET` `/_matrix/client/v3/rooms/{room_id}/ephemeral`

### 事件举报 （18 条）

#### `event_report.rs` — 18 条 ✅派生表

- `DELETE` `/_synapse/admin/v1/event_reports/{id}`
- `GET` `/_synapse/admin/v1/event_reports`
- `GET` `/_synapse/admin/v1/event_reports/count`
- `GET` `/_synapse/admin/v1/event_reports/event/{event_id}`
- `GET` `/_synapse/admin/v1/event_reports/rate_limit/{user_id}`
- `GET` `/_synapse/admin/v1/event_reports/reporter/{reporter_user_id}`
- `GET` `/_synapse/admin/v1/event_reports/room/{room_id}`
- `GET` `/_synapse/admin/v1/event_reports/stats`
- `GET` `/_synapse/admin/v1/event_reports/status/{status}`
- `GET` `/_synapse/admin/v1/event_reports/status/{status}/count`
- `GET` `/_synapse/admin/v1/event_reports/{id}`
- `POST` `/_synapse/admin/v1/event_reports`
- `POST` `/_synapse/admin/v1/event_reports/rate_limit/{user_id}/block`
- `POST` `/_synapse/admin/v1/event_reports/rate_limit/{user_id}/unblock`
- `POST` `/_synapse/admin/v1/event_reports/{id}/dismiss`
- `POST` `/_synapse/admin/v1/event_reports/{id}/escalate`
- `POST` `/_synapse/admin/v1/event_reports/{id}/resolve`
- `PUT` `/_synapse/admin/v1/event_reports/{id}`

### 关联 (Relations) （9 条）

#### `relations.rs` — 9 条 ✅派生表

- `GET` `/_matrix/client/v1/rooms/{room_id}/aggregations/{event_id}/{rel_type}`
- `GET` `/_matrix/client/v1/rooms/{room_id}/relations/{event_id}`
- `GET` `/_matrix/client/v1/rooms/{room_id}/relations/{event_id}/{rel_type}`
- `GET` `/_matrix/client/v1/rooms/{room_id}/relations/{event_id}/{rel_type}/{event_type}`
- `GET` `/_matrix/client/v3/rooms/{room_id}/aggregations/{event_id}/{rel_type}`
- `GET` `/_matrix/client/v3/rooms/{room_id}/relations/{event_id}`
- `GET` `/_matrix/client/v3/rooms/{room_id}/relations/{event_id}/{rel_type}`
- `GET` `/_matrix/client/v3/rooms/{room_id}/relations/{event_id}/{rel_type}/{event_type}`
- `PUT` `/_matrix/vendor/v1/rooms/{room_id}/relations/{event_id}/{rel_type}/{txn_id}`

### 其他 (Other) （23 条）

#### `handlers/thread.rs` — 23 条 ✅派生表

- `DELETE` `/_matrix/vendor/v1/rooms/{room_id}/threads/{thread_id}`
- `GET` `/_matrix/client/unstable/org.matrix.msc4155/rooms/{room_id}/threads`
- `GET` `/_matrix/client/unstable/org.matrix.msc4156/threads/subscribed`
- `GET` `/_matrix/client/v1/rooms/{room_id}/threads`
- `GET` `/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}`
- `GET` `/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/replies`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/threads/search`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/threads/unread`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/threads/{thread_id}/stats`
- `GET` `/_matrix/vendor/v1/threads`
- `GET` `/_matrix/vendor/v1/threads/subscribed`
- `GET` `/_matrix/vendor/v1/threads/unread`
- `GET` `/_matrix/vendor/v1/user/{user_id}/rooms/{room_id}/threads`
- `POST` `/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/replies`
- `POST` `/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/subscribe`
- `POST` `/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/unsubscribe`
- `POST` `/_matrix/vendor/v1/rooms/{room_id}/replies/{event_id}/redact`
- `POST` `/_matrix/vendor/v1/rooms/{room_id}/threads`
- `POST` `/_matrix/vendor/v1/rooms/{room_id}/threads/{thread_id}/freeze`
- `POST` `/_matrix/vendor/v1/rooms/{room_id}/threads/{thread_id}/mute`
- `POST` `/_matrix/vendor/v1/rooms/{room_id}/threads/{thread_id}/read`
- `POST` `/_matrix/vendor/v1/rooms/{room_id}/threads/{thread_id}/unfreeze`
- `POST` `/_matrix/vendor/v1/threads`

### 反应 (Reactions) （1 条）

#### `reactions.rs` — 1 条 ✅派生表

- `PUT` `/_matrix/client/v3/rooms/{room_id}/send/m.reaction/{txn_id}`

### 同步 (Sync) （3 条）

#### `sync.rs` — 3 条 ✅派生表

- `GET` `/_matrix/client/v3/events`
- `GET` `/_matrix/client/v3/joined_rooms`
- `GET` `/_matrix/client/v3/sync`

### 后台更新 （19 条）

#### `background_update.rs` — 19 条 ✅派生表

- `DELETE` `/_synapse/admin/v1/background_updates/{job_name}`
- `GET` `/_synapse/admin/v1/background_updates`
- `GET` `/_synapse/admin/v1/background_updates/count`
- `GET` `/_synapse/admin/v1/background_updates/next`
- `GET` `/_synapse/admin/v1/background_updates/pending`
- `GET` `/_synapse/admin/v1/background_updates/running`
- `GET` `/_synapse/admin/v1/background_updates/stats`
- `GET` `/_synapse/admin/v1/background_updates/status`
- `GET` `/_synapse/admin/v1/background_updates/status/{status}/count`
- `GET` `/_synapse/admin/v1/background_updates/{job_name}`
- `GET` `/_synapse/admin/v1/background_updates/{job_name}/history`
- `POST` `/_synapse/admin/v1/background_updates`
- `POST` `/_synapse/admin/v1/background_updates/cleanup_locks`
- `POST` `/_synapse/admin/v1/background_updates/retry_failed`
- `POST` `/_synapse/admin/v1/background_updates/{job_name}/cancel`
- `POST` `/_synapse/admin/v1/background_updates/{job_name}/complete`
- `POST` `/_synapse/admin/v1/background_updates/{job_name}/fail`
- `POST` `/_synapse/admin/v1/background_updates/{job_name}/progress`
- `POST` `/_synapse/admin/v1/background_updates/{job_name}/start`

### 在线状态 (Presence) （9 条）

#### `presence.rs` — 9 条 ✅派生表

- `GET` `/_matrix/client/v1/presence/{user_id}/status`
- `GET` `/_matrix/client/v3/presence/list`
- `GET` `/_matrix/client/v3/presence/list/{user_id}`
- `GET` `/_matrix/client/v3/presence/{user_id}/status`
- `POST` `/_matrix/client/v1/presence/{user_id}/status`
- `POST` `/_matrix/client/v3/presence/list`
- `POST` `/_matrix/client/v3/presence/{user_id}/status`
- `PUT` `/_matrix/client/v1/presence/{user_id}/status`
- `PUT` `/_matrix/client/v3/presence/{user_id}/status`

### 外部服务 （12 条）

#### `external_service.rs` — 12 条 ✅派生表

- `DELETE` `/_matrix/vendor/v1/external_services/{service_id}`
- `DELETE` `/_synapse/admin/v1/external_services/{as_id}`
- `GET` `/_matrix/vendor/v1/external_services/health`
- `GET` `/_synapse/admin/v1/external_services`
- `GET` `/_synapse/admin/v1/external_services/health`
- `GET` `/_synapse/admin/v1/external_services/{as_id}/health`
- `POST` `/_synapse/admin/v1/external_services`
- `POST` `/_synapse/admin/v1/external_services/{as_id}/health/check`
- `POST` `/_synapse/external/trendradar/{service_id}/webhook`
- `POST` `/_synapse/external/webhook/{service_id}`
- `PUT` `/_matrix/vendor/v1/external_services/{service_id}`
- `PUT` `/_synapse/admin/v1/external_services/{as_id}`

### 好友 (Friends) （29 条）

#### `friend_room.rs` — 29 条 ✅派生表

- `DELETE` `/_matrix/vendor/v1/friends/groups/{group_id}`
- `DELETE` `/_matrix/vendor/v1/friends/groups/{group_id}/remove/{user_id}`
- `DELETE` `/_matrix/vendor/v1/friends/{user_id}`
- `GET` `/_matrix/vendor/v1/friends`
- `GET` `/_matrix/vendor/v1/friends/check/{user_id}`
- `GET` `/_matrix/vendor/v1/friends/dm/{user_id}`
- `GET` `/_matrix/vendor/v1/friends/groups`
- `GET` `/_matrix/vendor/v1/friends/groups/{group_id}/friends`
- `GET` `/_matrix/vendor/v1/friends/request/received`
- `GET` `/_matrix/vendor/v1/friends/requests/incoming`
- `GET` `/_matrix/vendor/v1/friends/requests/outgoing`
- `GET` `/_matrix/vendor/v1/friends/search`
- `GET` `/_matrix/vendor/v1/friends/suggestions`
- `GET` `/_matrix/vendor/v1/friends/{user_id}/groups`
- `GET` `/_matrix/vendor/v1/friends/{user_id}/info`
- `GET` `/_matrix/vendor/v1/friends/{user_id}/status`
- `POST` `/_matrix/vendor/v1/friends`
- `POST` `/_matrix/vendor/v1/friends/dm/{user_id}`
- `POST` `/_matrix/vendor/v1/friends/groups`
- `POST` `/_matrix/vendor/v1/friends/groups/{group_id}/add/{user_id}`
- `POST` `/_matrix/vendor/v1/friends/request`
- `POST` `/_matrix/vendor/v1/friends/request/{user_id}/accept`
- `POST` `/_matrix/vendor/v1/friends/request/{user_id}/cancel`
- `POST` `/_matrix/vendor/v1/friends/request/{user_id}/reject`
- `POST` `/_matrix/vendor/v1/friends/search`
- `PUT` `/_matrix/vendor/v1/friends/groups/{group_id}/name`
- `PUT` `/_matrix/vendor/v1/friends/{user_id}/displayname`
- `PUT` `/_matrix/vendor/v1/friends/{user_id}/note`
- `PUT` `/_matrix/vendor/v1/friends/{user_id}/status`

### 媒体 (Media) （60 条）

#### `media/mod.rs` — 38 条 ✅派生表

- `GET` `/_matrix/client/v1/media/download/{server_name}/{media_id}`
- `GET` `/_matrix/client/v1/media/download/{server_name}/{media_id}/{filename}`
- `GET` `/_matrix/client/v1/media/preview_url`
- `GET` `/_matrix/client/v1/media/thumbnail/{server_name}/{media_id}`
- `GET` `/_matrix/client/v3/upload/provider`
- `GET` `/_matrix/media/r0/config`
- `GET` `/_matrix/media/r0/download/{server_name}/{media_id}`
- `GET` `/_matrix/media/r0/download/{server_name}/{media_id}/{filename}`
- `GET` `/_matrix/media/r0/preview_url`
- `GET` `/_matrix/media/r1/download/{server_name}/{media_id}`
- `GET` `/_matrix/media/r1/download/{server_name}/{media_id}/{filename}`
- `GET` `/_matrix/media/v1/config`
- `GET` `/_matrix/media/v1/download/{server_name}/{media_id}`
- `GET` `/_matrix/media/v1/download/{server_name}/{media_id}/{filename}`
- `GET` `/_matrix/media/v1/preview_url`
- `GET` `/_matrix/media/v1/quota/alerts`
- `GET` `/_matrix/media/v1/quota/check`
- `GET` `/_matrix/media/v1/quota/stats`
- `GET` `/_matrix/media/v1/upload/chunk/progress`
- `GET` `/_matrix/media/v3/config`
- `GET` `/_matrix/media/v3/download/{server_name}/{media_id}`
- `GET` `/_matrix/media/v3/download/{server_name}/{media_id}/{filename}`
- `GET` `/_matrix/media/v3/download_signed/{server_name}/{media_id}`
- `GET` `/_matrix/media/v3/download_signed/{server_name}/{media_id}/{filename}`
- `GET` `/_matrix/media/v3/preview_url`
- `GET` `/_matrix/media/v3/thumbnail/{server_name}/{media_id}`
- `POST` `/_matrix/client/v3/upload/token`
- `POST` `/_matrix/media/r0/delete/{server_name}/{media_id}`
- `POST` `/_matrix/media/r0/upload`
- `POST` `/_matrix/media/v1/delete/{server_name}/{media_id}`
- `POST` `/_matrix/media/v1/upload`
- `POST` `/_matrix/media/v1/upload/chunk`
- `POST` `/_matrix/media/v1/upload/chunk/cancel`
- `POST` `/_matrix/media/v1/upload/chunk/complete`
- `POST` `/_matrix/media/v1/upload/chunk/start`
- `POST` `/_matrix/media/v3/delete/{server_name}/{media_id}`
- `POST` `/_matrix/media/v3/upload`
- `PUT` `/_matrix/media/v3/upload/{server_name}/{media_id}`

#### `admin/media.rs` — 22 条 ✅派生表

- `DELETE` `/_synapse/admin/v1/media/{media_id}`
- `DELETE` `/_synapse/admin/v1/media/{server_name}/{media_id}`
- `DELETE` `/_synapse/admin/v1/rooms/{room_id}/media/{media_id}`
- `DELETE` `/_synapse/admin/v1/users/{user_id}/media`
- `GET` `/_synapse/admin/v1/media`
- `GET` `/_synapse/admin/v1/media/quarantine_changes`
- `GET` `/_synapse/admin/v1/media/quota`
- `GET` `/_synapse/admin/v1/media/{media_id}`
- `GET` `/_synapse/admin/v1/media/{server_name}/{media_id}`
- `GET` `/_synapse/admin/v1/quarantine_media/{media_id}/changes`
- `GET` `/_synapse/admin/v1/rooms/{room_id}/media`
- `GET` `/_synapse/admin/v1/users/{user_id}/media`
- `POST` `/_synapse/admin/v1/media/delete`
- `POST` `/_synapse/admin/v1/media/protect/{media_id}`
- `POST` `/_synapse/admin/v1/media/protect/{server_name}/{media_id}`
- `POST` `/_synapse/admin/v1/media/quarantine/{server_name}/{media_id}`
- `POST` `/_synapse/admin/v1/media/unprotect/{media_id}`
- `POST` `/_synapse/admin/v1/media/unquarantine/{server_name}/{media_id}`
- `POST` `/_synapse/admin/v1/purge_media_cache`
- `POST` `/_synapse/admin/v1/rooms/{room_id}/media/quarantine`
- `POST` `/_synapse/admin/v1/rooms/{room_id}/media/unquarantine`
- `POST` `/_synapse/admin/v1/user/{user_id}/media/quarantine`

### 审核 (Moderation) （6 条）

#### `moderation.rs` — 6 条 ✅派生表

- `GET` `/_matrix/vendor/v1/rooms/{room_id}/report/{event_id}/scanner_info`
- `POST` `/_matrix/client/v1/rooms/{room_id}/report/{event_id}`
- `POST` `/_matrix/client/v3/rooms/{room_id}/report`
- `POST` `/_matrix/client/v3/rooms/{room_id}/report/{event_id}`
- `POST` `/_matrix/client/v3/users/{user_id}/report`
- `PUT` `/_matrix/vendor/v1/rooms/{room_id}/report/{event_id}/score`

### 密钥备份 (Key Backup) （66 条）

#### `key_backup.rs` — 66 条 ✅派生表

- `DELETE` `/_matrix/client/v1/room_keys/keys`
- `DELETE` `/_matrix/client/v1/room_keys/keys/{room_id}`
- `DELETE` `/_matrix/client/v1/room_keys/keys/{room_id}/{session_id}`
- `DELETE` `/_matrix/client/v1/room_keys/version/{version}`
- `DELETE` `/_matrix/client/v1/room_keys/{version}/keys`
- `DELETE` `/_matrix/client/v1/room_keys/{version}/keys/{room_id}`
- `DELETE` `/_matrix/client/v1/room_keys/{version}/keys/{room_id}/{session_id}`
- `DELETE` `/_matrix/client/v3/room_keys/keys`
- `DELETE` `/_matrix/client/v3/room_keys/keys/{room_id}`
- `DELETE` `/_matrix/client/v3/room_keys/keys/{room_id}/{session_id}`
- `DELETE` `/_matrix/client/v3/room_keys/version/{version}`
- `DELETE` `/_matrix/client/v3/room_keys/{version}/keys`
- `DELETE` `/_matrix/client/v3/room_keys/{version}/keys/{room_id}`
- `DELETE` `/_matrix/client/v3/room_keys/{version}/keys/{room_id}/{session_id}`
- `GET` `/_matrix/client/v1/room_keys/export`
- `GET` `/_matrix/client/v1/room_keys/export/{version}`
- `GET` `/_matrix/client/v1/room_keys/keys`
- `GET` `/_matrix/client/v1/room_keys/keys/{room_id}`
- `GET` `/_matrix/client/v1/room_keys/keys/{room_id}/{session_id}`
- `GET` `/_matrix/client/v1/room_keys/recover/{version}/{room_id}`
- `GET` `/_matrix/client/v1/room_keys/recover/{version}/{room_id}/{session_id}`
- `GET` `/_matrix/client/v1/room_keys/recovery/{version}/progress`
- `GET` `/_matrix/client/v1/room_keys/verify/{version}`
- `GET` `/_matrix/client/v1/room_keys/version`
- `GET` `/_matrix/client/v1/room_keys/version/{version}`
- `GET` `/_matrix/client/v1/room_keys/{version}/keys`
- `GET` `/_matrix/client/v1/room_keys/{version}/keys/{room_id}`
- `GET` `/_matrix/client/v1/room_keys/{version}/keys/{room_id}/{session_id}`
- `GET` `/_matrix/client/v3/room_keys/export`
- `GET` `/_matrix/client/v3/room_keys/export/{version}`
- `GET` `/_matrix/client/v3/room_keys/keys`
- `GET` `/_matrix/client/v3/room_keys/keys/{room_id}`
- `GET` `/_matrix/client/v3/room_keys/keys/{room_id}/{session_id}`
- `GET` `/_matrix/client/v3/room_keys/recover/{version}/{room_id}`
- `GET` `/_matrix/client/v3/room_keys/recover/{version}/{room_id}/{session_id}`
- `GET` `/_matrix/client/v3/room_keys/recovery/{version}/progress`
- `GET` `/_matrix/client/v3/room_keys/verify/{version}`
- `GET` `/_matrix/client/v3/room_keys/version`
- `GET` `/_matrix/client/v3/room_keys/version/{version}`
- `GET` `/_matrix/client/v3/room_keys/{version}/keys`
- `GET` `/_matrix/client/v3/room_keys/{version}/keys/{room_id}`
- `GET` `/_matrix/client/v3/room_keys/{version}/keys/{room_id}/{session_id}`
- `POST` `/_matrix/client/v1/room_keys/batch_recover`
- `POST` `/_matrix/client/v1/room_keys/import`
- `POST` `/_matrix/client/v1/room_keys/import/{version}`
- `POST` `/_matrix/client/v1/room_keys/recover`
- `POST` `/_matrix/client/v1/room_keys/version`
- `POST` `/_matrix/client/v3/room_keys/batch_recover`
- `POST` `/_matrix/client/v3/room_keys/import`
- `POST` `/_matrix/client/v3/room_keys/import/{version}`
- `POST` `/_matrix/client/v3/room_keys/recover`
- `POST` `/_matrix/client/v3/room_keys/version`
- `PUT` `/_matrix/client/v1/room_keys/keys`
- `PUT` `/_matrix/client/v1/room_keys/keys/{room_id}`
- `PUT` `/_matrix/client/v1/room_keys/keys/{room_id}/{session_id}`
- `PUT` `/_matrix/client/v1/room_keys/version/{version}`
- `PUT` `/_matrix/client/v1/room_keys/{version}/keys`
- `PUT` `/_matrix/client/v1/room_keys/{version}/keys/{room_id}`
- `PUT` `/_matrix/client/v1/room_keys/{version}/keys/{room_id}/{session_id}`
- `PUT` `/_matrix/client/v3/room_keys/keys`
- `PUT` `/_matrix/client/v3/room_keys/keys/{room_id}`
- `PUT` `/_matrix/client/v3/room_keys/keys/{room_id}/{session_id}`
- `PUT` `/_matrix/client/v3/room_keys/version/{version}`
- `PUT` `/_matrix/client/v3/room_keys/{version}/keys`
- `PUT` `/_matrix/client/v3/room_keys/{version}/keys/{room_id}`
- `PUT` `/_matrix/client/v3/room_keys/{version}/keys/{room_id}/{session_id}`

### 密钥轮转 （9 条）

#### `key_rotation.rs` — 9 条 ✅派生表

- `GET` `/_matrix/vendor/v1/keys/rotation/check`
- `GET` `/_matrix/vendor/v1/keys/rotation/history/{device_id}`
- `GET` `/_matrix/vendor/v1/keys/rotation/status`
- `POST` `/_matrix/vendor/v1/keys/rotation/check`
- `POST` `/_matrix/vendor/v1/keys/rotation/config`
- `POST` `/_matrix/vendor/v1/keys/rotation/revoke`
- `POST` `/_matrix/vendor/v1/keys/rotation/rotate`
- `POST` `/_matrix/vendor/v1/keys/rotation/status`
- `PUT` `/_matrix/vendor/v1/keys/rotation/config`

### 小组件 (Widget) （16 条）

#### `widget.rs` — 16 条 ✅派生表

- `DELETE` `/_matrix/vendor/v1/widgets/sessions/{session_id}`
- `DELETE` `/_matrix/vendor/v1/widgets/{widget_id}`
- `DELETE` `/_matrix/vendor/v1/widgets/{widget_id}/permissions/{user_id}`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/widgets`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/widgets/jitsi/config`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/widgets/{widget_id}/capabilities`
- `GET` `/_matrix/vendor/v1/widgets/sessions/{session_id}`
- `GET` `/_matrix/vendor/v1/widgets/{widget_id}`
- `GET` `/_matrix/vendor/v1/widgets/{widget_id}/config`
- `GET` `/_matrix/vendor/v1/widgets/{widget_id}/permissions`
- `GET` `/_matrix/vendor/v1/widgets/{widget_id}/sessions`
- `POST` `/_matrix/vendor/v1/widgets`
- `POST` `/_matrix/vendor/v1/widgets/{widget_id}/permissions`
- `POST` `/_matrix/vendor/v1/widgets/{widget_id}/sessions`
- `PUT` `/_matrix/vendor/v1/rooms/{room_id}/widgets/{widget_id}/capabilities`
- `PUT` `/_matrix/vendor/v1/widgets/{widget_id}`

### 应用服务 (AppService) （39 条）

#### `app_service.rs` — 39 条 ✅派生表

- `DELETE` `/_matrix/app/v1/proxy/{as_id}/{*path}`
- `DELETE` `/_matrix/client/v1/proxy/{as_id}/{*path}`
- `DELETE` `/_synapse/admin/v1/appservices/{as_id}`
- `GET` `/_matrix/app/v1/proxy/{as_id}/{*path}`
- `GET` `/_matrix/app/v1/rooms/{alias}`
- `GET` `/_matrix/app/v1/users/{user_id}`
- `GET` `/_matrix/app/v1/{as_id}`
- `GET` `/_matrix/client/v1/proxy/{as_id}/{*path}`
- `GET` `/_matrix/client/v1/user/{user_id}/appservice`
- `GET` `/_matrix/client/v3/appservice/alias`
- `GET` `/_matrix/client/v3/appservice/user`
- `GET` `/_synapse/admin/v1/appservices`
- `GET` `/_synapse/admin/v1/appservices/query/alias`
- `GET` `/_synapse/admin/v1/appservices/query/user`
- `GET` `/_synapse/admin/v1/appservices/statistics`
- `GET` `/_synapse/admin/v1/appservices/{as_id}`
- `GET` `/_synapse/admin/v1/appservices/{as_id}/events`
- `GET` `/_synapse/admin/v1/appservices/{as_id}/namespaces`
- `GET` `/_synapse/admin/v1/appservices/{as_id}/state`
- `GET` `/_synapse/admin/v1/appservices/{as_id}/state/{state_key}`
- `GET` `/_synapse/admin/v1/appservices/{as_id}/users`
- `HEAD` `/_matrix/app/v1/proxy/{as_id}/{*path}`
- `HEAD` `/_matrix/client/v1/proxy/{as_id}/{*path}`
- `OPTIONS` `/_matrix/app/v1/proxy/{as_id}/{*path}`
- `OPTIONS` `/_matrix/client/v1/proxy/{as_id}/{*path}`
- `PATCH` `/_matrix/app/v1/proxy/{as_id}/{*path}`
- `PATCH` `/_matrix/client/v1/proxy/{as_id}/{*path}`
- `POST` `/_matrix/app/v1/ping`
- `POST` `/_matrix/app/v1/proxy/{as_id}/{*path}`
- `POST` `/_matrix/client/v1/proxy/{as_id}/{*path}`
- `POST` `/_synapse/admin/v1/appservices`
- `POST` `/_synapse/admin/v1/appservices/{as_id}/events`
- `POST` `/_synapse/admin/v1/appservices/{as_id}/ping`
- `POST` `/_synapse/admin/v1/appservices/{as_id}/state`
- `POST` `/_synapse/admin/v1/appservices/{as_id}/users`
- `PUT` `/_matrix/app/v1/proxy/{as_id}/{*path}`
- `PUT` `/_matrix/app/v1/transactions/{as_id}/{txn_id}`
- `PUT` `/_matrix/client/v1/proxy/{as_id}/{*path}`
- `PUT` `/_synapse/admin/v1/appservices/{as_id}`

### 延迟事件 （2 条）

#### `delayed_events.rs` — 2 条 ✅派生表

- `GET` `/_matrix/client/unstable/org.matrix.msc4140/delayed_events/{delay_id}`
- `POST` `/_matrix/client/unstable/org.matrix.msc4140/delayed_events/{delay_id}`

### 房间 (Room) （115 条）

#### `room.rs` — 94 条 ✅派生表

- `DELETE` `/_matrix/client/unstable/org.matrix.msc4354/rooms/{room_id}/sticky_events/{event_type}`
- `DELETE` `/_matrix/vendor/v1/rooms/{room_id}/pinned_events/{event_id}`
- `GET` `/_matrix/client/unstable/org.matrix.msc4354/rooms/{room_id}/sticky_events`
- `GET` `/_matrix/client/unstable/uk.half-shot.msc2666/user/mutual_rooms`
- `GET` `/_matrix/client/v1/rooms/{room_id}/state/m.room.power_levels/`
- `GET` `/_matrix/client/v3/rooms/{room_id}`
- `GET` `/_matrix/client/v3/rooms/{room_id}/account_data/{type}`
- `GET` `/_matrix/client/v3/rooms/{room_id}/aliases`
- `GET` `/_matrix/client/v3/rooms/{room_id}/event/{event_id}`
- `GET` `/_matrix/client/v3/rooms/{room_id}/initialSync`
- `GET` `/_matrix/client/v3/rooms/{room_id}/joined_members`
- `GET` `/_matrix/client/v3/rooms/{room_id}/members`
- `GET` `/_matrix/client/v3/rooms/{room_id}/messages`
- `GET` `/_matrix/client/v3/rooms/{room_id}/state`
- `GET` `/_matrix/client/v3/rooms/{room_id}/state/m.room.power_levels/`
- `GET` `/_matrix/client/v3/rooms/{room_id}/state/{event_type}`
- `GET` `/_matrix/client/v3/rooms/{room_id}/state/{event_type}/`
- `GET` `/_matrix/client/v3/rooms/{room_id}/state/{event_type}/{state_key}`
- `GET` `/_matrix/client/v3/rooms/{room_id}/threads/{thread_id}`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/anti_screenshot`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/capabilities`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/device/{device_id}`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/encrypted_events`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/event/{event_id}/url`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/event_perspective`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/external_ids`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/fragments/{user_id}`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/invites`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/keys`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/keys/count`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/keys/version`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/keys/{event_id}`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/members/recent`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/membership/{user_id}`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/message_queue`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/metadata`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/notifications`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/permissions`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/pinned_events`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/receipts/{receipt_type}/{event_id}`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/reduced_events`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/rendered/`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/resolve`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/retention`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/service_types`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/spaces`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/sync`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/thread/{event_id}`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/timeline`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/turn_server`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/unread_count`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/vault_data`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/version`
- `GET` `/_matrix/vendor/v1/user/mutual_rooms`
- `GET` `/_matrix/vendor/v1/user/{user_id}/rooms`
- `POST` `/_matrix/client/unstable/org.matrix.msc4354/rooms/{room_id}/sticky_events`
- `POST` `/_matrix/client/v3/createRoom`
- `POST` `/_matrix/client/v3/join/{room_id_or_alias}`
- `POST` `/_matrix/client/v3/knock/{room_id_or_alias}`
- `POST` `/_matrix/client/v3/rooms/{room_id}/ban`
- `POST` `/_matrix/client/v3/rooms/{room_id}/forget`
- `POST` `/_matrix/client/v3/rooms/{room_id}/invite`
- `POST` `/_matrix/client/v3/rooms/{room_id}/join`
- `POST` `/_matrix/client/v3/rooms/{room_id}/kick`
- `POST` `/_matrix/client/v3/rooms/{room_id}/leave`
- `POST` `/_matrix/client/v3/rooms/{room_id}/read_markers`
- `POST` `/_matrix/client/v3/rooms/{room_id}/receipt/{receipt_type}/{event_id}`
- `POST` `/_matrix/client/v3/rooms/{room_id}/redact/{event_id}/{txn_id}`
- `POST` `/_matrix/client/v3/rooms/{room_id}/send/{event_type}/{txn_id}`
- `POST` `/_matrix/client/v3/rooms/{room_id}/state/{event_type}`
- `POST` `/_matrix/client/v3/rooms/{room_id}/unban`
- `POST` `/_matrix/client/v3/rooms/{room_id}/upgrade`
- `POST` `/_matrix/vendor/v1/rooms/create_private`
- `POST` `/_matrix/vendor/v1/rooms/{room_id}/convert/{event_id}`
- `POST` `/_matrix/vendor/v1/rooms/{room_id}/get_membership_events`
- `POST` `/_matrix/vendor/v1/rooms/{room_id}/keys/claim`
- `POST` `/_matrix/vendor/v1/rooms/{room_id}/pinned_events`
- `POST` `/_matrix/vendor/v1/rooms/{room_id}/search`
- `POST` `/_matrix/vendor/v1/rooms/{room_id}/translate/{event_id}`
- `POST` `/_matrix/vendor/v1/rooms/{room_id}/verify/{event_id}`
- `POST` `/_matrix/vendor/v1/translate`
- `PUT` `/_matrix/client/v1/rooms/{room_id}/state/m.room.power_levels/`
- `PUT` `/_matrix/client/v3/rooms/{room_id}/account_data/{type}`
- `PUT` `/_matrix/client/v3/rooms/{room_id}/read_markers`
- `PUT` `/_matrix/client/v3/rooms/{room_id}/redact/{event_id}/{txn_id}`
- `PUT` `/_matrix/client/v3/rooms/{room_id}/send/{event_type}/{txn_id}`
- `PUT` `/_matrix/client/v3/rooms/{room_id}/state/m.room.power_levels/`
- `PUT` `/_matrix/client/v3/rooms/{room_id}/state/{event_type}`
- `PUT` `/_matrix/client/v3/rooms/{room_id}/state/{event_type}/`
- `PUT` `/_matrix/client/v3/rooms/{room_id}/state/{event_type}/{state_key}`
- `PUT` `/_matrix/vendor/v1/rooms/{room_id}/anti_screenshot`
- `PUT` `/_matrix/vendor/v1/rooms/{room_id}/room_keys/keys`
- `PUT` `/_matrix/vendor/v1/rooms/{room_id}/sign/{event_id}`
- `PUT` `/_matrix/vendor/v1/rooms/{room_id}/vault_data`

#### `room_summary.rs` — 21 条 ✅派生表

- `DELETE` `/_matrix/vendor/v1/rooms/{room_id}/summary`
- `DELETE` `/_matrix/vendor/v1/rooms/{room_id}/summary/members/{user_id}`
- `GET` `/_matrix/client/v1/rooms/{room_id}/summary`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/summary`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/summary/members`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/summary/state`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/summary/state/{event_type}/{state_key}`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/summary/stats`
- `GET` `/_synapse/room_summary/v1/summaries`
- `POST` `/_matrix/vendor/v1/rooms/{room_id}/summary`
- `POST` `/_matrix/vendor/v1/rooms/{room_id}/summary/heroes/recalculate`
- `POST` `/_matrix/vendor/v1/rooms/{room_id}/summary/members`
- `POST` `/_matrix/vendor/v1/rooms/{room_id}/summary/stats/recalculate`
- `POST` `/_matrix/vendor/v1/rooms/{room_id}/summary/sync`
- `POST` `/_matrix/vendor/v1/rooms/{room_id}/summary/unread/clear`
- `POST` `/_synapse/room_summary/v1/summaries`
- `POST` `/_synapse/room_summary/v1/summaries/batch`
- `POST` `/_synapse/room_summary/v1/updates/process`
- `PUT` `/_matrix/vendor/v1/rooms/{room_id}/summary`
- `PUT` `/_matrix/vendor/v1/rooms/{room_id}/summary/members/{user_id}`
- `PUT` `/_matrix/vendor/v1/rooms/{room_id}/summary/state/{event_type}/{state_key}`

### 推送 (Push) （25 条）

#### `push.rs` — 17 条 ✅派生表

- `DELETE` `/_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}`
- `GET` `/_matrix/client/v3/notifications`
- `GET` `/_matrix/client/v3/pushers`
- `GET` `/_matrix/client/v3/pushers/`
- `GET` `/_matrix/client/v3/pushrules`
- `GET` `/_matrix/client/v3/pushrules/{scope}`
- `GET` `/_matrix/client/v3/pushrules/{scope}/{kind}`
- `GET` `/_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}`
- `GET` `/_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}/enabled`
- `POST` `/_matrix/client/v3/notifications/{notification_id}/ack`
- `POST` `/_matrix/client/v3/pushers`
- `POST` `/_matrix/client/v3/pushers/`
- `POST` `/_matrix/client/v3/pushers/set`
- `POST` `/_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}`
- `PUT` `/_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}`
- `PUT` `/_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}/actions`
- `PUT` `/_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}/enabled`

#### `push_notification.rs` — 8 条 ✅派生表

- `DELETE` `/_matrix/vendor/v1/push/devices/{device_id}`
- `GET` `/_matrix/vendor/v1/push/devices`
- `GET` `/_synapse/admin/v1/push/config`
- `POST` `/_matrix/vendor/v1/push/devices`
- `POST` `/_matrix/vendor/v1/push/send`
- `POST` `/_synapse/admin/v1/push/cleanup`
- `POST` `/_synapse/admin/v1/push/process`
- `PUT` `/_synapse/admin/v1/push/config`

### 搜索 (Search) （6 条）

#### `handlers/search/mod.rs` — 6 条 ✅派生表

- `GET` `/_matrix/client/v1/rooms/{room_id}/context/{event_id}`
- `GET` `/_matrix/client/v1/rooms/{room_id}/hierarchy`
- `GET` `/_matrix/client/v1/rooms/{room_id}/timestamp_to_event`
- `GET` `/_matrix/client/v3/rooms/{room_id}/context/{event_id}`
- `GET` `/_matrix/client/v3/rooms/{room_id}/hierarchy`
- `POST` `/_matrix/client/v3/search`

### 标签 (Tags) （4 条）

#### `tags.rs` — 4 条 ✅派生表

- `DELETE` `/_matrix/client/v3/user/{user_id}/rooms/{room_id}/tags/{tag}`
- `GET` `/_matrix/client/v3/user/{user_id}/rooms/{room_id}/tags`
- `GET` `/_matrix/client/v3/user/{user_id}/tags`
- `PUT` `/_matrix/client/v3/user/{user_id}/rooms/{room_id}/tags/{tag}`

### 模块 （23 条）

#### `module.rs` — 23 条 ✅派生表

- `DELETE` `/_synapse/admin/v1/modules/{module_name}`
- `GET` `/_synapse/admin/v1/account_data_callbacks`
- `GET` `/_synapse/admin/v1/account_validity/{user_id}`
- `GET` `/_synapse/admin/v1/media_callbacks`
- `GET` `/_synapse/admin/v1/media_callbacks/{callback_type}`
- `GET` `/_synapse/admin/v1/modules`
- `GET` `/_synapse/admin/v1/modules/logs/{module_name}`
- `GET` `/_synapse/admin/v1/modules/spam_check/sender/{sender}`
- `GET` `/_synapse/admin/v1/modules/spam_check/{event_id}`
- `GET` `/_synapse/admin/v1/modules/third_party_rule/{event_id}`
- `GET` `/_synapse/admin/v1/modules/type/{module_type}`
- `GET` `/_synapse/admin/v1/modules/{module_name}`
- `GET` `/_synapse/admin/v1/password_auth_providers`
- `POST` `/_synapse/admin/v1/account_data_callbacks`
- `POST` `/_synapse/admin/v1/account_validity`
- `POST` `/_synapse/admin/v1/account_validity/{user_id}/renew`
- `POST` `/_synapse/admin/v1/media_callbacks`
- `POST` `/_synapse/admin/v1/modules`
- `POST` `/_synapse/admin/v1/modules/check_spam`
- `POST` `/_synapse/admin/v1/modules/check_third_party_rule`
- `POST` `/_synapse/admin/v1/modules/{module_name}/enable`
- `POST` `/_synapse/admin/v1/password_auth_providers`
- `PUT` `/_synapse/admin/v1/modules/{module_name}/config`

### 滑动同步 (Sliding Sync) （4 条）

#### `sliding_sync.rs` — 4 条 ✅派生表

- `POST` `/_matrix/client/unstable/org.matrix.msc3575/sync`
- `POST` `/_matrix/client/unstable/org.matrix.simplified_msc3575/sync`
- `POST` `/_matrix/client/v1/sync`
- `POST` `/_matrix/client/v4/sync`

### 特性开关 （4 条）

#### `feature_flags.rs` — 4 条 ✅派生表

- `GET` `/_synapse/admin/v1/feature-flags`
- `GET` `/_synapse/admin/v1/feature-flags/{flag_key}`
- `PATCH` `/_synapse/admin/v1/feature-flags/{flag_key}`
- `POST` `/_synapse/admin/v1/feature-flags`

### 私聊 (DM) （5 条）

#### `dm.rs` — 5 条 ✅派生表

- `GET` `/_matrix/vendor/v1/direct`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/dm`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/dm/partner`
- `POST` `/_matrix/vendor/v1/create_dm`
- `PUT` `/_matrix/vendor/v1/direct/{room_id}`

### 空间 (Space) （26 条）

#### `space/children_hierarchy.rs` — 9 条 ✅派生表

- `DELETE` `/_matrix/vendor/v1/spaces/{space_id}/children/{room_id}`
- `GET` `/_matrix/client/v1/spaces/{space_id}/hierarchy`
- `GET` `/_matrix/client/v1/spaces/{space_id}/hierarchy/v1`
- `GET` `/_matrix/client/v3/spaces/{space_id}/hierarchy`
- `GET` `/_matrix/client/v3/spaces/{space_id}/hierarchy/v1`
- `GET` `/_matrix/vendor/v1/spaces/room/{room_id}/parents`
- `GET` `/_matrix/vendor/v1/spaces/{space_id}/children`
- `GET` `/_matrix/vendor/v1/spaces/{space_id}/tree_path`
- `POST` `/_matrix/vendor/v1/spaces/{space_id}/children`

#### `space/lifecycle_query.rs` — 9 条 ✅派生表

- `DELETE` `/_matrix/vendor/v1/spaces/{space_id}`
- `GET` `/_matrix/vendor/v1/spaces/public`
- `GET` `/_matrix/vendor/v1/spaces/room/{room_id}`
- `GET` `/_matrix/vendor/v1/spaces/search`
- `GET` `/_matrix/vendor/v1/spaces/statistics`
- `GET` `/_matrix/vendor/v1/spaces/user`
- `GET` `/_matrix/vendor/v1/spaces/{space_id}`
- `POST` `/_matrix/vendor/v1/spaces`
- `PUT` `/_matrix/vendor/v1/spaces/{space_id}`

#### `space/membership_state.rs` — 6 条 ✅派生表

- `GET` `/_matrix/vendor/v1/spaces/{space_id}/members`
- `GET` `/_matrix/vendor/v1/spaces/{space_id}/rooms`
- `GET` `/_matrix/vendor/v1/spaces/{space_id}/state`
- `POST` `/_matrix/vendor/v1/spaces/{space_id}/invite`
- `POST` `/_matrix/vendor/v1/spaces/{space_id}/join`
- `POST` `/_matrix/vendor/v1/spaces/{space_id}/leave`

#### `space/summary.rs` — 2 条 ✅派生表

- `GET` `/_matrix/vendor/v1/spaces/{space_id}/summary`
- `GET` `/_matrix/vendor/v1/spaces/{space_id}/summary/with_children`

### 端到端加密 (E2EE) （36 条）

#### `e2ee/keys.rs` — 36 条 ✅派生表

- `DELETE` `/_matrix/client/v1/room_keys/request/{request_id}`
- `DELETE` `/_matrix/client/v3/keys/backup/secure/{backup_id}`
- `DELETE` `/_matrix/client/v3/room_keys/request/{request_id}`
- `GET` `/_matrix/client/v1/keys/changes`
- `GET` `/_matrix/client/v1/room_keys/request`
- `GET` `/_matrix/client/v1/rooms/{room_id}/keys/distribution`
- `GET` `/_matrix/client/v3/keys/backup/secure`
- `GET` `/_matrix/client/v3/keys/backup/secure/{backup_id}`
- `GET` `/_matrix/client/v3/keys/changes`
- `GET` `/_matrix/client/v3/keys/history`
- `GET` `/_matrix/client/v3/room_keys/request`
- `GET` `/_matrix/client/v3/rooms/{room_id}/keys/distribution`
- `POST` `/_matrix/client/v1/keys/claim`
- `POST` `/_matrix/client/v1/keys/device_list/update`
- `POST` `/_matrix/client/v1/keys/device_signing/upload`
- `POST` `/_matrix/client/v1/keys/query`
- `POST` `/_matrix/client/v1/keys/signatures/upload`
- `POST` `/_matrix/client/v1/keys/upload`
- `POST` `/_matrix/client/v1/keys/upload/{device_id}`
- `POST` `/_matrix/client/v1/room_keys/request`
- `POST` `/_matrix/client/v1/sendToDevice/{event_type}/{transaction_id}`
- `POST` `/_matrix/client/v3/keys/backup/secure`
- `POST` `/_matrix/client/v3/keys/backup/secure/{backup_id}/keys`
- `POST` `/_matrix/client/v3/keys/backup/secure/{backup_id}/restore`
- `POST` `/_matrix/client/v3/keys/backup/secure/{backup_id}/verify`
- `POST` `/_matrix/client/v3/keys/claim`
- `POST` `/_matrix/client/v3/keys/device_list/update`
- `POST` `/_matrix/client/v3/keys/device_signing/upload`
- `POST` `/_matrix/client/v3/keys/query`
- `POST` `/_matrix/client/v3/keys/signatures/upload`
- `POST` `/_matrix/client/v3/keys/upload`
- `POST` `/_matrix/client/v3/keys/upload/{device_id}`
- `POST` `/_matrix/client/v3/room_keys/request`
- `POST` `/_matrix/client/v3/sendToDevice/{event_type}/{transaction_id}`
- `PUT` `/_matrix/client/v1/sendToDevice/{event_type}/{transaction_id}`
- `PUT` `/_matrix/client/v3/sendToDevice/{event_type}/{transaction_id}`

### 第三方 (Third-party) （6 条）

#### `thirdparty.rs` — 6 条 ✅派生表

- `GET` `/_matrix/client/v3/thirdparty/location`
- `GET` `/_matrix/client/v3/thirdparty/location/{protocol}`
- `GET` `/_matrix/client/v3/thirdparty/protocol/{protocol}`
- `GET` `/_matrix/client/v3/thirdparty/protocols`
- `GET` `/_matrix/client/v3/thirdparty/user`
- `GET` `/_matrix/client/v3/thirdparty/user/{protocol}`

### 管理 (Admin) （142 条）

#### `admin/room/mod.rs` — 45 条 ✅派生表

- `DELETE` `/_synapse/admin/v1/rooms/{room_id}`
- `DELETE` `/_synapse/admin/v1/rooms/{room_id}/listings/public`
- `DELETE` `/_synapse/admin/v1/rooms/{room_id}/members/{user_id}`
- `DELETE` `/_synapse/admin/v1/spaces/{space_id}`
- `GET` `/_synapse/admin/v1/room_stats`
- `GET` `/_synapse/admin/v1/room_stats/{room_id}`
- `GET` `/_synapse/admin/v1/rooms`
- `GET` `/_synapse/admin/v1/rooms/search`
- `GET` `/_synapse/admin/v1/rooms/{room_id}`
- `GET` `/_synapse/admin/v1/rooms/{room_id}/aliases`
- `GET` `/_synapse/admin/v1/rooms/{room_id}/block`
- `GET` `/_synapse/admin/v1/rooms/{room_id}/event_context/{event_id}`
- `GET` `/_synapse/admin/v1/rooms/{room_id}/forward_extremities`
- `GET` `/_synapse/admin/v1/rooms/{room_id}/listings`
- `GET` `/_synapse/admin/v1/rooms/{room_id}/members`
- `GET` `/_synapse/admin/v1/rooms/{room_id}/messages`
- `GET` `/_synapse/admin/v1/rooms/{room_id}/state`
- `GET` `/_synapse/admin/v1/rooms/{room_id}/token_sync`
- `GET` `/_synapse/admin/v1/rooms/{room_id}/version`
- `GET` `/_synapse/admin/v1/spaces`
- `GET` `/_synapse/admin/v1/spaces/{space_id}`
- `GET` `/_synapse/admin/v1/spaces/{space_id}/rooms`
- `GET` `/_synapse/admin/v1/spaces/{space_id}/stats`
- `GET` `/_synapse/admin/v1/spaces/{space_id}/users`
- `POST` `/_matrix/client/v3/admin/room/{room_id}/redact`
- `POST` `/_synapse/admin/v1/purge_history`
- `POST` `/_synapse/admin/v1/purge_room`
- `POST` `/_synapse/admin/v1/rooms/cleanup`
- `POST` `/_synapse/admin/v1/rooms/search`
- `POST` `/_synapse/admin/v1/rooms/{room_id}/backfill`
- `POST` `/_synapse/admin/v1/rooms/{room_id}/ban`
- `POST` `/_synapse/admin/v1/rooms/{room_id}/ban/{user_id}`
- `POST` `/_synapse/admin/v1/rooms/{room_id}/block`
- `POST` `/_synapse/admin/v1/rooms/{room_id}/cascade_redact`
- `POST` `/_synapse/admin/v1/rooms/{room_id}/kick`
- `POST` `/_synapse/admin/v1/rooms/{room_id}/kick/{user_id}`
- `POST` `/_synapse/admin/v1/rooms/{room_id}/make_admin`
- `POST` `/_synapse/admin/v1/rooms/{room_id}/purge_history`
- `POST` `/_synapse/admin/v1/rooms/{room_id}/search`
- `POST` `/_synapse/admin/v1/rooms/{room_id}/unban/{user_id}`
- `POST` `/_synapse/admin/v1/rooms/{room_id}/unblock`
- `POST` `/_synapse/admin/v1/shutdown_room`
- `PUT` `/_synapse/admin/v1/rooms/{room_id}/listings/public`
- `PUT` `/_synapse/admin/v1/rooms/{room_id}/make_admin`
- `PUT` `/_synapse/admin/v1/rooms/{room_id}/members/{user_id}`

#### `admin/user.rs` — 25 条 ✅派生表

- `DELETE` `/_synapse/admin/v1/users/{user_id}`
- `DELETE` `/_synapse/admin/v1/users/{user_id}/devices/{device_id}`
- `DELETE` `/_synapse/admin/v2/users/{user_id}`
- `GET` `/_synapse/admin/v1/account/{user_id}`
- `GET` `/_synapse/admin/v1/user_sessions/{user_id}`
- `GET` `/_synapse/admin/v1/user_stats`
- `GET` `/_synapse/admin/v1/users`
- `GET` `/_synapse/admin/v1/users/{user_id}`
- `GET` `/_synapse/admin/v1/users/{user_id}/devices`
- `GET` `/_synapse/admin/v1/users/{user_id}/rooms`
- `GET` `/_synapse/admin/v1/users/{user_id}/stats`
- `GET` `/_synapse/admin/v2/users`
- `GET` `/_synapse/admin/v2/users/{user_id}`
- `POST` `/_synapse/admin/v1/account/{user_id}`
- `POST` `/_synapse/admin/v1/user_sessions/{user_id}/invalidate`
- `POST` `/_synapse/admin/v1/users/batch`
- `POST` `/_synapse/admin/v1/users/batch_deactivate`
- `POST` `/_synapse/admin/v1/users/{user_id}/deactivate`
- `POST` `/_synapse/admin/v1/users/{user_id}/devices/{device_id}/delete`
- `POST` `/_synapse/admin/v1/users/{user_id}/evict`
- `POST` `/_synapse/admin/v1/users/{user_id}/login`
- `POST` `/_synapse/admin/v1/users/{user_id}/logout`
- `POST` `/_synapse/admin/v1/users/{user_id}/password`
- `PUT` `/_synapse/admin/v1/users/{user_id}/admin`
- `PUT` `/_synapse/admin/v2/users/{user_id}`

#### `admin/server.rs` — 17 条 ✅派生表

- `GET` `/_synapse/admin/v1/config`
- `GET` `/_synapse/admin/v1/experimental_features`
- `GET` `/_synapse/admin/v1/health`
- `GET` `/_synapse/admin/v1/invite/allowlist`
- `GET` `/_synapse/admin/v1/invite/blocklist`
- `GET` `/_synapse/admin/v1/jitsi/config`
- `GET` `/_synapse/admin/v1/rate-limit-status`
- `GET` `/_synapse/admin/v1/server`
- `GET` `/_synapse/admin/v1/server_version`
- `GET` `/_synapse/admin/v1/statistics`
- `GET` `/_synapse/admin/v1/status`
- `GET` `/_synapse/admin/v1/whoami`
- `GET` `/_synapse/admin/v1/whois/{user_id}`
- `GET` `/_synapse/admin/v1/whois/{user_id}/{device_id}`
- `POST` `/_synapse/admin/v1/invite/allowlist`
- `POST` `/_synapse/admin/v1/invite/blocklist`
- `POST` `/_synapse/admin/v1/restart`

#### `admin/notification.rs` — 15 条 ✅派生表

- `DELETE` `/_synapse/admin/v1/notifications/{notification_id}`
- `DELETE` `/_synapse/admin/v1/server_notices/{notice_id}`
- `DELETE` `/_synapse/admin/v1/users/{user_id}/pushers/{pushkey}`
- `GET` `/_synapse/admin/v1/notifications`
- `GET` `/_synapse/admin/v1/notifications/active`
- `GET` `/_synapse/admin/v1/notifications/{notification_id}`
- `GET` `/_synapse/admin/v1/server_notices`
- `GET` `/_synapse/admin/v1/server_notices/{notice_id}`
- `GET` `/_synapse/admin/v1/users/{user_id}/notification`
- `GET` `/_synapse/admin/v1/users/{user_id}/pushers`
- `POST` `/_synapse/admin/v1/notifications`
- `POST` `/_synapse/admin/v1/send_server_notice`
- `PUT` `/_synapse/admin/v1/notifications/{notification_id}`
- `PUT` `/_synapse/admin/v1/notifications/{notification_id}/deactivate`
- `PUT` `/_synapse/admin/v1/users/{user_id}/notification`

#### `admin/token.rs` — 9 条 ✅派生表

- `DELETE` `/_synapse/admin/v1/registration_tokens/{token}`
- `DELETE` `/_synapse/admin/v1/users/{user_id}/refresh_tokens/{token_id}`
- `DELETE` `/_synapse/admin/v1/users/{user_id}/tokens/{token_id}`
- `GET` `/_synapse/admin/v1/registration_tokens`
- `GET` `/_synapse/admin/v1/registration_tokens/{token}`
- `GET` `/_synapse/admin/v1/users/{user_id}/refresh_tokens`
- `GET` `/_synapse/admin/v1/users/{user_id}/tokens`
- `POST` `/_synapse/admin/v1/registration_tokens`
- `POST` `/_synapse/admin/v1/registration_tokens/{token}`

#### `admin/security.rs` — 8 条 ✅派生表

- `DELETE` `/_synapse/admin/v1/users/{user_id}/override_ratelimit`
- `DELETE` `/_synapse/admin/v1/users/{user_id}/rate_limit`
- `DELETE` `/_synapse/admin/v1/users/{user_id}/shadow_ban`
- `GET` `/_synapse/admin/v1/users/{user_id}/override_ratelimit`
- `GET` `/_synapse/admin/v1/users/{user_id}/rate_limit`
- `POST` `/_synapse/admin/v1/users/{user_id}/override_ratelimit`
- `POST` `/_synapse/admin/v1/users/{user_id}/shadow_ban`
- `PUT` `/_synapse/admin/v1/users/{user_id}/rate_limit`

#### `admin/report.rs` — 6 条 ✅派生表

- `DELETE` `/_synapse/admin/v1/reports/{report_id}`
- `DELETE` `/_synapse/admin/v1/rooms/{room_id}/reports/{report_id}`
- `GET` `/_synapse/admin/v1/reports`
- `GET` `/_synapse/admin/v1/reports/{report_id}`
- `GET` `/_synapse/admin/v1/rooms/{room_id}/reports`
- `GET` `/_synapse/admin/v1/rooms/{room_id}/reports/{report_id}`

#### `admin/retention.rs` — 6 条 ✅派生表

- `GET` `/_synapse/admin/v1/retention/policy`
- `GET` `/_synapse/admin/v1/retention/policy/{room_id}`
- `GET` `/_synapse/admin/v1/retention/status`
- `POST` `/_synapse/admin/v1/retention/policy`
- `POST` `/_synapse/admin/v1/retention/policy/{room_id}`
- `POST` `/_synapse/admin/v1/retention/run`

#### `admin/audit.rs` — 3 条 ✅派生表

- `GET` `/_synapse/admin/v1/audit/events`
- `GET` `/_synapse/admin/v1/audit/events/{event_id}`
- `POST` `/_synapse/admin/v1/audit/events`

#### `admin/cleanup.rs` — 3 条 ✅派生表

- `POST` `/_synapse/admin/v1/cleanup/all`
- `POST` `/_synapse/admin/v1/cleanup/rooms`
- `POST` `/_synapse/admin/v1/cleanup/tokens`

#### `admin/policy.rs` — 2 条 ✅派生表

- `GET` `/_synapse/admin/v1/policy/status`
- `POST` `/_synapse/admin/v1/policy/check`

#### `admin/register.rs` — 2 条 ✅派生表

- `GET` `/_synapse/admin/v1/register/nonce`
- `POST` `/_synapse/admin/v1/register`

#### `admin/mod.rs` — 1 条 ✅派生表

- `GET` `/_synapse/admin/info`

### 联邦 (Federation) （70 条）

#### `federation/mod.rs` — 40 条 ✅派生表

- `GET` `/_matrix/federation/v1`
- `GET` `/_matrix/federation/v1/backfill/{room_id}`
- `GET` `/_matrix/federation/v1/event/{event_id}`
- `GET` `/_matrix/federation/v1/get_event_auth/{room_id}/{event_id}`
- `GET` `/_matrix/federation/v1/hierarchy/{room_id}`
- `GET` `/_matrix/federation/v1/media/download/{server_name}/{media_id}`
- `GET` `/_matrix/federation/v1/media/thumbnail/{server_name}/{media_id}`
- `GET` `/_matrix/federation/v1/openid/userinfo`
- `GET` `/_matrix/federation/v1/publicRooms`
- `GET` `/_matrix/federation/v1/query/destination`
- `GET` `/_matrix/federation/v1/query/directory`
- `GET` `/_matrix/federation/v1/query/directory/room/{room_id}`
- `GET` `/_matrix/federation/v1/query/profile`
- `GET` `/_matrix/federation/v1/query/profile/{user_id}`
- `GET` `/_matrix/federation/v1/room/{room_id}/{event_id}`
- `GET` `/_matrix/federation/v1/state/{room_id}`
- `GET` `/_matrix/federation/v1/state_ids/{room_id}`
- `GET` `/_matrix/federation/v1/timestamp_to_event/{room_id}`
- `GET` `/_matrix/federation/v1/version`
- `GET` `/_matrix/federation/v2/query/{server_name}`
- `GET` `/_matrix/federation/v2/query/{server_name}/{key_id}`
- `GET` `/_matrix/federation/v2/server`
- `GET` `/_matrix/key/v2/query/{server_name}`
- `GET` `/_matrix/key/v2/query/{server_name}/{key_id}`
- `GET` `/_matrix/key/v2/server`
- `GET` `/_synapse/federation/v1/query/auth`
- `GET` `/_synapse/federation/v1/room_auth/{room_id}`
- `POST` `/_matrix/federation/unstable/org.matrix.msc3720/account_status`
- `POST` `/_matrix/federation/v1/get_missing_events/{room_id}`
- `POST` `/_matrix/federation/v1/publicRooms`
- `POST` `/_matrix/federation/v1/user/keys/claim`
- `POST` `/_matrix/federation/v1/user/keys/query`
- `POST` `/_matrix/federation/v1/user/keys/upload`
- `POST` `/_matrix/federation/v2/user/keys/query`
- `POST` `/_matrix/key/v2/query`
- `POST` `/_synapse/federation/v1/keys/claim`
- `POST` `/_synapse/federation/v1/keys/query`
- `POST` `/_synapse/federation/v1/keys/upload`
- `POST` `/_synapse/federation/v2/key/clone`
- `PUT` `/_matrix/federation/v1/send/{txn_id}`

#### `admin/federation.rs` — 15 条 ✅派生表

- `DELETE` `/_synapse/admin/v1/federation/blacklist/{server_name}`
- `DELETE` `/_synapse/admin/v1/federation/cache/{key}`
- `DELETE` `/_synapse/admin/v1/federation/destinations/{destination}`
- `GET` `/_synapse/admin/v1/federation/blacklist`
- `GET` `/_synapse/admin/v1/federation/cache`
- `GET` `/_synapse/admin/v1/federation/destinations`
- `GET` `/_synapse/admin/v1/federation/destinations/{destination}`
- `GET` `/_synapse/admin/v1/federation/destinations/{destination}/rooms`
- `GET` `/_synapse/admin/v1/federation/pending`
- `POST` `/_synapse/admin/v1/federation/blacklist/{server_name}`
- `POST` `/_synapse/admin/v1/federation/cache/clear`
- `POST` `/_synapse/admin/v1/federation/confirm`
- `POST` `/_synapse/admin/v1/federation/destinations/{destination}/reset_connection`
- `POST` `/_synapse/admin/v1/federation/resolve`
- `POST` `/_synapse/admin/v1/federation/rewrite`

#### `federation/membership/mod.rs` — 15 条 ✅派生表

- `GET` `/_matrix/federation/v1/make_join/{room_id}/{user_id}`
- `GET` `/_matrix/federation/v1/make_leave/{room_id}/{user_id}`
- `GET` `/_matrix/federation/v1/members/{room_id}`
- `GET` `/_matrix/federation/v1/members/{room_id}/joined`
- `GET` `/_matrix/federation/v1/user/devices/{user_id}`
- `GET` `/_synapse/federation/v1/get_joining_rules/{room_id}`
- `POST` `/_matrix/federation/v1/knock/{room_id}/{user_id}`
- `POST` `/_matrix/federation/v1/thirdparty/invite`
- `PUT` `/_matrix/federation/v1/exchange_third_party_invite/{room_id}`
- `PUT` `/_matrix/federation/v1/invite/{room_id}/{event_id}`
- `PUT` `/_matrix/federation/v1/send_join/{room_id}/{event_id}`
- `PUT` `/_matrix/federation/v1/send_leave/{room_id}/{event_id}`
- `PUT` `/_matrix/federation/v2/invite/{room_id}/{event_id}`
- `PUT` `/_matrix/federation/v2/send_join/{room_id}/{event_id}`
- `PUT` `/_matrix/federation/v2/send_leave/{room_id}/{event_id}`

### 装配 (Assembly) （109 条）

#### `assembly.rs` — 109 条 ✅派生表

- `DELETE` `/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device`
- `DELETE` `/_matrix/client/unstable/uk.tcpip.msc4133/profile/{user_id}/{key_name}`
- `DELETE` `/_matrix/client/v3/directory/room/{room_alias}`
- `DELETE` `/_matrix/client/v3/directory/room/{room_id}/alias/{room_alias}`
- `DELETE` `/_matrix/client/v3/profile/{user_id}/{key_name}`
- `GET` `/`
- `GET` `/.well-known/matrix/client`
- `GET` `/.well-known/matrix/server`
- `GET` `/.well-known/matrix/support`
- `GET` `/_health`
- `GET` `/_matrix/client/unstable/org.matrix.msc2965/auth_metadata`
- `GET` `/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device`
- `GET` `/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device/status`
- `GET` `/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device/{device_id}/events`
- `GET` `/_matrix/client/unstable/org.matrix.msc4143/rtc/transports`
- `GET` `/_matrix/client/unstable/uk.tcpip.msc4133/profile/{user_id}`
- `GET` `/_matrix/client/unstable/uk.tcpip.msc4133/profile/{user_id}/{key_name}`
- `GET` `/_matrix/client/v1/account/3pid`
- `GET` `/_matrix/client/v1/account/whoami`
- `GET` `/_matrix/client/v1/auth_metadata`
- `GET` `/_matrix/client/v1/config/client`
- `GET` `/_matrix/client/v1/media/config`
- `GET` `/_matrix/client/v1/profile/{user_id}`
- `GET` `/_matrix/client/v1/profile/{user_id}/avatar_url`
- `GET` `/_matrix/client/v1/profile/{user_id}/displayname`
- `GET` `/_matrix/client/v3/account/3pid`
- `GET` `/_matrix/client/v3/account/whoami`
- `GET` `/_matrix/client/v3/auth/{auth_type}/fallback/web`
- `GET` `/_matrix/client/v3/capabilities`
- `GET` `/_matrix/client/v3/directory/list/room/{room_id}`
- `GET` `/_matrix/client/v3/directory/room/{room_alias}`
- `GET` `/_matrix/client/v3/directory/room/{room_id}/alias`
- `GET` `/_matrix/client/v3/login`
- `GET` `/_matrix/client/v3/media/config`
- `GET` `/_matrix/client/v3/profile/{user_id}`
- `GET` `/_matrix/client/v3/profile/{user_id}/avatar_url`
- `GET` `/_matrix/client/v3/profile/{user_id}/displayname`
- `GET` `/_matrix/client/v3/profile/{user_id}/{key_name}`
- `GET` `/_matrix/client/v3/publicRooms`
- `GET` `/_matrix/client/v3/pushrules/`
- `GET` `/_matrix/client/v3/pushrules/global/`
- `GET` `/_matrix/client/v3/register`
- `GET` `/_matrix/client/v3/register/available`
- `GET` `/_matrix/client/v3/rooms/{room_id}/call/{call_id}`
- `GET` `/_matrix/client/v3/user_directory/profiles/{user_id}`
- `GET` `/_matrix/client/v3/versions`
- `GET` `/_matrix/client/v3/voip/config`
- `GET` `/_matrix/client/v3/voip/turnServer`
- `GET` `/_matrix/client/v3/voip/turnServer/guest`
- `GET` `/_matrix/client/versions`
- `GET` `/_matrix/server_version`
- `GET` `/_matrix/static/client/login/`
- `GET` `/_matrix/vendor/v1/my_rooms`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/invite_allowlist`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/invite_blocklist`
- `GET` `/health`
- `POST` `/_matrix/client/unstable/org.matrix.msc3720/account_status`
- `POST` `/_matrix/client/v1/account/3pid`
- `POST` `/_matrix/client/v1/account/3pid/add`
- `POST` `/_matrix/client/v1/account/3pid/bind`
- `POST` `/_matrix/client/v1/account/3pid/delete`
- `POST` `/_matrix/client/v1/account/3pid/email/requestToken`
- `POST` `/_matrix/client/v1/account/3pid/email/submitToken`
- `POST` `/_matrix/client/v1/account/3pid/unbind`
- `POST` `/_matrix/client/v1/account/deactivate`
- `POST` `/_matrix/client/v1/account/password`
- `POST` `/_matrix/client/v1/account/password/email/requestToken`
- `POST` `/_matrix/client/v1/account/password/email/submitToken`
- `POST` `/_matrix/client/v1/login/qr_token`
- `POST` `/_matrix/client/v3/account/3pid`
- `POST` `/_matrix/client/v3/account/3pid/add`
- `POST` `/_matrix/client/v3/account/3pid/bind`
- `POST` `/_matrix/client/v3/account/3pid/delete`
- `POST` `/_matrix/client/v3/account/3pid/email/requestToken`
- `POST` `/_matrix/client/v3/account/3pid/email/submitToken`
- `POST` `/_matrix/client/v3/account/3pid/unbind`
- `POST` `/_matrix/client/v3/account/deactivate`
- `POST` `/_matrix/client/v3/account/password`
- `POST` `/_matrix/client/v3/account/password/email/requestToken`
- `POST` `/_matrix/client/v3/account/password/email/submitToken`
- `POST` `/_matrix/client/v3/login`
- `POST` `/_matrix/client/v3/logout`
- `POST` `/_matrix/client/v3/logout/all`
- `POST` `/_matrix/client/v3/publicRooms`
- `POST` `/_matrix/client/v3/refresh`
- `POST` `/_matrix/client/v3/register`
- `POST` `/_matrix/client/v3/register/email/requestToken`
- `POST` `/_matrix/client/v3/register/email/submitToken`
- `POST` `/_matrix/client/v3/user_directory/list`
- `POST` `/_matrix/client/v3/user_directory/search`
- `POST` `/_matrix/client/v3/voip/turnServer`
- `POST` `/_matrix/vendor/v1/rooms/{room_id}/invite_allowlist`
- `POST` `/_matrix/vendor/v1/rooms/{room_id}/invite_blocklist`
- `POST` `/_matrix/vendor/v1/search_recipients`
- `POST` `/_matrix/vendor/v1/search_rooms`
- `PUT` `/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device`
- `PUT` `/_matrix/client/unstable/uk.tcpip.msc4133/profile/{user_id}/{key_name}`
- `PUT` `/_matrix/client/v1/profile/{user_id}/avatar_url`
- `PUT` `/_matrix/client/v1/profile/{user_id}/displayname`
- `PUT` `/_matrix/client/v3/directory/list/room/{room_id}`
- `PUT` `/_matrix/client/v3/directory/room/{room_alias}`
- `PUT` `/_matrix/client/v3/directory/room/{room_id}/alias/{room_alias}`
- `PUT` `/_matrix/client/v3/profile/{user_id}/avatar_url`
- `PUT` `/_matrix/client/v3/profile/{user_id}/displayname`
- `PUT` `/_matrix/client/v3/profile/{user_id}/{key_name}`
- `PUT` `/_matrix/client/v3/rooms/{room_id}/send/m.call.answer/{txn_id}`
- `PUT` `/_matrix/client/v3/rooms/{room_id}/send/m.call.candidates/{txn_id}`
- `PUT` `/_matrix/client/v3/rooms/{room_id}/send/m.call.hangup/{txn_id}`
- `PUT` `/_matrix/client/v3/rooms/{room_id}/send/m.call.invite/{txn_id}`

### 设备 (Device) （6 条）

#### `device.rs` — 6 条 ✅派生表

- `DELETE` `/_matrix/client/v3/devices/{device_id}`
- `GET` `/_matrix/client/v3/devices`
- `GET` `/_matrix/client/v3/devices/{device_id}`
- `POST` `/_matrix/client/v3/delete_devices`
- `POST` `/_matrix/client/v3/keys/device_list_updates`
- `PUT` `/_matrix/client/v3/devices/{device_id}`

### 访客 (Guest) （3 条）

#### `guest.rs` — 3 条 ✅派生表

- `GET` `/_matrix/client/v3/account/guest`
- `POST` `/_matrix/client/v3/account/guest/upgrade`
- `POST` `/_matrix/client/v3/register/guest`

### 语音 (Voice) （12 条）

#### `voice.rs` — 12 条 ✅派生表

- `GET` `/_matrix/vendor/v1/voice/config`
- `GET` `/_matrix/vendor/v1/voice/room/{room_id}`
- `GET` `/_matrix/vendor/v1/voice/room/{room_id}/stats`
- `GET` `/_matrix/vendor/v1/voice/stats`
- `GET` `/_matrix/vendor/v1/voice/user/{user_id}`
- `GET` `/_matrix/vendor/v1/voice/user/{user_id}/stats`
- `GET` `/_matrix/vendor/v1/voice/{media_id}`
- `POST` `/_matrix/vendor/v1/voice/register`
- `POST` `/_matrix/vendor/v1/voice/upload`
- `POST` `/_matrix/vendor/v1/voice/{media_id}/convert`
- `POST` `/_matrix/vendor/v1/voice/{media_id}/optimize`
- `POST` `/_matrix/vendor/v1/voice/{media_id}/transcription`

### 账户 (Account) （15 条）

#### `account_data.rs` — 15 条 ✅派生表

- `DELETE` `/_matrix/client/v3/user/{user_id}/account_data/{type}`
- `DELETE` `/_matrix/client/v3/user/{user_id}/filter/{filter_id}`
- `DELETE` `/_matrix/client/v3/user/{user_id}/rooms/{room_id}/account_data/{type}`
- `GET` `/_matrix/client/v3/user/{user_id}/account_data/`
- `GET` `/_matrix/client/v3/user/{user_id}/account_data/{type}`
- `GET` `/_matrix/client/v3/user/{user_id}/filter/{filter_id}`
- `GET` `/_matrix/client/v3/user/{user_id}/openid/request_token`
- `GET` `/_matrix/client/v3/user/{user_id}/rooms/{room_id}/account_data/{type}`
- `POST` `/_matrix/client/v3/user/{user_id}/account_data/{type}`
- `POST` `/_matrix/client/v3/user/{user_id}/filter`
- `POST` `/_matrix/client/v3/user/{user_id}/openid/request_token`
- `POST` `/_matrix/client/v3/user/{user_id}/rooms/{room_id}/account_data/{type}`
- `PUT` `/_matrix/client/v3/user/{user_id}/account_data/{type}`
- `PUT` `/_matrix/client/v3/user/{user_id}/filter`
- `PUT` `/_matrix/client/v3/user/{user_id}/rooms/{room_id}/account_data/{type}`

### 输入状态 (Typing) （5 条）

#### `typing.rs` — 5 条 ✅派生表

- `GET` `/_matrix/client/v3/rooms/{room_id}/typing`
- `GET` `/_matrix/client/v3/rooms/{room_id}/typing/{user_id}`
- `POST` `/_matrix/client/v3/rooms/typing`
- `POST` `/_matrix/client/v3/rooms/{room_id}/typing/{user_id}`
- `PUT` `/_matrix/client/v3/rooms/{room_id}/typing/{user_id}`

### 遥测 (Telemetry) （6 条）

#### `telemetry.rs` — 6 条 ✅派生表

- `GET` `/_synapse/admin/v1/telemetry/alerts`
- `GET` `/_synapse/admin/v1/telemetry/attributes`
- `GET` `/_synapse/admin/v1/telemetry/health`
- `GET` `/_synapse/admin/v1/telemetry/metrics`
- `GET` `/_synapse/admin/v1/telemetry/status`
- `POST` `/_synapse/admin/v1/telemetry/alerts/{alert_id}/ack`

### 阅后即焚 （7 条）

#### `burn_after_read.rs` — 7 条 ✅派生表

- `DELETE` `/_matrix/vendor/v1/rooms/{room_id}/burn/{event_id}`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/burn`
- `GET` `/_matrix/vendor/v1/rooms/{room_id}/burn/pending`
- `GET` `/_matrix/vendor/v1/user/burn/stats`
- `POST` `/_matrix/vendor/v1/rooms/{room_id}/burn/{event_id}`
- `PUT` `/_matrix/vendor/v1/rooms/{room_id}/burn`
- `PUT` `/_matrix/vendor/v1/user/burn/config`

### 验证码 (Captcha) （5 条）

#### `captcha.rs` — 5 条 ✅派生表

- `DELETE` `/_matrix/client/v3/register/captcha/clean`
- `GET` `/_matrix/client/v3/register/captcha/status`
- `POST` `/_matrix/client/v3/register/captcha/send`
- `POST` `/_matrix/client/v3/register/captcha/verify`
- `POST` `/_synapse/admin/v1/captcha/cleanup`

---
> 本文件由 `scripts/contract/extract_registered.py` + `gen_contract_doc.py` 生成。路由面随代码变化，请定期重新生成（或运行 `make route-contract-check` / CI `route-contract-gate` 门禁）。
---
## 附录 A — 前端裸调→SDK 迁移专项契约（基于 handler 实读，2026-08-17）

> 本附录为人工维护，补充自动生成路由面之外的**请求/响应 wire-format** 与已知漂移；重新生成脚本不覆盖本段。
> 实读源：`synapse-web/src/routes/handlers/room/members.rs`、`synapse-web/src/routes/oidc/provider.rs`、`synapse-web/src/routes/captcha.rs`、`synapse-web/src/routes/e2ee/{keys.rs,devices.rs}`。

### A.1 本次迁移涉及的 6 个端点（已迁 Tjg 前端裸调 → SDK Manager 方法）

#### 1. `POST /knock/{room_id_or_alias}`
- **注册**：`room.rs` manifest ✅（路径 `POST /knock/{room_id_or_alias}`，位于 `/_matrix/client/v3`）
- **Handler**：`handlers/room/members.rs::knock_room`（L130）
- **请求**：path 取 `room_id_or_alias`（支持 `!room`、`#alias`、裸 alias→`#alias:server`）；**body 仅读 `reason`（可选）**；`via` / `server_name` **完全忽略**（不读、不传 federation）
- **响应**：`{ "room_id": "<resolved room id>" }`
- **迁移结论**：matrix-js-sdk `knockRoom()` 把 `viaServers` 作为 **query 参数** 发送，后端忽略；原裸调把 `via` 放 body 同样被忽略 → 迁移无回归。⚠️ knock 的 via 服务器路由本就不生效（后端不消费）。

#### 2. `POST /join/{room_id_or_alias}`
- **注册**：`room.rs` manifest ✅
- **Handler**：`handlers/room/members.rs::join_room_by_id_or_alias`（L29）
- **请求**：path 取 `room_id_or_alias`；**body 仅读 `via_servers`（数组）**；`reason`、`third_party_signed`、`server_name` **均不读/不使用**
- **响应**：`{ "room_id": "<resolved room id>" }`
- **🔴 已知历史 bug（与本次迁移无关）**：原前端裸调发送 `server_name`（body），与后端期望的 `via_servers` 键不匹配 → `via_servers` 始终为空；matrix-js-sdk `joinRoom()` 把 `viaServers` 作为 query 参数发送，同样被后端忽略 → 迁移前后行为等价（均为空）。要真正修复 via 路由需前后端协同（前端改发 `via_servers` body 字段，或后端改读 query）。

#### 3. `POST /_matrix/client/v3/oidc/logout`
- **注册**：`oidc/mod.rs` manifest ✅（r0 + v3）
- **Handler**：`oidc/provider.rs::oidc_logout`（L340）
- **请求**：body `OidcLogoutRequest { device_id?, refresh_token? }`，二者均可选；空 `{}` 合法
- **响应**：`{ "success": true }`
- **迁移结论**：SDK `OidcManager.logout()` POST `/oidc/logout` body `{}` → 命中，返回 boolean。✅

#### 4. `DELETE /_matrix/client/v3/register/captcha/clean`
- **注册**：路由树已注册（`captcha.rs` L121）且 **已纳入 `captcha_route_manifest()`**（漂移已于 2026-08-17 修复）
- **Handler**：`captcha.rs::cleanup_expired`（L99）
- **请求**：无 body / 无 query（名义 AdminContext，实际为 client 公开 DELETE 路由，未强制 admin）
- **响应**：`{ "cleaned_count": <number>, "message": "Cleaned up N expired captchas" }`
- **迁移结论**：SDK `CaptchaManager.deleteExpiredCaptchas()` → `DELETE /_matrix/client/v3/register/captcha/clean`，返回 `CaptchaCleanupResponse { cleaned_count, message }`。✅ 注意字段是 `cleaned_count`（**非** `cleaned`）。

#### 5. `GET /_matrix/client/{r0,v1,v3}/room_keys/request`
- **注册**：`e2ee/keys.rs` manifest ✅（本文档 E2EE 节原漏列 GET，已补）
- **Handler**：`e2ee/devices.rs::get_room_key_requests`（L308）
- **请求**：**query 参数** `limit`（默认 100，clamp 1–1000）、`from`（cursor）、`status`、`room_id`、`session_id`
- **响应（⚠️ 包裹结构）**：`{ "requests": [ …RoomKeyRequest… ], "next_batch": <cursor|null> }`
- **🔴 SDK 类型陷阱**：`E2EEManager.listRoomKeyRequests()` 的 `.d.ts` 声明为 `Promise<RoomKeyRequestResponse[]>`，但运行时返回上述原始 body（**包裹对象**）。调用方**必须解包 `.requests ?? []`**，否则会把对象当数组返回。Tjg `MatrixDeviceService.getRoomKeyRequests()` 已修复并新增 3 个回归测试。

#### 6. `DELETE /_matrix/client/{r0,v1,v3}/room_keys/request/{request_id}`
- **注册**：`e2ee/keys.rs` manifest ✅
- **Handler**：`e2ee/devices.rs::delete_room_key_request`（L345）
- **请求**：`request_id` 取 path（SDK 侧 `encodeURIComponent`）
- **响应**：`{}`（empty）
- **迁移结论**：SDK `E2EEManager.deleteRoomKeyRequest(id)` → 命中。✅

### A.2 路径一致性复核（迁移安全性）

| # | SDK 方法 | 实际请求路径 | 后端注册 | 结论 |
|---|---|---|---|---|
| 1 | `RoomManager.knockRoom()` | `POST /_matrix/client/v3/knock/{id}` | ✅ `room.rs` | 命中 |
| 2 | `RoomManager.joinRoom()` | `POST /_matrix/client/v3/join/{id}` | ✅ `room.rs` | 命中 |
| 3 | `OidcManager.logout()` | `POST /_matrix/client/v3/oidc/logout` | ✅ `oidc/mod.rs` | 命中 |
| 4 | `CaptchaManager.deleteExpiredCaptchas()` | `DELETE /_matrix/client/v3/register/captcha/clean` | ✅ `captcha.rs`（漂移已修复） | 命中 |
| 5 | `E2EEManager.listRoomKeyRequests()` | `GET /_matrix/client/v3/room_keys/request` | ✅ `e2ee/keys.rs` | 命中（须解包 `.requests`） |
| 6 | `E2EEManager.deleteRoomKeyRequest(id)` | `DELETE /_matrix/client/v3/room_keys/request/{id}` | ✅ `e2ee/keys.rs` | 命中 |

- 6 个端点 SDK 调用路径均与后端**已注册路由**一致，**无 404 风险**。
- 唯一硬伤（#5 包裹解包）已在 Tjg 侧修复（工作区，未提交），并新增 3 个回归测试锁住数组形状。
- #2 `via_servers` 为历史 bug，迁移前后行为等价，建议单独立项前后端协同修复。

### A.3 已知漂移（manifest vs 路由树）

- `captcha_route_manifest()`（`captcha.rs` L135–L147）**曾缺少** `DELETE /_matrix/client/v3/register/captcha/clean`：该路由已在 `create_captcha_router()`（L121）注册，但 manifest 仅列 7 条（6 client + 1 admin），漏了这条 client DELETE。后果：`route_ledger` 启动校验与集成测试 `api_route_ledger_tests.rs` 的 PATCH 探测**不覆盖**该端点。**已于 2026-08-17 修复**——已在 manifest 中补 `(Method::DELETE, ".../v3/register/captcha/clean")`，现为 8 条（7 client + 1 admin），与路由树、本文档三者一致。（本文档由路由树生成，始终已正确列出该路由。）

---

*本文件由 `scripts/contract/extract_registered.py` + `gen_contract_doc.py` 生成。路由面随代码变化，请定期重新生成。附录 A / 附录 B 为人工维护增补。*

---

## 附录 B — SDK 封装完整性审计（matrix-js-sdk，2026-10-09）

> 本附录为人工维护，基于 **SDK Manager 源码 + 后端 handler 源码双侧实读**，逐条核对「已封装后端功能」的完整性；重新生成脚本不覆盖本段。
> **审计对象**：`@langkebo/matrix-js-sdk@40.2.0-langkebo.5`（`/Users/ljf/Desktop/hu_ts/matrix-js-sdk`）。
> **后端事实源**：本文件路由面（1030 条路由 / 65 模块）+ `synapse-web/src/routes/**` handler 源码。
> **证据产物**：`artifacts/sdk-contract-gap-report.md`（1195 行）、`artifacts/sdk-contract-gap.json`、`scripts/quality/path-contract-coverage.json`；镜像 commit `09074226`。

### B.0 审计方法与口径

- **三级证据法**（`scripts/audit/compare-routes.mjs`）：
  - **L1 声明面**：SDK `src/**/__generated__/route-table.ts` 生成的路由声明（弱，仅代表「约定」）。
  - **L2 调用面**：Manager 中 `(prefix, path)` 的**真实调用点**（最强证据，构成主指标）。
  - **L3 构造面**：路径字符串构造器字面量（弱，30 候选前缀命中，不校验 method）。
- **双口径覆盖率**：声明面覆盖率 = 732/1025 = **71.4%**；实现面覆盖率 = 579/644 = **89.9%**（**主指标**）。
- **分桶**：T1_CALLSITE 331 / T2_CONSTRUCTOR 563 / T3_DECLARED_ONLY 70 / DRIFT 0 / CONDITIONAL 1 / GAP 65。
- **scope 分类**：CLIENT_FACING 644 / SERVER_ONLY 383 / ROOT_OR_SSO 2 / NON_NAMESPACED 1。
- **严重等级口径**：**P1** = 功能不可用或静默返回错误结果（含 400/解析失败/数据丢失）；**P2** = 边缘场景、静默降级或非主链路；**P3** = 代码卫生。本审计**未发现 P0**。

### B.1 结论速览

| 类别 | P1 | P2 | 合计 | 判定 |
|---|---:|---:|---:|---|
| 功能遗漏（GAP） | 0 | 3 | 3 | 后端已注册、SDK 无消费者 |
| 接口不匹配（路径/契约） | 4 | 2 | 6 | 路径或响应包裹结构不一致 |
| 参数传递错误（body/query） | 10 | 2 | 12 | 键名/必填缺失，多为 400 |
| 响应解析异常（形状/格式） | 5 | 1 | 6 | 裸数组/包裹/序列化格式不符 |
| **合计** | **19** | **8** | **27** | 全部含双侧 `file:line` 证据 |

> 另有「已核实无问题」4 项（见 B.6）与「扫描器盲区/假阳性」说明（见 B.7）。

### B.2 功能遗漏（后端已实现、SDK 未封装）

| # | 功能模块 | 后端位置 | 影响范围 | 等级 |
|---|---|---|---|---|
| G-01 | app_service 通配代理 | `synapse-web/src/routes/app_service.rs:722-723`（`any(proxy_to_as)`，`/_matrix/app/v1/proxy/{as_id}/{*path}` + `/_matrix/client/v1/proxy/{as_id}/{*path}`） | 通配代理的全部 method（对应 gap report §2 客户端面缺口 7 条）；SDK 无任何消费者。属纯代理性质，是否需 SDK 封装待定 | P2 |
| G-02 | admin.media protect/unprotect | `synapse-web/src/routes/admin/media.rs:57`、`:59`（`protect/{server_name}/{media_id}` 与 `protect/{media_id}`）、`:66`（`unprotect/{media_id}`） | SDK `admin/sub-managers/admin-media-manager.ts` 仅封装 quarantine（:117）/unquarantine（:141），**无 protect/unprotect** → 媒体保护功能缺失 | P2 |
| G-03 | account 邮箱验证 submitToken | `synapse-web/src/routes/assembly.rs:384`（`account/password/email/submitToken`）、`:390`（`account/3pid/email/submitToken`） | SDK `account/index.ts:257` 仅硬编码 `/register/email/submitToken`；密码重置 / 3PID 绑定的邮箱验证码提交流程无封装 | P2 |

### B.3 接口不匹配（路径或响应包裹结构）

| # | 功能模块 | SDK 位置 | 后端位置 | 影响与证据 | 等级 |
|---|---|---|---|---|---|
| M-01 | media 签名下载 | `src/media/index.ts:439-470`（`getDownloadUrl`，客户端把 `signature`/`ts` 挂到 `:464-467`） | 真实签名端点 `synapse-web/src/routes/media/mod.rs:113-117` → handler `download.rs:320`（读 `signature:325` + **`expires:330`**）；普通 `download_media`（`download.rs:298`）**完全忽略**签名参数 | SDK 把签名附加到 `/_matrix/media/{version}/download`（或 `/_matrix/client/v1/media/download`），后端忽略 → 签名不生效；且 SDK 传 `ts`、后端读 `expires`；SDK 从不构造 `/download_signed/...` | P1 |
| M-02 | cas 服务列表 | `src/cas/index.ts:57-60`（`CasServiceListResponse { services: CasService[] }`）、`listServices` 调用 `:149-155` | `synapse-web/src/routes/cas.rs:307-311` `list_services` 返回**裸数组** `Vec<ServiceResponse>` | SDK 期望 `{services:[]}`，实际收到数组 → `response.services` 为 `undefined`，列表恒为空 | P1 |
| M-05 | thread 搜索 | `src/thread/index.ts:180-198`（`searchThreads` 发 query `term`） | `synapse-web/src/routes/handlers/thread.rs:53-57`（`SearchQuery { q: String, limit }`，`q` **必填**） | 后端读 `q`、缺 `q` 反序列化失败 → 搜索请求 400；`term` 被忽略 | P1 |
| M-06 | e2ee 密钥请求字段名 | `src/room-keys/index.ts:35-43`（`RoomKeyRequest.state`，值域 `pending/approved/rejected`） | `synapse-web/src/routes/e2ee/devices.rs:395-414`（`serialize_room_key_request` 输出 **`status`**，值域 `pending/cancelled/fulfilled`） | 字段名与值域双重不符 → 前端无法正确渲染密钥请求状态 | P1 |
| M-04 | thread 房间话题列表 | `src/thread/index.ts:136-152`（`getRoomThreads` 发 query `include`） | `synapse-web/src/routes/handlers/thread.rs:59-64`（`ListQuery` 读 `include_all`） | `include` 被忽略 → `include_all` 恒为默认，全部话题拉取行为与预期不符（静默） | P2 |
| M-07 | oidc 发现回退 | `src/client-auth.ts:59-61`（回退分支请求 `GET /auth_issuer`） | `synapse-web/src/routes/assembly.rs:198-204`（仅注册 `auth_metadata`，**无 `auth_issuer` 路由**） | 正常路径不触发；一旦落入 MSC2965 旧变体回退分支即 404 | P2 |

### B.4 参数传递错误（body / query 键名或必填缺失）

| # | 功能模块 | SDK 位置（发出） | 后端位置（期望） | 影响与证据 | 等级 |
|---|---|---|---|---|---|
| P-01 | room_summary 批量摘要 | `src/room-summary/index.ts:498-516`（`batchGetSummaries` 发 `is_suggested_only`，见 `:504`）；`:532-546`（`fetchBatchSummaries` 见 `:543`） | `synapse-web/src/routes/room_summary.rs:526-535`（`RoomSummaryBatchRequest`，`#[serde(deny_unknown_fields)]` + `#[serde(default, rename="suggested_only")]`） | 键名 `is_suggested_only` 为未知字段 → `deny_unknown_fields` **400**。对照：同文件 `batchGetRoomSummaries`（`:456`）发的是**正确**的 `suggested_only`（`:469`） | P1 |
| P-02 | thread 创建话题 | `src/thread/index.ts:158-174`（`createThread` 发 `{event_id, name?}`） | `thread.rs:19-28`（`CreateThreadBody` 需 `root_event_id` + `content`） | 字段名不符且缺必填 → 反序列化失败 400 | P1 |
| P-03 | thread 全局创建话题 | `src/thread/index.ts:503-517`（`createGlobalThread` 发 `{room_id, event_id, name?}`） | 同上 `thread.rs:19-28` | 同 P-02 | P1 |
| P-04 | thread 创建回复 | `src/thread/index.ts:412-431`（`createThreadReply` 仅发 `{content}`） | `thread.rs:30-40`（`CreateReplyBody` 需 `event_id` + `root_event_id` + `content`） | 缺必填字段 → 400 | P1 |
| P-05 | thread 标记已读 | `src/thread/index.ts:322-340`（`markThreadRead` 发 `{read_up_to}`） | `thread.rs:47-51`（`MarkReadBody` 需 `event_id` + `origin_server_ts`） | 字段名不符且缺必填 → 400 | P1 |
| P-06 | thread 订阅 | `src/thread/index.ts:346-360`（`subscribeThread` 发 `{}`） | `thread.rs:42-45`（`SubscribeBody` 需 `notification_level`） | 缺必填字段 → 400 | P1 |
| P-07 | cas proxy | `src/cas/index.ts:315-327`（`proxy` 发 query `targetService`） | `synapse-web/src/routes/cas.rs:58-59`（`ProxyQuery { target_service: String }`，**非 Option**） | 参数名不符且必填缺失 → 400 | P1 |
| P-09 | cas 注册服务 | `src/cas/index.ts:170-185`（`createService` body 缺 `service_id`、用 `service_url` 代 `service_url_pattern`）；响应类型 `:69-72` 期望 `{id, name}` | `cas.rs:79-88`（`RegisterServiceBody` 需 `service_id` + `name` + `service_url_pattern`）；响应 `cas.rs:97-103` 用 `service_id`/`is_enabled` | 请求缺必填 + 响应字段名不符，注册服务链路不可用 | P1 |
| P-10 | e2ee 创建密钥请求 | `src/room-keys/index.ts:49-53`（`CreateRoomKeyRequest` 仅 `room_id/session_id/device_id?`） | `synapse-web/src/routes/e2ee/devices.rs:376-382`（`CreateRoomKeyRequestBody` 需 `algorithm` + `room_id` + `session_id`） | 缺必填 `algorithm` → 400 | P1 |
| P-12 | friend_room 添加好友 | `src/friend/sub-managers/friend-request-manager.ts:135-152`（`addFriend` body 于 `:149` 发 `{user_id, reason}`） | `synapse-web/src/routes/friend_room.rs:123-129`（`AddFriendRequest`，`#[serde(deny_unknown_fields)]`，字段为 `{user_id, message}`） | `reason` 为未知字段 → **400**。对照：同文件 `sendFriendRequest`（`:97-120`）发的是**正确**的 `message`（`:114`） | P1 |
| P-08 | cas validate 系列 | `src/cas/index.ts:252-313`（`serviceValidate`/`proxyValidate`/`p3ServiceValidate` 发 query `pgtUrl`，见 `:261`/`:282`/`:303`） | `cas.rs:51`/`:67`/`:75`（读 `pgt_url`，均 `Option`） | 参数名不符 → 后端静默丢弃（无 400，`pgt_url` 恒为 None） | P2 |
| P-11 | e2ee 列表分页 | `src/device-keys/index.ts:398-414`（声明 `limit` 于 `:402`，但 params 仅写入 `status`/`room_id`/`session_id`，见 `:405-407`） | `synapse-web/src/routes/e2ee/devices.rs:338-351`（后端支持 `limit` 分页） | `limit` 参数从未下发 → 分页能力失效（静默） | P2 |

### B.5 响应解析异常（形状 / 包裹 / 序列化格式）

| # | 功能模块 | SDK 位置（期望） | 后端位置（实际） | 影响与证据 | 等级 |
|---|---|---|---|---|---|
| R-01 | thread 话题回复列表 | `src/thread/index.ts:388-406`（`getThreadReplies`）；响应类型 `IThreadRepliesResponse { replies[], next_batch? }`（`:107-110`） | `thread.rs:453-458`（`get_replies` 返回**裸数组** `Json<Vec<ReplyResponse>>`） | SDK 取 `.replies` 得 `undefined` → 回复列表恒空、分页丢失 | P1 |
| R-03 | cas 校验响应格式 | `src/cas/index.ts:98-107`（`CasServiceValidateResponse`/`CasProxyResponse`，按 **JSON** 解析） | `cas.rs:191-207`（`service_validate` 返回 **`text/plain`** `yes\n{user}\n`）；`proxy_validate`/`proxy`/`p3_service_validate`（`cas.rs:209/237/249`）返回 **XML** | JSON 解析非 JSON 响应 → 抛错；CAS 校验链路不可用 | P1 |
| R-04 | media 分块上传 | `src/media/index.ts:138-163`：`ChunkUploadResponse.received_bytes`（`:141`）、`ChunkUploadCompleteResponse.upload_id`（`:146`）、`ChunkUploadProgressResponse.received_chunks/bytes_received/total_bytes`（`:160-162`） | `synapse-web/src/routes/media/upload.rs`：chunk 返回 `uploaded_chunks/uploaded_size/status`（`:269-274`）、complete 返回 `content_uri/media_id/size`（`:292-294`）、progress 返回 `uploaded_chunks/uploaded_size/total_size`（`:335-342`） | 字段名系统性不符 → 分块进度/完成结果字段读到 `undefined`（`received_bytes`、`upload_id`、`received_chunks` 等） | P1 |
| R-05 | e2ee 创建密钥请求响应 | `src/e2ee/index.ts:135-141`（`RoomKeyRequestResponse` 声明 `room_id/session_id/algorithm/state`）、`createRoomKeyRequest` `:308-310` | `synapse-web/src/routes/e2ee/devices.rs:275-300`（仅返回 `{request_id}`） | 期望的 4 个字段全部缺失 → 调用方拿到全 `undefined` | P1 |
| R-06 | e2ee 密钥请求列表分页 | `src/device-keys/index.ts:161-163` 与 `src/room-keys/index.ts:45-47`（`RoomKeyRequestsResponse` 仅 `requests`，**缺 `next_batch`**） | `synapse-web/src/routes/e2ee/devices.rs:319-351`（返回 `requests` + **`next_batch`**） | `next_batch` 游标丢失 → 无法翻页（数据截断） | P1 |
| R-02 | thread 话题详情 | `src/thread/index.ts:225-238`（`getThread`）；响应类型 `IThreadResponse { thread }`（`:112-114`） | `thread.rs:359-364`（`get_thread` 返回 `ThreadDetailResponse`，**非 `{thread}` 包裹**） | 取 `.thread` 得 `undefined`（形状不符） | P2 |

### B.6 已核实无问题（防误报留痕）

| 功能模块 | SDK 位置 | 后端位置 | 结论 |
|---|---|---|---|
| admin 房间 redact | `src/admin/sub-managers/admin-room-manager.ts:480-486`（`POST /admin/room/{roomId}/redact`，V3 前缀） | `synapse-web/src/routes/admin/room/mod.rs:148-151`；响应 `{redacted}`（`management.rs:652-654`） | ✅ 路径与响应字段一致 |
| appservices 管理面字面路径 | `src/.../app-service/index.ts`（统一 `/appservices`） | `synapse-web/src/routes/app_service.rs:728-744` | ✅ 一致（无历史下划线 bug） |
| m.call.* 呼叫事件 | 走通用 `sendEvent`（`PUT /rooms/$roomId/send/$eventType/$txnId`，`client-send-paths.ts:43`） | `synapse-web/src/routes/room.rs:64` | ✅ 命中通用发事件端点 |
| pushrules global | `getPushRulesByScope("global")` → `/pushrules/global` | `synapse-web/src/routes/push.rs:19` | ✅ 命中 |

### B.7 扫描器口径与已知盲区

1. **服务端面缺口 58 条不构成 SDK 问题**：gap report §6 的 58 条（federation 30 / admin 15 / app_service 13）多为 `SERVER_ONLY` 面（联邦/管理内部），非 SDK 客户端受众；仅 app_service 通配代理（G-01）与客户端面相关。
2. **T3 70 条含解析器盲区假阳性**：分布于 key_backup 18 / media 15 / assembly 13 / cas 9 / push_notification 4 / msc4108_rendezvous 3 / room_summary 3 / oidc 2 / thread 2 / friend_room 1。经源码核对，**key_backup 的 `room_keys/keys` 族实际已实现**（`src/rust-crypto/backup.ts:624/691/756/772/977`、`PerSessionKeyBackupDownloader.ts:264`），属 T3 构造面识别盲区（构造器路径未命中调用面）→ **非真缺口**。其余 T3 项需按本轮同样方法逐条人工复核，勿直接当缺口处理。
3. **`path-contract` 门禁实测（`pnpm quality:path-contract`）**：扫描 459 文件、提取 608 调用、匹配 579（含 1 通配）、豁免 29、**不匹配 0**、动态跳过 56、**未校验 8**、域外 7、已校验包装器 11、恒等包装器 46、**未覆盖包装器 3**。
   - **未校验 8 个调用点**（动态路径，绕过静态校验）：形态 `this-method 4 / identifier 2 / other 1 / concat 1`；文件 `client-auth.ts:1`、`client.ts:1`、`room-summary/sub-managers/room-invite-policy-manager.ts:4`、`rust-crypto/backup.ts:1`、`rust-crypto/rust-crypto.ts:1`；包装器 `authedRequest 3 / request 1 / requestV3 4`。`client-auth.ts` 的未校验项即 M-07（`/auth_issuer` 回退）。
   - **未覆盖包装器 3 个**：`requestOtherUrl`、`rawJsonRequest`、`sendToDeviceRequest`——这些包装器发起的请求不在静态校验覆盖内，需人工兜底。
4. **复现命令**（在 `/Users/ljf/Desktop/hu_ts/matrix-js-sdk`）：
   - 审计：`pnpm contract:sync && pnpm contract:codegen && node scripts/audit/compare-routes.mjs`
   - 门禁：`pnpm quality:path-contract`（另有 `quality:admin-response-contract` / `quality:contract-drift` / `quality:sdk-contracts` / `quality:route-set-parity`）。
5. **口径提示**：`generate_sdk_ledger_fixtures.sh` 注释称 “all-extensions all=1407” 为**陈旧注释**，与实测 1030 不符；一律以本文件头部计数与 `sdk-contract-gap.json` 实测为准。
