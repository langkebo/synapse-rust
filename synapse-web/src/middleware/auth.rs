use crate::routes::admin::audit::resolve_request_id;
use crate::routes::auth_source::AuthSource;
use crate::routes::context::{AdminContext, CoreContext};
use crate::routes::AppState;
use crate::utils::admin_auth::authorize_admin_from_services;
use crate::utils::auth::bearer_token;
use crate::utils::ip::extract_client_ip;
use axum::extract::State;
use axum::http::{HeaderMap, Method, Request};
use axum::response::IntoResponse;
use axum::{body::Body, response::Response, Json};
use serde_json::json;
use synapse_common::ApiError;
use synapse_services::admin_audit_service::CreateAuditEventRequest;

/// See [`extract_token`].
pub fn extract_token(headers: &HeaderMap, uri: &str) -> Option<String> {
    crate::utils::auth::extract_token_opt(headers, uri)
}

/// Build a common audit-event payload for admin-auth middleware.
///
/// `role` and `device_id` describe the *authenticated* admin path; for the
/// *denied* path callers override them via the returned mutable reference.
#[allow(clippy::too_many_arguments)]
fn build_admin_audit_event(
    actor_id: String,
    method: &Method,
    path: &str,
    status: u16,
    client_ip: Option<&str>,
    request_id: String,
    role: Option<&str>,
    device_id: Option<&str>,
) -> CreateAuditEventRequest {
    CreateAuditEventRequest {
        actor_id,
        action: format!("{method} {path}"),
        resource_type: "admin_api".to_string(),
        resource_id: path.to_owned(),
        result: "unknown".to_string(), // caller overrides with "success"/"failure"/"denied"
        request_id,
        details: Some(json!({
            "method": method.as_str(),
            "path": path,
            "status": status,
            "client_ip": client_ip,
            "role": role,
            "device_id": device_id,
        })),
    }
}

/// See [`auth_middleware`].
pub async fn auth_middleware(
    State(ctx): State<CoreContext>,
    request: Request<Body>,
    next: axum::middleware::Next,
) -> Response {
    let uri = request.uri().to_string();
    let token = match extract_token(request.headers(), &uri) {
        Some(token) => token,
        None => return ApiError::missing_token().into_response(),
    };

    if let Err(err) = ctx.token_auth.validate_token(&token).await {
        return err.into_response();
    }

    // OBS-03 (P2): 客户端主路由（CS API）也必须在响应头中返回 `x-request-id`，
    // 否则 4xx/5xx 时客户端拿到的错无法与服务器日志串联。在 next.run 前
    // 解析出 request_id，因为 response.headers() 不包含 incoming header。
    let request_id = crate::routes::admin::audit::resolve_request_id(request.headers());
    let mut response = next.run(request).await;
    if !response.headers().contains_key("x-request-id") {
        if let Ok(value) = axum::http::HeaderValue::from_str(&request_id) {
            response.headers_mut().insert("x-request-id", value);
        }
    }
    response
}

/// Client-side (CS API) audit middleware.
///
/// P1-2 (权限审查 2026-10-05): 此前客户端审计由 `AuthenticatedUser` 抽取器在
/// handler 执行前写入，`result` 只能硬编码为 `"success"`，无法反映真实响应
/// （例如水平越权 403 也被记成成功）。该中间件紧贴路由（在 rate_limit 内层），
/// 在 `next.run` 之后按真实状态码落库：2xx → success，其余 → failure。
///
/// 作用域与抽取器一致：仅审计写方法（POST/PUT/DELETE）且非 `/_synapse/admin` 的请求；
/// 无有效 bearer token 的请求（未认证）不写审计。
pub async fn client_audit_middleware(
    State(state): State<AppState>,
    request: Request<Body>,
    next: axum::middleware::Next,
) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_string();

    let is_auditable_write =
        matches!(method, Method::POST | Method::PUT | Method::DELETE) && !path.starts_with("/_synapse/admin");
    if !is_auditable_write {
        return next.run(request).await;
    }

    let Some(audit_svc) = state.admin_audit_service() else {
        return next.run(request).await;
    };

    let uri = request.uri().to_string();
    let headers = request.headers().clone();
    let request_id = resolve_request_id(&headers);

    // 解析发起用户；仅对已通过 token 校验的请求记录审计。
    let actor = match extract_token(&headers, &uri) {
        Some(token) => match state.token_auth().validate_token(&token).await {
            Ok((user_id, _, is_admin, _, _)) => Some((user_id, is_admin)),
            Err(_) => None,
        },
        None => None,
    };

    let response = next.run(request).await;

    let Some((user_id, is_admin)) = actor else {
        return response;
    };

    let result = if response.status().is_success() { "success" } else { "failure" };
    let event = CreateAuditEventRequest {
        actor_id: user_id,
        action: format!("user.{}", method.as_str().to_lowercase()),
        resource_type: "client_api".to_string(),
        resource_id: path.clone(),
        result: result.to_string(),
        request_id,
        details: Some(json!({
            "path": path,
            "method": method.as_str(),
            "is_admin": is_admin,
            "status": response.status().as_u16(),
        })),
    };
    if let Err(e) = audit_svc.create_event(event).await {
        ::tracing::error!(target: "security_audit", "Failed to create user audit event: {}", e);
    }

    response
}

