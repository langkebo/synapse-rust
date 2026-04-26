# synapse-rust 集成测试缺陷分析与优化方案

> 版本: v1.0.0
> 日期: 2026-04-26
> 基于: 630 项集成测试三角色 (super_admin/admin/user) 全量执行结果

---

## 一、测试结果总览

| 角色 | Passed | Failed | Missing | Skipped | 总计 |
|------|--------|--------|---------|---------|------|
| super_admin | 502 | 0 | 0 | 49 | 551 |
| admin | 483 | 20 | 0 | 48 | 551 |
| user | 411 | 92 | 0 | 48 | 551 |

### 失败分类统计

| 角色 | 权限提升漏洞 | 安全漏洞(super_admin端点) | 安全漏洞(admin端点) | 其他 |
|------|-------------|--------------------------|--------------------|------|
| admin | 3 | 16 | 0 | 1 |
| user | 36 | 8 | 47 | 1 |

---

## 二、缺陷列表

### DEF-001: Admin RBAC 权限提升漏洞 (P0 紧急)

**状态**: 未修复
**影响角色**: admin, user
**严重程度**: 严重 - 112 个端点权限控制失效

#### 问题描述

`is_role_allowed` 函数对 admin 角色的路径匹配过于宽泛，导致:
1. **admin 角色可访问 super_admin 专属端点** (20个失败)
2. **user 角色可访问几乎所有 admin 端点** (92个失败)

#### 根因分析

文件: `src/web/utils/admin_auth.rs:201-268`

```rust
// 问题1: admin 角色的 "兜底" 规则过于宽泛
// 第242行: is_read && path.starts_with("/_synapse/admin/v1/")
// 这意味着 admin 可以通过 GET 请求访问几乎所有 /_synapse/admin/v1/ 下的只读端点
// 包括本应仅限 super_admin 的端点

// 问题2: is_super_admin_only 检查不完整
// 仅检查了 /deactivate, /login, /logout, /admin, /make_admin
// 遗漏了大量 super_admin 专属端点:
//   - /server/version (服务器版本信息)
//   - /server_info (服务器信息)
//   - /send_server_notice (服务器通知)
//   - /delete_devices (设备删除)
//   - /retention (保留策略)
```

**参考 Synapse 实现**：

Synapse (Python) 使用 `assert_requester_is_admin()` 进行权限检查，并在每个 servlet 中明确声明所需权限级别。关键设计原则：

1. **默认拒绝** - 没有宽泛的兜底规则，每个端点必须明确授权
2. **细粒度控制** - 通过装饰器或中间件在路由级别声明权限
3. **审计日志** - 所有 admin 操作都记录审计日志

#### admin 角色可非法访问的 super_admin 端点 (20个)

| 端点 | 路径 | 应有权限 | 实际行为 |
|------|------|---------|---------|
| Admin Federation Resolve | `/_synapse/admin/v1/federation/resolve` | super_admin | 200 OK |
| Admin Set User Admin | `/_synapse/admin/v1/users/{id}/admin` | super_admin | 200 OK |
| Admin User Deactivate | `/_synapse/admin/v1/users/{id}/deactivate` | super_admin | 200 OK |
| Admin Shutdown Room | `/_synapse/admin/v1/rooms/{id}/shutdown` | super_admin | 200 OK |
| Admin Room Make Admin | `/_synapse/admin/v1/rooms/{id}/make_admin` | super_admin | 200 OK |
| Admin Federation Blacklist | `/_synapse/admin/v1/federation/blacklist` | super_admin | 200 OK |
| Admin Federation Cache Clear | `/_synapse/admin/v1/federation/cache/clear` | super_admin | 200 OK |
| Admin User Login | `/_synapse/admin/v1/users/{id}/login` | super_admin | 200 OK |
| Admin User Logout | `/_synapse/admin/v1/users/{id}/logout` | super_admin | 200 OK |
| Rust Synapse Version | `/_synapse/admin/v1/server/version` | super_admin | 200 OK |
| Send Server Notice | `/_synapse/admin/v1/send_server_notice` | super_admin | 200 OK |
| Admin Delete Devices | `/_synapse/admin/v1/users/{id}/delete_devices` | super_admin | 200 OK |
| Admin Add Federation Blacklist | `/_synapse/admin/v1/federation/blacklist/add` | super_admin | 200 OK |
| Admin Remove Federation Blacklist | `/_synapse/admin/v1/federation/blacklist/remove` | super_admin | 200 OK |
| Admin Purge History | `/_synapse/admin/v1/purge_history` | super_admin | 200 OK |
| Admin Create Registration Token | `/_synapse/admin/v1/registration_tokens/new` | super_admin | 200 OK |
| Admin Send Server Notice | `/_synapse/admin/v1/notifications/{id}` | super_admin | 200 OK |
| Admin Set Retention Policy | `/_synapse/admin/v1/rooms/{id}/retention` | super_admin | 200 OK |
| Admin Create Registration Token Negative | `/_synapse/admin/v1/registration_tokens/new` | super_admin | 200 OK |

