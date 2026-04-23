use crate::e2ee::verification::models::*;
use crate::e2ee::verification::storage::VerificationStorage;
use crate::error::ApiError;
use base64::Engine;
use hmac::{Hmac, Mac};
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::Sha256;
use std::sync::Arc;
use x25519_dalek::{PublicKey, StaticSecret};

type HmacSha256 = Hmac<Sha256>;

const SAS_EMOJIS: &[&str; 64] = &[
    "🐶", "🐱", "🐭", "🐹", "🐰", "🦊", "🐻", "🐼", "🐨", "🐯", "🦁", "🐮", "🐷", "🐸", "🐵", "🐔",
    "🐧", "🐦", "🐤", "🦆", "🦅", "🦉", "🦇", "🐺", "🐗", "🐴", "🦄", "🐝", "🐛", "🦋", "🐌", "🐞",
    "🐜", "🦟", "🦗", "🕷", "🦂", "🐢", "🐍", "🦎", "🦖", "🦕", "🐙", "🦑", "🦐", "🦞", "🦀", "🐡",
    "🐠", "🐟", "🐬", "🐳", "🦈", "🐊", "🐅", "🐆", "🦓", "🦍", "🦧", "🐘", "🦛", "🦏", "🐪", "🐫",
];

pub struct VerificationService {
    storage: Arc<VerificationStorage>,
}

impl Clone for VerificationService {
    fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
        }
    }
}

impl VerificationService {
    pub fn new(storage: Arc<VerificationStorage>) -> Self {
        Self { storage }
    }

    pub fn generate_key_pair(&self) -> (String, String) {
        let secret = StaticSecret::random_from_rng(OsRng);
        let public = PublicKey::from(&secret);

        let public_bytes = public.as_bytes();

        (
            String::new(),
            base64::engine::general_purpose::STANDARD.encode(public_bytes),
        )
    }

    pub fn compute_shared_secret(
        &self,
        our_secret: &str,
        their_public: &str,
    ) -> Result<[u8; 32], ApiError> {
        let secret_bytes = base64::engine::general_purpose::STANDARD
            .decode(our_secret)
            .map_err(|e| ApiError::internal(format!("Invalid secret key: {}", e)))?;

        let public_bytes = base64::engine::general_purpose::STANDARD
            .decode(their_public)
            .map_err(|e| ApiError::internal(format!("Invalid public key: {}", e)))?;

        if secret_bytes.len() != 32 || public_bytes.len() != 32 {
            return Err(ApiError::internal("Invalid key length".to_string()));
        }

        let mut secret_array = [0u8; 32];
        secret_array.copy_from_slice(&secret_bytes);
        let our_secret = StaticSecret::from(secret_array);

        let mut public_array = [0u8; 32];
        public_array.copy_from_slice(&public_bytes);
        let their_public = PublicKey::from(public_array);

        let shared_secret = our_secret.diffie_hellman(&their_public);
        Ok(*shared_secret.as_bytes())
    }

    pub fn derive_sas(&self, shared_secret: &[u8; 32], info: &str) -> [u8; 6] {
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();
        hasher.update(shared_secret);
        hasher.update(info.as_bytes());
        let result = hasher.finalize();

        let mut sas_bytes = [0u8; 6];
        sas_bytes.copy_from_slice(&result[..6]);
        sas_bytes
    }

    pub fn compute_mac(
        &self,
        keys: &[String],
        shared_secret: &[u8; 32],
        info: &str,
    ) -> Result<String, ApiError> {
        let mut mac = HmacSha256::new_from_slice(shared_secret)
            .map_err(|e| ApiError::internal(format!("MAC error: {}", e)))?;

        for key in keys {
            mac.update(key.as_bytes());
        }
        mac.update(info.as_bytes());

        let result = mac.finalize();
        Ok(base64::engine::general_purpose::STANDARD.encode(result.into_bytes()))
    }

