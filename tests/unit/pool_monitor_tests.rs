use synapse_rust::storage::pool_monitor::{
    DatabasePoolConfig, PoolHealthStatus, QueryTimeoutConfig, set_query_timeout,
    set_transaction_timeout,
};
use std::time::Duration;

#[test]
fn test_pool_config_default_values() {
    let config = DatabasePoolConfig::default();

    assert_eq!(config.max_connections, 20);
    assert_eq!(config.min_connections, 5);
    assert_eq!(config.acquire_timeout, Duration::from_secs(30));
    assert_eq!(config.idle_timeout, Some(Duration::from_secs(600)));
    assert_eq!(config.max_lifetime, Some(Duration::from_secs(1800)));
    assert_eq!(config.health_check_interval, Duration::from_secs(30));
}

#[test]
fn test_pool_config_custom_values() {
    let config = DatabasePoolConfig {
        max_connections: 50,
        min_connections: 10,
        acquire_timeout: Duration::from_secs(60),
        idle_timeout: Some(Duration::from_secs(1200)),
        max_lifetime: Some(Duration::from_secs(3600)),
        health_check_interval: Duration::from_secs(60),
    };

    assert_eq!(config.max_connections, 50);
    assert_eq!(config.min_connections, 10);
    assert_eq!(config.acquire_timeout, Duration::from_secs(60));
}

#[test]
fn test_pool_health_status_healthy() {
    let status = PoolHealthStatus {
        active_connections: 10,
        idle_connections: 10,
        max_connections: 20,
        is_healthy: true,
        connection_utilization: 0.5,
        last_check: tokio::time::Instant::now(),
    };

    assert!(status.is_healthy);
    assert!(!status.is_warning());
    assert!(!status.is_critical());
}

#[test]
fn test_pool_health_status_warning() {
    let status = PoolHealthStatus {
        active_connections: 15,
        idle_connections: 5,
        max_connections: 20,
        is_healthy: true,
        connection_utilization: 0.75,
        last_check: tokio::time::Instant::now(),
    };

    assert!(status.is_warning());
    assert!(!status.is_critical());
}

#[test]
fn test_pool_health_status_critical() {
    let status = PoolHealthStatus {
        active_connections: 19,
        idle_connections: 1,
        max_connections: 20,
        is_healthy: false,
        connection_utilization: 0.95,
        last_check: tokio::time::Instant::now(),
    };

    assert!(status.is_critical());
    assert!(status.is_warning());
}

#[test]
fn test_pool_health_status_utilization_boundaries() {
    let status_normal = PoolHealthStatus {
        active_connections: 10,
        idle_connections: 10,
        max_connections: 20,
        is_healthy: true,
        connection_utilization: 0.5,
        last_check: tokio::time::Instant::now(),
    };
    assert!(!status_normal.is_warning());

    let status_warning_boundary = PoolHealthStatus {
        active_connections: 14,
        idle_connections: 6,
        max_connections: 20,
        is_healthy: true,
        connection_utilization: 0.7,
        last_check: tokio::time::Instant::now(),
    };
    assert!(status_warning_boundary.is_warning());

    let status_critical_boundary = PoolHealthStatus {
        active_connections: 18,
        idle_connections: 2,
        max_connections: 20,
        is_healthy: false,
        connection_utilization: 0.9,
        last_check: tokio::time::Instant::now(),
    };
    assert!(status_critical_boundary.is_critical());
}

#[test]
fn test_query_timeout_config_default() {
    let config = QueryTimeoutConfig::default();

    assert_eq!(config.default_timeout, Duration::from_secs(30));
    assert_eq!(config.long_query_timeout, Duration::from_secs(120));
    assert_eq!(config.transaction_timeout, Duration::from_secs(300));
}

#[test]
fn test_set_query_timeout_sql_generation() {
    let timeout = Duration::from_secs(60);
    let sql = set_query_timeout(timeout);

    assert_eq!(sql, "SET statement_timeout = 60000ms");
}

#[test]
fn test_set_query_timeout_various_values() {
    assert_eq!(set_query_timeout(Duration::from_secs(1)), "SET statement_timeout = 1000ms");
    assert_eq!(set_query_timeout(Duration::from_secs(30)), "SET statement_timeout = 30000ms");
    assert_eq!(set_query_timeout(Duration::from_secs(120)), "SET statement_timeout = 120000ms");
    assert_eq!(set_query_timeout(Duration::from_millis(500)), "SET statement_timeout = 500ms");
}

#[test]
fn test_set_transaction_timeout_sql_generation() {
    let timeout = Duration::from_secs(300);
    let sql = set_transaction_timeout(timeout);

    assert_eq!(sql, "SET idle_in_transaction_session_timeout = 300000ms");
}

#[test]
fn test_set_transaction_timeout_various_values() {
    assert_eq!(set_transaction_timeout(Duration::from_secs(60)), "SET idle_in_transaction_session_timeout = 60000ms");
    assert_eq!(set_transaction_timeout(Duration::from_secs(180)), "SET idle_in_transaction_session_timeout = 180000ms");
    assert_eq!(set_transaction_timeout(Duration::from_millis(1500)), "SET idle_in_transaction_session_timeout = 1500ms");
}

#[test]
fn test_pool_health_status_zero_utilization() {
    let status = PoolHealthStatus {
        active_connections: 0,
        idle_connections: 20,
        max_connections: 20,
        is_healthy: true,
        connection_utilization: 0.0,
        last_check: tokio::time::Instant::now(),
    };

    assert!(status.is_healthy);
    assert!(!status.is_warning());
    assert!(!status.is_critical());
}

#[test]
fn test_pool_health_status_full_utilization() {
    let status = PoolHealthStatus {
        active_connections: 20,
        idle_connections: 0,
        max_connections: 20,
        is_healthy: false,
        connection_utilization: 1.0,
        last_check: tokio::time::Instant::now(),
    };

    assert!(!status.is_healthy);
    assert!(status.is_warning());
    assert!(status.is_critical());
}
