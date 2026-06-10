#![cfg(test)]

use std::sync::Arc;
use synapse_rust::auth::AuthService;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::common::config::SecurityConfig;
use synapse_rust::common::metrics::MetricsCollector;
use synapse_rust::common::ApiError;
use synapse_rust::services::uia_service::{UiaFlow, UiaService, UiaSession};

fn create_service() -> UiaService {
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    UiaService::new(cache, 3600)
}

#[test]
fn test_get_default_flows() {
    let flows = UiaService::get_default_flows();
    assert_eq!(flows.len(), 3);
    assert_eq!(flows[0].stages, vec!["m.login.password"]);
    assert_eq!(flows[1].stages, vec!["m.login.token"]);
    assert_eq!(flows[2].stages, vec!["m.login.email.identity"]);
}

#[test]
fn test_get_password_change_flows() {
    let flows = UiaService::get_password_change_flows();
    assert_eq!(flows.len(), 2);
    assert_eq!(flows[0].stages, vec!["m.login.password"]);
    assert_eq!(flows[1].stages, vec!["m.login.email.identity"]);
}

#[test]
fn test_get_delete_device_flows() {
    let flows = UiaService::get_delete_device_flows();
    assert_eq!(flows.len(), 2);
    assert_eq!(flows[0].stages, vec!["m.login.password"]);
    assert_eq!(flows[1].stages, vec!["m.login.email.identity"]);
}

#[test]
fn test_get_deactivate_account_flows() {
    let flows = UiaService::get_deactivate_account_flows();
    assert_eq!(flows.len(), 2);
    assert_eq!(flows[0].stages, vec!["m.login.password"]);
    assert_eq!(flows[1].stages, vec!["m.login.email.identity"]);
}

#[test]
fn test_get_cross_signing_flows() {
    let flows = UiaService::get_cross_signing_flows();
    assert_eq!(flows.len(), 2);
    assert_eq!(flows[0].stages, vec!["m.login.password"]);
    assert_eq!(flows[1].stages, vec!["m.login.email.identity"]);
}

#[test]
fn test_is_session_complete_single_stage_completed() {
    let service = create_service();
    let session = UiaSession {
        session_id: "test-session".to_string(),
        user_id: "@user:localhost".to_string(),
        completed: vec!["m.login.password".to_string()],
        created_ts: 1000,
        flows: vec![UiaFlow { stages: vec!["m.login.password".to_string()] }],
    };
    assert!(service.is_session_complete(&session));
}

#[test]
fn test_is_session_complete_single_stage_not_completed() {
    let service = create_service();
    let session = UiaSession {
        session_id: "test-session".to_string(),
        user_id: "@user:localhost".to_string(),
        completed: vec![],
        created_ts: 1000,
        flows: vec![UiaFlow { stages: vec!["m.login.password".to_string()] }],
    };
    assert!(!service.is_session_complete(&session));
}

#[test]
fn test_is_session_complete_multi_stage_partial() {
    let service = create_service();
    let session = UiaSession {
        session_id: "test-session".to_string(),
        user_id: "@user:localhost".to_string(),
        completed: vec!["m.login.password".to_string()],
        created_ts: 1000,
        flows: vec![UiaFlow { stages: vec!["m.login.password".to_string(), "m.login.email.identity".to_string()] }],
    };
    assert!(!service.is_session_complete(&session));
}

#[test]
fn test_is_session_complete_multi_stage_all_completed() {
    let service = create_service();
    let session = UiaSession {
        session_id: "test-session".to_string(),
        user_id: "@user:localhost".to_string(),
        completed: vec!["m.login.password".to_string(), "m.login.email.identity".to_string()],
        created_ts: 1000,
        flows: vec![UiaFlow { stages: vec!["m.login.password".to_string(), "m.login.email.identity".to_string()] }],
    };
    assert!(service.is_session_complete(&session));
}

#[test]
fn test_is_session_complete_multiple_flows_one_complete() {
    let service = create_service();
    let session = UiaSession {
        session_id: "test-session".to_string(),
        user_id: "@user:localhost".to_string(),
        completed: vec!["m.login.token".to_string()],
        created_ts: 1000,
        flows: vec![
            UiaFlow { stages: vec!["m.login.password".to_string()] },
            UiaFlow { stages: vec!["m.login.token".to_string()] },
        ],
    };
    assert!(service.is_session_complete(&session));
}

