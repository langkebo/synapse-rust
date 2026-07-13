use super::models::NamespacesInfo;
use super::scheduler::{
    SCHEDULER_STATE_LAST_ELAPSED_MS, SCHEDULER_STATE_LAST_RESULT, SCHEDULER_STATE_TOTAL_BACKOFF_COUNT,
    SCHEDULER_STATE_TRANSACTION_STATE,
};
use super::transaction::{TransactionFailureKind, APPSERVICE_STATE_DELIVERY_STATUS};
use super::*;
use reqwest::StatusCode;
use synapse_storage::EventStorage;

fn test_manager() -> ApplicationServiceManager {
    // sqlx 0.8 connect_lazy_with requires a Tokio runtime context to spawn
    // background tasks even though no actual connection is made.
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();
    let pool =
        Arc::new(sqlx::postgres::PgPoolOptions::new().connect_lazy_with(sqlx::postgres::PgConnectOptions::new()));

    ApplicationServiceManager::new(
        Arc::new(ApplicationServiceStorage::new(&pool)),
        Arc::new(EventStorage::new(&pool, "example.com".to_string())),
        Arc::new(EventStorage::new(&pool, "example.com".to_string())),
        "example.com".to_string(),
    )
}

#[test]
fn test_namespaces_info_serialization() {
    let info = NamespacesInfo { users: vec![], aliases: vec![], rooms: vec![] };
    let json = serde_json::to_string(&info).unwrap();
    assert!(json.contains("users"));
    assert!(json.contains("aliases"));
    assert!(json.contains("rooms"));
}

#[test]
fn test_namespaces_info_with_data() {
    let namespace = synapse_storage::application_service::ApplicationServiceNamespace {
        id: 1,
        as_id: "test-as".to_string(),
        namespace_pattern: "@_.*:example.com".to_string(),
        is_exclusive: true,
        regex: "@_.*:example.com".to_string(),
        created_ts: 1234567890,
    };
    let info = NamespacesInfo { users: vec![namespace.clone()], aliases: vec![], rooms: vec![namespace] };
    assert_eq!(info.users.len(), 1);
    assert_eq!(info.rooms.len(), 1);
    assert!(info.aliases.is_empty());
}

#[test]
fn test_update_request_builder() {
    let request = synapse_storage::application_service::UpdateApplicationServiceRequest::new()
        .url("http://new-url:8080")
        .description("New Description")
        .is_rate_limited(true)
        .is_enabled(true);

    assert_eq!(request.url, Some("http://new-url:8080".to_string()));
    assert_eq!(request.description, Some("New Description".to_string()));
    assert_eq!(request.is_rate_limited, Some(true));
    assert_eq!(request.is_enabled, Some(true));
}

#[test]
fn test_update_request_partial() {
    let request = synapse_storage::application_service::UpdateApplicationServiceRequest::new()
        .description("Only Description Update");

    assert!(request.url.is_none());
    assert_eq!(request.description, Some("Only Description Update".to_string()));
    assert!(request.is_rate_limited.is_none());
    assert!(request.is_enabled.is_none());
}

#[test]
fn test_update_request_protocols() {
    let request = synapse_storage::application_service::UpdateApplicationServiceRequest::new()
        .protocols(vec!["irc".to_string(), "matrix".to_string()]);

    assert_eq!(request.protocols.as_ref().unwrap().len(), 2);
    assert!(request.protocols.unwrap().contains(&"irc".to_string()));
}

#[test]
fn test_namespace_rule_creation() {
    let rule = synapse_storage::application_service::NamespaceRule {
        is_exclusive: true,
        regex: "@_irc_.*:example\\.com".to_string(),
        group_id: Some("group:example.com".to_string()),
    };
    assert!(rule.is_exclusive);
    assert_eq!(rule.regex, "@_irc_.*:example\\.com");
    assert_eq!(rule.group_id, Some("group:example.com".to_string()));
}

