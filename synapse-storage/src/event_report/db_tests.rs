use std::env;
use std::sync::Arc;

use chrono::Utc;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

use super::*;

async fn test_pool() -> Arc<PgPool> {
    let db_url = env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
    let pool =
        PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
    Arc::new(pool)
}

async fn cleanup_event_reports(pool: &PgPool, prefix: &str) {
    let _ =
        sqlx::query("DELETE FROM event_reports WHERE event_id LIKE $1").bind(format!("{prefix}%")).execute(pool).await;
}

async fn cleanup_rate_limits(pool: &PgPool, prefix: &str) {
    let _ = sqlx::query("DELETE FROM report_rate_limits WHERE user_id LIKE $1")
        .bind(format!("{prefix}%"))
        .execute(pool)
        .await;
}

async fn cleanup_users(pool: &PgPool, prefix: &str) {
    let _ = sqlx::query("DELETE FROM users WHERE user_id LIKE $1").bind(format!("{prefix}%")).execute(pool).await;
}

async fn cleanup_all(pool: &PgPool, prefix: &str) {
    cleanup_event_reports(pool, prefix).await;
    cleanup_rate_limits(pool, prefix).await;
    cleanup_users(pool, prefix).await;
}

/// Insert a user row to satisfy the FK constraint on report_rate_limits.user_id -> users.user_id.
async fn ensure_user(pool: &PgPool, user_id: &str) {
    let now = Utc::now().timestamp_millis();
    let _ = sqlx::query(
        "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(user_id)
    .bind(user_id)
    .bind(now)
    .execute(pool)
    .await;
}

fn make_request(prefix: &str, kind: &str) -> CreateEventReportRequest {
    CreateEventReportRequest {
        event_id: format!("{prefix}_ev_{kind}"),
        room_id: format!("{prefix}_room"),
        reporter_user_id: format!("{prefix}_reporter"),
        reported_user_id: Some(format!("{prefix}_reported")),
        event_json: Some(serde_json::json!({"content": kind})),
        reason: Some(format!("{kind} reason")),
        description: Some(format!("{kind} description")),
        score: None,
    }
}

// --- create_report ---

#[tokio::test]
async fn test_create_report_defaults() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string();
    let prefix = format!("cr_def_{suffix}");
    cleanup_all(&pool, &prefix).await;

    let storage = EventReportStorage::new(&pool);
    let request = make_request(&prefix, "a");
    let report = storage.create_report(request).await.expect("create_report should succeed");

    assert!(report.id > 0, "id should be assigned");
    assert_eq!(report.status, "open", "status should default to open");
    assert_eq!(report.score, 0, "score should default to 0 when None");
    assert!(report.received_ts > 0, "received_ts should be set");
    assert_eq!(report.resolved_ts, None, "resolved_ts should be null on creation");
    assert_eq!(report.resolved_by, None, "resolved_by should be null on creation");

    cleanup_all(&pool, &prefix).await;
}

#[tokio::test]
async fn test_create_report_with_explicit_score() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string();
    let prefix = format!("cr_score_{suffix}");
    cleanup_all(&pool, &prefix).await;

    let storage = EventReportStorage::new(&pool);
    let mut request = make_request(&prefix, "b");
    request.score = Some(42);
    let report = storage.create_report(request).await.expect("create_report should succeed");

    assert_eq!(report.score, 42, "explicit score should be preserved");

    cleanup_all(&pool, &prefix).await;
}

// --- get_report ---

#[tokio::test]
async fn test_get_report_found() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string();
    let prefix = format!("gr_found_{suffix}");
    cleanup_all(&pool, &prefix).await;

    let storage = EventReportStorage::new(&pool);
    let created = storage.create_report(make_request(&prefix, "x")).await.expect("create_report should succeed");

    let fetched =
        storage.get_report(created.id).await.expect("get_report should succeed").expect("report should exist");

    assert_eq!(fetched.id, created.id);
    assert_eq!(fetched.event_id, created.event_id);
    assert_eq!(fetched.room_id, created.room_id);
    assert_eq!(fetched.reporter_user_id, created.reporter_user_id);

    cleanup_all(&pool, &prefix).await;
}

#[tokio::test]
async fn test_get_report_not_found() {
    let pool = test_pool().await;
    let storage = EventReportStorage::new(&pool);

    let result = storage.get_report(-99999).await.expect("get_report should succeed");

    assert!(result.is_none(), "non-existent id should return None");
}

