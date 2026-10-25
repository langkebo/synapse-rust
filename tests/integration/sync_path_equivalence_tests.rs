#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! ARCH-02 等价断言 —— `/sync` 与滑动同步（MSC4186）在**同一房间状态**下必须给出
//! 一致的「可见事件集合」。
//!
//! 为什么需要这组测试：两条路径各有一份 timeline / state 组装实现，**没有共享基座**，
//! 每改一处语义就要在两个服务里各写一遍；已经发生过语义漂移（limited timeline 重放成员
//! 变更只修了其中一条）。审计结论是「没有断言就不要抽 —— 那是重写而不是重构」，所以这
//! 组断言存在的目的不是覆盖端点，而是**给后续的抽取工作当安全网**：
//!
//! 1. `initial_timeline_is_identical_across_both_paths` —— 完整时间线的**事件集合 + 顺序**；
//! 2. `truncation_keeps_the_same_tail` —— 裁剪方向（两条路径都必须是「取最新 N 条、升序输出」）；
//! 3. `truncation_is_reported_as_limited_on_both_paths` —— `limited` 语义（被裁剪时必须为真）。
//!
//! 夹具是**一份真实的房间状态**（真 `RoomService` 建房、真 `EventStorage` 落事件），两条
//! 路径共用同一个 pool。滑动侧的房间行**不手工播种** —— 必须由生产路径
//! `materialize_room_from_activity` 从 `room_memberships` + `events` 现算出来，否则断言就
//! 退化成「两边读同一份手写数据」，测不出漂移。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};
use sqlx::{Pool, Postgres};
use synapse_common::current_timestamp_millis;
use synapse_e2ee::device_keys::DeviceKeyStorage;
use synapse_e2ee::key_rotation::KeyRotationStorage;
use synapse_e2ee::to_device::ToDeviceStorage;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::common::config::PerformanceConfig;
use synapse_rust::common::metrics::MetricsCollector;
use synapse_rust::common::Validator;
use synapse_services::room::service::{CreateRoomConfig, RoomService};
use synapse_services::room::summary::RoomSummaryService;
use synapse_services::sliding_sync_service::SlidingSyncService;
use synapse_services::sync_service::SyncService;
use synapse_services::typing_service::TypingService;
use synapse_storage::device::DeviceStorage;
use synapse_storage::event::EventStorage;
use synapse_storage::membership::RoomMemberStorage;
use synapse_storage::room::RoomStorage;
use synapse_storage::room_summary::RoomSummaryStorage;
use synapse_storage::sliding_sync::{SlidingSyncRequest, SlidingSyncStorage};
use synapse_storage::sticky_event::StickyEventStorage;
use synapse_storage::user::{UserStorage, UserStore};
use synapse_storage::{
    AccountDataStorage, CreateFilterRequest, FilterStorage, PresenceStorage, RoomAccountDataStorage,
};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

async fn create_test_user(pool: &Pool<Postgres>, user_id: &str, username: &str) {
    sqlx::query(
        r#"
        INSERT INTO users (user_id, username, created_ts)
        VALUES ($1, $2, $3)
        ON CONFLICT (user_id) DO NOTHING
        "#,
    )
    .bind(user_id)
    .bind(username)
    .bind(current_timestamp_millis())
    .execute(pool)
    .await
    .expect("创建测试用户失败");
}

