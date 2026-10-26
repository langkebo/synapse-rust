use super::*;

/// Shared `public` is deliberately replaced by a per-test schema here:
/// `purge_room`/`delete_room` sweep `events` and `rooms` for whole rooms and several
/// tests assert on the resulting counts; shared `public` let a sibling's purge
/// remove the rows counted here.
///
/// Eliminating the shared state removes the race instead of serialising around it
/// (AGENTS.md rule 7). The guard is returned with the pool so the schema outlives
/// the whole test — dropping it early spawns a background `DROP SCHEMA`.
async fn test_pool() -> (crate::test_isolation::IsolatedTestPool, Arc<Pool<Postgres>>) {
    let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated pool");
    let pool = isolated.pool();
    (isolated, pool)
}

async fn ensure_test_room(pool: &Pool<Postgres>, room_id: &str, creator: &str) {
    let now = current_timestamp_millis();
    sqlx::query(
            r#"INSERT INTO rooms (room_id, creator, join_rules, room_version, is_public, history_visibility, created_ts, last_activity_ts)
               VALUES ($1, $2, 'invite', '10', false, 'joined', $3, $3)
               ON CONFLICT (room_id) DO NOTHING"#,
        )
        .bind(room_id)
        .bind(creator)
        .bind(now)
        .execute(pool)
        .await
        .expect("failed to create test room");
}

#[tokio::test]
async fn test_create_room_persists_row() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let room_id = format!("!create_test_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_room(&room_id).await;

    storage
        .create_room(&room_id, "@creator:example.com", "invite", "10", false)
        .await
        .expect("create_room should succeed");

    // 契约只承诺"落库"：快照必须读回，不能由 `create_room` 的返回值提供。
    let room =
        storage.get_room(&room_id).await.expect("get_room should succeed").expect("row should have been inserted");

    assert_eq!(room.room_id, room_id);
    assert_eq!(room.creator_user_id, Some("@creator:example.com".to_string()));
    assert_eq!(room.join_rule, "invite");
    assert!(!room.is_public);

    let _ = storage.delete_room(&room_id).await;
}

/// C88：同一个 `room_id` 再插一次必须**可判别地失败**（`RowNotFound`），而不是：
/// ① 静默成功（会让调用方以为房间是自己的，随后写的 `m.room.create`/成员关系指向别人的行）；
/// ② 把 23505 原样抛出（调用方分不清"派生 id 撞车"与真正的内部错误，客户端拿到 500）。
///
/// 背景：v12 的 room_id 由 create 事件的 reference hash 决定（MSC4291），同一毫秒 + 同一内容的
/// 并发 createRoom 会派生出同一个 id。进程内已由 `current_timestamp_millis_monotonic()` 避免，
/// 这条用例钉住**残留形态**的契约（跨进程撞车时 `create_room_in_db` 会把它转成 409
/// `M_ROOM_IN_USE`，见 `synapse-services/src/room/lifecycle/create_events.rs`）。
#[tokio::test]
async fn test_create_room_duplicate_room_id_is_a_distinguishable_conflict() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let room_id = format!("!c88_dup_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_room(&room_id).await;

    storage
        .create_room(&room_id, "@first:example.com", "invite", "12", false)
        .await
        .expect("首次 create_room 必须成功");

    let second = storage.create_room(&room_id, "@second:example.com", "public", "12", true).await;
    assert!(
        matches!(second, Err(sqlx::Error::RowNotFound)),
        "重复 room_id 必须返回 RowNotFound（'已存在' 的可判别信号），实际：{second:?}"
    );

    // 且**不得**覆盖既有行（静默 upsert 会让房间创建者/可见性被第二次请求改掉）
    let stored = storage.get_room(&room_id).await.expect("get_room").expect("row must exist");
    assert_eq!(stored.creator_user_id.as_deref(), Some("@first:example.com"), "既有行不得被覆盖");
    assert_eq!(stored.join_rule, "invite", "既有行不得被覆盖");

    let _ = storage.delete_room(&room_id).await;
}

#[tokio::test]
async fn test_get_room_found() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let room_id = format!("!get_test_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_room(&room_id).await;
    storage.create_room(&room_id, "@creator:example.com", "invite", "10", false).await.unwrap();

    let found = storage.get_room(&room_id).await.expect("get_room should succeed");
    assert!(found.is_some());
    assert_eq!(found.unwrap().room_id, room_id);

    let _ = storage.delete_room(&room_id).await;
}

#[tokio::test]
async fn test_get_room_not_found() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let result = storage.get_room("!nonexistent:example.com").await.expect("get_room should succeed");
    assert!(result.is_none());
}

