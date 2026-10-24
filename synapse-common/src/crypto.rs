//! Cryptographic helpers: base64/hex codecs, HMAC, password hashing, token generation, signing keys.

use crate::current_timestamp_millis;
// argon2 0.6 起 `password_hash::PasswordHash` 是**已弃用**的别名（真身迁到 `phc`），
// 而 `argon2::PasswordHash` 是非弃用的 re-export；`SaltString` 已被移除
// （password-hash 0.6 改用"自动生成盐 / 原始字节盐"）。
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use base64::{
    engine::general_purpose::{STANDARD_NO_PAD, URL_SAFE_NO_PAD},
    Engine as _,
};
use hmac::{Hmac, KeyInit, Mac};
use rand::Rng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::argon2_config::Argon2Config;

type HmacSha256 = Hmac<Sha256>;

/// Hashes a password asynchronously.
pub fn hash_password(password: &str) -> Result<String, String> {
    let config = Argon2Config::get_global();
    hash_password_with_config(password, &config)
}

/// hash password with config.
pub fn hash_password_with_config(password: &str, config: &Argon2Config) -> Result<String, String> {
    let params = config.to_argon2_params().map_err(|e| e.to_string())?;
    let argon2 = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);

    // password-hash 0.6：盐不再由调用方传入（`SaltString` 已删除）。`hash_password`
    // 自己取 16 字节随机盐（`getrandom` feature，argon2 0.6 默认启用），
    // 生成的 PHC 串格式与 0.5 一致 —— 这点由 KAT
    // `password_hash_produced_before_the_upgrade_still_verifies` 反向钉住。
    let password_hash = argon2.hash_password(password.as_bytes()).map_err(|e| e.to_string())?.to_string();

    Ok(password_hash)
}

/// hash password with params.
pub fn hash_password_with_params(password: &str, m_cost: u32, t_cost: u32, p_cost: u32) -> Result<String, String> {
    let config = Argon2Config::new(m_cost, t_cost, p_cost).map_err(|e| format!("Invalid Argon2 parameters: {e}"))?;
    hash_password_with_config(password, &config)
}

/// Verifies a password against a hash asynchronously.
pub fn verify_password(password: &str, password_hash: &str, allow_legacy: bool) -> Result<bool, String> {
    if password_hash.starts_with("$argon2") {
        let parsed_hash = PasswordHash::new(password_hash).map_err(|e| e.to_string())?;
        return Ok(Argon2::default().verify_password(password.as_bytes(), &parsed_hash).is_ok());
    }

    if !allow_legacy {
        tracing::warn!("Legacy password hash detected but legacy hashes are disabled");
        return Err("Legacy password hashes are no longer supported".to_string());
    }

    tracing::warn!("Legacy password hash detected - please update password");

    let password_hash_trimmed = password_hash.trim_start_matches('$');
    let parts: Vec<&str> = password_hash_trimmed.split('$').collect();

    if parts.len() < 4 {
        return Err("Invalid hash format".to_string());
    }

    let algo = parts[0];
    let version = parts[1];
    let _params = parts[2];
    let salt = parts[3];
    let hash = parts[parts.len() - 1];

    if algo != "sha256" || version != "v=1" {
        return Err(format!("Unsupported hash algorithm: {algo}"));
    }

    if parts.len() >= 7 {
        let iterations = parts[5].parse::<u32>().unwrap_or(10000);
        let mut computed_hash = [0u8; 32];
        let mut input = password.as_bytes().to_vec();
        input.extend_from_slice(salt.as_bytes());

        for _ in 0..iterations {
            let mut hasher = Sha256::new();
            hasher.update(&input);
            let result = hasher.finalize();
            computed_hash.copy_from_slice(&result);
            input = computed_hash.to_vec();
        }
        let encoded_hash = URL_SAFE_NO_PAD.encode(computed_hash);
        Ok(secure_compare(&encoded_hash, hash))
    } else {
        let mut hasher = Sha256::new();
        hasher.update(password);
        hasher.update(salt);
        let result = hasher.finalize();
        let encoded_hash = URL_SAFE_NO_PAD.encode(result);
        Ok(secure_compare(&encoded_hash, hash))
    }
}

/// Secures the compare.
pub fn secure_compare(a: &str, b: &str) -> bool {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let max_len = a_bytes.len().max(b_bytes.len());

    let mut result: u8 = if a_bytes.len() != b_bytes.len() { 0xFF } else { 0 };

    for i in 0..max_len {
        let a_byte = a_bytes.get(i).copied().unwrap_or(0);
        let b_byte = b_bytes.get(i).copied().unwrap_or(0);
        result |= a_byte ^ b_byte;
    }
    result == 0
}

/// verify password legacy.
pub fn verify_password_legacy(password: &str, password_hash: &str) -> bool {
    verify_password(password, password_hash, true).unwrap_or(false)
}

/// Returns true if legacy hash.
pub fn is_legacy_hash(password_hash: &str) -> bool {
    !password_hash.starts_with("$argon2")
}

/// Migrates the password.
pub fn migrate_password_hash(password: &str, m_cost: u32, t_cost: u32, p_cost: u32) -> Result<String, String> {
    hash_password_with_params(password, m_cost, t_cost, p_cost)
}

/// Migrates the password.
pub fn migrate_password_hash_with_config(password: &str, config: &Argon2Config) -> Result<String, String> {
    hash_password_with_config(password, config)
}

