//! `&dyn SlidingSyncStoreApi` 转发层契约测试。
//!
//! 与 `room/api_dyn_tests.rs` 同一动机：`sliding_sync/api.rs:197` 的
//! `impl SlidingSyncStoreApi for SlidingSyncStorage` 是 23 个纯 `.await` 转发体，
//! 生产者是 `synapse-services/src/sliding_sync_service/mod.rs`（字段类型
//! `Arc<dyn SlidingSyncStoreApi>`），而本 crate 的 `db_tests.rs` 全走固有方法
//! ⇒ 这份 `impl` 在 storage 车道上恒为 0%。删掉它会直接砍断 services 的装配，
//! 所以补"经过 trait 对象"的断言才是正确处置。
//!
//! 断言一律"dyn 写、另一条路径读"，以抓转发层最典型的错误：把 A 方法接到 B 方法上。

use crate::sliding_sync::{SlidingSyncStorage, SlidingSyncStoreApi};

use std::sync::Arc;

use sqlx::{Pool, Postgres};

async fn test_pool() -> (crate::test_isolation::IsolatedTestPool, Arc<Pool<Postgres>>) {
    let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated pool");
    let pool = isolated.pool();
    (isolated, pool)
}

fn unique_id(prefix: &str) -> String {
    format!("{prefix}_{}:localhost", uuid::Uuid::new_v4().simple())
}

#[tokio::test]
async fn token_written_through_dyn_is_readable_and_validated() {
    let (_isolated, pool) = test_pool().await;
    let store = SlidingSyncStorage::new(pool.clone());
    let api: Arc<dyn SlidingSyncStoreApi> = Arc::new(SlidingSyncStorage::new(pool.clone()));
    let user = unique_id("@dyn_user");
    let device = unique_id("DEV");

    let token = api.create_or_update_token(&user, &device, Some("conn"), 42).await.expect("dyn create_or_update_token");
    // 两个不同的整数列：`event_stream_pos` 是调用方给的参数（水位线），
    // `pos` 是 `sliding_sync_pos_seq` 序列值（客户端用来做 token 校验）。
    // 转发层若把参数接到错误的列上，正是这里能抓到的东西。
    assert_eq!(token.event_stream_pos, 42, "event_stream_pos 必须原样落到该列");
    assert!(token.pos > 0, "pos 必须由序列产生（>0），不是调用方参数");
    assert_eq!(token.user_id, user, "返回的 token 必须属于该用户（转发层不得丢参数）");

    let via_dyn = api.get_token(&user, &device, Some("conn")).await.expect("dyn get_token");
    let via_inherent = store.get_token(&user, &device, Some("conn")).await.expect("inherent get_token");
    let read_back = via_dyn.expect("dyn 写必须可被 dyn 读回");
    assert_eq!(read_back.event_stream_pos, 42, "dyn 读回的水位线必须是刚写的值");
    assert_eq!(read_back.pos, token.pos, "dyn 读回的 pos 必须与写入返回的一致");
    assert_eq!(
        via_inherent.expect("dyn 写必须可被固有读路径读回").event_stream_pos,
        42,
        "同一条行必须对固有读路径也可见"
    );

    // upsert 语义：第二次写同一个连接必须是**更新同一行**（id 不变、pos 前进），
    // 而不是插新行（否则每次同步都长一行，行数无界）。
    let updated = api
        .create_or_update_token(&user, &device, Some("conn"), 99)
        .await
        .expect("dyn create_or_update_token (update)");
    assert_eq!(updated.id, token.id, "同连接必须是同一行（id 不变）");
    assert!(updated.pos > token.pos, "更新必须推进 pos 序列，而不是复用旧值");
    assert_eq!(updated.event_stream_pos, 99, "更新必须覆盖 event_stream_pos");
}

#[tokio::test]
async fn validate_pos_forwards_the_pos_parameter_verbatim() {
    let (_isolated, pool) = test_pool().await;
    let api: Arc<dyn SlidingSyncStoreApi> = Arc::new(SlidingSyncStorage::new(pool.clone()));
    let user = unique_id("@dyn_user");
    let device = unique_id("DEV");

    let token = api.create_or_update_token(&user, &device, None, 42).await.expect("dyn create_or_update_token");
    let good = token.pos.to_string();
    let stale = (token.pos - 1).to_string();

    assert!(
        api.validate_pos(&user, &device, None, &good).await.expect("dyn validate_pos"),
        "已存的 pos={good} 必须校验通过"
    );
    assert!(
        !api.validate_pos(&user, &device, None, &stale).await.expect("dyn validate_pos"),
        "过期的 pos={stale} 必须校验失败；恒 true 说明参数被吞或转到别的查询"
    );
    // 未知连接：`fetch_optional` 走 None 分支，必须是 Ok(false) 而不是 Err。
    assert!(
        !api.validate_pos(&unique_id("@nobody"), &device, None, &good).await.expect("dyn validate_pos"),
        "不存在的连接必须返回 Ok(false)"
    );
}

#[tokio::test]
async fn list_lifecycle_through_dyn() {
    let (_isolated, pool) = test_pool().await;
    let api: Arc<dyn SlidingSyncStoreApi> = Arc::new(SlidingSyncStorage::new(pool.clone()));
    let user = unique_id("@dyn_user");
    let device = unique_id("DEV");
    let sort = vec!["bump_stamp".to_string()];

    let saved = api
        .save_list(&user, &device, Some("conn"), "main", &sort, None, None, &[(0, 10)])
        .await
        .expect("dyn save_list");
    assert_eq!(saved.list_key, "main");
    assert_eq!(saved.sort, serde_json::json!(sort), "sort 参数必须原样落到 sort 列");

    let lists = api.get_lists(&user, &device, Some("conn")).await.expect("dyn get_lists");
    assert_eq!(lists.len(), 1, "刚写入一个 list，数量必须是 1");
    assert_eq!(lists[0].list_key, "main");

    api.delete_list(&user, &device, Some("conn"), "main").await.expect("dyn delete_list");
    assert!(
        api.get_lists(&user, &device, Some("conn")).await.expect("dyn get_lists").is_empty(),
        "delete_list 必须真的删掉该 list"
    );
}

#[tokio::test]
async fn delete_connection_data_clears_token_and_lists() {
    let (_isolated, pool) = test_pool().await;
    let api: Arc<dyn SlidingSyncStoreApi> = Arc::new(SlidingSyncStorage::new(pool.clone()));
    let user = unique_id("@dyn_user");
    let device = unique_id("DEV");
    let sort = vec!["bump_stamp".to_string()];

    api.create_or_update_token(&user, &device, Some("conn"), 7).await.expect("dyn token");
    api.save_list(&user, &device, Some("conn"), "main", &sort, None, None, &[(0, 5)]).await.expect("dyn save_list");

    api.delete_connection_data(&user, &device, Some("conn")).await.expect("dyn delete_connection_data");

    assert!(
        api.get_token(&user, &device, Some("conn")).await.expect("dyn get_token").is_none(),
        "断连必须清掉该连接的 token"
    );
    assert!(
        api.get_lists(&user, &device, Some("conn")).await.expect("dyn get_lists").is_empty(),
        "断连必须清掉该连接的 list"
    );
}
