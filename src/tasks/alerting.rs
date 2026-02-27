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
    #[allow(dead_code)]
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
                alerts.push(alert.clone());
                self.triggered_alerts.push(alert);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alert_config_default() {
        let config = AlertConfig::default();
        
        assert!(config.enabled);
        assert_eq!(config.connection_pool_threshold, 85.0);
        assert_eq!(config.slow_query_threshold_ms, 100);
        assert_eq!(config.max_slow_queries, 10);
        assert_eq!(config.integrity_score_threshold, 90.0);
        assert_eq!(config.check_interval, Duration::from_secs(60));
    }

    #[test]
    fn test_alert_severity_variants() {
        let critical = AlertSeverity::Critical;
        let warning = AlertSeverity::Warning;
        let info = AlertSeverity::Info;

        assert!(matches!(critical, AlertSeverity::Critical));
        assert!(matches!(warning, AlertSeverity::Warning));
        assert!(matches!(info, AlertSeverity::Info));
    }

    #[test]
    fn test_alert_condition_variants() {
        let pool_high = AlertCondition::ConnectionPoolHigh { threshold: 90.0 };
        let slow_queries = AlertCondition::SlowQueriesHigh { count: 5 };
        let integrity_low = AlertCondition::IntegrityScoreLow { threshold: 95.0 };
        let query_time = AlertCondition::QueryTimeHigh { threshold_ms: 50 };

        assert!(matches!(pool_high, AlertCondition::ConnectionPoolHigh { .. }));
        assert!(matches!(slow_queries, AlertCondition::SlowQueriesHigh { .. }));
        assert!(matches!(integrity_low, AlertCondition::IntegrityScoreLow { .. }));
        assert!(matches!(query_time, AlertCondition::QueryTimeHigh { .. }));
    }

    #[test]
    fn test_alert_rule_creation() {
        let rule = AlertRule {
            name: "Test Alert".to_string(),
            severity: AlertSeverity::Warning,
            condition: AlertCondition::SlowQueriesHigh { count: 10 },
            message: "Test message".to_string(),
            enabled: true,
        };

        assert_eq!(rule.name, "Test Alert");
        assert!(rule.enabled);
    }

    #[test]
    fn test_alert_state_creation() {
        let state = AlertState {
            triggered_at: chrono::Utc::now(),
            rule_name: "Test Rule".to_string(),
            severity: AlertSeverity::Critical,
            message: "Critical alert triggered".to_string(),
            acknowledged: false,
        };

        assert_eq!(state.rule_name, "Test Rule");
        assert!(!state.acknowledged);
    }

    #[test]
    fn test_alert_manager_creation() {
        let manager = AlertManager::new();
        
        assert!(manager.config.enabled);
        assert!(!manager.rules.is_empty());
        assert!(manager.triggered_alerts.is_empty());
    }

    #[test]
    fn test_alert_manager_default() {
        let manager = AlertManager::default();
        
        assert!(manager.config.enabled);
    }

    #[test]
    fn test_alert_config_serialization() {
        let config = AlertConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        
        assert!(json.contains("enabled"));
        assert!(json.contains("connection_pool_threshold"));
    }

    #[test]
    fn test_alert_severity_serialization() {
        let critical = AlertSeverity::Critical;
        let json = serde_json::to_string(&critical).unwrap();
        
        assert!(json.contains("Critical"));
    }

    #[test]
    fn test_alert_rule_serialization() {
        let rule = AlertRule {
            name: "Test".to_string(),
            severity: AlertSeverity::Warning,
            condition: AlertCondition::ConnectionPoolHigh { threshold: 85.0 },
            message: "Test".to_string(),
            enabled: true,
        };

        let json = serde_json::to_string(&rule).unwrap();
        assert!(json.contains("Test"));
        assert!(json.contains("Warning"));
    }

    #[test]
    fn test_alert_state_serialization() {
        let state = AlertState {
            triggered_at: chrono::Utc::now(),
            rule_name: "Rule".to_string(),
            severity: AlertSeverity::Info,
            message: "Info alert".to_string(),
            acknowledged: true,
        };

        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("Rule"));
        assert!(json.contains("Info"));
    }

    #[test]
    fn test_alert_config_clone() {
        let config = AlertConfig::default();
        let cloned = config.clone();
        
        assert_eq!(config.enabled, cloned.enabled);
        assert_eq!(config.connection_pool_threshold, cloned.connection_pool_threshold);
    }

    #[test]
    fn test_alert_rule_clone() {
        let rule = AlertRule {
            name: "Clone Test".to_string(),
            severity: AlertSeverity::Critical,
            condition: AlertCondition::QueryTimeHigh { threshold_ms: 100 },
            message: "Clone message".to_string(),
            enabled: false,
        };

        let cloned = rule.clone();
        assert_eq!(rule.name, cloned.name);
        assert_eq!(rule.enabled, cloned.enabled);
    }
}
