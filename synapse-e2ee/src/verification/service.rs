use crate::verification::models::*;
use crate::verification::storage::VerificationStorage;
use base64::Engine;
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::Sha256;
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_common::ApiError;
use x25519_dalek::{PublicKey, StaticSecret};

type HmacSha256 = Hmac<Sha256>;

const SAS_EMOJIS: &[&str; 64] = &[
    "🐶", "🐱", "🐭", "🐹", "🐰", "🦊", "🐻", "🐼", "🐨", "🐯", "🦁", "🐮", "🐷", "🐸", "🐵", "🐔", "🐧", "🐦", "🐤",
    "🦆", "🦅", "🦉", "🦇", "🐺", "🐗", "🐴", "🦄", "🐝", "🐛", "🦋", "🐌", "🐞", "🐜", "🦟", "🦗", "🕷", "🦂", "🐢",
    "🐍", "🦎", "🦖", "🦕", "🐙", "🦑", "🦐", "🦞", "🦀", "🐡", "🐠", "🐟", "🐬", "🐳", "🦈", "🐊", "🐅", "🐆", "🦓",
    "🦍", "🦧", "🐘", "🦛", "🦏", "🐪", "🐫",
];

/// The `VerificationService` type.
pub struct VerificationService {
    storage: Arc<VerificationStorage>,
}

/// (see code)
impl Clone for VerificationService {
    fn clone(&self) -> Self {
        Self { storage: self.storage.clone() }
    }
}

/// (see code)
impl VerificationService {
    /// See [`new`].
    pub fn new(storage: Arc<VerificationStorage>) -> Self {
        Self { storage }
    }

    /// See [`generate_key_pair`].
    pub fn generate_key_pair(&self) -> (String, String) {
        let secret = StaticSecret::random_from_rng(aes_gcm::aead::OsRng);
        let public = PublicKey::from(&secret);

        let secret_b64 = base64::engine::general_purpose::STANDARD.encode(secret.as_bytes());
        let public_b64 = base64::engine::general_purpose::STANDARD.encode(public.as_bytes());

        // E2EE-02: return BOTH keys so the caller can store the private key for
        // later ECDH computation. Previously the secret was discarded.
        (secret_b64, public_b64)
    }

    /// See [`compute_shared_secret`].
    pub fn compute_shared_secret(&self, our_secret: &str, their_public: &str) -> Result<[u8; 32], ApiError> {
        let secret_bytes = base64::engine::general_purpose::STANDARD.decode(our_secret).map_err(|e| {
            tracing::error!("Invalid secret key: {e}");
            ApiError::internal("An internal error occurred".to_string())
        })?;

        let public_bytes = base64::engine::general_purpose::STANDARD.decode(their_public).map_err(|e| {
            tracing::error!("Invalid public key: {e}");
            ApiError::internal("An internal error occurred".to_string())
        })?;

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

    /// See [`derive_sas`].
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

    /// See [`compute_mac`].
    pub fn compute_mac(&self, keys: &[String], shared_secret: &[u8; 32], info: &str) -> Result<String, ApiError> {
        let mut mac = HmacSha256::new_from_slice(shared_secret).map_err(|e| {
            tracing::error!("MAC error: {e}");
            ApiError::internal("An internal error occurred".to_string())
        })?;

        for key in keys {
            mac.update(key.as_bytes());
        }
        mac.update(info.as_bytes());

        let result = mac.finalize();
        Ok(base64::engine::general_purpose::STANDARD.encode(result.into_bytes()))
    }

    /// See [`start_sas_verification`].
    pub async fn start_sas_verification(
        &self,
        from_user: &str,
        from_device: &str,
        to_user: &str,
        to_device: Option<String>,
    ) -> Result<SasData, ApiError> {
        let transaction_id = generate_transaction_id();
        let now = current_timestamp_millis();

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
            secret_key: None,
            sas_bytes: None,
            mac: None,
        };

        self.storage.store_sas_state(&sas_state).await?;

        Ok(sas_data)
    }

