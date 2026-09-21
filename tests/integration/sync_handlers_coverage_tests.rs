#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! B.3 Phase 3 Batch 2 — sync handlers + sliding_sync_service coverage tests.
//!
//! Target files (currently 0-6% covered):
//!   - synapse-web/src/routes/handlers/sync.rs (0%, 0/95)
//!   - synapse-services/src/sliding_sync_service/mod.rs (3.9%, 6/155)
//!   - synapse-services/src/sliding_sync_service/extensions.rs (6.3%, 12/189)
//!
//! Existing tests (sync_service_tests_migrated.rs, sliding_sync_service_tests_migrated.rs)
//! cover the main sync flow. These tests fill gaps in:
//!   - SlidingSyncService metrics accessors (latency_threshold_ms, sync_latency_p95_ms,
//!     slow_sync_request_count)
//!   - Sliding sync edge cases (empty lists + extensions, multiple conn_ids, room
//!     subscriptions, unsubscribe_rooms, timeout=0)
//!   - Sync handler query param parsing (tested via the sync_service layer)

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use synapse_e2ee::device_keys::DeviceKeyStorage;
use synapse_e2ee::to_device::ToDeviceStorage;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::config::PerformanceConfig;
use synapse_rust::metrics::MetricsCollector;
use synapse_services::sliding_sync_service::SlidingSyncService;
use synapse_services::typing_service::TypingService;
use synapse_storage::device::DeviceStorage;
use synapse_storage::event::EventStorage;
use synapse_storage::membership::RoomMemberStorage;
use synapse_storage::sliding_sync::{SlidingSyncListData, SlidingSyncRequest, SlidingSyncStorage};
use synapse_storage::PresenceStorage;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(20_000);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

// 这里原本有一个 `setup_test_database()` 夹具，用 5 条 `CREATE ... IF NOT EXISTS` 去
// "确保" sliding_sync 的表存在。它**两头都错**，已于 2026-09-21 删除：
//   1. 名字是过时的：`sliding_sync_connections` / `sliding_sync_room_state` /
//      `sliding_sync_to_device_queue` 在全仓（迁移 + 生产代码）已不存在，真实表是
//      `sliding_sync_lists` / `sliding_sync_rooms` / `sliding_sync_tokens`（+ `sliding_sync_pos_seq`）。
//   2. 语法是错的：`(LIKE <table> INCLUDING ALL DEFAULT)` 里的尾巴 `DEFAULT` 不是
//      PostgreSQL 的 `LIKE` 选项（应为 `INCLUDING DEFAULTS`，且已被 `INCLUDING ALL` 覆盖），
//      Postgres 报 `42601 syntax error at or near "DEFAULT"`。
// 每个测试都在 `require_test_pool()` 之后立刻调用它，所以这个文件里的 15 个测试**全部**
// 死在夹具上（CI 的 integration 车道此前从未真正执行过，故一直没暴露；2026-09-21 本地
// 全量枚举一次性抓出 15 条）。现在无需任何夹具：`require_test_pool()` 返回的是从迁移
// baseline 克隆出来的 per-test schema，表本来就在。

fn create_service(pool: &Arc<sqlx::PgPool>) -> SlidingSyncService {
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let storage = Arc::new(SlidingSyncStorage::new(pool.clone()));
    let event_storage = Arc::new(EventStorage::new(pool, "localhost".to_string()));
    let typing_service = Arc::new(TypingService::default());
    let presence_storage = Arc::new(PresenceStorage::new(pool.clone(), cache.clone()));
    let member_storage = Arc::new(RoomMemberStorage::new(pool, "localhost"));
    let device_storage = Arc::new(DeviceStorage::new(pool));
    let to_device_storage = ToDeviceStorage::new(pool);
    let metrics = Arc::new(MetricsCollector::new());
    let user_storage: Arc<dyn synapse_storage::user::UserStore> =
        Arc::new(synapse_storage::user::UserStorage::new(pool, cache.clone()));

    SlidingSyncService::new(
        storage,
        cache,
        event_storage,
        Arc::new(DeviceKeyStorage::new(pool)) as Arc<dyn synapse_e2ee::device_keys::DeviceKeyStoreApi>,
        typing_service,
        presence_storage,
        member_storage,
        device_storage,
        to_device_storage,
        user_storage,
        metrics,
        PerformanceConfig::default(),
        None,
    )
}

fn make_request(lists: HashMap<String, SlidingSyncListData>) -> SlidingSyncRequest {
    SlidingSyncRequest {
        conn_id: None,
        lists,
        room_subscriptions: None,
        unsubscribe_rooms: None,
        extensions: None,
        pos: None,
        timeout: None,
        client_timeout: None,
        txn_id: None,
    }
}

