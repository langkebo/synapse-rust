// End-to-end invite policy integration tests covering the full
// auth chain: room list → global blocklist/allowlist → MSC4155
// account policy → ignore list (MSC3873).
//
// Each test uses a fresh isolated schema pool via TestContext,
// ensuring no cross-test data interference.
//
// IMPORTANT: `account_policy_denies` only considers account data this server
// hosts, so an absent `m.invite_permission_config` defaults to allow. Users
// that must be *denied* need an explicit `{"default_action":"block"}`.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn setup_test_app() -> Option<axum::Router> {
    super::TestContext::new().await.map(|ctx| ctx.app)
}

async fn register_user(app: &axum::Router, username: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": username,
                "password": "Password123!",
                "auth": { "type": "m.login.dummy" }
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    v["access_token"].as_str().unwrap().to_string()
}

/// Register a user with an explicit `{"default_action":"allow"}`
/// `m.invite_permission_config`, so the tests exercise the allow path
/// rather than relying on the absent-policy default.
async fn register_invitee(app: &axum::Router, username: &str) -> String {
    let token = register_user(app, username).await;
    let user_id = format!("@{}:localhost", username);
    set_account_data(app, &token, &user_id, "m.invite_permission_config", &json!({"default_action": "allow"})).await;
    token
}

async fn create_room(app: &axum::Router, token: &str, name: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "name": name,
                "preset": "private_chat"
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 2048).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    v["room_id"].as_str().unwrap().to_string()
}

async fn invite_user(app: &axum::Router, token: &str, room_id: &str, user_id: &str) -> StatusCode {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{}/invite", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "user_id": user_id }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    response.status()
}

async fn set_account_data(app: &axum::Router, token: &str, user_id: &str, data_type: &str, content: &Value) {
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/user/{}/account_data/{}", user_id, data_type))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(content.to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn admin_post_global_blocklist(app: &axum::Router, token: &str, user_ids: &[&str]) -> StatusCode {
    let request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/invite/blocklist")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "user_ids": user_ids }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    response.status()
}

async fn admin_post_global_allowlist(app: &axum::Router, token: &str, user_ids: &[&str]) -> StatusCode {
    let request = Request::builder()
        .method("POST")
        .uri("/_synapse/admin/v1/invite/allowlist")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "user_ids": user_ids }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    response.status()
}

async fn admin_get_global_blocklist(app: &axum::Router, token: &str) -> Value {
    let request = Request::builder()
        .method("GET")
        .uri("/_synapse/admin/v1/invite/blocklist")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn admin_get_global_allowlist(app: &axum::Router, token: &str) -> Value {
    let request = Request::builder()
        .method("GET")
        .uri("/_synapse/admin/v1/invite/allowlist")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

/// Global blocklist blocks an invite even when the user is not
/// room-blocked — the auth chain checks global policy after room
/// lists and before MSC4155/ignore.
#[tokio::test]
async fn test_global_blocklist_blocks_invite() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, _) = super::get_super_admin_token(&app).await;
    let inviter_token = register_user(&app, "inviter_global_block").await;
    let invitee = "@blocked:localhost";
    let _invitee_token = register_user(&app, "blocked").await;

    let room_id = create_room(&app, &inviter_token, "Global Blocklist Guard").await;

    // Set the global blocklist to include the invitee
    assert_eq!(admin_post_global_blocklist(&app, &admin_token, &[invitee]).await, StatusCode::OK);

    // Verify the admin API round-trips correctly
    let blocklist = admin_get_global_blocklist(&app, &admin_token).await;
    let entries: Vec<&Value> =
        blocklist.get("blocklist").and_then(|v| v.as_array()).map(|arr| arr.iter().collect()).unwrap_or_default();
    assert!(!entries.is_empty(), "global blocklist should have at least 1 entry");

    // Invite should be rejected by the global blocklist
    let status = invite_user(&app, &inviter_token, &room_id, invitee).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "invite should be forbidden when user is globally blocked");
}