#[test]
fn test_namespace_rule_without_group() {
    let rule = synapse_storage::application_service::NamespaceRule {
        is_exclusive: false,
        regex: "#_.*:example\\.com".to_string(),
        group_id: None,
    };
    assert!(!rule.is_exclusive);
    assert!(rule.group_id.is_none());
}

#[test]
fn test_namespaces_structure() {
    let namespaces = synapse_storage::application_service::Namespaces {
        users: vec![synapse_storage::application_service::NamespaceRule {
            is_exclusive: true,
            regex: "@_.*:example.com".to_string(),
            group_id: None,
        }],
        aliases: vec![],
        rooms: vec![],
    };
    assert_eq!(namespaces.users.len(), 1);
    assert!(namespaces.aliases.is_empty());
    assert!(namespaces.rooms.is_empty());
}

#[test]
fn test_register_request_minimal() {
    let request = synapse_storage::application_service::RegisterApplicationServiceRequest {
        as_id: "minimal-as".to_string(),
        url: "http://localhost:8080".to_string(),
        as_token: "token".to_string(),
        hs_token: "hs_token".to_string(),
        sender: "@bot:example.com".to_string(),
        description: None,
        is_rate_limited: None,
        protocols: None,
        namespaces: None,
        api_key: None,
        config: None,
    };
    assert_eq!(request.as_id, "minimal-as");
    assert!(request.description.is_none());
    assert!(request.protocols.is_none());
}

#[test]
fn test_register_request_full() {
    let request = synapse_storage::application_service::RegisterApplicationServiceRequest {
        as_id: "full-as".to_string(),
        url: "http://localhost:9999".to_string(),
        as_token: "as_token".to_string(),
        hs_token: "hs_token".to_string(),
        sender: "@bridge:example.com".to_string(),
        description: Some("A full bridge service".to_string()),
        is_rate_limited: Some(true),
        protocols: Some(vec!["irc".to_string()]),
        namespaces: Some(serde_json::json!({
            "users": [{"exclusive": true, "regex": "@_.*:example.com"}],
            "aliases": [],
            "rooms": []
        })),
        api_key: None,
        config: None,
    };
    assert_eq!(request.description, Some("A full bridge service".to_string()));
    assert_eq!(request.is_rate_limited, Some(true));
    assert!(request.namespaces.is_some());
}

#[test]
fn test_parse_config_file_contents_normalizes_sender_localpart() {
    let manager = test_manager();
    let raw_config = r#"
id: irc-bridge
url: http://localhost:9999
as_token: appservice-token
hs_token: homeserver-token
sender_localpart: ircbot
rate_limited: false
protocols:
  - irc
namespaces:
  users:
    - exclusive: true
      regex: '@_irc_.*:example\.com'
  aliases: []
  rooms: []
receive_ephemeral: true
"#;

    let result = manager.parse_config_file_contents(raw_config, "inline");
    assert!(result.is_ok());
    let request = if let Ok(request) = result { request } else { return };

    assert_eq!(request.as_id, "irc-bridge");
    assert_eq!(request.sender, "@ircbot:example.com");
    assert_eq!(request.is_rate_limited, Some(false));
    assert_eq!(request.protocols, Some(vec!["irc".to_string()]));
    assert_eq!(request.config.unwrap()["receive_ephemeral"], serde_json::json!(true));
}

#[test]
fn test_parse_config_file_contents_rejects_invalid_namespace_regex() {
    let manager = test_manager();
    let raw_config = r#"
id: bad-bridge
url: http://localhost:9999
as_token: appservice-token
hs_token: homeserver-token
sender: '@bridge:example.com'
namespaces:
  users:
    - exclusive: true
      regex: '['
  aliases: []
  rooms: []
"#;

    let result = manager.parse_config_file_contents(raw_config, "inline");
    assert!(result.is_err());
    let error_text = if let Err(error) = result { error.to_string() } else { String::new() };
    assert!(error_text.contains("invalid users namespace regex"));
}