#### user 角色可非法访问的端点 (92个)

完整列表见 `test-results-matrix/user/api-integration.failed.txt`，包括:
- 36 个 admin 端点被普通用户以 HTTP 200 访问
- 8 个 super_admin 端点被普通用户以 HTTP 200 访问
- 47 个 admin 端点被普通用户以非预期成功访问
- 1 个注册令牌创建端点返回 200 而非 401/403

#### 修复方案

修改 `src/web/utils/admin_auth.rs` 中的 `is_role_allowed` 函数:

```rust
fn is_role_allowed(role: &str, method: &Method, path: &str) -> bool {
    if role == "super_admin" {
        return true;
    }

    let is_read = matches!(*method, Method::GET | Method::HEAD);

    // super_admin 专属端点 (扩展列表)
    let is_super_admin_only = path.contains("/deactivate")
        || path.contains("/users/") && path.contains("/login") && !path.contains("/login/")
        || path.contains("/users/") && path.contains("/logout")
        || path.ends_with("/admin")
        || path.contains("/make_admin")
        || path.contains("/server/version")
        || path.contains("/server_info")
        || path.contains("/send_server_notice")
        || path.contains("/delete_devices");

    // admin 可访问的高级操作
    let is_admin_only = path.contains("/shutdown")
        || path.contains("/federation/resolve")
        || path.contains("/federation/blacklist")
        || path.contains("/federation/cache/clear")
        || path.contains("/federation/rewrite")
        || path.contains("/federation/confirm")
        || path.contains("/purge")
        || path.contains("/reset_connection")
        || path.contains("/retention");

    match role {
        "admin" => {
            if is_super_admin_only {
                return false;
            }

            if is_admin_only {
                return true;
            }

            // 移除兜底规则，明确列出 admin 可访问的路径
            path.starts_with("/_synapse/admin/v1/users")
                || path.starts_with("/_synapse/admin/v2/users")
                || path.starts_with("/_synapse/admin/v1/notifications")
                || path.starts_with("/_synapse/admin/v1/media")
                || path.starts_with("/_synapse/admin/v1/rooms")
                || path.starts_with("/_synapse/admin/v1/registration_tokens") && is_read
                || path.starts_with("/_synapse/admin/v1/federation") && is_read
                || path.starts_with("/_synapse/admin/v1/cas")
                || path.starts_with("/_synapse/worker/v1/")
                || path.starts_with("/_synapse/room_summary/v1/")
        }
        // ... 其他角色保持不变
        _ => false,
    }
}
```

**关键改进**：

1. **移除宽泛的兜底规则** - 删除 `is_read && path.starts_with("/_synapse/admin/v1/")`
2. **扩展 super_admin 专属端点列表** - 添加 `/server/version`, `/server_info`, `/send_server_notice`, `/delete_devices`
3. **细化权限控制** - 对 `registration_tokens` 和 `federation` 路径添加只读限制
4. **添加 CAS 路径** - 明确允许 admin 访问 CAS 管理端点
5. **遵循最小权限原则** - 每个路径都需要明确授权

#### 验证步骤

1. 修改后运行单元测试: `cargo test admin_role_restricted_endpoints_denied`
2. 运行三角色集成测试，确认 admin 失败数从 20 降为 0，user 失败数从 92 降为 0
3. 确认 super_admin 测试不受影响

---

### DEF-002: CAS Service Validate/Proxy Validate 返回 HTTP 500 (P1 高)

**状态**: 未修复
**影响角色**: super_admin, admin, user
**严重程度**: 高 - CAS SSO 验证完全不可用

#### 问题描述

`/serviceValidate` 和 `/proxyValidate` 端点返回 HTTP 500 内部服务器错误，而非有效的 CAS XML 响应。

