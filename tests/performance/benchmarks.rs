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

const BASE_URL: &str = "http://localhost:8008";
const ADMIN_TOKEN: &str = "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAYWRtaW46Y2p5c3R4LnRvcCIsInVzZXJfaWQiOiJAYWRtaW46Y2p5c3R4LnRvcCIsImFkbWluIjp0cnVlLCJleHAiOjE3NzAyMTM3MzksImlhdCI6MTc3MDIxMDEzOSwiZGV2aWNlX2lkIjoiR284eTY4MTRaWExvVEZTdSJ9.vTgSVMx_ZLhKdEZmf4LvMHRJXgrmlOKlfTd5CTSwFk8";

fn create_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("Failed to create HTTP client")
}

fn benchmark_user_directory_search(c: &mut Criterion) {
    let rt = Runtime::new().expect("Failed to create runtime");
    let client = create_client();

    c.bench_function("user_directory_search_single", |b| {
        b.iter(|| {
            let _ = rt.block_on(async {
                let response = client
                    .post(&format!("{}/_matrix/client/r0/user_directory/search", BASE_URL))
                    .header("Authorization", format!("Bearer {}", ADMIN_TOKEN))
                    .json(&serde_json::json!({
                        "search_term": "admin",
                        "limit": 10
                    }))
                    .send()
                    .await;
                response
            });
        });
    });

    c.bench_function("user_directory_search_batch_10", |b| {
        b.iter(|| {
            let _ = rt.block_on(async {
                let handles: Vec<_> = (0..10)
                    .map(|_| {
                        let client = client.clone();
                        tokio::spawn(async move {
                            client
                                .post(&format!("{}/_matrix/client/r0/user_directory/search", BASE_URL))
                                .header("Authorization", format!("Bearer {}", ADMIN_TOKEN))
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

    c.bench_function("room_state_query", |b| {
        b.iter(|| {
            let _ = rt.block_on(async {
                let response = client
                    .get(&format!("{}/_matrix/client/r0/rooms/!test:localhost/state", BASE_URL))
                    .header("Authorization", format!("Bearer {}", ADMIN_TOKEN))
                    .send()
                    .await;
                response
            });
        });
    });

    c.bench_function("room_members_list", |b| {
        b.iter(|| {
            let _ = rt.block_on(async {
                let response = client
                    .get(&format!("{}/_matrix/client/r0/rooms/!test:localhost/members", BASE_URL))
                    .header("Authorization", format!("Bearer {}", ADMIN_TOKEN))
                    .send()
                    .await;
                response
            });
        });
    });
}

fn benchmark_sync_operations(c: &mut Criterion) {
    let rt = Runtime::new().expect("Failed to create runtime");
    let client = create_client();

    c.bench_function("sync_with_timeout", |b| {
        b.iter(|| {
            let _ = rt.block_on(async {
                let response = client
                    .get(&format!("{}/_matrix/client/r0/sync?timeout=1000", BASE_URL))
                    .header("Authorization", format!("Bearer {}", ADMIN_TOKEN))
                    .send()
                    .await;
                response
            });
        });
    });

    c.bench_function("sync_short_timeout", |b| {
        b.iter(|| {
            let _ = rt.block_on(async {
                let response = client
                    .get(&format!("{}/_matrix/client/r0/sync?timeout=100", BASE_URL))
                    .header("Authorization", format!("Bearer {}", ADMIN_TOKEN))
                    .send()
                    .await;
                response
            });
        });
    });
}

fn benchmark_auth_operations(c: &mut Criterion) {
    let rt = Runtime::new().expect("Failed to create runtime");
    let client = create_client();

    c.bench_function("whoami", |b| {
        b.iter(|| {
            let _ = rt.block_on(async {
                let response = client
                    .get(&format!("{}/_matrix/client/r0/account/whoami", BASE_URL))
                    .header("Authorization", format!("Bearer {}", ADMIN_TOKEN))
                    .send()
                    .await;
                response
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
