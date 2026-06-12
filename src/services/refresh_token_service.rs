pub use synapse_services::refresh_token_service::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_token() {
        let token = "test_token_123";
        let hash1 = RefreshTokenService::hash_token(token);
        let hash2 = RefreshTokenService::hash_token(token);

        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 43);
    }

    #[test]
    fn test_hash_token_different() {
        let hash1 = RefreshTokenService::hash_token("token1");
        let hash2 = RefreshTokenService::hash_token("token2");

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_generate_token() {
        let token1 = RefreshTokenService::generate_token();
        let token2 = RefreshTokenService::generate_token();

        assert_ne!(token1, token2);
        assert!(!token1.is_empty());
        assert_eq!(token1.len(), 43);
    }

    #[test]
    fn test_generate_family_id() {
        let id1 = RefreshTokenService::generate_family_id();
        let id2 = RefreshTokenService::generate_family_id();

        assert_ne!(id1, id2);
        assert!(!id1.is_empty());
        assert_eq!(id1.len(), 22);
    }

    #[test]
    fn test_hash_token_empty() {
        let hash = RefreshTokenService::hash_token("");
        assert!(!hash.is_empty());
    }

    #[test]
    fn test_hash_token_special_chars() {
        let token = "token/with+special=chars";
        let hash = RefreshTokenService::hash_token(token);

        assert!(!hash.contains('/'));
        assert!(!hash.contains('+'));
        assert!(!hash.contains('='));
    }
}
