#![cfg(test)]

use sqlx::{Pool, Postgres};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use synapse_rust::storage::federation_blacklist::{
    decode_federation_blacklist_cursor, encode_federation_blacklist_cursor, AddBlacklistRequest, CreateLogRequest,
    CreateRuleRequest, FederationBlacklistCursor, FederationBlacklistStorage, UpdateStatsRequest,
};
use tokio::runtime::Runtime;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

async fn setup_test_database() -> Option<Arc<Pool<Postgres>>> {
    let pool = match synapse_rust::test_utils::prepare_empty_isolated_test_pool().await {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!("Skipping federation blacklist storage tests because test database is unavailable: {error}");
            return None;
        }
    };

    sqlx::query(
        r#"
        CREATE TABLE federation_blacklist (
            id BIGSERIAL PRIMARY KEY,
            server_name TEXT NOT NULL UNIQUE,
            block_type TEXT NOT NULL DEFAULT 'blacklist',
            reason TEXT,
            blocked_by TEXT,
            added_by TEXT,
            created_ts BIGINT,
            added_ts BIGINT,
            updated_ts BIGINT,
            expires_at BIGINT,
            is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
            metadata JSONB NOT NULL DEFAULT '{}'
        )
        "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create federation_blacklist table");

    sqlx::query(
        r#"
        CREATE TABLE federation_blacklist_log (
            id BIGSERIAL PRIMARY KEY,
            server_name TEXT NOT NULL,
            action TEXT NOT NULL,
            old_status TEXT,
            new_status TEXT,
            reason TEXT,
            performed_by TEXT NOT NULL,
            performed_ts BIGINT NOT NULL,
            ip_address TEXT,
            user_agent TEXT,
            metadata JSONB NOT NULL DEFAULT '{}'
        )
        "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create federation_blacklist_log table");

    sqlx::query(
        r#"
        CREATE TABLE federation_access_stats (
            id BIGSERIAL PRIMARY KEY,
            server_name TEXT NOT NULL UNIQUE,
            total_requests BIGINT NOT NULL DEFAULT 0,
            successful_requests BIGINT NOT NULL DEFAULT 0,
            failed_requests BIGINT NOT NULL DEFAULT 0,
            last_request_ts BIGINT,
            last_success_ts BIGINT,
            last_failure_ts BIGINT,
            average_response_time_ms DOUBLE PRECISION NOT NULL DEFAULT 0,
            error_rate DOUBLE PRECISION NOT NULL DEFAULT 0,
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT NOT NULL
        )
        "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create federation_access_stats table");

    sqlx::query(
        r#"
        CREATE TABLE federation_blacklist_rule (
            id BIGSERIAL PRIMARY KEY,
            rule_name TEXT NOT NULL,
            rule_type TEXT NOT NULL,
            pattern TEXT NOT NULL,
            action TEXT NOT NULL,
            priority INTEGER NOT NULL DEFAULT 0,
            is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
            description TEXT,
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT NOT NULL,
            created_by TEXT NOT NULL
        )
        "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create federation_blacklist_rule table");

    Some(pool)
}

fn create_storage(pool: &Arc<Pool<Postgres>>) -> FederationBlacklistStorage {
    FederationBlacklistStorage::new(pool)
}

async fn insert_blacklist_entry(
    pool: &Pool<Postgres>,
    server_name: &str,
    block_type: &str,
    reason: Option<&str>,
    blocked_by: &str,
    is_enabled: bool,
    expires_at: Option<i64>,
) {
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        r#"
        INSERT INTO federation_blacklist (
            server_name, block_type, reason, blocked_by, added_by, created_ts, added_ts, updated_ts, expires_at, is_enabled, metadata
        )
        VALUES ($1, $2, $3, $4, $4, $5, $5, $5, $6, $7, '{}')
        ON CONFLICT (server_name) DO UPDATE SET
            block_type = $2, reason = $3, blocked_by = $4, added_by = $4,
            created_ts = $5, added_ts = $5, updated_ts = $5, expires_at = $6, is_enabled = $7
        "#,
    )
    .bind(server_name)
    .bind(block_type)
    .bind(reason)
    .bind(blocked_by)
    .bind(now)
    .bind(expires_at)
    .bind(is_enabled)
    .execute(pool)
    .await
    .expect("Failed to insert blacklist entry");
}