#[tokio::test]
async fn test_room_exists() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let room_id = format!("!exists_test_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_room(&room_id).await;
    storage.create_room(&room_id, "@creator:example.com", "invite", "10", false).await.unwrap();

    assert!(storage.room_exists(&room_id).await.expect("room_exists should succeed"));
    assert!(!storage.room_exists("!nope:example.com").await.expect("room_exists should succeed"));

    let _ = storage.delete_room(&room_id).await;
}

#[tokio::test]
async fn test_get_room_creator() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let room_id = format!("!creator_test_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_room(&room_id).await;
    storage.create_room(&room_id, "@thecreator:example.com", "invite", "10", false).await.unwrap();

    let creator = storage.get_room_creator(&room_id).await.expect("get_room_creator should succeed");
    assert_eq!(creator, Some("@thecreator:example.com".to_string()));

    let _ = storage.delete_room(&room_id).await;
}

#[tokio::test]
async fn test_get_rooms_batch() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let rid1 = format!("!batch1_{}:example.com", uuid::Uuid::new_v4());
    let rid2 = format!("!batch2_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_room(&rid1).await;
    let _ = storage.delete_room(&rid2).await;
    storage.create_room(&rid1, "@c:example.com", "invite", "10", false).await.unwrap();
    storage.create_room(&rid2, "@c:example.com", "invite", "10", false).await.unwrap();

    let rooms = storage.get_rooms_batch(&[rid1.clone(), rid2.clone()]).await.expect("get_rooms_batch should succeed");
    assert_eq!(rooms.len(), 2);

    let _ = storage.delete_room(&rid1).await;
    let _ = storage.delete_room(&rid2).await;
}

#[tokio::test]
async fn test_update_room_name() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let room_id = format!("!name_test_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_room(&room_id).await;
    storage.create_room(&room_id, "@c:example.com", "invite", "10", false).await.unwrap();

    storage.update_room_name(&room_id, "New Room Name").await.expect("update_room_name should succeed");
    let room = storage.get_room(&room_id).await.unwrap().unwrap();
    assert_eq!(room.name, Some("New Room Name".to_string()));

    let _ = storage.delete_room(&room_id).await;
}

#[tokio::test]
async fn test_update_room_topic() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let room_id = format!("!topic_test_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_room(&room_id).await;
    storage.create_room(&room_id, "@c:example.com", "invite", "10", false).await.unwrap();

    storage.update_room_topic(&room_id, "New Topic").await.expect("update_room_topic should succeed");
    let room = storage.get_room(&room_id).await.unwrap().unwrap();
    assert_eq!(room.topic, Some("New Topic".to_string()));

    let _ = storage.delete_room(&room_id).await;
}

#[tokio::test]
async fn test_get_room_count() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let count = storage.get_room_count().await.expect("get_room_count should succeed");
    assert!(count >= 0);
}

#[tokio::test]
async fn test_set_room_public_and_directory() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let room_id = format!("!pub_test_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_room(&room_id).await;
    storage.create_room(&room_id, "@c:example.com", "invite", "10", false).await.unwrap();
    assert!(!storage.is_room_in_directory(&room_id).await.unwrap());

    storage.set_room_visibility(&room_id, "public").await.expect("set_room_visibility should succeed");
    storage.set_room_directory(&room_id, true).await.expect("set_room_directory should succeed");
    let room = storage.get_room(&room_id).await.unwrap().unwrap();
    assert!(room.is_public);
    assert!(storage.is_room_in_directory(&room_id).await.unwrap());

    storage.set_room_visibility(&room_id, "private").await.expect("unset public should succeed");
    let room = storage.get_room(&room_id).await.unwrap().unwrap();
    assert!(!room.is_public);

    // STO-13: `set_room_directory(false)` must DELETE the directory row so the
    // standard path agrees with the admin path (`set_room_private_with_directory`).
    // Before the fix the row lingered with is_public=false, and the EXISTS check
    // in get_room_listings_status reported a contradictory in_directory=true.
    storage.set_room_directory(&room_id, false).await.expect("set_room_directory(false) should succeed");
    let (flag_is_public, in_directory) = storage
        .get_room_listings_status(&room_id)
        .await
        .expect("get_room_listings_status should succeed")
        .expect("room should exist");
    assert!(!flag_is_public);
    assert!(!in_directory, "private room must not remain in room_directory (STO-13)");
    assert!(!storage.is_room_in_directory(&room_id).await.unwrap());

    // Re-publishing must restore the row.
    storage.set_room_directory(&room_id, true).await.expect("re-set_room_directory(true) should succeed");
    let (_, in_directory) = storage
        .get_room_listings_status(&room_id)
        .await
        .expect("get_room_listings_status should succeed")
        .expect("room should exist");
    assert!(in_directory, "public room must have a room_directory row");

    let _ = storage.delete_room(&room_id).await;
}

