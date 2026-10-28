# Synapse Rust — SDK / 前端接入契约

面向客户端（Tjg / HuLa / Element 类 SDK）的接口契约文档。**每一条端点、字段、状态码都以本仓
源码为准**；端点存在性以 `synapse_ledger_export` 导出为准——SDK lane（`--features all-extensions`）
对应 `tests/unit/fixtures/ledger_export_sdk/default.json`（**1019 条**），
`docs/openapi/route-table.json` 则是 CI 对账用的 golden lane × default profile 视图（**959 条**）。

## 服务与部署事实

| 项 | 值 | 来源 |
|---|---|---|
| 产品版本 | `6.3.0` | `Cargo.toml` 的 `[workspace.package].version`，`GET /_matrix/server_version` 原样上报 |
| 协议 | Matrix Client-Server **v1.1 – v1.17**（`versions` 数组） | `GET /_matrix/client/v3/versions` |
| 默认房间版本 | `12` | `capabilities.m.room_versions.default` |
| 本地基址 | `http://127.0.0.1:8008`（容器 `synapse-app` 映射到宿主回环） | `docker/config/homeserver.yaml` 的 `server.public_baseurl` 默认值 |
| 参考 server_name | `matrix.test` | `/.well-known/matrix/client` 实测返回 `{"m.homeserver":{"base_url":"https://matrix.test"}}` |
| 单文件上传上限 | 52428800（50 MiB） | `server.max_upload_size`，`GET /_matrix/client/v3/media/config` 同值 |
| 服务端语言 | 单一 `en`（`server_version.python_version` 固定为 `"Rust"`） | `handlers/versions.rs` |

> 生产部署的 `server_name` / `public_baseurl` 由 `SERVER_NAME` / `PUBLIC_BASEURL` 环境变量注入，
> 不要硬编码本文的示例域名。

## 文档地图

| 文档 | 覆盖范围 |
|---|---|
| [sdk-guide.md](./sdk-guide.md) | **封装指导**：分层架构、Token 刷新单飞、sync 循环、事件管线、E2EE 会话、重试与幂等、能力探测、测试与发布 |
| [discovery.md](./discovery.md) | `/versions`、`/capabilities`、`.well-known/*`、`/login` 流程发现、健康检查 |
| [authentication.md](./authentication.md) | 注册（含 UIA）、登录、登出、refresh、账户资料、密码、停用、3PID、guest 升级、**SAML / CAS 单点登录**、AS 客户端面 |
| [devices.md](./devices.md) | 设备与会话、`delete_devices`、Presence、Pushers、Notifications |
| [directory.md](./directory.md) | User Directory（`user_directory/*`）、用户搜索、`publicRooms`、目录可见性、房间别名、Thirdparty、举报 |
| [sync.md](./sync.md) | `/sync`（v3 GET 与 v1/v4 POST）、Filter、Account Data、Tags、回执与 typing 的 sync 投影、Sticky |
| [rooms.md](./rooms.md) | 创建/加入/离开、成员、状态事件、别名与目录、层级、房间信息、报告 |
| [messages.md](./messages.md) | 发送/编辑/撤回、历史分页、relations / reactions / threads / pinned、搜索、context |
| [friends.md](./friends.md) | `/_matrix/vendor/v1/friends/*`（好友、请求、分组、搜索、建议、私信） |
| [media.md](./media.md) | 上传（含分片）、鉴权与非鉴权下载、缩略图、配额、preview_url |
| [e2ee.md](./e2ee.md) | 设备密钥、一次性密钥、cross-signing、key backup（`room_keys`）、密钥轮换、MSC4262 成员可见性 |
| [rtc.md](./rtc.md) | VoIP/TURN、MSC3079 `m.call.*` 通话事件、MSC4143 RTC transports、Widget API（MSC4261） |
| [extensions.md](./extensions.md) | `/_matrix/vendor/v1/*` 扩展：房间摘要、线程、置顶、延迟事件、Sticky、Direct、Spaces、搜索、翻译、**语音（voice）** |
| [appservices.md](./appservices.md) | Application Service：客户端面、`/_matrix/app/v1/*` 回调面、管理面、MSC4512 `fed_proxy` |
| [admin.md](./admin.md) | `/_synapse/admin/v1/*`（用户、房间、媒体、联邦、后台任务、注册令牌、modules、telemetry、policy、**通知/服务器通告**、**SAML/CAS 管理**） |
| [errors.md](./errors.md) | 错误响应格式、全部 `errcode`、限流头、重试策略 |

## 三条必须知道的全局契约

### 1. 没有 `{status, data}` 信封

成功响应就是**业务对象本身**；无返回体的端点绝大多数返回 `{}`（空对象，不是 `{"status":"ok"}`）。
历史版本文档里的 `{"status": "ok", "data": {...}}` 从未在本服务实现过，按它解析会拿到 `undefined`。

