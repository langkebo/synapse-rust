//! Integration tests for `ModuleStorage` at `synapse-storage/src/module.rs`.
//!
//! Covers all 27 public `async fn` methods of `ModuleStorage`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use synapse_storage::module::{
    CreateAccountDataCallbackRequest, CreateAccountValidityRequest, CreateExecutionLogRequest,
    CreateMediaCallbackRequest, CreateModuleRequest, CreatePasswordAuthProviderRequest,
    CreateSpamCheckRequest, CreateThirdPartyRuleRequest, ModuleStorage,
};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn module_test_guard() -> &'static Mutex<()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD.get_or_init(|| Mutex::new(()))
}

/// Warm up the shared pool on the current tokio runtime.
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

/// Clean ALL tables used by module.rs in dependency order.
async fn setup(pool: &Arc<sqlx::PgPool>) {
    warm_up_pool(pool).await;
    // Child tables first to respect FK constraints.
    sqlx::query("DELETE FROM module_execution_logs").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM spam_check_results").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM third_party_rule_results").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM account_data_callbacks").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM media_callbacks").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM account_validity").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM modules").execute(pool.as_ref()).await.ok();
}

fn make_module_request(name: &str, module_type: &str) -> CreateModuleRequest {
    CreateModuleRequest {
        module_name: name.to_string(),
        module_type: module_type.to_string(),
        version: "1.0.0".to_string(),
        description: Some("test module".to_string()),
        is_enabled: Some(true),
        priority: Some(100),
        config: Some(serde_json::json!({"key": "value"})),
    }
}

// =============================================================================
// new / register_module
// =============================================================================

#[tokio::test]
async fn test_new() {
    let _guard = module_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = ModuleStorage::new(&pool);
    // A trivial operation proves the storage was constructed with a usable pool.
    let _ = storage.get_all_modules(1, None).await.unwrap();
}

#[tokio::test]
async fn test_register_module_basic_and_defaults() {
    let _guard = module_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = ModuleStorage::new(&pool);

    let name = format!("mod_basic_{}", unique_id());
    let request = CreateModuleRequest {
        module_name: name.clone(),
        module_type: "spam_checker".to_string(),
        version: "1.0.0".to_string(),
        description: None,
        is_enabled: None,
        priority: None,
        config: None,
    };
    let module = storage.register_module(request).await.unwrap();

    assert_eq!(module.module_name, name);
    assert_eq!(module.module_type, "spam_checker");
    assert_eq!(module.version, "1.0.0");
    // Defaults from register_module: is_enabled=true, priority=100.
    assert!(module.is_enabled);
    assert_eq!(module.priority, 100);
    assert_eq!(module.execution_count, 0);
    assert_eq!(module.error_count, 0);
    assert!(module.last_executed_ts.is_none());
    assert!(module.last_error.is_none());
    assert!(module.description.is_none());
}

#[tokio::test]
async fn test_register_module_with_all_fields() {
    let _guard = module_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = ModuleStorage::new(&pool);

    let name = format!("mod_full_{}", unique_id());
    let request = CreateModuleRequest {
        module_name: name.clone(),
        module_type: "third_party_rules".to_string(),
        version: "2.3.4".to_string(),
        description: Some("a description".to_string()),
        is_enabled: Some(false),
        priority: Some(7),
        config: Some(serde_json::json!({"setting": 42})),
    };
    let module = storage.register_module(request).await.unwrap();

    assert_eq!(module.module_name, name);
    assert_eq!(module.version, "2.3.4");
    assert_eq!(module.description.as_deref(), Some("a description"));
    assert!(!module.is_enabled);
    assert_eq!(module.priority, 7);
    assert_eq!(module.config, Some(serde_json::json!({"setting": 42})));
}

// =============================================================================
// get_module
// =============================================================================

#[tokio::test]
async fn test_get_module_existing() {
    let _guard = module_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = ModuleStorage::new(&pool);

    let name = format!("mod_get_{}", unique_id());
    let module = storage.register_module(make_module_request(&name, "spam_checker")).await.unwrap();

    let fetched = storage.get_module(&name).await.unwrap().expect("module should exist");
    assert_eq!(fetched.id, module.id);
    assert_eq!(fetched.module_name, name);
    assert_eq!(fetched.module_type, "spam_checker");
    assert_eq!(fetched.priority, 100);
}

