//! MSC1763 Extensible Events — text extraction utility.
//!
//! MSC1763 defines a new event format where a single logical message is
//! composed of multiple typed content blocks (`m.message`, `m.file`,
//! `m.image`, `m.emote`, …). This module provides pure-function helpers
//! to extract a plain-text representation from such events, which is
//! required for:
//!
//! - Push notification previews
//! - Room list "last message" previews
//! - Search indexing
//! - Legacy client fallback rendering
//!
//! The helper is intentionally permissive about input shape: it accepts
//! both legacy `m.room.message` events (with `body`) and MSC1763
//! extensible events (with `m.message` array). When no textual
//! representation can be extracted, it returns an empty string
//! (fail-closed: callers treat empty as "no preview available").

use serde_json::Value;

/// Extract a plain-text representation from a Matrix event content object.
///
/// Resolution order (first non-empty wins):
/// 1. MSC1763 `m.message` array — first entry's `m.text` field
/// 2. MSC1763 `m.emote` array — first entry's `m.text` field (prefixed
///    with `* ` to mirror the legacy `m.emote` msgtype convention)
/// 3. MSC1763 `m.file.name` / `m.image.name` / `m.video.name` /
///    `m.audio.name` — file name fallback when no caption is present
/// 4. Legacy `body` field (m.room.message compat)
///
/// Returns an empty string when no textual representation can be
/// extracted. Callers MUST treat empty as "no preview available" rather
/// than substituting a placeholder (fail-closed).
pub fn extract_text_from_event_content(content: &Value) -> String {
    // MSC1763: m.message array → first entry's m.text
    if let Some(text) = extract_first_text_from_array(content, "m.message") {
        return text;
    }

    // MSC1763: m.emote array → first entry's m.text, prefixed with "* "
    if let Some(text) = extract_first_text_from_array(content, "m.emote") {
        return format!("* {}", text);
    }

    // MSC1763: file/image/video/audio name fallback
    for field in &["m.file", "m.image", "m.video", "m.audio"] {
        if let Some(name) = content.get(*field).and_then(|v| v.get("name")).and_then(|v| v.as_str()) {
            if !name.is_empty() {
                return name.to_string();
            }
        }
    }

    // Legacy m.room.message: body field
    if let Some(body) = content.get("body").and_then(|v| v.as_str()) {
        if !body.is_empty() {
            return body.to_string();
        }
    }

    String::new()
}

