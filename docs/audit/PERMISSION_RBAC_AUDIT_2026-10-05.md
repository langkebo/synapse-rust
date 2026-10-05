# 权限体系审查与越权测试报告

> 项目：synapse-rust v6.2.0（Rust Matrix homeserver）
> 报告日期：2026-10-05
> 测试环境：Docker 部署栈（matrix.test / synapse-nginx / app / postgres / redis 均 healthy）
> 测试对象：`super_admin`、`admin`、普通账户三种角色的权限体系与越权风险
> 结论摘要：**发现 1 个 P0、2 个 P1、1 个 P2 权限/审计缺陷，另 2 个 P3 字段/设计缺陷；垂直越权拦截整体有效，水平越权隔离完整。**

---

## 一、测试范围与方法

| 维度 | 方法 | 覆盖 |
|------|------|------|
| 权限清单梳理 | 静态代码审查（路由注册、授权判定函数、角色枚举） | 全部管理端与客户端路由 |
| 静态一致性 | 逐路由核对 `route_layer` 挂载、handler 内抽取器、字符串匹配规则 | `synapse-web/src/routes/admin/*`、`middleware/auth.rs`、`utils/admin_auth.rs` |
| 动态垂直越权 | 三角色真实 token 打真实接口（普通→管理员、管理员→超管） | 67 条用例 |
| 动态水平越权 | 用户1 token 访问用户2 的资源（account_data/filter/rooms/devices） | 9 条越权 + 3 条本人对照 |
| 敏感操作与审计 | 无 MFA / 非法 MFA / 正确 MFA 三态验证 + `audit_events` 落库核对 | 8 条 MFA 用例 + 审计行分析 |
| 边界与伪造 | 无 token / 伪造 token / 非本机访问 register | 5 条用例 |