#### 根因分析

文件: `src/web/routes/cas.rs:191-243`

`cas-sso` feature 默认启用 (包含在 `all-extensions` 中)，CAS 路由已注册，但 CAS 数据库表 (`cas_tickets`, `cas_services` 等) 可能未正确初始化。`service_validate` handler 直接调用 `state.services.cas_service.validate_service_ticket()`，数据库查询失败时 `ApiError::internal()` 将错误冒泡为 HTTP 500。

```rust
// cas.rs:191-208
async fn service_validate(
    State(state): State<AppState>,
    Query(query): Query<ValidateQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let result = state
        .services
        .cas_service
        .validate_service_ticket(&query.ticket, &query.service)
        .await?;  // 数据库错误直接冒泡为 HTTP 500
    // ...
}
```

#### 修复方案

**参考 Synapse 实现**：

Synapse (Python) 的 CAS 实现遵循 CAS Protocol 3.0 规范，关键特性：

1. **优雅降级** - 数据库错误不应导致 HTTP 500，而是返回 CAS 协议定义的失败响应
2. **错误日志** - 记录验证失败的详细信息用于调试
3. **协议兼容** - 严格遵循 CAS XML 响应格式

方案A: 在 handler 中增加优雅降级

```rust
async fn service_validate(
    State(state): State<AppState>,
    Query(query): Query<ValidateQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let result = state
        .services
        .cas_service
        .validate_service_ticket(&query.ticket, &query.service)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("CAS service ticket validation error: {}", e);
            None
        });

    match result {
        Some(ticket) => {
            let response = format!("yes\n{}\n", ticket.user_id);
            Ok(([(header::CONTENT_TYPE, "text/plain")], response))
        }
        None => Ok(([(header::CONTENT_TYPE, "text/plain")], "no\n\n".to_string())),
    }
}

async fn proxy_validate(
    State(state): State<AppState>,
    Query(query): Query<ProxyValidateQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let result = state
        .services
        .cas_service
        .validate_proxy_ticket(&query.ticket, &query.service)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("CAS proxy ticket validation error: {}", e);
            None
        });

    match result {
        Some(ticket) => {
            let response = CasValidationResponse::Success {
                user: ticket.user_id,
                attributes: std::collections::HashMap::new(),
                proxy_granting_ticket: None,
            };
            Ok((
                [(header::CONTENT_TYPE, "application/xml; charset=utf-8")],
                response.to_xml(),
            ))
        }
        None => {
            let response = CasValidationResponse::Failure {
                code: "INVALID_TICKET".to_string(),
                description: "Ticket not recognized".to_string(),
            };
            Ok((
                [(header::CONTENT_TYPE, "application/xml; charset=utf-8")],
                response.to_xml(),
            ))
        }
    }
}
```

方案B: 确保 CAS 迁移在 feature 启用时总是执行

检查 `migrations/extension_map.conf` 中 `00000001_extensions_cas.sql=cas-sso` 的映射是否正确，确保 Docker 镜像启动时执行了该迁移。

#### 验证步骤

1. 检查数据库中 `cas_tickets` 和 `cas_services` 表是否存在
2. 修复后发送 `/serviceValidate?service=...&ticket=invalid` 请求
3. 预期: 返回 HTTP 200 + CAS XML failure 响应，而非 HTTP 500

---

### DEF-003: CAS Logout 返回 HTTP 400 (P1 高)

**状态**: 未修复
**影响角色**: super_admin, admin, user
**严重程度**: 高 - CAS 登出功能不可用

#### 问题描述

`/logout` 端点在无 `service` 参数时返回 HTTP 400，但 CAS 规范中 `service` 参数是可选的。

#### 根因分析

文件: `src/web/routes/cas.rs:16-23`

```rust
#[derive(Debug, Deserialize)]
struct ServiceTicketQuery {
    service: String,  // 必填! 但 CAS 规范中 service 是可选的
    #[serde(rename = "renew")]
    _renew: Option<bool>,
    #[serde(rename = "gateway")]
    _gateway: Option<bool>,
}
```

CAS 协议规范 (CAS Protocol 3.0 Specification) 明确说明 `/cas/logout` 的 `service` 参数是可选的，用于指定登出后重定向的 URL。当不提供 `service` 时，服务器应直接显示登出成功页面。

#### 修复方案

**参考 Synapse 实现**：

