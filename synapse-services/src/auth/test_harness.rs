//! 测试专用 harness：基于内存 mock 构建 `AuthService`（无 DB、无 Redis）。
//!
//! 用于 S4（令牌撤销检查缓存）与 T1（power_levels 授权）的单元测试。
//! `room_storage` 为具体类型，使用 `connect_lazy_with` 懒连接池——
//! 测试用例通过 `m.room.create` 事件提供创建者信息（见
//! `AuthService::resolve_room_creator`），不会真正触达数据库。

use super::*;
use synapse_storage::test_mocks::{
    InMemoryAccessTokenStore, InMemoryDeviceListStore, InMemoryEventStore, InMemoryMemberStore,
    InMemoryRefreshTokenStore,
};
use synapse_storage::FakeUserStore;

/// 测试 harness：持有 service 及各 mock 存储的句柄，便于测试直接操纵状态。
pub(crate) struct TestAuthHarness {
    pub service: AuthService,
    pub user_store: FakeUserStore,
    pub token_store: InMemoryAccessTokenStore,
    pub member_store: InMemoryMemberStore,
    pub event_store: InMemoryEventStore,
    pub cache: Arc<CacheManager>,
}

/// 构建一个完全基于内存 mock 的 AuthService。
///
/// 注意：需在 tokio runtime 内调用（connect_lazy_with 需要 runtime 上下文），
/// 即只能在 #[tokio::test] 中使用。
pub(crate) fn build_test_auth_service() -> TestAuthHarness {
    let pool =
        Arc::new(sqlx::postgres::PgPoolOptions::new().connect_lazy_with(sqlx::postgres::PgConnectOptions::new()));
    let cache = Arc::new(CacheManager::new(&synapse_cache::CacheConfig::default()));

    let user_store = FakeUserStore::new();
    let user_storage: Arc<dyn UserStore> = Arc::new(user_store.clone());
    let user_service = Arc::new(crate::UserService::new(user_storage.clone()));

    let token_store = InMemoryAccessTokenStore::new();
    let member_store = InMemoryMemberStore::new();
    let event_store = InMemoryEventStore::new();

    let service = AuthService {
        user_storage,
        user_service,
        device_storage: Arc::new(InMemoryDeviceListStore::new()),
        token_storage: Arc::new(token_store.clone()),
        refresh_token_storage: Arc::new(InMemoryRefreshTokenStore::new()),
        room_storage: RoomStorage::new(&pool),
        member_storage: Arc::new(member_store.clone()),
        event_reader: Arc::new(event_store.clone()),
        cache: cache.clone(),
        metrics: Arc::new(MetricsCollector::new()),
        validator: Arc::new(Validator::default()),
        jwt_secret: b"test-jwt-secret-for-auth-harness-32bytes!".to_vec(),
        token_expiry: 3600,
        refresh_token_expiry: 86400,
        server_name: "test.server".to_string(),
        argon2_m_cost: 65536,
        argon2_t_cost: 3,
        argon2_p_cost: 1,
        allow_legacy_hashes: false,
        login_failure_lockout_threshold: 5,
        login_lockout_duration_seconds: 300,
        mas_validator: None,
        audit_storage: None,
    };

    TestAuthHarness { service, user_store, token_store, member_store, event_store, cache }
}
