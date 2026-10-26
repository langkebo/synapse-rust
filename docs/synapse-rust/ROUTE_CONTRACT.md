# synapse-rust 路由契约（Route Contract）

> 自动生成于 2026-10-27，源 = `synapse-web/src/routes/**` 真实 `.route()` 注册面 + `derived_routes.rs`（含按 profile 拆分的 `derived_route_table_{always,worker,oidc}.inc.rs`）派生覆盖。
>
> 本文件是后端 HTTP 契约的**事实来源之一**（机器侧权威为 `derived_routes.rs` 生成的 `RouteLedger`，启动时校验、集成测试 PATCH 探测）。人工文档（INDEX.md / API_COVERAGE_REPORT.md）须与之保持一致。
>
> ⚠️ MSC 编号在本仓的**实际语义**以 [`MSC_SEMANTICS.md`](MSC_SEMANTICS.md) 为唯一真相源；若干编号（4155 / 4204 / 3967）被借用承载了与官方提案不同的功能，按编号推断语义前请先查表。

## 总览

> **路由三口径**（互不可加、不可互换）的唯一计算实现是 `scripts/contract/route_metrics.py --json`，
> 数据源为本目录的 `ROUTE_HANDLERS.json`；`route_metrics.py --check` 会拿本节三个数字与 sidecar 对账，
> 任何一处手改或漂移都判红。引用「路由数量」时必须写明口径，三个数字不可互相替代。

- **注册路由条目**（唯一 `(method, absolute_path)`，经 `.nest()` 前缀解析后去重）：**979**
  · `/_matrix/client` 333 · `/_synapse/admin` 299 · 其他命名空间 347
- **唯一路径**（去掉方法后的唯一 `absolute_path`）：**779**
  · `/_matrix/client` 246 · `/_synapse/admin` 232 · 其他命名空间 301
- **逻辑端点**（唯一路径再折叠 `v1`/`v3`/`r0`/`unstable/<seg>` → `vX`）：**760**
  · `/_matrix/client` 229 · `/_synapse/admin` 230 · 其他命名空间 301
- 含路由注册的模块文件：**66**（生成器口径；sidecar 口径 113/74/51 的差异见附录 B）
- `derived_routes.rs` 中的 `registered_by` 标签：**75**
- 非默认 profile 门控的路由（`default` 构建不注册）：**19**（worker **11** · oidc **8**，明细见「运行时 Profile 门控」）
- 已被派生表覆盖的模块：**66**

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
- 〔`always` / `X` 双档注册〕 —— 同一 `(method, path)` 在 `always` 与 `X` 两档都注册，但两档的 `registered_by` 不同（默认档走回退实现，`X` 档走完整实现）。默认档可用，**不**计入上表；这类孪生行正是派生表 981 行去重为 979 条的来源。

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

## 有意不实现的端点（by-design unsupported）

以下端点**已注册且可达**，但规范描述的行为被有意不实现：handler 恒定返回同一个错误码，而不是给出一个可能被误当成真实结果的占位 200。登记表是 `scripts/contract/ledger_annotations.txt` 的 `unsupported=true`，守卫 `check_unsupported_endpoints.py` 把它与 handler 源码**双向**比对（漏登记 / 陈旧登记都转红），因此这张表不会悄悄漂移。

当前共 **16** 条：

| Method | Path |
|---|---|
| `GET` | `/_matrix/client/v3/rooms/{room_id}/keys/distribution` |
| `GET` | `/_matrix/client/v3/thirdparty/location` |
| `GET` | `/_matrix/client/v3/thirdparty/location/{protocol}` |
| `GET` | `/_matrix/client/v3/thirdparty/user` |
| `GET` | `/_matrix/client/v3/thirdparty/user/{protocol}` |
| `GET` | `/_matrix/vendor/v1/rooms/{room_id}/keys/{event_id}` |
| `GET` | `/_synapse/federation/v1/query/auth` |
| `POST` | `/_matrix/client/v3/keys/backup/secure/{backup_id}/verify` |
| `POST` | `/_matrix/federation/v1/user/keys/upload` |
| `POST` | `/_matrix/vendor/v1/voice/{media_id}/convert` |
| `POST` | `/_matrix/vendor/v1/voice/{media_id}/optimize` |
| `POST` | `/_matrix/vendor/v1/voice/{media_id}/transcription` |
| `POST` | `/_synapse/federation/v1/keys/claim` |
| `POST` | `/_synapse/federation/v1/keys/query` |
| `POST` | `/_synapse/federation/v1/keys/upload` |
| `PUT` | `/_matrix/vendor/v1/rooms/{room_id}/report/{event_id}/score` |

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

### 其他 (Other) （21 条）

#### `handlers/thread.rs` — 21 条 ✅派生表

- `DELETE` `/_matrix/vendor/v1/rooms/{room_id}/threads/{thread_id}`
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

### 外部服务 （11 条）

#### `external_service.rs` — 11 条 ✅派生表

- `DELETE` `/_matrix/vendor/v1/external_services/{service_id}`
- `DELETE` `/_synapse/admin/v1/external_services/{as_id}`
- `GET` `/_matrix/vendor/v1/external_services/health`
- `GET` `/_synapse/admin/v1/external_services`
- `GET` `/_synapse/admin/v1/external_services/health`
- `GET` `/_synapse/admin/v1/external_services/{as_id}/health`
- `POST` `/_synapse/admin/v1/external_services`
- `POST` `/_synapse/admin/v1/external_services/{as_id}/health/check`
- `POST` `/_synapse/external/webhook/{service_id}`
- `PUT` `/_matrix/vendor/v1/external_services/{service_id}`
- `PUT` `/_synapse/admin/v1/external_services/{as_id}`

### 好友 (Friends) （28 条）

#### `friend_room.rs` — 28 条 ✅派生表

- `DELETE` `/_matrix/vendor/v1/friends/groups/{group_id}`
- `DELETE` `/_matrix/vendor/v1/friends/groups/{group_id}/remove/{user_id}`
- `DELETE` `/_matrix/vendor/v1/friends/{user_id}`
- `GET` `/_matrix/vendor/v1/friends`
- `GET` `/_matrix/vendor/v1/friends/check/{user_id}`
- `GET` `/_matrix/vendor/v1/friends/dm/{user_id}`
- `GET` `/_matrix/vendor/v1/friends/groups`
- `GET` `/_matrix/vendor/v1/friends/groups/{group_id}/friends`
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

### 审核 (Moderation) （5 条）

#### `moderation.rs` — 5 条 ✅派生表

- `GET` `/_matrix/vendor/v1/rooms/{room_id}/report/{event_id}/scanner_info`
- `POST` `/_matrix/client/v3/rooms/{room_id}/report`
- `POST` `/_matrix/client/v3/rooms/{room_id}/report/{event_id}`
- `POST` `/_matrix/client/v3/users/{user_id}/report`
- `PUT` `/_matrix/vendor/v1/rooms/{room_id}/report/{event_id}/score`

### 密钥备份 (Key Backup) （33 条）

#### `key_backup.rs` — 33 条 ✅派生表

- `DELETE` `/_matrix/client/v3/room_keys/keys`
- `DELETE` `/_matrix/client/v3/room_keys/keys/{room_id}`
- `DELETE` `/_matrix/client/v3/room_keys/keys/{room_id}/{session_id}`
- `DELETE` `/_matrix/client/v3/room_keys/version/{version}`
- `DELETE` `/_matrix/client/v3/room_keys/{version}/keys`
- `DELETE` `/_matrix/client/v3/room_keys/{version}/keys/{room_id}`
- `DELETE` `/_matrix/client/v3/room_keys/{version}/keys/{room_id}/{session_id}`
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
- `POST` `/_matrix/client/v3/room_keys/batch_recover`
- `POST` `/_matrix/client/v3/room_keys/import`
- `POST` `/_matrix/client/v3/room_keys/import/{version}`
- `POST` `/_matrix/client/v3/room_keys/recover`
- `POST` `/_matrix/client/v3/room_keys/version`
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

### 应用服务 (AppService) （41 条）

#### `app_service.rs` — 41 条 ✅派生表

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
- `POST` `/_matrix/client/unstable/io.element.msc4512/appservice/fed_proxy`
- `POST` `/_matrix/client/v1/appservice/fed_proxy`
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

### 延迟事件 （7 条）

#### `delayed_events.rs` — 7 条 ✅派生表

- `GET` `/_matrix/client/unstable/org.matrix.msc4140/delayed_events`
- `GET` `/_matrix/client/unstable/org.matrix.msc4140/delayed_events/{delay_id}`
- `POST` `/_matrix/client/unstable/org.matrix.msc4140/delayed_events/{delay_id}`
- `POST` `/_matrix/client/unstable/org.matrix.msc4140/delayed_events/{delay_id}/cancel`
- `POST` `/_matrix/client/unstable/org.matrix.msc4140/delayed_events/{delay_id}/restart`
- `POST` `/_matrix/client/unstable/org.matrix.msc4140/delayed_events/{delay_id}/send`
- `PUT` `/_matrix/client/unstable/org.matrix.msc4140/rooms/{room_id}/delayed_event/{event_type}/{txn_id}`

### 房间 (Room) （114 条）

#### `room.rs` — 93 条 ✅派生表

- `DELETE` `/_matrix/client/unstable/org.matrix.msc4354/rooms/{room_id}/sticky_events/{event_type}`
- `DELETE` `/_matrix/vendor/v1/rooms/{room_id}/pinned_events/{event_id}`
- `GET` `/_matrix/client/unstable/org.matrix.msc4354/rooms/{room_id}/sticky_events`
- `GET` `/_matrix/client/unstable/uk.half-shot.msc2666/user/mutual_rooms`
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
- `PUT` `/_matrix/client/v3/rooms/{room_id}/account_data/{type}`
- `PUT` `/_matrix/client/v3/rooms/{room_id}/read_markers`
- `PUT` `/_matrix/client/v3/rooms/{room_id}/redact/{event_id}/{txn_id}`
- `PUT` `/_matrix/client/v3/rooms/{room_id}/send/{event_type}/{txn_id}`
- `PUT` `/_matrix/client/v3/rooms/{room_id}/state/m.room.power_levels/`
- `PUT` `/_matrix/client/v3/rooms/{room_id}/state/{event_type}`
- `PUT` `/_matrix/client/v3/rooms/{room_id}/state/{event_type}/`
- `PUT` `/_matrix/client/v3/rooms/{room_id}/state/{event_type}/{state_key}`
- `PUT` `/_matrix/vendor/v1/rooms/{room_id}/anti_screenshot`
- `PUT` `/_matrix/vendor/v1/rooms/{room_id}/retention`
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

### 搜索 (Search) （5 条）

#### `handlers/search/mod.rs` — 5 条 ✅派生表

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

### 特性开关 （5 条）

#### `feature_flags.rs` — 5 条 ✅派生表

- `DELETE` `/_synapse/admin/v1/feature-flags/{flag_key}`
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

### 端到端加密 (E2EE) （22 条）

#### `e2ee/keys.rs` — 22 条 ✅派生表

- `DELETE` `/_matrix/client/v3/keys/backup/secure/{backup_id}`
- `DELETE` `/_matrix/client/v3/room_keys/request/{request_id}`
- `GET` `/_matrix/client/v3/keys/backup/secure`
- `GET` `/_matrix/client/v3/keys/backup/secure/{backup_id}`
- `GET` `/_matrix/client/v3/keys/changes`
- `GET` `/_matrix/client/v3/keys/history`
- `GET` `/_matrix/client/v3/room_keys/request`
- `GET` `/_matrix/client/v3/rooms/{room_id}/keys/distribution`
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
- `PUT` `/_matrix/client/v3/sendToDevice/{event_type}/{transaction_id}`

### 第三方 (Third-party) （6 条）

#### `thirdparty.rs` — 6 条 ✅派生表

- `GET` `/_matrix/client/v3/thirdparty/location`
- `GET` `/_matrix/client/v3/thirdparty/location/{protocol}`
- `GET` `/_matrix/client/v3/thirdparty/protocol/{protocol}`
- `GET` `/_matrix/client/v3/thirdparty/protocols`
- `GET` `/_matrix/client/v3/thirdparty/user`
- `GET` `/_matrix/client/v3/thirdparty/user/{protocol}`

### 管理 (Admin) （150 条）

#### `admin/room/mod.rs` — 46 条 ✅派生表

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
- `POST` `/_synapse/admin/v1/join/{room_id_or_alias}`
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

#### `admin/user.rs` — 27 条 ✅派生表

- `DELETE` `/_synapse/admin/v1/users/{user_id}`
- `DELETE` `/_synapse/admin/v1/users/{user_id}/devices/{device_id}`
- `DELETE` `/_synapse/admin/v2/users/{user_id}`
- `GET` `/_synapse/admin/v1/account/{user_id}`
- `GET` `/_synapse/admin/v1/user_sessions/{user_id}`
- `GET` `/_synapse/admin/v1/user_stats`
- `GET` `/_synapse/admin/v1/users`
- `GET` `/_synapse/admin/v1/users/{user_id}`
- `GET` `/_synapse/admin/v1/users/{user_id}/admin`
- `GET` `/_synapse/admin/v1/users/{user_id}/devices`
- `GET` `/_synapse/admin/v1/users/{user_id}/rooms`
- `GET` `/_synapse/admin/v1/users/{user_id}/stats`
- `GET` `/_synapse/admin/v2/users`
- `GET` `/_synapse/admin/v2/users/{user_id}`
- `POST` `/_synapse/admin/v1/account/{user_id}`
- `POST` `/_synapse/admin/v1/deactivate/{user_id}`
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

#### `admin/server.rs` — 21 条 ✅派生表

- `DELETE` `/_synapse/admin/v1/invite/allowlist/{user_id}`
- `DELETE` `/_synapse/admin/v1/invite/blocklist/{user_id}`
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
- `POST` `/_synapse/admin/v1/invite/allowlist/{user_id}`
- `POST` `/_synapse/admin/v1/invite/blocklist`
- `POST` `/_synapse/admin/v1/invite/blocklist/{user_id}`
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

#### `admin/security.rs` — 9 条 ✅派生表

- `DELETE` `/_synapse/admin/v1/users/{user_id}/override_ratelimit`
- `DELETE` `/_synapse/admin/v1/users/{user_id}/rate_limit`
- `DELETE` `/_synapse/admin/v1/users/{user_id}/shadow_ban`
- `GET` `/_synapse/admin/v1/users/{user_id}/override_ratelimit`
- `GET` `/_synapse/admin/v1/users/{user_id}/rate_limit`
- `GET` `/_synapse/admin/v1/users/{user_id}/shadow_ban`
- `POST` `/_synapse/admin/v1/users/{user_id}/override_ratelimit`
- `POST` `/_synapse/admin/v1/users/{user_id}/shadow_ban`
- `PUT` `/_synapse/admin/v1/users/{user_id}/rate_limit`

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

### 装配 (Assembly) （89 条）