#[tokio::test]
async fn test_delete_room() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let room_id = format!("!delete_test_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_room(&room_id).await;
    storage.create_room(&room_id, "@c:example.com", "invite", "10", false).await.unwrap();
    assert!(storage.room_exists(&room_id).await.unwrap());

    storage.delete_room(&room_id).await.expect("delete_room should succeed");
    assert!(!storage.room_exists(&room_id).await.unwrap());
}

#[tokio::test]
async fn test_get_public_rooms() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let rooms = storage.get_public_rooms(5).await.expect("get_public_rooms should succeed");
    // All returned rooms should be public
    for room in &rooms {
        assert!(room.is_public);
    }
}

#[tokio::test]
async fn test_count_public_rooms() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let count = storage
        .count_public_rooms(&crate::room::RoomTypeFilter::default())
        .await
        .expect("count_public_rooms should succeed");
    assert!(count >= 0);
}

#[tokio::test]
async fn test_get_public_rooms_paginated() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let rooms = storage
        .get_public_rooms_paginated(5, None, None, &crate::room::RoomTypeFilter::default())
        .await
        .expect("get_public_rooms_paginated should succeed");
    assert!(rooms.len() <= 5);
    for room in &rooms {
        assert!(room.is_public);
    }
}

#[tokio::test]
async fn test_room_alias_crud() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!alias_test_{}:example.com", suffix);
    let alias = format!("#mytestalias_{}:example.com", suffix);
    let _ = storage.delete_room(&room_id).await;
    storage.create_room(&room_id, "@c:example.com", "invite", "10", false).await.unwrap();

    storage.set_room_alias(&room_id, &alias, "@c:example.com").await.expect("set_room_alias should succeed");

    let aliases = storage.get_room_aliases(&room_id).await.expect("get_room_aliases should succeed");
    assert!(aliases.contains(&alias));

    let resolved = storage.get_room_by_alias(&alias).await.expect("get_room_by_alias should succeed");
    assert_eq!(resolved, Some(room_id.clone()));

    storage.remove_room_alias_by_name(&alias).await.expect("remove_room_alias_by_name should succeed");
    let after = storage.get_room_by_alias(&alias).await.unwrap();
    assert!(after.is_none());

    let _ = storage.delete_room(&room_id).await;
}

/// `search_room_directory` 此前是**恒真断言**（`assert!(results.len() <= 10)`，且不造任何数据）
/// —— 这正是 D-108"能力未接线却无人发现"的原因之一。C67-0 按 C65-0 的先例把它改成真覆盖：
/// 三条**公开**房间分别只靠 name / topic / canonical_alias 命中，外加一条 name 也命中的
/// **私有**房间作反例（`is_public = FALSE` 必须被排除）。
#[tokio::test]
async fn test_search_room_directory_matches_name_topic_and_alias() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let term = format!("zzdir{suffix}");
    let by_name = format!("!dir_name_{suffix}:example.com");
    let by_topic = format!("!dir_topic_{suffix}:example.com");
    let by_alias = format!("!dir_alias_{suffix}:example.com");
    let private = format!("!dir_private_{suffix}:example.com");

    for (room_id, name, topic, alias, is_public) in [
        (&by_name, Some(format!("{term} alpha")), None, None, true),
        (&by_topic, None, Some(format!("topic {term}")), None, true),
        (&by_alias, None, None, Some(format!("#{term}:example.com")), true),
        (&private, Some(format!("{term} hidden")), None, None, false),
    ] {
        sqlx::query(
            "INSERT INTO rooms (room_id, creator, is_public, room_version, created_ts, name, topic, canonical_alias) \
                 VALUES ($1, '@dir:example.com', $2, '10', 1700000000000, $3, $4, $5)",
        )
        .bind(room_id)
        .bind(is_public)
        .bind(name)
        .bind(topic)
        .bind(alias)
        .execute(&*pool)
        .await
        .expect("insert fixture room");
    }

    let mut found: Vec<String> = storage
        .search_room_directory(&term, 10, &crate::room::RoomTypeFilter::default())
        .await
        .expect("search_room_directory should succeed")
        .into_iter()
        .map(|room| room.room_id)
        .collect();
    found.sort();
    let mut expected = vec![by_name.clone(), by_topic.clone(), by_alias.clone()];
    expected.sort();
    assert_eq!(found, expected, "name / topic / canonical_alias 三个面都要命中，私有房间必须被排除");

    // 计数必须与返回集合**同谓词** —— 否则 `total_room_count_estimate` 会与 chunk 漂移
    let total = storage
        .count_public_rooms_matching(&term, &crate::room::RoomTypeFilter::default())
        .await
        .expect("count_public_rooms_matching");
    assert_eq!(total, 3, "匹配总数 = 三条公开房间（私有房间不计）");

    // 大小写不敏感（SQL 侧 `LOWER(...)` + pattern 侧 `to_lowercase()`）
    let upper = storage
        .search_room_directory(&term.to_uppercase(), 10, &crate::room::RoomTypeFilter::default())
        .await
        .expect("uppercase search");
    assert_eq!(upper.len(), 3, "搜索必须大小写不敏感");

    // D-109：`filter.room_types` 也要在**存储层**生效（`["m.space"]` 只留空间；
    // 集成层用例覆盖"搜索路径 + 列表路径"，这里钉住搜索谓词侧的 JOIN 判据）。
    sqlx::query(
        "INSERT INTO room_summaries (room_id, room_type, is_space, updated_ts, created_ts) \
             VALUES ($1, 'm.space', TRUE, 1700000000000, 1700000000000)",
    )
    .bind(&by_name)
    .execute(&*pool)
    .await
    .expect("insert fixture room summary");
    let only_spaces = storage
        .search_room_directory(
            &term,
            10,
            &crate::room::RoomTypeFilter { types: Some(vec!["m.space".to_string()]), include_normal: false },
        )
        .await
        .expect("room_types search");
    assert_eq!(
        only_spaces.iter().map(|room| room.room_id.as_str()).collect::<Vec<_>>(),
        vec![by_name.as_str()],
        "room_types=[\"m.space\"] 只应返回被标记为空间的那条"
    );
    let normal_rooms = storage
        .search_room_directory(
            &term,
            10,
            &crate::room::RoomTypeFilter { types: Some(Vec::new()), include_normal: true },
        )
        .await
        .expect("room_types=[null] search");
    let mut normal_ids: Vec<&str> = normal_rooms.iter().map(|room| room.room_id.as_str()).collect();
    normal_ids.sort();
    let mut expected_normal = vec![by_topic.as_str(), by_alias.as_str()];
    expected_normal.sort();
    assert_eq!(normal_ids, expected_normal, "room_types=[null] 只应返回普通房间（空间被排除）");

    // `limit` 只截断返回集合，不影响匹配总数
    let limited =
        storage.search_room_directory(&term, 1, &crate::room::RoomTypeFilter::default()).await.expect("limited search");
    assert_eq!(limited.len(), 1, "limit 必须生效");
    assert_eq!(
        storage.count_public_rooms_matching(&term, &crate::room::RoomTypeFilter::default()).await.unwrap(),
        3,
        "limit 不得影响总数"
    );
}