#[test]
fn test_encode_cursor_round_trip() {
    let cursor =
        FederationBlacklistCursor { created_ts: 1_746_700_000_000, server_name: "evil.example.com".to_string() };
    let encoded = encode_federation_blacklist_cursor(&cursor);
    let decoded = decode_federation_blacklist_cursor(Some(&encoded));
    assert_eq!(decoded, Some(cursor));
}

#[test]
fn test_encode_cursor_different_values() {
    let cursor = FederationBlacklistCursor { created_ts: 0, server_name: "a".to_string() };
    let encoded = encode_federation_blacklist_cursor(&cursor);
    let decoded = decode_federation_blacklist_cursor(Some(&encoded));
    assert_eq!(decoded, Some(cursor));
}

#[test]
fn test_decode_cursor_none_input() {
    assert_eq!(decode_federation_blacklist_cursor(None), None);
}

#[test]
fn test_decode_cursor_no_pipe() {
    assert_eq!(decode_federation_blacklist_cursor(Some("nodelimiter")), None);
}

#[test]
fn test_decode_cursor_empty_server_name() {
    assert_eq!(decode_federation_blacklist_cursor(Some("123|")), None);
}

#[test]
fn test_decode_cursor_non_numeric_ts() {
    assert_eq!(decode_federation_blacklist_cursor(Some("abc|server.com")), None);
}

#[test]
fn test_decode_cursor_valid_format() {
    let result = decode_federation_blacklist_cursor(Some("9999|matrix.org"));
    assert!(result.is_some());
    let cursor = result.unwrap();
    assert_eq!(cursor.created_ts, 9999);
    assert_eq!(cursor.server_name, "matrix.org");
}

#[test]
fn test_add_to_blacklist() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let server_name = format!("evil-{suffix}.example.com");

        let request = AddBlacklistRequest {
            server_name: server_name.clone(),
            block_type: "blacklist".to_string(),
            reason: Some("Malicious activity".to_string()),
            blocked_by: "@admin:example.com".to_string(),
            expires_at: None,
            metadata: None,
        };

        let result = storage.add_to_blacklist(request).await.unwrap();
        assert_eq!(result.server_name, server_name);
        assert_eq!(result.block_type, "blacklist");
        assert!(result.is_enabled);
    });
}

#[test]
fn test_add_to_blacklist_with_metadata() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let server_name = format!("meta-{suffix}.example.com");

        let request = AddBlacklistRequest {
            server_name: server_name.clone(),
            block_type: "blacklist".to_string(),
            reason: None,
            blocked_by: "@admin:example.com".to_string(),
            expires_at: Some(chrono::Utc::now().timestamp_millis() + 86_400_000),
            metadata: Some(serde_json::json!({"source": "automated"})),
        };

        let result = storage.add_to_blacklist(request).await.unwrap();
        assert_eq!(result.server_name, server_name);
        assert!(result.expires_at.is_some());
    });
}

#[test]
fn test_add_to_blacklist_upsert() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let server_name = format!("upsert-{suffix}.example.com");

        let request1 = AddBlacklistRequest {
            server_name: server_name.clone(),
            block_type: "blacklist".to_string(),
            reason: Some("Initial block".to_string()),
            blocked_by: "@admin:example.com".to_string(),
            expires_at: None,
            metadata: None,
        };

        let result1 = storage.add_to_blacklist(request1).await.unwrap();
        assert_eq!(result1.reason, Some("Initial block".to_string()));

        let request2 = AddBlacklistRequest {
            server_name: server_name.clone(),
            block_type: "whitelist".to_string(),
            reason: Some("Updated reason".to_string()),
            blocked_by: "@mod:example.com".to_string(),
            expires_at: None,
            metadata: None,
        };

        let result2 = storage.add_to_blacklist(request2).await.unwrap();
        assert_eq!(result2.reason, Some("Updated reason".to_string()));
        assert_eq!(result2.id, result1.id);
    });
}

