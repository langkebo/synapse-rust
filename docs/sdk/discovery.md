# 服务发现与能力协商

客户端启动时的第一个阶段：确定服务端在哪个地址、支持哪些协议版本、哪些不稳定特性可用。
**不要按"产品版本号"硬编码分支**——一切以本页的响应为准。

本页所有示例均取自本仓 `6.2.0` 源码在默认配置（default profile）下的实际生成结果。

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
    "org.matrix.msc3245": true,
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
    "org.matrix.msc4354": true,
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
> 中声明该键，取值由**路由面**推导——`POST /_matrix/client/unstable/org.matrix.msc4354/rooms/{room_id}/sticky_events`
> 注册即 `true`（默认构建为 `true`；源码里没有 `msc4354` 配置开关）。
> 仍建议以该端点是否 404 作为最终判据，见 [extensions.md](./extensions.md)。

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
    "m.room.summary": { "enabled": true },
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
| `m.room.summary` | 房间摘要（`GET /_matrix/client/v1/rooms/{room_id}/summary` + `/_matrix/vendor/v1` 同名路径）。由 v1 路由存在性推导，默认 `true`；**没有 v3 路径** |
| `m.room.suggested` | `m.room.suggested` 事件支持 |
| `m.thread` | 线程（`/_matrix/client/v1/rooms/{id}/threads`） |
| `m.profile_fields` | 扩展 profile 字段（MSC4133） |
| `m.preview_url` / `io.element.msc4452.preview_url` | URL 预览。**两者取值相同**，均为 `experimental.msc4452_enabled`；为 `false` 时 `GET /_matrix/media/v3/preview_url` 返回 403 |
| `org.matrix.msc3720.account_status` | 账户状态（`POST /_matrix/client/unstable/org.matrix.msc3720/account_status`，受 `experimental.msc3720_enabled` 控制；关闭时该路由返回 403） |

> 上表与 `capabilities` 是**同一份事实**：`CapabilityGovernance` 用路由表
> （`declared_ledger_all()`，即最宽特性面）反推能力，因此"声明为 true 但路由不存在"
> 在本仓不会发生。少数能力还会叠加配置开关（`declare_private_extensions`、
> `experimental.msc3720_enabled`、`experimental.msc4452_enabled`）。
> 客户端可把能力位当作开关直接使用。

### 能力探针口径（历史缺陷已修复）

历史上曾有两条探针指向了不存在的路径，导致能力位与真实路由相反；现已改为指向实际在服务的路由，
并加了回归测试 `test_room_summary_and_widget_probes_point_at_served_routes`
（正向断言"路由在服务 ⇒ 能力为 true"，反向断言"幽灵路径不驱动能力"）：

| 能力 / 函数 | 探针路径 | 说明 |
|---|---|---|
| `m.room.summary`（`room_summary_capability`） | `GET /_matrix/client/v1/rooms/{room_id}/summary` | 摘要注册在 v1 与 vendor 两处，**没有 v3**；默认 `true` |
| `widget_capability` | `POST /_matrix/vendor/v1/widgets` | widgets 只在 vendor 前缀下；该函数未参与 `/capabilities` 响应（仅测试用） |
| `io.hula.voice_extended` | `GET /_matrix/vendor/v1/voice/config` | 路由常驻注册；还需 `experimental.declare_private_extensions = true`（默认 `true`） |

**客户端结论**：优先按能力位判断；若要兜底，可直接调用目标端点，以 404 判断不可用。
部署旧二进制时可能看到与当前源码不同的取值——那是版本差异，不是文档错误。

---

## `.well-known` 发现

| 端点 | 认证 | 实测响应 |
|---|---|---|
| `GET /.well-known/matrix/client` | 无 | `{"m.homeserver":{"base_url":"..."}}`；配置了 `map_style_url` 时额外含 `m.tile_server.map_style_url` |
| `GET /.well-known/matrix/server` | 无 | `{"m.server":"host:8448"}`（端口取 `federation_port`） |
| `GET /.well-known/matrix/support` | 无 | 配置了 `server.support_url` 时为 `{"support_page":"https://..."}`；否则为 `{}`（**不返回 `contacts`**） |
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
    { "type": "m.login.token" }
  ]
}
```

- **默认构建**只返回上面两种。`identity_providers` 与 `GET /capabilities` 的
  `m.sso.providers` **同源**（`capability_governance::sso_provider_brands`），两边口径永远一致：
  - `saml` —— 编译了 `saml-sso` **且**运行时 SAML 可用（`saml.enabled` 且已配置
    `metadata_url` / `metadata_xml`，与 `SamlConfig::is_enabled()` 完全一致）；
  - `oidc` —— 运行时配置了**外部** OIDC IdP（`oidc.enabled` + `issuer` + `client_id`）；
  - `cas` —— 只看编译特性 `cas-sso`（CAS 没有配置面，编进来即注册路由）。
  - 内置 OIDC provider（`builtin-oidc`）**不出现在** `identity_providers` / `m.sso.providers`，
    它通过独立的 `m.login.oidc` 流表达（且需运行时配置了内置 provider）。

- 参考部署（`--features all-extensions`，SAML 未启用、无外部 OIDC）编译了 `cas-sso`，
  因此 `flows` 形如：

```json
{
  "flows": [
    { "type": "m.login.password" },
    { "type": "m.login.token" },
    { "type": "m.login.cas" },
    { "type": "m.login.sso",
      "identity_providers": [
        { "id": "cas", "name": "CAS", "brand": "cas" }
      ] }
  ]
}
```

- provider 已按「配置启用 + 路由已编译」收敛，`identity_providers` 中每一项都应能真正
  发起跳转；仍建议 SDK 对跳转失败（4xx/5xx）降级到密码登录，而不是崩溃。
- 返回的 `type` 随构建特性与配置变化。
- `m.login.application_service` **不在此列表**（与上游一致），但 `POST /login` 仍接受它。
- 客户端应遍历 `flows` 渲染登录方式，而不是内置固定列表。
- 跳转端点：`GET /_matrix/client/v3/login/sso/redirect/saml?redirectUrl=` 与
  `GET /_matrix/client/v3/login/sso/redirect/cas?redirect_after=`
  （详见 [authentication.md](./authentication.md#saml-20-单点登录)）。

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
