//! Memory Usage Analysis Module
//!
//! This module provides utilities for analyzing memory usage
//! and detecting memory leaks in the federation module.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::RwLock;
use std::time::Instant;

#[derive(Debug)]
pub struct MemoryStats {
    pub allocations: AtomicUsize,
    pub deallocations: AtomicUsize,
    pub current_size: AtomicUsize,
    pub peak_size: AtomicUsize,
    pub operation_count: AtomicUsize,
    last_operation_time: RwLock<Instant>,
}

impl Default for MemoryStats {
    fn default() -> Self {
        Self {
            allocations: AtomicUsize::new(0),
            deallocations: AtomicUsize::new(0),
            current_size: AtomicUsize::new(0),
            peak_size: AtomicUsize::new(0),
            operation_count: AtomicUsize::new(0),
            last_operation_time: RwLock::new(Instant::now()),
        }
    }
}

impl MemoryStats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_allocation(&self, size: usize) {
        self.allocations.fetch_add(1, Ordering::SeqCst);
        let new_current = self.current_size.fetch_add(size, Ordering::SeqCst) + size;
        self.operation_count.fetch_add(1, Ordering::SeqCst);

        let mut last_time = self.last_operation_time.write().unwrap();
        *last_time = Instant::now();
        drop(last_time);

        let mut peak = self.peak_size.load(Ordering::SeqCst);
        while new_current > peak {
            match self.peak_size.compare_exchange(
                peak,
                new_current,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break,
                Err(current) => peak = current,
            }
        }
    }

    pub fn record_deallocation(&self, size: usize) {
        self.deallocations.fetch_add(1, Ordering::SeqCst);
        self.current_size.fetch_sub(size, Ordering::SeqCst);
    }

    pub fn get_stats(&self) -> MemoryStatsSnapshot {
        MemoryStatsSnapshot {
            total_allocations: self.allocations.load(Ordering::SeqCst),
            total_deallocations: self.deallocations.load(Ordering::SeqCst),
            current_size: self.current_size.load(Ordering::SeqCst),
            peak_size: self.peak_size.load(Ordering::SeqCst),
            operation_count: self.operation_count.load(Ordering::SeqCst),
        }
    }

    pub fn get_utilization_rate(&self) -> f64 {
        let current = self.current_size.load(Ordering::SeqCst);
        let peak = self.peak_size.load(Ordering::SeqCst);
        if peak == 0 {
            0.0
        } else {
            current as f64 / peak as f64
        }
    }
}

#[derive(Debug, Clone)]
pub struct MemoryStatsSnapshot {
    pub total_allocations: usize,
    pub total_deallocations: usize,
    pub current_size: usize,
    pub peak_size: usize,
    pub operation_count: usize,
}

impl MemoryStatsSnapshot {
    pub fn leak_count(&self) -> usize {
        self.total_allocations
            .saturating_sub(self.total_deallocations)
    }

    pub fn leak_percentage(&self) -> f64 {
        if self.total_allocations == 0 {
            0.0
        } else {
            (self.leak_count() as f64 / self.total_allocations as f64) * 100.0
        }
    }
}

#[derive(Default)]
pub struct FederationMemoryTracker {
    event_cache_stats: MemoryStats,
    auth_chain_stats: MemoryStats,
    key_cache_stats: MemoryStats,
    state_resolution_stats: MemoryStats,
}

