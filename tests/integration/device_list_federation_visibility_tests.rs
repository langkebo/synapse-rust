//! 联邦入站的设备列表变更必须对**客户端可见**（审计 NEW-07）。
//!
//! # 缺陷形态
//!
//! `PUT /_matrix/federation/v1/send/{txn}` 里的 `m.device_list_update` EDU 走
//! `federation::edu::handle_device_list_update_edu`。2026-10-24 之前那里调的是
//! `insert_device_list_change(user, device, change_type, stream_id)`：
//!
//! 1. 它**只**写 `device_lists_changes`，而客户端的两个读取面读的都是
//!    `device_lists_stream`：
//!    - `/sync` 的 `device_lists.changed` → `get_device_lists_since_with_shared_rooms`
//!      （`synapse-services/src/sync_service/data_fetch.rs`，SQL 读 `device_lists_stream`）；
//!    - `/keys/changes` → `get_device_list_changed_users`（同样读 `device_lists_stream`）。
//! 2. 它把**对端的** `stream_id`（毫秒时间戳，量级 1e12）写进那张表，而所有读取都带
//!    `stream_id <= to`（`to` = 本地序列的当前值，量级 1e3）—— 于是连唯一读
//!    `device_lists_changes` 的路径也读不到它。
//!
//! 净效果：远端用户增删设备后，本服务器上的客户端**永远**不会收到 `device_lists.changed`，
//! E2EE 会话继续用旧设备集。这条链路此前没有任何用例覆盖（`insert_device_list_change`
//! 在 `tests/` 下 0 引用），所以它能一直存在。
//!
//! # 为什么用例落在服务层而不是 `EduDispatcher`
//!
//! `FederationContext` 有 30+ 个字段（`routes/context.rs`），在集成测试里构造它等于把
//! 半个 app 装起来。所以这里测的是 **EDU handler 唯一被允许的写入方式**
//! （`AccountDeviceListService::record_device_list_change`）在两个真实读取面上的可见性；
//! "handler 确实调它、而不是调别的"由 `tests/unit/federation_edu_writer_tests.rs` 用源码
//! 断言钉住 —— 两边合起来才是完整判据。

use std::sync::Arc;

use synapse_services::account_device_list_service::AccountDeviceListService;
use synapse_storage::device::DeviceStorage;

/// 接收端写一条 `deleted` 变更后，`/keys/changes` 的读取面必须看到这个用户。
#[tokio::test]
async fn inbound_device_list_change_is_visible_to_keys_changes() {
    let pool = crate::require_test_pool().await;
    let service = AccountDeviceListService::new(Arc::new(DeviceStorage::new(&pool)));

    let remote_user = "@new07-remote:example.org";
    let requester = "@new07-local:example.com";

    let local_stream_id = service
        .record_device_list_change(remote_user, Some("REMOTE_DEVICE"), "deleted")
        .await
        .expect("recording an inbound device-list change must succeed");

    let to = service.get_max_device_list_stream_id_for_user(remote_user).await.expect("read the local stream head");
    assert!(
        local_stream_id <= to,
        "序号必须是**本地**序列里的值（{local_stream_id} vs 本地头 {to}）：\
         把对端的毫秒时间戳（1e12 量级）写进来，它就落在所有 `stream_id <= to` 的读取之外"
    );

    let changed = service.get_changed_user_ids(0, to, requester).await.expect("the /keys/changes reader must not fail");
    assert!(
        changed.iter().any(|user| user == remote_user),
        "`m.device_list_update` 写完后 `/keys/changes` 必须把 {remote_user} 列为 changed（实际 {changed:?}）；\
         否则远端设备变更对本服务器客户端永久不可见"
    );
}

/// 同一用户的两条变更必须拿到**递增**的本地序号。
///
/// 这条专门排除"用墙上时钟当序号"的写法：毫秒时间戳在同一毫秒内会给出**相同**的值，
/// 而接收端按 `stream_id` 排序（`ORDER BY stream_id ASC`）+ 去重，相同值意味着其中一条
/// 可能被当成重复。真实实现用 `BIGSERIAL` 序列，天然递增。
#[tokio::test]
async fn consecutive_inbound_changes_get_monotonic_local_stream_ids() {
    let pool = crate::require_test_pool().await;
    let service = AccountDeviceListService::new(Arc::new(DeviceStorage::new(&pool)));

    let remote_user = "@new07-monotonic:example.org";
    let first = service.record_device_list_change(remote_user, Some("DEV_A"), "updated").await.expect("first change");
    let second = service.record_device_list_change(remote_user, Some("DEV_B"), "updated").await.expect("second change");

    assert!(second > first, "第二条变更的序号必须严格大于第一条（{first} → {second}）");
}

/// `device_lists_stream` 与 `device_lists_changes` 必须都拿到这一行。
///
/// 只写后者的形态正是缺陷本身；这条用例把两张表都钉住，免得"修一处漏一处"。
#[tokio::test]
async fn inbound_device_list_change_writes_both_stream_and_change_rows() {
    let pool = crate::require_test_pool().await;
    let service = AccountDeviceListService::new(Arc::new(DeviceStorage::new(&pool)));

    let remote_user = "@new07-both:example.org";
    service.record_device_list_change(remote_user, Some("DEV_BOTH"), "deleted").await.expect("record change");

    let to = service.get_max_device_list_stream_id_for_user(remote_user).await.expect("stream head");

    // `device_lists_stream`（/sync 与 /keys/changes 的读取面）
    let from_stream = service.get_changed_user_ids(0, to, "@someone:example.com").await.expect("stream reader");
    assert!(from_stream.iter().any(|u| u == remote_user), "`device_lists_stream` 必须含这一行：{from_stream:?}");

    // `device_lists_changes`（逐条变更的读取面，`/keys/changes` 的 delta 走这里）
    let delta = service
        .get_device_list_delta(0, Some(to), std::slice::from_ref(&remote_user.to_string()))
        .await
        .expect("change reader");
    assert!(
        delta.deleted.iter().any(|d| d.user_id == remote_user && d.device_id == "DEV_BOTH"),
        "`device_lists_changes` 必须含这一条 deleted 变更（delta.deleted = {:?}）",
        delta.deleted
    );
}
