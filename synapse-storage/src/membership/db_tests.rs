use super::*;
use std::sync::Arc;

/// 每个测试一个从迁移 baseline 克隆出来的独立 schema（返回 guard 与 pool）。
///
/// 2026-09-21：原先用共享 `public` 池。共享池的问题：测试结果取决于环境里 `public` 的
/// 状态（本地 `public` 落后于迁移 baseline 时会直接 42P01），且并行测试互相影响。
/// 按铁律 7 消除状态共享：per-test schema 由模板克隆，表一定存在、行数从 0 开始。
async fn test_pool() -> (crate::test_isolation::IsolatedTestPool, Arc<sqlx::PgPool>) {
    let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated test pool");
    let pool = isolated.pool();
    (isolated, pool)
}

async fn ensure_test_room(pool: &sqlx::PgPool, room_id: &str) {
    sqlx::query(
        "INSERT INTO rooms (room_id, room_version, is_public, creator, created_ts) \
             VALUES ($1, '1', false, '@test:localhost', (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT) \
             ON CONFLICT (room_id) DO NOTHING",
    )
    .bind(room_id)
    .execute(pool)
    .await
    .expect("failed to create test room");
}

async fn ensure_test_user(pool: &sqlx::PgPool, user_id: &str) {
    let now = current_timestamp_millis();
    let username = user_id.strip_prefix('@').and_then(|u| u.split(':').next()).unwrap_or("testuser");
    sqlx::query(
        "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) \
             ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(user_id)
    .bind(username)
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to create test user");
}

/// Insert a membership row with explicit timestamps so LEFT_ROOM windows
/// can be exercised deterministically.
async fn insert_membership_row(
    pool: &sqlx::PgPool,
    room_id: &str,
    user_id: &str,
    membership: &str,
    joined_ts: Option<i64>,
    updated_ts: i64,
) {
    sqlx::query(
        "INSERT INTO room_memberships (room_id, user_id, membership, joined_ts, updated_ts) \
             VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(room_id)
    .bind(user_id)
    .bind(membership)
    .bind(joined_ts)
    .bind(updated_ts)
    .execute(pool)
    .await
    .expect("failed to insert membership row");
}

async fn cleanup_membership_data(pool: &sqlx::PgPool, suffix: &str) {
    let pattern = format!("%{suffix}%");
    sqlx::query("DELETE FROM room_memberships WHERE user_id LIKE $1")
        .bind(&pattern)
        .execute(pool)
        .await
        .expect("test fixture: delete must succeed — a swallowed error here surfaces later as an unrelated failure");
    sqlx::query("DELETE FROM room_memberships WHERE room_id LIKE $1")
        .bind(&pattern)
        .execute(pool)
        .await
        .expect("test fixture: delete must succeed — a swallowed error here surfaces later as an unrelated failure");
    sqlx::query("DELETE FROM rooms WHERE room_id LIKE $1")
        .bind(&pattern)
        .execute(pool)
        .await
        .expect("test fixture: delete must succeed — a swallowed error here surfaces later as an unrelated failure");
    sqlx::query("DELETE FROM users WHERE user_id LIKE $1")
        .bind(&pattern)
        .execute(pool)
        .await
        .expect("test fixture: delete must succeed — a swallowed error here surfaces later as an unrelated failure");
}

// ── 1. add_member ─────────────────────────────────────────────

#[tokio::test]
async fn test_add_member_all_membership_types() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomMemberStorage::new(&pool, "localhost");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user_id = format!("@mem_test_types_{suffix}:localhost");
    let room_id = format!("!room_test_types_{suffix}:localhost");

    cleanup_membership_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_id).await;

    // Join
    let m = storage.add_member(&room_id, &user_id, "join", Some("TN"), None, None, None).await.unwrap();
    assert_eq!(m.membership, "join");
    assert_eq!(m.display_name.as_deref(), Some("TN"));
    assert!(m.joined_ts.is_some());

    // Invite (upsert)
    let m = storage.add_member(&room_id, &user_id, "invite", None, None, None, None).await.unwrap();
    assert_eq!(m.membership, "invite");

    // Ban (upsert)
    let m = storage.add_member(&room_id, &user_id, "ban", None, None, None, None).await.unwrap();
    assert_eq!(m.membership, "ban");

    // Leave (upsert)
    let m = storage.add_member(&room_id, &user_id, "leave", None, None, None, None).await.unwrap();
    assert_eq!(m.membership, "leave");

    cleanup_membership_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_add_member_with_sender() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomMemberStorage::new(&pool, "localhost");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user_id = format!("@mem_sender_{suffix}:localhost");
    let sender = format!("@mem_inviter_{suffix}:localhost");
    let room_id = format!("!room_sender_{suffix}:localhost");

    cleanup_membership_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_id).await;
    ensure_test_user(&pool, &sender).await;

    let m = storage.add_member(&room_id, &user_id, "invite", None, None, Some(&sender), None).await.unwrap();
    assert_eq!(m.sender.as_deref(), Some(sender.as_str()));

    cleanup_membership_data(&pool, &suffix).await;
}

// ── 2. get_member ─────────────────────────────────────────────