#[tokio::test]
async fn test_get_module_nonexistent() {
    let _guard = module_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = ModuleStorage::new(&pool);

    let result = storage.get_module(&format!("nope_{}", unique_id())).await.unwrap();
    assert!(result.is_none());
}

// =============================================================================
// get_modules_by_type
// =============================================================================

#[tokio::test]
async fn test_get_modules_by_type_filters_and_excludes_disabled() {
    let _guard = module_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = ModuleStorage::new(&pool);

    let target_type = format!("type_target_{}", unique_id());

    // Two enabled modules of the target type with different priorities.
    let mut req_a = make_module_request(&format!("a_{}", unique_id()), &target_type);
    req_a.priority = Some(20);
    let mut req_b = make_module_request(&format!("b_{}", unique_id()), &target_type);
    req_b.priority = Some(10);
    storage.register_module(req_a).await.unwrap();
    storage.register_module(req_b).await.unwrap();

    // A disabled module of the same type — must be excluded.
    let mut req_disabled = make_module_request(&format!("d_{}", unique_id()), &target_type);
    req_disabled.is_enabled = Some(false);
    storage.register_module(req_disabled).await.unwrap();

    // A different type — must be excluded.
    storage
        .register_module(make_module_request(&format!("o_{}", unique_id()), "other_type"))
        .await
        .unwrap();

    let results = storage.get_modules_by_type(&target_type).await.unwrap();
    assert_eq!(results.len(), 2, "only enabled modules of the target type should be returned");
    // Ordered by priority ASC.
    assert!(results[0].priority <= results[1].priority);
    assert!(results.iter().all(|m| m.module_type == target_type && m.is_enabled));
}

// =============================================================================
// get_all_modules
// =============================================================================

#[tokio::test]
async fn test_get_all_modules_first_page_no_cursor() {
    let _guard = module_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = ModuleStorage::new(&pool);

    let uid = unique_id();
    let t = format!("page_{}", uid);
    for i in 0..3 {
        let mut req = make_module_request(&format!("m{i}_{uid}"), &t);
        req.priority = Some((i + 1) * 10);
        storage.register_module(req).await.unwrap();
    }

    let (rows, next_from) = storage.get_all_modules(2, None).await.unwrap();
    assert_eq!(rows.len(), 2, "limit should be respected on first page");
    // Ordered by module_type ASC, priority ASC, module_name ASC.
    assert!(rows[0].priority <= rows[1].priority);
    // Full page -> next_from should be Some.
    assert!(next_from.is_some(), "next cursor should be present when page is full");
}

#[tokio::test]
async fn test_get_all_modules_pagination() {
    let _guard = module_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = ModuleStorage::new(&pool);

    let uid = unique_id();
    let t = format!("pg_{}", uid);
    for i in 0..3 {
        let mut req = make_module_request(&format!("p{i}_{uid}"), &t);
        req.priority = Some((i + 1) * 10);
        storage.register_module(req).await.unwrap();
    }

    // Page 1: limit=2.
    let (page1, next1) = storage.get_all_modules(2, None).await.unwrap();
    assert_eq!(page1.len(), 2);
    let cursor1 = next1.expect("first page should yield a cursor");

    // Page 2: continue from cursor.
    let (page2, next2) = storage.get_all_modules(2, Some(cursor1)).await.unwrap();
    assert_eq!(page2.len(), 1, "only one remaining module");
    assert!(next2.is_none(), "no further pages");

    // No overlap between pages.
    assert_ne!(page1[0].id, page2[0].id);
    assert_ne!(page1[1].id, page2[0].id);
}

// =============================================================================
// update_module_config
// =============================================================================

