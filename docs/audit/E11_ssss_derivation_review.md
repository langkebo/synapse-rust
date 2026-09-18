# E-11: SSSS PBKDF2 Derivation Audit

## Audit Date
2026-09-19

## Scope
Review SSSS (Secret Storage Service) key derivation implementations for:
- Curve25519-AES-SHA2 algorithm (MSC2697)
- AES-HMAC-SHA2 algorithm

## Files Audited
- `synapse-e2ee/src/ssss/service.rs`

---

## 1. Curve25519-AES-SHA2 Algorithm

### Current Implementation
Located at `service.rs:56-98`

```rust
fn create_curve25519_key(key_id: &str) -> Result<SecretStorageKeyCreationTerm, ApiError> {
    let secret = StaticSecret::random_from_rng(OsRng);
    let public = PublicKey::from(&secret);
    // ... key exchange logic ...
    
    // Random session key generation
    let mut session_key_bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut session_key_bytes);
    
    // Encrypt with AES-256-GCM
    let cipher = Aes256Gcm::new_from_slice(&session_key_bytes)...
    let ciphertext = cipher.encrypt(nonce, key_data.as_bytes())...
}
```

### Findings

✅ **CORRECT**: Uses cryptographically secure random number generator (`OsRng`)
✅ **CORRECT**: Key length is 32 bytes (256-bit) for AES-256
✅ **CORRECT**: Uses AES-256-GCM mode (authenticated encryption)
✅ **CORRECT**: Random nonce generation for each encryption

### Recommendation
No changes needed. This implementation follows best practices for server-generated secret storage keys.

---

## 2. AES-HMAC-SHA2 Algorithm

### Current Implementation
Located at `service.rs:101-123`

```rust
fn create_aes_hmac_key(key_id: &str) -> SecretStorageKeyCreationTerm {
    let mut key_bytes = [0u8; SSSS_KEY_LENGTH];  // 32 bytes
    rand::rng().fill_bytes(&mut key_bytes);
    
    let mut iv_bytes = [0u8; SSSS_IV_LENGTH];   // 12 bytes
    rand::rng().fill_bytes(&mut iv_bytes);
    
    let key_data = format!("{key_base64}:{iv_base64}");
    let mac = compute_hmac(&key_data, b"secure_storage_key");
    // ...
}
```

### Findings

✅ **CORRECT**: Key length is 32 bytes (256-bit)
✅ **CORRECT**: Uses cryptographically secure RNG
✅ **CORRECT**: Uses HMAC-SHA2 for MAC computation

⚠️ **MISSING**: No PBKDF2 derivation observed. This algorithm appears to be designed for user-provided passwords, but the current implementation generates random keys server-side.

### Investigation

Search results show:
- `derive_ssss_key()` function NOT found in codebase
- `PBKDF2` / `iterations` / `salt` keywords NOT found in ssss module
- The HKDF constant `SSSS_HKDF_INFO` exists (`service.rs:24`) but is unused

### Gap Analysis

| Expected Behavior | Actual Behavior | Risk Level |
|-------------------|-----------------|------------|
| Client uploads password-derived key | Server generates random key | **LOW** |
| PBKDF2/HKDF used for key derivation | Random generation via `OsRng` | **LOW** |

### Root Cause
The current implementation assumes **server-managed** secret storage keys (Curve25519 flow), not **user-password-derived** keys (AES-HMAC flow). This matches the Matrix spec where:
- Curve25519-AES-SHA2: Server generates key, encrypts client-side
- AES-HMAC-SHA2: User provides password, derives key via PBKDF2

---

## 3. Client-Side Derivation Status

Based on the Tjg frontend codebase structure:

### MatrixAttachmentEncryptionService.ts
Uses Web Crypto API (AES-CTR) for attachment encryption, not SSSS key derivation.

### SSSS Key Derivation Location
Expected locations (not verified):
- `Tjg/src/services/matrix/crypto/MatrixE2EEBootstrapService.ts`
- `Tjg/src/services/matrix/crypto/CryptoSDKAdapter.ts`

These would contain client-side PBKDF2/HKDF derivation logic.

---

## Conclusion

### Server-Side (synapse-rust)
✅ **PASS** - All key generation uses cryptographic RNG
✅ **PASS** - Correct key lengths and algorithms
✅ **PASS** - Proper authenticated encryption modes

⚠️ **NOTE** - No PBKDF2 derivation needed for server-managed keys.

### Pending Actions
1. Audit client-side SSSS bootstrap flow in Tjg frontend
2. Verify user-password → AES-HMAC-SHA2 key derivation (if implemented)
3. Ensure consistent HKDF info parameter usage across client/server

### Recommendations

#### Short-term (P3)
Document that server-side SSSS keys are randomly generated (not derived) to avoid confusion.

#### Medium-term (P2)
If user-password-based secret storage is required:
- Implement PBKDF2-SHA256 derivation in client SDK
- Ensure iterations ≥ 100,000 (OWASP recommendation)
- Add salt per user/key pair
- Store iteration count and salt alongside encrypted keys

---

## References
- Matrix Spec MSC2697: Secret Storage
- OWASP Password Storage Cheat Sheet
- NIST SP 800-132 (PBKDF2 recommendations)

---

## Revision History
| Date | Author | Change |
|------|--------|--------|
| 2026-09-19 | synapse-rust team | Initial audit |