    pub async fn start_sas_verification(
        &self,
        from_user: &str,
        from_device: &str,
        to_user: &str,
        to_device: Option<String>,
    ) -> Result<SasData, ApiError> {
        let transaction_id = generate_transaction_id();
        let now = chrono::Utc::now().timestamp_millis();

        let request = VerificationRequest {
            transaction_id: transaction_id.clone(),
            from_user: from_user.to_string(),
            from_device: from_device.to_string(),
            to_user: to_user.to_string(),
            to_device: to_device.clone(),
            method: VerificationMethod::Sas,
            state: VerificationState::Requested,
            created_ts: now,
            updated_ts: Some(now),
        };

        self.storage.create_request(&request).await?;

        let sas_data = SasData {
            transaction_id: transaction_id.clone(),
            method: "m.sas.v1".to_string(),
            key_agreement_protocol: vec!["curve25519".to_string()],
            hash: vec!["sha256".to_string()],
            short_authentication_string: vec!["emoji".to_string(), "decimal".to_string()],
            commitment: None,
        };

        let sas_state = SasState {
            tx_id: transaction_id,
            from_device: from_device.to_string(),
            to_device,
            method: VerificationMethod::Sas,
            state: VerificationState::Requested,
            exchange_hashes: vec![],
            commitment: None,
            pubkey: None,
            sas_bytes: None,
            mac: None,
        };

        self.storage.store_sas_state(&sas_state).await?;

        Ok(sas_data)
    }

    pub async fn accept_sas(
        &self,
        transaction_id: &str,
        key_agreement_protocol: &str,
        hash: &str,
    ) -> Result<SasData, ApiError> {
        let request = self.storage.get_request(transaction_id).await?;
        let Some(request) = request else {
            return Err(ApiError::not_found(
                "Verification request not found".to_string(),
            ));
        };

        if request.state == VerificationState::Cancelled {
            return Err(ApiError::bad_request(
                "Verification was cancelled".to_string(),
            ));
        }
        if request.state == VerificationState::Done {
            return Err(ApiError::bad_request(
                "Verification already completed".to_string(),
            ));
        }

        let (_secret_key, public_key) = self.generate_key_pair();

        let commitment = self
            .compute_mac(
                slice_from_ref(&public_key),
                &[0u8; 32],
                "verification.commitment",
            )
            .map_err(|e| ApiError::internal(format!("Failed to compute commitment: {}", e)))?;

        let sas_data = SasData {
            transaction_id: transaction_id.to_string(),
            method: "m.sas.v1".to_string(),
            key_agreement_protocol: vec![key_agreement_protocol.to_string()],
            hash: vec![hash.to_string()],
            short_authentication_string: vec!["emoji".to_string(), "decimal".to_string()],
            commitment: Some(commitment),
        };

        self.storage
            .update_state(transaction_id, VerificationState::Ready)
            .await?;

        Ok(sas_data)
    }

    pub async fn generate_sas(
        &self,
        transaction_id: &str,
        other_pubkey: &str,
    ) -> Result<SasResult, ApiError> {
        let request = self.storage.get_request(transaction_id).await?;
        let Some(_request) = request else {
            return Err(ApiError::not_found(
                "Verification request not found".to_string(),
            ));
        };

        let (our_secret, _our_public) = self.generate_key_pair();

        let shared_secret = if !other_pubkey.is_empty() && !our_secret.is_empty() {
            self.compute_shared_secret(&our_secret, other_pubkey)?
        } else {
            let mut rng = OsRng;
            let mut bytes = [0u8; 32];
            rng.fill_bytes(&mut bytes);
            bytes
        };

        let sas_bytes = self.derive_sas(&shared_secret, "SAS");

        let decimal =
            ((sas_bytes[0] as u32) << 16) | ((sas_bytes[1] as u32) << 8) | (sas_bytes[2] as u32);
        let _decimal = (decimal % 900000) + 100000;

        let emoji_count = 7;
        let mut emojis = Vec::with_capacity(emoji_count);
        for &byte in sas_bytes.iter() {
            let idx = (byte as usize) % 64;
            emojis.push(SAS_EMOJIS[idx].to_string());
        }

        Ok(SasResult {
            transaction_id: transaction_id.to_string(),
            sas: SasRepresentation::Emoji(emojis),
            confirmed: false,
        })
    }

