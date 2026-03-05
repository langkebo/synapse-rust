use crate::common::metrics::{Counter, Gauge, Histogram, MetricsCollector};
use std::collections::HashMap;
use std::sync::Arc;

pub struct ServerMetrics {
    pub auth_attempts_total: Counter,
    pub auth_failures_total: Counter,
    pub auth_success_total: Counter,
    pub token_validations_total: Counter,
    pub token_validation_errors: Counter,
    
    pub db_query_duration: Histogram,
    pub db_connections_active: Gauge,
    pub db_connections_idle: Gauge,
    pub db_query_errors: Counter,
    pub db_transaction_duration: Histogram,
    
    pub cache_hits_total: Counter,
    pub cache_misses_total: Counter,
    pub cache_evictions_total: Counter,
    pub cache_errors: Counter,
    
    pub federation_requests_total: Counter,
    pub federation_request_duration: Histogram,
    pub federation_signature_verifications: Counter,
    pub federation_signature_errors: Counter,
    pub federation_replay_attacks_blocked: Counter,
    
    pub http_requests_total: Counter,
    pub http_request_duration: Histogram,
    pub http_request_errors: Counter,
    pub http_active_requests: Gauge,
    
    pub security_jwt_validation_errors: Counter,
    pub security_origin_validation_errors: Counter,
    pub security_timestamp_validation_errors: Counter,
    
    pub pool_health_status: Gauge,
    pub pool_utilization: Gauge,
    
    collector: Arc<MetricsCollector>,
}

impl ServerMetrics {
    pub fn new(collector: Arc<MetricsCollector>) -> Self {
        Self {
            auth_attempts_total: collector.register_counter_with_labels(
                "auth_attempts_total".to_string(),
                Self::labels(&[("type", "attempt")]),
            ),
            auth_failures_total: collector.register_counter_with_labels(
                "auth_failures_total".to_string(),
                Self::labels(&[("type", "failure")]),
            ),
            auth_success_total: collector.register_counter_with_labels(
                "auth_success_total".to_string(),
                Self::labels(&[("type", "success")]),
            ),
            token_validations_total: collector.register_counter(
                "token_validations_total".to_string(),
            ),
            token_validation_errors: collector.register_counter(
                "token_validation_errors".to_string(),
            ),

            db_query_duration: collector.register_histogram_with_labels(
                "db_query_duration_ms".to_string(),
                Self::labels(&[("unit", "ms")]),
            ),
            db_connections_active: collector.register_gauge(
                "db_connections_active".to_string(),
            ),
            db_connections_idle: collector.register_gauge(
                "db_connections_idle".to_string(),
            ),
            db_query_errors: collector.register_counter("db_query_errors".to_string()),
            db_transaction_duration: collector.register_histogram_with_labels(
                "db_transaction_duration_ms".to_string(),
                Self::labels(&[("unit", "ms")]),
            ),

            cache_hits_total: collector.register_counter_with_labels(
                "cache_hits_total".to_string(),
                Self::labels(&[("result", "hit")]),
            ),
            cache_misses_total: collector.register_counter_with_labels(
                "cache_misses_total".to_string(),
                Self::labels(&[("result", "miss")]),
            ),
            cache_evictions_total: collector.register_counter(
                "cache_evictions_total".to_string(),
            ),
            cache_errors: collector.register_counter("cache_errors".to_string()),

            federation_requests_total: collector.register_counter(
                "federation_requests_total".to_string(),
            ),
            federation_request_duration: collector.register_histogram_with_labels(
                "federation_request_duration_ms".to_string(),
                Self::labels(&[("unit", "ms")]),
            ),
            federation_signature_verifications: collector.register_counter(
                "federation_signature_verifications".to_string(),
            ),
            federation_signature_errors: collector.register_counter(
                "federation_signature_errors".to_string(),
            ),
            federation_replay_attacks_blocked: collector.register_counter(
                "federation_replay_attacks_blocked".to_string(),
            ),

            http_requests_total: collector.register_counter("http_requests_total".to_string()),
            http_request_duration: collector.register_histogram_with_labels(
                "http_request_duration_ms".to_string(),
                Self::labels(&[("unit", "ms")]),
            ),
            http_request_errors: collector.register_counter("http_request_errors".to_string()),
            http_active_requests: collector.register_gauge("http_active_requests".to_string()),

            security_jwt_validation_errors: collector.register_counter(
                "security_jwt_validation_errors".to_string(),
            ),
            security_origin_validation_errors: collector.register_counter(
                "security_origin_validation_errors".to_string(),
            ),
            security_timestamp_validation_errors: collector.register_counter(
                "security_timestamp_validation_errors".to_string(),
            ),

            pool_health_status: collector.register_gauge("pool_health_status".to_string()),
            pool_utilization: collector.register_gauge("pool_utilization".to_string()),

            collector,
        }
    }

