use serde_json::Value;

/// Canonical JSON serialization per Matrix specification.
///
/// Key differences from `serde_json::to_string`:
/// - Object keys are sorted lexicographically
/// - No whitespace between tokens
/// - Strings are escaped per Matrix canonical JSON rules (U+2028, U+2029, U+FFFD)
/// - Numbers are serialized without trailing `.0` for integer-valued floats
pub fn canonical_json(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => {
            if *b {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        Value::Number(n) => format_canonical_number(n),
        Value::String(s) => escape_canonical_string(s),
        Value::Array(arr) => {
            let mut out = String::from("[");
            let mut first = true;
            for v in arr {
                if !first {
                    out.push(',');
                }
                first = false;
                out.push_str(&canonical_json(v));
            }
            out.push(']');
            out
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = String::from("{");
            let mut first = true;
            for k in keys {
                if !first {
                    out.push(',');
                }
                first = false;
                out.push_str(&escape_canonical_string(k));
                out.push(':');
                if let Some(v) = map.get(k) {
                    out.push_str(&canonical_json(v));
                } else {
                    out.push_str("null");
                }
            }
            out.push('}');
            out
        }
    }
}

/// Canonical JSON as bytes (for signing).
pub fn canonical_json_bytes(value: &Value) -> Vec<u8> {
    canonical_json(value).into_bytes()
}

/// Escape a string value per Matrix Canonical JSON specification.
fn escape_canonical_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\x08' => out.push_str("\\b"),
            '\x0c' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            '\u{fffd}' => out.push_str("\\ufffd"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Format a JSON number per Matrix Canonical JSON specification.
fn format_canonical_number(n: &serde_json::Number) -> String {
    if n.is_i64() || n.is_u64() {
        return n.to_string();
    }
    if let Some(f) = n.as_f64() {
        if f.fract() == 0.0 && f.is_finite() {
            return format!("{}", f as i64);
        }
        return format!("{f}");
    }
    n.to_string()
}

/// Remove `signatures` and `unsigned` fields from a JSON value (in-place).
pub fn remove_signatures_and_unsigned(value: &mut Value) {
    if let Some(obj) = value.as_object_mut() {
        obj.remove("signatures");
        obj.remove("unsigned");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canonical_json_sorts_keys() {
        let json = serde_json::json!({
            "z_key": 1,
            "a_key": 2,
            "m_key": 3
        });
        let canonical = canonical_json(&json);
        assert_eq!(canonical, r#"{"a_key":2,"m_key":3,"z_key":1}"#);
    }

    #[test]
    fn test_canonical_json_nested() {
        let json = serde_json::json!({
            "outer": {"z": 1, "a": 2},
            "inner": [3, 4]
        });
        let canonical = canonical_json(&json);
        assert_eq!(canonical, r#"{"inner":[3,4],"outer":{"a":2,"z":1}}"#);
    }

    #[test]
    fn test_canonical_json_string_escaping() {
        let json = serde_json::json!({
            "key": "value with \"quotes\""
        });
        let canonical = canonical_json(&json);
        assert_eq!(canonical, r#"{"key":"value with \"quotes\""}"#);
    }

    #[test]
    fn test_canonical_json_primitives() {
        assert_eq!(canonical_json(&Value::Null), "null");
        assert_eq!(canonical_json(&Value::Bool(true)), "true");
        assert_eq!(canonical_json(&Value::Bool(false)), "false");
        assert_eq!(canonical_json(&serde_json::json!(42)), "42");
        assert_eq!(canonical_json(&serde_json::json!("hello")), r#""hello""#);
    }

    #[test]
    fn test_escape_unicode_line_separator() {
        let input = "\u{2028}foo";
        let json = serde_json::json!({ "key": input });
        let canonical = canonical_json(&json);
        assert_eq!(canonical, r#"{"key":"\u2028foo"}"#);
    }

    #[test]
    fn test_escape_unicode_paragraph_separator() {
        let input = "bar\u{2029}";
        let json = serde_json::json!({ "key": input });
        let canonical = canonical_json(&json);
        assert_eq!(canonical, r#"{"key":"bar\u2029"}"#);
    }

    #[test]
    fn test_escape_replacement_character() {
        let input = "a\u{fffd}b";
        let json = serde_json::json!({ "key": input });
        let canonical = canonical_json(&json);
        assert_eq!(canonical, r#"{"key":"a\ufffdb"}"#);
    }

    #[test]
    fn test_canonical_number_integer() {
        let json = serde_json::json!({"val": 42});
        let canonical = canonical_json(&json);
        assert_eq!(canonical, r#"{"val":42}"#);
    }

    #[test]
    fn test_canonical_number_negative() {
        let json = serde_json::json!({"val": -1});
        let canonical = canonical_json(&json);
        assert_eq!(canonical, r#"{"val":-1}"#);
    }

    #[test]
    fn test_remove_signatures_and_unsigned() {
        let mut json = serde_json::json!({
            "user_id": "@test:example.com",
            "signatures": {"key": "sig"},
            "unsigned": {"age": 10}
        });
        remove_signatures_and_unsigned(&mut json);
        assert!(json.get("signatures").is_none());
        assert!(json.get("unsigned").is_none());
        assert!(json.get("user_id").is_some());
    }

    #[test]
    fn test_canonical_json_bytes_matches_string() {
        let json = serde_json::json!({"a": 1});
        assert_eq!(canonical_json_bytes(&json), canonical_json(&json).into_bytes());
    }
}