/// Generates the token.
pub fn generate_token(length: usize) -> String {
    let mut bytes = vec![0u8; length];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Generates the room.
pub fn generate_room_id(server_name: &str) -> String {
    let mut bytes = [0u8; 18];
    rand::rng().fill_bytes(&mut bytes);
    format!("!{}:{}", URL_SAFE_NO_PAD.encode(bytes), server_name)
}

/// Generates the event.
///
/// ⚠️ DEPRECATED: This function generates legacy-style event IDs for backward compatibility.
/// For room versions 3+, use [`synapse_common::event_id::compute_event_id`] which computes
/// the event ID from the reference hash (SHA-256 of redacted event canonical JSON).
pub fn generate_event_id(server_name: &str) -> String {
    let timestamp = current_timestamp_millis();
    let mut bytes = [0u8; 18];
    rand::rng().fill_bytes(&mut bytes);
    format!("${}${}:{}", timestamp, URL_SAFE_NO_PAD.encode(bytes), server_name)
}

/// Generates the device.
pub fn generate_device_id() -> String {
    let mut bytes = [0u8; 10];
    rand::rng().fill_bytes(&mut bytes);
    format!("DEVICE{}", URL_SAFE_NO_PAD.encode(bytes).get(..10).unwrap_or("DEVICE0000"))
}

/// Generates the salt.
pub fn generate_salt() -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Computes the hash.
pub fn compute_hash(data: impl AsRef<[u8]>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data.as_ref());
    URL_SAFE_NO_PAD.encode(&hasher.finalize()[..])
}

/// SHA-256 of `data`, encoded with the **standard** Base64 alphabet and no
/// padding — the media content-hash format upstream Synapse persists.
///
/// Upstream reference (release-v1.161, `synapse/media/media_repository.py`):
/// `unpaddedbase64.encode_base64(sha256(content).digest())`, looked up with
/// `store.get_is_hash_quarantined(content_hash)`. The value stored in
/// `media_metadata.content_hash` must therefore be byte-compatible with that
/// helper, which uses the standard alphabet (`+`/`/`) and strips `=`.
///
/// Deliberately **not** [`compute_hash`]: that helper uses the URL-safe
/// alphabet (`-`/`_`). The two encodings disagree on most digests (e.g. the
/// digest of the empty input contains `/` in standard Base64), so reusing
/// `compute_hash` here would silently miss every hash written by upstream or a
/// peer server.
pub fn content_hash(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    STANDARD_NO_PAD.encode(hasher.finalize())
}

/// The known dev/test fallback secret. In production, this value must NEVER be used.
const DEV_TEST_TOKEN_HASH_SECRET: &str = "dev-test-token-hash-secret-do-not-use-in-production";

/// Exit code shared by every binary entry point when the startup secret check fails.
///
/// `2` is already "bad usage" in the CLIs under `src/bin/` (see
/// `federation_sign_request.rs`), so a missing/weak `TOKEN_HASH_SECRET` gets its own code.
pub const TOKEN_HASH_SECRET_EXIT_CODE: i32 = 3;

/// Outcome of inspecting a `TOKEN_HASH_SECRET` value.
///
/// Split out from [`validate_token_hash_secret`] so the decision table can be unit
/// tested without mutating process-global environment variables (which would race with
/// every other test in the same lib test binary that calls [`hash_token`]).
#[derive(Debug, PartialEq, Eq)]
enum TokenHashSecretVerdict {
    /// Usable secret, nothing to say.
    Configured,
    /// Debug build intentionally falling back to the well-known dev/test value.
    DevFallbackInDebug,
    /// Debug build with no secret at all: allowed, warned, never safe for production.
    MissingInDebug,
    /// Refuse to serve. Message is shown verbatim on stderr / in the startup error.
    Rejected(String),
}

/// Pure decision function behind [`validate_token_hash_secret`].
///
/// `release_build` mirrors `!cfg!(debug_assertions)`; taking it as a parameter keeps
/// both branches testable in one build profile.
fn classify_token_hash_secret(secret: Option<&str>, release_build: bool) -> TokenHashSecretVerdict {
    match secret {
        // 空串是最容易发生的 fail-open：env 存在但没赋值（`TOKEN_HASH_SECRET=`）。
        // 以前只查"变量是否存在"，于是空 key 会被拿去算 HMAC —— 这里按长度统一拒掉。
        Some(value) if value.len() < 32 => TokenHashSecretVerdict::Rejected(format!(
            "TOKEN_HASH_SECRET must be at least 32 bytes, got {} bytes",
            value.len()
        )),
        Some(value) if value == DEV_TEST_TOKEN_HASH_SECRET => {
            if release_build {
                TokenHashSecretVerdict::Rejected(
                    "TOKEN_HASH_SECRET is set to the known dev/test fallback value. This is NOT safe for production."
                        .to_string(),
                )
            } else {
                TokenHashSecretVerdict::DevFallbackInDebug
            }
        }
        Some(_) => TokenHashSecretVerdict::Configured,
        None => {
            if release_build {
                TokenHashSecretVerdict::Rejected(
                    "TOKEN_HASH_SECRET environment variable must be set in production".to_string(),
                )
            } else {
                TokenHashSecretVerdict::MissingInDebug
            }
        }
    }
}

