// Real type assertions replacing the previous JSON smoke tests.
//
// Each module exercises a real public type from the workspace crates
// (synapse_common / synapse_storage / synapse_services / synapse_e2ee /
// synapse_federation / synapse_cache). Only pure-logic paths are covered
// here; DB-dependent behaviour lives under tests/integration/.

// ---------------------------------------------------------------------------
// Auth domain
// ---------------------------------------------------------------------------

mod auth_tests {
    use synapse_common::config::SecurityConfig;
    use synapse_services::auth::{Claims, ClaimsBuilder, PasswordPolicy};

    #[test]
    fn test_password_policy_default_values() {
        let policy = PasswordPolicy::default();
        assert_eq!(policy.min_length, 8);
        assert_eq!(policy.max_length, 128);
        assert!(policy.require_uppercase);
        assert!(policy.require_lowercase);
        assert!(policy.require_digit);
        assert!(policy.require_special);
        assert_eq!(policy.max_age_days, 90);
        assert_eq!(policy.history_count, 5);
        assert_eq!(policy.max_failed_attempts, 5);
        assert_eq!(policy.lockout_duration_minutes, 30);
        assert!(policy.force_first_login_change);
    }

    #[test]
    fn test_password_policy_validate_strong_password() {
        let policy = PasswordPolicy::default();
        let result = policy.validate("Abcdef1!");
        assert!(result.is_valid);
        assert!(result.errors.is_empty());
        assert_eq!(result.strength_score, 100);
    }

    #[test]
    fn test_password_policy_validate_weak_password() {
        let policy = PasswordPolicy::default();
        let result = policy.validate("weak");
        assert!(!result.is_valid);
        // "weak" has lowercase only → fails length, uppercase, digit, special.
        assert!(result.errors.len() >= 4);
        // Lowercase check passes → +20 score.
        assert_eq!(result.strength_score, 20);
    }

    #[test]
    fn test_password_policy_is_password_expired_when_never_changed() {
        let policy = PasswordPolicy::default();
        // None means password has never been changed → should be expired.
        assert!(policy.is_password_expired(None));
    }

    #[test]
    fn test_password_policy_is_password_expired_recent_change() {
        let policy = PasswordPolicy::default();
        let now = chrono::Utc::now().timestamp_millis();
        // Changed right now → not expired.
        assert!(!policy.is_password_expired(Some(now)));
    }

    #[test]
    fn test_password_policy_no_expiry_when_max_age_zero() {
        let mut policy = PasswordPolicy::default();
        policy.max_age_days = 0;
        assert!(!policy.is_password_expired(None));
    }

    #[test]
    fn test_password_policy_serde_roundtrip() {
        let policy = PasswordPolicy::default();
        let json = serde_json::to_string(&policy).expect("serialize");
        let restored: PasswordPolicy = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.min_length, policy.min_length);
        assert_eq!(restored.require_special, policy.require_special);
    }

    #[test]
    fn test_claims_builder_builds_valid_claims() {
        let claims = ClaimsBuilder::new()
            .sub("@alice:example.com")
            .exp(1_800_000_000)
            .is_admin(true)
            .device_id(Some("DEVICE01".to_string()))
            .build();
        assert_eq!(claims.sub, "@alice:example.com");
        assert_eq!(claims.user_id, "@alice:example.com");
        assert!(claims.is_admin);
        assert_eq!(claims.exp, 1_800_000_000);
        assert!(claims.iat > 0);
        assert_eq!(claims.device_id.as_deref(), Some("DEVICE01"));
        assert!(!claims.jti.is_empty());
    }

    #[test]
    fn test_claims_serde_roundtrip() {
        let claims = ClaimsBuilder::new()
            .sub("@bob:example.com")
            .exp(1_900_000_000)
            .iss("example.com")
            .aud("example.com")
            .build();
        let json = serde_json::to_string(&claims).expect("serialize");
        let restored: Claims = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.sub, claims.sub);
        assert_eq!(restored.exp, claims.exp);
        assert_eq!(restored.iss.as_deref(), Some("example.com"));
        assert_eq!(restored.aud.as_deref(), Some("example.com"));
    }

    #[test]
    fn test_security_config_default() {
        // SecurityConfig::default() intentionally leaves token-expiry fields
        // at 0 (caller must configure them) but provides non-zero Argon2
        // costs via the default_* helper functions.
        let config = SecurityConfig::default();
        assert_eq!(config.expiry_time, 0);
        assert_eq!(config.refresh_token_expiry, 0);
        assert!(config.argon2_m_cost > 0);
        assert!(config.argon2_t_cost > 0);
        assert!(config.argon2_p_cost > 0);
        assert!(!config.admin_mfa_required);
    }
}

// ---------------------------------------------------------------------------
// Room / Event domain
// ---------------------------------------------------------------------------

