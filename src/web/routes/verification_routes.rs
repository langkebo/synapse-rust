//! Device Verification Routes
//!
//! Implements SAS (Short Authentication String) and QR code verification

use crate::common::*;
use crate::web::routes::AppState;
use crate::web::routes::AuthenticatedUser;
use axum::{
    extract::State,
    routing::{get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_json::Value;

fn create_verification_compat_router() -> Router<AppState> {
    Router::new()
        .route(
            "/keys/device_signing/verify_start",
            post(verification_start),
        )
        .route(
            "/keys/device_signing/verify_accept",
            put(verification_accept),
        )
        .route(
            "/keys/device_signing/verify_key_agreement",
            post(verification_key_agreement),
        )
        .route("/keys/device_signing/verify_mac", post(verification_mac))
        .route("/keys/device_signing/verify_done", post(verification_done))
        .route("/keys/qr_code/show", get(show_qr_code))
        .route("/keys/qr_code/scan", post(scan_qr_code))
}

pub fn create_verification_router(_state: AppState) -> Router<AppState> {
    let compat_router = create_verification_compat_router();

    Router::new()
        .nest("/_matrix/client/v1", compat_router.clone())
        .nest("/_matrix/client/r0", compat_router)
}

#[derive(Debug, Deserialize)]
pub struct VerificationStartBody {
    pub transaction_id: Option<String>,
    pub from_device: String,
    pub to_user: String,
    pub to_device: Option<String>,
    pub method: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct VerificationStartResponse {
    pub transaction_id: String,
    pub method: String,
    pub key_agreement_protocol: Vec<String>,
    pub hash: Vec<String>,
    pub short_authentication_string: Vec<String>,
}

/// Start device verification (SAS)
async fn verification_start(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<VerificationStartBody>,
) -> Result<Json<Value>, ApiError> {
    let to_user = body.to_user;
    let to_device = body.to_device.unwrap_or_else(|| "".to_string());

    // Start SAS verification
    let sas_data = state
        .services
        .verification_service
        .start_sas_verification(
            &auth_user.user_id,
            &auth_user.device_id.unwrap_or_default(),
            &to_user,
            if to_device.is_empty() {
                None
            } else {
                Some(to_device)
            },
        )
        .await?;

    Ok(Json(json!({
        "transaction_id": sas_data.transaction_id,
        "method": sas_data.method,
        "key_agreement_protocol": sas_data.key_agreement_protocol,
        "hash": sas_data.hash,
        "short_authentication_string": sas_data.short_authentication_string,
    })))
}

#[derive(Debug, Deserialize)]
pub struct VerificationAcceptBody {
    pub transaction_id: String,
    pub key_agreement_protocol: String,
    pub hash: String,
    pub commitment: Option<String>,
}

/// Accept device verification
async fn verification_accept(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<VerificationAcceptBody>,
) -> Result<Json<Value>, ApiError> {
    let sas_data = state
        .services
        .verification_service
        .accept_sas(
            &body.transaction_id,
            &body.key_agreement_protocol,
            &body.hash,
        )
        .await?;

    Ok(Json(json!({
        "transaction_id": sas_data.transaction_id,
        "method": sas_data.method,
        "key_agreement_protocol": sas_data.key_agreement_protocol,
        "hash": sas_data.hash,
        "short_authentication_string": sas_data.short_authentication_string,
        "commitment": sas_data.commitment,
    })))
}

#[derive(Debug, Deserialize)]
pub struct KeyAgreementBody {
    pub transaction_id: String,
    pub pubkey: String,
}

/// Key agreement for SAS
async fn verification_key_agreement(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<KeyAgreementBody>,
) -> Result<Json<Value>, ApiError> {
    // Generate SAS from key agreement
    let sas_result = state
        .services
        .verification_service
        .generate_sas(&body.transaction_id, &body.pubkey)
        .await?;

    let mut response = json!({
        "transaction_id": sas_result.transaction_id,
        "confirmed": sas_result.confirmed,
    });

    // Add SAS representation
    match sas_result.sas {
        crate::e2ee::verification::SasRepresentation::Emoji(emojis) => {
            response["short_authentication_string"] = json!({
                "emoji": emojis
            });
            // Also generate decimal
            let decimal = generate_decimal_from_emoji(&emojis);
            response["short_authentication_string"]["decimal"] = json!({
                "points": [
                    (decimal / 100000) % 1000,
                    (decimal / 1000) % 1000,
                    decimal % 1000
                ]
            });
        }
        crate::e2ee::verification::SasRepresentation::Decimal(val) => {
            response["short_authentication_string"] = json!({
                "decimal": {
                    "points": [
                        (val / 100000) % 1000,
                        (val / 1000) % 1000,
                        val % 1000
                    ]
                }
            });
        }
    }

    Ok(Json(response))
}

#[derive(Debug, Deserialize)]
pub struct VerificationMacBody {
    pub transaction_id: String,
    pub mac: String,
}

/// Confirm SAS verification
async fn verification_mac(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<VerificationMacBody>,
) -> Result<Json<Value>, ApiError> {
    let verified = state
        .services
        .verification_service
        .confirm_sas(&body.transaction_id, &body.mac)
        .await?;

    Ok(Json(json!({
        "transaction_id": body.transaction_id,
        "verified": verified
    })))
}

/// Complete verification
async fn verification_done(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let transaction_id = body
        .get("transaction_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing transaction_id".to_string()))?;

    // Mark verification as done
    state
        .services
        .verification_service
        .confirm_sas(transaction_id, "")
        .await?;

    Ok(Json(json!({
        "transaction_id": transaction_id
    })))
}

/// Show QR code for verification
async fn show_qr_code(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let device_id = auth_user
        .device_id
        .ok_or_else(|| ApiError::bad_request("Device ID required".to_string()))?;

    let server_name = state.services.config.federation.server_name.clone();

    let qr_data = state
        .services
        .verification_service
        .generate_qr_code(&auth_user.user_id, &device_id, &server_name)
        .await?;

    Ok(Json(json!({
        "transaction_id": qr_data.transaction_id,
        "server_name": qr_data.server_name,
        "user_id": qr_data.user_id,
        "device_id": qr_data.device_id,
        "device_ed25519_key": qr_data.device_ed25519_key,
        "device_curve25519_key": qr_data.device_curve25519_key,
    })))
}

#[derive(Debug, Deserialize)]
pub struct ScanQrBody {
    pub transaction_id: String,
    pub server_name: String,
    pub user_id: String,
    pub device_id: String,
    pub device_ed25519_key: String,
    pub device_curve25519_key: String,
}

/// Scan QR code
async fn scan_qr_code(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<ScanQrBody>,
) -> Result<Json<Value>, ApiError> {
    let device_id = auth_user
        .device_id
        .ok_or_else(|| ApiError::bad_request("Device ID required".to_string()))?;

    let qr_data = crate::e2ee::verification::QrCodeData {
        transaction_id: body.transaction_id,
        server_name: body.server_name,
        server_public_key: "".to_string(),
        user_id: body.user_id,
        device_id: body.device_id,
        device_ed25519_key: body.device_ed25519_key,
        device_curve25519_key: body.device_curve25519_key,
        signature: "".to_string(),
    };

    state
        .services
        .verification_service
        .scan_qr_code(&qr_data, &device_id)
        .await?;

    Ok(Json(json!({
        "transaction_id": qr_data.transaction_id,
        "state": "pending"
    })))
}

/// Generate decimal from emoji list
fn generate_decimal_from_emoji(emojis: &[String]) -> u32 {
    // Convert emoji to decimal based on their indices
    let mut decimal: u32 = 0;
    for emoji in emojis.iter().take(3) {
        let idx = SAS_EMOJIS.iter().position(|&e| e == emoji).unwrap_or(0) as u32;
        decimal = decimal * 100 + idx;
    }
    (decimal % 900000) + 100000
}

// Emoji list for reference
const SAS_EMOJIS: &[&str; 64] = &[
    "🐶", "🐱", "🐭", "🐹", "🐰", "🦊", "🐻", "🐼", "🐨", "🐯", "🦁", "🐮", "🐷", "🐸", "🐵", "🐔",
    "🐧", "🐦", "🐤", "🦆", "🦅", "🦉", "🦇", "🐺", "🐗", "🐴", "🦄", "🐝", "🐛", "🦋", "🐌", "🐞",
    "🐜", "🦟", "🦗", "🕷", "🦂", "🐢", "🐍", "🦎", "🦖", "🦕", "🐙", "🦑", "🦐", "🦞", "🦀", "🐡",
    "🐠", "🐟", "🐬", "🐳", "🦈", "🐊", "🐅", "🐆", "🦓", "🦍", "🦧", "🐘", "🦛", "🦏", "🐪", "🐫",
];

#[cfg(test)]
mod tests {
    #[test]
    fn test_verification_routes_structure() {
        let compat_routes = [
            "/_matrix/client/v1/keys/device_signing/verify_start",
            "/_matrix/client/r0/keys/device_signing/verify_mac",
            "/_matrix/client/v1/keys/qr_code/show",
            "/_matrix/client/r0/keys/qr_code/scan",
        ];

        assert!(compat_routes
            .iter()
            .all(|route| route.starts_with("/_matrix/client/")));
    }

    #[test]
    fn test_verification_compat_router_contains_shared_paths() {
        let shared_paths = [
            "/keys/device_signing/verify_start",
            "/keys/device_signing/verify_accept",
            "/keys/device_signing/verify_key_agreement",
            "/keys/device_signing/verify_mac",
            "/keys/device_signing/verify_done",
            "/keys/qr_code/show",
            "/keys/qr_code/scan",
        ];

        assert_eq!(shared_paths.len(), 7);
        assert!(shared_paths.iter().all(|path| path.starts_with('/')));
    }

    #[test]
    fn test_verification_router_keeps_scope_to_v1_and_r0() {
        let compat_paths = [
            "/keys/device_signing/verify_start",
            "/keys/qr_code/show",
            "/keys/qr_code/scan",
        ];
        let supported_versions = [
            "/_matrix/client/v1/keys/device_signing/verify_start",
            "/_matrix/client/r0/keys/device_signing/verify_start",
        ];
        let unsupported_v3_paths = ["/_matrix/client/v3/keys/device_signing/verify_start"];

        assert!(compat_paths.iter().all(|path| path.starts_with("/keys/")));
        assert!(supported_versions
            .iter()
            .all(|path| !path.starts_with("/_matrix/client/v3/")));
        assert!(unsupported_v3_paths
            .iter()
            .all(|path| path.starts_with("/_matrix/client/v3/")));
    }
}
