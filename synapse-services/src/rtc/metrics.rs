//! RTC 统一指标
//!
//! 所有 RTC 子服务共享的 Prometheus 指标。

use std::sync::atomic::{AtomicU64, Ordering};

static TURN_CREDENTIALS_ISSUED: AtomicU64 = AtomicU64::new(0);
static CALL_STARTED: AtomicU64 = AtomicU64::new(0);
static CALL_ENDED: AtomicU64 = AtomicU64::new(0);
static SESSION_CREATED: AtomicU64 = AtomicU64::new(0);
static MEMBERSHIP_CREATED: AtomicU64 = AtomicU64::new(0);

pub struct RtcMetrics;

impl RtcMetrics {
    pub fn increment_turn_credentials_issued() {
        TURN_CREDENTIALS_ISSUED.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_call_started() {
        CALL_STARTED.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_call_ended() {
        CALL_ENDED.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_session_created(_application: &str) {
        SESSION_CREATED.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_membership_created() {
        MEMBERSHIP_CREATED.fetch_add(1, Ordering::Relaxed);
    }

    pub fn turn_credentials_issued() -> u64 {
        TURN_CREDENTIALS_ISSUED.load(Ordering::Relaxed)
    }

    pub fn call_started() -> u64 {
        CALL_STARTED.load(Ordering::Relaxed)
    }

    pub fn call_ended() -> u64 {
        CALL_ENDED.load(Ordering::Relaxed)
    }

    pub fn session_created() -> u64 {
        SESSION_CREATED.load(Ordering::Relaxed)
    }

    pub fn membership_created() -> u64 {
        MEMBERSHIP_CREATED.load(Ordering::Relaxed)
    }
}
