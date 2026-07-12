//! synapse-rust Runtime Performance Benchmark Harness
//!
//! Measures end-to-end HTTP latency (p50/p95/p99), QPS, RSS, and DB pool metrics
//! against a running homeserver.
//!
//! Compile:  cargo build --release --bin bench_harness
//! Run:      BENCH_ADMIN_TOKEN=<token> .gstack/run_bench_harness.sh
//!
//! Environment:
//!   BENCH_BASE_URL       Server URL (default: http://localhost:8008)
//!   BENCH_ADMIN_TOKEN    Admin access token for authenticated endpoints
//!   BENCH_WARMUP_SECS    Warmup seconds (default: 10)
//!   BENCH_RUNTIME_SECS   Measurement seconds per endpoint (default: 30)
//!   BENCH_CONCURRENCY    Concurrent workers (default: 16)
//!   BENCH_OUTPUT         Output JSON path (default: .gstack/bench_results.json)

use std::env;
use std::fs;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Semaphore;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

struct Config {
    base_url: String,
    admin_token: Option<String>,
    warmup_secs: u64,
    runtime_secs: u64,
    concurrency: usize,
    output_path: String,
    endpoint_filter: Option<String>,
}

impl Config {
    fn from_env() -> Self {
        Self {
            base_url: env::var("BENCH_BASE_URL").unwrap_or_else(|_| "http://localhost:8008".into()),
            admin_token: env::var("BENCH_ADMIN_TOKEN").ok().filter(|t| !t.is_empty()),
            warmup_secs: env::var("BENCH_WARMUP_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(10),
            runtime_secs: env::var("BENCH_RUNTIME_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(30),
            concurrency: env::var("BENCH_CONCURRENCY").ok().and_then(|v| v.parse().ok()).unwrap_or(16),
            output_path: env::var("BENCH_OUTPUT").unwrap_or_else(|_| ".gstack/bench_results.json".into()),
            endpoint_filter: env::var("BENCH_ENDPOINTS").ok().filter(|v| !v.is_empty()),
        }
    }
}

// ---------------------------------------------------------------------------
// Statistics (no external crate — plain f64 sorting)
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Serialize)]
struct LatencyStats {
    label: String,
    count: usize,
    mean_ms: f64,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    min_ms: f64,
    max_ms: f64,
    errors: usize,
    qps: f64,
}

fn compute_stats(label: &str, mut lats: Vec<f64>, duration_s: f64, errors: usize) -> LatencyStats {
    if lats.is_empty() {
        return LatencyStats {
            label: label.into(),
            count: 0,
            mean_ms: 0.0,
            p50_ms: 0.0,
            p95_ms: 0.0,
            p99_ms: 0.0,
            min_ms: 0.0,
            max_ms: 0.0,
            errors,
            qps: 0.0,
        };
    }
    lats.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = lats.len();
    let mean = lats.iter().sum::<f64>() / n as f64;
    LatencyStats {
        label: label.into(),
        count: n,
        mean_ms: mean,
        p50_ms: lats[((n - 1) as f64 * 0.50).round() as usize],
        p95_ms: lats[((n - 1) as f64 * 0.95).round() as usize],
        p99_ms: lats[((n - 1) as f64 * 0.99).round() as usize],
        min_ms: lats[0],
        max_ms: lats[n - 1],
        errors,
        qps: if duration_s > 0.0 { n as f64 / duration_s } else { 0.0 },
    }
}

// ---------------------------------------------------------------------------
// Endpoint definition
// ---------------------------------------------------------------------------

struct Endpoint {
    name: &'static str,
    method: &'static str,
    path: &'static str,
    body: Option<serde_json::Value>,
    needs_auth: bool,
}

// ---------------------------------------------------------------------------
// Server probe
// ---------------------------------------------------------------------------

async fn server_ready(base_url: &str) -> bool {
    reqwest::get(format!("{base_url}/_matrix/client/versions")).await.map(|r| r.status().is_success()).unwrap_or(false)
}

// ---------------------------------------------------------------------------
// RSS measurement
// ---------------------------------------------------------------------------

fn measure_server_rss_mb(pid: u32) -> Option<f64> {
    #[cfg(target_os = "linux")]
    {
        if let Ok(data) = fs::read_to_string(format!("/proc/{pid}/status")) {
            for line in data.lines() {
                if line.starts_with("VmRSS:") {
                    return line.split_whitespace().nth(1).and_then(|s| s.parse::<f64>().ok()).map(|kb| kb / 1024.0);
                }
            }
        }
    }
    // macOS (and fallback): use ps
    if let Ok(out) = Command::new("ps").args(["-o", "rss=", "-p", &pid.to_string()]).output() {
        let s = String::from_utf8_lossy(&out.stdout);
        if let Ok(kb) = s.trim().parse::<f64>() {
            return Some(kb / 1024.0);
        }
    }
    None
}

