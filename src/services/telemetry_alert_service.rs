use crate::common::ApiError;
use crate::storage::{DatabaseHealthStatus, DatabaseMonitor};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TelemetryAlertSeverity {
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TelemetryAlertStatus {
    Warning,
    Critical,
    Acknowledged,
    Recovered,
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryAlert {
    pub alert_id: String,
    pub alert_key: String,
    pub rule_name: String,
    pub severity: TelemetryAlertSeverity,
    pub status: TelemetryAlertStatus,
    pub owner: String,
    pub message: String,
    pub trigger_count: u64,
    pub triggered_at: i64,
    pub last_seen_ts: i64,
    pub acknowledged_at: Option<i64>,
    pub acknowledged_by: Option<String>,
    pub recovered_at: Option<i64>,
    pub closed_at: Option<i64>,
    pub metrics: serde_json::Value,
}

#[derive(Debug, Clone, Default)]
pub struct TelemetryAlertFilters {
    pub status: Option<String>,
    pub severity: Option<String>,
}

#[derive(Clone)]
pub struct TelemetryAlertService {
    pool: Arc<PgPool>,
    max_connections: u32,
    alerts: Arc<RwLock<HashMap<String, TelemetryAlert>>>,
}

impl TelemetryAlertService {
    pub fn new(pool: Arc<PgPool>, max_connections: u32) -> Self {
        Self {
            pool,
            max_connections,
            alerts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn sync_with_health(
        &self,
    ) -> Result<(DatabaseHealthStatus, Vec<TelemetryAlert>), ApiError> {
        let monitor = DatabaseMonitor::new((*self.pool).clone(), self.max_connections);
        let health = monitor.get_full_health_status().await.map_err(|error| {
            ApiError::internal(format!("failed to collect telemetry health: {}", error))
        })?;
        let alerts = self.refresh_from_health_status(health.clone()).await;
        Ok((health, alerts))
    }

    pub async fn refresh_from_health_status(
        &self,
        health: DatabaseHealthStatus,
    ) -> Vec<TelemetryAlert> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut active_rules = HashMap::new();

        if !health.is_healthy {
            active_rules.insert(
                "db_health",
                build_candidate(
                    "db_health",
                    "Database health",
                    TelemetryAlertSeverity::Critical,
                    "database",
                    "database health check returned unhealthy".to_string(),
                    json!({ "is_healthy": false }),
                ),
            );
        }

        let pool_utilization = health.connection_pool_status.connection_utilization;
        if pool_utilization >= 90.0 {
            active_rules.insert(
                "db_connection_pool_utilization",
                build_candidate(
                    "db_connection_pool_utilization",
                    "Connection pool utilization",
                    TelemetryAlertSeverity::Critical,
                    "database",
                    format!("connection pool utilization is {:.1}%", pool_utilization),
                    json!({
                        "connection_utilization": pool_utilization,
                        "busy_connections": health.connection_pool_status.busy_connections,
                        "max_connections": health.connection_pool_status.max_connections
                    }),
                ),
            );
        }

        if health.performance_metrics.slow_queries_count >= 10 {
            active_rules.insert(
                "db_slow_queries",
                build_candidate(
                    "db_slow_queries",
                    "Slow query count",
                    TelemetryAlertSeverity::Warning,
                    "database",
                    format!(
                        "slow queries exceeded threshold: {}",
                        health.performance_metrics.slow_queries_count
                    ),
                    json!({
                        "slow_queries_count": health.performance_metrics.slow_queries_count
                    }),
                ),
            );
        }

        if health.performance_metrics.average_query_time_ms >= 100.0 {
            active_rules.insert(
                "db_average_query_time",
                build_candidate(
                    "db_average_query_time",
                    "Average query time",
                    TelemetryAlertSeverity::Warning,
                    "database",
                    format!(
                        "average query time is {:.2}ms",
                        health.performance_metrics.average_query_time_ms
                    ),
                    json!({
                        "average_query_time_ms": health.performance_metrics.average_query_time_ms
                    }),
                ),
            );
        }

        let mut alerts = self.alerts.write().await;
        for (alert_key, candidate) in &active_rules {
            match alerts.get_mut(*alert_key) {
                Some(existing) => {
                    existing.severity = candidate.severity.clone();
                    existing.message = candidate.message.clone();
                    existing.metrics = candidate.metrics.clone();
                    existing.last_seen_ts = now;
                    existing.recovered_at = None;
                    existing.closed_at = None;
                    existing.trigger_count += 1;
                    if existing.status != TelemetryAlertStatus::Acknowledged {
                        existing.status = status_from_severity(&candidate.severity);
                    }
                }
                None => {
                    alerts.insert(alert_key.to_string(), candidate.clone());
                }
            }
        }

        for (alert_key, alert) in alerts.iter_mut() {
            if active_rules.contains_key(alert_key.as_str()) {
                continue;
            }
            if matches!(
                alert.status,
                TelemetryAlertStatus::Warning
                    | TelemetryAlertStatus::Critical
                    | TelemetryAlertStatus::Acknowledged
            ) {
                alert.status = TelemetryAlertStatus::Recovered;
                alert.recovered_at = Some(now);
                alert.last_seen_ts = now;
            }
        }

        alerts.values().cloned().collect()
    }

    pub async fn raise_alert(
        &self,
        alert_key: &str,
        rule_name: &str,
        severity: TelemetryAlertSeverity,
        owner: &str,
        message: &str,
        metrics: serde_json::Value,
    ) -> TelemetryAlert {
        let mut alerts = self.alerts.write().await;
        let now = chrono::Utc::now().timestamp_millis();
        let entry = alerts
            .entry(alert_key.to_string())
            .or_insert_with(|| TelemetryAlert {
                alert_id: uuid::Uuid::new_v4().to_string(),
                alert_key: alert_key.to_string(),
                rule_name: rule_name.to_string(),
                severity: severity.clone(),
                status: status_from_severity(&severity),
                owner: owner.to_string(),
                message: message.to_string(),
                trigger_count: 0,
                triggered_at: now,
                last_seen_ts: now,
                acknowledged_at: None,
                acknowledged_by: None,
                recovered_at: None,
                closed_at: None,
                metrics: metrics.clone(),
            });
        entry.severity = severity.clone();
        entry.status = status_from_severity(&severity);
        entry.message = message.to_string();
        entry.metrics = metrics;
        entry.last_seen_ts = now;
        entry.recovered_at = None;
        entry.trigger_count += 1;
        entry.clone()
    }

    pub async fn list_alerts(
        &self,
        filters: TelemetryAlertFilters,
    ) -> Result<Vec<TelemetryAlert>, ApiError> {
        validate_filters(&filters)?;
        let alerts = self.alerts.read().await;
        let mut entries: Vec<TelemetryAlert> = alerts
            .values()
            .filter(|alert| match filters.status.as_deref() {
                Some(status) => matches_status(alert, status),
                None => true,
            })
            .filter(|alert| match filters.severity.as_deref() {
                Some(severity) => matches_severity(alert, severity),
                None => true,
            })
            .cloned()
            .collect();
        entries.sort_by(|left, right| right.last_seen_ts.cmp(&left.last_seen_ts));
        Ok(entries)
    }

    pub async fn acknowledge_alert(
        &self,
        alert_id: &str,
        acknowledged_by: &str,
    ) -> Result<TelemetryAlert, ApiError> {
        let mut alerts = self.alerts.write().await;
        let now = chrono::Utc::now().timestamp_millis();
        let alert = alerts
            .values_mut()
            .find(|alert| alert.alert_id == alert_id)
            .ok_or_else(|| ApiError::not_found(format!("alert not found: {}", alert_id)))?;

        if alert.status == TelemetryAlertStatus::Recovered {
            return Err(ApiError::bad_request(
                "ALERT_ACK_FORBIDDEN: recovered alerts cannot be acknowledged",
            ));
        }

        alert.status = TelemetryAlertStatus::Acknowledged;
        alert.acknowledged_at = Some(now);
        alert.acknowledged_by = Some(acknowledged_by.to_string());
        alert.last_seen_ts = now;
        Ok(alert.clone())
    }
}

fn build_candidate(
    alert_key: &str,
    rule_name: &str,
    severity: TelemetryAlertSeverity,
    owner: &str,
    message: String,
    metrics: serde_json::Value,
) -> TelemetryAlert {
    let now = chrono::Utc::now().timestamp_millis();
    TelemetryAlert {
        alert_id: uuid::Uuid::new_v4().to_string(),
        alert_key: alert_key.to_string(),
        rule_name: rule_name.to_string(),
        severity: severity.clone(),
        status: status_from_severity(&severity),
        owner: owner.to_string(),
        message,
        trigger_count: 1,
        triggered_at: now,
        last_seen_ts: now,
        acknowledged_at: None,
        acknowledged_by: None,
        recovered_at: None,
        closed_at: None,
        metrics,
    }
}

fn status_from_severity(severity: &TelemetryAlertSeverity) -> TelemetryAlertStatus {
    match severity {
        TelemetryAlertSeverity::Warning => TelemetryAlertStatus::Warning,
        TelemetryAlertSeverity::Critical => TelemetryAlertStatus::Critical,
    }
}

fn validate_filters(filters: &TelemetryAlertFilters) -> Result<(), ApiError> {
    if let Some(ref status) = filters.status {
        match status.as_str() {
            "warning" | "critical" | "acknowledged" | "recovered" | "closed" => {}
            _ => {
                return Err(ApiError::bad_request(
                    "ALERT_RULE_NOT_FOUND: unsupported alert status filter",
                ))
            }
        }
    }
    if let Some(ref severity) = filters.severity {
        match severity.as_str() {
            "warning" | "critical" => {}
            _ => {
                return Err(ApiError::bad_request(
                    "ALERT_RULE_NOT_FOUND: unsupported alert severity filter",
                ))
            }
        }
    }
    Ok(())
}

fn matches_status(alert: &TelemetryAlert, status: &str) -> bool {
    matches!(
        (&alert.status, status),
        (TelemetryAlertStatus::Warning, "warning")
            | (TelemetryAlertStatus::Critical, "critical")
            | (TelemetryAlertStatus::Acknowledged, "acknowledged")
            | (TelemetryAlertStatus::Recovered, "recovered")
            | (TelemetryAlertStatus::Closed, "closed")
    )
}

fn matches_severity(alert: &TelemetryAlert, severity: &str) -> bool {
    matches!(
        (&alert.severity, severity),
        (TelemetryAlertSeverity::Warning, "warning")
            | (TelemetryAlertSeverity::Critical, "critical")
    )
}