#[tokio::test]
async fn test_update_module_config() {
    let _guard = module_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = ModuleStorage::new(&pool);

    let name = format!("mod_cfg_{}", unique_id());
    storage.register_module(make_module_request(&name, "spam_checker")).await.unwrap();

    let new_config = serde_json::json!({"updated": true, "n": 99});
    let updated = storage.update_module_config(&name, new_config.clone()).await.unwrap();
    assert_eq!(updated.config, Some(new_config.clone()));

    // Persisted.
    let fetched = storage.get_module(&name).await.unwrap().unwrap();
    assert_eq!(fetched.config, Some(new_config));
}

// =============================================================================
// enable_module
// =============================================================================

#[tokio::test]
async fn test_enable_module_toggle() {
    let _guard = module_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = ModuleStorage::new(&pool);

    let name = format!("mod_toggle_{}", unique_id());
    storage.register_module(make_module_request(&name, "spam_checker")).await.unwrap();
    assert!(storage.get_module(&name).await.unwrap().unwrap().is_enabled);

    // Disable.
    let disabled = storage.enable_module(&name, false).await.unwrap();
    assert!(!disabled.is_enabled);
    assert!(!storage.get_module(&name).await.unwrap().unwrap().is_enabled);

    // Re-enable.
    let enabled = storage.enable_module(&name, true).await.unwrap();
    assert!(enabled.is_enabled);
}

// =============================================================================
// delete_module
// =============================================================================

#[tokio::test]
async fn test_delete_module() {
    let _guard = module_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = ModuleStorage::new(&pool);

    let name = format!("mod_del_{}", unique_id());
    storage.register_module(make_module_request(&name, "spam_checker")).await.unwrap();
    assert!(storage.get_module(&name).await.unwrap().is_some());

    storage.delete_module(&name).await.unwrap();
    assert!(storage.get_module(&name).await.unwrap().is_none());
}

// =============================================================================
// record_execution
// =============================================================================

#[tokio::test]
async fn test_record_execution_success() {
    let _guard = module_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = ModuleStorage::new(&pool);

    let name = format!("mod_exec_ok_{}", unique_id());
    storage.register_module(make_module_request(&name, "spam_checker")).await.unwrap();

    storage.record_execution(&name, true, None).await.unwrap();

    let module = storage.get_module(&name).await.unwrap().unwrap();
    assert_eq!(module.execution_count, 1);
    assert_eq!(module.error_count, 0, "success must not increment error_count");
    assert!(module.last_executed_ts.is_some());
    assert!(module.last_error.is_none());
}

#[tokio::test]
async fn test_record_execution_failure_increments_error_count() {
    let _guard = module_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = ModuleStorage::new(&pool);

    let name = format!("mod_exec_fail_{}", unique_id());
    storage.register_module(make_module_request(&name, "spam_checker")).await.unwrap();

    storage.record_execution(&name, false, Some("boom")).await.unwrap();

    let module = storage.get_module(&name).await.unwrap().unwrap();
    assert_eq!(module.execution_count, 1);
    assert_eq!(module.error_count, 1);
    assert_eq!(module.last_error.as_deref(), Some("boom"));
}

#[tokio::test]
async fn test_record_execution_multiple_increments() {
    let _guard = module_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = ModuleStorage::new(&pool);

    let name = format!("mod_exec_multi_{}", unique_id());
    storage.register_module(make_module_request(&name, "spam_checker")).await.unwrap();

    // 2 successes + 2 failures.
    storage.record_execution(&name, true, None).await.unwrap();
    storage.record_execution(&name, true, None).await.unwrap();
    storage.record_execution(&name, false, Some("e1")).await.unwrap();
    storage.record_execution(&name, false, Some("e2")).await.unwrap();

    let module = storage.get_module(&name).await.unwrap().unwrap();
    assert_eq!(module.execution_count, 4);
    assert_eq!(module.error_count, 2);
    assert_eq!(module.last_error.as_deref(), Some("e2"));
    assert!(module.last_executed_ts.is_some());
}

// =============================================================================
// create_spam_check_result / get_spam_check_result / get_spam_check_results_by_sender
// =============================================================================