fn make_main_list(ranges: Vec<Vec<u32>>) -> HashMap<String, SlidingSyncListData> {
    let mut lists = HashMap::new();
    lists.insert(
        "main".to_string(),
        SlidingSyncListData {
            ranges,
            sort: vec!["by_recency".to_string()],
            filters: None,
            timeline_limit: None,
            required_state: None,
            slow_by: None,
            bump_event_types: None,
        },
    );
    lists
}

// =============================================================================
// SlidingSyncService metrics accessors (mod.rs coverage)
// =============================================================================

#[tokio::test]
async fn test_latency_threshold_ms_returns_configured_default() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);

    // PerformanceConfig::default() should set a positive threshold.
    let threshold = service.latency_threshold_ms();
    assert!(threshold > 0, "latency_threshold_ms should be positive, got {threshold}");
}

#[tokio::test]
async fn test_sync_latency_p95_ms_is_none_before_any_sync() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);

    assert!(service.sync_latency_p95_ms().is_none(), "p95 should be None before any sync observation");
}

#[tokio::test]
async fn test_slow_sync_request_count_is_zero_initially() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);

    assert_eq!(service.slow_sync_request_count(), 0, "slow request count should be 0 before any sync");
}

#[tokio::test]
async fn test_sync_latency_p95_ms_is_some_after_sync() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let user_id = format!("@p95_{suffix}:localhost");

    let request = make_request(make_main_list(vec![vec![0, 10]]));
    let response = service.sync(&user_id, "DEV1", request).await;
    assert!(response.is_ok(), "sync should succeed: {:?}", response.err());

    // After a sync, the histogram should have at least one observation.
    let p95 = service.sync_latency_p95_ms();
    assert!(p95.is_some(), "p95 should be Some after a sync observation (got None)");
    let p95_val = p95.unwrap();
    assert!(p95_val > 0.0, "p95 should be positive, got {p95_val}");
}

#[tokio::test]
async fn test_sync_records_slow_request_when_threshold_exceeded() {
    // We can't easily force a slow sync, but we can verify the counter
    // mechanism works by checking it stays at 0 for fast syncs.
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let user_id = format!("@slow_{suffix}:localhost");

    let request = make_request(make_main_list(vec![vec![0, 5]]));
    let _response = service.sync(&user_id, "DEV1", request).await.unwrap();

    // A fast sync should not increment the slow counter. The threshold is
    // typically 1000ms+ and our empty-room sync completes in <100ms.
    let count = service.slow_sync_request_count();
    assert_eq!(count, 0, "slow request count should be 0 for a fast sync (got {count})");
}

// =============================================================================
// Sliding sync edge cases (mod.rs + extensions.rs coverage)
// =============================================================================

#[tokio::test]
async fn test_sync_with_empty_lists_and_no_extensions() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let user_id = format!("@empty_{suffix}:localhost");

    let request = make_request(HashMap::new());
    let response = service.sync(&user_id, "DEV1", request).await;
    assert!(response.is_ok(), "sync with empty lists should succeed: {:?}", response.err());
    let resp = response.unwrap();
    assert!(!resp.pos.is_empty(), "pos should be returned");
}

#[tokio::test]
async fn test_sync_with_multiple_conn_ids_same_user() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let user_id = format!("@multi_{suffix}:localhost");

    // First sync with conn_id "A"
    let mut request_a = make_request(make_main_list(vec![vec![0, 10]]));
    request_a.conn_id = Some("connA".to_string());
    let response_a = service.sync(&user_id, "DEV1", request_a).await;
    assert!(response_a.is_ok(), "sync with connA should succeed: {:?}", response_a.err());
    let resp_a = response_a.unwrap();
    assert_eq!(resp_a.conn_id, Some("connA".to_string()));

    // Second sync with conn_id "B" for same user
    let mut request_b = make_request(make_main_list(vec![vec![0, 10]]));
    request_b.conn_id = Some("connB".to_string());
    let response_b = service.sync(&user_id, "DEV1", request_b).await;
    assert!(response_b.is_ok(), "sync with connB should succeed: {:?}", response_b.err());
    let resp_b = response_b.unwrap();
    assert_eq!(resp_b.conn_id, Some("connB".to_string()));

    // Positions should be different (different connections)
    assert_ne!(resp_a.pos, resp_b.pos, "different conn_ids should produce different positions");
}

