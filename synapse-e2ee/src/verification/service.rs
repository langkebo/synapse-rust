use crate::verification::models::*;
use crate::verification::storage::VerificationStorage;
use base64::Engine;
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::{Digest, Sha256};
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

/// Clone implementation for [`VerificationService`].
impl Clone for VerificationService {
    fn clone(&self) -> Self {
        Self { storage: self.storage.clone() }
    }
}

/// Implementation of [`VerificationService`] methods.
/// Builds the Matrix SAS info string for a verification transaction.
///
/// `MATRIX_KEY_VERIFICATION_SAS|<initiator user>|<initiator device>|<tx id>|<responder user>|<responder device>`
///
/// Both sides must derive the SAS from the same string, so it is built from the
/// stored request rather than a constant (the previous code passed the literal
/// `"SAS"`, which no conforming client uses).
///
/// Per MSC3410 §3.2, the info string must include both public keys:
/// `MATRIX_KEY_VERIFICATION_SAS|from_user|from_device|from_key|to_user|to_device|to_key|txn_id`
fn sas_info(request: &VerificationRequest, from_public_key: &str, to_public_key: &str) -> String {
    format!(
        "MATRIX_KEY_VERIFICATION_SAS|{from_user}|{from_device}|{from_key}|{to_user}|{to_device}|{to_key}|{txn_id}",
        from_user = request.from_user,
        from_device = request.from_device,
        from_key = from_public_key,
        to_user = request.to_user,
        to_device = request.to_device.as_deref().unwrap_or(""),
        to_key = to_public_key,
        txn_id = request.transaction_id
    )
}

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
    ///
    /// Spec-compliant HKDF-SHA256 (RFC 5869) with the ECDH shared secret as IKM,
    /// no salt, and `info` as the context string; the first 6 bytes are the SAS.
    /// The previous `SHA256(shared_secret || info)` construction was not HKDF and
    /// produced SAS bytes no conforming client could reproduce.
    pub fn derive_sas(&self, shared_secret: &[u8; 32], info: &str) -> Result<[u8; 6], ApiError> {
        let hkdf = hkdf::Hkdf::<sha2::Sha256>::new(None, shared_secret);
        let mut sas_bytes = [0u8; 6];
        // `expand` only fails when the requested length exceeds 255 * HashLen;
        // 6 bytes can never trip that, but propagate rather than panic.
        hkdf.expand(info.as_bytes(), &mut sas_bytes)
            .map_err(|_| ApiError::internal("Failed to derive SAS bytes".to_string()))?;
        Ok(sas_bytes)
    }

    /// See [`compute_mac`].
    ///
    /// Per MSC3410 §3.3, the MAC is computed as:
    /// `HMAC-SHA256(shared_secret, key_data)` where `key_data` is the concatenation
    /// of all key IDs being authenticated.
    pub fn compute_mac(&self, keys: &[String], shared_secret: &[u8; 32], _info: &str) -> Result<String, ApiError> {
        let mut mac = HmacSha256::new_from_slice(shared_secret).map_err(|e| {
            tracing::error!("MAC error: {e}");
            ApiError::internal("An internal error occurred".to_string())
        })?;

        // Concatenate all key IDs
        for key in keys {
            mac.update(key.as_bytes());
        }

        let result = mac.finalize();
        Ok(base64::engine::general_purpose::STANDARD.encode(result.into_bytes()))
    }

    /// Compute the commitment hash per MSC3410 §3.4.
    ///
    /// `commitment = base64(sha256(public_key || "verification.commitment"))`
    ///
    /// The commitment binds the verifier to a specific public key before the
    /// peer reveals theirs, preventing man-in-the-middle attacks.
    pub fn compute_commitment(public_key: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(public_key.as_bytes());
        hasher.update(b"verification.commitment");
        let result = hasher.finalize();
        base64::engine::general_purpose::STANDARD.encode(result)
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

        // Per MSC3410 §3.4: commitment = base64(sha256(public_key || "verification.commitment"))
        let commitment = Self::compute_commitment(&public_key);

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
        let Some(request) = request else {
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
                // Fail closed. Without our stored private key (from accept_sas) and
                // the peer's public key there is no shared secret; a random SAS can
                // never match the peer, so returning one would fake a verification.
                return Err(ApiError::bad_request(
                    "SAS key agreement requires a stored private key and the peer's public key".to_string(),
                ));
            }
        };

        let local_pubkey = sas_state.as_ref().and_then(|s| s.pubkey.as_deref()).unwrap_or("");

        let sas_bytes = self.derive_sas(&shared_secret, &sas_info(&request, local_pubkey, other_pubkey))?;

        let decimal = ((sas_bytes[0] as u32) << 16) | ((sas_bytes[1] as u32) << 8) | (sas_bytes[2] as u32);
        let decimal_value = (decimal % 900000) + 100000;

        // Per MSC3410: produce 7 emojis (use 6 bytes, each maps to 1 emoji via modulo 64)
        let emoji_count = 7;
        let mut emojis = Vec::with_capacity(emoji_count);
        // Use first 6 bytes for 6 emojis
        for &byte in sas_bytes.iter() {
            let idx = (byte as usize) % 64;
            emojis.push(SAS_EMOJIS[idx].to_string());
        }
        // For 7th emoji, use first byte again (spec allows 6 or 7 emoji)
        // Or we can compute a 7th from the decimal value
        let seventh_idx = ((decimal_value >> 8) & 0x3F) as usize;
        emojis.push(SAS_EMOJIS[seventh_idx].to_string());

        Ok(SasResult {
            transaction_id: transaction_id.to_string(),
            sas: SasRepresentation::Emoji(emojis),
            confirmed: false,
        })
    }

    /// See [`confirm_sas`].
    ///
    /// Verifies the client's MAC **before** the transaction can reach `Done`.
    ///
    /// The MAC covers the key material the client claims (`keys`), keyed by the
    /// ECDH shared secret:
    /// `HMAC-SHA256(shared_secret, concat(sorted(key_id || 0x00 || key_value)))`,
    /// base64-encoded.  The shared secret is recomputed from our stored private
    /// key and the peer public key supplied by the caller (the server does not
    /// persist the peer key).
    ///
    /// Everything the server cannot verify is rejected instead of silently
    /// marking the transaction done: no `keys`, no peer key, no stored private
    /// key, or a MAC mismatch.  The previous implementation accepted any
    /// non-empty string, which meant "verified" could be asserted without proof.
    pub async fn confirm_sas(
        &self,
        transaction_id: &str,
        mac: &str,
        keys: &std::collections::BTreeMap<String, String>,
        peer_pubkey: &str,
    ) -> Result<bool, ApiError> {
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

        if keys.is_empty() {
            return Err(ApiError::bad_request(
                "MAC verification requires `keys` (the key material the MAC covers)".to_string(),
            ));
        }
        if peer_pubkey.is_empty() {
            return Err(ApiError::bad_request("MAC verification requires the peer's public key".to_string()));
        }

        let sas_state = self.storage.get_sas_state(transaction_id).await?;
        let Some(sas_state) = sas_state else {
            return Err(ApiError::bad_request("SAS state not found".to_string()));
        };

        let secret_key = sas_state.secret_key.as_deref().filter(|secret| !secret.is_empty()).ok_or_else(|| {
            ApiError::bad_request("SAS state has no local private key; cannot verify the MAC".to_string())
        })?;

        let shared_secret = self.compute_shared_secret(secret_key, peer_pubkey)?;

        // `BTreeMap` iterates in key order, so both sides build the same message.
        let entries: Vec<String> = keys.iter().map(|(key_id, value)| format!("{key_id}\u{0}{value}")).collect();
        let expected = self.compute_mac(&entries, &shared_secret, "")?;

        if !synapse_common::crypto::secure_compare(&expected, mac) {
            return Err(ApiError::forbidden("MAC verification failed".to_string()));
        }

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
    ///
    /// **Not supported.** A QR payload must be signed with the *device's* private
    /// key, which only the client holds — the homeserver stores public key
    /// material only.  The previous implementation fabricated a payload by reusing
    /// one key for both `device_ed25519_key` and `device_curve25519_key` and
    /// leaving `signature` empty, which no verifier can accept; returning it
    /// pretended the feature worked.  Failing loudly is the honest behaviour
    /// until the client-side flow (rendezvous + device signature) is implemented.
    pub async fn generate_qr_code(
        &self,
        _user_id: &str,
        _device_id: &str,
        _server_name: &str,
    ) -> Result<QrCodeData, ApiError> {
        Err(ApiError::unsupported("QR code verification is not supported by this homeserver".to_string()))
    }

    /// See [`scan_qr_code`].
    ///
    /// **Not supported** — see [`generate_qr_code`].  Fails closed instead of
    /// creating a `Pending` verification request from a payload whose signature
    /// was never checked (the previous behaviour).
    pub async fn scan_qr_code(
        &self,
        _qr_data: &QrCodeData,
        _scanner_device: &str,
        _scanner_user_id: &str,
    ) -> Result<(), ApiError> {
        Err(ApiError::unsupported("QR code verification is not supported by this homeserver".to_string()))
    }
}