#[test]
fn test_is_session_complete_multiple_flows_none_complete() {
    let service = create_service();
    let session = UiaSession {
        session_id: "test-session".to_string(),
        user_id: "@user:localhost".to_string(),
        completed: vec!["m.login.password".to_string()],
        created_ts: 1000,
        flows: vec![
            UiaFlow { stages: vec!["m.login.token".to_string()] },
            UiaFlow { stages: vec!["m.login.password".to_string(), "m.login.email.identity".to_string()] },
        ],
    };
    assert!(!service.is_session_complete(&session));
}

#[test]
fn test_is_session_complete_empty_flows() {
    let service = create_service();
    let session = UiaSession {
        session_id: "test-session".to_string(),
        user_id: "@user:localhost".to_string(),
        completed: vec![],
        created_ts: 1000,
        flows: vec![],
    };
    assert!(!service.is_session_complete(&session));
}

#[test]
fn test_build_uia_response_structure() {
    let service = create_service();
    let session = UiaSession {
        session_id: "sid-123".to_string(),
        user_id: "@user:localhost".to_string(),
        completed: vec!["m.login.password".to_string()],
        created_ts: 1000,
        flows: vec![
            UiaFlow { stages: vec!["m.login.password".to_string()] },
            UiaFlow { stages: vec!["m.login.token".to_string()] },
        ],
    };
    let response = service.build_uia_response(&session, "M_UIA_REQUIRED", "Authentication required");
    assert_eq!(response["errcode"], "M_UIA_REQUIRED");
    assert_eq!(response["error"], "Authentication required");
    assert_eq!(response["session"], "sid-123");
    // v10: params now includes threepid identity service mappings per Matrix spec.
    assert_eq!(response["params"]["m.login.email.identity"]["threepidCreds"], serde_json::json!([]));
    assert_eq!(response["params"]["m.login.msisdn"]["threepidCreds"], serde_json::json!([]));
    assert_eq!(response["completed"], serde_json::json!(["m.login.password"]));
    let flows = response["flows"].as_array().unwrap();
    assert_eq!(flows.len(), 2);
    assert_eq!(flows[0]["stages"], serde_json::json!(["m.login.password"]));
    assert_eq!(flows[1]["stages"], serde_json::json!(["m.login.token"]));
}

#[test]
fn test_verify_token_stage_missing_token() {
    let service = create_service();
    let auth = serde_json::json!({
        "txn_id": "txn123"
    });
    let result = service.verify_token_stage(&auth, "@user:localhost");
    assert!(result.is_err());
    match result.unwrap_err() {
        e if e.is_bad_request() && e.internal_message().contains("Token required") => {}
        _ => panic!("Expected BadRequest error"),
    }
}

#[test]
fn test_verify_token_stage_missing_txn_id() {
    let service = create_service();
    let auth = serde_json::json!({
        "token": "sometoken"
    });
    let result = service.verify_token_stage(&auth, "@user:localhost");
    assert!(result.is_err());
    match result.unwrap_err() {
        e if e.is_bad_request() && e.internal_message().contains("Transaction ID required") => {}
        _ => panic!("Expected BadRequest error"),
    }
}

#[test]
fn test_verify_token_stage_empty_txn_id() {
    let service = create_service();
    let auth = serde_json::json!({
        "token": "sometoken",
        "txn_id": ""
    });
    let result = service.verify_token_stage(&auth, "@user:localhost");
    assert!(result.is_err());
    match result.unwrap_err() {
        e if e.is_bad_request() && e.internal_message().contains("Transaction ID required") => {}
        _ => panic!("Expected BadRequest error"),
    }
}

#[test]
fn test_verify_token_stage_success() {
    let service = create_service();
    let auth = serde_json::json!({
        "token": "sometoken",
        "txn_id": "txn123"
    });
    let result = service.verify_token_stage(&auth, "@user:localhost");
    assert!(result.is_ok());
}