/// Global allowlist overrides global blocklist on a per-user basis.
/// A user in the global allowlist should be invited even if they
/// appear in the global blocklist.
#[tokio::test]
async fn test_global_allowlist_overrides_blocklist() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, _) = super::get_super_admin_token(&app).await;
    let inviter_token = register_user(&app, "inviter_allowlist_override").await;
    let invitee = "@allowed:localhost";
    let _invitee_token = register_invitee(&app, "allowed").await;

    let room_id = create_room(&app, &inviter_token, "Allowlist Override Guard").await;

    // Set both blocklist and allowlist
    assert_eq!(admin_post_global_blocklist(&app, &admin_token, &[invitee]).await, StatusCode::OK);
    assert_eq!(admin_post_global_allowlist(&app, &admin_token, &[invitee]).await, StatusCode::OK);

    // Invite should succeed because the global allowlist overrides the blocklist
    let status = invite_user(&app, &inviter_token, &room_id, invitee).await;
    assert_eq!(status, StatusCode::OK, "invite should succeed when user is in global allowlist");

    // Verify the allowlist API round-trips correctly
    let allowlist = admin_get_global_allowlist(&app, &admin_token).await;
    let entries: Vec<&Value> =
        allowlist.get("allowlist").and_then(|v| v.as_array()).map(|arr| arr.iter().collect()).unwrap_or_default();
    assert!(!entries.is_empty(), "global allowlist should have at least 1 entry");
}

/// Global blocklist does not affect users not listed in it.
#[tokio::test]
async fn test_global_blocklist_does_not_affect_others() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, _) = super::get_super_admin_token(&app).await;
    let inviter_token = register_user(&app, "inviter_not_blocked").await;
    let invitee = "@clear:localhost";
    let _invitee_token = register_invitee(&app, "clear").await;

    let room_id = create_room(&app, &inviter_token, "Clear Invite Guard").await;

    // Set global blocklist for a different user
    let other = "@other:localhost";
    assert_eq!(admin_post_global_blocklist(&app, &admin_token, &[other]).await, StatusCode::OK);

    // Invite for a non-blocked user should succeed
    let status = invite_user(&app, &inviter_token, &room_id, invitee).await;
    assert_eq!(status, StatusCode::OK, "invite should succeed for users not in the global blocklist");
}

/// Admin API round-trip: POST then GET global blocklist returns same data.
#[tokio::test]
async fn test_global_blocklist_admin_api_round_trip() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, _) = super::get_super_admin_token(&app).await;
    let users = vec!["@user_a_round_trip:localhost", "@user_b_round_trip:localhost"];

    // POST sets the blocklist
    assert_eq!(admin_post_global_blocklist(&app, &admin_token, &users).await, StatusCode::OK);

    // GET returns the blocklist
    let blocklist = admin_get_global_blocklist(&app, &admin_token).await;
    let blocklist_users: Vec<String> = blocklist
        .get("blocklist")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.get("user_id").and_then(|u| u.as_str()).map(String::from)).collect())
        .unwrap_or_default();
    assert!(blocklist_users.contains(&users[0].to_string()));
    assert!(blocklist_users.contains(&users[1].to_string()));
}

/// Admin API round-trip: POST then GET global allowlist returns same data.
#[tokio::test]
async fn test_global_allowlist_admin_api_round_trip() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, _) = super::get_super_admin_token(&app).await;
    let users = vec!["@user_a_allow_round_trip:localhost", "@user_b_allow_round_trip:localhost"];

    // POST sets the allowlist
    assert_eq!(admin_post_global_allowlist(&app, &admin_token, &users).await, StatusCode::OK);

    // GET returns the allowlist
    let allowlist = admin_get_global_allowlist(&app, &admin_token).await;
    let allowlist_users: Vec<String> = allowlist
        .get("allowlist")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.get("user_id").and_then(|u| u.as_str()).map(String::from)).collect())
        .unwrap_or_default();
    assert!(allowlist_users.contains(&users[0].to_string()));
    assert!(allowlist_users.contains(&users[1].to_string()));
}

/// Global blocklist + global allowlist: allowlist entry clears block.
#[tokio::test]
async fn test_global_allowlist_after_blocklist_coexists() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, _) = super::get_super_admin_token(&app).await;
    let inviter_token = register_user(&app, "inviter_coexist").await;
    let invitee = "@coexist:localhost";
    let _invitee_token = register_invitee(&app, "coexist").await;

    let room_id = create_room(&app, &inviter_token, "Coexist Guard").await;

    // Set blocklist first
    assert_eq!(admin_post_global_blocklist(&app, &admin_token, &[invitee]).await, StatusCode::OK);

    // Then set allowlist for the same user
    assert_eq!(admin_post_global_allowlist(&app, &admin_token, &[invitee]).await, StatusCode::OK);

    // Invite should succeed (allowlist overrides blocklist)
    let status = invite_user(&app, &inviter_token, &room_id, invitee).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "invite should succeed when user is in global allowlist even if also in blocklist"
    );
}

