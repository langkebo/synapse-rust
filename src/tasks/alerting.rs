use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertConfig {
    pub enabled: bool,
    pub connection_pool_threshold: f64,
    pub slow_query_threshold_ms: u64,
    pub max_slow_queries: u64,
    pub integrity_score_threshold: f64,
    pub check_interval: Duration,
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            connection_pool_threshold: 85.0,
            slow_query_threshold_ms: 100,
            max_slow_queries: 10,
            integrity_score_threshold: 90.0,
            check_interval: Duration::from_secs(60),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    pub name: String,
    pub severity: AlertSeverity,
    pub condition: AlertCondition,
    pub message: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertSeverity {
    Critical,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertCondition {
    ConnectionPoolHigh { threshold: f64 },
    SlowQueriesHigh { count: u64 },
    IntegrityScoreLow { threshold: f64 },
    QueryTimeHigh { threshold_ms: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertState {
    pub triggered_at: chrono::DateTime<chrono::Utc>,
    pub rule_name: String,
    pub severity: AlertSeverity,
    pub message: String,
    pub acknowledged: bool,
}

pub struct AlertManager {
    config: AlertConfig,
    rules: Vec<AlertRule>,
    triggered_alerts: Vec<AlertState>,
}

impl AlertManager {
    pub fn new() -> Self {
        Self {
            config: AlertConfig::default(),
            rules: Self::default_rules(),
            triggered_alerts: Vec::new(),
        }
    }

    fn default_rules() -> Vec<AlertRule> {
        vec![
            AlertRule {
                name: "High Connection Pool Utilization".to_string(),
                severity: AlertSeverity::Warning,
                condition: AlertCondition::ConnectionPoolHigh { threshold: 85.0 },
                message: "Connection pool utilization is above {}%".to_string(),
                enabled: true,
            },
            AlertRule {
                name: "High Slow Query Count".to_string(),
                severity: AlertSeverity::Warning,
                condition: AlertCondition::SlowQueriesHigh { count: 10 },
                message: "Number of slow queries exceeded threshold".to_string(),
                enabled: true,
            },
            AlertRule {
                name: "Low Data Integrity Score".to_string(),
                severity: AlertSeverity::Critical,
                condition: AlertCondition::IntegrityScoreLow { threshold: 90.0 },
                message: "Data integrity score is below {}%".to_string(),
                enabled: true,
            },
            AlertRule {
                name: "High Average Query Time".to_string(),
                severity: AlertSeverity::Warning,
                condition: AlertCondition::QueryTimeHigh { threshold_ms: 100 },
                message: "Average query time exceeds {}ms".to_string(),
                enabled: true,
            },
        ]
    }

    pub fn check_health_status(&mut self, health: &crate::storage::DatabaseHealthStatus) -> Vec<AlertState> {
        let mut alerts = Vec::new();

        for rule in &self.rules {
            if !rule.enabled {
                continue;
            }

            if let Some(alert) = self.evaluate_rule(rule, health) {
                alerts.push(alert);
                self.triggered_alerts.push(alert.clone());
            }
        }

        alerts
    }

    fn evaluate_rule(&self, rule: &AlertRule, health: &crate::storage::DatabaseHealthStatus) -> Option<AlertState> {
        match &rule.condition {
            AlertCondition::ConnectionPoolHigh { threshold } => {
                if health.connection_pool_status.connection_utilization > *threshold {
                    Some(AlertState {
                        triggered_at: chrono::Utc::now(),
                        rule_name: rule.name.clone(),
                        severity: rule.severity.clone(),
                        message: rule.message.replace("{}", &format!("{:.1}", health.connection_pool_status.connection_utilization)),
                        acknowledged: false,
                    })
                } else {
                    None
                }
            }
            AlertCondition::SlowQueriesHigh { count } => {
                if health.performance_metrics.slow_queries_count > *count {
                    Some(AlertState {
                        triggered_at: chrono::Utc::now(),
                        rule_name: rule.name.clone(),
                        severity: rule.severity.clone(),
                        message: format!("Slow queries: {}", health.performance_metrics.slow_queries_count),
                        acknowledged: false,
                    })
                } else {
                    None
                }
            }
            AlertCondition::IntegrityScoreLow { threshold: _ } => {
                None
            }
            AlertCondition::QueryTimeHigh { threshold_ms } => {
                if health.performance_metrics.average_query_time_ms > *threshold_ms as f64 {
                    Some(AlertState {
                        triggered_at: chrono::Utc::now(),
                        rule_name: rule.name.clone(),
                        severity: rule.severity.clone(),
                        message: format!("Avg query time: {:.2}ms", health.performance_metrics.average_query_time_ms),
                        acknowledged: false,
                    })
                } else {
                    None
                }
            }
        }
    }

    pub fn get_triggered_alerts(&self) -> &[AlertState] {
        &self.triggered_alerts
    }

    pub fn acknowledge_alert(&mut self, index: usize) -> bool {
        if index < self.triggered_alerts.len() {
            self.triggered_alerts[index].acknowledged = true;
            true
        } else {
            false
        }
    }

    pub fn clear_acknowledged(&mut self) {
        self.triggered_alerts.retain(|a| !a.acknowledged);
    }
}

impl Default for AlertManager {
    fn default() -> Self {
        Self::new()
    }
}
