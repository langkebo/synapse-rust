pub use synapse_storage::federation_blacklist::*;

#[cfg(test)]
mod tests {
    use super::{
        decode_federation_blacklist_cursor, encode_federation_blacklist_cursor, FederationBlacklist,
        FederationBlacklistCursor,
    };

    #[test]
    fn root_federation_blacklist_storage_reexport_keeps_cursor_round_trip() {
        let cursor =
            FederationBlacklistCursor { created_ts: 1_746_700_000_000, server_name: "matrix.example.com".to_string() };

        let encoded = encode_federation_blacklist_cursor(&cursor);
        assert_eq!(decode_federation_blacklist_cursor(Some(&encoded)), Some(cursor));
    }

    #[test]
    fn root_federation_blacklist_storage_reexport_keeps_entry_shape() {
        let blacklist = FederationBlacklist {
            id: 1,
            server_name: "evil-server.com".to_string(),
            block_type: "blacklist".to_string(),
            reason: Some("Malicious activity".to_string()),
            blocked_by: "@admin:example.com".to_string(),
            created_ts: 1234567890,
            updated_ts: 1234567891,
            expires_at: Some(1234567990),
            is_enabled: true,
            metadata: serde_json::json!({"source": "admin"}),
        };

        assert_eq!(blacklist.server_name, "evil-server.com");
        assert_eq!(blacklist.block_type, "blacklist");
        assert!(blacklist.is_enabled);
        assert_eq!(blacklist.metadata["source"], "admin");
    }
}