> **204 No Content 不是个例**（客户端必须统一处理，不能假设所有成功响应都有 body）。
> 实测返回 **204 无 body** 的端点至少包括：
>
> | 类 | 端点 |
> |---|---|
> | 管理面删除 | `DELETE /_synapse/admin/v1/modules/{module_name}` |
> | | `DELETE /_synapse/admin/v1/appservices/{as_id}` |
> | | `DELETE /_synapse/admin/v1/event_reports/{id}` |
> | | `DELETE /_synapse/admin/v1/background_updates/{job_name}` |
> | | `DELETE /_synapse/admin/v1/cas/services/{service_id}` |
> | | `DELETE /_synapse/admin/v1/external_services/{as_id}` |
> | 管理面动作 | `POST /_synapse/admin/v1/event_reports/rate_limit/{user_id}/block`、`/unblock` |
> | | `DELETE /_synapse/worker/v1/workers/{worker_id}` |
> | 客户端 / vendor | `DELETE /_matrix/client/v3/rooms/{room_id}/summary` |
> | | `DELETE /_matrix/client/v3/rooms/{room_id}/summary/members/{user_id}` |
> | | `DELETE /_matrix/vendor/v1/rooms/{room_id}/threads/{thread_id}` |
> | | `POST /_matrix/vendor/v1/spaces/{space_id}/leave` |
> | | `DELETE /_matrix/vendor/v1/spaces/{space_id}`、`.../children/{room_id}` |
> | | `DELETE /_matrix/client/unstable/org.matrix.msc4108/rendezvous/{session_id}` |
>
> SDK 里请统一用 `res.status === 204 ? null : await res.json()` 兜住。
>
> 反例：`DELETE /_synapse/admin/v1/spaces/{space_id}` **不是** 204，返回 `{"deleted": true}`；
> 因此不要用「HTTP 方法 = DELETE 就一定无 body」的规则推断。

```jsonc
// POST /_matrix/client/v3/login 的 200
{ "access_token": "...", "refresh_token": "...", "expires_in": 3600,
  "device_id": "...", "user_id": "@alice:matrix.test",
  "well_known": { "m.homeserver": { "base_url": "https://matrix.test" } } }

// PUT /_matrix/client/v3/rooms/{room}/send/m.room.message/{txn}
{ "event_id": "$abc:matrix.test" }
```

> `expires_in` 单位是**秒**（`server.access_token_lifetime_seconds`，默认 `3600`），不是毫秒。
> guest 登录也返回该字段，但不返回 `refresh_token`。**不要**把 `expires_in` 写进 `Authorization` 头的过期判断后直接丢弃 token——
> 应先用 `refresh_token` 续期，只有 refresh 也失败时才跳登录。

错误响应形状唯一：

```json
{ "errcode": "M_INVALID_PARAM", "error": "The 'dir' parameter must be 'f' or 'b'" }
```

### 2. Token 只能放 `Authorization` 头

服务端只从 `Authorization: Bearer <token>` 取 token（`utils/auth.rs::extract_token`）；
**`?access_token=` 查询参数会被静默忽略**，请求随即 401 `M_UNAUTHORIZED`（已实测）。

```bash
# 有效
curl -H "Authorization: Bearer $TOKEN" http://127.0.0.1:8008/_matrix/client/v3/account/whoami
# 无效：query 里的 token 不被读取
curl "http://127.0.0.1:8008/_matrix/client/v3/sync?access_token=$TOKEN"   # → 401
```

对 `<img>` / `<video>` 这类不能带头部的场景：**用非鉴权的 `/_matrix/media/v3/download/...`
与 `/_matrix/media/v3/thumbnail/...`**（无需 token），或由前端 `fetch` 拿到 blob 再 `createObjectURL`。
详见 [media.md](./media.md)。

### 3. 401 的 errcode 分三种

| 场景 | errcode | 说明 |
|---|---|---|
| 缺/坏 `Authorization` 头（**绝大多数**端点：client、vendor、admin、media、federation） | `M_UNAUTHORIZED` | `Missing or invalid authorization header`；`?access_token=` 也落在这里 |
| 缺 token 且端点显式区分（如 `GET /_matrix/client/v3/account/whoami`） | `M_MISSING_TOKEN` | `Missing access token` |
| token 无效/过期/已登出（主流端点） | `M_UNAUTHORIZED` | `error` 分别为 `Invalid token` / `Token expired` / `Token has been revoked` |
| token 无效（仅 `whoami`） | `M_UNKNOWN_TOKEN` | `Invalid token` |

**SDK 的处理口径**：不要用 errcode 判断是否需要 refresh —— 除 `M_MISSING_TOKEN` 外，
401 一律走「尝试 refresh，失败再重新登录」。服务端在本服务中把 token 无效/过期也归到
`M_UNAUTHORIZED`，只有 `whoami` 会返回 `M_UNKNOWN_TOKEN`。

## API 版本前缀

