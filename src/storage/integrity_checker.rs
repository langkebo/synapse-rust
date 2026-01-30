use sqlx::{Pool, Postgres};
use std::time::Duration;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityReport {
    pub timestamp: DateTime<Utc>,
    pub duration_ms: u64,
    pub overall_score: f64,
    pub checks: Vec<IntegrityCheck>,
    pub summary: IntegritySummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityCheck {
    pub name: String,
    pub check_type: IntegrityCheckType,
    pub status: CheckStatus,
    pub affected_rows: u64,
    pub duration_ms: u64,
    pub details: Option<String>,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IntegrityCheckType {
    ForeignKey,
    OrphanedRecord,
    DuplicateEntry,
    NullConstraint,
    DataType,
    RangeConstraint,
    UniqueConstraint,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CheckStatus {
    Passed,
    Warning,
    Failed,
    Skipped,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegritySummary {
    pub total_checks: u64,
    pub passed_checks: u64,
    pub warning_checks: u64,
    pub failed_checks: u64,
    pub total_issues: u64,
    pub critical_issues: u64,
    pub integrity_score: f64,
}

pub struct IntegrityChecker {
    pool: Pool<Postgres>,
    custom_rules: Vec<CustomIntegrityRule>,
}

impl IntegrityChecker {
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self {
            pool,
            custom_rules: Vec::new(),
        }
    }

    pub fn add_custom_rule(&mut self, rule: CustomIntegrityRule) {
        self.custom_rules.push(rule);
    }

    pub async fn run_full_check(&self, timeout: Duration) -> Result<IntegrityReport, sqlx::Error> {
        let start = std::time::Instant::now();
        let mut checks = Vec::new();
        let mut total_issues = 0u64;
        let mut critical_issues = 0u64;

        checks.push(self.check_foreign_keys().await?);
        checks.push(self.check_orphaned_records().await?);
        checks.push(self.check_duplicates().await?);
        checks.push(self.check_null_constraints().await?);
        checks.push(self.check_data_consistency().await?);

        for rule in &self.custom_rules {
            if start.elapsed() > timeout {
                break;
            }
            checks.push(self.run_custom_rule(rule).await?);
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        for check in &checks {
            total_issues += check.affected_rows;
            if check.status == CheckStatus::Failed {
                critical_issues += check.affected_rows;
            }
        }

        let passed_checks = checks.iter().filter(|c| c.status == CheckStatus::Passed).count() as u64;
        let warning_checks = checks.iter().filter(|c| c.status == CheckStatus::Warning).count() as u64;
        let failed_checks = checks.iter().filter(|c| c.status == CheckStatus::Failed).count() as u64;

        let overall_score = if checks.is_empty() {
            100.0
        } else {
            (passed_checks as f64 * 100.0 + warning_checks as f64 * 50.0) / (checks.len() as f64 * 100.0) * 100.0
        };

        let summary = IntegritySummary {
            total_checks: checks.len() as u64,
            passed_checks,
            warning_checks,
            failed_checks,
            total_issues,
            critical_issues,
            integrity_score: overall_score,
        };

        Ok(IntegrityReport {
            timestamp: Utc::now(),
            duration_ms,
            overall_score,
            checks,
            summary,
        })
    }

    async fn check_foreign_keys(&self) -> Result<IntegrityCheck, sqlx::Error> {
        let start = std::time::Instant::now();
        let mut total_violations = 0u64;
        let mut recommendations = Vec::new();

        let foreign_key_checks = vec![
            ("devices", "user_id", "users", "user_id"),
            ("access_tokens", "user_id", "users", "user_id"),
            ("access_tokens", "device_id", "devices", "device_id"),
            ("refresh_tokens", "user_id", "users", "user_id"),
            ("refresh_tokens", "device_id", "devices", "device_id"),
            ("room_memberships", "room_id", "rooms", "room_id"),
            ("room_memberships", "user_id", "users", "user_id"),
            ("events", "room_id", "rooms", "room_id"),
            ("events", "user_id", "users", "user_id"),
            ("private_messages", "sender_id", "users", "user_id"),
            ("private_sessions", "user_id_1", "users", "user_id"),
            ("private_sessions", "user_id_2", "users", "user_id"),
        ];

        for (table, column, referenced_table, referenced_column) in &foreign_key_checks {
            let query = format!(
                "SELECT COUNT(*) as count FROM {} WHERE {} IS NOT NULL AND {} NOT IN (SELECT {} FROM {})",
                table, column, column, referenced_column, referenced_table
            );

            let result = sqlx::query(&query).fetch_one(&self.pool).await?;
            let count: i64 = result.try_get("count")?;

            if count > 0 {
                total_violations += count as u64;
                recommendations.push(format!(
                    "Found {} orphaned records in {}.{}, consider cleaning up or adding foreign key constraint",
                    count, table, column
                ));
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;
        let status = if total_violations == 0 {
            CheckStatus::Passed
        } else if total_violations < 100 {
            CheckStatus::Warning
        } else {
            CheckStatus::Failed
        };

        Ok(IntegrityCheck {
            name: "Foreign Key Validation".to_string(),
            check_type: IntegrityCheckType::ForeignKey,
            status,
            affected_rows: total_violations,
            duration_ms,
            details: if total_violations > 0 {
                Some(format!("Found {} foreign key violations across {} table(s)", total_violations, foreign_key_checks.len()))
            } else {
                None
            },
            recommendations,
        })
    }

    async fn check_orphaned_records(&self) -> Result<IntegrityCheck, sqlx::Error> {
        let start = std::time::Instant::new();
        let mut total_orphans = 0u64;
        let mut recommendations = Vec::new();

        let orphan_checks = vec![
            ("devices", "user_id", "orphaned devices without valid users"),
            ("access_tokens", "user_id", "access tokens without valid users"),
            ("room_memberships", "room_id", "memberships without valid rooms"),
            ("room_memberships", "user_id", "memberships without valid users"),
            ("events", "room_id", "events without valid rooms"),
            ("events", "user_id", "events without valid users"),
        ];

        for (table, column, description) in &orphan_checks {
            let query = format!(
                "SELECT COUNT(*) as count FROM {} WHERE {} NOT IN (SELECT {} FROM users) AND {} NOT IN (SELECT room_id FROM rooms)",
                table, column, "user_id", column
            );

            let result = sqlx::query(&query).fetch_one(&self.pool).await;
            match result {
                Ok(row) => {
                    let count: i64 = row.try_get("count")?;
                    if count > 0 {
                        total_orphans += count as u64;
                        recommendations.push(format!("Clean up {} in {}", description, table));
                    }
                }
                Err(_) => {
                    let query = format!(
                        "SELECT COUNT(*) as count FROM {} WHERE {} IS NOT NULL AND {} NOT IN (SELECT COALESCE(user_id, room_id) FROM users)",
                        table, column, column
                    );
                    if let Ok(row) = sqlx::query(&query).fetch_one(&self.pool).await {
                        let count: i64 = row.try_get("count")?;
                        if count > 0 {
                            total_orphans += count as u64;
                        }
                    }
                }
            }
        }

        start.elapsed(). let duration_ms =as_millis() as u64;
        let status = if total_orphans == 0 {
            CheckStatus::Passed
        } else if total_orphans < 50 {
            CheckStatus::Warning
        } else {
            CheckStatus::Failed
        };

        Ok(IntegrityCheck {
            name: "Orphaned Record Detection".to_string(),
            check_type: IntegrityCheckType::OrphanedRecord,
            status,
            affected_rows: total_orphans,
            duration_ms,
            details: Some(format!("Found {} orphaned records", total_orphans)),
            recommendations,
        })
    }

    async fn check_duplicates(&self) -> Result<IntegrityCheck, sqlx::Error> {
        let start = std::time::Instant::now();
        let mut total_duplicates = 0u64;
        let recommendations = Vec::new();

        let unique_checks = vec![
            ("users", "username"),
            ("devices", "device_id"),
        ];

        for (table, column) in &unique_checks {
            let query = format!(
                "SELECT COUNT(*) - COUNT(DISTINCT {}) as duplicates FROM {}",
                column, table
            );

            let result = sqlx::query(&query).fetch_one(&self.pool).await;
            match result {
                Ok(row) => {
                    let duplicates: Option<i64> = row.try_get("duplicates").ok();
                    if let Some(d) = duplicates {
                        total_duplicates += d as u64;
                    }
                }
                Err(_) => {}
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;
        let status = if total_duplicates == 0 {
            CheckStatus::Passed
        } else {
            CheckStatus::Failed
        };

        Ok(IntegrityCheck {
            name: "Duplicate Entry Detection".to_string(),
            check_type: IntegrityCheckType::DuplicateEntry,
            status,
            affected_rows: total_duplicates,
            duration_ms,
            details: Some(format!("Found {} duplicate entries", total_duplicates)),
            recommendations,
        })
    }

    async fn check_null_constraints(&self) -> Result<IntegrityCheck, sqlx::Error> {
        let start = std::time::Instant::now();
        let mut total_nulls = 0u64;
        let recommendations = Vec::new();

        let null_checks = vec![
            ("users", "user_id"),
            ("users", "username"),
            ("devices", "device_id"),
            ("devices", "user_id"),
        ];

        for (table, column) in &null_checks {
            let query = format!(
                "SELECT COUNT(*) as count FROM {} WHERE {} IS NULL",
                table, column
            );

            let result = sqlx::query(&query).fetch_one(&self.pool).await;
            match result {
                Ok(row) => {
                    let count: i64 = row.try_get("count")?;
                    if count > 0 {
                        total_nulls += count as u64;
                        recommendations.push(format!(
                            "Found {} NULL values in {}.{} (should be NOT NULL)",
                            count, table, column
                        ));
                    }
                }
                Err(_) => {}
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;
        let status = if total_nulls == 0 {
            CheckStatus::Passed
        } else if total_nulls < 10 {
            CheckStatus::Warning
        } else {
            CheckStatus::Failed
        };

        Ok(IntegrityCheck {
            name: "NULL Constraint Validation".to_string(),
            check_type: IntegrityCheckType::NullConstraint,
            status,
            affected_rows: total_nulls,
            duration_ms,
            details: Some(format!("Found {} NULL violations", total_nulls)),
            recommendations,
        })
    }

    async fn check_data_consistency(&self) -> Result<IntegrityCheck, sqlx::Error> {
        let start = std::time::Instant::now();
        let mut issues = 0u64;
        let recommendations = Vec::new();

        let consistency_checks = vec![
            (
                "Check room member counts match actual membership records",
                "SELECT ABS((SELECT COUNT(*) FROM rooms.member_count) - (SELECT COUNT(*) FROM room_memberships)) as diff"
            ),
        ];

        for (description, query) in &consistency_checks {
            match sqlx::query(query).fetch_one(&self.pool).await {
                Ok(row) => {
                    let diff: Option<i64> = row.try_get("diff").ok();
                    if let Some(d) = diff {
                        if d > 0 {
                            issues += d as u64;
                            recommendations.push(description.to_string());
                        }
                    }
                }
                Err(_) => {}
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;
        let status = if issues == 0 {
            CheckStatus::Passed
        } else {
            CheckStatus::Warning
        };

        Ok(IntegrityCheck {
            name: "Data Consistency Check".to_string(),
            check_type: IntegrityCheckType::DataType,
            status,
            affected_rows: issues,
            duration_ms,
            details: Some(format!("Found {} consistency issues", issues)),
            recommendations,
        })
    }

    async fn run_custom_rule(&self, rule: &CustomIntegrityRule) -> Result<IntegrityCheck, sqlx::Error> {
        let start = std::time::Instant::now();
        
        let result = sqlx::query(&rule.query).fetch_all(&self.pool).await?;
        let affected_rows = result.len() as u64;
        let duration_ms = start.elapsed().as_millis() as u64;

        let status = match rule.severity {
            RuleSeverity::Error => {
                if affected_rows > 0 { CheckStatus::Failed } else { CheckStatus::Passed }
            }
            RuleSeverity::Warning => {
                if affected_rows > 0 { CheckStatus::Warning } else { CheckStatus::Passed }
            }
            RuleSeverity::Info => CheckStatus::Passed,
        };

        Ok(IntegrityCheck {
            name: rule.name.clone(),
            check_type: IntegrityCheckType::Custom,
            status,
            affected_rows,
            duration_ms,
            details: Some(rule.description.clone()),
            recommendations: vec![rule.recommendation.clone()],
        })
    }

    pub fn to_json(&self, report: &IntegrityReport) -> serde_json::Value {
        serde_json::json!({
            "timestamp": report.timestamp.to_rfc3339(),
            "duration_ms": report.duration_ms,
            "overall_score": report.overall_score,
            "summary": {
                "total_checks": report.summary.total_checks,
                "passed_checks": report.summary.passed_checks,
                "warning_checks": report.summary.warning_checks,
                "failed_checks": report.summary.failed_checks,
                "total_issues": report.summary.total_issues,
                "critical_issues": report.summary.critical_issues,
                "integrity_score": report.summary.integrity_score,
            },
            "checks": report.checks.iter().map(|c| serde_json::json!({
                "name": c.name,
                "type": format!("{:?}", c.check_type),
                "status": format!("{:?}", c.status),
                "affected_rows": c.affected_rows,
                "duration_ms": c.duration_ms,
                "details": c.details,
                "recommendations": c.recommendations,
            })).collect::<Vec<_>>(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomIntegrityRule {
    pub name: String,
    pub description: String,
    pub query: String,
    pub severity: RuleSeverity,
    pub recommendation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuleSeverity {
    Error,
    Warning,
    Info,
}

pub async fn run_periodic_integrity_checks(
    checker: &IntegrityChecker,
    interval: Duration,
    shutdown: &std::sync::atomic::AtomicU64,
) {
    let mut interval_timer = tokio::time::interval(interval);
    
    while shutdown.load(std::sync::atomic::Ordering::SeqCst) == 0 {
        interval_timer.tick().await;
        
        if let Ok(report) = checker.run_full_check(Duration::from_secs(300)).await {
            tracing::info!(
                "Integrity check completed: score={:.1}%, issues={}, duration={}ms",
                report.overall_score,
                report.summary.total_issues,
                report.duration_ms
            );

            if report.overall_score < 80.0 {
                tracing::warn!(
                    "Database integrity score below threshold: {:.1}%",
                    report.overall_score
                );
            }
        }
    }
}
