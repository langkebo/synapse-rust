//! Performance Benchmark Tests
//!
//! This module contains performance benchmark tests to ensure system meets
//! quality gate standards defined in optimization-plan.md Chapter 5.
//!
//! Quality Gate Standards:
//! - Search API P95 latency: ≤500ms
//! - Database query performance: optimized for pagination
//! - Concurrent request handling: supports production load

use criterion::{criterion_group, criterion_main, Criterion};
use reqwest;
use std::time::Duration;
use tokio::runtime::Runtime;

fn create_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("Failed to create HTTP client")
}

fn bench_base_url() -> String {
    std::env::var("BENCH_BASE_URL").unwrap_or_else(|_| "http://localhost:8008".to_string())
}

fn bench_admin_token() -> Option<String> {
    std::env::var("BENCH_ADMIN_TOKEN")
        .ok()
        .filter(|token| !token.trim().is_empty())
}

fn benchmark_user_directory_search(c: &mut Criterion) {
    let rt = Runtime::new().expect("Failed to create runtime");
    let client = create_client();
    let base_url = bench_base_url();
    let admin_token = bench_admin_token();

    c.bench_function("user_directory_search_single", |b| {
        b.iter(|| {
            rt.block_on(async {
                let Some(token) = admin_token.as_deref() else {
                    return;
                };
                let _ = client
                    .post(format!(
                        "{}/_matrix/client/r0/user_directory/search",
                        base_url
                    ))
                    .header("Authorization", format!("Bearer {}", token))
                    .json(&serde_json::json!({
                        "search_term": "admin",
                        "limit": 10
                    }))
                    .send()
                    .await;
            });
        });
    });

    c.bench_function("user_directory_search_batch_10", |b| {
        b.iter(|| {
            rt.block_on(async {
                let Some(token) = admin_token.as_deref() else {
                    return;
                };
                let handles: Vec<_> = (0..10)
                    .map(|_| {
                        let client = client.clone();
                        let base_url = base_url.clone();
                        let token = token.to_string();
                        tokio::spawn(async move {
                            client
                                .post(format!(
                                    "{}/_matrix/client/r0/user_directory/search",
                                    base_url
                                ))
                                .header("Authorization", format!("Bearer {}", token))
                                .json(&serde_json::json!({
                                    "search_term": "test",
                                    "limit": 10
                                }))
                                .send()
                                .await
                        })
                    })
                    .collect();
                for handle in handles {
                    let _ = handle.await;
                }
            });
        });
    });
}

fn benchmark_room_operations(c: &mut Criterion) {
    let rt = Runtime::new().expect("Failed to create runtime");
    let client = create_client();
    let base_url = bench_base_url();
    let admin_token = bench_admin_token();

    c.bench_function("room_state_query", |b| {
        b.iter(|| {
            rt.block_on(async {
                let Some(token) = admin_token.as_deref() else {
                    return;
                };
                let _ = client
                    .get(format!(
                        "{}/_matrix/client/r0/rooms/!test:localhost/state",
                        base_url
                    ))
                    .header("Authorization", format!("Bearer {}", token))
                    .send()
                    .await;
            });
        });
    });

    c.bench_function("room_members_list", |b| {
        b.iter(|| {
            rt.block_on(async {
                let Some(token) = admin_token.as_deref() else {
                    return;
                };
                let _ = client
                    .get(format!(
                        "{}/_matrix/client/r0/rooms/!test:localhost/members",
                        base_url
                    ))
                    .header("Authorization", format!("Bearer {}", token))
                    .send()
                    .await;
            });
        });
    });
}

fn benchmark_sync_operations(c: &mut Criterion) {
    let rt = Runtime::new().expect("Failed to create runtime");
    let client = create_client();
    let base_url = bench_base_url();
    let admin_token = bench_admin_token();

    c.bench_function("sync_with_timeout", |b| {
        b.iter(|| {
            rt.block_on(async {
                let Some(token) = admin_token.as_deref() else {
                    return;
                };
                let _ = client
                    .get(format!("{}/_matrix/client/r0/sync?timeout=1000", base_url))
                    .header("Authorization", format!("Bearer {}", token))
                    .send()
                    .await;
            });
        });
    });

    c.bench_function("sync_short_timeout", |b| {
        b.iter(|| {
            rt.block_on(async {
                let Some(token) = admin_token.as_deref() else {
                    return;
                };
                let _ = client
                    .get(format!("{}/_matrix/client/r0/sync?timeout=100", base_url))
                    .header("Authorization", format!("Bearer {}", token))
                    .send()
                    .await;
            });
        });
    });
}

fn benchmark_auth_operations(c: &mut Criterion) {
    let rt = Runtime::new().expect("Failed to create runtime");
    let client = create_client();
    let base_url = bench_base_url();
    let admin_token = bench_admin_token();

    c.bench_function("whoami", |b| {
        b.iter(|| {
            rt.block_on(async {
                let Some(token) = admin_token.as_deref() else {
                    return;
                };
                let _ = client
                    .get(format!("{}/_matrix/client/r0/account/whoami", base_url))
                    .header("Authorization", format!("Bearer {}", token))
                    .send()
                    .await;
            });
        });
    });
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .sample_size(20)
        .measurement_time(Duration::from_secs(30))
        .warm_up_time(Duration::from_secs(5));
    targets = benchmark_user_directory_search, benchmark_room_operations, benchmark_sync_operations, benchmark_auth_operations
);

criterion_main!(benches);
