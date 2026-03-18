// QR Login Routes - MSC4388
// Secure out-of-band channel for sign in with QR

use crate::web::routes::{ApiError, AppState, AuthenticatedUser};
use axum::{
    extract::{Path, State},
    Json,
};
use serde_json::{json, Value};

/// Get login QR code
/// Generates a QR code for login confirmation
/// POST /_matrix/client/v1/login/get_qr_code
pub async fn get_qr_code(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let transaction_id = format!(
        "qr_{}_{}",
        uuid::Uuid::new_v4(),
        chrono::Utc::now().timestamp_millis()
    );

    // Generate a random challenge
    let challenge = uuid::Uuid::new_v4().to_string();

    // Store the transaction
    state
        .services
        .qr_login_storage
        .create_qr_login(&transaction_id, &auth_user.user_id, None)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create QR transaction: {}", e)))?;

    // Return QR code data (in real implementation, would generate actual QR code)
    Ok(Json(json!({
        "transaction_id": transaction_id,
        "mode": "login",
        "challenge": challenge,
        "expires_in": 300,  // 5 minutes
    })))
}

/// Confirm QR code login
/// User scans QR code on another device to confirm login
/// POST /_matrix/client/v1/login/qr/confirm
pub async fn confirm_qr_login(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let transaction_id = body
        .get("transaction_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Transaction ID required".to_string()))?;

    // Verify the transaction exists and is valid
    let transaction = state
        .services
        .qr_login_storage
        .get_qr_transaction(transaction_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get QR transaction: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Transaction not found".to_string()))?;

    // Check if expired
    let now = chrono::Utc::now().timestamp_millis();
    if now > transaction.expires_at {
        state
            .services
            .qr_login_storage
            .update_qr_status(transaction_id, "expired")
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update status: {}", e)))?;

        return Err(ApiError::bad_request("QR code expired".to_string()));
    }

    // Verify the user confirming matches the transaction user
    if transaction.user_id != auth_user.user_id {
        return Err(ApiError::forbidden(
            "Transaction does not match authenticated user".to_string(),
        ));
    }

    // Update status to confirmed
    state
        .services
        .qr_login_storage
        .update_qr_status(transaction_id, "confirmed")
        .await
        .map_err(|e| ApiError::internal(format!("Failed to update status: {}", e)))?;

    Ok(Json(json!({
        "transaction_id": transaction_id,
        "status": "confirmed"
    })))
}

/// Start QR login (for the scanning device)
/// POST /_matrix/client/v1/login/qr/start
pub async fn start_qr_login(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let transaction_id = body
        .get("transaction_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Transaction ID required".to_string()))?;

    let device_id = body.get("device_id").and_then(|v| v.as_str());
    let initial_display_name = body
        .get("initial_display_name")
        .and_then(|v| v.as_str());

    // Get transaction to verify it exists
    let transaction = state
        .services
        .qr_login_storage
        .get_qr_transaction(transaction_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get QR transaction: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Transaction not found".to_string()))?;

    // Check if expired
    let now = chrono::Utc::now().timestamp_millis();
    if now > transaction.expires_at {
        state
            .services
            .qr_login_storage
            .update_qr_status(transaction_id, "expired")
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update status: {}", e)))?;

        return Err(ApiError::bad_request("QR code expired".to_string()));
    }

    // Perform login for the target user
    // Note: In a full implementation, this would use a special QR login flow
    // For now, we return the transaction info for the client to complete
    Ok(Json(json!({
        "transaction_id": transaction_id,
        "user_id": transaction.user_id,
        "device_id": device_id,
        "initial_display_name": initial_display_name,
        "status": "pending_confirmation"
    })))
}

/// Fetch QR login status
/// GET /_matrix/client/v1/login/qr/{transaction_id}/status
pub async fn get_qr_status(
    State(state): State<AppState>,
    Path(transaction_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let transaction = state
        .services
        .qr_login_storage
        .get_qr_transaction(&transaction_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get QR transaction: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Transaction not found".to_string()))?;

    // Check if expired
    let now = chrono::Utc::now().timestamp_millis();
    let status = if now > transaction.expires_at && transaction.status == "pending" {
        "expired"
    } else {
        &transaction.status
    };

    Ok(Json(json!({
        "transaction_id": transaction.transaction_id,
        "user_id": transaction.user_id,
        "status": status,
    })))
}

/// Invalidate QR code login
/// Cancel a QR login transaction
/// POST /_matrix/client/v1/login/qr/invalidate
pub async fn invalidate_qr_login(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let transaction_id = body
        .get("transaction_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Transaction ID required".to_string()))?;

    // Get transaction to verify it exists
    let transaction = state
        .services
        .qr_login_storage
        .get_qr_transaction(transaction_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get QR transaction: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Transaction not found".to_string()))?;

    // Verify the user owns the transaction
    if transaction.user_id != auth_user.user_id {
        return Err(ApiError::forbidden(
            "Transaction does not match authenticated user".to_string(),
        ));
    }

    // Update status to invalidated
    state
        .services
        .qr_login_storage
        .update_qr_status(transaction_id, "invalidated")
        .await
        .map_err(|e| ApiError::internal(format!("Failed to update status: {}", e)))?;

    Ok(Json(json!({
        "transaction_id": transaction_id,
        "status": "invalidated"
    })))
}