#### `assembly.rs` — 89 条 ✅派生表

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
- `GET` `/_matrix/client/unstable/org.matrix.msc4143/rtc/transports`
- `GET` `/_matrix/client/unstable/uk.tcpip.msc4133/profile/{user_id}`
- `GET` `/_matrix/client/unstable/uk.tcpip.msc4133/profile/{user_id}/{key_name}`
- `GET` `/_matrix/client/v1/auth_metadata`
- `GET` `/_matrix/client/v1/config/client`
- `GET` `/_matrix/client/v1/media/config`
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
- `POST` `/_matrix/client/v1/login/get_token`
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
- `POST` `/_matrix/vendor/v1/invites/claim`
- `POST` `/_matrix/vendor/v1/invites/one-time`
- `POST` `/_matrix/vendor/v1/rooms/{room_id}/invite_allowlist`
- `POST` `/_matrix/vendor/v1/rooms/{room_id}/invite_blocklist`
- `POST` `/_matrix/vendor/v1/search_recipients`
- `POST` `/_matrix/vendor/v1/search_rooms`
- `PUT` `/_matrix/client/unstable/uk.tcpip.msc4133/profile/{user_id}/{key_name}`
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

### 设备 (Device) （12 条）

#### `device.rs` — 6 条 ✅派生表

- `DELETE` `/_matrix/client/v3/devices/{device_id}`
- `GET` `/_matrix/client/v3/devices`
- `GET` `/_matrix/client/v3/devices/{device_id}`
- `POST` `/_matrix/client/v3/delete_devices`
- `POST` `/_matrix/client/v3/keys/device_list_updates`
- `PUT` `/_matrix/client/v3/devices/{device_id}`

#### `handlers/dehydrated_device.rs` — 6 条 ✅派生表

- `DELETE` `/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device`
- `GET` `/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device`
- `GET` `/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device/status`
- `GET` `/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device/{device_id}/events`
- `POST` `/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device/{device_id}/events`
- `PUT` `/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device`

### 访客 (Guest) （3 条）

#### `guest.rs` — 3 条 ✅派生表

- `GET` `/_matrix/client/v3/account/guest`
- `POST` `/_matrix/client/v3/account/guest/upgrade`
- `POST` `/_matrix/client/v3/register/guest`

### 语音 (Voice) （13 条）

#### `voice.rs` — 13 条 ✅派生表

- `DELETE` `/_matrix/vendor/v1/voice/{media_id}`
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
- **Handler**：`handlers/room/members.rs::knock_room`
- **请求**：path 取 `room_id_or_alias`（支持 `!room`、`#alias`、裸 alias→`#alias:server`）；**query 读重复参数 `via`（`?via=srv1&via=srv2`，percent-decode）**；body 读 `reason`（可选）与历史兼容键 `via_servers`（仅在 query 无 `via` 时生效）
- **响应**：`{ "room_id": "<resolved room id>" }`
- **迁移结论**：matrix-js-sdk `knockRoom()` 把 `viaServers` 作为 **query 参数** 发送 → 现在会被消费。⚠️ 该端点契约类别为 `query: literal`（`Query<Vec<(String,String)>>`，键名由 `k == "via"` 字面量证明），见 A.4。

#### 2. `POST /join/{room_id_or_alias}`
- **注册**：`room.rs` manifest ✅
- **Handler**：`handlers/room/members.rs::join_room_by_id_or_alias`
- **请求**：path 取 `room_id_or_alias`；**query 读重复参数 `via`**；body 读历史兼容键 `via_servers`（仅 query 无 `via` 时生效）；`reason`、`third_party_signed`、`server_name` **均不读/不使用**
- **响应**：`{ "room_id": "<resolved room id>" }`
- **迁移结论**：原「`via` 被忽略」的历史 bug 已修复——`?via=` 优先，body `via_servers` 作为回退。该端点契约类别同为 `query: literal`（见 A.4）。

#### 3. `POST /_matrix/client/v3/oidc/logout`
- **注册**：`oidc/mod.rs` manifest ✅（r0 + v3）
- **Handler**：`oidc/provider.rs::oidc_logout`
- **请求**：body 同时接受两族字段（全部可选）——
  - 会话族：`refresh_token`、`device_id`（矩阵客户端注销）
  - RP-Initiated Logout 族（MSC2964 / OIDC RP-Initiated Logout）：`client_id`、`id_token_hint`、`post_logout_redirect_uri`、`state`
- **语义**：`device_id` → 撤销该设备；`refresh_token` → 校验归属后撤销（已过期/已撤销视为幂等成功）；两者都不传（含空 `{}`）→ **400 `M_MISSING_PARAM`**，不再返回假的 `{"success":true}`。非本人 `refresh_token` → 403。
- **响应**：`{ "success": true, "device_revoked": <bool>, "refresh_token_revoked": <bool> }`
  （RP 字段目前只做读取与日志，不改响应形状；`post_logout_redirect_uri` 的跳转由客户端自行完成。`refresh_token` 已过期/已撤销时按幂等成功处理：`success=true` 且 `refresh_token_revoked=false`。）
- **迁移结论**：SDK `OidcManager.logout()` 若发送空 `{}`，现在会拿到 400 `M_MISSING_PARAM`；须至少传 `device_id` 或 `refresh_token`（RP 三字段可与它们同时传，但不能替代）。

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
| 3 | `OidcManager.logout()` | `POST /_matrix/client/v3/oidc/logout` | ✅ `oidc/mod.rs` | 命中（空 body 现为 400 `M_MISSING_PARAM`） |
| 4 | `CaptchaManager.deleteExpiredCaptchas()` | `DELETE /_matrix/client/v3/register/captcha/clean` | ✅ `captcha.rs`（漂移已修复） | 命中 |
| 5 | `E2EEManager.listRoomKeyRequests()` | `GET /_matrix/client/v3/room_keys/request` | ✅ `e2ee/keys.rs` | 命中（须解包 `.requests`） |
| 6 | `E2EEManager.deleteRoomKeyRequest(id)` | `DELETE /_matrix/client/v3/room_keys/request/{id}` | ✅ `e2ee/keys.rs` | 命中 |

- 6 个端点 SDK 调用路径均与后端**已注册路由**一致，**无 404 风险**。
- 唯一硬伤（#5 包裹解包）已在 Tjg 侧修复（工作区，未提交），并新增 3 个回归测试锁住数组形状。
- #2 `via_servers` 为历史 bug，迁移前后行为等价，建议单独立项前后端协同修复。

### A.3 已知漂移（manifest vs 路由树）

- `captcha_route_manifest()`（`captcha.rs` L135–L147）**曾缺少** `DELETE /_matrix/client/v3/register/captcha/clean`：该路由已在 `create_captcha_router()`（L121）注册，但 manifest 仅列 7 条（6 client + 1 admin），漏了这条 client DELETE。后果：`route_ledger` 启动校验与集成测试 `api_route_ledger_tests.rs` 的 PATCH 探测**不覆盖**该端点。**已于 2026-08-17 修复**——已在 manifest 中补 `(Method::DELETE, ".../v3/register/captcha/clean")`，现为 8 条（7 client + 1 admin），与路由树、本文档三者一致。（本文档由路由树生成，始终已正确列出该路由。）

### A.4 契约可判定性：`query: literal` 与「不可判」的区别（2026-10-20）

`ROUTE_HANDLERS.json` 的 `query` 分类是**给下游判定用的**，不是猜的。与本文档相关的三类要分清：

| 分类 | 含义 | 下游能做什么 |
|---|---|---|
| `typed` | `Query<T>` 单一 struct，`query_params` 是 `T` 的 wire 名全集 | 可逐字段比对 SDK 是否发错/漏发 |
| `literal` | 动态提取器（如 `Query<Vec<(String, String)>>`），但键名可由 handler 内的字符串字面量证明 | **键名可比对**（`query_params` 是证明出来的键集）；值类型不可比 |
| `opaque` | 键集在运行时决定 | 记为「不可判」，不要当成「后端未声明」 |

（v3 里还有 `ambiguous` 一档，表示无单一 handler 定义；schema v4 起该桶已归零，见 A.10。）

当前 `query: literal` 为 **0** 条：原先仅有的两条（join/knock）已改为类型化
`Query<ViaQuery>`（手写 `Deserialize` 的 `visit_map` 逐次 `push`，以满足重复 key），
因此现在是 `typed`，键集为 `via`。`literal` 机制本身仍保留给任何未来的动态提取器，
并由 `test_extract_registered.py` 的合成 probe（含 mutation#11）锁定。

| Method | Path | 提取器 | 可证明键 |
|---|---|---|---|
| `POST` | `/_matrix/client/v3/join/{room_id_or_alias}` | `Query<ViaQuery>`（手写 `Deserialize`） | `via`（可重复） |
| `POST` | `/_matrix/client/v3/knock/{room_id_or_alias}` | 同上 | `via`（可重复） |

说明：之所以手写 `Deserialize` 而不是 `#[derive(Deserialize)]`，是因为
`serde_urlencoded` 对 `Vec<String>` 字段遇到重复 key 会报 duplicate field；`visit_map`
里逐次 `push` 才能让 `?via=a&via=b` 成立，同时字段名对契约扫描器可见。
除 `via` 外的 query key 会被忽略（不报错）。SDK 只需保证 `viaServers` 以
`?via=a&via=b` 形式发送，不要发 `via[]=` 或 JSON。

### A.5 2026-10-20 语义补齐（`/notifications` 与 vendor keys）

#### 1. `GET /_matrix/client/v3/notifications`（B-7）

- **query**：`from`（cursor）、`limit`（默认 **50**，上限 **500**）、`only`（可重复）。
  - `limit` 非整数 / 负数 → 400 `M_INVALID_PARAM`；`> 500` 夹紧到 500。
  - `from` 为空串视为首页；格式非法 → 400 `M_INVALID_PARAM`。
  - `only` 支持三类，形如 `room_id:!a:b`、`type:m.room.message`、`sender:@u:s`；可重复（同类取并集）。
    缺 `:`、值为空、未知 key（`highlight` / `is_encrypted` / `is_one_to_one` / `contains_user_name:`）→ **400 `M_INVALID_PARAM`**（不再静默忽略）。
- **响应**：`{ "notifications": [...], "next_token": <string|null> }`
  - `next_token` 仅在**确认还有下一页**时非 null（内部按 `limit+1` 探测），可直接回填到下一次 `from`。
  - cursor 是不透明字符串（`"{ts}_{id}"` 的 keyset），调用方**不要解析或构造**，只回填。
- **翻页**：`while (next_token) { GET ?from=<next_token>&limit=... }`；空 `notifications` + `next_token=null` 才是真正的末页。

#### 2. `GET /_matrix/vendor/v1/rooms/{room_id}/keys/{event_id}`（C-1）

- 之前返回 `200 {"keys": []}`，客户端无法区分「没有密钥」与「没实现」。
- 现在返回 **501 `M_UNKNOWN`**，`error` 明确说明未实现；鉴权与房间可见性校验仍在返回 501 之前执行（无权限仍是 403/404）。
- SDK 侧请把该状态识别为 `Unsupported`，不要当成空结果。

### A.6 2026-10-20 admin 列表端点分页语义（B-10 / B-11 / B-12）

之前这批端点的 sidecar 要么是 `query: none`（没有 `Query` 提取器，SDK 发的 `from`/`limit` 被静默忽略），要么复用只认 `limit` 的 `SpamCheckQuery`（发 `from` 直接 400）。现已统一为「`limit` + 不透明 keyset 游标 `from`」协议。

**通用协议**（下列每条 `query_params=from,limit` 的端点都适用）：

- `limit`：默认 **100**，夹紧到 **1..=500**；非整数或负数 → **400 `M_INVALID_PARAM`**。
- `from`：**不透明游标**，只能原样回填上一页响应里的 `next_batch`；缺省或空串 = 首页。
  - cursor 形态非法（分段/整型解析失败）→ **400 `M_INVALID_PARAM`**（不再静默从头开始）。
  - 内部按 `limit + 1` 探测是否还有下一页，因此 `next_batch` **仅在确认有下一页时非 null**。
- 响应统一为对象：数组字段 + `total`（本页条数）+ `next_batch`（string \| null）。
- SDK 侧 `toPaginatedResult` 已认 `next_batch`，无需改代码；TS 类型补 `next_batch` 即可。

| Method | Path | `from` 游标语义 | 响应字段 | 排序 |
|---|---|---|---|---|
| `GET` | `/_synapse/admin/v1/modules/logs/{module_name}` | `"{executed_at}\|{id}"` | `logs` / `total` / `next_batch` | `executed_at DESC, id DESC` |
| `GET` | `/_synapse/admin/v1/modules/spam_check/sender/{sender}` | `"{checked_at}\|{id}"` | `checks` / `total` / `next_batch` | `checked_at DESC, id DESC` |
| `GET` | `/_synapse/admin/v1/spaces` | `"{created_ts}\|{space_id}"` | `spaces` / `total` / `next_batch` | `created_ts DESC, space_id DESC` |
| `GET` | `/_synapse/admin/v1/spaces/{space_id}/users` | 裸 `user_id`（**非**复合游标） | `users` / `total` / `next_batch` | `user_id ASC` |
| `GET` | `/_synapse/admin/v1/spaces/{space_id}/rooms` | 裸 `room_id`（**非**复合游标） | `rooms` / `total` / `next_batch` | `room_id ASC` |
| `GET` | `/_synapse/admin/v1/users/{user_id}/media` | `"{created_ts}\|{media_id}"`（与 `/media` 同源） | `media` / `total` / `next_batch` | `created_ts DESC, media_id DESC` |
| `GET` | `/_matrix/vendor/v1/keys/rotation/history/{device_id}` | `"{rotated_at}\|{id}"` | `device_id` / `rotations` / `next_batch` | `rotated_at DESC, id DESC` |

补充说明：

1. **B-10 / B-11**：`SpamCheckQuery` 已删除。`/modules`、`/modules/logs/{module_name}`、`/modules/spam_check/sender/{sender}` 现在共用同一个 `ListQuery { limit, from }`（`deny_unknown_fields`），两条列表路由都真做 keyset 翻页。响应由裸数组改为上表对象；旧 SDK 若按数组消费需同步改。
2. **`/spaces/{space_id}/users|rooms` 的 `from` 是裸标识符**，不是 `"{ts}|{id}"`。传 `"{ts}|{id}"` 形状会当作字面 id 参与比较（返回空页），因此**只回填 `next_batch`**。
3. **`/users/{user_id}/media` 的 `limit`/`from` 解析沿用历史宽松行为**：`limit` 非整数回落默认 100（不 400），`from` 形态非法视为首页（不 400）。这是为兼容既有调用方刻意保留的，其余端点均为严格 400。
4. **`GET /_synapse/admin/v1/room_stats` 不分页**：该端点返回全局概览对象（单条 `{...}`，不是列表），不接受 `from`/`limit`（契约 `query: none`）；携带这两个参数会被**静默忽略**（仍 200，不报错、结果不变），SDK 请勿再传；`GET /_synapse/admin/v1/room_stats/{room_id}` 同。该「接受但忽略」已登记为 `query_ignored=*`（§A.11），不再是散落的散文。
5. **`GET /_matrix/vendor/v1/rooms/{room_id}/message_queue` 不分页**：它是实时发送队列的**快照**（无稳定 tiebreaker），只接受 `limit`（默认 100，1..=500）；传 `from` 会因 `deny_unknown_fields` 返回 **400**，SDK 请删除该参数。