#[tokio::test]
async fn test_get_user_rooms() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let room_ids = storage.get_user_rooms("@testuser:example.com").await.expect("get_user_rooms should succeed");
    // Room membership may be empty for a test user — the .expect() above
    // already verifies the query itself succeeds without error.
    let _ = room_ids;
}

#[tokio::test]
async fn test_update_room_avatar() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let room_id = format!("!avatar_test_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_room(&room_id).await;
    storage.create_room(&room_id, "@c:example.com", "invite", "10", false).await.unwrap();

    storage
        .update_room_avatar(&room_id, "mxc://example.com/avatar123")
        .await
        .expect("update_room_avatar should succeed");
    let room = storage.get_room(&room_id).await.unwrap().unwrap();
    assert_eq!(room.avatar_url.as_deref(), Some("mxc://example.com/avatar123"));

    let _ = storage.delete_room(&room_id).await;
}

#[tokio::test]
async fn test_set_canonical_alias_some_then_none() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let room_id = format!("!alias_set_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_room(&room_id).await;
    storage.create_room(&room_id, "@c:example.com", "invite", "10", false).await.unwrap();

    storage
        .set_canonical_alias(&room_id, Some("#alias:example.com"))
        .await
        .expect("set_canonical_alias Some should succeed");
    let room = storage.get_room(&room_id).await.unwrap().unwrap();
    assert_eq!(room.canonical_alias.as_deref(), Some("#alias:example.com"));

    storage.set_canonical_alias(&room_id, None).await.expect("set_canonical_alias None should succeed");
    let room = storage.get_room(&room_id).await.unwrap().unwrap();
    assert!(room.canonical_alias.is_none());

    let _ = storage.delete_room(&room_id).await;
}

#[tokio::test]
async fn test_update_canonical_alias() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let room_id = format!("!upd_alias_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_room(&room_id).await;
    storage.create_room(&room_id, "@c:example.com", "invite", "10", false).await.unwrap();

    storage
        .update_canonical_alias(&room_id, "#first:example.com")
        .await
        .expect("update_canonical_alias should succeed");
    let room = storage.get_room(&room_id).await.unwrap().unwrap();
    assert_eq!(room.canonical_alias.as_deref(), Some("#first:example.com"));

    let _ = storage.delete_room(&room_id).await;
}