/// Validate that `TOKEN_HASH_SECRET` is properly configured for the current build mode.
///
/// In production (release builds): the secret must be set, at least 32 bytes, and must
/// not equal the known dev/test fallback. Returns `Err` if any check fails.
///
/// In dev/test (debug builds): a missing or weak secret is allowed with a warning.
/// An **empty** value (variable present, no value) is rejected in every build mode —
/// that is the one shape that used to fail open silently.
///
/// Call this at server startup before any request is processed. Every binary under
/// `src/bin/` does so through [`ensure_token_hash_secret_or_exit`].
pub fn validate_token_hash_secret() -> Result<(), String> {
    let secret = std::env::var("TOKEN_HASH_SECRET").ok();
    match classify_token_hash_secret(secret.as_deref(), !cfg!(debug_assertions)) {
        TokenHashSecretVerdict::Configured => Ok(()),
        TokenHashSecretVerdict::DevFallbackInDebug => {
            tracing::warn!(
                "TOKEN_HASH_SECRET is set to the dev/test fallback. \
                 This is NOT safe for production."
            );
            Ok(())
        }
        TokenHashSecretVerdict::MissingInDebug => {
            tracing::warn!(
                "TOKEN_HASH_SECRET not set, using dev/test fallback. \
                 This is NOT safe for production."
            );
            Ok(())
        }
        TokenHashSecretVerdict::Rejected(message) => Err(message),
    }
}

/// Startup guard for **every** executable entry point under `src/bin/`.
///
/// Calls [`validate_token_hash_secret`] and terminates the process with
/// [`TOKEN_HASH_SECRET_EXIT_CODE`] instead of letting the first real lookup reach
/// `hash_token`'s `unreachable!()` (CODE-01b: in a release build a missing secret used
/// to mean "start fine, then panic on the first token hash").
///
/// Put this as the **first statement** of `main` (the one exception is `synapse_worker`,
/// which runs its offline `--validate-topology` check first because that mode needs no
/// secret, and validates immediately after). The guard test
/// `every_binary_entry_point_validates_the_token_hash_secret` fails if any binary is
/// added or edited without it.
pub fn ensure_token_hash_secret_or_exit(component: &str) {
    if let Err(reason) = validate_token_hash_secret() {
        eprintln!("[{component}] startup aborted: TOKEN_HASH_SECRET check failed: {reason}");
        eprintln!("[{component}] set TOKEN_HASH_SECRET to >= 32 bytes (deployment env / docker/.env) and retry.");
        std::process::exit(TOKEN_HASH_SECRET_EXIT_CODE);
    }
}

/// hash token.
///
/// ⚠️ **本函数在 release 构建且 `TOKEN_HASH_SECRET` 缺失时是 panic 的**（fail-closed：
/// 用弱默认值算哈希比直接停更危险 —— 那会让所有人以为登录态还在）。
///
/// 这个 panic 面在**进程层**已被封死（CODE-01b）：主服务在 `src/server/mod.rs`、
/// `src/bin/` 下 8 个可执行入口在 `main` 第一行调用
/// [`ensure_token_hash_secret_or_exit`] ⇒ 空/缺失/过短的 secret 在启动期就非零退出，
/// 走不到这里。守卫见 `every_binary_entry_point_validates_the_token_hash_secret`。
///
/// 库层仍保留 `Result` 化的余地（签名改 `Result` 要动 `verify_token_hash` 与
/// `tests/**` 里的调用点，属另一笔），所以这里继续用 `#[expect(clippy::unreachable)]`
/// **显式登记**，而不是让它躲过 `-D clippy::unreachable` 门禁：一旦这一行被删掉，
/// expectation 落空、编译失败。
#[expect(
    clippy::unreachable,
    reason = "fail-closed defence-in-depth: the server and all 8 src/bin entry points validate the \
              secret at startup; a Result-returning signature is tracked separately"
)]
pub fn hash_token(token: &str) -> String {
    let server_secret = std::env::var("TOKEN_HASH_SECRET").unwrap_or_else(|_| {
        if cfg!(debug_assertions) {
            // Dev/test fallback only — never used in production (startup validation enforces this).
            DEV_TEST_TOKEN_HASH_SECRET.to_string()
        } else {
            // Production: this should never be reached because validate_token_hash_secret()
            // is called at startup. Defense in depth: fail rather than use a weak secret.
            unreachable!("TOKEN_HASH_SECRET environment variable must be set in production")
        }
    });
    encode_base64(hmac_sha256(server_secret, token))
}

/// hash token legacy.
pub fn hash_token_legacy(token: &str) -> String {
    compute_hash(token)
}

/// verify token hash.
pub fn verify_token_hash(token: &str, stored_hash: &str) -> bool {
    if secure_compare(&hash_token(token), stored_hash) {
        return true;
    }
    secure_compare(&hash_token_legacy(token), stored_hash)
}

