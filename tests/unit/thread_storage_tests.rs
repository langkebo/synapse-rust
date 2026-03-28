#![cfg(test)]

mod thread_storage_tests {
    use sqlx::postgres::PgPoolOptions;
    use sqlx::Row;
    use std::sync::Arc;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use synapse_rust::storage::thread::{CreateThreadReplyParams, CreateThreadRootParams, ThreadStorage};

    fn unique_suffix() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }

    async fn connect_pool() -> Arc<sqlx::PgPool> {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .unwrap_or_else(|_| "postgresql://synapse:synapse@localhost:5432/synapse_test".to_string());

        Arc::new(
            PgPoolOptions::new()
                .max_connections(5)
                .acquire_timeout(Duration::from_secs(10))
                .connect(&database_url)
                .await
                .expect("Failed to connect to test database"),
        )
    }

    async fn seed_room(pool: &sqlx::PgPool, suffix: u128) -> (String, String, String, String, String) {
        let creator = format!("@threadcreator{suffix}:localhost");
        let replier = format!("@threadreplier{suffix}:localhost");
        let reader = format!("@threadreader{suffix}:localhost");
        let room_id = format!("!threadroom{suffix}:localhost");
        let thread_id = format!("thread-{suffix}");

        sqlx::query("INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING")
            .bind(&creator)
            .bind(format!("threadcreator{suffix}"))
            .bind(0_i64)
            .execute(pool)
            .await
            .expect("Failed to seed creator");

        sqlx::query("INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING")
            .bind(&replier)
            .bind(format!("threadreplier{suffix}"))
            .bind(0_i64)
            .execute(pool)
            .await
            .expect("Failed to seed replier");

        sqlx::query("INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING")
            .bind(&reader)
            .bind(format!("threadreader{suffix}"))
            .bind(0_i64)
            .execute(pool)
            .await
            .expect("Failed to seed reader");

        sqlx::query("INSERT INTO rooms (room_id, creator, created_ts) VALUES ($1, $2, $3) ON CONFLICT (room_id) DO NOTHING")
            .bind(&room_id)
            .bind(&creator)
            .bind(0_i64)
            .execute(pool)
            .await
            .expect("Failed to seed room");

        (creator, replier, reader, room_id, thread_id)
    }

    async fn cleanup(pool: &sqlx::PgPool, room_id: &str, users: &[String]) {
        sqlx::query("DELETE FROM thread_read_receipts WHERE room_id = $1")
            .bind(room_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup thread_read_receipts");

        sqlx::query("DELETE FROM thread_replies WHERE room_id = $1")
            .bind(room_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup thread_replies");

        sqlx::query("DELETE FROM thread_roots WHERE room_id = $1")
            .bind(room_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup thread_roots");

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
    async fn test_thread_root_and_reply_roundtrip() {
        let pool = connect_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = unique_suffix();
        let (creator, replier, reader, room_id, thread_id) = seed_room(&pool, suffix).await;
        let root_event_id = format!("$root{suffix}:localhost");
        let reply_event_id = format!("$reply{suffix}:localhost");

        let root = storage
            .create_thread_root(CreateThreadRootParams {
                room_id: room_id.clone(),
                root_event_id: root_event_id.clone(),
                sender: creator.clone(),
                thread_id: Some(thread_id.clone()),
            })
            .await
            .expect("Failed to create thread root");

        assert_eq!(root.root_event_id, root_event_id);
        assert_eq!(root.participants, Some(serde_json::json!([creator.clone()])));

        let loaded = storage
            .get_thread_root_by_event(&room_id, &root_event_id)
            .await
            .expect("Failed to load thread root")
            .expect("Thread root should exist");

        assert_eq!(loaded.thread_id.as_deref(), Some(thread_id.as_str()));

        let reply = storage
            .create_thread_reply(CreateThreadReplyParams {
                room_id: room_id.clone(),
                thread_id: thread_id.clone(),
                event_id: reply_event_id.clone(),
                root_event_id: root_event_id.clone(),
                sender: replier.clone(),
                in_reply_to_event_id: Some(root_event_id.clone()),
                content: serde_json::json!({ "body": "hello", "msgtype": "m.text" }),
                origin_server_ts: 1234,
            })
            .await
            .expect("Failed to create thread reply");

        assert_eq!(reply.root_event_id, root_event_id);
        assert_eq!(reply.event_id, reply_event_id);

        let relation = storage
            .create_thread_relation(
                &room_id,
                &reply_event_id,
                &root_event_id,
                "m.thread",
                Some(&thread_id),
                false,
            )
            .await
            .expect("Failed to create thread relation");

        assert_eq!(relation.event_id, reply_event_id);
        assert_eq!(relation.relates_to_event_id, root_event_id);
        assert_eq!(relation.thread_id.as_deref(), Some(thread_id.as_str()));

        let relation_row = sqlx::query(
            "SELECT event_id, relates_to_event_id, relation_type, thread_id FROM thread_relations WHERE room_id = $1 AND event_id = $2",
        )
        .bind(&room_id)
        .bind(&reply_event_id)
        .fetch_one(&*pool)
        .await
        .expect("Failed to query thread relation");

        let relation_event_id: String = relation_row.get("event_id");
        let relation_root_event_id: String = relation_row.get("relates_to_event_id");
        let relation_type: String = relation_row.get("relation_type");
        let relation_thread_id: Option<String> = relation_row.get("thread_id");

        assert_eq!(relation_event_id, reply_event_id);
        assert_eq!(relation_root_event_id, root_event_id);
        assert_eq!(relation_type, "m.thread");
        assert_eq!(relation_thread_id.as_deref(), Some(thread_id.as_str()));

        let replies = storage
            .get_thread_replies(&room_id, &thread_id, Some(10), None)
            .await
            .expect("Failed to list thread replies");

        assert_eq!(replies.len(), 1);
        assert_eq!(replies[0].event_id, reply_event_id);

        let reply_count = storage
            .get_reply_count(&room_id, &thread_id)
            .await
            .expect("Failed to get reply count");

        assert_eq!(reply_count, 1);

        let participants = storage
            .get_thread_participants(&room_id, &thread_id)
            .await
            .expect("Failed to get participants");

        assert!(participants.contains(&creator));
        assert!(participants.contains(&replier));

        let updated_root = storage
            .get_thread_root(&room_id, &thread_id)
            .await
            .expect("Failed to load thread root by thread id")
            .expect("Thread root should exist");

        assert_eq!(updated_root.reply_count, 1);
        assert_eq!(updated_root.last_reply_event_id.as_deref(), Some(reply_event_id.as_str()));
        assert_eq!(updated_root.last_reply_sender.as_deref(), Some(replier.as_str()));
        assert_eq!(updated_root.last_reply_ts, Some(1234));
        assert_eq!(
            updated_root.participants,
            Some(serde_json::json!([creator.clone(), replier.clone()]))
        );

        let summary = storage
            .get_thread_summary(&room_id, &thread_id)
            .await
            .expect("Failed to load thread summary")
            .expect("Thread summary should exist");

        assert_eq!(summary.thread_id, thread_id);
        assert_eq!(summary.latest_event_id.as_deref(), Some(reply_event_id.as_str()));
        assert_eq!(summary.reply_count, 1);

        let statistics = storage
            .get_thread_statistics(&room_id, &thread_id)
            .await
            .expect("Failed to load thread statistics")
            .expect("Thread statistics should exist");

        assert_eq!(statistics.total_replies, 1);
        assert_eq!(statistics.total_participants, 2);
        assert_eq!(statistics.total_edits, 0);
        assert_eq!(statistics.total_redactions, 0);
        assert_eq!(statistics.first_reply_ts, Some(1234));
        assert_eq!(statistics.last_reply_ts, Some(1234));

        let search_results = storage
            .search_threads(&room_id, "hello", Some(10))
            .await
            .expect("Failed to search threads");

        assert_eq!(search_results.len(), 1);
        assert_eq!(search_results[0].thread_id, thread_id);

        cleanup(&pool, &room_id, &[creator, replier, reader]).await;
    }

    #[tokio::test]
    async fn test_thread_read_receipt_roundtrip() {
        let pool = connect_pool().await;
        let storage = ThreadStorage::new(&pool);
        let suffix = unique_suffix();
        let (creator, replier, reader, room_id, thread_id) = seed_room(&pool, suffix).await;
        let root_event_id = format!("$rootreceipt{suffix}:localhost");

        storage
            .create_thread_root(CreateThreadRootParams {
                room_id: room_id.clone(),
                root_event_id: root_event_id.clone(),
                sender: creator.clone(),
                thread_id: Some(thread_id.clone()),
            })
            .await
            .expect("Failed to create thread root");

        let receipt = storage
            .update_read_receipt(&room_id, &thread_id, &reader, &root_event_id, 5555)
            .await
            .expect("Failed to update read receipt");

        assert_eq!(receipt.last_read_event_id.as_deref(), Some(root_event_id.as_str()));
        assert_eq!(receipt.unread_count, 0);

        storage
            .increment_unread_count(&room_id, &thread_id, &reader)
            .await
            .expect("Failed to increment unread count");

        let loaded = storage
            .get_read_receipt(&room_id, &thread_id, &reader)
            .await
            .expect("Failed to load read receipt")
            .expect("Read receipt should exist");

        assert_eq!(loaded.unread_count, 1);

        let unread = storage
            .get_threads_with_unread(&reader, Some(&room_id))
            .await
            .expect("Failed to list unread threads");

        assert_eq!(unread.len(), 1);
        assert_eq!(unread[0].thread_id, thread_id);

        let row = sqlx::query("SELECT root_event_id FROM thread_roots WHERE room_id = $1 AND thread_id = $2")
            .bind(&room_id)
            .bind(&thread_id)
            .fetch_one(&*pool)
            .await
            .expect("Failed to query root_event_id");

        let stored_root_event_id: String = row.get("root_event_id");
        assert_eq!(stored_root_event_id, root_event_id);

        cleanup(&pool, &room_id, &[creator, replier, reader]).await;
    }
}
