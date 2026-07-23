#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use synapse_common::current_timestamp_millis;

use serde_json::json;
use sqlx::PgPool;
use synapse_storage::{CreateFilterRequest, Filter, FilterStorage, FilterStoreApi};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

async fn setup_test_database(pool: &Arc<sqlx::PgPool>) {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS filters (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL,
            filter_id TEXT NOT NULL,
            content JSONB NOT NULL DEFAULT '{}',
            created_ts BIGINT NOT NULL,
            CONSTRAINT uq_filters_user_filter UNIQUE (user_id, filter_id)
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create filters table");
}

fn create_storage(pool: &Arc<PgPool>) -> Arc<dyn FilterStoreApi> {
    Arc::new(FilterStorage::new(pool))
}

#[tokio::test]
async fn test_create_filter_request_construction() {
    let request = CreateFilterRequest {
        user_id: "@alice:example.com".to_string(),
        filter_id: "my_filter".to_string(),
        content: json!({"room": {"timeline": {"limit": 50}}}),
    };
    assert_eq!(request.user_id, "@alice:example.com");
    assert_eq!(request.filter_id, "my_filter");
    assert_eq!(request.content.get("room").unwrap().get("timeline").unwrap().get("limit"), Some(&json!(50)));
}

#[tokio::test]
async fn test_create_filter_request_with_empty_content() {
    let request = CreateFilterRequest {
        user_id: "@bob:example.com".to_string(),
        filter_id: "empty".to_string(),
        content: json!({}),
    };
    assert_eq!(request.user_id, "@bob:example.com");
    assert!(request.content.as_object().unwrap().is_empty());
}

#[tokio::test]
async fn test_create_filter_request_with_complex_content() {
    let content = json!({
        "room": {
            "timeline": {"limit": 100, "types": ["m.room.message"]},
            "state": {"types": ["m.room.member"], "lazy_load_members": true},
            "ephemeral": {"types": ["m.typing", "m.receipt"]},
            "account_data": {"types": ["m.fully_read"]}
        },
        "presence": {"types": ["m.presence"], "limit": 20}
    });
    let request = CreateFilterRequest {
        user_id: "@charlie:example.com".to_string(),
        filter_id: "complex_filter".to_string(),
        content,
    };
    assert_eq!(request.filter_id, "complex_filter");
    let room = request.content.get("room").unwrap();
    assert!(room.get("timeline").is_some());
    assert!(room.get("state").is_some());
    assert!(room.get("ephemeral").is_some());
    assert!(room.get("account_data").is_some());
    assert!(request.content.get("presence").is_some());
}

#[tokio::test]
async fn test_filter_struct_construction() {
    let filter = Filter {
        id: 42,
        user_id: "@dave:example.com".to_string(),
        filter_id: "test_filter".to_string(),
        content: json!({"room": {"timeline": {"limit": 25}}}),
        created_ts: 1700000000000,
    };
    assert_eq!(filter.id, 42);
    assert_eq!(filter.user_id, "@dave:example.com");
    assert_eq!(filter.filter_id, "test_filter");
    assert_eq!(filter.created_ts, 1700000000000);
}

#[tokio::test]
async fn test_filter_clone() {
    let filter = Filter {
        id: 1,
        user_id: "@eve:example.com".to_string(),
        filter_id: "clone_me".to_string(),
        content: json!({"key": "value"}),
        created_ts: 1700000000000,
    };
    let cloned = filter.clone();
    assert_eq!(cloned.id, filter.id);
    assert_eq!(cloned.user_id, filter.user_id);
    assert_eq!(cloned.filter_id, filter.filter_id);
    assert_eq!(cloned.content, filter.content);
    assert_eq!(cloned.created_ts, filter.created_ts);
}

#[tokio::test]
async fn test_create_filter_request_serialization() {
    let request = CreateFilterRequest {
        user_id: "@frank:example.com".to_string(),
        filter_id: "ser_test".to_string(),
        content: json!({"room": {"timeline": {"limit": 10}}}),
    };
    let serialized = serde_json::to_string(&request).unwrap();
    let deserialized: CreateFilterRequest = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.user_id, request.user_id);
    assert_eq!(deserialized.filter_id, request.filter_id);
    assert_eq!(deserialized.content, request.content);
}

#[tokio::test]
async fn test_filter_serialization() {
    let filter = Filter {
        id: 99,
        user_id: "@grace:example.com".to_string(),
        filter_id: "ser_filter".to_string(),
        content: json!({"presence": {"limit": 5}}),
        created_ts: 1700000000000,
    };
    let serialized = serde_json::to_string(&filter).unwrap();
    let deserialized: Filter = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.id, filter.id);
    assert_eq!(deserialized.user_id, filter.user_id);
    assert_eq!(deserialized.filter_id, filter.filter_id);
    assert_eq!(deserialized.content, filter.content);
    assert_eq!(deserialized.created_ts, filter.created_ts);
}

