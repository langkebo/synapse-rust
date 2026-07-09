#[cfg(test)]
mod cursor_tests {
    use super::*;

    #[test]
    fn room_token_sync_cursor_round_trip() {
        let cursor = RoomTokenSyncCursor {
            room_updated_ts: 1_700_000_000_000,
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE".to_string(),
            conn_id: Some("main|conn".to_string()),
        };

        let encoded = encode_room_token_sync_cursor(&cursor);
        assert_eq!(decode_room_token_sync_cursor(Some(&encoded)), Some(cursor));
    }

    #[test]
    fn room_token_sync_cursor_rejects_invalid_values() {
        assert_eq!(decode_room_token_sync_cursor(Some("bad")), None);
        assert_eq!(decode_room_token_sync_cursor(Some("123|||")), None);
    }

    #[test]
    fn room_token_sync_cursor_round_trip_no_conn_id() {
        let cursor = RoomTokenSyncCursor {
            room_updated_ts: 1_700_000_000_000,
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE".to_string(),
            conn_id: None,
        };

        let encoded = encode_room_token_sync_cursor(&cursor);
        assert_eq!(decode_room_token_sync_cursor(Some(&encoded)), Some(cursor));
    }

    #[test]
    fn room_token_sync_cursor_round_trip_empty_conn_id() {
        let cursor = RoomTokenSyncCursor {
            room_updated_ts: 1_700_000_000_000,
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE".to_string(),
            conn_id: Some(String::new()),
        };

        let encoded = encode_room_token_sync_cursor(&cursor);
        assert_eq!(decode_room_token_sync_cursor(Some(&encoded)), Some(cursor));
    }

    #[test]
    fn room_token_sync_cursor_rejects_empty_user_id() {
        let cursor = RoomTokenSyncCursor {
            room_updated_ts: 1_700_000_000_000,
            user_id: String::new(),
            device_id: "DEVICE".to_string(),
            conn_id: None,
        };

        let encoded = encode_room_token_sync_cursor(&cursor);
        assert_eq!(decode_room_token_sync_cursor(Some(&encoded)), None);
    }

    #[test]
    fn room_token_sync_cursor_rejects_empty_device_id() {
        let cursor = RoomTokenSyncCursor {
            room_updated_ts: 1_700_000_000_000,
            user_id: "@alice:example.com".to_string(),
            device_id: String::new(),
            conn_id: None,
        };

        let encoded = encode_room_token_sync_cursor(&cursor);
        assert_eq!(decode_room_token_sync_cursor(Some(&encoded)), None);
    }

    #[test]
    fn room_token_sync_cursor_rejects_extra_parts() {
        // 6 pipe-separated segments, the 6th triggers the parts.next().is_some() guard
        let encoded = "123|dXNlcg==|ZGV2|0||extra_part";
        assert_eq!(decode_room_token_sync_cursor(Some(encoded)), None);
    }

    #[test]
    fn room_token_sync_cursor_none_input() {
        assert_eq!(decode_room_token_sync_cursor(None), None);
    }

