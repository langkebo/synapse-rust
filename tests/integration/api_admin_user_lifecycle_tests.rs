use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::sync::{Arc, OnceLock};
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_services::ServiceContainer;
use synapse_web::routes::state::AppState;
use tower::ServiceExt;

static TEST_MUTEX: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

fn test_mutex() -> &'static tokio::sync::Mutex<()> {
    TEST_MUTEX.get_or_init(|| tokio::sync::Mutex::new(()))
}

async fn setup_test_context() -> Option<(axum::Router, Arc<PgPool>, Arc<CacheManager>)> {
    // fail-closed：缺库/建池失败必须 **panic**，不能返回 `None` —— 调用点是
    // `let Some(..) else { return; }`，返回 None 会让 libtest 记 `ok`（一条断言都不跑）。
    // 与 `tests/integration/mod.rs::TestContext::build` 同口径，
    // 见 `docs/audit/TEST_COMPROMISE_AUDIT_2026-10-25.md` §2。
    let pool = match synapse_test_utils::prepare_shared_test_pool().await {
        Ok(pool) => pool,
        Err(error) => panic!(
            "admin user lifecycle setup failed (shared pool): {error}\n\
             Refusing to skip-as-pass: this suite is the only end-to-end guard for the \
             admin user lifecycle endpoints."
        ),
    };
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let container = ServiceContainer::new_test_with_pool_and_cache(pool.clone(), cache.clone()).await;
    let state = AppState::new(container, cache.clone());
    let app = synapse_web::create_router(state);
    Some((app, pool, cache))
}

async fn get_super_admin_token(app: &axum::Router, pool: &PgPool, cache: &CacheManager) -> String {
    let username = format!("admin_user_lifecycle_{}", rand::random::<u32>());
    let register_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": username,
                "password": "AdminTest@123",
                "auth": { "type": "m.login.dummy" }
            })
            .to_string(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), register_request)
        .await
        .expect("failed to register lifecycle admin user");
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 4096).await.expect("failed to read lifecycle admin response");
    let json: Value = serde_json::from_slice(&body).expect("invalid lifecycle admin json");
    let token = json["access_token"].as_str().expect("missing lifecycle admin access token").to_string();

    sqlx::query("UPDATE users SET user_type = 'super_admin', is_admin = TRUE WHERE username = $1")
        .bind(&username)
        .execute(pool)
        .await
        .expect("failed to promote admin test user to super_admin");
    let user_id = format!("@{}:localhost", username);
    cache.delete(&format!("user:admin:{}", user_id)).await;
    token
}

#[tokio::test]
async fn test_admin_user_stats_reflect_real_counts() {
    let _guard = test_mutex().lock().await;
    let Some((app, pool, cache)) = setup_test_context().await else {
        return;
    };
    let admin_token = get_super_admin_token(&app, &pool, &cache).await;

    let baseline_request = Request::builder()
        .uri("/_synapse/admin/v1/user_stats")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let baseline_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), baseline_request).await.unwrap();
    assert_eq!(baseline_response.status(), StatusCode::OK);
    let baseline_body = axum::body::to_bytes(baseline_response.into_body(), 2048).await.unwrap();
    let baseline_json: Value = serde_json::from_slice(&baseline_body).unwrap();

    let username = format!("stats_user_{}", rand::random::<u32>());
    let user_id = format!("@{}:localhost", username);
    let encoded_user_id = user_id.replace('@', "%40").replace(':', "%3A");

    let register_request = Request::builder()
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
    let register_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), register_request).await.unwrap();
    assert_eq!(register_response.status(), StatusCode::OK);

    let deactivate_request = Request::builder()
        .method("PUT")
        .uri(format!("/_synapse/admin/v2/users/{}", encoded_user_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "deactivated": true }).to_string()))
        .unwrap();
    let deactivate_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), deactivate_request).await.unwrap();
    assert_eq!(deactivate_response.status(), StatusCode::OK);

    let updated_request = Request::builder()
        .uri("/_synapse/admin/v1/user_stats")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let updated_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), updated_request).await.unwrap();
    assert_eq!(updated_response.status(), StatusCode::OK);
    let updated_body = axum::body::to_bytes(updated_response.into_body(), 2048).await.unwrap();
    let updated_json: Value = serde_json::from_slice(&updated_body).unwrap();

    assert_eq!(updated_json["total_users"].as_i64().unwrap(), baseline_json["total_users"].as_i64().unwrap() + 1);
    assert_eq!(
        updated_json["deactivated_users"].as_i64().unwrap(),
        baseline_json["deactivated_users"].as_i64().unwrap() + 1
    );
    assert_eq!(updated_json["active_users"].as_i64().unwrap(), baseline_json["active_users"].as_i64().unwrap());
    assert_eq!(updated_json["admin_users"].as_i64().unwrap(), baseline_json["admin_users"].as_i64().unwrap());
    assert_eq!(
        updated_json["user_registration_enabled"].as_bool(),
        baseline_json["user_registration_enabled"].as_bool()
    );
}

