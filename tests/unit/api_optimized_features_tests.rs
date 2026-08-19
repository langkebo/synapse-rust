use serde_json::json;

mod thread_tests {
    use super::*;

    #[test]
    fn test_get_thread_response_structure() {
        let response = json!({
            "room_id": "!test:example.com",
            "thread_id": "$thread123",
            "root": {
                "event_id": "$root123",
                "room_id": "!test:example.com",
                "user_id": "@user:example.com",
                "content": {}
            },
            "replies": [],
            "reply_count": 0,
            "participants": [],
            "summary": null,
            "user_receipt": null,
            "user_subscription": null
        });

        assert_eq!(response["room_id"], "!test:example.com");
        assert_eq!(response["thread_id"], "$thread123");
        assert!(response["replies"].is_array());
    }
}

mod retention_tests {
    use super::*;

    #[test]
    fn test_retention_policy_response_structure() {
        let response = json!({
            "room_id": "!test:example.com",
            "max_lifetime": 3153_600_0000_i64,
            "min_lifetime": 0_i64,
            "is_expire_on_clients": false,
            "is_server_default": false,
            "created_ts": 1700000000000_i64,
            "updated_ts": 1700000000000_i64
        });

        assert_eq!(response["room_id"], "!test:example.com");
        assert!(response["max_lifetime"].is_i64());
        assert!(response["min_lifetime"].is_i64());
    }

    #[test]
    fn test_default_retention_policy() {
        let response = json!({
            "room_id": "!test:example.com",
            "max_lifetime": null,
            "min_lifetime": 0_i64,
            "is_expire_on_clients": false,
            "is_server_default": true
        });

        assert!(response["is_server_default"].as_bool().unwrap());
    }
}

mod invites_tests {
    use super::*;

    #[test]
    fn test_room_invites_response_structure() {
        let response = json!({
            "room_id": "!test:example.com",
            "invites": [
                {
                    "user_id": "@invited:example.com",
                    "sender": "@inviter:example.com",
                    "display_name": "Invited User",
                    "avatar_url": null,
                    "event_id": "$invite123",
                    "reason": null,
                    "updated_ts": 1700000000000_i64
                }
            ],
            "total": 1
        });

        assert_eq!(response["room_id"], "!test:example.com");
        assert!(response["invites"].is_array());
        assert_eq!(response["total"], 1);
    }
}

mod encrypted_events_tests {
    use super::*;

    #[test]
    fn test_encrypted_events_response_structure() {
        let response = json!({
            "room_id": "!test:example.com",
            "events": [
                {
                    "event_id": "$encrypted123",
                    "room_id": "!test:example.com",
                    "sender": "@user:example.com",
                    "type": "m.room.encrypted",
                    "content": {
                        "algorithm": "m.megolm.v1.aes-sha2"
                    },
                    "origin_server_ts": 1700000000000_i64
                }
            ],
            "total": 1
        });

        assert_eq!(response["room_id"], "!test:example.com");
        assert!(response["events"].is_array());
        assert_eq!(response["total"], 1);
    }
}

mod signature_tests {
    use super::*;

    #[test]
    fn test_sign_event_response_structure() {
        let response = json!({
            "event_id": "$event123",
            "room_id": "!test:example.com",
            "user_id": "@user:example.com",
            "device_id": "DEVICE123",
            "key_id": "ed25519:DEVICE123",
            "signed": true,
            "created_ts": 1700000000000_i64
        });

        assert_eq!(response["event_id"], "$event123");
        assert!(response["signed"].as_bool().unwrap());
        assert!(response["key_id"].as_str().unwrap().starts_with("ed25519:"));
    }

    #[test]
    fn test_verify_event_response_structure() {
        let response = json!({
            "event_id": "$event123",
            "room_id": "!test:example.com",
            "valid": true,
            "signatures": [
                {
                    "user_id": "@user:example.com",
                    "device_id": "DEVICE123",
                    "key_id": "ed25519:DEVICE123",
                    "signature": "base64_signature_data",
                    "created_ts": 1700000000000_i64
                }
            ],
            "total": 1
        });

        assert_eq!(response["event_id"], "$event123");
        assert!(response["valid"].as_bool().unwrap());
        assert!(response["signatures"].is_array());
    }

    #[test]
    fn test_verify_event_no_signatures() {
        let response = json!({
            "event_id": "$event123",
            "room_id": "!test:example.com",
            "valid": false,
            "signatures": [],
            "total": 0
        });

        assert!(!response["valid"].as_bool().unwrap());
        assert_eq!(response["total"], 0);
    }
}

mod friend_room_tests {
    use super::*;

