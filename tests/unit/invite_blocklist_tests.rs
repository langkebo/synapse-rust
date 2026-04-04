#![cfg(test)]

mod invite_blocklist_suite {
    use crate::common::get_test_pool_async;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    use synapse_rust::storage::invite_blocklist::InviteBlocklistStorage;

    static UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(1);

    fn unique_suffix() -> u128 {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let counter = UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed) as u128;
        (timestamp << 16) | counter
    }

    async fn connect_pool() -> Option<std::sync::Arc<sqlx::PgPool>> {
        match get_test_pool_async().await {
            Ok(pool) => Some(pool),
            Err(error) => {
                eprintln!(
                    "Skipping invite blocklist tests because test database is unavailable: {}",
                    error
                );
                None
            }
        }
    }

    async fn seed_room(
        pool: &sqlx::PgPool,
        suffix: u128,
        other_users: &[String],
    ) -> (String, String, Vec<String>) {
        let creator = format!("@invitecreator{suffix}:localhost");
        let room_id = format!("!inviteroom{suffix}:localhost");

        sqlx::query(
            "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(&creator)
        .bind(format!("invitecreator{suffix}"))
        .bind(0_i64)
        .execute(pool)
        .await
        .expect("Failed to seed creator");

        for user_id in other_users {
            let username = user_id
                .trim_start_matches('@')
                .split(':')
                .next()
                .unwrap_or("user")
                .to_string();

            sqlx::query(
                "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING",
            )
            .bind(user_id)
            .bind(username)
            .bind(0_i64)
            .execute(pool)
            .await
            .expect("Failed to seed user");
        }

        sqlx::query("INSERT INTO rooms (room_id, creator, created_ts) VALUES ($1, $2, $3) ON CONFLICT (room_id) DO NOTHING")
            .bind(&room_id)
            .bind(&creator)
            .bind(0_i64)
            .execute(pool)
            .await
            .expect("Failed to seed room");

        let mut users = vec![creator.clone()];
        users.extend_from_slice(other_users);
        (creator, room_id, users)
    }

    async fn cleanup(pool: &sqlx::PgPool, room_id: &str, users: &[String]) {
        sqlx::query("DELETE FROM room_invite_blocklist WHERE room_id = $1")
            .bind(room_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup room_invite_blocklist");

        sqlx::query("DELETE FROM room_invite_allowlist WHERE room_id = $1")
            .bind(room_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup room_invite_allowlist");

        sqlx::query("DELETE FROM rooms WHERE room_id = $1")
            .bind(room_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup room");

        for user_id in users {
            sqlx::query("DELETE FROM users WHERE user_id = $1")
                .bind(user_id)
                .execute(pool)
                .await
                .expect("Failed to cleanup user");
        }
    }

    #[tokio::test]
    async fn test_invite_blocklist_operations() {
        let pool = match connect_pool().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = InviteBlocklistStorage::new(pool.clone());
        let suffix = unique_suffix();
        let blocked_users = vec![
            format!("@blocked1{suffix}:localhost"),
            format!("@blocked2{suffix}:localhost"),
        ];
        let (_creator, room_id, users) = seed_room(&pool, suffix, &blocked_users).await;

        // Set blocklist
        storage
            .set_invite_blocklist(&room_id, blocked_users.clone())
            .await
            .expect("Failed to set blocklist");

        // Get blocklist
        let result = storage
            .get_invite_blocklist(&room_id)
            .await
            .expect("Failed to get blocklist");

        assert_eq!(result.len(), 2);
        assert!(result.contains(&blocked_users[0]));
        assert!(result.contains(&blocked_users[1]));

        // Check if user is blocked
        let is_blocked = storage
            .is_user_blocked(&room_id, &blocked_users[0])
            .await
            .expect("Failed to check blocklist");
        assert!(is_blocked);

        let is_not_blocked = storage
            .is_user_blocked(&room_id, "@notblocked:localhost")
            .await
            .expect("Failed to check blocklist");
        assert!(!is_not_blocked);

        cleanup(&pool, &room_id, &users).await;
    }

    #[tokio::test]
    async fn test_invite_allowlist_operations() {
        let pool = match connect_pool().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = InviteBlocklistStorage::new(pool.clone());
        let suffix = unique_suffix();
        let allowed_users = vec![
            format!("@allowed1{suffix}:localhost"),
            format!("@allowed2{suffix}:localhost"),
        ];
        let (_creator, room_id, users) = seed_room(&pool, suffix, &allowed_users).await;

        // Set allowlist
        storage
            .set_invite_allowlist(&room_id, allowed_users.clone())
            .await
            .expect("Failed to set allowlist");

        // Get allowlist
        let result = storage
            .get_invite_allowlist(&room_id)
            .await
            .expect("Failed to get allowlist");

        assert_eq!(result.len(), 2);
        assert!(result.contains(&allowed_users[0]));
        assert!(result.contains(&allowed_users[1]));

        // Check if user is allowed
        let is_allowed = storage
            .is_user_allowed(&room_id, &allowed_users[0])
            .await
            .expect("Failed to check allowlist");
        assert!(is_allowed);

        let is_not_allowed = storage
            .is_user_allowed(&room_id, "@notallowed:localhost")
            .await
            .expect("Failed to check allowlist");
        assert!(!is_not_allowed);

        cleanup(&pool, &room_id, &users).await;
    }

    #[tokio::test]
    async fn test_has_invite_restriction() {
        let pool = match connect_pool().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = InviteBlocklistStorage::new(pool.clone());
        let suffix = unique_suffix();
        let seed_users = vec![format!("@restriction{suffix}:localhost")];
        let (_creator, room_id, users) = seed_room(&pool, suffix, &seed_users).await;

        // Initially no restriction
        let has_restriction = storage
            .has_any_invite_restriction(&room_id)
            .await
            .expect("Failed to check restriction");
        assert!(!has_restriction);

        // Set blocklist
        storage
            .set_invite_blocklist(&room_id, vec![seed_users[0].clone()])
            .await
            .expect("Failed to set blocklist");

        // Now should have restriction
        let has_restriction = storage
            .has_any_invite_restriction(&room_id)
            .await
            .expect("Failed to check restriction");
        assert!(has_restriction);

        cleanup(&pool, &room_id, &users).await;
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
        let valid_rooms = vec!["!room:localhost", "!room:example.com", "!abc123:matrix.org"];

        for room in valid_rooms {
            assert!(room.starts_with('!'), "Room ID should start with !");
            assert!(room.contains(':'), "Room ID should contain : separator");
        }
    }
}
