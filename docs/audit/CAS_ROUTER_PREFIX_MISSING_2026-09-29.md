# Backend Issue: CAS Router Missing Nest Prefix (P0)

## 问题概述

后端的 `cas_routes` 函数将 CAS 公共协议端点（`/login`、`/serviceValidate`、`/logout` 等）以**根级绝对路径**注册到 router 上，导致这些端点最终挂载位置为 **`http://host:port/login`** 而不是预期的 **`http://host:port/_synapse/cas/login`**。

这与 SDK 当前实现产生的路径假设不一致，造成前后端联调时 CAS 协议端点不可达。

### 修复状态

✅ **已修复** (2026-09-29): `cas.rs:138-185`

CAS protocol routes 现在正确嵌套在 `/_synapse/cas` 命名空间下：

```rust
let cas_protocol_routes = Router::new()
    .route("/login", get(login_redirect))
    .route("/serviceValidate", get(service_validate))
    // ...
    .route_layer(middleware::from_fn_with_state(state.clone(), cas_config_check_middleware));

let public_routes = Router::new()
    .nest("/_synapse/cas", cas_protocol_routes)
    .merge(sso_redirect_routes);
```

**最终路径映射**:
| Endpoint | Before | After | Status |
|----------|--------|-------|--------|
| CAS Login | `/login` | `/_synapse/cas/login` | ✅ Fixed |
| Service Validate | `/serviceValidate` | `/_synapse/cas/serviceValidate` | ✅ Fixed |
| Proxy Validate | `/proxyValidate` | `/_synapse/cas/proxyValidate` | ✅ Fixed |
| SSO Redirect | `/_matrix/client/v3/login/sso/redirect/cas` | Same | ✅ Unchanged |

## 技术细节

### 后端源码定位
文件：`synapse-rust/synapse-web/src/routes/cas.rs`  
函数：`pub fn cas_routes(state: AppState) -> Router<AppState>`  
第 138-170 行:

```rust
pub fn cas_routes(state: AppState) -> Router<AppState> {
    let public_routes = Router::new()
        .route("/login", get(login_redirect))                    // 直接 → /login
        .route("/serviceValidate", get(service_validate))        // 直接 → /serviceValidate
        .route("/proxyValidate", get(proxy_validate))            // 直接 → /proxyValidate
        .route("/proxy", get(proxy))                             // 直接 → /proxy
        .route("/p3/serviceValidate", get(p3_service_validate))  // 直接 → /p3/serviceValidate
        .route("/logout", get(logout))                           // 直接 → /logout
        .route_layer(middleware::from_fn_with_state(state.clone(), cas_config_check_middleware));
    
    let standard_admin_routes = Router::new()
        .route("/_synapse/admin/v1/cas/services", post(register_service))
        // ...
    
    let legacy_admin_routes = Router::new()
        .route("/admin/services", post(register_service))
        // ...
    
    public_routes.merge(standard_admin_routes).merge(legacy_admin_routes).with_state(state)
}
```

### 当前路由结构

三组路由的**实际挂载位置**：

| 路由组 | 路径形态 | 最终挂载位置 | 特点 |
|--------|---------|------------|------|
| **Public** | `.route("/login", ...)` | **`/login`**（根级） | ❌ 无前缀 |
| **Standard Admin** | `.route("/_synapse/admin/v1/cas/services", ...)` | `/_synapse/admin/v1/cas/services`（完整路径） | ✅ 正确 |
| **Legacy Admin** | `.route("/admin/services", ...)` | **`/admin/services`**（根级） | ⚠️ 弃用别名 |

### 预期 vs 实际行为对比

| Endpoint Type | Expected Path | Actual Path | Impact |
|--------------|---------------|-------------|--------|
| CAS Login | `/_synapse/cas/login` | `/login` | 🔴 **BREAKING** - Will collide with root `/login` |
| Service Validate | `/_synapse/cas/serviceValidate` | `/serviceValidate` | 🔴 **BREAKING** |
| Proxy Validate | `/_synapse/cas/proxyValidate` | `/proxyValidate` | 🔴 **BREAKING** |
| SSO Redirect | `/_matrix/client/v3/login/sso/redirect/cas` | Same ✅ | ✅ OK |
| Admin Services | `/_synapse/admin/v1/cas/services` | Same ✅ | ✅ OK |
| Legacy Services | `/admin/services` | Same ✅ | ⚠️ Deprecated but functional |