| 前缀 | 用途 | 备注 |
|---|---|---|
| `/_matrix/client/v3/` | Matrix 客户端 API 主体（含 SAML/CAS 登录、`m.call.*` 通话） | 249 条 |
| `/_matrix/client/v1/` | 较新的稳定端点（threads、relations、media、profile 部分） | 111 条，与 v3 并存，不是替代 |
| `/_matrix/client/unstable/` | MSC 命名空间（`org.matrix.msc4140`、`msc4354`、`msc3575`、`msc3814`、`msc4143`…） | 31 条，需 `/versions` 的 `unstable_features` 为 `true` |
| `/_matrix/media/v3/` | 媒体（**非鉴权**直链） | 10 条；另见 `/_matrix/client/v1/media/*`（需鉴权） |
| `/_matrix/vendor/v1/` | 本仓客户端扩展（好友、摘要、线程、置顶、burn、spaces、widgets、**voice**…） | 197 条，非 Matrix 规范 |
| `/_synapse/admin/v1/` | 管理 API（含 SAML/CAS 管理、通知与服务器通告） | 289 条，需管理员 token；`/_synapse/admin/` 合计 294 条（含 `v2/users` 4 条与 `/info` 1 条） |
| `/_matrix/federation/v1/` | 服务端互通 | 35 条，客户端不应调用 |
| `/_matrix/app/v1/` | Application Service 回调面 | 12 条，由 AS（非客户端）实现/调用 |
| `/_synapse/cas/` | CAS 协议面（`login`、`serviceValidate`、`proxyValidate`、`proxy`、`p3/serviceValidate`、`logout`） | 6 条，供 CAS IdP 调用，非 SDK 直连 |

> 上表按 SDK lane（`--features all-extensions`）× `default` profile 计数，共 **1019** 条
> （源：`tests/unit/fixtures/ledger_export_sdk/default.json`，`generated_at = 2026-05-02T00:00:00Z`）。
> golden lane（默认特性闭包）为 **959** 条，即 `docs/openapi/route-table.json` 的 CI 对账口径。
> `/_matrix/client/v1/` 中的 "v1" 是**端点版本**，与本服务产品版本 `6.3.0` 无关。
>
> 扩展族（SAML/CAS、voice、`m.call.*`、admin 通知）在源码中受 Cargo 特性门控
> （`saml-sso` / `cas-sso` / `voice-extended` / `voip-tracking` / `server-notifications`）；
> 参考部署用 `--features all-extensions` 构建，故本文按 SDK lane 的「已编译」口径描述；
> route-table 只反映 golden lane，不含这些扩展族。
> SDK 仍应以**实际请求结果**（404 表示未编译）为准做能力探测。

## 分页约定（按端点而异，勿套用统一模型）

本服务**没有**统一的 `offset/since` 分页协议，不同家族各自成约：

| 家族 | 机制 | 关键字段 |
|---|---|---|
| `/sync` | 增量游标 | `next_batch`，`since` |
| `/rooms/{id}/messages` | 方向游标 | `from` / `to` / `dir`（缺省 `b`），响应 `start` / `end` / `chunk` |
| `/_matrix/vendor/v1/friends` | **Keyset 游标**（`offset` 已废弃） | 请求 `from`，响应 `next_batch` / `next_offset`；仅当 `offset > 0` 且未带合法 `from` 游标时返回 400 `M_INVALID_PARAM`，带 `from` 时 `offset` 被忽略 |
| `/_synapse/admin/v1/reports` 等 | Keyset | `since_ts` / `since_id` / `since_score`；多数管理端点在 `offset > 0` 时同样 400 |
| `/publicRooms` | `since` + `limit` | `next_batch` |

`limit` 上限因端点而异（`/messages` 最大 1000、`friends` 最大 200），**不要假设全局 10–1000**。

## 错误处理最小实现

```typescript
export class MatrixApiError extends Error {
  constructor(readonly errcode: string, message: string, readonly status: number,
              readonly retryAfterMs?: number) {
    super(message);
  }
}

export async function call<T>(base: string, path: string, token: string | null,
                             init: RequestInit = {}): Promise<T> {
  const headers = new Headers(init.headers);
  headers.set('Content-Type', 'application/json');
  if (token) headers.set('Authorization', `Bearer ${token}`);

  const res = await fetch(base + path, { ...init, headers });
  const body = res.status === 204 ? {} : await res.json().catch(() => ({}));

  if (!res.ok) {
    throw new MatrixApiError(
      body.errcode ?? 'M_UNKNOWN',
      body.error ?? `HTTP ${res.status}`,
      res.status,
      body.retry_after_ms,
    );
  }
  return body as T;
}
```

完整错误码清单见 [errors.md](./errors.md)。

## 一致性约束（写文档/改代码时的口径）

1. **端点存在性**以 `synapse_ledger_export` 导出为准：SDK lane 用
   `tests/unit/fixtures/ledger_export_sdk/*.json`，golden lane 的 CI 视图是
   `docs/openapi/route-table.json`；改路由必须同步契约链
   （`scripts/contract/check_route_contract.sh`）。
2. **响应字段**以 handler 里 `json!({...})` / `Serialize` 结构体为准；本文每篇的示例均按此提取。
3. **能力发现**优先于猜测：客户端应读 `/versions` 的 `unstable_features` 与 `/capabilities`，
   而不是按版本号硬编码分支。
