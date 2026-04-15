use crate::common::config::SecurityConfig;
use crate::common::ApiError;
use crate::storage::User;
use crate::web::routes::AppState;
use axum::http::{HeaderMap, Method};
use hmac::{Hmac, Mac};
use sha1::Sha1;
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha1 = Hmac<Sha1>;

#[derive(Clone, Debug)]
pub(crate) struct AuthorizedAdmin {
    pub user_id: String,
    pub device_id: Option<String>,
    pub access_token: String,
    pub role: String,
}

pub(crate) async fn authorize_admin_request(
    headers: &HeaderMap,
    method: &Method,
    path: &str,
    state: &AppState,
) -> Result<AuthorizedAdmin, ApiError> {
    let access_token = super::auth::bearer_token(headers)?;
    let (user_id, device_id, is_admin, _, _) = state
        .services
        .auth_service
        .validate_token(&access_token)
        .await?;

    if !is_admin {
        return Err(ApiError::forbidden("Admin access required".to_string()));
    }

    let user = state
        .services
        .user_storage
        .get_user_by_id(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to load admin user: {}", e)))?
        .ok_or_else(|| ApiError::unauthorized("Admin user not found".to_string()))?;

    if !user.is_admin {
        return Err(ApiError::forbidden(
            "Admin access has been revoked".to_string(),
        ));
    }

    let role = normalize_admin_role(user.user_type.as_deref());
    if state.services.config.security.admin_rbac_enabled && !is_role_allowed(&role, method, path) {
        return Err(ApiError::forbidden(format!(
            "Admin role '{}' is not allowed to access this resource",
            role
        )));
    }

    if should_require_admin_mfa(&state.services.config.security, method, path) {
        let mfa_code = headers
            .get("x-admin-mfa-code")
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                ApiError::forbidden("Sensitive admin operation requires MFA code".to_string())
            })?;

        verify_totp_code(&state.services.config.security, mfa_code, Some(&user))?;
    }

    Ok(AuthorizedAdmin {
        user_id,
        device_id,
        access_token,
        role,
    })
}

pub(crate) async fn enforce_admin_login_mfa(
    state: &AppState,
    username: &str,
    mfa_code: Option<&str>,
) -> Result<(), ApiError> {
    if !state.services.config.security.admin_mfa_required {
        return Ok(());
    }

    let Some(user) = state
        .services
        .user_storage
        .get_user_by_identifier(username)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to load user for admin MFA: {}", e)))?
    else {
        return Ok(());
    };

    if !user.is_admin {
        return Ok(());
    }

    let mfa_code = mfa_code
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::forbidden("Admin login requires MFA code".to_string()))?;

    verify_totp_code(&state.services.config.security, mfa_code, Some(&user))
}

pub(crate) fn normalize_admin_role(user_type: Option<&str>) -> String {
    match user_type.map(str::trim).filter(|value| !value.is_empty()) {
        None => {
            ::tracing::warn!(
                target: "security_audit",
                event = "admin_role_fallback",
                "Admin user has no user_type set - defaulting to 'admin' role (not super_admin). Set user_type explicitly for proper RBAC."
            );
            "admin".to_string()
        }
        Some("admin") => "admin".to_string(),
        Some("super_admin") => "super_admin".to_string(),
        Some(value) => value.to_ascii_lowercase(),
    }
}

pub(crate) fn should_require_admin_mfa(
    security: &SecurityConfig,
    method: &Method,
    path: &str,
) -> bool {
    security.admin_mfa_required
        && !security.admin_mfa_shared_secret.trim().is_empty()
        && is_sensitive_admin_request(method, path)
}