// --- get_reports_by_event ---

#[tokio::test]
async fn test_get_reports_by_event_ordering() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string();
    let prefix = format!("gbe_{suffix}");
    cleanup_all(&pool, &prefix).await;

    let storage = EventReportStorage::new(&pool);
    let event_id = format!("{prefix}_shared_ev");

    // Create 3 reports for the same event
    for i in 0..3 {
        let mut req = make_request(&prefix, &format!("ev_{i}"));
        req.event_id = event_id.clone();
        storage.create_report(req).await.expect("create_report should succeed");
    }

    let reports = storage.get_reports_by_event(&event_id).await.expect("get_reports_by_event should succeed");

    assert_eq!(reports.len(), 3, "should return all 3 reports");

    // Verify ordering: received_ts DESC, id DESC
    for i in 1..reports.len() {
        if reports[i - 1].received_ts == reports[i].received_ts {
            assert!(reports[i - 1].id > reports[i].id, "same received_ts should sort by id DESC");
        } else {
            assert!(reports[i - 1].received_ts > reports[i].received_ts, "should sort by received_ts DESC");
        }
    }

    cleanup_all(&pool, &prefix).await;
}

#[tokio::test]
async fn test_get_reports_by_event_none() {
    let pool = test_pool().await;
    let storage = EventReportStorage::new(&pool);

    let reports =
        storage.get_reports_by_event("$nonexistent:example.com").await.expect("get_reports_by_event should succeed");

    assert!(reports.is_empty(), "should return empty Vec for unknown event");
}

// --- get_reports_by_room ---

#[tokio::test]
async fn test_get_reports_by_room_basic() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string();
    let prefix = format!("gbr_{suffix}");
    cleanup_all(&pool, &prefix).await;

    let storage = EventReportStorage::new(&pool);
    let room_a = format!("{prefix}_roomA");
    let room_b = format!("{prefix}_roomB");

    // Create 2 reports in room A
    for i in 0..2 {
        let mut req = make_request(&prefix, &format!("ra_{i}"));
        req.room_id = room_a.clone();
        req.event_id = format!("{prefix}_ev_ra_{i}");
        storage.create_report(req).await.expect("create_report should succeed");
    }

    // Create 3 reports in room B
    for i in 0..3 {
        let mut req = make_request(&prefix, &format!("rb_{i}"));
        req.room_id = room_b.clone();
        req.event_id = format!("{prefix}_ev_rb_{i}");
        storage.create_report(req).await.expect("create_report should succeed");
    }

    let room_a_reports =
        storage.get_reports_by_room(&room_a, 20, None, None).await.expect("get_reports_by_room should succeed");

    assert_eq!(room_a_reports.len(), 2, "should return only room A reports");
    for r in &room_a_reports {
        assert_eq!(r.room_id, room_a, "all reports should belong to room A");
    }

    let room_b_reports =
        storage.get_reports_by_room(&room_b, 20, None, None).await.expect("get_reports_by_room should succeed");

    assert_eq!(room_b_reports.len(), 3);

    cleanup_all(&pool, &prefix).await;
}

#[tokio::test]
async fn test_get_reports_by_room_limit() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string();
    let prefix = format!("gbr_lim_{suffix}");
    cleanup_all(&pool, &prefix).await;

    let storage = EventReportStorage::new(&pool);
    let room_id = format!("{prefix}_room");

    for i in 0..5 {
        let mut req = make_request(&prefix, &format!("r_{i}"));
        req.room_id = room_id.clone();
        req.event_id = format!("{prefix}_ev_r_{i}");
        storage.create_report(req).await.expect("create_report should succeed");
    }

    let reports =
        storage.get_reports_by_room(&room_id, 2, None, None).await.expect("get_reports_by_room should succeed");

    assert_eq!(reports.len(), 2, "should respect limit");

    cleanup_all(&pool, &prefix).await;
}

// --- get_reports_by_reporter ---