/// Helper: extract the first `m.text` from an MSC1763 array field.
///
/// MSC1763 array fields (`m.message`, `m.emote`) are arrays of objects
/// where each object may contain `m.text` and/or `m.html` representations
/// of the same content. We prefer `m.text` (plain) over `m.html` to keep
/// previews readable in push notifications and plain-text contexts.
fn extract_first_text_from_array(content: &Value, field: &str) -> Option<String> {
    let array = content.get(field)?.as_array()?;
    for entry in array {
        // Prefer plain text over HTML.
        if let Some(text) = entry.get("m.text").and_then(|v| v.as_str()) {
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -----------------------------------------------------------------------
    // Legacy m.room.message compatibility
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_text_from_legacy_message_body() {
        let content = json!({"body": "hello world", "msgtype": "m.text"});
        assert_eq!(extract_text_from_event_content(&content), "hello world");
    }

    #[test]
    fn test_extract_text_from_legacy_notice_body() {
        // m.notice events carry the same body field as m.text.
        let content = json!({"body": "system notice", "msgtype": "m.notice"});
        assert_eq!(extract_text_from_event_content(&content), "system notice");
    }

    #[test]
    fn test_extract_text_from_legacy_emote_body() {
        // Legacy m.emote body is NOT prefixed with "* " — that convention
        // is only applied to MSC1763 m.emote arrays. Legacy clients
        // already render "* " themselves based on msgtype.
        let content = json!({"body": "waves", "msgtype": "m.emote"});
        assert_eq!(extract_text_from_event_content(&content), "waves");
    }

    // -----------------------------------------------------------------------
    // MSC1763 m.message array
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_text_from_msc1763_message_array() {
        let content = json!({
            "m.message": [
                {"m.text": "hi from MSC1763"}
            ]
        });
        assert_eq!(extract_text_from_event_content(&content), "hi from MSC1763");
    }

    #[test]
    fn test_extract_text_prefers_plain_text_over_html() {
        // When both m.text and m.html are present in the same entry,
        // prefer the plain text representation for preview rendering.
        let content = json!({
            "m.message": [
                {"m.html": "<b>bold text</b>", "m.text": "bold text"}
            ]
        });
        assert_eq!(extract_text_from_event_content(&content), "bold text");
    }

    #[test]
    fn test_extract_text_skips_empty_entries_in_message_array() {
        // The first entry may have only m.html (no m.text); we should
        // skip it and continue scanning for a plain-text entry.
        let content = json!({
            "m.message": [
                {"m.html": "<i>html only</i>"},
                {"m.text": "plain fallback"}
            ]
        });
        assert_eq!(extract_text_from_event_content(&content), "plain fallback");
    }

    #[test]
    fn test_extract_text_returns_empty_when_message_array_has_only_html() {
        // Fail-closed: if no plain text is available anywhere in the
        // m.message array, return empty rather than falling back to HTML
        // (which would produce noisy previews with tags).
        let content = json!({
            "m.message": [
                {"m.html": "<b>only html</b>"}
            ]
        });
        assert_eq!(extract_text_from_event_content(&content), "");
    }

    // -----------------------------------------------------------------------
    // MSC1763 m.emote array
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_text_from_msc1763_emote_array() {
        // MSC1763 m.emote is rendered with "* " prefix to mirror the
        // legacy /me convention used by IRC and Matrix clients.
        let content = json!({
            "m.emote": [
                {"m.text": "waves hello"}
            ]
        });
        assert_eq!(extract_text_from_event_content(&content), "* waves hello");
    }

    // -----------------------------------------------------------------------
    // MSC1763 file/image/video/audio name fallback
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_text_from_msc1763_file_name() {
        let content = json!({
            "m.file": {
                "name": "report.pdf",
                "url": "mxc://server/mediaid",
                "mimetype": "application/pdf"
            }
        });
        assert_eq!(extract_text_from_event_content(&content), "report.pdf");
    }

    #[test]
    fn test_extract_text_from_msc1763_image_name() {
        let content = json!({
            "m.image": {
                "name": "vacation.jpg",
                "url": "mxc://server/mediaid"
            }
        });
        assert_eq!(extract_text_from_event_content(&content), "vacation.jpg");
    }

    #[test]
    fn test_extract_text_prefers_message_over_file_name() {
        // When both m.message (caption) and m.file are present, the
        // caption should win — it's the user-visible description.
        let content = json!({
            "m.message": [{"m.text": "check this out"}],
            "m.file": {"name": "doc.pdf"}
        });
        assert_eq!(extract_text_from_event_content(&content), "check this out");
    }

    // -----------------------------------------------------------------------
    // Fail-closed behaviour
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_text_returns_empty_for_missing_content() {
        let content = json!({});
        assert_eq!(extract_text_from_event_content(&content), "");
    }

    #[test]
    fn test_extract_text_returns_empty_for_null() {
        assert_eq!(extract_text_from_event_content(&Value::Null), "");
    }

    #[test]
    fn test_extract_text_returns_empty_for_empty_body() {
        // Empty body should not produce an empty-string preview that
        // could mask other fallback fields.
        let content = json!({"body": "", "msgtype": "m.text"});
        assert_eq!(extract_text_from_event_content(&content), "");
    }

    #[test]
    fn test_extract_text_returns_empty_for_empty_message_array() {
        let content = json!({"m.message": []});
        assert_eq!(extract_text_from_event_content(&content), "");
    }

    #[test]
    fn test_extract_text_returns_empty_for_array_with_empty_text() {
        let content = json!({"m.message": [{"m.text": ""}]});
        assert_eq!(extract_text_from_event_content(&content), "");
    }
}