#[test]
fn test_remove_from_blacklist() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let server_name = format!("remove-{suffix}.example.com");

        insert_blacklist_entry(&pool, &server_name, "blacklist", Some("Spam"), "@admin:example.com", true, None).await;

        storage.remove_from_blacklist(&server_name, "@admin:example.com").await.unwrap();

        let row = sqlx::query_scalar::<_, bool>("SELECT is_enabled FROM federation_blacklist WHERE server_name = $1")
            .bind(&server_name)
            .fetch_one(&*pool)
            .await
            .unwrap();
        assert!(!row);
    });
}

#[test]
fn test_remove_from_blacklist_creates_log() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let server_name = format!("removelog-{suffix}.example.com");

        insert_blacklist_entry(&pool, &server_name, "blacklist", Some("Test"), "@admin:example.com", true, None).await;

        storage.remove_from_blacklist(&server_name, "@admin:example.com").await.unwrap();

        let log_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM federation_blacklist_log WHERE server_name = $1 AND action = 'remove'",
        )
        .bind(&server_name)
        .fetch_one(&*pool)
        .await
        .unwrap();
        assert_eq!(log_count, 1);
    });
}

#[test]
fn test_get_blacklist_entry_found() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let server_name = format!("getfound-{suffix}.example.com");

        insert_blacklist_entry(&pool, &server_name, "blacklist", Some("Bad actor"), "@admin:example.com", true, None)
            .await;

        let result = storage.get_blacklist_entry(&server_name).await.unwrap();
        assert!(result.is_some());
        let entry = result.unwrap();
        assert_eq!(entry.server_name, server_name);
    });
}

#[test]
fn test_get_blacklist_entry_not_found() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();

        let result = storage.get_blacklist_entry(&format!("nonexistent-{suffix}.example.com")).await.unwrap();
        assert!(result.is_none());
    });
}

#[test]
fn test_is_server_blocked_true() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let server_name = format!("blocked-{suffix}.example.com");

        insert_blacklist_entry(&pool, &server_name, "blacklist", Some("Spam"), "@admin:example.com", true, None).await;

        let result = storage.is_server_blocked(&server_name).await.unwrap();
        assert!(result);
    });
}

#[test]
fn test_is_server_blocked_false_not_in_list() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();

        let result = storage.is_server_blocked(&format!("clean-{suffix}.example.com")).await.unwrap();
        assert!(!result);
    });
}

#[test]
fn test_is_server_blocked_false_whitelist_type() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let server_name = format!("whitelisted-{suffix}.example.com");

        insert_blacklist_entry(&pool, &server_name, "whitelist", None, "@admin:example.com", true, None).await;

        let result = storage.is_server_blocked(&server_name).await.unwrap();
        assert!(!result);
    });
}

#[test]
fn test_is_server_blocked_expired() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let server_name = format!("expired-{suffix}.example.com");

        let past_ts = chrono::Utc::now().timestamp_millis() - 86_400_000;

        insert_blacklist_entry(
            &pool,
            &server_name,
            "blacklist",
            Some("Temporary"),
            "@admin:example.com",
            true,
            Some(past_ts),
        )
        .await;

        let result = storage.is_server_blocked(&server_name).await.unwrap();
        assert!(!result);
    });
}

#[test]
fn test_is_server_whitelisted_true() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let server_name = format!("wl-true-{suffix}.example.com");

        insert_blacklist_entry(&pool, &server_name, "whitelist", None, "@admin:example.com", true, None).await;

        let result = storage.is_server_whitelisted(&server_name).await.unwrap();
        assert!(result);
    });
}

#[test]
fn test_is_server_whitelisted_false_not_in_list() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();

        let result = storage.is_server_whitelisted(&format!("notwl-{suffix}.example.com")).await.unwrap();
        assert!(!result);
    });
}

#[test]
fn test_is_server_whitelisted_false_blacklist_type() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let server_name = format!("wl-bl-{suffix}.example.com");

        insert_blacklist_entry(&pool, &server_name, "blacklist", Some("Blocked"), "@admin:example.com", true, None)
            .await;

        let result = storage.is_server_whitelisted(&server_name).await.unwrap();
        assert!(!result);
    });
}

