# SDK 封装指导

面向要把本服务 HTTP API 封装成客户端 SDK 的团队。本文只讲**架构与行为约定**；每个端点的字段级契约在其余各篇。

目标：让 SDK 在弱网、token 过期、服务端限流、增量同步、E2EE 五种压力下都不丢消息、不重复发消息、不误登出。

---

## 目录

- [分层建议](#分层建议)
- [第 0 层：基址与能力发现](#第-0-层基址与能力发现)
- [第 1 层：传输与错误模型](#第-1-层传输与错误模型)
- [第 2 层：认证与会话](#第-2-层认证与会话)
- [第 3 层：sync 循环](#第-3-层sync-循环)
- [第 4 层：事件管线](#第-4-层事件管线)
- [第 5 层：E2EE](#第-5-层e2ee)
- [第 6 层：媒体](#第-6-层媒体)
- [幂等：txn_id 规则](#幂等txn_id-规则)
- [限流与重试矩阵](#限流与重试矩阵)
- [测试与发布](#测试与发布)
- [常见错误封装](#常见错误封装)

---

## 分层建议

```text
┌─────────────────────────────────────────────┐
│ 应用层（UI / 状态管理 / 本地数据库）          │
├─────────────────────────────────────────────┤
│ 领域层 RoomApi / MessageApi / FriendsApi …   │  ← 本篇各专题文档
├─────────────────────────────────────────────┤
│ 会话层 Session（token、refresh 单飞、登出）   │
├─────────────────────────────────────────────┤
│ 同步层 SyncEngine（since、长轮询、事件分发）  │
├─────────────────────────────────────────────┤
│ 加密层 Crypto（Olm/Megolm、密钥上传、解密）   │
├─────────────────────────────────────────────┤
│ 传输层 HttpTransport（baseUrl、超时、重试）   │
└─────────────────────────────────────────────┘
```

约束：

1. **只有传输层能拼 URL**。领域层只传「路径 + 查询 + 体」，避免各处硬编码 `/_matrix/client/v3`。
2. **只有会话层能改 token**。其他层拿到的永远是「当前 token 的 getter」，不是快照字符串。
3. **同步层不直接写 UI**。它只产出事件流，应用层决定如何合入本地状态。
4. **加密层是可选的**。不启用 E2EE 时，发送路径应完全绕过加密层，而不是走一个空实现。

---

## 第 0 层：基址与能力发现

### 基址解析

用户输入的是域名，不是 URL。标准顺序：

```text
用户输入 user@matrix.test
  → GET https://matrix.test/.well-known/matrix/client
  → 读 m.homeserver.base_url
  → 所有 API 都拼在这个 base_url 上
```

本服务返回：

```json
{ "m.homeserver": { "base_url": "https://matrix.test" } }
```

若 `.well-known` 404、超时或没有 `m.homeserver.base_url`，才把用户输入本身当 base URL。**不要**在 `.well-known` 失败时直接判登录失败。

### 能力发现

优先读两个端点，不要按产品版本号写分支：

```typescript
const versions = await http.get<{
  versions: string[];
  unstable_features: Record<string, boolean>;
}>('/_matrix/client/v3/versions');

const caps = await http.get<{
  capabilities: {
    'm.room_versions': { default: string; available: Record<string, string> };
    'm.change_password'?: { enabled: boolean };
    'm.set_displayname'?: { enabled: boolean };
    // 认证后还会追加 io.hula.* / m.sso / external_services 等
  };
}>('/_matrix/client/v3/capabilities');
```

判定规则：

| 想用的能力 | 判定方式 |
|---|---|
| 某个 MSC 端点 | `versions.unstable_features['org.matrix.mscXXXX'] === true`，**且**路由表里存在该端点 |
| 房间版本 | `capabilities['m.room_versions'].available`，默认值用 `.default` |
| 私有媒体下载 | 探测 `GET /_matrix/client/v1/media/config` 是否 200 |
| 好友 / burn / sticky 等扩展 | 读 `/capabilities` 里的 `io.hula.*`；不要假设所有部署都开启 |

> 关键点：`unstable_features` 是由**路由探针**推导的，不是写死的常量。部署裁剪过路由后，同一版本号的服务端能力可能不同。

---

## 第 1 层：传输与错误模型

### 请求管道

每次请求固定做五件事，顺序不能乱：

1. 拼 `baseUrl + path`，查询参数用 `URLSearchParams`（不要手拼 `?`）。
2. 注入 `Authorization: Bearer <current token>`（仅当会话层有 token）。
3. `Content-Type: application/json` **只加在有 JSON 体的请求上**；上传媒体时用真实 MIME。
4. `AbortController` 超时（普通请求 30s，`/sync` 用服务端 `timeout` + 10s 余量）。
5. 解析响应：成功直接返回业务对象；失败统一抛 `MatrixApiError`。

**没有 `{status, data}` 信封**。`204` 在本服务基本不出现，无返回体的端点返回 `{}`。

### 错误模型

```typescript
export class MatrixApiError extends Error {
  constructor(
    readonly errcode: string,
    message: string,
    readonly status: number,
    readonly retryAfterMs?: number,
  ) {
    super(message);
    this.name = 'MatrixApiError';
  }
}

export async function call<T>(
  base: string,
  path: string,
  token: string | null,
  init: RequestInit = {},
): Promise<T> {
  const headers = new Headers(init.headers);
  if (init.body && !(init.body instanceof Blob) && !(init.body instanceof ArrayBuffer)) {
    headers.set('Content-Type', 'application/json');
  }
  if (token) headers.set('Authorization', `Bearer ${token}`);

  const res = await fetch(base + path, { ...init, headers });
  const body = await res.json().catch(() => ({}));

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

### 401 分派规则（最容易写错的地方）

| `errcode` | 含义 | SDK 动作 |
|---|---|---|
| `M_UNAUTHORIZED` | 缺/坏 `Authorization` 头，或 token 无效/过期/已登出 | 若本地有 refresh_token 就尝试一次 refresh；失败再清会话并跳登录 |
| `M_MISSING_TOKEN` | 少数端点（如 `whoami`）显式区分「缺 token」 | 不要 refresh，补上 token 重试；仍失败则视为 SDK bug |
| `M_UNKNOWN_TOKEN` | 仅 `whoami` 在 token 无效时返回 | 同 `M_UNAUTHORIZED` 的刷新路径 |

**关键**：本服务把 token 无效/过期也归到 `M_UNAUTHORIZED`，因此不能只对
`M_UNKNOWN_TOKEN` 触发 refresh；除 `M_MISSING_TOKEN` 外的 401 都应尝试 refresh。

注意：`?access_token=` 查询参数被服务端忽略，带 query token 的请求会得到 `M_UNAUTHORIZED`。

### 不能用状态码判断业务错误

`M_FORBIDDEN` 在登录失败时返回 **401**（规范状态是 403）。判断「密码错」只能看 `errcode`，不能只看 `status === 403`。

---

## 第 2 层：认证与会话

### 登录

```typescript
interface LoginResponse {
  access_token: string;
  refresh_token: string;
  expires_in: number;   // 秒，默认 3600
  device_id: string;
  user_id: string;
  well_known?: { 'm.homeserver'?: { base_url?: string } };
}

const res = await call<LoginResponse>(base, '/_matrix/client/v3/login', null, {
  method: 'POST',
  body: JSON.stringify({
    type: 'm.login.password',
    identifier: { type: 'm.id.user', user: username },
    password,
    device_id: existingDeviceId,      // 不传则服务端新建
    initial_device_display_name: 'Web',
  }),
});
```

要点：

- `expires_in` 是**秒**。不要按毫秒算过期时间。
- guest 登录返回 `expires_in` 但**没有** `refresh_token`，只能过期后重新 guest 登录。
- 重新登录时带同一个 `device_id`，可以复用设备身份（E2EE 设备密钥不丢）。

### refresh 单飞（single-flight）

并发请求同时 401 时，**只能发一次 refresh**，否则 refresh token 轮换会让其他请求拿到已作废的 token。

```typescript
class Session {
  private accessToken: string | null = null;
  private refreshToken: string | null = null;
  private refreshing: Promise<string> | null = null;

  async getToken(): Promise<string | null> {
    return this.accessToken;
  }

  /** 所有请求收到 M_UNKNOWN_TOKEN 后调用；并发调用共享同一个 Promise */
  refresh(): Promise<string> {
    if (!this.refreshing) {
      this.refreshing = this.doRefresh().finally(() => {
        this.refreshing = null;
      });
    }
    return this.refreshing;
  }

  private async doRefresh(): Promise<string> {
    if (!this.refreshToken) throw new Error('no refresh token');
    const res = await call<{
      access_token: string;
      refresh_token: string;
      expires_in: number;
      device_id: string;
    }>(this.base, '/_matrix/client/v3/refresh', null, {
      method: 'POST',
      body: JSON.stringify({ refresh_token: this.refreshToken }),
    });
    this.accessToken = res.access_token;
    this.refreshToken = res.refresh_token;   // 轮换：必须覆盖旧值
    await this.persist();
    return res.access_token;
  }
}
```

请求层重试伪代码：

```typescript
async function authed<T>(path: string, init: RequestInit): Promise<T> {
  try {
    return await call<T>(base, path, await session.getToken(), init);
  } catch (e) {
    if (e instanceof MatrixApiError && e.errcode === 'M_UNKNOWN_TOKEN') {
      const token = await session.refresh();          // 单飞
      return await call<T>(base, path, token, init);  // 只重试一次
    }
    throw e;
  }
}
```

### 登出

| 端点 | 语义 |
|---|---|
| `POST /_matrix/client/v3/logout` | 作废当前 access token + 当前设备 |
| `POST /_matrix/client/v3/logout/all` | 作废该用户所有设备 |

登出后本地必须清：access token、refresh token、sync `since`、E2EE 会话库、媒体缓存索引。**不要**清用户偏好（除非产品要求）。

---

## 第 3 层：sync 循环

### 参数

`GET /_matrix/client/v3/sync`

| 参数 | 说明 |
|---|---|
| `since` | 上一轮的 `next_batch`；首次不传（initial sync） |
| `timeout` | 长轮询毫秒数，默认 `30000` |
| `filter` | filter ID 或内联 JSON；建议预创建 filter 后只传 ID |
| `full_state` | `true` 时返回全量状态（仅首轮或本地库损坏时用） |
| `set_presence` | 默认 `online`；后台可传 `offline` |

`since` 非法返回 **400 `M_BAD_PAGINATION`**，不是 401。SDK 此时应丢弃 since 做一次 initial sync，**绝不能**据此登出。

### 循环骨架

```typescript
class SyncEngine {
  private since: string | undefined;
  private running = false;
  private controller: AbortController | null = null;

  async start() {
    if (this.running) return;
    this.running = true;
    while (this.running) {
      try {
        const q = new URLSearchParams({ timeout: '30000' });
        if (this.since) q.set('since', this.since);
        if (this.filterId) q.set('filter', this.filterId);

        const res = await authed<SyncResponse>(
          `/_matrix/client/v3/sync?${q}`,
          { signal: this.controller?.signal },
        );
        this.since = res.next_batch;
        await this.persistSince(res.next_batch);
        this.emit(res);
      } catch (e) {
        if (e instanceof MatrixApiError) {
          if (e.errcode === 'M_BAD_PAGINATION') {
            this.since = undefined;          // 重新 initial sync
            continue;
          }
          if (e.status === 429) {
            await sleep(e.retryAfterMs ?? 1000);
            continue;                        // 限流不是登出
          }
        }
        await sleep(backoff());              // 网络错误：指数退避 + 抖动
      }
    }
  }

  stop() {
    this.running = false;
    this.controller?.abort();
  }
}
```

### 生命周期

| 场景 | 动作 |
|---|---|
| 应用进入前台 | `start()`；若已停止则用持久化的 `since` 续传 |
| 应用进入后台 | `stop()`；**保留** `since` |
| 网络恢复 | `start()`；用旧 `since` 拉增量，服务端会补齐 |
| 收到 `M_UNKNOWN_TOKEN` 且 refresh 失败 | 停循环、清会话、跳登录 |
| 服务端返回 `next_batch` 与上次相同 | 正常，长轮询超时；不要当成错误 |

`since` 必须**持久化**。只在内存里保存会导致每次冷启动全量同步。

---

## 第 4 层：事件管线

`/sync` 的响应是**增量快照**，不是事件流。合入顺序必须是：

```text
1. account_data（全局）      → 覆盖本地账户数据
2. presence                 → 合并在线状态
3. to_device                → 交给加密层处理（先于房间事件）
4. device_lists             → 标记需要重新查询密钥的用户
5. rooms.join/invite/leave  → 逐房间按 timeline 顺序处理
6. rooms.join.account_data  → 房间级账户数据
```

房间内顺序：

```text
state.events → timeline.events → ephemeral.events → account_data.events
```

规则：

1. **timeline 内严格按数组顺序**。`sender`/`origin_server_ts` 相同也要保持数组序。
2. **按 `event_id` 去重**。长轮询重连、initial sync 与增量都可能重复下发同一事件。
3. **`unsigned.transaction_id` 用于本地回显对齐**：发送时记下 `txn_id`，sync 回来后用 `unsigned.transaction_id` 把「发送中」气泡替换成真实事件。
4. **`limited: true` 表示 timeline 被截断**，应调用 `/messages` 向前翻页补齐，而不是假设收到了完整历史。
5. **`prev_batch` 只用于 `/messages` 的 `from`**，不要拿它当 sync 游标。

```typescript
function onSync(res: SyncResponse, store: Store) {
  for (const [roomId, room] of Object.entries(res.rooms?.join ?? {})) {
    for (const ev of room.timeline.events) {
      if (store.hasEvent(ev.event_id)) continue;
      store.appendTimeline(roomId, ev);
    }
    if (room.timeline.limited) {
      store.markGap(roomId, room.timeline.prev_batch);
    }
    const echo = room.timeline.events.find(e => e.unsigned?.transaction_id);
    if (echo) store.resolvePending(echo.unsigned!.transaction_id, echo);
  }
}
```

---

## 第 5 层：E2EE

E2EE 是**横切层**，不要塞进 `MessageApi` 里。推荐接口：

```typescript
interface CryptoAdapter {
  init(userId: string, deviceId: string): Promise<void>;
  uploadKeys(): Promise<void>;                 // /keys/upload
  queryKeys(userIds: string[]): Promise<void>; // /keys/query
  claimKeys(requests: KeyClaim[]): Promise<void>; // /keys/claim
  encryptRoom(roomId: string, event: ClientEvent): Promise<ClientEvent>;
  decryptRoom(event: MatrixEvent): Promise<MatrixEvent>;
  handleToDevice(events: ToDeviceEvent[]): Promise<void>;
  handleDeviceLists(changed: string[], left: string[]): Promise<void>;
}
```

接入顺序：

1. 登录后立刻 `uploadKeys()`，再开始 sync。
2. 首次进入加密房间前 `queryKeys(房间成员)`。
3. 发送前 `claimKeys(缺失的 one-time keys)`。
4. sync 收到 `to_device` 先交给 `handleToDevice`，再处理房间事件。
5. `device_lists.changed` 中的用户重新 `queryKeys`；`left` 中的用户可以停止加密目标追踪。

细节见 [e2ee.md](./e2ee.md)。不要自己实现 Olm/Megolm 的线格式；用经过审计的库，并只把本服务的 HTTP 契约当作传输层。

---

## 第 6 层：媒体

| 场景 | 用法 |
|---|---|
| `<img src>` / `<video src>` | 直接用 `/_matrix/media/v3/download` 或 `/_matrix/media/v3/thumbnail/...`（**不需要 token**） |
| 私有媒体 | `/_matrix/client/v1/media/download/...` + `Authorization`，拿 blob 后 `createObjectURL` |
| 上传 | `POST /_matrix/media/v3/upload`，体是原始字节，不是 multipart |
| 大文件 | `/_matrix/media/v1/upload/chunk/*` 分片系列 |

注意：

- `mxc://server/media_id` 需要拆成两段再拼 URL，`mxc://` 不是可直接请求的协议。
- `createObjectURL` 的 URL 用完必须 `revokeObjectURL`，否则长会话内存泄漏。
- 上传前用 `GET /_matrix/client/v3/media/config` 读 `m.upload.size`，超限直接本地拒绝，别等服务端 413。

---

## 幂等：txn_id 规则

带 `{txn_id}` 的端点：

```text
PUT /_matrix/client/v3/rooms/{room_id}/send/{event_type}/{txn_id}
PUT /_matrix/client/v3/rooms/{room_id}/state/{event_type}/{state_key}?...
PUT /_matrix/client/v3/sendToDevice/{event_type}/{txn_id}
```

规则：

1. `txn_id` 由客户端生成，建议 `crypto.randomUUID()`。
2. **同一次逻辑发送的所有重试必须复用同一个 `txn_id`**。服务端据此去重，返回同一个 `event_id`。
3. 用户再次点击「发送」是新的逻辑发送，用新的 `txn_id`。
4. `txn_id` 作用域是「用户 + 设备 + 端点」；跨设备不要复用。

反例：重试时重新生成 `txn_id`，会在弱网下产生重复消息。

---

## 限流与重试矩阵

| 端点类型 | 可重试 | 退避依据 |
|---|---|---|
| `GET`（sync 以外） | 是 | `retry_after_ms` → `Retry-After` → 指数退避 |
| `GET /sync` | 是 | 同上；限流不应登出 |
| 带 `txn_id` 的 `PUT` | 是（复用 txn_id） | 同上 |
| 无 `txn_id` 的 `POST`（如创建房间） | **默认不重试** | 除非业务层有去重键 |
| 媒体上传 | 是（幂等性由服务端 media_id 保证） | 同上 |
| `4xx` 除 `429` | 否 | 直接抛给业务层 |
| `5xx` | 是（有限次） | 指数退避 + 抖动 |

429 响应头：

```text
Retry-After: 2                       # 秒，向上取整
X-RateLimit-Retry-After-Ms: 1500     # 毫秒，优先用这个
X-RateLimit-Remaining: 0
```

**没有 `X-RateLimit-Limit`**，不要读它。

---

## 测试与发布

### 契约测试

1. 用 `docs/openapi/route-table.json` 生成端点清单，断言 SDK 的每个方法都映射到真实存在的路由。
2. 对每个领域模块保留至少一组「真实响应快照」，字段名以 handler 为准。
3. 覆盖四条异常路径：`M_UNKNOWN_TOKEN` → refresh 成功 / refresh 失败；`429`；`M_BAD_PAGINATION`；`5xx`。

### 本地验证

```bash
# 起服务（示例端口 8008）
curl -s http://127.0.0.1:8008/_matrix/client/v3/versions | jq '.versions[-3:]'
curl -s http://127.0.0.1:8008/.well-known/matrix/client | jq .
```

### 版本与打包

- SDK 版本与 homeserver 产品版本**解耦**。用能力探测决定行为，不要用 `6.2.0` 做分支。
- 拆包：`core`（传输 + 认证 + sync）、`crypto`（Olm/Megolm，体积大）、`extensions`（`/_matrix/vendor/v1/*`）。
- `crypto` 应为可选依赖；不启用 E2EE 的应用不应被迫加载 WASM。
- 提供 ESM + CJS 双产物；`fetch` 通过参数注入，方便 Node / React Native / 小程序替换实现。

---

## 常见错误封装

| 错误做法 | 后果 | 正确做法 |
|---|---|---|
| 用 `?access_token=` 传 token | 401 `M_UNAUTHORIZED` | 只用 `Authorization: Bearer` |
| 把 `expires_in` 当毫秒 | token 提前 1000 倍过期 | 按秒处理 |
| 401 一律清 token 跳登录 | 网络抖动导致误登出 | 仅 `M_UNKNOWN_TOKEN` 且 refresh 失败才登出 |
| 每个 401 各自 refresh | refresh token 轮换互相作废 | 单飞 refresh |
| 重试时换新 `txn_id` | 重复消息 | 复用同一 `txn_id` |
| `since` 只存内存 | 每次冷启动全量同步 | 持久化 `next_batch` |
| 用 `prev_batch` 当 sync 游标 | 重复/丢事件 | `prev_batch` 只给 `/messages` |
| 假设 `/_matrix/media/v3/*` 需要 token | 图片全部 401 | 该面是非鉴权的 |
| 按产品版本号硬编码能力 | 裁剪部署上调用不存在端点 | 读 `/versions` + `/capabilities` |