fn create_room_service(
    pool: &Arc<Pool<Postgres>>,
    room_storage: Arc<RoomStorage>,
    member_storage: Arc<RoomMemberStorage>,
    event_storage: Arc<EventStorage>,
    user_storage: Arc<dyn UserStore>,
) -> RoomService {
    let room_summary_storage = Arc::new(RoomSummaryStorage::new(pool));
    let room_summary_service =
        Arc::new(RoomSummaryService::new(room_summary_storage, event_storage.clone(), Some(member_storage.clone())));
    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let invite_policy_gate = Arc::new(synapse_services::invite_blocklist_service::InviteBlocklistService::new(
        Arc::new(synapse_storage::invite_blocklist::InviteBlocklistStorage::new(pool.clone())),
        Arc::new(AccountDataStorage::new(pool)),
    ));

    RoomService::new(synapse_services::room::service::RoomServiceConfig {
        room_storage,
        member_storage,
        event_reader: Some(event_storage.clone()),
        event_writer: Some(event_storage),
        room_tag_storage: Arc::new(synapse_storage::room_tag::RoomTagStorage::new(pool.clone())),
        user_storage,
        room_auth: Arc::new(synapse_services::auth::AuthService::new(
            pool,
            cache.clone(),
            Arc::new(MetricsCollector::new()),
            &synapse_rust::common::config::SecurityConfig::default(),
            "localhost",
        )),
        room_summary_service,
        validator: Arc::new(Validator::default()),
        server_name: "localhost".to_string(),
        task_queue: None,
        relations_storage: Arc::new(synapse_storage::relations::RelationsStorage::new(pool)),
        event_broadcaster: Some(Arc::new(synapse_federation::event_broadcaster::EventBroadcaster::new(
            "localhost".to_string(),
        ))),
        app_service_manager: None,
        key_rotation_manager: None,
        federation_client: None,
        beacon_service: None,
        sticky_event_storage: Arc::new(StickyEventStorage::new(pool.clone())),
        cache,
        key_rotation_storage: None,
        db_pool: None,
        policy_service: None,
        invite_policy_gate,
        event_admission_gate: Arc::new(synapse_services::test_mocks::FakeEventAdmissionGate::new()),
    })
}

/// 两条路径共用的**一份真实房间状态**。
struct SharedFixture {
    pool: Arc<Pool<Postgres>>,
    user_id: String,
    room_id: String,
    /// 主动发出的消息 `event_id`，按发送顺序。
    sent: Vec<String>,
    sync_service: SyncService,
    sliding_service: SlidingSyncService,
    /// 留着以便按**生产路径**（invite → join）往房间里加第二个成员。
    room_service: RoomService,
}

impl SharedFixture {
    /// 建房 + 发 `messages` 条消息，并在**同一个 pool** 上构造两个服务。
    async fn build(messages: usize) -> Self {
        let pool = crate::require_test_pool().await;
        let id = unique_id();
        let user_id = format!("@equiv_{id}:localhost");
        create_test_user(&pool, &user_id, &format!("equiv_{id}")).await;

        let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
        let presence_storage = Arc::new(PresenceStorage::new(pool.clone(), cache.clone()));
        let member_storage = Arc::new(RoomMemberStorage::new(&pool, "localhost"));
        let event_storage = Arc::new(EventStorage::new(&pool, "localhost".to_string()));
        let room_storage = Arc::new(RoomStorage::new(&pool));
        let user_storage: Arc<dyn UserStore> = Arc::new(UserStorage::new(&pool, cache.clone()));

        let room_service = create_room_service(
            &pool,
            room_storage.clone(),
            member_storage.clone(),
            event_storage.clone(),
            user_storage.clone(),
        );

        let sync_service = SyncService::new(
            presence_storage.clone(),
            member_storage.clone(),
            event_storage.clone(),
            Arc::new(RoomAccountDataStorage::new(&pool)),
            Arc::new(AccountDataStorage::new(&pool)),
            Arc::new(FilterStorage::new(&pool)),
            Arc::new(DeviceStorage::new(&pool)),
            Arc::new(DeviceKeyStorage::new(&pool)) as Arc<dyn synapse_e2ee::device_keys::DeviceKeyStoreApi>,
            KeyRotationStorage::new(pool.clone()),
            ToDeviceStorage::new(&pool),
            Arc::new(MetricsCollector::new()),
            PerformanceConfig::default(),
            cache.clone(),
            None,
        );

        let sliding_service = SlidingSyncService::new(
            Arc::new(SlidingSyncStorage::new(pool.clone())),
            cache.clone(),
            event_storage,
            Arc::new(DeviceKeyStorage::new(&pool)) as Arc<dyn synapse_e2ee::device_keys::DeviceKeyStoreApi>,
            Arc::new(TypingService::default()),
            presence_storage,
            member_storage,
            Arc::new(DeviceStorage::new(&pool)),
            ToDeviceStorage::new(&pool),
            user_storage,
            Arc::new(MetricsCollector::new()),
            PerformanceConfig::default(),
            None,
        );

        let config = CreateRoomConfig { name: Some(format!("Equivalence {id}")), ..Default::default() };
        let room_val = room_service.lifecycle.create_room(&user_id, config).await.unwrap();
        let room_id = room_val["room_id"].as_str().unwrap().to_string();

        let mut sent = Vec::with_capacity(messages);
        for index in 0..messages {
            let content = json!({ "msgtype": "m.text", "body": format!("message {index}") });
            let event =
                room_service.messaging.send_message(&room_id, &user_id, "m.room.message", &content).await.unwrap();
            sent.push(event["event_id"].as_str().expect("send_message 必须返回 event_id").to_string());
        }

        Self { pool, user_id, room_id, sent, sync_service, sliding_service, room_service }
    }

