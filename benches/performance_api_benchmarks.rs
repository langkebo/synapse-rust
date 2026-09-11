//! API Performance Benchmarks
//!
//! Criterion-based benchmarks covering the hot path of the
//! client-server API. **Most** benchmarks require a real homeserver
//! reachable at `BENCH_BASE_URL` (default `http://localhost:8008`)
//! and authenticate with `BENCH_ADMIN_TOKEN`. When either is
//! missing the group is **skipped** with a clear log line.
//!
//! The `benchmark_pagination_strategies` group is the exception: it is
//! pure in-process computation and therefore runs everywhere, including
//! CI. It backs the blocking `check_pagination_benchmark.py` step in
//! `.github/workflows/benchmark.yml`.
//!
//! ⚠️ Skipping is silent w.r.t. the process exit code — `cargo bench`
//! still exits 0 when a benchmark was skipped. Set
//! `BENCH_REQUIRE=<group>[,<group>...]` to turn "a requested benchmark
//! silently skipped" into a hard failure. Group names are the
//! `criterion_group!` target function names: `versions`,
//! `user_directory`, `rooms`, `sync`, `auth`, `concurrent_throughput`,
//! `pagination`.
//!
//! A "did anything at all run?" check would be useless here, because the
//! in-process `pagination` group always runs — which is exactly the trap
//! this guard avoids.
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

// Benchmarks deliberately use `.expect()` for setup invariants (runtime
// construction, client builder) — a failure there is a harness bug, not a
// recoverable error. Criterion also panics internally on setup failures.
#![allow(clippy::expect_used)]

use criterion::{black_box, criterion_group, BenchmarkId, Criterion};
use serde_json::json;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

// ---------------------------------------------------------------------------
//  Required-group guard: a requested benchmark must not silently skip
// ---------------------------------------------------------------------------
//
// Historically every server-dependent group bailed out with a bare
// `eprintln!` + `return`, and `cargo bench` still exited 0. That made a
// missing homeserver (or a missing `BENCH_ADMIN_TOKEN`) indistinguishable
// from a fully successful benchmark run — the same false-green class of
// failure as the empty `cargo test --doc` gate.
//
// A naive "did *any* benchmark run?" check is not enough: the in-process
// `pagination` group always runs, so `executed > 0` is always true and such
// a guard could never fire. What actually matters is that the benchmarks you
// **asked for** really ran.
//
//   BENCH_REQUIRE=<group>[,<group>...]
//
// lists the groups that must execute. Any required group that skipped makes
// the process exit non-zero, catching the silent-skip false green while still
// allowing a run that intentionally omits server-dependent groups. Group
// names are the `criterion_group!` function names (e.g. `pagination`).
//
// A group registers itself exactly once, *after* its last guarded `return`,
// via [`require_bench_group`].

/// Group names that actually reached their `bench_function` call.
static EXECUTED_GROUPS: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