#[test]
fn test_namespace_matches_can_require_exclusive_rules() {
    let namespaces = serde_json::json!({
        "users": [
            {"exclusive": false, "regex": "^@shared:example\\.com$"},
            {"exclusive": true, "regex": "^@owned:example\\.com$"}
        ]
    });

    assert!(ApplicationServiceManager::namespace_matches(&namespaces, "users", "@shared:example.com", false));
    assert!(!ApplicationServiceManager::namespace_matches(&namespaces, "users", "@shared:example.com", true));
    assert!(ApplicationServiceManager::namespace_matches(&namespaces, "users", "@owned:example.com", true));
}

#[test]
fn test_scheduler_statistics_from_states_parses_scheduler_fields() {
    let states = vec![
        ApplicationServiceState {
            as_id: "as-1".to_string(),
            state_key: SCHEDULER_STATE_LAST_RESULT.to_string(),
            state_value: "backoff".to_string(),
            updated_ts: 1,
        },
        ApplicationServiceState {
            as_id: "as-1".to_string(),
            state_key: SCHEDULER_STATE_TRANSACTION_STATE.to_string(),
            state_value: "retry_backoff".to_string(),
            updated_ts: 2,
        },
        ApplicationServiceState {
            as_id: "as-1".to_string(),
            state_key: SCHEDULER_STATE_TOTAL_BACKOFF_COUNT.to_string(),
            state_value: "3".to_string(),
            updated_ts: 3,
        },
        ApplicationServiceState {
            as_id: "as-1".to_string(),
            state_key: SCHEDULER_STATE_LAST_ELAPSED_MS.to_string(),
            state_value: "42".to_string(),
            updated_ts: 4,
        },
    ];

    let scheduler = ApplicationServiceManager::scheduler_statistics_from_states(&states);

    assert_eq!(scheduler["available"], true);
    assert_eq!(scheduler["last_result"], "backoff");
    assert_eq!(scheduler["transaction_state"], "retry_backoff");
    assert_eq!(scheduler["total_backoff_count"], 3);
    assert_eq!(scheduler["last_elapsed_ms"], 42);
    assert!(scheduler["total_success_count"].is_null());
}

#[test]
fn test_scheduler_statistics_from_states_is_unavailable_without_scheduler_keys() {
    let states = vec![ApplicationServiceState {
        as_id: "as-1".to_string(),
        state_key: APPSERVICE_STATE_DELIVERY_STATUS.to_string(),
        state_value: "up".to_string(),
        updated_ts: 1,
    }];

    let scheduler = ApplicationServiceManager::scheduler_statistics_from_states(&states);

    assert_eq!(scheduler["available"], false);
    assert!(scheduler["last_result"].is_null());
    assert!(scheduler["pending_event_count"].is_null());
}

#[test]
fn test_exclusive_namespace_patterns_extracts_only_exclusive_rules() {
    let namespaces = serde_json::json!({
        "users": [
            {"exclusive": false, "regex": "^@shared:example\\.com$"},
            {"exclusive": true, "regex": "^@owned:example\\.com$"},
            {"exclusive": true, "regex": "   ^@other:example\\.com$   "}
        ]
    });

    let patterns = ApplicationServiceManager::exclusive_namespace_patterns(Some(&namespaces), "users");
    assert_eq!(patterns, vec!["^@owned:example\\.com$", "^@other:example\\.com$"]);
}

#[test]
fn test_is_local_user_id_requires_matching_server_name() {
    assert!(ApplicationServiceManager::is_local_user_id("@bot:example.com", "example.com"));
    assert!(!ApplicationServiceManager::is_local_user_id("@bot:other.com", "example.com"));
    assert!(!ApplicationServiceManager::is_local_user_id("bot:example.com", "example.com"));
}

