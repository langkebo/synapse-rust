use super::*;

#[test]
fn server_notification_cursor_round_trip() {
    let cursor = ServerNotificationCursor { created_ts: 1_700_000_000_000, id: 7 };
    let encoded = encode_server_notification_cursor(&cursor);
    assert_eq!(decode_server_notification_cursor(Some(&encoded)), Some(cursor));
}

#[test]
fn server_notification_cursor_rejects_invalid_value() {
    assert_eq!(decode_server_notification_cursor(Some("bad-cursor")), None);
    assert_eq!(decode_server_notification_cursor(Some("123|")), None);
}