#[test]
fn test_is_server_whitelisted_false_disabled() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let server_name = format!("wl-disabled-{suffix}.example.com");

        insert_blacklist_entry(&pool, &server_name, "whitelist", None, "@admin:example.com", false, None).await;

        let result = storage.is_server_whitelisted(&server_name).await.unwrap();
        assert!(!result);
    });
}

#[test]
fn test_get_all_blacklist_empty() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);

        let (entries, next_batch) = storage.get_all_blacklist(10, None).await.unwrap();
        assert!(entries.is_empty());
        assert!(next_batch.is_none());
    });
}

#[test]
fn test_get_all_blacklist_with_entries() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();

        for i in 0..3 {
            let server_name = format!("list-{suffix}-{i}.example.com");
            insert_blacklist_entry(&pool, &server_name, "blacklist", Some("Test"), "@admin:example.com", true, None)
                .await;
        }

        let (entries, next_batch) = storage.get_all_blacklist(10, None).await.unwrap();
        assert_eq!(entries.len(), 3);
        assert!(next_batch.is_none());
    });
}

#[test]
fn test_get_all_blacklist_pagination() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();

        for i in 0..5 {
            let server_name = format!("page-{suffix}-{i}.example.com");
            insert_blacklist_entry(&pool, &server_name, "blacklist", Some("Test"), "@admin:example.com", true, None)
                .await;
        }

        let (page1, next_batch) = storage.get_all_blacklist(2, None).await.unwrap();
        assert_eq!(page1.len(), 2);
        assert!(next_batch.is_some());

        let cursor = decode_federation_blacklist_cursor(next_batch.as_deref()).unwrap();
        let (page2, next_batch2) = storage.get_all_blacklist(2, Some(cursor)).await.unwrap();
        assert_eq!(page2.len(), 2);
        assert!(next_batch2.is_some());

        let cursor2 = decode_federation_blacklist_cursor(next_batch2.as_deref()).unwrap();
        let (page3, next_batch3) = storage.get_all_blacklist(2, Some(cursor2)).await.unwrap();
        assert_eq!(page3.len(), 1);
        assert!(next_batch3.is_none());
    });
}

#[test]
fn test_create_log() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let server_name = format!("log-{suffix}.example.com");

        let request = CreateLogRequest {
            server_name: server_name.clone(),
            action: "add".to_string(),
            old_status: None,
            new_status: Some("blocked".to_string()),
            reason: Some("Spam detected".to_string()),
            performed_by: "@admin:example.com".to_string(),
            ip_address: Some("192.168.1.1".to_string()),
            user_agent: Some("Mozilla/5.0".to_string()),
            metadata: Some(serde_json::json!({"auto": true})),
        };

        let result = storage.create_log(request).await.unwrap();
        assert_eq!(result.server_name, server_name);
        assert_eq!(result.action, "add");
        assert_eq!(result.old_status, None);
        assert_eq!(result.new_status, Some("blocked".to_string()));
        assert_eq!(result.reason, Some("Spam detected".to_string()));
        assert_eq!(result.performed_by, "@admin:example.com");
        assert_eq!(result.ip_address, Some("192.168.1.1".to_string()));
    });
}

#[test]
fn test_create_log_minimal() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let server_name = format!("logmin-{suffix}.example.com");

        let request = CreateLogRequest {
            server_name: server_name.clone(),
            action: "remove".to_string(),
            old_status: Some("blocked".to_string()),
            new_status: Some("unblocked".to_string()),
            reason: None,
            performed_by: "@mod:example.com".to_string(),
            ip_address: None,
            user_agent: None,
            metadata: None,
        };

        let result = storage.create_log(request).await.unwrap();
        assert_eq!(result.server_name, server_name);
        assert_eq!(result.action, "remove");
        assert_eq!(result.ip_address, None);
        assert_eq!(result.user_agent, None);
    });
}

