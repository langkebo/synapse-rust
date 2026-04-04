//! Database performance benchmarks.
//!
//! Measures the performance of critical database operations.
//! Run with: cargo bench --bench performance_database_benchmarks

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use sqlx::PgPool;
use std::time::Duration;
use tokio::runtime::Runtime;

// Helper to get database pool
async fn get_pool() -> PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://synapse:synapse@localhost:5432/synapse_test".to_string());

    PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to database")
}

// ============================================================================
// User Query Benchmarks
// ============================================================================

fn bench_user_query_by_id(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let pool = rt.block_on(get_pool());

    let mut group = c.benchmark_group("database/user_query");
    group.throughput(Throughput::Elements(1));
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("by_user_id", |b| {
        b.to_async(&rt).iter(|| async {
            let user_id = black_box("@bench_user_1:benchmark.local");
            sqlx::query("SELECT user_id, username, created_ts FROM users WHERE user_id = $1")
                .bind(user_id)
                .fetch_optional(&pool)
                .await
                .unwrap()
        })
    });

    group.bench_function("by_username", |b| {
        b.to_async(&rt).iter(|| async {
            let username = black_box("bench_user_1");
            sqlx::query("SELECT user_id, username, created_ts FROM users WHERE username = $1")
                .bind(username)
                .fetch_optional(&pool)
                .await
                .unwrap()
        })
    });

    group.finish();
}

// ============================================================================
// Room Query Benchmarks
// ============================================================================

fn bench_room_query(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let pool = rt.block_on(get_pool());

    let mut group = c.benchmark_group("database/room_query");
    group.throughput(Throughput::Elements(1));
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("by_room_id", |b| {
        b.to_async(&rt).iter(|| async {
            let room_id = black_box("!bench_room_1:benchmark.local");
            sqlx::query(
                "SELECT room_id, creator, created_ts, is_public FROM rooms WHERE room_id = $1",
            )
            .bind(room_id)
            .fetch_optional(&pool)
            .await
            .unwrap()
        })
    });

    group.bench_function("public_rooms_list", |b| {
        b.to_async(&rt).iter(|| async {
            sqlx::query(
                "SELECT room_id, creator, created_ts FROM rooms WHERE is_public = true LIMIT 20",
            )
            .fetch_all(&pool)
            .await
            .unwrap()
        })
    });

    group.finish();
}

// ============================================================================
// Event Query Benchmarks
// ============================================================================

fn bench_event_query(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let pool = rt.block_on(get_pool());

    let mut group = c.benchmark_group("database/event_query");
    group.throughput(Throughput::Elements(1));
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("by_event_id", |b| {
        b.to_async(&rt).iter(|| async {
            let event_id = black_box("$bench_event_1:benchmark.local");
            sqlx::query("SELECT event_id, room_id, sender, event_type, content FROM events WHERE event_id = $1")
                .bind(event_id)
                .fetch_optional(&pool)
                .await
                .unwrap()
        })
    });

    group.bench_function("by_room_recent", |b| {
        b.to_async(&rt).iter(|| async {
            let room_id = black_box("!bench_room_1:benchmark.local");
            sqlx::query(
                "SELECT event_id, sender, event_type, content, origin_server_ts
                 FROM events
                 WHERE room_id = $1
                 ORDER BY origin_server_ts DESC
                 LIMIT 50",
            )
            .bind(room_id)
            .fetch_all(&pool)
            .await
            .unwrap()
        })
    });

    group.bench_function("by_room_time_range", |b| {
        b.to_async(&rt).iter(|| async {
            let room_id = black_box("!bench_room_1:benchmark.local");
            let now = chrono::Utc::now().timestamp_millis();
            let one_day_ago = now - 86400000;

            sqlx::query(
                "SELECT event_id, sender, event_type, content, origin_server_ts
                 FROM events
                 WHERE room_id = $1 AND origin_server_ts BETWEEN $2 AND $3
                 ORDER BY origin_server_ts DESC",
            )
            .bind(room_id)
            .bind(one_day_ago)
            .bind(now)
            .fetch_all(&pool)
            .await
            .unwrap()
        })
    });

    group.finish();
}