#[tokio::test]
async fn test_create_spam_check_result() {
    let _guard = module_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = ModuleStorage::new(&pool);

    let request = CreateSpamCheckRequest {
        event_id: format!("$evt_{}", unique_id()),
        room_id: format!("!room_{}", unique_id()),
        sender: format!("@s_{}", unique_id()),
        event_type: "m.room.message".to_string(),
        content: serde_json::json!({"body": "hi"}),
        result: "allow".to_string(),
        score: Some(0),
        reason: None,
        checker_module: "checker".to_string(),
        action_taken: None,
    };
    let result = storage.create_spam_check_result(request).await;
    assert!(result.is_ok(), "create_spam_check_result should succeed: {result:?}");
    let row = result.unwrap();
    assert_eq!(row.result, "allow");
    assert_eq!(row.score, 0);
    assert_eq!(row.checker_module, "checker");
}

#[tokio::test]
async fn test_get_spam_check_result_none_and_existing() {
    let _guard = module_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = ModuleStorage::new(&pool);

    let missing = format!("$missing_{}", unique_id());
    assert!(storage.get_spam_check_result(&missing).await.unwrap().is_none());

    // Populate a row directly to test the getter independently.
    let event_id = format!("$evt_{}", unique_id());
    let sender = format!("@s_{}", unique_id());
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        r"
        INSERT INTO spam_check_results
            (event_id, room_id, sender, event_type, content, result, score,
             reason, checker_module, checked_ts, action_taken, created_ts)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        ",
    )
    .bind(&event_id)
    .bind("!room")
    .bind(&sender)
    .bind("m.room.message")
    .bind(serde_json::json!({"body": "hi"}))
    .bind("allow")
    .bind(0)
    .bind(None::<&str>)
    .bind("checker")
    .bind(now)
    .bind(None::<&str>)
    .bind(now)
    .execute(pool.as_ref())
    .await
    .unwrap();

    let fetched = storage
        .get_spam_check_result(&event_id)
        .await
        .unwrap()
        .expect("row should be retrievable");
    assert_eq!(fetched.event_id, event_id);
    assert_eq!(fetched.sender, sender);
    assert_eq!(fetched.result, "allow");
    assert_eq!(fetched.score, 0);
}

#[tokio::test]
async fn test_get_spam_check_results_by_sender_ordering_limit() {
    let _guard = module_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = ModuleStorage::new(&pool);

    let sender = format!("@s_{}", unique_id());
    let now = chrono::Utc::now().timestamp_millis();
    // Insert 3 rows with ascending checked_ts; expect DESC ordering and limit.
    for i in 0..3 {
        let ts = now + i; // ascending insert order
        let event_id = format!("$evt_{i}_{}", unique_id());
        sqlx::query(
            r"
            INSERT INTO spam_check_results
                (event_id, room_id, sender, event_type, content, result, score,
                 reason, checker_module, checked_ts, action_taken, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            ",
        )
        .bind(&event_id)
        .bind("!room")
        .bind(&sender)
        .bind("m.room.message")
        .bind(serde_json::json!({}))
        .bind("allow")
        .bind(0)
        .bind(None::<&str>)
        .bind("checker")
        .bind(ts)
        .bind(None::<&str>)
        .bind(ts)
        .execute(pool.as_ref())
        .await
        .unwrap();
    }

    let results = storage.get_spam_check_results_by_sender(&sender, 2).await.unwrap();
    assert_eq!(results.len(), 2, "limit should be respected");
    // DESC by checked_ts then id DESC.
    assert!(results[0].checked_ts >= results[1].checked_ts);
    assert!(results.iter().all(|r| r.sender == sender));

    // A different sender returns nothing.
    let other = storage.get_spam_check_results_by_sender("@nobody", 10).await.unwrap();
    assert!(other.is_empty());
}

// =============================================================================
// create_third_party_rule_result / get_third_party_rule_results
// =============================================================================

#[tokio::test]
async fn test_create_third_party_rule_result() {
    let _guard = module_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = ModuleStorage::new(&pool);

    let request = CreateThirdPartyRuleRequest {
        event_id: format!("$evt_{}", unique_id()),
        room_id: format!("!room_{}", unique_id()),
        sender: format!("@s_{}", unique_id()),
        event_type: "m.room.message".to_string(),
        rule_name: "rule1".to_string(),
        is_allowed: true,
        reason: None,
        modified_content: None,
    };
    let result = storage.create_third_party_rule_result(request).await;
    assert!(result.is_ok(), "create_third_party_rule_result should succeed: {result:?}");
    let row = result.unwrap();
    assert_eq!(row.rule_name, "rule1");
    assert!(row.is_allowed);
}

#[tokio::test]
async fn test_get_third_party_rule_results_empty_and_existing() {
    let _guard = module_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = ModuleStorage::new(&pool);

    let missing = format!("$missing_{}", unique_id());
    assert!(storage.get_third_party_rule_results(&missing).await.unwrap().is_empty());

    // Populate a row directly to test the getter independently.
    let event_id = format!("$evt_{}", unique_id());
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        r"
        INSERT INTO third_party_rule_results
            (event_id, room_id, sender, event_type, rule_name, is_allowed,
             reason, modified_content, checked_ts, created_ts)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        ",
    )
    .bind(&event_id)
    .bind("!room")
    .bind("@s")
    .bind("m.room.message")
    .bind("rule1")
    .bind(true)
    .bind(None::<&str>)
    .bind(None::<serde_json::Value>)
    .bind(now)
    .bind(now)
    .execute(pool.as_ref())
    .await
    .unwrap();

    let results = storage.get_third_party_rule_results(&event_id).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].event_id, event_id);
    assert_eq!(results[0].rule_name, "rule1");
    assert!(results[0].is_allowed);
}