mod room_tests {
    use synapse_common::event_models::RoomEvent;
    use synapse_services::CreateRoomConfig;
    use synapse_storage::room::{Room, RoomEncryptionStatus};

    #[test]
    fn test_create_room_config_default_all_none() {
        let config = CreateRoomConfig::default();
        assert!(config.name.is_none());
        assert!(config.topic.is_none());
        assert!(config.invite_list.is_none());
        assert!(config.encryption.is_none());
        assert!(config.is_direct.is_none());
    }

    #[test]
    fn test_create_room_config_construction_and_defaults() {
        // CreateRoomConfig derives Default but not Serialize/Deserialize.
        let config = CreateRoomConfig {
            name: Some("Test Room".to_string()),
            topic: Some("Topic".to_string()),
            visibility: Some("public".to_string()),
            preset: Some("public_chat".to_string()),
            ..Default::default()
        };
        assert_eq!(config.name.as_deref(), Some("Test Room"));
        assert_eq!(config.topic.as_deref(), Some("Topic"));
        assert_eq!(config.visibility.as_deref(), Some("public"));
        assert_eq!(config.preset.as_deref(), Some("public_chat"));
        // Fields not set should inherit Default (None).
        assert!(config.invite_list.is_none());
        assert!(config.encryption.is_none());
        assert!(config.is_direct.is_none());
        assert!(config.room_type.is_none());
    }

    #[test]
    fn test_room_serde_roundtrip() {
        let room = Room {
            room_id: "!room:example.com".to_string(),
            name: Some("Test".to_string()),
            topic: None,
            avatar_url: None,
            canonical_alias: None,
            join_rule: "invite".to_string(),
            creator_user_id: Some("@creator:example.com".to_string()),
            room_version: "9".to_string(),
            encryption: None,
            is_public: false,
            member_count: 1,
            history_visibility: "joined".to_string(),
            created_ts: 1_700_000_000_000,
            is_federatable: true,
            is_spotlight: false,
            is_flagged: false,
        };
        let json = serde_json::to_string(&room).expect("serialize");
        let restored: Room = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.room_id, room.room_id);
        assert_eq!(restored.room_version, "9");
        assert!(restored.is_federatable);
    }

    #[test]
    fn test_room_encryption_status_unencrypted() {
        let status = RoomEncryptionStatus {
            is_encrypted: false,
            algorithm: None,
            rotation_period_ms: None,
            rotation_period_msgs: None,
        };
        assert!(!status.is_encrypted);
        assert!(status.algorithm.is_none());
    }

    #[test]
    fn test_room_event_serde_roundtrip() {
        let event = RoomEvent {
            event_id: "$event:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            user_id: "@sender:example.com".to_string(),
            event_type: "m.room.message".to_string(),
            content: serde_json::json!({"msgtype": "m.text", "body": "hello"}),
            state_key: None,
            depth: 5,
            origin_server_ts: 1_700_000_000_000,
            processed_ts: 1_700_000_000_100,
            not_before: 4,
            status: None,
            reference_image: None,
            origin: "example.com".to_string(),
            stream_ordering: Some(42),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        let restored: RoomEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.event_id, event.event_id);
        assert_eq!(restored.origin_server_ts, event.origin_server_ts);
        assert_eq!(restored.stream_ordering, Some(42));
    }
}

// ---------------------------------------------------------------------------
// Common types (UserId / EventId / RoomAlias / Membership / PresenceState)
// ---------------------------------------------------------------------------

mod common_types_tests {
    use std::str::FromStr;

    use synapse_common::{Membership, PresenceState, SecretString};

    #[test]
    fn test_membership_display() {
        assert_eq!(Membership::Join.to_string(), "join");
        assert_eq!(Membership::Leave.to_string(), "leave");
        assert_eq!(Membership::Invite.to_string(), "invite");
        assert_eq!(Membership::Ban.to_string(), "ban");
        assert_eq!(Membership::Knock.to_string(), "knock");
    }

