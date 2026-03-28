#![cfg(test)]

mod db_schema_smoke_tests {
    use serde_json::json;
    use sqlx::postgres::PgPoolOptions;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;
    use synapse_rust::storage::space::SpaceStorage;

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

    fn unique_id() -> u64 {
        TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
    }

    async fn connect_pool() -> Option<Arc<sqlx::PgPool>> {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .unwrap_or_else(|_| "postgresql://synapse:synapse@localhost:5432/synapse_test".to_string());

        match PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(10))
            .connect(&database_url)
            .await
        {
            Ok(pool) => Some(Arc::new(pool)),
            Err(error) => {
                eprintln!(
                    "Skipping db schema smoke tests because test database is unavailable: {}",
                    error
                );
                None
            }
        }
    }

    async fn assert_table_exists(pool: &sqlx::PgPool, table_name: &str) {
        let regclass: Option<String> =
            sqlx::query_scalar("SELECT to_regclass($1)::text")
                .bind(format!("public.{table_name}"))
                .fetch_one(pool)
                .await
                .expect("Failed to query table existence");
        assert_eq!(regclass.as_deref(), Some(format!("public.{table_name}").as_str()));
    }

    async fn seed_space(pool: &sqlx::PgPool, suffix: u64) -> (String, String) {
        let creator = format!("@spacecreator_{suffix}:localhost");
        let space_id = format!("!space_{suffix}:localhost");

        sqlx::query(
            "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(&creator)
        .bind(format!("spacecreator_{suffix}"))
        .bind(0_i64)
        .execute(pool)
        .await
        .expect("Failed to seed creator");

        sqlx::query(
            "INSERT INTO spaces (space_id, room_id, creator, join_rule, visibility, is_public, created_ts) VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT (space_id) DO NOTHING",
        )
        .bind(&space_id)
        .bind(&space_id)
        .bind(&creator)
        .bind("invite")
        .bind("private")
        .bind(false)
        .bind(0_i64)
        .execute(pool)
        .await
        .expect("Failed to seed space");

        (creator, space_id)
    }

    async fn cleanup_space(pool: &sqlx::PgPool, creator: &str, space_id: &str) {
        sqlx::query("DELETE FROM space_events WHERE space_id = $1")
            .bind(space_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup space_events");

        sqlx::query("DELETE FROM space_statistics WHERE space_id = $1")
            .bind(space_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup space_statistics");

        sqlx::query("DELETE FROM space_summaries WHERE space_id = $1")
            .bind(space_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup space_summaries");

        sqlx::query("DELETE FROM spaces WHERE space_id = $1")
            .bind(space_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup spaces");

        sqlx::query("DELETE FROM users WHERE user_id = $1")
            .bind(creator)
            .execute(pool)
            .await
            .expect("Failed to cleanup creator");
    }

    #[tokio::test]
    async fn test_space_schema_smoke_roundtrip() {
        let pool = match connect_pool().await {
            Some(pool) => pool,
            None => return,
        };

        for table_name in [
            "room_retention_policies",
            "room_summary_members",
            "space_members",
            "space_summaries",
            "space_statistics",
            "space_events",
        ] {
            assert_table_exists(&pool, table_name).await;
        }

        let storage = SpaceStorage::new(&pool);
        let suffix = unique_id();
        let (creator, space_id) = seed_space(&pool, suffix).await;

        sqlx::query(
            "INSERT INTO space_summaries (space_id, summary, children_count, member_count, updated_ts) VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (space_id) DO UPDATE SET summary = EXCLUDED.summary, children_count = EXCLUDED.children_count, member_count = EXCLUDED.member_count, updated_ts = EXCLUDED.updated_ts",
        )
        .bind(&space_id)
        .bind(json!({"children_count": 1, "member_count": 2}))
        .bind(1_i64)
        .bind(2_i64)
        .bind(100_i64)
        .execute(&*pool)
        .await
        .expect("Failed to seed space summary");

        let summary = storage
            .get_space_summary(&space_id)
            .await
            .expect("Failed to load space summary")
            .expect("Space summary should exist");
        assert_eq!(summary.space_id, space_id);
        assert_eq!(summary.children_count, 1);
        assert_eq!(summary.member_count, 2);

        sqlx::query(
            "INSERT INTO space_statistics (space_id, name, is_public, child_room_count, member_count, created_ts, updated_ts) VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (space_id) DO UPDATE SET name = EXCLUDED.name, is_public = EXCLUDED.is_public, child_room_count = EXCLUDED.child_room_count, member_count = EXCLUDED.member_count, updated_ts = EXCLUDED.updated_ts",
        )
        .bind(&space_id)
        .bind("Space Smoke")
        .bind(false)
        .bind(1_i64)
        .bind(2_i64)
        .bind(0_i64)
        .bind(100_i64)
        .execute(&*pool)
        .await
        .expect("Failed to seed space statistics");

        let statistics = storage
            .get_space_statistics()
            .await
            .expect("Failed to load space statistics");
        assert!(statistics.iter().any(|item| item["space_id"] == space_id));

        let event = storage
            .add_space_event(
                "$space_event_smoke",
                &space_id,
                "m.space.child",
                &creator,
                json!({"via": ["localhost"]}),
                Some(""),
            )
            .await
            .expect("Failed to add space event");
        assert_eq!(event.space_id, space_id);

        let events = storage
            .get_space_events(&space_id, Some("m.space.child"), 10)
            .await
            .expect("Failed to get space events");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_id, "$space_event_smoke");

        cleanup_space(&pool, &creator, &space_id).await;
    }
}
