use crate::cache::CacheManager;
use crate::e2ee::olm::models::OlmAccountInfo;
use std::sync::Arc;

pub struct OlmService {
    account: vodozemac::olm::Account,
    cache: Arc<CacheManager>,
}

impl OlmService {
    pub fn new(cache: Arc<CacheManager>) -> Self {
        Self {
            account: vodozemac::olm::Account::new(),
            cache,
        }
    }

    pub fn generate_one_time_keys(&mut self, count: usize) {
        self.account.generate_one_time_keys(count);
    }

    pub fn get_account_info(&self) -> OlmAccountInfo {
        let identity_keys = self.account.identity_keys();
        
        let one_time_keys: Vec<String> = self
            .account
            .one_time_keys()
            .iter()
            .map(|(id, k)| format!("{}:{}", id.to_base64(), k.to_base64()))
            .collect();

        let fallback_key = self.account.fallback_key()
            .iter()
            .next()
            .map(|(id, k)| format!("{}:{}", id.to_base64(), k.to_base64()));

        OlmAccountInfo {
            identity_key: identity_keys.curve25519.to_base64(),
            one_time_keys,
            fallback_key,
        }
    }

    pub fn sign(&self, message: &[u8]) -> String {
        let signature = self.account.sign(message);
        signature.to_base64()
    }

    pub fn mark_keys_as_published(&mut self) {
        self.account.mark_keys_as_published();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::CacheConfig;

    fn create_test_cache() -> Arc<CacheManager> {
        let config = CacheConfig::default();
        Arc::new(CacheManager::new(config))
    }

    #[test]
    fn test_olm_account_creation() {
        let cache = create_test_cache();
        let service = OlmService::new(cache);
        
        let info = service.get_account_info();
        assert!(!info.identity_key.is_empty());
    }

    #[test]
    fn test_generate_one_time_keys() {
        let cache = create_test_cache();
        let mut service = OlmService::new(cache);
        
        service.generate_one_time_keys(5);
        let info = service.get_account_info();
        assert!(!info.one_time_keys.is_empty());
    }

    #[test]
    fn test_sign() {
        let cache = create_test_cache();
        let service = OlmService::new(cache);
        
        let message = b"Test message";
        let signature = service.sign(message);
        
        assert!(!signature.is_empty());
    }
}