    #[test]
    fn test_membership_serde_roundtrip() {
        // Membership derives Serialize/Deserialize but not PartialEq, so
        // compare via the Display impl instead. Without #[serde(rename_all)]
        // the variant name is serialized verbatim as "Join".
        let json = serde_json::to_string(&Membership::Join).expect("serialize");
        assert_eq!(json, "\"Join\"");
        let restored: Membership = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.to_string(), "join");
    }

    #[test]
    fn test_presence_state_as_str() {
        assert_eq!(PresenceState::Online.as_str(), "online");
        assert_eq!(PresenceState::Offline.as_str(), "offline");
        assert_eq!(PresenceState::Unavailable.as_str(), "unavailable");
        assert_eq!(PresenceState::Busy.as_str(), "busy");
    }

    #[test]
    fn test_presence_state_from_str_valid() {
        assert_eq!(PresenceState::from_str("online").unwrap(), PresenceState::Online);
        assert_eq!(PresenceState::from_str("offline").unwrap(), PresenceState::Offline);
        assert_eq!(PresenceState::from_str("busy").unwrap(), PresenceState::Busy);
    }

    #[test]
    fn test_presence_state_from_str_away_maps_to_unavailable() {
        // "away" is a legacy alias for Unavailable.
        assert_eq!(PresenceState::from_str_opt("away"), Some(PresenceState::Unavailable));
    }

    #[test]
    fn test_presence_state_from_str_invalid() {
        assert!(PresenceState::from_str("unknown").is_err());
        assert!(PresenceState::from_str_opt("unknown").is_none());
    }

    #[test]
    fn test_presence_state_from_str_unknown_falls_back_to_offline() {
        // The From<&str> impl falls back to Offline for unknown values.
        let s: PresenceState = "unknown".into();
        assert_eq!(s, PresenceState::Offline);
    }

    #[test]
    fn test_presence_state_is_active() {
        assert!(PresenceState::Online.is_active());
        assert!(PresenceState::Unavailable.is_active());
        assert!(PresenceState::Busy.is_active());
        assert!(!PresenceState::Offline.is_active());
    }

    #[test]
    fn test_presence_state_derive_activity_offline_returns_none() {
        let (active_ago, currently_active) =
            PresenceState::Offline.derive_activity(Some(1_000), 2_000);
        assert_eq!(active_ago, None);
        assert_eq!(currently_active, None);
    }

    #[test]
    fn test_presence_state_derive_activity_online_within_threshold() {
        let now = 10_000_000;
        let (active_ago, currently_active) =
            PresenceState::Online.derive_activity(Some(now - 1000), now);
        assert_eq!(active_ago, Some(1000));
        assert_eq!(currently_active, Some(true));
    }

    #[test]
    fn test_presence_state_valid_strs_includes_away_alias() {
        let strs = PresenceState::valid_strs();
        assert!(strs.contains(&"online"));
        assert!(strs.contains(&"away"));
        assert!(strs.contains(&"busy"));
    }

    #[test]
    fn test_secret_string_redacts_in_display() {
        let s = SecretString::new("super-secret-token".to_string());
        assert_eq!(s.expose(), "super-secret-token");
        assert_eq!(format!("{s}"), "[REDACTED]");
        assert_eq!(format!("{s:?}"), "SecretString([REDACTED])");
    }

    #[test]
    fn test_secret_string_serde_redacts_on_serialize() {
        let s = SecretString::new("real-value".to_string());
        let json = serde_json::to_string(&s).expect("serialize");
        // Serialize always emits the redacted placeholder.
        assert_eq!(json, "\"[REDACTED]\"");
        // Deserialize wraps whatever string was provided.
        let restored: SecretString = serde_json::from_str("\"input\"").expect("deserialize");
        assert_eq!(restored.expose(), "input");
    }

    #[test]
    fn test_secret_string_from_str_and_string() {
        let from_str_val: SecretString = "literal".into();
        assert_eq!(from_str_val.expose(), "literal");
        let from_string_val: SecretString = "owned".to_string().into();
        assert_eq!(from_string_val.expose(), "owned");
    }
}

// ---------------------------------------------------------------------------
// E2EE domain
// ---------------------------------------------------------------------------

mod e2ee_tests {
    use std::str::FromStr;

    use synapse_e2ee::megolm::models::PickleFormat;
    use synapse_e2ee::{BackupKeyUploadRequest, KeyBackup, OlmAccountData};

    #[test]
    fn test_pickle_format_default_is_legacy() {
        assert_eq!(PickleFormat::default(), PickleFormat::Legacy);
    }

    #[test]
    fn test_pickle_format_as_str() {
        assert_eq!(PickleFormat::Legacy.as_str(), "legacy");
        assert_eq!(PickleFormat::Vodozemac.as_str(), "vodozemac");
        assert_eq!(PickleFormat::Dual.as_str(), "dual");
    }

    #[test]
    fn test_pickle_format_from_str_known() {
        assert_eq!(PickleFormat::from_str("vodozemac").unwrap(), PickleFormat::Vodozemac);
        assert_eq!(PickleFormat::from_str("dual").unwrap(), PickleFormat::Dual);
        assert_eq!(PickleFormat::from_str("legacy").unwrap(), PickleFormat::Legacy);
    }

    #[test]
    fn test_pickle_format_from_str_unknown_falls_back_to_legacy() {
        // Unknown strings fall back to Legacy rather than erroring.
        assert_eq!(PickleFormat::from_str("nope").unwrap(), PickleFormat::Legacy);
    }