#[tokio::test]
async fn test_get_member_found_and_not_found() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomMemberStorage::new(&pool, "localhost");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user_id = format!("@mem_found_{suffix}:localhost");
    let room_id = format!("!room_found_{suffix}:localhost");

    cleanup_membership_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_id).await;

    storage.add_member(&room_id, &user_id, "join", None, None, None, None).await.unwrap();

    // Found
    let result = storage.get_member(&room_id, &user_id).await.unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().membership, "join");

    // Not found — non-existent user
    let result = storage.get_member(&room_id, "@nonexistent:localhost").await.unwrap();
    assert!(result.is_none());

    // Not found — non-existent room
    let result = storage.get_member("!nonexistent:localhost", &user_id).await.unwrap();
    assert!(result.is_none());

    cleanup_membership_data(&pool, &suffix).await;
}

// ── 3. get_room_members ───────────────────────────────────────

#[tokio::test]
async fn test_get_room_members_filtered_by_type() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomMemberStorage::new(&pool, "localhost");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user_a = format!("@mem_filter_a_{suffix}:localhost");
    let user_b = format!("@mem_filter_b_{suffix}:localhost");
    let room_id = format!("!room_filter_{suffix}:localhost");

    cleanup_membership_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_a).await;
    ensure_test_user(&pool, &user_b).await;

    storage.add_member(&room_id, &user_a, "join", None, None, None, None).await.unwrap();
    storage.add_member(&room_id, &user_b, "invite", None, None, None, None).await.unwrap();

    let joins = storage.get_room_members(&room_id, "join").await.unwrap();
    assert_eq!(joins.len(), 1);
    assert_eq!(joins[0].user_id, user_a);

    let invites = storage.get_room_members(&room_id, "invite").await.unwrap();
    assert_eq!(invites.len(), 1);
    assert_eq!(invites[0].user_id, user_b);

    let leaves = storage.get_room_members(&room_id, "leave").await.unwrap();
    assert!(leaves.is_empty());

    cleanup_membership_data(&pool, &suffix).await;
}

// ── 4. has_any_non_banned_member_from_server ──────────────────

#[tokio::test]
async fn test_has_any_non_banned_member_from_server_true() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomMemberStorage::new(&pool, "localhost");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user_id = format!("@mem_srv_true_{suffix}:localhost");
    let room_id = format!("!room_srv_true_{suffix}:localhost");

    cleanup_membership_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_id).await;

    storage.add_member(&room_id, &user_id, "join", None, None, None, None).await.unwrap();

    let has = storage.has_any_non_banned_member_from_server(&room_id, "localhost").await.unwrap();
    assert!(has);

    cleanup_membership_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_has_any_non_banned_member_from_server_false() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomMemberStorage::new(&pool, "localhost");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user_id = format!("@mem_srv_false_{suffix}:localhost");
    let room_id = format!("!room_srv_false_{suffix}:localhost");

    cleanup_membership_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_id).await;

    storage.add_member(&room_id, &user_id, "ban", None, None, None, None).await.unwrap();

    let has = storage.has_any_non_banned_member_from_server(&room_id, "localhost").await.unwrap();
    assert!(!has);

    // Also false for a server with no members at all
    let has_other = storage.has_any_non_banned_member_from_server(&room_id, "otherhost").await.unwrap();
    assert!(!has_other);

    cleanup_membership_data(&pool, &suffix).await;
}

// ── 5. get_room_member_count ───────────────────────────────────

#[tokio::test]
async fn test_get_room_member_count() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomMemberStorage::new(&pool, "localhost");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user_a = format!("@mem_count_a_{suffix}:localhost");
    let user_b = format!("@mem_count_b_{suffix}:localhost");
    let room_id = format!("!room_count_{suffix}:localhost");
    let empty_room = format!("!room_count_empty_{suffix}:localhost");

    cleanup_membership_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_room(&pool, &empty_room).await;
    ensure_test_user(&pool, &user_a).await;
    ensure_test_user(&pool, &user_b).await;

    // Empty room
    let count = storage.get_room_member_count(&empty_room).await.unwrap();
    assert_eq!(count, 0);

    // With join members
    storage.add_member(&room_id, &user_a, "join", None, None, None, None).await.unwrap();
    storage.add_member(&room_id, &user_b, "join", None, None, None, None).await.unwrap();
    let count = storage.get_room_member_count(&room_id).await.unwrap();
    assert_eq!(count, 2);

    // Leave member should not count
    storage.add_member(&room_id, &user_b, "leave", None, None, None, None).await.unwrap();
    let count = storage.get_room_member_count(&room_id).await.unwrap();
    assert_eq!(count, 1);

    cleanup_membership_data(&pool, &suffix).await;
}

// ── 6. get_room_members_paginated ──────────────────────────────

#[tokio::test]
async fn test_get_room_members_paginated() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomMemberStorage::new(&pool, "localhost");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user_0 = format!("@mem_page_0_{suffix}:localhost");
    let user_1 = format!("@mem_page_1_{suffix}:localhost");
    let user_2 = format!("@mem_page_2_{suffix}:localhost");
    let room_id = format!("!room_page_{suffix}:localhost");

    cleanup_membership_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_0).await;
    ensure_test_user(&pool, &user_1).await;
    ensure_test_user(&pool, &user_2).await;

    storage.add_member(&room_id, &user_0, "join", None, None, None, None).await.unwrap();
    storage.add_member(&room_id, &user_1, "join", None, None, None, None).await.unwrap();
    storage.add_member(&room_id, &user_2, "join", None, None, None, None).await.unwrap();

    // No cursor, limit=2 — first page
    let page = storage.get_room_members_paginated(&room_id, "join", 2, None).await.unwrap();
    assert_eq!(page.len(), 2);
    assert_eq!(page[0].user_id, user_0);
    assert_eq!(page[1].user_id, user_1);

    // Cursor from user_0, limit=2 — second page
    let page = storage.get_room_members_paginated(&room_id, "join", 2, Some(&user_0)).await.unwrap();
    assert_eq!(page.len(), 2);
    assert_eq!(page[0].user_id, user_1);
    assert_eq!(page[1].user_id, user_2);

    // Cursor from user_1, limit=5 — remaining
    let page = storage.get_room_members_paginated(&room_id, "join", 5, Some(&user_1)).await.unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].user_id, user_2);

    cleanup_membership_data(&pool, &suffix).await;
}