/// 测试用户管理完整生命周期：注册 → 查询 → 封禁 → 解封 → 删除
#[tokio::test]
async fn test_admin_user_lifecycle_management() {
    let _guard = test_mutex().lock().await;
    let Some((app, pool, cache)) = setup_test_context().await else {
        return;
    };
    let admin_token = get_super_admin_token(&app, &pool, &cache).await;

    // 1. 创建测试用户
    let username = format!("testuser_{}", rand::random::<u32>());
    let user_id = format!("@{}:localhost", username);

    let register_request = Request::builder()
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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), register_request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), query_request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), deactivate_request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // 4. 验证用户已被封禁（查询显示 deactivated = true）
    let verify_deactivated_request = Request::builder()
        .uri(format!("/_synapse/admin/v2/users/{}", encoded_user_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), verify_deactivated_request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), refresh_request).await.unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_USER_DEACTIVATED");

    // 6. 验证被封禁用户无法继续使用既有 access token
    let test_banned_request = Request::builder()
        .uri("/_matrix/client/v3/account/whoami")
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), test_banned_request).await.unwrap();

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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), reactivate_request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // 8. 验证用户已解封
    let verify_reactivated_request = Request::builder()
        .uri(format!("/_synapse/admin/v2/users/{}", encoded_user_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), verify_reactivated_request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["deactivated"], false);

    // 9. 删除用户（永久删除）
    let delete_request = Request::builder()
        .method("DELETE")
        .uri(format!("/_synapse/admin/v2/users/{}", encoded_user_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), delete_request).await.unwrap();

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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), verify_deleted_request).await.unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// 测试批量用户查询的边界条件
#[tokio::test]
async fn test_admin_user_list_pagination_and_limits() {
    let _guard = test_mutex().lock().await;
    let Some((app, pool, cache)) = setup_test_context().await else {
        return;
    };
    let admin_token = get_super_admin_token(&app, &pool, &cache).await;

    // 1. 创建多个测试用户
    let mut user_ids = Vec::new();
    for i in 0..5 {
        let username = format!("bulkuser_{}_{}", i, rand::random::<u32>());
        let register_request = Request::builder()
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

        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), register_request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        user_ids.push(format!("@{}:localhost", username));
    }

    // 2. 测试用户列表查询（无分页）
    let list_request = Request::builder()
        .uri("/_synapse/admin/v2/users")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), list_request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 10240).await.unwrap();
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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), paginated_request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 10240).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    let users = json["users"].as_array().unwrap();
    assert_eq!(users.len(), 2, "Should return exactly 2 users with limit=2");

    // 验证有 next_token（如果有更多用户）
    if json["total"].as_u64().unwrap() > 2 {
        assert!(json["next_token"].is_string(), "Should have next_token for pagination");
    }

    // 4. 测试边界条件：limit = 0（应该返回错误或默认值）
    let zero_limit_request = Request::builder()
        .uri("/_synapse/admin/v2/users?limit=0")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), zero_limit_request).await.unwrap();

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

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), large_limit_request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 102400).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    let users = json["users"].as_array().unwrap();
    // 应该被限制在合理范围内（例如最多 1000）
    assert!(users.len() <= 1000, "Large limit should be capped");
}