#[tokio::test]
async fn test_get_reports_by_reporter_basic() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string();
    let prefix = format!("gbrep_{suffix}");
    cleanup_all(&pool, &prefix).await;

    let storage = EventReportStorage::new(&pool);
    let reporter_a = format!("{prefix}_reporterA");
    let reporter_b = format!("{prefix}_reporterB");

    // 2 reports from reporter A
    for i in 0..2 {
        let mut req = make_request(&prefix, &format!("rep_a_{i}"));
        req.reporter_user_id = reporter_a.clone();
        req.event_id = format!("{prefix}_ev_rep_a_{i}");
        storage.create_report(req).await.expect("create_report should succeed");
    }

    // 1 report from reporter B
    let mut req = make_request(&prefix, "rep_b_0");
    req.reporter_user_id = reporter_b.clone();
    req.event_id = format!("{prefix}_ev_rep_b_0");
    storage.create_report(req).await.expect("create_report should succeed");

    let a_reports = storage
        .get_reports_by_reporter(&reporter_a, 20, None, None)
        .await
        .expect("get_reports_by_reporter should succeed");

    assert_eq!(a_reports.len(), 2);
    for r in &a_reports {
        assert_eq!(r.reporter_user_id, reporter_a);
    }

    cleanup_all(&pool, &prefix).await;
}

#[tokio::test]
async fn test_get_reports_by_reporter_cursor_pagination() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string();
    let prefix = format!("gbrep_cur_{suffix}");
    cleanup_all(&pool, &prefix).await;

    let storage = EventReportStorage::new(&pool);
    let reporter = format!("{prefix}_reporter");

    let mut created_ids = Vec::new();
    for i in 0..5 {
        let mut req = make_request(&prefix, &format!("cur_{i}"));
        req.reporter_user_id = reporter.clone();
        req.event_id = format!("{prefix}_ev_cur_{i}");
        let report = storage.create_report(req).await.expect("create_report should succeed");
        created_ids.push((report.received_ts, report.id));
    }

    // Fetch first page
    let page = storage
        .get_reports_by_reporter(&reporter, 20, None, None)
        .await
        .expect("get_reports_by_reporter should succeed");

    assert_eq!(page.len(), 5);

    // Use cursor from the last item to fetch next page (should be empty)
    let last = page.last().unwrap();
    let next_page = storage
        .get_reports_by_reporter(&reporter, 20, Some(last.received_ts), Some(last.id))
        .await
        .expect("cursor pagination should succeed");

    assert!(next_page.is_empty(), "cursor on last item should return empty page");

    cleanup_all(&pool, &prefix).await;
}

// --- get_reports_by_status ---

#[tokio::test]
async fn test_get_reports_by_status_filtering() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string();
    let prefix = format!("gbs_{suffix}");
    cleanup_all(&pool, &prefix).await;

    let storage = EventReportStorage::new(&pool);

    // Create 2 open reports
    for i in 0..2 {
        storage
            .create_report(make_request(&prefix, &format!("s_open_{i}")))
            .await
            .expect("create_report should succeed");
    }

    // Create 1 resolved report and update its status
    let resolved =
        storage.create_report(make_request(&prefix, "s_resolved")).await.expect("create_report should succeed");
    storage
        .update_report(
            resolved.id,
            UpdateEventReportRequest { status: Some("resolved".to_string()), ..Default::default() },
        )
        .await
        .expect("update_report should succeed");

    let open_reports = storage
        .get_reports_by_status("open", 20, None, None, None)
        .await
        .expect("get_reports_by_status should succeed");

    assert!(open_reports.len() >= 2, "should return at least 2 open reports");
    for r in &open_reports {
        if r.event_id.starts_with(&prefix) {
            assert_eq!(r.status, "open");
        }
    }

    let resolved_reports = storage
        .get_reports_by_status("resolved", 20, None, None, None)
        .await
        .expect("get_reports_by_status should succeed");

    let our_resolved: Vec<_> = resolved_reports.iter().filter(|r| r.id == resolved.id).collect();
    assert_eq!(our_resolved.len(), 1, "should find our resolved report");

    cleanup_all(&pool, &prefix).await;
}

