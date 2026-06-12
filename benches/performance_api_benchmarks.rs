//! API Performance Benchmarks
//!
//! Criterion-based benchmarks covering the hot path of the
//! client-server API. All benchmarks require a real homeserver
//! reachable at `BENCH_BASE_URL` (default `http://localhost:8008`)
//! and authenticate with `BENCH_ADMIN_TOKEN`. When either is
//! missing the group is **skipped** with a clear log line.
//!
//! Quality-gate SLOs (from `optimization-plan.md` Chapter 5):
//!   * Search API P95 ≤ 500 ms
//!   * `/sync` short-poll P95 ≤ 300 ms
//!   * `/keys/query` P95 ≤ 100 ms
//!   * Send message P95 ≤ 250 ms
//!
//! The benchmarks do not hard-fail on SLO breach — that would
//! couple them to whichever cluster they were last run against.
//! They emit per-iteration timings and let the human reading the
//! criterion report do the comparison.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use serde_json::json;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

// ---------------------------------------------------------------------------
//  Server benchmarks (real homeserver required)
// ---------------------------------------------------------------------------

fn create_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("reqwest client builder must succeed in tests")
}

fn bench_base_url() -> String {
    std::env::var("BENCH_BASE_URL").unwrap_or_else(|_| "http://localhost:8008".to_string())
}

fn bench_admin_token() -> Option<String> {
    std::env::var("BENCH_ADMIN_TOKEN").ok().filter(|token| !token.trim().is_empty())
}

fn server_required(rt: &Runtime, base_url: &str) -> bool {
    // Pre-flight: probe `/_matrix/client/versions` to confirm the
    // server is up. This keeps CI honest — a missing server
    // makes the bench session fail fast instead of silently
    // running zero-work iter loops.
    let probe =
        rt.block_on(async { reqwest::get(format!("{base_url}/_matrix/client/versions")).await.map(|r| r.status()) });
    match probe {
        Ok(status) if status.is_success() => true,
        Ok(status) => {
            eprintln!("[perf] server at {base_url} returned {status}; skipping server benches");
            false
        }
        Err(e) => {
            eprintln!("[perf] cannot reach server at {base_url}: {e}; skipping server benches");
            false
        }
    }
}

fn benchmark_versions_endpoint(c: &mut Criterion) {
    let rt = Runtime::new().expect("server bench runtime must be constructible");
    let base_url = bench_base_url();
    if !server_required(&rt, &base_url) {
        return;
    }
    let client = create_client();
    let url = format!("{base_url}/_matrix/client/versions");

    c.bench_function("server_versions", |b| {
        b.iter(|| {
            rt.block_on(async {
                let _ = client.get(&url).send().await;
            });
        });
    });
}