    fn labels(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    pub fn record_auth_attempt(&self, success: bool) {
        self.auth_attempts_total.inc();
        if success {
            self.auth_success_total.inc();
        } else {
            self.auth_failures_total.inc();
        }
    }

    pub fn record_token_validation(&self, success: bool) {
        self.token_validations_total.inc();
        if !success {
            self.token_validation_errors.inc();
        }
    }

    pub fn record_db_query(&self, duration_ms: f64, success: bool) {
        self.db_query_duration.observe(duration_ms);
        if !success {
            self.db_query_errors.inc();
        }
    }

    pub fn update_pool_metrics(&self, active: f64, idle: f64, utilization: f64, is_healthy: bool) {
        self.db_connections_active.set(active);
        self.db_connections_idle.set(idle);
        self.pool_utilization.set(utilization);
        self.pool_health_status.set(if is_healthy { 1.0 } else { 0.0 });
    }

    pub fn record_cache_operation(&self, hit: bool) {
        if hit {
            self.cache_hits_total.inc();
        } else {
            self.cache_misses_total.inc();
        }
    }

    pub fn record_federation_request(&self, duration_ms: f64, success: bool) {
        self.federation_requests_total.inc();
        self.federation_request_duration.observe(duration_ms);
        if !success {
            self.federation_signature_errors.inc();
        }
    }

    pub fn record_federation_signature_verification(&self, success: bool) {
        self.federation_signature_verifications.inc();
        if !success {
            self.federation_signature_errors.inc();
        }
    }

    pub fn record_replay_attack_blocked(&self) {
        self.federation_replay_attacks_blocked.inc();
    }

    pub fn record_http_request(&self, duration_ms: f64, success: bool) {
        self.http_requests_total.inc();
        self.http_request_duration.observe(duration_ms);
        if !success {
            self.http_request_errors.inc();
        }
    }

    pub fn http_request_started(&self) {
        self.http_active_requests.inc();
    }

    pub fn http_request_finished(&self) {
        self.http_active_requests.dec();
    }

    pub fn record_security_validation(&self, validation_type: SecurityValidationType, success: bool) {
        if !success {
            match validation_type {
                SecurityValidationType::Jwt => self.security_jwt_validation_errors.inc(),
                SecurityValidationType::Origin => self.security_origin_validation_errors.inc(),
                SecurityValidationType::Timestamp => self.security_timestamp_validation_errors.inc(),
            }
        }
    }

    pub fn get_collector(&self) -> &Arc<MetricsCollector> {
        &self.collector
    }

    pub fn get_summary(&self) -> MetricsSummary {
        MetricsSummary {
            auth_attempts: self.auth_attempts_total.get(),
            auth_failures: self.auth_failures_total.get(),
            auth_success: self.auth_success_total.get(),
            token_validations: self.token_validations_total.get(),
            token_errors: self.token_validation_errors.get(),
            cache_hits: self.cache_hits_total.get(),
            cache_misses: self.cache_misses_total.get(),
            cache_hit_rate: self.calculate_cache_hit_rate(),
            federation_requests: self.federation_requests_total.get(),
            federation_errors: self.federation_signature_errors.get(),
            replay_attacks_blocked: self.federation_replay_attacks_blocked.get(),
            http_requests: self.http_requests_total.get(),
            http_errors: self.http_request_errors.get(),
            db_errors: self.db_query_errors.get(),
        }
    }

    fn calculate_cache_hit_rate(&self) -> f64 {
        let hits = self.cache_hits_total.get();
        let misses = self.cache_misses_total.get();
        let total = hits + misses;
        if total == 0 {
            0.0
        } else {
            (hits as f64 / total as f64) * 100.0
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SecurityValidationType {
    Jwt,
    Origin,
    Timestamp,
}

#[derive(Debug, Clone)]
pub struct MetricsSummary {
    pub auth_attempts: u64,
    pub auth_failures: u64,
    pub auth_success: u64,
    pub token_validations: u64,
    pub token_errors: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_hit_rate: f64,
    pub federation_requests: u64,
    pub federation_errors: u64,
    pub replay_attacks_blocked: u64,
    pub http_requests: u64,
    pub http_errors: u64,
    pub db_errors: u64,
}

impl MetricsSummary {
    pub fn auth_success_rate(&self) -> f64 {
        if self.auth_attempts == 0 {
            0.0
        } else {
            (self.auth_success as f64 / self.auth_attempts as f64) * 100.0
        }
    }

    pub fn error_rate(&self) -> f64 {
        if self.http_requests == 0 {
            0.0
        } else {
            (self.http_errors as f64 / self.http_requests as f64) * 100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_metrics_creation() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);
        
        assert_eq!(metrics.auth_attempts_total.get(), 0);
        assert_eq!(metrics.cache_hits_total.get(), 0);
    }

    #[test]
    fn test_record_auth_attempt() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);
        
        metrics.record_auth_attempt(true);
        assert_eq!(metrics.auth_attempts_total.get(), 1);
        assert_eq!(metrics.auth_success_total.get(), 1);
        assert_eq!(metrics.auth_failures_total.get(), 0);
        
        metrics.record_auth_attempt(false);
        assert_eq!(metrics.auth_attempts_total.get(), 2);
        assert_eq!(metrics.auth_success_total.get(), 1);
        assert_eq!(metrics.auth_failures_total.get(), 1);
    }

    #[test]
    fn test_record_cache_operation() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);
        
        metrics.record_cache_operation(true);
        metrics.record_cache_operation(true);
        metrics.record_cache_operation(false);
        
        assert_eq!(metrics.cache_hits_total.get(), 2);
        assert_eq!(metrics.cache_misses_total.get(), 1);
    }