// =============================================================================
// create_execution_log / get_execution_logs
// =============================================================================

#[tokio::test]
async fn test_create_execution_log() {
    let _guard = module_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = ModuleStorage::new(&pool);

    let module_name = format!("log_mod_{}", unique_id());
    let request = CreateExecutionLogRequest {
        module_name: module_name.clone(),
        module_type: "spam_checker".to_string(),
        event_id: Some(format!("$evt_{}", unique_id())),
        room_id: Some(format!("!room_{}", unique_id())),
        execution_time_ms: 42,
        is_success: true,
        error_message: None,
        metadata: Some(serde_json::json!({"k": "v"})),
    };
    let log = storage.create_execution_log(request).await.unwrap();

    assert_eq!(log.module_name, module_name);
    assert_eq!(log.module_type, "spam_checker");
    assert!(log.is_success);
    assert_eq!(log.execution_time_ms, 42);
    assert!(log.event_id.is_some());
    assert!(log.metadata.is_some());
    assert!(log.executed_ts > 0);
}

#[tokio::test]
async fn test_get_execution_logs_ordering_and_limit() {
    let _guard = module_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = ModuleStorage::new(&pool);

    let module_name = format!("log_list_{}", unique_id());
    let other = format!("log_other_{}", unique_id());

    for i in 0..3 {
        let req = CreateExecutionLogRequest {
            module_name: module_name.clone(),
            module_type: "spam_checker".to_string(),
            event_id: Some(format!("$evt_{i}_{}", unique_id())),
            room_id: None,
            execution_time_ms: (i + 1) as i64,
            is_success: i != 1,
            error_message: if i == 1 { Some("err".to_string()) } else { None },
            metadata: None,
        };
        storage.create_execution_log(req).await.unwrap();
    }
    // A log for a different module — must be excluded.
    let other_req = CreateExecutionLogRequest {
        module_name: other.clone(),
        module_type: "spam_checker".to_string(),
        event_id: None,
        room_id: None,
        execution_time_ms: 1,
        is_success: true,
        error_message: None,
        metadata: None,
    };
    storage.create_execution_log(other_req).await.unwrap();

    let all = storage.get_execution_logs(&module_name, 100).await.unwrap();
    assert_eq!(all.len(), 3);
    assert!(all.iter().all(|l| l.module_name == module_name));
    // DESC by executed_ts.
    assert!(all[0].executed_ts >= all[1].executed_ts);
    assert!(all[1].executed_ts >= all[2].executed_ts);

    let limited = storage.get_execution_logs(&module_name, 2).await.unwrap();
    assert_eq!(limited.len(), 2, "limit should be respected");
}

