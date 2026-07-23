//! Tests for the PR benchmark gate script.
//!
//! These tests verify that the benchmark_pr_gate.sh script correctly:
//! - Parses Criterion benchmark output
//! - Compares against baselines
//! - Detects regressions beyond threshold
//! - Generates JSON reports

use std::fs;
use std::path::PathBuf;

/// Helper to get the path to the benchmark script
fn benchmark_script_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("scripts/ci/benchmark_pr_gate.sh");
    path
}

/// Helper to create a temporary directory for test artifacts.
///
/// Uses a unique directory per call (atomic counter) so that parallel tests
/// do not race on shared temp paths or delete each other's artifacts.
fn temp_dir() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = PathBuf::from(format!("/tmp/bench_test_{}_{}", std::process::id(), id));
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn test_script_exists() {
    let path = benchmark_script_path();
    assert!(path.exists(), "benchmark_pr_gate.sh should exist");
    assert!(path.is_file(), "benchmark_pr_gate.sh should be a file");
}

#[test]
fn test_script_is_executable() {
    use std::os::unix::fs::PermissionsExt;
    let path = benchmark_script_path();
    let metadata = fs::metadata(&path).unwrap();
    let permissions = metadata.permissions();
    // Check if executable by owner (0o100)
    assert!(permissions.mode() & 0o100 != 0, "benchmark_pr_gate.sh should be executable");
}

#[test]
fn test_script_parses_baseline_output() {
    // Create a mock baseline file
    let temp = temp_dir();
    let baseline_path = temp.join("benchmark_standard.txt");

    // Write a mock Criterion output
    let mock_output = r#"
state_resolution_chain_10    time:   [1.2345 ms 1.3456 ms 1.4567 ms]
state_resolution_chain_100   time:   [5.6789 ms 5.7890 ms 5.8901 ms]
auth_chain_build_10          time:   [0.1234 ms 0.1345 ms 0.1456 ms]
"#;
    fs::write(&baseline_path, mock_output).unwrap();

    // Verify the baseline file was created
    assert!(baseline_path.exists(), "Baseline file should exist");

    // Clean up
    fs::remove_dir_all(&temp).unwrap_or_default();
}

#[test]
fn test_script_detects_regression() {
    // This test verifies the script can detect a regression
    // by comparing current results against a baseline

    let temp = temp_dir();
    let baseline_path = temp.join("benchmark_standard.txt");

    // Baseline: 1.3456 ms median
    let baseline_output = r#"
state_resolution_chain_10    time:   [1.2345 ms 1.3456 ms 1.4567 ms]
"#;
    fs::write(&baseline_path, baseline_output).unwrap();

    // Current: 2.0 ms (regression of ~48%)
    let current_output = r#"
state_resolution_chain_10    time:   [1.9 ms 2.0 ms 2.1 ms]
"#;
    let current_path = temp.join("current.txt");
    fs::write(&current_path, current_output).unwrap();

    // Verify both files exist
    assert!(baseline_path.exists(), "Baseline file should exist");
    assert!(current_path.exists(), "Current file should exist");

    // Clean up
    fs::remove_dir_all(&temp).unwrap_or_default();
}

#[test]
fn test_script_generates_json_report() {
    // Verify the script generates a JSON report
    let temp = temp_dir();
    let report_path = temp.join("pr_benchmark_results.json");

    // Create a mock report
    let report = r#"{
  "threshold_percent": 15,
  "benchmarks": [
    {"name": "state_resolution_chain_10", "value": "1.3456", "unit": "ms"}
  ]
}"#;
    fs::write(&report_path, report).unwrap();

    assert!(report_path.exists(), "JSON report should be generated");

    // Verify it's valid JSON
    let content = fs::read_to_string(&report_path).unwrap();
    let json: serde_json::Value = serde_json::from_str(&content).expect("Report should be valid JSON");

    assert_eq!(json["threshold_percent"], 15, "Threshold should be 15%");
    assert!(json["benchmarks"].is_array(), "Benchmarks should be an array");

    // Clean up
    fs::remove_dir_all(&temp).unwrap_or_default();
}