#[tokio::test]
async fn test_create_filter_success() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@filter_create_{suffix}:localhost");

    let request = CreateFilterRequest {
        user_id: user_id.clone(),
        filter_id: "filter_1".to_string(),
        content: json!({"room": {"timeline": {"limit": 100}}}),
    };

    let filter = storage.create_filter(request).await.unwrap();
    assert!(filter.id > 0);
    assert_eq!(filter.user_id, user_id);
    assert_eq!(filter.filter_id, "filter_1");
    assert!(filter.created_ts > 0);
    assert_eq!(filter.content.get("room").unwrap().get("timeline").unwrap().get("limit"), Some(&json!(100)));
}

#[tokio::test]
async fn test_create_filter_sets_created_ts() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@filter_ts_{suffix}:localhost");

    let before = current_timestamp_millis();

    let request =
        CreateFilterRequest { user_id: user_id.clone(), filter_id: "ts_filter".to_string(), content: json!({}) };

    let filter = storage.create_filter(request).await.unwrap();
    let after = current_timestamp_millis();

    assert!(filter.created_ts >= before);
    assert!(filter.created_ts <= after);
}

#[tokio::test]
async fn test_create_filter_duplicate_returns_error() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@filter_dup_{suffix}:localhost");

    let request = CreateFilterRequest {
        user_id: user_id.clone(),
        filter_id: "dup_filter".to_string(),
        content: json!({"key": "first"}),
    };
    storage.create_filter(request).await.unwrap();

    let request2 = CreateFilterRequest {
        user_id: user_id.clone(),
        filter_id: "dup_filter".to_string(),
        content: json!({"key": "second"}),
    };
    let result = storage.create_filter(request2).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_get_filter_found() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@filter_get_{suffix}:localhost");

    let request = CreateFilterRequest {
        user_id: user_id.clone(),
        filter_id: "get_filter".to_string(),
        content: json!({"room": {"timeline": {"limit": 50}}}),
    };
    let created = storage.create_filter(request).await.unwrap();

    let found = storage.get_filter(&user_id, "get_filter").await.unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.id, created.id);
    assert_eq!(found.user_id, user_id);
    assert_eq!(found.filter_id, "get_filter");
    assert_eq!(found.content, created.content);
}

#[tokio::test]
async fn test_get_filter_not_found() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();

    let result = storage.get_filter(&format!("@nobody_{suffix}:localhost"), "no_filter").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_filter_wrong_user() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@filter_owner_{suffix}:localhost");

    let request =
        CreateFilterRequest { user_id: user_id.clone(), filter_id: "owned_filter".to_string(), content: json!({}) };
    storage.create_filter(request).await.unwrap();

    let result = storage.get_filter(&format!("@other_{suffix}:localhost"), "owned_filter").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_filters_by_user_multiple() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@filter_multi_{suffix}:localhost");

    for i in 1..=3 {
        let request = CreateFilterRequest {
            user_id: user_id.clone(),
            filter_id: format!("multi_{i}"),
            content: json!({"index": i}),
        };
        storage.create_filter(request).await.unwrap();
    }

    let filters = storage.get_filters_by_user(&user_id).await.unwrap();
    assert_eq!(filters.len(), 3);
    let filter_ids: Vec<&str> = filters.iter().map(|f| f.filter_id.as_str()).collect();
    assert!(filter_ids.contains(&"multi_1"));
    assert!(filter_ids.contains(&"multi_2"));
    assert!(filter_ids.contains(&"multi_3"));
}

#[tokio::test]
async fn test_get_filters_by_user_empty() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();

    let filters = storage.get_filters_by_user(&format!("@empty_{suffix}:localhost")).await.unwrap();
    assert!(filters.is_empty());
}

#[tokio::test]
async fn test_get_filters_by_user_isolation() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_a = format!("@iso_a_{suffix}:localhost");
    let user_b = format!("@iso_b_{suffix}:localhost");

    let request_a = CreateFilterRequest {
        user_id: user_a.clone(),
        filter_id: "shared_id".to_string(),
        content: json!({"owner": "a"}),
    };
    storage.create_filter(request_a).await.unwrap();

    let request_b = CreateFilterRequest {
        user_id: user_b.clone(),
        filter_id: "shared_id".to_string(),
        content: json!({"owner": "b"}),
    };
    storage.create_filter(request_b).await.unwrap();

    let filters_a = storage.get_filters_by_user(&user_a).await.unwrap();
    let filters_b = storage.get_filters_by_user(&user_b).await.unwrap();

    assert_eq!(filters_a.len(), 1);
    assert_eq!(filters_b.len(), 1);
    assert_eq!(filters_a[0].content.get("owner"), Some(&json!("a")));
    assert_eq!(filters_b[0].content.get("owner"), Some(&json!("b")));
}

