// DM Service Tests - 直接消息服务测试

#[cfg(test)]
mod tests {
    use synapse_rust::{DMService, DMServiceImpl};

    #[test]
    fn test_create_dm_key() {
        let key1 = DMServiceImpl::create_dm_key("@alice:example.com", "@bob:example.com");
        let key2 = DMServiceImpl::create_dm_key("@bob:example.com", "@alice:example.com");

        // Keys should be the same regardless of order
        assert_eq!(key1, key2);
    }

    #[tokio::test]
    async fn test_mark_room_as_dm() {
        let service = DMServiceImpl::new();

        // Mark room as DM
        service
            .mark_room_as_dm(
                "!dm:example.com",
                "@alice:example.com",
                &["@bob:example.com".to_string()],
            )
            .await
            .unwrap();

        // Check if it's a DM
        let is_dm = service
            .is_dm_room("!dm:example.com", "@alice:example.com")
            .await
            .unwrap();
        assert!(is_dm);
    }

    #[tokio::test]
    async fn test_is_not_dm_room() {
        let service = DMServiceImpl::new();

        // Check non-DM room
        let is_dm = service
            .is_dm_room("!room:example.com", "@alice:example.com")
            .await
            .unwrap();
        assert!(!is_dm);
    }

    #[tokio::test]
    async fn test_get_dm_partner() {
        let service = DMServiceImpl::new();

        // Create DM
        service
            .mark_room_as_dm(
                "!dm:example.com",
                "@alice:example.com",
                &["@bob:example.com".to_string()],
            )
            .await
            .unwrap();

        // Get partner from alice's perspective
        let partner = service
            .get_dm_partner("!dm:example.com", "@alice:example.com")
            .await
            .unwrap();
        assert_eq!(partner, Some("@bob:example.com".to_string()));

        // Get partner from bob's perspective
        let partner = service
            .get_dm_partner("!dm:example.com", "@bob:example.com")
            .await
            .unwrap();
        assert_eq!(partner, Some("@alice:example.com".to_string()));
    }

    #[tokio::test]
    async fn test_get_user_dms() {
        let service = DMServiceImpl::new();

        // Create DMs
        service
            .mark_room_as_dm(
                "!dm1:example.com",
                "@alice:example.com",
                &["@bob:example.com".to_string()],
            )
            .await
            .unwrap();

        service
            .mark_room_as_dm(
                "!dm2:example.com",
                "@alice:example.com",
                &["@charlie:example.com".to_string()],
            )
            .await
            .unwrap();

        // Get user's DMs
        let dms = service.get_user_dms("@alice:example.com").await.unwrap();
        assert_eq!(dms.len(), 2);
    }

    #[tokio::test]
    async fn test_get_existing_dm() {
        let service = DMServiceImpl::new();

        // Create DM
        let room_id = service
            .get_existing_dm("@alice:example.com", "@bob:example.com")
            .await
            .unwrap();
        assert_eq!(room_id, None);

        // Mark as DM
        service
            .mark_room_as_dm(
                "!dm:example.com",
                "@alice:example.com",
                &["@bob:example.com".to_string()],
            )
            .await
            .unwrap();

        // Should find existing DM
        let room_id = service
            .get_existing_dm("@alice:example.com", "@bob:example.com")
            .await
            .unwrap();
        assert_eq!(room_id, Some("!dm:example.com".to_string()));
    }

    #[tokio::test]
    async fn test_update_dm_users() {
        let service = DMServiceImpl::new();

        // Create DM
        service
            .mark_room_as_dm(
                "!dm:example.com",
                "@alice:example.com",
                &["@bob:example.com".to_string()],
            )
            .await
            .unwrap();

        // Update DM users
        service
            .update_dm_users(
                "!dm:example.com",
                "@alice:example.com",
                &[
                    "@bob:example.com".to_string(),
                    "@charlie:example.com".to_string(),
                ],
            )
            .await
            .unwrap();
    }
}