// ============================================================================
// Device Query Benchmarks
// ============================================================================

fn bench_device_query(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let pool = rt.block_on(get_pool());

    let mut group = c.benchmark_group("database/device_query");
    group.throughput(Throughput::Elements(1));
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("by_user_id", |b| {
        b.to_async(&rt).iter(|| async {
            let user_id = black_box("@bench_user_1:benchmark.local");
            sqlx::query(
                "SELECT device_id, display_name, last_seen_ts FROM devices WHERE user_id = $1",
            )
            .bind(user_id)
            .fetch_all(&pool)
            .await
            .unwrap()
        })
    });

    group.bench_function("by_device_id", |b| {
        b.to_async(&rt).iter(|| async {
            let user_id = black_box("@bench_user_1:benchmark.local");
            let device_id = black_box("BENCH_DEVICE_1");
            sqlx::query("SELECT device_id, display_name, last_seen_ts FROM devices WHERE user_id = $1 AND device_id = $2")
                .bind(user_id)
                .bind(device_id)
                .fetch_optional(&pool)
                .await
                .unwrap()
        })
    });

    group.finish();
}

// ============================================================================
// Batch Insert Benchmarks
// ============================================================================

fn bench_batch_insert(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let pool = rt.block_on(get_pool());

    let mut group = c.benchmark_group("database/batch_insert");
    group.measurement_time(Duration::from_secs(15));

    for batch_size in [10, 50, 100].iter() {
        group.throughput(Throughput::Elements(*batch_size as u64));

        group.bench_with_input(
            BenchmarkId::new("devices", batch_size),
            batch_size,
            |b, &size| {
                b.to_async(&rt).iter(|| async {
                    let user_id = "@bench_batch_user:benchmark.local";
                    let mut tx = pool.begin().await.unwrap();

                    for i in 0..size {
                        let device_id = format!("BENCH_BATCH_{}", i);
                        let display_name = format!("Batch Device {}", i);
                        let now = chrono::Utc::now().timestamp_millis();

                        sqlx::query(
                            "INSERT INTO devices (device_id, user_id, display_name, last_seen_ts)
                             VALUES ($1, $2, $3, $4)
                             ON CONFLICT (device_id, user_id) DO UPDATE SET last_seen_ts = $4",
                        )
                        .bind(&device_id)
                        .bind(user_id)
                        .bind(&display_name)
                        .bind(now)
                        .execute(&mut *tx)
                        .await
                        .unwrap();
                    }

                    tx.commit().await.unwrap();

                    // Cleanup
                    sqlx::query("DELETE FROM devices WHERE user_id = $1")
                        .bind(user_id)
                        .execute(&pool)
                        .await
                        .unwrap();
                })
            },
        );
    }

    group.finish();
}

// ============================================================================
// Index Efficiency Benchmarks
// ============================================================================

fn bench_index_efficiency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let pool = rt.block_on(get_pool());

    let mut group = c.benchmark_group("database/index_efficiency");
    group.throughput(Throughput::Elements(1));
    group.measurement_time(Duration::from_secs(10));

    // Test indexed vs non-indexed queries
    group.bench_function("indexed_user_lookup", |b| {
        b.to_async(&rt).iter(|| async {
            let user_id = black_box("@bench_user_50:benchmark.local");
            sqlx::query("SELECT user_id FROM users WHERE user_id = $1")
                .bind(user_id)
                .fetch_optional(&pool)
                .await
                .unwrap()
        })
    });

    group.bench_function("indexed_room_lookup", |b| {
        b.to_async(&rt).iter(|| async {
            let room_id = black_box("!bench_room_50:benchmark.local");
            sqlx::query("SELECT room_id FROM rooms WHERE room_id = $1")
                .bind(room_id)
                .fetch_optional(&pool)
                .await
                .unwrap()
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_user_query_by_id,
    bench_room_query,
    bench_event_query,
    bench_device_query,
    bench_batch_insert,
    bench_index_efficiency,
);

criterion_main!(benches);