/// See [`shadow_ban_middleware`].
pub async fn shadow_ban_middleware(
    State(ctx): State<CoreContext>,
    request: Request<Body>,
    next: axum::middleware::Next,
) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let is_write = matches!(method, Method::POST | Method::PUT | Method::DELETE | Method::PATCH);

    if !is_write {
        return next.run(request).await;
    }

    if is_shadow_ban_exempt_path(&path) {
        return next.run(request).await;
    }

    let uri = request.uri().to_string();
    let token = match extract_token(request.headers(), &uri) {
        Some(token) => token,
        None => return next.run(request).await,
    };

    match ctx.token_auth.validate_token(&token).await {
        Ok((_, _, _, is_shadow_banned, is_guest)) => {
            if is_shadow_banned {
                ::tracing::warn!(
                    target: "security_audit",
                    event = "shadow_banned_write_blocked",
                    path = path.as_str(),
                    method = method.to_string(),
                    "Shadow-banned user attempted write operation - silently dropping"
                );

                if path.contains("/send/")
                    || path.contains("/invite")
                    || path.contains("/join")
                    || path.contains("/leave")
                    || path.contains("/kick")
                    || path.contains("/ban")
                    || path.contains("/redact")
                {
                    return Json(json!({"event_id": format!("${}", uuid::Uuid::new_v4())})).into_response();
                }

                return Json(json!({})).into_response();
            }

            if is_guest {
                let guest_blocked_paths = [
                    "/createRoom",
                    "/invite",
                    "/kick",
                    "/ban",
                    "/unban",
                    "/redact",
                    "/devices",
                    "/account/3pid",
                    "/account/password",
                    "/account/deactivate",
                    "/keys/claim",
                    "/keys/upload",
                    "/admin/",
                    "/register",
                ];
                let is_blocked = guest_blocked_paths.iter().any(|p| path.contains(p));
                if is_blocked {
                    ::tracing::warn!(
                        target: "security_audit",
                        event = "guest_access_blocked",
                        path = path,
                        method = method.to_string(),
                        "Guest user attempted restricted write operation"
                    );
                    return ApiError::forbidden("Guest access is not allowed for this endpoint".to_string())
                        .into_response();
                }
            }

            next.run(request).await
        }
        Err(_) => next.run(request).await,
    }
}

fn is_shadow_ban_exempt_path(path: &str) -> bool {
    path.starts_with("/_synapse/admin/")
}

/// See [`admin_auth_middleware`].
pub async fn admin_auth_middleware(
    State(ctx): State<AdminContext>,
    request: Request<Body>,
    next: axum::middleware::Next,
) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let headers = request.headers().clone();
    let request_id = resolve_request_id(&headers);
    let client_ip = extract_client_ip(
        &headers,
        &["x-forwarded-for".to_string(), "x-real-ip".to_string(), "forwarded".to_string()],
        None,
        &[],
    );

    let admin = match authorize_admin_from_services(
        ctx.token_auth.as_ref(),
        ctx.user_service.as_ref(),
        &ctx.config.security,
        Some(ctx.admin_audit_service.as_ref()),
        &headers,
        &method,
        &path,
    )
    .await
    {
        Ok(admin) => admin,
        Err(err) => {
            let response = err.into_response();
            let status = response.status().as_u16();
            let (actor_id, device_id, _authenticated_admin) = match bearer_token(&headers) {
                Ok(token) => match ctx.token_auth.validate_token(&token).await {
                    Ok((user_id, device_id, is_admin, _, _)) => (user_id, device_id, Some(is_admin)),
                    Err(_) => ("anonymous".to_string(), None, None),
                },
                Err(_) => ("anonymous".to_string(), None, None),
            };

            let mut event = build_admin_audit_event(
                actor_id,
                &method,
                &path,
                status,
                client_ip.as_deref(),
                request_id.clone(),
                None, // role not known (denied)
                device_id.as_deref(),
            );
            // P1-01: 拒绝分支必须显式覆盖 `result`，否则默认值 "unknown" 会被
            // `AdminAuditService::validate_request` 拒绝（M_BAD_JSON），导致 denied
            // 审计事件全部丢失。
            event.result = "denied".to_string();
            if let Err(error) = ctx.admin_audit_service.create_event(event).await {
                tracing::warn!(
                    target: "admin_auth",
                    %error,
                    "Failed to persist denied admin audit event"
                );
            }

            return response;
        }
    };

    let mut response = next.run(request).await;
    let result = if response.status().is_success() { "success" } else { "failure" };

    let mut event = build_admin_audit_event(
        admin.user_id.clone(),
        &method,
        &path,
        response.status().as_u16(),
        client_ip.as_deref(),
        request_id.clone(),
        Some(admin.role.as_str()),
        admin.device_id.as_deref(),
    );
    event.result = result.to_string();

    if let Err(error) = ctx.admin_audit_service.create_event(event).await {
        tracing::warn!(target: "admin_auth", %error, "Failed to persist admin audit event");
    }

    if let Ok(value) = axum::http::HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-request-id", value);
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shadow_ban_exempts_admin_routes() {
        assert!(is_shadow_ban_exempt_path("/_synapse/admin/v1/users/%40testuser1%3Alocalhost/shadow_ban"));
        assert!(is_shadow_ban_exempt_path("/_synapse/admin/v1/users"));
    }

    #[test]
    fn test_shadow_ban_does_not_exempt_client_routes() {
        assert!(!is_shadow_ban_exempt_path("/_matrix/client/v3/createRoom"));
        assert!(!is_shadow_ban_exempt_path("/_matrix/client/v1/rendezvous"));
    }
}
