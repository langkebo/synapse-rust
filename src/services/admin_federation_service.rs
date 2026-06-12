pub use synapse_services::admin_federation_service::*;

#[cfg(test)]
mod tests {
    use super::{
        decode_destination_cursor, decode_pending_federation_cursor, encode_destination_cursor,
        encode_pending_federation_cursor, ConfirmFederationResult, DestinationCursor, PendingFederationCursor,
    };

    #[test]
    fn root_admin_federation_service_reexport_keeps_destination_cursor_round_trip() {
        let cursor = DestinationCursor { server_name: "matrix.example.com".to_string() };
        let encoded = encode_destination_cursor(&cursor);
        assert_eq!(decode_destination_cursor(Some(&encoded)), Some(cursor));
    }

    #[test]
    fn root_admin_federation_service_reexport_keeps_pending_cursor_round_trip() {
        let cursor = PendingFederationCursor {
            updated_ts: 1_700_000_000_000,
            server_name: "matrix.example.com".to_string(),
        };
        let encoded = encode_pending_federation_cursor(&cursor);
        assert_eq!(decode_pending_federation_cursor(Some(&encoded)), Some(cursor));
    }

    #[test]
    fn root_admin_federation_service_reexport_keeps_result_shape() {
        let result = ConfirmFederationResult {
            status: "active".to_string(),
            previous_status: "pending".to_string(),
            updated_ts: 1_700_000_000_000,
        };

        let json = serde_json::to_value(&result).expect("serialize confirm result");
        assert_eq!(json.get("status").and_then(serde_json::Value::as_str), Some("active"));
        assert_eq!(json.get("previous_status").and_then(serde_json::Value::as_str), Some("pending"));
    }
}
