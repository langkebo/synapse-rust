#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::common::config::SecurityConfig;
use synapse_rust::common::metrics::MetricsCollector;
use synapse_services::account::UserService;
use synapse_services::auth::{AuthService, CredentialAuth, TokenAuth};
use synapse_services::registration_service::RegistrationService;
use synapse_storage::user::UserStorage;
use synapse_storage::user::UserStore;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

#[tokio::test]
async fn test_register_user_success() {
    let pool = crate::require_test_pool().await;

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
        ..Default::default()
    };
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let canonical_cache = cache.clone();
    let metrics = Arc::new(MetricsCollector::new());
    let auth_service = AuthService::new(&pool, canonical_cache.clone(), metrics.clone(), &security, "localhost");
    let user_store: Arc<dyn UserStore> = Arc::new(UserStorage::new(&pool, canonical_cache));
    let user_service = Arc::new(UserService::new(user_store.clone()));
    let auth_arc = Arc::new(auth_service);
    let token_auth: Arc<dyn TokenAuth> = auth_arc.clone();
    let credential_auth: Arc<dyn CredentialAuth> = auth_arc;
    let registration_service =
        RegistrationService::new(user_service, token_auth, credential_auth, metrics, "localhost", true, None);

    let id = unique_id();
    let username = format!("alice_{}", id);
    let result = registration_service.register_user(&username, "Password123!", Some("Alice"), None).await;
    assert!(result.is_ok());
    let val = result.unwrap();
    assert_eq!(val["user_id"], format!("@{}:localhost", username));
    assert!(val["access_token"].is_string());
}

#[tokio::test]
async fn test_login_success() {
    let pool = crate::require_test_pool().await;

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
        ..Default::default()
    };
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let canonical_cache = cache.clone();
    let metrics = Arc::new(MetricsCollector::new());
    let auth_service = AuthService::new(&pool, canonical_cache.clone(), metrics.clone(), &security, "localhost");
    let user_store: Arc<dyn UserStore> = Arc::new(UserStorage::new(&pool, canonical_cache));
    let user_service = Arc::new(UserService::new(user_store.clone()));
    let auth_arc = Arc::new(auth_service);
    let token_auth: Arc<dyn TokenAuth> = auth_arc.clone();
    let credential_auth: Arc<dyn CredentialAuth> = auth_arc;
    let registration_service =
        RegistrationService::new(user_service, token_auth, credential_auth, metrics, "localhost", true, None);

    let id = unique_id();
    let username = format!("alice_{}", id);
    registration_service.register_user(&username, "Password123!", None, None).await.unwrap();

    let result = registration_service.login(&username, "Password123!", None, None).await;
    assert!(result.is_ok());
    let val = result.unwrap();
    assert_eq!(val["user_id"], format!("@{}:localhost", username));
}

#[tokio::test]
async fn test_get_profile_success() {
    let pool = crate::require_test_pool().await;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let canonical_cache = cache.clone();
    let id = unique_id();
    let user_id = format!("@alice_{}:localhost", id);
    let username = format!("alice_{}", id);

    let user_storage: Arc<dyn UserStore> = Arc::new(UserStorage::new(&pool, canonical_cache.clone()));
    let user_service = Arc::new(UserService::new(user_storage.clone()));
    user_storage.create_user(&user_id, &username, None, false).await.unwrap();
    user_storage.update_displayname(&user_id, Some("Alice")).await.unwrap();

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
        ..Default::default()
    };
    let metrics = Arc::new(MetricsCollector::new());
    let auth_service = AuthService::new(&pool, canonical_cache.clone(), metrics.clone(), &security, "localhost");
    let auth_arc = Arc::new(auth_service);
    let token_auth: Arc<dyn TokenAuth> = auth_arc.clone();
    let credential_auth: Arc<dyn CredentialAuth> = auth_arc;
    let registration_service =
        RegistrationService::new(user_service, token_auth, credential_auth, metrics, "localhost", true, None);

    let result = registration_service.get_profile(&user_id).await;
    assert!(result.is_ok());
    let val = result.unwrap();
    assert_eq!(val["displayname"], "Alice");
}

