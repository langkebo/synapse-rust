#![allow(clippy::unwrap_used)]

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Instant;

    use serde_json::json;
    use sqlx::PgPool;
    use synapse_rust::services::application_service::{ApplicationServiceManager, ApplicationServiceScheduler};
    use synapse_rust::storage::application_service::{ApplicationServiceStorage, RegisterApplicationServiceRequest};
    use synapse_rust::storage::EventStorage;
    use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

    #[derive(Clone, Copy)]
    enum ScenarioKind {
        EventOnly,
        TransactionOnly,
        Mixed,
    }

    impl ScenarioKind {
        fn as_str(self) -> &'static str {
            match self {
                Self::EventOnly => "event_only",
                Self::TransactionOnly => "transaction_only",
                Self::Mixed => "mixed",
            }
        }
    }

    async fn setup_appservice_perf_tables(pool: &PgPool) {
        let statements = [
            r#"
            CREATE TABLE IF NOT EXISTS application_services (
                id BIGSERIAL PRIMARY KEY,
                as_id TEXT NOT NULL UNIQUE,
                url TEXT NOT NULL,
                as_token TEXT NOT NULL,
                hs_token TEXT NOT NULL,
                sender_localpart TEXT NOT NULL,
                is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
                is_rate_limited BOOLEAN NOT NULL DEFAULT FALSE,
                protocols TEXT[] NOT NULL DEFAULT '{}',
                namespaces JSONB NOT NULL DEFAULT '{"users":[],"aliases":[],"rooms":[]}',
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT,
                description TEXT,
                api_key TEXT,
                config JSONB NOT NULL DEFAULT '{}'
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS application_service_user_namespaces (
                id BIGSERIAL PRIMARY KEY,
                as_id TEXT NOT NULL,
                namespace TEXT NOT NULL,
                is_exclusive BOOLEAN NOT NULL DEFAULT FALSE,
                created_ts BIGINT NOT NULL
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS application_service_room_alias_namespaces (
                id BIGSERIAL PRIMARY KEY,
                as_id TEXT NOT NULL,
                namespace TEXT NOT NULL,
                is_exclusive BOOLEAN NOT NULL DEFAULT FALSE,
                created_ts BIGINT NOT NULL
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS application_service_room_namespaces (
                id BIGSERIAL PRIMARY KEY,
                as_id TEXT NOT NULL,
                namespace TEXT NOT NULL,
                is_exclusive BOOLEAN NOT NULL DEFAULT FALSE,
                created_ts BIGINT NOT NULL
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS application_service_users (
                as_id TEXT NOT NULL,
                user_id TEXT NOT NULL,
                displayname TEXT,
                avatar_url TEXT,
                created_ts BIGINT NOT NULL,
                PRIMARY KEY (as_id, user_id)
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS application_service_events (
                id BIGSERIAL PRIMARY KEY,
                event_id TEXT NOT NULL UNIQUE,
                as_id TEXT NOT NULL,
                room_id TEXT,
                event_type TEXT,
                is_processed BOOLEAN NOT NULL DEFAULT FALSE,
                processed_ts BIGINT,
                created_ts BIGINT NOT NULL
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS application_service_transactions (
                id BIGSERIAL PRIMARY KEY,
                as_id TEXT NOT NULL,
                txn_id TEXT NOT NULL UNIQUE,
                transaction_id TEXT,
                data JSONB NOT NULL DEFAULT '{}',
                events JSONB,
                sent_ts BIGINT NOT NULL,
                is_processed BOOLEAN NOT NULL DEFAULT FALSE,
                processed_ts BIGINT,
                completed_ts BIGINT,
                retry_count INTEGER NOT NULL DEFAULT 0,
                last_error TEXT,
                created_ts BIGINT NOT NULL
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS application_service_statistics (
                as_id TEXT PRIMARY KEY,
                name TEXT,
                is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
                is_rate_limited BOOLEAN NOT NULL DEFAULT FALSE,
                virtual_user_count BIGINT NOT NULL DEFAULT 0,
                pending_event_count BIGINT NOT NULL DEFAULT 0,
                pending_transaction_count BIGINT NOT NULL DEFAULT 0,
                last_seen_ts BIGINT,
                created_ts BIGINT NOT NULL
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS application_service_state (
                id BIGSERIAL PRIMARY KEY,
                as_id TEXT NOT NULL,
                state_key TEXT NOT NULL,
                value JSONB NOT NULL,
                state_value TEXT,
                updated_ts BIGINT NOT NULL,
                CONSTRAINT uq_application_service_state_as_key UNIQUE (as_id, state_key)
            )
            "#,
        ];

        for statement in statements {
            sqlx::query(statement).execute(pool).await.unwrap();
        }
    }

    async fn seed_statistics_scenario(
        storage: &ApplicationServiceStorage,
        scenario: ScenarioKind,
        service_count: usize,
        pending_events_per_service: usize,
        pending_txns_per_service: usize,
    ) {
        for idx in 0..service_count {
            let as_id = format!("perf-{}-{}", scenario.as_str(), idx);
            storage
                .register(RegisterApplicationServiceRequest {
                    as_id: as_id.clone(),
                    url: format!("http://127.0.0.1:18{:03}", idx),
                    as_token: format!("as_token_{as_id}"),
                    hs_token: format!("hs_token_{as_id}"),
                    sender: format!("@perf_{}:localhost", idx),
                    description: Some(format!("perf {}", as_id)),
                    is_rate_limited: Some(false),
                    protocols: Some(vec![]),
                    namespaces: Some(json!({
                        "users": [],
                        "aliases": [],
                        "rooms": []
                    })),
                    api_key: None,
                    config: Some(json!({})),
                })
                .await
                .unwrap();

            let add_events = match scenario {
                ScenarioKind::EventOnly => true,
                ScenarioKind::TransactionOnly => false,
                ScenarioKind::Mixed => idx % 2 == 0,
            };

            let add_transactions = match scenario {
                ScenarioKind::EventOnly => false,
                ScenarioKind::TransactionOnly => true,
                ScenarioKind::Mixed => idx % 2 == 1,
            };

            if add_events {
                for event_idx in 0..pending_events_per_service {
                    storage
                        .add_event(
                            &format!("${as_id}-event-{event_idx}"),
                            &as_id,
                            &format!("!room-{as_id}:localhost"),
                            "m.room.message",
                            "@perf:localhost",
                            json!({"body": "perf"}),
                            None,
                        )
                        .await
                        .unwrap();
                }
            }

            if add_transactions {
                for txn_idx in 0..pending_txns_per_service {
                    storage
                        .create_transaction(
                            &as_id,
                            &format!("{as_id}-txn-{txn_idx}"),
                            &[json!({
                                "event_id": format!("${as_id}-txn-event-{txn_idx}"),
                                "type": "m.room.message"
                            })],
                        )
                        .await
                        .unwrap();
                }
            }
        }
    }

    async fn measure_get_statistics_latency(
        storage: &ApplicationServiceStorage,
        service_count: usize,
        iterations: usize,
    ) -> Vec<u128> {
        let mut latencies_ms = Vec::with_capacity(iterations);
        for _ in 0..iterations {
            let started_at = Instant::now();
            let rows = storage.get_statistics().await.unwrap();
            assert_eq!(rows.len(), service_count);
            latencies_ms.push(started_at.elapsed().as_millis());
        }
        latencies_ms.sort_unstable();
        latencies_ms
    }

    fn percentile(latencies_ms: &[u128], ratio: f64) -> u128 {
        let index = ((latencies_ms.len() as f64) * ratio) as usize;
        latencies_ms[index.min(latencies_ms.len().saturating_sub(1))]
    }

    fn create_test_appservice_manager(pool: &Arc<PgPool>) -> Arc<ApplicationServiceManager> {
        Arc::new(ApplicationServiceManager::new(
            Arc::new(ApplicationServiceStorage::new(pool)),
            Arc::new(EventStorage::new(pool, "localhost".to_string())),
            "localhost".to_string(),
        ))
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore]
    async fn appservice_statistics_load_smoke() {
        let pool: Arc<PgPool> = match synapse_rust::test_utils::prepare_isolated_test_pool().await {
            Ok(pool) => pool,
            Err(error) => {
                eprintln!("Skipping appservice statistics load smoke: {}", error);
                return;
            }
        };

        setup_appservice_perf_tables(pool.as_ref()).await;
        let storage = Arc::new(ApplicationServiceStorage::new(&pool));

        let service_count = 512;
        let event_backlog = 256;
        let transaction_backlog = 8;
        let iterations = 20;

        for scenario in [ScenarioKind::EventOnly, ScenarioKind::TransactionOnly, ScenarioKind::Mixed] {
            sqlx::query("TRUNCATE application_service_statistics, application_service_transactions, application_service_events, application_service_users, application_service_room_namespaces, application_service_room_alias_namespaces, application_service_user_namespaces, application_services RESTART IDENTITY CASCADE")
                .execute(pool.as_ref())
                .await
                .unwrap();

            seed_statistics_scenario(storage.as_ref(), scenario, service_count, event_backlog, transaction_backlog)
                .await;

            let latencies_ms = measure_get_statistics_latency(storage.as_ref(), service_count, iterations).await;
            let p50_ms = percentile(&latencies_ms, 0.50);
            let p95_ms = percentile(&latencies_ms, 0.95);
            let p99_ms = percentile(&latencies_ms, 0.99);
            let max_ms = *latencies_ms.last().unwrap_or(&0);

            println!(
                "APPSERVICE_STATS_PERF scenario={} services={} event_backlog={} transaction_backlog={} iterations={} p50={}ms p95={}ms p99={}ms max={}ms",
                scenario.as_str(),
                service_count,
                event_backlog,
                transaction_backlog,
                iterations,
                p50_ms,
                p95_ms,
                p99_ms,
                max_ms
            );
            println!(
                "PERF_SMOKE_JSON={}",
                json!({
                    "name": "appservice_statistics_load_smoke",
                    "scenario": scenario.as_str(),
                    "services": service_count,
                    "event_backlog_per_service": event_backlog,
                    "transaction_backlog_per_service": transaction_backlog,
                    "iterations": iterations,
                    "p50_ms": p50_ms,
                    "p95_ms": p95_ms,
                    "p99_ms": p99_ms,
                    "max_ms": max_ms
                })
            );
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore]
    async fn appservice_scheduler_mixed_backlog_load_smoke() {
        let pool: Arc<PgPool> = match synapse_rust::test_utils::prepare_isolated_test_pool().await {
            Ok(pool) => pool,
            Err(error) => {
                eprintln!("Skipping appservice scheduler mixed backlog smoke: {}", error);
                return;
            }
        };

        setup_appservice_perf_tables(pool.as_ref()).await;
        sqlx::query("TRUNCATE application_service_state, application_service_statistics, application_service_transactions, application_service_events, application_service_users, application_service_room_namespaces, application_service_room_alias_namespaces, application_service_user_namespaces, application_services RESTART IDENTITY CASCADE")
            .execute(pool.as_ref())
            .await
            .unwrap();

        let storage = ApplicationServiceStorage::new(&pool);
        let manager = create_test_appservice_manager(&pool);
        let scheduler = ApplicationServiceScheduler::with_capacity_options(manager.clone(), 50, 500, 8, 50, 2);

        let mock_server = MockServer::start().await;
        Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&mock_server).await;

        let transaction_service_count = 16usize;
        let event_service_count = 16usize;
        let pending_transactions_per_service = 2usize;
        let pending_events_per_service = 60usize;

        for idx in 0..transaction_service_count {
            let as_id = format!("perf-mixed-txn-{idx}");
            manager
                .register(RegisterApplicationServiceRequest {
                    as_id: as_id.clone(),
                    url: mock_server.uri(),
                    as_token: format!("as_token_{as_id}"),
                    hs_token: format!("hs_token_{as_id}"),
                    sender: format!("@txn_{idx}:localhost"),
                    description: Some(format!("perf mixed txn {idx}")),
                    is_rate_limited: Some(false),
                    protocols: None,
                    namespaces: Some(json!({
                        "users": [],
                        "aliases": [],
                        "rooms": [{"exclusive": true, "regex": format!("^!perf-mixed-txn-{idx}.*:localhost$")}]
                    })),
                    api_key: None,
                    config: None,
                })
                .await
                .unwrap();

            for txn_idx in 0..pending_transactions_per_service {
                storage
                    .create_transaction(
                        &as_id,
                        &format!("{as_id}-txn-{txn_idx}"),
                        &[json!({
                            "event_id": format!("${as_id}-txn-event-{txn_idx}"),
                            "type": "m.room.message"
                        })],
                    )
                    .await
                    .unwrap();
            }
        }

        for idx in 0..event_service_count {
            let as_id = format!("perf-mixed-event-{idx}");
            let room_id = format!("!perf-mixed-event-{idx}:localhost");
            manager
                .register(RegisterApplicationServiceRequest {
                    as_id: as_id.clone(),
                    url: mock_server.uri(),
                    as_token: format!("as_token_{as_id}"),
                    hs_token: format!("hs_token_{as_id}"),
                    sender: format!("@event_{idx}:localhost"),
                    description: Some(format!("perf mixed event {idx}")),
                    is_rate_limited: Some(false),
                    protocols: None,
                    namespaces: Some(json!({
                        "users": [],
                        "aliases": [],
                        "rooms": [{"exclusive": true, "regex": format!("^!perf-mixed-event-{idx}.*:localhost$")}]
                    })),
                    api_key: None,
                    config: None,
                })
                .await
                .unwrap();

            for event_idx in 0..pending_events_per_service {
                manager
                    .push_event(
                        &as_id,
                        &room_id,
                        "m.room.message",
                        "@bridge:localhost",
                        json!({"msgtype": "m.text", "body": format!("event-{idx}-{event_idx}")}),
                        None,
                    )
                    .await
                    .unwrap();
            }
        }

        let expected_total_requests =
            transaction_service_count * pending_transactions_per_service + event_service_count * 2;
        let started_at = Instant::now();
        let mut tick_latencies_ms = Vec::new();
        let mut tick_count = 0usize;
        let mut requests_after_each_tick = Vec::new();

        loop {
            let mut remaining_transactions = 0_i64;
            for idx in 0..transaction_service_count {
                remaining_transactions +=
                    storage.count_pending_transactions(&format!("perf-mixed-txn-{idx}")).await.unwrap();
            }

            let mut remaining_event_services = 0usize;
            for idx in 0..event_service_count {
                if !storage.get_pending_events(&format!("perf-mixed-event-{idx}"), 1).await.unwrap().is_empty() {
                    remaining_event_services += 1;
                }
            }

            if remaining_transactions == 0 && remaining_event_services == 0 {
                break;
            }

            let tick_started_at = Instant::now();
            scheduler.run_once().await.unwrap();
            tick_latencies_ms.push(tick_started_at.elapsed().as_millis());
            tick_count += 1;
            requests_after_each_tick.push(mock_server.received_requests().await.unwrap().len());

            if tick_count > 16 {
                panic!("scheduler mixed backlog smoke exceeded expected tick budget");
            }
        }

        tick_latencies_ms.sort_unstable();
        let p50_ms = percentile(&tick_latencies_ms, 0.50);
        let p95_ms = percentile(&tick_latencies_ms, 0.95);
        let p99_ms = percentile(&tick_latencies_ms, 0.99);
        let max_ms = *tick_latencies_ms.last().unwrap_or(&0);
        let total_elapsed_ms = started_at.elapsed().as_millis();
        let total_requests = mock_server.received_requests().await.unwrap().len();

        println!(
            "APPSERVICE_SCHEDULER_PERF scenario=mixed_backlog txn_services={} event_services={} pending_txns_per_service={} pending_events_per_service={} ticks={} requests={} expected_requests={} total_elapsed_ms={} tick_p50={}ms tick_p95={}ms tick_p99={}ms tick_max={}ms",
            transaction_service_count,
            event_service_count,
            pending_transactions_per_service,
            pending_events_per_service,
            tick_count,
            total_requests,
            expected_total_requests,
            total_elapsed_ms,
            p50_ms,
            p95_ms,
            p99_ms,
            max_ms
        );
        println!("APPSERVICE_SCHEDULER_PERF_TICK_COUNTS {:?}", requests_after_each_tick);
        println!(
            "PERF_SMOKE_JSON={}",
            json!({
                "name": "appservice_scheduler_mixed_backlog_load_smoke",
                "transaction_service_count": transaction_service_count,
                "event_service_count": event_service_count,
                "pending_transactions_per_service": pending_transactions_per_service,
                "pending_events_per_service": pending_events_per_service,
                "ticks": tick_count,
                "requests": total_requests,
                "expected_requests": expected_total_requests,
                "total_elapsed_ms": total_elapsed_ms,
                "tick_p50_ms": p50_ms,
                "tick_p95_ms": p95_ms,
                "tick_p99_ms": p99_ms,
                "tick_max_ms": max_ms,
                "requests_after_each_tick": requests_after_each_tick
            })
        );

        assert_eq!(total_requests, expected_total_requests);
        assert!(tick_count <= 9, "mixed backlog should drain within nine ticks under current capacity defaults");
    }
}