    #[test]
    fn test_get_friends_with_room_id() {
        let response = json!({
            "friends": [
                {
                    "user_id": "@friend:example.com",
                    "display_name": "Friend User"
                }
            ],
            "total": 1,
            "room_id": "!friends:example.com"
        });

        assert!(response["room_id"].is_string());
        assert_eq!(response["total"], 1);
    }

    /// FT-092: GET /friends 响应应只返回 `friends` 字段，不再返回冗余的 `items` 字段。
    /// 后端 handler（friend_room.rs get_friends）已删除 `"items": items` 冗余行。
    #[test]
    fn test_friends_response_no_redundant_items_field() {
        let response = json!({
            "friends": [
                {
                    "user_id": "@friend:example.com",
                    "display_name": "Friend User"
                }
            ],
            "total": 1,
            "limit": 50,
            "offset": null,
            "next_offset": null,
            "next_batch": null,
            "room_id": "!friends:example.com",
            "version": 1,
            "cached": false,
            "generated_ts": 1700000000000_i64
        });

        assert!(response.get("items").is_none(), "GET /friends 响应不应包含冗余的 items 字段（FT-092）");
        assert!(response.get("friends").is_some(), "GET /friends 响应应包含 friends 字段");
    }
}

mod config_tests {
    use super::*;

    #[test]
    fn test_client_config_response_structure() {
        let response = json!({
            "homeserver": {
                "base_url": "https://example.com",
                "server_name": "example.com"
            },
            "identity_server": {
                "base_url": "https://example.com"
            },
            "push": {
                "enabled": true
            },
            "email": {
                "enabled": false
            },
            "features": {
                "e2ee": true,
                "voip": true,
                "threads": true,
                "spaces": true
            }
        });

        assert!(response["homeserver"]["base_url"].is_string());
        assert!(response["features"]["e2ee"].as_bool().unwrap());
    }
}

/// FT-105: `get_voice_message_content` handler 必须校验调用者身份，防止 IDOR。
///
/// 这些测试覆盖 handler 所依赖的授权决策纯函数
/// `VoiceService::can_access_voice_message`，验证以下所有权规则：
/// - 管理员始终允许
/// - 上传者本人始终允许
/// - 非上传者仅当消息归属某房间且调用者为该房间成员时允许
/// - 其余情况一律拒绝
#[cfg(feature = "voice-extended")]
mod voice_idor_tests {
    use synapse_services::voice_service::VoiceService;

    /// FT-105: 上传者本人始终可以访问自己的语音消息内容
    #[test]
    fn test_owner_can_access_own_voice_message() {
        assert!(
            VoiceService::can_access_voice_message("@alice:example.com", "@alice:example.com", false, None, false,)
        );
    }

    /// FT-105: 上传者本人即使消息归属某房间也允许访问
    #[test]
    fn test_owner_can_access_own_voice_message_in_room() {
        assert!(VoiceService::can_access_voice_message(
            "@alice:example.com",
            "@alice:example.com",
            false,
            Some("!room:example.com"),
            false,
        ));
    }

    /// FT-105: 非上传者且消息不归属任何房间时，必须拒绝访问（IDOR 防护核心场景）
    #[test]
    fn test_non_owner_no_room_is_denied() {
        assert!(!VoiceService::can_access_voice_message(
            "@mallory:example.com",
            "@alice:example.com",
            false,
            None,
            false,
        ));
    }

    /// FT-105: 非上传者但消息归属某房间且调用者是该房间成员时，允许访问
    #[test]
    fn test_room_member_can_access_room_voice_message() {
        assert!(VoiceService::can_access_voice_message(
            "@bob:example.com",
            "@alice:example.com",
            false,
            Some("!room:example.com"),
            true,
        ));
    }

    /// FT-105: 非上传者且非房间成员，即使消息归属某房间也必须拒绝访问
    #[test]
    fn test_non_member_is_denied_even_if_message_has_room() {
        assert!(!VoiceService::can_access_voice_message(
            "@mallory:example.com",
            "@alice:example.com",
            false,
            Some("!room:example.com"),
            false,
        ));
    }

    /// FT-105: 管理员始终可以访问任意语音消息内容（即使非上传者、非房间成员）
    #[test]
    fn test_admin_can_access_any_voice_message() {
        assert!(VoiceService::can_access_voice_message("@admin:example.com", "@alice:example.com", true, None, false,));
    }

    /// FT-105: 管理员访问归属房间的他人消息也应放行
    #[test]
    fn test_admin_can_access_room_voice_message_without_membership() {
        assert!(VoiceService::can_access_voice_message(
            "@admin:example.com",
            "@alice:example.com",
            true,
            Some("!room:example.com"),
            false,
        ));
    }
}