// ── 6b. get_room_members_paginated_with_profiles ──────────────

/// MSC4502 的 profile 分页（游标 + profile 透传）—— **此前零 storage 级覆盖**：
/// 只有 `synapse-services` 服务层的 `get_room_members_paginated_*` 用例覆盖它，而那层注入的是
/// `test_mocks::InMemoryMemberStore`，**从不执行这里的 SQL**。
///
/// 为什么必须用真 baseline 往返：方法内部按 `membership` / `not_membership` / 游标
/// 三个可选谓词拼 `WHERE`，每条又有 `ASC`/`DESC` 与 `>`/`<` 两种方向
/// ⇒ **多种 SQL 文本**；`$n` 占位符与 `bind(...)` 的顺序只在运行期对齐，编译期看不见
/// ⇒ "拼出来的 SQL 在当前 schema 上真的能跑、参数没串位"只能靠这里钉住
/// （列名漂移在运行期是 42703，正是 D14-2 想提前到测试期的那类失败）。
///
/// 本用例逐条走完 8 种过滤/游标形态 + `membership_type = None`（不过滤状态，I-13），
/// 并顺带钉住 `LEFT JOIN users` 的 profile 透传
/// （有 profile ⇒ `Some`，无 profile ⇒ `None`，后者是 D-20 型的可空性方向）。
#[tokio::test]
async fn test_get_room_members_paginated_with_profiles_covers_every_query_shape() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomMemberStorage::new(&pool, "localhost");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let room_id = format!("!room_mem_prof_{suffix}:localhost");
    // 4 个 join + 1 个 invite：invite 用来验证 `not_membership` 过滤真的生效
    // （若过滤失效，分支 3/4 会多出一条 membership='invite' 的行，断言立刻红）。
    let joins: Vec<String> = (0..4).map(|i| format!("@mem_prof_{i}_{suffix}:localhost")).collect();
    let invited = format!("@mem_prof_inv_{suffix}:localhost");

    cleanup_membership_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;
    for u in joins.iter().chain(std::iter::once(&invited)) {
        ensure_test_user(&pool, u).await;
    }
    // profile 只给 joins[1]：其余兄弟保持无 profile ⇒ LEFT JOIN 的 NULL 分支也被走到
    sqlx::query("UPDATE users SET displayname = $1, avatar_url = $2 WHERE user_id = $3")
        .bind("Alice")
        .bind("mxc://localhost/alice")
        .bind(&joins[1])
        .execute(&*pool)
        .await
        .expect("seed profile");
    for u in &joins {
        storage.add_member(&room_id, u, "join", None, None, None, None).await.unwrap();
    }
    storage.add_member(&room_id, &invited, "invite", None, None, None, None).await.unwrap();

    // 该元组类型就是本方法的返回元素（方法签名里也这么写），局部别名只为避开
    // clippy::type_complexity（本仓 clippy 以 `-D warnings` 阻断）。
    type MemberWithProfile = (RoomMember, Option<String>, Option<String>);
    let ids = |rows: &[MemberWithProfile]| -> Vec<String> { rows.iter().map(|(m, _, _)| m.user_id.clone()).collect() };

    // ① 无 not_membership、无游标（ORDER BY rm.user_id ASC）
    let page = storage
        .get_room_members_paginated_with_profiles(&room_id, Some("join"), None, 10, None, None)
        .await
        .expect("branch 1 (no filter, no cursor)");
    assert_eq!(ids(&page), joins, "① ASC 全量");
    // profile 透传：只有 joins[1] 有，其余是 None（LEFT JOIN 的 NULL 分支）
    assert_eq!(page[1].1.as_deref(), Some("Alice"), "① profile.displayname 必须透传");
    assert_eq!(page[1].2.as_deref(), Some("mxc://localhost/alice"), "① profile.avatar_url 必须透传");
    assert_eq!(page[0].1, None, "① 无 profile 的用户必须是 None（LEFT JOIN 未命中）");
    assert_eq!(page[0].2, None, "① 无 avatar 的用户必须是 None");

    // ② 无 not_membership、有游标（`rm.user_id > $3`）
    let page = storage
        .get_room_members_paginated_with_profiles(&room_id, Some("join"), None, 10, Some(&joins[0]), None)
        .await
        .expect("branch 2 (no filter, cursor)");
    assert_eq!(ids(&page), joins[1..].to_vec(), "② 游标之后的行");

    // ③ 有 not_membership、无游标（invite 必须被排除）
    let page = storage
        .get_room_members_paginated_with_profiles(&room_id, Some("join"), Some("invite"), 10, None, None)
        .await
        .expect("branch 3 (filter, no cursor)");
    assert_eq!(ids(&page), joins, "③ not_membership='invite' 必须过滤掉 invite 行");

    // ④ 有 not_membership、有游标
    let page = storage
        .get_room_members_paginated_with_profiles(&room_id, Some("join"), Some("invite"), 10, Some(&joins[1]), None)
        .await
        .expect("branch 4 (filter, cursor)");
    assert_eq!(ids(&page), joins[2..].to_vec(), "④ 过滤 + 游标");

    // ⑤–⑧ 反向（dir=Some("b") ⇒ ORDER BY … DESC + `rm.user_id < $n`）
    let page = storage
        .get_room_members_paginated_with_profiles(&room_id, Some("join"), None, 10, None, Some("b"))
        .await
        .expect("branch 5 (no filter, no cursor, backward)");
    let mut desc = joins.clone();
    desc.reverse();
    assert_eq!(ids(&page), desc, "⑤ DESC 全量");

    let page = storage
        .get_room_members_paginated_with_profiles(&room_id, Some("join"), None, 10, Some(&joins[2]), Some("b"))
        .await
        .expect("branch 6 (no filter, cursor, backward)");
    assert_eq!(ids(&page), vec![joins[1].clone(), joins[0].clone()], "⑥ DESC 游标之前");

    let page = storage
        .get_room_members_paginated_with_profiles(&room_id, Some("join"), Some("invite"), 10, None, Some("b"))
        .await
        .expect("branch 7 (filter, no cursor, backward)");
    assert_eq!(ids(&page), desc, "⑦ DESC + 过滤");

    let page = storage
        .get_room_members_paginated_with_profiles(
            &room_id,
            Some("join"),
            Some("invite"),
            10,
            Some(&joins[2]),
            Some("b"),
        )
        .await
        .expect("branch 8 (filter, cursor, backward)");
    assert_eq!(ids(&page), vec![joins[1].clone(), joins[0].clone()], "⑧ DESC + 过滤 + 游标");

    // 游标语义的另一半：`from_user_id` 只按 `>`/`<` 比较，不包含游标自身
    let page = storage
        .get_room_members_paginated_with_profiles(&room_id, Some("join"), None, 1, None, None)
        .await
        .expect("limit 生效");
    assert_eq!(ids(&page), vec![joins[0].clone()], "limit 必须生效");

    // ⑨ `membership_type = None` ⇒ 不过滤状态，invite 行也必须出现（I-13：
    // `GET /rooms/{roomId}/members` 不带 `membership` 参数时的语义）。
    let page = storage
        .get_room_members_paginated_with_profiles(&room_id, None, None, 10, None, None)
        .await
        .expect("no membership filter ⇒ every state");
    let mut all = joins.clone();
    all.push(invited.clone());
    all.sort();
    let mut actual = ids(&page);
    actual.sort();
    assert_eq!(actual, all, "⑨ 无 membership 过滤必须返回全部状态（含 invite）");

    cleanup_membership_data(&pool, &suffix).await;
}

