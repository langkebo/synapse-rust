pub use synapse_services::typing_service::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::CacheConfig;

    fn create_test_service() -> TypingService {
        let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
        TypingService::new(cache)
    }

    #[tokio::test]
    async fn test_set_typing() {
        let service = create_test_service();

        service.set_typing("!room:example.com", "@user:example.com", 30000).await.unwrap();

        let timeout = service.get_user_typing("!room:example.com", "@user:example.com").await.unwrap();
        assert!(timeout.is_some());
        assert_eq!(timeout, Some(30000));
    }

    #[tokio::test]
    async fn test_clear_typing() {
        let service = create_test_service();

        service.set_typing("!room:example.com", "@user:example.com", 30000).await.unwrap();
        service.clear_typing("!room:example.com", "@user:example.com").await.unwrap();

        let timeout = service.get_user_typing("!room:example.com", "@user:example.com").await.unwrap();
        assert!(timeout.is_none());
    }

    #[tokio::test]
    async fn test_get_typing_users() {
        let service = create_test_service();

        service.set_typing("!room:example.com", "@user1:example.com", 30000).await.unwrap();
        service.set_typing("!room:example.com", "@user2:example.com", 30000).await.unwrap();

        let users = service.get_typing_users("!room:example.com").await.unwrap();

        assert_eq!(users.len(), 2);
        assert!(users.contains_key("@user1:example.com"));
        assert!(users.contains_key("@user2:example.com"));
    }

    #[tokio::test]
    async fn test_get_user_not_typing() {
        let service = create_test_service();

        let timeout = service.get_user_typing("!room:example.com", "@user:example.com").await.unwrap();
        assert!(timeout.is_none());
    }

    #[tokio::test]
    async fn test_typing_different_rooms() {
        let service = create_test_service();

        service.set_typing("!room1:example.com", "@user:example.com", 30000).await.unwrap();
        service.set_typing("!room2:example.com", "@user:example.com", 30000).await.unwrap();

        let users1 = service.get_typing_users("!room1:example.com").await.unwrap();
        let users2 = service.get_typing_users("!room2:example.com").await.unwrap();

        assert!(users1.contains_key("@user:example.com"));
        assert!(users2.contains_key("@user:example.com"));
    }

    #[tokio::test]
    async fn test_clear_expired_typing() {
        let service = create_test_service();

        // Set typing with 0 timeout (immediately expired)
        service.set_typing("!room:example.com", "@user:example.com", 0).await.unwrap();

        // Clear expired
        service.clear_expired_typing().unwrap();

        // Expired users are cleaned up on read in get_typing_users
        let timeout = service.get_user_typing("!room:example.com", "@user:example.com").await.unwrap();
        assert!(timeout.is_none());
    }

    #[tokio::test]
    async fn test_typing_timeout() {
        let service = create_test_service();

        service.set_typing("!room:example.com", "@user:example.com", 5000).await.unwrap();

        let timeout = service.get_user_typing("!room:example.com", "@user:example.com").await.unwrap();
        assert_eq!(timeout, Some(5000));
    }

    #[tokio::test]
    async fn test_overwrite_typing() {
        let service = create_test_service();

        service.set_typing("!room:example.com", "@user:example.com", 30000).await.unwrap();
        service.set_typing("!room:example.com", "@user:example.com", 60_000).await.unwrap();

        let timeout = service.get_user_typing("!room:example.com", "@user:example.com").await.unwrap();
        assert_eq!(timeout, Some(60_000));
    }
}