    #[test]
    fn test_pickle_format_serde_roundtrip() {
        let fmt = PickleFormat::Vodozemac;
        let json = serde_json::to_string(&fmt).expect("serialize");
        assert_eq!(json, "\"vodozemac\"");
        let restored: PickleFormat = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, fmt);
    }

    #[test]
    fn test_olm_account_data_serde_roundtrip() {
        let account = OlmAccountData {
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE01".to_string(),
            identity_key: "ed25519:KEY".to_string(),
            serialized_account: "pickle-data".to_string(),
            has_published_one_time_keys: true,
            has_published_fallback_key: false,
        };
        let json = serde_json::to_string(&account).expect("serialize");
        let restored: OlmAccountData = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.user_id, account.user_id);
        assert_eq!(restored.identity_key, "ed25519:KEY");
        assert!(restored.has_published_one_time_keys);
        assert!(!restored.has_published_fallback_key);
    }

    #[test]
    fn test_key_backup_serde_roundtrip() {
        let backup = KeyBackup {
            user_id: "@alice:example.com".to_string(),
            backup_id: "backup-1".to_string(),
            version: 1,
            algorithm: "m.megolm_backup.v1.curve25519-aes-sha2".to_string(),
            auth_key: "auth".to_string(),
            mgmt_key: "mgmt".to_string(),
            backup_data: serde_json::json!({}),
            etag: Some("etag-1".to_string()),
        };
        let json = serde_json::to_string(&backup).expect("serialize");
        let restored: KeyBackup = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.backup_id, backup.backup_id);
        assert_eq!(restored.version, 1);
        assert_eq!(restored.algorithm, backup.algorithm);
    }

    #[test]
    fn test_backup_key_upload_request_serde_roundtrip() {
        let req = BackupKeyUploadRequest {
            first_message_index: 0,
            forwarded_count: 0,
            is_verified: true,
            session_data: "encrypted-session-data".to_string(),
        };
        let json = serde_json::to_string(&req).expect("serialize");
        let restored: BackupKeyUploadRequest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.first_message_index, 0);
        assert!(restored.is_verified);
        assert_eq!(restored.session_data, "encrypted-session-data");
    }
}

// ---------------------------------------------------------------------------
// Media domain
// ---------------------------------------------------------------------------

mod media_tests {
    use std::str::FromStr;

    use synapse_services::{ThumbnailMethod, ThumbnailSettings};

    #[test]
    fn test_thumbnail_method_from_str_valid() {
        assert_eq!(ThumbnailMethod::from_str("crop").unwrap(), ThumbnailMethod::Crop);
        assert_eq!(ThumbnailMethod::from_str("scale").unwrap(), ThumbnailMethod::Scale);
        assert_eq!(ThumbnailMethod::from_str("CROP").unwrap(), ThumbnailMethod::Crop);
    }

    #[test]
    fn test_thumbnail_method_from_str_invalid() {
        assert!(ThumbnailMethod::from_str("bogus").is_err());
    }

    #[test]
    fn test_thumbnail_settings_default() {
        let settings = ThumbnailSettings::default();
        assert_eq!(settings.width, 800);
        assert_eq!(settings.height, 600);
        assert_eq!(settings.method, ThumbnailMethod::Scale);
        assert_eq!(settings.quality, 80);
    }

    #[test]
    fn test_thumbnail_settings_custom() {
        let settings = ThumbnailSettings {
            width: 128,
            height: 128,
            method: ThumbnailMethod::Crop,
            quality: 50,
        };
        assert_eq!(settings.width, 128);
        assert_eq!(settings.method, ThumbnailMethod::Crop);
    }
}

// ---------------------------------------------------------------------------
// Federation domain
// ---------------------------------------------------------------------------

mod federation_tests {
    use std::str::FromStr;

    use synapse_federation::edu::{EduProcessResult, EduType, user_matches_origin};
    use synapse_federation::ServerAclContent;

    #[test]
    fn test_server_acl_content_default_denies_ip_literals() {
        let acl = ServerAclContent::default();
        assert!(acl.allow.is_empty());
        assert!(acl.deny.is_empty());
        // #[derive(Default)] gives bool false.
        assert!(!acl.allow_ip_literals);
    }

