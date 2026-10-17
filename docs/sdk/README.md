# Synapse Rust — SDK / 前端接入契约

面向客户端（Tjg / HuLa / Element 类 SDK）的接口契约文档。**每一条端点、字段、状态码都以本仓
源码为准**；`docs/openapi/route-table.json`（由 `scripts/api_test/gen_route_table.py` 从路由表生成，
959 条）是端点存在性的唯一权威来源。

## 服务与部署事实

| 项 | 值 | 来源 |
|---|---|---|
| 产品版本 | `6.2.0` | `Cargo.toml` 的 `[workspace.package].version`，`GET /_matrix/server_version` 原样上报 |
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
| [discovery.md](./discovery.md) | `/versions`、`/capabilities`、`.well-known/*`、`/login` 流程发现、健康检查 |
| [authentication.md](./authentication.md) | 注册（含 UIA）、登录、登出、refresh、账户资料、密码、停用、3PID、guest 升级 |
| [devices.md](./devices.md) | 设备与会话、`delete_devices`、Presence、Pushers、Notifications |
| [sync.md](./sync.md) | `/sync`（v3 GET 与 v1/v4 POST）、Filter、Account Data、Tags、回执与 typing 的 sync 投影 |
| [rooms.md](./rooms.md) | 创建/加入/离开、成员、状态事件、别名与目录、层级、房间信息、报告 |
| [messages.md](./messages.md) | 发送/编辑/撤回、历史分页、relations / reactions / threads / pinned、搜索、context |
| [friends.md](./friends.md) | `/_matrix/vendor/v1/friends/*`（好友、请求、分组、搜索、建议、私信） |
| [media.md](./media.md) | 上传（含分片）、鉴权与非鉴权下载、缩略图、配额、preview_url |
| [e2ee.md](./e2ee.md) | 设备密钥、一次性密钥、cross-signing、key backup（`room_keys`）、密钥轮换 |
| [extensions.md](./extensions.md) | `/_matrix/vendor/v1/*` 扩展：房间摘要、线程、置顶、延迟事件、Sticky、Direct、Spaces |
| [admin.md](./admin.md) | `/_synapse/admin/v1/*`（用户、房间、媒体、联邦、后台任务、注册令牌） |
| [errors.md](./errors.md) | 错误响应格式、全部 `errcode`、限流头、重试策略 |

## 三条必须知道的全局契约

### 1. 没有 `{status, data}` 信封

成功响应就是**业务对象本身**；无返回体的端点返回 `{}`（空对象，不是 204、不是 `{"status":"ok"}`）。
历史版本文档里的 `{"status": "ok", "data": {...}}` 从未在本服务实现过，按它解析会拿到 `undefined`。

```jsonc
// POST /_matrix/client/v3/login 的 200
{ "access_token": "...", "refresh_token": "...", "expires_in": 3600000,
  "device_id": "...", "user_id": "@alice:matrix.test",
  "well_known": { "m.homeserver": { "base_url": "https://matrix.test" } } }

// PUT /_matrix/client/v3/rooms/{room}/send/m.room.message/{txn}
{ "event_id": "$abc:matrix.test" }
```

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

### 3. 401 的 errcode 分两种

| 场景 | errcode | 说明 |
|---|---|---|
| 缺 token（client API） | `M_MISSING_TOKEN` | `Missing access token` |
| 缺/坏 `Authorization` 头（media、admin、federation 等） | `M_UNAUTHORIZED` | `Missing or invalid authorization header` |
| token 无效/过期 | `M_UNKNOWN_TOKEN` | 客户端应清 token 并重新登录 |

## API 版本前缀

| 前缀 | 用途 | 备注 |
|---|---|---|
| `/_matrix/client/v3/` | Matrix 客户端 API 主体 | 379 条 |
| `/_matrix/client/v1/` | 较新的稳定端点（threads、relations、media、profile 部分） | 与 v3 并存，不是替代 |
| `/_matrix/client/unstable/` | MSC 命名空间（`org.matrix.msc4140`、`msc4354`、`msc3575`、`msc3814`…） | 需 `/versions` 的 `unstable_features` 为 `true` |
| `/_matrix/media/v3/` | 媒体（**非鉴权**直链） | 另见 `/_matrix/client/v1/media/*`（需鉴权） |
| `/_matrix/vendor/v1/` | 本仓客户端扩展（好友、摘要、线程、置顶、burn、spaces…） | 185 条，非 Matrix 规范 |
| `/_synapse/admin/v1/` | 管理 API | 266 条，需管理员 token |
| `/_matrix/federation/v1/` | 服务端互通 | 客户端不应调用 |

> `/_matrix/client/v1/` 中的 "v1" 是**端点版本**，与本服务产品版本 `6.2.0` 无关。

## 分页约定（按端点而异，勿套用统一模型）

本服务**没有**统一的 `offset/since` 分页协议，不同家族各自成约：

| 家族 | 机制 | 关键字段 |
|---|---|---|
| `/sync` | 增量游标 | `next_batch`，`since` |
| `/rooms/{id}/messages` | 方向游标 | `from` / `to` / `dir`（缺省 `b`），响应 `start` / `end` / `chunk` |
| `/_matrix/vendor/v1/friends` | **Keyset 游标**（`offset` 已废弃） | 请求 `from`，响应 `next_batch` / `next_offset`；传 `offset` 直接 400 |
| `/_synapse/admin/v1/reports` 等 | Keyset | `since_ts` / `since_id` / `since_score` |
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

1. **端点存在性**只以 `docs/openapi/route-table.json` 为准；改路由必须同步契约链
   （`scripts/contract/check_route_contract.sh`）。
2. **响应字段**以 handler 里 `json!({...})` / `Serialize` 结构体为准；本文每篇的示例均按此提取。
3. **能力发现**优先于猜测：客户端应读 `/versions` 的 `unstable_features` 与 `/capabilities`，
   而不是按版本号硬编码分支。
