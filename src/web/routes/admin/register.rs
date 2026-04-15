// 管理后台 - 管理员注册
// 实现 Synapse 兼容的管理员注册 API
// API: /_synapse/admin/v1/register/nonce, /_synapse/admin/v1/register
//
// 安全说明：此 API 默认仅允许从 localhost (127.0.0.1) 调用
// 如需从外部调用，请修改 allow_external_access 配置

use crate::auth::AuthService;
use crate::services::captcha_service::VerifyCaptchaRequest;
use crate::web::routes::AppState;
use crate::web::utils::ip::extract_client_ip;
use axum::{
    body::Body,
    extract::ConnectInfo,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use hmac::{Hmac, Mac};
use ipnetwork::IpNetwork;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use std::net::IpAddr;
use std::net::SocketAddr;
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;
use validator::Validate;

// HMAC 类型，保持与 Synapse 共享密钥注册接口一致
type HmacSha1 = Hmac<Sha1>;

// nonce 存储 (内存中，生产环境应该用 Redis)
lazy_static::lazy_static! {
    static ref NONCES: std::sync::Mutex<std::collections::HashMap<String, NonceData>> =
        std::sync::Mutex::new(std::collections::HashMap::new());
}

#[derive(Clone)]
#[allow(dead_code)]
struct NonceData {
    created_ts: u64,
    expires_at: u64,
}

pub fn create_register_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/_synapse/admin/v1/register/nonce", get(get_nonce))
        .route("/_synapse/admin/v1/register", post(register))
        .with_state(state)
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

#[derive(Serialize)]
struct RegisterError {
    errcode: String,
    error: String,
}

fn register_error_response(status: u16, errcode: &str, error: impl Into<String>) -> Response<Body> {
    let body = serde_json::to_string(&RegisterError {
        errcode: errcode.to_string(),
        error: error.into(),
    })
    .unwrap_or_else(|_| r#"{"errcode":"M_UNKNOWN","error":"Internal error"}"#.to_string());

    let mut response = Response::new(Body::from(body));
    *response.status_mut() =
        StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/json"),
    );
    response
}

/// 生成随机 nonce
fn generate_nonce() -> String {
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..32).map(|_| rng.gen()).collect();
    use std::fmt::Write;
    let mut s = String::with_capacity(64);
    for byte in bytes {
        let _ = write!(&mut s, "{:02x}", byte);
    }
    s
}

/// 验证 HMAC-SHA256
fn verify_mac(
    shared_secret: &str,
    nonce: &str,
    username: &str,
    password: &str,
    admin: bool,
    user_type: &Option<String>,
    mac_hex: &str,
) -> bool {
    let mut message = nonce.as_bytes().to_vec();
    message.push(b'\x00');
    message.extend(username.as_bytes());
    message.push(b'\x00');
    message.extend(password.as_bytes());
    message.push(b'\x00');
    match admin {
        true => message.extend(b"admin"),
        false => message.extend(b"notadmin"),
    }

    if let Some(ref ut) = user_type {
        message.push(b'\x00');
        message.extend(ut.as_bytes());
    }

    let mut mac = HmacSha1::new_from_slice(shared_secret.as_bytes())
        .expect("HMAC can take key of any size");
    mac.update(&message);
    let result = mac.finalize();

    let expected = {
        use std::fmt::Write;
        let mut s = String::with_capacity(64);
        for byte in &result.into_bytes().to_vec() {
            let _ = write!(&mut s, "{:02x}", byte);
        }
        s
    };
    expected == mac_hex
}

fn extract_registration_client_ip(headers: &HeaderMap) -> Option<String> {
    let priority = vec![
        "x-forwarded-for".to_string(),
        "x-real-ip".to_string(),
        "forwarded".to_string(),
    ];
    extract_client_ip(headers, &priority)
}

fn is_local_client_ip(ip: &str) -> bool {
    if ip.eq_ignore_ascii_case("localhost") {
        return true;
    }
    ip.parse::<IpAddr>()
        .map(|ip| ip.is_loopback())
        .unwrap_or(false)
}

fn is_local_registration_origin(value: &str) -> bool {
    if value.eq_ignore_ascii_case("null") {
        return false;
    }
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let normalized_host = host.trim_matches(|c| c == '[' || c == ']');
    normalized_host
        .parse::<IpAddr>()
        .map(|ip| ip.is_loopback())
        .unwrap_or(false)
}