impl FederationMemoryTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_event_cached(&self, size: usize) {
        self.event_cache_stats.record_allocation(size);
    }

    pub fn record_event_removed(&self, size: usize) {
        self.event_cache_stats.record_deallocation(size);
    }

    pub fn record_auth_chain_operation(&self, size: usize) {
        self.auth_chain_stats.record_allocation(size);
    }

    pub fn record_key_cached(&self, size: usize) {
        self.key_cache_stats.record_allocation(size);
    }

    pub fn record_key_removed(&self, size: usize) {
        self.key_cache_stats.record_deallocation(size);
    }

    pub fn record_state_resolution(&self, size: usize) {
        self.state_resolution_stats.record_allocation(size);
    }

    pub fn get_report(&self) -> FederationMemoryReport {
        FederationMemoryReport {
            event_cache: self.event_cache_stats.get_stats(),
            auth_chain: self.auth_chain_stats.get_stats(),
            key_cache: self.key_cache_stats.get_stats(),
            state_resolution: self.state_resolution_stats.get_stats(),
            total_current: self.event_cache_stats.get_stats().current_size
                + self.auth_chain_stats.get_stats().current_size
                + self.key_cache_stats.get_stats().current_size
                + self.state_resolution_stats.get_stats().current_size,
            total_peak: self.event_cache_stats.get_stats().peak_size
                + self.auth_chain_stats.get_stats().peak_size
                + self.key_cache_stats.get_stats().peak_size
                + self.state_resolution_stats.get_stats().peak_size,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FederationMemoryReport {
    pub event_cache: MemoryStatsSnapshot,
    pub auth_chain: MemoryStatsSnapshot,
    pub key_cache: MemoryStatsSnapshot,
    pub state_resolution: MemoryStatsSnapshot,
    pub total_current: usize,
    pub total_peak: usize,
}

impl FederationMemoryReport {
    pub fn format_human_readable(&self) -> String {
        format!(
            "=== Federation Memory Report ===

Event Cache:
  - Allocations: {}
  - Deallocations: {}
  - Current Size: {:.2} KB
  - Peak Size: {:.2} KB
  - Leak Count: {}

Auth Chain:
  - Allocations: {}
  - Deallocations: {}
  - Current Size: {:.2} KB
  - Peak Size: {:.2} KB
  - Leak Count: {}

Key Cache:
  - Allocations: {}
  - Deallocations: {}
  - Current Size: {:.2} KB
  - Peak Size: {:.2} KB
  - Leak Count: {}

State Resolution:
  - Allocations: {}
  - Deallocations: {}
  - Current Size: {:.2} KB
  - Peak Size: {:.2} KB
  - Leak Count: {}

Total:
  - Current Size: {:.2} KB
  - Peak Size: {:.2} KB
",
            self.event_cache.total_allocations,
            self.event_cache.total_deallocations,
            self.event_cache.current_size as f64 / 1024.0,
            self.event_cache.peak_size as f64 / 1024.0,
            self.event_cache.leak_count(),
            self.auth_chain.total_allocations,
            self.auth_chain.total_deallocations,
            self.auth_chain.current_size as f64 / 1024.0,
            self.auth_chain.peak_size as f64 / 1024.0,
            self.auth_chain.leak_count(),
            self.key_cache.total_allocations,
            self.key_cache.total_deallocations,
            self.key_cache.current_size as f64 / 1024.0,
            self.key_cache.peak_size as f64 / 1024.0,
            self.key_cache.leak_count(),
            self.state_resolution.total_allocations,
            self.state_resolution.total_deallocations,
            self.state_resolution.current_size as f64 / 1024.0,
            self.state_resolution.peak_size as f64 / 1024.0,
            self.state_resolution.leak_count(),
            self.total_current as f64 / 1024.0,
            self.total_peak as f64 / 1024.0,
        )
    }
}

#[cfg(test)]
mod memory_tests {
    use super::*;

    #[test]
    fn test_memory_stats_allocation() {
        let stats = MemoryStats::new();

        stats.record_allocation(100);
        stats.record_allocation(200);
        stats.record_deallocation(50);

        let snapshot = stats.get_stats();

        assert_eq!(snapshot.total_allocations, 2);
        assert_eq!(snapshot.total_deallocations, 1);
        assert_eq!(snapshot.current_size, 250);
        assert_eq!(snapshot.peak_size, 300);
    }

    #[test]
    fn test_memory_leak_detection() {
        let stats = MemoryStats::new();

        stats.record_allocation(100);
        stats.record_allocation(200);
        stats.record_deallocation(100);

        let snapshot = stats.get_stats();

        assert_eq!(snapshot.leak_count(), 1);
        assert!((snapshot.leak_percentage() - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_federation_memory_tracker() {
        let tracker = FederationMemoryTracker::new();

        tracker.record_event_cached(1024);
        tracker.record_event_cached(2048);
        tracker.record_event_removed(1024);
        tracker.record_key_cached(512);

        let report = tracker.get_report();

        assert_eq!(report.event_cache.current_size, 2048);
        assert_eq!(report.key_cache.current_size, 512);
        assert_eq!(report.total_current, 2560);
    }

    #[test]
    fn test_memory_report_formatting() {
        let tracker = FederationMemoryTracker::new();
        tracker.record_event_cached(2048);

        let report = tracker.get_report();
        let formatted = report.format_human_readable();

        assert!(formatted.contains("Event Cache"));
        assert!(formatted.contains("2.00 KB"));
    }
}
