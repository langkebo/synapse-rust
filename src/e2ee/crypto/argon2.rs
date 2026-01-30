use argon2::password_hash::{rand_core::OsRng, SaltString};
use argon2::Params;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};

#[derive(Debug, Clone)]
pub struct Argon2Params {
    pub t_cost: u32,
    pub m_cost: u32,
    pub p_cost: u32,
    pub output_len: usize,
}

impl Default for Argon2Params {
    fn default() -> Self {
        Self {
            t_cost: 3,
            m_cost: 65536,
            p_cost: 4,
            output_len: 32,
        }
    }
}

pub struct Argon2Kdf {
    algorithm: Argon2<'static>,
    params: Argon2Params,
}

impl Argon2Kdf {
    pub fn new(params: Argon2Params) -> Result<Self, super::CryptoError> {
        let params_obj = Params::new(
            params.m_cost,
            params.t_cost,
            params.p_cost,
            Some(params.output_len),
        )
        .map_err(|e| super::CryptoError::HashError(e.to_string()))?;

        let algorithm = Argon2::new(
            argon2::Algorithm::Argon2id,
            argon2::Version::V0x13,
            params_obj,
        );
        Ok(Self { algorithm, params })
    }

    pub fn hash_password(&self, password: &str) -> Result<String, super::CryptoError> {
        let salt = SaltString::generate(&mut OsRng);
        let password_hash = self
            .algorithm
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| super::CryptoError::HashError(e.to_string()))?;
        Ok(password_hash.to_string())
    }

    pub fn verify_password(&self, password: &str, hash: &str) -> Result<bool, super::CryptoError> {
        let parsed_hash =
            PasswordHash::new(hash).map_err(|e| super::CryptoError::HashError(e.to_string()))?;
        Ok(self
            .algorithm
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok())
    }

    pub fn derive_key(&self, password: &str, salt: &[u8]) -> Result<Vec<u8>, super::CryptoError> {
        let mut output = vec![0u8; self.params.output_len];
        self.algorithm
            .hash_password_into(password.as_bytes(), salt, &mut output)
            .map_err(|e| super::CryptoError::HashError(e.to_string()))?;
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_argon2_params_default() {
        let params = Argon2Params::default();
        assert_eq!(params.t_cost, 3);
        assert_eq!(params.m_cost, 65536);
        assert_eq!(params.p_cost, 4);
        assert_eq!(params.output_len, 32);
    }

    #[test]
    fn test_argon2_params_custom() {
        let params = Argon2Params {
            t_cost: 2,
            m_cost: 32768,
            p_cost: 2,
            output_len: 64,
        };
        assert_eq!(params.t_cost, 2);
        assert_eq!(params.m_cost, 32768);
        assert_eq!(params.p_cost, 2);
        assert_eq!(params.output_len, 64);
    }

    #[test]
    fn test_argon2_kdf_new_default_params() {
        let kdf = Argon2Kdf::new(Argon2Params::default()).unwrap();
        let params = kdf.params;
        assert_eq!(params.t_cost, 3);
        assert_eq!(params.m_cost, 65536);
        assert_eq!(params.p_cost, 4);
        assert_eq!(params.output_len, 32);
    }

    #[test]
    fn test_argon2_kdf_new_custom_params() {
        let params = Argon2Params {
            t_cost: 2,
            m_cost: 32768,
            p_cost: 2,
            output_len: 64,
        };
        let kdf = Argon2Kdf::new(params).unwrap();
        assert_eq!(kdf.params.output_len, 64);
    }

    #[test]
    fn test_argon2_hash_password() {
        let kdf = Argon2Kdf::new(Argon2Params::default()).unwrap();
        let password = "test_password_123";

        let hash = kdf.hash_password(password).unwrap();
        assert!(!hash.is_empty());
        assert!(hash.contains("$argon2id$"));
    }

    #[test]
    fn test_argon2_verify_password_correct() {
        let kdf = Argon2Kdf::new(Argon2Params::default()).unwrap();
        let password = "secure_password";

        let hash = kdf.hash_password(password).unwrap();
        let result = kdf.verify_password(password, &hash).unwrap();
        assert!(result);
    }

    #[test]
    fn test_argon2_verify_password_incorrect() {
        let kdf = Argon2Kdf::new(Argon2Params::default()).unwrap();
        let password = "correct_password";
        let wrong_password = "wrong_password";

        let hash = kdf.hash_password(password).unwrap();
        let result = kdf.verify_password(wrong_password, &hash).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_argon2_verify_password_empty() {
        let kdf = Argon2Kdf::new(Argon2Params::default()).unwrap();
        let password = "my_password";

        let hash = kdf.hash_password(password).unwrap();
        let result = kdf.verify_password("", &hash).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_argon2_verify_invalid_hash() {
        let kdf = Argon2Kdf::new(Argon2Params::default()).unwrap();
        let result = kdf.verify_password("password", "invalid_hash");
        assert!(result.is_err());
    }

    #[test]
    fn test_argon2_derive_key() {
        let kdf = Argon2Kdf::new(Argon2Params::default()).unwrap();
        let password = "key_derivation_password";
        let salt = b"unique_salt_value";

        let key = kdf.derive_key(password, salt).unwrap();
        assert_eq!(key.len(), 32);
        assert!(!key.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_argon2_derive_key_same_password_salt() {
        let kdf = Argon2Kdf::new(Argon2Params::default()).unwrap();
        let password = "password";
        let salt = b"fixed_salt_12345678";

        let key1 = kdf.derive_key(password, salt).unwrap();
        let key2 = kdf.derive_key(password, salt).unwrap();
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_argon2_derive_key_different_salts() {
        let kdf = Argon2Kdf::new(Argon2Params::default()).unwrap();
        let password = "password";

        let key1 = kdf.derive_key(password, b"salt1_12345678").unwrap();
        let key2 = kdf.derive_key(password, b"salt2_12345678").unwrap();
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_argon2_derive_key_different_passwords() {
        let kdf = Argon2Kdf::new(Argon2Params::default()).unwrap();
        let salt = b"fixed_salt_12345678";

        let key1 = kdf.derive_key("password1", salt).unwrap();
        let key2 = kdf.derive_key("password2", salt).unwrap();
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_argon2_derive_key_custom_output_length() {
        let params = Argon2Params {
            t_cost: 2,
            m_cost: 32768,
            p_cost: 2,
            output_len: 64,
        };
        let kdf = Argon2Kdf::new(params).unwrap();
        let password = "password";
        let salt = b"salt12345678";

        let key = kdf.derive_key(password, salt).unwrap();
        assert_eq!(key.len(), 64);
    }

    #[test]
    fn test_argon2_hash_deterministic_for_same_password() {
        let kdf = Argon2Kdf::new(Argon2Params::default()).unwrap();
        let password = "test_password";

        let hash1 = kdf.hash_password(password).unwrap();
        let hash2 = kdf.hash_password(password).unwrap();
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_argon2_verify_after_hash_and_verify() {
        let kdf = Argon2Kdf::new(Argon2Params::default()).unwrap();
        let password = "complex_password_!@#$%";

        let hash = kdf.hash_password(password).unwrap();
        let verify1 = kdf.verify_password(password, &hash).unwrap();
        let verify2 = kdf.verify_password(password, &hash).unwrap();

        assert!(verify1);
        assert!(verify2);
    }
}