#[test]
fn test_update_access_stats_first_success() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let server_name = format!("stats-ok-{suffix}.example.com");

        let request =
            UpdateStatsRequest { server_name: server_name.clone(), is_success: true, response_time_ms: Some(50.0) };

        let result = storage.update_access_stats(request).await.unwrap();
        assert_eq!(result.server_name, server_name);
        assert_eq!(result.total_requests, 1);
        assert_eq!(result.successful_requests, 1);
        assert_eq!(result.failed_requests, 0);
        assert!(result.last_success_ts.is_some());
        assert!(result.last_failure_ts.is_none());
    });
}

#[test]
fn test_update_access_stats_first_failure() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let server_name = format!("stats-fail-{suffix}.example.com");

        let request =
            UpdateStatsRequest { server_name: server_name.clone(), is_success: false, response_time_ms: None };

        let result = storage.update_access_stats(request).await.unwrap();
        assert_eq!(result.total_requests, 1);
        assert_eq!(result.successful_requests, 0);
        assert_eq!(result.failed_requests, 1);
        assert!(result.last_success_ts.is_none());
        assert!(result.last_failure_ts.is_some());
    });
}

#[test]
fn test_update_access_stats_accumulates() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let server_name = format!("stats-acc-{suffix}.example.com");

        storage
            .update_access_stats(UpdateStatsRequest {
                server_name: server_name.clone(),
                is_success: true,
                response_time_ms: Some(100.0),
            })
            .await
            .unwrap();

        storage
            .update_access_stats(UpdateStatsRequest {
                server_name: server_name.clone(),
                is_success: true,
                response_time_ms: Some(200.0),
            })
            .await
            .unwrap();

        storage
            .update_access_stats(UpdateStatsRequest {
                server_name: server_name.clone(),
                is_success: false,
                response_time_ms: None,
            })
            .await
            .unwrap();

        let result = storage.get_access_stats(&server_name).await.unwrap();
        assert!(result.is_some());
        let stats = result.unwrap();
        assert_eq!(stats.total_requests, 3);
        assert_eq!(stats.successful_requests, 2);
        assert_eq!(stats.failed_requests, 1);
    });
}

#[test]
fn test_get_access_stats_not_found() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();

        let result = storage.get_access_stats(&format!("nostats-{suffix}.example.com")).await.unwrap();
        assert!(result.is_none());
    });
}

#[test]
fn test_create_rule() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();

        let request = CreateRuleRequest {
            rule_name: format!("block-malware-{suffix}"),
            rule_type: "domain".to_string(),
            pattern: "*.evil.com".to_string(),
            action: "block".to_string(),
            priority: 100,
            description: Some("Block malware domains".to_string()),
            created_by: "@admin:example.com".to_string(),
        };

        let result = storage.create_rule(request).await.unwrap();
        assert_eq!(result.rule_name, format!("block-malware-{suffix}"));
        assert_eq!(result.rule_type, "domain");
        assert_eq!(result.pattern, "*.evil.com");
        assert_eq!(result.action, "block");
        assert_eq!(result.priority, 100);
        assert_eq!(result.description, Some("Block malware domains".to_string()));
        assert_eq!(result.created_by, "@admin:example.com");
        assert!(result.is_enabled);
    });
}

#[test]
fn test_create_rule_minimal() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();

        let request = CreateRuleRequest {
            rule_name: format!("basic-rule-{suffix}"),
            rule_type: "ip".to_string(),
            pattern: "10.0.0.0/8".to_string(),
            action: "allow".to_string(),
            priority: 50,
            description: None,
            created_by: "@admin:example.com".to_string(),
        };

        let result = storage.create_rule(request).await.unwrap();
        assert_eq!(result.rule_type, "ip");
        assert_eq!(result.description, None);
    });
}

#[test]
fn test_get_all_rules_empty() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);

        let rules = storage.get_all_rules().await.unwrap();
        assert!(rules.is_empty());
    });
}