// =============================================================================
// create_account_validity / get_account_validity
// =============================================================================

#[tokio::test]
async fn test_create_account_validity() {
    let _guard = module_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = ModuleStorage::new(&pool);

    let user_id = format!("@u_{}", unique_id());
    let expiration = chrono::Utc::now().timestamp_millis() + 86_400_000;
    let request = CreateAccountValidityRequest {
        user_id: user_id.clone(),
        expiration_at: expiration,
        is_valid: Some(true),
    };
    let validity = storage.create_account_validity(request).await.unwrap();

    assert_eq!(validity.user_id, user_id);
    assert_eq!(validity.expiration_at, expiration);
    assert!(validity.is_valid);
    assert!(validity.renewal_token.is_none());
    assert!(validity.created_ts > 0);
}

#[tokio::test]
async fn test_get_account_validity_existing_and_nonexistent() {
    let _guard = module_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = ModuleStorage::new(&pool);

    let user_id = format!("@u_{}", unique_id());
    let expiration = chrono::Utc::now().timestamp_millis() + 3_600_000;
    storage
        .create_account_validity(CreateAccountValidityRequest {
            user_id: user_id.clone(),
            expiration_at: expiration,
            is_valid: Some(true),
        })
        .await
        .unwrap();

    let fetched = storage.get_account_validity(&user_id).await.unwrap().expect("should exist");
    assert_eq!(fetched.user_id, user_id);
    assert_eq!(fetched.expiration_at, expiration);
    assert!(fetched.is_valid);

    let missing = storage.get_account_validity(&format!("@nope_{}", unique_id())).await.unwrap();
    assert!(missing.is_none());
}

#[tokio::test]
async fn test_create_account_validity_upsert_on_conflict() {
    let _guard = module_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = ModuleStorage::new(&pool);

    // ON CONFLICT (user_id) DO UPDATE — second call updates the existing row.
    let user_id = format!("@u_{}", unique_id());
    let first_exp = chrono::Utc::now().timestamp_millis() + 1_000_000;
    storage
        .create_account_validity(CreateAccountValidityRequest {
            user_id: user_id.clone(),
            expiration_at: first_exp,
            is_valid: Some(true),
        })
        .await
        .unwrap();

    let second_exp = first_exp + 5_000_000;
    let updated = storage
        .create_account_validity(CreateAccountValidityRequest {
            user_id: user_id.clone(),
            expiration_at: second_exp,
            is_valid: Some(false),
        })
        .await
        .unwrap();

    assert_eq!(updated.expiration_at, second_exp);
    assert!(!updated.is_valid);

    // Only one row exists.
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM account_validity WHERE user_id = $1")
            .bind(&user_id)
            .fetch_one(pool.as_ref())
            .await
            .unwrap();
    assert_eq!(count, 1);
}

// =============================================================================
// set_renewal_token / renew_account
// =============================================================================

#[tokio::test]
async fn test_set_renewal_token_and_renew_account_success() {
    let _guard = module_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = ModuleStorage::new(&pool);

    let user_id = format!("@u_{}", unique_id());
    let old_exp = chrono::Utc::now().timestamp_millis() + 1_000;
    storage
        .create_account_validity(CreateAccountValidityRequest {
            user_id: user_id.clone(),
            expiration_at: old_exp,
            is_valid: Some(true),
        })
        .await
        .unwrap();

    // set_renewal_token returns ().
    storage.set_renewal_token(&user_id, "token-abc").await.unwrap();

    let with_token = storage.get_account_validity(&user_id).await.unwrap().unwrap();
    assert_eq!(with_token.renewal_token.as_deref(), Some("token-abc"));

    let new_exp = chrono::Utc::now().timestamp_millis() + 90_000_000;
    let renewed = storage.renew_account(&user_id, "token-abc", new_exp).await.unwrap();
    assert_eq!(renewed.expiration_at, new_exp);
    assert!(renewed.is_valid);
    assert!(renewed.renewal_token.is_none(), "renewal_token should be cleared after renewal");

    // Persisted.
    let fetched = storage.get_account_validity(&user_id).await.unwrap().unwrap();
    assert_eq!(fetched.expiration_at, new_exp);
    assert!(fetched.renewal_token.is_none());
}