    #[test]
    fn test_server_acl_content_serde_roundtrip() {
        let acl = ServerAclContent {
            allow: vec!["*".to_string()],
            deny: vec!["evil.example".to_string()],
            allow_ip_literals: true,
        };
        let json = serde_json::to_string(&acl).expect("serialize");
        let restored: ServerAclContent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.allow, acl.allow);
        assert_eq!(restored.deny, acl.deny);
        assert!(restored.allow_ip_literals);
    }

    #[test]
    fn test_edu_type_from_str_valid() {
        assert_eq!(EduType::from_str("m.typing").unwrap(), EduType::Typing);
        assert_eq!(EduType::from_str("m.presence").unwrap(), EduType::Presence);
        assert_eq!(
            EduType::from_str("m.device_list_update").unwrap(),
            EduType::DeviceListUpdate
        );
        assert_eq!(
            EduType::from_str("m.direct_to_device").unwrap(),
            EduType::DirectToDevice
        );
    }

    #[test]
    fn test_edu_type_from_str_unknown_returns_error() {
        let err = EduType::from_str("m.unknown").unwrap_err();
        assert!(format!("{err}").contains("unknown EDU type"));
    }

    #[test]
    fn test_edu_process_result_default_is_empty() {
        let result = EduProcessResult::default();
        assert_eq!(result.processed, 0);
        assert_eq!(result.dropped, 0);
        assert_eq!(result.errored, 0);
        assert!(result.is_empty());
    }

    #[test]
    fn test_edu_process_result_not_empty_after_processing() {
        let result = EduProcessResult {
            processed: 5,
            dropped: 1,
            errored: 0,
        };
        assert!(!result.is_empty());
    }

    #[test]
    fn test_user_matches_origin_positive() {
        assert!(user_matches_origin("@alice:example.com", "example.com"));
    }

    #[test]
    fn test_user_matches_origin_negative() {
        assert!(!user_matches_origin("@alice:other.com", "example.com"));
        assert!(!user_matches_origin("malformed-user-id", "example.com"));
    }
}

// ---------------------------------------------------------------------------
// Push notification domain
// ---------------------------------------------------------------------------

mod push_tests {
    use synapse_services::{NotificationPayload, PushRuleResult};

    #[test]
    fn test_notification_payload_serde_roundtrip() {
        let payload = NotificationPayload {
            title: "New message".to_string(),
            body: "Hello world".to_string(),
            icon: Some("mxc://example.com/icon".to_string()),
            badge: None,
            sound: None,
            tag: Some("m.message".to_string()),
            data: serde_json::json!({}),
            event_id: Some("$event:example.com".to_string()),
            room_id: Some("!room:example.com".to_string()),
            room_name: Some("Test Room".to_string()),
            sender: Some("@alice:example.com".to_string()),
            counts: None,
        };
        let json = serde_json::to_string(&payload).expect("serialize");
        let restored: NotificationPayload = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.title, payload.title);
        assert_eq!(restored.event_id, payload.event_id);
        assert!(restored.badge.is_none());
    }

    #[test]
    fn test_push_rule_result_notify_true() {
        let result = PushRuleResult {
            notify: true,
            tweaks: serde_json::json!({"sound": "default"}),
        };
        assert!(result.notify);
        assert!(result.tweaks.is_object());
    }

    #[test]
    fn test_push_rule_result_notify_false() {
        let result = PushRuleResult {
            notify: false,
            tweaks: serde_json::json!({}),
        };
        assert!(!result.notify);
    }
}

// ---------------------------------------------------------------------------
// Search domain
// ---------------------------------------------------------------------------

mod search_tests {
    use synapse_services::{
        AdvancedSearchOptions, RoomEventsSearchFilter, SearchFilters, SearchResult, SearchResultItem,
    };

    #[test]
    fn test_search_filters_default_all_none() {
        let filters = SearchFilters::default();
        assert!(filters.sender_id.is_none());
        assert!(filters.room_id.is_none());
        assert!(filters.has_media.is_none());
    }

    #[test]
    fn test_advanced_search_options_default_values() {
        let opts = AdvancedSearchOptions::default();
        assert_eq!(opts.limit, 20);
        assert_eq!(opts.offset, 0);
        assert!(opts.highlight);
        assert!(opts.fuzzy);
        assert!(opts.query.is_empty());
    }

    #[test]
    fn test_room_events_search_filter_default_all_none() {
        let filter = RoomEventsSearchFilter::default();
        assert!(filter.rooms.is_none());
        assert!(filter.not_rooms.is_none());
        assert!(filter.types.is_none());
        assert!(filter.senders.is_none());
    }

    #[test]
    fn test_search_result_serde_roundtrip() {
        let result = SearchResult {
            results: vec![SearchResultItem {
                event_id: "$event:example.com".to_string(),
                room_id: "!room:example.com".to_string(),
                sender: "@alice:example.com".to_string(),
                content: "hello".to_string(),
                event_type: "m.room.message".to_string(),
                origin_server_ts: 1_700_000_000_000,
                highlights: Some(vec!["hello".to_string()]),
                room_name: Some("Test Room".to_string()),
            }],
            total_count: 1,
            next_batch: Some("cursor-1".to_string()),
        };
        let json = serde_json::to_string(&result).expect("serialize");
        let restored: SearchResult = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.total_count, 1);
        assert_eq!(restored.results.len(), 1);
        assert_eq!(restored.results[0].event_id, "$event:example.com");
    }
}

// ---------------------------------------------------------------------------
// Space / Thread domain
// ---------------------------------------------------------------------------