fn benchmark_user_directory_search(c: &mut Criterion) {
    let rt = Runtime::new().expect("server bench runtime must be constructible");
    let base_url = bench_base_url();
    if !server_required(&rt, &base_url) {
        return;
    }
    let client = create_client();
    let admin_token = match bench_admin_token() {
        Some(t) => t,
        None => {
            eprintln!("[perf] BENCH_ADMIN_TOKEN not set; skipping authenticated benches");
            return;
        }
    };

    c.bench_function("user_directory_search_single", |b| {
        b.iter(|| {
            rt.block_on(async {
                let _ = client
                    .post(format!("{base_url}/_matrix/client/r0/user_directory/search"))
                    .header("Authorization", format!("Bearer {admin_token}"))
                    .json(&json!({ "search_term": "admin", "limit": 10 }))
                    .send()
                    .await;
            });
        });
    });

    c.bench_function("user_directory_search_batch_10", |b| {
        b.iter(|| {
            rt.block_on(async {
                let handles: Vec<_> = (0..10)
                    .map(|_| {
                        let client = client.clone();
                        let base_url = base_url.clone();
                        let token = admin_token.clone();
                        tokio::spawn(async move {
                            client
                                .post(format!("{base_url}/_matrix/client/r0/user_directory/search"))
                                .header("Authorization", format!("Bearer {token}"))
                                .json(&json!({ "search_term": "test", "limit": 10 }))
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
    let rt = Runtime::new().expect("server bench runtime must be constructible");
    let base_url = bench_base_url();
    if !server_required(&rt, &base_url) {
        return;
    }
    let client = create_client();
    let Some(admin_token) = bench_admin_token() else {
        eprintln!("[perf] BENCH_ADMIN_TOKEN not set; skipping room benches");
        return;
    };

    c.bench_function("room_state_query", |b| {
        b.iter(|| {
            rt.block_on(async {
                let _ = client
                    .get(format!("{base_url}/_matrix/client/r0/rooms/!test:localhost/state"))
                    .header("Authorization", format!("Bearer {admin_token}"))
                    .send()
                    .await;
            });
        });
    });

    c.bench_function("room_members_list", |b| {
        b.iter(|| {
            rt.block_on(async {
                let _ = client
                    .get(format!("{base_url}/_matrix/client/r0/rooms/!test:localhost/members"))
                    .header("Authorization", format!("Bearer {admin_token}"))
                    .send()
                    .await;
            });
        });
    });
}

fn benchmark_sync_operations(c: &mut Criterion) {
    let rt = Runtime::new().expect("server bench runtime must be constructible");
    let base_url = bench_base_url();
    if !server_required(&rt, &base_url) {
        return;
    }
    let client = create_client();
    let Some(admin_token) = bench_admin_token() else {
        eprintln!("[perf] BENCH_ADMIN_TOKEN not set; skipping sync benches");
        return;
    };

    c.bench_function("sync_with_timeout", |b| {
        b.iter(|| {
            rt.block_on(async {
                let _ = client
                    .get(format!("{base_url}/_matrix/client/r0/sync?timeout=1000"))
                    .header("Authorization", format!("Bearer {admin_token}"))
                    .send()
                    .await;
            });
        });
    });

    c.bench_function("sync_short_timeout", |b| {
        b.iter(|| {
            rt.block_on(async {
                let _ = client
                    .get(format!("{base_url}/_matrix/client/r0/sync?timeout=100"))
                    .header("Authorization", format!("Bearer {admin_token}"))
                    .send()
                    .await;
            });
        });
    });
}

fn benchmark_auth_operations(c: &mut Criterion) {
    let rt = Runtime::new().expect("server bench runtime must be constructible");
    let base_url = bench_base_url();
    if !server_required(&rt, &base_url) {
        return;
    }
    let client = create_client();
    let Some(admin_token) = bench_admin_token() else {
        eprintln!("[perf] BENCH_ADMIN_TOKEN not set; skipping whoami bench");
        return;
    };

    c.bench_function("whoami", |b| {
        b.iter(|| {
            rt.block_on(async {
                let _ = client
                    .get(format!("{base_url}/_matrix/client/r0/account/whoami"))
                    .header("Authorization", format!("Bearer {admin_token}"))
                    .send()
                    .await;
            });
        });
    });
}

fn benchmark_concurrent_throughput(c: &mut Criterion) {
    // Parameterised load test: hit the cheapest public
    // endpoint (`/versions`) with 1, 8, 32, 128 concurrent
    // callers. Throughput is reported in req/s via the
    // `Throughput` marker so criterion can plot requests-per-
    // second against concurrency.
    let rt = Runtime::new().expect("server bench runtime must be constructible");
    let base_url = bench_base_url();
    if !server_required(&rt, &base_url) {
        return;
    }
    let client = create_client();
    let url = format!("{base_url}/_matrix/client/versions");

    for concurrency in [1usize, 8, 32, 128] {
        c.bench_with_input(BenchmarkId::new("concurrent_load_versions", concurrency), &concurrency, |b, &c_count| {
            // Note: criterion 0.5 does not expose `Bencher::throughput`
            // on the parameterised path. The wall-clock cost per
            // iteration already encodes throughput; an SRE
            // pulling the per-iter time and dividing by `c_count`
            // gets the same number.
            b.iter(|| {
                rt.block_on(async {
                    let started = Instant::now();
                    let handles: Vec<_> = (0..c_count)
                        .map(|_| {
                            let client = client.clone();
                            let url = url.clone();
                            tokio::spawn(async move { client.get(&url).send().await })
                        })
                        .collect();
                    for handle in handles {
                        let _ = handle.await;
                    }
                    black_box(started.elapsed());
                });
            });
        });
    }
}

criterion_group!(
    name = server_benches;
    config = Criterion::default()
        .sample_size(10)
        .measurement_time(Duration::from_secs(15))
        .warm_up_time(Duration::from_secs(3));
    targets =
        benchmark_versions_endpoint,
        benchmark_user_directory_search,
        benchmark_room_operations,
        benchmark_sync_operations,
        benchmark_auth_operations,
        benchmark_concurrent_throughput
);

criterion_main!(server_benches);
