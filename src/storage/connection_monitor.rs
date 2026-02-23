use sqlx::{Pool, Postgres};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct ConnectionStateMonitor {
    pool: Pool<Postgres>,
    stats: Arc<ConnectionStats>,
    events: Arc<RwLock<Vec<ConnectionEvent>>>,
    max_events: usize,
}

#[derive(Debug, Clone)]
pub struct ConnectionStats {
    total_connections_created: AtomicU64,
    total_connections_closed: AtomicU64,
    total_errors: AtomicU64,
    total_timeouts: AtomicU64,
    current_active_connections: AtomicU64,
    peak_connections: AtomicU64,
    average_connection_duration: AtomicU64,
    last_error_time: AtomicU64,
}

impl Default for ConnectionStats {
    fn default() -> Self {
        Self {
            total_connections_created: AtomicU64::new(0),
            total_connections_closed: AtomicU64::new(0),
            total_errors: AtomicU64::new(0),
            total_timeouts: AtomicU64::new(0),
            current_active_connections: AtomicU64::new(0),
            peak_connections: AtomicU64::new(0),
            average_connection_duration: AtomicU64::new(0),
            last_error_time: AtomicU64::new(0),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConnectionEventType {
    Connected,
    Disconnected,
    Error,
    Timeout,
    Reconnected,
    PoolResize,
    IdleTimeout,
    LifetimeExpiry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionEvent {
    pub timestamp: DateTime<Utc>,
    pub event_type: ConnectionEventType,
    pub connection_id: Option<String>,
    pub message: String,
    pub metadata: Option<serde_json::Value>,
}

impl ConnectionStateMonitor {
    pub fn new(pool: Pool<Postgres>, max_events: usize) -> Self {
        Self {
            pool,
            stats: Arc::new(ConnectionStats::default()),
            events: Arc::new(RwLock::new(Vec::with_capacity(max_events))),
            max_events,
        }
    }

    pub async fn record_connection_created(&self, connection_id: &str) {
        self.stats.total_connections_created.fetch_add(1, Ordering::SeqCst);
        let current = self.stats.current_active_connections.fetch_add(1, Ordering::SeqCst) + 1;
        
        let peak = self.stats.peak_connections.load(Ordering::SeqCst);
        if current > peak {
            self.stats.peak_connections.store(current, Ordering::SeqCst);
        }

        self.add_event(ConnectionEvent {
            timestamp: Utc::now(),
            event_type: ConnectionEventType::Connected,
            connection_id: Some(connection_id.to_string()),
            message: format!("Connection {} created", connection_id),
            metadata: None,
        }).await;
    }

    pub async fn record_connection_closed(&self, connection_id: &str, duration: Duration) {
        self.stats.total_connections_closed.fetch_add(1, Ordering::SeqCst);
        self.stats.current_active_connections.fetch_sub(1, Ordering::SeqCst);
        self.update_average_duration(duration);

        self.add_event(ConnectionEvent {
            timestamp: Utc::now(),
            event_type: ConnectionEventType::Disconnected,
            connection_id: Some(connection_id.to_string()),
            message: format!("Connection {} closed after {:?}", connection_id, duration),
            metadata: None,
        }).await;
    }

    pub async fn record_error(&self, connection_id: &str, error: &str) {
        self.stats.total_errors.fetch_add(1, Ordering::SeqCst);
        self.stats.last_error_time.store(Utc::now().timestamp() as u64, Ordering::SeqCst);

        self.add_event(ConnectionEvent {
            timestamp: Utc::now(),
            event_type: ConnectionEventType::Error,
            connection_id: Some(connection_id.to_string()),
            message: format!("Connection error: {}", error),
            metadata: None,
        }).await;
    }

    pub async fn record_timeout(&self, operation: &str) {
        self.stats.total_timeouts.fetch_add(1, Ordering::SeqCst);

        self.add_event(ConnectionEvent {
            timestamp: Utc::now(),
            event_type: ConnectionEventType::Timeout,
            connection_id: None,
            message: format!("Operation timeout: {}", operation),
            metadata: None,
        }).await;
    }

    pub async fn record_reconnection(&self, connection_id: &str) {
        self.add_event(ConnectionEvent {
            timestamp: Utc::now(),
            event_type: ConnectionEventType::Reconnected,
            connection_id: Some(connection_id.to_string()),
            message: format!("Reconnected after failure: {}", connection_id),
            metadata: None,
        }).await;
    }

    pub async fn record_pool_resize(&self, old_size: u32, new_size: u32) {
        self.add_event(ConnectionEvent {
            timestamp: Utc::now(),
            event_type: ConnectionEventType::PoolResize,
            connection_id: None,
            message: format!("Pool resized from {} to {}", old_size, new_size),
            metadata: Some(serde_json::json!({
                "old_size": old_size,
                "new_size": new_size,
            })),
        }).await;
    }

    fn update_average_duration(&self, duration: Duration) {
        let total_closed = self.stats.total_connections_closed.load(Ordering::SeqCst);
        if total_closed == 0 {
            return;
        }

        let new_avg = duration.as_millis() as u64;
        let current_avg = self.stats.average_connection_duration.load(Ordering::SeqCst);
        let avg = (current_avg * (total_closed - 1) + new_avg) / total_closed;
        self.stats.average_connection_duration.store(avg, Ordering::SeqCst);
    }

    async fn add_event(&self, event: ConnectionEvent) {
        let mut events = self.events.write().await;
        if events.len() >= self.max_events {
            events.remove(0);
        }
        events.push(event);
    }

    pub fn get_stats(&self) -> ConnectionStatsSnapshot {
        ConnectionStatsSnapshot {
            total_connections_created: self.stats.total_connections_created.load(Ordering::SeqCst),
            total_connections_closed: self.stats.total_connections_closed.load(Ordering::SeqCst),
            total_errors: self.stats.total_errors.load(Ordering::SeqCst),
            total_timeouts: self.stats.total_timeouts.load(Ordering::SeqCst),
            current_active: self.stats.current_active_connections.load(Ordering::SeqCst),
            peak_connections: self.stats.peak_connections.load(Ordering::SeqCst),
            average_connection_duration_ms: self.stats.average_connection_duration.load(Ordering::SeqCst),
            last_error_time: self.stats.last_error_time.load(Ordering::SeqCst),
        }
    }

    pub async fn get_recent_events(&self, limit: usize) -> Vec<ConnectionEvent> {
        let events = self.events.read().await;
        events.iter().rev().take(limit).cloned().collect()
    }

    pub async fn get_error_events(&self, since: DateTime<Utc>) -> Vec<ConnectionEvent> {
        let events = self.events.read().await;
        events.iter()
            .filter(|e| e.event_type == ConnectionEventType::Error && e.timestamp >= since)
            .cloned()
            .collect()
    }

    pub fn health_score(&self) -> f64 {
        let stats = self.get_stats();
        let total_ops = stats.total_connections_created.max(1);
        let error_rate = stats.total_errors as f64 / total_ops as f64;
        let timeout_rate = stats.total_timeouts as f64 / total_ops as f64;
        
        let base_score = 100.0;
        let error_penalty = error_rate * 30.0;
        let timeout_penalty = timeout_rate * 20.0;
        
        (base_score - error_penalty - timeout_penalty).max(0.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionStatsSnapshot {
    pub total_connections_created: u64,
    pub total_connections_closed: u64,
    pub total_errors: u64,
    pub total_timeouts: u64,
    pub current_active: u64,
    pub peak_connections: u64,
    pub average_connection_duration_ms: u64,
    pub last_error_time: u64,
}

impl ConnectionStatsSnapshot {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "total_connections_created": self.total_connections_created,
            "total_connections_closed": self.total_connections_closed,
            "total_errors": self.total_errors,
            "total_timeouts": self.total_timeouts,
            "current_active": self.current_active,
            "peak_connections": self.peak_connections,
            "average_connection_duration_ms": self.average_connection_duration_ms,
            "last_error_time": if self.last_error_time > 0 {
                Some(DateTime::from_timestamp(self.last_error_time as i64, 0).map(|d| d.to_rfc3339()))
            } else {
                None
            },
            "health_score": self.health_score(),
        })
    }

    pub fn health_score(&self) -> f64 {
        let total_ops = self.total_connections_created.max(1);
        let error_rate = self.total_errors as f64 / total_ops as f64;
        let timeout_rate = self.total_timeouts as f64 / total_ops as f64;
        
        let base_score = 100.0;
        let error_penalty = error_rate * 30.0;
        let timeout_penalty = timeout_rate * 20.0;
        
        (base_score - error_penalty - timeout_penalty).max(0.0)
    }
}

pub struct ConnectionHealthChecker {
    monitor: ConnectionStateMonitor,
    check_interval: Duration,
    consecutive_failures: AtomicU64,
    max_consecutive_failures: u64,
}

impl ConnectionHealthChecker {
    pub fn new(pool: Pool<Postgres>, check_interval: Duration) -> Self {
        Self {
            monitor: ConnectionStateMonitor::new(pool, 1000),
            check_interval,
            consecutive_failures: AtomicU64::new(0),
            max_consecutive_failures: 3,
        }
    }

    pub async fn perform_check(&self) -> HealthCheckResult {
        let start = Instant::now();
        let result = sqlx::query("SELECT 1")
            .fetch_one(self.monitor.pool())
            .await;

        let duration = start.elapsed();

        match result {
            Ok(_) => {
                self.consecutive_failures.store(0, Ordering::SeqCst);
                HealthCheckResult {
                    is_healthy: true,
                    latency_ms: duration.as_millis() as u64,
                    error: None,
                    timestamp: Utc::now(),
                }
            }
            Err(e) => {
                let failures = self.consecutive_failures.fetch_add(1, Ordering::SeqCst) + 1;
                self.monitor.record_error("health_check", &e.to_string()).await;
                
                HealthCheckResult {
                    is_healthy: failures < self.max_consecutive_failures,
                    latency_ms: duration.as_millis() as u64,
                    error: Some(e.to_string()),
                    timestamp: Utc::now(),
                }
            }
        }
    }

    pub fn monitor(&self) -> &ConnectionStateMonitor {
        &self.monitor
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    pub is_healthy: bool,
    pub latency_ms: u64,
    pub error: Option<String>,
    pub timestamp: DateTime<Utc>,
}

pub async fn run_periodic_health_checks(
    checker: Arc<ConnectionHealthChecker>,
    shutdown: Arc<AtomicU64>,
) {
    let mut interval = tokio::time::interval(checker.check_interval);
    
    while shutdown.load(Ordering::SeqCst) == 0 {
        interval.tick().await;
        let _ = checker.perform_check().await;
    }
}