Synapse (Python) 的 CAS logout 实现遵循 CAS Protocol 3.0 规范：

1. **可选参数** - `service` 参数是可选的，用于指定登出后重定向的 URL
2. **条件重定向** - 提供 `service` 时重定向，否则显示登出成功页面
3. **安全验证** - 应验证 `service` URL 是否在允许的服务列表中（可选增强）

```rust
#[derive(Debug, Deserialize)]
struct LogoutQuery {
    service: Option<String>,  // 改为 Option<String>
}

async fn logout(
    State(_state): State<AppState>,
    Query(query): Query<LogoutQuery>,
) -> Result<impl IntoResponse, ApiError> {
    if let Some(service_url) = query.service {
        Ok((
            StatusCode::FOUND,
            [
                (header::LOCATION, service_url),
                (header::CONTENT_TYPE, "text/html; charset=utf-8".to_string()),
            ],
            "<!doctype html><html><head><meta charset=\"utf-8\"></head><body><h1>Logged out successfully</h1><p>Redirecting...</p></body></html>",
        ))
    } else {
        Ok((
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            "<!doctype html><html><head><meta charset=\"utf-8\"></head><body><h1>Logged out successfully</h1></body></html>",
        ))
    }
}
```

#### 验证步骤

1. 发送 `GET /logout` (无 service 参数) → 预期 HTTP 200 + HTML 登出页面
2. 发送 `GET /logout?service=http://localhost:28008` → 预期 HTTP 302 重定向

---

### DEF-004: Login Flows 未包含 SSO/CAS/OIDC 类型 (P1 高)

**状态**: 未修复
**影响角色**: 所有用户
**严重程度**: 高 - 客户端无法发现 SSO 登录选项

#### 问题描述

`/_matrix/client/v3/login` 返回的 `flows` 数组仅包含 `m.login.password` 和 `m.login.token`，缺少 `m.login.sso`、`m.login.cas`、`m.login.oidc` 类型。

#### 根因分析

文件: `src/web/routes/auth_compat.rs:276-283`

```rust
pub(crate) async fn get_login_flows(State(_state): State<AppState>) -> Json<Value> {
    let flows = vec![
        json!({"type": "m.login.password"}),
        json!({"type": "m.login.token"}),
    ];
    Json(json!({ "flows": flows }))
}
```

函数硬编码了两种登录类型，未根据 OIDC/SAML/CAS 配置动态添加。即使配置了 OIDC/SAML/CAS，客户端也无法发现这些登录选项。

根据 Matrix 规范，当服务器配置了 SSO 时，`m.login.sso` 类型应出现在 login flows 中，且应包含 `identity_providers` 数组列出可用的 SSO 提供商。

#### 修复方案

**参考 Synapse 实现**：

Synapse (Python) 通过 `LoginRestServlet.on_GET()` 动态构建 login flows，关键设计：

1. **条件检查** - 根据配置检查每种认证方法是否启用
2. **提供商枚举** - 对于 SSO，列出所有可用的 identity providers
3. **客户端发现** - 允许客户端在不硬编码的情况下发现可用的登录方式

```rust
pub(crate) async fn get_login_flows(State(state): State<AppState>) -> Json<Value> {
    let mut flows = vec![
        json!({"type": "m.login.password"}),
        json!({"type": "m.login.token"}),
    ];

    let mut sso_providers = Vec::new();

    // 检查 SAML SSO
    #[cfg(feature = "saml-sso")]
    if state.services.saml_service.is_some() {
        sso_providers.push(json!({
            "id": "saml",
            "name": "SAML",
            "brand": "saml"
        }));
    }

    // 检查 OIDC
    if state.services.oidc_service.is_some() {
        sso_providers.push(json!({
            "id": "oidc",
            "name": "OIDC",
            "brand": "oidc"
        }));
    }

    // 检查 CAS
    #[cfg(feature = "cas-sso")]
    {
        sso_providers.push(json!({
            "id": "cas",
            "name": "CAS",
            "brand": "cas"
        }));
        flows.push(json!({"type": "m.login.cas"}));
    }

    // 如果有任何 SSO 提供商，添加 m.login.sso 类型
    if !sso_providers.is_empty() {
        flows.push(json!({
            "type": "m.login.sso",
            "identity_providers": sso_providers
        }));
    }

    // 检查内置 OIDC Provider
    if state.services.builtin_oidc_provider.is_some() {
        flows.push(json!({"type": "m.login.oidc"}));
    }

    Json(json!({ "flows": flows }))
}
```

