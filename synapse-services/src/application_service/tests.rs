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
        url: Some("http://localhost:8080".to_string()),
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
        url: Some("http://localhost:9999".to_string()),
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
        url: Some("http://localhost:9999".to_string()),
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
        sent_ts: Some(1_000),
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

fn make_service_with_protocols(as_id: &str, protocols: Vec<&str>) -> ApplicationService {
    ApplicationService {
        id: 1,
        as_id: as_id.to_string(),
        url: Some("http://localhost:9999".to_string()),
        as_token: "token".to_string(),
        hs_token: "hs_token".to_string(),
        sender_localpart: format!("@{as_id}:example.com"),
        is_enabled: true,
        is_rate_limited: false,
        protocols: protocols.into_iter().map(String::from).collect(),
        namespaces: serde_json::json!({}),
        created_ts: 1,
        updated_ts: None,
        description: None,
        api_key: None,
        config: serde_json::json!({}),
    }
}

#[test]
fn aggregate_protocols_returns_map_from_registered_protocols() {
    let services = vec![
        make_service_with_protocols("irc-bridge", vec!["irc"]),
        make_service_with_protocols("gitter-bridge", vec!["gitter"]),
    ];

    let result = ApplicationServiceManager::aggregate_protocols(&services);

    assert!(result.is_object(), "result must be a JSON object");
    assert!(result.get("irc").is_some(), "irc protocol must be present");
    assert!(result.get("gitter").is_some(), "gitter protocol must be present");
}

#[test]
fn aggregate_protocols_deduplicates_shared_protocol() {
    let services = vec![
        make_service_with_protocols("irc-bridge-1", vec!["irc"]),
        make_service_with_protocols("irc-bridge-2", vec!["irc"]),
    ];

    let result = ApplicationServiceManager::aggregate_protocols(&services);

    let irc = result.get("irc").expect("irc protocol must be present");
    assert!(irc.is_object(), "protocol entry must be an object");
    assert!(irc.get("instances").is_some(), "must have instances field");
    assert!(irc.get("user_fields").is_some(), "must have user_fields field");
    assert!(irc.get("location_fields").is_some(), "must have location_fields field");
}

#[test]
fn aggregate_protocols_returns_empty_object_when_no_services() {
    let result = ApplicationServiceManager::aggregate_protocols(&[]);
    assert!(result.as_object().unwrap().is_empty(), "empty services should yield empty protocol map");
}

#[test]
fn aggregate_protocols_skips_services_with_no_protocols() {
    let services = vec![make_service_with_protocols("noop-bridge", vec![])];

    let result = ApplicationServiceManager::aggregate_protocols(&services);
    assert!(result.as_object().unwrap().is_empty(), "service with no protocols should not contribute entries");
}

// ── B3 / 上游 20273：`url: null` 的 appservice（无推送端点）────────────────────

/// `url: null` 是**合法**注册：bridge 双傀儡就是这种形态。
#[test]
fn parse_config_accepts_null_url_as_no_push_endpoint() {
    let manager = test_manager();
    let raw_config = r#"
id: nullurl-bridge
url: null
as_token: appservice-token
hs_token: homeserver-token
sender: '@nullurlbot:example.com'
"#;

    let request = manager
        .parse_config_file_contents(raw_config, "inline")
        .expect("`url: null` must be accepted (upstream 20273)");
    assert_eq!(request.url, None, "`url: null` 必须解析成 None，而不是空串");
}

/// 省略 `url` 与显式 `null` 等价（serde 对 `Option<String>` 的既有语义）。
#[test]
fn parse_config_accepts_omitted_url() {
    let manager = test_manager();
    let raw_config = r#"
id: omitted-url-bridge
as_token: appservice-token
hs_token: homeserver-token
sender: '@omittedbot:example.com'
"#;

    let request = manager.parse_config_file_contents(raw_config, "inline").expect("omitted url must be accepted");
    assert_eq!(request.url, None);
}

/// 空串**不是** `null` 的同义词 —— 它必须被拒绝，并在报错里指明正确写法，
/// 否则配置里的手滑会静默退化成"没有推送端点"。
#[test]
fn parse_config_rejects_empty_url_and_points_at_null() {
    let manager = test_manager();
    let raw_config = r#"
id: empty-url-bridge
url: ""
as_token: appservice-token
hs_token: homeserver-token
sender: '@emptybot:example.com'
"#;

    let result = manager.parse_config_file_contents(raw_config, "inline");
    assert!(result.is_err(), "empty url must be rejected");
    let error_text = if let Err(error) = result { error.to_string() } else { String::new() };
    assert!(error_text.contains("url: null"), "报错必须指向 `url: null` 的正确写法，实际：{error_text}");
}

/// 非空的 `url` 仍要被校验成合法 URL —— 可空化不能顺带放宽这一条。
#[test]
fn parse_config_still_rejects_an_invalid_url() {
    let manager = test_manager();
    let raw_config = r#"
id: bad-url-bridge
url: "not a url"
as_token: appservice-token
hs_token: homeserver-token
sender: '@badurlbot:example.com'
"#;

    let result = manager.parse_config_file_contents(raw_config, "inline");
    assert!(result.is_err(), "an invalid non-null url must still be rejected");
}

/// 上游 20273 的本体：`url: null` 的 appservice **一行排队记录都不该产生**，
/// 而有 `url` 的照常排队（对照组，防止"谁都别排队"这种把测试做绿的假实现）。
#[tokio::test]
async fn enqueue_skips_appservices_without_a_push_endpoint() {
    let pool = crate::test_utils::prepare_isolated_test_pool().await.expect("isolated test pool");
    let manager = ApplicationServiceManager::new(
        Arc::new(ApplicationServiceStorage::new(&pool)),
        Arc::new(EventStorage::new(&pool, "example.com".to_string())),
        "example.com".to_string(),
    );

    // 非独占：两个 appservice 共用同一命名空间，事件才会同时命中两者 —— 这样
    // 唯一的差别就只剩 `url`，对照组才成立（独占命名空间只允许一个属主）。
    let namespaces = serde_json::json!({
        "users": [],
        "aliases": [],
        "rooms": [{"exclusive": false, "regex": "!bridge-.*:example\\.com"}]
    });

    let registration = |as_id: &str, url: Option<&str>| RegisterApplicationServiceRequest {
        as_id: as_id.to_string(),
        url: url.map(str::to_string),
        as_token: format!("{as_id}-as-token"),
        hs_token: format!("{as_id}-hs-token"),
        sender: format!("@{as_id}:example.com"),
        description: None,
        is_rate_limited: Some(false),
        protocols: None,
        namespaces: Some(namespaces.clone()),
        api_key: None,
        config: None,
    };

    manager.register(registration("null_url_as", None)).await.expect("`url: null` registration must succeed");
    manager.register(registration("with_url_as", Some("http://127.0.0.1:1"))).await.expect("register");

    let enqueued = manager
        .enqueue_matching_event(
            "$evt_b3:example.com",
            "!bridge-room:example.com",
            "m.room.message",
            "@alice:example.com",
            &serde_json::json!({"body": "hi"}),
            None,
        )
        .await
        .expect("enqueue_matching_event");

    assert_eq!(enqueued, 1, "只有带 url 的那个 appservice 应该被排队（对照组）");
    assert_eq!(
        manager.count_pending_events("null_url_as").await.expect("count pending"),
        0,
        "`url: null` 的 appservice 必须一行排队都没有（上游 20273）"
    );
    assert_eq!(
        manager.count_pending_events("with_url_as").await.expect("count pending"),
        1,
        "对照组：有 url 的 appservice 必须照常排队"
    );
}
