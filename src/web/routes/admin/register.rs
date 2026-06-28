// 管理后台 - 管理员注册
// 实现 Synapse 兼容的管理员注册 API
// API: /_synapse/admin/v1/register/nonce, /_synapse/admin/v1/register
//
// 安全说明：此 API 默认仅允许从 localhost (127.0.0.1) 调用
// 如需从外部调用，请修改 allow_external_access 配置

use crate::common::error::MatrixErrorCode;
use crate::common::ApiError;
use crate::services::captcha_service::VerifyCaptchaRequest;
use crate::services::AdminRegisterRequest;
use crate::web::routes::extractors::localhost_guard::{register_error_response, LocalhostGuard};
use crate::web::routes::AppState;
use crate::web::utils::ip::extract_client_ip;
use axum::{
    body::Body,
    extract::ConnectInfo,
    extract::State,
    http::HeaderMap,
    response::Response,
    routing::{get, post},
    Json, Router,
};
use ipnetwork::IpNetwork;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::net::SocketAddr;
use validator::Validate;

pub fn create_register_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_synapse/admin/v1/register/nonce", get(get_nonce))
        .route("/_synapse/admin/v1/register", post(register))
        .with_state(state)
}

pub fn admin_register_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;
    [(Method::GET, "/_synapse/admin/v1/register/nonce"), (Method::POST, "/_synapse/admin/v1/register")]
        .into_iter()
        .map(|(m, p)| RouteEntry::new(m, p, "admin::register"))
        .collect()
}

#[derive(Serialize)]
struct NonceResponse {
    nonce: String,
}

#[derive(Deserialize, Validate)]
struct RegisterRequest {
    #[validate(length(min = 1, max = 255))]
    nonce: String,
    #[validate(length(min = 1, max = 255))]
    username: String,
    #[validate(length(min = 8, max = 512))]
    password: String,
    admin: bool,
    #[validate(length(max = 255))]
    #[serde(default)]
    displayname: Option<String>,
    #[validate(length(min = 1, max = 1024))]
    mac: String,
    #[validate(length(max = 255))]
    #[serde(default)]
    user_type: Option<String>,
    #[validate(length(min = 1, max = 255))]
    #[serde(default)]
    captcha_id: Option<String>,
    #[validate(length(min = 1, max = 32))]
    #[serde(default)]
    captcha_code: Option<String>,
    #[validate(length(min = 1, max = 255))]
    #[serde(default)]
    approval_token: Option<String>,
}

#[derive(Serialize)]
struct RegisterResponse {
    access_token: String,
    refresh_token: String,
    expires_in: u64,
    device_id: String,
    user_id: String,
    home_server: String,
}

#[allow(clippy::needless_pass_by_value)]
fn map_admin_register_service_error(error: ApiError) -> Response<Body> {
    let message = error.message();

    match message.as_str() {
        "Unrecognised nonce" => register_error_response(400, "M_UNKNOWN", &message),
        "HMAC incorrect" => register_error_response(400, "M_UNKNOWN", &message),
        "Admin registration is not enabled" => register_error_response(400, "M_UNKNOWN", &message),
        _ if error.is_conflict() || error.code_is(MatrixErrorCode::UserInUse) => {
            register_error_response(400, "M_USER_IN_USE", "User already exists")
        }
        _ => register_error_response(error.http_status().as_u16(), error.code().as_str(), &message),
    }
}

fn extract_registration_client_ip(headers: &HeaderMap) -> Option<String> {
    let priority = vec!["x-forwarded-for".to_string(), "x-real-ip".to_string(), "forwarded".to_string()];
    extract_client_ip(headers, &priority)
}

fn runtime_environment() -> String {
    std::env::var("RUST_ENV").unwrap_or_else(|_| "production".to_string()).to_ascii_lowercase()
}

fn ensure_admin_registration_environment(production_only: bool) -> Result<(), Box<Response<Body>>> {
    if production_only && runtime_environment() != "production" {
        return Err(Box::new(register_error_response(
            403,
            "M_FORBIDDEN",
            "Admin registration is disabled outside production",
        )));
    }

    Ok(())
}

fn ip_matches_whitelist(ip: IpAddr, whitelist: &[String]) -> bool {
    whitelist.iter().any(|entry| {
        let candidate = entry.trim();
        if candidate.is_empty() {
            return false;
        }

        candidate
            .parse::<IpNetwork>()
            .map(|network| network.contains(ip))
            .or_else(|_| candidate.parse::<IpAddr>().map(|allowed| allowed == ip))
            .unwrap_or(false)
    })
}

fn ensure_admin_registration_ip_policy(
    headers: &HeaderMap,
    connect_info: &ConnectInfo<SocketAddr>,
    ip_whitelist: &[String],
) -> Result<(), Box<Response<Body>>> {
    if ip_whitelist.is_empty() {
        return Ok(());
    }

    let remote_ip = connect_info.0.ip();
    if !ip_matches_whitelist(remote_ip, ip_whitelist) {
        return Err(Box::new(register_error_response(
            403,
            "M_FORBIDDEN",
            "Admin registration source IP is not allowed",
        )));
    }

    if let Some(client_ip) = extract_registration_client_ip(headers).and_then(|ip| ip.parse::<IpAddr>().ok()) {
        if !ip_matches_whitelist(client_ip, ip_whitelist) {
            return Err(Box::new(register_error_response(
                403,
                "M_FORBIDDEN",
                "Admin registration source IP is not allowed",
            )));
        }
    }

    Ok(())
}

