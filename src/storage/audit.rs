pub use synapse_storage::audit::*;

#[cfg(test)]
mod cursor_tests {
    use super::{decode_audit_event_cursor, encode_audit_event_cursor, AuditEventCursor};

    #[test]
    fn audit_event_cursor_round_trip() {
        let cursor = AuditEventCursor { created_ts: 1_746_700_000_000, event_id: "evt-123".to_string() };

        let encoded = encode_audit_event_cursor(&cursor);
        assert_eq!(decode_audit_event_cursor(Some(&encoded)), Some(cursor));
    }

    #[test]
    fn audit_event_cursor_rejects_invalid_values() {
        assert_eq!(decode_audit_event_cursor(None), None);
        assert_eq!(decode_audit_event_cursor(Some("bad")), None);
        assert_eq!(decode_audit_event_cursor(Some("123|")), None);
    }
}
