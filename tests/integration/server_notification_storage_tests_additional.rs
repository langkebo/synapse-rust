//! Additional integration tests for `ServerNotificationStorage` covering all
//! public methods in `synapse-storage/src/server_notification.rs`:
//!   - Notification CRUD: create / get / update / delete / deactivate
//!   - Listing: `list_active_notifications` (time-window filtering),
//!     `list_all_notifications` (audience filter + cursor pagination)
//!   - Per-user status: `get_or_create_status`,
//!     `get_or_create_statuses_batch` (incl. empty-input edge case),
//!     `mark_as_read` / `mark_as_dismissed` (incl. not-found errors),
//!     `mark_all_as_read`, `get_user_notifications` (all + specific audience)
//!   - Templates: create / get / list / delete (soft)
//!   - Delivery log: `log_delivery`
//!   - Scheduling: `schedule_notification` / `get_pending_scheduled_notifications`
//!     / `mark_scheduled_sent`
//!   - User notification settings: get (None) / upsert / get (Some)
//!   - Pushers: `get_user_pushers` / `delete_user_pusher`
//!   - Server notices: count / paginated / get_by_id / get_with_room / delete
//!   - Cascade helpers: `delete_room_cascade` / `delete_event_by_id`
//!   - Full transactional flow: `send_server_notice`

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use synapse_storage::server_notification::{
    CreateNotificationRequest, CreateTemplateRequest, ServerNotificationCursor,
    ServerNotificationStorage,
};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn sn_test_guard() -> &'static Mutex<()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD.get_or_init(|| Mutex::new(()))
}

/// Warm up the shared pool on the current tokio runtime (cross-runtime guard).
async fn warm_up_pool(pool: &Arc<sqlx::PgPool>) {
    for _ in 0..8 {
        match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            sqlx::query("SELECT 1").execute(pool.as_ref()),
        )
        .await
        {
            Ok(Ok(_)) => return,
            Ok(Err(_)) | Err(_) => {
                tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            }
        }
    }
    let _ = sqlx::query("SELECT 1").execute(pool.as_ref()).await;
}

fn new_storage(pool: &Arc<sqlx::PgPool>) -> ServerNotificationStorage {
    ServerNotificationStorage::new(pool)
}

fn unique_user() -> String {
    format!("@sntest_{}:localhost", unique_id())
}

fn unique_room() -> String {
    format!("!sntest_{}:localhost", unique_id())
}

/// Ensure a user row exists in `users` (needed because several notification
/// tables have FK constraints on `user_id`).
async fn ensure_user(pool: &sqlx::PgPool, user_id: &str) {
    let username = user_id.trim_start_matches('@').replace(':', "_");
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query("INSERT INTO users (user_id, username, creation_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING")
        .bind(user_id)
        .bind(username)
        .bind(now)
        .execute(pool)
        .await
        .ok();
}

/// Minimal notification request with unique title/content.
fn make_request(suffix: &str) -> CreateNotificationRequest {
    CreateNotificationRequest {
        title: format!("title-{suffix}"),
        content: format!("content-{suffix}"),
        notification_type: None,
        priority: None,
        target_audience: None,
        target_user_ids: None,
        starts_at: None,
        expires_at: None,
        is_dismissable: None,
        action_url: None,
        action_text: None,
        created_by: None,
    }
}

async fn setup(pool: &Arc<sqlx::PgPool>) {
    warm_up_pool(pool).await;
    clean_notification_tables(pool.as_ref()).await;
}

async fn teardown(pool: &sqlx::PgPool) {
    clean_notification_tables(pool).await;
}

