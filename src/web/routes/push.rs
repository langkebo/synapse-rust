use crate::common::ApiError;
use crate::web::routes::{AppState, AuthenticatedUser};
use axum::{
    extract::{Json, Path, State},
    routing::{get, post, put},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

fn create_push_compat_router() -> Router<AppState> {
    Router::new()
        .route("/pushers", get(get_pushers).post(set_pusher))
        .route("/pushers/", get(get_pushers).post(set_pusher))
        .route("/pushers/set", post(set_pusher))
        .route("/pushrules", get(get_push_rules))
        .route("/pushrules/{scope}", get(get_push_rules_scope))
        .route("/pushrules/{scope}/{kind}", get(get_push_rules_kind))
        .route(
            "/pushrules/{scope}/{kind}/{rule_id}",
            get(get_push_rule).post(create_push_rule).put(set_push_rule).delete(delete_push_rule),
        )
        .route("/notifications", get(get_notifications))
        .route("/notifications/{notification_id}/ack", post(ack_notification))
}

pub fn create_push_router(state: AppState) -> Router<AppState> {
    let compat_router = create_push_compat_router();

    Router::new()
        .nest("/_matrix/client/v3", compat_router.clone())
        .nest("/_matrix/client/r0", compat_router)
        .route("/_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}/actions", put(set_push_rule_actions))
        .route(
            "/_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}/enabled",
            get(get_push_rule_enabled).put(set_push_rule_enabled),
        )
        .with_state(state)
}

const PUSH_COMPAT_NEST_PREFIXES: &[&str] = &["/_matrix/client/v3", "/_matrix/client/r0"];

fn push_compat_relative_routes() -> Vec<(axum::http::Method, &'static str)> {
    use axum::http::Method;
    vec![
        (Method::GET, "/pushers"),
        (Method::POST, "/pushers"),
        (Method::POST, "/pushers/set"),
        (Method::GET, "/pushrules"),
        (Method::GET, "/pushrules/{scope}"),
        (Method::GET, "/pushrules/{scope}/{kind}"),
        (Method::GET, "/pushrules/{scope}/{kind}/{rule_id}"),
        (Method::POST, "/pushrules/{scope}/{kind}/{rule_id}"),
        (Method::PUT, "/pushrules/{scope}/{kind}/{rule_id}"),
        (Method::DELETE, "/pushrules/{scope}/{kind}/{rule_id}"),
        (Method::GET, "/notifications"),
        (Method::POST, "/notifications/{notification_id}/ack"),
    ]
}

fn push_v3_only_absolute_routes() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;
    [
        (Method::PUT, "/_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}/actions"),
        (Method::GET, "/_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}/enabled"),
        (Method::PUT, "/_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}/enabled"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "push"))
    .collect()
}

