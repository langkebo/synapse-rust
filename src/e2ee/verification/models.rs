// Verification models

use serde::{Deserialize, Serialize};

/// Verification methods
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum VerificationMethod {
    #[default]
    Sas,
    Qr,
    Emoji,
    Decimal,
}

/// Verification status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VerificationState {
    Requested,
    Ready,
    Pending,
    Done,
    Cancelled,
}

/// SAS verification state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SasState {
    pub tx_id: String,
    pub from_device: String,
    pub to_device: Option<String>,
    pub method: VerificationMethod,
    pub state: VerificationState,
    pub exchange_hashes: Vec<String>,
    pub commitment: Option<String>,
    pub pubkey: Option<String>,
    pub sas_bytes: Option<Vec<u8>>,
    pub mac: Option<String>,
}

/// QR code verification state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QrState {
    pub tx_id: String,
    pub from_device: String,
    pub to_device: Option<String>,
    pub state: VerificationState,
    pub qr_code_data: Option<String>,
    pub scanned_data: Option<String>,
}

/// Verification request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationRequest {
    pub transaction_id: String,
    pub from_user: String,
    pub from_device: String,
    pub to_user: String,
    pub to_device: Option<String>,
    pub method: VerificationMethod,
    pub state: VerificationState,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
}

/// SAS verification data for API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SasData {
    pub transaction_id: String,
    pub method: String,
    pub key_agreement_protocol: Vec<String>,
    pub hash: Vec<String>,
    pub short_authentication_string: Vec<String>,
    pub commitment: Option<String>,
}

/// QR code data for display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QrCodeData {
    pub transaction_id: String,
    pub server_name: String,
    pub server_public_key: String,
    pub user_id: String,
    pub device_id: String,
    pub device_ed25519_key: String,
    pub device_curve25519_key: String,
    pub signature: String,
}

/// SAS emoji/decimal representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SasResult {
    pub transaction_id: String,
    pub sas: SasRepresentation,
    pub confirmed: bool,
}

/// SAS can be represented as emoji or decimal
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SasRepresentation {
    Emoji(Vec<String>),
    Decimal(u32),
}

/// Key verification complete event content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyVerificationComplete {
    pub transaction_id: String,
    pub from_device: String,
}

/// Key verification cancel event content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyVerificationCancel {
    pub transaction_id: String,
    pub code: String,
    pub reason: String,
}
