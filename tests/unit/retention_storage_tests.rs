#![cfg(test)]

mod retention_storage_tests {
    use sqlx::postgres::PgPoolOptions;
    use std::sync::Arc;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use synapse_rust::storage::retention::{
        CreateRoomRetentionPolicyRequest, RetentionStorage, UpdateRoomRetentionPolicyRequest,
        UpdateServerRetentionPolicyRequest,
    };

    fn unique_suffix() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }

    async fn connect_pool() -> Arc<sqlx::PgPool> {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .unwrap_or_else(|_| "postgresql://synapse:secret@localhost:5432/synapse_test".to_string());

        Arc::new(
            PgPoolOptions::new()
                .max_connections(5)
                .acquire_timeout(Duration::from_secs(10))
                .connect(&database_url)
                .await
                .expect("Failed to connect to test database"),
        )
    }

    async fn seed_room(pool: &sqlx::PgPool, suffix: u128) -> (String, String) {
        let creator = format!("@retentioncreator{suffix}:localhost");
        let room_id = format!("!retentionroom{suffix}:localhost");

        sqlx::query("INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING")
            .bind(&creator)
            .bind(format!("retentioncreator{suffix}"))
            .bind(0_i64)
            .execute(pool)
            .await
            .expect("Failed to seed creator");

        sqlx::query("INSERT INTO rooms (room_id, creator, created_ts) VALUES ($1, $2, $3) ON CONFLICT (room_id) DO NOTHING")
            .bind(&room_id)
            .bind(&creator)
            .bind(0_i64)
            .execute(pool)
            .await
            .expect("Failed to seed room");

        (creator, room_id)
    }

    async fn cleanup(pool: &sqlx::PgPool, room_id: &str, creator: &str) {
        sqlx::query("DELETE FROM room_retention_policies WHERE room_id = $1")
            .bind(room_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup room retention policy");

        sqlx::query("DELETE FROM rooms WHERE room_id = $1")
            .bind(room_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup room");

        sqlx::query("DELETE FROM users WHERE user_id = $1")
            .bind(creator)
            .execute(pool)
            .await
            .expect("Failed to cleanup user");
    }

    #[tokio::test]
    async fn test_retention_storage_roundtrip() {
        let pool = connect_pool().await;
        let storage = RetentionStorage::new(&pool);
        let suffix = unique_suffix();
        let (creator, room_id) = seed_room(&pool, suffix).await;

        let policy = storage
            .create_room_policy(CreateRoomRetentionPolicyRequest {
                room_id: room_id.clone(),
                max_lifetime: Some(86_400_000),
                min_lifetime: Some(3_600_000),
                expire_on_clients: Some(true),
            })
            .await
            .expect("Failed to create room policy");

        assert_eq!(policy.room_id, room_id);
        assert_eq!(policy.max_lifetime, Some(86_400_000));
        assert_eq!(policy.min_lifetime, 3_600_000);
        assert!(policy.expire_on_clients);

        let loaded = storage
            .get_room_policy(&room_id)
            .await
            .expect("Failed to get room policy")
            .expect("Room policy should exist");

        assert_eq!(loaded.id, policy.id);

        let updated = storage
            .update_room_policy(
                &room_id,
                UpdateRoomRetentionPolicyRequest {
                    max_lifetime: Some(172_800_000),
                    min_lifetime: Some(7_200_000),
                    expire_on_clients: Some(false),
                },
            )
            .await
            .expect("Failed to update room policy");

        assert_eq!(updated.max_lifetime, Some(172_800_000));
        assert_eq!(updated.min_lifetime, 7_200_000);
        assert!(!updated.expire_on_clients);

        let server_policy = storage
            .get_server_policy()
            .await
            .expect("Failed to get server policy");

        assert_eq!(server_policy.id, 1);

        let updated_server_policy = storage
            .update_server_policy(UpdateServerRetentionPolicyRequest {
                max_lifetime: Some(259_200_000),
                min_lifetime: Some(10_800_000),
                expire_on_clients: Some(true),
            })
            .await
            .expect("Failed to update server policy");

        assert_eq!(updated_server_policy.max_lifetime, Some(259_200_000));
        assert_eq!(updated_server_policy.min_lifetime, 10_800_000);
        assert!(updated_server_policy.expire_on_clients);

        let effective_policy = storage
            .get_effective_policy(&room_id)
            .await
            .expect("Failed to get effective policy");

        assert_eq!(effective_policy.max_lifetime, Some(172_800_000));
        assert_eq!(effective_policy.min_lifetime, 7_200_000);
        assert!(!effective_policy.expire_on_clients);

        cleanup(&pool, &room_id, &creator).await;
    }
}