fn ensure_local_admin_registration_request(
    headers: &HeaderMap,
    connect_info: &ConnectInfo<SocketAddr>,
    allow_external_access: bool,
) -> Result<(), Box<Response<Body>>> {
    if allow_external_access {
        return Ok(());
    }

    let remote_ip = connect_info.0.ip();
    if !remote_ip.is_loopback() {
        return Err(Box::new(register_error_response(
            403,
            "M_FORBIDDEN",
            "Admin registration is only available from localhost",
        )));
    }

    if let Some(client_ip) = extract_registration_client_ip(headers) {
        if !is_local_client_ip(&client_ip) {
            return Err(Box::new(register_error_response(
                403,
                "M_FORBIDDEN",
                "Admin registration is only available from localhost",
            )));
        }
    }

    if let Some(origin) = headers.get("origin").and_then(|value| value.to_str().ok()) {
        if !is_local_registration_origin(origin) {
            return Err(Box::new(register_error_response(
                403,
                "M_FORBIDDEN",
                "Admin registration origin is not allowed",
            )));
        }
    }

    if let Some(referer) = headers.get("referer").and_then(|value| value.to_str().ok()) {
        if !is_local_registration_origin(referer) {
            return Err(Box::new(register_error_response(
                403,
                "M_FORBIDDEN",
                "Admin registration origin is not allowed",
            )));
        }
    }

    Ok(())
}

fn runtime_environment() -> String {
    std::env::var("RUST_ENV")
        .unwrap_or_else(|_| "production".to_string())
        .to_ascii_lowercase()
}

fn ensure_admin_registration_environment(
    production_only: bool,
) -> Result<(), Box<Response<Body>>> {
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
    allow_external_access: bool,
    ip_whitelist: &[String],
) -> Result<(), Box<Response<Body>>> {
    ensure_local_admin_registration_request(headers, connect_info, allow_external_access)?;

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

    if let Some(client_ip) = extract_registration_client_ip(headers)
        .and_then(|ip| ip.parse::<IpAddr>().ok())
    {
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
    if state.services.config.admin_registration.require_captcha {
        let captcha_id = payload.captcha_id.as_ref().ok_or_else(|| {
            register_error_response(400, "M_INVALID_PARAM", "captcha_id is required")
        })?;
        let captcha_code = payload.captcha_code.as_ref().ok_or_else(|| {
            register_error_response(400, "M_INVALID_PARAM", "captcha_code is required")
        })?;

        let verified = state
            .services
            .captcha_service
            .verify_captcha(VerifyCaptchaRequest {
                captcha_id: captcha_id.clone(),
                code: captcha_code.clone(),
            })
            .await
            .map_err(|e| register_error_response(400, "M_FORBIDDEN", e.to_string()))?;

        if !verified {
            return Err(register_error_response(
                400,
                "M_FORBIDDEN",
                "Captcha verification failed",
            ));
        }
    }

    if state.services.config.admin_registration.require_manual_approval {
        let approval_token = payload.approval_token.as_ref().ok_or_else(|| {
            register_error_response(400, "M_INVALID_PARAM", "approval_token is required")
        })?;

        if !state
            .services
            .config
            .admin_registration
            .approval_tokens
            .iter()
            .any(|token| token == approval_token)
        {
            return Err(register_error_response(
                403,
                "M_FORBIDDEN",
                "Manual approval token is invalid",
            ));
        }
    }

    Ok(())
}

/// 获取 nonce
async fn get_nonce(
    State(state): State<AppState>,
    headers: HeaderMap,
    connect_info: ConnectInfo<SocketAddr>,
) -> Result<Json<NonceResponse>, Response<Body>> {
    let config = &state.services.config;

    // 检查是否启用
    if !config.admin_registration.enabled {
        return Err(register_error_response(
            400,
            "M_UNKNOWN",
            "Admin registration is not enabled",
        ));
    }

    // 检查 shared_secret
    if config.admin_registration.shared_secret.is_empty() {
        return Err(register_error_response(
            400,
            "M_UNKNOWN",
            "Shared secret is not configured",
        ));
    }

    ensure_admin_registration_environment(config.admin_registration.production_only)
        .map_err(|response| *response)?;
    ensure_admin_registration_ip_policy(
        &headers,
        &connect_info,
        config.admin_registration.allow_external_access,
        &config.admin_registration.ip_whitelist,
    )
    .map_err(|response| *response)?;

    let nonce = generate_nonce();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| register_error_response(500, "M_UNKNOWN", "System time error"))?
        .as_secs();
    let timeout = config.admin_registration.nonce_timeout_seconds;

    // 存储 nonce
    {
        let mut nonces = NONCES
            .lock()
            .map_err(|_| register_error_response(500, "M_UNKNOWN", "Lock poisoned"))?;
        nonces.insert(
            nonce.clone(),
            NonceData {
                created_ts: now,
                expires_at: now + timeout,
            },
        );
    }

    Ok(Json(NonceResponse { nonce }))
}