### A.7 2026-10-20 语义缺口修复（D-7 ~ D-12 + 联邦 `/publicRooms` 游标）

这批端点的共同症状是「**返回成功但没做事**」：解析器或 handler 把调用方真正发出的参数/body 丢掉，契约侧也看不出缺口。以下逐条给出修复后的 wire-format。

#### 1. `POST /_matrix/client/v3/register/guest`（D-10）

- 现在读取**可选** body：`{"device_id"?: string, "initial_device_display_name"?: string}`。
- 两个字段都缺省时行为不变（服务端分配 `guest_device_<n>` / 显示名 `Guest Device`）。
- 传入的 `device_id` 会原样登记到新游客账号上（E2EE 设备身份不再丢失）；`initial_device_display_name` 用于设备显示名。

#### 2. `POST /_matrix/client/v3/account/guest/upgrade`（D-11）

- 现在走 UIA：接受 `{"username", "password", "auth"?: {...}}`。
- 不带 `auth`（或 `auth` 未完成）→ **401** + `flows` / `params` / `session`，与 `/register` 同一套 UIA 方言。
- 当前可满足的 stage 只有 **`m.login.dummy`**（游客尚无密码/3PID，`m.login.password`、`m.login.email.identity` 无法完成）。
- 注意：此前 `auth` 字段被 `deny_unknown_fields` 判为未知字段 → 直接 400；现在不再报错。

#### 3. `POST /_synapse/admin/v1/event_reports/{id}/escalate`（D-12）

- 现在接受**可选** body：`{"reason"?: string}`，写入举报的 `resolution_reason`。
- 省略 body 仍然 200（保持向后兼容）；此前 body 完全不被读取。

#### 4. `GET /_matrix/client/{v1,v3}/rooms/{room_id}/context/{event_id}`（D-8）

- 新增 `filter` query 参数（JSON 编码的 `RoomEventFilter`，与 `/messages`、`/sync` 同一个类型）。
- 过滤是**后置**的：先按 `limit`（默认 10，夹紧 1..=100）取窗口，再按 `types` / `senders` / `not_types` / `not_senders` / `contains_url` 收窄。因此 `events_before` / `events_after` **可能少于** `limit` 条 —— 这是规范的允许行为，不要用长度判断是否还有更多。
- `filter` 含 `"lazy_load_members": true` 时，`state` 只返回相关发送者的 `m.room.member` 事件；不带该标志时 `state` 维持原有语义（仅与该事件相关的 state 子集）。
- `filter` 不是合法 JSON 对象 → **400 `M_BAD_JSON`**。

#### 5. `GET /_matrix/federation/v1/backfill/{room_id}`（D-9）

- 查询参数改为符合 S2S 规范的 `?v=<room_version>&from=<event_id>&limit=<n>`：
  - `v`：房间版本，**必填**，空串 → 400。
  - `from`：**事件 id**（不是时间戳、不是 id 列表），**必填**；响应从它**之前**开始且不含它。未知事件 id → **404**（以前会静默退化成「返回最新事件」，凭空造出一个调用方没要的分页窗口）。
  - `limit`：默认 **10**，夹紧 **1..=100**。
- 旧实现把 `v` 当成「事件 id 列表」、把 `from` 读进变量后丢弃，两者均已修正。契约侧该端点现在是 `query: from,limit,v`。

#### 6. 联邦 `/publicRooms` 游标（GET 与 POST 一致）

- `GET /_matrix/federation/v1/publicRooms?limit&since&include_all_networks&third_party_instance_id` 与 `POST /_matrix/federation/v1/publicRooms`（body `limit` / `since` / `filter`）现在都真正消费 `since`，并返回 `next_batch`。
- 游标与客户端 `/publicRooms` **同源**：`"{created_ts}|{room_id}"` 不透明字符串，只回填、不要解析。
- `limit` 统一默认 **20**，夹紧 **1..=1000**（GET 以前是 10 且无下限）。
- `next_batch` **仅在返回满页时非 null**；搜索路径（`filter.generic_search_term`，`ORDER BY name`）与 keyset 游标不同构，**不发** `next_batch`（续传会跳进未过滤的列表）。
- 响应同时带 `total_room_count_estimate`；POST 变体现在也返回 `next_batch`（此前响应体里根本没有这个字段）。

#### 7. 契约工具：`RawQuery` 不再被记成 `query: none`（D-7）

- `scripts/contract/extract_registered.py` 现在识别 `RawQuery` 提取器，把这类端点记为「**不可判定（opaque）**」而不是「没有 query」。
- 区分这两者很重要：`query: none` 断言「服务端不读任何 query」，而 `RawQuery` 只是「无法静态列出键名」。SDK 不要再把前者当成后者的证据。

### A.8 2026-10-20 收尾：D-13 / D-14 与 Space 查询能力边界（§L）

#### 1. `GET /_matrix/vendor/v1/rooms/{room_id}/event_perspective`（D-14，**行为变更**）

- 现在**必须**带 `?event_id=<事件 id>`：缺失 → **400**（`Query` 提取失败）。以前只按房间聚合，`event_id` / `room_version` 都被静默忽略。
- `?room_version=` 可选；传了服务端不支持的版本 → **400 `M_UNSUPPORTED_ROOM_VERSION`**。
- 响应由「房间级聚合」改为「**该事件的视角窗口**」：
  - 顶层平铺 anchor 事件的客户端 JSON：`event_id` / `sender` / `type` / `content` / `room_id` / `origin_server_ts` / `state_key` / `unsigned`；
  - 追加 `perspective` 对象：`{ "event_count": n, "latest_event_id": "…", "sender_activity": { "@u:h": n }, "event_types": { "m.room.message": n } }`（`latest_event_id` 即 anchor 的 `event_id`）；
  - 窗口 = anchor **之前** 99 条 + anchor 本身，`event_count` 含 anchor（最小 1）。
- anchor 同样受 history visibility 约束：调用方看不到的事件 → **404**（不泄漏窗口）。
- 契约侧该端点现为 `query: event_id,room_version`。

#### 2. `GET /_synapse/worker/v1/workers`、`GET /_synapse/worker/v1/workers/type/{worker_type}`（D-13，**形状不变**）

- 这两个列表**不分页**，返回**裸数组** `[ {…}, … ]`；`limit` / `from` 会被**静默忽略**（不 400，已登记为 `query_ignored=*`，见 §A.11）。
- 刻意保持数组形状：现有 SDK `worker-admin` 已按数组解构，改成 `{items,total}` 会让调用方再次静默拿到空列表。需要总量请用 `array.length`。

#### 3. worker 任务/命令列表响应形状修正（本轮横向排查，后端单侧）

- `GET /_synapse/worker/v1/tasks`：裸数组 → **`{"tasks": [...], "total": n}`**。
- `GET /_synapse/worker/v1/workers/{worker_id}/commands`：裸数组 → **`{"commands": [...], "total": n}`**。
- 原因：SDK 的 `WorkerManager.listTasks()` / `WorkerBodyManager.listPendingCommands()` 读的是对象字段；裸数组下它们永远得到 `[]`（静默）。SDK 无需改动。
- 两者均支持 `?limit=`（默认 100）。

#### 4. Space 查询能力边界（§L，文档项）

- `GET /_matrix/client/{v1,v3}/spaces/{space_id}/hierarchy`：**不分页**，只接受 `max_depth`（1..=20）；`HierarchyQuery` 是 `deny_unknown_fields`，传 `from` / `limit` / `since` / `suggested_only` / `server` / `search_term` → **400**。
- 需要分页请用 `GET /_matrix/client/v1/spaces/{space_id}/hierarchy/v1`（`from` / `limit` / `max_depth` / `suggested_only`）。
- 以下端点**没有 query 能力**（无 `Query` 提取器，携带任何 query 都被**静默忽略**，不 400；已逐条登记为 `query_ignored=*`，见 §A.11）：
  - `GET|POST /_matrix/vendor/v1/spaces/{space_id}/children`、`DELETE .../children/{room_id}`
  - `GET /_matrix/vendor/v1/spaces/{space_id}/summary`、`GET .../summary/with_children`
  - `GET /_matrix/vendor/v1/spaces/{space_id}/tree_path`
  - `GET /_matrix/vendor/v1/spaces/room/{room_id}/parents`
  - `GET /_matrix/{client/v1,vendor/v1}/rooms/{room_id}/summary`（MSC3266；同一 handler 的两个挂载点。
    SDK 的 `room-summary` 读路径仍会带 `?via=…`，属历史遗留 —— 服务端接受并丢弃，已登记 `query_ignored=*`）
- 若未来要给它们加 `max_depth` / 过滤，必须同时更新 sidecar，避免 SDK 只能靠猜。

### A.9 2026-10-20 语义缺口修复（B-15 / B-16 / B-17 / B-18）

#### 1. `POST /_matrix/client/v3/rooms/{room_id}/unban` 接受 `reason`（B-15）

- body：`{ "user_id": "...", "reason": "..." }`，`reason` **可选**（`serde(default)`）。
- 写入 `m.room.member` 的 leave content：`{"membership":"leave","reason":"…"}`；未传时与 kick 一致写 `""`（不是省略）。
- 长度/内容校验与 ban / kick 共用同一套（超限 → 400 `M_INVALID_PARAM`）。
- 兼容性：旧客户端不传 `reason` → 行为不变。

#### 2. `POST /_matrix/client/v3/rooms/{room_id}/upgrade` 接受 `additional_creators`（B-16，MSC4289）

- body：`{ "new_version": "12", "additional_creators": ["@a:h"] }`，`additional_creators` **可选**。
- 每个元素按 user ID 语法校验，不合法 → **400**（在创建任何房间之前失败）。
- **仅当目标房间版本实现了该规则（v12+）**才写入替代房间 `m.room.create` 的 `additional_creators`；旧版本目标会**丢弃**该字段（写入旧版本 create 事件等于制造不可解释的状态）。当前 `SUPPORTED_ROOM_VERSIONS` 只有 v12，故可创建目标均生效。
- 回源语义：替代房间由服务端 `create_room` 创建，发起人始终在 creator 集合内；`additional_creators` 只是**追加**。
- 兼容性：不传 → 行为不变。

#### 3. `GET /_matrix/client/v1/rooms/{room_id}/hierarchy` 的 query 与 v3 对齐（B-18）

- v1 现在接受 `from` / `limit` / `max_depth` / `suggested_only`，与 v3 同一集合；此前只认 `limit` / `max_depth`，`suggested_only` / `from` 被**静默丢弃**（过滤失效、分页永远第一页）。
- 默认值差异**保留**：v1 的 `max_depth` 缺省为 **3**，v3 为 **1**（历史行为，未改动）。
- 契约侧该端点现为 `query: from,limit,max_depth,suggested_only`。

#### 4. `GET|POST /_matrix/client/v3/publicRooms` 的 `server` 实现 relay（B-17）

- 两个变体都接受 `server`（GET 在 query，POST 在 body）；此前该规范字段被**静默忽略**，永远返回本机目录。
- 语义对齐上游 `RoomListRestServlet.get_remote_public_room_list`：
  1. `server` 先做结构校验（空 / 超长 / 含 `/`、`\`、空字符 → 400 `M_INVALID_PARAM`）；
  2. `server` 与本机 `server_name` 相同（大小写不敏感）→ 视作本机目录，不转发（避免自环）；
  3. 远端目标 + **未认证** → 401 `M_MISSING_TOKEN`：匿名调用者不得让本机代签出站联邦请求；
  4. 命中本机联邦黑名单 / 隔离区（`FederationBlacklistService::check_server`）→ 403 `M_FORBIDDEN`。
- 出站请求走本机 federation client（`/_matrix/federation/v1/publicRooms`，由客户端签名），并受其既有 SSRF 防线约束（拒绝 IP 字面量与私有网段）。
- 转发字段：GET 转发 `limit` / `since`；POST 转发 `limit` / `since` / `filter` / `include_all_networks` / `third_party_instance_id`。响应体原样返回（联邦 `/publicRooms` 与客户端变体同构）。
- 契约侧：`GET .../publicRooms` 的 query 现为 `limit,server,since`；POST 的 `server` 在 body 中（body 不逐字段登记）。

### A.10 2026-10-21 sidecar schema v4：per-route `profile` 与显式归属

`ROUTE_HANDLERS.json` 的 `schema_version` 由 **3 → 4**，纯增量：新增两个字段，既有字段语义不变，v3 消费者可继续按原字段读取。

#### 1. `profile`：每条路由的**边际**部署档位（P1-4）

| 取值 | 含义 |
|---|---|
| `[]` | 默认部署即可达（`always` 路由器） |
| `["worker"]` | 仅当合并 `worker_enabled` 路由器（`worker.enabled = true`）时可达 |
| `["oidc"]` | 仅当合并 `oidc_enabled` 路由器（`oidc.enabled = true`）时可达 |

- 推导自提取器的 `guards`，其累积集合是 `default ⊆ worker ⊆ all`；本字段发布的是**边际**（最小可服务档位），因此与本文档 §「运行时 Profile 门控」的 19 条门控路由（worker 11 + oidc 8）逐条对应。
- 实测分布（2026-10-24）：`"" = 960` / `worker = 11` / `oidc = 8`，合计 **979**（= `ROUTE_HANDLERS.json` 的 `counts.profile`）。
- 下游用途：端点生成器/SDK 门禁可直接过滤掉默认部署下必然 404 的 worker / oidc 路由，不再需要自己解析 `derived_route_table_*.inc.rs`。
- 守卫：某条路由若不属于三个档位中的任何一档（被一个没有任何部署会合并的 cfg 守卫包住），生成器**直接失败**而不是发布 `[]`（那等于谎称可达）。

#### 2. `attribution` / `attribution_note`：归属是「推导」还是「钉死」（P2-10）

- `derived`（976 条）：`file` / `handler` 由注册点静态解析得到。
- `explicit`（3 条）：解析器**按构造**无法给出唯一定义，在 `gen_route_handlers.py` 的 `EXPLICIT_ROUTE_HANDLERS` 中带证据钉死，`attribution_note` 说明原因：

| Method | Path | `file` / `handler` | 原因 |
|---|---|---|---|
| `GET` | `/` | `assembly.rs` / `create_router` | 服务横幅是 `create_router` 里的**内联闭包**，没有命名函数 |
| `GET` | `/.well-known/jwks.json` | `oidc/builtin.rs` / `jwks` | 主 router 与 fallback router 双注册（`jwks_fallback` 为关闭 builtin-oidc 时的同路径变体） |
| `GET` | `/.well-known/openid-configuration` | `oidc/builtin.rs` / `openid_discovery` | 同上（`get_openid_configuration` 为 fallback 变体） |

守卫：钉死的路径若已不再注册，生成器失败（stale pin 不允许静默保留）。

#### 3. 结果：`status` / `confidence` 不再有不确定档

- `status`：`exact = 979`（`ambiguous` 0、`unresolved` 0）；`confidence`：`high = 979`（`low` 0、`none` 0）。数字取自 `ROUTE_HANDLERS.json` 的 `counts.status` / `counts.confidence`（2026-10-24）。
- 因此 `ledger_annotations.txt` 的 `dynamic=true` 清单与 `EXPECTED_ANNOTATIONS` 中的对应项**已清空**（原唯一成员就是 `GET /`）。`check_dynamic_allowlist()` 的双向相等约束保留，空集是预期稳态。
- `body` / `query` 的 `ambiguous` 桶随之归零（2026-10-24 实测：`body` `none` 658 / `opaque` 80 / `typed` 226 / `foreign` 15；`query` `typed` 190 / `none` 789 —— 均取自 `counts`）。

### A.11 2026-10-21 sidecar schema v5：`query_unknown` / `query_ignored`（P2-7 + P2-8）

`ROUTE_HANDLERS.json` 的 `schema_version` 由 **4 → 5**，仍是纯增量：新增一个 per-route 字段与一个 ledger 镜像字段，既有字段语义不变。

#### 1. `query_unknown`：未知 query 名是 400 还是静默丢弃

对每条路由发布 `reject` / `ignore` / `null`，由解析器读 `Query<..>` 结构体是否带 `#[serde(deny_unknown_fields)]` 得出：