## 影响面分析

### 1. 潜在的路由冲突风险 🔴 HIGH
如果后端有任何其他模块注册了 `/login`、`/logout` 等根级路由，会产生**路径冲突**，Axum 会在启动时报错或静默覆盖。

### 2. SDK 联调失败 🔴 CRITICAL
当前 SDK (`matrix-js-sdk`) 的 CAS Manager 假设 CAS 端点挂载在 `/_synapse/cas` 下，会导致所有 CAS 协议端点的调用地址错误：
```typescript
// SDK 期望: https://matrix.test/_synapse/cas/serviceValidate?service=xxx&ticket=xxx
// 后端实际：https://matrix.test/serviceValidate?service=xxx&ticket=xxx
```

### 3. API Coverage Gap 🟡 MEDIUM
ROUTE_CONTRACT.md 中列出的 CAS 路由虽然是正确的绝对路径，但注释说明「路径由 `.nest()` 前缀解析后去重」，这会误导开发者认为 CAS public routes 是有前缀的。

## 建议修复方案

### 方案 A: 添加 Nest Prefix（推荐 ✅）

修改 `cas_routes` 函数，将 public 路由包裹在 `/_synapse/cas` 下：

```rust
pub fn cas_routes(state: AppState) -> Router<AppState> {
    let public_routes = Router::new()
        .route("/login", get(login_redirect))
        .route("/serviceValidate", get(service_validate))
        .route("/proxyValidate", get(proxy_validate))
        .route("/proxy", get(proxy))
        .route("/p3/serviceValidate", get(p3_service_validate))
        .route("/logout", get(logout))
        .route_layer(middleware::from_fn_with_state(state.clone(), cas_config_check_middleware));

    // ✅ Wrap under nest prefix
    let public_routes = Router::new().nest("/_synapse/cas", public_routes);

    let standard_admin_routes = Router::new()
        // ... (no change needed, already has full paths)

    let legacy_admin_routes = Router::new()
        // ... (keep as root-level aliases with deprecation middleware)
    
    public_routes.merge(standard_admin_routes).merge(legacy_admin_routes).with_state(state)
}
```

**优点**:
- ✅ 符合 Matrix API 命名规范
- ✅ 避免与 Matrix 标准端点冲突
- ✅ 与 SDK 实现保持一致
- ✅ 清晰的命名空间划分

**缺点**:
- ⚠️ 需要更新 SDK 测试代码（之前已部分修复）

### 方案 B: 保持现状，移除 SDK 前缀（不推荐 ❌）

修改 SDK 去掉 `cas` 前缀，直接使用 root-level 路径。

**理由反对**:
- ❌ 违反 Matrix API 约定（所有 Synapse 特有端点都应该在 `/_synapse/*` 下）
- ❌ 容易与第三方扩展或未来 Matrix 规范的新端点冲突
- ❌ `/login` 可能与其他身份认证方式（OIDC/SAML）混淆

## 验证步骤

修复后需要通过以下方式验证：

1. **Unit Test** - 确保启动时路由正确挂载
2. **Integration Test** - 调用 CAS 端点验证可达性
3. **SDK Integration** - 通过 SDK 调用验证路径正确
4. **Conflict Check** - 确保没有其他路由使用相同路径

## 优先级判定

**P0 - Critical**  
原因：
- 当前 SDK 联调必然失败
- 存在潜在的启动时路由冲突风险
- 不符合 Matrix/Synapse API 最佳实践

## Related Issues

- SDK issue: CAS Manager `listServices` 路径构造 bug (已修复 #244da3ae)
- ROUTE_CONTRACT.md 文档不准确 (待更新)
- SDK `CAS_API_PREFIX.cas` 映射错误 (待修复)

## References

- Source: `synapse-rust/synapse-web/src/routes/cas.rs:139-170`
- Contract: `synapse-rust/docs/synapse-rust/ROUTE_CONTRACT.md#cas-17 条`
- SDK: `matrix-js-sdk/src/cas/index.ts:40-43`

---

**Created**: 2026-09-29 10:38 UTC  
**Reporter**: SDK Audit Team  
**Assignee**: Backend Team