    /// 按**生产路径**加第二个成员（invite → join），返回其 user_id。
    ///
    /// 不手写 `room_memberships` 行：懒加载的可见成员集正是按"谁是时间线里的发送者 /
    /// 谁的 membership 事件落在窗口内"算出来的，手工播种会让断言退化。
    async fn add_member(&self, suffix: &str) -> String {
        let user_id = format!("@equiv_peer_{suffix}:localhost");
        create_test_user(&self.pool, &user_id, &format!("equiv_peer_{suffix}")).await;
        self.room_service.membership.invite_user(&self.room_id, &self.user_id, &user_id, None).await.expect("邀请失败");
        self.room_service.membership.join_room(&self.room_id, &user_id).await.expect("入房失败");
        user_id
    }

    /// 再发一条消息（走生产的 `MessagingService::send_message`）。
    async fn send(&self, body: &str) {
        let content = json!({ "msgtype": "m.text", "body": body });
        self.room_service
            .messaging
            .send_message(&self.room_id, &self.user_id, "m.room.message", &content)
            .await
            .expect("追加消息失败");
    }

    async fn sync(&self, filter_id: Option<&str>) -> Value {
        self.sync_from(filter_id, None, false, false).await
    }

    /// `/sync`，可给 `since`（增量）与 MSC4222 开关。
    async fn sync_from(
        &self,
        filter_id: Option<&str>,
        since: Option<&str>,
        use_state_after: bool,
        unstable: bool,
    ) -> Value {
        self.sync_service
            .sync(&self.user_id, None, 0, false, "online", filter_id, since, use_state_after, unstable)
            .await
            .expect("/sync 失败")
    }

    async fn sync_with_state_after(&self, filter_id: Option<&str>, use_state_after: bool, unstable: bool) -> Value {
        self.sync_from(filter_id, None, use_state_after, unstable).await
    }

    async fn sliding(&self, timeline_limit: u32) -> Value {
        self.sliding_with_required_state(timeline_limit, None).await
    }

    /// 滑动同步，可显式给 `required_state`（MSC4186：`[[type, state_key], …]`，`*` 为通配）。
    async fn sliding_with_required_state(&self, timeline_limit: u32, required_state: Option<Value>) -> Value {
        let mut subscription = json!({ "timeline_limit": timeline_limit });
        if let Some(required_state) = required_state {
            subscription["required_state"] = required_state;
        }
        let mut subscriptions = serde_json::Map::new();
        subscriptions.insert(self.room_id.clone(), subscription);
        let request = SlidingSyncRequest {
            conn_id: None,
            lists: HashMap::new(),
            room_subscriptions: Some(Value::Object(subscriptions)),
            unsubscribe_rooms: None,
            extensions: None,
            pos: None,
            timeout: None,
            client_timeout: None,
            txn_id: None,
        };
        self.sliding_service.sync(&self.user_id, "DEV1", request).await.expect("滑动同步失败").rooms
    }

    /// 存一个只限制 timeline 的 filter，返回 filter_id。
    async fn store_timeline_filter(&self, limit: u32) -> String {
        self.store_filter(json!({ "room": { "timeline": { "limit": limit } } })).await
    }

    /// 存任意 filter 内容，返回 filter_id。
    async fn store_filter(&self, content: Value) -> String {
        let filter_id = format!("equiv-{}", unique_id());
        FilterStorage::new(&self.pool)
            .create_filter(CreateFilterRequest { user_id: self.user_id.clone(), filter_id: filter_id.clone(), content })
            .await
            .expect("写 filter 失败");
        filter_id
    }
}

/// `/sync`：`rooms.join[room_id].timeline.events[].event_id`（响应内为升序）。
fn sync_event_ids(sync_response: &Value, room_id: &str) -> Vec<String> {
    sync_response["rooms"]["join"][room_id]["timeline"]["events"]
        .as_array()
        .unwrap_or_else(|| panic!("/sync 的 rooms.join[{room_id}].timeline.events 不是数组：{sync_response}"))
        .iter()
        .map(|event| event["event_id"].as_str().unwrap_or_default().to_string())
        .collect()
}