| 取值 | 含义 |
|---|---|
| `reject` | 未知名字 → **400**（结构体开了 `deny_unknown_fields`） |
| `ignore` | 未知名字被 serde 静默丢弃 → **200，结果不变** |
| `null` | 无可判定对象：无 `Query` 提取器，或 handler 无法静态定位 |

> 各桶条数**不在本文手抄**。唯一出处是
> `python3 scripts/contract/check_query_unknown_decisions.py` 的输出（它同时校验
> `scripts/contract/query_unknown_decisions.txt` 与 sidecar 双向一致）。任何写进
> 散文的数字都会在下一轮改动后变成第二个口径。

- 语义边界：`literal` / `opaque`（动态提取器）恒为 `ignore`——handler 自己挑键，serde 不参与拒绝。
- **本字段只报告推导结果，裁决另存一处**：给 113 个 Query 结构体统一加
  `deny_unknown_fields` 会让任何多传参数的既有客户端从 200 变 400，属破坏性变更
  （裁决见 §A.11.3）。因此「应该是 reject 还是 ignore」不写在本文，而是逐条写进
  `scripts/contract/query_unknown_decisions.txt`（每个 typed 路由一行：裁决 + 理由类 +
  结构体 + 备注），由 `check_query_unknown_decisions.py` 强制与 sidecar 一致并接入
  `check_route_contract.sh`。

#### 1a. 2026-10-21 更正：旧版「`reject = 0`」是度量缺陷，不是代码现状

P2-7 首次发布 `query_unknown` 时给出 `reject = 0 / ignore = 201 / null = 847`，并据此写下
「整个路由树里没有任何一个 `Query<T>` 开了 `deny_unknown_fields`」。**该结论是错的。**

根因在 `scripts/contract/extract_registered.py` 的 `_attrs_before()`：它从 `struct`
关键字**往前**逐 token 扫属性，遇到 `pub` 就判定「前面没有属性」并返回空表。于是
**所有 `pub struct` 的容器级 `#[serde(..)]` 全部丢失**——`parse_struct_deny_unknown`
恒为 `False`、`rename_all` 一并丢失。修法是让 `_attrs_before` 跳过可见性修饰
（`pub` / `pub(crate)` / `pub(super)` / `pub(in path)`）后继续扫，并在
`scripts/contract/test_extract_registered.py` 加 `check_struct_container_attrs()`
（6 个 deny 夹具 + `rename_all` 夹具 + 一条端到端 `handler_query_unknown == "reject"`）。

修复后重生成 sidecar：**路由集不变、`query_params` 零漂移、`body_type` 零漂移**，
203 条 typed 路由里有一批自动从 `ignore` 变成 `reject`——它们本来就是 `reject`，
只是度量看不见。所以「本轮新增了 reject 桶」的说法也不准确：**桶一直在，是尺子坏了**。

#### 1b. 逐条裁决的结果：只收紧项目私有的内部端点

`scripts/contract/query_unknown_decisions.txt` 给每条 typed 路由定了一个裁决与理由类。
理由类分两族：

| 裁决 | 理由类 | 适用面 |
|---|---|---|
| `reject` | `already-deny` | 早于本轮就带 `deny_unknown_fields` 的 struct |
| `reject` | `internal-strict` | 项目私有端点（worker 协议 / 内部 admin 触发器），无外部客户端 |
| `ignore` | `matrix-cs-ignore` | C-S 协议：规范要求忽略未知 query |
| `ignore` | `vendor-open` | `_matrix/vendor` 扩展，已部署客户端要能跨服务端版本存活 |
| `ignore` | `federation-forward-compat` | S2S：对端版本未知 |
| `ignore` | `upstream-admin-parity` | 对齐上游 Synapse admin，上游本就在忽略 |
| `ignore` | `external-idp` | CAS 重定向参数由外部 IdP 决定 |
| `ignore` | `shared-struct` | 结构体与 client 面路由共用，收紧会改 client API |
| `ignore` | `manual-deserialize` | 手写 `Deserialize`，serde 属性是死代码且会让 sidecar 说谎 |
| `ignore` | `compat-alias` | 有明确 back-compat 别名，代码注释已声明保持开放 |

本轮唯一从 `ignore` 转成 `reject` 的是 `internal-strict` 这一类：

| 结构体 | 路由 | 依据 |
|---|---|---|
| `worker::QueryLimit` | `GET /_synapse/worker/v1/statistics`、`/tasks`、`/workers/{worker_id}/commands` | 项目自有 worker 协议；仓内调用方（`src/worker/heartbeat.rs`、`examples/worker_demo.rs`）不带额外参数 |
| `worker::QueryStream` | `GET /_synapse/worker/v1/events` | 同上 |
| `worker::QueryPosition` | `GET /_synapse/worker/v1/replication/{worker_id}/position` | 同上 |
| `room_summary::QueryLimit` | `POST /_synapse/room_summary/v1/updates/process` | 全仓无 Rust/REST 调用方，仅 `AdminUser` 触发的内部作业 |

其余保持 `ignore`，理由逐条写在登记表的最后一列。**没有一刀切**：`shared-struct`
与 `manual-deserialize` 两类是「想拒也不能拒」，`compat-alias` 是「代码已明确决定不拒」。

#### 2. `query_ignored`：把「接受但忽略」从散文变成可校验登记

`ledger_annotations.txt` 新增注解键 `query_ignored`，sidecar 逐条镜像（`null` 表示未声明）。两种形状：

| 形状 | 含义 | 今日条数 |
|---|---|---|
| `*` | handler **没有** `Query` 提取器，任何 query 串都是 no-op | 11 |
| `name,name` | 有 `Query<Struct>`，但名字不在结构体字段里，被 serde 丢弃 | 2（两条 `/threads` 的 `filter`） |

登记清单：

| Method | Path | 声明 |
|---|---|---|
| `GET` | `/_synapse/admin/v1/room_stats`、`.../room_stats/{room_id}` | `*` |
| `GET` | `/_synapse/worker/v1/workers`、`.../workers/type/{worker_type}` | `*` |
| `GET` | `/_matrix/vendor/v1/spaces/{space_id}/children` | `*` |
| `POST` | `/_matrix/vendor/v1/spaces/{space_id}/children` | `*` |
| `DELETE` | `/_matrix/vendor/v1/spaces/{space_id}/children/{room_id}` | `*` |
| `GET` | `/_matrix/vendor/v1/spaces/{space_id}/summary`、`.../summary/with_children` | `*` |
| `GET` | `/_matrix/vendor/v1/spaces/{space_id}/tree_path` | `*` |
| `GET` | `/_matrix/vendor/v1/spaces/room/{room_id}/parents` | `*` |
| `GET` | `/_matrix/client/v1/rooms/{room_id}/threads`、`/_matrix/vendor/v1/threads` | `filter` |

#### 3. 守卫：`check_annotation_fidelity` 第 7 层

P2-7 的原始形态（「`query_params` 非空则 handler 必须有 `Query<`」）是同义反复——`query_params` 本就由 handler 推导。真正缺失的是**「接受但忽略」的机器登记**，所以守卫改为三条可证伪断言：

1. 声明 `*` 的路由，其 handler 若长出 `Query<..>` 提取器 → **RED**（声明已过期，应改登记 `query_params`）。
2. 声明具名的路由，该名字若出现在结构体字段里 → **RED**（已被解析，不是忽略）。
3. 声明具名的路由，其结构体若开了 `deny_unknown_fields` → **RED**（那是 400，不是忽略）。

三条都读 handler 源码，因此这份清单无法腐化成散文：新增端点不能「声明了却不读」，改写 handler 也不能让旧登记静默失效。注解表同时与 `EXPECTED_ANNOTATIONS` 双向相等（第 1 层），ledger 与常量必须同步改。

### A.12 2026-10-21 body 收编：E2EE 三条 + sliding sync（P1-5）

`counts.body` 由 `{opaque: 99, foreign: 19, typed: 240}` 变为 **`{opaque: 93, foreign: 15, typed: 250}`**。没有任何路由语义改变，只把 `serde_json::Value` 换成声明了字段名的结构体，让契约拿到字段集。

#### 1. 收编的 10 条路由

| 路由 | 原 `body_type` | 现 `body_type` |
|---|---|---|
| `POST /_matrix/client/{v1,v3}/keys/query` | `opaque` | `KeysQueryBody` |
| `POST /_matrix/client/{v1,v3}/keys/claim` | `opaque` | `KeysClaimBody` |
| `POST /_matrix/client/{v1,v3}/keys/device_signing/upload` | `opaque` | `DeviceSigningUploadBody` |
| `POST /_matrix/client/{v1,v4}/sync` + `unstable/org.matrix.msc3575/sync` + `unstable/org.matrix.simplified_msc3575/sync` | `foreign` | `SlidingSyncRequest` |

`/keys/query`、`/keys/claim`、`/keys/device_signing/upload` 是 E2EE 最关键的上传/查询入口——字段名写错过去只会在运行期静默失败，现在是可静态校验的 `typed`。

#### 2. 刻意**不**开 `deny_unknown_fields`

四条新结构体都不带 `#[serde(deny_unknown_fields)]`：

- Matrix 规范要求端点忽略不认识的字段，开 `deny` 会让「多传一个字段」从 200 变 400，直接打断客户端 E2EE 初始化；
- 与 §A.11.3「只登记不改行为」的裁决一致——body 的未知字段策略不在本轮变更范围内。

因此这 10 条路由的 `query_unknown` 语义不受影响（这是 body，不是 query）。

#### 3. 内层 `Value` 仍然是 `Value`

`KeysQueryBody.device_keys`、`KeysClaimBody.one_time_keys`、`DeviceSigningUploadBody` 的三个 key 对象都是 `Option<Value>`/`Value`：整张 per-user / per-device map 是**原样转发**给 `device_keys_service` 的（`/keys/query` 的死循环根因即来自提前裁剪这张 map），拆成 typed 结构会改变转发语义。收编的目标是**顶层字段名可校验**，不是重写转发路径。

#### 4. `foreign` 的消除机制：`EXTERNAL_TYPE_FILES`

`SlidingSyncRequest` 定义在 `synapse-storage/src/sliding_sync/models.rs`（由 `synapse-services` 再导出），不在路由树的扫描面内，因此过去只能报 `foreign`。解析器新增常量：

```python
EXTERNAL_TYPE_FILES = ("synapse-storage/src/sliding_sync/models.rs",)
```

它只喂**类型索引**（`query_structs` / `query_struct_deny`），不喂 handler 索引（`fn_all`）；且与路由树同名的结构体会被跳过——否则重名会让 `lookup_struct_fields` 读到多个定义而退回「不可判定」，静默变回 `foreign`。

#### 5. 剩余 15 条 `foreign` 与 93 条 `opaque`

`foreign` 剩下的是类型定义在扫描面之外且未纳入 `EXTERNAL_TYPE_FILES` 的（`m.call.*` 事件、feature-flags、room summary、one-time invite、worker replication 等）；`opaque` 剩下的是 `Value`/`Bytes` 或手动 `from_value` 的端点（`e2ee/devices.rs`、`key_backup.rs`、`federation/keys.rs`、`handlers/room/state.rs`、`account_data.rs` 等）。继续收编按 SDK 实际调用面排序，每收编一条，SDK 门禁的 `backend-body-opaque` 桶自动减一。

---

*本文件由 `scripts/contract/extract_registered.py` + `gen_contract_doc.py` 生成。路由面随代码变化，请定期重新生成。附录 A / 附录 B 为人工维护增补。*

---

## 附录 B — SDK 封装完整性审计（matrix-js-sdk，2026-10-09）

> 本附录为人工维护，基于 **SDK Manager 源码 + 后端 handler 源码双侧实读**，逐条核对「已封装后端功能」的完整性；重新生成脚本不覆盖本段。
> **审计对象**：`@langkebo/matrix-js-sdk@40.2.0-langkebo.5`（`/Users/ljf/Desktop/hu_ts/matrix-js-sdk`）。
> **后端事实源**：本文件路由面（**979** 条注册条目；另两个口径 779 / 760 见「总览」）+ `synapse-web/src/routes/**` handler 源码。
> 　· 计数口径（2026-10-24 复核）：**979** = `all` profile 去重后的 `method + path` 条目数，三处独立读数一致 ——
> 　　`tests/unit/fixtures/ledger_export_sdk/all.json` 的 `entry_count`、`docs/synapse-rust/ROUTE_HANDLERS.json` 的 `counts.routes`、
> 　　以及 `derived_route_table_*.inc.rs` 的 981 行去重结果（960 `always` + 11 `worker` + 10 `oidc`）。
> 　· **模块口径有三种，混用就会互相"打假"**（2026-10-24 逐一实跑复核）：
> 　　① **生成器口径**（本文件「总览」那两行用的就是它）：`artifacts/registered_routes.json["modules"]` 中**有路由的模块** = **66**；
> 　　　`derived_routes.rs` + `derived_route_table_*.inc.rs` 的 `RouteEntry::new(…, "<label>")` 去重 = **75** 个标签；
> 　　② **sidecar 口径**（`ROUTE_HANDLERS.json` 现算）：源文件 `file` **113** / `registered_by` **74** / 其首段（模块）**51**；
> 　　③ 两者只差 **1** 个标签：`oidc_fallback` —— 它是 `/.well-known/{jwks.json,openid-configuration}` 的
> 　　　**always 孪生**合成标签（见 `derived_routes.rs` 头部注释：同一路径在 `Oidc` 车道另有一条，cfg 裁掉后由它兜底），
> 　　　sidecar 把这两条路径记在 `oidc` 名下，所以不含这个字面量。
> 　　⇒ **66/75 与 113/74/51 都是可复现的**，不是"算不出来"，引用时必须写明口径。
> 　· 复算（一次跑完三种口径；`artifacts/registered_routes.json` 未受版本控制，缺失时先跑 `python3 scripts/contract/extract_registered.py`）：
>
> ```bash
> python3 - <<'PY'
> import glob, json, re
> mods = json.load(open("artifacts/registered_routes.json"))["modules"]
> print("生成器口径 模块:", sum(1 for v in mods.values() if v))
> pat = re.compile(r'RouteEntry::new\(\s*axum::http::Method::[A-Z]+,\s*"[^"]*",\s*"([^"]+)"')
> files = ["synapse-web/src/routes/derived_routes.rs",
>          *sorted(glob.glob("synapse-web/src/routes/derived_route_table*.inc.rs"))]
> print("生成器口径 标签:", len({m for p in files for m in pat.findall(open(p).read())}))
> r = json.load(open("docs/synapse-rust/ROUTE_HANDLERS.json"))["routes"]
> print("sidecar 口径:", len(r), len({x["file"] for x in r}),
>       len({x["registered_by"] for x in r}), len({x["registered_by"].split("::")[0] for x in r}))
> PY
> # 实测：66 / 75 / 979 113 74 51
> ```
> **证据产物**：`artifacts/sdk-contract-gap-report.md`、`artifacts/sdk-contract-gap.json`、`scripts/quality/path-contract-coverage.json`；镜像 commit **`2946e19eb`**（2026-10-17）。
> ⚠️ **时点提示**：本节 B.1–B.7 的**所有数字**（覆盖率、分桶、缺口条数）都是 **2026-10-09 审计时点**的口径，此后后端与 SDK 均有前进；
> 　　需要当前值请以 `compare-routes.mjs` 的当次输出为准。**结论层面的复核见 B.8（2026-10-17）**。