/// 注册管理员账号
async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    connect_info: ConnectInfo<SocketAddr>,
    Json(payload): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>, Response<Body>> {
    // Validate input
    if let Err(e) = payload.validate() {
        return Err(register_error_response(
            400,
            "M_INVALID_PARAM",
            format!("Validation error: {}", e),
        ));
    }

    let config = &state.services.config;

    // 检查是否启用
    if !config.admin_registration.enabled {
        return Err(register_error_response(
            400,
            "M_UNKNOWN",
            "Admin registration is not enabled",
        ));
    }

    ensure_admin_registration_environment(config.admin_registration.production_only)
        .map_err(|response| *response)?;
    ensure_admin_registration_ip_policy(
        &headers,
        &connect_info,
        config.admin_registration.allow_external_access,
        &config.admin_registration.ip_whitelist,
    )
    .map_err(|response| *response)?;
    verify_additional_registration_controls(&state, &payload).await?;

    // 验证 nonce
    let nonce_valid = {
        let mut nonces = NONCES
            .lock()
            .map_err(|_| register_error_response(500, "M_UNKNOWN", "Lock poisoned"))?;
        if let Some(data) = nonces.get(&payload.nonce) {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| register_error_response(500, "M_UNKNOWN", "System time error"))?
                .as_secs();
            if now <= data.expires_at {
                nonces.remove(&payload.nonce); // 使用后删除
                true
            } else {
                false
            }
        } else {
            false
        }
    };

    if !nonce_valid {
        return Err(register_error_response(
            400,
            "M_UNKNOWN",
            "Unrecognised nonce",
        ));
    }

    // 验证 HMAC
    if !verify_mac(
        &config.admin_registration.shared_secret,
        &payload.nonce,
        &payload.username,
        &payload.password,
        payload.admin,
        &payload.user_type,
        &payload.mac,
    ) {
        return Err(register_error_response(400, "M_UNKNOWN", "HMAC incorrect"));
    }

    // 创建用户。保持与上游 shared-secret registration 一致，只允许部署侧显式调用。
    let user_id = format!("@{}:{}", payload.username, config.server.name);
    let display_name = payload.displayname.unwrap_or(payload.username.clone());

    // 使用 AuthService 注册用户
    let auth_service = AuthService::new(
        &state.services.user_storage.pool,
        state.cache.clone(),
        state.services.metrics.clone(),
        &config.security,
        &config.server.name,
    );

    let register_result = auth_service
        .register(&payload.username, &payload.password, payload.admin, Some(&display_name))
        .await;

    match register_result {
        Ok((_user, access_token, refresh_token, device_id)) => {
            Ok(Json(RegisterResponse {
                access_token,
                refresh_token,
                expires_in: 3600,
                device_id,
                user_id: user_id.clone(),
                home_server: config.server.name.clone(),
            }))
        }
        Err(e) => {
            let error_msg = e.to_string();
            let error_msg_lower = error_msg.to_lowercase();
            let user_conflict = error_msg_lower.contains("already exists")
                || error_msg_lower.contains("already taken")
                || error_msg_lower.contains("duplicate key value")
                || error_msg_lower.contains("unique constraint")
                || error_msg_lower.contains("user_in_use")
                || error_msg_lower.contains("m_user_in_use");
            if user_conflict {
                Err(register_error_response(
                    400,
                    "M_USER_IN_USE",
                    "User already exists",
                ))
            } else {
                Err(register_error_response(
                    500,
                    "M_UNKNOWN",
                    format!("Failed to create user: {}", error_msg),
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_local_registration_origin() {
        assert!(is_local_registration_origin("http://localhost:8008"));
        assert!(is_local_registration_origin("https://127.0.0.1:8448"));
        assert!(is_local_registration_origin("http://[::1]:8008"));
        assert!(!is_local_registration_origin("https://example.com"));
        assert!(!is_local_registration_origin("null"));
    }

    #[test]
    fn test_ensure_local_admin_registration_request_rejects_non_local_origin() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "https://evil.example.com".parse().unwrap());
        let connect_info = ConnectInfo("127.0.0.1:8008".parse::<SocketAddr>().unwrap());

        let result = ensure_local_admin_registration_request(&headers, &connect_info, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_ensure_local_admin_registration_request_accepts_local_origin() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "http://localhost:3000".parse().unwrap());
        headers.insert("referer", "http://127.0.0.1:3000/setup".parse().unwrap());
        let connect_info = ConnectInfo("127.0.0.1:8008".parse::<SocketAddr>().unwrap());

        let result = ensure_local_admin_registration_request(&headers, &connect_info, false);
        assert!(result.is_ok());
    }

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
