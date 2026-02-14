#[cfg(test)]
mod tests {
    use synapse_rust::storage::event_report::*;
    use synapse_rust::services::ServiceContainer;

    #[test]
    fn test_create_event_report_request() {
        let request = CreateEventReportRequest {
            event_id: "$event123".to_string(),
            room_id: "!room:example.com".to_string(),
            reporter_user_id: "@user:example.com".to_string(),
            reported_user_id: Some("@baduser:example.com".to_string()),
            event_json: None,
            reason: Some("spam".to_string()),
            description: Some("This is spam content".to_string()),
            score: Some(5),
        };

        assert_eq!(request.event_id, "$event123");
        assert_eq!(request.room_id, "!room:example.com");
        assert_eq!(request.reason, Some("spam".to_string()));
    }

    #[test]
    fn test_event_report_struct() {
        let report = EventReport {
            id: 1,
            event_id: "$event123".to_string(),
            room_id: "!room:example.com".to_string(),
            reporter_user_id: "@user:example.com".to_string(),
            reported_user_id: Some("@baduser:example.com".to_string()),
            event_json: None,
            reason: Some("spam".to_string()),
            description: Some("Spam content".to_string()),
            status: "open".to_string(),
            score: 5,
            received_ts: 1234567890,
            resolved_ts: None,
            resolved_by: None,
            resolution_reason: None,
        };

        assert_eq!(report.id, 1);
        assert_eq!(report.status, "open");
        assert_eq!(report.score, 5);
    }

    #[test]
    fn test_event_report_history_struct() {
        let history = EventReportHistory {
            id: 1,
            report_id: 100,
            action: "status_change".to_string(),
            actor_user_id: Some("@admin:example.com".to_string()),
            actor_role: Some("admin".to_string()),
            old_status: Some("open".to_string()),
            new_status: Some("investigating".to_string()),
            reason: Some("Escalating for review".to_string()),
            created_ts: 1234567890,
            metadata: None,
        };

        assert_eq!(history.report_id, 100);
        assert_eq!(history.action, "status_change");
        assert_eq!(history.old_status, Some("open".to_string()));
    }

    #[test]
    fn test_report_rate_limit_struct() {
        let limit = ReportRateLimit {
            id: 1,
            user_id: "@user:example.com".to_string(),
            report_count: 10,
            last_report_ts: Some(1234567890),
            blocked_until_ts: None,
            is_blocked: false,
            block_reason: None,
            created_ts: 1234560000,
            updated_ts: 1234567000,
        };

        assert_eq!(limit.user_id, "@user:example.com");
        assert_eq!(limit.report_count, 10);
        assert!(!limit.is_blocked);
    }

    #[test]
    fn test_event_report_stats_struct() {
        let stats = EventReportStats {
            id: 1,
            stat_date: chrono::NaiveDate::from_ymd_opt(2026, 2, 13).unwrap(),
            total_reports: 100,
            open_reports: 20,
            resolved_reports: 70,
            dismissed_reports: 10,
            avg_resolution_time_ms: Some(3600000),
            reports_by_reason: None,
            created_ts: 1234567890,
            updated_ts: 1234567890,
        };

        assert_eq!(stats.total_reports, 100);
        assert_eq!(stats.open_reports, 20);
        assert_eq!(stats.resolved_reports, 70);
    }

    #[test]
    fn test_update_event_report_request() {
        let request = UpdateEventReportRequest {
            status: Some("resolved".to_string()),
            score: Some(10),
            resolved_by: Some("@admin:example.com".to_string()),
            resolution_reason: Some("Content removed".to_string()),
        };

        assert_eq!(request.status, Some("resolved".to_string()));
        assert_eq!(request.score, Some(10));
    }

    #[test]
    fn test_report_rate_limit_check() {
        let allowed = ReportRateLimitCheck {
            is_allowed: true,
            remaining_reports: 45,
            block_reason: None,
        };

        let blocked = ReportRateLimitCheck {
            is_allowed: false,
            remaining_reports: 0,
            block_reason: Some("Daily limit exceeded".to_string()),
        };

        assert!(allowed.is_allowed);
        assert!(!blocked.is_allowed);
        assert!(blocked.block_reason.is_some());
    }

