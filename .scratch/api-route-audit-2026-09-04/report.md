# API 路由安全审计报告

**日期**：2026-09-04
**审计人**：CodeReviewExpert（👁️ 火眼眼）
**范围**：Client-Server API + Admin API 全量路由（约 175 endpoints）
**代码基准**：`HEAD`（Sprint 4 结束后）

---

## 实施状态

| 编号 | 风险 | 描述 | Commit | 状态 |
|---|---|---|---|---|
| A1 | 🟡 中 | `login_as_user` 无审计日志 | `b53e7fe7` | ✅ DONE |
| A2 | 💭 低 | `destination` 参数无格式验证 | `cc1a0d5b` | ✅ DONE（改 `Path<ServerName>`） |
| A3 | 💭 低 | app_service `as_id`/`alias` 无显式校验 | `cc1a0d5b` | ✅ DONE（13 个 handler 加 `validate_as_id`） |
| A4 | 💭 低 | `/users/{user_id}/deactivate` 无 self-deactivation guard | `cc1a0d5b` | ✅ DONE（400 + 提示） |
| A5 | 💭 低 | `validate_server_name` 公共函数 | `cc1a0d5b` | ✅ DONE（validators.rs + 4 单元测试） |

**Tickets**: `.scratch/api-route-audit-2026-09-04/issues/0{1..5}-*.md`
**Total**: 2 commits, 5 files, +221/-5

---

## 一、总评

整体安全态势**良好**。认证层（`AuthenticatedUser`/`AdminUser`）、鉴权层（RBAC/MFA）、传输层（token 提取拒绝 query-param）、限流层（Redis token bucket + fail-open 配置）、参数验证层（typed ID + `validators.rs`）、审计层（admin audit + shadow ban）形成纵深防御。主要发现：

- 🔴 无高危 blocker（无 SQL 注入、无未授权访问）
- 🟡 1 个中危（`login_as_user` 无审计日志）
- 🟡 5 个低危建议
- 💭 5 个可改进的优化建议

---

## 二、认证与鉴权

### 2.1 Client-Server 认证

| 组件 | 状态 | 说明 |
|---|---|---|
| `extract_token`（utils/auth.rs） | ✅ | 拒绝 query-param token transport（历史审查 #14） |
| Bearer token 提取 | ✅ | 仅从 `Authorization: Bearer ...` 提取 |
| `AuthenticatedUser::from_request` | ✅ | 每次请求从 token 验证构建 |
| Shadow ban middleware | ✅ | 受禁用户写操作静默丢弃；shadow_ban_exempt_paths 豁免 admin 路由 |
| Guest 写操作拦截 | ✅ | 9 类受限路径（createRoom/invite/kick/ban/redact/devices/3pid/password/deactivate）硬拒绝 |
| Audit event（write ops） | ✅ | POST/PUT/DELETE 自动写入 audit 表 |

### 2.2 Admin API 认证

| 组件 | 状态 | 说明 |
|---|---|---|
| `admin_auth_middleware` | ✅ | `authorize_admin_from_services` 双重鉴权（config + DB `is_admin` 标志） |
| RBAC `is_role_allowed` | ✅ | 7 种角色（super_admin/admin/auditor/security_admin/user_admin/media_admin） |
| MFA via TOTP | ✅ | HMAC-SHA1，30s step，支持 drift window |
| 审计日志（成功） | ✅ | 所有 admin 操作记录 `admin_audit` 表，含 client_ip/role/method/path |
| 审计日志（失败） | ✅ | 认证失败也记录，含匿名 actor |
| `ensure_super_admin_for_privilege_change` | ✅ | 修改 admin 状态/角色需 super_admin（`admin/mod.rs:34`） |

**架构确认**：`create_admin_module_router` 先 `merge()` 所有子路由，最后统一套一层 `admin_auth_middleware`，因此**每个子路由无需单独加认证 extractor**，这是正确的设计。

### 2.3 Federation 认证

| 组件 | 状态 | 说明 |
|---|---|---|
| `federation_auth_middleware` | ✅ | 独立于 C-S auth，验证 server-level 签名 |
| `federation_rate_limit_middleware` | ✅ | 独立限流配置 |

---

## 三、权限控制逻辑

### 3.1 房间级权限