#[tokio::test]
async fn test_get_reports_by_status_triple_cursor() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string();
    let prefix = format!("gbs_cur_{suffix}");
    cleanup_all(&pool, &prefix).await;

    let storage = EventReportStorage::new(&pool);

    // Create reports with different scores
    for score in [10, 5, 0] {
        let mut req = make_request(&prefix, &format!("tc_{score}"));
        req.score = Some(score);
        req.event_id = format!("{prefix}_ev_tc_{score}");
        storage.create_report(req).await.expect("create_report should succeed");
    }

    let all = storage
        .get_reports_by_status("open", 20, None, None, None)
        .await
        .expect("get_reports_by_status should succeed");

    let ours: Vec<_> = all.iter().filter(|r| r.event_id.starts_with(&prefix)).collect();
    assert_eq!(ours.len(), 3, "should have 3 open reports");

    // Use triple cursor from the last item
    let last = ours.last().unwrap();
    let next = storage
        .get_reports_by_status("open", 20, Some(last.score), Some(last.received_ts), Some(last.id))
        .await
        .expect("triple cursor should succeed");

    let next_ours: Vec<_> = next.iter().filter(|r| r.event_id.starts_with(&prefix)).collect();
    assert!(next_ours.is_empty(), "triple cursor on last item should return empty");

    cleanup_all(&pool, &prefix).await;
}

// --- get_all_reports ---

#[tokio::test]
async fn test_get_all_reports_returns_entries() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string();
    let prefix = format!("gar_{suffix}");
    cleanup_all(&pool, &prefix).await;

    let storage = EventReportStorage::new(&pool);

    for i in 0..3 {
        let mut req = make_request(&prefix, &format!("all_{i}"));
        req.score = Some(i * 10);
        req.event_id = format!("{prefix}_ev_all_{i}");
        storage.create_report(req).await.expect("create_report should succeed");
    }

    let reports = storage.get_all_reports(20, None, None, None).await.expect("get_all_reports should succeed");

    let ours: Vec<_> = reports.iter().filter(|r| r.event_id.starts_with(&prefix)).collect();
    assert_eq!(ours.len(), 3, "should return all 3 reports");

    // Verify order: score DESC, received_ts DESC, id DESC
    for i in 1..ours.len() {
        assert!(
            ours[i - 1].score >= ours[i].score,
            "should sort by score DESC (at {i}: {} >= {})",
            ours[i - 1].score,
            ours[i].score
        );
    }

    cleanup_all(&pool, &prefix).await;
}

#[tokio::test]
async fn test_get_all_reports_triple_cursor_pagination() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string();
    let prefix = format!("gar_cur_{suffix}");
    cleanup_all(&pool, &prefix).await;

    let storage = EventReportStorage::new(&pool);

    for i in 0..4 {
        let mut req = make_request(&prefix, &format!("ac_{i}"));
        req.score = Some((4 - i) * 5);
        req.event_id = format!("{prefix}_ev_ac_{i}");
        storage.create_report(req).await.expect("create_report should succeed");
    }

    let page = storage.get_all_reports(2, None, None, None).await.expect("get_all_reports should succeed");

    // Our reports have score 20, 15, 10, 5 — first page should have top 2
    let ours_page1: Vec<_> = page.iter().filter(|r| r.event_id.starts_with(&prefix)).collect();
    // The limit is global, so it may include non-prefix reports
    assert!(!ours_page1.is_empty(), "should get at least one of our reports");

    // Use triple cursor for next page
    if let Some(last) = page.last() {
        let next = storage
            .get_all_reports(20, Some(last.score), Some(last.received_ts), Some(last.id))
            .await
            .expect("triple cursor pagination should succeed");

        // Should not contain items we already saw (based on cursor)
        for r in &next {
            if r.event_id.starts_with(&prefix) {
                assert!(
                    r.score < last.score
                        || (r.score == last.score && r.received_ts < last.received_ts)
                        || (r.score == last.score && r.received_ts == last.received_ts && r.id < last.id),
                    "next page items should be after the cursor"
                );
            }
        }
    }

    cleanup_all(&pool, &prefix).await;
}

// --- update_report ---