async fn verify_additional_registration_controls(
    state: &AppState,
    payload: &RegisterRequest,
) -> Result<(), Response<Body>> {
    if state.services.core.config.admin_registration.require_captcha {
        let captcha_id = payload
            .captcha_id
            .as_ref()
            .ok_or_else(|| register_error_response(400, "M_INVALID_PARAM", "captcha_id is required"))?;
        let captcha_code = payload
            .captcha_code
            .as_ref()
            .ok_or_else(|| register_error_response(400, "M_INVALID_PARAM", "captcha_code is required"))?;

        let verified = state
            .services
            .admin
            .security
            .captcha_service
            .verify_captcha(VerifyCaptchaRequest { captcha_id: captcha_id.clone(), code: captcha_code.clone() })
            .await
            .map_err(|e| register_error_response(400, "M_FORBIDDEN", &e.to_string()))?;

        if !verified {
            return Err(register_error_response(400, "M_FORBIDDEN", "Captcha verification failed"));
        }
    }

    if state.services.core.config.admin_registration.require_manual_approval {
        let approval_token = payload
            .approval_token
            .as_ref()
            .ok_or_else(|| register_error_response(400, "M_INVALID_PARAM", "approval_token is required"))?;

        if !state.services.core.config.admin_registration.approval_tokens.iter().any(|token| token == approval_token) {
            return Err(register_error_response(403, "M_FORBIDDEN", "Manual approval token is invalid"));
        }
    }

    Ok(())
}

/// 获取 nonce
async fn get_nonce(
    State(state): State<AppState>,
    _guard: LocalhostGuard,
    headers: HeaderMap,
    connect_info: ConnectInfo<SocketAddr>,
) -> Result<Json<NonceResponse>, Response<Body>> {
    let config = &state.services.core.config;

    // 检查是否启用
    if !config.admin_registration.enabled {
        return Err(register_error_response(400, "M_UNKNOWN", "Admin registration is not enabled"));
    }

    // 检查 shared_secret
    if config.admin_registration.shared_secret.is_empty() {
        return Err(register_error_response(400, "M_UNKNOWN", "Shared secret is not configured"));
    }

    ensure_admin_registration_environment(config.admin_registration.production_only).map_err(|response| *response)?;
    ensure_admin_registration_ip_policy(&headers, &connect_info, &config.admin_registration.ip_whitelist)
        .map_err(|response| *response)?;

    let response = state
        .services
        .admin
        .user
        .admin_registration_service
        .generate_nonce()
        .await
        .map_err(map_admin_register_service_error)?;

    Ok(Json(NonceResponse { nonce: response.nonce }))
}

/// 注册管理员账号
async fn register(
    State(state): State<AppState>,
    _guard: LocalhostGuard,
    headers: HeaderMap,
    connect_info: ConnectInfo<SocketAddr>,
    Json(payload): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>, Response<Body>> {
    // Validate input
    if let Err(e) = payload.validate() {
        return Err(register_error_response(400, "M_INVALID_PARAM", &format!("Validation error: {e}")));
    }

    let config = &state.services.core.config;

    // 检查是否启用
    if !config.admin_registration.enabled {
        return Err(register_error_response(400, "M_UNKNOWN", "Admin registration is not enabled"));
    }

    ensure_admin_registration_environment(config.admin_registration.production_only).map_err(|response| *response)?;
    ensure_admin_registration_ip_policy(&headers, &connect_info, &config.admin_registration.ip_whitelist)
        .map_err(|response| *response)?;
    verify_additional_registration_controls(&state, &payload).await?;

    let display_name = payload.displayname.clone().unwrap_or_else(|| payload.username.clone());
    let response = state
        .services
        .admin
        .user
        .admin_registration_service
        .register_admin_user(AdminRegisterRequest {
            nonce: payload.nonce.clone(),
            username: payload.username.clone(),
            password: payload.password.clone(),
            admin: Some(payload.admin),
            user_type: payload.user_type.clone(),
            displayname: Some(display_name),
            mac: payload.mac.clone(),
        })
        .await
        .map_err(map_admin_register_service_error)?;

    Ok(Json(RegisterResponse {
        access_token: response.access_token,
        refresh_token: response.refresh_token,
        expires_in: response.expires_in.max(0) as u64,
        device_id: response.device_id,
        user_id: response.user_id,
        home_server: response.home_server,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_admin_registration_environment_blocks_non_production() {
        unsafe {
            std::env::set_var("RUST_ENV", "development");
        }
        let result = ensure_admin_registration_environment(true);
        unsafe {
            std::env::remove_var("RUST_ENV");
        }
        assert!(result.is_err());
    }

    #[test]
    fn test_ip_matches_whitelist_supports_ip_and_cidr() {
        let whitelist = vec!["127.0.0.1".to_string(), "10.0.0.0/8".to_string()];
        assert!(ip_matches_whitelist("127.0.0.1".parse().unwrap(), &whitelist));
        assert!(ip_matches_whitelist("10.10.1.3".parse().unwrap(), &whitelist));
        assert!(!ip_matches_whitelist("192.168.1.10".parse().unwrap(), &whitelist));
    }
}