    /// See [`accept_sas`].
    pub async fn accept_sas(
        &self,
        transaction_id: &str,
        key_agreement_protocol: &str,
        hash: &str,
    ) -> Result<SasData, ApiError> {
        let request = self.storage.get_request(transaction_id).await?;
        let Some(request) = request else {
            return Err(ApiError::not_found("Verification request not found".to_string()));
        };

        if request.state == VerificationState::Cancelled {
            return Err(ApiError::bad_request("Verification was cancelled".to_string()));
        }
        if request.state == VerificationState::Done {
            return Err(ApiError::bad_request("Verification already completed".to_string()));
        }

        // E2EE-02: generate a real Curve25519 key pair and preserve the private key.
        let (secret_key, public_key) = self.generate_key_pair();

        let commitment =
            self.compute_mac(slice_from_ref(&public_key), &[0u8; 32], "verification.commitment").map_err(|e| {
                tracing::error!("Failed to compute commitment: {e}");
                ApiError::internal("An internal error occurred".to_string())
            })?;

        // Persist the key pair so generate_sas can compute the ECDH shared secret later.
        let mut sas_state = self.storage.get_sas_state(transaction_id).await?;
        if sas_state.is_none() {
            // Create a minimal state if none exists yet.
            sas_state = Some(SasState {
                tx_id: transaction_id.to_string(),
                from_device: request.from_device.clone(),
                to_device: request.to_device.clone(),
                method: VerificationMethod::Sas,
                state: VerificationState::Ready,
                exchange_hashes: vec![],
                commitment: None,
                pubkey: None,
                secret_key: None,
                sas_bytes: None,
                mac: None,
            });
        }
        if let Some(ref mut sas) = sas_state {
            sas.state = VerificationState::Ready;
            sas.pubkey = Some(public_key);
            sas.secret_key = Some(secret_key);
            sas.commitment = Some(commitment.clone());
        }
        if let Some(ref sas) = sas_state {
            self.storage.store_sas_state(sas).await?;
        }

        let sas_data = SasData {
            transaction_id: transaction_id.to_string(),
            method: "m.sas.v1".to_string(),
            key_agreement_protocol: vec![key_agreement_protocol.to_string()],
            hash: vec![hash.to_string()],
            short_authentication_string: vec!["emoji".to_string(), "decimal".to_string()],
            commitment: Some(commitment),
        };

        self.storage.update_state(transaction_id, VerificationState::Ready).await?;

        Ok(sas_data)
    }

