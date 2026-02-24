use synapse_rust::common::rate_limit_config::{
    RateLimitConfigFile, RateLimitConfigManager, RateLimitEndpointRule, RateLimitMatchType,
    RateLimitRule, select_endpoint_rule,
};
use std::sync::Arc;
use tempfile::NamedTempFile;
use std::io::Write;

fn create_temp_config_file() -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    let config_content = r#"
enabled: true
default:
  per_second: 10
  burst_size: 20
endpoints:
  - path: /_matrix/client/r0/login
    match_type: exact
    rule:
      per_second: 5
      burst_size: 10
  - path: /_matrix/client/r0/sync
    match_type: prefix
    rule:
      per_second: 20
      burst_size: 40
ip_header_priority:
  - x-forwarded-for
  - x-real-ip
include_headers: true
exempt_paths:
  - /
  - /_matrix/client/versions
exempt_path_prefixes: []
endpoint_aliases:
  /_matrix/client/r0/login: login_endpoint
fail_open_on_error: false
reload_interval_seconds: 30
"#;
    file.write_all(config_content.as_bytes()).unwrap();
    file
}

#[tokio::test]
async fn test_load_rate_limit_config_from_file() {
    let temp_file = create_temp_config_file();
    let config = RateLimitConfigFile::load(temp_file.path()).await.unwrap();
    
    assert!(config.enabled);
    assert_eq!(config.default.per_second, 10);
    assert_eq!(config.default.burst_size, 20);
    assert_eq!(config.endpoints.len(), 2);
    assert_eq!(config.ip_header_priority.len(), 2);
    assert!(config.include_headers);
    assert_eq!(config.exempt_paths.len(), 2);
}

#[tokio::test]
async fn test_config_manager_from_file() {
    let temp_file = create_temp_config_file();
    let manager = RateLimitConfigManager::from_file(temp_file.path()).await.unwrap();
    
    let config = manager.get_config();
    assert!(config.enabled);
    assert_eq!(config.default.per_second, 10);
}

#[tokio::test]
async fn test_config_manager_reload() {
    let temp_file = create_temp_config_file();
    let manager = RateLimitConfigManager::from_file(temp_file.path()).await.unwrap();
    
    let initial_config = manager.get_config();
    assert_eq!(initial_config.default.per_second, 10);
    
    let updated_content = r#"
enabled: true
default:
  per_second: 50
  burst_size: 100
endpoints: []
ip_header_priority:
  - x-forwarded-for
include_headers: true
exempt_paths: []
exempt_path_prefixes: []
endpoint_aliases: {}
fail_open_on_error: false
reload_interval_seconds: 30
"#;
    std::fs::write(temp_file.path(), updated_content).unwrap();
    
    manager.reload().await.unwrap();
    
    let reloaded_config = manager.get_config();
    assert_eq!(reloaded_config.default.per_second, 50);
    assert_eq!(reloaded_config.default.burst_size, 100);
}

#[tokio::test]
async fn test_config_manager_update() {
    let temp_file = NamedTempFile::new().unwrap();
    let manager = Arc::new(RateLimitConfigManager::new(
        RateLimitConfigFile::default(),
        temp_file.path().to_path_buf(),
    ));
    
    manager.set_enabled(false).await.unwrap();
    let config = manager.get_config();
    assert!(!config.enabled);
    
    let new_rule = RateLimitRule {
        per_second: 100,
        burst_size: 200,
    };
    manager.set_default_rule(new_rule).await.unwrap();
    
    let config = manager.get_config();
    assert_eq!(config.default.per_second, 100);
    assert_eq!(config.default.burst_size, 200);
}

