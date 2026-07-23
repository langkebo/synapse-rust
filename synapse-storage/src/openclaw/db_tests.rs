use super::{
    decode_conversation_cursor, decode_generation_cursor, decode_message_cursor, CreateChatRoleParams,
    CreateConnectionParams, CreateConversationParams, OpenClawStorage, UpdateChatRoleParams, UpdateConnectionParams,
};
use sqlx::postgres::PgPoolOptions;
use std::env;
use std::sync::Arc;

async fn test_pool() -> Arc<sqlx::PgPool> {
    let db_url = env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
    let pool =
        PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
    Arc::new(pool)
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

async fn clean_test_data(pool: &sqlx::PgPool, suffix: &str) {
    let pattern = format!("%{}%", suffix);
    // Must delete child-table rows first due to FK constraints
    sqlx::query(
        "DELETE FROM ai_messages WHERE conversation_id IN (SELECT id FROM ai_conversations WHERE user_id LIKE $1)",
    )
    .bind(&pattern)
    .execute(pool)
    .await
    .ok();
    sqlx::query("DELETE FROM ai_conversations WHERE user_id LIKE $1").bind(&pattern).execute(pool).await.ok();
    sqlx::query("DELETE FROM ai_generations WHERE user_id LIKE $1").bind(&pattern).execute(pool).await.ok();
    sqlx::query("DELETE FROM ai_chat_roles WHERE user_id LIKE $1").bind(&pattern).execute(pool).await.ok();
    sqlx::query("DELETE FROM openclaw_connections WHERE user_id LIKE $1").bind(&pattern).execute(pool).await.ok();
}

fn unique_suffix() -> String {
    uuid::Uuid::new_v4().to_string().replace('-', "")
}

fn build_user_id(suffix: &str) -> String {
    format!("@oc_test_{suffix}:localhost")
}

// ========================================================================
// Connection tests
// ========================================================================

#[tokio::test]
async fn test_create_connection() {
    let pool = test_pool().await;
    let suffix = unique_suffix();
    let user_id = build_user_id(&suffix);
    clean_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = OpenClawStorage::new(Arc::clone(&pool));

    let config = serde_json::json!({"temperature": 0.7, "max_tokens": 4096});
    let conn = storage
        .create_connection(CreateConnectionParams {
            user_id: &user_id,
            name: "test-conn",
            provider: "openai",
            base_url: "https://api.openai.com",
            encrypted_api_key: Some("enc-key-123"),
            config: Some(config.clone()),
            is_default: true,
        })
        .await
        .expect("create_connection failed");

    assert_eq!(conn.user_id, user_id);
    assert_eq!(conn.name, "test-conn");
    assert_eq!(conn.provider, "openai");
    assert_eq!(conn.base_url, "https://api.openai.com");
    assert_eq!(conn.encrypted_api_key.as_deref(), Some("enc-key-123"));
    assert_eq!(conn.config.as_ref(), Some(&config));
    assert!(conn.is_default);
    assert!(conn.is_active);
    assert!(conn.created_ts > 0);
    assert_eq!(conn.created_ts, conn.updated_ts);

    clean_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_create_connection_nullable_fields_none() {
    let pool = test_pool().await;
    let suffix = unique_suffix();
    let user_id = build_user_id(&suffix);
    clean_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = OpenClawStorage::new(Arc::clone(&pool));

    let conn = storage
        .create_connection(CreateConnectionParams {
            user_id: &user_id,
            name: "minimal-conn",
            provider: "ollama",
            base_url: "http://localhost:11434",
            encrypted_api_key: None,
            config: None,
            is_default: false,
        })
        .await
        .expect("create_connection should succeed with nullable fields as None");

    assert_eq!(conn.name, "minimal-conn");
    assert!(conn.encrypted_api_key.is_none());
    assert!(conn.config.is_none());
    assert!(!conn.is_default);

    clean_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_get_connection_found_and_not_found() {
    let pool = test_pool().await;
    let suffix = unique_suffix();
    let user_id = build_user_id(&suffix);
    clean_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = OpenClawStorage::new(Arc::clone(&pool));

    let created = storage
        .create_connection(CreateConnectionParams {
            user_id: &user_id,
            name: "get-test",
            provider: "anthropic",
            base_url: "https://api.anthropic.com",
            encrypted_api_key: None,
            config: None,
            is_default: false,
        })
        .await
        .expect("create_connection failed");

    let found =
        storage.get_connection(created.id).await.expect("get_connection failed").expect("connection should exist");
    assert_eq!(found.id, created.id);
    assert_eq!(found.name, "get-test");

    let missing = storage.get_connection(-1).await.expect("get_connection should not error for missing id");
    assert!(missing.is_none());

    clean_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_get_user_connections_multiple_and_ordered() {
    let pool = test_pool().await;
    let suffix = unique_suffix();
    let user_id = build_user_id(&suffix);
    clean_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = OpenClawStorage::new(Arc::clone(&pool));

    // Create two non-default connections first, then a default one
    storage
        .create_connection(CreateConnectionParams {
            user_id: &user_id,
            name: "conn-a",
            provider: "openai",
            base_url: "https://a.example.com",
            encrypted_api_key: None,
            config: None,
            is_default: false,
        })
        .await
        .expect("create conn-a");
    // Small sleep to ensure different created_ts
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    storage
        .create_connection(CreateConnectionParams {
            user_id: &user_id,
            name: "conn-b",
            provider: "anthropic",
            base_url: "https://b.example.com",
            encrypted_api_key: None,
            config: None,
            is_default: true,
        })
        .await
        .expect("create conn-b");
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    storage
        .create_connection(CreateConnectionParams {
            user_id: &user_id,
            name: "conn-c",
            provider: "ollama",
            base_url: "https://c.example.com",
            encrypted_api_key: None,
            config: None,
            is_default: false,
        })
        .await
        .expect("create conn-c");

    let connections = storage.get_user_connections(&user_id).await.expect("get_user_connections failed");
    assert_eq!(connections.len(), 3);
    // Default connection should be first (ordered by is_default DESC, created_ts DESC)
    assert!(connections[0].is_default);
    assert_eq!(connections[0].name, "conn-b");

    clean_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_get_user_connections_empty() {
    let pool = test_pool().await;
    let suffix = unique_suffix();
    let user_id = build_user_id(&suffix);
    clean_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = OpenClawStorage::new(Arc::clone(&pool));

    let connections = storage.get_user_connections(&user_id).await.expect("get_user_connections failed");
    assert!(connections.is_empty());

    clean_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_get_default_connection_found_and_not_found() {
    let pool = test_pool().await;
    let suffix = unique_suffix();
    let user_id = build_user_id(&suffix);
    clean_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = OpenClawStorage::new(Arc::clone(&pool));

    // No default should exist yet
    let none = storage.get_default_connection(&user_id).await.expect("get_default_connection failed");
    assert!(none.is_none());

    // Create a default (is_active=true by default in create_connection)
    let created = storage
        .create_connection(CreateConnectionParams {
            user_id: &user_id,
            name: "default-conn",
            provider: "openai",
            base_url: "https://api.openai.com",
            encrypted_api_key: None,
            config: None,
            is_default: true,
        })
        .await
        .expect("create_connection failed");

    let found = storage
        .get_default_connection(&user_id)
        .await
        .expect("get_default_connection failed")
        .expect("should find default connection");
    assert_eq!(found.id, created.id);
    assert!(found.is_default);
    assert!(found.is_active);

    clean_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_update_connection_fields() {
    let pool = test_pool().await;
    let suffix = unique_suffix();
    let user_id = build_user_id(&suffix);
    clean_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = OpenClawStorage::new(Arc::clone(&pool));

    let created = storage
        .create_connection(CreateConnectionParams {
            user_id: &user_id,
            name: "to-update",
            provider: "openai",
            base_url: "https://a.example.com",
            encrypted_api_key: None,
            config: None,
            is_default: false,
        })
        .await
        .expect("create_connection");

    let new_config = serde_json::json!({"temperature": 0.5});
    let updated = storage
        .update_connection(UpdateConnectionParams {
            id: created.id,
            name: Some("updated-name"),
            base_url: Some("https://b.example.com"),
            encrypted_api_key: Some("new-key"),
            config: Some(new_config.clone()),
            is_default: None,
            is_active: Some(false),
        })
        .await
        .expect("update_connection failed");

    assert_eq!(updated.id, created.id);
    assert_eq!(updated.name, "updated-name");
    assert_eq!(updated.base_url, "https://b.example.com");
    assert_eq!(updated.encrypted_api_key.as_deref(), Some("new-key"));
    assert_eq!(updated.config.as_ref(), Some(&new_config));
    assert!(!updated.is_active);

    clean_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_update_connection_set_default() {
    let pool = test_pool().await;
    let suffix = unique_suffix();
    let user_id = build_user_id(&suffix);
    clean_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = OpenClawStorage::new(Arc::clone(&pool));

    let conn1 = storage
        .create_connection(CreateConnectionParams {
            user_id: &user_id,
            name: "default-conn",
            provider: "openai",
            base_url: "https://a.example.com",
            encrypted_api_key: None,
            config: None,
            is_default: true,
        })
        .await
        .expect("create conn1");

    let conn2 = storage
        .create_connection(CreateConnectionParams {
            user_id: &user_id,
            name: "new-default",
            provider: "anthropic",
            base_url: "https://b.example.com",
            encrypted_api_key: None,
            config: None,
            is_default: false,
        })
        .await
        .expect("create conn2");

    // Make conn2 the new default
    let updated = storage
        .update_connection(UpdateConnectionParams {
            id: conn2.id,
            name: None,
            base_url: None,
            encrypted_api_key: None,
            config: None,
            is_default: Some(true),
            is_active: None,
        })
        .await
        .expect("update_connection to set default");
    assert!(updated.is_default);

    // conn1 should no longer be default
    let conn1_after = storage.get_connection(conn1.id).await.expect("get").expect("exists");
    assert!(!conn1_after.is_default);

    clean_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_delete_connection() {
    let pool = test_pool().await;
    let suffix = unique_suffix();
    let user_id = build_user_id(&suffix);
    clean_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = OpenClawStorage::new(Arc::clone(&pool));

    let created = storage
        .create_connection(CreateConnectionParams {
            user_id: &user_id,
            name: "to-delete",
            provider: "openai",
            base_url: "https://example.com",
            encrypted_api_key: None,
            config: None,
            is_default: false,
        })
        .await
        .expect("create_connection");

    storage.delete_connection(created.id).await.expect("delete_connection failed");

    let after = storage.get_connection(created.id).await.expect("get_connection");
    assert!(after.is_none());

    // Idempotent: deleting again should not error
    storage.delete_connection(created.id).await.expect("delete_connection should be idempotent");

    clean_test_data(&pool, &suffix).await;
}

// ========================================================================
// Conversation tests
// ========================================================================

#[tokio::test]
async fn test_create_conversation() {
    let pool = test_pool().await;
    let suffix = unique_suffix();
    let user_id = build_user_id(&suffix);
    clean_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = OpenClawStorage::new(Arc::clone(&pool));

    let conv = storage
        .create_conversation(CreateConversationParams {
            user_id: &user_id,
            connection_id: None,
            title: Some("Test Conversation"),
            model_id: Some("gpt-4"),
            system_prompt: Some("You are helpful."),
            temperature: Some(0.8),
            max_tokens: Some(2048),
        })
        .await
        .expect("create_conversation failed");

    assert_eq!(conv.user_id, user_id);
    assert_eq!(conv.title.as_deref(), Some("Test Conversation"));
    assert_eq!(conv.model_id.as_deref(), Some("gpt-4"));
    assert_eq!(conv.system_prompt.as_deref(), Some("You are helpful."));
    assert!((conv.temperature.unwrap() - 0.8).abs() < f32::EPSILON);
    assert_eq!(conv.max_tokens, Some(2048));
    assert!(!conv.is_pinned);
    assert!(conv.connection_id.is_none());
    assert!(conv.created_ts > 0);

    // Also test creating with only required fields (title=None)
    let conv2 = storage
        .create_conversation(CreateConversationParams {
            user_id: &user_id,
            connection_id: None,
            title: None,
            model_id: None,
            system_prompt: None,
            temperature: None,
            max_tokens: None,
        })
        .await
        .expect("create_conversation with None fields");
    assert!(conv2.title.is_none());
    assert!(conv2.id != conv.id);

    clean_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_get_conversation_found_and_not_found() {
    let pool = test_pool().await;
    let suffix = unique_suffix();
    let user_id = build_user_id(&suffix);
    clean_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = OpenClawStorage::new(Arc::clone(&pool));

    let created = storage
        .create_conversation(CreateConversationParams {
            user_id: &user_id,
            connection_id: None,
            title: Some("My Conversation"),
            model_id: None,
            system_prompt: None,
            temperature: None,
            max_tokens: None,
        })
        .await
        .expect("create_conversation");

    let found =
        storage.get_conversation(created.id).await.expect("get_conversation failed").expect("should find conversation");
    assert_eq!(found.id, created.id);
    assert_eq!(found.title.as_deref(), Some("My Conversation"));

    let missing = storage.get_conversation(-1).await.expect("get_conversation should not error");
    assert!(missing.is_none());

    clean_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_get_user_conversations_pagination_and_empty() {
    let pool = test_pool().await;
    let suffix = unique_suffix();
    let user_id = build_user_id(&suffix);
    clean_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = OpenClawStorage::new(Arc::clone(&pool));

    // Empty case first
    let (empty, next) =
        storage.get_user_conversations(&user_id, 10, None).await.expect("get_user_conversations failed");
    assert!(empty.is_empty());
    assert!(next.is_none());

    // Create 5 conversations
    for i in 0..5 {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        storage
            .create_conversation(CreateConversationParams {
                user_id: &user_id,
                connection_id: None,
                title: Some(&format!("Conv {i}")),
                model_id: None,
                system_prompt: None,
                temperature: None,
                max_tokens: None,
            })
            .await
            .expect("create_conversation");
    }

    // Fetch with limit=3, should return 3 + next_batch cursor
    let (convs, cursor) =
        storage.get_user_conversations(&user_id, 3, None).await.expect("get_user_conversations failed");
    assert_eq!(convs.len(), 3);
    assert!(cursor.is_some(), "should have next-batch cursor");

    // Use cursor to get next page (limit=3 again)
    let decoded = decode_conversation_cursor(cursor.as_deref()).expect("cursor should decode");
    let (convs2, cursor2) = storage
        .get_user_conversations(&user_id, 3, Some(decoded))
        .await
        .expect("get_user_conversations with cursor failed");
    // With limit=3 and 5 total, we fetched 4 rows (limit+1) on page1,
    // returned 3, and the cursor points to the 4th item. On page2, only
    // the 5th (oldest) item remains.
    assert_eq!(convs2.len(), 1);
    assert!(cursor2.is_none(), "no more pages");

    clean_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_update_conversation() {
    let pool = test_pool().await;
    let suffix = unique_suffix();
    let user_id = build_user_id(&suffix);
    clean_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = OpenClawStorage::new(Arc::clone(&pool));

    let created = storage
        .create_conversation(CreateConversationParams {
            user_id: &user_id,
            connection_id: None,
            title: Some("Original"),
            model_id: None,
            system_prompt: None,
            temperature: None,
            max_tokens: None,
        })
        .await
        .expect("create_conversation");

    let updated = storage
        .update_conversation(
            created.id,
            Some("Updated Title"),
            Some("New system prompt"),
            Some(0.3),
            Some(1024),
            Some(true),
        )
        .await
        .expect("update_conversation failed");

    assert_eq!(updated.id, created.id);
    assert_eq!(updated.title.as_deref(), Some("Updated Title"));
    assert_eq!(updated.system_prompt.as_deref(), Some("New system prompt"));
    assert!((updated.temperature.unwrap() - 0.3).abs() < f32::EPSILON);
    assert_eq!(updated.max_tokens, Some(1024));
    assert!(updated.is_pinned);

    clean_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_delete_conversation() {
    let pool = test_pool().await;
    let suffix = unique_suffix();
    let user_id = build_user_id(&suffix);
    clean_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = OpenClawStorage::new(Arc::clone(&pool));

    let created = storage
        .create_conversation(CreateConversationParams {
            user_id: &user_id,
            connection_id: None,
            title: Some("To Delete"),
            model_id: None,
            system_prompt: None,
            temperature: None,
            max_tokens: None,
        })
        .await
        .expect("create_conversation");

    storage.delete_conversation(created.id).await.expect("delete_conversation failed");

    let after = storage.get_conversation(created.id).await.expect("get_conversation");
    assert!(after.is_none());

    // Idempotent: deleting again does not error
    storage.delete_conversation(created.id).await.expect("delete_conversation idempotent");

    clean_test_data(&pool, &suffix).await;
}

// ========================================================================
// Message tests
// ========================================================================

#[tokio::test]
async fn test_create_message_user_and_assistant_roles() {
    let pool = test_pool().await;
    let suffix = unique_suffix();
    let user_id = build_user_id(&suffix);
    clean_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = OpenClawStorage::new(Arc::clone(&pool));

    let conv = storage
        .create_conversation(CreateConversationParams {
            user_id: &user_id,
            connection_id: None,
            title: None,
            model_id: None,
            system_prompt: None,
            temperature: None,
            max_tokens: None,
        })
        .await
        .expect("create_conversation");

    let tool_calls = serde_json::json!([{"name": "search", "args": {"q": "weather"}}]);
    let msg_user = storage
        .create_message(conv.id, "user", "Hello, world!", Some(50), None, None)
        .await
        .expect("create user message");

    assert_eq!(msg_user.conversation_id, conv.id);
    assert_eq!(msg_user.role, "user");
    assert_eq!(msg_user.content, "Hello, world!");
    assert_eq!(msg_user.token_count, Some(50));
    assert!(msg_user.tool_calls.is_none());
    assert!(msg_user.created_ts > 0);

    let msg_assistant = storage
        .create_message(conv.id, "assistant", "Hi there!", Some(30), Some(tool_calls.clone()), Some("call_123"))
        .await
        .expect("create assistant message");

    assert_eq!(msg_assistant.role, "assistant");
    assert_eq!(msg_assistant.content, "Hi there!");
    assert_eq!(msg_assistant.tool_calls.as_ref(), Some(&tool_calls));
    assert_eq!(msg_assistant.tool_call_id.as_deref(), Some("call_123"));

    clean_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_get_conversation_messages_ordered_and_pagination() {
    let pool = test_pool().await;
    let suffix = unique_suffix();
    let user_id = build_user_id(&suffix);
    clean_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = OpenClawStorage::new(Arc::clone(&pool));

    let conv = storage
        .create_conversation(CreateConversationParams {
            user_id: &user_id,
            connection_id: None,
            title: None,
            model_id: None,
            system_prompt: None,
            temperature: None,
            max_tokens: None,
        })
        .await
        .expect("create_conversation");

    // Create 4 messages (sleep to guarantee different timestamps)
    for i in 0..4 {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        storage
            .create_message(conv.id, "user", &format!("Message {i}"), None, None, None)
            .await
            .expect("create message");
    }

    // Fetch all 4
    let (msgs, next) =
        storage.get_conversation_messages(conv.id, 10, None).await.expect("get_conversation_messages failed");
    assert_eq!(msgs.len(), 4);
    assert!(next.is_none());

    // Messages should be in descending order by created_ts (newest first)
    for i in 1..msgs.len() {
        assert!(msgs[i - 1].created_ts >= msgs[i].created_ts, "messages should be ordered by created_ts DESC");
    }

    // Pagination: limit=2
    let (page1, cursor1) = storage.get_conversation_messages(conv.id, 2, None).await.expect("first page");
    assert_eq!(page1.len(), 2);
    assert!(cursor1.is_some());

    let decoded = decode_message_cursor(cursor1.as_deref()).expect("message cursor should decode");
    let (page2, cursor2) = storage.get_conversation_messages(conv.id, 2, Some(decoded)).await.expect("second page");
    assert_eq!(page2.len(), 1);
    assert!(cursor2.is_none(), "no more pages");

    // Ensure pages don't overlap
    let page1_ids: Vec<i64> = page1.iter().map(|m| m.id).collect();
    let page2_ids: Vec<i64> = page2.iter().map(|m| m.id).collect();
    for id in &page1_ids {
        assert!(!page2_ids.contains(id), "pages must not overlap");
    }

    clean_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_get_message_found_and_not_found() {
    let pool = test_pool().await;
    let suffix = unique_suffix();
    let user_id = build_user_id(&suffix);
    clean_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = OpenClawStorage::new(Arc::clone(&pool));

    let conv = storage
        .create_conversation(CreateConversationParams {
            user_id: &user_id,
            connection_id: None,
            title: None,
            model_id: None,
            system_prompt: None,
            temperature: None,
            max_tokens: None,
        })
        .await
        .expect("create_conversation");

    let msg = storage.create_message(conv.id, "user", "Hello", None, None, None).await.expect("create message");

    let found = storage.get_message(msg.id).await.expect("get_message failed").expect("should find message");
    assert_eq!(found.id, msg.id);
    assert_eq!(found.content, "Hello");

    let missing = storage.get_message(-1).await.expect("get_message for missing id");
    assert!(missing.is_none());

    clean_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_delete_message() {
    let pool = test_pool().await;
    let suffix = unique_suffix();
    let user_id = build_user_id(&suffix);
    clean_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = OpenClawStorage::new(Arc::clone(&pool));

    let conv = storage
        .create_conversation(CreateConversationParams {
            user_id: &user_id,
            connection_id: None,
            title: None,
            model_id: None,
            system_prompt: None,
            temperature: None,
            max_tokens: None,
        })
        .await
        .expect("create_conversation");

    let msg = storage.create_message(conv.id, "user", "Delete me", None, None, None).await.expect("create message");

    storage.delete_message(msg.id).await.expect("delete_message failed");

    let after = storage.get_message(msg.id).await.expect("get_message");
    assert!(after.is_none());

    // Idempotent
    storage.delete_message(msg.id).await.expect("delete_message idempotent");

    clean_test_data(&pool, &suffix).await;
}

// ========================================================================
// Generation tests
// ========================================================================

#[tokio::test]
async fn test_create_and_update_generation() {
    let pool = test_pool().await;
    let suffix = unique_suffix();
    let user_id = build_user_id(&suffix);
    clean_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = OpenClawStorage::new(Arc::clone(&pool));

    let gen = storage
        .create_generation(&user_id, None, "image", "A beautiful sunset")
        .await
        .expect("create_generation failed");

    assert_eq!(gen.user_id, user_id);
    assert_eq!(gen.r#type, "image");
    assert_eq!(gen.prompt, "A beautiful sunset");
    assert_eq!(gen.status, "pending");
    assert!(gen.created_ts > 0);
    assert!(gen.completed_ts.is_none());
    assert!(gen.conversation_id.is_none());

    // Update to completed
    let updated = storage
        .update_generation_status(
            gen.id,
            "completed",
            Some("https://cdn.example.com/img.png"),
            Some("mxc://localhost/img"),
            None,
        )
        .await
        .expect("update_generation_status failed");

    assert_eq!(updated.status, "completed");
    assert_eq!(updated.result_url.as_deref(), Some("https://cdn.example.com/img.png"));
    assert_eq!(updated.result_mxc.as_deref(), Some("mxc://localhost/img"));
    assert!(updated.completed_ts.is_some());

    // Update to failed with error message
    let gen2 =
        storage.create_generation(&user_id, None, "video", "Generate a video").await.expect("create second generation");

    let failed = storage
        .update_generation_status(gen2.id, "failed", None, None, Some("API quota exceeded"))
        .await
        .expect("update to failed");

    assert_eq!(failed.status, "failed");
    assert_eq!(failed.error_message.as_deref(), Some("API quota exceeded"));

    clean_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_get_generation_found_and_not_found() {
    let pool = test_pool().await;
    let suffix = unique_suffix();
    let user_id = build_user_id(&suffix);
    clean_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = OpenClawStorage::new(Arc::clone(&pool));

    let gen = storage.create_generation(&user_id, None, "image", "Prompt").await.expect("create_generation");

    let found = storage.get_generation(gen.id).await.expect("get_generation failed").expect("should find generation");
    assert_eq!(found.id, gen.id);

    let missing = storage.get_generation(-1).await.expect("get_generation for missing");
    assert!(missing.is_none());

    clean_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_get_user_generations_filter_and_pagination() {
    let pool = test_pool().await;
    let suffix = unique_suffix();
    let user_id = build_user_id(&suffix);
    clean_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = OpenClawStorage::new(Arc::clone(&pool));

    // Create 3 image and 2 audio generations
    for i in 0..3 {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        storage
            .create_generation(&user_id, None, "image", &format!("Image prompt {i}"))
            .await
            .expect("create image gen");
    }
    for i in 0..2 {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        storage
            .create_generation(&user_id, None, "audio", &format!("Audio prompt {i}"))
            .await
            .expect("create audio gen");
    }

    // All generations without type filter
    let (all, _) = storage.get_user_generations(&user_id, None, 10, None).await.expect("get_user_generations all");
    assert_eq!(all.len(), 5);

    // Filter by type "image" → 3
    let (images, _) = storage
        .get_user_generations(&user_id, Some("image"), 10, None)
        .await
        .expect("get_user_generations image filter");
    assert_eq!(images.len(), 3);
    for img in &images {
        assert_eq!(img.r#type, "image");
    }

    // Filter by type "audio" → 2
    let (audios, _) = storage
        .get_user_generations(&user_id, Some("audio"), 10, None)
        .await
        .expect("get_user_generations audio filter");
    assert_eq!(audios.len(), 2);
    for a in &audios {
        assert_eq!(a.r#type, "audio");
    }

    // Pagination with type filter: limit=2
    let (page1, cursor1) =
        storage.get_user_generations(&user_id, Some("image"), 2, None).await.expect("get_user_generations page1");
    assert_eq!(page1.len(), 2);
    assert!(cursor1.is_some());

    let decoded = decode_generation_cursor(cursor1.as_deref()).expect("cursor should decode");
    let (page2, cursor2) = storage
        .get_user_generations(&user_id, Some("image"), 2, Some(decoded))
        .await
        .expect("get_user_generations page2");
    assert_eq!(page2.len(), 0);
    assert!(cursor2.is_none());

    clean_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_delete_generation() {
    let pool = test_pool().await;
    let suffix = unique_suffix();
    let user_id = build_user_id(&suffix);
    clean_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = OpenClawStorage::new(Arc::clone(&pool));

    let gen = storage.create_generation(&user_id, None, "image", "Delete me").await.expect("create_generation");

    storage.delete_generation(gen.id).await.expect("delete_generation failed");

    let after = storage.get_generation(gen.id).await.expect("get_generation");
    assert!(after.is_none());

    // Idempotent
    storage.delete_generation(gen.id).await.expect("delete_generation idempotent");

    clean_test_data(&pool, &suffix).await;
}

// ========================================================================
// Chat role tests
// ========================================================================

#[tokio::test]
async fn test_create_and_get_chat_role() {
    let pool = test_pool().await;
    let suffix = unique_suffix();
    let user_id = build_user_id(&suffix);
    clean_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = OpenClawStorage::new(Arc::clone(&pool));

    let role = storage
        .create_chat_role(CreateChatRoleParams {
            user_id: &user_id,
            name: "My Role",
            description: Some("A test role"),
            system_message: "You are a helpful assistant.",
            model_id: Some("gpt-4"),
            avatar_url: Some("mxc://localhost/avatar"),
            category: Some("general"),
            temperature: Some(0.7),
            max_tokens: Some(2048),
            is_public: true,
        })
        .await
        .expect("create_chat_role failed");

    assert_eq!(role.user_id, user_id);
    assert_eq!(role.name, "My Role");
    assert_eq!(role.description.as_deref(), Some("A test role"));
    assert_eq!(role.system_message, "You are a helpful assistant.");
    assert!(role.is_public);
    assert!(role.created_ts > 0);

    // Get by id
    let found = storage.get_chat_role(role.id).await.expect("get_chat_role failed").expect("should find role");
    assert_eq!(found.id, role.id);

    let missing = storage.get_chat_role(-1).await.expect("get_chat_role missing");
    assert!(missing.is_none());

    clean_test_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_get_user_chat_roles_includes_public() {
    let pool = test_pool().await;
    let suffix = unique_suffix();
    let user_id = build_user_id(&suffix);
    let suffix2 = unique_suffix();
    let user_id2 = build_user_id(&suffix2);
    clean_test_data(&pool, &suffix).await;
    clean_test_data(&pool, &suffix2).await;
    // Also clean any residual test chat roles with oc_test_ pattern
    sqlx::query("DELETE FROM ai_chat_roles WHERE user_id LIKE '%oc_test_%'").execute(pool.as_ref()).await.ok();
    ensure_test_user(&pool, &user_id).await;
    ensure_test_user(&pool, &user_id2).await;

    let storage = OpenClawStorage::new(Arc::clone(&pool));

    // User 1 creates a role
    storage
        .create_chat_role(CreateChatRoleParams {
            user_id: &user_id,
            name: "Private Role",
            description: None,
            system_message: "Private",
            model_id: None,
            avatar_url: None,
            category: None,
            temperature: None,
            max_tokens: None,
            is_public: false,
        })
        .await
        .expect("create private role");

    // User 2 creates a public role
    storage
        .create_chat_role(CreateChatRoleParams {
            user_id: &user_id2,
            name: "Public Role",
            description: None,
            system_message: "Public",
            model_id: None,
            avatar_url: None,
            category: None,
            temperature: None,
            max_tokens: None,
            is_public: true,
        })
        .await
        .expect("create public role");

    // User 1 sees own private role + public role of user 2
    // (may also see other public roles from residual test data)
    let roles = storage.get_user_chat_roles(&user_id).await.expect("get_user_chat_roles failed");
    assert!(roles.len() >= 2, "should see at least own private role + public role from user 2");
    // Verify that our private role is present
    let private = roles.iter().find(|r| r.name == "Private Role");
    assert!(private.is_some(), "should find own private role");
    assert!(!private.unwrap().is_public);
    // Verify that user2's public role is present
    let public = roles.iter().find(|r| r.name == "Public Role");
    assert!(public.is_some(), "should find user2's public role");
    assert!(public.unwrap().is_public);
    // Own private roles should come before public roles (ORDER BY is_public)
    let private_pos = roles.iter().position(|r| r.name == "Private Role").unwrap();
    let public_pos = roles.iter().position(|r| r.name == "Public Role").unwrap();
    assert!(private_pos < public_pos, "private roles should be ordered before public roles");

    clean_test_data(&pool, &suffix).await;
    clean_test_data(&pool, &suffix2).await;
}

#[tokio::test]
async fn test_update_and_delete_chat_role() {
    let pool = test_pool().await;
    let suffix = unique_suffix();
    let user_id = build_user_id(&suffix);
    clean_test_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let storage = OpenClawStorage::new(Arc::clone(&pool));

    let role = storage
        .create_chat_role(CreateChatRoleParams {
            user_id: &user_id,
            name: "Original",
            description: None,
            system_message: "Original prompt",
            model_id: None,
            avatar_url: None,
            category: None,
            temperature: None,
            max_tokens: None,
            is_public: false,
        })
        .await
        .expect("create_chat_role");

    let updated = storage
        .update_chat_role(UpdateChatRoleParams {
            id: role.id,
            name: Some("Updated Role"),
            description: Some("New description"),
            system_message: Some("Updated prompt"),
            model_id: Some("claude-4"),
            avatar_url: None,
            category: None,
            temperature: Some(0.5),
            max_tokens: Some(4096),
            is_public: Some(true),
        })
        .await
        .expect("update_chat_role failed");

    assert_eq!(updated.name, "Updated Role");
    assert_eq!(updated.description.as_deref(), Some("New description"));
    assert_eq!(updated.system_message, "Updated prompt");
    assert_eq!(updated.model_id.as_deref(), Some("claude-4"));
    assert!(updated.is_public);

    // Delete
    storage.delete_chat_role(role.id).await.expect("delete_chat_role failed");
    let after = storage.get_chat_role(role.id).await.expect("get_chat_role");
    assert!(after.is_none());

    // Idempotent
    storage.delete_chat_role(role.id).await.expect("delete_chat_role idempotent");

    clean_test_data(&pool, &suffix).await;
}