### B.0 审计方法与口径

- **三级证据法**（`scripts/audit/compare-routes.mjs`）：
  - **L1 声明面**：SDK `src/**/__generated__/route-table.ts` 生成的路由声明（弱，仅代表「约定」）。
  - **L2 调用面**：Manager 中 `(prefix, path)` 的**真实调用点**（最强证据，构成主指标）。
  - **L3 构造面**：路径字符串构造器字面量（弱，30 候选前缀命中，不校验 method）。
- **双口径覆盖率**（2026-10-09 时点）：声明面覆盖率 = 732/1025 = **71.4%**；实现面覆盖率 = 579/644 = **89.9%**（**主指标**）。
  - 2026-10-17 复核值：声明面 983/1033 ≈ **95.2%**、实现面 589/641 ≈ **91.9%**（口径与分桶见 B.8）。
- **分桶**（2026-10-09 时点）：T1_CALLSITE 331 / T2_CONSTRUCTOR 563 / T3_DECLARED_ONLY 70 / DRIFT 0 / CONDITIONAL 1 / GAP 65。
- **scope 分类**（2026-10-09 时点）：CLIENT_FACING 644 / SERVER_ONLY 383 / ROOT_OR_SSO 2 / NON_NAMESPACED 1。
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
| M-02 | cas 服务列表 | `src/cas/index.ts:57-60`（`CasServiceListResponse { services: CasService[] }`）、`listServices` 调用 `:149-155` | `synapse-web/src/routes/cas.rs:332-336` `list_services` 返回**裸数组** `Vec<ServiceResponse>` | SDK 期望 `{services:[]}`，实际收到数组 → `response.services` 为 `undefined`，列表恒为空 | P1 |
| M-05 | thread 搜索 | `src/thread/index.ts:180-198`（`searchThreads` 发 query `term`） | `synapse-web/src/routes/handlers/thread.rs:53-57`（`SearchQuery { q: String, limit }`，`q` **必填**） | 后端读 `q`、缺 `q` 反序列化失败 → 搜索请求 400；`term` 被忽略 | P1 |
| M-06 | e2ee 密钥请求字段名 | `src/room-keys/index.ts:35-43`（`RoomKeyRequest.state`，值域 `pending/approved/rejected`） | `synapse-web/src/routes/e2ee/devices.rs:395-414`（`serialize_room_key_request` 输出 **`status`**，值域 `pending/cancelled/fulfilled`） | 字段名与值域双重不符 → 前端无法正确渲染密钥请求状态 | P1 |
| M-04 | thread 房间话题列表 | `src/thread/index.ts:136-152`（`getRoomThreads` 发 query `include`） | `synapse-web/src/routes/handlers/thread.rs`（`ListQuery` 读 `include`，见附录 D.2） | ~~`include` 被忽略，`include_all` 恒为默认~~ → **2026-10-20 已修复（B-20）**：`include=all\|participated` 真正生效，SQL 下推过滤 | ~~P2~~ ✅ |
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
| P-07 | cas proxy | `src/cas/index.ts:315-327`（`proxy` 发 query `targetService`） | `synapse-web/src/routes/cas.rs:59-62`（`ProxyQuery { target_service: String }`，**非 Option**） | 参数名不符且必填缺失 → 400 | P1 |
| P-09 | cas 注册服务 | `src/cas/index.ts:170-185`（`createService` body 缺 `service_id`、用 `service_url` 代 `service_url_pattern`）；响应类型 `:69-72` 期望 `{id, name}` | `cas.rs:87-96`（`RegisterServiceBody` 需 `service_id` + `name` + `service_url_pattern`）；响应 `cas.rs:106-112` 用 `service_id`/`is_enabled` | 请求缺必填 + 响应字段名不符，注册服务链路不可用 | P1 |
| P-10 | e2ee 创建密钥请求 | `src/room-keys/index.ts:49-53`（`CreateRoomKeyRequest` 仅 `room_id/session_id/device_id?`） | `synapse-web/src/routes/e2ee/devices.rs:376-382`（`CreateRoomKeyRequestBody` 需 `algorithm` + `room_id` + `session_id`） | 缺必填 `algorithm` → 400 | P1 |
| P-12 | friend_room 添加好友 | `src/friend/sub-managers/friend-request-manager.ts:135-152`（`addFriend` body 于 `:149` 发 `{user_id, reason}`） | `synapse-web/src/routes/friend_room.rs:123-129`（`AddFriendRequest`，`#[serde(deny_unknown_fields)]`，字段为 `{user_id, message}`） | `reason` 为未知字段 → **400**。对照：同文件 `sendFriendRequest`（`:97-120`）发的是**正确**的 `message`（`:114`） | P1 |
| P-08 | cas validate 系列 | `src/cas/index.ts:252-313`（`serviceValidate`/`proxyValidate`/`p3ServiceValidate` 发 query `pgtUrl`，见 `:261`/`:282`/`:303`） | `cas.rs:53`/`:69`/`:81`（读 `pgt_url`，均 `Option`） | 参数名不符 → 后端静默丢弃（无 400，`pgt_url` 恒为 None） | P2 |
| P-11 | e2ee 列表分页 | `src/device-keys/index.ts:398-414`（声明 `limit` 于 `:402`，但 params 仅写入 `status`/`room_id`/`session_id`，见 `:405-407`） | `synapse-web/src/routes/e2ee/devices.rs:338-351`（后端支持 `limit` 分页） | `limit` 参数从未下发 → 分页能力失效（静默） | P2 |

### B.5 响应解析异常（形状 / 包裹 / 序列化格式）

| # | 功能模块 | SDK 位置（期望） | 后端位置（实际） | 影响与证据 | 等级 |
|---|---|---|---|---|---|
| R-01 | thread 话题回复列表 | `src/thread/index.ts:388-406`（`getThreadReplies`）；响应类型 `IThreadRepliesResponse { replies[], next_batch? }`（`:107-110`） | `thread.rs:453-458`（`get_replies` 返回**裸数组** `Json<Vec<ReplyResponse>>`） | SDK 取 `.replies` 得 `undefined` → 回复列表恒空、分页丢失 | P1 |
| R-03 | cas 校验响应格式 | `src/cas/index.ts:98-107`（`CasServiceValidateResponse`/`CasProxyResponse`，按 **JSON** 解析） | `cas.rs:216-232`（`service_validate` 返回 **`text/plain`** `yes\n{user}\n`）；`proxy_validate`/`proxy`/`p3_service_validate`（`cas.rs:234/262/274`）返回 **XML** | JSON 解析非 JSON 响应 → 抛错；CAS 校验链路不可用 | P1 |
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
5. **口径提示**：`generate_sdk_ledger_fixtures.sh` 头部注释里的条数只是**示例历史值**，请以当次实测为准：**979**（2026-10-24；2026-10-22 A3 第二批后为 975；第一批后为 997；2026-10-20 时为 1048；2026-10-19 时为 1046、2026-10-18 时为 1041、2026-10-17 时为 1038、2026-10-09 时为 1030）；一律以本文件头部计数与 `sdk-contract-gap.json` 当次实测为准。

### B.8 复核结论（2026-10-17，双侧源码 + 真实后端联调）

> 复核口径：后端 `2946e19eb`、SDK `@langkebo/matrix-js-sdk`，逐条回源 SDK 方法体与后端 handler 提取器，**不接受仅凭字符串命中判定已修**。

**B.2–B.5 的 27 条：26 条已在 SDK 侧修复，1 条待产品决策。**

| 分组 | 条数 | 结论 |
|---|---:|---|
| B.2 功能遗漏（GAP） | 3 | G-01（`appservice/fed_proxy` 通配代理）**仍未封装且待产品决策**；其余 2 条已随本轮封装修复 |
| B.3 接口不匹配（路径/契约） | 6 | 全部已修 |
| B.4 参数传递错误（body/query） | 12 | 全部已修（`createThread.name` 见下条说明） |
| B.5 响应解析异常（形状/格式） | 6 | 全部已修（`getThreadReplies`/`getThread` 已适配裸响应形状；CAS 已改 `text/plain`/XML 解析；分块上传字段名已对齐；密钥请求列表已补 `next_batch`） |

**复核后需保留的两条已知语义缺陷（非"未修"，是修复不彻底）**：

1. `createThread` / `createGlobalThread` 的 `name` 参数**仍无效，但已由静默丢弃改为显式拒绝** —— 后端 `CreateThreadBody.content` 不再声明为 `_content`；线程根事件已存在（`root_event_id`），其 content 由该事件定义，`thread_roots` 无内容列，因此非空 `content` 现在返回 **400 `M_INVALID_PARAM`**（缺省 / `null` / `{}` 仍放行）。SDK 需删除 `name` 参数或改为发送根事件本身的内容（避免破坏公开 API 时可先保留并在 JSDoc 标注「会 400」）。
2. `markThreadRead` 的 `origin_server_ts` 原固定发 `Date.now()`（客户端时钟）。SDK 已改为 **显式入参 > 本地缓存事件 `getTs()` > `Date.now()`** 的三级取值；后端 `MarkReadBody.origin_server_ts: i64` 语义未变。

**T3「仅声明」条目的处置口径（重要，勿当缺口清单）**：

2026-10-17 复核时 T3 = **79** 条。按本附录自带的「范围」列：`SERVER_ONLY` **24** / `CLIENT_FACING` 52 / `NON_NAMESPACED` 1 / `ROOT_OR_SSO` 2。客户端面 55 条逐条探证据后，「无调用点」有五种成因：

| 成因 | 代表 | 是否缺口 |
|---|---|---|
| 解析器盲区（实际已实现） | `key_backup` 的 `room_keys/keys` 全 18 条均命中 `src/key-backup/*.ts` | ❌ 否 |
| URL 构造器（设计上无调用点） | `media` 的 `r0`/`r1`/`v1`/`v3` `/download`、`download_signed` —— `getDownloadUrl()` 只拼链接给 `<img>` | ❌ 否 |
| 通用端点承载 | `m.call.*` 走 `sendEvent` | ❌ 否 |
| 尾斜杠 / 版本变体 | `pushrules/global/`（SDK 用无尾斜杠版）；`media/preview_url` 的 v1 变体 | ❌ 否 |
| 真未封 | `appservice/fed_proxy` × 2 | ⚠️ 待决策 |

⇒ **24 条连客户端面都不是；真正需要产品决策的只有 G-01。**

### B.9 真实后端联调发现的后端缺陷（2026-10-17，`https://matrix.test`）

> 联调方法：SDK 真实代码 + 原始 HTTP 探测 + DB 只读对拍（两侧各打一次，判定"谁错"）。
> 详细复现步骤与证据见 `matrix-js-sdk/artifacts/live-integration-test-report-2026-10-17.md`。
> 结论：主链路健康（75 个用例 74 通过），问题集中在**投影表一致性**与**错误响应规范性**。

| ID | 级别 | 问题 | 证据位置 |
|---|---|---|---|
| I-01 | **P1** | `/summaries` 的成员投影表 `room_summary_members` **未随 membership 维护**：join 不写（新加入房间在 `/summaries` 里看不到）、leave 不清理（user1 实测 49 条已 leave 房间仍在返回）。`INNER JOIN` 把缺失放大成静默数据丢失。 | `synapse-services/src/room/summary/service.rs:54`；`synapse-storage/src/room_summary/repository.rs:231-247`；`synapse-services/src/room/membership/actions.rs:64`（`join_room` 无任何 summary 调用） |
| I-02 | **P1** | 裸 `axum::extract::Json<T>` 的端点错误响应**缺 `errcode`**：`createRoom` 的 415 / 语法错 400 均返回 `text/plain`；`summaries/batch` 未知字段返回 **422 `text/plain`** 且泄漏内部字段名。`MatrixJson` 已正确实现但仅 17 处使用，裸 `Json` ≥60 处。 | `routes/handlers/room/management/create.rs:79`；对照 `routes/extractors/json.rs:16-35` |
| I-03 | P2 | 登录失败返回 **`401` + `M_FORBIDDEN`**：规范（`login.yaml`）规定失败为 **403 `M_FORBIDDEN`**，401 会让「通用 401 处理」误伤登录接口。~~且连续 3 次失败无限流~~ → **2026-10-18 复核更正：限流存在**（第 6 次 429 + `retry_after_ms=300000`，阈值 `login_failure_lockout_threshold: 5`；原报告只试了 3 次）。 | `POST /_matrix/client/v3/login`；`synapse-services/src/auth/login.rs:53`；`synapse-web/src/utils/login_lockout.rs` |
| I-04 | P2 | `createRoom` 对 `{"name":123}` **静默接受并 200**（`Json<Value>` 无类型校验，非字符串被跳过）。 | `create.rs` |
| I-05 | P3 | `/summaries` 的 `INNER JOIN room_summary_members` 使"投影无行"变成永久不可见（无告警、无回退）。 | `repository.rs:237` |

**已核实无问题**（防复报）：401 三态（`M_MISSING_TOKEN` / `M_UNKNOWN_TOKEN` / 200）、403 admin 面、404 分 `M_UNRECOGNIZED` 与 `M_NOT_FOUND`、405、`400 M_INVALID_PARAM`（非法用户/房间 ID）、非成员读单房间 `/summary` 返回 403、`M_UIA_REQUIRED` 注销流程。

**一处需澄清的"疑似越权"**：普通用户 `/summaries` 返回的 12 条非 joined 房间，经 `room_memberships` 对拍确认全部是 **invite**（invite 用户看摘要符合语义），**不构成越权**；越权判据必须对拍成员关系，不能只看列表内容。

**修复优先级**：I-01（同事务写投影 + 回填脚本 + DB 一致性测试）→ I-02（全局 `JsonRejection → ApiError` 或批量换 `MatrixJson` + 契约测试）→ I-03/I-04 → I-05。

**2026-10-18 复核（`docs/audit/LIVE_INTEGRATION_BACKEND_FINDINGS_AUDIT_2026-10-18.md`）**：
上表 5 条逐条重跑了一遍（代码 + 活体 + DB 对拍），**4 条成立、I-03 部分成立**。更正与补充：