    #[test]
    fn test_update_pool_metrics() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);
        
        metrics.update_pool_metrics(15.0, 5.0, 0.75, true);
        
        assert_eq!(metrics.db_connections_active.get(), 15.0);
        assert_eq!(metrics.db_connections_idle.get(), 5.0);
        assert_eq!(metrics.pool_utilization.get(), 0.75);
        assert_eq!(metrics.pool_health_status.get(), 1.0);
        
        metrics.update_pool_metrics(19.0, 1.0, 0.95, false);
        assert_eq!(metrics.pool_health_status.get(), 0.0);
    }

    #[test]
    fn test_record_federation_request() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);
        
        metrics.record_federation_request(50.0, true);
        metrics.record_federation_request(100.0, false);
        
        assert_eq!(metrics.federation_requests_total.get(), 2);
        assert_eq!(metrics.federation_signature_errors.get(), 1);
    }

    #[test]
    fn test_record_replay_attack_blocked() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);
        
        metrics.record_replay_attack_blocked();
        metrics.record_replay_attack_blocked();
        
        assert_eq!(metrics.federation_replay_attacks_blocked.get(), 2);
    }

    #[test]
    fn test_get_summary() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);
        
        metrics.record_auth_attempt(true);
        metrics.record_auth_attempt(false);
        metrics.record_cache_operation(true);
        metrics.record_cache_operation(false);
        metrics.record_http_request(50.0, true);
        metrics.record_http_request(100.0, false);
        
        let summary = metrics.get_summary();
        
        assert_eq!(summary.auth_attempts, 2);
        assert_eq!(summary.auth_success, 1);
        assert_eq!(summary.auth_failures, 1);
        assert_eq!(summary.cache_hits, 1);
        assert_eq!(summary.cache_misses, 1);
        assert_eq!(summary.http_requests, 2);
        assert_eq!(summary.http_errors, 1);
    }

    #[test]
    fn test_metrics_summary_calculations() {
        let summary = MetricsSummary {
            auth_attempts: 100,
            auth_failures: 10,
            auth_success: 90,
            token_validations: 500,
            token_errors: 5,
            cache_hits: 800,
            cache_misses: 200,
            cache_hit_rate: 80.0,
            federation_requests: 50,
            federation_errors: 2,
            replay_attacks_blocked: 3,
            http_requests: 1000,
            http_errors: 20,
            db_errors: 5,
        };
        
        assert_eq!(summary.auth_success_rate(), 90.0);
        assert_eq!(summary.error_rate(), 2.0);
    }

    #[test]
    fn test_security_validation_recording() {
        let collector = Arc::new(MetricsCollector::new());
        let metrics = ServerMetrics::new(collector);
        
        metrics.record_security_validation(SecurityValidationType::Jwt, false);
        metrics.record_security_validation(SecurityValidationType::Origin, false);
        metrics.record_security_validation(SecurityValidationType::Timestamp, false);
        
        assert_eq!(metrics.security_jwt_validation_errors.get(), 1);
        assert_eq!(metrics.security_origin_validation_errors.get(), 1);
        assert_eq!(metrics.security_timestamp_validation_errors.get(), 1);
    }
}
