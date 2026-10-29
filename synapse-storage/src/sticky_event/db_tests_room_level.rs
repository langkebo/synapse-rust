// UG-7 regression coverage: federation-origin (room-level) sticky pins.
//
// Split out of `db_tests.rs` so both files stay under the 600-line ratchet
// threshold; the module path (`sticky_event::db_tests_room_level`) is new but
// the cases it holds used to live in `sticky_event::db_tests`.

use super::db_tests::{ensure_test_room, ensure_test_user, test_pool};
use super::*;

/// UG-7: federation writes a sticky with the **origin** as the owner key — a bare
/// server name, not an `@user:server` id — and those are room-level rows that every
/// member of the room must see. Two defects used to hide them: the user FK rejected
/// the bare origin on write, and the read paths filtered on `user_id = 本人`.
#[tokio::test]
async fn test_room_level_pin_is_visible_to_every_member() {
    let (_isolated, pool) = test_pool().await;
    let storage = StickyEventStorage::new(pool.clone());
    let suffix = uuid::Uuid::new_v4();
    let room_id = &format!("!room_lvl_{suffix}:test.com");
    let other_room = &format!("!room_lvl_other_{suffix}:test.com");
    let user_id = &format!("@user_lvl_{suffix}:test.com");
    let origin_a = "remote-a.example";
    let origin_b = "remote-b.example";

    ensure_test_user(&pool, user_id).await;
    ensure_test_room(&pool, room_id).await;
    ensure_test_room(&pool, other_room).await;
    sqlx::query("DELETE FROM room_sticky_events WHERE room_id = ANY($1::text[])")
        .bind(vec![room_id.to_string(), other_room.to_string()])
        .execute(&*pool)
        .await
        .expect("failed to cleanup sticky events");

    // 两条房级行：不同 origin、不同 event_type。
    storage
        .set_is_sticky_event(room_id, origin_a, &format!("$lvl_a_{suffix}:test.com"), "m.room.message", true, None)
        .await
        .expect("room-level row must be accepted without a users row (no user FK)");
    storage
        .set_is_sticky_event(room_id, origin_b, &format!("$lvl_b_{suffix}:test.com"), "m.room.topic", true, None)
        .await
        .expect("second origin's room-level row must be accepted");

    // 从未写过任何 pin 的本地成员，也能读到这两条房级置顶。
    let visible = storage.get_all_is_sticky_events(room_id, user_id).await.expect("bulk read should succeed");
    assert_eq!(visible.len(), 2, "both room-level pins must be visible to a member with no pins of their own");
    let mut types: Vec<&str> = visible.iter().map(|e| e.event_type.as_str()).collect();
    types.sort_unstable();
    assert_eq!(types, vec!["m.room.message", "m.room.topic"]);
    assert!(visible.iter().all(|e| e.user_id != *user_id), "these rows belong to origins, not to the reader");

    assert!(
        storage
            .get_is_sticky_event(room_id, user_id, "m.room.topic")
            .await
            .expect("single read should succeed")
            .is_some(),
        "the single read must see room-level rows too"
    );

    // 本人行覆盖同 event_type 的房级行，且 `DISTINCT ON` 保证每类只出一条。
    let own_event = format!("$lvl_own_{suffix}:test.com");
    storage
        .set_is_sticky_event(room_id, user_id, &own_event, "m.room.message", true, None)
        .await
        .expect("setting the user's own pin should succeed");

    let single = storage
        .get_is_sticky_event(room_id, user_id, "m.room.message")
        .await
        .expect("single read should succeed")
        .expect("the merged pin should be present");
    assert_eq!(single.event_id, own_event, "the user's own row must outrank the room-level row");

    let merged = storage.get_all_is_sticky_events(room_id, user_id).await.expect("bulk read should succeed");
    assert_eq!(merged.len(), 2, "DISTINCT ON (event_type) must keep one row per type");
    let messages: Vec<&StickyEvent> = merged.iter().filter(|e| e.event_type == "m.room.message").collect();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].event_id, own_event);

    // `/sync` 的预取同样只看 `room_ids` 范围内的房间。
    assert_eq!(
        storage
            .get_rooms_with_is_sticky_events(&[room_id.to_string()], user_id)
            .await
            .expect("rooms read should succeed"),
        vec![room_id.to_string()],
        "a room-level pin must make the room sticky for members"
    );
    assert!(
        storage
            .get_rooms_with_is_sticky_events(&[other_room.to_string()], user_id)
            .await
            .expect("rooms read should succeed")
            .is_empty(),
        "prefetch must be scoped to the caller's room_ids"
    );

    sqlx::query("DELETE FROM room_sticky_events WHERE room_id = $1")
        .bind(room_id)
        .execute(&*pool)
        .await
        .expect("failed to cleanup sticky events");
}
