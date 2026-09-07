// Verification models

use serde::{Deserialize, Serialize};

/// Verification methods
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum VerificationMethod {
    /// The `Sas` variant.
    #[default]
    Sas,
    /// The `Qr` variant.
    /// The `Emoji` variant.
    /// The `Decimal` variant.
    Qr,
    /// The `Emoji` variant.
    /// The `Decimal` variant.
    Emoji,
    /// The `Decimal` variant.
    Decimal,
}

/// Verification status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VerificationState {
    /// The `Requested` variant.
    /// The `Ready` variant.
    /// The `Pending` variant.
    /// The `Done` variant.
    /// The `Cancelled` variant.
    Requested,
    /// The `Ready` variant.
    /// The `Pending` variant.
    /// The `Done` variant.
    /// The `Cancelled` variant.
    Ready,
    /// The `Pending` variant.
    /// The `Done` variant.
    /// The `Cancelled` variant.
    Pending,
    /// The `Done` variant.
    /// The `Cancelled` variant.
    Done,
    /// The `Cancelled` variant.
    Cancelled,
}

/// SAS verification state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SasState {
    /// The `tx_id` field.
    /// The `from_device` field.
    /// The `to_device` field.
    /// The `method` field.
    /// The `state` field.
    /// The `exchange_hashes` field.
    /// The `commitment` field.
    /// The `pubkey` field.
    pub tx_id: String,
    /// The `from_device` field.
    /// The `to_device` field.
    /// The `method` field.
    /// The `state` field.
    pub from_device: String,
    /// The `to_device` field.
    /// The `method` field.
    pub to_device: Option<String>,
    /// The `method` field.
    pub method: VerificationMethod,
    /// The `commitment` field.
    /// The `pubkey` field.
    pub state: VerificationState,
    /// The `sas_bytes` field.
    /// The `commitment` field.
    /// The `mac` field.
    /// The `commitment` field.
    pub exchange_hashes: Vec<String>,
    /// The `commitment` field.
    pub commitment: Option<String>,
    /// The `sas_bytes` field.
    /// The `mac` field.
    pub pubkey: Option<String>,
    /// E2EE-02: base64-encoded Curve25519 private key, stored so that
    /// `generate_sas` can compute the ECDH shared secret with the peer's
    /// The `sas_bytes` field.
    /// The `mac` field.
    /// public key.  This is transient verification state and is cleared
    /// The `sas_bytes` field.
    /// The `mac` field.
    /// when the transaction completes.
    /// The `sas_bytes` field.
    /// The `mac` field.
    pub secret_key: Option<String>,
    /// The `sas_bytes` field.
    /// The `mac` field.
    pub sas_bytes: Option<Vec<u8>>,
    /// The `mac` field.
    pub mac: Option<String>,
}

/// QR code verification state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QrState {
    /// The `tx_id` field.
    /// The `from_device` field.
    /// The `to_device` field.
    /// The `state` field.
    /// The `qr_code_data` field.
    /// The `scanned_data` field.
    pub tx_id: String,
    /// The `from_device` field.
    /// The `to_device` field.
    /// The `state` field.
    /// The `qr_code_data` field.
    /// The `scanned_data` field.
    pub from_device: String,
    /// The `to_device` field.
    /// The `state` field.
    /// The `qr_code_data` field.
    /// The `scanned_data` field.
    pub to_device: Option<String>,
    /// The `state` field.
    /// The `qr_code_data` field.
    /// The `scanned_data` field.
    pub state: VerificationState,
    /// The `qr_code_data` field.
    /// The `scanned_data` field.
    pub qr_code_data: Option<String>,
    /// The `scanned_data` field.
    pub scanned_data: Option<String>,
}

