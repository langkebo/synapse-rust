//! DB 集成测试（直连 TEST_DATABASE_URL 的 public schema，uuid 后缀 + 用例后清理）。
//! server_notifications 表无 FK 到 users；user_notification_status 有 FK 到
//! users(user_id) 与 server_notifications(id)（ON DELETE CASCADE），故 mark_* 类
//! 测试需先 ensure_test_user。

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use sqlx::postgres::PgPoolOptions;
use std::env;
use std::sync::Arc;

async fn test_pool() -> Arc<sqlx::PgPool> {
    let db_url = env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse_test".to_string());
    let pool =
        PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
    Arc::new(pool)
}

fn make_suffix() -> String {
    uuid::Uuid::new_v4().to_string().replace('-', "")
}

fn make_request(title: &str) -> CreateNotificationRequest {
    CreateNotificationRequest {
        title: title.to_string(),
        content: "Content".to_string(),
        notification_type: Some("info".to_string()),
        priority: Some(1),
        target_audience: Some("all".to_string()),
        target_user_ids: None,
        starts_at: None,
        expires_at: None,
        is_dismissable: Some(true),
        action_url: None,
        action_text: None,
        created_by: Some("@admin:test".to_string()),
    }
}

async fn ensure_test_user(pool: &sqlx::PgPool, user_id: &str) {
    let username = user_id.strip_prefix('@').and_then(|u| u.split(':').next()).unwrap_or("testuser");
    sqlx::query(
        "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, EXTRACT(EPOCH FROM NOW()) * 1000) ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(user_id)
    .bind(username)
    .execute(pool)
    .await
    .ok();
}

#[tokio::test]
async fn create_notification_then_get() {
    let pool = test_pool().await;
    let storage = ServerNotificationStorage::new(&pool);
    let suffix = make_suffix();
    let title = format!("notif_{suffix}");

    let created = storage.create_notification(make_request(&title)).await.unwrap();
    assert_eq!(created.title, title);
    assert_eq!(created.notification_type, "info");
    assert_eq!(created.priority, 1);

    let fetched = storage.get_notification(created.id).await.unwrap().unwrap();
    assert_eq!(fetched.id, created.id);
    assert_eq!(fetched.content, "Content");

    let _ = sqlx::query("DELETE FROM server_notifications WHERE id = $1").bind(created.id).execute(pool.as_ref()).await;
}

#[tokio::test]
async fn get_notification_none_for_missing() {
    let pool = test_pool().await;
    let storage = ServerNotificationStorage::new(&pool);
    assert!(storage.get_notification(i64::MAX).await.unwrap().is_none());
}

#[tokio::test]
async fn list_active_notifications_filters_expired() {
    let pool = test_pool().await;
    let storage = ServerNotificationStorage::new(&pool);
    let suffix = make_suffix();

    // 一条未过期（expires_at 未来）→ 应出现；一条已过期（expires_at 过去）→ 应排除。
    let active = CreateNotificationRequest {
        title: format!("active_{suffix}"),
        expires_at: Some(synapse_common::current_timestamp_millis() + 3600_000),
        ..make_request(&format!("active_{suffix}"))
    };
    let expired = CreateNotificationRequest {
        title: format!("expired_{suffix}"),
        expires_at: Some(1),
        ..make_request(&format!("expired_{suffix}"))
    };
    let a = storage.create_notification(active).await.unwrap();
    let e = storage.create_notification(expired).await.unwrap();

    let active_list = storage.list_active_notifications().await.unwrap();
    let titles: Vec<&str> = active_list.iter().map(|n| n.title.as_str()).collect();
    assert!(titles.contains(&a.title.as_str()));
    assert!(!titles.contains(&e.title.as_str()));

    let _ = sqlx::query("DELETE FROM server_notifications WHERE id = ANY($1)")
        .bind(&[a.id, e.id][..])
        .execute(pool.as_ref())
        .await;
}

#[tokio::test]
async fn update_notification_changes_fields() {
    let pool = test_pool().await;
    let storage = ServerNotificationStorage::new(&pool);
    let suffix = make_suffix();

    let created = storage.create_notification(make_request(&format!("before_{suffix}"))).await.unwrap();
    let mut update = make_request(&format!("after_{suffix}"));
    update.priority = Some(9);
    let updated = storage.update_notification(created.id, update).await.unwrap();
    assert_eq!(updated.title, format!("after_{suffix}"));
    assert_eq!(updated.priority, 9);

    let _ = sqlx::query("DELETE FROM server_notifications WHERE id = $1").bind(created.id).execute(pool.as_ref()).await;
}

#[tokio::test]
async fn delete_notification_returns_true_then_get_none() {
    let pool = test_pool().await;
    let storage = ServerNotificationStorage::new(&pool);
    let suffix = make_suffix();

    let created = storage.create_notification(make_request(&format!("del_{suffix}"))).await.unwrap();
    assert!(storage.delete_notification(created.id).await.unwrap());
    assert!(storage.get_notification(created.id).await.unwrap().is_none());
    // 二次删除返回 false。
    assert!(!storage.delete_notification(created.id).await.unwrap());
}

#[tokio::test]
async fn deactivate_notification_sets_disabled() {
    let pool = test_pool().await;
    let storage = ServerNotificationStorage::new(&pool);
    let suffix = make_suffix();

    let created = storage.create_notification(make_request(&format!("deact_{suffix}"))).await.unwrap();
    assert!(storage.deactivate_notification(created.id).await.unwrap());
    let fetched = storage.get_notification(created.id).await.unwrap().unwrap();
    assert!(!fetched.is_enabled);
    // 二次 deactivate 返回 false（已 disabled）。
    assert!(!storage.deactivate_notification(created.id).await.unwrap());

    let _ = sqlx::query("DELETE FROM server_notifications WHERE id = $1").bind(created.id).execute(pool.as_ref()).await;
}

#[tokio::test]
async fn create_template_then_get() {
    let pool = test_pool().await;
    let storage = ServerNotificationStorage::new(&pool);
    let suffix = make_suffix();
    let name = format!("tpl_{suffix}");

    let req = CreateTemplateRequest {
        name: name.clone(),
        title_template: "Hello {{name}}".to_string(),
        content_template: "Body".to_string(),
        notification_type: Some("info".to_string()),
        variables: Some(vec!["name".to_string()]),
    };
    let created = storage.create_template(req).await.unwrap();
    assert_eq!(created.name, name);

    let fetched = storage.get_template(&name).await.unwrap().unwrap();
    assert_eq!(fetched.title_template, "Hello {{name}}");

    let _ = sqlx::query("DELETE FROM notification_templates WHERE name = $1").bind(&name).execute(pool.as_ref()).await;
}

#[tokio::test]
async fn mark_as_read_creates_status() {
    let pool = test_pool().await;
    let storage = ServerNotificationStorage::new(&pool);
    let suffix = make_suffix();
    let user_id = format!("@sn_read_{suffix}:test");
    ensure_test_user(pool.as_ref(), &user_id).await;

    let created = storage.create_notification(make_request(&format!("read_{suffix}"))).await.unwrap();
    assert!(storage.mark_as_read(&user_id, created.id).await.unwrap());

    let status = storage.get_or_create_status(&user_id, created.id).await.unwrap();
    assert!(status.is_read);

    let _ = sqlx::query("DELETE FROM server_notifications WHERE id = $1").bind(created.id).execute(pool.as_ref()).await;
    let _ = sqlx::query("DELETE FROM users WHERE user_id = $1").bind(&user_id).execute(pool.as_ref()).await;
}

#[tokio::test]
async fn mark_as_read_missing_notification_returns_not_found() {
    let pool = test_pool().await;
    let storage = ServerNotificationStorage::new(&pool);
    let suffix = make_suffix();
    let user_id = format!("@sn_missing_{suffix}:test");
    ensure_test_user(pool.as_ref(), &user_id).await;

    let err = storage.mark_as_read(&user_id, i64::MAX).await.unwrap_err();
    assert!(matches!(err.kind, synapse_common::ApiErrorKind::NotFound));

    let _ = sqlx::query("DELETE FROM users WHERE user_id = $1").bind(&user_id).execute(pool.as_ref()).await;
}
