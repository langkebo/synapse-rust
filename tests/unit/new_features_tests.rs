#[cfg(test)]
mod push_api_tests {
    use serde_json::json;

    #[test]
    fn test_push_rule_serialization() {
        let rule = json!({
            "rule_id": ".m.rule.message",
            "default": true,
            "enabled": true,
            "conditions": [
                {
                    "kind": "event_match",
                    "key": "type",
                    "pattern": "m.room.message"
                }
            ],
            "actions": [
                "notify",
                {
                    "set_tweak": "sound",
                    "value": "default"
                }
            ]
        });

        assert!(rule.get("rule_id").is_some());
        assert!(rule.get("default").and_then(|v| v.as_bool()).unwrap_or(false));
        assert!(rule.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false));
    }

    #[test]
    fn test_pusher_request_validation() {
        let pusher_request = json!({
            "pushkey": "unique_push_key_123",
            "kind": "http",
            "app_id": "com.example.app",
            "app_display_name": "Example App",
            "device_display_name": "My Device",
            "lang": "en",
            "data": {
                "url": "https://push.example.com/notify"
            }
        });

        assert!(pusher_request.get("pushkey").is_some());
        assert!(pusher_request.get("kind").is_some());
        assert!(pusher_request.get("app_id").is_some());
    }

    #[test]
    fn test_notification_counts() {
        let counts = json!({
            "unread": 5,
            "missed_calls": 2
        });

        assert_eq!(counts.get("unread").and_then(|v| v.as_u64()), Some(5));
        assert_eq!(counts.get("missed_calls").and_then(|v| v.as_u64()), Some(2));
    }
}

#[cfg(test)]
mod account_data_tests {
    use serde_json::json;

    #[test]
    fn test_account_data_types() {
        let direct_account_data = json!({
            "type": "m.direct",
            "content": {
                "@user1:example.com": ["!room1:example.com"],
                "@user2:example.com": ["!room2:example.com", "!room3:example.com"]
            }
        });

        assert!(direct_account_data.get("content").is_some());
    }

    #[test]
    fn test_room_account_data() {
        let read_markers = json!({
            "type": "m.fully_read",
            "content": {
                "event_id": "$event123:example.com"
            }
        });

        assert!(read_markers.get("content").is_some());
    }

    #[test]
    fn test_filter_definition() {
        let filter = json!({
            "room": {
                "state": {
                    "types": ["m.room.*"],
                    "not_types": ["m.room.member"]
                },
                "timeline": {
                    "limit": 50,
                    "types": ["m.room.message"]
                },
                "ephemeral": {
                    "types": ["m.receipt", "m.typing"]
                }
            },
            "presence": {
                "types": ["m.presence"]
            },
            "account_data": {
                "types": ["m.*"]
            }
        });

        assert!(filter.get("room").is_some());
        assert!(filter.get("presence").is_some());
    }

    #[test]
    fn test_openid_token_response() {
        let openid_response = json!({
            "access_token": "some_token_value",
            "token_type": "Bearer",
            "matrix_server_name": "example.com",
            "expires_in": 3600
        });

        assert_eq!(openid_response.get("token_type").and_then(|v| v.as_str()), Some("Bearer"));
        assert_eq!(openid_response.get("expires_in").and_then(|v| v.as_u64()), Some(3600));
    }
}

#[cfg(test)]
mod search_api_tests {
    use serde_json::json;

    #[test]
    fn test_search_request() {
        let search_request = json!({
            "search_categories": {
                "room_events": {
                    "search_term": "hello world",
                    "keys": ["content.body"],
                    "filter": {
                        "limit": 20,
                        "rooms": ["!room1:example.com"]
                    }
                }
            }
        });

        let categories = search_request.get("search_categories").unwrap();
        let room_events = categories.get("room_events").unwrap();
        
        assert_eq!(room_events.get("search_term").and_then(|v| v.as_str()), Some("hello world"));
    }

    #[test]
    fn test_search_filter() {
        let filter = json!({
            "limit": 10,
            "rooms": ["!room1:example.com", "!room2:example.com"],
            "not_rooms": ["!room3:example.com"],
            "types": ["m.room.message"],
            "senders": ["@user:example.com"]
        });

        assert!(filter.get("rooms").and_then(|v| v.as_array()).is_some());
        assert!(filter.get("types").and_then(|v| v.as_array()).is_some());
    }

    #[test]
    fn test_search_result_format() {
        let search_result = json!({
            "search_categories": {
                "room_events": {
                    "results": [
                        {
                            "result": {
                                "event_id": "$event1:example.com",
                                "room_id": "!room1:example.com",
                                "sender": "@user:example.com",
                                "type": "m.room.message",
                                "content": {
                                    "body": "hello world"
                                },
                                "origin_server_ts": 1234567890
                            },
                            "rank": 0.95
                        }
                    ],
                    "count": 1,
                    "highlights": ["hello", "world"],
                    "next_batch": null
                }
            }
        });

        let results = search_result
            .get("search_categories")
            .and_then(|c| c.get("room_events"))
            .and_then(|r| r.get("results"))
            .and_then(|r| r.as_array());

        assert!(results.is_some());
        assert_eq!(results.unwrap().len(), 1);
    }

    #[test]
    fn test_threads_response() {
        let threads_response = json!({
            "chunk": [
                {
                    "event_id": "$thread_root:example.com",
                    "sender": "@user:example.com",
                    "content": {
                        "msgtype": "m.text",
                        "body": "Thread start"
                    },
                    "origin_server_ts": 1234567890
                }
            ],
            "next_batch": null
        });

        assert!(threads_response.get("chunk").and_then(|c| c.as_array()).is_some());
    }

    #[test]
    fn test_event_context() {
        let context_response = json!({
            "event": {
                "event_id": "$target:example.com",
                "sender": "@user:example.com",
                "type": "m.room.message",
                "content": {
                    "body": "Target message"
                }
            },
            "events_before": [],
            "events_after": [],
            "state": [],
            "start": "$start:example.com",
            "end": "$end:example.com"
        });

        assert!(context_response.get("event").is_some());
        assert!(context_response.get("events_before").and_then(|e| e.as_array()).is_some());
    }

    #[test]
    fn test_timestamp_to_event() {
        let response = json!({
            "event_id": "$event123:example.com",
            "origin_server_ts": 1234567890
        });

        assert!(response.get("event_id").is_some());
        assert!(response.get("origin_server_ts").is_some());
    }
}

#[cfg(test)]
mod media_api_tests {
    use serde_json::json;

    #[test]
    fn test_media_upload_response() {
        let upload_response = json!({
            "content_uri": "mxc://example.com/abc123"
        });

        assert!(upload_response.get("content_uri").is_some());
    }

    #[test]
    fn test_url_preview_response() {
        let preview = json!({
            "url": "https://example.com/page",
            "title": "Example Page",
            "description": "A sample page",
            "og:title": "Example Page",
            "og:description": "A sample page",
            "og:image": "https://example.com/image.png",
            "og:image:width": 800,
            "og:image:height": 600
        });

        assert!(preview.get("title").is_some());
        assert!(preview.get("og:image").is_some());
    }

    #[test]
    fn test_media_config() {
        let config = json!({
            "m.upload.size": 104857600
        });

        assert!(config.get("m.upload.size").is_some());
    }
}