// ── 7. remove_member ──────────────────────────────────────────

#[tokio::test]
async fn test_remove_member() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomMemberStorage::new(&pool, "localhost");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user_id = format!("@mem_remove_{suffix}:localhost");
    let room_id = format!("!room_remove_{suffix}:localhost");

    cleanup_membership_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_id).await;

    storage.add_member(&room_id, &user_id, "join", None, None, None, None).await.unwrap();
    assert!(storage.is_member(&room_id, &user_id).await.unwrap());

    storage.remove_member(&room_id, &user_id, None).await.unwrap();

    // Should no longer be a member
    assert!(!storage.is_member(&room_id, &user_id).await.unwrap());
    let m = storage.get_member(&room_id, &user_id).await.unwrap();
    assert!(m.is_some());
    assert_eq!(m.unwrap().membership, "leave");

    cleanup_membership_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_remove_member_idempotent() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomMemberStorage::new(&pool, "localhost");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user_id = format!("@mem_rm_idem_{suffix}:localhost");
    let room_id = format!("!room_rm_idem_{suffix}:localhost");

    cleanup_membership_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_id).await;

    storage.add_member(&room_id, &user_id, "leave", None, None, None, None).await.unwrap();
    // Removing an already-left member should not panic/error
    let result = storage.remove_member(&room_id, &user_id, None).await;
    assert!(result.is_ok());

    cleanup_membership_data(&pool, &suffix).await;
}

// ── 8. forget_member / is_forgotten ────────────────────────────

#[tokio::test]
async fn test_forget_member() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomMemberStorage::new(&pool, "localhost");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user_id = format!("@mem_forget_{suffix}:localhost");
    let room_id = format!("!room_forget_{suffix}:localhost");

    cleanup_membership_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_id).await;

    // Join then leave, then forget
    storage.add_member(&room_id, &user_id, "join", None, None, None, None).await.unwrap();
    storage.remove_member(&room_id, &user_id, None).await.unwrap();

    storage.forget_member(&room_id, &user_id, None).await.unwrap();

    assert!(storage.is_forgotten(&room_id, &user_id).await.unwrap());

    cleanup_membership_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_is_forgotten_returns_false() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomMemberStorage::new(&pool, "localhost");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user_id = format!("@mem_not_fgt_{suffix}:localhost");
    let room_id = format!("!room_not_fgt_{suffix}:localhost");

    cleanup_membership_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_id).await;

    storage.add_member(&room_id, &user_id, "join", None, None, None, None).await.unwrap();

    // Not forgotten (still join)
    assert!(!storage.is_forgotten(&room_id, &user_id).await.unwrap());

    // Non-existent member
    assert!(!storage.is_forgotten(&room_id, "@noone:localhost").await.unwrap());

    cleanup_membership_data(&pool, &suffix).await;
}