fn is_role_allowed(role: &str, method: &Method, path: &str) -> bool {
    if role == "super_admin" {
        return true;
    }

    let is_read = matches!(*method, Method::GET | Method::HEAD);
    match role {
        "admin" => {
            path.starts_with("/_synapse/admin/v1/users")
                || path.starts_with("/_synapse/admin/v1/register")
                || path.starts_with("/_synapse/admin/v1/registration_tokens")
                || path.starts_with("/_synapse/admin/v1/notifications")
                || path.starts_with("/_synapse/admin/v1/media")
                || path.starts_with("/_synapse/admin/v1/rooms")
                || (is_read && path.starts_with("/_synapse/admin/v1/"))
        }
        "auditor" => {
            is_read
                && (path.starts_with("/_synapse/admin/v1/audit")
                    || path.starts_with("/_synapse/admin/v1/event_reports")
                    || path.starts_with("/_synapse/admin/v1/security"))
        }
        "security_admin" => {
            path.starts_with("/_synapse/admin/v1/security")
                || path.starts_with("/_synapse/admin/v1/audit")
                || path.starts_with("/_synapse/admin/v1/event_reports")
                || path.starts_with("/_synapse/admin/v1/server")
        }
        "user_admin" => {
            path.starts_with("/_synapse/admin/v1/users")
                || path.starts_with("/_synapse/admin/v1/register")
                || path.starts_with("/_synapse/admin/v1/registration_tokens")
                || path.starts_with("/_synapse/admin/v1/notifications")
        }
        "media_admin" => path.starts_with("/_synapse/admin/v1/media"),
        _ => false,
    }
}

fn is_sensitive_admin_request(method: &Method, path: &str) -> bool {
    if matches!(
        *method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    ) {
        return true;
    }

    path.starts_with("/_synapse/admin/v1/security")
        || path.starts_with("/_synapse/admin/v1/server")
        || path.starts_with("/_synapse/admin/v1/media/quarantine")
}

fn verify_totp_code(
    security: &SecurityConfig,
    provided_code: &str,
    user: Option<&User>,
) -> Result<(), ApiError> {
    if !security.admin_mfa_required {
        return Ok(());
    }

    let secret = decode_secret(&security.admin_mfa_shared_secret).ok_or_else(|| {
        ApiError::forbidden(
            "Admin MFA is enabled but no valid TOTP secret is configured".to_string(),
        )
    })?;

    let provided_code = provided_code.trim();
    if provided_code.len() != 6 || !provided_code.chars().all(|ch| ch.is_ascii_digit()) {
        return Err(ApiError::forbidden(
            "Invalid admin MFA code format".to_string(),
        ));
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ApiError::internal("System time error".to_string()))?
        .as_secs();
    let current_step = now / 30;
    let drift = security.admin_mfa_allowed_drift_steps as i64;

    for offset in -drift..=drift {
        let Some(step) = current_step.checked_add_signed(offset) else {
            continue;
        };

        if generate_totp_code(&secret, step) == provided_code {
            return Ok(());
        }
    }

    let user_id = user
        .map(|value| value.user_id.as_str())
        .unwrap_or("unknown");
    tracing::warn!(target: "admin_auth", user_id, "Admin MFA verification failed");
    Err(ApiError::forbidden("Invalid admin MFA code".to_string()))
}

fn generate_totp_code(secret: &[u8], step: u64) -> String {
    let mut mac = HmacSha1::new_from_slice(secret).expect("TOTP secret must be non-empty");
    mac.update(&step.to_be_bytes());
    let hash = mac.finalize().into_bytes();
    let offset = (hash[19] & 0x0f) as usize;
    let binary = ((u32::from(hash[offset]) & 0x7f) << 24)
        | (u32::from(hash[offset + 1]) << 16)
        | (u32::from(hash[offset + 2]) << 8)
        | u32::from(hash[offset + 3]);
    format!("{:06}", binary % 1_000_000)
}

fn decode_secret(secret: &str) -> Option<Vec<u8>> {
    let trimmed = secret.trim();
    if trimmed.is_empty() {
        return None;
    }

    decode_base32_secret(trimmed).or_else(|| Some(trimmed.as_bytes().to_vec()))
}

fn decode_base32_secret(secret: &str) -> Option<Vec<u8>> {
    let mut bits = 0u32;
    let mut bit_count = 0u8;
    let mut output = Vec::new();

    for ch in secret.chars().filter(|ch| !matches!(ch, ' ' | '-')) {
        if ch == '=' {
            break;
        }

        let value = match ch.to_ascii_uppercase() {
            'A'..='Z' => ch.to_ascii_uppercase() as u8 - b'A',
            '2'..='7' => (ch as u8 - b'2') + 26,
            _ => return None,
        };

        bits = (bits << 5) | u32::from(value);
        bit_count += 5;
        while bit_count >= 8 {
            bit_count -= 8;
            output.push(((bits >> bit_count) & 0xff) as u8);
            bits &= (1 << bit_count) - 1;
        }
    }

    if output.is_empty() {
        None
    } else {
        Some(output)
    }
}
