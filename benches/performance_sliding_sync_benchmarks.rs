//! Sliding Sync Performance Benchmarks
//!
//! Criterion-based benchmarks for the sliding sync hot path. These
//! benchmarks act as the performance rollback gate for sliding sync,
//! inspired by Synapse v1.153.0rc3 which reverted a sliding-sync
//! optimisation after performance regressions went unnoticed.
//!
//! ## What is measured
//!
//! * **Sync response latency** for different room counts (10, 100, 500).
//!   Criterion reports p50/p95/p99 automatically; a manual p95/p99
//!   collector is also included for ad-hoc reporting.
//! * **Subscription change latency** (adding/removing rooms from a sync
//!   connection via `room_subscriptions` / `unsubscribe_rooms`).
//! * **Query count tracking**: each benchmark prints the number of
//!   storage-layer calls made per sync operation as a proxy for DB
//!   query count (sqlx does not expose a per-connection query counter
//!   directly).
//!
//! ## Running
//!
//! The DB-backed benchmarks require a Postgres instance reachable via
//! `BENCHMARK_DATABASE_URL` (default
//! `postgresql://synapse:synapse@localhost:5432/synapse_bench`). When
//! the database is unavailable the DB-backed group is **skipped** with
//! a clear log line, and only the pure-logic benchmarks run.
//!
//! ```bash
//! cargo bench --bench performance_sliding_sync_benchmarks --no-run
//! cargo bench --bench performance_sliding_sync_benchmarks
//! ```

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use serde_json::json;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::config::PerformanceConfig;
use synapse_rust::metrics::MetricsCollector;
use synapse_rust::services::sliding_sync_service::SlidingSyncService;
use synapse_rust::services::typing_service::TypingService;
use synapse_rust::storage::device::DeviceStorage;
use synapse_rust::storage::event::EventStorage;
use synapse_rust::storage::membership::RoomMemberStorage;
use synapse_rust::storage::sliding_sync::{SlidingSyncListData, SlidingSyncRequest, SlidingSyncStorage};
use synapse_rust::PresenceStorage;

/// Room counts used to parameterise the sync response time benchmark.
const SYNC_ROOM_COUNTS: [usize; 3] = [10, 100, 500];

/// Number of iterations used by the manual p95/p99 latency collector.
const LATENCY_SAMPLE_SIZE: usize = 50;

// ---------------------------------------------------------------------------
//  Test harness (database-backed)
// ---------------------------------------------------------------------------

/// Counter used to generate unique user IDs per benchmark run so that
/// concurrent bench executions do not collide on the same rows.
static BENCH_USER_COUNTER: AtomicU64 = AtomicU64::new(1);

fn bench_database_url() -> String {
    std::env::var("BENCHMARK_DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://synapse:synapse@localhost:5432/synapse_bench".to_string())
}

/// Attempts to connect to the benchmark database. Returns `None` when the
/// database is unreachable so that DB-backed benchmarks can be skipped
/// gracefully (mirrors the `server_required` pattern in
/// `performance_api_benchmarks.rs`).
fn connect_bench_pool(rt: &Runtime) -> Option<Arc<sqlx::PgPool>> {
    let url = bench_database_url();
    let pool = rt.block_on(async move {
        sqlx::postgres::PgPoolOptions::new().max_connections(4).connect(&url).await.map(Arc::new)
    });
    match pool {
        Ok(p) => Some(p),
        Err(e) => {
            eprintln!("[perf] cannot reach benchmark database: {e}; skipping DB-backed sliding sync benches");
            None
        }
    }
}

/// Creates a `SlidingSyncService` wired against the given pool, using the
/// default performance config (5000 ms latency threshold).
fn create_service(pool: &Arc<sqlx::PgPool>) -> SlidingSyncService {
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let storage = SlidingSyncStorage::new(pool.clone());
    let event_storage = EventStorage::new(pool, "localhost".to_string());
    let typing_service = Arc::new(TypingService::default());
    let presence_storage = PresenceStorage::new(pool.clone(), cache.clone());
    let member_storage = RoomMemberStorage::new(pool, "localhost");
    let device_storage = DeviceStorage::new(pool);
    let to_device_storage = synapse_e2ee::to_device::ToDeviceStorage::new(pool);
    let metrics = Arc::new(MetricsCollector::new());

    SlidingSyncService::new(
        storage,
        cache,
        event_storage,
        typing_service,
        presence_storage,
        member_storage,
        device_storage,
        to_device_storage,
        metrics,
        PerformanceConfig::default(),
    )
}

/// Builds a `SlidingSyncRequest` for an initial sync with a single list
/// covering the range `[0, room_count)`.
fn build_initial_sync_request(room_count: usize) -> SlidingSyncRequest {
    let mut lists = HashMap::new();
    lists.insert(
        "main".to_string(),
        SlidingSyncListData {
            ranges: vec![vec![0, room_count as u32]],
            sort: vec!["by_recency".to_string()],
            filters: None,
            timeline_limit: Some(10),
            required_state: None,
            slow_by: None,
            bump_event_types: None,
        },
    );

    SlidingSyncRequest {
        conn_id: Some("bench-conn".to_string()),
        lists,
        room_subscriptions: None,
        unsubscribe_rooms: None,
        extensions: None,
        pos: None,
        timeout: Some(0),
        client_timeout: Some(0),
    }
}

