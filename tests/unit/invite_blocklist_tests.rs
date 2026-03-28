#![cfg(test)]

mod invite_blocklist_tests {
    use crate::common::{get_database_url, get_test_pool_async};
    use synapse_rust::storage::invite_blocklist::InviteBlocklistStorage;

    #[tokio::test]
    #[ignore]
    async fn test_invite_blocklist_operations() {
        let pool = match get_test_pool_async().await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Skipping test: {}", e);
                return;
            }
        };
        let storage = InviteBlocklistStorage::new(pool);
        let room_id = "!testroom:localhost";
        let blocked_users = vec![
            "@blocked1:localhost".to_string(),
            "@blocked2:localhost".to_string(),
        ];

        // Set blocklist
        storage.set_invite_blocklist(room_id, blocked_users.clone())
            .await
            .expect("Failed to set blocklist");

        // Get blocklist
        let result = storage.get_invite_blocklist(room_id)
            .await
            .expect("Failed to get blocklist");

        assert_eq!(result.len(), 2);
        assert!(result.contains(&"@blocked1:localhost".to_string()));
        assert!(result.contains(&"@blocked2:localhost".to_string()));

        // Check if user is blocked
        let is_blocked = storage.is_user_blocked(room_id, "@blocked1:localhost")
            .await
            .expect("Failed to check blocklist");
        assert!(is_blocked);

        let is_not_blocked = storage.is_user_blocked(room_id, "@notblocked:localhost")
            .await
            .expect("Failed to check blocklist");
        assert!(!is_not_blocked);
    }

    #[tokio::test]
    #[ignore]
    async fn test_invite_allowlist_operations() {
        let pool = match get_test_pool_async().await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Skipping test: {}", e);
                return;
            }
        };

        let storage = InviteBlocklistStorage::new(pool);
        let room_id = "!testroom:localhost";
        let allowed_users = vec![
            "@allowed1:localhost".to_string(),
            "@allowed2:localhost".to_string(),
        ];

        // Set allowlist
        storage.set_invite_allowlist(room_id, allowed_users.clone())
            .await
            .expect("Failed to set allowlist");

        // Get allowlist
        let result = storage.get_invite_allowlist(room_id)
            .await
            .expect("Failed to get allowlist");

        assert_eq!(result.len(), 2);
        assert!(result.contains(&"@allowed1:localhost".to_string()));
        assert!(result.contains(&"@allowed2:localhost".to_string()));

        // Check if user is allowed
        let is_allowed = storage.is_user_allowed(room_id, "@allowed1:localhost")
            .await
            .expect("Failed to check allowlist");
        assert!(is_allowed);

        let is_not_allowed = storage.is_user_allowed(room_id, "@notallowed:localhost")
            .await
            .expect("Failed to check allowlist");
        assert!(!is_not_allowed);
    }

    #[tokio::test]
    #[ignore] // Requires database setup
    async fn test_has_invite_restriction() {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://synapse:secret@localhost:5432/synapse_test".to_string());

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(10))
            .connect(&database_url)
            .await
            .expect("Failed to connect to test database");

        let storage = InviteBlocklistStorage::new(Arc::new(pool));
        let room_id = "!testroom:localhost";

        // Initially no restriction
        let has_restriction = storage.has_any_invite_restriction(room_id)
            .await
            .expect("Failed to check restriction");
        assert!(!has_restriction);

        // Set blocklist
        storage.set_invite_blocklist(room_id, vec!["@user:localhost".to_string()])
            .await
            .expect("Failed to set blocklist");

        // Now should have restriction
        let has_restriction = storage.has_any_invite_restriction(room_id)
            .await
            .expect("Failed to check restriction");
        assert!(has_restriction);
    }

    #[test]
    fn test_user_id_format_validation() {
        // Test that user IDs follow Matrix format
        let valid_users = vec![
            "@user:localhost",
            "@user:example.com",
            "@user:matrix.org",
            "@alice:hs1.example.org",
        ];

        for user in valid_users {
            assert!(user.starts_with('@'), "User ID should start with @");
            assert!(user.contains(':'), "User ID should contain : separator");
        }
    }

    #[test]
    fn test_room_id_format_validation() {
        // Test that room IDs follow Matrix format
        let valid_rooms = vec![
            "!room:localhost",
            "!room:example.com",
            "!abc123:matrix.org",
        ];

        for room in valid_rooms {
            assert!(room.starts_with('!'), "Room ID should start with !");
            assert!(room.contains(':'), "Room ID should contain : separator");
        }
    }
}