    pub async fn confirm_sas(&self, transaction_id: &str, mac: &str) -> Result<bool, ApiError> {
        if mac.is_empty() {
            return Err(ApiError::bad_request("MAC must not be empty".to_string()));
        }

        let request = self.storage.get_request(transaction_id).await?;
        let Some(request) = request else {
            return Err(ApiError::bad_request(
                "Verification request not found".to_string(),
            ));
        };

        match request.state {
            VerificationState::Cancelled => {
                return Err(ApiError::bad_request(
                    "Verification was cancelled".to_string(),
                ));
            }
            VerificationState::Done => {
                return Ok(true);
            }
            VerificationState::Requested | VerificationState::Ready => {}
            _ => {}
        }

        let sas_state = self.storage.get_sas_state(transaction_id).await?;
        let Some(sas_state) = sas_state else {
            return Err(ApiError::bad_request("SAS state not found".to_string()));
        };

        if let Some(stored_mac) = &sas_state.mac {
            if mac != stored_mac {
                self.storage
                    .update_state(transaction_id, VerificationState::Cancelled)
                    .await?;
                tracing::warn!("SAS MAC mismatch for transaction {}", transaction_id);
                return Err(ApiError::bad_request("MAC verification failed".to_string()));
            }
        }

        self.storage
            .update_state(transaction_id, VerificationState::Done)
            .await?;

        tracing::info!(
            "SAS verification confirmed for transaction {}",
            transaction_id
        );
        Ok(true)
    }

    pub async fn get_pending_verifications(
        &self,
        user_id: &str,
    ) -> Result<Vec<VerificationRequest>, ApiError> {
        self.storage.get_pending_verifications(user_id).await
    }

    pub async fn get_request(
        &self,
        transaction_id: &str,
    ) -> Result<Option<VerificationRequest>, ApiError> {
        self.storage.get_request(transaction_id).await
    }

    pub async fn cancel_verification(
        &self,
        transaction_id: &str,
        code: &str,
        reason: &str,
    ) -> Result<(), ApiError> {
        self.storage
            .update_state(transaction_id, VerificationState::Cancelled)
            .await?;
        tracing::info!(
            "Verification {} cancelled: {} - {}",
            transaction_id,
            code,
            reason
        );
        Ok(())
    }

    pub async fn generate_qr_code(
        &self,
        user_id: &str,
        device_id: &str,
        server_name: &str,
    ) -> Result<QrCodeData, ApiError> {
        let transaction_id = generate_transaction_id();

        let (_secret_key, public_key) = self.generate_key_pair();

        let qr_data = QrCodeData {
            transaction_id: transaction_id.clone(),
            server_name: server_name.to_string(),
            server_public_key: public_key.clone(),
            user_id: user_id.to_string(),
            device_id: device_id.to_string(),
            device_ed25519_key: public_key.clone(),
            device_curve25519_key: public_key,
            signature: String::new(),
        };

        let qr_state = QrState {
            tx_id: transaction_id,
            from_device: device_id.to_string(),
            to_device: None,
            state: VerificationState::Ready,
            qr_code_data: Some(serde_json::to_string(&qr_data).unwrap_or_default()),
            scanned_data: None,
        };

        self.storage.store_qr_state(&qr_state).await?;

        Ok(qr_data)
    }

    pub async fn scan_qr_code(
        &self,
        qr_data: &QrCodeData,
        scanner_device: &str,
        scanner_user_id: &str,
    ) -> Result<(), ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        let request = VerificationRequest {
            transaction_id: qr_data.transaction_id.clone(),
            from_user: qr_data.user_id.clone(),
            from_device: qr_data.device_id.clone(),
            to_user: scanner_user_id.to_string(),
            to_device: Some(scanner_device.to_string()),
            method: VerificationMethod::Qr,
            state: VerificationState::Pending,
            created_ts: now,
            updated_ts: Some(now),
        };

        self.storage.create_request(&request).await?;

        Ok(())
    }
}

fn generate_transaction_id() -> String {
    let mut bytes = [0u8; 16];
    OsRng.fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn slice_from_ref<T>(val: &T) -> &[T] {
    std::slice::from_ref(val)
}