mod space_thread_tests {
    use synapse_services::thread_service::{CreateThreadRequest, ListThreadsRequest, ThreadListResponse};
    use synapse_storage::room_summary::UpdateRoomSummaryRequest;
    use synapse_storage::space::UpdateSpaceRequest;

    #[test]
    fn test_update_space_request_default_and_builder() {
        let req = UpdateSpaceRequest::default();
        assert!(req.name.is_none());
        assert!(req.topic.is_none());
        assert!(req.is_public.is_none());

        let built = UpdateSpaceRequest::new()
            .name("My Space")
            .topic("A space topic")
            .is_public(true);
        assert_eq!(built.name.as_deref(), Some("My Space"));
        assert_eq!(built.topic.as_deref(), Some("A space topic"));
        assert_eq!(built.is_public, Some(true));
    }

    #[test]
    fn test_update_room_summary_request_default_all_none() {
        let req = UpdateRoomSummaryRequest::default();
        assert!(req.name.is_none());
        assert!(req.is_encrypted.is_none());
        assert!(req.last_event_id.is_none());
        assert!(req.last_event_ts.is_none());
        assert!(req.hero_users.is_none());
    }

    #[test]
    fn test_update_room_summary_request_serde_roundtrip() {
        let req = UpdateRoomSummaryRequest {
            name: Some("Updated Name".to_string()),
            is_encrypted: Some(true),
            last_event_ts: Some(1_700_000_000_000),
            ..Default::default()
        };
        let json = serde_json::to_string(&req).expect("serialize");
        let restored: UpdateRoomSummaryRequest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.name, req.name);
        assert_eq!(restored.is_encrypted, Some(true));
        assert_eq!(restored.last_event_ts, Some(1_700_000_000_000));
    }

    #[test]
    fn test_create_thread_request_serde_roundtrip() {
        let req = CreateThreadRequest {
            room_id: "!room:example.com".to_string(),
            root_event_id: "$root:example.com".to_string(),
        };
        let json = serde_json::to_string(&req).expect("serialize");
        let restored: CreateThreadRequest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.room_id, req.room_id);
        assert_eq!(restored.root_event_id, req.root_event_id);
    }

    #[test]
    fn test_list_threads_request_serde_roundtrip() {
        let req = ListThreadsRequest {
            room_id: "!room:example.com".to_string(),
            limit: Some(20),
            from: None,
            include_all: false,
        };
        let json = serde_json::to_string(&req).expect("serialize");
        let restored: ListThreadsRequest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.room_id, req.room_id);
        assert_eq!(restored.limit, Some(20));
        assert!(!restored.include_all);
    }

    #[test]
    fn test_thread_list_response_empty_serde() {
        let resp = ThreadListResponse {
            threads: vec![],
            next_batch: None,
            total: 0,
        };
        let json = serde_json::to_string(&resp).expect("serialize");
        let restored: ThreadListResponse = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.total, 0);
        assert!(restored.threads.is_empty());
        assert!(restored.next_batch.is_none());
    }
}

// ---------------------------------------------------------------------------
// Sync domain
// ---------------------------------------------------------------------------

mod sync_tests {
    use synapse_services::{RoomSyncCounts, SyncEventFormat, SyncFilter, SyncToken};

    #[test]
    fn test_sync_token_encode_simple() {
        let token = SyncToken {
            stream_id: 42,
            room_id: None,
            event_type: None,
            to_device_stream_id: None,
            device_list_stream_id: None,
        };
        assert_eq!(token.encode(), "s42");
    }

    #[test]
    fn test_sync_token_encode_with_streams() {
        let token = SyncToken {
            stream_id: 42,
            room_id: None,
            event_type: None,
            to_device_stream_id: Some(7),
            device_list_stream_id: Some(3),
        };
        assert_eq!(token.encode(), "s42_7_3");
    }

    #[test]
    fn test_sync_token_parse_simple() {
        let token = SyncToken::parse("s42").expect("valid simple token");
        assert_eq!(token.stream_id, 42);
        assert!(token.to_device_stream_id.is_none());
        assert!(token.device_list_stream_id.is_none());
    }

    #[test]
    fn test_sync_token_parse_with_streams() {
        let token = SyncToken::parse("s42_7_3").expect("valid composite token");
        assert_eq!(token.stream_id, 42);
        assert_eq!(token.to_device_stream_id, Some(7));
        assert_eq!(token.device_list_stream_id, Some(3));
    }

    #[test]
    fn test_sync_token_parse_invalid_returns_none() {
        assert!(SyncToken::parse("no-prefix").is_none());
        assert!(SyncToken::parse("snotanumber").is_none());
        assert!(SyncToken::parse("").is_none());
    }