#[tokio::test]
async fn test_add_remove_endpoint_rule() {
    let temp_file = NamedTempFile::new().unwrap();
    let manager = Arc::new(RateLimitConfigManager::new(
        RateLimitConfigFile::default(),
        temp_file.path().to_path_buf(),
    ));
    
    let rule = RateLimitEndpointRule {
        path: "/_matrix/client/r0/login".to_string(),
        match_type: RateLimitMatchType::Exact,
        rule: RateLimitRule {
            per_second: 5,
            burst_size: 10,
        },
    };
    
    manager.add_endpoint_rule(rule).await.unwrap();
    let config = manager.get_config();
    assert_eq!(config.endpoints.len(), 1);
    assert_eq!(config.endpoints[0].path, "/_matrix/client/r0/login");
    
    manager.remove_endpoint_rule("/_matrix/client/r0/login").await.unwrap();
    let config = manager.get_config();
    assert!(config.endpoints.is_empty());
}

#[tokio::test]
async fn test_add_remove_exempt_path() {
    let temp_file = NamedTempFile::new().unwrap();
    let manager = Arc::new(RateLimitConfigManager::new(
        RateLimitConfigFile::default(),
        temp_file.path().to_path_buf(),
    ));
    
    manager.add_exempt_path("/health".to_string()).await.unwrap();
    let config = manager.get_config();
    assert!(config.exempt_paths.contains(&"/health".to_string()));
    
    manager.remove_exempt_path("/health").await.unwrap();
    let config = manager.get_config();
    assert!(!config.exempt_paths.contains(&"/health".to_string()));
}

#[test]
fn test_select_endpoint_rule_exact_match() {
    let config = RateLimitConfigFile {
        endpoints: vec![RateLimitEndpointRule {
            path: "/_matrix/client/r0/login".to_string(),
            match_type: RateLimitMatchType::Exact,
            rule: RateLimitRule {
                per_second: 5,
                burst_size: 10,
            },
        }],
        ..Default::default()
    };
    
    let (id, rule) = select_endpoint_rule(&config, "/_matrix/client/r0/login");
    assert_eq!(id, "/_matrix/client/r0/login");
    assert_eq!(rule.per_second, 5);
    assert_eq!(rule.burst_size, 10);
    
    let (id, rule) = select_endpoint_rule(&config, "/_matrix/client/r0/login?redirect=1");
    assert_eq!(rule.per_second, config.default.per_second);
}

#[test]
fn test_select_endpoint_rule_prefix_match() {
    let config = RateLimitConfigFile {
        endpoints: vec![
            RateLimitEndpointRule {
                path: "/_matrix/client".to_string(),
                match_type: RateLimitMatchType::Prefix,
                rule: RateLimitRule {
                    per_second: 50,
                    burst_size: 100,
                },
            },
            RateLimitEndpointRule {
                path: "/_matrix/client/r0/sync".to_string(),
                match_type: RateLimitMatchType::Prefix,
                rule: RateLimitRule {
                    per_second: 20,
                    burst_size: 40,
                },
            },
        ],
        ..Default::default()
    };
    
    let (id, rule) = select_endpoint_rule(&config, "/_matrix/client/r0/sync?since=123");
    assert_eq!(id, "/_matrix/client/r0/sync");
    assert_eq!(rule.per_second, 20);
    
    let (id, rule) = select_endpoint_rule(&config, "/_matrix/client/versions");
    assert_eq!(id, "/_matrix/client");
    assert_eq!(rule.per_second, 50);
}

#[test]
fn test_select_endpoint_rule_longest_prefix_wins() {
    let config = RateLimitConfigFile {
        endpoints: vec![
            RateLimitEndpointRule {
                path: "/api".to_string(),
                match_type: RateLimitMatchType::Prefix,
                rule: RateLimitRule {
                    per_second: 100,
                    burst_size: 200,
                },
            },
            RateLimitEndpointRule {
                path: "/api/v1".to_string(),
                match_type: RateLimitMatchType::Prefix,
                rule: RateLimitRule {
                    per_second: 50,
                    burst_size: 100,
                },
            },
            RateLimitEndpointRule {
                path: "/api/v1/users".to_string(),
                match_type: RateLimitMatchType::Prefix,
                rule: RateLimitRule {
                    per_second: 10,
                    burst_size: 20,
                },
            },
        ],
        ..Default::default()
    };
    
    let (_, rule) = select_endpoint_rule(&config, "/api/v1/users/123");
    assert_eq!(rule.per_second, 10);
    
    let (_, rule) = select_endpoint_rule(&config, "/api/v1/posts");
    assert_eq!(rule.per_second, 50);
    
    let (_, rule) = select_endpoint_rule(&config, "/api/v2/users");
    assert_eq!(rule.per_second, 100);
}

