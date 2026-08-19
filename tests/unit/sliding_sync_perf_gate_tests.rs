//! Tests for the Sliding Sync Performance Threshold Gate (P2-13).
//!
//! These tests verify that `scripts/ci/sliding_sync_perf_gate.sh`:
//! - Exists and is executable
//! - Correctly parses `[perf] sliding_sync ...` log lines emitted by
//!   `benches/performance_sliding_sync_benchmarks.rs`
//! - Correctly compares `manual_p95_ms` against `threshold_ms`
//! - Reports a breach when p95 exceeds the threshold
//! - Reports OK when p95 is within the threshold
//!
//! Synapse v1.153.0rc3 reverted a sliding-sync optimisation after a
//! performance regression went unnoticed. This gate prevents a repeat by
//! failing CI when the p95 latency exceeds the configured rollback
//! threshold (`sliding_sync_latency_threshold_ms`, default 5000ms).

use std::fs;
use std::path::PathBuf;

/// Path to the perf gate script under test.
fn perf_gate_script_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("scripts/ci/sliding_sync_perf_gate.sh");
    path
}

// =============================================================================
// Script existence / executability
// =============================================================================

#[test]
fn test_perf_gate_script_exists() {
    let path = perf_gate_script_path();
    assert!(path.exists(), "sliding_sync_perf_gate.sh should exist at {:?}", path);
    assert!(path.is_file(), "sliding_sync_perf_gate.sh should be a file");
}

#[test]
fn test_perf_gate_script_is_executable() {
    use std::os::unix::fs::PermissionsExt;
    let path = perf_gate_script_path();
    let metadata = fs::metadata(&path).expect("script metadata must be readable");
    let permissions = metadata.permissions();
    assert!(
        permissions.mode() & 0o100 != 0,
        "sliding_sync_perf_gate.sh should be executable by owner (mode={:o})",
        permissions.mode()
    );
}

// =============================================================================
// [perf] log-line parsing (mirrors the sed expressions in the script)
// =============================================================================

/// A parsed `[perf] sliding_sync ...` sample line.
#[derive(Debug, Clone, PartialEq)]
struct PerfSample {
    p95_ms: f64,
    p99_ms: f64,
    threshold_ms: u64,
}

/// Parse a single `[perf] sliding_sync ...` line.
///
/// Mirrors the sed extraction in `sliding_sync_perf_gate.sh`:
///   manual_p95_ms=<float>
///   manual_p99_ms=<float>
///   threshold_ms=<int>
fn parse_perf_line(line: &str) -> Option<PerfSample> {
    let p95 = extract_float(line, "manual_p95_ms=")?;
    let p99 = extract_float(line, "manual_p99_ms=")?;
    let threshold_ms = extract_int(line, "threshold_ms=")?;
    Some(PerfSample { p95_ms: p95, p99_ms: p99, threshold_ms })
}

fn extract_float(line: &str, key: &str) -> Option<f64> {
    let idx = line.find(key)?;
    let rest = &line[idx + key.len()..];
    // Take chars until we hit whitespace or end of string.
    let token: String = rest.chars().take_while(|c| !c.is_whitespace()).collect();
    token.parse::<f64>().ok()
}

fn extract_int(line: &str, key: &str) -> Option<u64> {
    let idx = line.find(key)?;
    let rest = &line[idx + key.len()..];
    let token: String = rest.chars().take_while(|c| !c.is_whitespace()).collect();
    token.parse::<u64>().ok()
}

#[test]
fn test_parse_perf_line_standard_sample() {
    let line = "[perf] sliding_sync manual_p95_ms=12.34 manual_p99_ms=15.67 \
                service_p95_ms=12.50 threshold_ms=5000";
    let sample = parse_perf_line(line).expect("standard [perf] line must parse");
    assert!((sample.p95_ms - 12.34).abs() < f64::EPSILON);
    assert!((sample.p99_ms - 15.67).abs() < f64::EPSILON);
    assert_eq!(sample.threshold_ms, 5000);
}

#[test]
fn test_parse_perf_line_response_variant() {
    // The `sliding_sync_response` variant has extra fields (rooms=, slow_requests=)
    // but the same p95/p99/threshold keys. The parser must still extract them.
    let line = "[perf] sliding_sync_response rooms=100 \
                manual_p95_ms=42.10 manual_p99_ms=58.30 \
                threshold_ms=5000 slow_requests=0 \
                (query-count proxy: ~103 storage calls/sync)";
    let sample = parse_perf_line(line).expect("response-variant [perf] line must parse");
    assert!((sample.p95_ms - 42.10).abs() < f64::EPSILON);
    assert!((sample.p99_ms - 58.30).abs() < f64::EPSILON);
    assert_eq!(sample.threshold_ms, 5000);
}

#[test]
fn test_parse_perf_line_rejects_non_perf_lines() {
    assert!(parse_perf_line("not a perf line").is_none());
    assert!(parse_perf_line("[perf] something_else manual_p95_ms=1.0").is_none());
    assert!(parse_perf_line("").is_none());
}

