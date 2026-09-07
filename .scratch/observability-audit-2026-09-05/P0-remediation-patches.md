# synapse-rust 可观测性审计 — P0 修复补丁（C1 + D1）

> 本文件是 `report.md` 中两个 🔴 严重（P0）发现的**可直接落地的修复补丁**。
> 所有改动均基于当前实际代码（已逐文件核对），未做签名破坏性修改以控制改动面。
> 行号引用自 2026-09-05 审计时的源码快照。

---

## 前置核对（确保补丁可编译）

| 依赖项 | 位置 | 状态 |
|--------|------|------|
| `RequestId` 结构体 | `synapse-common/src/tracing.rs:12`（`pub struct RequestId(pub String)`） | ✅ 公开 |
| `tracing` 模块 | `synapse-common/src/lib.rs:46`（`pub mod tracing;`） | ✅ 公开 |
| `resolve_request_id` | `src/web/utils/auth.rs:8`（`pub(crate) fn resolve_request_id(&HeaderMap) -> String`） | ✅ 可复用 |
| `AuditEventStoreApi` | `synapse-storage/src/audit.rs:68`（`pub trait AuditEventStoreApi: Send + Sync`） | ✅ |
| `CreateAuditEventRequest` | `synapse-storage/src/audit.rs:40`（从 `synapse_storage` 根再导出） | ✅ |
| `RequestIdPropagationLayer` | `synapse-common/src/tracing.rs:26`（已挂在 subscriber，但无"水源"） | ✅ |

---

## D1 — 让 `x-request-id` 真正进入 tracing 上下文（修复链路追踪"无水源"）

**问题**：`RequestIdPropagationLayer`（`synapse-common/src/tracing.rs:28`）会从父 span 的扩展里拷贝 `RequestId`，但全仓库没有任何代码把 `RequestId` 注入根 span。
结果：`x-request-id` 只在 HTTP 头里流转，**未进入 tracing 字段**，全链路日志无法按 request_id 串联。

**文件**：`src/web/middleware/security.rs`

### D1-1 增加导入（在文件顶部 `use std::time::Instant;` 之后）

```diff
 use std::time::Instant;
+use synapse_common::tracing::RequestId;
+use tracing::Instrument;
```

### D1-2 改写 `request_id_middleware`（原 186–200 行）

```diff
 pub async fn request_id_middleware(mut request: Request<Body>, next: Next) -> Response {
     let request_id = resolve_request_id(request.headers());

     if let Ok(v) = HeaderValue::from_str(&request_id) {
         request.headers_mut().insert("x-request-id", v);
     }

-    let mut response = next.run(request).await;
-
-    if let Ok(v) = HeaderValue::from_str(&request_id) {
-        response.headers_mut().insert("x-request-id", v);
-    }
-
-    response
+    // D1 修复：建立根 span 并注入 RequestId 扩展。
+    // 这是 RequestIdPropagationLayer 的"水源"——自此之后所有子 span
+    // 都会自动继承 request_id，全链路日志/审计事件可携带该字段。
+    let span = tracing::info_span!("http_request", request_id = %request_id);
+    span.extensions_mut().insert(RequestId(request_id.clone()));
+
+    let mut response = next.run(request).instrument(span).await;
+
+    if let Ok(v) = HeaderValue::from_str(&request_id) {
+        response.headers_mut().insert("x-request-id", v);
+    }
+
+    response
 }
```

### D1-3 在访问日志中带上 request_id（原 `logging_middleware` 30–38 行）

> 现在 `x-request-id` 已进入 span 上下文，访问日志天然会带上；但为在**未进入 span 的旁路**也可见，显式打一行更稳妥。

```diff
     let log_path = uri.path();
+    let log_request_id = resolve_request_id(&headers);
     tracing::info!(
         "Request: {} {} {} {} {:?} {}ms",
         if authenticated { "authenticated" } else { "anonymous" },
         method,
         log_path,
         status.as_u_str(),
         headers,
         duration.as_millis()
+        request_id = %log_request_id,
     );
```

