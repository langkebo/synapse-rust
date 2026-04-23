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

        let now = chrono::Utc::now().timestamp_millis();
        let mut active_rules: HashMap<&str, TelemetryAlert> = HashMap::new();

        if !health.is_healthy {
            active_rules.insert(
                "db_health",
                build_alert(
                    "db_health",
                    "Database health",
                    TelemetryAlertSeverity::Critical,
                    "database",
                    "database health check returned unhealthy".to_string(),
                    json!({ "is_healthy": false }),
                ),
            );
        }

        let util = health.connection_pool_status.connection_utilization;
        if util >= 90.0 {
            active_rules.insert(
                "db_pool_utilization",
                build_alert(
                    "db_pool_utilization",
                    "Connection pool utilization",
                    TelemetryAlertSeverity::Critical,
                    "database",
                    format!("connection pool utilization is {:.1}%", util),
                    json!({ "connection_utilization": util }),
                ),
            );
        }

        let mut alerts = self.alerts.write().await;
        for (key, candidate) in &active_rules {
            match alerts.get_mut(*key) {
                Some(existing) => {
                    existing.severity = candidate.severity.clone();
                    existing.message.clone_from(&candidate.message);
                    existing.metrics = candidate.metrics.clone();
                    existing.last_seen_ts = now;
                    existing.recovered_at = None;
                    existing.trigger_count += 1;
                    if existing.status != TelemetryAlertStatus::Acknowledged {
                        existing.status = status_from_severity(&candidate.severity);
                    }
                }
                None => {
                    alerts.insert(key.to_string(), candidate.clone());
                }
            }
        }
        for (key, alert) in alerts.iter_mut() {
            if !active_rules.contains_key(key.as_str())
                && matches!(
                    alert.status,
                    TelemetryAlertStatus::Warning
                        | TelemetryAlertStatus::Critical
                        | TelemetryAlertStatus::Acknowledged
                )
            {
                alert.status = TelemetryAlertStatus::Recovered;
                alert.recovered_at = Some(now);
                alert.last_seen_ts = now;
            }
        }
        let result = alerts.values().cloned().collect();
        drop(alerts);

        Ok((health, result))
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
        let entry = alerts.entry(alert_key.to_string()).or_insert_with(|| {
            build_alert(
                alert_key,
                rule_name,
                severity.clone(),
                owner,
                message.to_string(),
                metrics.clone(),
            )
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
            .filter(|a| {
                filters
                    .status
                    .as_deref()
                    .is_none_or(|s| matches_status(a, s))
            })
            .filter(|a| {
                filters
                    .severity
                    .as_deref()
                    .is_none_or(|s| matches_severity(a, s))
            })
            .cloned()
            .collect();
        entries.sort_by(|l, r| r.last_seen_ts.cmp(&l.last_seen_ts));
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
            .find(|a| a.alert_id == alert_id)
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

fn build_alert(
    key: &str,
    rule: &str,
    severity: TelemetryAlertSeverity,
    owner: &str,
    message: String,
    metrics: serde_json::Value,
) -> TelemetryAlert {
    let now = chrono::Utc::now().timestamp_millis();
    TelemetryAlert {
        alert_id: uuid::Uuid::new_v4().to_string(),
        alert_key: key.to_string(),
        rule_name: rule.to_string(),
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

fn status_from_severity(s: &TelemetryAlertSeverity) -> TelemetryAlertStatus {
    match s {
        TelemetryAlertSeverity::Warning => TelemetryAlertStatus::Warning,
        TelemetryAlertSeverity::Critical => TelemetryAlertStatus::Critical,
    }
}

fn validate_filters(filters: &TelemetryAlertFilters) -> Result<(), ApiError> {
    if let Some(ref s) = filters.status {
        match s.as_str() {
            "warning" | "critical" | "acknowledged" | "recovered" | "closed" => {}
            _ => {
                return Err(ApiError::bad_request(
                    "ALERT_RULE_NOT_FOUND: unsupported alert status filter",
                ))
            }
        }
    }
    if let Some(ref s) = filters.severity {
        match s.as_str() {
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
