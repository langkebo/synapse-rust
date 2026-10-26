//! `&dyn RoomStoreApi` 转发层契约测试。
//!
//! **为什么需要这个文件**：`room/api.rs:305` 的 `impl RoomStoreApi for RoomStorage` 是 50 个
//! 纯 `.await` 转发体，自身没有任何逻辑，所以：
//!
//! * 生产装配把它装进 `Arc<dyn RoomStoreApi>`
//!   （`synapse-services/src/wiring/rooms.rs`：`let room_storage: Arc<dyn RoomStoreApi> =
//!   Arc::new(RoomStorage::new(&infra.pool))`），**运行时确实执行这些行**；
//! * 但本 crate 的 `db_tests.rs` 全部走**固有**方法（`storage.get_room(...)`），
//!   从不经过 trait 对象 ⇒ 这份 `impl` 在 storage 车道上永远是 0%。
//!
//! 因此「0% ⇒ 删掉」是错的判定：删了之后 services 里所有
//! `Arc<dyn RoomStoreApi>` 字段会直接编译不过。正确处置是补一条**经过 trait 对象**的
//! 契约测试（与 `event/trait_object_dispatch_tests.rs` 同一思路）。
//!
//! **覆盖策略**：不逐个钉 50 个方法，而是按"形状"抽样，重点抓转发层唯一能犯的错 ——
//! 把 A 方法接到 B 方法上（50 个几乎同形的 `self.xxx(..).await` 最容易出这类错）。
//! 所以每条断言都要求：**用 dyn 写，然后用另一条读路径验**。

use super::*;

async fn test_pool() -> (crate::test_isolation::IsolatedTestPool, Arc<Pool<Postgres>>) {
    let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated pool");
    let pool = isolated.pool();
    (isolated, pool)
}

fn unique_room() -> String {
    format!("!dyn_{}:example.com", uuid::Uuid::new_v4().simple())
}

#[tokio::test]
async fn dyn_pool_is_the_pool_it_was_constructed_with() {
    let (_isolated, pool) = test_pool().await;
    let store = RoomStorage::new(&pool);
    let api: Arc<dyn RoomStoreApi> = Arc::new(RoomStorage::new(&pool));

    // `pool()` 是唯一一个非 async 的转发体：它一旦返回别的池（新建 / 全局单例），
    // 服务就会在另一个库上读写，且不会有任何编译期信号。
    assert!(
        Arc::ptr_eq(api.pool(), &store.pool),
        "trait 对象的 pool() 必须与构造时的池是同一个 Arc（偷换连接池会让调用方读到另一个库）"
    );
    assert!(Arc::ptr_eq(api.pool(), &pool), "pool() 也不得返回包装过的新 Arc");
}

#[tokio::test]
async fn create_via_dyn_is_visible_to_the_inherent_reader() {
    let (_isolated, pool) = test_pool().await;
    let store = RoomStorage::new(&pool);
    let api: Arc<dyn RoomStoreApi> = Arc::new(RoomStorage::new(&pool));
    let room_id = unique_room();

    api.create_room(&room_id, "@creator:example.com", "invite", "10", false).await.expect("dyn create_room");

    let via_dyn = api.get_room(&room_id).await.expect("dyn get_room");
    let via_inherent = store.get_room(&room_id).await.expect("inherent get_room");
    let room = via_dyn.expect("dyn 写入的行必须在 trait 读路径可见");
    let room_inherent = via_inherent.expect("dyn 写入的行也必须在固有读路径可见");

    assert_eq!(room.room_id, room_inherent.room_id);
    // 参数顺序错位是本文件最想抓的缺陷：create_room(join_rule, version) 相邻且同为 &str。
    assert_eq!(room.join_rule, "invite", "create_room 的 join_rule 参数必须落到 join_rule 列");
    assert_eq!(room.room_version, "10", "create_room 的 version 参数必须落到 room_version 列");
    assert!(!room.is_public, "create_room 的 is_public 参数必须落到 is_public 列");

    assert!(api.room_exists(&room_id).await.expect("dyn room_exists"));
    assert!(
        !api.room_exists(&unique_room()).await.expect("dyn room_exists on missing"),
        "不存在的房间必须是 Ok(false)，不是 Err"
    );
}

#[tokio::test]
async fn duplicate_create_through_dyn_still_surfaces_the_error() {
    let (_isolated, pool) = test_pool().await;
    let api: Arc<dyn RoomStoreApi> = Arc::new(RoomStorage::new(&pool));
    let room_id = unique_room();

    api.create_room(&room_id, "@creator:example.com", "invite", "10", false).await.expect("首次 create 必须成功");
    let dup = api.create_room(&room_id, "@other:example.com", "public", "10", true).await;
    assert!(dup.is_err(), "重复 create_room 必须返回可判别错误，转发层不得把 Err 吞成 Ok");
}