> 说明：若 `logging_middleware` 运行在 `request_id_middleware` 外层并已进入 span，则该行会同时收到 span 注入的 `request_id` 字段与显式 `request_id`，tracing 会以 span 字段为准，不冲突。

### D1 验证点

- 单测 `test_request_id_middleware_injects_header_for_downstream`（已存在，367–395 行）仍通过。
- 新增断言：构造请求带上 `x-request-id: req-xyz`，handler 内 `tracing::Span::current().extensions().get::<RequestId>()` 应等于 `req-xyz`。

---

## C1 — 把认证/授权类安全事件落 `audit_events` 不可篡改表

**问题**：登录成败、账户锁定、改密、令牌吊销、权限变更目前只写易丢失的 `security_audit` tracing 日志，
**未落库**到 append-only 的 `audit_events` 表。真正落库的只有 feature_flag / burn_after_read / friend_room / 部分 admin 路由。
这意味着安全事件**没有不可篡改的审计轨迹**，无法满足可追溯性要求。

**设计原则**：
1. 用 `Option<Arc<dyn AuditEventStoreApi>>` 字段 + builder 方法 `with_audit_storage(...)`，
   避免修改 `AuthService::new` / `new_with_lifetime` 签名（否则需改 ~11 个测试调用点和若干 wiring，改动面过大）。
2. 写入策略：**可用性 fail-soft、可观测性 fail-loud**——DB 写入失败时不阻断正常登录，
   但必须打 `error` 级 `security_audit` 日志，使缺口在高严重度告警中暴露。
3. `request_id` 取自当前 span 的 `RequestId` 扩展（D1 打通后自动生效），回退到生成的 uuid。

---

### C1a — 在 `AuthService` 上挂载审计存储（零签名破坏性）

**文件**：`synapse-services/src/auth/mod.rs`

#### C1a-1 结构体新增字段（原 46–74 行结构体定义末尾，`mas_validator` 之后）

```diff
     pub mas_validator: Option<Arc<dyn MasTokenValidator>>,
+    /// C1：安全审计事件落库存储。None 表示未接入（测试/旧 wiring 保持兼容）。
+    pub audit_storage: Option<Arc<dyn synapse_storage::audit::AuditEventStoreApi>>,
 }
```

#### C1a-2 在 `new_with_lifetime` 的 `Self { ... }` 初始化里补默认值（原 122–148 行末尾）

```diff
             login_lockout_duration_seconds: security.login_lockout_duration_seconds,
             mas_validator: None,
+            audit_storage: None,
         }
```

#### C1a-3 新增 builder（紧跟 `with_mas_validator`，原 155–158 行之后）

```diff
     pub fn with_mas_validator(mut self, validator: Arc<dyn MasTokenValidator>) -> Self {
         self.mas_validator = Some(validator);
         self
     }
+
+    /// C1：接入不可篡改审计表存储。生产 wiring 调用；测试保持 None。
+    pub fn with_audit_storage(mut self, storage: Arc<dyn synapse_storage::audit::AuditEventStoreApi>) -> Self {
+        self.audit_storage = Some(storage);
+        self
+    }
```

#### C1a-4 在生产 wiring 接入（文件 `synapse-services/src/container.rs` 199–211 行）

```diff
         let auth_concrete: std::sync::Arc<AuthService> = std::sync::Arc::new(AuthService::new_with_lifetime(
             pool,
             cache.clone(),
             metrics.clone(),
             &config.security,
             &config.server.name,
             config.access_token_lifetime_seconds(),
             user_service.clone(),
             user_storage.clone(),
             device_storage.clone(),
             token_storage.clone(),
             refresh_token_storage.clone(),
+        )
+        .with_audit_storage(Arc::new(synapse_storage::audit::AuditEventStorage::new(pool))));
-        ));
```