- **I-03 的「不限流」不成立**：登录锁定是好的（`(ip, username)` 5 次 / IP 30 次 / 300 s），
  实测第 6 次即 429；原报告只有 3 次取样。但「401 vs 规范的 403」成立，且**被单元测试钉死**
  （`synapse-services/src/auth/tests.rs` 里断言 `kind=Unauthorized` 的用例），改的时候要同步改断言与注释。
- **I-01 是双向错的**：除 leave 残留（投影多 71/49/14 条）外，join 也不写投影
  （真实有、投影无 3 条，其中 1 条为本轮活体造出）；且 `kick`/`ban` 同样没有投影维护
  （`moderation.rs` 只有两处 invite 写投影）。
- **I-02 规模低估约 4 倍**：裸 `Json` 提取器实测 **254 处 / 82 文件**，`MatrixJson` 22 处。
- **I-04 同源补一条**：`createRoom` 的未知字段也被静默接受（`Json<Value>` 既无类型校验也无未知字段拒绝）。

## 附录 C — purge_history 与上游 Synapse 的已知差异（2026-10-19）

本节为人工维护的**实现口径说明**，重新生成契约时原样保留。上游参考：Synapse
`POST /_synapse/admin/v1/purge_history/{room_id}` + `GET /_synapse/admin/v1/purge_history_status/{purge_id}`。

### C.1 端点

| 方法 | 路径 | 请求体 | handler |
|---|---|---|---|
| POST | `/_synapse/admin/v1/purge_history` | `room_id` + 截断点 | `synapse-web/src/routes/admin/room/management.rs` `purge_history` |
| POST | `/_synapse/admin/v1/rooms/{room_id}/purge_history` | 仅截断点（房间取自路径） | 同上 `purge_history_by_room` |

### C.2 请求字段

| 字段 | 类型 | 必填 | 本项目行为 |
|---|---|---|---|
| `room_id` | string | 仅无路径版本必填 | 缺失 → 400 |
| `purge_up_to_ts` | integer（毫秒时间戳） | 与 `purge_up_to_event_id` 二选一 | 直接作为截断点 |
| `purge_up_to_event_id` | string | 与 `purge_up_to_ts` 二选一 | 解析该事件的 `origin_server_ts` 作为截断点；事件不存在或不属于该房间 → 404 `M_NOT_FOUND` |
| `delete_local_events` | boolean（默认 false） | 否 | **接受但不生效**：仅记录告警日志，实际仍只清远端/联邦事件 |
| `dry_run` | boolean（默认 false） | 否 | 只统计不删除（本项目扩展字段，上游无此字段） |

校验顺序：`room_id` 必填(400) → 截断点互斥/必填(400) → 房间存在(404) → 事件解析(404)。

**截断点口径（与上游一致）**：必须显式提供 `purge_up_to_ts` 或 `purge_up_to_event_id` 之一；
两者同时提供 → 400。不再使用早期的「缺省 = 当前时间 − 30 天」隐式默认值。

### C.3 与上游的行为差异

1. **同步实现**：请求在事务内同步完成删除后才返回，**没有** `purge_id`，也**没有**
   `GET /_synapse/admin/v1/purge_history_status/{purge_id}` 轮询端点。上游返回
   `{"purge_id": "..."}`，本项目返回如下响应，调用方不能按上游流程轮询状态。
   ```json
   { "success": true, "deleted_events": 42, "dry_run": false }
   ```
2. **只删远端事件**：无论 `delete_local_events` 取值如何，SQL 始终限定
   `origin != 'self'`，且永远保留 `m.room.create`；`delete_local_events=true` 只在日志中告警。
3. **截断点语义**：删除 `origin_server_ts < 截断点` 的事件（严格小于），与上游一致。
4. **路径形态**：上游带房间的版本是 `/_synapse/admin/v1/purge_history/{room_id}`，本项目为
   `/_synapse/admin/v1/rooms/{room_id}/purge_history`；按上游路径调用会得到 404。
   上游兼容调用请改用 `/_synapse/admin/v1/purge_history` 并在 body 中传 `room_id`。

## 附录 D — 请求参数契约修复（2026-10-20，B-19 / B-20 / B-21）

本节记录 2026-10-19 后端缺口台账 §N 三条的落地口径，**同为人工维护段，重新生成契约时原样保留**。
三条的共同根因是「handler 声明了参数但没消费」或「消费了非规范参数名」，客户端侧表现为
**请求被静默接受却毫无效果**（没有错误码、没有日志）。

### D.1 B-19 管理面用户列表的 `from` 分页

| 端点 | `from` | `since` | `offset` | `limit` |
|---|---|---|---|---|
| `GET /_synapse/admin/v1/users` | ✅ 规范参数 | ✅ 历史别名（`from` 优先） | ⚠️ 仅接受 `0`；`>0` → 400 | ✅ |
| `GET /_synapse/admin/v2/users` | ✅（此前已支持） | ❌ 不声明 | ⚠️ 仅接受 `0`；`>0` → 400 | ✅ |

1. **游标形状与上游不同（重要）**：本项目 `from` 是 **keyset 游标** `"{created_ts}|{user_id}"`，
   即上一页响应里的 `next_batch`，客户端只需原样回传；上游 Synapse 的 `from` 是整数偏移量。
   把上游的 `"0"` / `"100"` 直接塞进 `from` 不会 400，而是被当作无法解析的游标 ⇒ 退化为第一页。
2. **`offset` 不再静默生效**：此前 `offset` 被解析但翻页结果错误（永远第一页）。现在 `offset>0`
   直接返回 **400**（`Legacy offset pagination is no longer supported; use the from cursor`），
   避免「200 + 错页」这种最难排查的形态。`offset=0` / 缺省视为未提供。
3. **`since` 保留为别名**：为兼容旧调用方，`from` 缺失时回退到 `since`；两者都给时只认 `from`。
4. **与台账原文的差异（事实修正）**：台账称「v1/v2 共用 `AdminUsersQuery`」不成立——v1 用
   `AdminUsersQuery`、v2 用 `AdminUsersV2Query`，且 **v2 早已接受 `from`**；本轮真正缺 `from`
   的只有 v1（`synapse-web/src/routes/admin/user.rs` 见两处结构体定义）。

### D.2 B-20 房间话题列表的 `include`

| 端点 | 接受的 query | `include` 是否生效 |
|---|---|---|
| `GET /_matrix/client/v1/rooms/{room_id}/threads` | `from`、`limit`、`include` | ✅ |
| `GET /_matrix/vendor/v1/threads` | `from`、`limit`、`include` | ✅ |
| `GET /_matrix/vendor/v1/user/{user_id}/rooms/{room_id}/threads` | `from`、`limit`、`include` | ✅ |
| `GET /_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/replies` | `from`、`limit`、`dir` | —（不声明，见下） |
| `GET /_matrix/vendor/v1/threads/subscribed` | `from`、`limit` | —（同上） |

1. **取值**：`include=all`（缺省，等价于不传）返回房间内全部话题；`include=participated`
   只返回**请求者本人参与过**的话题。其它取值 → **400 `M_INVALID_PARAM`**
   （不再被静默当成 `all`）。历史布尔参数 `include_all` **已移除且不保留别名**。
2. **「参与」的判据**：`thread_roots.participants`（根事件发送者 + 全部回复者的并集），
   因此「只发过回复、没发根事件」的用户也算参与。
3. **下推位置**：过滤在 SQL 内完成（`participants @> to_jsonb($n::TEXT)`），不是取出后内存过滤，
   `limit` 语义因此保持「本页最多 N 条 participated」。存储层见
   `synapse-storage/src/thread/storage.rs` 的 `list_thread_roots` 两个分支。
4. **`filter` 登记为「接受但忽略」**：它不是 threads 端点的规范参数，SDK 沿用
   `/messages` 的形状多传了它。服务端按 serde 默认行为忽略未知 query 参数，不报错。
   `dir` 则**已在本轮真实现**（话题列表见 E.14 §2，回复列表见 E.15），不再属于忽略项。
   契约侧把它登记为 `query_ignored=filter`（**不是** `query_params`）：`query_params` 只列
   结构体真正声明的字段，`query_ignored` 列「SDK 会发、服务端丢弃」的名字，两者由
   `check_annotation_fidelity` 的第 7 层分别对照 handler 源码校验（§A.11）。
5. **`/replies` 与 `/threads/subscribed` 不声明 `include`**：它们不消费该参数，因此提取器
   拆分结构体：`/replies` 登记 `dir,from,limit`（`RepliesQuery`），`/threads/subscribed`
   登记 `from,limit`（`ThreadPageQuery`）——这是刻意拆分，不是遗漏。

### D.3 B-21 `POST /rooms/{room_id}/invite` 的 `reason`

| 字段 | 类型 | 必填 | 行为 |
|---|---|---|---|
| `reason` | string | 否 | 原样写入 `m.room.member` 事件的 `content.reason`；缺省则**不产生该键**（与上游一致，不写空串占位） |

1. **三条路径都透传**：本地邀请（`moderation.rs` 直接拼 content）、联邦邀请
   （`federation.rs` 在**签名前**写入 invite PDU 的 content）、3PID 邀请分支
   （解析出 MXID 后走同一条本地/联邦路径）。
2. **长度上限 512 字节**：与 ban/kick 共用 `validate_moderation_reason`，超限 → **400**
   （错误文本 `Reason too long`）。空串合法。
3. **联邦路径的顺序约束**：`reason` 必须在 `sign_and_hash_event` **之前**写入 content——
   content 参与签名，签名后再补键会使本方签名自校验失败。
4. **入站邀请同样保留 `reason`**：远端推来的 `m.room.member`(invite) 按原事件持久化，
   不做剥离。

## 附录 E — 请求参数/响应契约修复（2026-10-20，B-22 ~ B-32 / D-15）

本节记录后端缺口台账 §O 共 12 条的落地口径，**同为人工维护段，重新生成契约时原样保留**。
与附录 D 同源，根因仍是三类：**参数声明但未消费**、**消费了非规范名**、**路由整体缺失**。
所有 `alias` 都只做「输入兼容」，响应侧只输出规范名。

### E.1 B-22 OIDC authorize 的 PKCE（RFC 7636）

端点：`GET /_matrix/client/v3/oidc/authorize`（`synapse-web/src/routes/oidc/provider.rs`）。

| 参数 | 必填 | 行为 |
|---|---|---|
| `response_type` | ✅ | 仅 `code`；其它 → 400 |
| `client_id` | ✅ | 已注册客户端按其 `redirect_uris` 校验；未注册客户端仅接受本服务器回调或 allowlist 内的 URI |
| `redirect_uri` | ✅ | 必须属于该 `client_id`，否则 400（否则是开放重定向） |
| `scope` | ❌ | 必须是配置 `scopes` 的子集 |
| `state` / `nonce` | ❌ | 缺省由服务端生成，并在响应中原样返回 |
| `code_challenge` | ❌ | 客户端自带时**必须**随后带 `code_verifier` 换 token |
| `code_challenge_method` | ❌ | 仅 `S256`；缺省按 `plain` 处理并拒绝（400 `M_INVALID_PARAM`） |

1. **不再下发 verifier（安全修复）**：响应固定为
   `{"authorization_url", "state", "nonce"}`，**没有** `code_verifier` 字段。此前下发 verifier
   等于让任何调用方都能在 token 端点冒充客户端，PKCE 形同虚设。
2. **challenge 长度校验**：43–128 个 unreserved 字符，越界 → 400。
3. **服务端代生成仅限机密客户端**：只有请求里**完全没有** `code_challenge` 时，服务端才生成
   `(verifier, challenge=S256(verifier))` 并把 verifier 只留在会话存储里（`oidc_sessions`），
   永不回传；客户端自带 challenge 时服务端**不生成** verifier，token 端点用会话里的
   challenge 校验客户端提交的 verifier。
4. **会话存储形状不变**：仍是 `(state, nonce, code_verifier, code_challenge, code_challenge_method, redirect_uri)`，
   区别只在「自带 challenge 的会话 `code_verifier` 为空串」。

### E.2 B-23 `publicRooms` 的 `server`

| 端点 | `server` 来源 | 优先级 |
|---|---|---|
| `GET /_matrix/client/v3/publicRooms` | query | — |
| `POST /_matrix/client/v3/publicRooms` | query + body | **query 优先**，body 仅作兼容 |

1. 此前 POST 只读 body 的 `server`，客户端按规范放 query 时被静默忽略（退化为本服务器目录）。
2. 现在 `query.server.as_deref().or(body.server.as_deref())`；两者都给时用 query，
   避免「body 覆盖 query」的歧义。

### E.3 B-24 已读标记的私有键名

端点：`POST /_matrix/client/v3/rooms/{room_id}/read_markers`。

| 键 | 角色 |
|---|---|
| `m.read.private` | ✅ 规范键（MSC2285），**唯一**会写库的键 |
| `m.private_read` | 仅输入别名（`alias`），不参与输出 |

1. 台账核实：spec 是 `m.read.private`，此前实现写成了 `m.private_read` 且**两个都读**，
   导致按规范发的客户端读不到私有已读位置。
2. 现在的处理：`#[serde(rename = "m.read.private", alias = "m.private_read")]`；
   **只发规范键**，旧名仅作输入兼容。两个键同时出现在同一 JSON 对象里会被 serde 判为
   duplicate field → 400（`M_BAD_JSON`），客户端不应同时发送。

### E.4 B-25 `join` 的 `third_party_signed` 与 `via`

| 端点 | 新增/修复 | 行为 |
|---|---|---|
| `POST /rooms/{room_id}/join` | `third_party_signed` | 透传给 `join_room_with_reason`，用于 3PID 签名校验 |
| `POST /_matrix/client/v3/join/{room_id_or_alias}` | `third_party_signed` + `via` | 同上；`via` 为可重复 query 参数 |

1. **`via` 是 query 参数**：`?via=srv1&via=srv2`，此前只读 body 的 `via_servers`，
   标准客户端载荷 `{"via":[...]}`（以及 query）全被丢弃。现在 `via`（query，可重复）优先，
   `via_servers`（body）仅在 `via` 缺失时回退，且兼容单值字符串 / 数组两种写法。
2. `third_party_signed` 原样进入成员事件校验路径；缺省不产生该键。

### E.5 B-26 举报列表的 `from` / `next_token` / `total`

| 端点 | `from` 游标形状 | `next_token` | `total` |
|---|---|---|---|
| `GET /_synapse/admin/v1/event_reports` | `{score}\|{received_ts}\|{id}` | 仅满页时下发 | 全表匹配总数 |
| `GET /_synapse/admin/v1/event_reports/room/{room_id}` | `{received_ts}\|{id}` | 仅满页时下发 | 该房间匹配总数 |

1. **对齐上游键名**：上游用 `from` 翻页，本项目此前只有 `since_score`/`since_ts`/`since_id`，
   SDK 传 `from` 时永远第一页。现在 `from` 优先，旧的 `since_*` 三/两个参数保留为过渡别名。
2. **`next_token` 只在满页时非 null**：不满页即表示没有下一页，客户端据此终止循环；
   这个「只在满页给游标」的口径与 B-28 一致。
3. **`total` 改为真实总数**：此前是被 `limit` 截断后的本页条数，客户端据此算总页数会少算。
4. `offset > 0` → **400**（与附录 D.1 同一策略），不再静默返回错误页。