#[test]
fn test_get_all_rules_returns_enabled_only() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();

        storage
            .create_rule(CreateRuleRequest {
                rule_name: format!("enabled-rule-{suffix}"),
                rule_type: "domain".to_string(),
                pattern: "*.bad.com".to_string(),
                action: "block".to_string(),
                priority: 100,
                description: None,
                created_by: "@admin:example.com".to_string(),
            })
            .await
            .unwrap();

        storage
            .create_rule(CreateRuleRequest {
                rule_name: format!("another-rule-{suffix}"),
                rule_type: "domain".to_string(),
                pattern: "*.worse.com".to_string(),
                action: "block".to_string(),
                priority: 50,
                description: None,
                created_by: "@admin:example.com".to_string(),
            })
            .await
            .unwrap();

        sqlx::query("UPDATE federation_blacklist_rule SET is_enabled = false WHERE rule_name = $1")
            .bind(format!("another-rule-{suffix}"))
            .execute(&*pool)
            .await
            .unwrap();

        let rules = storage.get_all_rules().await.unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].rule_name, format!("enabled-rule-{suffix}"));
    });
}

#[test]
fn test_get_all_rules_ordered_by_priority() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();

        storage
            .create_rule(CreateRuleRequest {
                rule_name: format!("low-priority-{suffix}"),
                rule_type: "domain".to_string(),
                pattern: "*.low.com".to_string(),
                action: "block".to_string(),
                priority: 10,
                description: None,
                created_by: "@admin:example.com".to_string(),
            })
            .await
            .unwrap();

        storage
            .create_rule(CreateRuleRequest {
                rule_name: format!("high-priority-{suffix}"),
                rule_type: "domain".to_string(),
                pattern: "*.high.com".to_string(),
                action: "block".to_string(),
                priority: 200,
                description: None,
                created_by: "@admin:example.com".to_string(),
            })
            .await
            .unwrap();

        let rules = storage.get_all_rules().await.unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].priority, 200);
        assert_eq!(rules[1].priority, 10);
    });
}

#[test]
fn test_cleanup_expired_entries() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();

        let past_ts = chrono::Utc::now().timestamp_millis() - 86_400_000;

        insert_blacklist_entry(
            &pool,
            &format!("expired-cleanup-{suffix}.example.com"),
            "blacklist",
            Some("Expired"),
            "@admin:example.com",
            true,
            Some(past_ts),
        )
        .await;

        insert_blacklist_entry(
            &pool,
            &format!("active-cleanup-{suffix}.example.com"),
            "blacklist",
            Some("Still active"),
            "@admin:example.com",
            true,
            None,
        )
        .await;

        let cleaned = storage.cleanup_expired_entries().await.unwrap();
        assert_eq!(cleaned, 1);

        let active_count =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM federation_blacklist WHERE is_enabled = true")
                .fetch_one(&*pool)
                .await
                .unwrap();
        assert_eq!(active_count, 1);
    });
}

#[test]
fn test_cleanup_expired_entries_none_expired() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();

        let future_ts = chrono::Utc::now().timestamp_millis() + 86_400_000;

        insert_blacklist_entry(
            &pool,
            &format!("future-cleanup-{suffix}.example.com"),
            "blacklist",
            Some("Not yet expired"),
            "@admin:example.com",
            true,
            Some(future_ts),
        )
        .await;

        let cleaned = storage.cleanup_expired_entries().await.unwrap();
        assert_eq!(cleaned, 0);
    });
}

#[test]
fn test_get_config_returns_none() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);

        let result = storage.get_config("any_key").unwrap();
        assert_eq!(result, None);
    });
}

#[test]
fn test_get_config_as_bool_default_true() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);

        let result = storage.get_config_as_bool("any_key", true).unwrap();
        assert!(result);
    });
}

#[test]
fn test_get_config_as_bool_default_false() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);

        let result = storage.get_config_as_bool("any_key", false).unwrap();
        assert!(!result);
    });
}

#[test]
fn test_get_config_as_int_default() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);

        let result = storage.get_config_as_int("any_key", 42).unwrap();
        assert_eq!(result, 42);
    });
}

#[test]
fn test_get_config_as_int_default_zero() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);

        let result = storage.get_config_as_int("any_key", 0).unwrap();
        assert_eq!(result, 0);
    });
}