fn detect_server_pid() -> Option<u32> {
    // Check PID file written by run_bench_server.sh
    if let Ok(s) = fs::read_to_string(".gstack/bench_server.pid") {
        if let Ok(pid) = s.trim().parse::<u32>() {
            return Some(pid);
        }
    }
    // Try pgrep
    if let Ok(out) = Command::new("pgrep").args(["-f", "target/release/synapse-rust"]).output() {
        if let Some(line) = String::from_utf8_lossy(&out.stdout).lines().next() {
            return line.trim().parse::<u32>().ok();
        }
    }
    eprintln!("[harness] could not detect server PID");
    None
}

// ---------------------------------------------------------------------------
// Benchmark runner
// ---------------------------------------------------------------------------

async fn measure_endpoint(client: &reqwest::Client, config: &Config, ep: &Endpoint) -> LatencyStats {
    let sem = Arc::new(Semaphore::new(config.concurrency));
    let measure_deadline = Instant::now() + Duration::from_secs(config.warmup_secs + config.runtime_secs);
    let collect_start = Instant::now() + Duration::from_secs(config.warmup_secs);

    let mut handles = Vec::new();
    for _ in 0..config.concurrency {
        let sem = sem.clone();
        let client = client.clone();
        let url = format!("{}{}", config.base_url, ep.path);
        let method = ep.method.to_string();
        let body = ep.body.clone();
        let needs_auth = ep.needs_auth;
        let token = config.admin_token.clone();
        let deadline = measure_deadline;
        let collect = collect_start;

        handles.push(tokio::spawn(async move {
            let mut lats = Vec::new();
            let mut errs = 0usize;
            let mut iter = 0u64;
            let task_start = Instant::now();
            while Instant::now() < deadline {
                iter += 1;
                let _permit = sem.acquire().await.unwrap();
                let mut req = match method.as_str() {
                    "GET" => client.get(&url),
                    "POST" => {
                        let r = client.post(&url).header("Content-Type", "application/json");
                        if let Some(ref b) = body {
                            r.json(b)
                        } else {
                            r
                        }
                    }
                    _ => client.get(&url),
                };

                if needs_auth {
                    if let Some(ref t) = token {
                        req = req.header("Authorization", format!("Bearer {t}"));
                    }
                }

                let start = Instant::now();
                let elapsed = match req.send().await {
                    Ok(resp) => {
                        let _code = resp.status().as_u16();
                        let _ = resp.bytes().await;
                        start.elapsed().as_secs_f64() * 1000.0
                    }
                    Err(e) => {
                        errs += 1;
                        if iter <= 3 {
                            eprintln!("  [task debug] request error: {e:?}");
                        }
                        start.elapsed().as_secs_f64() * 1000.0
                    }
                };

                // Only record measurements after warmup
                if Instant::now() >= collect {
                    lats.push(elapsed);
                }
            }
            let task_elapsed = task_start.elapsed().as_secs_f64();
            if iter > 0 && iter < 10 {
                eprintln!("  [task debug] {iter} iterations in {task_elapsed:.1}s, {errs} errors");
            }
            (lats, errs)
        }));
    }

    let mut all_lats = Vec::new();
    let mut total_errs = 0usize;
    for h in handles {
        if let Ok((lats, errs)) = h.await {
            all_lats.extend(lats);
            total_errs += errs;
        }
    }

    compute_stats(ep.name, all_lats, config.runtime_secs as f64, total_errs)
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let config = Config::from_env();

    eprintln!("=== synapse-rust Performance Benchmark Harness ===");
    eprintln!("Server:  {}", config.base_url);
    eprintln!("Auth:    {}", if config.admin_token.is_some() { "configured" } else { "MISSING" });
    eprintln!(
        "Warmup:  {}s  Runtime: {}s/endpoint  Concurrency: {}",
        config.warmup_secs, config.runtime_secs, config.concurrency
    );

    if !server_ready(&config.base_url).await {
        eprintln!("ERROR: Server unreachable at {}", config.base_url);
        std::process::exit(1);
    }
    eprintln!("Status:  server reachable");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .pool_idle_timeout(Duration::from_secs(90))
        .pool_max_idle_per_host(config.concurrency)
        .tcp_keepalive(Duration::from_secs(60))
        .no_proxy()
        .build()
        .unwrap();

    // RSS before
    let server_pid = detect_server_pid();
    let rss_before = server_pid.and_then(|p| measure_server_rss_mb(p));

    // Define endpoints
    let endpoints = [
        Endpoint { name: "versions", method: "GET", path: "/_matrix/client/versions", body: None, needs_auth: false },
        Endpoint {
            name: "whoami",
            method: "GET",
            path: "/_matrix/client/r0/account/whoami",
            body: None,
            needs_auth: true,
        },
        Endpoint {
            name: "sync_short",
            method: "GET",
            path: "/_matrix/client/r0/sync?timeout=100",
            body: None,
            needs_auth: true,
        },
        Endpoint {
            name: "sync_long",
            method: "GET",
            path: "/_matrix/client/r0/sync?timeout=1000",
            body: None,
            needs_auth: true,
        },
        Endpoint {
            name: "room_messages_b",
            method: "GET",
            path: "/_matrix/client/r0/rooms/!bench_room_00000:localhost/messages?dir=b&limit=50",
            body: None,
            needs_auth: true,
        },
        Endpoint {
            name: "room_messages_f",
            method: "GET",
            path: "/_matrix/client/r0/rooms/!bench_room_00000:localhost/messages?dir=f&limit=50",
            body: None,
            needs_auth: true,
        },
        Endpoint {
            name: "room_members",
            method: "GET",
            path: "/_matrix/client/r0/rooms/!bench_room_00000:localhost/members",
            body: None,
            needs_auth: true,
        },
    ];

    // Filter endpoints requiring auth if no token
    let mut filtered: Vec<&Endpoint> =
        endpoints.iter().filter(|ep| !ep.needs_auth || config.admin_token.is_some()).collect();

    // Apply endpoint name filter if set
    if let Some(ref filter_str) = config.endpoint_filter {
        let names: Vec<&str> = filter_str.split(',').map(|s| s.trim()).collect();
        filtered.retain(|ep| names.contains(&ep.name));
        eprintln!("Filter:  only benchmarking [{}]", filter_str);
    }

    // Run benchmarks
    let mut results = Vec::new();
    for ep in &filtered {
        eprintln!("Benchmarking {}...", ep.name);
        let stats = measure_endpoint(&client, &config, ep).await;
        eprintln!(
            "  p50={:.1}ms p95={:.1}ms p99={:.1}ms qps={:.1} errs={}",
            stats.p50_ms, stats.p95_ms, stats.p99_ms, stats.qps, stats.errors
        );
        results.push(stats);
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    // RSS after
    let rss_after = server_pid.and_then(|p| measure_server_rss_mb(p));
    let rss_peak = match (rss_before, rss_after) {
        (Some(b), Some(a)) => Some(b.max(a)),
        (Some(v), _) | (_, Some(v)) => Some(v),
        _ => None,
    };

    // Build report
    let total_requests: usize = results.iter().map(|r| r.count).sum();
    let total_errors: usize = results.iter().map(|r| r.errors).sum();
    let aggregate_qps: f64 = results.iter().map(|r| r.qps).sum();

    #[derive(serde::Serialize)]
    struct Report {
        schema_version: &'static str,
        date: String,
        base_url: String,
        concurrency: usize,
        warmup_secs: u64,
        runtime_secs: u64,
        server_pid: Option<u32>,
        server_rss_peak_mb: Option<f64>,
        total_requests: usize,
        total_errors: usize,
        aggregate_qps: f64,
        results: Vec<LatencyStats>,
    }

    let now = std::process::Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".into());

    let report = Report {
        schema_version: "1.0.0",
        date: now,
        base_url: config.base_url.clone(),
        concurrency: config.concurrency,
        warmup_secs: config.warmup_secs,
        runtime_secs: config.runtime_secs,
        server_pid,
        server_rss_peak_mb: rss_peak,
        total_requests,
        total_errors,
        aggregate_qps,
        results,
    };

    // Output
    if let Some(parent) = std::path::Path::new(&config.output_path).parent() {
        let _ = fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(&report).unwrap();
    fs::write(&config.output_path, &json).unwrap();

    // Summary table
    println!();
    println!("{:-^70}", " PERFORMANCE BENCHMARK RESULTS ");
    println!(
        "Server: {} | Concurrency: {} | RSS peak: {:.1} MB",
        config.base_url,
        config.concurrency,
        rss_peak.unwrap_or(0.0)
    );
    println!("{:-<70}", "");
    println!("{:<25} {:>8} {:>8} {:>8} {:>10}", "Endpoint", "p50", "p95", "p99", "QPS");
    println!("{:-<70}", "");
    for r in &report.results {
        println!("{:<25} {:>7.1}ms {:>7.1}ms {:>7.1}ms {:>9.1}", r.label, r.p50_ms, r.p95_ms, r.p99_ms, r.qps);
    }
    println!("{:-<70}", "");
    println!("Total: {} requests | {} errors | {:.1} aggregate QPS", total_requests, total_errors, aggregate_qps);
    println!("Results: {}", config.output_path);
    println!("{:-^70}", "");
}