/// Verification request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationRequest {
    /// The `transaction_id` field.
    /// The `from_user` field.
    /// The `from_device` field.
    /// The `to_user` field.
    /// The `to_device` field.
    /// The `method` field.
    /// The `state` field.
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub transaction_id: String,
    /// The `from_user` field.
    /// The `from_device` field.
    /// The `to_user` field.
    /// The `to_device` field.
    /// The `method` field.
    /// The `state` field.
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub from_user: String,
    /// The `from_device` field.
    /// The `to_user` field.
    /// The `to_device` field.
    /// The `method` field.
    /// The `state` field.
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub from_device: String,
    /// The `to_user` field.
    /// The `to_device` field.
    /// The `method` field.
    /// The `state` field.
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub to_user: String,
    /// The `to_device` field.
    /// The `method` field.
    /// The `state` field.
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub to_device: Option<String>,
    /// The `method` field.
    /// The `state` field.
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub method: VerificationMethod,
    /// The `state` field.
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub state: VerificationState,
    /// The `created_ts` field.
    /// The `updated_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: Option<i64>,
}

/// SAS verification data for API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SasData {
    /// The `transaction_id` field.
    /// The `method` field.
    /// The `key_agreement_protocol` field.
    /// The `hash` field.
    /// The `short_authentication_string` field.
    /// The `commitment` field.
    pub transaction_id: String,
    /// The `method` field.
    /// The `key_agreement_protocol` field.
    /// The `hash` field.
    /// The `short_authentication_string` field.
    /// The `commitment` field.
    pub method: String,
    /// The `key_agreement_protocol` field.
    /// The `hash` field.
    /// The `short_authentication_string` field.
    /// The `commitment` field.
    pub key_agreement_protocol: Vec<String>,
    /// The `hash` field.
    /// The `short_authentication_string` field.
    /// The `commitment` field.
    pub hash: Vec<String>,
    /// The `short_authentication_string` field.
    /// The `commitment` field.
    pub short_authentication_string: Vec<String>,
    /// The `commitment` field.
    pub commitment: Option<String>,
}

/// QR code data for display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QrCodeData {
    /// The `transaction_id` field.
    /// The `server_name` field.
    /// The `server_public_key` field.
    /// The `user_id` field.
    /// The `device_id` field.
    /// The `device_ed25519_key` field.
    /// The `device_curve25519_key` field.
    /// The `signature` field.
    pub transaction_id: String,
    /// The `server_name` field.
    /// The `server_public_key` field.
    /// The `user_id` field.
    /// The `device_id` field.
    /// The `device_ed25519_key` field.
    /// The `device_curve25519_key` field.
    /// The `signature` field.
    pub server_name: String,
    /// The `server_public_key` field.
    /// The `user_id` field.
    /// The `device_id` field.
    /// The `device_ed25519_key` field.
    /// The `device_curve25519_key` field.
    /// The `signature` field.
    pub server_public_key: String,
    /// The `user_id` field.
    /// The `device_id` field.
    /// The `device_ed25519_key` field.
    /// The `device_curve25519_key` field.
    /// The `signature` field.
    pub user_id: String,
    /// The `device_id` field.
    /// The `device_ed25519_key` field.
    /// The `device_curve25519_key` field.
    /// The `signature` field.
    pub device_id: String,
    /// The `device_ed25519_key` field.
    /// The `device_curve25519_key` field.
    /// The `signature` field.
    pub device_ed25519_key: String,
    /// The `device_curve25519_key` field.
    /// The `signature` field.
    pub device_curve25519_key: String,
    /// The `signature` field.
    pub signature: String,
}

/// SAS emoji/decimal representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SasResult {
    /// The `transaction_id` field.
    /// The `sas` field.
    /// The `confirmed` field.
    pub transaction_id: String,
    /// The `sas` field.
    /// The `confirmed` field.
    pub sas: SasRepresentation,
    /// The `confirmed` field.
    pub confirmed: bool,
}

/// SAS can be represented as emoji or decimal
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SasRepresentation {
    /// The `Emoji` variant.
    /// The `Decimal` variant.
    Emoji(Vec<String>),
    /// The `Decimal` variant.
    Decimal(u32),
}

/// Key verification complete event content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyVerificationComplete {
    /// The `transaction_id` field.
    /// The `from_device` field.
    pub transaction_id: String,
    /// The `from_device` field.
    pub from_device: String,
}

/// Key verification cancel event content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyVerificationCancel {
    /// The `transaction_id` field.
    /// The `code` field.
    /// The `reason` field.
    pub transaction_id: String,
    /// The `code` field.
    /// The `reason` field.
    pub code: String,
    /// The `reason` field.
    pub reason: String,
}