#[tokio::test]
async fn canonical_alias_and_room_alias_are_not_swapped() {
    let (_isolated, pool) = test_pool().await;
    let api: Arc<dyn RoomStoreApi> = Arc::new(RoomStorage::new(&pool));
    let room_id = unique_room();
    api.create_room(&room_id, "@creator:example.com", "invite", "10", true).await.expect("dyn create_room");

    let canon = format!("#canon_{}:example.com", uuid::Uuid::new_v4().simple());
    let alias = format!("#alias_{}:example.com", uuid::Uuid::new_v4().simple());
    api.set_canonical_alias(&room_id, Some(&canon)).await.expect("dyn set_canonical_alias");
    api.set_room_alias(&room_id, &alias, "@creator:example.com").await.expect("dyn set_room_alias");

    let room = api.get_room(&room_id).await.expect("dyn get_room").expect("房间必须还在");
    assert_eq!(
        room.canonical_alias.as_deref(),
        Some(canon.as_str()),
        "set_canonical_alias 必须落到 rooms.canonical_alias"
    );
    assert_eq!(
        api.get_room_by_alias(&alias).await.expect("dyn get_room_by_alias").as_deref(),
        Some(room_id.as_str()),
        "set_room_alias 必须写 aliases 表；按别名反查不到本房间＝这两个方法被接反了"
    );
    assert!(
        api.get_room_aliases(&room_id).await.expect("dyn get_room_aliases").contains(&alias),
        "别名列表必须包含刚写入的别名"
    );
}

#[tokio::test]
async fn name_and_topic_forwards_do_not_swap_columns() {
    let (_isolated, pool) = test_pool().await;
    let api: Arc<dyn RoomStoreApi> = Arc::new(RoomStorage::new(&pool));
    let room_id = unique_room();
    api.create_room(&room_id, "@creator:example.com", "invite", "10", false).await.expect("dyn create_room");

    api.update_room_name(&room_id, "dyn-name").await.expect("dyn update_room_name");
    api.update_room_topic(&room_id, "dyn-topic").await.expect("dyn update_room_topic");

    let room = api.get_room(&room_id).await.expect("dyn get_room").expect("房间必须还在");
    assert_eq!(room.name.as_deref(), Some("dyn-name"), "update_room_name 不得写到 topic 列");
    assert_eq!(room.topic.as_deref(), Some("dyn-topic"), "update_room_topic 不得写到 name 列");
}

#[tokio::test]
async fn tx_variants_write_inside_the_callers_transaction() {
    let (_isolated, pool) = test_pool().await;
    let store = RoomStorage::new(&pool);
    let api: Arc<dyn RoomStoreApi> = Arc::new(RoomStorage::new(&pool));
    let room_id = unique_room();

    // 隔离池只有一条连接，所以"另开连接看未提交数据"这条路走不通（PoolTimedOut）。
    // 改成**回滚**来证明写进的是调用方的事务：若转发层另取连接自行提交，
    // 回滚只影响那条空事务，行会残留下来。
    let mut tx = pool.begin().await.expect("begin");
    api.create_room_in_tx(&mut tx, &room_id, "@creator:example.com", "invite", "10", false)
        .await
        .expect("dyn create_room_in_tx");
    api.update_join_rule_in_tx(&mut tx, &room_id, "public").await.expect("dyn update_join_rule_in_tx");
    tx.rollback().await.expect("rollback");

    assert!(
        store.get_room(&room_id).await.expect("inherent get_room").is_none(),
        "回滚后必须没有行：转发层若另取连接自行提交，这里会看到残留行"
    );
}

#[tokio::test]
async fn tx_variants_commit_persists_the_join_rule_update() {
    let (_isolated, pool) = test_pool().await;
    let store = RoomStorage::new(&pool);
    let api: Arc<dyn RoomStoreApi> = Arc::new(RoomStorage::new(&pool));
    let room_id = unique_room();

    let mut tx = pool.begin().await.expect("begin");
    api.create_room_in_tx(&mut tx, &room_id, "@creator:example.com", "invite", "10", false)
        .await
        .expect("dyn create_room_in_tx");
    api.update_join_rule_in_tx(&mut tx, &room_id, "public").await.expect("dyn update_join_rule_in_tx");
    tx.commit().await.expect("commit");

    let room = store.get_room(&room_id).await.expect("inherent get_room").expect("commit 后必须可见");
    assert_eq!(room.join_rule, "public", "update_join_rule_in_tx 必须写 join_rule 列");
}

#[tokio::test]
async fn delete_and_count_forward_to_the_same_table() {
    let (_isolated, pool) = test_pool().await;
    let api: Arc<dyn RoomStoreApi> = Arc::new(RoomStorage::new(&pool));
    let before = api.get_room_count().await.expect("dyn get_room_count");
    let room_id = unique_room();

    api.create_room(&room_id, "@creator:example.com", "invite", "10", false).await.expect("dyn create_room");
    assert_eq!(
        api.get_room_count().await.expect("dyn get_room_count"),
        before + 1,
        "get_room_count 必须反映 dyn 写入的行"
    );

    api.delete_room(&room_id).await.expect("dyn delete_room");
    assert_eq!(
        api.get_room_count().await.expect("dyn get_room_count"),
        before,
        "delete_room 必须真的删行，而不是转到别的表/别的条件"
    );
    assert!(!api.room_exists(&room_id).await.expect("dyn room_exists"), "删完必须不存在");
}