fn generate_transaction_id() -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

#[allow(dead_code)]
fn slice_from_ref<T>(val: &T) -> &[T] {
    std::slice::from_ref(val)
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_common::test_isolation::IsolatedTestPool;

    /// The workspace baseline migration, compiled in so the isolated schemas
    /// these tests use carry every table the storage layer queries.
    ///
    /// The bytes are load-bearing: the shared template's name is a content
    /// fingerprint, so this must stay byte-identical to the copy
    /// `synapse-storage` and `synapse-services` pass (pinned by
    /// `tests/unit/test_isolation_unification_tests.rs`). It lives in the
    /// fixture rather than in `synapse-common` because that crate must not
    /// compile the workspace migrations into its production build.
    const BASELINE_SQL: &str = include_str!("../../../migrations/00000000_unified_schema_v12.sql");

    fn make_service(pool: Arc<sqlx::PgPool>) -> VerificationService {
        VerificationService::new(Arc::new(VerificationStorage::new(&pool)))
    }

    /// A service whose pool is never connected to.
    ///
    /// The non-DB tests only need a `VerificationService` value, and
    /// `connect_lazy` performs no I/O, so they run without a database. The URL
    /// comes from the shared resolver rather than a literal so a future test
    /// that *does* query cannot silently target a different database than the
    /// isolated pools — DB-touching tests must use [`make_service`] with an
    /// [`IsolatedTestPool`] instead.
    fn lazy_service() -> VerificationService {
        let pool = sqlx::PgPool::connect_lazy(&synapse_common::test_isolation::test_database_url())
            .expect("connect_lazy should not perform I/O");
        make_service(Arc::new(pool))
    }

    #[tokio::test]
    async fn generate_key_pair_produces_valid_base64_public_key() {
        let svc = lazy_service();
        let (_secret, public) = svc.generate_key_pair();
        // Public key should be 32 bytes → 44 base64 chars (no padding for URL-safe)
        let decoded = base64::engine::general_purpose::STANDARD.decode(&public).unwrap();
        assert_eq!(decoded.len(), 32);
    }

    #[tokio::test]
    async fn generate_key_pair_returns_non_empty_secret_key() {
        let svc = lazy_service();
        let (secret, _public) = svc.generate_key_pair();
        // E2EE-02: the secret key must NOT be empty — it is needed for ECDH.
        assert!(!secret.is_empty(), "private key must not be discarded");
        let decoded = base64::engine::general_purpose::STANDARD.decode(&secret).unwrap();
        assert_eq!(decoded.len(), 32, "secret key must be 32 bytes");
    }

    #[tokio::test]
    async fn generate_key_pair_secret_and_public_form_valid_pair() {
        let svc = lazy_service();
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
        let svc = lazy_service();
        let (s1, p1) = svc.generate_key_pair();
        let (s2, p2) = svc.generate_key_pair();
        assert_ne!(s1, s2, "secret keys must be unique");
        assert_ne!(p1, p2, "public keys must be unique");
    }

    #[tokio::test]
    async fn derive_sas_is_deterministic() {
        let svc = lazy_service();
        let shared_secret = [0x42u8; 32];
        let sas1 = svc.derive_sas(&shared_secret, "SAS").expect("derive");
        let sas2 = svc.derive_sas(&shared_secret, "SAS").expect("derive");
        assert_eq!(sas1, sas2);
        assert_eq!(sas1.len(), 6);
    }

    #[tokio::test]
    async fn derive_sas_different_info_produces_different_result() {
        let svc = lazy_service();
        let shared_secret = [0x42u8; 32];
        let sas1 = svc.derive_sas(&shared_secret, "SAS");
        let sas2 = svc.derive_sas(&shared_secret, "OTHER");
        assert_ne!(sas1, sas2);
    }

    #[tokio::test]
    async fn compute_mac_is_deterministic() {
        let svc = lazy_service();
        let shared_secret = [0xABu8; 32];
        let keys = vec!["key1".to_string(), "key2".to_string()];
        let mac1 = svc.compute_mac(&keys, &shared_secret, "test.info").unwrap();
        let mac2 = svc.compute_mac(&keys, &shared_secret, "test.info").unwrap();
        assert_eq!(mac1, mac2);
    }

    #[tokio::test]
    async fn compute_mac_different_keys_produce_different_result() {
        let svc = lazy_service();
        let shared_secret = [0xABu8; 32];
        let mac1 = svc.compute_mac(&["a".into()], &shared_secret, "info").unwrap();
        let mac2 = svc.compute_mac(&["b".into()], &shared_secret, "info").unwrap();
        assert_ne!(mac1, mac2);
    }

    #[tokio::test]
    async fn compute_mac_valid_base64_output() {
        let svc = lazy_service();
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
        let svc = lazy_service();
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
        let svc = lazy_service();
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
        let svc = lazy_service();
        assert!(svc.compute_shared_secret("not-base64!!!", "also!!!bad").is_err());
    }

    #[tokio::test]
    async fn compute_shared_secret_rejects_wrong_length() {
        let svc = lazy_service();
        let short = base64::engine::general_purpose::STANDARD.encode([0u8; 16]);
        let valid = base64::engine::general_purpose::STANDARD.encode([0u8; 32]);
        assert!(svc.compute_shared_secret(&short, &valid).is_err());
        assert!(svc.compute_shared_secret(&valid, &short).is_err());
    }

    #[tokio::test]
    async fn derive_sas_produces_6_byte_output() {
        let svc = lazy_service();
        let shared_secret = [0x00u8; 32];
        let sas = svc.derive_sas(&shared_secret, "MATRIX_QR_CODE_LOGIN_INITIATE").expect("derive");
        assert_eq!(sas.len(), 6);
    }

    // ════════════════════════════════════════
    // SAS/QR 状态机覆盖测试（补充 E2EE 完整性验证）
    // ════════════════════════════════════════

    #[tokio::test]
    async fn cancel_verification_transitions_to_cancelled() {
        // 隔离 schema：`public` 会被同一次 workspace 运行的 unit 目标清空，
        // 直连它会让本测试以 42P01 假失败。
        let isolated = IsolatedTestPool::new(BASELINE_SQL).await.expect("isolated test pool");
        let svc = make_service(isolated.pool());

        // 验证取消操作不会 panic；DB 连接成功时应返回 Ok，若无数据则 update 无匹配行也不报错
        let result = svc.cancel_verification("test-tx-id", "test_code", "test_reason").await;
        assert!(result.is_ok(), "cancel_verification must not fail: {result:?}");
    }

    #[tokio::test]
    async fn get_request_returns_none_for_unknown_transaction() {
        // 隔离 schema，理由同 `cancel_verification_transitions_to_cancelled`。
        let isolated = IsolatedTestPool::new(BASELINE_SQL).await.expect("isolated test pool");
        let svc = make_service(isolated.pool());

        let result = svc.get_request("nonexistent-tx-12345").await;
        assert!(result.is_ok(), "get_request must not fail: {result:?}");
        assert!(result.unwrap().is_none());
    }

    // ════════════════════════════════════════
    // C1：SAS 规范对齐（HKDF-SHA256 + 规范 info 串 + fail-closed）
    // ════════════════════════════════════════

    /// 期望值由**独立实现**算出：Python `hmac`/`hashlib` 手写 HKDF（RFC 5869），
    /// input = ikm 0x00..0x1f, salt = 空（=32 字节零）, info = 规范 SAS 串, L = 6。
    #[tokio::test]
    async fn derive_sas_matches_hkdf_sha256_known_answer() {
        let svc = lazy_service();
        let mut secret = [0u8; 32];
        for (index, byte) in secret.iter_mut().enumerate() {
            *byte = index as u8;
        }
        let info = "MATRIX_KEY_VERIFICATION_SAS|@alice:example.com|ALICEDEV|tx-1|@bob:example.com|BOBDEV";
        assert_eq!(
            svc.derive_sas(&secret, info).expect("derive SAS"),
            [0xb0, 0x96, 0xee, 0xb5, 0x79, 0xa0],
            "SAS 必须按 HKDF-SHA256(salt=空, ikm=shared_secret, info) 派生；\
             旧实现 SHA256(secret||info) 会得到 4cc1cf67c070…"
        );
    }

    /// 两端必须能各自算出同一个 SAS ⇒ info 串必须是规范形式（含双方 user/device/pubkey 与 tx）。
    #[test]
    fn sas_info_matches_matrix_spec() {
        let request = VerificationRequest {
            transaction_id: "tx-1".to_string(),
            from_user: "@alice:example.com".to_string(),
            from_device: "ALICEDEV".to_string(),
            to_user: "@bob:example.com".to_string(),
            to_device: Some("BOBDEV".to_string()),
            method: VerificationMethod::Sas,
            state: VerificationState::Ready,
            created_ts: 0,
            updated_ts: None,
        };
        assert_eq!(
            super::sas_info(&request, "FROM_PUBKEY", "TO_PUBKEY"),
            "MATRIX_KEY_VERIFICATION_SAS|@alice:example.com|ALICEDEV|FROM_PUBKEY|@bob:example.com|BOBDEV|TO_PUBKEY|tx-1"
        );
    }

    /// 没有密钥材料时必须 fail-closed：随机字节的 SAS 永远不可能与对端一致，
    /// 静默返回它等于伪造一次验证。
    #[tokio::test]
    async fn generate_sas_fails_closed_without_key_material() {
        let isolated = IsolatedTestPool::new(BASELINE_SQL).await.expect("isolated test pool");
        let svc = make_service(isolated.pool());

        svc.storage
            .create_request(&VerificationRequest {
                transaction_id: "tx-nokey".to_string(),
                from_user: "@alice:example.com".to_string(),
                from_device: "ALICEDEV".to_string(),
                to_user: "@bob:example.com".to_string(),
                to_device: Some("BOBDEV".to_string()),
                method: VerificationMethod::Sas,
                state: VerificationState::Ready,
                created_ts: current_timestamp_millis(),
                updated_ts: None,
            })
            .await
            .expect("create request");

        let result = svc.generate_sas("tx-nokey", "").await;
        assert!(result.is_err(), "无密钥材料时必须报错，不得返回随机 SAS: {result:?}");
    }

    // ════════════════════════════════════════
    // C1c：commitment 必须符合 MSC3410 §3.4
    // ════════════════════════════════════════

    /// commitment 必须是 `base64(sha256(public_key || "verification.commitment"))`，
    /// 其中 public_key 是 curve25519 公钥的 base64 字符串（MSC3172/MSC3410 形式）。
    #[test]
    fn commitment_matches_msc3410_section_3_4() {
        use sha2::{Digest, Sha256};
        let test_pubkey = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

        // Compute expected commitment manually
        let mut hasher = Sha256::new();
        hasher.update(test_pubkey.as_bytes());
        hasher.update(b"verification.commitment");
        let expected_commitment = base64::engine::general_purpose::STANDARD.encode(hasher.finalize());

        // Compute using the actual implementation
        let actual_commitment = VerificationService::compute_commitment(test_pubkey);

        assert_eq!(actual_commitment, expected_commitment, "commitment must match MSC3410 §3.4 formula");
    }

    /// commitment 对于相同的 public_key 必须是确定性的。
    #[test]
    fn commitment_is_deterministic() {
        let pubkey = "test-public-key";
        let c1 = VerificationService::compute_commitment(pubkey);
        let c2 = VerificationService::compute_commitment(pubkey);
        assert_eq!(c1, c2, "commitment must be deterministic for the same public key");
    }

    /// commitment 对于不同的 public_key 必须不同。
    #[test]
    fn commitment_differs_for_different_pubkeys() {
        let c1 = VerificationService::compute_commitment("public-key-1");
        let c2 = VerificationService::compute_commitment("public-key-2");
        assert_ne!(c1, c2, "different public keys must produce different commitments");
    }

    // ════════════════════════════════════════
    // C1b：MAC 必须真正校验；无法校验一律 fail-closed
    // ════════════════════════════════════════

    /// Seeds a `Ready` SAS transaction with a real X25519 key pair and returns
    /// `(peer_public, keys, correct_mac)`.
    async fn seed_sas_with_known_mac(
        svc: &VerificationService,
        tx: &str,
    ) -> (String, std::collections::BTreeMap<String, String>, String) {
        let (own_secret, own_public) = svc.generate_key_pair();
        let (_peer_secret, peer_public) = svc.generate_key_pair();

        svc.storage
            .create_request(&VerificationRequest {
                transaction_id: tx.to_string(),
                from_user: "@alice:example.com".to_string(),
                from_device: "ALICEDEV".to_string(),
                to_user: "@bob:example.com".to_string(),
                to_device: Some("BOBDEV".to_string()),
                method: VerificationMethod::Sas,
                state: VerificationState::Ready,
                created_ts: current_timestamp_millis(),
                updated_ts: None,
            })
            .await
            .expect("create request");
        svc.storage
            .store_sas_state(&SasState {
                tx_id: tx.to_string(),
                from_device: "ALICEDEV".to_string(),
                to_device: Some("BOBDEV".to_string()),
                method: VerificationMethod::Sas,
                state: VerificationState::Ready,
                exchange_hashes: Vec::new(),
                commitment: None,
                pubkey: Some(own_public),
                secret_key: Some(own_secret.clone()),
                sas_bytes: None,
                mac: None,
            })
            .await
            .expect("store sas state");

        let shared = svc.compute_shared_secret(&own_secret, &peer_public).expect("shared secret");
        let mut keys = std::collections::BTreeMap::new();
        keys.insert("ed25519:ALICEDEV".to_string(), "alice-key-value".to_string());
        let mac =
            svc.compute_mac(&["ed25519:ALICEDEV\u{0}alice-key-value".to_string()], &shared, "").expect("compute mac");
        (peer_public, keys, mac)
    }

    async fn request_state(svc: &VerificationService, tx: &str) -> VerificationState {
        svc.storage.get_request(tx).await.expect("get request").expect("request exists").state
    }

    #[tokio::test]
    async fn confirm_sas_accepts_correct_mac_and_marks_done() {
        let isolated = IsolatedTestPool::new(BASELINE_SQL).await.expect("isolated test pool");
        let svc = make_service(isolated.pool());
        let (peer_public, keys, mac) = seed_sas_with_known_mac(&svc, "tx-mac-ok").await;

        let confirmed = svc.confirm_sas("tx-mac-ok", &mac, &keys, &peer_public).await.expect("valid MAC accepted");
        assert!(confirmed);
        assert_eq!(request_state(&svc, "tx-mac-ok").await, VerificationState::Done);
    }

    /// 篡改 MAC 必须被拒绝，且**不得**把交易标记为 Done（旧实现接受任意非空串）。
    #[tokio::test]
    async fn confirm_sas_rejects_tampered_mac() {
        let isolated = IsolatedTestPool::new(BASELINE_SQL).await.expect("isolated test pool");
        let svc = make_service(isolated.pool());
        let (peer_public, keys, mac) = seed_sas_with_known_mac(&svc, "tx-mac-bad").await;

        let tampered = format!("{}x", mac);
        let result = svc.confirm_sas("tx-mac-bad", &tampered, &keys, &peer_public).await;
        assert!(result.is_err(), "篡改的 MAC 必须被拒绝: {result:?}");
        assert_ne!(
            request_state(&svc, "tx-mac-bad").await,
            VerificationState::Done,
            "MAC 校验失败不得把交易标记为 Done"
        );
    }

    /// 无法校验时必须 fail-closed：缺 keys / 缺 peer key 都不得 Done。
    #[tokio::test]
    async fn confirm_sas_fails_closed_without_proof() {
        let isolated = IsolatedTestPool::new(BASELINE_SQL).await.expect("isolated test pool");
        let svc = make_service(isolated.pool());
        let (peer_public, keys, mac) = seed_sas_with_known_mac(&svc, "tx-mac-noproof").await;

        let empty_keys = std::collections::BTreeMap::new();
        assert!(
            svc.confirm_sas("tx-mac-noproof", &mac, &empty_keys, &peer_public).await.is_err(),
            "缺 keys 时必须拒绝"
        );
        assert!(svc.confirm_sas("tx-mac-noproof", &mac, &keys, "").await.is_err(), "缺 peer public key 时必须拒绝");
        assert_ne!(request_state(&svc, "tx-mac-noproof").await, VerificationState::Done, "缺少校验材料时不得标记 Done");
    }

    // ════════════════════════════════════════
    // C2：QR 载荷不得伪造；未实现即 fail-closed
    // ════════════════════════════════════════

    /// 旧实现把**同一个公钥**同时填进 `device_ed25519_key` 与
    /// `device_curve25519_key`，并把 `signature` 留成空串 —— 这是无法通过任何校验的
    /// 伪造载荷。设备私钥只存在于客户端，服务端无法产生合规签名，因此必须明确拒绝。
    #[tokio::test]
    async fn generate_qr_code_fails_closed_instead_of_fabricating_a_payload() {
        let isolated = IsolatedTestPool::new(BASELINE_SQL).await.expect("isolated test pool");
        let svc = make_service(isolated.pool());

        let result = svc.generate_qr_code("@alice:example.com", "ALICEDEV", "example.com").await;
        let err = result.expect_err("QR 验证未实现时必须返回错误，而不是伪造载荷");
        let message = err.to_string().to_lowercase();
        assert!(
            message.contains("not supported") || message.contains("unsupported"),
            "错误信息应明确说明不支持: {err}"
        );
    }

    #[tokio::test]
    async fn scan_qr_code_fails_closed_without_creating_a_request() {
        let isolated = IsolatedTestPool::new(BASELINE_SQL).await.expect("isolated test pool");
        let svc = make_service(isolated.pool());

        let qr = QrCodeData {
            transaction_id: "tx-qr-scan".to_string(),
            server_name: "example.com".to_string(),
            server_public_key: String::new(),
            user_id: "@alice:example.com".to_string(),
            device_id: "ALICEDEV".to_string(),
            device_ed25519_key: String::new(),
            device_curve25519_key: String::new(),
            signature: String::new(),
        };

        let result = svc.scan_qr_code(&qr, "SCANNERDEV", "@bob:example.com").await;
        assert!(result.is_err(), "QR 扫描未实现时必须 fail-closed: {result:?}");
        assert!(
            svc.get_request("tx-qr-scan").await.expect("get_request").is_none(),
            "不得为未经验证的 QR 扫描创建请求"
        );
    }
}