// ── 9. get_shared_room_users ───────────────────────────────────

#[tokio::test]
async fn test_get_shared_room_users_shared() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomMemberStorage::new(&pool, "localhost");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user_a = format!("@mem_share_a_{suffix}:localhost");
    let user_b = format!("@mem_share_b_{suffix}:localhost");
    let room_id = format!("!room_share_{suffix}:localhost");

    cleanup_membership_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_a).await;
    ensure_test_user(&pool, &user_b).await;

    storage.add_member(&room_id, &user_a, "join", None, None, None, None).await.unwrap();
    storage.add_member(&room_id, &user_b, "join", None, None, None, None).await.unwrap();

    let shared = storage.get_shared_room_users(&user_a).await.unwrap();
    assert_eq!(shared.len(), 1);
    assert!(shared.contains(&user_b));

    cleanup_membership_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_get_shared_room_users_no_shared() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomMemberStorage::new(&pool, "localhost");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user_a = format!("@mem_noshare_{suffix}:localhost");
    let room_id = format!("!room_noshare_{suffix}:localhost");

    cleanup_membership_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_a).await;

    storage.add_member(&room_id, &user_a, "join", None, None, None, None).await.unwrap();

    let shared = storage.get_shared_room_users(&user_a).await.unwrap();
    assert!(shared.is_empty());

    cleanup_membership_data(&pool, &suffix).await;
}

// ── 9b. get_users_no_longer_sharing_rooms_since (MSC4262 LEFT_ROOM) ──

#[tokio::test]
async fn test_left_room_reports_a_user_who_left_a_room_i_still_joined() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomMemberStorage::new(&pool, "localhost");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let me = format!("@mem_left_me_{suffix}:localhost");
    let alice = format!("@mem_left_alice_{suffix}:localhost");
    let bob = format!("@mem_left_bob_{suffix}:localhost");
    let room_shared = format!("!room_left_shared_{suffix}:localhost");
    let room_other = format!("!room_left_other_{suffix}:localhost");

    cleanup_membership_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_shared).await;
    ensure_test_room(&pool, &room_other).await;
    for user in [&me, &alice, &bob] {
        ensure_test_user(&pool, user).await;
    }

    const JOINED: i64 = 1_000;
    const SINCE: i64 = 1_500;
    const LEFT: i64 = 2_000;

    insert_membership_row(&pool, &room_shared, &me, "join", Some(JOINED), JOINED).await;
    insert_membership_row(&pool, &room_shared, &alice, "join", Some(JOINED), JOINED).await;
    insert_membership_row(&pool, &room_shared, &bob, "leave", Some(JOINED), LEFT).await;
    insert_membership_row(&pool, &room_other, &me, "join", Some(JOINED), JOINED).await;
    insert_membership_row(&pool, &room_other, &bob, "join", Some(JOINED), LEFT).await;

    // bob still shares `room_other`, so nothing is reported yet.
    let users = storage.get_users_no_longer_sharing_rooms_since(&me, SINCE).await.unwrap();
    assert!(users.is_empty(), "still sharing a room must suppress LEFT_ROOM: {users:?}");

    sqlx::query("DELETE FROM room_memberships WHERE room_id = $1 AND user_id = $2")
        .bind(&room_other)
        .bind(&bob)
        .execute(pool.as_ref())
        .await
        .unwrap();

    let users = storage.get_users_no_longer_sharing_rooms_since(&me, SINCE).await.unwrap();
    assert_eq!(users, vec![bob]);

    cleanup_membership_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_left_room_reports_room_mates_after_i_left() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomMemberStorage::new(&pool, "localhost");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let me = format!("@mem_dep_me_{suffix}:localhost");
    let alice = format!("@mem_dep_alice_{suffix}:localhost");
    let carol = format!("@mem_dep_carol_{suffix}:localhost");
    let dave = format!("@mem_dep_dave_{suffix}:localhost");
    let eve = format!("@mem_dep_eve_{suffix}:localhost");
    let room_left = format!("!room_dep_left_{suffix}:localhost");
    let room_other = format!("!room_dep_other_{suffix}:localhost");

    cleanup_membership_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_left).await;
    ensure_test_room(&pool, &room_other).await;
    for user in [&me, &alice, &carol, &dave, &eve] {
        ensure_test_user(&pool, user).await;
    }

    const JOINED: i64 = 1_000;
    const SINCE: i64 = 2_500;
    const MY_LEAVE: i64 = 3_000;
    const JOINED_LATER: i64 = 4_000;

    insert_membership_row(&pool, &room_left, &me, "leave", Some(JOINED), MY_LEAVE).await;
    insert_membership_row(&pool, &room_left, &alice, "join", Some(JOINED), JOINED).await;
    insert_membership_row(&pool, &room_left, &carol, "join", Some(JOINED_LATER), JOINED_LATER).await;
    insert_membership_row(&pool, &room_left, &dave, "join", Some(JOINED), JOINED).await;
    // Invite-only membership never became a joined room, so it must not leak.
    insert_membership_row(&pool, &room_left, &eve, "invite", None, JOINED).await;
    insert_membership_row(&pool, &room_other, &me, "join", Some(JOINED), JOINED).await;
    insert_membership_row(&pool, &room_other, &dave, "join", Some(JOINED), JOINED).await;

    let mut users = storage.get_users_no_longer_sharing_rooms_since(&me, SINCE).await.unwrap();
    users.sort();
    assert_eq!(users, vec![alice], "only room mates present before my leave, minus still-shared users");

    cleanup_membership_data(&pool, &suffix).await;
}

