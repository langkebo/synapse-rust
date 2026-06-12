pub use synapse_services::thread_service::*;

#[cfg(test)]
mod tests {
    #[test]
    fn test_create_thread_request() {
        let request = super::CreateThreadRequest {
            room_id: "!room:example.com".to_string(),
            root_event_id: "$event:example.com".to_string(),
        };
        assert_eq!(request.room_id, "!room:example.com");
        assert_eq!(request.root_event_id, "$event:example.com");
    }

    #[test]
    fn test_create_reply_request() {
        let request = super::CreateReplyRequest {
            room_id: "!room:example.com".to_string(),
            thread_id: "$thread:example.com".to_string(),
            event_id: "$reply:example.com".to_string(),
            root_event_id: "$root:example.com".to_string(),
            content: serde_json::json!({"msgtype": "m.text", "body": "Reply"}),
            in_reply_to_event_id: Some("$prev:example.com".to_string()),
            origin_server_ts: 1234567890,
        };
        assert_eq!(request.thread_id, "$thread:example.com");
        assert!(request.in_reply_to_event_id.is_some());
    }

    #[test]
    fn test_get_thread_request() {
        let request = super::GetThreadRequest {
            room_id: "!room:example.com".to_string(),
            thread_id: "$thread:example.com".to_string(),
            include_replies: true,
            reply_limit: Some(100),
        };
        assert!(request.include_replies);
        assert_eq!(request.reply_limit, Some(100));
    }

    #[test]
    fn test_list_threads_request() {
        let request = super::ListThreadsRequest {
            room_id: "!room:example.com".to_string(),
            limit: Some(50),
            from: Some("batch_token".to_string()),
            include_all: false,
        };
        assert_eq!(request.limit, Some(50));
        assert!(!request.include_all);
    }

    #[test]
    fn test_subscribe_request() {
        let request = super::SubscribeRequest {
            room_id: "!room:example.com".to_string(),
            thread_id: "$thread:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            notification_level: "all".to_string(),
        };
        assert_eq!(request.notification_level, "all");
    }

    #[test]
    fn test_mark_read_request() {
        let request = super::MarkReadRequest {
            room_id: "!room:example.com".to_string(),
            thread_id: "$thread:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            event_id: "$event:example.com".to_string(),
            origin_server_ts: 1234567890,
        };
        assert_eq!(request.user_id, "@user:example.com");
    }

    #[test]
    fn test_thread_list_response_serialization() {
        let response = super::ThreadListResponse { threads: vec![], next_batch: Some("token".to_string()), total: 0 };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("threads"));
        assert!(json.contains("total"));
    }

    #[test]
    fn test_unread_threads_response() {
        let response = super::UnreadThreadsResponse { threads: vec![], total_unread: 5, total_threads: 10 };
        assert_eq!(response.total_unread, 5);
        assert_eq!(response.total_threads, 10);
    }

    #[test]
    fn test_notification_levels() {
        let valid_levels = ["all", "mentions", "none"];
        for level in valid_levels {
            assert!(matches!(level, "all" | "mentions" | "none"));
        }
    }

    #[test]
    fn test_thread_root_structure() {
        let root = crate::storage::thread::ThreadRoot {
            id: 1,
            room_id: "!room:example.com".to_string(),
            thread_id: Some("$thread:example.com".to_string()),
            root_event_id: "$root:example.com".to_string(),
            sender: "@user:example.com".to_string(),
            reply_count: 5,
            last_reply_event_id: Some("$last:example.com".to_string()),
            last_reply_sender: Some("@replier:example.com".to_string()),
            last_reply_ts: Some(1234567890),
            participants: Some(serde_json::json!(["@user:example.com"])),
            is_fetched: false,
            created_ts: 1234567800,
            updated_ts: Some(1234567890),
        };
        assert_eq!(root.reply_count, 5);
        assert!(!root.is_fetched);
    }

    #[test]
    fn test_thread_reply_structure() {
        let reply = crate::storage::thread::ThreadReply {
            id: 1,
            room_id: "!room:example.com".to_string(),
            thread_id: "$thread:example.com".to_string(),
            event_id: "$reply:example.com".to_string(),
            root_event_id: "$root:example.com".to_string(),
            sender: "@user:example.com".to_string(),
            in_reply_to_event_id: Some("$prev:example.com".to_string()),
            content: serde_json::json!({"msgtype": "m.text"}),
            origin_server_ts: 1234567890,
            is_redacted: false,
            is_edited: false,
            created_ts: 1234567890,
        };
        assert!(!reply.is_redacted);
        assert!(!reply.is_edited);
    }

    #[test]
    fn test_thread_subscription_structure() {
        let subscription = crate::storage::thread::ThreadSubscription {
            id: 1,
            room_id: "!room:example.com".to_string(),
            thread_id: "$thread:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            notification_level: "all".to_string(),
            is_muted: false,
            is_pinned: false,
            subscribed_ts: 1234567890,
            updated_ts: 1234567890,
        };
        assert_eq!(subscription.notification_level, "all");
        assert!(!subscription.is_muted);
    }

    #[test]
    fn test_thread_read_receipt_structure() {
        let receipt = crate::storage::thread::ThreadReadReceipt {
            id: 1,
            room_id: "!room:example.com".to_string(),
            thread_id: "$thread:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            last_read_event_id: Some("$event:example.com".to_string()),
            last_read_ts: 1234567890,
            unread_count: 3,
            updated_ts: 1234567890,
        };
        assert_eq!(receipt.unread_count, 3);
    }
}