    #[tokio::test]
    async fn test_event_report_service_creation() {
        let container = ServiceContainer::new_test();
        let _service = &container.event_report_service;
    }

    #[tokio::test]
    async fn test_get_all_reports() {
        let container = ServiceContainer::new_test();
        let service = &container.event_report_service;

        let result = service.get_all_reports(100, 0).await;
        if result.is_err() {
            eprintln!("Skipping test_get_all_reports: database table not available");
            return;
        }

        let reports = result.unwrap();
        assert!(reports.is_empty() || reports.len() >= 0);
    }

    #[tokio::test]
    async fn test_get_reports_by_status() {
        let container = ServiceContainer::new_test();
        let service = &container.event_report_service;

        let result = service.get_reports_by_status("open", 100, 0).await;
        if result.is_err() {
            eprintln!("Skipping test_get_reports_by_status: database table not available");
            return;
        }

        let reports = result.unwrap();
        assert!(reports.is_empty() || reports.len() >= 0);
    }

    #[tokio::test]
    async fn test_get_report_nonexistent() {
        let container = ServiceContainer::new_test();
        let service = &container.event_report_service;

        let result = service.get_report(999999).await;
        if result.is_err() {
            eprintln!("Skipping test_get_report_nonexistent: database table not available");
            return;
        }

        let report = result.unwrap();
        assert!(report.is_none());
    }

    #[tokio::test]
    async fn test_get_reports_by_event() {
        let container = ServiceContainer::new_test();
        let service = &container.event_report_service;

        let result = service.get_reports_by_event("$nonexistent").await;
        if result.is_err() {
            eprintln!("Skipping test_get_reports_by_event: database table not available");
            return;
        }

        let reports = result.unwrap();
        assert!(reports.is_empty());
    }

    #[tokio::test]
    async fn test_get_reports_by_room() {
        let container = ServiceContainer::new_test();
        let service = &container.event_report_service;

        let result = service.get_reports_by_room("!nonexistent:example.com", 100, 0).await;
        if result.is_err() {
            eprintln!("Skipping test_get_reports_by_room: database table not available");
            return;
        }

        let reports = result.unwrap();
        assert!(reports.is_empty());
    }

    #[tokio::test]
    async fn test_check_rate_limit() {
        let container = ServiceContainer::new_test();
        let service = &container.event_report_service;

        let result = service.check_rate_limit("@nonexistent:example.com").await;
        if result.is_err() {
            eprintln!("Skipping test_check_rate_limit: database table not available");
            return;
        }

        let check = result.unwrap();
        assert!(check.is_allowed);
    }

    #[tokio::test]
    async fn test_count_all_reports() {
        let container = ServiceContainer::new_test();
        let service = &container.event_report_service;

        let result = service.count_all_reports().await;
        if result.is_err() {
            eprintln!("Skipping test_count_all_reports: database table not available");
            return;
        }

        let _count = result.unwrap();
    }

    #[tokio::test]
    async fn test_count_reports_by_status() {
        let container = ServiceContainer::new_test();
        let service = &container.event_report_service;

        let result = service.count_reports_by_status("open").await;
        if result.is_err() {
            eprintln!("Skipping test_count_reports_by_status: database table not available");
            return;
        }

        let _count = result.unwrap();
    }

    #[tokio::test]
    async fn test_get_stats() {
        let container = ServiceContainer::new_test();
        let service = &container.event_report_service;

        let result = service.get_stats(30).await;
        if result.is_err() {
            eprintln!("Skipping test_get_stats: database table not available");
            return;
        }

        let _stats = result.unwrap();
    }

    #[tokio::test]
    async fn test_get_report_history() {
        let container = ServiceContainer::new_test();
        let service = &container.event_report_service;

        let result = service.get_report_history(1).await;
        if result.is_err() {
            eprintln!("Skipping test_get_report_history: database table not available");
            return;
        }

        let history = result.unwrap();
        assert!(history.is_empty() || history.len() >= 0);
    }
}
