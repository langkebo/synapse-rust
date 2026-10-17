# 服务发现与能力协商

客户端启动时的第一个阶段：确定服务端在哪个地址、支持哪些协议版本、哪些不稳定特性可用。
**不要按"产品版本号"硬编码分支**——一切以本页的响应为准。

本页所有示例均取自本机运行中的 `synapse-app`（`6.2.0`）实测返回。

## 目录

- [协议版本 `/versions`](#协议版本-versions)
- [能力 `/capabilities`](#能力-capabilities)
- [`.well-known` 发现](#well-known-发现)
- [登录流程发现 `GET /login`](#登录流程发现-get-login)
- [OIDC / 认证元数据](#oidc--认证元数据)
- [服务端版本与健康检查](#服务端版本与健康检查)
- [客户端接入顺序](#客户端接入顺序)

---

## 协议版本 `/versions`

`GET /_matrix/client/v3/versions`（同时提供 `GET /_matrix/client/versions` 无版本别名）

**认证**：不需要。响应带 `Cache-Control: public, max-age=600, s-maxage=3600, stale-while-revalidate=600`，
客户端可缓存 10 分钟。

**响应（实测）**：

```json
{
  "versions": ["v1.1", "v1.2", "v1.3", "v1.4", "v1.5", "v1.6", "v1.7", "v1.8", "v1.9",
               "v1.10", "v1.11", "v1.12", "v1.13", "v1.14", "v1.15", "v1.16", "v1.17"],
  "unstable_features": {
    "m.lazy_load_members": true,
    "m.require_identity_server": false,
    "m.supports_login_via_phone_number": false,
    "org.matrix.msc1763": true,
    "org.matrix.msc3245": false,
    "org.matrix.msc3266": true,
    "org.matrix.msc3814": true,
    "org.matrix.msc3873": true,
    "org.matrix.msc3882": true,
    "org.matrix.msc3886.sliding_sync": true,
    "org.matrix.msc3912": true,
    "org.matrix.msc3981": true,
    "org.matrix.msc3981.stable": true,
    "org.matrix.msc3983": true,
    "org.matrix.msc4108": true,
    "org.matrix.msc4140": true,
    "org.matrix.msc4143": true,
    "org.matrix.msc4155": true,
    "org.matrix.msc4186": true,
    "org.matrix.msc4445.initial_sync_timeline_topological_ordering": true,
    "org.matrix.msc4446": true,
    "org.matrix.simplified_msc3575": true,
    "uk.tcpip.msc4133": true
  }
}
```

### 与本仓功能对应的关键开关

| `unstable_features` 键 | `true` 意味着 | 客户端该做什么 |
|---|---|---|
| `org.matrix.msc3886.sliding_sync` / `org.matrix.simplified_msc3575` | Sliding Sync 可用（`POST /_matrix/client/unstable/org.matrix.simplified_msc3575/sync` 等） | 可以走简化滑动同步；否则退回 `/sync` |
| `org.matrix.msc4140` | **延迟事件**（`…/org.matrix.msc4140/delayed_events`）可用 | 见 [extensions.md](./extensions.md) |
| `org.matrix.msc3814` | 脱水设备（dehydrated device）可用 | 见 [e2ee.md](./e2ee.md) |
| `org.matrix.msc4186` | Sliding Sync 的 `pos` 令牌语义 | 用 `M_UNKNOWN_POS` 处理过期 |
| `uk.tcpip.msc4133` | 扩展 profile 字段（`/profile/{user_id}/{key_name}`） | 见 [authentication.md](./authentication.md) |
| `org.matrix.msc4155` | 账户级邀请策略 | 邀请权限配置 |
| `m.lazy_load_members` | `/sync?filter=` 支持懒加载成员 | 可省带宽 |

> **关于 `org.matrix.msc4354`（Sticky 事件）**：本仓源码在 `/versions.unstable_features`
> 中声明该键，取值由路由面推导。该声明是**较新加入**的——运行中若部署的是此前的二进制，
> 该键不会出现（本机 `synapse-app` 即为此种情况）。判断 Sticky 是否可用请以
> `/_matrix/client/unstable/org.matrix.msc4354/rooms/{room_id}/sticky_events` 是否 404 为准，
> 见 [extensions.md](./extensions.md)。

---

## 能力 `/capabilities`

`GET /_matrix/client/v3/capabilities`

**认证**：**可选**。不带 token 也能读，但 5 项与账户相关的能力**只在带 token 时出现**
（`build_capabilities_response(authenticated)`）：

| 仅认证后出现 | 含义 |
|---|---|
| `io.hula.friends` | 好友系统（`/_matrix/vendor/v1/friends/*`）可用 |
| `m.sso` | 含 `providers` 列表的 SSO 能力 |
| `external_services` | 外部服务注册可用 |
| `io.hula.voice_extended` | 扩展语音消息 |
| `io.hula.burn_after_read` | 阅后即焚 |

**响应（实测整份）**：

```json
{
  "capabilities": {
    "io.element.msc4452.preview_url": { "enabled": false },
    "m.3pid_changes": { "enabled": true },
    "m.change_password": { "enabled": true },
    "m.forget_forced_upon_leave": { "enabled": true },
    "m.preview_url": { "enabled": false },
    "m.profile_fields": { "enabled": true },
    "m.room.suggested": { "enabled": true },
    "m.room.summary": { "enabled": false },
    "m.room_versions": {
      "available": { "1": "stable", "2": "stable", "3": "stable", "4": "stable", "5": "stable",
                     "6": "stable", "7": "stable", "8": "stable", "9": "stable", "10": "stable",
                     "11": "stable", "12": "stable" },
      "default": "12"
    },
    "m.set_avatar_url": { "enabled": true },
    "m.set_displayname": { "enabled": true },
    "m.thread": { "enabled": true },
    "m.voice": { "enabled": true },
    "org.matrix.msc3720.account_status": { "enabled": false }
  },
  "unstable_features": {
    "io.hula.burn_after_read": true,
    "io.hula.friends": true,
    "org.matrix.msc3245.voice": true,
    "org.matrix.msc3873": true,
    "org.matrix.msc3886.sliding_sync": true,
    "org.matrix.msc3983.thread": true,
    "org.matrix.msc4108": true,
    "org.matrix.msc4155": true,
    "org.matrix.msc4186": true,
    "org.matrix.simplified_msc3575": true,
    "uk.tcpip.msc4133": true
  }
}
```

### 逐项语义

| 能力 | 本仓行为 |
|---|---|
| `m.room_versions` | `available` 为**房间版本 → 稳定度**映射；`default` 是 `createRoom` 不指定 `room_version` 时的取值（当前 `12`）。创建不支持的房间版本返回 `M_UNSUPPORTED_ROOM_VERSION`（400），与默认版本不兼容返回 `M_INCOMPATIBLE_ROOM_VERSION`（400） |
| `m.change_password` | `false` 时客户端应隐藏改密入口（`POST /account/password` 仍会按既有密码校验处理） |
| `m.set_displayname` / `m.set_avatar_url` | 对应 `PUT /profile/{user_id}/displayname`、`/avatar_url` 是否开放 |
| `m.3pid_changes` | 绑定/解绑邮箱手机是否开放 |
| `m.forget_forced_upon_leave` | 支持 `POST /rooms/{room_id}/leave?forget=true`（离开与遗忘同一事务） |
| `m.room_summary` | 房间摘要扩展。⚠️ **当前恒为 `false`**，见下方「已知能力声明缺陷」 |
| `m.room.suggested` | `m.room.suggested` 事件支持 |
| `m.thread` | 线程（`/_matrix/client/v1/rooms/{id}/threads`） |
| `m.profile_fields` | 扩展 profile 字段（MSC4133） |
| `m.preview_url` / `io.element.msc4452.preview_url` | URL 预览。**两者取值相同**，均为 `experimental.msc4452_enabled`；为 `false` 时 `GET /_matrix/media/v3/preview_url` 返回 403 |
| `org.matrix.msc3720.account_status` | 账户状态（`POST /_matrix/client/unstable/org.matrix.msc3720/account_status`，受 `experimental.msc3720_enabled` 控制；关闭时该路由返回 403） |

> 上表与 `capabilities` 是**同一份事实**：`CapabilityGovernance` 用路由表（`declared_ledger_all()`）
> 反推能力，因此"声明为 true 但路由不存在"在本仓不会发生。客户端可放心把能力当作开关
> ——**但下面两项除外**，它们的探针路径本身写错了。

### ⚠️ 已知能力声明缺陷（探针路径与真实路由不一致）

把 `capability_governance.rs` 里全部 19 个 `manifest_has_route(...)` 探针与真实路由逐条对账
（默认 profile 959 条 + `voice-extended` 等特性下的路由），**2 条**探针指向了不存在的路径
（其中 1 条只在测试里用），另有 1 条曾被误判为缺陷（其实是 feature 未启用）：

| 能力 / 函数 | 探针写的路径 | 真实注册的路径 | 对外影响 |
|---|---|---|---|
| `m.room.summary`（`room_summary_capability`） | ~~`GET /_matrix/client/v3/rooms/{room_id}/summary`~~ → **已修为** `GET /_matrix/client/v1/rooms/{room_id}/summary` | 摘要实际注册在 v1 与 vendor 两处，**没有 v3** | 修复前恒为 `false`（能力说没有、接口其实可用）；修复后随路由面为 `true` |
| `widget_capability` | ~~`POST /_matrix/client/v1/widgets`~~ → **已修为** `POST /_matrix/vendor/v1/widgets` | widgets 只在 vendor 前缀下 | 该函数未参与 `/capabilities` 响应（仅测试用），修复前恒 `false` |
| `io.hula.voice_extended`（`voice_extended_capability`） | `GET /_matrix/vendor/v1/voice/config` | **存在**，但只在 `voice-extended` feature 下注册（默认特性集不含） | 探针**正确**：默认构建报 `false` 是对的；曾被误判为"路径不存在"——那是拿默认 profile 的路由表去比特性内路由 |

**客户端结论**：优先按能力位判断；若遇到旧二进制（`m.room.summary` 仍为 `false`），
可直接调用 `GET /_matrix/client/v1/rooms/{room_id}/summary`，以 404 判断不可用。

> 前两条已修复（探针改指真实路由，并加回归测试
> `test_room_summary_and_widget_probes_point_at_served_routes`：正向断言"路由在服务 ⇒ 能力为 true"，
> 反向断言"幽灵路径不驱动能力"）。部署旧二进制时仍会看到 `false`——那是版本差异，不是文档错误。

---

## `.well-known` 发现

| 端点 | 认证 | 实测响应 |
|---|---|---|
| `GET /.well-known/matrix/client` | 无 | `{"m.homeserver":{"base_url":"https://matrix.test"}}` |
| `GET /.well-known/matrix/server` | 无 | `{"m.server":"matrix.test:8448"}` |
| `GET /.well-known/matrix/support` | 无 | 支持联系信息（`contacts` / `support_page`） |
| `GET /.well-known/openid-configuration` | 无 | OIDC 发现文档（`issuer`、`authorization_endpoint`、`token_endpoint`、`userinfo_endpoint`、`jwks_uri`） |
| `GET /.well-known/jwks.json` | 无 | OIDC 签名公钥集 |

客户端接入应遵循：**用户输入 `example.org` → 拉 `https://example.org/.well-known/matrix/client`
→ 取 `m.homeserver.base_url` 作为 API 基址**。`m.homeserver.base_url` 与 `m.server` 是两回事
（前者给客户端，后者给联邦）。

---

## 登录流程发现 `GET /login`

`GET /_matrix/client/v3/login` **返回可用登录类型**（实测）：

```json
{
  "flows": [
    { "type": "m.login.password" },
    { "type": "m.login.token" },
    { "type": "m.login.cas" },
    {
      "type": "m.login.sso",
      "identity_providers": [
        { "brand": "saml", "id": "saml", "name": "SAML" },
        { "brand": "cas", "id": "cas", "name": "CAS" }
      ]
    }
  ]
}
```

- 返回的 `type` 是**服务端实际能处理的**，随配置变化（`m.login.sso` 的 `identity_providers`
  随 SAML/CAS 开关增减）。
- `m.login.application_service` **不在此列表**（与上游一致），但 `POST /login` 仍接受它。
- 客户端应遍历 `flows` 渲染登录方式，而不是内置固定列表。

> 注册的 UIA 流程与登录**不同**：`POST /register` 不带 `auth` 时返回 **401** 且体为
> `{"flows":[{"stages":["m.login.dummy"]},{"stages":["m.login.password"]}],"params":{},"session":"<uuid>"}`。
> 详见 [authentication.md](./authentication.md)。

---

## OIDC / 认证元数据

| 端点 | 行为 |
|---|---|
| `GET /_matrix/client/v1/auth_metadata`（MSC2965） | **仅在启用 OIDC/SSO 时**返回元数据（含 `ui_locales_supported`）。未启用时实测返回 **404 `M_UNRECOGNIZED`**，`error` 为 `Authentication metadata is not available because OIDC/SSO is not enabled` |
| `GET /_matrix/client/unstable/org.matrix.msc2965/auth_metadata` | 同上，不稳定命名空间 |
| `GET /.well-known/openid-configuration` | 独立于上者，**默认配置下也返回 200** |

**语言支持**：服务端只提供 `en`。启用 OIDC 时 `ui_locales_supported` 为 `["en"]`；
**未启用 OIDC 时没有该字段可读**——客户端不得假设该字段一定存在，界面文案一律客户端本地化。

---

## 服务端版本与健康检查

| 端点 | 认证 | 响应 |
|---|---|---|
| `GET /_matrix/server_version` | 无 | `{"python_version":"Rust","server_name":"matrix.test","server_version":"6.2.0"}` |
| `GET /_synapse/admin/v1/server_version` | 管理员 | `{"server_version":"6.2.0","implementation":"synapse-rust"}` |
| `GET /health` | 无 | `{"status":"ok","rate_limit_config":{...},"timestamp":"<RFC3339>"}` |
| `GET /_health` | 无 | `{"checks":{"database":{"status":"healthy","duration_ms":0,"message":"Connection successful"},"rate_limit_config":{...}}}` |

`/health` 与 `/_health` 是**给编排/探活**用的：前者聚合出 `status`，后者给出逐项 `checks`。
客户端不应把它们当作业务接口。

---

## 客户端接入顺序（推荐）

```typescript
// 1) 解析用户输入的域名 → 真实 API 基址
async function resolveHomeserver(input: string): Promise<string> {
  const url = input.includes('://') ? input : `https://${input}`;
  try {
    const wk = await fetch(`${url}/.well-known/matrix/client`)
      .then(r => (r.ok ? r.json() : null));
    if (wk?.['m.homeserver']?.base_url) return wk['m.homeserver'].base_url;
  } catch { /* 忽略，退回直连 */ }
  return url;
}

// 2) 读版本与能力（能力需要 token 才完整，登录后要再读一次）
const versions = await fetch(`${base}/_matrix/client/v3/versions`).then(r => r.json());
const slidingSync = versions.unstable_features['org.matrix.simplified_msc3575'] === true;
const supportsDelayed = versions.unstable_features['org.matrix.msc4140'] === true;

// 3) 登录后补读能力（这一步才能看到 io.hula.friends 等 5 项）
const caps = await call(base, '/_matrix/client/v3/capabilities', accessToken);
const friendsEnabled = caps.capabilities['io.hula.friends']?.enabled === true;
const canThread = caps.capabilities['m.thread']?.enabled === true;
```

**常见误用**

- ❌ 用 `server_version` 判断功能（版本号与能力无关，能力随配置变化）。
- ❌ 假设 `/capabilities` 一定含 `m.sso`（未认证时不含）。
- ❌ 把 `m.room_versions.available` 当数组（它是对象，键是版本号字符串）。