#[test]
fn test_endpoint_aliases() {
    let mut config = RateLimitConfigFile::default();
    config.endpoint_aliases.insert(
        "/_matrix/client/r0/login".to_string(),
        "login_endpoint".to_string(),
    );
    config.endpoints.push(RateLimitEndpointRule {
        path: "/_matrix/client/r0/login".to_string(),
        match_type: RateLimitMatchType::Exact,
        rule: RateLimitRule {
            per_second: 5,
            burst_size: 10,
        },
    });
    
    let (id, _) = select_endpoint_rule(&config, "/_matrix/client/r0/login");
    assert_eq!(id, "login_endpoint");
}

#[test]
fn test_config_validation_zero_per_second() {
    let mut config = RateLimitConfigFile::default();
    config.default.per_second = 0;
    assert!(config.validate().is_err());
}

#[test]
fn test_config_validation_zero_burst_size() {
    let mut config = RateLimitConfigFile::default();
    config.default.burst_size = 0;
    assert!(config.validate().is_err());
}

#[test]
fn test_config_validation_empty_endpoint_path() {
    let mut config = RateLimitConfigFile::default();
    config.endpoints.push(RateLimitEndpointRule {
        path: "".to_string(),
        match_type: RateLimitMatchType::Exact,
        rule: RateLimitRule::default(),
    });
    assert!(config.validate().is_err());
}

#[test]
fn test_config_validation_endpoint_zero_per_second() {
    let mut config = RateLimitConfigFile::default();
    config.endpoints.push(RateLimitEndpointRule {
        path: "/test".to_string(),
        match_type: RateLimitMatchType::Exact,
        rule: RateLimitRule {
            per_second: 0,
            burst_size: 10,
        },
    });
    assert!(config.validate().is_err());
}

#[tokio::test]
async fn test_config_persistence() {
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_path_buf();
    
    let config = RateLimitConfigFile {
        enabled: true,
        default: RateLimitRule {
            per_second: 25,
            burst_size: 50,
        },
        endpoints: vec![RateLimitEndpointRule {
            path: "/api/test".to_string(),
            match_type: RateLimitMatchType::Prefix,
            rule: RateLimitRule {
                per_second: 100,
                burst_size: 200,
            },
        }],
        exempt_paths: vec!["/health".to_string()],
        ..Default::default()
    };
    
    config.save(&path).await.unwrap();
    
    let loaded = RateLimitConfigFile::load(&path).await.unwrap();
    assert_eq!(loaded.enabled, config.enabled);
    assert_eq!(loaded.default.per_second, 25);
    assert_eq!(loaded.default.burst_size, 50);
    assert_eq!(loaded.endpoints.len(), 1);
    assert_eq!(loaded.endpoints[0].path, "/api/test");
    assert!(loaded.exempt_paths.contains(&"/health".to_string()));
}

#[tokio::test]
async fn test_concurrent_config_access() {
    let temp_file = NamedTempFile::new().unwrap();
    let manager = Arc::new(RateLimitConfigManager::new(
        RateLimitConfigFile::default(),
        temp_file.path().to_path_buf(),
    ));
    
    let mut handles = vec![];
    
    for i in 0..10 {
        let m = manager.clone();
        handles.push(tokio::spawn(async move {
            let rule = RateLimitRule {
                per_second: i as u32 * 10,
                burst_size: i as u32 * 20,
            };
            m.set_default_rule(rule).await.unwrap();
        }));
    }
    
    for handle in handles {
        handle.await.unwrap();
    }
    
    let config = manager.get_config();
    assert!(config.default.per_second % 10 == 0);
}
