use crate::common::ApiError;
use crate::web::routes::admin::audit::resolve_request_id;
use crate::web::utils::auth::bearer_token;
use crate::web::routes::AppState;
use crate::web::utils::admin_auth::authorize_admin_request;
use crate::web::utils::ip::extract_client_ip;
use axum::extract::State;
use axum::http::{HeaderMap, Method, Request};
use axum::response::IntoResponse;
use axum::{body::Body, response::Response, Json};
use serde_json::json;
use synapse_storage::audit::CreateAuditEventRequest;

pub fn extract_token(headers: &HeaderMap, uri: &str) -> Option<String> {
    if let Some(token) = crate::web::utils::auth::bearer_token_opt(headers) {
        return Some(token);
    }
    if let Some(query) = uri.split('?').nth(1) {
        for pair in query.split('&') {
            if let Some(value) = pair.strip_prefix("access_token=") {
                return Some(value.to_string());
            }
        }
    }
    None
}

pub async fn auth_middleware(
    State(state): State<AppState>,
    request: Request<Body>,
    next: axum::middleware::Next,
) -> Response {
    let uri = request.uri().to_string();
    let token = match extract_token(request.headers(), &uri) {
        Some(token) => token,
        None => return ApiError::missing_token().into_response(),
    };

    if let Err(err) = state.services.core.auth_service.validate_token(&token).await {
        return err.into_response();
    }

    next.run(request).await
}

pub async fn shadow_ban_middleware(
    State(state): State<AppState>,
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

    match state.services.core.auth_service.validate_token(&token).await {
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

pub async fn admin_auth_middleware(
    State(state): State<AppState>,
    request: Request<Body>,
    next: axum::middleware::Next,
) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let headers = request.headers().clone();
    let request_id = resolve_request_id(&headers);
    let client_ip =
        extract_client_ip(&headers, &["x-forwarded-for".to_string(), "x-real-ip".to_string(), "forwarded".to_string()]);

    let admin = match authorize_admin_request(&headers, &method, &path, &state).await {
        Ok(admin) => admin,
        Err(err) => {
            let response = err.into_response();
            let status = response.status().as_u16();
            let (actor_id, device_id, authenticated_admin) = match bearer_token(&headers) {
                Ok(token) => match state.services.core.auth_service.validate_token(&token).await {
                    Ok((user_id, device_id, is_admin, _, _)) => (user_id, device_id, Some(is_admin)),
                    Err(_) => ("anonymous".to_string(), None, None),
                },
                Err(_) => ("anonymous".to_string(), None, None),
            };

            if let Err(error) = state
                .services
                .admin
                .security
                .admin_audit_service
                .create_event(CreateAuditEventRequest {
                    actor_id,
                    action: format!("{method} {path}"),
                    resource_type: "admin_api".to_string(),
                    resource_id: path.clone(),
                    result: "failure".to_string(),
                    request_id: request_id.clone(),
                    details: Some(json!({
                        "method": method.as_str(),
                        "path": path,
                        "status": status,
                        "client_ip": client_ip,
                        "authenticated_admin": authenticated_admin,
                        "device_id": device_id,
                    })),
                })
                .await
            {
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

    if let Err(error) = state
        .services
        .admin
        .security
        .admin_audit_service
        .create_event(CreateAuditEventRequest {
            actor_id: admin.user_id.clone(),
            action: format!("{method} {path}"),
            resource_type: "admin_api".to_string(),
            resource_id: path.clone(),
            result: result.to_string(),
            request_id: request_id.clone(),
            details: Some(json!({
                "method": method.as_str(),
                "path": path,
                "role": admin.role,
                "device_id": admin.device_id,
                "status": response.status().as_u16(),
                "client_ip": client_ip,
            })),
        })
        .await
    {
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
