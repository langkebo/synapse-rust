#![cfg(test)]
#![allow(clippy::module_inception)]

mod db_schema_smoke_tests {
    use crate::common::get_test_pool_async;
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};
    use synapse_rust::e2ee::device_trust::models::DeviceTrustLevel;
    use synapse_rust::e2ee::device_trust::storage::DeviceTrustStorage;
    use synapse_rust::e2ee::verification::models::{
        QrState, SasState, VerificationMethod, VerificationRequest, VerificationState,
    };
    use synapse_rust::e2ee::verification::storage::VerificationStorage;
    use synapse_rust::storage::moderation::{
        CreateModerationRuleParams, ModerationAction, ModerationLogStorage, ModerationRuleType,
        ModerationStorage,
    };
    use synapse_rust::storage::retention::RetentionStorage;
    use synapse_rust::storage::room_summary::RoomSummaryStorage;
    use synapse_rust::storage::space::SpaceStorage;
    use synapse_rust::worker::storage::{UpdateConnectionStatsRequest, WorkerStorage};
    use synapse_rust::worker::types::{
        AssignTaskRequest, RegisterWorkerRequest, WorkerLoadStatsUpdate, WorkerType,
    };

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

    fn unique_id() -> u64 {
        TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
    }

    async fn connect_pool() -> Option<std::sync::Arc<sqlx::PgPool>> {
        match get_test_pool_async().await {
            Ok(pool) => Some(pool),
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
        let regclass: Option<String> = sqlx::query_scalar("SELECT to_regclass($1)::text")
            .bind(format!("public.{table_name}"))
            .fetch_one(pool)
            .await
            .expect("Failed to query table existence");
        assert!(
            regclass.as_deref() == Some(table_name)
                || regclass.as_deref() == Some(format!("public.{table_name}").as_str()),
            "Expected table '{table_name}' to exist, got: {:?}",
            regclass
        );
    }

    async fn assert_view_exists(pool: &sqlx::PgPool, view_name: &str) {
        let regclass: Option<String> = sqlx::query_scalar("SELECT to_regclass($1)::text")
            .bind(format!("public.{view_name}"))
            .fetch_one(pool)
            .await
            .expect("Failed to query view existence");
        assert!(
            regclass.as_deref() == Some(view_name)
                || regclass.as_deref() == Some(format!("public.{view_name}").as_str()),
            "Expected view '{view_name}' to exist, got: {:?}",
            regclass
        );
    }

    async fn seed_room(pool: &sqlx::PgPool, suffix: u64, prefix: &str) -> (String, String) {
        let creator = format!("@{prefix}_creator_{suffix}:localhost");
        let room_id = format!("!{prefix}_room_{suffix}:localhost");

        sqlx::query(
            "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(&creator)
        .bind(format!("{prefix}_creator_{suffix}"))
        .bind(0_i64)
        .execute(pool)
        .await
        .expect("Failed to seed user");

        sqlx::query(
            "INSERT INTO rooms (room_id, creator, created_ts) VALUES ($1, $2, $3) ON CONFLICT (room_id) DO NOTHING",
        )
        .bind(&room_id)
        .bind(&creator)
        .bind(0_i64)
        .execute(pool)
        .await
        .expect("Failed to seed room");

        (creator, room_id)
    }

    async fn cleanup_room(pool: &sqlx::PgPool, room_id: &str, creator: &str) {
        sqlx::query("DELETE FROM room_children WHERE parent_room_id = $1 OR child_room_id = $1")
            .bind(room_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup room_children");
        sqlx::query("DELETE FROM room_summary_update_queue WHERE room_id = $1")
            .bind(room_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup room_summary_update_queue");
        sqlx::query("DELETE FROM room_summary_stats WHERE room_id = $1")
            .bind(room_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup room_summary_stats");
        sqlx::query("DELETE FROM room_summary_state WHERE room_id = $1")
            .bind(room_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup room_summary_state");
        sqlx::query("DELETE FROM deleted_events_index WHERE room_id = $1")
            .bind(room_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup deleted_events_index");
        sqlx::query("DELETE FROM retention_stats WHERE room_id = $1")
            .bind(room_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup retention_stats");
        sqlx::query("DELETE FROM retention_cleanup_logs WHERE room_id = $1")
            .bind(room_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup retention_cleanup_logs");
        sqlx::query("DELETE FROM retention_cleanup_queue WHERE room_id = $1")
            .bind(room_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup retention_cleanup_queue");
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

    #[tokio::test]
    async fn test_retention_and_room_summary_schema_smoke_roundtrip() {
        let pool = match connect_pool().await {
            Some(pool) => pool,
            None => return,
        };

        for table_name in [
            "retention_cleanup_queue",
            "retention_cleanup_logs",
            "retention_stats",
            "deleted_events_index",
            "room_summary_state",
            "room_summary_stats",
            "room_summary_update_queue",
            "room_children",
        ] {
            assert_table_exists(&pool, table_name).await;
        }

        let retention_storage = RetentionStorage::new(&pool);
        let summary_storage = RoomSummaryStorage::new(&pool);
        let suffix = unique_id();
        let (creator, room_id) = seed_room(&pool, suffix, "schema_smoke").await;
        let (_, child_room_id) = seed_room(&pool, suffix + 10_000, "schema_child").await;

        retention_storage
            .queue_cleanup(&room_id, "$cleanup_event", "m.room.message", 123_i64)
            .await
            .expect("Failed to queue cleanup");
        retention_storage
            .update_stats(&room_id, 10, 8, 2, Some(500))
            .await
            .expect("Failed to update retention stats");
        retention_storage
            .record_deleted_event(&room_id, "$cleanup_event", "retention")
            .await
            .expect("Failed to record deleted event");
        let cleanup_log = retention_storage
            .create_cleanup_log(&room_id)
            .await
            .expect("Failed to create cleanup log");
        retention_storage
            .complete_cleanup_log(cleanup_log.id, 2, 1, 0, 128)
            .await
            .expect("Failed to complete cleanup log");

        let pending_count = retention_storage
            .get_pending_cleanup_count(&room_id)
            .await
            .expect("Failed to count cleanup queue");
        assert_eq!(pending_count, 1);

        let stats = retention_storage
            .get_stats(&room_id)
            .await
            .expect("Failed to load retention stats")
            .expect("Retention stats should exist");
        assert_eq!(stats.events_expired, 2);

        let deleted_events = retention_storage
            .get_deleted_events(&room_id, 0)
            .await
            .expect("Failed to load deleted events");
        assert_eq!(deleted_events.len(), 1);

        let state = summary_storage
            .set_state(
                &room_id,
                "m.room.name",
                "",
                Some("$state_event"),
                json!({"name": "Schema Smoke"}),
            )
            .await
            .expect("Failed to set room summary state");
        assert_eq!(state.room_id, room_id);

        let summary_stats = summary_storage
            .update_stats(&room_id, 42, 5, 37, 2, 2048)
            .await
            .expect("Failed to update room summary stats");
        assert_eq!(summary_stats.total_messages, 37);

        summary_storage
            .queue_update(&room_id, "$summary_event", "m.room.message", None, 9)
            .await
            .expect("Failed to queue room summary update");

        let pending_updates = summary_storage
            .get_pending_updates(10)
            .await
            .expect("Failed to load room summary updates");
        assert!(pending_updates.iter().any(|item| item.room_id == room_id));

        sqlx::query(
            "INSERT INTO room_children (parent_room_id, child_room_id, state_key, content, suggested, created_ts)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (parent_room_id, child_room_id) DO UPDATE SET content = EXCLUDED.content, suggested = EXCLUDED.suggested",
        )
        .bind(&room_id)
        .bind(&child_room_id)
        .bind("")
        .bind(json!({"via": ["localhost"]}))
        .bind(true)
        .bind(0_i64)
        .execute(&*pool)
        .await
        .expect("Failed to seed room_children");

        let child_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM room_children WHERE parent_room_id = $1 AND child_room_id = $2",
        )
        .bind(&room_id)
        .bind(&child_room_id)
        .fetch_one(&*pool)
        .await
        .expect("Failed to count room_children");
        assert_eq!(child_count, 1);

        cleanup_room(
            &pool,
            &child_room_id,
            &format!("@schema_child_creator_{}:localhost", suffix + 10_000),
        )
        .await;
        cleanup_room(&pool, &room_id, &creator).await;
    }

    #[tokio::test]
    async fn test_device_trust_schema_smoke_roundtrip() {
        let pool = match connect_pool().await {
            Some(pool) => pool,
            None => return,
        };

        for table_name in [
            "device_trust_status",
            "cross_signing_trust",
            "device_verification_request",
        ] {
            assert_table_exists(&pool, table_name).await;
        }

        let storage = DeviceTrustStorage::new(&pool);
        let suffix = unique_id();
        let user_id = format!("@device_trust_{suffix}:localhost");
        let device_id = format!("DEVICE_TRUST_{suffix}");
        let other_user_id = format!("@device_trust_peer_{suffix}:localhost");

        storage
            .set_device_trust(
                &user_id,
                &device_id,
                DeviceTrustLevel::Verified,
                Some("MASTER"),
            )
            .await
            .expect("Failed to set device trust");
        storage
            .set_cross_signing_trust(&user_id, &other_user_id, true)
            .await
            .expect("Failed to set cross signing trust");

        let trust = storage
            .get_device_trust(&user_id, &device_id)
            .await
            .expect("Failed to get device trust")
            .expect("Device trust should exist");
        assert_eq!(trust.trust_level, DeviceTrustLevel::Verified);

        let trusted_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM cross_signing_trust WHERE user_id = $1 AND target_user_id = $2 AND is_trusted = TRUE",
        )
        .bind(&user_id)
        .bind(&other_user_id)
        .fetch_one(&*pool)
        .await
        .expect("Failed to count cross signing trust");
        assert_eq!(trusted_count, 1);

        sqlx::query("DELETE FROM cross_signing_trust WHERE user_id = $1 AND target_user_id = $2")
            .bind(&user_id)
            .bind(&other_user_id)
            .execute(&*pool)
            .await
            .expect("Failed to cleanup cross_signing_trust");
        sqlx::query("DELETE FROM device_trust_status WHERE user_id = $1 AND device_id = $2")
            .bind(&user_id)
            .bind(&device_id)
            .execute(&*pool)
            .await
            .expect("Failed to cleanup device_trust_status");
    }

    #[tokio::test]
    async fn test_verification_and_moderation_schema_smoke_roundtrip() {
        let pool = match connect_pool().await {
            Some(pool) => pool,
            None => return,
        };

        for table_name in [
            "verification_requests",
            "verification_sas",
            "verification_qr",
            "moderation_actions",
            "moderation_rules",
            "moderation_logs",
        ] {
            assert_table_exists(&pool, table_name).await;
        }

        let verification_storage = VerificationStorage::new(&pool);
        let moderation_storage = ModerationStorage::new(pool.clone());
        let moderation_log_storage = ModerationLogStorage::new(pool.clone());
        let suffix = unique_id();
        let tx_id = format!("txn_{suffix}");
        let request = VerificationRequest {
            transaction_id: tx_id.clone(),
            from_user: format!("@verify_from_{suffix}:localhost"),
            from_device: format!("VERIFY_FROM_{suffix}"),
            to_user: format!("@verify_to_{suffix}:localhost"),
            to_device: Some(format!("VERIFY_TO_{suffix}")),
            method: VerificationMethod::Sas,
            state: VerificationState::Requested,
            created_ts: 0,
            updated_ts: 0,
        };

        verification_storage
            .create_request(&request)
            .await
            .expect("Failed to create verification request");
        verification_storage
            .store_sas_state(&SasState {
                tx_id: tx_id.clone(),
                from_device: request.from_device.clone(),
                to_device: request.to_device.clone(),
                method: VerificationMethod::Sas,
                state: VerificationState::Ready,
                exchange_hashes: vec!["sha256".to_string()],
                commitment: Some("commitment".to_string()),
                pubkey: Some("pubkey".to_string()),
                sas_bytes: Some(vec![1, 2, 3]),
                mac: Some("mac".to_string()),
            })
            .await
            .expect("Failed to store SAS state");
        verification_storage
            .store_qr_state(&QrState {
                tx_id: tx_id.clone(),
                from_device: request.from_device.clone(),
                to_device: request.to_device.clone(),
                state: VerificationState::Pending,
                qr_code_data: Some("qr-data".to_string()),
                scanned_data: Some("scanned-data".to_string()),
            })
            .await
            .expect("Failed to store QR state");

        let loaded_request = verification_storage
            .get_request(&tx_id)
            .await
            .expect("Failed to load verification request")
            .expect("Verification request should exist");
        assert_eq!(loaded_request.transaction_id, tx_id);

        let (creator, room_id) = seed_room(&pool, suffix + 20_000, "moderation_smoke").await;
        let created_rule = moderation_storage
            .create_rule(CreateModerationRuleParams {
                rule_type: ModerationRuleType::Keyword,
                pattern: "forbidden".to_string(),
                action: ModerationAction::Flag,
                reason: Some("schema smoke".to_string()),
                created_by: creator.clone(),
                server_id: Some("localhost".to_string()),
                priority: Some(50),
            })
            .await
            .expect("Failed to create moderation rule");
        moderation_log_storage
            .log_action(
                &created_rule.rule_id,
                "$moderation_event",
                &room_id,
                &creator,
                "content-hash",
                "flag",
                0.9,
            )
            .await
            .expect("Failed to create moderation log");

        sqlx::query(
            "INSERT INTO moderation_actions (user_id, action_type, reason, report_id, created_ts, expires_at) VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(&creator)
        .bind("warn")
        .bind("schema smoke")
        .bind(1_i64)
        .bind(0_i64)
        .bind(1_000_i64)
        .execute(&*pool)
        .await
        .expect("Failed to create moderation action");

        let room_logs = moderation_log_storage
            .get_logs_for_room(&room_id, 10)
            .await
            .expect("Failed to fetch moderation logs");
        assert_eq!(room_logs.len(), 1);

        let moderation_action_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM moderation_actions WHERE user_id = $1 AND action_type = $2",
        )
        .bind(&creator)
        .bind("warn")
        .fetch_one(&*pool)
        .await
        .expect("Failed to count moderation actions");
        assert_eq!(moderation_action_count, 1);

        sqlx::query("DELETE FROM moderation_actions WHERE user_id = $1")
            .bind(&creator)
            .execute(&*pool)
            .await
            .expect("Failed to cleanup moderation_actions");
        sqlx::query("DELETE FROM moderation_logs WHERE rule_id = $1")
            .bind(&created_rule.rule_id)
            .execute(&*pool)
            .await
            .expect("Failed to cleanup moderation_logs");
        sqlx::query("DELETE FROM moderation_rules WHERE rule_id = $1")
            .bind(&created_rule.rule_id)
            .execute(&*pool)
            .await
            .expect("Failed to cleanup moderation_rules");
        sqlx::query("DELETE FROM verification_qr WHERE tx_id = $1")
            .bind(&tx_id)
            .execute(&*pool)
            .await
            .expect("Failed to cleanup verification_qr");
        sqlx::query("DELETE FROM verification_sas WHERE tx_id = $1")
            .bind(&tx_id)
            .execute(&*pool)
            .await
            .expect("Failed to cleanup verification_sas");
        sqlx::query("DELETE FROM verification_requests WHERE transaction_id = $1")
            .bind(&tx_id)
            .execute(&*pool)
            .await
            .expect("Failed to cleanup verification_requests");
        cleanup_room(&pool, &room_id, &creator).await;
    }

    #[tokio::test]
    async fn test_worker_schema_smoke_roundtrip() {
        let pool = match connect_pool().await {
            Some(pool) => pool,
            None => return,
        };

        for table_name in [
            "replication_positions",
            "worker_load_stats",
            "worker_task_assignments",
            "worker_connections",
        ] {
            assert_table_exists(&pool, table_name).await;
        }
        for view_name in ["active_workers", "worker_type_statistics"] {
            assert_view_exists(&pool, view_name).await;
        }

        let storage = WorkerStorage::new(&pool);
        let suffix = unique_id();
        let worker_id = format!("worker_{suffix}");
        let peer_worker_id = format!("worker_peer_{suffix}");

        storage
            .register_worker(RegisterWorkerRequest {
                worker_id: worker_id.clone(),
                worker_name: format!("worker-name-{suffix}"),
                worker_type: WorkerType::Frontend,
                host: "127.0.0.1".to_string(),
                port: 7000,
                config: None,
                metadata: None,
                version: Some("smoke".to_string()),
            })
            .await
            .expect("Failed to register worker");
        storage
            .register_worker(RegisterWorkerRequest {
                worker_id: peer_worker_id.clone(),
                worker_name: format!("peer-worker-{suffix}"),
                worker_type: WorkerType::Background,
                host: "127.0.0.1".to_string(),
                port: 7001,
                config: None,
                metadata: None,
                version: Some("smoke".to_string()),
            })
            .await
            .expect("Failed to register peer worker");

        storage
            .update_worker_status(&worker_id, "running")
            .await
            .expect("Failed to update worker status");
        storage
            .record_load_stats(
                &worker_id,
                &WorkerLoadStatsUpdate {
                    cpu_usage: Some(0.5),
                    memory_usage: Some(1024),
                    active_connections: Some(4),
                    requests_per_second: Some(12.0),
                    average_latency_ms: Some(8.0),
                    queue_depth: Some(1),
                },
            )
            .await
            .expect("Failed to record load stats");
        storage
            .update_replication_position(&worker_id, "events", 42)
            .await
            .expect("Failed to update replication position");
        storage
            .assign_task(AssignTaskRequest {
                task_type: "smoke".to_string(),
                task_data: json!({"kind": "schema"}),
                priority: Some(5),
                preferred_worker_id: None,
            })
            .await
            .expect("Failed to assign task");
        storage
            .record_connection(&worker_id, &peer_worker_id, "tcp")
            .await
            .expect("Failed to record connection");
        storage
            .update_connection_stats(
                &UpdateConnectionStatsRequest::new(&worker_id, &peer_worker_id, "tcp")
                    .bytes_sent(10)
                    .bytes_received(20)
                    .messages_sent(1)
                    .messages_received(2),
            )
            .await
            .expect("Failed to update connection stats");

        let active_workers = storage
            .get_active_workers()
            .await
            .expect("Failed to load active workers");
        assert!(active_workers
            .iter()
            .any(|worker| worker.worker_id == worker_id));

        let position = storage
            .get_replication_position(&worker_id, "events")
            .await
            .expect("Failed to load replication position");
        assert_eq!(position, Some(42));

        let type_statistics = storage
            .get_type_statistics()
            .await
            .expect("Failed to load worker type statistics");
        assert!(type_statistics
            .iter()
            .any(|item| item["worker_type"] == "frontend"));

        let connection_stats: (i64, i64) = sqlx::query_as(
            "SELECT bytes_sent, messages_received FROM worker_connections WHERE source_worker_id = $1 AND target_worker_id = $2 AND connection_type = $3",
        )
        .bind(&worker_id)
        .bind(&peer_worker_id)
        .bind("tcp")
        .fetch_one(&*pool)
        .await
        .expect("Failed to fetch worker connection stats");
        assert_eq!(connection_stats, (10, 2));

        sqlx::query("DELETE FROM worker_connections WHERE source_worker_id = $1 OR target_worker_id = $1 OR source_worker_id = $2 OR target_worker_id = $2")
            .bind(&worker_id)
            .bind(&peer_worker_id)
            .execute(&*pool)
            .await
            .expect("Failed to cleanup worker_connections");
        sqlx::query("DELETE FROM worker_task_assignments WHERE task_type = 'smoke'")
            .execute(&*pool)
            .await
            .expect("Failed to cleanup worker_task_assignments");
        sqlx::query("DELETE FROM worker_load_stats WHERE worker_id = $1 OR worker_id = $2")
            .bind(&worker_id)
            .bind(&peer_worker_id)
            .execute(&*pool)
            .await
            .expect("Failed to cleanup worker_load_stats");
        sqlx::query("DELETE FROM replication_positions WHERE worker_id = $1 OR worker_id = $2")
            .bind(&worker_id)
            .bind(&peer_worker_id)
            .execute(&*pool)
            .await
            .expect("Failed to cleanup replication_positions");
        sqlx::query("DELETE FROM workers WHERE worker_id = $1 OR worker_id = $2")
            .bind(&worker_id)
            .bind(&peer_worker_id)
            .execute(&*pool)
            .await
            .expect("Failed to cleanup workers");
    }
}