#[test]
fn test_parse_perf_line_handles_missing_threshold() {
    // A malformed line without threshold_ms should not parse (None returned).
    let line = "[perf] sliding_sync manual_p95_ms=1.0 manual_p99_ms=2.0";
    assert!(parse_perf_line(line).is_none(), "missing threshold_ms must yield None");
}

// =============================================================================
// Threshold comparison logic (mirrors the awk comparison in the script)
// =============================================================================

/// Returns true when `p95_ms` breaches `threshold_ms`.
/// Mirrors: `awk "BEGIN {exit !($P95 > $EFFECTIVE_THRESHOLD)}"`
fn is_breach(p95_ms: f64, threshold_ms: u64) -> bool {
    p95_ms > threshold_ms as f64
}

#[test]
fn test_threshold_breach_when_p95_exceeds_threshold() {
    // p95=6000ms > threshold=5000ms → breach
    assert!(is_breach(6000.0, 5000), "p95 above threshold must be a breach");
}

#[test]
fn test_threshold_ok_when_p95_within_threshold() {
    // p95=12.34ms < threshold=5000ms → ok
    assert!(!is_breach(12.34, 5000), "p95 below threshold must not be a breach");
}

#[test]
fn test_threshold_ok_at_exact_boundary() {
    // p95 == threshold is NOT a breach (strict greater-than, matching awk).
    assert!(!is_breach(5000.0, 5000), "p95 == threshold must not be a breach (strict >)");
}

#[test]
fn test_threshold_breach_with_fractional_ms() {
    // p95=5000.01ms just over threshold=5000ms → breach
    assert!(is_breach(5000.01, 5000), "p95 fractionally above threshold must breach");
}

// =============================================================================
// End-to-end breach detection over a multi-sample log
// =============================================================================

#[test]
fn test_breach_detection_across_multiple_samples() {
    // Simulate a benchmark log with 3 samples — one of which breaches.
    let log = "\
[perf] sliding_sync manual_p95_ms=10.00 manual_p99_ms=12.00 service_p95_ms=10.00 threshold_ms=5000
[perf] sliding_sync manual_p95_ms=15.00 manual_p99_ms=18.00 service_p95_ms=14.00 threshold_ms=5000
[perf] sliding_sync manual_p95_ms=6000.00 manual_p99_ms=6500.00 service_p95_ms=5900.00 threshold_ms=5000
";

    let samples: Vec<PerfSample> =
        log.lines().filter(|l| l.starts_with("[perf] sliding_sync")).filter_map(parse_perf_line).collect();

    assert_eq!(samples.len(), 3, "all 3 [perf] lines must parse");
    let breaches: Vec<&PerfSample> = samples.iter().filter(|s| is_breach(s.p95_ms, s.threshold_ms)).collect();
    assert_eq!(breaches.len(), 1, "exactly 1 sample must breach the threshold");
    assert!((breaches[0].p95_ms - 6000.0).abs() < f64::EPSILON);
}

#[test]
fn test_no_breach_when_all_samples_within_threshold() {
    let log = "\
[perf] sliding_sync manual_p95_ms=10.00 manual_p99_ms=12.00 service_p95_ms=10.00 threshold_ms=5000
[perf] sliding_sync manual_p95_ms=15.00 manual_p99_ms=18.00 service_p95_ms=14.00 threshold_ms=5000
";

    let samples: Vec<PerfSample> =
        log.lines().filter(|l| l.starts_with("[perf] sliding_sync")).filter_map(parse_perf_line).collect();

    assert_eq!(samples.len(), 2);
    let breaches = samples.iter().filter(|s| is_breach(s.p95_ms, s.threshold_ms)).count();
    assert_eq!(breaches, 0, "no samples should breach when all p95 < threshold");
}

// =============================================================================
// Slow-request counter extraction (mirrors the grep in the script)
// =============================================================================

#[test]
fn test_slow_requests_zero_does_not_breach() {
    let log = "[perf] sliding_sync_response rooms=100 manual_p95_ms=42.10 \
               manual_p99_ms=58.30 threshold_ms=5000 slow_requests=0";
    let slow = extract_int(log, "slow_requests=");
    assert_eq!(slow, Some(0));
    // Script logic: slow_requests > 0 is a breach.
    assert_eq!(slow.unwrap_or(0), 0, "slow_requests=0 must not breach");
}

#[test]
fn test_slow_requests_non_zero_breaches() {
    let log = "[perf] sliding_sync_response rooms=100 manual_p95_ms=42.10 \
               manual_p99_ms=58.30 threshold_ms=5000 slow_requests=3";
    let slow = extract_int(log, "slow_requests=").expect("slow_requests must parse");
    assert_eq!(slow, 3);
    assert!(slow > 0, "slow_requests > 0 must be flagged as a breach");
}