#[test]
fn test_cleanup_expired_sessions() {
    let service = create_service();
    let result = service.cleanup_expired_sessions();
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_create_session() {
    let service = create_service();
    let flows = UiaService::get_default_flows();
    let session = service.create_session("@user:localhost", flows.clone()).await;
    assert!(!session.session_id.is_empty());
    assert_eq!(session.user_id, "@user:localhost");
    assert!(session.completed.is_empty());
    assert!(session.created_ts > 0);
    assert_eq!(session.flows.len(), flows.len());
}

#[tokio::test]
async fn test_create_session_unique_ids() {
    let service = create_service();
    let flows = UiaService::get_default_flows();
    let session1 = service.create_session("@user:localhost", flows.clone()).await;
    let session2 = service.create_session("@user:localhost", flows.clone()).await;
    assert_ne!(session1.session_id, session2.session_id);
}

#[tokio::test]
async fn test_get_session_existing() {
    let service = create_service();
    let flows = UiaService::get_default_flows();
    let created = service.create_session("@user:localhost", flows.clone()).await;
    let retrieved = service.get_session(&created.session_id).await;
    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.session_id, created.session_id);
    assert_eq!(retrieved.user_id, created.user_id);
    assert_eq!(retrieved.completed.len(), created.completed.len());
}

#[tokio::test]
async fn test_get_session_nonexistent() {
    let service = create_service();
    let result = service.get_session("nonexistent-session-id").await;
    assert!(result.is_none());
}

#[tokio::test]
async fn test_complete_stage() {
    let service = create_service();
    let flows = UiaService::get_default_flows();
    let created = service.create_session("@user:localhost", flows.clone()).await;
    let updated = service.complete_stage(&created.session_id, "m.login.password").await;
    assert!(updated.is_some());
    let updated = updated.unwrap();
    assert!(updated.completed.contains(&"m.login.password".to_string()));
}

#[tokio::test]
async fn test_complete_stage_idempotent() {
    let service = create_service();
    let flows = UiaService::get_default_flows();
    let created = service.create_session("@user:localhost", flows.clone()).await;
    service.complete_stage(&created.session_id, "m.login.password").await;
    let updated = service.complete_stage(&created.session_id, "m.login.password").await.unwrap();
    let count = updated.completed.iter().filter(|s| *s == "m.login.password").count();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn test_complete_stage_nonexistent_session() {
    let service = create_service();
    let result = service.complete_stage("nonexistent-session", "m.login.password").await;
    assert!(result.is_none());
}

#[tokio::test]
async fn test_complete_multiple_stages() {
    let service = create_service();
    let flows = vec![UiaFlow { stages: vec!["m.login.password".to_string(), "m.login.email.identity".to_string()] }];
    let created = service.create_session("@user:localhost", flows).await;
    service.complete_stage(&created.session_id, "m.login.password").await;
    let updated = service.complete_stage(&created.session_id, "m.login.email.identity").await.unwrap();
    assert_eq!(updated.completed.len(), 2);
    assert!(updated.completed.contains(&"m.login.password".to_string()));
    assert!(updated.completed.contains(&"m.login.email.identity".to_string()));
}

#[tokio::test]
async fn test_remove_session() {
    let service = create_service();
    let flows = UiaService::get_default_flows();
    let created = service.create_session("@user:localhost", flows.clone()).await;
    let retrieved = service.get_session(&created.session_id).await;
    assert!(retrieved.is_some());
    service.remove_session(&created.session_id).await;
    let retrieved = service.get_session(&created.session_id).await;
    assert!(retrieved.is_none());
}

#[tokio::test]
async fn test_remove_session_nonexistent() {
    let service = create_service();
    service.remove_session("nonexistent-session").await;
}

#[tokio::test]
async fn test_validate_auth_no_session() {
    let service = create_service();
    let flows = UiaService::get_default_flows();
    let auth = serde_json::json!({
        "type": "m.login.password"
    });
    let result = service.validate_auth(&auth, "@user:localhost", flows).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err["errcode"], "M_UIA_REQUIRED");
    assert!(err["session"].is_string());
    assert!(err["flows"].is_array());
}

#[tokio::test]
async fn test_validate_auth_unknown_session() {
    let service = create_service();
    let flows = UiaService::get_default_flows();
    let auth = serde_json::json!({
        "type": "m.login.password",
        "session": "nonexistent-session-id"
    });
    let result = service.validate_auth(&auth, "@user:localhost", flows).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err["errcode"], "M_UNKNOWN");
}

