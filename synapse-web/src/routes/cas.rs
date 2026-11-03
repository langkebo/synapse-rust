use crate::routes::context::SsoContext;
use crate::routes::extractors::MatrixPath;
use crate::routes::extractors::MatrixQuery;
use crate::routes::extractors::UserId;
use crate::routes::MatrixJson;
use crate::routes::{AdminUser, AppState};
use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Form, Json, Router,
};
use serde::{Deserialize, Serialize};
use synapse_common::ApiError;
use synapse_services::cas_service::{CasRegisteredService, CasValidationResponse, RegisterServiceRequest};

/// CAS 配置检查中间件
async fn cas_config_check_middleware(
    State(ctx): State<SsoContext>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    // 检查 CAS 服务是否已配置
    if !ctx.cas_service.is_configured().await {
        tracing::error!("CAS service is not properly configured - database tables may not exist");
        return Err(ApiError::internal(
            "CAS service is not available. Please ensure database migrations have been run.".to_string(),
        ));
    }
    Ok(next.run(request).await)
}

#[derive(Debug, Deserialize)]
struct ServiceTicketQuery {
    service: String,
    #[serde(rename = "renew")]
    _renew: Option<bool>,
    #[serde(rename = "gateway")]
    _gateway: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct LogoutQuery {
    service: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ValidateQuery {
    service: String,
    ticket: String,
    #[serde(rename = "pgt_url")]
    _pgt_url: Option<String>,
    #[serde(rename = "renew")]
    _renew: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ProxyQuery {
    target_service: String,
    pgt: String,
}

#[derive(Debug, Deserialize)]
struct ProxyValidateQuery {
    service: String,
    ticket: String,
    #[serde(rename = "pgt_url")]
    _pgt_url: Option<String>,
    /// B-30: CAS `proxyValidate` accepts `renew` alongside `service`/`ticket`.
    /// Declared so the parameter is part of the contract instead of being
    /// silently dropped by the query extractor.
    #[serde(rename = "renew")]
    _renew: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct P3ServiceValidateQuery {
    service: String,
    ticket: String,
    pgt_url: Option<String>,
    renew: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RegisterServiceBody {
    service_id: String,
    name: String,
    description: Option<String>,
    service_url_pattern: String,
    allowed_attributes: Option<Vec<String>>,
    allowed_proxy_callbacks: Option<Vec<String>>,
    require_secure: Option<bool>,
    single_logout: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SetAttributeBody {
    attribute_name: String,
    attribute_value: String,
}

#[derive(Debug, Serialize)]
struct ServiceResponse {
    service_id: String,
    name: String,
    description: Option<String>,
    service_url_pattern: String,
    is_enabled: bool,
}

impl From<CasRegisteredService> for ServiceResponse {
    fn from(s: CasRegisteredService) -> Self {
        Self {
            service_id: s.service_id,
            name: s.name,
            description: s.description,
            service_url_pattern: s.service_url_pattern,
            is_enabled: s.is_enabled,
        }
    }
}

/// SEC-09: `redirect_after` must be a **same-origin path**.
///
/// The value is carried into `/login?service=<encoded>`, and the login page
/// sends the browser on to that `service` after authentication. Accepting an
/// absolute URL (or a protocol-relative `//host`, or the `/\host` form that
/// browsers normalise to `//host`) would turn this endpoint into an open
/// redirect, so anything that is not a plain local path is rejected.
fn is_same_origin_path(url: &str) -> bool {
    url.starts_with('/') && !url.starts_with("//") && !url.contains('\\') && !url.bytes().any(|b| b < 0x20 || b == 0x7f)
}

/// Matrix 客户端 CAS 回调路径，`service` 只允许指向它，避免登录页被当成开放跳板。
const MATRIX_CAS_CALLBACK_PATH: &str = "/_matrix/client/v3/login/sso/redirect/cas/callback";

fn matrix_cas_service(redirect_after: &str) -> String {
    format!("{MATRIX_CAS_CALLBACK_PATH}?redirect_after={}", urlencoding::encode(redirect_after))
}

fn is_matrix_cas_service(service: &str) -> bool {
    let prefix = format!("{MATRIX_CAS_CALLBACK_PATH}?redirect_after=");
    if !service.starts_with(&prefix) || service.len() <= prefix.len() {
        return false;
    }
    // 原始字节层面不允许控制字符与反斜杠。
    if service.bytes().any(|b| b < 0x20 || b == 0x7f || b == b'\\') {
        return false;
    }
    // 百分号编码后的控制字符 / 反斜杠解码后会破坏重定向语义，同样拒绝。
    let bytes = &service.as_bytes()[prefix.len()..];
    let mut i = 0;
    while i + 2 < bytes.len() {
        if bytes[i] == b'%' {
            if let (Some(hi), Some(lo)) = (hex_value(bytes[i + 1]), hex_value(bytes[i + 2])) {
                let value = hi * 16 + lo;
                if value < 0x20 || value == 0x7f || value == b'\\' {
                    return false;
                }
                i += 3;
                continue;
            }
        }
        i += 1;
    }
    true
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn append_ticket(service: &str, ticket: &str) -> String {
    let sep = if service.contains('?') { '&' } else { '?' };
    format!("{service}{sep}ticket={}", urlencoding::encode(ticket))
}

fn build_client_redirect(redirect_after: &str, login_token: &str) -> String {
    let sep = if redirect_after.contains('?') { '&' } else { '?' };
    format!("{redirect_after}{sep}loginToken={}", urlencoding::encode(login_token))
}

fn escape_html(value: &str) -> String {
    value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

async fn cas_sso_redirect(
    State(ctx): State<SsoContext>,
    MatrixQuery(params): MatrixQuery<SsoRedirectQuery>,
) -> Result<axum::response::Response, ApiError> {
    if ctx.cas_service.is_configured().await {
        // `redirect_after` is required: this endpoint exists to hand the browser
        // to CAS and then back to a chosen page. Defaulting to `/login` turned
        // "the caller forgot the parameter" into a silent redirect at 302, which
        // hides the integration bug and looks like a successful SSO start.
        // Synapse answers `M_MISSING_PARAM` here; so do we. An empty value is
        // the same omission, not a path.
        let redirect = params
            .redirect_after
            .as_deref()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ApiError::missing_param("Missing required query parameter: redirect_after"))?;

        if !is_same_origin_path(redirect) {
            // Deliberately not echoing the value back: it is attacker-controlled.
            return Err(ApiError::invalid_param("redirect_after must be a same-origin path"));
        }

        // Hand the browser to the built-in CAS login page, with the Matrix
        // callback (carrying the validated `redirect_after`) as `service`.
        // The login page only accepts this callback path as `service`, so the
        // endpoint cannot be turned into an open redirect via `service`.
        let service = matrix_cas_service(redirect);
        let location = format!("/_synapse/cas/login?service={}", urlencoding::encode(&service));
        return Ok((StatusCode::FOUND, [(header::LOCATION, location)]).into_response());
    }
    Err(ApiError::bad_request("CAS SSO is not configured".to_string()))
}

#[derive(Debug, Deserialize)]
struct SsoRedirectQuery {
    redirect_after: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SsoCallbackQuery {
    redirect_after: Option<String>,
    ticket: Option<String>,
}

async fn cas_sso_callback(
    State(ctx): State<SsoContext>,
    MatrixQuery(query): MatrixQuery<SsoCallbackQuery>,
) -> Result<Response, ApiError> {
    let redirect_after = query
        .redirect_after
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::missing_param("Missing required query parameter: redirect_after"))?;
    if !is_same_origin_path(&redirect_after) {
        return Err(ApiError::invalid_param("redirect_after must be a same-origin path"));
    }
    let ticket = query
        .ticket
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::missing_param("Missing required query parameter: ticket"))?;

    let service = matrix_cas_service(&redirect_after);
    let validated = ctx
        .cas_service
        .validate_service_ticket(&ticket, &service)
        .await?
        .ok_or_else(|| ApiError::forbidden("Invalid or expired CAS ticket"))?;

    let login_token = ctx.login_token_service.generate(&validated.user_id, None).await?;

    Ok((StatusCode::FOUND, [(header::LOCATION, build_client_redirect(&redirect_after, &login_token))]).into_response())
}

/// See [`cas_routes`].
pub fn cas_routes(state: AppState) -> Router<AppState> {
    // CAS 协议端点（模拟 CAS server 的协议面）统一收敛到 /_synapse/cas 命名空间下，
    // 避免占用根级路径（/login、/logout 等易与 Matrix 标准端点冲突）。
    let cas_protocol_routes = Router::new()
        .route("/login", get(cas_login_page).post(cas_login_submit))
        .route("/serviceValidate", get(service_validate))
        .route("/proxyValidate", get(proxy_validate))
        .route("/proxy", get(proxy))
        .route("/p3/serviceValidate", get(p3_service_validate))
        .route("/logout", get(logout))
        .route_layer(middleware::from_fn_with_state(state.clone(), cas_config_check_middleware));

    // Matrix 标准 SSO 重定向端点保持完整路径（不参与 /_synapse/cas 嵌套），
    // 但沿用 cas_config_check_middleware（与历史行为一致）。
    let sso_redirect_routes = Router::new()
        .route("/_matrix/client/v3/login/sso/redirect/cas", get(cas_sso_redirect))
        .route("/_matrix/client/v3/login/sso/redirect/cas/callback", get(cas_sso_callback))
        .route_layer(middleware::from_fn_with_state(state.clone(), cas_config_check_middleware));

    let public_routes = Router::new().nest("/_synapse/cas", cas_protocol_routes).merge(sso_redirect_routes);

    let standard_admin_routes = Router::new()
        .route("/_synapse/admin/v1/cas/services", post(register_service))
        .route("/_synapse/admin/v1/cas/services", get(list_services))
        .route("/_synapse/admin/v1/cas/services/{service_id}", delete(delete_service))
        .route("/_synapse/admin/v1/cas/users/{user_id}/attributes", post(set_user_attribute))
        .route("/_synapse/admin/v1/cas/users/{user_id}/attributes", get(get_user_attributes))
        .route_layer(axum::middleware::from_fn_with_state(
            <crate::routes::context::AdminContext as axum::extract::FromRef<crate::routes::AppState>>::from_ref(&state),
            crate::middleware::admin_auth_middleware,
        ))
        .route_layer(middleware::from_fn_with_state(state.clone(), cas_config_check_middleware));

    public_routes.merge(standard_admin_routes).with_state(state)
}

#[derive(Debug, Deserialize)]
struct CasLoginForm {
    service: String,
    username: String,
    password: String,
}

fn html_response(status: StatusCode, body: String) -> Response {
    (status, [(header::CONTENT_TYPE, "text/html; charset=utf-8"), (header::CACHE_CONTROL, "no-store")], body)
        .into_response()
}

fn render_login_page(service: &str, error: Option<&str>) -> String {
    let service = escape_html(service);
    let error = error.map_or_else(String::new, |msg| format!(r#"<p class="error">{}</p>"#, escape_html(msg)));
    format!(
        r#"<!doctype html>
<html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>CAS 登录</title>
<style>
:root{{color-scheme:light dark}}
body{{margin:0;min-height:100vh;display:flex;align-items:center;justify-content:center;background:#f5f6f8;font-family:system-ui,-apple-system,"Segoe UI",sans-serif}}
.card{{width:320px;padding:28px 24px;border-radius:12px;background:#fff;box-shadow:0 8px 24px rgba(0,0,0,.08)}}
h1{{margin:0 0 8px;font-size:20px}}
p.sub{{margin:0 0 20px;color:#666;font-size:13px}}
label{{display:block;margin:14px 0 6px;font-size:13px;color:#333}}
input{{width:100%;box-sizing:border-box;padding:10px 12px;border:1px solid #d9d9d9;border-radius:8px;font-size:14px}}
button{{width:100%;margin-top:20px;padding:11px;border:0;border-radius:8px;background:#5b6cff;color:#fff;font-size:15px;cursor:pointer}}
.error{{margin:0 0 12px;color:#d03050;font-size:13px}}
</style></head>
<body><main class="card">
<h1>统一身份认证</h1>
<p class="sub">使用矩阵账号登录以继续</p>
{error}
<form method="post" action="/_synapse/cas/login">
<input type="hidden" name="service" value="{service}">
<label for="username">用户名</label>
<input id="username" name="username" autocomplete="username" required>
<label for="password">密码</label>
<input id="password" name="password" type="password" autocomplete="current-password" required>
<button type="submit">登录</button>
</form>
</main></body></html>"#
    )
}

async fn cas_login_page(
    State(_ctx): State<SsoContext>,
    MatrixQuery(query): MatrixQuery<ServiceTicketQuery>,
) -> Result<Response, ApiError> {
    if !is_matrix_cas_service(&query.service) {
        return Err(ApiError::invalid_param("service must be the Matrix CAS callback path"));
    }
    Ok(html_response(StatusCode::OK, render_login_page(&query.service, None)))
}

async fn cas_login_submit(State(ctx): State<SsoContext>, Form(form): Form<CasLoginForm>) -> Result<Response, ApiError> {
    if !is_matrix_cas_service(&form.service) {
        return Err(ApiError::invalid_param("service must be the Matrix CAS callback path"));
    }
    if form.username.trim().is_empty() || form.password.is_empty() {
        return Ok(html_response(
            StatusCode::UNAUTHORIZED,
            render_login_page(&form.service, Some("请输入用户名和密码")),
        ));
    }

    let user = match ctx.account_identity_service.get_user_by_identifier(form.username.trim()).await? {
        Some(user) => user,
        None => {
            return Ok(html_response(
                StatusCode::UNAUTHORIZED,
                render_login_page(&form.service, Some("用户名或密码错误")),
            ))
        }
    };
    let user_id = user.user_id();

    if let Err(err) = ctx.credential_auth.verify_user_credentials(&user_id, &form.password).await {
        if err.is_forbidden() || err.is_unauthorized() || err.is_bad_request() {
            return Ok(html_response(
                StatusCode::UNAUTHORIZED,
                render_login_page(&form.service, Some("用户名或密码错误")),
            ));
        }
        return Err(err);
    }

    let ticket = ctx.cas_service.create_service_ticket(&user_id, &form.service).await?;
    let location = append_ticket(&form.service, &ticket.ticket_id);
    Ok((StatusCode::FOUND, [(header::LOCATION, location)]).into_response())
}

async fn service_validate(
    State(ctx): State<SsoContext>,
    MatrixQuery(query): MatrixQuery<ValidateQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let result = ctx.cas_service.validate_service_ticket(&query.ticket, &query.service).await.unwrap_or_else(|e| {
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
    State(ctx): State<SsoContext>,
    MatrixQuery(query): MatrixQuery<ProxyValidateQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let result = ctx.cas_service.validate_proxy_ticket(&query.ticket, &query.service).await.unwrap_or_else(|e| {
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
            Ok(([(header::CONTENT_TYPE, "application/xml; charset=utf-8")], response.to_xml()))
        }
        None => {
            let response = CasValidationResponse::Failure {
                code: "INVALID_TICKET".to_string(),
                description: "Proxy ticket not found or invalid".to_string(),
            };
            Ok(([(header::CONTENT_TYPE, "application/xml; charset=utf-8")], response.to_xml()))
        }
    }
}

async fn proxy(
    State(ctx): State<SsoContext>,
    MatrixQuery(query): MatrixQuery<ProxyQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let ticket = ctx.cas_service.create_proxy_ticket(&query.pgt, &query.target_service).await?;

    let response = CasValidationResponse::Success {
        user: ticket.user_id.clone(),
        attributes: std::collections::HashMap::new(),
        proxy_granting_ticket: Some(ticket.proxy_ticket_id),
    };

    Ok(([(header::CONTENT_TYPE, "application/xml; charset=utf-8")], response.to_xml()))
}

async fn p3_service_validate(
    State(ctx): State<SsoContext>,
    MatrixQuery(query): MatrixQuery<P3ServiceValidateQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let response = ctx
        .cas_service
        .validate_service_ticket_v3(
            &query.ticket,
            &query.service,
            query.pgt_url.as_deref(),
            query.renew.unwrap_or(false),
        )
        .await?;

    Ok(([(header::CONTENT_TYPE, "application/xml; charset=utf-8")], response.to_xml()))
}

async fn logout(
    State(_ctx): State<SsoContext>,
    MatrixQuery(query): MatrixQuery<LogoutQuery>,
) -> Result<impl IntoResponse, ApiError> {
    match query.service {
        Some(service_url) => {
            // 重定向到指定的 service URL
            Ok((StatusCode::FOUND, [(header::LOCATION, service_url)], ""))
        }
        None => {
            // 显示登出成功页面
            Ok((
                StatusCode::OK,
                [(header::CONTENT_TYPE, "text/html; charset=utf-8".to_string())],
                "<!doctype html><html><head><meta charset=\"utf-8\"></head><body><h1>Logged out successfully</h1></body></html>",
            ))
        }
    }
}

async fn register_service(
    State(ctx): State<SsoContext>,
    _admin: AdminUser,
    MatrixJson(body): MatrixJson<RegisterServiceBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = RegisterServiceRequest {
        service_id: body.service_id,
        name: body.name,
        description: body.description,
        service_url_pattern: body.service_url_pattern,
        allowed_attributes: body.allowed_attributes,
        allowed_proxy_callbacks: body.allowed_proxy_callbacks,
        is_require_secure: body.require_secure,
        is_single_logout: body.single_logout,
    };

    let service = ctx.cas_service.register_service(request).await?;

    Ok((StatusCode::CREATED, Json(ServiceResponse::from(service))))
}

async fn list_services(State(ctx): State<SsoContext>, _admin: AdminUser) -> Result<impl IntoResponse, ApiError> {
    let services = ctx.cas_service.list_services().await?;
    let response: Vec<ServiceResponse> = services.into_iter().map(ServiceResponse::from).collect();
    Ok(Json(response))
}

async fn delete_service(
    State(ctx): State<SsoContext>,
    _admin: AdminUser,
    MatrixPath(service_id): MatrixPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    let deleted = ctx.cas_service.delete_service(&service_id).await?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found("Service not found"))
    }
}

async fn set_user_attribute(
    State(ctx): State<SsoContext>,
    _admin: AdminUser,
    MatrixPath(user_id): MatrixPath<UserId>,
    MatrixJson(body): MatrixJson<SetAttributeBody>,
) -> Result<impl IntoResponse, ApiError> {
    let attr = ctx.cas_service.set_user_attribute(&user_id, &body.attribute_name, &body.attribute_value).await?;

    Ok(Json(serde_json::json!({
        "user_id": attr.user_id,
        "attribute_name": attr.attribute_name,
        "attribute_value": attr.attribute_value,
    })))
}

async fn get_user_attributes(
    State(ctx): State<SsoContext>,
    _admin: AdminUser,
    MatrixPath(user_id): MatrixPath<UserId>,
) -> Result<impl IntoResponse, ApiError> {
    let attrs = ctx.cas_service.get_user_attributes(&user_id).await?;

    let response: Vec<serde_json::Value> = attrs
        .into_iter()
        .map(|a| {
            serde_json::json!({
                "name": a.attribute_name,
                "value": a.attribute_value,
            })
        })
        .collect();

    Ok(Json(response))
}

// -------------------------------------------------------------------------
// Tests
// -------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that `ServiceResponse::from` correctly converts CasRegisteredService.
    #[test]
    fn test_service_response_from_cas_registered_service() {
        let service = CasRegisteredService {
            id: 1,
            service_id: "test-service".to_string(),
            name: "Test Service".to_string(),
            description: Some("A test service".to_string()),
            service_url_pattern: "https://example.com/*".to_string(),
            allowed_attributes: serde_json::json!({}),
            allowed_proxy_callbacks: serde_json::json!({}),
            is_enabled: true,
            is_require_secure: false,
            is_single_logout: false,
            created_ts: 0,
            updated_ts: 0,
        };
        let response = ServiceResponse::from(service);
        assert_eq!(response.service_id, "test-service");
        assert_eq!(response.name, "Test Service");
        assert!(response.is_enabled);
    }

    /// Test that `ServiceTicketQuery` deserialization works.
    #[test]
    fn test_service_ticket_query_deserialization() {
        let json = serde_json::json!({
            "service": "https://example.com",
            "renew": true
        });
        let query: ServiceTicketQuery = serde_json::from_value(json).unwrap();
        assert_eq!(query.service, "https://example.com");
        assert_eq!(query._renew, Some(true));
    }

    /// Test that `LogoutQuery` deserialization works.
    #[test]
    fn test_logout_query_deserialization() {
        let json = serde_json::json!({
            "service": "https://example.com/logout"
        });
        let query: LogoutQuery = serde_json::from_value(json).unwrap();
        assert_eq!(query.service, Some("https://example.com/logout".to_string()));
    }

    /// Test that `ValidateQuery` deserialization works.
    #[test]
    fn test_validate_query_deserialization() {
        let json = serde_json::json!({
            "service": "https://example.com",
            "ticket": "ST-12345"
        });
        let query: ValidateQuery = serde_json::from_value(json).unwrap();
        assert_eq!(query.service, "https://example.com");
        assert_eq!(query.ticket, "ST-12345");
    }

    /// Test that `RegisterServiceBody` deserialization works.
    #[test]
    fn test_register_service_body_deserialization() {
        let json = serde_json::json!({
            "service_id": "svc-1",
            "name": "My Service",
            "description": "A service",
            "service_url_pattern": "https://*.example.com/*",
            "require_secure": true,
            "single_logout": false
        });
        let body: RegisterServiceBody = serde_json::from_value(json).unwrap();
        assert_eq!(body.service_id, "svc-1");
        assert_eq!(body.name, "My Service");
        assert_eq!(body.require_secure, Some(true));
    }

    /// Test that `SetAttributeBody` deserialization works.
    #[test]
    fn test_set_attribute_body_deserialization() {
        let json = serde_json::json!({
            "attribute_name": "email",
            "attribute_value": "user@example.com"
        });
        let body: SetAttributeBody = serde_json::from_value(json).unwrap();
        assert_eq!(body.attribute_name, "email");
        assert_eq!(body.attribute_value, "user@example.com");
    }

    /// SEC-09: only same-origin paths may flow into `/login?service=…`.
    #[test]
    fn test_redirect_after_accepts_only_same_origin_paths() {
        // Local paths are the supported form.
        assert!(is_same_origin_path("/"));
        assert!(is_same_origin_path("/rooms/!abc:example.org?via=example.org"));
        assert!(is_same_origin_path("/home#/room/!abc:example.org"));

        // Absolute URLs, protocol-relative URLs and the `/\host` variant that
        // browsers normalise to `//host` are all open redirects.
        assert!(!is_same_origin_path("https://evil.example/steal"));
        assert!(!is_same_origin_path("http://evil.example"));
        assert!(!is_same_origin_path("//evil.example/steal"));
        assert!(!is_same_origin_path("/\\evil.example"));
        assert!(!is_same_origin_path("javascript:alert(1)"));

        // A bare host or an empty string is not a path either.
        assert!(!is_same_origin_path("evil.example"));
        assert!(!is_same_origin_path(""));

        // Control characters would let the value escape its query slot.
        assert!(!is_same_origin_path("/ok\r\nLocation: https://evil.example"));
        assert!(!is_same_origin_path("/ok\u{7f}"));
    }
}

#[cfg(test)]
mod cas_login_tests {
    use super::*;

    #[test]
    fn matrix_cas_service_round_trips_redirect_after() {
        let service = matrix_cas_service("/");
        assert_eq!(service, "/_matrix/client/v3/login/sso/redirect/cas/callback?redirect_after=%2F");
        assert!(is_matrix_cas_service(&service));
    }

    #[test]
    fn rejects_foreign_service_urls() {
        assert!(!is_matrix_cas_service("https://evil.example/callback"));
        assert!(!is_matrix_cas_service("/other/path?redirect_after=%2F"));
        assert!(!is_matrix_cas_service("/_matrix/client/v3/login/sso/redirect/cas/callback?redirect_after=%2F%0D"));
    }

    #[test]
    fn append_ticket_respects_existing_query() {
        assert_eq!(append_ticket("/callback?redirect_after=%2F", "ST-1"), "/callback?redirect_after=%2F&ticket=ST-1");
        assert_eq!(append_ticket("/callback", "ST-1"), "/callback?ticket=ST-1");
    }

    #[test]
    fn build_client_redirect_appends_login_token() {
        assert_eq!(build_client_redirect("/", "tok"), "/?loginToken=tok");
        assert_eq!(build_client_redirect("/home?tab=a", "tok"), "/home?tab=a&loginToken=tok");
    }

    /// Task 4 regression: the login token must survive the redirect as a
    /// percent-encoded query value, never as raw text.
    #[test]
    fn build_client_redirect_encodes_token() {
        assert_eq!(build_client_redirect("/", "a b"), "/?loginToken=a%20b");
    }

    #[test]
    fn escape_html_escapes_dangerous_characters() {
        assert_eq!(escape_html(r#"<a href="x">&'"#), "&lt;a href=&quot;x&quot;&gt;&amp;&#39;");
    }

    #[test]
    fn render_login_page_escapes_service_and_error() {
        let html = render_login_page(r#"/callback?redirect_after=%2F" onfocus="alert(1)"#, Some("<script>"));
        assert!(html.contains("name=\"service\""));
        assert!(!html.contains(r#"onfocus="alert(1)""#));
        assert!(!html.contains("<script>"));
        assert!(html.contains("/callback?redirect_after=%2F"));
    }

    #[test]
    fn render_login_page_has_no_inline_script() {
        let html = render_login_page("/callback?redirect_after=%2F", None);
        assert!(!html.contains("<script"));
        assert!(html.contains("method=\"post\""));
        assert!(html.contains("action=\"/_synapse/cas/login\""));
    }
}