#[tokio::test]
async fn test_get_room_alias_singular() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!single_alias_{}:example.com", suffix);
    let alias = format!("#single_{}:example.com", suffix);
    let _ = storage.delete_room(&room_id).await;
    storage.create_room(&room_id, "@c:example.com", "invite", "10", false).await.unwrap();
    storage.set_room_alias(&room_id, &alias, "@c:example.com").await.unwrap();

    // get_room_alias returns a single alias (LIMIT 1)
    let result = storage.get_room_alias(&room_id).await.expect("get_room_alias should succeed");
    assert_eq!(result.as_deref(), Some(alias.as_str()));

    // Absent room returns None
    let absent = storage.get_room_alias("!nonexistent:example.com").await.unwrap();
    assert!(absent.is_none());

    let _ = storage.delete_room(&room_id).await;
}

#[tokio::test]
async fn test_remove_room_alias_by_room_id() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!rm_alias_{}:example.com", suffix);
    let alias = format!("#rm_{}:example.com", suffix);
    let _ = storage.delete_room(&room_id).await;
    storage.create_room(&room_id, "@c:example.com", "invite", "10", false).await.unwrap();
    storage.set_room_alias(&room_id, &alias, "@c:example.com").await.unwrap();
    assert!(storage.get_room_by_alias(&alias).await.unwrap().is_some());

    storage.remove_room_alias(&room_id).await.expect("remove_room_alias should succeed");
    assert!(storage.get_room_by_alias(&alias).await.unwrap().is_none());

    let _ = storage.delete_room(&room_id).await;
}

#[tokio::test]
async fn test_set_room_version() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let room_id = format!("!version_test_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_room(&room_id).await;
    storage.create_room(&room_id, "@c:example.com", "invite", "10", false).await.unwrap();

    storage.set_room_version(&room_id, "11").await.expect("set_room_version should succeed");
    let room = storage.get_room(&room_id).await.unwrap().unwrap();
    assert_eq!(room.room_version, "11");

    let _ = storage.delete_room(&room_id).await;
}

/// A federated join stores the remote room version verbatim
/// (`synapse-services/src/room/membership/federation.rs`: make_join -> `create_room`),
/// and the capability map marks v12 as joinable/federatable.
/// `rooms.room_version` must therefore accept any digits/dots version, not just 1..11
/// (a future version can arrive through federation before this server knows it).
#[tokio::test]
async fn test_room_version_check_accepts_versions_beyond_eleven() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let room_id = format!("!v12_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_room(&room_id).await;

    storage
        .create_room(&room_id, "@c:example.com", "invite", "12", false)
        .await
        .expect("a v12 room must be storable: a hard-coded 1..11 CHECK breaks federated joins");

    let room = storage.get_room(&room_id).await.unwrap().unwrap();
    assert_eq!(room.room_version, "12");

    // Dotted form (MSC4186 stable room IDs) must also pass.
    storage.set_room_version(&room_id, "12.0").await.expect("dotted room version must be storable");

    // The CHECK must still have teeth: free-form junk is rejected.
    let bad = storage.set_room_version(&room_id, "not-a-version").await;
    assert!(bad.is_err(), "non-numeric room versions must still be rejected");

    let _ = storage.delete_room(&room_id).await;
}

#[tokio::test]
async fn test_shutdown_room_marks_private_and_renames() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let room_id = format!("!shutdown_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_room(&room_id).await;
    storage.create_room(&room_id, "@c:example.com", "invite", "10", true).await.unwrap();
    storage.set_room_directory(&room_id, true).await.unwrap();

    storage.shutdown_room(&room_id).await.expect("shutdown_room should succeed");
    let room = storage.get_room(&room_id).await.unwrap().unwrap();
    assert!(!room.is_public, "shutdown should make room private");
    assert!(room.name.as_deref().unwrap_or("").contains("SHUTDOWN"), "name should contain SHUTDOWN suffix");

    let _ = storage.delete_room(&room_id).await;
}

#[tokio::test]
async fn test_get_rooms_map_empty_input() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let map = storage.get_rooms_map(&[]).await.expect("get_rooms_map empty should succeed");
    assert!(map.is_empty());
}

#[tokio::test]
async fn test_get_rooms_map_with_rooms() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let rid1 = format!("!map1_{}:example.com", uuid::Uuid::new_v4());
    let rid2 = format!("!map2_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_room(&rid1).await;
    let _ = storage.delete_room(&rid2).await;
    storage.create_room(&rid1, "@c:example.com", "invite", "10", false).await.unwrap();
    storage.create_room(&rid2, "@c:example.com", "invite", "10", false).await.unwrap();

    let map = storage.get_rooms_map(&[rid1.clone(), rid2.clone()]).await.expect("get_rooms_map should succeed");
    assert_eq!(map.len(), 2);
    assert!(map.contains_key(&rid1));
    assert!(map.contains_key(&rid2));

    let _ = storage.delete_room(&rid1).await;
    let _ = storage.delete_room(&rid2).await;
}