// ── 10. remove_all_members ─────────────────────────────────────

#[tokio::test]
async fn test_remove_all_members() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomMemberStorage::new(&pool, "localhost");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user_a = format!("@mem_clr_a_{suffix}:localhost");
    let user_b = format!("@mem_clr_b_{suffix}:localhost");
    let room_id = format!("!room_clr_{suffix}:localhost");

    cleanup_membership_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_a).await;
    ensure_test_user(&pool, &user_b).await;

    storage.add_member(&room_id, &user_a, "join", None, None, None, None).await.unwrap();
    storage.add_member(&room_id, &user_b, "join", None, None, None, None).await.unwrap();

    let count_before = storage.get_room_member_count(&room_id).await.unwrap();
    assert_eq!(count_before, 2);

    storage.remove_all_members(&room_id).await.unwrap();

    let members = storage.get_room_members(&room_id, "join").await.unwrap();
    assert!(members.is_empty());

    // Idempotent
    let result = storage.remove_all_members(&room_id).await;
    assert!(result.is_ok());

    cleanup_membership_data(&pool, &suffix).await;
}

// ── 11. ban_member / unban_member ──────────────────────────────

#[tokio::test]
async fn test_ban_and_unban_member() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomMemberStorage::new(&pool, "localhost");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user_id = format!("@mem_ban_{suffix}:localhost");
    let banned_by = format!("@admin_{suffix}:localhost");
    let room_id = format!("!room_ban_{suffix}:localhost");

    cleanup_membership_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_id).await;
    ensure_test_user(&pool, &banned_by).await;

    // Ban (works even without prior add_member)
    storage.ban_member(&room_id, &user_id, &banned_by).await.unwrap();

    let m = storage.get_member(&room_id, &user_id).await.unwrap().unwrap();
    assert_eq!(m.membership, "ban");
    assert_eq!(m.banned_by.as_deref(), Some(banned_by.as_str()));

    // Unban
    storage.unban_member(&room_id, &user_id).await.unwrap();

    let m = storage.get_member(&room_id, &user_id).await.unwrap().unwrap();
    assert_eq!(m.membership, "leave");
    assert!(m.banned_by.is_none());

    cleanup_membership_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_ban_member_idempotent() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomMemberStorage::new(&pool, "localhost");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user_id = format!("@mem_ban2_{suffix}:localhost");
    let admin = format!("@admin2_{suffix}:localhost");
    let room_id = format!("!room_ban2_{suffix}:localhost");

    cleanup_membership_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_id).await;
    ensure_test_user(&pool, &admin).await;

    storage.ban_member(&room_id, &user_id, &admin).await.unwrap();
    // Banning again should not fail
    let result = storage.ban_member(&room_id, &user_id, &admin).await;
    assert!(result.is_ok());

    cleanup_membership_data(&pool, &suffix).await;
}

// ── 12. get_joined_rooms ───────────────────────────────────────

#[tokio::test]
async fn test_get_joined_rooms_multiple() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomMemberStorage::new(&pool, "localhost");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user_id = format!("@mem_jrooms_{suffix}:localhost");
    let room_1 = format!("!room_jr1_{suffix}:localhost");
    let room_2 = format!("!room_jr2_{suffix}:localhost");

    cleanup_membership_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_1).await;
    ensure_test_room(&pool, &room_2).await;
    ensure_test_user(&pool, &user_id).await;

    storage.add_member(&room_1, &user_id, "join", None, None, None, None).await.unwrap();
    storage.add_member(&room_2, &user_id, "join", None, None, None, None).await.unwrap();

    let rooms = storage.get_joined_rooms(&user_id).await.unwrap();
    assert_eq!(rooms.len(), 2);
    assert!(rooms.contains(&room_1));
    assert!(rooms.contains(&room_2));

    cleanup_membership_data(&pool, &suffix).await;
}

#[tokio::test]
async fn test_get_joined_rooms_empty() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomMemberStorage::new(&pool, "localhost");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user_id = format!("@mem_nojr_{suffix}:localhost");

    cleanup_membership_data(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let rooms = storage.get_joined_rooms(&user_id).await.unwrap();
    assert!(rooms.is_empty());

    cleanup_membership_data(&pool, &suffix).await;
}

// ── 13. get_sync_rooms ─────────────────────────────────────────

#[tokio::test]
async fn test_get_sync_rooms() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomMemberStorage::new(&pool, "localhost");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user_id = format!("@mem_sync_{suffix}:localhost");
    let room_1 = format!("!room_sync1_{suffix}:localhost");
    let room_2 = format!("!room_sync2_{suffix}:localhost");

    cleanup_membership_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_1).await;
    ensure_test_room(&pool, &room_2).await;
    ensure_test_user(&pool, &user_id).await;

    storage.add_member(&room_1, &user_id, "join", None, None, None, None).await.unwrap();
    storage.add_member(&room_2, &user_id, "join", None, None, None, None).await.unwrap();
    // Leave room_2
    storage.remove_member(&room_2, &user_id, None).await.unwrap();

    // Without leave
    let rooms = storage.get_sync_rooms(&user_id, false).await.unwrap();
    assert_eq!(rooms.len(), 1);
    assert_eq!(rooms[0].membership, "join");

    // With leave
    let rooms = storage.get_sync_rooms(&user_id, true).await.unwrap();
    assert_eq!(rooms.len(), 2);
    let memberships: Vec<&str> = rooms.iter().map(|r| r.membership.as_str()).collect();
    assert!(memberships.contains(&"join"));
    assert!(memberships.contains(&"leave"));

    cleanup_membership_data(&pool, &suffix).await;
}

