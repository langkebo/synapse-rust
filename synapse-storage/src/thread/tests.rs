#![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    fn create_test_thread_root() -> ThreadRoot {
        ThreadRoot {
            id: 1,
            room_id: "!test:example.com".to_string(),
            root_event_id: "$event1".to_string(),
            sender: "@user:example.com".to_string(),
            thread_id: Some("thread-001".to_string()),
            reply_count: Some(0),
            last_reply_event_id: None,
            last_reply_sender: None,
            last_reply_ts: None,
            participants: Some(serde_json::json!(["@user:example.com"])),
            is_fetched: false,
            created_ts: 1234567890,
            updated_ts: None,
        }
    }

    fn create_test_thread_reply() -> ThreadReply {
        ThreadReply {
            id: 1,
            room_id: "!test:example.com".to_string(),
            thread_id: "thread-001".to_string(),
            event_id: "$reply1".to_string(),
            root_event_id: "$event1".to_string(),
            sender: "@user2:example.com".to_string(),
            in_reply_to_event_id: Some("$event1".to_string()),
            content: serde_json::json!({"body": "Reply"}),
            origin_server_ts: 1234567891,
            is_edited: false,
            is_redacted: false,
            created_ts: 1234567891,
        }
    }

    fn create_test_thread_subscription() -> ThreadSubscription {
        ThreadSubscription {
            id: 1,
            room_id: "!test:example.com".to_string(),
            thread_id: "thread-001".to_string(),
            user_id: "@user:example.com".to_string(),
            notification_level: "all".to_string(),
            is_muted: false,
            is_pinned: false,
            subscribed_ts: 1234567890,
            updated_ts: 1234567890,
        }
    }

    #[test]
    fn test_thread_root_creation() {
        let thread = create_test_thread_root();
        assert_eq!(thread.id, 1);
        assert_eq!(thread.room_id, "!test:example.com");
        assert_eq!(thread.sender, "@user:example.com");
        assert_eq!(thread.reply_count, Some(0));
        assert!(!thread.is_fetched);
    }

    #[test]
    fn test_thread_reply_creation() {
        let reply = create_test_thread_reply();
        assert_eq!(reply.thread_id, "thread-001");
        assert_eq!(reply.sender, "@user2:example.com");
        assert!(reply.in_reply_to_event_id.is_some());
        assert!(!reply.is_edited);
        assert!(!reply.is_redacted);
    }

    #[test]
    fn test_thread_subscription_creation() {
        let subscription = create_test_thread_subscription();
        assert_eq!(subscription.notification_level, "all");
        assert!(!subscription.is_muted);
    }

    #[test]
    fn test_thread_list_params_defaults() {
        let params =
            ThreadListParams { room_id: "!test:example.com".to_string(), from: None, limit: None, include_all: false };
        assert_eq!(params.room_id, "!test:example.com");
        assert!(params.from.is_none());
        assert!(params.limit.is_none());
        assert!(!params.include_all);
    }

    #[test]
    fn test_create_thread_root_params() {
        let params = CreateThreadRootParams {
            room_id: "!test:example.com".to_string(),
            root_event_id: "$event1".to_string(),
            sender: "@user:example.com".to_string(),
            thread_id: Some("thread-001".to_string()),
        };
        assert_eq!(params.room_id, "!test:example.com");
        assert_eq!(params.root_event_id, "$event1");
        assert!(params.thread_id.is_some());
    }

    #[test]
    fn test_create_thread_reply_params() {
        let params = CreateThreadReplyParams {
            room_id: "!test:example.com".to_string(),
            thread_id: "thread-001".to_string(),
            event_id: "$reply1".to_string(),
            root_event_id: "$event1".to_string(),
            sender: "@user2:example.com".to_string(),
            in_reply_to_event_id: Some("$event1".to_string()),
            content: serde_json::json!({"body": "Reply"}),
            origin_server_ts: 1234567891,
        };
        assert_eq!(params.thread_id, "thread-001");
        assert!(params.in_reply_to_event_id.is_some());
    }

    #[test]
    fn test_notification_level_values() {
        let levels = vec!["all", "mentions", "none"];
        for level in levels {
            let subscription = ThreadSubscription {
                id: 1,
                room_id: "!test:example.com".to_string(),
                thread_id: "thread-001".to_string(),
                user_id: "@user:example.com".to_string(),
                notification_level: level.to_string(),
                is_muted: false,
                is_pinned: false,
                subscribed_ts: 1234567890,
                updated_ts: 1234567890,
            };
            assert_eq!(subscription.notification_level, level);
        }
    }

    #[test]
    fn test_thread_fetched_status() {
        let mut thread = create_test_thread_root();
        assert!(!thread.is_fetched);

        thread.is_fetched = true;
        assert!(thread.is_fetched);
    }

    #[test]
    fn test_thread_reply_edit_status() {
        let mut reply = create_test_thread_reply();
        assert!(!reply.is_edited);

        reply.is_edited = true;
        assert!(reply.is_edited);
    }
