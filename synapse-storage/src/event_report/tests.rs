use super::*;

#[test]
fn test_event_report_creation() {
    let report = EventReport {
        id: 1,
        event_id: "$event:example.com".to_string(),
        room_id: "!room:example.com".to_string(),
        reporter_user_id: "@reporter:example.com".to_string(),
        reported_user_id: Some("@reported:example.com".to_string()),
        event_json: None,
        reason: Some("Spam".to_string()),
        description: None,
        status: "pending".to_string(),
        score: 0,
        received_ts: 1234567890,
        resolved_ts: None,
        resolved_by: None,
        resolution_reason: None,
    };
    assert_eq!(report.id, 1);
    assert_eq!(report.event_id, "$event:example.com");
}

#[test]
fn test_event_report_with_reason() {
    let report = EventReport {
        id: 2,
        event_id: "$event2:example.com".to_string(),
        room_id: "!room2:example.com".to_string(),
        reporter_user_id: "@reporter2:example.com".to_string(),
        reported_user_id: None,
        event_json: None,
        reason: Some("Inappropriate content".to_string()),
        description: Some("Test description".to_string()),
        status: "open".to_string(),
        score: -50,
        received_ts: 1234567890,
        resolved_ts: None,
        resolved_by: None,
        resolution_reason: None,
    };
    assert!(report.reason.is_some());
    assert_eq!(report.reason.as_deref(), Some("Inappropriate content"));
}

#[test]
fn test_event_report_history_creation() {
    let history = EventReportHistory {
        id: 1,
        report_id: 1,
        action: "status_change".to_string(),
        actor_user_id: Some("@admin:example.com".to_string()),
        actor_role: Some("moderator".to_string()),
        old_status: Some("pending".to_string()),
        new_status: Some("resolved".to_string()),
        reason: Some("Reviewed and resolved".to_string()),
        created_ts: 1234567890,
        metadata: None,
    };
    assert_eq!(history.report_id, 1);
    assert!(history.actor_user_id.is_some());
}

#[test]
fn test_report_rate_limit_creation() {
    let rate_limit = ReportRateLimit {
        id: 1,
        user_id: "@user:example.com".to_string(),
        report_count: 5,
        last_report_at: Some(1234567890),
        blocked_until_at: None,
        is_blocked: false,
        block_reason: None,
        created_ts: 1234567800,
        updated_ts: 1234567890,
    };
    assert_eq!(rate_limit.report_count, 5);
}

#[test]
fn test_create_event_report_request() {
    let request = CreateEventReportRequest {
        event_id: "$new_event:example.com".to_string(),
        room_id: "!new_room:example.com".to_string(),
        reporter_user_id: "@reporter:example.com".to_string(),
        reported_user_id: Some("@reported:example.com".to_string()),
        reason: Some("New report".to_string()),
        description: None,
        event_json: None,
        score: Some(0),
    };
    assert_eq!(request.event_id, "$new_event:example.com");
}

#[test]
fn test_update_event_report_request() {
    let request = UpdateEventReportRequest {
        status: Some("resolved".to_string()),
        score: Some(0),
        resolved_by: Some("@admin:example.com".to_string()),
        resolution_reason: Some("Resolved by admin".to_string()),
    };
    assert!(request.status.is_some());
    assert!(request.resolved_by.is_some());
}