    #[test]
    fn room_token_sync_cursor_rejects_invalid_base64() {
        assert_eq!(decode_room_token_sync_cursor(Some("123|!!!invalid!!!|ZGV2|0|")), None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sliding_sync_token_struct() {
        let token = SlidingSyncToken {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE123".to_string(),
            conn_id: Some("conn456".to_string()),
            token: "test_token".to_string(),
            pos: 100,
            created_ts: 1234567890000,
            expires_at: Some(1235172690000),
        };

        assert_eq!(token.user_id, "@alice:example.com");
        assert_eq!(token.pos, 100);
        assert!(token.conn_id.is_some());
    }

    #[test]
    fn test_sliding_sync_filters_default() {
        let filters = SlidingSyncFilters::default();
        assert!(filters.is_dm.is_none());
        assert!(filters.is_encrypted.is_none());
    }

    #[test]
    fn test_sliding_sync_filters_with_values() {
        let filters = SlidingSyncFilters {
            is_dm: Some(true),
            is_encrypted: Some(true),
            is_invite: Some(false),
            room_name_like: Some("test".to_string()),
            ..Default::default()
        };

        assert_eq!(filters.is_dm, Some(true));
        assert_eq!(filters.room_name_like, Some("test".to_string()));
    }

    #[test]
    fn test_sliding_sync_request() {
        let mut lists = std::collections::HashMap::new();
        lists.insert(
            "main".to_string(),
            SlidingSyncListData {
                ranges: vec![vec![0, 20]],
                sort: vec!["by_recency".to_string()],
                filters: None,
                timeline_limit: Some(100),
                required_state: None,
                slow_by: None,
                bump_event_types: None,
            },
        );

        let request = SlidingSyncRequest {
            conn_id: Some("test_conn".to_string()),
            lists,
            room_subscriptions: None,
            unsubscribe_rooms: None,
            extensions: None,
            pos: None,
            timeout: Some(30000),
            client_timeout: None,
        };

        assert!(request.conn_id.is_some());
        assert_eq!(request.lists.len(), 1);
    }

    #[test]
    fn test_sliding_sync_response() {
        let response = SlidingSyncResponse {
            pos: "12345".to_string(),
            conn_id: Some("conn123".to_string()),
            lists: serde_json::json!({}),
            rooms: serde_json::json!({}),
            extensions: None,
        };

        assert_eq!(response.pos, "12345");
    }

    #[test]
    fn test_sliding_sync_room_struct() {
        let room = SlidingSyncRoom {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE123".to_string(),
            room_id: "!room:example.com".to_string(),
            conn_id: None,
            list_key: Some("main".to_string()),
            bump_stamp: 1234567890000,
            highlight_count: 5,
            notification_count: 10,
            is_dm: true,
            is_encrypted: true,
            is_tombstoned: false,
            is_invited: false,
            name: Some("Test Room".to_string()),
            avatar: None,
            timestamp: 1234567890000,
            created_ts: 1234567890000,
            updated_ts: 1234567890000,
        };

        assert_eq!(room.highlight_count, 5);
        assert!(room.is_dm);
    }

    // ── push_room_filters tests ──

    /// Helper: build SQL from a base query and optional filters, returning the SQL string.
    /// push_room_filters is an associated function (no self) so no storage instance is needed.
    fn build_filtered_sql(filters: Option<&SlidingSyncFilters>) -> String {
        let mut query = QueryBuilder::<Postgres>::new("SELECT * FROM t WHERE 1=1");
        SlidingSyncStorage::push_room_filters(&mut query, filters);
        query.sql().to_string()
    }

    #[test]
    fn test_push_room_filters_none() {
        let sql = build_filtered_sql(None);
        assert_eq!(sql, "SELECT * FROM t WHERE 1=1");
    }

    #[test]
    fn test_push_room_filters_default_empty() {
        let sql = build_filtered_sql(Some(&SlidingSyncFilters::default()));
        assert_eq!(sql, "SELECT * FROM t WHERE 1=1");
    }

    #[test]
    fn test_push_room_filters_is_dm_true() {
        let filters = SlidingSyncFilters { is_dm: Some(true), ..Default::default() };
        let sql = build_filtered_sql(Some(&filters));
        assert!(sql.contains("AND is_dm = $1"), "expected is_dm filter in SQL: {sql}");
    }

    #[test]
    fn test_push_room_filters_is_dm_false() {
        let filters = SlidingSyncFilters { is_dm: Some(false), ..Default::default() };
        let sql = build_filtered_sql(Some(&filters));
        assert!(sql.contains("AND is_dm = $1"), "expected is_dm filter in SQL: {sql}");
    }

    #[test]
    fn test_push_room_filters_is_encrypted() {
        let filters = SlidingSyncFilters { is_encrypted: Some(true), ..Default::default() };
        let sql = build_filtered_sql(Some(&filters));
        assert!(sql.contains("AND is_encrypted = $1"), "expected is_encrypted filter in SQL: {sql}");
    }

    #[test]
    fn test_push_room_filters_is_invite() {
        let filters = SlidingSyncFilters { is_invite: Some(true), ..Default::default() };
        let sql = build_filtered_sql(Some(&filters));
        assert!(sql.contains("AND invited = $1"), "expected invited filter in SQL: {sql}");
    }

    #[test]
    fn test_push_room_filters_is_tombstoned() {
        let filters = SlidingSyncFilters { is_tombstoned: Some(true), ..Default::default() };
        let sql = build_filtered_sql(Some(&filters));
        assert!(sql.contains("AND is_tombstoned = $1"), "expected is_tombstoned filter in SQL: {sql}");
    }

    #[test]
    fn test_push_room_filters_room_name_like() {
        let filters = SlidingSyncFilters { room_name_like: Some("test".to_string()), ..Default::default() };
        let sql = build_filtered_sql(Some(&filters));
        assert!(sql.contains("AND COALESCE(name, '') ILIKE $1"), "expected room_name_like filter in SQL: {sql}");
    }

    #[test]
    fn test_push_room_filters_all_combined() {
        let filters = SlidingSyncFilters {
            is_dm: Some(true),
            is_encrypted: Some(true),
            is_invite: Some(false),
            is_tombstoned: Some(false),
            room_name_like: Some("chat".to_string()),
            ..Default::default()
        };
        let sql = build_filtered_sql(Some(&filters));
        assert!(sql.starts_with("SELECT * FROM t WHERE 1=1"), "expected base query preserved: {sql}");
        assert!(sql.contains("AND is_dm = $1"), "missing is_dm: {sql}");
        assert!(sql.contains("AND is_encrypted = $2"), "missing is_encrypted: {sql}");
        assert!(sql.contains("AND invited = $3"), "missing invited: {sql}");
        assert!(sql.contains("AND is_tombstoned = $4"), "missing is_tombstoned: {sql}");
        assert!(sql.contains("AND COALESCE(name, '') ILIKE $5"), "missing room_name_like: {sql}");
    }

    #[test]
    fn test_push_room_filters_partial() {
        // Only set is_dm and room_name_like; all others remain None → not pushed
        let filters =
            SlidingSyncFilters { is_dm: Some(true), room_name_like: Some("office".to_string()), ..Default::default() };
        let sql = build_filtered_sql(Some(&filters));
        assert!(sql.contains("AND is_dm = $1"), "missing is_dm: {sql}");
        assert!(sql.contains("AND COALESCE(name, '') ILIKE $2"), "missing room_name_like: {sql}");
        assert!(!sql.contains("is_encrypted"), "unexpected is_encrypted: {sql}");
        assert!(!sql.contains("invited"), "unexpected invited: {sql}");
        assert!(!sql.contains("is_tombstoned"), "unexpected is_tombstoned: {sql}");
    }
}