### E.6 B-27 媒体隔离变更的 `from` / `since`

| 端点 | `from` | `since` | 值语义 |
|---|---|---|---|
| `GET /_synapse/admin/v1/quarantine_media/{media_id}/changes` | ✅ | ✅ 别名 | `stream_id`（默认 0） |
| `GET /_synapse/admin/v1/media/quarantine_changes` | ✅ | ❌ | `stream_id`（默认 0） |

1. 上游把游标叫 `from`，本项目此前只认 `since`；`MediaQuarantineChangesQuery` 现在
   `#[serde(alias = "since")] from`，两者同义，`from` 优先。
2. 游标不是时间戳，是单调递增的 `stream_id`；无法解析时按 0 处理（回到第一页），不报 400。
3. 全局视图的 `next_batch` = 本页最后一条的 `stream_id`，空页时回显请求传入的 `from`。

### E.7 B-28 应用服务事件的分页

端点：`GET /_synapse/admin/v1/appservices/{as_id}/events`。

| 参数/字段 | 说明 |
|---|---|
| `limit` | 1–500，缺省 100 |
| `from` | 游标 `{origin_server_ts}\|{id}`，与存储排序键一一对应 |
| `events` | 本页事件数组 |
| `next_token` | 仅满页时非 null；否则 `null` 表示已到末尾 |

1. 此前响应是裸数组，没有游标，`limit` 之外的参数全部无效 ⇒ 永远只能取第一页。
2. `from` 解析失败 → 400 `M_INVALID_PARAM`（错误文本给出期望形状）。
3. 用 `(created_ts, id)` 而不是单纯的 ts 做 keyset，保证页边界既不跳行也不重复。

### E.8 B-29 房间同步的 `timeout` / `timeout_ms`

端点：`GET /_matrix/vendor/v1/rooms/{room_id}/sync`（`RoomSyncQueryDto`）。

| 名字 | 角色 |
|---|---|
| `timeout` | ✅ 后端结构体字段名（上游口径） |
| `timeout_ms` | 仅输入别名（`alias`） |

1. 以**后端 struct 为准**：规范参数是 `timeout`（毫秒），`timeout_ms` 作为过渡别名保留；
   两者都给时 serde 取先出现的那个键，不做额外合并。
2. 值域：无符号整数；不可解析时按缺省处理（与既有 `full_state` / `since` 的宽松策略一致）。

### E.9 B-30 CAS `proxyValidate` 的 `renew`

| 端点 | `service` | `ticket` | `renew` | `pgt_url` |
|---|---|---|---|---|
| `GET /_synapse/cas/proxyValidate` | ✅ | ✅ | ✅ | ✅ |
| `GET /_synapse/cas/serviceValidate` | ✅ | ✅ | ✅ | ✅ |
| `GET /_synapse/cas/p3/serviceValidate` | ✅ | ✅ | ✅ | ✅ |

1. 此前 `proxyValidate` 声明了 `service`/`ticket` 却漏了 `renew`，按 CAS 协议带 `renew` 的调用
   会因 `Query` 反序列化失败而 400（`renew` 是可选布尔，缺省 false）。
2. 三个端点现在都接受并解析 `renew`（`#[serde(rename = "renew")] _renew: Option<bool>`），
   当前实现不使用其值，但**不再因为多传一个合法参数而拒绝请求**。
3. CAS 协议面统一挂在 `/_synapse/cas` 下（避免占用根级 `/login`、`/logout`）；
   Matrix 侧的 SSO 入口是 `/_matrix/client/v3/login/sso/redirect/cas`，不在此列表。

### E.10 B-31 语音消息删除路由

| 项 | 值 |
|---|---|
| 端点 | `DELETE /_matrix/vendor/v1/voice/{media_id}` |
| 鉴权 | 已登录用户 |
| 权限 | 记录所有者，或 `is_admin`；其他 → **403** |
| 不存在 | **404** |
| 成功响应 | `{"deleted": true, "message_id": "<media_id>"}` |
| lane | 仅 `sdk`（`voice-extended` feature），golden lane 不注册 |

1. **选「后端补路由」而非改 SDK**：服务层 `delete_voice_message` 早已实现 owner/admin 语义，
   只是没挂路由；缺路由时客户端只能 404/405。
2. **响应字段稳定**：`deleted` 恒为 `true`（失败走错误码），`message_id` 回显请求的 media_id，
   SDK 可据此更新本地缓存而不必二次查询。
3. **幂等性边界**：底层媒体文件已不存在时视为删除成功（`is_not_found` 被吞掉），
   但**路由记录**必须存在，否则仍是 404——避免「删不存在的消息」被当成成功。

### E.11 B-32 停用用户的 legacy 路径

| 端点 | 关系 |
|---|---|
| `POST /_synapse/admin/v1/users/{user_id}/deactivate` | 规范/A 面路径 |
| `POST /_synapse/admin/v1/deactivate/{user_id}` | **legacy 别名**，内部转调同一 handler |

1. 上游保留 `/_synapse/admin/v1/deactivate/{user_id}`，Element Admin 等工具仍按它调用；
   本项目此前只有 `/users/{user_id}/deactivate`，按上游路径调用会 404。
2. 两条路径**共用** `deactivate_user`：鉴权（`AdminUser`）、请求体
   （`erase`、`by_admin` 等）与响应完全一致，不存在行为分叉。
3. 别名不是 feature-gated，`golden` 与 `sdk` 两个 lane 都注册（见 E.12 的双视图对账）。

### E.12 D-15 路由表的两个坐标视图

| 产物 | 坐标 | 条数 | 用途 |
|---|---|---|---|
| `docs/openapi/route-table.json` | golden × default | **899** | CI drift 基线、`placeholder_scan_tests` 的唯一输入 |
| `docs/openapi/route-table-sdk.json` | sdk × all | **979** | SDK 实际摄入的最大完整面 |
| `tests/unit/fixtures/ledger_export{,_sdk}/{default,worker,all}.json` | 两 lane × 三 profile，共 6 份 | **各文件自带 `entry_count`**（实测 899/910/917、960/971/979）—— 不在此手抄，见下方复算命令 | 契约提取的输入 |

> **这三行不再手抄数字的理由**：2026-10-24 复核发现它们全是旧读数（`968 / 975` 与
> `968/979/986、1029/1040/1048`），而 R6 门禁只比对 `route-table-sdk.json` 与 fixture，
> **不比对本文档的文字**——于是文档可以一直错下去而不变红。前两行改成 2026-10-24 实测值，
> 第三行改成"读各文件自己的 `entry_count`"：文件已经自述条数，再抄一遍就是第四份表示。
>
> 复算：`for f in docs/openapi/route-table{-sdk,}.json; do python3 -c "import json,sys;d=json.load(open(sys.argv[1]));print(sys.argv[1], d['lane'], d['profile'], d['total_routes'])" "$f"; done`
> 　　　`for f in tests/unit/fixtures/ledger_export{,_sdk}/{default,worker,all}.json; do python3 -c "import json,sys;print(sys.argv[1], json.load(open(sys.argv[1]))['entry_count'])" "$f"; done`

1. **不把派生路由标 `derived: true`**：`voice` 在 golden 视图里为 0，根因是 `voice-extended`
   只编进 sdk lane，不是「派生表不同源」。打标记只会掩盖 lane 差异，故改为**同一生成器的第二个
   坐标视图**。
2. **门禁 R6**：`scripts/contract/check_route_surface_reconciliation.py` 断言
   `route-table-sdk.json` 的 `lane=sdk`、`profile=all`、`total_routes` 与
   `ledger_export_sdk/all.json` 的 `entry_count` 一致且路由集合完全相同；
   `--mutation-check` 的 **Mutation 3** 会从该视图删一条路由，要求 R6 必须变红，
   以证明这条断言真的会失败。
3. **两视图都由 `gen_route_table.py` 生成**，命令分别指向对应 fixture；任何一份 fixture 重导后
   都要重跑两视图，否则 R6 报 drift。
4. **不要拿 golden 视图当全集**：它刻意不含 feature-gated 路由族（`/voice/*`、SAML/CAS 回调、
   `server-notifications` 等）；SDK 侧对账请用 `route-table-sdk.json`。

### E.13 本轮计数变化

> ⚠️ **历史增量表（2026-09 那一轮，当时值）**：下面的 `1048 / 968 / 1339` 是**那次改动的前后差**，
> 不是当前值。当前值：sdk × all **979**、golden × default **899**（见 E.12）。
> 保留它是因为它记录了"为什么多了这两条路由"，删掉会丢失动机；但**不要拿它当读数**。

| 指标 | 修复前 | 修复后 | 差值来源 |
|---|---|---|---|
| sdk × all 路由数 | 1046 | **1048** | `+DELETE /_matrix/vendor/v1/voice/{media_id}`、`+POST /_synapse/admin/v1/deactivate/{user_id}` |
| golden × default 路由数 | 967 | **968** | `+POST /_synapse/admin/v1/deactivate/{user_id}`（voice 仅 sdk） |
| ledger fixture（sdk all） | 1046 | **1048** | 同上 |
| `.sqlx` 查询缓存 | 1336 | **1339** | B-22 的 client 查询与 B-26 的 `count_*` 新增语句 |

### E.14 2026-10-21 B-29 / B-20 残留与工具面类型化 query

本轮**不改路由数**（当时为 golden × default 968、sdk × all 1048；**2026-10-24 当前值：899 / 979**，见 E.12），只补齐三处可选参数的
真实语义、一处「接受但忽略」，并把 5 条动态 query 提取器改为类型化 `Query<T>`。
口径统一为：**能力要么真实现，要么显式 400**，不存在「接受后静默丢弃」。

#### 1. room-summary 的三个可选参数（B-29 残留）

| 端点 | 参数 | 语义 |
|---|---|---|
| `GET /_matrix/vendor/v1/rooms/{room_id}/timeline` | `to` | `[from, to]` 区间上界（**exclusive**）；位置元组 `(origin_server_ts, stream_ordering)`；`dir` 决定保留哪一侧。被 `to` 截断时响应 `end` 回显 `to` 的 token，客户端保持 `to` 不变再请求会得到空 chunk |
| 同上 | `filter` | **不实现**：非空即 **400 `M_INVALID_PARAM`**（与 `/messages` 同一口径） |
| `GET /_matrix/vendor/v1/rooms/{room_id}/sync` | `filter` | **不实现**：先做房间可见性校验，非空即 **400 `M_INVALID_PARAM`** |
| `GET /_matrix/vendor/v1/rooms/{room_id}/notifications` | `only` | **可重复**，与全局 `/notifications` 同口径：`room_id:` / `type:` / `sender:`；未知 key、缺 `:`、空值 → 400。`from` 游标与 `next_token` 也改用同一套 keyset 实现，条目字段形状不变 |

1. `timeline` 的 `to` 是**内存截断**，不额外打一次 SQL：先按原分页取数，再按位置元组
   切掉区间外的事件；`dir=f` 保留 `pos < to`，`dir=b` 保留 `pos > to`。
2. `only=room_id:<其它房间>` 在房间级端点上返回**空页**而不是 400——这是过滤条件，
   不是语法错误。
3. 选择「显式 400」而不是「接受并忽略」：客户端依赖 `filter` 时会拿到未过滤的全量数据，
   静默丢弃比报错更危险。

#### 2. threads 的 `dir`（B-20 残留）

| 端点 | `dir` |
|---|---|
| `GET /_matrix/client/v1/rooms/{room_id}/threads` | ✅ `b`（缺省）/ `f` |
| `GET /_matrix/vendor/v1/threads` | ✅ 同上 |
| `GET /_matrix/vendor/v1/user/{user_id}/rooms/{room_id}/threads` | ✅ 同上 |

1. `b` = 最近活跃优先（`updated_ts` 降序 → `created_ts` 降序 → `thread_id` 升序），
   这是历史实际顺序；`f` 是整条比较链的严格逆序。
2. 其它取值 → **400 `M_INVALID_PARAM`**，不再「接受但忽略」。
3. 实现方式是**扫全量 + 内存排序分页**（每次向存储要 500 条直到游标耗尽），因为存储层
   的 `list_thread_roots` 以 `thread_id` 升序做 keyset，改排序方向需要新的静态 SQL 文本。
   `next_batch` 仍是同一条活动序列上的游标，`dir` 只改方向不改游标形状。

#### 3. 工具面 5 条改为类型化 `Query<T>`

| 端点 | 结构体 | 键 | 备注 |
|---|---|---|---|
| `POST /_matrix/client/v3/join/{room_id_or_alias}` | `ViaQuery` | `via`（可重复） | 手写 `Deserialize`；`via` 优先于 body 的 `via_servers` |
| `POST /_matrix/client/v3/knock/{room_id_or_alias}` | `ViaQuery` | 同上 | 同上 |
| `GET /_matrix/federation/v1/query/destination` | `QueryDestinationQuery` | `destination` | 缺省为本服务器 |
| `GET /_synapse/admin/v1/federation/destinations/{destination}/rooms` | `DestinationRoomsQuery` | `from`, `limit` | `from` 为非负偏移（传 `next_batch` 回填）；`limit` 夹紧到平台上下限；响应含 `next_batch`（仅当还有更多） |
| `GET /_matrix/vendor/v1/rooms/{room_id}/encrypted_events` | `EncryptedEventsQuery` | `from`, `limit` | `from` 为最新优先时间线上的偏移；内部多取 1 条以判定 `next_batch` |

改类型化的目的不只是「让契约生成器有键集」：`Query<HashMap>` 的键在运行时才确定，
SDK 门禁只能落到 `backend-query-unknown`；类型化后 `query` 分类为 `typed`，键集可逐字段
比对，并且非数字的 `from` / `limit` 会由提取器直接 **400**，而不是被静默忽略。

### E.15 2026-10-21 `/replies` 的 `dir`（B-20 最后一条残项）

`GET /_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/replies` 是 MSC3856 的
**非规范扩展端点**（规范只定义 `/threads` 根列表），因此 `dir` 的语义由本仓定义，
但仍与线程列表保持同一套取值词汇。

| 取值 | 行为 |
|---|---|
| 缺省 / `f` | **时间升序**（最早回复在前）——与该端点历史返回顺序一致，不产生行为回归 |
| `b` | 时间降序（最新回复在前） |
| 其它 | **400 `M_INVALID_PARAM`** |

1. **缺省是 `f` 而不是 `b`**：线程列表的缺省是 `b`（最近活跃优先），但本端点历史上一直
   按 `origin_server_ts ASC` 返回；把缺省改成 `b` 会让所有不传 `dir` 的客户端翻页顺序反转。
   两个端点的解析函数因此独立（`resolve_reply_direction` / `resolve_thread_direction`）。
2. **排序键**是 `(origin_server_ts, event_id)`，`b` 取整条比较链的严格逆序，保证
   同一毫秒内的多条回复也有稳定顺序。
3. **排序与分页下推 SQL**（2026-10-21 P0-2）：存储层新增 `get_thread_replies_page`，
   按 `dir` 二选一两条静态 SQL 文本（`f` 升序 / `b` 降序），游标为复合键
   `(origin_server_ts, event_id)`，与 `get_room_messages` 的 ISSUE-06 复合游标同口径。
   每页固定 2 次查询（按 `from` 的 `event_id` 查锚点 `origin_server_ts` + 取本页），
   复杂度 O(limit)，不再把整条线程拉回内存排序。刻意不用单条 `CASE WHEN $dir`
   排序，避免表达式排序破坏 `(room_id, thread_id, origin_server_ts, event_id)` 索引可用性。