/// A1 (API 路由审计 2026-09-04): `admin.login_as_user` 必须在 audit_events 表
/// 留下独立 action 记录，actor_id 为发起 admin（非 target user）。
#[tokio::test]
async fn test_admin_login_as_user_writes_audit_event() {
    let _guard = test_mutex().lock().await;
    let Some((app, pool, cache)) = setup_test_context().await else {
        return;
    };
    let admin_token = get_super_admin_token(&app, &pool, &cache).await;

    // 1. 创建一个普通 target 用户
    let username = format!("loginas_{}", rand::random::<u32>());
    let user_id = format!("@{}:localhost", username);

    let register_request = Request::builder()
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
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), register_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "target user registration should succeed");

    // 记录 audit_events 起始行数（避免并发测试污染）
    let baseline: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_events WHERE action = 'admin.login_as_user'")
        .fetch_one(pool.as_ref())
        .await
        .expect("count audit_events");

    // 2. admin 调用 /_synapse/admin/v1/users/{user_id}/login
    let encoded_user_id = user_id.replace('@', "%40").replace(':', "%3A");
    let login_as_request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/admin/v1/users/{}/login", encoded_user_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), login_as_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "admin login_as_user should return 200");

    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json["access_token"].as_str().is_some(), "response should include access_token");
    assert_eq!(json["user_id"], user_id);

    // 3. 验证 audit_events 表有 admin.login_as_user 记录，actor_id 是发起 admin
    let new_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_events WHERE action = 'admin.login_as_user'")
        .fetch_one(pool.as_ref())
        .await
        .expect("count audit_events after login_as_user");
    assert!(
        new_count > baseline,
        "login_as_user must write a new audit_events row (baseline={}, new={})",
        baseline,
        new_count
    );

    let row: (String, String, String, String) = sqlx::query_as(
        "SELECT actor_id, action, resource_type, resource_id FROM audit_events \
         WHERE action = 'admin.login_as_user' \
         ORDER BY created_ts DESC LIMIT 1",
    )
    .fetch_one(pool.as_ref())
    .await
    .expect("query latest audit_events row");

    // A1 核心断言：actor_id 必须是发起 admin，而非 target user。
    // 若 audit_events 缺少独立的 admin.login_as_user 事件（只靠 middleware 记录），
    // 则 actor_id 会是 target user_id（因为 middleware 用生效 token 溯源）。
    assert_ne!(row.0, user_id, "actor_id must be the admin who called login_as_user, not the target user");
    assert_eq!(row.1, "admin.login_as_user");
    assert_eq!(row.2, "user");
    assert_eq!(row.3, user_id, "resource_id must be the target user_id");
}

/// 普通 admin **不能**改 super_admin 的密码。
///
/// 改密码是接管账号的捷径：`/login` 按目标用户的 `is_admin` 签发 token，而
/// `should_require_admin_mfa` 只覆盖 `/_synapse/admin/**`，不拦 `/login`。见
/// `docs/audit/ADMIN_RBAC_SURFACE_2026-10-21.md` §2。
#[tokio::test]
async fn test_plain_admin_cannot_reset_super_admin_password() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (plain_admin_token, _) = super::get_admin_token(&app).await;
    let (_super_token, super_localpart) = super::get_super_admin_token(&app).await;
    let super_id = format!("@{super_localpart}:localhost").replace('@', "%40").replace(':', "%3A");

    let request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/admin/v1/users/{super_id}/password"))
        .header("Authorization", format!("Bearer {plain_admin_token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "new_password": "NewPassw0rd!" }).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "a plain admin resetting a super_admin's password would be a privilege escalation"
    );
}