**判定基准（权威代码路径）**：[`authorize_admin_from_services`](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/utils/admin_auth.rs#L31-L113) —— 所有管理端请求的最终授权判定均收敛于此，供 `admin_auth_middleware` 与 `AdminUser` 抽取器共同调用。

---

## 二、角色模型与权限判定链路

### 2.1 双层角色模型

项目**没有独立角色表、没有权限位**，角色由两个字段叠加表达：

| 层 | 字段 | 类型 | 语义 |
|----|------|------|------|
| 硬门槛 | `users.is_admin` | BOOLEAN NOT NULL | 是否为管理员（否 → 直接 403） |
| 细粒度 | `users.user_type` | TEXT NULL | 角色名，缺失归一化为 `none` |

`normalize_admin_role`（[L172-188](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/utils/admin_auth.rs#L172-L188)）把 `None` 归一为 `none`；`super_admin` / `admin` 原样保留；其余转小写。

**可识别角色枚举**：`super_admin`、`admin`、`auditor`、`security_admin`、`user_admin`、`media_admin`、`none`（未识别值 → 兜底 `_ => false`，[L392](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/utils/admin_auth.rs#L392)）。

### 2.2 判定链路（6 步）

```
Bearer token → validate_token → is_admin?
  ├─ false → 403 "Admin access required"（写审计前返回）
  └─ true  → DB 复查 user.is_admin
       ├─ false → 403 "Admin access has been revoked"
       └─ true  → normalize_admin_path → normalize_admin_role
                   → is_role_allowed（先判 super_admin 独占端点，再按角色前缀白名单）
                   → rbac_enabled? → 写审计
                   → 拒绝? → 403 "Admin role '{role}' is not allowed..."
                   → 敏感请求? → 校验 MFA（x-admin-mfa-code）
                   → 放行
```

### 2.3 三个关键判定函数

- **`is_super_admin_only_endpoint`**（[L197-252](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/utils/admin_auth.rs#L197-L252)）：基于**路径字符串匹配**识别超管独占端点；非 super_admin 角色命中即拒。
- **`is_role_allowed`**（[L254-394](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/utils/admin_auth.rs#L254-L394)）：super_admin 全放行；其余角色按前缀白名单；`_ => false` 默认拒绝。
- **`is_sensitive_admin_request`**（[L396-404](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/utils/admin_auth.rs#L396-L404)）：所有 `POST/PUT/PATCH/DELETE` + `security/server/media quarantine` 前缀 → 需 MFA。

---

## 三、角色权限矩阵

> 依据 `is_role_allowed` 前缀白名单 + `is_super_admin_only_endpoint` 独占清单整理。`✅`=允许，`❌`=拒绝，`—`=不适用。

| 资源/操作域 | 端点前缀（归一化后） | super_admin | admin | auditor | security_admin | user_admin | media_admin | none（普通） |
|-------------|---------------------|:--:|:--:|:--:|:--:|:--:|:--:|:--:|
| 用户管理 | `/_synapse/admin/v1/users*`、`/v2/users*` | ✅ | ✅ | ❌ | ❌ | ✅ | ❌ | ❌ |
| 会话/令牌 | `/_synapse/admin/v1/user_sessions` | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 用户统计 | `/_synapse/admin/v1/user_stats` | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ |
| 账户详情 | `/_synapse/admin/v1/account/` | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 通知 | `/_synapse/admin/v1/notifications` | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 媒体管理 | `/_synapse/admin/v1/media` | ✅ | ✅ | ❌ | ❌ | ❌ | ✅ | ❌ |
| 房间管理 | `/_synapse/admin/v1/rooms*`、`/room/*`、`shutdown_room` | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 房间统计 | `/_synapse/admin/v1/room_stats/` | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ |
| 注册令牌（读） | `.../registration_tokens`（GET） | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
| **注册令牌（写）** | `.../registration_tokens`（POST/DELETE） | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 联邦目标（读） | `/_synapse/admin/v1/federation/*`（GET） | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
| **联邦敏感操作** | `federation/resolve`、`blacklist`、`cache/clear` | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 审计日志 | `/_synapse/admin/v1/audit/*` | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ |
| 安全模块 | `/_synapse/admin/v1/security/*` | ✅ | ❌ | ❌ | ✅ | ❌ | ❌ | ❌ |
| 服务器信息 | `/_synapse/admin/info` | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 服务器通告 | `.../send_server_notice` | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| **批量建用户** | `.../users/batch` | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| **设置他人管理员** | `.../users/{id}/admin`（PUT）、`make_admin` | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| **批量删设备** | `.../delete_devices`（**见缺陷 P1-1**） | ✅ | ❌* | ❌ | ❌ | ❌ | ❌ | ❌ |
| **清历史/保留策略** | `.../purge*`、`.../retention*` | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 客户端 API（本人资源） | `/_matrix/client/v3/user/{self}/*` 等 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅（限本人） |

> `*` 意图为 super_admin 独占，但实现存在字符串匹配错位，普通 admin 实际可绕过（见 [P1-1](#p1-1-垂直越权--devicesdelete-字符串匹配错位)）。

**边界清晰度评估**：整体清晰。三类角色（super_admin / admin / 普通）在管理端与客户端之间边界明确；但细粒度角色（auditor/security_admin/user_admin/media_admin）**无独立测试账户、无动态验证**，其白名单分支仅经静态审查，且**部分分支（如 media_admin）仅在个别前缀生效**，属于"声明存在但未充分验证"的风险区。

---

## 四、静态审查：授权一致性与完整性

### 4.1 路由挂载一致性

| 路由组 | 鉴权方式 | 一致性 |
|--------|----------|--------|
| `create_admin_module_router` protected 组（[mod.rs L68-80](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/routes/admin/mod.rs#L68-L80)） | `route_layer(admin_auth_middleware)` | ✅ 统一 |
| CAS 管理路由（[cas.rs L159-180](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/routes/cas.rs#L159-L180)） | `admin_auth_middleware` + `normalize_admin_path` | ✅ 一致（角色判定与标准路由同源） |
| 密钥轮换 `/_matrix/client/v1/keys/rotation/*`（[key_rotation.rs L263-280](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/routes/key_rotation.rs#L263-L280)） | **无 route_layer**，靠 handler 参数 `AdminUser` 抽取器 | ⚠️ 依赖 handler 显式声明，漏写即无鉴权 |
| `register` 路由（[register.rs L34-35](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/routes/admin/register.rs#L34-L35)） | **在 route_layer 之外**（[mod.rs L82](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/routes/admin/mod.rs#L82)），靠 `LocalhostGuard` + HMAC + captcha | ⚠️ 见 [P3-2](#p3-2-敏感操作设计残留register-路由脱离统一授权链) |

### 4.2 敏感操作覆盖

`is_sensitive_admin_request`（所有写方法 + security/server/media quarantine 前缀）→ 需 MFA，动态验证有效（见第六节）。**但**：`is_super_admin_only_endpoint` 采用**路径字符串匹配**，与真实路由注册名存在**两处不一致**，是本次 P0/P1 漏洞的共性根因：

1. `path.contains("/delete_devices")` —— 真实管理路由为 `/devices/delete`（[P1-1](#p1-1-垂直越权--devicesdelete-字符串匹配错位)）。
2. `login_as_user` 完全未纳入超管独占判定（[P0-1](#p0-1-垂直越权--login_as_user-可冒充-super_admin)）。

### 4.3 数据层授权

管理端数据访问均在 handler 内通过 `AdminUser` 抽取器触发一次 `authorize_admin_from_services`；客户端数据访问在 handler 内显式比对 `user_id`（如 account_data/filter 的 `Cannot ... for other users`）。**未发现绕过授权直达 SQL 的管理端 handler。**

---

## 五、动态越权测试结果

**总用例：67 条 → PASS 64 / FAIL 3**（FAIL 全部指向同一缺陷 P1-1）。

| 分组 | 用例数 | PASS | FAIL | 说明 |
|------|:--:|:--:|:--:|------|
| A. 垂直：普通 → 管理员 | 17 | 17 | 0 | 全部 403 "Admin access required" |
| B. 垂直：管理员 → 超管 | 15 | 12 | **3** | 3 条设备批量注销路由返回 404 而非 403 |
| C. 正向：管理员放行 | 6 | 6 | 0 | 均 200/404 |
| D. 正向：超管放行 | 4 | 4 | 0 | 均 200 |
| E. 水平：用户1 → 用户2 | 9 | 9 | 0 | 全部 403（含 3 条本人对照 200） |
| F. MFA 三态 | 8 | 8 | 0 | 无/非法 → 403；正确 → 放行 |
| G. 边界 | 5 | 5 | 0 | 无 token 401、伪造 token 401、非本机 register 403 |

### 5.1 垂直越权（有效拦截）

- 普通账户访问 17 个管理端点（用户列表、房间、审计、注册令牌、联邦、服务器信息、批量建用户、密钥轮换等）→ 全部 `403 M_FORBIDDEN "Admin access required"`。
- 管理员访问 12 个超管独占端点（服务器信息、设置管理员、批量建用户、注册令牌写、联邦 resolve/blacklist/cache、purge、retention、send_server_notice）→ 全部 `403 "Admin role 'admin' is not allowed to access this resource"`。

### 5.2 水平越权（隔离完整）

用户1（`@test2`）访问用户2（`@test4`）的资源 → 全部 403：

| 用例 | 结果 |
|------|------|
| 读/写他人 account_data | `403 Cannot get/set account data for other users` |
| 创建他人 filter | `403 Cannot create filter for other users` |
| 读他人房间消息/状态/成员 | `403 You must be a member ...` |
| 向他人房间发消息 | `403 Insufficient permission to send this event` |
| 退出他人房间 | `403 Cannot leave a room you are not a member of` |

本人对照 3 条均 200，确认非环境假阳性。

### 5.3 三次 FAIL（真实漏洞证据）

| # | 请求 | 期望 | 实际 |
|---|------|:--:|:--:|
| 1 | AD `POST /_synapse/admin/v1/users/@test4:matrix.test/delete_devices` | 403 | **404 M_UNRECOGNIZED** |
| 2 | AD `POST /_synapse/admin/v1/users/@nobody:matrix.test/devices/delete` | 403 | **404 M_NOT_FOUND "User not found"** |
| 3 | AD `POST /_synapse/admin/v1/users/@nobody:matrix.test/devices/DEVID/delete` | 403 | **404 M_NOT_FOUND "User not found"** |

> #2/#3 的 404 来自 **业务 handler**（`User not found`），即请求已**穿透 RBAC 抵达业务逻辑**——这是越权成立的直接证据。

---

## 六、敏感操作与审计日志验证

### 6.1 MFA 控制（有效）

| 场景 | 结果 |
|------|------|
| 管理员敏感写，无 `x-admin-mfa-code` | 403 `Sensitive admin operation requires MFA code` |
| 管理员敏感写，错误 MFA 码 | 403 `Invalid admin MFA code` |
| 管理员/超管敏感写，正确 TOTP | 200 / 404（业务态） |
| 只读管理请求，无 MFA | 200（符合设计：仅写操作需 MFA） |

### 6.2 审计日志（有留痕，但存在 4 类质量问题）

已确认敏感操作**有审计落库**（`audit_events` 表），但存在以下缺陷：

1. **客户端审计 result 恒为 success**（[P1-2](#p1-2-审计完整性客户端审计结果恒为-success)）
2. **单次请求最多 4 条重复审计**（[P2-1](#p2-1-审计重复与膨胀)）
3. **字段/关联缺陷**：无独立 IP 列、action 命名不统一、`auth.*` 事件 request_id 为空（[P3-1](#p3-1-审计字段与关联缺陷)）
4. **登录冒充事件留痕正确**：`admin.login_as_user` 事件携带 actor=发起 admin、details 含 `target_user`/`admin_role`/`target_is_admin`，可溯源（但**该操作本身不应被低权限角色执行**，见 P0-1）。

---

## 七、缺陷清单（证据 / 根因 / 修复建议）

### P0-1（垂直越权）`login_as_user` 可冒充 super_admin

**严重级别**：P0 / 高危

**现象**：普通 `admin` 调用 `POST /_synapse/admin/v1/users/{super_admin_id}/login` + MFA → **200**，返回一个属于目标 super_admin、`admin:true` 的 access_token；用该 token 访问超管独占端点成功。

**实测证据**（原文）：

```
[1]  AD POST /users/@apitest_admin:matrix.test/login -> 200
     {"access_token":"eyJ...","device_id":"ZigZu0dfj4","user_id":"@apitest_admin:matrix.test"}
[1c] GET /_synapse/admin/info  (使用提权 token) -> 200
     {"implementation":"synapse-rust","server_name":"matrix.test","server_version":"6.2.0"}
[1d] GET /_synapse/admin/v1/user_stats (使用提权 token) -> 200
```

审计留痕（2 条）：

```
@admin:matrix.test | admin.login_as_user | @apitest_admin:matrix.test | success |
{"device_id":"ZigZu0dfj4","admin_role":"admin","target_user":"@apitest_admin:matrix.test","target_is_admin":true}
```

**根因**：[`login_as_user`](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/routes/admin/user.rs#L577-L642) 仅要求 `AdminUser`（任意管理员即可），**未做超管独占校验**；且直接用 `user.is_admin`（目标用户的权限）签发 token（[L590-604](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/routes/admin/user.rs#L590-L604)）。对比 `create_or_update_user_v2` 已有 `ensure_super_admin_for_privilege_change`（[L763-765](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/routes/admin/user.rs#L763-L765)）——**同类提权操作保护不一致**。

**影响**：普通 admin 可完全冒充 super_admin 会话，实现完整权限提升，绕过整个 RBAC 体系。

**修复建议**：在 `login_as_user` 开头增加与 `create_or_update_user_v2` 一致的保护——当目标用户 `is_admin == true` 或其 `user_type == "super_admin"` 时，要求发起者 `admin.role == "super_admin"`：

```rust
if user.is_admin {
    crate::routes::admin::ensure_super_admin_for_privilege_change(&admin)?;
}
```

（建议同时将该端点纳入 `is_super_admin_only_endpoint` 的显式判定，作为纵深防御。）

---

### P1-1（垂直越权）`/devices/delete` 字符串匹配错位

**严重级别**：P1 / 中高危

**现象**：普通 `admin` 可对**任意用户（含其他 admin / super_admin）**执行 `POST /_synapse/admin/v1/users/{user_id}/devices/delete`，批量注销目标全部会话。

**实测证据**（原文）：

```
[2] AD POST /_synapse/admin/v1/users/@nobody:matrix.test/devices/delete
    -> 404 M_NOT_FOUND "User not found"   （期望 403）
```

审计铁证（同一 request_id `req-10363061-bcbe-489f-b07e-18007bdfe0cb`）：

```
admin.post | success            （authorize 层判定为 ALLOWED，×2）
POST /_synapse/admin/v1/users/@nobody:matrix.test/devices/delete | failure
                                （middleware 层，因 handler 返回 404）
```

**根因**：[`is_super_admin_only_endpoint`](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/utils/admin_auth.rs#L214) 检查 `path.contains("/delete_devices")`，而该字面量**只对应客户端端点** `POST /_matrix/client/v3/delete_devices`（[device.rs L242](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/routes/device.rs#L242)）；真实管理路由为 `/devices/delete`（[user.rs L53-56](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/routes/admin/user.rs#L53-L56)）与兼容路由 `/devices/{device_id}/delete`（[L61-64](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/routes/admin/user.rs#L61-L64)），**均不含该子串**，因此 admin 被 `path.starts_with("/_synapse/admin/v1/users")` 白名单放行。

设计意图（历史文档 [defects_integration_test_analysis.md L83](file:///Users/ljf/Desktop/hu_ts/synapse-rust/docs/archive/quality/defects_integration_test_analysis.md)）明确为：批量删除设备应 `super_admin` 独占。

**影响**：普通 admin 可强制注销任意用户（含管理员/超管）的全部 access + refresh token，造成拒绝服务与会话劫持窗口；`logout_user_devices` 内部走 `token_auth.logout_all`（[L658](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/routes/admin/user.rs#L658)），破坏性完整。

**单测未拦住**：`admin_role_batch_delete_devices_denied`（[admin_auth.rs L609-610](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/utils/admin_auth.rs#L609-L610)）用**不存在的路径名**断言，故测不出真实路由绕过。

**修复建议**：
1. 将 `path.contains("/delete_devices")` 修正为匹配真实路由，例如：

```rust
if path.ends_with("/devices/delete")
    || (path.contains("/devices/") && path.ends_with("/delete"))
{
    return true;
}
```

2. 修正/新增单元测试，改用真实路径 `/_synapse/admin/v1/users/@u:x/devices/delete` 与 `.../devices/{id}/delete` 断言 `is_role_allowed("admin", POST, ...) == false`。
3. 厘清 `/logout` 与 `/devices/delete` 语义：若二者均调用 `logout_user_devices` 且都应超管独占，须一并纳入；若 `/logout` 允许 admin，则须在文档中显式声明并说明理由。

---

### P1-2（审计完整性）客户端审计结果恒为 success

**严重级别**：P1 / 中危

**现象**：`audit_user_action` 在**鉴权成功后、handler 执行前**写入客户端审计，`result` 硬编码为 `"success"`，无法反映真实结果。

**实测证据**（同一请求实际返回 403，审计却记 success）：

```
user.put  | /user/@test4:matrix.test/account_data/m.push_rules | success  (actor=@test2, is_admin=false)
user.post | /user/@test4:matrix.test/filter                    | success
user.post | /rooms/!Gm_.../leave                               | success
user.put  | /rooms/!Gm_.../send/m.room.message/t1              | success
```

以上均为水平越权请求，实际 HTTP 403。

**根因**：[`audit_user_action`](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/routes/extractors/auth.rs#L59-L86) 的 `result: "success"` 硬编码（[L74](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/routes/extractors/auth.rs#L74)），且由 `AuthenticatedUser` 抽取器在 handler 前调用。

**影响**：审计日志无法用于失败操作取证，合规性受损；越权尝试会被误记为成功。

**修复建议**：改为在 handler 之后按真实响应状态码写 `result`（`2xx → success`，`4xx/5xx → failure/denied`），或在中间件层统一落库；若保留抽取器方案，须移除硬编码 `success`。

---

### P2-1（审计重复与膨胀）

**严重级别**：P2 / 中低危

**现象**：单次管理请求最多产生 **4 条** `audit_events`。

**实测证据**：241 行 / 111 个 distinct request_id；同一 request_id 最多 4 条（如 `req-43ba50be…`、`req-1c5659d6…`）。来源：

- middleware 成功行：`action = "POST /path"`，`result = 2xx?"success":"failure"`
- authorize 行：`action = "admin.post"` —— **middleware 与 handler 的 `AdminUser` 抽取器各触发一次**

**根因**：`authorize_admin_from_services`（[L78-95](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/utils/admin_auth.rs#L78-L95)）、`admin_auth_middleware`（[auth.rs L237-254](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/middleware/auth.rs#L237-L254)）、`AdminUser` 抽取器三处重复写入，且 action 命名不统一。

**影响**：审计表膨胀、同一请求语义分散，增加取证与存储成本。

**修复建议**：统一为**单点写入**（建议中间件层，因它掌握最终状态码），移除 `authorize_admin_from_services` 与 handler 抽取器中的重复写；统一 action 命名规范（如统一 `admin.{method}.{normalized_path}`）。

---

### P3-1（审计字段与关联缺陷）

**严重级别**：P3 / 低危

- `audit_events` 仅 9 列：`event_id, actor_id, action, resource_type, resource_id, result, request_id, details, created_ts` —— **无独立 IP 列**，IP 仅存在于 middleware 行的 `details.client_ip`。
- action 命名不一致：middleware=`"POST /path"`，authorize=`admin.post`。
- `auth.*` 事件（`auth.login_success` 147、`auth.login_failed` 31、`auth.account_locked` 4）`request_id` 全为空串 → 与请求链路无法关联。

**修复建议**：为 `audit_events` 增加 `client_ip` 列并在所有写入路径填充；统一 action 命名；在认证事件中补 `request_id`。

---

### P3-2（敏感操作设计残留）`register` 路由脱离统一授权链

**严重级别**：P3 / 低危（设计残留，受其他门禁保护）

**现象**：`/_synapse/admin/v1/register*`（[register.rs L34-35](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/routes/admin/register.rs#L34-L35)）在 `route_layer` 之外合并（[mod.rs L82](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/routes/admin/mod.rs#L82)），无 `admin_auth_middleware`；请求体 `RegisterRequest` 含可由调用者控制的 `admin: bool` 与 `user_type`。

**现有门禁**（动态验证有效）：`LocalhostGuard`（非本机 → 403 "Admin registration is only available from localhost"）+ HMAC-SHA256 签名（`mac` + shared_secret）+ 可选 captcha/approval。这是 Synapse 兼容设计。

**残留风险**：一旦 `allow_external_access=true` 或 shared_secret 泄漏，则可直接创建 `admin:true` / `user_type=super_admin` 账户，绕过全部 RBAC。

**修复建议**：保持 `allow_external_access=false` 默认；在文档中显式标注该端点为"高价值目标"；考虑对 `admin:true` / `super_admin` 的创建增加额外二次确认或强制 MFA。

---

## 八、修复优先级与建议汇总

| 优先级 | 编号 | 缺陷 | 建议动作 | 影响面 |
|:--:|------|------|----------|--------|
| **P0** | P0-1 | `login_as_user` 无超管限制 | 目标为 admin/super_admin 时要求发起者 super_admin | 完整权限提升 |
| **P1** | P1-1 | `/devices/delete` 字符串匹配错位 | 修正匹配 + 修单测 | 会话强制注销 |
| **P1** | P1-2 | 客户端审计 result 恒 success | handler 后按真实状态码写 result | 审计失真 |
| **P2** | P2-1 | 审计重复（≤4 条/请求） | 单点写入 + 统一命名 | 审计表膨胀 |
| **P3** | P3-1 | 审计字段/关联缺陷 | 增 IP 列、补 request_id | 取证能力 |
| **P3** | P3-2 | `register` 路由脱离统一授权 | 保持默认配置 + 文档标注 | 高危目标 |

**共性根因**：P0-1 与 P1-1 同源于 `is_super_admin_only_endpoint` 与 `login_as_user` 对**「提权类操作」的覆盖不完整**——判定基于路径字符串，与真实路由注册名脱节。**建议引入"提权操作端点清单"集中维护，并在 CI 中加入"路由注册 vs 独占清单"一致性断言**，从根本上防止新增路由漏配。

**回归验证建议**：
1. P0-1 修复后：普通 admin 对 super_admin 目标 `login_as_user` → 403；对普通用户 → 200。
2. P1-1 修复后：普通 admin 对任意用户 `devices/delete` → 403；super_admin → 正常。
3. P1-2 修复后：水平越权请求在审计中记为 failure。
4. 为上述三条补充集成测试并纳入 CI。

---

## 九、附录

### 9.1 测试账户

| 角色 | 用户名 | 用途 |
|------|--------|------|
| super_admin | `@apitest_admin:matrix.test` | 超管基线 |
| admin | `@admin:matrix.test` | 管理员越权测试 |
| 普通用户1 | `@test2:matrix.test` | 水平越权发起者 |
| 普通用户2 | `@test4:matrix.test` | 水平越权目标 |

### 9.2 关键证据文件

- 动态用例原始结果：`/tmp/permtest/results.json`（67 条）
- 提权验证结果：`/tmp/permtest/priv_esc_results.json`
- 测试脚本：`/tmp/permtest/common.py`、`priv_esc.py`

### 9.3 关键代码引用

- 授权判定：[admin_auth.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/utils/admin_auth.rs)
- 管理中间件：[auth.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/middleware/auth.rs)
- 用户管理路由：[user.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/routes/admin/user.rs)
- 管理模块装配：[mod.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/routes/admin/mod.rs)
- 客户端审计：[auth.rs（extractors）](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/routes/extractors/auth.rs)

### 9.4 测试残留清理

测试窗口内产生的数据已全部清理：`access_tokens` 19 行、`refresh_tokens` 17 行、`devices` 8 行；`_permtest_backup` 临时表已删除；测试用户 0 残留；`@apitest_admin` 密码哈希已从备份还原。