/// Clean every table touched by these tests. Shared tables (events/rooms/etc.)
/// are scoped to the `sntest_` prefix so concurrent test modules are not
/// disturbed.
async fn clean_notification_tables(pool: &sqlx::PgPool) {
    // Children first (FK ordering).
    sqlx::query("DELETE FROM notification_delivery_log").execute(pool).await.ok();
    sqlx::query("DELETE FROM scheduled_notifications").execute(pool).await.ok();
    sqlx::query("DELETE FROM user_notification_status").execute(pool).await.ok();
    sqlx::query("DELETE FROM user_notification_settings").execute(pool).await.ok();
    sqlx::query("DELETE FROM server_notices").execute(pool).await.ok();
    sqlx::query("DELETE FROM server_notifications").execute(pool).await.ok();
    sqlx::query("DELETE FROM notification_templates").execute(pool).await.ok();
    sqlx::query("DELETE FROM pushers").execute(pool).await.ok();
    // Scoped shared-table cleanup for send_server_notice / delete_room_cascade.
    sqlx::query("DELETE FROM room_summary_members WHERE room_id LIKE '!sntest_%'").execute(pool).await.ok();
    sqlx::query("DELETE FROM room_summaries WHERE room_id LIKE '!sntest_%'").execute(pool).await.ok();
    sqlx::query("DELETE FROM room_memberships WHERE room_id LIKE '!sntest_%'").execute(pool).await.ok();
    sqlx::query("DELETE FROM events WHERE room_id LIKE '!sntest_%' OR event_id LIKE '\\$sntest_%'")
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM rooms WHERE room_id LIKE '!sntest_%'").execute(pool).await.ok();
    // Test users (FK dependents already cleaned above).
    sqlx::query("DELETE FROM users WHERE user_id LIKE '@sntest_%:localhost'").execute(pool).await.ok();
}