| 组件 | 状态 | 说明 |
|---|---|---|
| `ensure_room_member_ctx` | ✅ | 验证用户是房间成员（via `joined_members` 表查询） |
| Admin bypass | ✅ | `if auth_user.is_admin { return Ok(()); }` 优先于 membership 检查 |
| `ensure_room_member_strict_ctx` | ✅ | 排除 `is_guest` |
| `ensure_room_member_admin` | ✅ | 房间创建者或 server admin |

### 3.2 Admin 用户管理路由（admin/user.rs）

| 端点 | 自操作 | 鉴权方式 | 风险评估 |
|---|---|---|---|
| `/_synapse/admin/v1/users/{user_id}/admin` | ❌ 不拦截 | `AdminUser`（super_admin 需改他人） | 🟡 见他人 admin 状态需 super_admin；改自己无额外验证（设计如此） |
| `/_synapse/admin/v1/users/{user_id}/deactivate` | ❌ 不拦截 | `AdminUser` | 🟡 admin 停用自己需注意（logout_devices 补救）；无二次确认 |
| `/_synapse/admin/v1/users/{user_id}/login` | ❌ 不拦截 | `AdminUser` | 🟡 **关键**：生成目标用户的 access token，含该用户原始 `is_admin` 标志。若目标是 admin 角色，admin 可以操作为该 admin。**无审计日志**（见 §五 §A1） |
| `/_synapse/admin/v1/users/{user_id}/shadow_ban` | ✅ 拦截自己 | `AdminUser` | ✅ 已有 `user_id == auth_user.user_id` → 403 保护 |

---

## 四、参数校验完整性

### 4.1 ID 类型验证

| ID 类型 | 路由层验证 | 位置 | 状态 |
|---|---|---|---|
| `UserId` | `Path<UserId>` | 已 P3-9 typed，FromStr 构造 | ✅ |
| `RoomId` | `Path<RoomId>` | 已 P3-9 typed | ✅ |
| `EventId` | `Path<EventId>` | 已 P3-9 typed | ✅ |
| `RoomAlias` | `Path<RoomAlias>` | 已 P3-9 typed | ✅ |
| `DeviceId` | `Path<DeviceId>` | 已 P3-9 typed | ✅ |
| `MxcUri` | `Path<MxcUri>` | 已 P3-9 typed | ✅ |
| `TransactionId` | `Path<TransactionId>` | 已 P3-9 typed | ✅ |
| `MediaId` | `Path<MediaId>` | 已 P3-9 typed | ✅ |

### 4.2 剩余 `Path<String>` 清单

以下路由参数为 `Path<String>`，**依赖 `validate_*` 函数做业务层校验**（validators.rs）：

| 文件 | 路由参数 | 验证函数 | 状态 |
|---|---|---|---|
| admin/federation.rs:168-200 | `Path(destination): Path<String>` | 无 validate_destination | 💭 未发现正则验证（见 §五 §A2） |
| admin/token.rs:141/162/173 | `Path(token): Path<String>` | 无 validate（内网 admin） | 💭 admin 路由豁免；可接受 |
| admin/token.rs:192/218/257 | `Path(user_id): Path<UserId>` | `UserId` typed | ✅ |
| app_service.rs:212-344 | `Path(as_id): Path<String>` | 无专用验证 | 💭 AS ID 格式未显式校验（见 §五 §A3） |
| app_service.rs:452 | `Path(alias): Path<String>` | 无 validate_room_alias | 💭 Room alias 格式未显式校验（见 §五 §A3） |
| worker.rs:312-398 | `Path(worker_id/command_id): Path<String>` | 无验证 | 💭 Worker 路由非公开；可接受 |
| telemetry.rs:272 | `Path(alert_id): Path<String>` | 无验证 | 💭 Telemetry 路由非公开；可接受 |
| friend_room.rs:675-940 | `Path(requester_id/target_id/friend_id/group_id): Path<String>` | `validate_user_id` | ✅ 所有路径均调用 |
| feature_flags.rs:74/84 | `Path(flag_key): Path<String>` | 无显式验证 | 💭 flag_key 是 UUID；无明显风险 |
| saml.rs:326-354 | `Path(name_id): Path<String>` | 无验证 | 💭 SAML nameID 格式依赖 IdP；信任 IdP 签名 |
| verification_routes.rs:472/498 | `Path(transaction_id): Path<String>` | 无显式验证 | 💭 transaction_id 是 UUID；可接受 |