#[allow(clippy::expect_used, reason = "HMAC-SHA256 accepts a key of any length; new_from_slice cannot fail")]
/// Hmacs the sha256.
pub fn hmac_sha256(key: impl AsRef<[u8]>, data: impl AsRef<[u8]>) -> Vec<u8> {
    let key = key.as_ref();
    let data = data.as_ref();
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC-SHA256 accepts keys of any size");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// Randoms the string.
pub fn random_string(length: usize) -> String {
    static CHARSET: [u8; 62] = *b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::rng();
    let result: String = (0..length)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect();
    result
}

/// Encodes base64.
pub fn encode_base64(data: impl AsRef<[u8]>) -> String {
    URL_SAFE_NO_PAD.encode(data.as_ref())
}

/// Decodes base64.
pub fn decode_base64(s: &str) -> Result<Vec<u8>, base64::DecodeError> {
    URL_SAFE_NO_PAD.decode(s)
}

/// Decode a base64 string into a fixed-size 32-byte array.
/// Tries multiple base64 engines (STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD)
/// to maximize compatibility with various key formats.
pub fn decode_base64_32(value: &str) -> Option<[u8; 32]> {
    use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE};
    let value = value.trim();
    let try_decode = |engine: &base64::engine::general_purpose::GeneralPurpose| -> Option<[u8; 32]> {
        let bytes = engine.decode(value).ok()?;
        if bytes.len() == 32 {
            let mut out = [0u8; 32];
            out.copy_from_slice(&bytes);
            Some(out)
        } else {
            None
        }
    };
    try_decode(&STANDARD)
        .or_else(|| try_decode(&STANDARD_NO_PAD))
        .or_else(|| try_decode(&URL_SAFE))
        .or_else(|| try_decode(&URL_SAFE_NO_PAD))
}

/// Constant-time comparison for byte slices.
/// Returns `true` if `a` and `b` are equal in both length and content.
/// The comparison runs in time proportional to the longer slice, preventing timing attacks.
/// Length differences are folded into the comparison result rather than returning early.
pub fn secure_compare_bytes(a: &[u8], b: &[u8]) -> bool {
    let max_len = a.len().max(b.len());
    let mut result: u8 = if a.len() != b.len() { 0xFF } else { 0 };
    for i in 0..max_len {
        let a_byte = a.get(i).copied().unwrap_or(0);
        let b_byte = b.get(i).copied().unwrap_or(0);
        result |= a_byte ^ b_byte;
    }
    result == 0
}

/// Encode binary data as a lowercase hexadecimal string.
pub fn encode_hex(data: impl AsRef<[u8]>) -> String {
    hex::encode(data.as_ref())
}