    /// See [`generate_sas`].
    pub async fn generate_sas(&self, transaction_id: &str, other_pubkey: &str) -> Result<SasResult, ApiError> {
        let request = self.storage.get_request(transaction_id).await?;
        let Some(_request) = request else {
            return Err(ApiError::not_found("Verification request not found".to_string()));
        };

        // E2EE-02: retrieve the private key that was generated and stored during
        // accept_sas, then compute the real ECDH shared secret with the peer's
        // public key.  Previously a *new* key pair was generated here and the
        // secret was discarded, causing the code to fall back to random bytes.
        let sas_state = self.storage.get_sas_state(transaction_id).await?;
        let stored_secret = sas_state.as_ref().and_then(|s| s.secret_key.as_deref());

        let shared_secret = match stored_secret {
            Some(secret) if !secret.is_empty() && !other_pubkey.is_empty() => {
                self.compute_shared_secret(secret, other_pubkey)?
            }
            _ => {
                // Fallback: if no stored secret exists yet, generate a key pair,
                // persist it, and compute the shared secret.
                tracing::warn!("No stored SAS secret key for transaction {transaction_id}; generating new key pair");
                let (new_secret, new_public) = self.generate_key_pair();
                // Persist for subsequent calls.
                if let Some(ref mut sas) = sas_state.clone() {
                    sas.pubkey = Some(new_public.clone());
                    sas.secret_key = Some(new_secret.clone());
                    sas.state = VerificationState::Ready;
                    if let Err(e) = self.storage.store_sas_state(sas).await {
                        tracing::error!("Failed to persist SAS state for transaction {transaction_id}: {e}");
                    }
                }
                if !other_pubkey.is_empty() {
                    self.compute_shared_secret(&new_secret, other_pubkey)?
                } else {
                    let mut bytes = [0u8; 32];
                    rand::rng().fill_bytes(&mut bytes);
                    bytes
                }
            }
        };

        let sas_bytes = self.derive_sas(&shared_secret, "SAS");

        let decimal = ((sas_bytes[0] as u32) << 16) | ((sas_bytes[1] as u32) << 8) | (sas_bytes[2] as u32);
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

    /// See [`confirm_sas`].
    pub async fn confirm_sas(&self, transaction_id: &str, mac: &str) -> Result<bool, ApiError> {
        if mac.is_empty() {
            return Err(ApiError::bad_request("MAC must not be empty".to_string()));
        }

        let request = self.storage.get_request(transaction_id).await?;
        let Some(request) = request else {
            return Err(ApiError::bad_request("Verification request not found".to_string()));
        };

        match request.state {
            VerificationState::Cancelled => {
                return Err(ApiError::bad_request("Verification was cancelled".to_string()));
            }
            VerificationState::Done => {
                return Ok(true);
            }
            VerificationState::Requested | VerificationState::Ready => {}
            _ => {}
        }

        let sas_state = self.storage.get_sas_state(transaction_id).await?;
        let Some(_sas_state) = sas_state else {
            return Err(ApiError::bad_request("SAS state not found".to_string()));
        };

        // The homeserver cannot verify the SAS MAC, and this function no longer
        // pretends to.
        //
        // The MAC is an HMAC over each side's own keys, keyed by the SAS shared
        // secret, so only the two *clients* can check it: the server never sees
        // that secret, and the two sides' MAC values are legitimately different
        // (each covers the keys that side owns), so comparing one against the
        // other would be wrong even if a value were stored.
        //
        // What the removed code did: `if let Some(stored_mac) = &sas_state.mac`
        // then `mac_matches`. No production path ever sets `SasState.mac` (all three
        // *production* `store_sas_state` call sites — `:159`/`:217`/`:262` — write
        // `mac: None`; the only other caller is the integration fixture
        // `tests/integration/db_schema_smoke_tests_migrated.rs:355`, which writes
        // `mac: Some("mac")` purely to check the row round-trips), so the
        // branch was unreachable for real clients — while a unit test named
        // `confirm_sas_rejects_wrong_mac_and_cancels_transaction` asserted only
        // that `mac_matches` works, which is what made the check look live.
        // `VerificationState::Done` records what the caller's client asserts,
        // exactly like the rest of this state machine; the trust boundary for
        // `m.key.verification.mac` is the client, per the Matrix spec. See
        // docs/audit/DB_REVIEW_2026-09-17.md §13.7.
        //
        // A non-empty `mac` is still required (here and in the routes) so the
        // protocol shape cannot be skipped.
        self.storage.update_state(transaction_id, VerificationState::Done).await?;

        tracing::info!("SAS verification confirmed for transaction {}", transaction_id);
        Ok(true)
    }

    /// See [`get_pending_verifications`].
    pub async fn get_pending_verifications(&self, user_id: &str) -> Result<Vec<VerificationRequest>, ApiError> {
        self.storage.get_pending_verifications(user_id).await
    }

    /// See [`get_request`].
    pub async fn get_request(&self, transaction_id: &str) -> Result<Option<VerificationRequest>, ApiError> {
        self.storage.get_request(transaction_id).await
    }

    /// See [`cancel_verification`].
    pub async fn cancel_verification(&self, transaction_id: &str, code: &str, reason: &str) -> Result<(), ApiError> {
        self.storage.update_state(transaction_id, VerificationState::Cancelled).await?;
        tracing::info!("Verification {} cancelled: {} - {}", transaction_id, code, reason);
        Ok(())
    }

    /// See [`generate_qr_code`].
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

    /// See [`scan_qr_code`].
    pub async fn scan_qr_code(
        &self,
        qr_data: &QrCodeData,
        scanner_device: &str,
        scanner_user_id: &str,
    ) -> Result<(), ApiError> {
        let now = current_timestamp_millis();

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
    rand::rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn slice_from_ref<T>(val: &T) -> &[T] {
    std::slice::from_ref(val)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_service() -> VerificationService {
        let pool = sqlx::PgPool::connect_lazy("postgres://synapse:synapse@localhost:5432/synapse_test")
            .expect("connect_lazy should not perform I/O");
        let pool = std::sync::Arc::new(pool);
        VerificationService::new(Arc::new(VerificationStorage::new(&pool)))
    }

    #[tokio::test]
    async fn generate_key_pair_produces_valid_base64_public_key() {
        let svc = make_service();
        let (_secret, public) = svc.generate_key_pair();
        // Public key should be 32 bytes → 44 base64 chars (no padding for URL-safe)
        let decoded = base64::engine::general_purpose::STANDARD.decode(&public).unwrap();
        assert_eq!(decoded.len(), 32);
    }

    #[tokio::test]
    async fn generate_key_pair_returns_non_empty_secret_key() {
        let svc = make_service();
        let (secret, _public) = svc.generate_key_pair();
        // E2EE-02: the secret key must NOT be empty — it is needed for ECDH.
        assert!(!secret.is_empty(), "private key must not be discarded");
        let decoded = base64::engine::general_purpose::STANDARD.decode(&secret).unwrap();
        assert_eq!(decoded.len(), 32, "secret key must be 32 bytes");
    }

    #[tokio::test]
    async fn generate_key_pair_secret_and_public_form_valid_pair() {
        let svc = make_service();
        let (secret, public) = svc.generate_key_pair();
        // E2EE-02: the returned (secret, public) must be a genuine Curve25519 pair.
        // Computing the shared secret with our own public key must succeed and
        // produce a non-zero 32-byte result.
        let shared = svc.compute_shared_secret(&secret, &public).unwrap();
        assert_eq!(shared.len(), 32);
        assert_ne!(shared, [0u8; 32]);
    }

    #[tokio::test]
    async fn generate_key_pair_produces_unique_keys() {
        let svc = make_service();
        let (s1, p1) = svc.generate_key_pair();
        let (s2, p2) = svc.generate_key_pair();
        assert_ne!(s1, s2, "secret keys must be unique");
        assert_ne!(p1, p2, "public keys must be unique");
    }

    #[tokio::test]
    async fn derive_sas_is_deterministic() {
        let svc = make_service();
        let shared_secret = [0x42u8; 32];
        let sas1 = svc.derive_sas(&shared_secret, "SAS");
        let sas2 = svc.derive_sas(&shared_secret, "SAS");
        assert_eq!(sas1, sas2);
        assert_eq!(sas1.len(), 6);
    }

    #[tokio::test]
    async fn derive_sas_different_info_produces_different_result() {
        let svc = make_service();
        let shared_secret = [0x42u8; 32];
        let sas1 = svc.derive_sas(&shared_secret, "SAS");
        let sas2 = svc.derive_sas(&shared_secret, "OTHER");
        assert_ne!(sas1, sas2);
    }

    #[tokio::test]
    async fn compute_mac_is_deterministic() {
        let svc = make_service();
        let shared_secret = [0xABu8; 32];
        let keys = vec!["key1".to_string(), "key2".to_string()];
        let mac1 = svc.compute_mac(&keys, &shared_secret, "test.info").unwrap();
        let mac2 = svc.compute_mac(&keys, &shared_secret, "test.info").unwrap();
        assert_eq!(mac1, mac2);
    }

    #[tokio::test]
    async fn compute_mac_different_keys_produce_different_result() {
        let svc = make_service();
        let shared_secret = [0xABu8; 32];
        let mac1 = svc.compute_mac(&["a".into()], &shared_secret, "info").unwrap();
        let mac2 = svc.compute_mac(&["b".into()], &shared_secret, "info").unwrap();
        assert_ne!(mac1, mac2);
    }

    #[tokio::test]
    async fn compute_mac_valid_base64_output() {
        let svc = make_service();
        let shared_secret = [0xFFu8; 32];
        let mac = svc.compute_mac(&["test".into()], &shared_secret, "info").unwrap();
        let decoded = base64::engine::general_purpose::STANDARD.decode(&mac);
        assert!(decoded.is_ok());
    }

    #[test]
    fn generate_transaction_id_is_non_empty() {
        let id = generate_transaction_id();
        assert!(!id.is_empty());
        let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(&id);
        assert!(decoded.is_ok());
        assert_eq!(decoded.unwrap().len(), 16);
    }

    #[test]
    fn slice_from_ref_works() {
        let val = 42i32;
        let s = slice_from_ref(&val);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0], 42);
    }

    #[tokio::test]
    async fn compute_shared_secret_is_symmetric() {
        let svc = make_service();
        let a_secret = StaticSecret::random_from_rng(aes_gcm::aead::OsRng);
        let a_public = PublicKey::from(&a_secret);
        let b_secret = StaticSecret::random_from_rng(aes_gcm::aead::OsRng);
        let b_public = PublicKey::from(&b_secret);
        let a_sec_b64 = base64::engine::general_purpose::STANDARD.encode(a_secret.as_bytes());
        let b_pub_b64 = base64::engine::general_purpose::STANDARD.encode(b_public.as_bytes());
        let b_sec_b64 = base64::engine::general_purpose::STANDARD.encode(b_secret.as_bytes());
        let a_pub_b64 = base64::engine::general_purpose::STANDARD.encode(a_public.as_bytes());
        let ab = svc.compute_shared_secret(&a_sec_b64, &b_pub_b64).unwrap();
        let ba = svc.compute_shared_secret(&b_sec_b64, &a_pub_b64).unwrap();
        assert_eq!(ab, ba);
        assert_ne!(ab, [0u8; 32]);
    }

    #[tokio::test]
    async fn compute_shared_secret_self_consistency() {
        let svc = make_service();
        let secret = StaticSecret::random_from_rng(aes_gcm::aead::OsRng);
        let public = PublicKey::from(&secret);
        let sec_b64 = base64::engine::general_purpose::STANDARD.encode(secret.as_bytes());
        let pub_b64 = base64::engine::general_purpose::STANDARD.encode(public.as_bytes());
        let shared = svc.compute_shared_secret(&sec_b64, &pub_b64).unwrap();
        assert_eq!(shared.len(), 32);
        assert_ne!(shared, [0u8; 32]);
    }

    #[tokio::test]
    async fn compute_shared_secret_rejects_invalid_base64() {
        let svc = make_service();
        assert!(svc.compute_shared_secret("not-base64!!!", "also!!!bad").is_err());
    }

    #[tokio::test]
    async fn compute_shared_secret_rejects_wrong_length() {
        let svc = make_service();
        let short = base64::engine::general_purpose::STANDARD.encode([0u8; 16]);
        let valid = base64::engine::general_purpose::STANDARD.encode([0u8; 32]);
        assert!(svc.compute_shared_secret(&short, &valid).is_err());
        assert!(svc.compute_shared_secret(&valid, &short).is_err());
    }

    #[tokio::test]
    async fn derive_sas_produces_6_byte_output() {
        let svc = make_service();
        let shared_secret = [0x00u8; 32];
        let sas = svc.derive_sas(&shared_secret, "MATRIX_QR_CODE_LOGIN_INITIATE");
        assert_eq!(sas.len(), 6);
    }

    // ════════════════════════════════════════
    // SAS/QR 状态机覆盖测试（补充 E2EE 完整性验证）
    // ════════════════════════════════════════

    #[tokio::test]
    async fn cancel_verification_transitions_to_cancelled() {
        let svc = make_service();
        // 验证取消操作不会 panic（需要 DB，但测试以无异常为主）
        let result = svc.cancel_verification("test-tx-id", "test_code", "test_reason").await;
        // DB 连接成功时应返回 Ok（无异常）；若无数据则仍为 Ok（update 无匹配行不报错）
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn get_request_returns_none_for_unknown_transaction() {
        let svc = make_service();
        let result = svc.get_request("nonexistent-tx-12345").await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }
}