#[tokio::test]
async fn test_renew_account_wrong_token() {
    let _guard = module_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = ModuleStorage::new(&pool);

    let user_id = format!("@u_{}", unique_id());
    storage
        .create_account_validity(CreateAccountValidityRequest {
            user_id: user_id.clone(),
            expiration_at: chrono::Utc::now().timestamp_millis() + 1_000_000,
            is_valid: Some(true),
        })
        .await
        .unwrap();
    storage.set_renewal_token(&user_id, "correct-token").await.unwrap();

    let new_exp = chrono::Utc::now().timestamp_millis() + 90_000_000;
    let result = storage.renew_account(&user_id, "wrong-token", new_exp).await;
    assert!(result.is_err(), "renew_account with wrong token should return RowNotFound");
    assert!(matches!(result, Err(sqlx::Error::RowNotFound)));

    // Expiration unchanged.
    let fetched = storage.get_account_validity(&user_id).await.unwrap().unwrap();
    assert_ne!(fetched.expiration_at, new_exp);
}

// =============================================================================
// get_expired_accounts
// =============================================================================

#[tokio::test]
async fn test_get_expired_accounts() {
    let _guard = module_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = ModuleStorage::new(&pool);

    let now = chrono::Utc::now().timestamp_millis();

    // Expired + valid.
    let expired_user = format!("@expired_{}", unique_id());
    storage
        .create_account_validity(CreateAccountValidityRequest {
            user_id: expired_user.clone(),
            expiration_at: now - 10_000,
            is_valid: Some(true),
        })
        .await
        .unwrap();

    // Not expired + valid — excluded.
    let active_user = format!("@active_{}", unique_id());
    storage
        .create_account_validity(CreateAccountValidityRequest {
            user_id: active_user.clone(),
            expiration_at: now + 90_000_000,
            is_valid: Some(true),
        })
        .await
        .unwrap();

    // Expired but is_valid=false — excluded by `is_valid = true` filter.
    let invalid_user = format!("@invalid_{}", unique_id());
    storage
        .create_account_validity(CreateAccountValidityRequest {
            user_id: invalid_user.clone(),
            expiration_at: now - 20_000,
            is_valid: Some(false),
        })
        .await
        .unwrap();

    let expired = storage.get_expired_accounts(now).await.unwrap();
    assert!(expired.iter().any(|a| a.user_id == expired_user), "expired+valid should be returned");
    assert!(!expired.iter().any(|a| a.user_id == active_user), "active should be excluded");
    assert!(
        !expired.iter().any(|a| a.user_id == invalid_user),
        "expired but invalid should be excluded"
    );
}

// =============================================================================
// password auth provider stubs
// =============================================================================

#[tokio::test]
async fn test_create_password_auth_provider_stub() {
    let _guard = module_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = ModuleStorage::new(&pool);

    let request = CreatePasswordAuthProviderRequest {
        provider_name: "default".to_string(),
        provider_type: "password".to_string(),
        config: serde_json::json!({}),
        is_enabled: Some(true),
        priority: Some(0),
    };
    let result = storage.create_password_auth_provider(request).await;
    // Stub returns RowNotFound unconditionally.
    assert!(result.is_err());
    assert!(matches!(result, Err(sqlx::Error::RowNotFound)));
}

#[tokio::test]
async fn test_get_password_auth_providers_stub() {
    let _guard = module_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = ModuleStorage::new(&pool);

    let providers = storage.get_password_auth_providers().await.unwrap();
    assert!(providers.is_empty(), "stub always returns an empty vec");
}

// =============================================================================
// create_media_callback / get_media_callbacks
// =============================================================================