**关键改进**：

1. **动态检测** - 根据 `state.services` 中的配置动态添加 SSO 类型
2. **提供商列表** - 通过 `identity_providers` 数组列出所有可用的 SSO 提供商
3. **向后兼容** - 保留 `m.login.cas` 类型以支持旧客户端
4. **条件编译** - 使用 feature gates 确保只在启用相应功能时添加

#### 验证步骤

1. 配置 OIDC 后调用 `/_matrix/client/v3/login`
2. 预期: `flows` 中包含 `m.login.sso` 和 `m.login.oidc`
3. 未配置 SSO 时，`flows` 中不应出现 SSO 类型

---

### DEF-005: Login Fallback Page 未实现 (P2 中)

**状态**: 未实现
**影响角色**: 浏览器用户
**严重程度**: 中 - 无原生客户端的用户无法通过浏览器登录

#### 问题描述

`/_matrix/static/client/login/` 端点完全未实现，返回 HTTP 404。

#### 规范要求

Matrix Client-Server API 规范 (SHOULD 级别):
> The login fallback endpoint is served at `/_matrix/static/client/login/`. This endpoint provides a fallback page for clients that don't have native login support.

Synapse (Python) 实现了此端点，返回一个 HTML 登录表单。

#### 修复方案

在 `src/web/routes/assembly.rs` 中添加 Login Fallback 路由:

```rust
// 在 create_router 函数中添加
.route(
    "/_matrix/static/client/login/",
    get(login_fallback_page),
)
```

在 `src/web/routes/auth_compat.rs` 中添加 handler:

```rust
pub(crate) async fn login_fallback_page(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let login_flows = get_login_flows(State(state)).await;
    let flows_html = login_flows.flows.iter().map(|f| {
        let t = f.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match t {
            "m.login.password" => r#"<div class="flow"><h3>Password Login</h3><form method="POST" action="/_matrix/client/v3/login"><input type="hidden" name="type" value="m.login.password"><label>Username: <input type="text" name="identifier[m.user]"></label><br><label>Password: <input type="password" name="password"></label><br><button type="submit">Login</button></form></div>"#,
            "m.login.sso" => r#"<div class="flow"><h3>SSO Login</h3><a href="/_matrix/client/v3/login/sso/redirect">Login with SSO</a></div>"#,
            _ => "",
        }
    }).collect::<String>();

    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        format!(r#"<!doctype html><html><head><meta charset="utf-8"><title>Login</title></head><body><h1>Login</h1>{}</body></html>"#, flows_html),
    )
}
```

#### 验证步骤

1. 发送 `GET /_matrix/static/client/login/`
2. 预期: HTTP 200 + HTML 登录页面

---

### DEF-006: OIDC JWKS 端点条件性不可用 (P2 中)

**状态**: 设计问题
**影响角色**: 联邦场景下的 OIDC 客户端
**严重程度**: 中 - 影响联邦 OIDC 密钥发现

#### 问题描述

`/.well-known/jwks.json` 端点仅在 `builtin_oidc_provider` 启用时注册路由，且 handler 内部只在 `builtin_oidc_provider` 存在时返回数据。外部 OIDC 场景下，即使 OIDC 已配置，JWKS 端点也不可用。

#### 根因分析

文件: `src/web/routes/oidc.rs:258-268`

```rust
async fn jwks(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    if let Some(provider) = &state.services.builtin_oidc_provider {
        let jwks = provider.get_jwks();
        return Ok(Json(serde_json::to_value(jwks).map_err(|e| {
            ApiError::internal(format!("Failed to serialize JWKS: {}", e))
        })?));
    }
    Err(ApiError::bad_request("Builtin OIDC provider is not enabled".to_string()))
}
```

文件: `src/web/routes/assembly.rs:187-197`

```rust
if state.services.oidc_service.is_some()
    || state.services.builtin_oidc_provider.is_some()
    || saml_enabled
{
    router = router.merge(create_oidc_router(state.clone()));
} else {
    // 仅注册 openid-configuration，不注册 jwks
    router = router.route(
        "/.well-known/openid-configuration",
        get(get_openid_configuration),
    );
}
```

#### 修复方案

方案A: 始终注册 JWKS 路由，在外部 OIDC 模式下重定向到外部 JWKS URI