#[tokio::test]
async fn test_get_rooms_batch_empty_input() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let rooms = storage.get_rooms_batch(&[]).await.expect("get_rooms_batch empty should succeed");
    assert!(rooms.is_empty());
}

#[tokio::test]
async fn test_get_user_rooms_paginated_no_cursor() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    // Querying for a user with no memberships should return empty without error
    let rooms = storage
        .get_user_rooms_paginated("@paginated_nobody:example.com", 10, None)
        .await
        .expect("get_user_rooms_paginated no cursor should succeed");
    assert!(rooms.is_empty());
}

#[tokio::test]
async fn test_get_user_room_list_summary_empty() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let summaries = storage
        .get_user_room_list_summary("@summary_nobody:example.com")
        .await
        .expect("get_user_room_list_summary should succeed");
    assert!(summaries.is_empty());
}

#[tokio::test]
async fn test_update_read_marker_default_type() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let room_id = format!("!rm_{}:example.com", uuid::Uuid::new_v4());
    let user_id = format!("@rm_user_{}:example.com", uuid::Uuid::new_v4());

    storage
        .update_read_marker(&room_id, &user_id, "$event1:example.com")
        .await
        .expect("update_read_marker should succeed");

    // Default marker_type is m.fully_read
    let marker =
        storage.get_read_marker(&room_id, &user_id, "m.fully_read").await.expect("get_read_marker should succeed");
    assert_eq!(marker.as_deref(), Some("$event1:example.com"));

    sqlx::query("DELETE FROM read_markers WHERE user_id = $1").bind(&user_id).execute(&*pool).await.unwrap();
}

#[tokio::test]
async fn test_update_read_marker_with_type() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let room_id = format!("!rmt_{}:example.com", uuid::Uuid::new_v4());
    let user_id = format!("@rmt_user_{}:example.com", uuid::Uuid::new_v4());

    storage
        .update_read_marker_with_type(&room_id, &user_id, "$e1:example.com", "m.read")
        .await
        .expect("update_read_marker_with_type m.read should succeed");
    storage
        .update_read_marker_with_type(&room_id, &user_id, "$e2:example.com", "m.marked_unread")
        .await
        .expect("update_read_marker_with_type m.marked_unread should succeed");

    let read = storage.get_read_marker(&room_id, &user_id, "m.read").await.unwrap();
    assert_eq!(read.as_deref(), Some("$e1:example.com"));
    let unread = storage.get_read_marker(&room_id, &user_id, "m.marked_unread").await.unwrap();
    assert_eq!(unread.as_deref(), Some("$e2:example.com"));

    sqlx::query("DELETE FROM read_markers WHERE user_id = $1").bind(&user_id).execute(&*pool).await.unwrap();
}

#[tokio::test]
async fn test_get_read_marker_absent() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let marker = storage
        .get_read_marker("!nope:example.com", "@nobody:example.com", "m.fully_read")
        .await
        .expect("get_read_marker absent should succeed");
    assert!(marker.is_none());
}

#[tokio::test]
async fn test_get_all_read_markers() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let room_id = format!("!allrm_{}:example.com", uuid::Uuid::new_v4());
    let user_id = format!("@allrm_user_{}:example.com", uuid::Uuid::new_v4());

    storage.update_read_marker_with_type(&room_id, &user_id, "$e1:example.com", "m.fully_read").await.unwrap();
    storage.update_read_marker_with_type(&room_id, &user_id, "$e2:example.com", "m.read").await.unwrap();

    let all = storage.get_all_read_markers(&room_id, &user_id).await.expect("get_all_read_markers should succeed");
    assert_eq!(all.len(), 2);
    assert_eq!(all.get("m.fully_read"), Some(&"$e1:example.com".to_string()));
    assert_eq!(all.get("m.read"), Some(&"$e2:example.com".to_string()));

    sqlx::query("DELETE FROM read_markers WHERE user_id = $1").bind(&user_id).execute(&*pool).await.unwrap();
}

#[tokio::test]
async fn test_get_all_read_markers_empty() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let all = storage
        .get_all_read_markers("!nope:example.com", "@nobody:example.com")
        .await
        .expect("get_all_read_markers empty should succeed");
    assert!(all.is_empty());
}