/// 同一守卫不能把正常运维也堵掉：普通 admin 改**普通用户**的密码仍应成功。
#[tokio::test]
async fn test_plain_admin_can_reset_plain_user_password() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let (plain_admin_token, _) = super::get_admin_token(&app).await;

    let username = format!("pwd_target_{}", rand::random::<u32>());
    let register = Request::builder()
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
    let registered = ServiceExt::<Request<Body>>::oneshot(app.clone(), register).await.unwrap();
    assert_eq!(registered.status(), StatusCode::OK);

    let user_id = format!("@{username}:localhost").replace('@', "%40").replace(':', "%3A");
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_synapse/admin/v1/users/{user_id}/password"))
        .header("Authorization", format!("Bearer {plain_admin_token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "new_password": "AnotherPassw0rd!" }).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "resetting a plain user's password is legitimate admin work");
}

// ─────────────────────────────────────────────────────────────────────────────
// 交叉签名单设备验证：POST /_synapse/admin/v1/users/{user_id}/devices/{device_id}/verify
//
// 这条路由是 `CrossSigningVerificationService::verify_device` 的**唯一生产入口**
// （此前整个服务只有集成测试在构造，`verify_device` 因此从未被执行过 ——
// 见 docs/audit/TEST_COMPROMISE_AUDIT_2026-10-25.md §3.1.7.11）。
// ─────────────────────────────────────────────────────────────────────────────

/// 注册一个普通用户，返回 `(user_id, access_token, device_id)`。
async fn register_plain_user(app: &axum::Router, prefix: &str) -> (String, String, String) {
    let username = format!("{prefix}_{}", rand::random::<u32>());
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": username,
                "password": "VerifyDevice!123",
                "auth": { "type": "m.login.dummy" }
            })
            .to_string(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.expect("register plain user");
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 4096).await.expect("read register body");
    let json: Value = serde_json::from_slice(&body).expect("register json");
    let user_id = json["user_id"].as_str().expect("register user_id").to_string();
    let token = json["access_token"].as_str().expect("register access_token").to_string();
    let device_id = json["device_id"].as_str().expect("register device_id").to_string();
    (user_id, token, device_id)
}

fn encode_user_id(user_id: &str) -> String {
    user_id.replace('@', "%40").replace(':', "%3A")
}

fn verify_uri(user_id: &str, device_id: &str) -> String {
    format!("/_synapse/admin/v1/users/{}/devices/{}/verify", encode_user_id(user_id), device_id.replace(' ', "%20"))
}

async fn post_verify(app: &axum::Router, token: &str, user_id: &str, device_id: &str) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("POST")
        .uri(verify_uri(user_id, device_id))
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.expect("verify request");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 8192).await.expect("read verify body");
    let json: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    (status, json)
}

/// 没有交叉签名、没有设备签名 ⇒ 三项全 false，且响应必须回显路径上的 owner/device。
///
/// 反向断言（同一条用例里）：**GET 必须 405** —— 这个端点会写 `e2ee_audit_log`，
/// 做成 GET 会让管理面板的轮询把审计表撑大。
#[tokio::test]
async fn test_admin_verify_device_reports_unverified_without_cross_signing() {
    let _guard = test_mutex().lock().await;
    let Some((app, pool, cache)) = setup_test_context().await else {
        return;
    };
    let admin_token = get_super_admin_token(&app, &pool, &cache).await;
    let (user_id, _token, device_id) = register_plain_user(&app, "verify_plain").await;

    let (status, json) = post_verify(&app, &admin_token, &user_id, &device_id).await;
    assert_eq!(status, StatusCode::OK, "admin verify device: {json}");
    assert_eq!(json["user_id"], user_id, "响应必须回显路径上的 user_id");
    assert_eq!(json["device_id"], device_id, "响应必须回显路径上的 device_id");
    assert_eq!(json["signature_valid"], false, "没有设备签名时 signature_valid 必须 false");
    assert_eq!(json["is_cross_signed"], false, "没有 self_signing 密钥时 is_cross_signed 必须 false");
    assert_eq!(json["is_verified"], false, "is_verified = signature_valid && is_cross_signed");

    let get_request = Request::builder()
        .uri(verify_uri(&user_id, &device_id))
        .header("Authorization", format!("Bearer {admin_token}"))
        .body(Body::empty())
        .unwrap();
    let get_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), get_request).await.unwrap();
    assert_eq!(
        get_response.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "该端点会写审计行，必须是 POST；GET 若可用就等于把写操作暴露成可被轮询的读"
    );
}