#[test]
fn test_service_matches_event_for_user_and_room_namespaces() {
    let manager = test_manager();
    let service = ApplicationService {
        id: 1,
        as_id: "bridge".to_string(),
        url: "http://localhost:9999".to_string(),
        as_token: "as-token".to_string(),
        hs_token: "hs-token".to_string(),
        sender_localpart: "@bridge:example.com".to_string(),
        is_enabled: true,
        is_rate_limited: false,
        protocols: vec![],
        namespaces: serde_json::json!({
            "users": [{"exclusive": true, "regex": "@_bridge_.*:example\\.com"}],
            "aliases": [],
            "rooms": [{"exclusive": true, "regex": "!bridge-.*:example\\.com"}]
        }),
        created_ts: 1,
        updated_ts: None,
        description: None,
        api_key: None,
        config: serde_json::json!({}),
    };

    assert!(manager.service_matches_event(&service, "!bridge-room:example.com", "@alice:example.com", None,));
    assert!(manager.service_matches_event(&service, "!random:example.com", "@_bridge_alice:example.com", None,));
    assert!(manager.service_matches_event(
        &service,
        "!random:example.com",
        "@alice:example.com",
        Some("@_bridge_bot:example.com"),
    ));
    assert!(!manager.service_matches_event(&service, "!random:example.com", "@alice:example.com", None,));
}

#[test]
fn test_source_event_id_strips_appservice_suffix() {
    assert_eq!(
        ApplicationServiceManager::source_event_id("$abc123:example.com::bridge"),
        "$abc123:example.com".to_string()
    );
    assert_eq!(ApplicationServiceManager::source_event_id("$plain:example.com"), "$plain:example.com".to_string());
}

#[test]
fn test_retry_backoff_ms_grows_and_caps() {
    assert_eq!(ApplicationServiceManager::retry_backoff_ms(0), 0);
    assert_eq!(ApplicationServiceManager::retry_backoff_ms(1), 5_000);
    assert_eq!(ApplicationServiceManager::retry_backoff_ms(2), 10_000);
    assert_eq!(ApplicationServiceManager::retry_backoff_ms(3), 20_000);
    assert_eq!(ApplicationServiceManager::retry_backoff_ms(10), 300_000);
}

#[test]
fn test_is_transaction_ready_to_retry_respects_backoff_window() {
    let transaction = ApplicationServiceTransaction {
        id: 1,
        as_id: "bridge".to_string(),
        txn_id: "txn".to_string(),
        transaction_id: Some("txn".to_string()),
        events: serde_json::json!([]),
        sent_ts: 1_000,
        completed_ts: None,
        retry_count: 2,
        last_error: Some("boom".to_string()),
    };

    assert!(!ApplicationServiceManager::is_transaction_ready_to_retry(&transaction, 10_999));
    assert!(ApplicationServiceManager::is_transaction_ready_to_retry(&transaction, 11_000));
}

#[test]
fn test_classify_http_failure_distinguishes_retryable_and_fatal_statuses() {
    assert_eq!(
        ApplicationServiceManager::classify_http_failure(StatusCode::BAD_GATEWAY),
        TransactionFailureKind::Retryable
    );
    assert_eq!(
        ApplicationServiceManager::classify_http_failure(StatusCode::TOO_MANY_REQUESTS),
        TransactionFailureKind::Retryable
    );
    assert_eq!(
        ApplicationServiceManager::classify_http_failure(StatusCode::UNAUTHORIZED),
        TransactionFailureKind::Fatal
    );
    assert_eq!(ApplicationServiceManager::classify_http_failure(StatusCode::NOT_FOUND), TransactionFailureKind::Fatal);
}

#[test]
fn test_should_disable_service_uses_kind_specific_thresholds() {
    assert!(!ApplicationServiceManager::should_disable_service(TransactionFailureKind::Fatal, 2));
    assert!(ApplicationServiceManager::should_disable_service(TransactionFailureKind::Fatal, 3));
    assert!(!ApplicationServiceManager::should_disable_service(TransactionFailureKind::Retryable, 7));
    assert!(ApplicationServiceManager::should_disable_service(TransactionFailureKind::Retryable, 8));
}