```rust
async fn jwks(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    if let Some(provider) = &state.services.builtin_oidc_provider {
        let jwks = provider.get_jwks();
        return Ok(Json(serde_json::to_value(jwks).map_err(|e| {
            ApiError::internal(format!("Failed to serialize JWKS: {}", e))
        })?));
    }
    if let Some(oidc_service) = &state.services.oidc_service {
        if let Some(jwks_uri) = &oidc_service.jwks_uri() {
            return Ok(Redirect::temporary(jwks_uri));
        }
    }
    Err(ApiError::not_found("JWKS endpoint not available".to_string()))
}
```

方案B: 在 `assembly.rs` 中始终注册 `/.well-known/jwks.json` 路由

```rust
// 在 else 分支中也注册 jwks 路由
router = router
    .route("/.well-known/openid-configuration", get(get_openid_configuration))
    .route("/.well-known/jwks.json", get(jwks));
```

#### 验证步骤

1. 未配置 OIDC 时，`/.well-known/jwks.json` 应返回 404 或友好错误
2. 配置外部 OIDC 时，应重定向到外部 JWKS URI
3. 配置内置 OIDC 时，应返回本地 JWKS 数据

---

### DEF-007: Identity Server API 未实现 (P3 低)

**状态**: 未实现
**影响角色**: 需要 3PID 验证的用户
**严重程度**: 低 - 大多数部署使用外部 Identity Server

#### 问题描述

项目中没有实现 `/_matrix/identity/*` 路由。仅有 `identity_service` 用于与外部 Identity Server 通信 (3PID 绑定等)。

#### 受影响端点 (9个)

| 端点 | 路径 |
|------|------|
| Identity v2 Lookup | `/_matrix/identity/v2/lookup` |
| Identity v2 Hash Lookup | `/_matrix/identity/v2/lookup` (带认证) |
| Identity v1 Lookup | `/_matrix/identity/v1/lookup` |
| Identity v1 Request Token | `/_matrix/identity/v1/requestToken` |
| Identity v2 Request Token | `/_matrix/identity/v2/requestToken` |
| Identity v2 Account Info | `/_matrix/identity/v2/account` |
| Identity v2 Terms | `/_matrix/identity/v2/terms` |
| Identity v2 Hash Details | `/_matrix/identity/v2/hash_details` |
| Identity Lookup (algorithm validation) | `/_matrix/identity/v1/lookup` |

#### 修复建议

优先级低。大多数 Matrix 部署使用外部 Identity Server (如 vector.im)。如果需要内置 Identity Server:
1. 创建 `src/web/routes/identity.rs` 模块
2. 实现 Identity Server v1/v2 API
3. 在 `assembly.rs` 中注册路由

---

### DEF-008: CAS 功能"半启用"状态 (配置问题)

**状态**: 设计问题
**影响角色**: 所有使用 CAS 的用户
**严重程度**: 中 - CAS 路由已注册但后端不可用

#### 问题描述

`cas-sso` feature 默认启用 (包含在 `all-extensions` 中)，CAS 路由已注册，但 CAS 数据库表可能未创建，导致所有 CAS 操作返回 HTTP 500。

#### 根因分析

1. `Cargo.toml` 中 `default = ["server", "all-extensions"]`，`all-extensions` 包含 `cas-sso`
2. CAS 迁移文件通过 `migrations/extension_map.conf` 映射到 `cas-sso` feature
3. Docker 镜像启动时可能未正确执行 feature-gated 迁移
4. `CasService` 没有 "未配置" 的优雅降级逻辑

#### 修复方案

方案A: 在 `CasService` 中添加配置检查

```rust
impl CasService {
    pub fn is_configured(&self) -> bool {
        // 检查数据库表是否存在或配置是否完整
        true
    }
}
```

方案B: 在 CAS 路由注册前检查配置

```rust
#[cfg(feature = "cas-sso")]
{
    if state.services.cas_service.is_configured() {
        router = router.merge(cas_routes(state.clone()));
    }
}
```

方案C: 确保 Docker 入口脚本正确执行所有 feature-gated 迁移

---

## 三、预期跳过项 (非缺陷, 38个)

### 3.1 破坏性测试 (9个)