/// Empty global blocklist does not block anyone.
#[tokio::test]
async fn test_empty_global_blocklist_does_not_block() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (admin_token, _) = super::get_super_admin_token(&app).await;
    let inviter_token = register_user(&app, "inviter_empty").await;
    let invitee = "@empty:localhost";
    let _invitee_token = register_invitee(&app, "empty").await;

    let room_id = create_room(&app, &inviter_token, "Empty Guard").await;

    // Set an empty global blocklist
    assert_eq!(admin_post_global_blocklist(&app, &admin_token, &[] as &[&str]).await, StatusCode::OK);

    // Invite should succeed (no global blocklist entries)
    let status = invite_user(&app, &inviter_token, &room_id, invitee).await;
    assert_eq!(status, StatusCode::OK, "invite should succeed when global blocklist is empty");
}

// ── Q1（2026-10-16）：per-user 端点的幂等 / 互斥 / 入参校验 / 门禁接线 ─────────────
//
// 覆盖交接单 Q1 与实施决策 D3/D4/D5：
//   * D4 幂等：重复 POST ⇒ `already_present`，重复 DELETE ⇒ `already_absent`；
//   * D5 审计：`reason` / `created_by` 落库并经 GET 回传（重复 POST **不覆盖**）；
//   * D3 互斥：per-user 加入一侧会把同一 user 从另一侧移除（整体替换端点不互斥）；
//   * 入参：非法 user_id ⇒ 400（而不是撞 DB CHECK 变成 500）。

/// 对 per-user 端点发一次写请求，返回 `(状态码, JSON body)`。
///
/// `body = None` 走"完全不发 body 也没有 Content-Type"的分支 —— 这正是 SDK 在"只给
/// user_id、没有 reason"时的行为，用来锁死"整个 body 可缺"这条契约。
async fn per_user_write(
    app: &axum::Router,
    token: &str,
    method: &str,
    list: &str,
    user_id: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let builder = Request::builder()
        .method(method)
        .uri(format!("/_synapse/admin/v1/invite/{list}/{user_id}"))
        .header("Authorization", format!("Bearer {token}"));

    let request = if let Some(body) = body {
        builder.header("Content-Type", "application/json").body(Body::from(body.to_string())).unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    };

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

/// 从全局名单 GET 的响应里取出 `user_id` 列表。
fn list_user_ids(list: &Value, key: &str) -> Vec<String> {
    list.get(key)
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|e| e.get("user_id").and_then(|u| u.as_str()).map(String::from)).collect())
        .unwrap_or_default()
}

/// 在名单里找一条，返回它的整行（用于断言审计列）。
fn find_entry<'a>(list: &'a Value, key: &str, user_id: &str) -> Option<&'a Value> {
    list.get(key)?.as_array()?.iter().find(|e| e.get("user_id").and_then(|u| u.as_str()) == Some(user_id))
}

#[tokio::test]
async fn test_per_user_blocklist_add_remove_are_idempotent() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_super_admin_token(&app).await;
    let victim = "@q1_victim:localhost";

    // 第一次加入：确实新增
    let (st, body) =
        per_user_write(&app, &admin_token, "POST", "blocklist", victim, Some(json!({"reason": "spam wave #1"}))).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body["already_present"], json!(false), "首次加入不应报 already_present");
    assert_eq!(body["removed_from_other_list"], json!(false), "白名单本来没有他");

    // 第二次加入：幂等
    let (st, body) =
        per_user_write(&app, &admin_token, "POST", "blocklist", victim, Some(json!({"reason": "SPAM WAVE #2"}))).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body["already_present"], json!(true), "重复 POST 必须报 already_present（幂等，不是 409）");

    let list = admin_get_global_blocklist(&app, &admin_token).await;
    let entry = find_entry(&list, "blocklist", victim).expect("victim 应在全局黑名单里");
    assert_eq!(entry["reason"], json!("spam wave #1"), "幂等 POST 不得覆盖既有 reason");
    assert!(
        entry["created_by"].as_str().is_some_and(|s| s.starts_with('@')),
        "created_by 应落库为管理员 MXID，实际 {:?}",
        entry["created_by"]
    );

    // 删除两次：第二次幂等
    let (st, body) = per_user_write(&app, &admin_token, "DELETE", "blocklist", victim, None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body["already_absent"], json!(false));
    let (st, body) = per_user_write(&app, &admin_token, "DELETE", "blocklist", victim, None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body["already_absent"], json!(true), "重复 DELETE 必须报 already_absent（幂等，不是 404）");

    let list = admin_get_global_blocklist(&app, &admin_token).await;
    assert!(!list_user_ids(&list, "blocklist").contains(&victim.to_string()));
}