    #[test]
    fn test_sync_token_roundtrip() {
        let original = SyncToken {
            stream_id: 100,
            room_id: None,
            event_type: None,
            to_device_stream_id: Some(10),
            device_list_stream_id: Some(5),
        };
        let encoded = original.encode();
        let parsed = SyncToken::parse(&encoded).expect("roundtrip parse");
        assert_eq!(parsed.stream_id, original.stream_id);
        assert_eq!(parsed.to_device_stream_id, original.to_device_stream_id);
        assert_eq!(parsed.device_list_stream_id, original.device_list_stream_id);
    }

    #[test]
    fn test_sync_filter_default_limit_100() {
        let filter = SyncFilter::default();
        assert_eq!(filter.limit, Some(100));
        assert!(filter.types.is_none());
        assert!(filter.rooms.is_none());
    }

    #[test]
    fn test_sync_filter_serde_roundtrip() {
        let filter = SyncFilter {
            limit: Some(50),
            types: Some(vec!["m.room.message".to_string()]),
            not_types: Some(vec!["m.reaction".to_string()]),
            rooms: Some(vec!["!room:example.com".to_string()]),
            not_rooms: None,
            contains_url: Some(false),
            lazy_load_members: Some(true),
            include_redundant_members: None,
            senders: None,
            not_senders: None,
        };
        let json = serde_json::to_string(&filter).expect("serialize");
        let restored: SyncFilter = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.limit, Some(50));
        assert_eq!(restored.types, Some(vec!["m.room.message".to_string()]));
        assert!(restored.lazy_load_members.unwrap_or(false));
    }

    #[test]
    fn test_sync_event_format_default_is_client() {
        assert_eq!(SyncEventFormat::default(), SyncEventFormat::Client);
    }

    #[test]
    fn test_sync_event_format_serde_roundtrip() {
        let json = serde_json::to_string(&SyncEventFormat::Federation).expect("serialize");
        assert_eq!(json, "\"federation\"");
        let restored: SyncEventFormat = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, SyncEventFormat::Federation);
    }

    #[test]
    fn test_room_sync_counts_default_zero() {
        let counts = RoomSyncCounts::default();
        assert_eq!(counts.highlight_count, 0);
        assert_eq!(counts.notification_count, 0);
    }
}

// ---------------------------------------------------------------------------
// Cache domain
// ---------------------------------------------------------------------------

mod cache_tests {
    use synapse_cache::{CacheConfig, CacheStats, DegradationMetrics};

    #[test]
    fn test_cache_stats_default_all_zero() {
        let stats = CacheStats::default();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
        assert_eq!(stats.evictions, 0);
        assert_eq!(stats.total_entries, 0);
        assert_eq!(stats.memory_usage_bytes, 0);
        assert_eq!(stats.hit_rate, 0.0);
    }

    #[test]
    fn test_cache_stats_serde_roundtrip() {
        let stats = CacheStats {
            hits: 100,
            misses: 20,
            evictions: 5,
            total_entries: 80,
            memory_usage_bytes: 1024 * 1024,
            hit_rate: 0.833,
        };
        let json = serde_json::to_string(&stats).expect("serialize");
        let restored: CacheStats = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.hits, 100);
        assert_eq!(restored.total_entries, 80);
        assert!((restored.hit_rate - 0.833).abs() < 1e-6);
    }

    #[test]
    fn test_cache_config_default_values() {
        let config = CacheConfig::default();
        assert_eq!(config.max_capacity, 100_000);
        assert_eq!(config.time_to_live, 7200);
    }

    #[test]
    fn test_cache_config_custom() {
        let config = CacheConfig {
            max_capacity: 5000,
            time_to_live: 60,
        };
        assert_eq!(config.max_capacity, 5000);
        assert_eq!(config.time_to_live, 60);
    }

    #[test]
    fn test_degradation_metrics_default_all_zero() {
        let metrics = DegradationMetrics::default();
        assert_eq!(metrics.local_cache_hits, 0);
        assert_eq!(metrics.local_cache_misses, 0);
        assert_eq!(metrics.redis_cache_hits, 0);
        assert_eq!(metrics.redis_cache_misses, 0);
        assert_eq!(metrics.circuit_breaker_rejections, 0);
        assert_eq!(metrics.fallback_operations, 0);
        assert_eq!(metrics.total_degraded_requests, 0);
    }
}

// ---------------------------------------------------------------------------
// Error handling domain
// ---------------------------------------------------------------------------

mod error_tests {
    use axum::http::StatusCode;
    use synapse_common::error::{ApiError, ApiErrorKind, ErrorSource, MatrixErrorCode};

    #[test]
    fn test_api_error_not_found_factory() {
        let err = ApiError::not_found("resource missing");
        assert_eq!(err.kind, ApiErrorKind::NotFound);
        assert_eq!(err.code, MatrixErrorCode::NotFound);
        assert_eq!(err.code.as_str(), "M_NOT_FOUND");
        assert_eq!(err.code.http_status(), StatusCode::NOT_FOUND);
        assert_eq!(err.message, "resource missing");
    }