#[tokio::test]
async fn test_update_report_coalesce_preserves_fields() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string();
    let prefix = format!("upd_{suffix}");
    cleanup_all(&pool, &prefix).await;

    let storage = EventReportStorage::new(&pool);
    let mut req = make_request(&prefix, "upd");
    req.score = Some(5);
    let created = storage.create_report(req).await.expect("create_report should succeed");

    assert_eq!(created.status, "open");
    assert_eq!(created.score, 5);

    // Partial update: only change status, leave score as None
    let updated = storage
        .update_report(
            created.id,
            UpdateEventReportRequest {
                status: Some("resolved".to_string()),
                score: None,
                resolved_by: Some(format!("{prefix}_admin")),
                resolution_reason: None,
            },
        )
        .await
        .expect("update_report should succeed");

    assert_eq!(updated.status, "resolved", "status should be updated");
    assert_eq!(updated.score, 5, "score should be preserved via COALESCE");
    let expected_admin = format!("{prefix}_admin");
    assert_eq!(updated.resolved_by.as_deref(), Some(expected_admin.as_str()));
    assert!(updated.resolved_ts.is_some(), "resolved_ts should be set when status is resolved");
    assert_eq!(updated.resolution_reason, None, "resolution_reason should still be None");

    // Partial update: only change score
    let updated2 = storage
        .update_report(
            created.id,
            UpdateEventReportRequest {
                status: None,
                score: Some(99),
                resolved_by: None,
                resolution_reason: Some("Updated reason".to_string()),
            },
        )
        .await
        .expect("second update should succeed");

    assert_eq!(updated2.status, "resolved", "status should be preserved via COALESCE");
    assert_eq!(updated2.score, 99, "score should be updated");
    assert_eq!(updated2.resolution_reason.as_deref(), Some("Updated reason"));

    cleanup_all(&pool, &prefix).await;
}

// --- delete_report ---

#[tokio::test]
async fn test_delete_report_removes_record() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string();
    let prefix = format!("del_{suffix}");
    cleanup_all(&pool, &prefix).await;

    let storage = EventReportStorage::new(&pool);
    let created = storage.create_report(make_request(&prefix, "del")).await.expect("create_report should succeed");

    // Verify it exists
    assert!(storage.get_report(created.id).await.unwrap().is_some());

    // Delete it
    storage.delete_report(created.id).await.expect("delete_report should succeed");

    // Verify it is gone
    assert!(storage.get_report(created.id).await.unwrap().is_none(), "report should be gone after delete");

    // Deleting again should not error (no-op)
    storage.delete_report(created.id).await.expect("deleting non-existent report should succeed");

    cleanup_all(&pool, &prefix).await;
}

// --- check_rate_limit ---

#[tokio::test]
async fn test_check_rate_limit_new_user_allowed() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string();
    let user_id = format!("rl_new_{suffix}");
    cleanup_rate_limits(&pool, &user_id).await;

    let storage = EventReportStorage::new(&pool);
    let check = storage.check_rate_limit(&user_id).await.expect("check_rate_limit should succeed");

    assert!(check.is_allowed, "new user should be allowed");
    assert_eq!(check.remaining_reports, 50, "new user should have max remaining");
    assert!(check.block_reason.is_none(), "new user should have no block reason");

    cleanup_rate_limits(&pool, &user_id).await;
}

#[tokio::test]
async fn test_check_rate_limit_blocked_user() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string();
    let user_id = format!("rl_blocked_{suffix}");
    cleanup_all(&pool, &user_id).await;

    ensure_user(&pool, &user_id).await;
    let storage = EventReportStorage::new(&pool);
    let blocked_until = Utc::now().timestamp_millis() + 86_400_000; // 1 day from now
    storage.block_user_reports(&user_id, blocked_until, "Harassment").await.expect("block_user_reports should succeed");

    let check = storage.check_rate_limit(&user_id).await.expect("check_rate_limit should succeed");

    assert!(!check.is_allowed, "blocked user should not be allowed");
    assert_eq!(check.remaining_reports, 0, "blocked user should have 0 remaining");
    assert!(check.block_reason.is_some(), "blocked user should have a reason");

    cleanup_all(&pool, &user_id).await;
}

#[tokio::test]
async fn test_check_rate_limit_block_expired_auto_unblocks() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string();
    let user_id = format!("rl_expired_{suffix}");
    cleanup_all(&pool, &user_id).await;

    ensure_user(&pool, &user_id).await;
    let storage = EventReportStorage::new(&pool);
    let past = Utc::now().timestamp_millis() - 1000; // 1 second ago
    storage.block_user_reports(&user_id, past, "Old block").await.expect("block_user_reports should succeed");

    let check = storage.check_rate_limit(&user_id).await.expect("check_rate_limit should succeed");

    assert!(check.is_allowed, "expired block should auto-unblock");
    assert_eq!(check.remaining_reports, 50, "unblocked user should have max remaining");
    assert!(check.block_reason.is_none(), "unblocked user should have no block reason");

    cleanup_all(&pool, &user_id).await;
}