#[tokio::test]
async fn test_validate_auth_wrong_user_session() {
    let service = create_service();
    let flows = UiaService::get_default_flows();
    let session = service.create_session("@user1:localhost", flows.clone()).await;
    let auth = serde_json::json!({
        "type": "m.login.password",
        "session": session.session_id
    });
    let result = service.validate_auth(&auth, "@user2:localhost", flows).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err["errcode"], "M_FORBIDDEN");
}

#[tokio::test]
async fn test_validate_auth_no_type() {
    let service = create_service();
    let flows = UiaService::get_default_flows();
    let session = service.create_session("@user:localhost", flows.clone()).await;
    let auth = serde_json::json!({
        "session": session.session_id
    });
    let result = service.validate_auth(&auth, "@user:localhost", flows).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err["errcode"], "M_UIA_REQUIRED");
}

#[tokio::test]
async fn test_validate_auth_unsupported_type() {
    let service = create_service();
    let flows = UiaService::get_default_flows();
    let session = service.create_session("@user:localhost", flows.clone()).await;
    let auth = serde_json::json!({
        "type": "m.login.dummy",
        "session": session.session_id
    });
    let result = service.validate_auth(&auth, "@user:localhost", flows).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err["errcode"], "M_INVALID_PARAM");
}

#[tokio::test]
async fn test_validate_auth_single_stage_success() {
    let service = create_service();
    let flows = vec![UiaFlow { stages: vec!["m.login.password".to_string()] }];
    let session = service.create_session("@user:localhost", flows.clone()).await;
    let auth = serde_json::json!({
        "type": "m.login.password",
        "session": session.session_id
    });
    let result = service.validate_auth(&auth, "@user:localhost", flows).await;
    assert!(result.is_ok());
    let auth_result = result.unwrap();
    assert!(auth_result.session.is_none());
    assert!(auth_result.completed.contains(&"m.login.password".to_string()));
}

#[tokio::test]
async fn test_validate_auth_single_stage_removes_session_on_complete() {
    let service = create_service();
    let flows = vec![UiaFlow { stages: vec!["m.login.password".to_string()] }];
    let session = service.create_session("@user:localhost", flows.clone()).await;
    let session_id = session.session_id.clone();
    let auth = serde_json::json!({
        "type": "m.login.password",
        "session": session_id
    });
    let _ = service.validate_auth(&auth, "@user:localhost", flows).await;
    let retrieved = service.get_session(&session_id).await;
    assert!(retrieved.is_none());
}

#[tokio::test]
async fn test_validate_auth_multi_stage_partial() {
    let service = create_service();
    let flows = vec![UiaFlow { stages: vec!["m.login.password".to_string(), "m.login.email.identity".to_string()] }];
    let session = service.create_session("@user:localhost", flows.clone()).await;
    let auth = serde_json::json!({
        "type": "m.login.password",
        "session": session.session_id
    });
    let result = service.validate_auth(&auth, "@user:localhost", flows).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err["errcode"], "M_UIA_REQUIRED");
    let completed = err["completed"].as_array().unwrap();
    assert!(completed.iter().any(|v| v == "m.login.password"));
}

#[tokio::test]
async fn test_validate_auth_multi_stage_complete() {
    let service = create_service();
    let flows = vec![UiaFlow { stages: vec!["m.login.password".to_string(), "m.login.email.identity".to_string()] }];
    let session = service.create_session("@user:localhost", flows.clone()).await;
    let auth1 = serde_json::json!({
        "type": "m.login.password",
        "session": session.session_id
    });
    let _ = service.validate_auth(&auth1, "@user:localhost", flows.clone()).await;
    let auth2 = serde_json::json!({
        "type": "m.login.email.identity",
        "session": session.session_id
    });
    let result = service.validate_auth(&auth2, "@user:localhost", flows).await;
    assert!(result.is_ok());
    let auth_result = result.unwrap();
    assert!(auth_result.session.is_none());
    assert!(auth_result.completed.contains(&"m.login.password".to_string()));
    assert!(auth_result.completed.contains(&"m.login.email.identity".to_string()));
}