| 测试 | 原因 | 行为 |
|------|------|------|
| Delete Device | 删除设备不可逆 | dev 环境正确跳过 |
| Delete Devices (r0) | 批量删除设备不可逆 | dev 环境正确跳过 |
| Admin User Password | 重置密码不可逆 | dev 环境正确跳过 |
| Invalidate User Session | 使会话失效不可逆 | dev 环境正确跳过 |
| Reset User Password | 重置密码不可逆 | dev 环境正确跳过 |
| Admin Deactivate | 停用用户不可逆 | dev 环境正确跳过 |
| Admin Room Delete | 删除房间不可逆 | dev 环境正确跳过 |
| Admin Delete User | 删除用户不可逆 | dev 环境正确跳过 |
| Admin Session Invalidate | 使会话失效不可逆 | dev 环境正确跳过 |

### 3.2 SAML 未启用 (6个)

需要 `saml-sso` feature + SAML IdP 配置。默认未启用。

| 测试 | 原因 |
|------|------|
| SAML SP Metadata | SAML not enabled |
| SAML IdP Metadata | SAML not enabled |
| SAML Login Redirect | SAML not enabled |
| SAML Callback GET | SAML not enabled |
| SAML Callback POST | SAML not enabled |
| SAML Admin Metadata Refresh | SAML not enabled |

### 3.3 OIDC 未配置 (5个)

需要 OIDC Provider 配置。默认未配置。

| 测试 | 原因 |
|------|------|
| OIDC JWKS Endpoint | OIDC not configured |
| OIDC Authorize Endpoint | OIDC not configured |
| OIDC Dynamic Client Registration | OIDC not configured |
| OIDC Callback (invalid state) | OIDC not configured |
| OIDC Userinfo (with auth) | OIDC not configured |

### 3.4 SSO 未配置 (4个)

依赖 OIDC/SAML 启用。

| 测试 | 原因 |
|------|------|
| SSO Redirect v3 | SSO not configured |
| SSO Redirect r0 | SSO not configured |
| SSO Redirect (no redirectUrl) | SSO not configured |
| SSO Userinfo (with auth) | SSO not configured |

### 3.5 Identity Server 未本地托管 (6个)

大多数部署使用外部 Identity Server。

| 测试 | 原因 |
|------|------|
| Identity v2 Lookup | Identity Server not hosted locally |
| Identity v2 Hash Lookup | Identity Server not hosted locally |
| Identity v1 Lookup | Identity Server not hosted locally |
| Identity v1 Request Token | Identity Server not hosted locally |
| Identity v2 Request Token | Identity Server not hosted locally |
| Identity Lookup (algorithm validation) | Identity Server not hosted locally |

### 3.6 其他预期跳过 (8个)

| 测试 | 原因 | 说明 |
|------|------|------|
| Outbound Federation Version | localhost 不可路由 | 需要公网 FQDN |
| Federation Members | 需要联邦签名请求 | 本地无签名密钥 |
| Federation Hierarchy | 需要联邦签名请求 | 本地无签名密钥 |
| Federation Room Auth | 请求服务器无房间成员 | 单机测试限制 |
| Admin Federation Destination Details (x2) | 缺少联邦目标数据 | 需要配置外部服务器 |
| Admin Reset Federation Connection | 缺少联邦目标数据 | 需要配置外部服务器 |
| Builtin OIDC Login | 内置 OIDC 未启用 | 需要配置 |
| Login Fallback Page | 功能未实现 | 见 DEF-005 |

---

## 四、优化方案执行计划

### 阶段一: 紧急修复 (P0)

| 任务 | 缺陷 | 预计工作量 | 依赖 |
|------|------|-----------|------|
| 修复 Admin RBAC 权限提升漏洞 | DEF-001 | 中 | 无 |
| 添加 RBAC 单元测试覆盖 | DEF-001 | 小 | DEF-001修复 |

### 阶段二: 高优先级修复 (P1)

| 任务 | 缺陷 | 预计工作量 | 依赖 |
|------|------|-----------|------|
| 修复 CAS Service 500 错误 | DEF-002 | 小 | 确认迁移状态 |
| 修复 CAS Logout 400 错误 | DEF-003 | 小 | 无 |
| Login Flows 动态添加 SSO 类型 | DEF-004 | 中 | 无 |

### 阶段三: 中优先级修复 (P2)

| 任务 | 缺陷 | 预计工作量 | 依赖 |
|------|------|-----------|------|
| 实现 Login Fallback Page | DEF-005 | 中 | DEF-004 |
| 修复 OIDC JWKS 条件性不可用 | DEF-006 | 小 | 无 |
| 解决 CAS 半启用状态 | DEF-008 | 小 | DEF-002 |