/// PERF-02 门禁：注册的 5 次写入（user / displayname / device / access_token /
/// refresh_token）必须落在**同一个事务**里。
///
/// 失败注入：在本次测试独占的隔离 schema 内给 `refresh_tokens` 加一条恒假 CHECK
/// 约束，使"签发 refresh token"这一步必然失败。注册必须整体回滚——`users` 与
/// `devices` 里不得留下任何残留行（当前实现在第 1 步就已提交，因此本用例会先红）。
#[tokio::test]
async fn register_is_atomic() {
    let pool = crate::require_test_pool().await;

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
        ..Default::default()
    };
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let canonical_cache = cache.clone();
    let metrics = Arc::new(MetricsCollector::new());
    let auth_service = AuthService::new(&pool, canonical_cache.clone(), metrics.clone(), &security, "localhost");
    let user_store: Arc<dyn UserStore> = Arc::new(UserStorage::new(&pool, canonical_cache));
    let user_service = Arc::new(UserService::new(user_store.clone()));
    let auth_arc = Arc::new(auth_service);
    let token_auth: Arc<dyn TokenAuth> = auth_arc.clone();
    let credential_auth: Arc<dyn CredentialAuth> = auth_arc;
    let registration_service =
        RegistrationService::new(user_service, token_auth, credential_auth, metrics, "localhost", true, None);

    sqlx::query("ALTER TABLE refresh_tokens ADD CONSTRAINT register_is_atomic_no_insert CHECK (false)")
        .execute(&*pool)
        .await
        .expect("failed to inject a refresh_tokens write failure");

    let id = unique_id();
    let username = format!("atomic_{}", id);
    let user_id = format!("@{}:localhost", username);

    let result = registration_service.register_user(&username, "Password123!", Some("Atomic"), None).await;
    assert!(result.is_err(), "refresh token 写入失败时注册必须返回错误");

    let users: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE user_id = $1")
        .bind(&user_id)
        .fetch_one(&*pool)
        .await
        .expect("failed to count residual users");
    assert_eq!(users, 0, "注册失败后 users 不得残留行（写路径必须原子）");

    let devices: i64 = sqlx::query_scalar("SELECT count(*) FROM devices WHERE user_id = $1")
        .bind(&user_id)
        .fetch_one(&*pool)
        .await
        .expect("failed to count residual devices");
    assert_eq!(devices, 0, "注册失败后 devices 不得残留行（写路径必须原子）");
}

// ─────────────────────────────────────────────────────────────────────────────
// `RegistrationService::login` 的指标语义（§3.1.7.9 A 组最后一条）
//
// 背景：CI 的 lcov 把 `login` 的**签名块**（135–140，`pub async fn login(` 与 5 个
// 参数行）记成 0 命中，而函数体开括号与结尾 `}` 是 1、函数体 142–173 干脆没有任何
// region —— 这是 `#[tracing::instrument]` 展开后的插桩归属问题，不是"某个分支没跑"
// （证据见 `docs/audit/TEST_COMPROMISE_AUDIT_2026-10-25.md` §3.1.7.13）。
//
// 那 6 行本身没有行为可断言，但**它们包着的行为确实一条断言都没有**：
// `login` 在整仓只有 1 个调用点，没人验过"首次调用要注册指标""失败也要记耗时"
// "失败不得计成功数"。下面三条钉的就是这些。
// ─────────────────────────────────────────────────────────────────────────────

/// 构造一个带**全新** `MetricsCollector` 的 `RegistrationService`，并把 collector
/// 一并返回 —— 下面几条要读的就是这份注册表。
///
/// 必须每次 `MetricsCollector::new()`：复用实例会让 `get_histogram` 提前命中
/// `Some` 分支，"首次使用才注册"这条语义就永远测不到。
fn build_service(pool: &Arc<sqlx::PgPool>) -> (RegistrationService, Arc<MetricsCollector>) {
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
        ..Default::default()
    };
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let metrics = Arc::new(MetricsCollector::new());
    let auth_service = AuthService::new(pool, cache.clone(), metrics.clone(), &security, "localhost");
    let user_store: Arc<dyn UserStore> = Arc::new(UserStorage::new(pool, cache));
    let user_service = Arc::new(UserService::new(user_store));
    let auth_arc = Arc::new(auth_service);
    let token_auth: Arc<dyn TokenAuth> = auth_arc.clone();
    let credential_auth: Arc<dyn CredentialAuth> = auth_arc;
    let service =
        RegistrationService::new(user_service, token_auth, credential_auth, metrics.clone(), "localhost", true, None);
    (service, metrics)
}

