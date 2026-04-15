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
        .route(
            "/keys/device_signing/verify_cancel",
            post(verification_cancel),
        )
        .route(
            "/keys/device_signing/requests",
            get(list_verification_requests),
        )
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

async fn verification_start(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<VerificationStartBody>,
) -> Result<Json<Value>, ApiError> {
    let to_user = body.to_user;
    let to_device = body.to_device.unwrap_or_else(|| "".to_string());

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

async fn verification_accept(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<VerificationAcceptBody>,
) -> Result<Json<Value>, ApiError> {
    let request = state
        .services
        .verification_service
        .get_request(&body.transaction_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Verification request not found".to_string()))?;

    let device_id = auth_user.device_id.unwrap_or_default();
    let is_participant = request.from_user == auth_user.user_id
        || request.to_user == auth_user.user_id
        || request.from_device == device_id
        || request.to_device.as_deref() == Some(device_id.as_str());

    if !is_participant {
        return Err(ApiError::forbidden(
            "Cannot accept another user's verification request".to_string(),
        ));
    }

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

async fn verification_key_agreement(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<KeyAgreementBody>,
) -> Result<Json<Value>, ApiError> {
    let request = state
        .services
        .verification_service
        .get_request(&body.transaction_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Verification request not found".to_string()))?;

    let device_id = auth_user.device_id.unwrap_or_default();
    let is_participant = request.from_user == auth_user.user_id
        || request.to_user == auth_user.user_id
        || request.from_device == device_id
        || request.to_device.as_deref() == Some(device_id.as_str());

    if !is_participant {
        return Err(ApiError::forbidden(
            "Cannot participate in another user's verification".to_string(),
        ));
    }

    let sas_result = state
        .services
        .verification_service
        .generate_sas(&body.transaction_id, &body.pubkey)
        .await?;

    let mut response = json!({
        "transaction_id": sas_result.transaction_id,
        "confirmed": sas_result.confirmed,
    });

    match sas_result.sas {
        crate::e2ee::verification::SasRepresentation::Emoji(emojis) => {
            response["short_authentication_string"] = json!({
                "emoji": emojis
            });
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

async fn verification_mac(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<VerificationMacBody>,
) -> Result<Json<Value>, ApiError> {
    let request = state
        .services
        .verification_service
        .get_request(&body.transaction_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Verification request not found".to_string()))?;

    let device_id = auth_user.device_id.unwrap_or_default();
    let is_participant = request.from_user == auth_user.user_id
        || request.to_user == auth_user.user_id
        || request.from_device == device_id
        || request.to_device.as_deref() == Some(device_id.as_str());

    if !is_participant {
        return Err(ApiError::forbidden(
            "Cannot confirm another user's verification".to_string(),
        ));
    }

    if body.mac.is_empty() {
        return Err(ApiError::bad_request(
            "MAC must not be empty".to_string(),
        ));
    }

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

async fn verification_done(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let transaction_id = body
        .get("transaction_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing transaction_id".to_string()))?;

    let mac = body
        .get("mac")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing mac".to_string()))?;

    let request = state
        .services
        .verification_service
        .get_request(transaction_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Verification request not found".to_string()))?;

    let device_id = auth_user.device_id.unwrap_or_default();
    let is_participant = request.from_user == auth_user.user_id
        || request.to_user == auth_user.user_id
        || request.from_device == device_id
        || request.to_device.as_deref() == Some(device_id.as_str());

    if !is_participant {
        return Err(ApiError::forbidden(
            "Cannot complete another user's verification".to_string(),
        ));
    }

    if mac.is_empty() {
        return Err(ApiError::bad_request(
            "MAC must not be empty for verification completion".to_string(),
        ));
    }

    state
        .services
        .verification_service
        .confirm_sas(transaction_id, mac)
        .await?;

    Ok(Json(json!({
        "transaction_id": transaction_id
    })))
}

#[derive(Debug, Deserialize)]
pub struct VerificationCancelBody {
    pub transaction_id: String,
    pub code: String,
    pub reason: String,
}

async fn verification_cancel(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<VerificationCancelBody>,
) -> Result<Json<Value>, ApiError> {
    let request = state
        .services
        .verification_service
        .get_request(&body.transaction_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Verification request not found".to_string()))?;

    let device_id = auth_user.device_id.unwrap_or_default();
    let is_participant = request.from_user == auth_user.user_id
        || request.to_user == auth_user.user_id
        || request.from_device == device_id
        || request.to_device.as_deref() == Some(device_id.as_str());

    if !is_participant {
        return Err(ApiError::forbidden(
            "Cannot cancel another user's verification request".to_string(),
        ));
    }

    state
        .services
        .verification_service
        .cancel_verification(&body.transaction_id, &body.code, &body.reason)
        .await?;

    Ok(Json(json!({
        "transaction_id": body.transaction_id,
        "state": "cancelled",
        "code": body.code,
        "reason": body.reason,
    })))
}

async fn list_verification_requests(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let requests = state
        .services
        .verification_service
        .get_pending_verifications(&auth_user.user_id)
        .await?;

    Ok(Json(json!({
        "requests": requests.into_iter().map(serialize_verification_request).collect::<Vec<_>>()
    })))
}

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
        server_public_key: String::new(),
        user_id: body.user_id,
        device_id: body.device_id,
        device_ed25519_key: body.device_ed25519_key,
        device_curve25519_key: body.device_curve25519_key,
        signature: String::new(),
    };

    state
        .services
        .verification_service
        .scan_qr_code(&qr_data, &device_id, &auth_user.user_id)
        .await?;

    Ok(Json(json!({
        "transaction_id": qr_data.transaction_id,
        "state": "pending"
    })))
}

fn generate_decimal_from_emoji(emojis: &[String]) -> u32 {
    let mut decimal: u32 = 0;
    for emoji in emojis.iter().take(3) {
        let idx = SAS_EMOJIS.iter().position(|&e| e == emoji).unwrap_or(0) as u32;
        decimal = decimal * 100 + idx;
    }
    (decimal % 900000) + 100000
}

fn serialize_verification_request(
    request: crate::e2ee::verification::VerificationRequest,
) -> Value {
    json!({
        "transaction_id": request.transaction_id,
        "from_user": request.from_user,
        "from_device": request.from_device,
        "to_user": request.to_user,
        "to_device": request.to_device,
        "method": request.method,
        "state": request.state,
        "created_ts": request.created_ts,
        "updated_ts": request.updated_ts,
    })
}

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
            "/keys/device_signing/verify_cancel",
            "/keys/device_signing/requests",
            "/keys/qr_code/show",
            "/keys/qr_code/scan",
        ];

        assert_eq!(shared_paths.len(), 9);
        assert!(shared_paths.iter().all(|path| path.starts_with('/')));
    }

    #[test]
    fn test_serialize_verification_request() {
        let request = crate::e2ee::verification::VerificationRequest {
            transaction_id: "txn-1".to_string(),
            from_user: "@alice:example.org".to_string(),
            from_device: "ALICE".to_string(),
            to_user: "@bob:example.org".to_string(),
            to_device: Some("BOB".to_string()),
            method: crate::e2ee::verification::VerificationMethod::Sas,
            state: crate::e2ee::verification::VerificationState::Requested,
            created_ts: 1,
            updated_ts: 2,
        };

        let json = super::serialize_verification_request(request);
        assert_eq!(json["transaction_id"], "txn-1");
        assert_eq!(json["method"], "sas");
        assert_eq!(json["state"], "requested");
    }
}