#[tokio::test]
async fn test_check_rate_limit_daily_limit_exceeded() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string();
    let user_id = format!("rl_daily_{suffix}");
    cleanup_all(&pool, &user_id).await;

    ensure_user(&pool, &user_id).await;
    let now = Utc::now().timestamp_millis();

    // Directly insert a row with 50 reports in the last 24h
    sqlx::query(
        "INSERT INTO report_rate_limits (user_id, report_count, last_report_at, created_ts, updated_ts) VALUES ($1, $2, $3, $4, $4)",
    )
    .bind(&user_id)
    .bind(50)
    .bind(now)
    .bind(now)
    .execute(&*pool)
    .await
    .expect("direct insert should succeed");

    let storage = EventReportStorage::new(&pool);
    let check = storage.check_rate_limit(&user_id).await.expect("check_rate_limit should succeed");

    assert!(!check.is_allowed, "user at daily limit should not be allowed");
    assert_eq!(check.remaining_reports, 0, "should have 0 remaining");
    assert_eq!(
        check.block_reason.as_deref(),
        Some("Daily report limit exceeded"),
        "should indicate daily limit exceeded"
    );

    cleanup_all(&pool, &user_id).await;
}

#[tokio::test]
async fn test_check_rate_limit_under_daily_limit() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string();
    let user_id = format!("rl_under_{suffix}");
    cleanup_all(&pool, &user_id).await;

    ensure_user(&pool, &user_id).await;
    let storage = EventReportStorage::new(&pool);
    // Record 10 reports
    for _ in 0..10 {
        storage.record_report(&user_id).await.expect("record_report should succeed");
    }

    let check = storage.check_rate_limit(&user_id).await.expect("check_rate_limit should succeed");

    assert!(check.is_allowed, "under limit user should be allowed");
    assert_eq!(check.remaining_reports, 40, "50 - 10 = 40 remaining");

    cleanup_all(&pool, &user_id).await;
}

// --- record_report ---

#[tokio::test]
async fn test_record_report_inserts_new_row() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string();
    let user_id = format!("rr_new_{suffix}");
    cleanup_all(&pool, &user_id).await;

    ensure_user(&pool, &user_id).await;
    let storage = EventReportStorage::new(&pool);
    storage.record_report(&user_id).await.expect("record_report should succeed");

    // Verify the row was inserted
    let row = sqlx::query_as::<_, ReportRateLimit>(
        "SELECT id, user_id, report_count, last_report_at, blocked_until_at, is_blocked, block_reason, created_ts, COALESCE(updated_ts, created_ts) AS updated_ts FROM report_rate_limits WHERE user_id = $1",
    )
    .bind(&user_id)
    .fetch_optional(&*pool)
    .await
    .expect("query should succeed")
    .expect("row should exist");

    assert_eq!(row.report_count, 1, "first report should set count to 1");
    assert!(!row.is_blocked, "should not be blocked");

    cleanup_all(&pool, &user_id).await;
}

#[tokio::test]
async fn test_record_report_increments_existing() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string();
    let user_id = format!("rr_incr_{suffix}");
    cleanup_all(&pool, &user_id).await;

    ensure_user(&pool, &user_id).await;
    let storage = EventReportStorage::new(&pool);

    // Record twice
    storage.record_report(&user_id).await.expect("first record_report should succeed");
    storage.record_report(&user_id).await.expect("second record_report should succeed");

    let row = sqlx::query_as::<_, ReportRateLimit>(
        "SELECT id, user_id, report_count, last_report_at, blocked_until_at, is_blocked, block_reason, created_ts, COALESCE(updated_ts, created_ts) AS updated_ts FROM report_rate_limits WHERE user_id = $1",
    )
    .bind(&user_id)
    .fetch_optional(&*pool)
    .await
    .expect("query should succeed")
    .expect("row should exist");

    assert_eq!(row.report_count, 2, "second report should increment count to 2");

    cleanup_all(&pool, &user_id).await;
}

// --- block_user_reports / unblock_user_reports ---