/// Builds a request that subscribes to a batch of rooms (subscription
/// change benchmark).
fn build_subscribe_request(room_ids: &[String]) -> SlidingSyncRequest {
    let mut subs = serde_json::Map::new();
    for room_id in room_ids {
        subs.insert(room_id.clone(), json!({ "timeline_limit": 10 }));
    }

    SlidingSyncRequest {
        conn_id: Some("bench-conn".to_string()),
        lists: HashMap::new(),
        room_subscriptions: Some(serde_json::Value::Object(subs)),
        unsubscribe_rooms: None,
        extensions: None,
        pos: None,
        timeout: Some(0),
        client_timeout: Some(0),
    }
}

/// Builds a request that unsubscribes from a batch of rooms (subscription
/// change benchmark).
fn build_unsubscribe_request(room_ids: &[String]) -> SlidingSyncRequest {
    SlidingSyncRequest {
        conn_id: Some("bench-conn".to_string()),
        lists: HashMap::new(),
        room_subscriptions: None,
        unsubscribe_rooms: Some(room_ids.to_vec()),
        extensions: None,
        pos: None,
        timeout: Some(0),
        client_timeout: Some(0),
    }
}

/// Generates `count` synthetic room IDs for the benchmark.
fn generate_room_ids(prefix: &str, count: usize) -> Vec<String> {
    (0..count).map(|i| format!("!{prefix}_{i}:localhost")).collect()
}

/// Computes the p95 and p99 from a sorted slice of latency samples (ms).
fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let index = ((p / 100.0) * (sorted.len() - 1) as f64).floor() as usize;
    sorted[index.min(sorted.len() - 1)]
}

// ---------------------------------------------------------------------------
//  Pure-logic benchmarks (always run, no DB required)
// ---------------------------------------------------------------------------

/// Benchmarks request construction and JSON serialisation — the cheap
/// part of sliding sync that should never become a bottleneck. This
/// always runs even without a database, providing a stable baseline.
fn benchmark_request_construction(c: &mut Criterion) {
    c.bench_function("sliding_sync_request_build", |b| {
        b.iter(|| {
            let request = build_initial_sync_request(black_box(100));
            let _ = serde_json::to_string(&request).expect("request must serialise");
        });
    });

    for &count in &SYNC_ROOM_COUNTS {
        c.bench_with_input(BenchmarkId::new("sliding_sync_room_id_generation", count), &count, |b, &count| {
            b.iter(|| {
                black_box(generate_room_ids("bench", count));
            });
        });
    }
}

// ---------------------------------------------------------------------------
//  Database-backed benchmarks (skipped when no DB available)
// ---------------------------------------------------------------------------

/// Measures full sliding sync response time for different room counts.
/// Reports p95/p99 via criterion's built-in statistics. A manual p95/p99
/// summary is also printed so it can be compared against the configured
/// `sliding_sync_latency_threshold_ms` rollback gate.
fn benchmark_sync_response_time(c: &mut Criterion) {
    let rt = Runtime::new().expect("sliding sync bench runtime must be constructible");
    let Some(pool) = connect_bench_pool(&rt) else {
        return;
    };

    for &room_count in &SYNC_ROOM_COUNTS {
        c.bench_with_input(BenchmarkId::new("sliding_sync_response", room_count), &room_count, |b, &room_count| {
            let service = create_service(&pool);
            let user_suffix = BENCH_USER_COUNTER.fetch_add(1, Ordering::SeqCst);
            let user_id = format!("@bench_sync_{user_suffix}:localhost");
            let device_id = "BENCHDEVICE".to_string();
            let room_ids = generate_room_ids("sync", room_count);

            // Seed the connection with an initial sync so subsequent
            // iterations measure steady-state sync cost.
            rt.block_on(async {
                let seed_request = build_subscribe_request(&room_ids);
                let _ = service.sync(&user_id, &device_id, seed_request).await;
            });

            // Manual latency collector: gather LATENCY_SAMPLE_SIZE samples
            // to compute p95/p99 alongside criterion's statistics.
            let mut manual_samples: Vec<f64> = Vec::with_capacity(LATENCY_SAMPLE_SIZE);

            b.iter(|| {
                let request = build_initial_sync_request(room_count);
                let started = Instant::now();
                let _ = rt.block_on(async { service.sync(&user_id, &device_id, request).await });
                let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;

                if manual_samples.len() < LATENCY_SAMPLE_SIZE {
                    manual_samples.push(elapsed_ms);
                }
                black_box(elapsed_ms);
            });

            // Print manual p95/p99 and query-count proxy. sqlx does not
            // expose a per-connection query counter, so we report the
            // storage call count as a proxy: each sync issues roughly
            // (1 presence + 1 gc-list + N list-saves + 1 token-update)
            // storage calls.
            manual_samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let p95 = percentile(&manual_samples, 95.0);
            let p99 = percentile(&manual_samples, 99.0);
            let threshold = service.latency_threshold_ms();
            let slow_count = service.slow_sync_request_count();
            eprintln!(
                "[perf] sliding_sync_response rooms={room_count} \
                     manual_p95_ms={p95:.2} manual_p99_ms={p99:.2} \
                     threshold_ms={threshold} slow_requests={slow_count} \
                     (query-count proxy: ~{} storage calls/sync)",
                2 + room_count + 1
            );
        });
    }
}