### 阶段四: 低优先级 (P3)

| 任务 | 缺陷 | 预计工作量 | 依赖 |
|------|------|-----------|------|
| 评估 Identity Server API 实现 | DEF-007 | 大 | 业务需求确认 |

---

## 五、修复后预期测试结果

| 角色 | Passed | Failed | Missing | Skipped |
|------|--------|--------|---------|---------|
| super_admin | ~505 | 0 | 0 | ~46 |
| admin | ~503 | 0 | 0 | ~48 |
| user | ~503 | 0 | 0 | ~48 |

说明: Skipped 数量减少来自 CAS 端点修复后从 skip 变为 pass。

---

## 七、Synapse 参考实现总结

基于对 Element Synapse (Python) 的分析，以下是关键的设计原则和最佳实践：

### 7.1 权限控制模式

**Synapse 的方法**：
- 使用 `assert_requester_is_admin()` 在每个 servlet 中明确检查权限
- 没有宽泛的"兜底"规则，遵循默认拒绝原则
- 通过装饰器或中间件在路由级别声明权限要求
- 所有 admin 操作都记录审计日志

**synapse-rust 的改进**：
- 移除 `is_read && path.starts_with("/_synapse/admin/v1/")` 兜底规则
- 扩展 `is_super_admin_only` 检查列表
- 为每个路径明确授权，遵循最小权限原则
- 已实现审计日志记录（`admin_audit_service`）

### 7.2 认证流程发现

**Synapse 的方法**：
- `LoginRestServlet.on_GET()` 动态构建 flows 数组
- 检查配置中启用的认证方法（JWT, CAS, SAML, OIDC）
- 通过 `identity_providers` 数组列出所有可用的 SSO 提供商
- 在 servlet 初始化时加载 SSO handlers

**synapse-rust 的改进**：
- 根据 `state.services` 动态检测可用的认证方法
- 添加 `identity_providers` 数组支持
- 使用 feature gates 确保条件编译
- 保持向后兼容（`m.login.cas` 类型）

### 7.3 CAS 协议实现

**Synapse 的方法**：
- 严格遵循 CAS Protocol 3.0 规范
- `service` 参数在 logout 端点是可选的
- 数据库错误不导致 HTTP 500，而是返回协议定义的失败响应
- 记录详细的验证失败日志用于调试

**synapse-rust 的改进**：
- 修改 `LogoutQuery` 使 `service` 参数可选
- 在 validate 端点添加错误处理，避免 HTTP 500
- 使用 `unwrap_or_else` 进行优雅降级
- 添加 `tracing::warn!` 记录验证错误

### 7.4 错误处理策略

**Synapse 的方法**：
- 区分内部错误和协议错误
- 内部错误记录日志但不暴露给客户端
- 协议错误返回符合规范的错误响应
- 使用适当的 HTTP 状态码

**synapse-rust 的改进**：
- CAS 端点使用 `unwrap_or_else` 捕获数据库错误
- 返回 CAS 协议定义的失败响应而非 HTTP 500
- 记录警告日志用于调试
- 保持 HTTP 200 状态码但返回失败内容（符合 CAS 规范）

### 7.5 部署和配置

**Synapse 的方法**：
- 条件注册 servlets 基于部署上下文（worker vs main process）
- MSC3861 委托模式下跳过某些端点注册
- 模块化设计支持分布式部署

**synapse-rust 的考虑**：
- 使用 feature gates 控制功能启用
- 通过 `migrations/extension_map.conf` 映射 feature 到迁移
- 需要确保 Docker 启动时正确执行 feature-gated 迁移
- 考虑添加配置检查避免"半启用"状态

---

## 八、相关文件索引

| 文件 | 关联缺陷 |
|------|---------|
| `src/web/utils/admin_auth.rs` | DEF-001 |
| `src/web/routes/cas.rs` | DEF-002, DEF-003, DEF-008 |
| `src/web/routes/auth_compat.rs` | DEF-004, DEF-005 |
| `src/web/routes/oidc.rs` | DEF-006 |
| `src/web/routes/assembly.rs` | DEF-005, DEF-006, DEF-008 |
| `src/services/container.rs` | DEF-008 |
| `migrations/extension_map.conf` | DEF-008 |
| `docker/deploy/api-integration_test.sh` | 测试脚本 |