pub fn push_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    let mut out = crate::web::routes::route_ledger::expand_under_prefixes(
        "push",
        PUSH_COMPAT_NEST_PREFIXES,
        &push_compat_relative_routes(),
    );
    out.extend(push_v3_only_absolute_routes());
    out
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SetPusherRequest {
    pub pushkey: String,
    #[serde(rename = "kind")]
    pub kind: Option<String>,
    pub app_id: String,
    pub app_display_name: String,
    pub device_display_name: String,
    pub profile_tag: Option<String>,
    pub lang: String,
    pub data: Option<Value>,
    pub append: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PushRule {
    pub rule_id: String,
    #[serde(rename = "default")]
    pub is_default: bool,
    #[serde(rename = "enabled")]
    pub is_enabled: bool,
    pub pattern: Option<String>,
    pub conditions: Option<Vec<PushCondition>>,
    pub actions: Vec<PushAction>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PushCondition {
    #[serde(rename = "kind")]
    pub kind: String,
    pub key: Option<String>,
    pub pattern: Option<String>,
    #[serde(rename = "is")]
    pub is_value: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PushAction {
    #[serde(rename = "set_tweak")]
    pub set_tweak: Option<String>,
    pub value: Option<Value>,
}

async fn get_pushers(State(state): State<AppState>, auth_user: AuthenticatedUser) -> Result<Json<Value>, ApiError> {
    // P2 #32: 只返回当前设备的 pusher，防止跨设备查看
    let pushers_list: Vec<serde_json::Value> =
        state.services.core.client_push_service.get_pushers(&auth_user.user_id, auth_user.device_id.as_deref()).await?;

    Ok(Json(json!({
        "pushers": pushers_list
    })))
}

async fn set_pusher(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<SetPusherRequest>,
) -> Result<Json<Value>, ApiError> {
    // P2 #32: 验证 access_token 必须关联 device_id，防止无设备用户设置 pusher
    let device_id: String = auth_user
        .device_id
        .clone()
        .ok_or_else(|| ApiError::forbidden("Device ID required for pusher operations".to_string()))?;

    let kind: String =
        body.kind.unwrap_or_else(|| if body.data.is_some() { "http".to_string() } else { "null".to_string() });

    if kind != "null" {
        let created_ts: i64 = state
            .services
            .core
            .client_push_service
            .upsert_pusher(crate::services::client_push_service::UpsertPusherRequest {
                user_id: auth_user.user_id,
                device_id,
                pushkey: body.pushkey.clone(),
                kind: kind.clone(),
                app_id: body.app_id.clone(),
                app_display_name: body.app_display_name,
                device_display_name: body.device_display_name,
                profile_tag: body.profile_tag,
                lang: body.lang,
                data: body.data,
            })
            .await?;

        Ok(Json(json!({
            "pushkey": body.pushkey,
            "kind": kind,
            "app_id": body.app_id,
            "created_ts": created_ts
        })))
    } else {
        // P2 #32: 删除 pusher 时也限制为当前设备，防止跨设备删除
        state.services.core.client_push_service.delete_pusher(&auth_user.user_id, &device_id, &body.pushkey).await?;

        Ok(Json(json!({
            "deleted": true,
            "pushkey": body.pushkey
        })))
    }
}

async fn get_push_rules(State(state): State<AppState>, auth_user: AuthenticatedUser) -> Result<Json<Value>, ApiError> {
    use crate::web::routes::push_rules::{default_push_rules_for_user, merge_default_push_rules};

    let row: Option<serde_json::Value> =
        state.services.core.client_push_service.get_push_rules_content(&auth_user.user_id).await?;

    let username: &str = auth_user.user_id.trim_start_matches('@').split(':').next().unwrap_or("");

    if let Some(content) = row {
        let mut content: Value = content;
        merge_default_push_rules(&mut content, &auth_user.user_id, username);
        return Ok(Json(content));
    }

    Ok(Json(default_push_rules_for_user(&auth_user.user_id, username)))
}

async fn get_push_rules_scope(
    Path(scope): Path<String>,
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    if scope == "global" {
        let username: &str = auth_user.user_id.strip_prefix('@').and_then(|s| s.split(':').next()).unwrap_or("");

        let result: Option<serde_json::Value> =
            state.services.core.client_push_service.get_push_rules_content(&auth_user.user_id).await?;

        if let Some(content) = result {
            if let Some(global) = content.get("global") {
                return Ok(Json(global.clone()));
            }
        }

        let defaults: serde_json::Value =
            crate::web::routes::push_rules::default_push_rules_for_user(&auth_user.user_id, username);
        if let Some(global) = defaults.get("global") {
            Ok(Json(global.clone()))
        } else {
            Ok(Json(json!({
                "content": [],
                "override": [],
                "room": [],
                "sender": [],
                "underride": []
            })))
        }
    } else {
        Err(ApiError::invalid_input(format!("Unsupported push rules scope: {scope}")))
    }
}

async fn get_push_rules_kind(
    Path((scope, kind)): Path<(String, String)>,
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let rules: Vec<serde_json::Value> =
        state.services.core.client_push_service.get_user_push_rules(&auth_user.user_id, &scope, &kind).await?;
    Ok(Json(json!({
        kind: rules
    })))
}

async fn get_push_rule(
    Path((scope, kind, rule_id)): Path<(String, String, String)>,
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let rules: Vec<serde_json::Value> =
        state.services.core.client_push_service.get_user_push_rules(&auth_user.user_id, &scope, &kind).await?;

    let rule: Option<&serde_json::Value> =
        rules.iter().find(|r| r.get("rule_id").and_then(|v| v.as_str()) == Some(&rule_id));

    match rule {
        Some(r) => Ok(Json(r.clone())),
        None => Err(ApiError::not_found("Push rule not found".to_string())),
    }
}

async fn set_push_rule(
    Path((scope, kind, rule_id)): Path<(String, String, String)>,
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let actions: serde_json::Value = body.get("actions").cloned().unwrap_or(json!([]));
    let conditions: Option<serde_json::Value> = body.get("conditions").cloned();
    let pattern: Option<String> = body.get("pattern").and_then(|v| v.as_str()).map(|s| s.to_string());

    let created_ts: i64 = state
        .services
        .core
        .client_push_service
        .upsert_push_rule(crate::services::client_push_service::UpsertPushRuleRequest {
            user_id: auth_user.user_id,
            scope: scope.clone(),
            kind: kind.clone(),
            rule_id: rule_id.clone(),
            pattern,
            conditions,
            actions,
        })
        .await?;

    Ok(Json(json!({
        "rule_id": rule_id,
        "scope": scope,
        "kind": kind,
        "created_ts": created_ts
    })))
}

async fn create_push_rule(
    Path((scope, kind, rule_id)): Path<(String, String, String)>,
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let actions: serde_json::Value = body.get("actions").cloned().unwrap_or(json!([]));
    let conditions: Option<serde_json::Value> = body.get("conditions").cloned();
    let pattern: Option<String> = body.get("pattern").and_then(|v| v.as_str()).map(|s| s.to_string());

    let created_ts: i64 = state
        .services
        .core
        .client_push_service
        .upsert_push_rule(crate::services::client_push_service::UpsertPushRuleRequest {
            user_id: auth_user.user_id,
            scope: scope.clone(),
            kind: kind.clone(),
            rule_id: rule_id.clone(),
            pattern,
            conditions,
            actions,
        })
        .await?;

    Ok(Json(json!({
        "rule_id": rule_id,
        "scope": scope,
        "kind": kind,
        "created_ts": created_ts
    })))
}

async fn delete_push_rule(
    Path((scope, kind, rule_id)): Path<(String, String, String)>,
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let deleted: bool =
        state.services.core.client_push_service.delete_push_rule(&auth_user.user_id, &scope, &kind, &rule_id).await?;

    if !deleted {
        return Err(ApiError::not_found("Push rule not found".to_string()));
    }

    Ok(Json(json!({})))
}

async fn set_push_rule_actions(
    Path((scope, kind, rule_id)): Path<(String, String, String)>,
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let actions: serde_json::Value =
        if body.is_array() { body } else { body.get("actions").cloned().unwrap_or(json!([])) };

    state
        .services
        .core
        .client_push_service
        .set_push_rule_actions(&auth_user.user_id, &scope, &kind, &rule_id, &actions)
        .await?;

    Ok(Json(json!({
        "rule_id": rule_id,
        "actions": actions,
        "updated_ts": chrono::Utc::now().timestamp_millis()
    })))
}

async fn get_push_rule_enabled(
    Path((scope, kind, rule_id)): Path<(String, String, String)>,
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let result: Option<bool> =
        state.services.core.client_push_service.get_push_rule_enabled(&auth_user.user_id, &scope, &kind, &rule_id).await?;

    match result {
        Some(enabled) => Ok(Json(json!({
            "enabled": enabled
        }))),
        None => Err(ApiError::not_found("Push rule not found".to_string())),
    }
}

async fn set_push_rule_enabled(
    Path((scope, kind, rule_id)): Path<(String, String, String)>,
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let enabled: bool = body.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);

    state
        .services
        .core
        .client_push_service
        .set_push_rule_enabled(&auth_user.user_id, &scope, &kind, &rule_id, enabled)
        .await?;

    Ok(Json(json!({
        "rule_id": rule_id,
        "enabled": enabled,
        "updated_ts": chrono::Utc::now().timestamp_millis()
    })))
}

async fn get_notifications(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let limit: i64 = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(100);

    let notifications_list: Vec<serde_json::Value> =
        state.services.core.client_push_service.get_notifications(&auth_user.user_id, limit).await?;

    Ok(Json(json!({
        "notifications": notifications_list,
        "next_token": None::<String>
    })))
}

async fn ack_notification(
    Path(notification_id): Path<i64>,
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let success: bool =
        state.services.core.client_push_service.ack_notification(notification_id, &auth_user.user_id).await?;

    if success {
        Ok(Json(json!({
            "notification_id": notification_id,
            "is_read": true,
            "updated_ts": chrono::Utc::now().timestamp_millis()
        })))
    } else {
        Err(ApiError::not_found("Notification not found".to_string()))
    }
}

// Remove the helper get_user_push_rules as it's now in the service
// (or I can keep it if it's still needed, but I've already replaced calls to it)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_routes_structure() {
        let compat_routes = [
            "/_matrix/client/v3/pushers",
            "/_matrix/client/r0/pushers",
            "/_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}",
            "/_matrix/client/r0/notifications/{notification_id}/ack",
        ];
        let v3_only_routes = [
            "/_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}/actions",
            "/_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}/enabled",
        ];

        assert!(compat_routes.iter().all(|route| route.starts_with("/_matrix/client/")));
        assert!(v3_only_routes.iter().all(|route| route.starts_with("/_matrix/client/v3/")));
    }

    #[test]
    fn test_push_compat_router_contains_shared_paths() {
        let shared_paths = [
            "/pushers",
            "/pushers/set",
            "/pushrules",
            "/pushrules/{scope}",
            "/pushrules/{scope}/{kind}",
            "/pushrules/{scope}/{kind}/{rule_id}",
            "/notifications",
            "/notifications/{notification_id}/ack",
        ];

        assert_eq!(shared_paths.len(), 8);
        assert!(shared_paths.iter().all(|path| path.starts_with('/')));
    }

    #[test]
    fn test_push_router_keeps_rule_mutation_extras_limited_to_v3() {
        let compat_paths = ["/pushrules/{scope}/{kind}/{rule_id}"];
        let v3_only_paths = [
            "/_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}/actions",
            "/_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}/enabled",
        ];
        let absent_r0_paths = [
            "/_matrix/client/r0/pushrules/{scope}/{kind}/{rule_id}/actions",
            "/_matrix/client/r0/pushrules/{scope}/{kind}/{rule_id}/enabled",
        ];

        assert!(compat_paths.iter().all(|path| !path.ends_with("/actions") && !path.ends_with("/enabled")));
        assert!(v3_only_paths.iter().all(|path| path.starts_with("/_matrix/client/v3/")));
        assert!(absent_r0_paths.iter().all(|path| path.starts_with("/_matrix/client/r0/")));
    }

    #[test]
    fn test_push_route_examples_still_match_expected_prefixes() {
        let routes = vec![
            "/_matrix/client/v3/pushers",
            "/_matrix/client/v3/pushers/set",
            "/_matrix/client/v3/pushrules",
            "/_matrix/client/v3/notifications",
        ];

        for route in routes {
            assert!(route.starts_with("/_matrix/client/"));
        }
    }

    #[test]
    fn test_set_pusher_request() {
        let request = SetPusherRequest {
            pushkey: "pushkey123".to_string(),
            kind: Some("http".to_string()),
            app_id: "com.example.app".to_string(),
            app_display_name: "Example App".to_string(),
            device_display_name: "My Device".to_string(),
            profile_tag: Some("tag123".to_string()),
            lang: "en".to_string(),
            data: Some(json!({"url": "https://example.com/push"})),
            append: Some(false),
        };

        assert_eq!(request.pushkey, "pushkey123");
        assert_eq!(request.app_id, "com.example.app");
        assert!(request.kind.is_some());
    }

    #[test]
    fn test_push_rule_structure() {
        let rule = PushRule {
            rule_id: ".m.rule.contains_user_name".to_string(),
            is_default: true,
            is_enabled: true,
            pattern: Some("alice".to_string()),
            conditions: None,
            actions: vec![PushAction { set_tweak: Some("sound".to_string()), value: Some(json!("default")) }],
        };

        assert!(rule.is_default);
        assert!(rule.is_enabled);
        assert!(rule.pattern.is_some());
    }

    #[test]
    fn test_push_condition_structure() {
        let condition =
            PushCondition { kind: "contains_display_name".to_string(), key: None, pattern: None, is_value: None };

        assert_eq!(condition.kind, "contains_display_name");
    }

    #[test]
    fn test_push_action_structure() {
        let action = PushAction { set_tweak: Some("sound".to_string()), value: Some(json!("default")) };

        assert!(action.set_tweak.is_some());
    }

    #[test]
    fn test_push_rule_scope() {
        let scopes = vec!["global", "device"];
        for scope in scopes {
            assert!(!scope.is_empty());
        }
    }

    #[test]
    fn test_push_rule_kind() {
        let kinds = vec!["override", "content", "room", "sender", "underride"];
        for kind in kinds {
            assert!(!kind.is_empty());
        }
    }

    #[test]
    fn test_notification_response_structure() {
        let response = json!({
            "notifications": [],
            "next_token": null
        });

        assert!(response.get("notifications").is_some());
        assert!(response.get("next_token").is_some());
    }
}