#[tokio::test]
async fn test_per_user_lists_are_mutually_exclusive() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_super_admin_token(&app).await;
    let user = "@q1_exclusive:localhost";

    // 先放进白名单
    let (st, body) = per_user_write(&app, &admin_token, "POST", "allowlist", user, None).await;
    assert_eq!(st, StatusCode::OK, "无 body 的 POST 必须可用（SDK 就是这么发的）");
    assert_eq!(body["removed_from_other_list"], json!(false));

    // 再加入黑名单 ⇒ 必须把他从白名单移除（D3 互斥）
    let (st, body) = per_user_write(&app, &admin_token, "POST", "blocklist", user, None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body["removed_from_other_list"], json!(true), "应报告'已从另一侧名单移除'");

    let allow = admin_get_global_allowlist(&app, &admin_token).await;
    assert!(!list_user_ids(&allow, "allowlist").contains(&user.to_string()), "互斥：白名单里不该还有他");
    let block = admin_get_global_blocklist(&app, &admin_token).await;
    assert!(list_user_ids(&block, "blocklist").contains(&user.to_string()));

    // 反向：加回白名单 ⇒ 从黑名单移除
    let (st, body) = per_user_write(&app, &admin_token, "POST", "allowlist", user, None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body["removed_from_other_list"], json!(true));
    let block = admin_get_global_blocklist(&app, &admin_token).await;
    assert!(!list_user_ids(&block, "blocklist").contains(&user.to_string()), "互斥：黑名单里不该还有他");
}

#[tokio::test]
async fn test_per_user_endpoints_reject_invalid_user_id() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_super_admin_token(&app).await;

    for bad in ["notauser", "@nopcolon", "bad@host.example.com"] {
        let (st, _) = per_user_write(&app, &admin_token, "POST", "blocklist", bad, None).await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "POST {bad} 应为 400（不是撞 DB CHECK 的 500）");
        let (st, _) = per_user_write(&app, &admin_token, "DELETE", "allowlist", bad, None).await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "DELETE {bad} 应为 400");
    }
}

#[tokio::test]
async fn test_per_user_endpoints_require_admin() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let user_token = register_user(&app, "q1_nonadmin").await;

    let (st, _) = per_user_write(&app, &user_token, "POST", "blocklist", "@someone:localhost", None).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "非管理员不得写全局名单");
    let (st, _) = per_user_write(&app, &user_token, "DELETE", "blocklist", "@someone:localhost", None).await;
    assert_eq!(st, StatusCode::FORBIDDEN);
}

/// per-user 端点必须真的接进邀请门禁（不只是"写进去了"）。
#[tokio::test]
async fn test_per_user_blocklist_is_enforced_by_invite_gate() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_super_admin_token(&app).await;
    let inviter_token = register_user(&app, "inviter_q1_per_user").await;
    let invitee = "@q1_gated:localhost";
    let _invitee_token = register_invitee(&app, "q1_gated").await;

    let room_id = create_room(&app, &inviter_token, "Q1 per-user gate").await;

    let (st, _) = per_user_write(&app, &admin_token, "POST", "blocklist", invitee, None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(
        invite_user(&app, &inviter_token, &room_id, invitee).await,
        StatusCode::FORBIDDEN,
        "per-user 拉黑后邀请必须被拒"
    );

    let (st, _) = per_user_write(&app, &admin_token, "DELETE", "blocklist", invitee, None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(invite_user(&app, &inviter_token, &room_id, invitee).await, StatusCode::OK, "移除后邀请应当放行");
}

/// per-user 端点与整体替换端点并存，各自语义不变：替换是"全量声明"，会覆盖 per-user 写入的行。
#[tokio::test]
async fn test_per_user_and_replace_endpoints_coexist() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_super_admin_token(&app).await;
    let a = "@q1_coexist_a:localhost";
    let b = "@q1_coexist_b:localhost";

    let (st, _) = per_user_write(&app, &admin_token, "POST", "blocklist", a, None).await;
    assert_eq!(st, StatusCode::OK);

    assert_eq!(admin_post_global_blocklist(&app, &admin_token, &[b]).await, StatusCode::OK);
    let list = admin_get_global_blocklist(&app, &admin_token).await;
    assert_eq!(list_user_ids(&list, "blocklist"), vec![b.to_string()], "整体替换后应只剩 b");

    let (st, body) = per_user_write(&app, &admin_token, "POST", "blocklist", a, None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body["already_present"], json!(false), "a 已被替换端点清掉，这里应当是新增");
    let list = admin_get_global_blocklist(&app, &admin_token).await;
    let mut got = list_user_ids(&list, "blocklist");
    got.sort();
    let mut want = vec![a.to_string(), b.to_string()];
    want.sort();
    assert_eq!(got, want);
}