/// 播种 `self_signing` 交叉签名密钥 + 该设备的签名 ⇒ 三项全 true，并落一行
/// `operation = "verify_device"` 的审计记录（`e2ee_audit_log`）。
#[tokio::test]
async fn test_admin_verify_device_marks_verified_and_records_audit_entry() {
    let _guard = test_mutex().lock().await;
    let Some((app, pool, cache)) = setup_test_context().await else {
        return;
    };
    let admin_token = get_super_admin_token(&app, &pool, &cache).await;
    let (user_id, _token, device_id) = register_plain_user(&app, "verify_signed").await;

    let now = synapse_common::current_timestamp_millis();
    sqlx::query(
        r"INSERT INTO cross_signing_keys (user_id, key_type, key_data, added_ts)
          VALUES ($1, 'self_signing', $2, $3)
          ON CONFLICT (user_id, key_type) DO NOTHING",
    )
    .bind(&user_id)
    .bind(json!({"ed25519:AA": "fakebase64keydata"}))
    .bind(now)
    .execute(pool.as_ref())
    .await
    .expect("seed cross_signing_keys");

    sqlx::query(
        r"INSERT INTO device_signatures
              (user_id, device_id, target_user_id, target_device_id, algorithm, signature, created_ts)
          VALUES ($1, $2, $1, $2, 'ed25519', $3, $4)",
    )
    .bind(&user_id)
    .bind(&device_id)
    .bind(json!({"ed25519:AA": "sigbytes"}))
    .bind(now)
    .execute(pool.as_ref())
    .await
    .expect("seed device_signatures");

    let (status, json) = post_verify(&app, &admin_token, &user_id, &device_id).await;
    assert_eq!(status, StatusCode::OK, "admin verify device: {json}");
    assert_eq!(json["signature_valid"], true, "播种了设备签名就必须 true");
    assert_eq!(json["is_cross_signed"], true, "播种了 self_signing 就必须 true");
    assert_eq!(json["is_verified"], true);

    // 审计侧证：服务必须真的写了 `verify_device` 行（不是只读不算）。
    let audit = synapse_services::e2ee_audit::E2eeAuditService::new(pool.clone());
    let history = audit.get_key_history(&user_id).await.expect("key history");
    let entry = history
        .iter()
        .find(|entry| entry.operation == "verify_device" && entry.device_id.as_deref() == Some(device_id.as_str()))
        .expect("verify_device audit entry must be written");
    assert!(entry.details["is_verified"].as_bool() == Some(true), "审计行必须记录当时的判定结果：{:?}", entry.details);
}

/// 404 三种形态，每一种都会让"用 GET 设备表直接定位"的写法蒙混过关：
/// ① 设备 id 不存在；② 设备存在但属于**别人**；③ 用户不存在。
#[tokio::test]
async fn test_admin_verify_device_404s_for_unknown_device_and_for_another_users_device() {
    let _guard = test_mutex().lock().await;
    let Some((app, pool, cache)) = setup_test_context().await else {
        return;
    };
    let admin_token = get_super_admin_token(&app, &pool, &cache).await;
    let (owner, _owner_token, owner_device) = register_plain_user(&app, "verify_owner").await;
    let (_other, _other_token, other_device) = register_plain_user(&app, "verify_other").await;

    let (status, json) = post_verify(&app, &admin_token, &owner, "NO_SUCH_DEVICE").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "不存在的设备 id 必须 404：{json}");

    // 关键：`other_device` 真实存在，但它不属于 `owner`。路径里的 user_id 是权威口径，
    // 所以这里必须 404 —— 否则就是把「按用户校验」退化成了「按设备 id 全局查」。
    let (status, json) = post_verify(&app, &admin_token, &owner, &other_device).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "别人的设备不能通过本用户的路径验证：{json}");

    let (status, json) = post_verify(&app, &admin_token, "@nosuchuser:localhost", &owner_device).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "不存在的用户必须 404：{json}");
}

/// 非管理员一律 403（`AdminUser` 提取器兜底），且**不得**因为 404 分支先执行而泄漏设备是否存在。
#[tokio::test]
async fn test_admin_verify_device_rejects_non_admin_callers() {
    let _guard = test_mutex().lock().await;
    let Some((app, _pool, _cache)) = setup_test_context().await else {
        return;
    };
    let (user_id, token, device_id) = register_plain_user(&app, "verify_nonadmin").await;

    let (status, json) = post_verify(&app, &token, &user_id, &device_id).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "普通用户不得调用管理端点：{json}");
}
