use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use argon2::password_hash::{SaltString, rand_core::OsRng};

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
        let algorithm = Argon2::new(
            argon2::Algorithm::Argon2id,
            argon2::Version::V0x13,
            argon2::Params::new(params.m_cost, params.t_cost, params.p_cost, None)
                .map_err(|e| super::CryptoError::HashError(e.to_string()))?,
        );
        Ok(Self { algorithm, params })
    }
    
    pub fn hash_password(&self, password: &str) -> Result<String, super::CryptoError> {
        let salt = SaltString::generate(&mut OsRng);
        let password_hash = self.algorithm
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| super::CryptoError::HashError(e.to_string()))?;
        Ok(password_hash.to_string())
    }
    
    pub fn verify_password(&self, password: &str, hash: &str) -> Result<bool, super::CryptoError> {
        let parsed_hash = PasswordHash::new(hash)
            .map_err(|e| super::CryptoError::HashError(e.to_string()))?;
        Ok(self.algorithm.verify_password(password.as_bytes(), &parsed_hash).is_ok())
    }
    
    pub fn derive_key(&self, password: &str, salt: &[u8]) -> Result<Vec<u8>, super::CryptoError> {
        let mut output = vec![0u8; self.params.output_len];
        self.algorithm.hash_password_into(
            self.algorithm.params(),
            password.as_bytes(),
            salt,
            &mut output
        ).map_err(|e| super::CryptoError::HashError(e.to_string()))?;
        Ok(output)
    }
}