/// Every membership that has its own `/sync` section must be returned here.
///
/// `get_sync_rooms` is the **only** producer of the list `/sync` turns into
/// `rooms.join` / `rooms.invite` / `rooms.knock` / `rooms.leave`
/// (`sync_service::room_sections_from_memberships`), so a membership filtered
/// out here can never reach its section: an invited (or knocking) user sees
/// nothing at all, and the whole `invite_state` / `knock_state` rendering
/// pipeline in `sync_service::response` becomes unreachable.
#[tokio::test]
async fn test_get_sync_rooms_includes_invite_and_knock() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomMemberStorage::new(&pool, "localhost");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user_id = format!("@mem_sync_all_{suffix}:localhost");
    let room_join = format!("!room_sync_join_{suffix}:localhost");
    let room_invite = format!("!room_sync_invite_{suffix}:localhost");
    let room_knock = format!("!room_sync_knock_{suffix}:localhost");
    let room_leave = format!("!room_sync_leave_{suffix}:localhost");

    cleanup_membership_data(&pool, &suffix).await;
    for room in [&room_join, &room_invite, &room_knock, &room_leave] {
        ensure_test_room(&pool, room).await;
    }
    ensure_test_user(&pool, &user_id).await;

    storage.add_member(&room_join, &user_id, "join", None, None, None, None).await.unwrap();
    storage.add_member(&room_invite, &user_id, "invite", None, None, None, None).await.unwrap();
    storage.add_member(&room_knock, &user_id, "knock", None, None, None, None).await.unwrap();
    storage.add_member(&room_leave, &user_id, "leave", None, None, None, None).await.unwrap();

    // `include_leave = false` is the default `/sync` filter: join, invite and
    // knock all still have a section of their own.
    let rooms = storage.get_sync_rooms(&user_id, false).await.unwrap();
    let memberships: Vec<&str> = rooms.iter().map(|r| r.membership.as_str()).collect();
    for expected in ["join", "invite", "knock"] {
        assert!(memberships.contains(&expected), "`{expected}` must reach /sync, got {memberships:?}");
    }
    assert!(!memberships.contains(&"leave"), "leave is opt-in, got {memberships:?}");

    let rooms = storage.get_sync_rooms(&user_id, true).await.unwrap();
    let memberships: Vec<&str> = rooms.iter().map(|r| r.membership.as_str()).collect();
    for expected in ["join", "invite", "knock", "leave"] {
        assert!(memberships.contains(&expected), "`{expected}` must be present, got {memberships:?}");
    }

    cleanup_membership_data(&pool, &suffix).await;
}

// ── 14. get_membership_state ───────────────────────────────────

#[tokio::test]
async fn test_get_membership_state() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomMemberStorage::new(&pool, "localhost");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user_id = format!("@mem_state_{suffix}:localhost");
    let room_id = format!("!room_state_{suffix}:localhost");

    cleanup_membership_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_id).await;

    // No membership
    let state = storage.get_membership_state(&room_id, &user_id).await.unwrap();
    assert!(state.is_none());

    // Join
    storage.add_member(&room_id, &user_id, "join", None, None, None, None).await.unwrap();
    let state = storage.get_membership_state(&room_id, &user_id).await.unwrap();
    assert_eq!(state.as_deref(), Some("join"));

    // Leave
    storage.remove_member(&room_id, &user_id, None).await.unwrap();
    let state = storage.get_membership_state(&room_id, &user_id).await.unwrap();
    assert_eq!(state.as_deref(), Some("leave"));

    cleanup_membership_data(&pool, &suffix).await;
}

// ── 15. get_joined_room_count ──────────────────────────────────

#[tokio::test]
async fn test_get_joined_room_count() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomMemberStorage::new(&pool, "localhost");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user_id = format!("@mem_jrcount_{suffix}:localhost");
    let room_a = format!("!room_jrc_a_{suffix}:localhost");
    let room_b = format!("!room_jrc_b_{suffix}:localhost");

    cleanup_membership_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_a).await;
    ensure_test_room(&pool, &room_b).await;
    ensure_test_user(&pool, &user_id).await;

    // Zero
    let count = storage.get_joined_room_count(&user_id).await.unwrap();
    assert_eq!(count, 0);

    // With rooms
    storage.add_member(&room_a, &user_id, "join", None, None, None, None).await.unwrap();
    storage.add_member(&room_b, &user_id, "join", None, None, None, None).await.unwrap();
    let count = storage.get_joined_room_count(&user_id).await.unwrap();
    assert_eq!(count, 2);

    cleanup_membership_data(&pool, &suffix).await;
}

// ── 16. is_member ──────────────────────────────────────────────

#[tokio::test]
async fn test_is_member() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomMemberStorage::new(&pool, "localhost");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user_id = format!("@mem_ismem_{suffix}:localhost");
    let room_id = format!("!room_ismem_{suffix}:localhost");

    cleanup_membership_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_id).await;

    // Not a member yet
    assert!(!storage.is_member(&room_id, &user_id).await.unwrap());

    // Join
    storage.add_member(&room_id, &user_id, "join", None, None, None, None).await.unwrap();
    assert!(storage.is_member(&room_id, &user_id).await.unwrap());

    // Leave
    storage.remove_member(&room_id, &user_id, None).await.unwrap();
    assert!(!storage.is_member(&room_id, &user_id).await.unwrap());

    cleanup_membership_data(&pool, &suffix).await;
}