/// Parses `BENCH_REQUIRE` (comma/space separated) into group names.
fn required_groups() -> Vec<String> {
    std::env::var("BENCH_REQUIRE")
        .unwrap_or_default()
        .split([',', ' '])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Registers `group` as executed, and enforces `BENCH_REQUIRE` immediately:
/// a required group that skips must fail loudly rather than look like success.
///
/// Call once per group, after the last guarded `return` and immediately before
/// the first `bench_function`/`bench_with_input` call.
fn require_bench_group(group: &'static str) {
    if let Ok(mut executed) = EXECUTED_GROUPS.lock() {
        executed.push(group);
    }

    let required = required_groups();
    if required.iter().any(|r| r == group) {
        eprintln!("[bench-guard] required group `{group}` executed");
        return;
    }
    // Not a required group: nothing to enforce here.
    if !required.is_empty() {
        eprintln!("[bench-guard] group `{group}` executed (not required)");
    }
}

/// After all groups have had a chance to register, verify that every group
/// named in `BENCH_REQUIRE` actually executed.
///
/// This catches the case the old "any group ran?" check could not: a *specific*
/// requested benchmark silently skipping (missing server, missing
/// `BENCH_ADMIN_TOKEN`, or a filter that matched nothing).
fn enforce_required_groups() {
    let required = required_groups();
    if required.is_empty() {
        return;
    }

    let executed = EXECUTED_GROUPS.lock().map(|g| g.clone()).unwrap_or_default();

    let missing: Vec<&String> = required.iter().filter(|r| !executed.iter().any(|e| e == r)).collect();

    if !missing.is_empty() {
        eprintln!(
            "BENCH_REQUIRE: required benchmark group(s) did not execute: {}.\n\
             Executed groups: {:?}.\n\
             A required benchmark silently skipped — this is a false green. \
             Provide BENCH_ADMIN_TOKEN and/or a homeserver at BENCH_BASE_URL \
             (default http://localhost:8008), and make sure the criterion \
             filter did not exclude the group.",
            missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "),
            executed
        );
        std::process::exit(1);
    }

    eprintln!("BENCH_REQUIRE: all required group(s) executed: {}", required.join(", "));
}

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
    require_bench_group("versions");

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

    require_bench_group("user_directory");

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

    require_bench_group("rooms");

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

    require_bench_group("sync");

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

    require_bench_group("auth");

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

    require_bench_group("concurrent_throughput");

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

// ---------------------------------------------------------------------------
//  Pagination strategy benchmarks (in-process, no server required)
// ---------------------------------------------------------------------------
//
// ⚠️ 历史背景：这两个基准由 `a465d0fd` 引入，与
// `.github/workflows/benchmark.yml` 的阻塞步骤
// `python3 scripts/check_pagination_benchmark.py benchmark.txt --minimum-improvement 0.30`
// 配对，用于证明 keyset 分页相对 offset 分页有 ≥30% 的收益。
//
// 它们在 `8c7b4860`（2026-06-05）随一次"slimming"重构被一并删除，
// 但 workflow 的断言步骤没有被同步移除 —— 于是该阻塞步骤从 2026-06-05 起
// 必然失败（脚本对缺失的基准行 `raise SystemExit`），
// 同时分页性能**完全没有被测量**。
//
// 这里恢复基准本体。它是**纯内存**测量（250k 合成行），不依赖服务与数据库，
// 因此可以在 CI 中真实运行 —— 这正是门禁需要它的原因。
// 回归保护见 `tests/unit/pagination_gate_tests.rs`。

#[derive(Clone, Copy)]
struct SyntheticReportRow {
    score: i32,
    received_ts: i64,
    id: i64,
}

fn synthetic_reports(count: usize) -> Vec<SyntheticReportRow> {
    (0..count)
        .map(|i| SyntheticReportRow {
            score: 1000 - ((i / 50) % 1000) as i32,
            received_ts: 2_000_000_000_000_i64 - i as i64,
            id: (count - i) as i64,
        })
        .collect()
}

/// 模拟 `OFFSET n LIMIT m`：必须扫描并丢弃前 `offset` 行。
fn offset_page_checksum(rows: &[SyntheticReportRow], offset: usize, limit: usize) -> i64 {
    let mut skipped_scan_cost = 0_i64;
    for row in rows.iter().take(offset) {
        skipped_scan_cost ^= row.id;
    }

    skipped_scan_cost + rows.iter().skip(offset).take(limit).map(|row| row.id ^ row.received_ts).sum::<i64>()
}

/// 模拟 keyset（游标）分页：按 (score, received_ts, id) 降序二分定位游标，
/// 然后只取 `limit` 行 —— 不需要扫描被跳过的前缀。
fn keyset_page_checksum(rows: &[SyntheticReportRow], cursor: SyntheticReportRow, limit: usize) -> i64 {
    let start = rows
        .binary_search_by(|probe| {
            probe
                .score
                .cmp(&cursor.score)
                .reverse()
                .then_with(|| probe.received_ts.cmp(&cursor.received_ts).reverse())
                .then_with(|| probe.id.cmp(&cursor.id).reverse())
        })
        .map_or_else(|index| index, |index| index + 1);

    rows[start..].iter().take(limit).map(|row| row.id ^ row.received_ts).sum()
}

fn benchmark_pagination_strategies(c: &mut Criterion) {
    let rows = synthetic_reports(250_000);
    let limit = 100;
    let offset = 175_000;
    let cursor = rows[offset - 1];

    require_bench_group("pagination");

    c.bench_function("pagination_offset_deep_page", |b| {
        b.iter(|| black_box(offset_page_checksum(&rows, offset, limit)));
    });

    c.bench_function("pagination_keyset_deep_page", |b| {
        b.iter(|| black_box(keyset_page_checksum(&rows, cursor, limit)));
    });
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
        benchmark_concurrent_throughput,
        benchmark_pagination_strategies
);

/// Explicit `main` (instead of `criterion_main!`) so the strict-mode
/// check runs after all groups have had a chance to register.
fn main() {
    server_benches();
    enforce_required_groups();
}