/// MSC4446: Without `allow_backward`, moving the fully read marker
/// backwards must be silently dropped (return `false`, no update).
#[tokio::test]
async fn test_update_read_marker_monotonic_blocks_backward() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!msc4446_blk_{}:example.com", suffix);
    let user_id = format!("@msc4446_user_{}:example.com", suffix);
    let old_event_id = format!("$msc4446_old_{}:example.com", suffix);
    let new_event_id = format!("$msc4446_new_{}:example.com", suffix);

    let _ = sqlx::query("DELETE FROM read_markers WHERE user_id = $1").bind(&user_id).execute(&*pool).await;
    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id, &user_id).await;

    // Insert two events: old (ts=1000) and new (ts=2000)
    for (eid, ts) in [(&old_event_id, 1000i64), (&new_event_id, 2000i64)] {
        sqlx::query(
            r#"INSERT INTO events (event_id, room_id, sender, user_id, event_type, content, origin_server_ts)
                   VALUES ($1, $2, $3, $3, 'm.room.message', '{}'::jsonb, $4)
                   ON CONFLICT (event_id) DO NOTHING"#,
        )
        .bind(eid)
        .bind(&room_id)
        .bind(&user_id)
        .bind(ts)
        .execute(&*pool)
        .await
        .expect("insert test event");
    }

    // Set marker to the NEW event first
    storage.update_read_marker_with_type(&room_id, &user_id, &new_event_id, "m.fully_read").await.unwrap();

    // Try to move BACKWARD to old event without allow_backward → should be blocked
    let updated = storage
        .update_read_marker_monotonic(&room_id, &user_id, &old_event_id, "m.fully_read", false)
        .await
        .expect("update_read_marker_monotonic should succeed");

    assert!(!updated, "backward move without allow_backward should be silently dropped");

    // Verify marker still points to the new event
    let marker = storage.get_read_marker(&room_id, &user_id, "m.fully_read").await.unwrap();
    assert_eq!(marker.as_deref(), Some(new_event_id.as_str()));

    let _ = sqlx::query("DELETE FROM read_markers WHERE user_id = $1").bind(&user_id).execute(&*pool).await;
    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
}

/// MSC4446: With `allow_backward=true`, moving the `m.fully_read` marker
/// backwards is accepted, but `m.read` still enforces monotonicity.
#[tokio::test]
async fn test_update_read_marker_monotonic_allows_backward_with_flag() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4();
    let room_id = format!("!msc4446_ab_{}:example.com", suffix);
    let user_id = format!("@msc4446_ab_user_{}:example.com", suffix);
    let old_event_id = format!("$msc4446_ab_old_{}:example.com", suffix);
    let new_event_id = format!("$msc4446_ab_new_{}:example.com", suffix);

    let _ = sqlx::query("DELETE FROM read_markers WHERE user_id = $1").bind(&user_id).execute(&*pool).await;
    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
    ensure_test_room(&pool, &room_id, &user_id).await;

    for (eid, ts) in [(&old_event_id, 1000i64), (&new_event_id, 2000i64)] {
        sqlx::query(
            r#"INSERT INTO events (event_id, room_id, sender, user_id, event_type, content, origin_server_ts)
                   VALUES ($1, $2, $3, $3, 'm.room.message', '{}'::jsonb, $4)
                   ON CONFLICT (event_id) DO NOTHING"#,
        )
        .bind(eid)
        .bind(&room_id)
        .bind(&user_id)
        .bind(ts)
        .execute(&*pool)
        .await
        .expect("insert test event");
    }

    // Set fully_read marker to NEW event
    storage.update_read_marker_with_type(&room_id, &user_id, &new_event_id, "m.fully_read").await.unwrap();

    // Move BACKWARD with allow_backward=true → should succeed
    let updated = storage
        .update_read_marker_monotonic(&room_id, &user_id, &old_event_id, "m.fully_read", true)
        .await
        .expect("update_read_marker_monotonic should succeed");

    assert!(updated, "backward move with allow_backward=true for m.fully_read should succeed");
    let marker = storage.get_read_marker(&room_id, &user_id, "m.fully_read").await.unwrap();
    assert_eq!(marker.as_deref(), Some(old_event_id.as_str()));

    // Set m.read marker to NEW event
    storage.update_read_marker_with_type(&room_id, &user_id, &new_event_id, "m.read").await.unwrap();

    // Try to move m.read BACKWARD with allow_backward=true → should be BLOCKED
    let updated_read = storage
        .update_read_marker_monotonic(&room_id, &user_id, &old_event_id, "m.read", true)
        .await
        .expect("update_read_marker_monotonic should succeed");

    assert!(!updated_read, "m.read must always enforce monotonicity even with allow_backward");
    let read_marker = storage.get_read_marker(&room_id, &user_id, "m.read").await.unwrap();
    assert_eq!(read_marker.as_deref(), Some(new_event_id.as_str()));

    let _ = sqlx::query("DELETE FROM read_markers WHERE user_id = $1").bind(&user_id).execute(&*pool).await;
    let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(&room_id).execute(&*pool).await;
}