// ── 17. get_room_member ────────────────────────────────────────

#[tokio::test]
async fn test_get_room_member() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomMemberStorage::new(&pool, "localhost");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user_id = format!("@mem_rmem_{suffix}:localhost");
    let room_id = format!("!room_rmem_{suffix}:localhost");

    cleanup_membership_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_id).await;

    // Not found
    let m = storage.get_room_member(&room_id, &user_id).await.unwrap();
    assert!(m.is_none());

    // Found
    storage.add_member(&room_id, &user_id, "join", None, None, None, None).await.unwrap();
    let m = storage.get_room_member(&room_id, &user_id).await.unwrap();
    assert!(m.is_some());
    assert_eq!(m.unwrap().membership, "join");

    cleanup_membership_data(&pool, &suffix).await;
}

// ── 18. get_joined_members ─────────────────────────────────────

#[tokio::test]
async fn test_get_joined_members() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomMemberStorage::new(&pool, "localhost");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user_a = format!("@mem_jm_a_{suffix}:localhost");
    let user_b = format!("@mem_jm_b_{suffix}:localhost");
    let room_id = format!("!room_jm_{suffix}:localhost");

    cleanup_membership_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_a).await;
    ensure_test_user(&pool, &user_b).await;

    // Join only
    storage.add_member(&room_id, &user_a, "join", None, None, None, None).await.unwrap();
    // Invite (should NOT be in joined members)
    storage.add_member(&room_id, &user_b, "invite", None, None, None, None).await.unwrap();

    let joined = storage.get_joined_members(&room_id).await.unwrap();
    assert_eq!(joined.len(), 1);
    assert_eq!(joined[0].user_id, user_a);

    cleanup_membership_data(&pool, &suffix).await;
}

// ── 19. get_joined_member ──────────────────────────────────────

#[tokio::test]
async fn test_get_joined_member() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomMemberStorage::new(&pool, "localhost");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user_id = format!("@mem_jm_one_{suffix}:localhost");
    let room_id = format!("!room_jm_one_{suffix}:localhost");

    cleanup_membership_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_id).await;

    storage.add_member(&room_id, &user_id, "join", None, None, None, None).await.unwrap();

    // Found for join
    let m = storage.get_joined_member(&room_id, &user_id).await.unwrap();
    assert!(m.is_some());
    assert_eq!(m.unwrap().membership, "join");

    // Leave the room
    storage.remove_member(&room_id, &user_id, None).await.unwrap();

    // Not found for leave
    let m = storage.get_joined_member(&room_id, &user_id).await.unwrap();
    assert!(m.is_none());

    cleanup_membership_data(&pool, &suffix).await;
}

// ── 20. share_common_room ──────────────────────────────────────

#[tokio::test]
async fn test_share_common_room() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomMemberStorage::new(&pool, "localhost");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user_a = format!("@mem_scr_a_{suffix}:localhost");
    let user_b = format!("@mem_scr_b_{suffix}:localhost");
    let user_c = format!("@mem_scr_c_{suffix}:localhost");
    let room_id = format!("!room_scr_{suffix}:localhost");

    cleanup_membership_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_a).await;
    ensure_test_user(&pool, &user_b).await;
    ensure_test_user(&pool, &user_c).await;

    storage.add_member(&room_id, &user_a, "join", None, None, None, None).await.unwrap();
    storage.add_member(&room_id, &user_b, "join", None, None, None, None).await.unwrap();

    // Shares common room
    assert!(storage.share_common_room(&user_a, &user_b).await.unwrap());

    // Does not share (user_c not in any room)
    assert!(!storage.share_common_room(&user_a, &user_c).await.unwrap());

    cleanup_membership_data(&pool, &suffix).await;
}

// ── 21. get_membership_history ─────────────────────────────────

#[tokio::test]
async fn test_get_membership_history() {
    let (_isolated, pool) = test_pool().await;
    let storage = RoomMemberStorage::new(&pool, "localhost");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user_a = format!("@mem_hist_a_{suffix}:localhost");
    let user_b = format!("@mem_hist_b_{suffix}:localhost");
    let room_id = format!("!room_hist_{suffix}:localhost");

    cleanup_membership_data(&pool, &suffix).await;
    ensure_test_room(&pool, &room_id).await;
    ensure_test_user(&pool, &user_a).await;
    ensure_test_user(&pool, &user_b).await;

    storage.add_member(&room_id, &user_a, "join", None, None, None, None).await.unwrap();
    storage.add_member(&room_id, &user_b, "invite", None, None, None, None).await.unwrap();

    let history = storage.get_membership_history(&room_id, 10).await.unwrap();
    assert_eq!(history.len(), 2);
    // Ordered by updated_ts DESC — most recent first
    assert_eq!(history[0].user_id, user_b);
    assert_eq!(history[1].user_id, user_a);

    // Limit
    let limited = storage.get_membership_history(&room_id, 1).await.unwrap();
    assert_eq!(limited.len(), 1);
    assert_eq!(limited[0].user_id, user_b);

    cleanup_membership_data(&pool, &suffix).await;
}