#[tokio::test]
async fn test_validate_auth_already_completed_stage() {
    let service = create_service();
    let flows = vec![UiaFlow { stages: vec!["m.login.password".to_string(), "m.login.email.identity".to_string()] }];
    let session = service.create_session("@user:localhost", flows.clone()).await;
    let auth = serde_json::json!({
        "type": "m.login.password",
        "session": session.session_id
    });
    let _ = service.validate_auth(&auth, "@user:localhost", flows.clone()).await;
    let result = service.validate_auth(&auth, "@user:localhost", flows).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err["errcode"], "M_UIA_REQUIRED");
}

#[tokio::test]
async fn test_validate_auth_multiple_flows_alternate_path() {
    let service = create_service();
    let flows = vec![
        UiaFlow { stages: vec!["m.login.password".to_string()] },
        UiaFlow { stages: vec!["m.login.token".to_string()] },
    ];
    let session = service.create_session("@user:localhost", flows.clone()).await;
    let auth = serde_json::json!({
        "type": "m.login.token",
        "session": session.session_id
    });
    let result = service.validate_auth(&auth, "@user:localhost", flows).await;
    assert!(result.is_ok());
    let auth_result = result.unwrap();
    assert!(auth_result.completed.contains(&"m.login.token".to_string()));
}

#[tokio::test]
async fn test_validate_auth_empty_auth_object() {
    let service = create_service();
    let flows = UiaService::get_default_flows();
    let auth = serde_json::json!({});
    let result = service.validate_auth(&auth, "@user:localhost", flows).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err["errcode"], "M_UIA_REQUIRED");
}

#[test]
fn test_verify_password_stage_missing_password() {
    let service = create_service();
    let auth = serde_json::json!({
        "identifier": {
            "user": "@user:localhost"
        }
    });
    let pool = match prepare_test_pool() {
        Some(p) => p,
        None => return,
    };
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let auth_service = create_auth_service(&pool);
        let result = service.verify_password_stage(&auth, "@user:localhost", &auth_service).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            e if e.is_bad_request() && e.internal_message().contains("Password required") => {}
            _ => panic!("Expected BadRequest error"),
        }
    });
}

#[test]
fn test_verify_password_stage_user_mismatch_identifier() {
    let service = create_service();
    let auth = serde_json::json!({
        "password": "somepassword",
        "identifier": {
            "user": "@different:localhost"
        }
    });
    let pool = match prepare_test_pool() {
        Some(p) => p,
        None => return,
    };
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let auth_service = create_auth_service(&pool);
        let result = service.verify_password_stage(&auth, "@user:localhost", &auth_service).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            e if e.is_forbidden() && e.internal_message().contains("User mismatch") => {}
            _ => panic!("Expected Forbidden error"),
        }
    });
}

#[test]
fn test_verify_password_stage_user_mismatch_legacy_user_field() {
    let service = create_service();
    let auth = serde_json::json!({
        "password": "somepassword",
        "user": "@different:localhost"
    });
    let pool = match prepare_test_pool() {
        Some(p) => p,
        None => return,
    };
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let auth_service = create_auth_service(&pool);
        let result = service.verify_password_stage(&auth, "@user:localhost", &auth_service).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            e if e.is_forbidden() && e.internal_message().contains("User mismatch") => {}
            _ => panic!("Expected Forbidden error"),
        }
    });
}

fn prepare_test_pool() -> Option<Arc<sqlx::PgPool>> {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        match synapse_rust::test_utils::prepare_empty_isolated_test_pool().await {
            Ok(pool) => Some(pool),
            Err(error) => {
                eprintln!("Skipping UIA password stage test because test database is unavailable: {error}");
                None
            }
        }
    })
}

fn create_auth_service(pool: &Arc<sqlx::PgPool>) -> AuthService {
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let metrics = Arc::new(MetricsCollector::new());
    let security = SecurityConfig {
        secret: "test_secret".to_string(),
        expiry_time: 3600,
        refresh_token_expiry: 604800,
        argon2_m_cost: 2048,
        argon2_t_cost: 1,
        argon2_p_cost: 1,
        allow_legacy_hashes: false,
        login_failure_lockout_threshold: 5,
        login_lockout_duration_seconds: 900,
        admin_mfa_required: false,
        admin_mfa_shared_secret: String::new(),
        admin_mfa_allowed_drift_steps: 1,
        admin_rbac_enabled: true,
        ui_auth_session_timeout: 900,
    };
    AuthService::new(pool, cache, metrics, &security, "localhost")
}