fn sync_timeline_limited(sync_response: &Value, room_id: &str) -> bool {
    sync_response["rooms"]["join"][room_id]["timeline"]["limited"].as_bool().unwrap_or(false)
}

/// 滑动同步：`rooms.join[room_id].timeline[].event_id`（`timeline` 是数组，不是 `{events: []}`）。
///
/// ⚠️ NEW-17（`2377043e5`）之后 `rooms` 已按 MSC3575/MSC4186 分桶成 `{join, invite, leave}`，
/// 本文件的三条 ARCH-02 用例当时没跟着改，从那一笔起就一直红（产品行为是对的，
/// 是测试侧的读取路径过期）。这里统一收口到 `rooms.join[…]`。
fn sliding_room<'a>(rooms: &'a Value, room_id: &str) -> &'a Value {
    &rooms["join"][room_id]
}

fn sliding_event_ids(rooms: &Value, room_id: &str) -> Vec<String> {
    sliding_room(rooms, room_id)["timeline"]
        .as_array()
        .unwrap_or_else(|| panic!("滑动同步的 rooms.join[{room_id}].timeline 不是数组：{rooms}"))
        .iter()
        .map(|event| event["event_id"].as_str().unwrap_or_default().to_string())
        .collect()
}

fn sliding_limited(rooms: &Value, room_id: &str) -> bool {
    sliding_room(rooms, room_id)["limited"].as_bool().unwrap_or(false)
}

/// 断言 ①：不裁剪时，两条路径的可见事件**序列**完全一致。
#[tokio::test]
async fn initial_timeline_is_identical_across_both_paths() {
    let fixture = SharedFixture::build(3).await;

    let sync_response = fixture.sync(None).await;
    let sliding_rooms = fixture.sliding(50).await;

    let sync_ids = sync_event_ids(&sync_response, &fixture.room_id);
    let sliding_ids = sliding_event_ids(&sliding_rooms, &fixture.room_id);

    // 最低限度的语义：我们主动发出的每条消息，两条路径都必须看得到。
    for event_id in &fixture.sent {
        assert!(sync_ids.contains(event_id), "/sync 的 timeline 里没有 {event_id}（/sync = {sync_ids:?}）");
        assert!(sliding_ids.contains(event_id), "滑动同步的 timeline 里没有 {event_id}（滑动 = {sliding_ids:?}）");
    }

    // 核心断言：完整时间线的逐条一致 + 顺序一致。
    assert_eq!(
        sync_ids, sliding_ids,
        "同一房间状态下两条路径的可见事件序列不一致 —— 这正是 ARCH-02 要拦住的语义漂移。\n\
         /sync  = {sync_ids:?}\n滑动同步 = {sliding_ids:?}"
    );
}

/// 断言 ②：两条路径的裁剪方向一致 —— 都是「取**最新** N 条，升序输出」。
#[tokio::test]
async fn truncation_keeps_the_same_tail() {
    let fixture = SharedFixture::build(5).await;
    let limit = 2;

    let full = sync_event_ids(&fixture.sync(None).await, &fixture.room_id);
    assert!(full.len() > limit, "前置条件：完整时间线要长于裁剪上限（实际 {} 条）", full.len());

    let filter_id = fixture.store_timeline_filter(limit as u32).await;
    let sync_ids = sync_event_ids(&fixture.sync(Some(&filter_id)).await, &fixture.room_id);
    let sliding_ids = sliding_event_ids(&fixture.sliding(limit as u32).await, &fixture.room_id);

    let expected_tail: Vec<String> = full[full.len() - limit..].to_vec();
    assert_eq!(
        sync_ids, expected_tail,
        "/sync 的 timeline 裁剪方向不对（应当保留最新 {limit} 条），完整序列 = {full:?}"
    );
    assert_eq!(
        sliding_ids, expected_tail,
        "滑动同步的 timeline 裁剪方向不对（应当保留最新 {limit} 条），完整序列 = {full:?}"
    );
    assert_eq!(sync_ids, sliding_ids, "两条路径裁剪后的序列不一致");
}