#[tokio::test]
async fn test_block_user_reports_upsert() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string();
    let user_id = format!("blk_{suffix}");
    cleanup_all(&pool, &user_id).await;

    ensure_user(&pool, &user_id).await;
    let storage = EventReportStorage::new(&pool);
    let until = Utc::now().timestamp_millis() + 86_400_000;

    // First block
    storage.block_user_reports(&user_id, until, "First reason").await.expect("first block should succeed");

    // Second block (upsert via ON CONFLICT)
    storage.block_user_reports(&user_id, until, "Second reason").await.expect("second block (upsert) should succeed");

    let row = sqlx::query_as::<_, ReportRateLimit>(
        "SELECT id, user_id, report_count, last_report_at, blocked_until_at, is_blocked, block_reason, created_ts, COALESCE(updated_ts, created_ts) AS updated_ts FROM report_rate_limits WHERE user_id = $1",
    )
    .bind(&user_id)
    .fetch_optional(&*pool)
    .await
    .expect("query should succeed")
    .expect("row should exist");

    assert!(row.is_blocked, "user should be blocked");
    assert_eq!(row.block_reason.as_deref(), Some("Second reason"), "upsert should update reason");

    cleanup_all(&pool, &user_id).await;
}

#[tokio::test]
async fn test_unblock_user_reports() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string();
    let user_id = format!("unblk_{suffix}");
    cleanup_all(&pool, &user_id).await;

    ensure_user(&pool, &user_id).await;
    let storage = EventReportStorage::new(&pool);
    let until = Utc::now().timestamp_millis() + 86_400_000;

    // Block first
    storage.block_user_reports(&user_id, until, "Test block").await.expect("block should succeed");

    assert!(!storage.check_rate_limit(&user_id).await.unwrap().is_allowed, "should be blocked");

    // Unblock
    storage.unblock_user_reports(&user_id).await.expect("unblock should succeed");

    assert!(storage.check_rate_limit(&user_id).await.unwrap().is_allowed, "should be unblocked after unblock");

    cleanup_all(&pool, &user_id).await;
}

#[tokio::test]
async fn test_unblock_user_reports_noop_on_nonexistent() {
    let pool = test_pool().await;
    let user_id = "unblk_nonexistent_noop_user";

    let storage = EventReportStorage::new(&pool);
    storage.unblock_user_reports(user_id).await.expect("unblock on non-existent user should not error");
}

// --- count_reports_by_status ---

#[tokio::test]
async fn test_count_reports_by_status_returns_count() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string();
    let prefix = format!("cnt_{suffix}");
    cleanup_all(&pool, &prefix).await;

    let storage = EventReportStorage::new(&pool);

    // Create 2 open reports and verify they exist individually
    let mut req1 = make_request(&prefix, "cnt_a");
    req1.event_id = format!("{prefix}_ev_cnt_a");
    let r1 = storage.create_report(req1).await.expect("create_report should succeed");
    assert_eq!(r1.status, "open");

    let mut req2 = make_request(&prefix, "cnt_b");
    req2.event_id = format!("{prefix}_ev_cnt_b");
    let r2 = storage.create_report(req2).await.expect("create_report should succeed");
    assert_eq!(r2.status, "open");

    // Verify the count method at least runs and returns a number
    let open_count = storage.count_reports_by_status("open").await.unwrap();
    assert!(open_count >= 2, "should have at least our 2 open reports, got {open_count}");

    // Resolve one and verify individual status changed
    let report = storage.create_report(make_request(&prefix, "cnt_c")).await.expect("create_report should succeed");
    storage
        .update_report(
            report.id,
            UpdateEventReportRequest { status: Some("resolved".to_string()), ..Default::default() },
        )
        .await
        .expect("update_report should succeed");

    let updated = storage.get_report(report.id).await.expect("get_report should succeed");
    assert_eq!(updated.unwrap().status, "resolved");

    cleanup_all(&pool, &prefix).await;
}

// --- count_all_reports ---

#[tokio::test]
async fn test_count_all_reports_is_global() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string();
    let prefix = format!("car_{suffix}");
    cleanup_all(&pool, &prefix).await;

    let storage = EventReportStorage::new(&pool);

    let before = storage.count_all_reports().await.expect("count_all_reports should succeed");

    // Create 3 reports
    for i in 0..3 {
        let mut req = make_request(&prefix, &format!("car_{i}"));
        req.event_id = format!("{prefix}_ev_car_{i}");
        storage.create_report(req).await.expect("create_report should succeed");
    }

    let after = storage.count_all_reports().await.expect("count_all_reports should succeed");

    assert!(after >= before + 3, "global count should increase by at least 3 (before={before}, after={after})");

    cleanup_all(&pool, &prefix).await;
}
