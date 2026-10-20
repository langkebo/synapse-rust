//! Memory-pressure detection for the test harness defaults.
//!
//! Split out of `lib.rs` (2026-10-20) so the parsing and the decision can be
//! unit-tested without a host of a particular size, and so `lib.rs` stays under
//! its line-count ceiling.

// ============================================================================
// Low-memory environment detection (2026-09-23, SIGTERM root-cause fix)
// ============================================================================
//
// On constrained hosts (sandboxed CLI tools, small CI runners, laptops), the
// historical defaults — 12-way parallel schema clones, 40-connection pool
// ceilings — drive RSS past the OOM limit and the test process dies to a
// SIGKILL/SIGTERM before the first assertion. Detect the memory situation once
// per process and scale the *defaults* down; explicit env vars still win, so CI
// machines keep their calibrated numbers.
//
// 2026-10-19: the check used to look at **physical** RAM only, so a 16 GB host
// never counted as constrained even with swap exhausted — the exact state that
// produced the SIGKILL storm this was written for. It now also looks at
// *available* memory, which is what the allocator actually has to work with.

/// Total physical RAM below which the host counts as "low memory" no matter how
/// much of it is currently free.
const LOW_MEMORY_THRESHOLD_BYTES: u64 = 6 * 1024 * 1024 * 1024;

/// Available (free + reclaimable) memory below which the host counts as "low
/// memory" even when its total is large. Two 16 GB test processes plus the
/// shared Postgres hold ~10 GB; once less than this is claimable, another
/// 12-way clone wave swaps the machine out.
const LOW_AVAILABLE_MEMORY_THRESHOLD_BYTES: u64 = 4 * 1024 * 1024 * 1024;

pub(crate) fn detect_physical_memory_bytes() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("sysctl").args(["-n", "hw.memsize"]).output().ok()?;
        let stdout = String::from_utf8(output.stdout).ok()?;
        stdout.trim().parse::<u64>().ok()
    }
    #[cfg(target_os = "linux")]
    {
        let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
        let line = meminfo.lines().find(|l| l.starts_with("MemTotal:"))?;
        let kb = line.split_whitespace().nth(1)?.parse::<u64>().ok()?;
        Some(kb.saturating_mul(1024))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    None
}

/// Memory the allocator can still claim without swapping.
///
/// Linux reports it directly (`MemAvailable`). macOS has no single field, so
/// free + inactive + speculative pages are summed — the three classes the VM
/// reclaims on demand — from `vm_stat`, whose first line carries the page size.
pub(crate) fn detect_available_memory_bytes() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("vm_stat").output().ok()?;
        let stdout = String::from_utf8(output.stdout).ok()?;
        parse_macos_vm_stat_available(&stdout)
    }
    #[cfg(target_os = "linux")]
    {
        let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
        let line = meminfo.lines().find(|l| l.starts_with("MemAvailable:"))?;
        let kb = line.split_whitespace().nth(1)?.parse::<u64>().ok()?;
        Some(kb.saturating_mul(1024))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    None
}

/// Free + inactive + speculative pages of a `vm_stat` dump, in bytes.
///
/// Split out from [`detect_available_memory_bytes`] so the parsing runs against
/// a fixture instead of whatever this machine happens to be doing.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn parse_macos_vm_stat_available(output: &str) -> Option<u64> {
    let page_size =
        output.lines().next()?.split("page size of ").nth(1)?.split_whitespace().next()?.parse::<u64>().ok()?;
    let mut pages: u64 = 0;
    for line in output.lines() {
        let Some((label, rest)) = line.split_once(':') else {
            continue;
        };
        let label = label.trim();
        if !matches!(label, "Pages free" | "Pages inactive" | "Pages speculative") {
            continue;
        }
        let count: u64 = rest.trim().trim_end_matches('.').parse().ok()?;
        pages = pages.saturating_add(count);
    }
    Some(pages.saturating_mul(page_size))
}

/// The decision, with both inputs injected so it is testable without a host of
/// a particular size. `None` means "could not measure", which must not count as
/// pressure: an unknown environment keeps the calibrated defaults.
pub(crate) fn low_memory_from(total: Option<u64>, available: Option<u64>) -> bool {
    let tight_total = total.is_some_and(|bytes| bytes < LOW_MEMORY_THRESHOLD_BYTES);
    let tight_available = available.is_some_and(|bytes| bytes < LOW_AVAILABLE_MEMORY_THRESHOLD_BYTES);
    tight_total || tight_available
}

/// True when this host cannot afford the calibrated defaults — either too
/// little RAM in total or too little claimable right now.
pub(crate) fn is_low_memory_environment() -> bool {
    use std::sync::OnceLock;
    static FLAG: OnceLock<bool> = OnceLock::new();
    *FLAG.get_or_init(|| low_memory_from(detect_physical_memory_bytes(), detect_available_memory_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bounds used by the real macOS parser: `vm_stat` prints one
    /// `Label: count.` line per page class.
    const VM_STAT_FIXTURE: &str = "Mach Virtual Memory Statistics: (page size of 16384 bytes)\n\
Pages free:                               1000.\n\
Pages active:                            9000000.\n\
Pages inactive:                          2000.\n\
Pages speculative:                        500.\n\
Pages throttled:                            0.\n";

    #[test]
    fn macos_vm_stat_counts_only_reclaimable_pages() {
        // (1000 + 2000 + 500) pages x 16384 bytes; active/throttled excluded.
        assert_eq!(parse_macos_vm_stat_available(VM_STAT_FIXTURE), Some(3500 * 16384));
    }

    #[test]
    fn macos_vm_stat_without_page_size_is_unknown() {
        assert_eq!(parse_macos_vm_stat_available("Pages free: 10.\n"), None);
    }

    /// The 2026-10-19 bug: a 16 GB host with no claimable memory must count as
    /// low even though its total is far above the total-RAM threshold.
    #[test]
    fn available_memory_can_mark_a_large_host_as_low() {
        let sixteen_gib = 16 * 1024 * 1024 * 1024;
        assert!(low_memory_from(Some(sixteen_gib), Some(1024 * 1024 * 1024)));
        assert!(!low_memory_from(Some(sixteen_gib), Some(8 * 1024 * 1024 * 1024)));
        // Small total is low regardless of what is free right now.
        assert!(low_memory_from(Some(4 * 1024 * 1024 * 1024), Some(3 * 1024 * 1024 * 1024)));
        // Unmeasurable is not pressure: keep the calibrated defaults.
        assert!(!low_memory_from(None, None));
    }
}
