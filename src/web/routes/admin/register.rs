// 管理后台 - 管理员注册
// 实现 Synapse 兼容的管理员注册 API
// API: /_synapse/admin/v1/register/nonce, /_synapse/admin/v1/register
//
// 安全说明：此 API 默认仅允许从 localhost (127.0.0.1) 调用
// 如需从外部调用，请修改 allow_external_access 配置

use crate::auth::AuthService;
use crate::web::routes::AppState;
use axum::{
    body::Body,
    extract::State,
    response::Response,
    routing::{get, post},
    Json, Router,
};
use hmac::{Hmac, Mac};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};
use validator::Validate;

// HMAC 类型
type HmacSha256 = Hmac<Sha256>;

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

/// 生成随机 nonce
fn generate_nonce() -> String {
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..32).map(|_| rng.gen()).collect();
    use std::fmt::Write;
    let mut s = String::new();
    for byte in bytes {
        write!(&mut s, "{:02x}", byte).unwrap();
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

    let mut mac = HmacSha256::new_from_slice(shared_secret.as_bytes())
        .expect("HMAC can take key of any size");
    mac.update(&message);
    let result = mac.finalize();

    let expected = {
        use std::fmt::Write;
        let mut s = String::new();
        for byte in &result.into_bytes().to_vec() {
            write!(&mut s, "{:02x}", byte).unwrap();
        }
        s
    };
    expected == mac_hex
}

/// 获取 nonce
async fn get_nonce(State(state): State<AppState>) -> Result<Json<NonceResponse>, Response<Body>> {
    let config = &state.services.config;

    // 检查是否启用
    if !config.admin_registration.enabled {
        return Err(Response::builder()
            .status(400)
            .header("Content-Type", "application/json")
            .body(Body::from(
                serde_json::to_string(&RegisterError {
                    errcode: "M_UNKNOWN".to_string(),
                    error: "Admin registration is not enabled".to_string(),
                })
                .unwrap(),
            ))
            .unwrap());
    }

    // 检查 shared_secret
    if config.admin_registration.shared_secret.is_empty() {
        return Err(Response::builder()
            .status(400)
            .header("Content-Type", "application/json")
            .body(Body::from(
                serde_json::to_string(&RegisterError {
                    errcode: "M_UNKNOWN".to_string(),
                    error: "Shared secret is not configured".to_string(),
                })
                .unwrap(),
            ))
            .unwrap());
    }

    let nonce = generate_nonce();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let timeout = config.admin_registration.nonce_timeout_seconds;

    // 存储 nonce
    {
        let mut nonces = NONCES.lock().unwrap();
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
    Json(payload): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>, Response<Body>> {
    // Validate input
    if let Err(e) = payload.validate() {
        return Err(Response::builder()
            .status(400)
            .header("Content-Type", "application/json")
            .body(Body::from(
                serde_json::to_string(&RegisterError {
                    errcode: "M_INVALID_PARAM".to_string(),
                    error: format!("Validation error: {}", e),
                })
                .unwrap(),
            ))
            .unwrap());
    }

    let config = &state.services.config;

    // 检查是否启用
    if !config.admin_registration.enabled {
        return Err(Response::builder()
            .status(400)
            .header("Content-Type", "application/json")
            .body(Body::from(
                serde_json::to_string(&RegisterError {
                    errcode: "M_UNKNOWN".to_string(),
                    error: "Admin registration is not enabled".to_string(),
                })
                .unwrap(),
            ))
            .unwrap());
    }

    // 验证 nonce
    let nonce_valid = {
        let mut nonces = NONCES.lock().unwrap();
        if let Some(data) = nonces.get(&payload.nonce) {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
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
        return Err(Response::builder()
            .status(400)
            .header("Content-Type", "application/json")
            .body(Body::from(
                serde_json::to_string(&RegisterError {
                    errcode: "M_UNKNOWN".to_string(),
                    error: "Unrecognised nonce".to_string(),
                })
                .unwrap(),
            ))
            .unwrap());
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
        return Err(Response::builder()
            .status(400)
            .header("Content-Type", "application/json")
            .body(Body::from(
                serde_json::to_string(&RegisterError {
                    errcode: "M_UNKNOWN".to_string(),
                    error: "HMAC incorrect".to_string(),
                })
                .unwrap(),
            ))
            .unwrap());
    }

    // 创建用户
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
        .register(
            &payload.username,
            &payload.password,
            payload.admin,
            Some(&display_name),
        )
        .await;

    match register_result {
        Ok((_user, access_token, refresh_token, device_id)) => Ok(Json(RegisterResponse {
            access_token,
            refresh_token,
            expires_in: 3600,
            device_id,
            user_id: user_id.clone(),
            home_server: config.server.name.clone(),
        })),
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("already exists") {
                Err(Response::builder()
                    .status(400)
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&RegisterError {
                            errcode: "M_UNKNOWN".to_string(),
                            error: "User already exists".to_string(),
                        })
                        .unwrap(),
                    ))
                    .unwrap())
            } else {
                Err(Response::builder()
                    .status(500)
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&RegisterError {
                            errcode: "M_UNKNOWN".to_string(),
                            error: format!("Failed to create user: {}", error_msg),
                        })
                        .unwrap(),
                    ))
                    .unwrap())
            }
        }
    }
}