> `pool` 在该作用域已是 `&Arc<sqlx::PgPool>`，`AuditEventStorage::new` 签名接受 `&Arc<PgPool>`，无需新建设。

---

### C1b — 登录成功/失败/账户锁定 写入审计表

**文件**：`synapse-services/src/auth/login.rs`

#### C1b-1 新增导入（文件顶部）

```diff
 use super::auth_generate_token;
 use super::AuthService;
 use chrono::Utc;
 use std::sync::Arc;
 use synapse_common::crypto::hash_password_with_params;
 use synapse_common::*;
 use synapse_storage::User;
+use synapse_common::tracing::RequestId;
+use synapse_storage::{audit::AuditEventStoreApi, CreateAuditEventRequest};
```

#### C1b-2 通用私有方法：把安全事件写审计表（放在 `log_login_failure` 之前）

```rust
    /// C1：把一条安全审计事件写入不可篡改的 `audit_events` 表。
    /// 可用性 fail-soft：DB 写入失败不阻断业务，但打 error 级
    /// `security_audit` 日志，确保缺口在高严重度告警中被发现。
    /// request_id 优先取自 D1 打通后的 span 扩展，回退到生成的 uuid。
    async fn record_security_audit(
        &self,
        action: &str,
        resource_type: &str,
        resource_id: &str,
        result: &str,
        details: serde_json::Value,
    ) {
        let request_id = tracing::Span::current()
            .extensions()
            .get::<RequestId>()
            .map(|r| r.0.clone())
            .unwrap_or_else(|| format!("req-{}", uuid::Uuid::new_v4()));

        if let Some(storage) = &self.audit_storage {
            let req = CreateAuditEventRequest {
                actor_id: resource_id.to_string(),
                action: action.to_string(),
                resource_type: resource_type.to_string(),
                resource_id: resource_id.to_string(),
                result: result.to_string(),
                request_id,
                details: Some(details),
            };
            if let Err(e) = storage
                .create_event(&uuid::Uuid::new_v4().to_string(), current_timestamp_millis(), &req)
                .await
            {
                ::tracing::error!(
                    target: "security_audit",
                    event = action,
                    error = %e,
                    "FAILED to persist security audit event to audit_events (tamper-evident gap)"
                );
            }
        }
    }
```

#### C1b-3 改写 `log_login_failure` / `log_login_success`（原 190–206 行）为调用方

```diff
-    fn log_login_failure(username: &str, reason: &str) {
-        ::tracing::warn!(
-            target: "security_audit",
-            event = "login_failure",
-            username = username,
-            reason = reason
-        );
-    }
-
-    fn log_login_success(user: &User, device_id: Option<&str>) {
-        ::tracing::info!(
-            target: "security_audit",
-            event = "login_success",
-            user_id = user.user_id(),
-            device_id = device_id
-        );
-    }
+    async fn log_login_failure(&self, username: &str, reason: &str) {
+        ::tracing::warn!(
+            target: "security_audit",
+            event = "login_failure",
+            username = username,
+            reason = reason
+        );
+        self.record_security_audit(
+            "auth.login_failed",
+            "user",
+            username,
+            "failure",
+            serde_json::json!({"reason": reason}),
+        )
+        .await;
+    }
+
+    async fn log_login_success(&self, user: &User, device_id: Option<&str>) {
+        ::tracing::info!(
+            target: "security_audit",
+            event = "login_success",
+            user_id = user.user_id(),
+            device_id = device_id
+        );
+        self.record_security_audit(
+            "auth.login_success",
+            "user",
+            &user.user_id(),
+            "success",
+            serde_json::json!({"device_id": device_id}),
+        )
+        .await;
+    }
```

#### C1b-4 更新调用点（`login_internal` 内，原 70、83、103 行）