/// Decode a hexadecimal string into bytes.
pub fn decode_hex(s: &str) -> Result<Vec<u8>, hex::FromHexError> {
    hex::decode(s)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Represents ServerSigningKey.
pub struct ServerSigningKey {
    /// `key_id` field.
    pub key_id: String,
    /// `key` field.
    pub key: String,
    /// `expired_ts` field.
    pub expired_ts: i64,
}

/// Generate an Ed25519 signing key pair for testing.
/// Returns `(key_id, base64_public_key)`.
#[cfg(test)]
/// Generates the signing.
pub fn generate_signing_key() -> (String, String) {
    let key_id = format!("ed25519:{}", random_string(8));
    let key = random_string(44);
    (key_id, key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::argon2_config::Argon2Config;

    #[test]
    fn test_hash_password_and_verify() {
        let config = Argon2Config::new(65536, 3, 1).unwrap();
        let password = "test_password_123";
        let hash = hash_password_with_config(password, &config).unwrap();
        assert!(hash.starts_with("$argon2"));
        assert!(verify_password(password, &hash, false).unwrap());
        assert!(!verify_password("wrong_password", &hash, false).unwrap());
    }

    #[test]
    fn test_hash_password_with_params() {
        let password = "test_password";
        let hash = hash_password_with_params(password, 65536, 3, 1).unwrap();
        assert!(hash.starts_with("$argon2"));
    }

    #[test]
    fn test_hash_password_backward_compatible() {
        let password = "test_password";
        let hash = hash_password_with_params(password, 65536, 3, 1).unwrap();
        assert!(hash.starts_with("$argon2"));
        assert!(verify_password(password, &hash, false).unwrap());
    }

    #[test]
    fn test_hash_password_with_config() {
        let config = Argon2Config::new(65536, 3, 1).unwrap();
        let password = "test_password_with_config";
        let hash = hash_password_with_config(password, &config).unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(verify_password(password, &hash, false).unwrap());
    }

    #[test]
    fn test_verify_password_invalid_format() {
        let result = verify_password("password", "invalid", true);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Invalid hash format".to_string());
    }

    #[test]
    fn test_verify_password_legacy_valid() {
        let password = "test_password";
        let salt = "testsalt12345678";
        let mut hasher = Sha256::new();
        hasher.update(password);
        hasher.update(salt);
        let result = hasher.finalize();
        let encoded = URL_SAFE_NO_PAD.encode(result);
        let hash = format!("sha256$v=1$m=32,p=1${salt}${encoded}");
        assert!(verify_password_legacy(password, &hash));
        assert!(!verify_password_legacy("wrong_password", &hash));
    }

    #[test]
    fn test_verify_password_legacy_invalid() {
        assert!(!verify_password_legacy("password", "invalid"));
        assert!(!verify_password_legacy("password", "sha256$v=1$invalid$hash"));
    }

    #[test]
    fn test_is_legacy_hash() {
        let argon2_hash = "$argon2id$v=19$m=65536,t=3,p=1$salt$hash";
        assert!(!is_legacy_hash(argon2_hash));

        let legacy_hash = "sha256$v=1$m=32,p=1$salt$hash";
        assert!(is_legacy_hash(legacy_hash));

        let bcrypt_hash = "$2b$12$abcdefghijklmnopqrstuv";
        assert!(is_legacy_hash(bcrypt_hash));
    }

    #[test]
    fn test_migrate_password_hash() {
        let password = "password_to_migrate";
        let new_hash = migrate_password_hash(password, 65536, 3, 1).unwrap();

        assert!(new_hash.starts_with("$argon2"));
        assert!(verify_password(password, &new_hash, false).unwrap());
    }

    #[test]
    fn test_migrate_password_hash_with_config() {
        let config = Argon2Config::new(65536, 3, 1).unwrap();
        let password = "password_to_migrate_with_config";
        let new_hash = migrate_password_hash_with_config(password, &config).unwrap();

        assert!(new_hash.starts_with("$argon2id$"));
        assert!(verify_password(password, &new_hash, false).unwrap());
    }

    #[test]
    fn test_password_migration_flow() {
        let password = "test_migration_flow";
        let salt = "migrationtestsalt";
        let mut hasher = Sha256::new();
        hasher.update(password);
        hasher.update(salt);
        let result = hasher.finalize();
        let encoded = URL_SAFE_NO_PAD.encode(result);
        let legacy_hash = format!("sha256$v=1$m=32,p=1${salt}${encoded}");

        assert!(is_legacy_hash(&legacy_hash));
        assert!(verify_password_legacy(password, &legacy_hash));

        let new_hash = migrate_password_hash(password, 65536, 3, 1).unwrap();
        assert!(!is_legacy_hash(&new_hash));
        assert!(verify_password(password, &new_hash, false).unwrap());
    }

    #[test]
    fn test_generate_token() {
        let token1 = generate_token(32);
        let token2 = generate_token(32);
        assert_eq!(token1.len(), 43);
        assert_eq!(token2.len(), 43);
        assert_ne!(token1, token2);
    }

    #[test]
    fn test_generate_room_id() {
        let room_id = generate_room_id("example.com");
        assert!(room_id.starts_with('!'));
        assert!(room_id.contains(":example.com"));
    }

    #[test]
    fn test_generate_event_id() {
        let event_id = generate_event_id("example.com");
        assert!(event_id.starts_with('$'));
        assert!(event_id.contains(":example.com"));
    }

    #[test]
    fn test_generate_device_id() {
        let device_id = generate_device_id();
        assert!(device_id.starts_with("DEVICE"));
        assert_eq!(device_id.len(), 16);
    }

    #[test]
    fn test_generate_salt() {
        let salt1 = generate_salt();
        let salt2 = generate_salt();
        assert_eq!(salt1.len(), 22);
        assert_eq!(salt2.len(), 22);
        assert_ne!(salt1, salt2);
    }

    #[test]
    fn test_compute_hash() {
        let data = b"test data";
        let hash = compute_hash(data);
        assert_eq!(hash.len(), 43);
        assert_ne!(hash, compute_hash(b"different data"));
    }

    /// Known-answer vectors for the upstream Synapse media content hash:
    /// standard Base64 (`+`/`/`) of the SHA-256 digest, padding stripped.
    ///
    /// Independently reproducible with:
    /// `printf '' | openssl dgst -sha256 -binary | openssl base64 -A`
    /// → `47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=`
    #[test]
    fn test_content_hash_known_answers() {
        // sha256("") = e3b0c442…b855. Do NOT "fix" this to
        // `2jmj7l5rSw0yVb/vlWAYkK/YBwk`: that value is SHA-1(""), a different
        // algorithm. `test_content_hash_differs_from_compute_hash` below pins
        // the standard-vs-URL-safe alphabet distinction separately.
        assert_eq!(content_hash(b""), "47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU");
        assert_eq!(content_hash(b"abc"), "ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0");
        assert_eq!(
            content_hash(b"The quick brown fox jumps over the lazy dog"),
            "16j7swfXgJRpypq8sAguT41WUeRtPNt2LQLQvzfJ5ZI"
        );
    }

    #[test]
    fn test_content_hash_never_emits_padding() {
        // Every digest whose byte length is not a multiple of 3 would be padded
        // in padded Base64; the media hash must never carry `=`.
        for data in [&b""[..], b"a", b"ab", b"abc", b"abcd"] {
            let hash = content_hash(data);
            assert!(!hash.contains('='), "content_hash must be unpadded, got: {hash}");
        }
    }

    /// The whole reason this helper exists next to `compute_hash`: the two
    /// alphabets differ, and only the standard one matches upstream Synapse.
    #[test]
    fn test_content_hash_differs_from_compute_hash() {
        let standard = content_hash(b"");
        let url_safe = compute_hash(b"");
        // sha256("")'s Base64 contains `/` in the standard alphabet and `_` in
        // the URL-safe one — the exact character the two engines disagree on.
        assert!(standard.contains('/'), "standard alphabet must contain '/': {standard}");
        assert!(url_safe.contains('_'), "URL-safe alphabet must contain '_': {url_safe}");
        assert_ne!(standard, url_safe);
    }

    #[test]
    fn test_hmac_sha256() {
        let key = b"test_key";
        let data = b"test_data";
        let hmac1 = hmac_sha256(key, data);
        let hmac2 = hmac_sha256(key, data);
        assert_eq!(hmac1.len(), 32);
        assert_eq!(hmac1, hmac2);
        assert_ne!(hmac1, hmac_sha256(b"wrong_key", data));
    }

    #[test]
    fn test_random_string() {
        let s1 = random_string(10);
        let s2 = random_string(10);
        assert_eq!(s1.len(), 10);
        assert_eq!(s2.len(), 10);
        assert_ne!(s1, s2);
        assert!(s1.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn test_encode_decode_base64() {
        let original = b"hello world test data";
        let encoded = encode_base64(original);
        let decoded = decode_base64(&encoded).unwrap();
        assert_eq!(original, decoded.as_slice());
    }

    #[test]
    fn test_decode_base64_invalid() {
        let result = decode_base64("!!!invalid base64!!!");
        assert!(result.is_err());
    }

    #[test]
    fn test_generate_signing_key() {
        let (key_id, key) = generate_signing_key();
        assert!(key_id.starts_with("ed25519:"));
        assert_eq!(key_id.len(), 8 + 8);
        assert_eq!(key.len(), 44);
    }

    #[test]
    fn test_hash_token_deterministic() {
        let token = "syt_abc123_def456_ghijk789";
        let h1 = hash_token(token);
        let h2 = hash_token(token);
        assert_eq!(h1, h2, "hash_token must be deterministic");
    }

    #[test]
    fn test_hash_token_different_inputs_produce_different_hashes() {
        let h1 = hash_token("token_a");
        let h2 = hash_token("token_b");
        assert_ne!(h1, h2, "different inputs must produce different hashes");
    }

    #[test]
    fn test_hash_token_non_empty() {
        let result = hash_token("some_token_value");
        assert!(!result.is_empty(), "hash must not be empty");
    }

    #[test]
    fn test_hash_token_legacy_deterministic() {
        let token = "MDAxYWxvY2F0aW9uIGV4YW1wbGUuY29t";
        let h1 = hash_token_legacy(token);
        let h2 = hash_token_legacy(token);
        assert_eq!(h1, h2, "hash_token_legacy must be deterministic");
    }

    #[test]
    fn test_hash_token_legacy_different_from_current() {
        let token = "same_token_for_both";
        let h_cur = hash_token(token);
        let h_leg = hash_token_legacy(token);
        // Legacy and current hash algorithms should produce different outputs
        assert_ne!(h_cur, h_leg, "legacy and current hashes must differ");
    }

    #[test]
    fn test_hash_token_legacy_non_empty() {
        let result = hash_token_legacy("another_token");
        assert!(!result.is_empty(), "legacy hash must not be empty");
    }

    #[test]
    fn test_hash_token_edge_empty_string() {
        let result = hash_token("");
        assert!(!result.is_empty(), "hash of empty string must still produce output");
    }

    #[test]
    fn test_hash_token_legacy_edge_empty_string() {
        let result = hash_token_legacy("");
        assert!(!result.is_empty(), "legacy hash of empty string must still produce output");
    }

    // ── decode_base64_32 ─────────────────────────────────────────────

    #[test]
    fn decode_base64_32_standard_padded() {
        // "A" * 32 bytes → base64 with padding
        let result = decode_base64_32("QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE=");
        assert!(result.is_some());
        assert_eq!(result.unwrap(), [b'A'; 32]);
    }

    #[test]
    fn decode_base64_32_standard_no_pad() {
        let result = decode_base64_32("QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE");
        assert!(result.is_some());
        assert_eq!(result.unwrap(), [b'A'; 32]);
    }

    #[test]
    fn decode_base64_32_url_safe() {
        // 32 bytes where the standard encoding would produce '+' and '/'
        // \xFB\xFF... produces -/_ in URL-safe encoding
        let mut bytes = [0u8; 32];
        bytes[0] = 0xFB;
        bytes[1] = 0xFF;
        let encoded = URL_SAFE_NO_PAD.encode(bytes);
        let result = decode_base64_32(&encoded);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), bytes);
    }

    #[test]
    fn decode_base64_32_wrong_length() {
        // "A" * 31 bytes → base64 will decode to 31 bytes, not 32
        let result = decode_base64_32("QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQ");
        assert!(result.is_none());
    }

    #[test]
    fn decode_base64_32_empty() {
        let result = decode_base64_32("");
        assert!(result.is_none());
    }

    #[test]
    fn decode_base64_32_invalid() {
        let result = decode_base64_32("!!!invalid!!!");
        assert!(result.is_none());
    }

    #[test]
    fn decode_base64_32_trims_whitespace() {
        let result = decode_base64_32(" QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE= ");
        assert!(result.is_some());
        assert_eq!(result.unwrap(), [b'A'; 32]);
    }

    // ── encode_hex / decode_hex ──────────────────────────────────────

    #[test]
    fn encode_hex_roundtrip() {
        let data = [0x00u8, 0xFF, 0xAB, 0x12];
        let encoded = encode_hex(data);
        assert_eq!(encoded, "00ffab12");
        let decoded = decode_hex(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn decode_hex_invalid() {
        let result = decode_hex("xyz");
        assert!(result.is_err());
    }

    #[test]
    fn encode_hex_empty() {
        let result = encode_hex([]);
        assert!(result.is_empty());
    }

    // ── secure_compare ────────────────────────────────────────────────

    #[test]
    fn test_secure_compare_equal() {
        assert!(secure_compare("abc123", "abc123"));
    }

    #[test]
    fn test_secure_compare_different() {
        assert!(!secure_compare("abc123", "xyz789"));
    }

    #[test]
    fn test_secure_compare_different_lengths() {
        assert!(!secure_compare("short", "longer string"));
    }

    #[test]
    fn test_secure_compare_empty() {
        assert!(secure_compare("", ""));
    }

    #[test]
    fn test_secure_compare_one_empty() {
        assert!(!secure_compare("", "non-empty"));
        assert!(!secure_compare("non-empty", ""));
    }

    #[test]
    fn test_secure_compare_prefix_does_not_match() {
        assert!(!secure_compare("abc", "abcdef"));
        assert!(!secure_compare("abcdef", "abc"));
    }

    #[test]
    fn test_secure_compare_case_sensitive() {
        assert!(!secure_compare("ABC", "abc"));
    }

    // ── secure_compare_bytes ──────────────────────────────────────────

    #[test]
    fn test_secure_compare_bytes_equal() {
        assert!(secure_compare_bytes(b"abc123", b"abc123"));
    }

    #[test]
    fn test_secure_compare_bytes_different() {
        assert!(!secure_compare_bytes(b"abc123", b"xyz789"));
    }

    #[test]
    fn test_secure_compare_bytes_different_lengths() {
        assert!(!secure_compare_bytes(b"short", b"longer string"));
    }

    #[test]
    fn test_secure_compare_bytes_empty() {
        assert!(secure_compare_bytes(b"", b""));
    }

    #[test]
    fn test_secure_compare_bytes_one_empty() {
        assert!(!secure_compare_bytes(b"", b"non-empty"));
        assert!(!secure_compare_bytes(b"non-empty", b""));
    }

    #[test]
    fn test_secure_compare_bytes_binary_data() {
        let a = [0x00u8, 0xFF, 0xAB, 0x12];
        let b = [0x00u8, 0xFF, 0xAB, 0x12];
        assert!(secure_compare_bytes(&a, &b));
        let c = [0x00u8, 0xFF, 0xAB, 0x13];
        assert!(!secure_compare_bytes(&a, &c));
    }

    #[test]
    fn test_secure_compare_bytes_leading_null_bytes() {
        assert!(secure_compare_bytes(b"\x00\x00abc", b"\x00\x00abc"));
        assert!(!secure_compare_bytes(b"\x00\x00abc", b"\x00\x00xyz"));
    }

    // ── verify_token_hash ─────────────────────────────────────────────

    #[test]
    fn test_verify_token_hash_current() {
        let token = "syt_test_token_abc123";
        let hash = hash_token(token);
        assert!(verify_token_hash(token, &hash));
    }

    #[test]
    fn test_verify_token_hash_legacy_fallback() {
        let token = "syt_test_token_legacy";
        let legacy_hash = hash_token_legacy(token);
        assert!(verify_token_hash(token, &legacy_hash));
    }

    #[test]
    fn test_verify_token_hash_wrong_token() {
        let token = "valid_token";
        let wrong = "wrong_token";
        let hash = hash_token(token);
        assert!(!verify_token_hash(wrong, &hash));
    }

    #[test]
    fn test_verify_token_hash_empty_token() {
        let token = "";
        let hash = hash_token(token);
        assert!(verify_token_hash(token, &hash));
    }

    #[test]
    fn test_verify_token_hash_empty_hash() {
        let token = "some_token";
        assert!(!verify_token_hash(token, ""));
    }

    #[test]
    fn test_verify_token_hash_consistency() {
        let token = "consistent_test_token";
        let hash = hash_token(token);
        assert!(verify_token_hash(token, &hash));
        assert!(verify_token_hash(token, &hash));
    }

    // --- RustCrypto 大版本升级的跨版本 KAT ---------------------------------
    //
    // 这些向量**不是**"记录当前实现输出"，而是规范值（NIST FIPS 180-4 /
    // RFC 4231），因此它们同时约束升级前后两侧：任何一侧算错都会红。
    //
    // 为什么必须单独钉：`digest` 0.11 把 `finalize()` 的返回类型换成了
    // `hybrid_array::Array`，**不再实现 `LowerHex`**。仓库里原本有三处
    // `format!("{:x}", hasher.finalize())`，那种写法会从"编译通过"变成
    // "编译失败"；更危险的是有人为了让它编译过去而改成
    // `format!("{:x}", hasher.finalize().as_slice())` 之类的变形 —— 那会静默
    // 改变输出格式（大端/小端、大小写、是否带分隔符），而 `state_record` 的
    // 房间状态哈希与 `ledger_export` 的路由面指纹都是**持久化/对外**的值。
    // 所以这里把 hex 的**逐字符**形态也钉住。

    #[test]
    fn sha256_matches_nist_vectors_and_hex_lowercase() {
        assert_eq!(
            hex_lower(&Sha256::digest(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "SHA-256 of the empty input (FIPS 180-4)"
        );
        assert_eq!(
            hex_lower(&Sha256::digest(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            "SHA-256(\"abc\") — lowercase, no separators, big-endian byte order"
        );
    }

    #[test]
    fn hmac_sha256_matches_rfc4231() {
        // RFC 4231 test case 1: key = 0x0b repeated 20 times, data = "Hi There".
        let mut mac = HmacSha256::new_from_slice(&[0x0bu8; 20]).expect("HMAC accepts any key length");
        mac.update(b"Hi There");
        assert_eq!(
            hex_lower(&mac.finalize().into_bytes()),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7",
            "RFC 4231 §4.2 test case 1"
        );
    }

    /// 升级 `argon2` 之后，**库里已有的口令哈希必须仍然可验证**。
    ///
    /// 这条向量是用 argon2 0.5 + 固定盐生成的 PHC 串（不是随机哈希），所以它的
    /// 期望结论是"永远能验过"。如果 0.6 改了默认参数、编码方式或解析器行为，
    /// 这里会红 —— 否则症状会出现在线上：所有老用户登录失败。
    #[test]
    fn password_hash_produced_before_the_upgrade_still_verifies() {
        const LEGACY_PHC: &str =
            "$argon2id$v=19$m=19456,t=2,p=1$a2F0LWZpeGVkLXNhbHQtMTY$XrzqQZ+XcIaigagA5qEH3yK7FXtYJ9NFh6460PTPNds";
        assert!(
            verify_password("correct horse battery staple", LEGACY_PHC, false).unwrap(),
            "an Argon2id hash written by the previous dependency version must still verify"
        );
    }

    // ── CODE-01b：TOKEN_HASH_SECRET 启动校验 ───────────────────────────────
    //
    // 判据不是"函数返回 Ok"，而是"**空键与缺键都不会走到 hash_token 的 unreachable**"。
    // 决策表做成纯函数（`classify_token_hash_secret`）就是为了不碰进程级 env ——
    // 同一个 lib test 二进制里所有 hash_token 用例都读 TOKEN_HASH_SECRET，set_var 会串扰。

    #[test]
    fn empty_token_hash_secret_is_rejected_in_every_build_mode() {
        // `TOKEN_HASH_SECRET=`（变量在、值为空）以前能过 `std::env::var` 的 Ok 分支，
        // 于是空 key 被拿去算 HMAC ⇒ 所有人共享同一个可预测的哈希。这是 fail-open。
        for release_build in [false, true] {
            match classify_token_hash_secret(Some(""), release_build) {
                TokenHashSecretVerdict::Rejected(msg) => {
                    assert!(msg.contains("at least 32 bytes"), "release_build={release_build}: {msg}");
                }
                other => panic!("empty secret must be rejected (release_build={release_build}), got {other:?}"),
            }
        }
    }

    #[test]
    fn missing_token_hash_secret_is_fatal_in_release_and_warned_in_debug() {
        assert_eq!(
            classify_token_hash_secret(None, true),
            TokenHashSecretVerdict::Rejected(
                "TOKEN_HASH_SECRET environment variable must be set in production".to_string()
            ),
            "release build without the secret must refuse to start rather than panic on first use"
        );
        assert_eq!(classify_token_hash_secret(None, false), TokenHashSecretVerdict::MissingInDebug);
    }

    #[test]
    fn dev_fallback_secret_is_fatal_in_release_only() {
        assert_eq!(
            classify_token_hash_secret(Some(DEV_TEST_TOKEN_HASH_SECRET), true),
            TokenHashSecretVerdict::Rejected(
                "TOKEN_HASH_SECRET is set to the known dev/test fallback value. This is NOT safe for production."
                    .to_string()
            )
        );
        assert_eq!(
            classify_token_hash_secret(Some(DEV_TEST_TOKEN_HASH_SECRET), false),
            TokenHashSecretVerdict::DevFallbackInDebug
        );
    }

    #[test]
    fn a_32_byte_secret_is_accepted_in_both_modes() {
        let secret = "x".repeat(32);
        assert_eq!(classify_token_hash_secret(Some(&secret), false), TokenHashSecretVerdict::Configured);
        assert_eq!(classify_token_hash_secret(Some(&secret), true), TokenHashSecretVerdict::Configured);
    }

    /// 每个 `src/bin/` 入口的 `main` 必须**第一件事**就是校验 secret（CODE-01b）。
    ///
    /// 为什么是源码扫描而不是起进程：debug 构建下缺 secret 是允许的（dev fallback），
    /// 真跑起来也只会打警告，所以"启动即非零退出"只能在 release 里观察；而 release
    /// 编译 8 个二进制不适合放进默认测试车道。这条守卫钉的是**接线本身**：新增/删改
    /// 二进制时漏掉 guard ⇒ 红。
    #[test]
    fn every_binary_entry_point_validates_the_token_hash_secret() {
        const GUARD: &str = "ensure_token_hash_secret_or_exit(";
        let bin_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/bin");
        let mut checked: Vec<String> = Vec::new();
        for entry in std::fs::read_dir(&bin_dir).unwrap_or_else(|e| panic!("read {}: {e}", bin_dir.display())) {
            let path = entry.expect("readable dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let name = path.file_name().and_then(|n| n.to_str()).expect("utf-8 file name").to_string();
            let source = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            // 只在**代码**里找，不认注释：否则把调用注释掉（`// ensure_...(...)`）仍能骗过守卫。
            let code: String =
                source.lines().map(|line| line.split("//").next().unwrap_or("")).collect::<Vec<_>>().join("\n");
            assert!(
                code.contains(GUARD),
                "src/bin/{name} 没有调用 {GUARD}：release 构建缺 TOKEN_HASH_SECRET 时，\
                 它会在第一次 hash_token 调用处 panic，而不是启动即退出"
            );
            checked.push(name);
        }
        checked.sort();
        assert_eq!(
            checked,
            vec![
                "federation_sign_request.rs",
                "generate_test_keypair.rs",
                "healthcheck.rs",
                "run_migrations.rs",
                "schema_health_check.rs",
                "synapse_ledger_export.rs",
                "synapse_ledger_fingerprint.rs",
                "synapse_worker.rs",
            ],
            "src/bin 的入口集合变了 —— 新增入口必须在 main 第一行调用 {GUARD}，然后更新本清单"
        );
    }

    /// 小写十六进制，无前缀、无分隔符 —— 与 `serde_json` 之外的持久化格式一致。
    fn hex_lower(bytes: &[u8]) -> String {
        use std::fmt::Write as _;
        let mut out = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            // `write!` 到 `String` 不会失败（`fmt::Write for String` 的 `Err` 不可达）。
            let _ = write!(out, "{byte:02x}");
        }
        out
    }
}