    #[test]
    fn test_api_error_forbidden_factory() {
        let err = ApiError::forbidden("no access");
        assert_eq!(err.kind, ApiErrorKind::Forbidden);
        assert_eq!(err.code, MatrixErrorCode::Forbidden);
        assert_eq!(err.code.http_status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_api_error_unauthorized_factory() {
        let err = ApiError::unauthorized("token missing");
        assert_eq!(err.kind, ApiErrorKind::Unauthorized);
        assert_eq!(err.code, MatrixErrorCode::Unauthorized);
        assert_eq!(err.code.http_status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_api_error_rate_limited_factory() {
        let err = ApiError::rate_limited("slow down");
        assert_eq!(err.kind, ApiErrorKind::RateLimited);
        assert_eq!(err.code, MatrixErrorCode::LimitExceeded);
        assert_eq!(err.code.http_status(), StatusCode::TOO_MANY_REQUESTS);
        // rate_limited hardcodes the message.
        assert_eq!(err.message, "Rate limited");
    }

    #[test]
    fn test_api_error_internal_factory() {
        let err = ApiError::internal("boom");
        assert_eq!(err.kind, ApiErrorKind::Internal);
        assert_eq!(err.code, MatrixErrorCode::Unknown);
        assert_eq!(err.code.http_status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_api_error_conflict_factory() {
        let err = ApiError::conflict("user exists");
        assert_eq!(err.kind, ApiErrorKind::Conflict);
        assert_eq!(err.code, MatrixErrorCode::UserInUse);
        assert_eq!(err.code.http_status(), StatusCode::CONFLICT);
    }

    #[test]
    fn test_api_error_bad_request_factory() {
        let err = ApiError::bad_request("malformed");
        assert_eq!(err.kind, ApiErrorKind::BadRequest);
        assert_eq!(err.code, MatrixErrorCode::BadJson);
        assert_eq!(err.code.http_status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_api_error_validation_uses_invalid_param() {
        let err = ApiError::validation("bad field");
        assert_eq!(err.kind, ApiErrorKind::BadRequest);
        assert_eq!(err.code, MatrixErrorCode::InvalidParam);
        assert_eq!(err.code.as_str(), "M_INVALID_PARAM");
        assert_eq!(err.code.http_status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_api_error_too_large_factory() {
        let err = ApiError::too_large("file too big");
        assert_eq!(err.code, MatrixErrorCode::TooLarge);
        assert_eq!(err.code.http_status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[test]
    fn test_api_error_serde_roundtrip_skips_cause() {
        let err = ApiError::not_found("missing");
        let json = serde_json::to_string(&err).expect("serialize");
        // cause/source are #[serde(skip)] so the JSON must include kind+code+message only.
        let restored: ApiError = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, err);
    }

    #[test]
    fn test_api_error_display_without_source() {
        let err = ApiError::not_found("missing");
        let s = format!("{err}");
        assert!(s.contains("M_NOT_FOUND"));
        assert!(s.contains("missing"));
    }

    #[test]
    fn test_api_error_display_with_source() {
        let mut err = ApiError::not_found("missing");
        err.source = Some(ErrorSource::new("storage::room", "get_room"));
        let s = format!("{err}");
        assert!(s.contains("[storage::room::get_room]"));
        assert!(s.contains("M_NOT_FOUND"));
    }

    #[test]
    fn test_error_source_new_and_display() {
        let src = ErrorSource::new("module", "op");
        assert_eq!(src.module, "module");
        assert_eq!(src.operation, "op");
        assert_eq!(format!("{src}"), "[module::op]");
    }

    #[test]
    fn test_matrix_error_code_as_str_examples() {
        assert_eq!(MatrixErrorCode::Forbidden.as_str(), "M_FORBIDDEN");
        assert_eq!(MatrixErrorCode::UnknownToken.as_str(), "M_UNKNOWN_TOKEN");
        assert_eq!(MatrixErrorCode::MissingToken.as_str(), "M_MISSING_TOKEN");
        assert_eq!(MatrixErrorCode::BadJson.as_str(), "M_BAD_JSON");
        assert_eq!(MatrixErrorCode::LimitExceeded.as_str(), "M_LIMIT_EXCEEDED");
        assert_eq!(MatrixErrorCode::Unknown.as_str(), "M_UNKNOWN");
        assert_eq!(MatrixErrorCode::Unrecognized.as_str(), "M_UNRECOGNIZED");
        assert_eq!(MatrixErrorCode::TooLarge.as_str(), "M_TOO_LARGE");
        assert_eq!(MatrixErrorCode::InvalidParam.as_str(), "M_INVALID_PARAM");
        assert_eq!(MatrixErrorCode::RequestTimeout.as_str(), "M_REQUEST_TIMEOUT");
    }
}