#[tokio::test]
async fn test_add_receipt_and_get_receipts() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let room_id = format!("!rcpt_{}:example.com", uuid::Uuid::new_v4());
    let user_id = format!("@rcpt_user_{}:example.com", uuid::Uuid::new_v4());
    let event_id = format!("$rcpt_evt_{}:example.com", uuid::Uuid::new_v4());

    storage
        .add_receipt(&user_id, "server", &room_id, &event_id, "m.read", &json!({"ts": 123}))
        .await
        .expect("add_receipt should succeed");

    let receipts = storage.get_receipts(&room_id, "m.read", &event_id).await.expect("get_receipts should succeed");
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].user_id, user_id);
    assert_eq!(receipts[0].event_id, event_id);
    assert_eq!(receipts[0].receipt_type, "m.read");

    sqlx::query("DELETE FROM event_receipts WHERE user_id = $1").bind(&user_id).execute(&*pool).await.unwrap();
}

#[tokio::test]
async fn test_add_receipt_replaces_previous_for_same_type() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let room_id = format!("!rcpt2_{}:example.com", uuid::Uuid::new_v4());
    let user_id = format!("@rcpt2_user_{}:example.com", uuid::Uuid::new_v4());
    let event1 = format!("$evt1_{}:example.com", uuid::Uuid::new_v4());
    let event2 = format!("$evt2_{}:example.com", uuid::Uuid::new_v4());

    // Add first receipt
    storage.add_receipt(&user_id, "server", &room_id, &event1, "m.read", &json!({})).await.unwrap();
    // Add second receipt for the same user+type but different event:
    // add_receipt deletes prior receipts of the same type for other events
    storage.add_receipt(&user_id, "server", &room_id, &event2, "m.read", &json!({})).await.unwrap();

    // The first event should no longer have a receipt
    let receipts1 = storage.get_receipts(&room_id, "m.read", &event1).await.unwrap();
    assert!(receipts1.is_empty(), "old event receipt should be removed");

    // The new event should have the receipt
    let receipts2 = storage.get_receipts(&room_id, "m.read", &event2).await.unwrap();
    assert_eq!(receipts2.len(), 1);

    sqlx::query("DELETE FROM event_receipts WHERE user_id = $1").bind(&user_id).execute(&*pool).await.unwrap();
}

#[tokio::test]
async fn test_get_receipts_empty() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let receipts = storage
        .get_receipts("!nope:example.com", "m.read", "$nope:example.com")
        .await
        .expect("get_receipts empty should succeed");
    assert!(receipts.is_empty());
}

#[tokio::test]
async fn test_remove_room_directory() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let room_id = format!("!dirdel_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_room(&room_id).await;
    storage.create_room(&room_id, "@c:example.com", "invite", "10", true).await.unwrap();
    storage.set_room_directory(&room_id, true).await.unwrap();
    assert!(storage.is_room_in_directory(&room_id).await.unwrap());

    storage.remove_room_directory(&room_id).await.expect("remove_room_directory should succeed");
    assert!(!storage.is_room_in_directory(&room_id).await.unwrap());

    let _ = storage.delete_room(&room_id).await;
}

#[tokio::test]
async fn test_set_room_visibility_invalid_defaults_private() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    let room_id = format!("!vis_{}:example.com", uuid::Uuid::new_v4());
    let _ = storage.delete_room(&room_id).await;
    storage.create_room(&room_id, "@c:example.com", "invite", "10", true).await.unwrap();
    assert!(storage.get_room(&room_id).await.unwrap().unwrap().is_public);

    // Unknown visibility string should default to "private"
    storage.set_room_visibility(&room_id, "unknown_value").await.expect("set_room_visibility unknown should succeed");
    let room = storage.get_room(&room_id).await.unwrap().unwrap();
    assert!(!room.is_public, "unknown visibility should default to private");

    let _ = storage.delete_room(&room_id).await;
}

#[tokio::test]
async fn test_get_public_rooms_paginated_with_cursor() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomStorage::new(&pool);
    // First page
    let first_page =
        storage.get_public_rooms_paginated(5, None, None, &crate::room::RoomTypeFilter::default()).await.unwrap();
    assert!(first_page.len() <= 5);

    // If we got a full page, attempt a second page using the last item as cursor
    if let Some(last) = first_page.last() {
        let second_page = storage
            .get_public_rooms_paginated(
                5,
                Some(last.created_ts),
                Some(&last.room_id),
                &crate::room::RoomTypeFilter::default(),
            )
            .await
            .expect("get_public_rooms_paginated with cursor should succeed");
        // Second page should not overlap with first page's last item
        if let Some(second_last) = second_page.last() {
            assert!(second_last.room_id != last.room_id || second_last.created_ts != last.created_ts);
        }
    }
}