/// 断言 ③：被裁剪时两条路径都必须报 `limited`（否则客户端不会回翻，会静默丢事件）。
#[tokio::test]
async fn truncation_is_reported_as_limited_on_both_paths() {
    let fixture = SharedFixture::build(5).await;
    let limit = 2;

    let filter_id = fixture.store_timeline_filter(limit as u32).await;
    let sync_response = fixture.sync(Some(&filter_id)).await;
    let sliding_rooms = fixture.sliding(limit as u32).await;

    assert!(
        sync_timeline_limited(&sync_response, &fixture.room_id),
        "/sync 把 timeline 裁到 {limit} 条却没有置 limited=true（客户端会静默丢事件）"
    );
    assert!(
        sliding_limited(&sliding_rooms, &fixture.room_id),
        "滑动同步把 timeline 裁到 {limit} 条却没有置 limited=true：{}",
        sliding_rooms[&fixture.room_id]
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// NEW-11：**state 组装层**的等价断言（ARCH-02 只共享了 timeline 层）
//
// 两条路径各有一份"把房间当前状态取出来发给客户端"的实现：
//  * `/sync`：`sync_service/response.rs` 的 `state_by_room` → `apply_sync_filter_to_values`
//    （Matrix `StateFilter`：`types`/`not_types`/…）→ `apply_lazy_load_members`；
//  * 滑动同步：`sliding_sync_service/state.rs::build_required_state_events`
//    （MSC4186 `required_state`：`[[type, state_key], …]`，`*` 通配）。
// 两处的**过滤语言不同**（`StateFilter` vs `[[type,key]]`）、**数据源也不同**
// （`/sync` 走 DB 批次、滑动同步走 `room_state_cache`），所以先钉住"同一房间状态下两边
// 给出的可见状态集合一致"，再谈抽取 —— 与 ARCH-02 同一条原则：没有断言就不抽。
// ─────────────────────────────────────────────────────────────────────────────

/// 从一份 state/required_state 数组里抽出 `(type, state_key)` 集合并排序。
fn state_key_set(events: &[Value]) -> Vec<(String, String)> {
    let mut keys: Vec<(String, String)> = events
        .iter()
        .map(|event| {
            (
                event["type"].as_str().unwrap_or_default().to_string(),
                event["state_key"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    keys.sort();
    keys
}

/// `/sync` 里房间的 state 事件（MSC4222 下键名可能是 `state_after`）。
fn sync_state_events(sync_response: &Value, room_id: &str, field: &str) -> Vec<Value> {
    sync_response["rooms"]["join"][room_id][field]["events"]
        .as_array()
        .unwrap_or_else(|| panic!("/sync 的 rooms.join[{room_id}].{field}.events 不是数组：{sync_response}"))
        .clone()
}

/// 滑动同步里房间的 state 事件（同步实现同时写 `required_state` 与 `state`，这里取前者）。
///
/// NEW-17 之后 `rooms` 是 `{join, invite, leave}` 三桶，房间对象在 `join` 下。
fn sliding_state_events(rooms: &Value, room_id: &str) -> Vec<Value> {
    rooms["join"][room_id]["required_state"]
        .as_array()
        .unwrap_or_else(|| panic!("滑动同步的 rooms.join[{room_id}].required_state 不是数组：{rooms}"))
        .clone()
}

/// 断言 ④（NEW-11）：**不裁剪**时，两条路径给出的可见状态集合完全一致。
#[tokio::test]
async fn full_state_is_identical_across_both_paths() {
    let fixture = SharedFixture::build(2).await;

    let sync_response = fixture.sync(None).await;
    let sliding_rooms = fixture.sliding_with_required_state(50, Some(json!([["*", "*"]]))).await;

    let sync_keys = state_key_set(&sync_state_events(&sync_response, &fixture.room_id, "state"));
    let sliding_keys = state_key_set(&sliding_state_events(&sliding_rooms, &fixture.room_id));

    // 最低限度的语义：建房产生的关键状态，两边都必须给出。
    for required in ["m.room.create", "m.room.member", "m.room.power_levels", "m.room.join_rules"] {
        assert!(
            sync_keys.iter().any(|(event_type, _)| event_type == required),
            "/sync 的 state 里没有 {required}：{sync_keys:?}"
        );
        assert!(
            sliding_keys.iter().any(|(event_type, _)| event_type == required),
            "滑动同步的 required_state 里没有 {required}：{sliding_keys:?}"
        );
    }

    assert_eq!(
        sync_keys, sliding_keys,
        "同一房间状态下两条路径的**可见状态集合**不一致 —— 这正是 NEW-11 要拦住的语义漂移。\n\
         /sync  = {sync_keys:?}\n滑动同步 = {sliding_keys:?}"
    );
}

/// 断言 ⑤（NEW-11）：同一份过滤语义在两种方言下必须落到同一个集合 ——
/// 滑动同步 `required_state = [["m.room.member","*"], ["m.room.name",""]]`
/// ⇔ `/sync` 的 `filter.room.state.types = ["m.room.member", "m.room.name"]`。
#[tokio::test]
async fn state_filter_and_required_state_agree() {
    let fixture = SharedFixture::build(1).await;

    let filter_id = fixture
        .store_filter(json!({
            "room": { "state": { "types": ["m.room.member", "m.room.name"], "lazy_load_members": false } }
        }))
        .await;
    let sync_response = fixture.sync(Some(&filter_id)).await;
    let sliding_rooms =
        fixture.sliding_with_required_state(50, Some(json!([["m.room.member", "*"], ["m.room.name", ""]]))).await;

    let sync_keys = state_key_set(&sync_state_events(&sync_response, &fixture.room_id, "state"));
    let sliding_keys = state_key_set(&sliding_state_events(&sliding_rooms, &fixture.room_id));

    assert!(!sync_keys.is_empty(), "前置条件：filter 不该把 state 过滤成空集（否则这条断言没有意义）");
    assert!(
        sync_keys.iter().all(|(event_type, _)| event_type == "m.room.member" || event_type == "m.room.name"),
        "/sync 返回了 filter 之外的状态类型：{sync_keys:?}"
    );
    assert_eq!(
        sync_keys, sliding_keys,
        "同一份过滤语义在两种方言下给出了不同集合（`state.types` vs `required_state`）：\n\
         /sync  = {sync_keys:?}\n滑动同步 = {sliding_keys:?}"
    );
}

/// 断言 ⑥（NEW-11 / MSC4222）：`state_after` 是**重命名而不是重算** ——
/// 打开后房对象里只有 `state_after`（不再有 `state`），且集合与关闭时逐项相同。
#[tokio::test]
async fn state_after_is_a_rename_not_a_requery() {
    let fixture = SharedFixture::build(2).await;

    let plain = fixture.sync(None).await;
    let renamed = fixture.sync_with_state_after(None, true, false).await;

    let plain_keys = state_key_set(&sync_state_events(&plain, &fixture.room_id, "state"));
    let renamed_keys = state_key_set(&sync_state_events(&renamed, &fixture.room_id, "state_after"));

    let room_object = renamed["rooms"]["join"][&fixture.room_id]
        .as_object()
        .unwrap_or_else(|| panic!("rooms.join[{}] 不是对象：{renamed}", fixture.room_id));
    assert!(
        !room_object.contains_key("state"),
        "MSC4222 打开后房对象里不能再有 `state`（与 `state_after` 二选一）：{room_object:?}"
    );
    assert_eq!(
        plain_keys, renamed_keys,
        "`state_after` 与 `state` 的集合不一致 —— 说明不是同一个 payload 改名，而是另算了一遍。\n\
         state       = {plain_keys:?}\nstate_after = {renamed_keys:?}"
    );

    // 不稳定前缀同样只是改键名。
    let unstable = fixture.sync_with_state_after(None, true, true).await;
    let unstable_keys =
        state_key_set(&sync_state_events(&unstable, &fixture.room_id, "org.matrix.msc4222.state_after"));
    assert_eq!(
        plain_keys, unstable_keys,
        "`org.matrix.msc4222.state_after` 与 `state_after` 的集合不一致：{unstable_keys:?}"
    );
}

/// 断言 ⑦（NEW-11 第三组）：`lazy_load_members` 的**可见成员集** —— 只发客户端需要的
/// membership，且 `include_redundant_members` 能把"已经发过的"重新发一遍。
///
/// 这条断言为什么必须存在：两根线各有一份实现（`sync_service/lazy_load.rs` 的
/// `apply_lazy_load_members_with_cache` 带 `LazyLoadedMemberState` 记账；滑动同步只有
/// `required_state` 的朴素过滤），一旦有人改了一边而没改另一边，客户端就会**永久看不到**
/// 某个成员的昵称/头像（懒加载把它当"已发送"跳过了），且没有任何报错 —— 只能靠这条拦。
#[tokio::test]
async fn lazy_load_members_only_sends_what_the_client_needs() {
    let fixture = SharedFixture::build(3).await;
    let peer = fixture.add_member("one").await;

    // 只开懒加载、不限 timeline：首屏必须**同时**给出两个人的 membership
    // （客户端手上什么都没有，抑制会让它永远看不到 peer 的名字）。
    let filter_id = fixture.store_filter(json!({ "room": { "state": { "lazy_load_members": true } } })).await;
    let initial = fixture.sync(Some(&filter_id)).await;
    let members = sync_member_keys(&initial, &fixture.room_id);
    assert!(
        members.contains(&fixture.user_id),
        "懒加载首屏必须包含**请求者自己**的 membership（否则客户端连自己的昵称都没有）：{members:?}"
    );
    assert!(
        members.contains(&peer),
        "懒加载首屏必须包含对端成员的 membership（首屏没有任何'已发送'可抑制）：{members:?}"
    );

    let since = initial["next_batch"].as_str().expect("next_batch 必须是字符串").to_string();

    // 增量：发**两条**消息再把 timeline 裁到 1 条 ⇒ `limited=true`，`state` 块转而描述
    // "窗口起点"的状态（重建式）。这时对已经发过的 membership 一律抑制。
    // 房间必须**在场**，否则"没有 membership"可能只是"整个房间没被返回"，断言会假绿。
    fixture.send("after-initial-1").await;
    fixture.send("after-initial-2").await;
    let limited_filter = fixture
        .store_filter(json!({ "room": { "state": { "lazy_load_members": true }, "timeline": { "limit": 1 } } }))
        .await;
    let incremental = fixture.sync_from(Some(&limited_filter), Some(&since), false, false).await;
    assert!(
        incremental["rooms"]["join"].get(&fixture.room_id).is_some(),
        "增量里有新消息时房间必须在 join 里（否则下一步的断言测不出东西）：{incremental}"
    );
    assert_eq!(
        incremental["rooms"]["join"][&fixture.room_id]["timeline"]["limited"], true,
        "前置条件：timeline 必须真的被裁到 1 条（否则走不到重建式分支，这条断言就没意义）：{incremental}"
    );
    let repeated = sync_member_keys(&incremental, &fixture.room_id);
    assert!(repeated.is_empty(), "增量同步不该重发已经发过的 membership（客户端会当成重复的状态事件）：{repeated:?}");

    // 同一份请求 + `include_redundant_members = true`：必须把成员**再发一遍**。
    //
    // ⚠️ 这条语义容易被想当然：`required_members`（自己 + 时间线发送者 + 变更成员）这道门禁
    // 在**冗余判断之前**，所以"冗余"并不会把无关成员拽进来；它只在已经入选的集合上解除
    // "已发送"抑制。上面那个 `limited=true` 的重建式分支里没有这道门禁（state 块描述窗口
    // 起点），因此冗余会把全部成员重发 —— 客户端换设备/丢本地库时靠它补齐。
    let redundant_filter = fixture
        .store_filter(json!({
            "room": {
                "state": { "lazy_load_members": true, "include_redundant_members": true },
                "timeline": { "limit": 1 }
            }
        }))
        .await;
    let redundant = fixture.sync_from(Some(&redundant_filter), Some(&since), false, false).await;
    let mut resent = sync_member_keys(&redundant, &fixture.room_id);
    resent.sort();
    let mut expected = vec![fixture.user_id.clone(), peer.clone()];
    expected.sort();
    assert_eq!(
        resent, expected,
        "`include_redundant_members=true` 必须把成员再发一遍（重建式分支下等于全量成员集）：{resent:?}"
    );
}

/// `/sync` 里 room.state.events 中的成员 user_id（`m.room.member` 的 state_key）。
fn sync_member_keys(sync_response: &Value, room_id: &str) -> Vec<String> {
    // 增量响应**可以不带 `state` 块**（规范允许：没有状态变更就不必给），所以缺块按空集算 ——
    // 调用方必须同时断言"房间确实在响应里"，否则"没有成员"与"房间没返回"分不开。
    sync_response["rooms"]["join"][room_id]["state"]["events"]
        .as_array()
        .map(|events| {
            events
                .iter()
                .filter(|event| event["type"] == "m.room.member")
                .map(|event| event["state_key"].as_str().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default()
}
