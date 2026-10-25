#![allow(clippy::unwrap_used)]

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Instant;

    use serde_json::json;
    use sqlx::PgPool;
    use synapse_services::application_service::{ApplicationServiceManager, ApplicationServiceScheduler};
    use synapse_storage::application_service::{ApplicationServiceStorage, RegisterApplicationServiceRequest};
    use synapse_storage::EventStorage;
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
                    url: Some(format!("http://127.0.0.1:18{:03}", idx)),
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
    #[ignore = "手工负载冒烟：appservice 统计查询在负载下的耗时，按墙钟时间断言。结果依赖机器负载，在 CI 上会假失败。显式运行：cargo nextest run --all-features --test performance_manual --run-ignored ignored-only appservice_statistics_load_smoke -- --nocapture"]
    async fn appservice_statistics_load_smoke() {
        let pool: Arc<PgPool> = match synapse_test_utils::prepare_isolated_test_pool().await {
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
    #[ignore = "手工负载冒烟：appservice 调度器混合积压下的耗时，按墙钟时间断言。结果依赖机器负载，在 CI 上会假失败。显式运行：cargo nextest run --all-features --test performance_manual --run-ignored ignored-only appservice_scheduler_mixed_backlog_load_smoke -- --nocapture"]
    async fn appservice_scheduler_mixed_backlog_load_smoke() {
        let pool: Arc<PgPool> = match synapse_test_utils::prepare_isolated_test_pool().await {
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
                    url: Some(mock_server.uri()),
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
                    url: Some(mock_server.uri()),
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
    /// 与墙钟冒烟共用的清理：把这些表恢复到空，并重置自增 id。
    async fn truncate_appservice_perf_tables(pool: &PgPool) {
        sqlx::query(
            "TRUNCATE application_service_statistics, application_service_transactions, \
             application_service_events, application_service_users, \
             application_service_room_namespaces, application_service_room_alias_namespaces, \
             application_service_user_namespaces, application_services RESTART IDENTITY CASCADE",
        )
        .execute(pool)
        .await
        .unwrap();
    }

    /// 跑 `iterations` 次 `get_statistics` 的总纳秒数（**含一次预热**，见下）。
    ///
    /// 用 `as_nanos()` 而不是 `as_millis()`：小规模下毫秒整数会全落成 0，
    /// 比值退化成分母为零 —— 那正是"看起来合理、其实什么都没有测"的形态。
    async fn measure_statistics_total_ns(
        storage: &ApplicationServiceStorage,
        expected_rows: usize,
        iterations: usize,
    ) -> u128 {
        // 预热一次：首个查询会付连接池取连接、语句缓存、plan cache 的一次性成本，
        // 把它算进小规模会**抬高**分母、让退化看起来更小（假绿方向）。
        let warmup = storage.get_statistics().await.unwrap();
        assert_eq!(warmup.len(), expected_rows, "种子数据的服务数不对，后续比值没有意义");
        let started_at = Instant::now();
        for _ in 0..iterations {
            let rows = storage.get_statistics().await.unwrap();
            assert_eq!(rows.len(), expected_rows);
        }
        started_at.elapsed().as_nanos()
    }

    /// 参照单位：同一 pool 上「一次最简查询往返」的平均纳秒数。
    ///
    /// 为什么需要它：per-service 成本的**绝对值**依赖机器；而"每服务涨了多少倍"抓不到 N+1 ——
    /// N+1 是"每行固定多一次查询"，每服务成本对 N 仍是常数。实测（2026-10-25）：给
    /// `get_statistics` 注入 N+1 后，每服务成本从 13.1μs 涨到 181μs（14 倍），
    /// 但小规模被同样拖慢，`per_service_large / per_service_small` 只从 0.55 变到 **1.08**
    /// —— 用"复杂度比值"当阈值会**放过**它。
    ///
    /// 真正能表达退化的是「每行成本 ≈ 几次数据库往返」：好实现的全部工作在一次查询里完成，
    /// 摊到 N 行 ⇒ 远小于 1；N+1 ⇒ 每行就是 1 次往返。
    ///
    /// 实测对照（2026-10-25，本机）：
    /// | | 每行成本 | 折算往返次数 | 结果 |
    /// |---|---|---|---|
    /// | 正常实现 | 9.9μs / 5.1μs | **0.061 / 0.031** | PASS |
    /// | 注入 N+1 | 187μs / 195μs | **0.985 / 1.026** | FAIL |
    async fn measure_reference_query_ns(pool: &PgPool, iterations: usize) -> u128 {
        let warmup: i64 = sqlx::query_scalar("SELECT 1::bigint").fetch_one(pool).await.unwrap();
        assert_eq!(warmup, 1);
        let started_at = Instant::now();
        for _ in 0..iterations {
            let _: i64 = sqlx::query_scalar("SELECT 1::bigint").fetch_one(pool).await.unwrap();
        }
        started_at.elapsed().as_nanos()
    }

    /// 线性度门禁（**故意不加 `#[ignore]`**）：`get_statistics` 的**每服务成本**不得随服务数增长。
    ///
    /// 与上面两条墙钟冒烟的分工：
    /// - 那两条断言**绝对毫秒**，结果依赖机器负载，只能 `--run-ignored` 手工跑；
    /// - 本条断言**相对量**（per-service 纳秒之比），同机同进程内自比，与机器速度无关 ⇒ 能进 CI。
    ///
    /// 它抓的是 **N+1 / 缺索引 / 全表扫描** 这类复杂度退化（4 倍服务数给出 4 倍以上每服务成本），
    /// 不抓"整体慢 2 倍" —— 后者仍需人工基准（`TESTING.md` §四）。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn statistics_per_service_cost_does_not_grow_with_service_count() {
        const SMALL: usize = 40;
        const LARGE: usize = 160; // 4×
        const ITERATIONS: usize = 60;
        // 阈值语义：**每行读出成本折算成「几次数据库往返」**。取 0.5 的依据是本机实测两端：
        //   正常实现：0.061（40 服务）/ 0.031（160 服务）—— 一次查询取回全部行，摊到 N 行；
        //   注入 N+1：0.985 / 1.026 —— 每行再查一次，正好 ≈ 1 次往返。
        // 0.5 落在两者之间：正常侧 8 倍余量，退化侧 2 倍超出。分子分母同机同 pool，与机器速度无关。
        const MAX_ROUNDTRIPS_PER_ROW: f64 = 0.5;

        let pool: Arc<PgPool> = match synapse_test_utils::prepare_isolated_test_pool().await {
            Ok(pool) => pool,
            // fail-closed：与 `docs/audit/TEST_COMPROMISE_AUDIT_2026-10-25.md` §2 同口径。
            // 本文件另两条 smoke 仍是 `eprintln!("Skipping …") + return`（它们 `#[ignore]`、手工跑，
            // 影响面小），本条要进 CI，就必须缺库即 panic。
            Err(error) => panic!(
                "appservice statistics linearity gate: test database unavailable ({error}) — \
                 refusing to skip-as-pass: the gate would look green without measuring anything"
            ),
        };

        setup_appservice_perf_tables(pool.as_ref()).await;
        let storage = Arc::new(ApplicationServiceStorage::new(&pool));

        let mut totals = Vec::with_capacity(2);
        for service_count in [SMALL, LARGE] {
            truncate_appservice_perf_tables(pool.as_ref()).await;
            seed_statistics_scenario(storage.as_ref(), ScenarioKind::EventOnly, service_count, 0, 0).await;
            totals.push(measure_statistics_total_ns(storage.as_ref(), service_count, ITERATIONS).await);
        }

        let reference_ns = measure_reference_query_ns(pool.as_ref(), ITERATIONS).await as f64 / ITERATIONS as f64;
        let per_row_small = totals[0] as f64 / (SMALL * ITERATIONS) as f64;
        let per_row_large = totals[1] as f64 / (LARGE * ITERATIONS) as f64;
        let cost_small = per_row_small / reference_ns;
        let cost_large = per_row_large / reference_ns;
        let worst = cost_small.max(cost_large);

        println!(
            "APPSERVICE_STATS_COST_PER_ROW small={SMALL} large={LARGE} iterations={ITERATIONS} \
             reference_roundtrip={reference_ns:.0}ns per_row_small={per_row_small:.0}ns \
             per_row_large={per_row_large:.0}ns cost_in_roundtrips_small={cost_small:.3} \
             cost_in_roundtrips_large={cost_large:.3} limit={MAX_ROUNDTRIPS_PER_ROW}"
        );

        assert!(
            worst < MAX_ROUNDTRIPS_PER_ROW,
            "get_statistics 的**每行**成本达到 {worst:.2} 次数据库往返 \
             （{per_row_small:.0}ns / {per_row_large:.0}ns，一次往返 {reference_ns:.0}ns，\
             阈值 {MAX_ROUNDTRIPS_PER_ROW} 次）。\n\
             正常实现把全部工作放在一次查询里、摊到 N 行，该值应远小于 1；接近或超过 1 \
             意味着**每行又查了一次库**（N+1）或丢了索引。查 \
             `synapse-storage/src/application_service/repository.rs` 的 get_statistics。"
        );
    }
}
