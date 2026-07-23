use super::{
    decode_conversation_cursor, decode_generation_cursor, decode_message_cursor, encode_conversation_cursor,
    encode_generation_cursor, encode_message_cursor, ConversationCursor, GenerationCursor, MessageCursor,
};

#[test]
fn conversation_cursor_round_trip() {
    let cursor = ConversationCursor { is_pinned: true, updated_ts: 1_746_700_000_000, id: 42 };
    let encoded = encode_conversation_cursor(&cursor);
    assert_eq!(decode_conversation_cursor(Some(&encoded)), Some(cursor));
}

#[test]
fn generation_cursor_round_trip() {
    let cursor = GenerationCursor { created_ts: 1_746_700_000_000, id: 42 };
    let encoded = encode_generation_cursor(&cursor);
    assert_eq!(decode_generation_cursor(Some(&encoded)), Some(cursor));
}

#[test]
fn message_cursor_round_trip() {
    let cursor = MessageCursor { created_ts: 1_746_700_000_000, id: 42 };
    let encoded = encode_message_cursor(&cursor);
    assert_eq!(decode_message_cursor(Some(&encoded)), Some(cursor));
}

#[test]
fn openclaw_cursor_rejects_invalid_values() {
    assert_eq!(decode_conversation_cursor(Some("bad")), None);
    assert_eq!(decode_generation_cursor(Some("bad")), None);
    assert_eq!(decode_message_cursor(Some("bad")), None);
    assert_eq!(decode_generation_cursor(Some("123|")), None);
}
