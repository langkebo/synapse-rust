// Typing Service Tests - 打字提示服务测试

#[cfg(test)]
mod tests {
    use synapse_rust::{TypingService, TypingServiceImpl};

    #[tokio::test]
    async fn test_set_typing() {
        let service = TypingServiceImpl::new();

        // Set typing
        service
            .set_typing("!room:example.com", "@user:example.com", 30000)
            .await
            .unwrap();

        // Check if user is typing
        let timeout = service
            .get_user_typing("!room:example.com", "@user:example.com")
            .await
            .unwrap();
        assert!(timeout.is_some());
    }

    #[tokio::test]
    async fn test_clear_typing() {
        let service = TypingServiceImpl::new();

        // Set typing
        service
            .set_typing("!room:example.com", "@user:example.com", 30000)
            .await
            .unwrap();

        // Clear typing
        service
            .clear_typing("!room:example.com", "@user:example.com")
            .await
            .unwrap();

        // Should not be typing
        let timeout = service
            .get_user_typing("!room:example.com", "@user:example.com")
            .await
            .unwrap();
        assert!(timeout.is_none());
    }

    #[tokio::test]
    async fn test_get_typing_users() {
        let service = TypingServiceImpl::new();

        // Set multiple users typing
        service
            .set_typing("!room:example.com", "@user1:example.com", 30000)
            .await
            .unwrap();
        service
            .set_typing("!room:example.com", "@user2:example.com", 30000)
            .await
            .unwrap();

        // Get typing users
        let users = service.get_typing_users("!room:example.com").await.unwrap();

        assert_eq!(users.len(), 2);
        assert!(users.contains_key("@user1:example.com"));
        assert!(users.contains_key("@user2:example.com"));
    }

    #[tokio::test]
    async fn test_get_user_not_typing() {
        let service = TypingServiceImpl::new();

        // Check user who hasn't typed
        let timeout = service
            .get_user_typing("!room:example.com", "@user:example.com")
            .await
            .unwrap();
        assert!(timeout.is_none());
    }

    #[tokio::test]
    async fn test_typing_different_rooms() {
        let service = TypingServiceImpl::new();

        // Set typing in different rooms
        service
            .set_typing("!room1:example.com", "@user:example.com", 30000)
            .await
            .unwrap();
        service
            .set_typing("!room2:example.com", "@user:example.com", 30000)
            .await
            .unwrap();

        // Should be typing in both rooms
        let users1 = service
            .get_typing_users("!room1:example.com")
            .await
            .unwrap();
        let users2 = service
            .get_typing_users("!room2:example.com")
            .await
            .unwrap();

        assert!(users1.contains_key("@user:example.com"));
        assert!(users2.contains_key("@user:example.com"));
    }

    #[tokio::test]
    async fn test_clear_expired_typing() {
        let service = TypingServiceImpl::new();

        // Set typing with very short timeout (would expire)
        service
            .set_typing("!room:example.com", "@user:example.com", 0)
            .await
            .unwrap();

        // Clear expired
        service.clear_expired_typing().await.unwrap();

        // Should be cleared
        let timeout = service
            .get_user_typing("!room:example.com", "@user:example.com")
            .await
            .unwrap();
        assert!(timeout.is_none());
    }

    #[tokio::test]
    async fn test_typing_timeout() {
        let service = TypingServiceImpl::new();

        // Set typing with specific timeout
        service
            .set_typing("!room:example.com", "@user:example.com", 5000)
            .await
            .unwrap();

        // Check timeout value
        let timeout = service
            .get_user_typing("!room:example.com", "@user:example.com")
            .await
            .unwrap();
        assert_eq!(timeout, Some(5000));
    }
}