4. **`from` 语义不变**：仍是上一页最后一条的 `event_id`，只作为切页锚点；token 形状
   未变，但缺失锚点时返回 **400 `M_INVALID_PARAM`**（见 E.16），不再静默回到第一页。
   `limit` 缺省 50、下限 1。
5. **契约同步**：`ledger_annotations.txt` 与 `EXPECTED_ANNOTATIONS` 均登记为
   `dir,from,limit`，提取器产出 `query_params: ["dir","from","limit"]` /
   `query_type: "RepliesQuery"`。SDK 门禁的 `query-param-ignored` 桶由 1 归零。

### E.16 2026-10-21 threads 分页锚点缺失改为 400（P0-3）

适用端点（`from` 是位置锚点，缺失锚点返回 400）：

| Method | Path | 锚点 | `next_batch` |
|---|---|---|---|
| `GET` | `/_matrix/client/v1/rooms/{room_id}/threads` | `thread_id` | 本页最后一条的 `thread_id` |
| `GET` | `/_matrix/vendor/v1/threads` | `thread_id` | 同上 |
| `GET` | `/_matrix/vendor/v1/user/{user_id}/rooms/{room_id}/threads` | `thread_id` | 同上 |
| `GET` | `/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/replies` | `event_id` | 本页最后一条的 `event_id` |

**例外：`/_matrix/vendor/v1/threads/subscribed` 是 keyset 游标，不是位置锚点。**
它用 `thread_id > $from ORDER BY thread_id ASC` 直接下推给 SQL，`from` 不在订阅集里时
返回 **200 + 空页**（`threads: []`、无 `next_batch`），既不会回落到第一页，也不会死循环，
因此**不适用下面第 2 条的 400**。

1. **`from` 是位置锚点，必须原样回填上一页的 `next_batch`**。服务端在**本次结果集**中
   查找该锚点，取其后一页。
2. **锚点不在当前结果集 → 400 `M_INVALID_PARAM`**，`error` 为 `Unknown from token '<原样回显>'`。
   此前实现是 `position().map_or(0, |i| i + 1)`：锚点找不到就**静默回落第一页**，
   客户端 `while (next_batch)` 循环永远翻不回末尾，表现为死循环。
3. **何时会命中 400（正常客户端操作也会）**：同一个 `from` 换到不同的 `dir` 或
   `include=participated` 组合下求解时，锚点可能不在结果集里；上一页的根事件被 redact
   或线程被删除也会。客户端拿到 400 应**从首页重新开始**（丢弃 `next_batch`），
   不要把 400 当成可重试错误原地重试。
4. **空结果仍返回 200**：只有「明确给了 `from` 且找不到锚点」才 400；`from` 缺省 / 空串 = 首页。
5. **实现位置**：`synapse-web/src/routes/handlers/thread.rs` 的 `paginate_threads` /
   `paginate_replies`（两者都返回 `Result`，`ApiError::invalid_param`）。单测：
   `test_thread_list_rejects_unknown_from_anchor`、`test_reply_list_rejects_unknown_from_anchor`。
6. **`/threads/subscribed` 的批量补全**：`get_subscribed_threads` 对整页订阅做一次
   `get_thread_summaries`（按 room 分组），取代原先每条订阅一次 `get_thread_summary`；
   摘要缺失的订阅只出现在 `subscribed` 里，不进 `threads`，与原语义一致。

### E.17 2026-10-22 分片上传 `total_size` 必填与硬上界（FUNC-06）

分片上传的**契约加严**：`total_size` 从可选改为**必填**，并成为整条上传会话的硬上界。
变更前它可以省略（整段配额预检被跳过），也可以谎报（预检按 1 B 通过、实际写到
`complete` 才由 `max_file_size` 拦截，期间磁盘/内存已被占用）。

| 端点 | 参数 | 语义 |
|---|---|---|
| `POST /_matrix/media/v1/upload/chunk/start` | `total_size`（body，**必填**） | 缺失 / `null` → **400 `M_MISSING_PARAM`**；负数 → **400 `M_BAD_JSON`**；`> max_upload_size` → **413 `M_TOO_LARGE`** |
| 同上 | `total_chunks`（可选，缺省 `1`） | `< 1` → **400 `M_BAD_JSON`** |
| `POST /_matrix/media/v1/upload/chunk` | `upload_id` / `chunk_index`（query） | 必填（不变） |
| 同上 | 单块上界 | `chunk_size > chunk_size_limit`（10 MiB）→ **400 `M_BAD_JSON`**（不变） |
| 同上 | 累计上界 | `uploaded_size + chunk_size > total_size` → **413 `M_TOO_LARGE`**，并**删除该 upload 会话** |
| `POST /_matrix/media/v1/upload/chunk/complete` | — | 按**实际**字节数记账（不按声明值） |

1. **为什么必填**：`total_size` 是唯一可用于配额预占（reservation）的输入。允许 `None`
   时 `ensure_upload_allowed` 整段被跳过，配额预检形同虚设。
2. **start 时预占**：`total_size > 0` 时按声明值走与普通上传同一套 `ensure_upload_allowed`，
   该值计入 `pending_bytes`；`complete` 时转为已用配额，`cancel` / 过期释放。
3. **谎报也被拦住**：`upload_chunk` 以 `progress.total_size` 为硬上界，越界的那一块直接
   413，并**同时删除会话**（客户端无法继续往同一 session 追加）。`progress.total_size`
   为空的遗留 pending 记录回退到 `max_file_size`，不弱于旧行为。
4. **记账取实际值**：`complete` 走 `upload_media_with_id_accounted`，按实际字节数记账，
   并排除该会话自身的预占，避免「预占 + 实占」重复计数。声明值小于实际值时以实际值为准
   （配额不可少记）。
5. **客户端影响**：这是**加严**——此前省略 `total_size` 能成功，现在会 400。SDK / 客户端
   必须在 `start` 给出准确的文件总大小；"声明值超 `max_upload_size`"的错误码由 400 改为
   **413**，按 400 分支处理的调用方需同步调整。
6. **实现位置**：服务层 `synapse-services/src/media/mod.rs`（`start_chunked_upload`）与
   `synapse-services/src/media/chunked_upload.rs`（`upload_chunk` 的累计校验）；
   web 层的早拒绝在 `synapse-web/src/routes/media/upload.rs`（`chunked_upload_start`），
   与服务层同 errcode。门禁用例：`chunked_upload_requires_total_size`、
   `chunked_upload_rejects_lying_total_size`、`chunked_upload_counts_toward_quota`。

### E.18 2026-10-22 线程回复请求体去掉客户端事件 id（ARCH-01 方案 A）

线程的阴影表（`thread_roots` / `thread_replies`）降级为**规范事件的索引**：每建一个线程、
每写一条回复，都必须在 `events` 里对应一条真实事件。为此两处语义收紧：

| 端点 | 变更 | 语义 |
|---|---|---|
| `POST /_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/replies` | body 不再使用 `event_id` | 回复事件的 id 由服务端写入口定稿（v3+ 房间是引用哈希），响应体 `event_id` 即定稿值 |
| 同上 | body 不再使用 `root_event_id` | `thread_id` 已唯一确定线程，`m.relates_to.event_id` 取服务端索引里的线程根，客户端无法改写指向 |
| `POST /_matrix/vendor/v1/rooms/{room_id}/threads` | `root_event_id` 必须是房间内**真实存在**的事件 | 不存在或不属于该房间 → **404 `M_NOT_FOUND`**（此前会在阴影表里造出指向空气的线程行） |

1. **为什么去字段**：旧实现把客户端给的 `event_id` 直接写进 `thread_replies`，`events` 里
   没有任何事件 —— 线程回复在同步、联邦、搜索、客户端渲染里全都不可见。回复改为走
   `RelationsService::send_reference`（`rel_type: "m.thread"`）后，事件 id 只能由写入口定稿，
   客户端再传一个 id 就是"收了但不用"的兼容 shim，因此直接删除。
2. **兼容性**：`CreateReplyBody` 未启用 `deny_unknown_fields`，老客户端继续发
   `event_id` / `root_event_id` 会被**忽略**，不会 400；但两者从此不再影响结果。
3. **实现位置**：`synapse-services/src/thread_service.rs`（`ThreadEventPort` /
   `CanonicalThreadEvents`、`create_thread` 的根事件校验、`add_reply` 先写事件再写索引）、
   `synapse-web/src/routes/handlers/thread.rs`（`CreateReplyBody`）。
   门禁用例：`test_thread_reply_writes_canonical_m_thread_event`、
   `test_create_thread_rejects_unknown_root_event`。

## 附录 F — 未实现语义表（ARCH-03，2026-10-24）

> ⚠️ 本节位于「`## 附录`」之后，因此 `gen_contract_doc.py` 重新生成契约时会**保留**它
> （生成器只重建附录之前的路由派生部分）。语义的可执行版本见
> `synapse-common/src/error.rs` 的 `arch03_unimplemented_semantics_table_is_pinned`。

这张表回答一个问题：**客户端拿到 404/501/500 时应该改什么**。
三个状态码刻意为三件事服务，**不允许合并**（合并后客户端分不清「换端点」「去掉某个字段」「等运维修」）。

| # | 情形（判定条件） | HTTP | errcode | 构造器 | 客户端该做什么 |
|---|---|---|---|---|---|
| 1 | **端点/能力整体不提供**：路由未注册；或端点只由本服务器未采纳的 MSC/厂商扩展定义；或该部署整体未启用该交付通道（SMTP / SMS provider / 后台队列） | `404` | `M_UNRECOGNIZED` | `ApiError::not_implemented()` | 换一条路径或换一种能力，**重试无用** |
| 2 | **端点可用，请求的字段/扩展/取值被拒**：路由已注册、请求合法，但其中某个字段或整个响应所依赖的能力未实现（静默忽略会把"没做"伪装成"成功"） | `501` | `M_UNKNOWN` | `ApiError::not_supported()` | 去掉该字段/改用受支持的取值后重试 |
| 3 | **服务器自身出错**：端点本应可用，失败源于缺陷或配置缺失 | `500` | `M_UNKNOWN` | `ApiError::internal*()` | 报障；**不得**把它映射成 404/501 来"降级" |

判定顺序：先问"路径/方法在派生路由表里命中吗"（不命中 ⇒ #1），再问"失败是否只由请求内容决定"（是 ⇒ #1 或 #2，取决于整条能力是否存在），最后才知道是不是 #3。

**wire 值与内部值**：`MatrixErrorCode::Unrecognized` 与 `MatrixErrorCode::Unimplemented` **共用**线上字符串
`M_UNRECOGNIZED`（`MatrixErrorCode::as_str` 有既有用例），本仓用后者构造"未实现"。对外只承诺**状态码 + wire errcode**；
内部用 `ApiError::is_not_implemented()` 区分"缺端点/未实现"与"路径打错"，运维排障时才用得上。
这也解释了 ARCH-03 的"混用"观感：两个内部变体、一个线上值，责任在生成端不在客户端。

### F.1 全部调用点（`ApiError::not_implemented` / `not_supported` 在生产代码中的每一处）

口径：`grep -rn 'ApiError::not_(implemented|supported)(' synapse-*/src src`，排除构造器定义与 `#[cfg(test)]`。
当前 **10 处生产调用**（另有 2 处在 `#[cfg(test)]` 里钉住语义，见末行）。

| 文件 | 位置（符号） | 语义行 | 改前 | 改后 | 说明 |
|---|---|---|---|---|---|
| `synapse-web/src/routes/voice.rs` | `voice_operation_not_provided` ← `convert_voice_message` | #1 | `404 M_UNRECOGNIZED` | `404 M_UNRECOGNIZED` | 保持：MSC3245 规定转换在客户端，服务端不提供整条能力。抽成函数只是为了让 `arch03_voice_unimplemented_semantics_tests` 能钉住它 |
| `synapse-web/src/routes/voice.rs` | 同上 ← `optimize_voice_message` | #1 | `404 M_UNRECOGNIZED` | `404 M_UNRECOGNIZED` | 保持 |
| `synapse-web/src/routes/voice.rs` | 同上 ← `transcribe_voice_message` | #1 | `404 M_UNRECOGNIZED` | `404 M_UNRECOGNIZED` | 保持 |
| `synapse-web/src/routes/account_compat.rs` | `reject_unsupported_account_erasure` ← `deactivate_account` | #2 | `501 M_UNKNOWN` | `501 M_UNKNOWN` | 保持：端点可用、`erase` 字段被拒。抽成函数以便钉住 |
| `synapse-web/src/routes/handlers/room/events.rs` | `get_event_keys`（实际路径 `/_matrix/vendor/v1/rooms/{room_id}/keys/{event_id}`，见 by-design 表） | #2 | `501 M_UNKNOWN` | `501 M_UNKNOWN` | 保持：不能回 `200 {"keys": []}`（读起来像"这个事件没有密钥"） |
| `synapse-services/src/captcha_service.rs` | `send_email`（SMTP 未启用分支） | #1 | `404 M_UNRECOGNIZED` | `404 M_UNRECOGNIZED` | 保持：email-captcha 交付通道整体未启用 ⇒ 客户端换验证方式。**偏离登记**：Synapse 对同类情况回 `400 M_THREEPID_DENIED`；改成本表 #2（501）会动注册/三方 ID 业务分支，不属 ARCH-03 改动面 |
| `synapse-services/src/captcha_service.rs` | `send_email`（任务队列未配置分支） | #1 | `404 M_UNRECOGNIZED` | `404 M_UNRECOGNIZED` | 同上（偏离登记同上） |
| `synapse-services/src/captcha_service.rs` | `send_sms_async`（无 provider） | #1 | `404 M_UNRECOGNIZED` | `404 M_UNRECOGNIZED` | 同上（偏离登记同上） |
| `synapse-services/src/sms_provider/mod.rs` | `NoopSmsProvider::send` | #1 | `404 M_UNRECOGNIZED` | `404 M_UNRECOGNIZED` | 同上；no-op provider 就是"该通道未接入"的显式表示 |
| `synapse-services/src/auth/credential_auth.rs` | `CredentialAuth::register_without_login`（默认实现） | #1 | `404 M_UNRECOGNIZED` | `404 M_UNRECOGNIZED` | 内部 trait 默认值，只给测试替身用，不直接产出对外响应；保留以便漏实现时**响亮失败**而不是静默 |
| `#[cfg(test)]`（不计入 10） | `synapse-web/src/routes/handlers/room/events.rs`、`synapse-common/src/error.rs` | — | — | — | 钉住语义的用例，不是调用点 |

**已知偏离（登记，不在本卡修）**：上表 4 处"交付通道未启用"按 #1 回 404，而 Synapse 对同一情形回 `400 M_THREEPID_DENIED`。
两者都满足"客户端应改走别的路径"，差别只在 errcode 的提示强度；改它要动 `synapse-services` 的注册/三方 ID 分支与对应测试，
留给能力台账重做（MAINT-04）时统一处理。
