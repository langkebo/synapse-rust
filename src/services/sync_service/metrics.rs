use super::types::*;
use super::SyncService;
use crate::storage::RoomEvent;
use std::collections::HashMap;

impl SyncService {
    pub(crate) fn count_events_by_room(room_events: &HashMap<String, Vec<RoomEvent>>) -> usize {
        room_events.values().map(Vec::len).sum()
    }

    pub(crate) fn observe_histogram(&self, name: &str, value: f64) {
        if let Some(histogram) = self.metrics.get_histogram(name) {
            histogram.observe(value);
        } else {
            self.metrics.register_histogram(name.to_string()).observe(value);
        }
    }

    pub(crate) fn increment_counter(&self, name: &str) {
        if let Some(counter) = self.metrics.get_counter(name) {
            counter.inc();
        } else {
            self.metrics.register_counter(name.to_string()).inc();
        }
    }

    pub(crate) fn record_sync_request_metrics(
        &self,
        request_kind: &str,
        total_ms: f64,
        room_count: usize,
        event_count: usize,
        is_incremental: bool,
    ) {
        self.increment_counter(&format!("{request_kind}_requests_total"));
        self.observe_histogram(&format!("{request_kind}_request_duration_ms"), total_ms);
        self.observe_histogram(&format!("{request_kind}_request_room_count"), room_count as f64);
        self.observe_histogram(&format!("{request_kind}_request_event_count"), event_count as f64);

        if is_incremental {
            self.increment_counter(&format!("{request_kind}_incremental_requests_total"));
        } else {
            self.increment_counter(&format!("{request_kind}_full_requests_total"));
        }

        if self.is_slow_request(total_ms) {
            self.increment_counter(&format!("{request_kind}_slow_requests_total"));
        }
    }

    pub(crate) fn is_slow_request(&self, total_ms: f64) -> bool {
        Self::is_slow_request_for(total_ms, self.performance.sync_slow_request_threshold_ms)
    }

    pub(crate) fn is_slow_request_for(total_ms: f64, threshold_ms: u64) -> bool {
        total_ms >= threshold_ms as f64
    }

    pub(crate) fn log_slow_sync_request(&self, snapshot: &SyncPerformanceSnapshot<'_>) {
        if self.is_slow_request(snapshot.total_ms) {
            ::tracing::event!(
                target: "sync_performance",
                ::tracing::Level::WARN,
                request_kind = snapshot.request_kind,
                user_id = snapshot.user_id,
                total_ms = snapshot.total_ms,
                room_count = snapshot.room_count,
                event_count = snapshot.event_count,
                is_incremental = snapshot.is_incremental,
                phase_one_name = snapshot.phases[0].0,
                phase_one_ms = snapshot.phases[0].1,
                phase_two_name = snapshot.phases[1].0,
                phase_two_ms = snapshot.phases[1].1,
                phase_three_name = snapshot.phases[2].0,
                phase_three_ms = snapshot.phases[2].1,
                "Slow sync request detected"
            );
        }
    }

    pub(crate) fn sync_event_limit(&self) -> i64 {
        i64::from(self.performance.sync_event_limit)
    }

    pub(crate) fn sync_to_device_limit(&self) -> i64 {
        i64::from(self.performance.sync_to_device_limit)
    }

    pub(crate) fn sync_ephemeral_limit(&self) -> i64 {
        i64::from(self.performance.sync_ephemeral_limit)
    }

    pub(crate) fn sync_poll_interval(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.performance.sync_poll_interval_ms)
    }
}