/// 首次登录必须**新建**两个指标，且各只记一次。
///
/// 变异：把两个 `else` 分支删掉（只留 `if let Some(..)`）⇒ 全新 collector 里什么都不会注册，
/// 本用例在 `expect("...必须注册...")` 处红。
#[tokio::test]
async fn login_registers_login_metrics_on_first_use() {
    let pool = crate::require_test_pool().await;
    let (service, metrics) = build_service(&pool);

    // 前置断言：这两个名字此刻必须都不存在，否则下面的断言证明不了"首次使用才注册"。
    assert!(metrics.get_histogram("login_duration_seconds").is_none(), "全新 collector 里不应该已经有 login 直方图");
    assert!(metrics.get_counter("login_success_total").is_none(), "全新 collector 里不应该已经有 login 计数器");

    let username = format!("metrics_{}", unique_id());
    service.register_user(&username, "Password123!", None, None).await.unwrap();

    // 注册走的是另外两个名字 ⇒ 更不能顺手把 login 的指标建出来。
    assert!(
        metrics.get_histogram("login_duration_seconds").is_none(),
        "register_user 不得提前创建 login 的直方图，否则 login 的注册分支永远测不到"
    );

    service.login(&username, "Password123!", None, None).await.unwrap();

    let histogram = metrics.get_histogram("login_duration_seconds").expect("首次登录必须注册耗时直方图");
    assert_eq!(histogram.get_count(), 1, "一次登录只应观察一次耗时");
    let counter = metrics.get_counter("login_success_total").expect("首次成功登录必须注册成功计数器");
    assert_eq!(counter.get(), 1, "一次成功登录只应计一次");
}

/// 登录失败：**仍要**记录耗时（否则面板上看不到失败请求的延迟），
/// 但**不得**创建或自增成功计数器。
///
/// 变异：把 `let (user, ..) = result?;` 挪到计数器自增之后 ⇒
/// `login_success_total` 会在失败时被注册/自增，本用例第二条断言红。
#[tokio::test]
async fn login_failure_records_duration_but_never_counts_a_success() {
    let pool = crate::require_test_pool().await;
    let (service, metrics) = build_service(&pool);

    let username = format!("metfail_{}", unique_id());
    service.register_user(&username, "Password123!", None, None).await.unwrap();

    let failed = service.login(&username, "WrongPassword99!", None, None).await;
    assert!(failed.is_err(), "口令错误必须返回错误");

    let histogram = metrics.get_histogram("login_duration_seconds").expect("失败的登录同样要记录耗时");
    assert_eq!(histogram.get_count(), 1, "失败路径也必须 observe 一次，否则失败延迟在面板上不可见");
    assert!(
        metrics.get_counter("login_success_total").is_none(),
        "登录失败不得创建 login_success_total —— `result?` 必须早于计数器自增"
    );
}

/// 第二次登录必须**复用**已注册的指标句柄，而不是重新注册（重注册会把计数清零）。
///
/// 变异：把 `if let Some(hist)` 也改成无条件 `register_histogram` ⇒
/// 第二次调用后 `get_count()` 是 1 而不是 2，本用例红。
#[tokio::test]
async fn login_reuses_the_registered_metrics_on_subsequent_calls() {
    let pool = crate::require_test_pool().await;
    let (service, metrics) = build_service(&pool);

    let username = format!("metreuse_{}", unique_id());
    service.register_user(&username, "Password123!", None, None).await.unwrap();

    service.login(&username, "Password123!", None, None).await.unwrap();
    service.login(&username, "Password123!", None, None).await.unwrap();

    let histogram = metrics.get_histogram("login_duration_seconds").expect("直方图必须存在");
    assert_eq!(histogram.get_count(), 2, "两次登录累计两次观察；得 1 说明每次都在重新注册（计数被清零）");
    let counter = metrics.get_counter("login_success_total").expect("成功计数器必须存在");
    assert_eq!(counter.get(), 2, "两次成功登录累计 2；得 1 说明计数器被重新注册清零");
}
