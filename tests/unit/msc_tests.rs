// Unit tests for MSC features - standalone
// This is a simplified test module

#![cfg(test)]

mod qr_login_tests {
    #[test]
    fn test_transaction_id_format() {
        let transaction_id = format!("qr_{}", uuid::Uuid::new_v4());
        assert!(transaction_id.starts_with("qr_"));
        assert!(transaction_id.len() > 10);
    }

    #[test]
    fn test_qr_expiry_calculation() {
        let created_at = 1700000000000i64;
        let expires_in_ms = 5 * 60 * 1000;
        let expected = created_at + expires_in_ms;
        assert_eq!(expected, 1700000300000i64);
    }

    #[test]
    fn test_user_id_format() {
        let valid = ["@user:localhost", "@alice:example.com"];
        for user in valid {
            assert!(user.starts_with('@'));
            assert!(user.contains(':'));
        }
    }

    /// Both room-id forms are legal (C-3): v1–v11's `!opaque:server` **and**
    /// v12's domainless `!` + 43 unpadded URL-safe Base64 characters. This used
    /// to assert `room.contains(':')`, which is exactly the shape rule MSC4291
    /// removed — so it pinned the old grammar instead of the validator, and it
    /// passed even on a build that rejected every v12 id.
    #[test]
    fn test_room_id_format() {
        for room in ["!room:localhost", "!abc:example.com"] {
            assert!(synapse_common::room_id::is_well_formed_room_id(room), "legacy form must be legal: {room}");
        }
        // 43 characters, the sha256 of a reference hash in unpadded Base64.
        let domainless = "!31hneApxJ_1o-63DmFrpeqnkFfWppnzWso1JvH3ogLM";
        assert_eq!(domainless.len(), 44, "fixture must be `!` + 43 characters");
        assert!(synapse_common::room_id::is_well_formed_room_id(domainless), "v12 form must be legal");
        for invalid in ["!x", "room:localhost", "!room", "!:localhost", ""] {
            assert!(!synapse_common::room_id::is_well_formed_room_id(invalid), "must be rejected: {invalid:?}");
        }
    }
}

mod invite_blocklist_tests {
    #[test]
    fn test_user_id_validation() {
        let valid = ["@user:localhost", "@user:example.com"];
        for user in valid {
            assert!(user.starts_with('@'));
            assert!(user.contains(':'));
        }
    }

    /// See [`super::qr_login_tests::test_room_id_format`]: the `:` requirement is
    /// the pre-MSC4291 grammar, so this now exercises the shared validator on
    /// both legal forms instead of asserting the old shape.
    #[test]
    fn test_room_id_validation() {
        for room in ["!room:localhost", "!room:example.com"] {
            assert!(synapse_common::room_id::is_well_formed_room_id(room), "legacy form must be legal: {room}");
        }
        assert!(synapse_common::room_id::is_well_formed_room_id("!31hneApxJ_1o-63DmFrpeqnkFfWppnzWso1JvH3ogLM"));
        assert!(!synapse_common::room_id::is_well_formed_room_id("!x"));
    }
}

mod sticky_event_tests {
    #[test]
    fn test_event_type_validation() {
        let valid = ["m.room.message", "m.room.topic", "m.room.avatar"];
        for et in valid {
            assert!(et.starts_with("m.") || et.starts_with("com."));
        }
    }
}

mod common_tests {
    #[test]
    fn test_server_name_validation() {
        let valid = ["localhost", "example.com", "matrix.org"];
        for name in valid {
            assert!(!name.is_empty());
            assert!(!name.contains(' '));
        }
    }

    #[test]
    fn test_port_validation() {
        let valid = [80, 443, 8080, 8448];
        for port in valid {
            assert!(port > 0 && port <= 65535);
        }
    }
}