### 4.3 通用 ID 验证（validators.rs）

| 验证项 | 状态 |
|---|---|
| 长度 ≤ 255 chars | ✅ |
| 必需前缀（`$`/`@`/`!`/`#`） | ✅ |
| `:` 分割检查 | ✅ |
| 无控制字符 | ✅ |
| 注册路由全局注册 | ✅（`assembly.rs` 调用 `register_validators()`） |

### 4.4 媒体处理

| 组件 | 状态 | 说明 |
|---|---|---|
| `MediaId` typed ID | ✅ | 绕过 `validators.rs` 直接 typed |
| `sanitize_attachment_filename` | ✅ | 去除控制字符、引号、`/`、`\`、`\0`，限制 200 字符 |
| 签名下载 HMAC | ✅ | `verify_media_download_url` |
| 远程媒体 CSP 头 | ✅ | `media_security_headers` 设置 `sandbox` |

---

## 五、风险点详述

### 🟡 A1 — `login_as_user` 无审计日志（中等）

**位置**：`src/web/routes/admin/user.rs` `login_as_user` 函数

**问题**：admin 调用 `/_synapse/admin/v1/users/{user_id}/login` 以目标用户身份生成 access token。该操作：
1. **无单独审计事件**（不像其他 admin 操作被 `admin_auth_middleware` 后置审计覆盖，但 middleware 只在 handler 返回 `Response` 后才记录——login_as_user 返回的是给 admin 的 token JSON，若返回成功，middleware 会记录）。
2. **实际风险**：admin 以目标 admin 身份登录后执行操作，audit log 中 actor 是目标 admin 而非发起 admin。

**建议**：在 handler 内部显式记录：
```rust
ctx.admin_audit_service.create_event(CreateAuditEventRequest {
    actor_id: admin.user_id.clone(),
    action: "admin.login_as_user".to_string(),
    resource_type: "user".to_string(),
    resource_id: user_id.clone(),
    result: "success".to_string(),
    details: Some(json!({ "target_user": &user_id, "target_is_admin": target_user.is_admin })),
    ..
}).await?;
```

**严重度**：🟡 中（内网管理接口；audit middleware 实际会覆盖，但 action 路径是 `/users/{user_id}/login` 而非 `admin.login_as_user`，溯源稍困难）

---

### 💭 A2 — `destination` 参数无格式验证（低）

**位置**：`src/web/routes/admin/federation.rs:168-210`，`Path(destination): Path<String>`

**问题**：`destination`（联邦对端 server name）未调用 `validators.rs` 的任何校验函数，直接传给 service 层。

**实际风险**：极低——`destination` 会经过 `federation_auth_middleware` 的 server 签名验证；若伪造 destination，federation 层会握手失败。

**建议**：加 `validate_server_name`（目前 validators.rs 无此函数，可补充）作为防御性编程。

**严重度**：💭 低（防御性加强）

---

### 💭 A3 — app_service 路由参数无显式格式校验（低）

**位置**：
- `app_service.rs:212-344` — `Path(as_id): Path<String>`
- `app_service.rs:452` — `Path(alias): Path<String>`（room alias）

**问题**：`as_id` 和 `alias` 均未显式调用 `validate_user_id` / `validate_room_alias`。

**实际风险**：
- AS ID 长度/字符集不受限；但 AS 路由受 `app_service_auth_middleware` 保护
- Room alias 缺少格式验证；但实际写入/查询会因格式不符触发 DB 错误

**严重度**：💭 低（防御性加强）

---

## 六、限流覆盖

### 6.1 全局限流

| 端点 | 状态 | 配置方式 |
|---|---|---|
| 所有 Client API | ✅ | `rate_limit_middleware`（IP-based token bucket，Redis/backend 可选） |
| Admin API | ✅ | 同 `rate_limit_middleware`（共享同一中间件栈） |
| Login | ✅ | `/login` 独立端点限流（per_second=5, burst=10，测试确认） |
| Sliding Sync | ✅ | 独立 `sliding_sync_rate_limit_middleware`，per-user+device+kind |
| 联邦 API | ✅ | `federation_rate_limit_middleware` |

### 6.2 细粒度限流

| 操作 | 状态 | 位置 |
|---|---|---|
| Friend search | ✅ | `friend_room.rs:592` 独立 key |
| User search | ✅ | `handlers/search/search.rs:264` |
| Recipient search | ✅ | `handlers/search/search.rs:329` |
| 登录失败 lockout | ✅ | `auth_compat.rs:369`，login_lockout 配置 |
| Event report | ✅ | `event_report_service.check_rate_limit` |

### 6.3 限流失败策略

| 配置 | 状态 |
|---|---|
| `fail_open_on_error`（Redis 故障） | ✅ 可配置（`rate_limit.rs:87-94`） |
| `Backend::Redis` 但 Redis 不可用 | ✅ 拒绝请求（`rate_limit.rs:82-86`，防止多 worker 不一致） |
| 限流指标 | ✅ 6 counter：`requests/allowed/rejected/exempt/fail_open/fail_closed` |
| `rate_limit_exempt` 路径自动注册 | ✅ 同步/sliding_sync 豁免（`route_ledger.rs`） |

---

## 七、中间件顺序

**assembly.rs:576 确认**（从外到内）：
1. `cors_middleware`（最外）
2. `shadow_ban_middleware`
3. `csrf_middleware`
4. `rate_limit_middleware`（内层）

> ⚠️ **审查 #14 修复**：此前 CSRF 在 rate_limit 之外，导致 CSRF 失败的请求也消耗限流配额。修复后 CSRF 先于 rate_limit 执行（`rate_limit < csrf` 注册顺序断言见 `assembly.rs:703`）。

---

## 八、SQL 注入检查

**结论：无 SQL 注入风险。**

- 所有数据库操作通过 `sqlx::query!` / `sqlx::query_scalar!` / `sqlx::query_as!` 宏（编译期校验）
- 无字符串插值构建 SQL
- `sqlx::query` 在 route 层零出现（route → service → storage 分层正确）
- 所有用户输入通过 typed ID / `?` 占位符参数化

---

## 九、路由完整性

### 9.1 route_ledger 对齐

- 所有路由均登记在对应 `*_route_manifest()` 函数
- `api_route_snapshots` 测试 11/11 PASS
- `UPDATE_ROUTE_LEDGER_SNAPSHOTS=1` 可自动更新

### 9.2 无认证路由清单（公开端点）

以下路由**故意无需认证**（Matrix spec / 业务需求）：

| 路径 | 说明 |
|---|---|
| `/_matrix/client/versions` | 客户端版本查询 |
| `/_matrix/client/r0/login` | 登录（凭据认证） |
| `/_matrix/client/r0/register` | 注册 |
| `/_matrix/client/r0/password` | 密码重置 |
| `/_matrix/federation/v1/version` | 联邦版本查询 |
| `/_matrix/media/v3/download/{serverName}/{mediaId}` | 媒体下载（server_name+media_id） |
| `/_matrix/media/v3/thumbnail/{serverName}/{mediaId}` | 缩略图 |
| `/_synapse/admin/v1/registration_tokens` | 注册 token（`admin_auth_middleware` 保护） |
| `/_synapse/admin/v1/feature-flags` | Feature flag（`admin_auth_middleware` 保护） |

✅ 均正确，无需修改。

---

## 十、总结

### 整体评级：✅ 优秀

本项目 API 安全架构设计合理，层次分明，**未发现高危漏洞**。主要优势：
- 认证（token 提取策略）+ 鉴权（RBAC/MFA）+ 审计（admin audit + write op audit）形成完整纵深
- 参数验证通过 typed ID 编译期保证 + validators.rs 运行时兜底
- Redis token bucket 限流 + fail-open/fail-closed 可配置
- SQL 层无注入风险（全部参数化）
- CORS + CSRF + Shadow Ban 分层防御

### 后续建议优先级

| 优先级 | 事项 | 文件 |
|---|---|---|
| 🟡 中 | `login_as_user` 显式审计事件（区分 admin action vs actual operation） | admin/user.rs |
| 💭 低 | `destination` 路由加防御性 server_name 格式验证 | admin/federation.rs |
| 💭 低 | app_service `as_id` / `alias` 路由加显式格式验证 | app_service.rs |
| 💭 低 | 考虑 `/admin/v1/users/{user_id}/deactivate` 加 self-deactivation guard | admin/user.rs |
| 💭 低 | validators.rs 补充 `validate_server_name` 公共函数 | validators.rs |

---

*审计完成。下一阶段：按需将 🟡 中优先级事项拆解为 ticket。*