#[tokio::test]
async fn test_create_media_callback_and_defaults() {
    let _guard = module_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = ModuleStorage::new(&pool);

    let cb_name = format!("cb_{}", unique_id());
    let cb_type = format!("upload_{}", unique_id());
    let request = CreateMediaCallbackRequest {
        callback_name: cb_name.clone(),
        callback_type: cb_type.clone(),
        url: "https://example.com/hook".to_string(),
        method: None,
        headers: None,
        is_enabled: None,
        timeout_ms: None,
        retry_count: None,
    };
    let cb = storage.create_media_callback(request).await.unwrap();
    assert_eq!(cb.callback_type, cb_type);
    // Defaults applied by register path: status=pending, is_enabled=true.
    assert!(cb.is_enabled);
    assert_eq!(cb.status, "pending");
    assert_eq!(cb.media_id, "");
    assert_eq!(cb.user_id, "");
    assert!(cb.result.is_none());
    assert!(cb.completed_ts.is_none());
}

#[tokio::test]
async fn test_get_media_callbacks_all_and_by_type() {
    let _guard = module_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = ModuleStorage::new(&pool);

    let target = format!("target_{}", unique_id());

    // Enabled callback of target type.
    storage
        .create_media_callback(CreateMediaCallbackRequest {
            callback_name: format!("en_{}", unique_id()),
            callback_type: target.clone(),
            url: "https://example.com".to_string(),
            method: Some("POST".to_string()),
            headers: Some(serde_json::json!({})),
            is_enabled: Some(true),
            timeout_ms: Some(1000),
            retry_count: Some(2),
        })
        .await
        .unwrap();

    // Disabled callback of target type — excluded by `is_enabled = true` filter.
    storage
        .create_media_callback(CreateMediaCallbackRequest {
            callback_name: format!("dis_{}", unique_id()),
            callback_type: target.clone(),
            url: "https://example.com".to_string(),
            method: None,
            headers: None,
            is_enabled: Some(false),
            timeout_ms: None,
            retry_count: None,
        })
        .await
        .unwrap();

    // Enabled callback of a different type.
    let other_type = format!("other_{}", unique_id());
    storage
        .create_media_callback(CreateMediaCallbackRequest {
            callback_name: format!("ot_{}", unique_id()),
            callback_type: other_type.clone(),
            url: "https://example.com".to_string(),
            method: None,
            headers: None,
            is_enabled: Some(true),
            timeout_ms: None,
            retry_count: None,
        })
        .await
        .unwrap();

    // Filter by type.
    let by_type = storage.get_media_callbacks(Some(&target)).await.unwrap();
    assert_eq!(by_type.len(), 1, "only enabled callbacks of the target type");
    assert_eq!(by_type[0].callback_type, target);
    assert!(by_type[0].is_enabled);

    // No filter: all enabled callbacks (target + other).
    let all = storage.get_media_callbacks(None).await.unwrap();
    assert!(all.len() >= 2, "should return all enabled callbacks");
    assert!(all.iter().all(|c| c.is_enabled));
    assert!(all.iter().any(|c| c.callback_type == target));
    assert!(all.iter().any(|c| c.callback_type == other_type));
}

// =============================================================================
// create_account_data_callback / get_account_data_callbacks
// =============================================================================

#[tokio::test]
async fn test_create_and_get_account_data_callback() {
    let _guard = module_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = ModuleStorage::new(&pool);

    let request = CreateAccountDataCallbackRequest {
        callback_name: format!("adc_{}", unique_id()),
        config: serde_json::json!({"key": "value"}),
        is_enabled: Some(true),
        data_types: Some(vec!["m.direct".to_string(), "m.room".to_string()]),
    };
    let result = storage.create_account_data_callback(request).await;
    assert!(result.is_ok(), "create_account_data_callback should succeed: {result:?}");
    let callback = result.unwrap();
    assert_eq!(callback.callback_name.as_str().starts_with("adc_"), true);
    assert!(callback.is_enabled);
    assert!(callback.data_types.is_some());

    let callbacks = storage.get_account_data_callbacks().await;
    assert!(callbacks.is_ok(), "get_account_data_callbacks should succeed: {callbacks:?}");
    assert!(!callbacks.unwrap().is_empty());
}