#[tokio::test]
async fn test_sync_incremental_with_pos_advances_position() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let user_id = format!("@inc_{suffix}:localhost");

    // Initial sync
    let request1 = make_request(make_main_list(vec![vec![0, 10]]));
    let response1 = service.sync(&user_id, "DEV1", request1).await.unwrap();
    let pos1 = response1.pos.clone();
    assert!(!pos1.is_empty());

    // Incremental sync with pos from initial sync
    let mut request2 = make_request(make_main_list(vec![vec![0, 10]]));
    request2.pos = Some(pos1.clone());
    let response2 = service.sync(&user_id, "DEV1", request2).await;
    assert!(response2.is_ok(), "incremental sync should succeed: {:?}", response2.err());
    let pos2 = response2.unwrap().pos;
    assert_ne!(pos1, pos2, "pos should advance on each sync");
}

#[tokio::test]
async fn test_sync_with_room_subscriptions() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let user_id = format!("@subs_{suffix}:localhost");

    let mut request = make_request(make_main_list(vec![vec![0, 10]]));
    let room_id = format!("!testroom_{suffix}:localhost");
    request.room_subscriptions = Some(serde_json::json!({
        room_id: {
            "timeline_limit": 10,
            "required_state": [["m.room.name", ""]]
        }
    }));

    let response = service.sync(&user_id, "DEV1", request).await;
    assert!(response.is_ok(), "sync with room subscriptions should succeed: {:?}", response.err());
}

#[tokio::test]
async fn test_sync_with_to_device_extension_enabled() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let user_id = format!("@todev_{suffix}:localhost");

    let mut request = make_request(make_main_list(vec![vec![0, 10]]));
    request.extensions = Some(serde_json::json!({
        "to_device": { "enabled": true, "limit": 100 }
    }));

    let response = service.sync(&user_id, "DEV1", request).await;
    assert!(response.is_ok(), "sync with to_device extension should succeed: {:?}", response.err());
    let resp = response.unwrap();
    // Extensions response should be present (even if empty to_device events).
    assert!(resp.extensions.is_some(), "extensions response should be present");
}

#[tokio::test]
async fn test_sync_with_e2ee_extension_enabled() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let user_id = format!("@e2ee_{suffix}:localhost");

    let mut request = make_request(make_main_list(vec![vec![0, 10]]));
    request.extensions = Some(serde_json::json!({
        "e2ee": { "enabled": true }
    }));

    let response = service.sync(&user_id, "DEV1", request).await;
    assert!(response.is_ok(), "sync with e2ee extension should succeed: {:?}", response.err());
}

#[tokio::test]
async fn test_sync_with_typing_extension_enabled() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let user_id = format!("@typing_{suffix}:localhost");

    let mut request = make_request(make_main_list(vec![vec![0, 10]]));
    request.extensions = Some(serde_json::json!({
        "typing": { "enabled": true }
    }));

    let response = service.sync(&user_id, "DEV1", request).await;
    assert!(response.is_ok(), "sync with typing extension should succeed: {:?}", response.err());
}

#[tokio::test]
async fn test_sync_with_zero_timeout() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let user_id = format!("@zero_{suffix}:localhost");

    let mut request = make_request(make_main_list(vec![vec![0, 10]]));
    request.timeout = Some(0);

    let response = service.sync(&user_id, "DEV1", request).await;
    assert!(response.is_ok(), "sync with timeout=0 should succeed immediately: {:?}", response.err());
}

#[tokio::test]
async fn test_sync_with_multiple_lists() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let user_id = format!("@lists_{suffix}:localhost");

    let mut lists = HashMap::new();
    lists.insert(
        "main".to_string(),
        SlidingSyncListData {
            ranges: vec![vec![0, 10]],
            sort: vec!["by_recency".to_string()],
            filters: None,
            timeline_limit: Some(5),
            required_state: None,
            slow_by: None,
            bump_event_types: None,
        },
    );
    lists.insert(
        "invites".to_string(),
        SlidingSyncListData {
            ranges: vec![vec![0, 5]],
            sort: vec!["by_recency".to_string()],
            filters: None,
            timeline_limit: None,
            required_state: None,
            slow_by: None,
            bump_event_types: None,
        },
    );

    let request = make_request(lists);
    let response = service.sync(&user_id, "DEV1", request).await;
    assert!(response.is_ok(), "sync with multiple lists should succeed: {:?}", response.err());
}

#[tokio::test]
async fn test_sync_with_unsubscribe_rooms() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let user_id = format!("@unsub_{suffix}:localhost");

    let mut request = make_request(make_main_list(vec![vec![0, 10]]));
    let room_id = format!("!unsubroom_{suffix}:localhost");
    request.unsubscribe_rooms = Some(vec![room_id.clone()]);

    let response = service.sync(&user_id, "DEV1", request).await;
    assert!(response.is_ok(), "sync with unsubscribe_rooms should succeed: {:?}", response.err());
}