#[tokio::test]
async fn test_delete_filter_success() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@filter_del_{suffix}:localhost");

    let request =
        CreateFilterRequest { user_id: user_id.clone(), filter_id: "del_filter".to_string(), content: json!({}) };
    storage.create_filter(request).await.unwrap();

    let deleted = storage.delete_filter(&user_id, "del_filter").await.unwrap();
    assert!(deleted);

    let found = storage.get_filter(&user_id, "del_filter").await.unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn test_delete_filter_not_found() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();

    let deleted = storage.delete_filter(&format!("@ghost_{suffix}:localhost"), "ghost_filter").await.unwrap();
    assert!(!deleted);
}

#[tokio::test]
async fn test_delete_filter_wrong_user() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@filter_owner_del_{suffix}:localhost");

    let request =
        CreateFilterRequest { user_id: user_id.clone(), filter_id: "owner_del".to_string(), content: json!({}) };
    storage.create_filter(request).await.unwrap();

    let deleted = storage.delete_filter(&format!("@wrong_{suffix}:localhost"), "owner_del").await.unwrap();
    assert!(!deleted);

    let found = storage.get_filter(&user_id, "owner_del").await.unwrap();
    assert!(found.is_some());
}

#[tokio::test]
async fn test_delete_filters_by_user() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@filter_delall_{suffix}:localhost");

    for i in 1..=3 {
        let request = CreateFilterRequest {
            user_id: user_id.clone(),
            filter_id: format!("delall_{i}"),
            content: json!({"i": i}),
        };
        storage.create_filter(request).await.unwrap();
    }

    let count = storage.delete_filters_by_user(&user_id).await.unwrap();
    assert_eq!(count, 3);

    let filters = storage.get_filters_by_user(&user_id).await.unwrap();
    assert!(filters.is_empty());
}

#[tokio::test]
async fn test_delete_filters_by_user_empty() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();

    let count = storage.delete_filters_by_user(&format!("@no_filters_{suffix}:localhost")).await.unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_delete_filters_by_user_isolation() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_a = format!("@iso_del_a_{suffix}:localhost");
    let user_b = format!("@iso_del_b_{suffix}:localhost");

    let request_a = CreateFilterRequest {
        user_id: user_a.clone(),
        filter_id: "iso_del".to_string(),
        content: json!({"owner": "a"}),
    };
    storage.create_filter(request_a).await.unwrap();

    let request_b = CreateFilterRequest {
        user_id: user_b.clone(),
        filter_id: "iso_del".to_string(),
        content: json!({"owner": "b"}),
    };
    storage.create_filter(request_b).await.unwrap();

    let count = storage.delete_filters_by_user(&user_a).await.unwrap();
    assert_eq!(count, 1);

    let filters_b = storage.get_filters_by_user(&user_b).await.unwrap();
    assert_eq!(filters_b.len(), 1);
    assert_eq!(filters_b[0].content.get("owner"), Some(&json!("b")));
}

#[tokio::test]
async fn test_get_filter_after_delete_returns_none() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@filter_reget_{suffix}:localhost");

    let request = CreateFilterRequest {
        user_id: user_id.clone(),
        filter_id: "reget_filter".to_string(),
        content: json!({"temp": true}),
    };
    storage.create_filter(request).await.unwrap();

    let found = storage.get_filter(&user_id, "reget_filter").await.unwrap();
    assert!(found.is_some());

    storage.delete_filter(&user_id, "reget_filter").await.unwrap();

    let found = storage.get_filter(&user_id, "reget_filter").await.unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn test_create_filter_with_various_content_types() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    let storage = create_storage(&pool);
    let suffix = unique_id();
    let user_id = format!("@filter_content_{suffix}:localhost");

    let content = json!({
        "string_val": "hello",
        "number_val": 42,
        "bool_val": true,
        "null_val": null,
        "array_val": [1, 2, 3],
        "nested": {"deep": {"key": "value"}}
    });

    let request = CreateFilterRequest { user_id: user_id.clone(), filter_id: "content_filter".to_string(), content };

    let filter = storage.create_filter(request).await.unwrap();
    assert_eq!(filter.content.get("string_val"), Some(&json!("hello")));
    assert_eq!(filter.content.get("number_val"), Some(&json!(42)));
    assert_eq!(filter.content.get("bool_val"), Some(&json!(true)));
    assert_eq!(filter.content.get("null_val"), Some(&json!(null)));
    assert_eq!(filter.content.get("array_val"), Some(&json!([1, 2, 3])));
    assert_eq!(filter.content.get("nested").unwrap().get("deep").unwrap().get("key"), Some(&json!("value")));
}
