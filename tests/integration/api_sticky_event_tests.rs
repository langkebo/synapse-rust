#![cfg(test)]

mod sticky_event_integration_suite {
    use sqlx::PgPool;
    use synapse_common::current_timestamp_millis;
    use synapse_storage::sticky_event::{StickyEvent, StickyEventStorage};

    /// Insert prerequisite rooms rows so the `room_sticky_events.room_id` FK is
    /// satisfied, plus the `users` row for the local accounts these tests act as.
    /// The `room_sticky_events.user_id` FK was dropped in UG-7 (federation writes
    /// room-level rows keyed by a bare origin), so this helper no longer exists to
    /// satisfy that constraint. Uses ON CONFLICT DO NOTHING so repeated calls
    /// within the same schema are safe.
    async fn ensure_room_and_user(pool: &PgPool, room_id: &str, user_id: &str) {
        let now = current_timestamp_millis();
        sqlx::query("INSERT INTO rooms (room_id, room_version, creator, created_ts) VALUES ($1, '11', $2, $3) ON CONFLICT (room_id) DO NOTHING")
            .bind(room_id)
            .bind(user_id)
            .bind(now)
            .execute(pool)
            .await
            .expect("Failed to ensure room row");
        sqlx::query(
            "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(user_id.trim_start_matches('@'))
        .bind(now)
        .execute(pool)
        .await
        .expect("Failed to ensure user row");
    }

    #[tokio::test]
    async fn test_sticky_event_operations() {
        let pool = crate::require_test_pool().await;
        let storage = StickyEventStorage::new(pool.clone());
        let room_id = "!testroom:localhost";
        let user_id = "@testuser:localhost";
        let event_id = "$testevent:localhost";
        let event_type = "m.room.message";

        ensure_room_and_user(&pool, room_id, user_id).await;

        storage
            .set_is_sticky_event(room_id, user_id, event_id, event_type, true, None)
            .await
            .expect("Failed to set sticky event");

        let result =
            storage.get_is_sticky_event(room_id, user_id, event_type).await.expect("Failed to get sticky event");

        assert!(result.is_some());
        let sticky = result.unwrap();
        assert_eq!(sticky.room_id, room_id);
        assert_eq!(sticky.user_id, user_id);
        assert_eq!(sticky.event_id, event_id);
        assert_eq!(sticky.event_type, event_type);
        assert!(sticky.is_sticky);

        let all_events =
            storage.get_all_is_sticky_events(room_id, user_id).await.expect("Failed to get all sticky events");

        assert!(!all_events.is_empty());
        assert!(all_events.iter().any(|e| e.event_type == event_type));

        storage.clear_is_sticky_event(room_id, user_id, event_type).await.expect("Failed to clear sticky event");

        let result =
            storage.get_is_sticky_event(room_id, user_id, event_type).await.expect("Failed to get sticky event");

        assert!(result.is_none() || !result.unwrap().is_sticky);
    }

    #[tokio::test]
    async fn test_multiple_sticky_events() {
        let pool = crate::require_test_pool().await;
        let storage = StickyEventStorage::new(pool.clone());
        let room_id = "!testroom:localhost";
        let user_id = "@testuser:localhost";

        ensure_room_and_user(&pool, room_id, user_id).await;

        storage
            .set_is_sticky_event(room_id, user_id, "$event1:localhost", "m.room.message", true, None)
            .await
            .expect("Failed to set sticky event 1");

        storage
            .set_is_sticky_event(room_id, user_id, "$event2:localhost", "m.room.topic", true, None)
            .await
            .expect("Failed to set sticky event 2");

        storage
            .set_is_sticky_event(room_id, user_id, "$event3:localhost", "m.room.avatar", true, None)
            .await
            .expect("Failed to set sticky event 3");

        let all_events =
            storage.get_all_is_sticky_events(room_id, user_id).await.expect("Failed to get all sticky events");

        assert_eq!(all_events.len(), 3);

        let event_types: Vec<&str> = all_events.iter().map(|e| e.event_type.as_str()).collect();
        assert!(event_types.contains(&"m.room.message"));
        assert!(event_types.contains(&"m.room.topic"));
        assert!(event_types.contains(&"m.room.avatar"));
    }

    #[tokio::test]
    async fn test_get_rooms_with_sticky_events() {
        let pool = crate::require_test_pool().await;
        let storage = StickyEventStorage::new(pool.clone());
        let user_id = "@testuser:localhost";

        ensure_room_and_user(&pool, "!room1:localhost", user_id).await;
        ensure_room_and_user(&pool, "!room2:localhost", user_id).await;

        storage
            .set_is_sticky_event("!room1:localhost", user_id, "$event1:localhost", "m.room.message", true, None)
            .await
            .expect("Failed to set sticky event");

        storage
            .set_is_sticky_event("!room2:localhost", user_id, "$event2:localhost", "m.room.message", true, None)
            .await
            .expect("Failed to set sticky event");

        let rooms = storage
            .get_rooms_with_is_sticky_events(&["!room1:localhost".to_string(), "!room2:localhost".to_string()], user_id)
            .await
            .expect("Failed to get rooms with sticky events");

        assert!(rooms.len() >= 2);
        assert!(rooms.contains(&"!room1:localhost".to_string()));
        assert!(rooms.contains(&"!room2:localhost".to_string()));
    }

    #[tokio::test]
    async fn test_update_sticky_event() {
        let pool = crate::require_test_pool().await;
        let storage = StickyEventStorage::new(pool.clone());
        let room_id = "!testroom:localhost";
        let user_id = "@testuser:localhost";
        let event_type = "m.room.message";

        ensure_room_and_user(&pool, room_id, user_id).await;

        storage
            .set_is_sticky_event(room_id, user_id, "$event1:localhost", event_type, true, None)
            .await
            .expect("Failed to set initial sticky event");

        storage
            .set_is_sticky_event(room_id, user_id, "$event2:localhost", event_type, true, None)
            .await
            .expect("Failed to update sticky event");

        let result =
            storage.get_is_sticky_event(room_id, user_id, event_type).await.expect("Failed to get sticky event");

        assert!(result.is_some());
        assert_eq!(result.unwrap().event_id, "$event2:localhost");
    }

    /// C2（§19.5 批次 C）：`/sync` 的 `sticky_events` 集合与
    /// `GET /rooms/{room_id}/sticky_events` 的集合**同源** —— 两者都落到
    /// `StickyEventStorage::get_all_is_sticky_events(room_id, user_id)`（过期过滤也在该读取内）。
    /// 本用例锁定该"同源读"的两个后果：
    /// 1) 有 pin 的用户读到同一集合（同一 (room, user, type) 由唯一键保证单行）；
    /// 2) 本房间只有用户行、新成员自己没设 pin ⇒ 该成员读到空集，两路同时为空、彼此恒等。
    ///    房级行（`user_id` 为裸 origin）对全体成员可见是 UG-7 之后的另一条路径，见
    ///    `room_level_pins_are_visible_to_members_and_yield_to_own_pins`。
    #[tokio::test]
    async fn sticky_view_is_identical_for_sync_and_get_and_empty_for_new_members() {
        let pool = crate::require_test_pool().await;
        let storage = StickyEventStorage::new(pool.clone());
        let room_id = "!c2room:localhost";
        let pinned_user = "@c2pinned:localhost";
        let new_member = "@c2newbie:localhost";

        ensure_room_and_user(&pool, room_id, pinned_user).await;
        ensure_room_and_user(&pool, room_id, new_member).await;

        storage
            .set_is_sticky_event(room_id, pinned_user, "$s1:localhost", "m.room.message", true, None)
            .await
            .expect("Failed to set sticky event 1");
        storage
            .set_is_sticky_event(room_id, pinned_user, "$s2:localhost", "m.room.topic", true, None)
            .await
            .expect("Failed to set sticky event 2");

        // 有 pin 的用户：该读取即 `/sync` 与 `GET` 两路端点共用的集合。
        let pinned_view =
            storage.get_all_is_sticky_events(room_id, pinned_user).await.expect("Failed to get pinned view");
        let mut pinned_types: Vec<&str> = pinned_view.iter().map(|e| e.event_type.as_str()).collect();
        pinned_types.sort_unstable();
        assert_eq!(pinned_types, vec!["m.room.message", "m.room.topic"]);

        // 新成员：本房间没有房级行、他自己也没设 pin ⇒ 两路皆空、恒等。
        let newbie_view =
            storage.get_all_is_sticky_events(room_id, new_member).await.expect("Failed to get new member view");
        assert!(newbie_view.is_empty(), "无房级行且本人未设 pin 时，新成员的 sticky 集合必须为空");
    }

    /// UG-7：远端置顶以 **origin**（裸服务器名，非 `@user:server`）落库，是**房级行**，
    /// 房间内所有成员都应看到；同一 `event_type` 上本人行优先于房级行，且只出一条。
    /// 用例同时锁定 `get_rooms_with_is_sticky_events` 的 `room_ids` 作用域（`/sync` 预取）。
    #[tokio::test]
    async fn room_level_pins_are_visible_to_members_and_yield_to_own_pins() {
        let pool = crate::require_test_pool().await;
        let storage = StickyEventStorage::new(pool.clone());
        let room_id = "!ug7room:localhost";
        let other_room = "!ug7other:localhost";
        let member = "@ug7member:localhost";
        let origin = "remote.example";

        ensure_room_and_user(&pool, room_id, member).await;
        ensure_room_and_user(&pool, other_room, member).await;
        sqlx::query("DELETE FROM room_sticky_events WHERE room_id = ANY($1::text[])")
            .bind(vec![room_id.to_string(), other_room.to_string()])
            .execute(&*pool)
            .await
            .expect("Failed to clean sticky events");

        // 远端置顶：owner key 是裸 origin，不依赖 users 表（旧用户外键会在这里拒绝写入）。
        storage
            .set_is_sticky_event(room_id, origin, "$remote:remote.example", "m.room.message", true, None)
            .await
            .expect("room-level row must be accepted without a users row");

        let view = storage.get_all_is_sticky_events(room_id, member).await.expect("Failed to get member view");
        assert_eq!(view.len(), 1, "房级置顶必须对本地成员可见");
        assert_eq!(view[0].user_id, origin);

        // 本人对同一 event_type 设 pin ⇒ 覆盖房级行，且不会重复出现。
        storage
            .set_is_sticky_event(room_id, member, "$own:localhost", "m.room.message", true, None)
            .await
            .expect("Failed to set the member's own pin");

        let single = storage
            .get_is_sticky_event(room_id, member, "m.room.message")
            .await
            .expect("Failed to get sticky event")
            .expect("merged pin should exist");
        assert_eq!(single.event_id, "$own:localhost", "本人行必须优先于房级行");

        let merged = storage.get_all_is_sticky_events(room_id, member).await.expect("Failed to get member view");
        assert_eq!(merged.len(), 1, "同一 event_type 只允许出一条");
        assert_eq!(merged[0].event_id, "$own:localhost");

        // `/sync` 预取：只认 room_ids 范围内的房间，且房级行同样让房间入选。
        assert_eq!(
            storage
                .get_rooms_with_is_sticky_events(&[room_id.to_string()], member)
                .await
                .expect("Failed to get rooms with sticky events"),
            vec![room_id.to_string()],
            "房级置顶必须让房间出现在 sticky_rooms 里"
        );
        assert!(
            storage
                .get_rooms_with_is_sticky_events(&[other_room.to_string()], member)
                .await
                .expect("Failed to get rooms with sticky events")
                .is_empty(),
            "预取必须受 room_ids 限定"
        );

        sqlx::query("DELETE FROM room_sticky_events WHERE room_id = $1")
            .bind(room_id)
            .execute(&*pool)
            .await
            .expect("Failed to clean sticky events");
    }

    /// C3（§19.5 批次 C）：`room_sticky_events` 的唯一键
    /// `(room_id, user_id, event_type)` 保证同**一个 owner key** 对同一 sticky 事件恒为一行
    /// （`ON CONFLICT DO UPDATE`），因此反复写入不会堆出多行。
    /// 注意它约束的是 owner key 而不是"每个读者"，同一 `event_type` 仍可同时有本人行与房级行，
    /// 对读者只出一条由读路径的 `DISTINCT ON (event_type)` 负责（见 UG-7 用例）。
    /// 多写者/分片位置仍是**未做**的边界（见代码内注释）。
    #[tokio::test]
    async fn sticky_event_unique_key_keeps_one_row_per_room_user_type() {
        let pool = crate::require_test_pool().await;
        let storage = StickyEventStorage::new(pool.clone());
        let room_id = "!c3room:localhost";
        let user_id = "@c3user:localhost";
        let event_type = "m.room.message";

        ensure_room_and_user(&pool, room_id, user_id).await;

        // 同一 (room, user, type) 连续写两次（换 event_id）⇒ 唯一键走 DO UPDATE，仍只有一行。
        storage
            .set_is_sticky_event(room_id, user_id, "$first:localhost", event_type, true, None)
            .await
            .expect("Failed to set first sticky event");
        storage
            .set_is_sticky_event(room_id, user_id, "$second:localhost", event_type, true, None)
            .await
            .expect("Failed to update sticky event");

        let all = storage.get_all_is_sticky_events(room_id, user_id).await.expect("Failed to get all sticky events");
        let matching: Vec<&StickyEvent> = all.iter().filter(|e| e.event_type == event_type).collect();
        assert_eq!(matching.len(), 1, "唯一键必须保证同一 (room, user, type) 单行");
        assert_eq!(matching[0].event_id, "$second:localhost", "后写必须覆盖前写");
    }

    #[test]
    fn test_sticky_event_struct() {
        let event = StickyEvent {
            room_id: "!room:localhost".to_string(),
            user_id: "@user:localhost".to_string(),
            event_id: "$event:localhost".to_string(),
            event_type: "m.room.message".to_string(),
            is_sticky: true,
            sticky_duration_ms: None,
            expires_at: None,
            created_ts: 1700000000000i64,
            updated_ts: 1700000000000i64,
        };

        assert_eq!(event.room_id, "!room:localhost");
        assert_eq!(event.user_id, "@user:localhost");
        assert_eq!(event.event_id, "$event:localhost");
        assert_eq!(event.event_type, "m.room.message");
        assert!(event.is_sticky);
    }

    #[test]
    fn test_event_type_validation() {
        let valid_types =
            vec!["m.room.message", "m.room.topic", "m.room.avatar", "m.room.name", "m.room.member", "m.room.encrypted"];

        for event_type in valid_types {
            assert!(
                event_type.starts_with("m.") || event_type.starts_with("com."),
                "Event type should start with m. or com."
            );
        }
    }
}