// ---------------------------------------------------------------------------
// Notification CRUD
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_notification_applies_defaults() {
    let _guard = sn_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let created = storage.create_notification(make_request(&format!("dflt-{uid}"))).await.unwrap();
    assert_eq!(created.title, format!("title-dflt-{uid}"));
    assert_eq!(created.notification_type, "info");
    assert_eq!(created.priority, 0);
    assert_eq!(created.target_audience, "all");
    assert!(created.is_enabled);
    assert!(created.is_dismissable);
    assert_eq!(created.target_user_ids, serde_json::json!([]));
    assert!(created.starts_at.is_none());
    assert!(created.expires_at.is_none());
    assert!(created.action_url.is_none());
    assert!(created.created_by.is_none());
    assert!(created.created_ts > 0);
    assert_eq!(created.created_ts, created.updated_ts);

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_notification_with_all_fields() {
    let _guard = sn_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let now = chrono::Utc::now().timestamp_millis();
    let request = CreateNotificationRequest {
        title: format!("T-{uid}"),
        content: format!("C-{uid}"),
        notification_type: Some("warning".to_string()),
        priority: Some(2),
        target_audience: Some("specific".to_string()),
        target_user_ids: Some(vec!["@bob:localhost".to_string()]),
        starts_at: Some(now - 1000),
        expires_at: Some(now + 86_400_000),
        is_dismissable: Some(false),
        action_url: Some("https://example.com".to_string()),
        action_text: Some("Open".to_string()),
        created_by: Some("@admin:localhost".to_string()),
    };
    let created = storage.create_notification(request).await.unwrap();
    assert_eq!(created.notification_type, "warning");
    assert_eq!(created.priority, 2);
    assert_eq!(created.target_audience, "specific");
    assert_eq!(created.target_user_ids, serde_json::json!(["@bob:localhost"]));
    assert!(!created.is_dismissable);
    assert_eq!(created.action_url.as_deref(), Some("https://example.com"));
    assert_eq!(created.action_text.as_deref(), Some("Open"));
    assert_eq!(created.created_by.as_deref(), Some("@admin:localhost"));
    assert!(created.starts_at.is_some());
    assert!(created.expires_at.is_some());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_notification_found_and_not_found() {
    let _guard = sn_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let created = storage.create_notification(make_request("get")).await.unwrap();
    let fetched = storage.get_notification(created.id).await.unwrap().unwrap();
    assert_eq!(fetched.id, created.id);
    assert_eq!(fetched.title, created.title);

    let missing = storage.get_notification(9_999_999_999).await.unwrap();
    assert!(missing.is_none());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_update_notification_overwrites_fields() {
    let _guard = sn_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let created = storage.create_notification(make_request("upd")).await.unwrap();
    let update = CreateNotificationRequest {
        title: "Updated Title".to_string(),
        content: "Updated Content".to_string(),
        notification_type: Some("error".to_string()),
        priority: Some(1),
        target_audience: Some("admins".to_string()),
        target_user_ids: Some(vec!["@x:localhost".to_string()]),
        starts_at: None,
        expires_at: None,
        is_dismissable: Some(false),
        action_url: Some("https://a.b".to_string()),
        action_text: Some("Go".to_string()),
        created_by: None,
    };
    let updated = storage.update_notification(created.id, update).await.unwrap();
    assert_eq!(updated.title, "Updated Title");
    assert_eq!(updated.content, "Updated Content");
    assert_eq!(updated.notification_type, "error");
    assert_eq!(updated.priority, 1);
    assert_eq!(updated.target_audience, "admins");
    assert_eq!(updated.target_user_ids, serde_json::json!(["@x:localhost"]));
    assert!(!updated.is_dismissable);
    // updated_ts advances.
    assert!(updated.updated_ts >= created.updated_ts);

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_delete_notification_returns_bool() {
    let _guard = sn_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let created = storage.create_notification(make_request("del")).await.unwrap();
    let ok = storage.delete_notification(created.id).await.unwrap();
    assert!(ok);
    // Second delete returns false (already gone).
    let again = storage.delete_notification(created.id).await.unwrap();
    assert!(!again);
    assert!(storage.get_notification(created.id).await.unwrap().is_none());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_deactivate_notification_disables_and_excludes_from_active() {
    let _guard = sn_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let created = storage.create_notification(make_request("deact")).await.unwrap();
    assert!(created.is_enabled);

    let ok = storage.deactivate_notification(created.id).await.unwrap();
    assert!(ok);

    let fetched = storage.get_notification(created.id).await.unwrap().unwrap();
    assert!(!fetched.is_enabled);

    // Disabled notifications are excluded from list_active.
    let active = storage.list_active_notifications().await.unwrap();
    assert!(active.iter().all(|n| n.id != created.id));

    // Deactivating a nonexistent id returns false.
    let missing = storage.deactivate_notification(9_999_999_999).await.unwrap();
    assert!(!missing);

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// list_active_notifications (time-window filtering)
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_list_active_notifications_filters_by_time_window() {
    let _guard = sn_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let now = chrono::Utc::now().timestamp_millis();

    // 1) Active (no window) — included.
    let active = storage.create_notification(make_request("act-1")).await.unwrap();

    // 2) Expired (expires_at in the past) — excluded.
    let mut expired_req = make_request("act-2");
    expired_req.expires_at = Some(now - 1000);
    let expired = storage.create_notification(expired_req).await.unwrap();

    // 3) Future-start (starts_at in the future) — excluded.
    let mut future_req = make_request("act-3");
    future_req.starts_at = Some(now + 86_400_000);
    let future = storage.create_notification(future_req).await.unwrap();

    // 4) Currently within window — included.
    let mut window_req = make_request("act-4");
    window_req.starts_at = Some(now - 1000);
    window_req.expires_at = Some(now + 86_400_000);
    let window = storage.create_notification(window_req).await.unwrap();

    let list = storage.list_active_notifications().await.unwrap();
    let ids: Vec<i64> = list.iter().map(|n| n.id).collect();
    assert!(ids.contains(&active.id), "active notification should be listed");
    assert!(ids.contains(&window.id), "in-window notification should be listed");
    assert!(!ids.contains(&expired.id), "expired notification should be excluded");
    assert!(!ids.contains(&future.id), "future-start notification should be excluded");
    assert_eq!(list.len(), 2, "expected exactly two active notifications");

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// list_all_notifications (audience filter + cursor pagination)
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_list_all_notifications_audience_filter() {
    let _guard = sn_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let mut a = make_request("aud-all");
    a.target_audience = Some("all".to_string());
    let mut b = make_request("aud-admins");
    b.target_audience = Some("admins".to_string());
    storage.create_notification(a).await.unwrap();
    let admins_n = storage.create_notification(b).await.unwrap();

    // No audience filter -> returns both.
    let (all, next) = storage.list_all_notifications(None, 10, None).await.unwrap();
    assert!(next.is_none());
    assert_eq!(all.len(), 2);

    // Filter by "admins" -> only the admins notification.
    let (filtered, _) = storage.list_all_notifications(Some("admins"), 10, None).await.unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, admins_n.id);

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_list_all_notifications_cursor_pagination() {
    let _guard = sn_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    // Create 3 notifications; ordering is (created_ts DESC, id DESC).
    let mut ids = Vec::new();
    for i in 0..3 {
        let mut req = make_request(&format!("page-{i}"));
        req.priority = Some(i as i32);
        let n = storage.create_notification(req).await.unwrap();
        ids.push(n.id);
    }

    // Page 1: limit 2.
    let (page1, next1) = storage.list_all_notifications(None, 2, None).await.unwrap();
    assert_eq!(page1.len(), 2);
    assert!(next1.is_some(), "should have a next cursor");

    // Decode the cursor and fetch page 2.
    let cursor = decode(next1.as_deref());
    let (page2, next2) = storage.list_all_notifications(None, 2, cursor).await.unwrap();
    assert_eq!(page2.len(), 1);
    assert!(next2.is_none(), "no more pages");

    // Combined results equal all 3 ids, no duplicates.
    let mut collected: Vec<i64> = page1.iter().map(|n| n.id).collect();
    collected.extend(page2.iter().map(|n| n.id));
    collected.sort_unstable();
    ids.sort_unstable();
    assert_eq!(collected, ids);

    teardown(pool.as_ref()).await;
}

fn decode(cursor: Option<&str>) -> Option<ServerNotificationCursor> {
    synapse_storage::server_notification::decode_server_notification_cursor(cursor)
}

// ---------------------------------------------------------------------------
// get_or_create_status / get_or_create_statuses_batch
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_or_create_status_creates_then_is_idempotent() {
    let _guard = sn_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user = unique_user();
    ensure_user(pool.as_ref(), &user).await;
    let n = storage.create_notification(make_request("st-1")).await.unwrap();

    let status = storage.get_or_create_status(&user, n.id).await.unwrap();
    assert_eq!(status.user_id, user);
    assert_eq!(status.notification_id, n.id);
    assert!(!status.is_read);
    assert!(!status.is_dismissed);
    let first_id = status.id;

    // Calling again must return the same row (idempotent).
    let again = storage.get_or_create_status(&user, n.id).await.unwrap();
    assert_eq!(again.id, first_id);

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_or_create_statuses_batch_empty_and_populated() {
    let _guard = sn_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user = unique_user();
    ensure_user(pool.as_ref(), &user).await;

    // Empty input -> empty map (no DB round-trip needed).
    let empty = storage.get_or_create_statuses_batch(&user, &[]).await.unwrap();
    assert!(empty.is_empty());

    let n1 = storage.create_notification(make_request("b-1")).await.unwrap();
    let n2 = storage.create_notification(make_request("b-2")).await.unwrap();

    let map = storage.get_or_create_statuses_batch(&user, &[n1.id, n2.id]).await.unwrap();
    assert_eq!(map.len(), 2);
    assert!(map.contains_key(&n1.id));
    assert!(map.contains_key(&n2.id));

    // Calling again with the same ids stays idempotent.
    let again = storage.get_or_create_statuses_batch(&user, &[n1.id, n2.id]).await.unwrap();
    assert_eq!(again.len(), 2);

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// mark_as_read / mark_as_dismissed (incl. not-found errors)
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_mark_as_read_not_found_returns_error() {
    let _guard = sn_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user = unique_user();
    ensure_user(pool.as_ref(), &user).await;

    let result = storage.mark_as_read(&user, 9_999_999_999).await;
    assert!(result.is_err(), "mark_as_read on a nonexistent notification must error");

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_mark_as_read_success_sets_read_flag() {
    let _guard = sn_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user = unique_user();
    ensure_user(pool.as_ref(), &user).await;
    let n = storage.create_notification(make_request("read-ok")).await.unwrap();

    let ok = storage.mark_as_read(&user, n.id).await.unwrap();
    assert!(ok);

    let status = storage.get_or_create_status(&user, n.id).await.unwrap();
    assert!(status.is_read);
    assert!(status.read_ts.is_some());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_mark_as_dismissed_not_found_and_success() {
    let _guard = sn_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user = unique_user();
    ensure_user(pool.as_ref(), &user).await;

    // Not found.
    let err = storage.mark_as_dismissed(&user, 9_999_999_999).await;
    assert!(err.is_err());

    let n = storage.create_notification(make_request("dismiss-ok")).await.unwrap();
    let ok = storage.mark_as_dismissed(&user, n.id).await.unwrap();
    assert!(ok);

    let status = storage.get_or_create_status(&user, n.id).await.unwrap();
    assert!(status.is_dismissed);
    assert!(status.dismissed_ts.is_some());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_mark_all_as_read_returns_count() {
    let _guard = sn_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user = unique_user();
    ensure_user(pool.as_ref(), &user).await;

    // Two notifications targeting "all" audience so get_user_notifications sees them.
    let mut r1 = make_request("all-1");
    r1.target_audience = Some("all".to_string());
    let mut r2 = make_request("all-2");
    r2.target_audience = Some("all".to_string());
    let n1 = storage.create_notification(r1).await.unwrap();
    let n2 = storage.create_notification(r2).await.unwrap();

    let count = storage.mark_all_as_read(&user).await.unwrap();
    assert_eq!(count, 2, "both notifications should be marked as read");

    // Verify both statuses are now read.
    let s1 = storage.get_or_create_status(&user, n1.id).await.unwrap();
    let s2 = storage.get_or_create_status(&user, n2.id).await.unwrap();
    assert!(s1.is_read);
    assert!(s2.is_read);

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// get_user_notifications (all + specific audience)
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_user_notifications_all_audience_includes_status() {
    let _guard = sn_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user = unique_user();
    ensure_user(pool.as_ref(), &user).await;

    let mut req = make_request("u-all");
    req.target_audience = Some("all".to_string());
    let n = storage.create_notification(req).await.unwrap();

    let list = storage.get_user_notifications(&user).await.unwrap();
    let found = list.iter().find(|wn| wn.notification.id == n.id);
    assert!(found.is_some(), "all-audience notification should be visible");
    let found = found.unwrap();
    assert!(!found.is_read);
    assert!(!found.is_dismissed);

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_user_notifications_specific_audience_filters_by_user() {
    let _guard = sn_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let included = unique_user();
    let excluded = unique_user();
    ensure_user(pool.as_ref(), &included).await;
    ensure_user(pool.as_ref(), &excluded).await;

    let mut req = make_request("u-spec");
    req.target_audience = Some("specific".to_string());
    req.target_user_ids = Some(vec![included.clone()]);
    let n = storage.create_notification(req).await.unwrap();

    // The targeted user sees it.
    let for_included = storage.get_user_notifications(&included).await.unwrap();
    assert!(for_included.iter().any(|wn| wn.notification.id == n.id));

    // A different user does not.
    let for_excluded = storage.get_user_notifications(&excluded).await.unwrap();
    assert!(!for_excluded.iter().any(|wn| wn.notification.id == n.id));

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// Templates
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_template_create_get_list_delete() {
    let _guard = sn_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let req = CreateTemplateRequest {
        name: format!("welcome-{uid}"),
        title_template: "Welcome {username}".to_string(),
        content_template: "Hello {username}".to_string(),
        notification_type: Some("info".to_string()),
        variables: Some(vec!["username".to_string()]),
    };
    let created = storage.create_template(req).await.unwrap();
    assert!(created.is_enabled);
    assert_eq!(created.variables, serde_json::json!(["username"]));

    // get by name.
    let fetched = storage.get_template(&created.name).await.unwrap().unwrap();
    assert_eq!(fetched.id, created.id);

    // list includes it.
    let list = storage.list_templates().await.unwrap();
    assert!(list.iter().any(|t| t.id == created.id));

    // delete (soft) -> get returns None (get_template filters is_enabled = TRUE).
    let ok = storage.delete_template(&created.name).await.unwrap();
    assert!(ok);
    let after = storage.get_template(&created.name).await.unwrap();
    assert!(after.is_none(), "soft-deleted template should not be returned");

    // The row still exists in the table (soft-deleted, not removed), so a
    // second call still matches the `WHERE name = $1` clause and reports a
    // row affected. This documents the actual `delete_template` behavior.
    let again = storage.delete_template(&created.name).await.unwrap();
    assert!(again, "row still exists, so a second soft-delete still matches");

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// log_delivery
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_log_delivery_inserts_rows() {
    let _guard = sn_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user = unique_user();
    ensure_user(pool.as_ref(), &user).await;
    let n = storage.create_notification(make_request("dlv")).await.unwrap();

    // Success delivery log (no error message).
    storage.log_delivery(n.id, Some(&user), "push", "sent", None).await.unwrap();
    // Failed delivery log (with error message).
    storage.log_delivery(n.id, None, "email", "failed", Some("timeout")).await.unwrap();

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*)::BIGINT FROM notification_delivery_log WHERE notification_id = $1")
        .bind(n.id)
        .fetch_one(pool.as_ref())
        .await
        .unwrap();
    assert_eq!(count, 2);

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// Scheduling
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_schedule_notification_lifecycle() {
    let _guard = sn_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let n = storage.create_notification(make_request("sch")).await.unwrap();
    let now = chrono::Utc::now().timestamp_millis();

    // Schedule one for the future (should NOT be pending yet).
    let future = storage.schedule_notification(n.id, now + 86_400_000).await.unwrap();
    assert!(!future.is_sent);
    assert!(future.sent_ts.is_none());

    // Schedule one in the past (should appear in pending).
    let past = storage.schedule_notification(n.id, now - 1000).await.unwrap();

    let pending = storage.get_pending_scheduled_notifications().await.unwrap();
    assert!(pending.iter().any(|s| s.id == past.id));
    assert!(!pending.iter().any(|s| s.id == future.id));

    // Mark the past one as sent.
    let ok = storage.mark_scheduled_sent(past.id).await.unwrap();
    assert!(ok);
    let still_pending = storage.get_pending_scheduled_notifications().await.unwrap();
    assert!(!still_pending.iter().any(|s| s.id == past.id));

    // Marking a nonexistent id returns false.
    let missing = storage.mark_scheduled_sent(9_999_999_999).await.unwrap();
    assert!(!missing);

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// User notification settings
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_user_notification_setting_get_none_then_upsert() {
    let _guard = sn_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user = unique_user();
    ensure_user(pool.as_ref(), &user).await;

    // No setting yet -> None.
    let before = storage.get_user_notification_setting(&user).await.unwrap();
    assert!(before.is_none());

    // Upsert to false.
    storage.upsert_user_notification_setting(&user, false).await.unwrap();
    let after = storage.get_user_notification_setting(&user).await.unwrap().unwrap();
    assert!(!after);

    // Upsert to true (update path).
    storage.upsert_user_notification_setting(&user, true).await.unwrap();
    let toggled = storage.get_user_notification_setting(&user).await.unwrap().unwrap();
    assert!(toggled);

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// Pushers
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_user_pushers_empty_and_with_rows() {
    let _guard = sn_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user = unique_user();
    let now = chrono::Utc::now().timestamp_millis();

    // Initially empty.
    let empty = storage.get_user_pushers(&user).await.unwrap();
    assert!(empty.is_empty());

    // Insert a pusher directly (pushers has no FK on user_id).
    sqlx::query(
        r#"INSERT INTO pushers (
            user_id, device_id, pushkey, pushkey_ts, kind, app_id,
            app_display_name, device_display_name, lang, data, created_ts, is_enabled
        ) VALUES ($1, $2, $3, $4, 'http', 'app.example', 'App', 'Phone', 'en', '{}'::jsonb, $4, TRUE)"#,
    )
    .bind(&user)
    .bind("DEV1")
    .bind(format!("pk-{now}"))
    .bind(now)
    .execute(pool.as_ref())
    .await
    .unwrap();

    let list = storage.get_user_pushers(&user).await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["app_id"].as_str(), Some("app.example"));

    // Delete it.
    let deleted = storage.delete_user_pusher(&user, &format!("pk-{now}")).await.unwrap();
    assert!(deleted);
    let after = storage.get_user_pushers(&user).await.unwrap();
    assert!(after.is_empty());

    // Deleting again returns false.
    let again = storage.delete_user_pusher(&user, &format!("pk-{now}")).await.unwrap();
    assert!(!again);

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// Server notices
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_server_notices_count_paginated_get_and_delete() {
    let _guard = sn_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let now = chrono::Utc::now().timestamp_millis();

    // Insert 3 notices directly (user_id NULL to avoid FK).
    let mut ids = Vec::new();
    for i in 0..3 {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO server_notices (user_id, event_id, content, sent_ts) VALUES (NULL, $1, $2, $3) RETURNING id",
        )
        .bind(format!("$sntest_notice_{i}_{now}"))
        .bind(format!("body-{i}"))
        .bind(now - i * 1000)
        .fetch_one(pool.as_ref())
        .await
        .unwrap();
        ids.push(id);
    }

    // Count.
    let count = storage.get_server_notices_count().await.unwrap();
    assert_eq!(count, 3);

    // Paginated (limit 2) -> first page + next cursor.
    let (page1, total1, next1) = storage.get_server_notices_paginated(None, 2).await.unwrap();
    assert_eq!(total1, 3);
    assert_eq!(page1.len(), 2);
    assert!(next1.is_some());

    // Second page using the cursor.
    let (ts, id) = parse_cursor(next1.as_deref().unwrap());
    let (page2, total2, next2) = storage.get_server_notices_paginated(Some((ts, id)), 2).await.unwrap();
    assert_eq!(total2, 3);
    assert_eq!(page2.len(), 1);
    assert!(next2.is_none());

    // get by id.
    let fetched = storage.get_server_notice_by_id(ids[0]).await.unwrap().unwrap();
    assert_eq!(fetched["id"].as_i64(), Some(ids[0]));

    // get_server_notice_with_room: event does not exist -> room_id is null.
    let with_room = storage.get_server_notice_with_room(ids[0]).await.unwrap().unwrap();
    let (event_id_opt, room_id_opt) = with_room;
    assert!(event_id_opt.is_some());
    assert!(room_id_opt.is_none(), "event not in events table -> room_id should be None");

    // get_server_notice_with_room for a nonexistent notice -> None.
    let none = storage.get_server_notice_with_room(9_999_999_999).await.unwrap();
    assert!(none.is_none());

    // Delete one.
    let ok = storage.delete_server_notice_by_id(ids[0]).await.unwrap();
    assert!(ok);
    let after = storage.get_server_notice_by_id(ids[0]).await.unwrap();
    assert!(after.is_none());
    let count_after = storage.get_server_notices_count().await.unwrap();
    assert_eq!(count_after, 2);

    teardown(pool.as_ref()).await;
}

fn parse_cursor(cursor: &str) -> (i64, i64) {
    let (a, b) = cursor.split_once('|').unwrap();
    (a.parse().unwrap(), b.parse().unwrap())
}

// ---------------------------------------------------------------------------
// delete_room_cascade / delete_event_by_id
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_delete_room_cascade_removes_room_and_events() {
    let _guard = sn_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let room = unique_room();
    let now = chrono::Utc::now().timestamp_millis();

    // Insert a room, an event and a membership directly.
    sqlx::query("INSERT INTO rooms (room_id, name, topic, creator, is_public, join_rules, room_version, history_visibility, created_ts, last_activity_ts) VALUES ($1, 'n', 't', 'c', false, 'private', '6', 'joined', $2, $2) ON CONFLICT (room_id) DO NOTHING")
        .bind(&room)
        .bind(now)
        .execute(pool.as_ref())
        .await
        .unwrap();

    let event_id = format!("$sntest_drc_{}:localhost", unique_id());
    sqlx::query("INSERT INTO events (event_id, room_id, user_id, event_type, content, origin_server_ts, sender) VALUES ($1, $2, 'c', 'm.room.message', '{}'::jsonb, $3, 'c') ON CONFLICT (event_id) DO NOTHING")
        .bind(&event_id)
        .bind(&room)
        .bind(now)
        .execute(pool.as_ref())
        .await
        .unwrap();

    sqlx::query("INSERT INTO room_memberships (room_id, user_id, sender, membership, event_id, event_type, updated_ts, joined_ts) VALUES ($1, 'c', 'c', 'join', $2, 'm.room.member', $3, $3) ON CONFLICT (room_id, user_id) DO NOTHING")
        .bind(&room)
        .bind(&event_id)
        .bind(now)
        .execute(pool.as_ref())
        .await
        .unwrap();

    storage.delete_room_cascade(&room).await.unwrap();

    let room_count: i64 = sqlx::query_scalar("SELECT COUNT(*)::BIGINT FROM rooms WHERE room_id = $1")
        .bind(&room)
        .fetch_one(pool.as_ref())
        .await
        .unwrap();
    assert_eq!(room_count, 0);

    let event_count: i64 = sqlx::query_scalar("SELECT COUNT(*)::BIGINT FROM events WHERE room_id = $1")
        .bind(&room)
        .fetch_one(pool.as_ref())
        .await
        .unwrap();
    assert_eq!(event_count, 0);

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_delete_event_by_id_removes_row() {
    let _guard = sn_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let room = unique_room();
    let now = chrono::Utc::now().timestamp_millis();
    let event_id = format!("$sntest_deb_{}:localhost", unique_id());

    sqlx::query("INSERT INTO rooms (room_id, name, topic, creator, is_public, join_rules, room_version, history_visibility, created_ts, last_activity_ts) VALUES ($1, 'n', 't', 'c', false, 'private', '6', 'joined', $2, $2) ON CONFLICT (room_id) DO NOTHING")
        .bind(&room)
        .bind(now)
        .execute(pool.as_ref())
        .await
        .unwrap();
    sqlx::query("INSERT INTO events (event_id, room_id, user_id, event_type, content, origin_server_ts, sender) VALUES ($1, $2, 'c', 'm.room.message', '{}'::jsonb, $3, 'c')")
        .bind(&event_id)
        .bind(&room)
        .bind(now)
        .execute(pool.as_ref())
        .await
        .unwrap();

    storage.delete_event_by_id(&event_id).await.unwrap();

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*)::BIGINT FROM events WHERE event_id = $1")
        .bind(&event_id)
        .fetch_one(pool.as_ref())
        .await
        .unwrap();
    assert_eq!(count, 0);

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// send_server_notice (full transactional flow)
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_send_server_notice_creates_notice_and_room() {
    let _guard = sn_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let server_user = unique_user();
    let target_user = unique_user();
    let room = unique_room();
    ensure_user(pool.as_ref(), &server_user).await;
    ensure_user(pool.as_ref(), &target_user).await;

    let now = chrono::Utc::now().timestamp_millis();
    let message_event_id = format!("$sntest_msg_{uid}:localhost");
    let create_event_id = format!("$sntest_create_{uid}:localhost");
    let membership_event_id = format!("$sntest_member_{uid}:localhost");

    let notice_id = storage
        .send_server_notice(
            &room,
            &server_user,
            &target_user,
            &Some("Alice".to_string()),
            &Some("mxc://localhost/avatar".to_string()),
            &message_event_id,
            &create_event_id,
            &membership_event_id,
            "m.text",
            "Hello from server",
            now,
        )
        .await
        .unwrap();
    assert!(notice_id > 0);

    // The server_notices row was created and is retrievable.
    let fetched = storage.get_server_notice_by_id(notice_id).await.unwrap().unwrap();
    assert_eq!(fetched["user_id"].as_str(), Some(target_user.as_str()));
    assert_eq!(fetched["event_id"].as_str(), Some(message_event_id.as_str()));

    // get_server_notice_with_room now resolves a room_id (event exists).
    let with_room = storage.get_server_notice_with_room(notice_id).await.unwrap().unwrap();
    let (_event_id_opt, room_id_opt) = with_room;
    assert_eq!(room_id_opt.as_deref(), Some(room.as_str()));

    // The target user is a joined member of the notice room.
    let membership_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*)::BIGINT FROM room_memberships WHERE room_id = $1 AND user_id = $2 AND membership = 'join'")
            .bind(&room)
            .bind(&target_user)
            .fetch_one(pool.as_ref())
            .await
            .unwrap();
    assert_eq!(membership_count, 1);

    teardown(pool.as_ref()).await;
}