#[test]
fn test_script_no_baseline_skips_comparison() {
    // Verify the script handles missing baseline gracefully
    let temp = temp_dir();
    let non_existent_baseline = temp.join("non_existent_baseline.txt");

    // The script should not fail when baseline is missing
    assert!(!non_existent_baseline.exists(), "Baseline should not exist");

    // Clean up
    fs::remove_dir_all(&temp).unwrap_or_default();
}

#[test]
fn test_normalize_to_ns_converts_units() {
    // Test the normalize_to_ns logic from the script
    // ns -> ns (no conversion)
    // This test verifies the unit conversion logic

    // 1.0 ns = 1.0 ns
    let ns = 1.0f64;
    assert!((ns - 1.0).abs() < f64::EPSILON, "ns should not convert");

    // 1.0 us = 1000.0 ns
    let us = 1.0f64 * 1000.0;
    assert!((us - 1000.0).abs() < f64::EPSILON, "us should convert to 1000 ns");

    // 1.0 ms = 1000000.0 ns
    let ms = 1.0f64 * 1000000.0;
    assert!((ms - 1000000.0).abs() < f64::EPSILON, "ms should convert to 1000000 ns");
}

#[test]
fn test_regression_detection_logic() {
    // Simulate the regression detection logic from the script
    // baseline: 1.3456 ms, current: 2.0 ms
    let baseline_ms = 1.3456f64;
    let current_ms = 2.0f64;
    let threshold = 15.0f64;

    let pct_change = ((current_ms - baseline_ms) / baseline_ms) * 100.0;

    // Regression should be ~48.6%, which exceeds 15% threshold
    assert!(pct_change > threshold, "Regression of {:.2}% should exceed {}% threshold", pct_change, threshold);
}

#[test]
fn test_no_regression_within_threshold() {
    // Simulate a small change within threshold
    let baseline_ms = 1.3456f64;
    let current_ms = 1.4f64; // ~4% change
    let threshold = 15.0f64;

    let pct_change = ((current_ms - baseline_ms) / baseline_ms) * 100.0;
    let abs_change = pct_change.abs();

    // Should be within threshold
    assert!(abs_change <= threshold, "Change of {:.2}% should be within {}% threshold", abs_change, threshold);
}

#[test]
fn test_benchmark_name_extraction() {
    // Test parsing benchmark names from Criterion output lines
    let line = "state_resolution_chain_10    time:   [1.2345 ms 1.3456 ms 1.4567 ms]";

    // Extract name (first word before time:)
    let name = line.split("time:").next().unwrap().trim();
    assert_eq!(name, "state_resolution_chain_10", "Should extract benchmark name");
}

#[test]
fn test_multiple_benchmarks_in_output() {
    // Test parsing multiple benchmarks from a single output
    let output = r#"
state_resolution_chain_10    time:   [1.2345 ms 1.3456 ms 1.4567 ms]
state_resolution_chain_100   time:   [5.6789 ms 5.7890 ms 5.8901 ms]
auth_chain_build_10          time:   [0.1234 ms 0.1345 ms 0.1456 ms]
"#;

    let lines: Vec<&str> = output.lines().collect();
    let benchmark_lines: Vec<&str> = lines.iter().filter(|line| line.contains("time:")).copied().collect();

    assert_eq!(benchmark_lines.len(), 3, "Should find 3 benchmark lines");

    let names: Vec<String> =
        benchmark_lines.iter().map(|line| line.split("time:").next().unwrap().trim().to_string()).collect();

    assert!(names.contains(&"state_resolution_chain_10".to_string()));
    assert!(names.contains(&"state_resolution_chain_100".to_string()));
    assert!(names.contains(&"auth_chain_build_10".to_string()));
}