```diff
         if is_locked {
-            Self::log_login_failure(username, "account_locked");
+            self.log_login_failure(username, "account_locked").await;
             return Err(ApiError::rate_limited( ... ));
         }
         ...
-                Self::log_login_failure(username, "invalid_credentials");
+                self.log_login_failure(username, "invalid_credentials").await;
                 return Err(invalid());
         };
         ...
-        Self::log_login_success(&user, device_id);
+        self.log_login_success(&user, device_id).await;
```

#### C1b-5 账户锁定事件落库（`record_login_failure` 内，原 160–167 行）

```diff
             ::tracing::warn!(
                 target: "security_audit",
                 event = "account_locked",
                 user_id = user_id,
                 failure_count = failures,
                 lockout_duration_seconds = self.login_lockout_duration_seconds,
                 "Account locked due to too many failed login attempts"
             );
+            self.record_security_audit(
+                "auth.account_locked",
+                "user",
+                user_id,
+                "failure",
+                serde_json::json!({
+                    "failure_count": failures,
+                    "lockout_duration_seconds": self.login_lockout_duration_seconds
+                }),
+            )
+            .await;
```

---

### C1c — 改密 / 令牌吊销 / 权限变更 复用同一机制

> 以下三处目前仅写 `security_audit` tracing（`auth/account.rs:95–243`、`auth/token.rs:320–326`、
> `auth/power_levels.rs`）。它们同样持有 `&self`（`AuthService`），直接复用 C1b-2 的
> `record_security_audit`，把对应调用替换为 `.await` 版本即可。

| 操作 | 文件（行号，审计快照） | 建议 `action` 值 | `result` |
|------|------------------------|------------------|----------|
| 修改密码 | `auth/account.rs:95–243` | `auth.password_changed` | `success` / `failure` |
| 注销账户 | `auth/account.rs` | `auth.account_deactivated` | `success` |
| 吊销访问令牌 | `auth/token.rs:320–326` | `auth.token_revoked` | `success` |
| 吊销刷新令牌 | `auth/token.rs` | `auth.refresh_token_revoked` | `success` |
| 权限（power level）变更 | `auth/power_levels.rs` | `room.power_levels_changed` | `success` |

示例（以改密为例，插入到现 `tracing::info!(target:"security_audit", ...)` 之后）：

```rust
self.record_security_audit(
    "auth.password_changed",
    "user",
    &user.user_id(),
    "success",
    serde_json::json!({"changed_by": changed_by}),
).await;
```

---

## 风险与回滚

1. **D1** 纯增量：新增根 span + 扩展注入，不改变既有日志文本语义；若引发 span 嵌套预期外问题，
   回滚 `request_id_middleware` 一处即可（保留 header 注入）。
2. **C1** 通过 `Option` 字段默认 `None` 保证向后兼容：未接入 wiring 时行为与现状完全一致（仅 tracing）。
   回滚只需在 `container.rs` 去掉 `.with_audit_storage(...)` 一行。
3. **审计写入失败策略**：采用 fail-soft + error 日志，不会因审计库抖动导致用户无法登录；
   若希望更强保证（fail-closed），把 `record_security_audit` 中 `Err(e)` 分支改为
   `return;` → `self.auth_metrics...; return Err(ApiError::internal(...))` 即可（需调用方处理 `ApiResult`）。

## 验证清单

- [ ] `cargo build -p synapse-services -p synapse-common` 通过（`Option` 字段默认 `None`）
- [ ] `cargo test -p synapse-services auth` 现有用例仍绿（未改 `AuthService::new` 签名）
- [ ] 本地起服务，用错误密码登录 → `audit_events` 表新增 `action=auth.login_failed`
- [ ] 正确登录 → 表新增 `action=auth.login_success`，且 `request_id` 与响应头 `x-request-id` 一致
- [ ] 连续错误触发锁定 → 表新增 `action=auth.account_locked`
- [ ] 临时 `drop audit_events` 权限模拟写入失败 → 日志出现 `error` 级 `FAILED to persist security audit event`