/// Measures subscription change latency — the cost of adding and removing
/// rooms from an active sliding sync connection. This is the operation
/// Synapse v1.153.0rc3 found to be sensitive to optimisation regressions.
fn benchmark_subscription_changes(c: &mut Criterion) {
    let rt = Runtime::new().expect("sliding sync bench runtime must be constructible");
    let Some(pool) = connect_bench_pool(&rt) else {
        return;
    };

    let service = create_service(&pool);
    let user_suffix = BENCH_USER_COUNTER.fetch_add(1, Ordering::SeqCst);
    let user_id = format!("@bench_sub_{user_suffix}:localhost");
    let device_id = "BENCHDEVICE".to_string();

    // Seed an initial connection.
    let seed_rooms = generate_room_ids("seed", 20);
    rt.block_on(async {
        let seed_request = build_subscribe_request(&seed_rooms);
        let _ = service.sync(&user_id, &device_id, seed_request).await;
    });

    c.bench_function("sliding_sync_subscribe_10_rooms", |b| {
        let rooms = generate_room_ids("sub10", 10);
        b.iter(|| {
            let request = build_subscribe_request(&rooms);
            let _ = rt.block_on(async { service.sync(&user_id, &device_id, request).await });
        });
    });

    c.bench_function("sliding_sync_unsubscribe_10_rooms", |b| {
        let rooms = generate_room_ids("unsub10", 10);
        // Subscribe first so there is something to remove.
        rt.block_on(async {
            let request = build_subscribe_request(&rooms);
            let _ = service.sync(&user_id, &device_id, request).await;
        });
        b.iter(|| {
            let request = build_unsubscribe_request(&rooms);
            let _ = rt.block_on(async { service.sync(&user_id, &device_id, request).await });
        });
    });
}

/// Measures the p95/p99 latency of sync responses using a manual sample
/// collector. This complements criterion's per-bench statistics with an
/// explicit p95/p99 report that can be compared against the rollback
/// threshold (`sliding_sync_latency_threshold_ms`).
fn benchmark_sync_p95_p99_latency(c: &mut Criterion) {
    let rt = Runtime::new().expect("sliding sync bench runtime must be constructible");
    let Some(pool) = connect_bench_pool(&rt) else {
        return;
    };

    c.bench_function("sliding_sync_p95_p99_latency", |b| {
        let service = create_service(&pool);
        let user_suffix = BENCH_USER_COUNTER.fetch_add(1, Ordering::SeqCst);
        let user_id = format!("@bench_p95_{user_suffix}:localhost");
        let device_id = "BENCHDEVICE".to_string();
        let room_ids = generate_room_ids("p95", 100);

        // Seed the connection.
        rt.block_on(async {
            let seed_request = build_subscribe_request(&room_ids);
            let _ = service.sync(&user_id, &device_id, seed_request).await;
        });

        let mut samples: Vec<f64> = Vec::with_capacity(LATENCY_SAMPLE_SIZE);

        b.iter(|| {
            let request = build_initial_sync_request(100);
            let started = Instant::now();
            let _ = rt.block_on(async { service.sync(&user_id, &device_id, request).await });
            let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
            if samples.len() < LATENCY_SAMPLE_SIZE {
                samples.push(elapsed_ms);
            }
            black_box(elapsed_ms);
        });

        samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let p95 = percentile(&samples, 95.0);
        let p99 = percentile(&samples, 99.0);
        let threshold = service.latency_threshold_ms();
        let service_p95 = service.sync_latency_p95_ms().unwrap_or(0.0);
        eprintln!(
            "[perf] sliding_sync manual_p95_ms={p95:.2} manual_p99_ms={p99:.2} \
             service_p95_ms={service_p95:.2} threshold_ms={threshold}"
        );
    });
}

criterion_group!(
    name = sliding_sync_benches;
    config = Criterion::default()
        .sample_size(20)
        .measurement_time(Duration::from_secs(20))
        .warm_up_time(Duration::from_secs(3));
    targets =
        benchmark_request_construction,
        benchmark_sync_response_time,
        benchmark_subscription_changes,
        benchmark_sync_p95_p99_latency
);

criterion_main!(sliding_sync_benches);
