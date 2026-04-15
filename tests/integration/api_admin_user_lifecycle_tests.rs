use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

/// 测试用户管理完整生命周期：注册 → 查询 → 封禁 → 解封 → 删除
#[tokio::test]
async fn test_admin_user_lifecycle_management() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_admin_token(&app).await;

    // 1. 创建测试用户
    let username = format!("testuser_{}", rand::random::<u32>());
    let user_id = format!("@{}:localhost", username);

    let register_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/register")
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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), register_request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let user_token = json["access_token"].as_str().unwrap().to_string();
    let user_refresh_token = json["refresh_token"].as_str().unwrap().to_string();

    // 2. 查询用户信息（验证用户存在且活跃）
    let encoded_user_id = user_id.replace('@', "%40").replace(':', "%3A");
    let query_request = Request::builder()
        .uri(format!("/_synapse/admin/v2/users/{}", encoded_user_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), query_request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["name"], user_id);
    assert_eq!(json["deactivated"], false);

    // 3. 封禁用户（设置 deactivated = true）
    let deactivate_request = Request::builder()
        .method("PUT")
        .uri(format!("/_synapse/admin/v2/users/{}", encoded_user_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "deactivated": true
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), deactivate_request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // 4. 验证用户已被封禁（查询显示 deactivated = true）
    let verify_deactivated_request = Request::builder()
        .uri(format!("/_synapse/admin/v2/users/{}", encoded_user_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), verify_deactivated_request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["deactivated"], true);

    // 5. 验证被封禁用户无法使用 refresh token 刷新会话
    let refresh_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/refresh")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "refresh_token": user_refresh_token
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), refresh_request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_USER_DEACTIVATED");

    // 6. 验证被封禁用户无法继续使用既有 access token
    let test_banned_request = Request::builder()
        .uri("/_matrix/client/r0/account/whoami")
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), test_banned_request)
        .await
        .unwrap();

    // 被封禁用户应该返回 403 或 401
    assert!(
        response.status() == StatusCode::FORBIDDEN || response.status() == StatusCode::UNAUTHORIZED,
        "Deactivated user should not be able to use their token"
    );

    // 7. 解封用户（设置 deactivated = false）
    let reactivate_request = Request::builder()
        .method("PUT")
        .uri(format!("/_synapse/admin/v2/users/{}", encoded_user_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "deactivated": false
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), reactivate_request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // 8. 验证用户已解封
    let verify_reactivated_request = Request::builder()
        .uri(format!("/_synapse/admin/v2/users/{}", encoded_user_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), verify_reactivated_request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["deactivated"], false);

    // 9. 删除用户（永久删除）
    let delete_request = Request::builder()
        .method("DELETE")
        .uri(format!("/_synapse/admin/v2/users/{}", encoded_user_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), delete_request)
        .await
        .unwrap();

    // 删除应该返回 200 或 204
    assert!(
        response.status() == StatusCode::OK || response.status() == StatusCode::NO_CONTENT,
        "User deletion should succeed"
    );

    // 10. 验证用户已被删除（查询返回 404）
    let verify_deleted_request = Request::builder()
        .uri(format!("/_synapse/admin/v2/users/{}", encoded_user_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), verify_deleted_request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// 测试批量用户查询的边界条件
#[tokio::test]
async fn test_admin_user_list_pagination_and_limits() {
    let Some(app) = super::setup_test_app().await else {
        return;
    };
    let (admin_token, _) = super::get_admin_token(&app).await;

    // 1. 创建多个测试用户
    let mut user_ids = Vec::new();
    for i in 0..5 {
        let username = format!("bulkuser_{}_{}", i, rand::random::<u32>());
        let register_request = Request::builder()
            .method("POST")
            .uri("/_matrix/client/r0/register")
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

        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), register_request)
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        user_ids.push(format!("@{}:localhost", username));
    }

    // 2. 测试用户列表查询（无分页）
    let list_request = Request::builder()
        .uri("/_synapse/admin/v2/users")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), list_request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 10240)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    // 验证返回的用户列表包含我们创建的用户
    let users = json["users"].as_array().unwrap();
    assert!(users.len() >= 5, "Should return at least 5 users");

    // 3. 测试分页查询（limit = 2）
    let paginated_request = Request::builder()
        .uri("/_synapse/admin/v2/users?limit=2")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), paginated_request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 10240)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    let users = json["users"].as_array().unwrap();
    assert_eq!(users.len(), 2, "Should return exactly 2 users with limit=2");

    // 验证有 next_token（如果有更多用户）
    if json["total"].as_u64().unwrap() > 2 {
        assert!(
            json["next_token"].is_string(),
            "Should have next_token for pagination"
        );
    }

    // 4. 测试边界条件：limit = 0（应该返回错误或默认值）
    let zero_limit_request = Request::builder()
        .uri("/_synapse/admin/v2/users?limit=0")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), zero_limit_request)
        .await
        .unwrap();

    // 应该返回 400 或使用默认 limit
    assert!(
        response.status() == StatusCode::BAD_REQUEST || response.status() == StatusCode::OK,
        "limit=0 should be handled gracefully"
    );

    // 5. 测试边界条件：limit 过大（应该被限制）
    let large_limit_request = Request::builder()
        .uri("/_synapse/admin/v2/users?limit=10000")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), large_limit_request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 102400)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    let users = json["users"].as_array().unwrap();
    // 应该被限制在合理范围内（例如最多 1000）
    assert!(users.len() <= 1000, "Large limit should be capped");
}
