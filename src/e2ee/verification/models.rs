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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verification_method_default() {
        assert_eq!(VerificationMethod::default(), VerificationMethod::Sas);
    }

    #[test]
    fn test_verification_method_serialization() {
        let sas = VerificationMethod::Sas;
        assert_eq!(serde_json::to_string(&sas).unwrap(), r#""sas""#);

        let qr = VerificationMethod::Qr;
        assert_eq!(serde_json::to_string(&qr).unwrap(), r#""qr""#);

        let emoji = VerificationMethod::Emoji;
        assert_eq!(serde_json::to_string(&emoji).unwrap(), r#""emoji""#);

        let decimal = VerificationMethod::Decimal;
        assert_eq!(serde_json::to_string(&decimal).unwrap(), r#""decimal""#);
    }

    #[test]
    fn test_verification_state_serialization() {
        assert_eq!(serde_json::to_string(&VerificationState::Requested).unwrap(), r#""requested""#);
        assert_eq!(serde_json::to_string(&VerificationState::Ready).unwrap(), r#""ready""#);
        assert_eq!(serde_json::to_string(&VerificationState::Pending).unwrap(), r#""pending""#);
        assert_eq!(serde_json::to_string(&VerificationState::Done).unwrap(), r#""done""#);
        assert_eq!(serde_json::to_string(&VerificationState::Cancelled).unwrap(), r#""cancelled""#);
    }

    #[test]
    fn test_sas_state() {
        let state = SasState {
            tx_id: "tx_123".to_string(),
            from_device: "DEVICE1".to_string(),
            to_device: Some("DEVICE2".to_string()),
            method: VerificationMethod::Emoji,
            state: VerificationState::Pending,
            exchange_hashes: vec!["hash1".to_string()],
            commitment: Some("commitment".to_string()),
            pubkey: Some("pubkey".to_string()),
            sas_bytes: Some(vec![1, 2, 3]),
            mac: Some("mac_value".to_string()),
        };
        assert_eq!(state.tx_id, "tx_123");
        assert_eq!(state.method, VerificationMethod::Emoji);
        assert_eq!(state.state, VerificationState::Pending);
        assert!(state.sas_bytes.is_some());
    }

    #[test]
    fn test_qr_state() {
        let state = QrState {
            tx_id: "tx_456".to_string(),
            from_device: "DEVICE1".to_string(),
            to_device: None,
            state: VerificationState::Ready,
            qr_code_data: Some("qr_data".to_string()),
            scanned_data: None,
        };
        assert_eq!(state.tx_id, "tx_456");
        assert_eq!(state.state, VerificationState::Ready);
        assert!(state.qr_code_data.is_some());
        assert!(state.scanned_data.is_none());
    }

    #[test]
    fn test_verification_request() {
        let request = VerificationRequest {
            transaction_id: "tx_789".to_string(),
            from_user: "@alice:example.com".to_string(),
            from_device: "DEVICE1".to_string(),
            to_user: "@bob:example.com".to_string(),
            to_device: Some("DEVICE2".to_string()),
            method: VerificationMethod::Sas,
            state: VerificationState::Requested,
            created_ts: 1700000000000,
            updated_ts: None,
        };
        assert_eq!(request.transaction_id, "tx_789");
        assert_eq!(request.from_user, "@alice:example.com");
        assert_eq!(request.method, VerificationMethod::Sas);
    }

    #[test]
    fn test_sas_data() {
        let data = SasData {
            transaction_id: "tx_abc".to_string(),
            method: "m.sas.v1".to_string(),
            key_agreement_protocol: vec!["curve25519".to_string()],
            hash: vec!["sha256".to_string()],
            short_authentication_string: vec!["emoji".to_string()],
            commitment: Some("commitment_value".to_string()),
        };
        assert_eq!(data.method, "m.sas.v1");
        assert_eq!(data.key_agreement_protocol.len(), 1);
        assert!(data.commitment.is_some());
    }

    #[test]
    fn test_qr_code_data() {
        let data = QrCodeData {
            transaction_id: "tx_qr".to_string(),
            server_name: "example.com".to_string(),
            server_public_key: "server_key".to_string(),
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE1".to_string(),
            device_ed25519_key: "ed25519_key".to_string(),
            device_curve25519_key: "curve25519_key".to_string(),
            signature: "signature".to_string(),
        };
        assert_eq!(data.server_name, "example.com");
        assert_eq!(data.user_id, "@alice:example.com");
    }

    #[test]
    fn test_sas_result_emoji() {
        let result = SasResult {
            transaction_id: "tx_sas".to_string(),
            sas: SasRepresentation::Emoji(vec!["🐶".to_string(), "🐱".to_string()]),
            confirmed: true,
        };
        assert!(result.confirmed);
        match &result.sas {
            SasRepresentation::Emoji(emojis) => assert_eq!(emojis.len(), 2),
            _ => panic!("Expected Emoji representation"),
        }
    }

    #[test]
    fn test_sas_result_decimal() {
        let result = SasResult {
            transaction_id: "tx_dec".to_string(),
            sas: SasRepresentation::Decimal(12345),
            confirmed: false,
        };
        assert!(!result.confirmed);
        match result.sas {
            SasRepresentation::Decimal(n) => assert_eq!(n, 12345),
            _ => panic!("Expected Decimal representation"),
        }
    }

    #[test]
    fn test_key_verification_complete() {
        let complete = KeyVerificationComplete {
            transaction_id: "tx_comp".to_string(),
            from_device: "DEVICE1".to_string(),
        };
        assert_eq!(complete.transaction_id, "tx_comp");
    }

    #[test]
    fn test_key_verification_cancel() {
        let cancel = KeyVerificationCancel {
            transaction_id: "tx_cancel".to_string(),
            code: "m.user".to_string(),
            reason: "User cancelled".to_string(),
        };
        assert_eq!(cancel.code, "m.user");
        assert_eq!(cancel.reason, "User cancelled");
    }
}
