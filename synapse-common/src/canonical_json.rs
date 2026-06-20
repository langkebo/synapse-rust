use serde_json::Value;

/// Canonical JSON serialization per Matrix specification.
///
/// Key differences from `serde_json::to_string`:
/// - Object keys are sorted lexicographically
/// - No whitespace between tokens
/// - Strings are escaped per Matrix canonical JSON rules (U+2028, U+2029, U+FFFD)
/// - Numbers must be integers in the range `[-(2^53)+1, 2^53-1]`; all floats
///   and out-of-range integers are rejected.
pub fn canonical_json(value: &Value) -> Result<String, CanonicalJsonError> {
    match value {
        Value::Null => Ok("null".to_string()),
        Value::Bool(b) => Ok(if *b { "true".to_string() } else { "false".to_string() }),
        Value::Number(n) => Ok(format_canonical_number(n)?),
        Value::String(s) => Ok(escape_canonical_string(s)),
        Value::Array(arr) => {
            let mut out = String::from("[");
            let mut first = true;
            for v in arr {
                if !first {
                    out.push(',');
                }
                first = false;
                out.push_str(&canonical_json(v)?);
            }
            out.push(']');
            Ok(out)
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
                    out.push_str(&canonical_json(v)?);
                } else {
                    out.push_str("null");
                }
            }
            out.push('}');
            Ok(out)
        }
    }
}

/// Canonical JSON as bytes (for signing).
pub fn canonical_json_bytes(value: &Value) -> Result<Vec<u8>, CanonicalJsonError> {
    Ok(canonical_json(value)?.into_bytes())
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

/// Minimum permitted integer value per Matrix canonical JSON: `-(2^53)+1`.
const MIN_CANONICAL_INT: i64 = -(2_i64.pow(53)) + 1;
/// Maximum permitted integer value per Matrix canonical JSON: `2^53-1`.
const MAX_CANONICAL_INT: i64 = 2_i64.pow(53) - 1;

/// Format a JSON number per Matrix Canonical JSON specification.
///
/// - `i64`/`u64` integers are accepted if within `[-(2^53)+1, 2^53-1]`.
/// - All floats (including integer-valued floats such as `1.0`) are rejected.
/// - Out-of-range integers are rejected.
fn format_canonical_number(n: &serde_json::Number) -> Result<String, CanonicalJsonError> {
    if let Some(i) = n.as_i64() {
        if !(MIN_CANONICAL_INT..=MAX_CANONICAL_INT).contains(&i) {
            return Err(CanonicalJsonError::IntegerOutOfRange(i as i128));
        }
        return Ok(i.to_string());
    }
    if let Some(u) = n.as_u64() {
        if (u as i128) > MAX_CANONICAL_INT as i128 {
            return Err(CanonicalJsonError::IntegerOutOfRange(u as i128));
        }
        return Ok(u.to_string());
    }
    if let Some(f) = n.as_f64() {
        if !f.is_finite() {
            return Err(CanonicalJsonError::FloatNotAllowed(f));
        }
        return Err(CanonicalJsonError::FloatNotAllowed(f));
    }
    // serde_json::Number should always be i64, u64, or f64. If we reach here,
    // the number is malformed.
    Err(CanonicalJsonError::InvalidNumber)
}

/// Remove `signatures` and `unsigned` fields from a JSON value (in-place).
pub fn remove_signatures_and_unsigned(value: &mut Value) {
    if let Some(obj) = value.as_object_mut() {
        obj.remove("signatures");
        obj.remove("unsigned");
    }
}

/// Errors that can occur during canonical JSON serialization.
#[derive(Debug, thiserror::Error)]
pub enum CanonicalJsonError {
    /// A float was encountered. Matrix canonical JSON only permits integers.
    #[error("Floats are not permitted in canonical JSON: {0}")]
    FloatNotAllowed(f64),

    /// An integer was outside the permitted range `[-(2^53)+1, 2^53-1]`.
    #[error("Integer {0} is out of range [-(2^53)+1, 2^53-1]")]
    IntegerOutOfRange(i128),

    /// The JSON number could not be represented as i64, u64, or f64.
    #[error("Invalid JSON number")]
    InvalidNumber,
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
        let canonical = canonical_json(&json).unwrap();
        assert_eq!(canonical, r#"{"a_key":2,"m_key":3,"z_key":1}"#);
    }

    #[test]
    fn test_canonical_json_nested() {
        let json = serde_json::json!({
            "outer": {"z": 1, "a": 2},
            "inner": [3, 4]
        });
        let canonical = canonical_json(&json).unwrap();
        assert_eq!(canonical, r#"{"inner":[3,4],"outer":{"a":2,"z":1}}"#);
    }

    #[test]
    fn test_canonical_json_string_escaping() {
        let json = serde_json::json!({
            "key": "value with \"quotes\""
        });
        let canonical = canonical_json(&json).unwrap();
        assert_eq!(canonical, r#"{"key":"value with \"quotes\""}"#);
    }

    #[test]
    fn test_canonical_json_primitives() {
        assert_eq!(canonical_json(&Value::Null).unwrap(), "null");
        assert_eq!(canonical_json(&Value::Bool(true)).unwrap(), "true");
        assert_eq!(canonical_json(&Value::Bool(false)).unwrap(), "false");
        assert_eq!(canonical_json(&serde_json::json!(42)).unwrap(), "42");
        assert_eq!(canonical_json(&serde_json::json!("hello")).unwrap(), r#""hello""#);
    }

    #[test]
    fn test_escape_unicode_line_separator() {
        let input = "\u{2028}foo";
        let json = serde_json::json!({ "key": input });
        let canonical = canonical_json(&json).unwrap();
        assert_eq!(canonical, r#"{"key":"\u2028foo"}"#);
    }

    #[test]
    fn test_escape_unicode_paragraph_separator() {
        let input = "bar\u{2029}";
        let json = serde_json::json!({ "key": input });
        let canonical = canonical_json(&json).unwrap();
        assert_eq!(canonical, r#"{"key":"bar\u2029"}"#);
    }

    #[test]
    fn test_escape_replacement_character() {
        let input = "a\u{fffd}b";
        let json = serde_json::json!({ "key": input });
        let canonical = canonical_json(&json).unwrap();
        assert_eq!(canonical, r#"{"key":"a\ufffdb"}"#);
    }

    #[test]
    fn test_canonical_number_integer() {
        let json = serde_json::json!({"val": 42});
        let canonical = canonical_json(&json).unwrap();
        assert_eq!(canonical, r#"{"val":42}"#);
    }

    #[test]
    fn test_canonical_number_negative() {
        let json = serde_json::json!({"val": -1});
        let canonical = canonical_json(&json).unwrap();
        assert_eq!(canonical, r#"{"val":-1}"#);
    }

    #[test]
    fn test_canonical_number_integer_valued_float_rejected() {
        let json: Value = serde_json::from_str(r#"{"val": 1.0}"#).unwrap();
        let result = canonical_json(&json);
        assert!(matches!(result, Err(CanonicalJsonError::FloatNotAllowed(_))));
    }

    #[test]
    fn test_canonical_number_non_integer_float_rejected() {
        let json: Value = serde_json::from_str(r#"{"val": 1.5}"#).unwrap();
        let result = canonical_json(&json);
        assert!(matches!(result, Err(CanonicalJsonError::FloatNotAllowed(_))));
    }

    #[test]
    fn test_canonical_number_out_of_range_rejected() {
        // 2^53 is out of range (max is 2^53 - 1)
        let json: Value = serde_json::from_str(r#"{"val": 9007199254740992}"#).unwrap();
        let result = canonical_json(&json);
        assert!(matches!(result, Err(CanonicalJsonError::IntegerOutOfRange(_))));
    }

    #[test]
    fn test_canonical_number_max_range_accepted() {
        // 2^53 - 1 is the max permitted value
        let json: Value = serde_json::from_str(r#"{"val": 9007199254740991}"#).unwrap();
        let canonical = canonical_json(&json).unwrap();
        assert_eq!(canonical, r#"{"val":9007199254740991}"#);
    }

    #[test]
    fn test_canonical_number_min_range_accepted() {
        // -(2^53)+1 is the min permitted value
        let json: Value = serde_json::from_str(r#"{"val": -9007199254740991}"#).unwrap();
        let canonical = canonical_json(&json).unwrap();
        assert_eq!(canonical, r#"{"val":-9007199254740991}"#);
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
        assert_eq!(canonical_json_bytes(&json).unwrap(), canonical_json(&json).unwrap().into_bytes());
    }

    // =========================================================================
    // Matrix Specification Test Vectors
    //
    // These vectors are derived from the Matrix specification appendices
    // (Canonical JSON) and the sytest conformance suite. They serve as a
    // conformance gate to ensure the canonical JSON implementation matches
    // the reference behavior expected by other Matrix homeservers (Synapse,
    // Dendrite, Conduit) for event signing, federation request signing, and
    // server key signing.
    //
    // References:
    // - Matrix Spec v1.18 § Appendices → Canonical JSON
    // - sytest: tests/50federation/40canonicaljson.pl
    // - Synapse #19739 (Rust canonical JSON serializer)
    // =========================================================================

    /// Spec vector: empty object → `{}`
    #[test]
    fn spec_vector_empty_object() {
        let json = serde_json::json!({});
        assert_eq!(canonical_json(&json).unwrap(), "{}");
    }

    /// Spec vector: empty array → `[]`
    #[test]
    fn spec_vector_empty_array() {
        let json = serde_json::json!([]);
        assert_eq!(canonical_json(&json).unwrap(), "[]");
    }

    /// Spec vector: keys must be sorted lexicographically by Unicode code point.
    /// Uppercase letters (A-Z, U+0041-005A) sort before lowercase (a-z, U+0061-007A).
    #[test]
    fn spec_vector_key_sorting_codepoint_order() {
        let json = serde_json::json!({"b": 2, "A": 1, "a": 3, "B": 4});
        let canonical = canonical_json(&json).unwrap();
        // Code point order: A(0x41) < B(0x42) < a(0x61) < b(0x62)
        assert_eq!(canonical, r#"{"A":1,"B":4,"a":3,"b":2}"#);
    }

    /// Spec vector: nested objects must also have sorted keys.
    #[test]
    fn spec_vector_nested_key_sorting() {
        let json = serde_json::json!({"outer": {"z": 1, "a": 2, "m": 3}});
        let canonical = canonical_json(&json).unwrap();
        assert_eq!(canonical, r#"{"outer":{"a":2,"m":3,"z":1}}"#);
    }

    /// Spec vector: arrays preserve insertion order (no sorting).
    #[test]
    fn spec_vector_array_order_preserved() {
        let json = serde_json::json!([3, 1, 2]);
        let canonical = canonical_json(&json).unwrap();
        assert_eq!(canonical, "[3,1,2]");
    }

    /// Spec vector: no whitespace between tokens or around delimiters.
    #[test]
    fn spec_vector_no_whitespace() {
        let json = serde_json::json!({"a": [1, 2], "b": {"c": 3}});
        let canonical = canonical_json(&json).unwrap();
        assert_eq!(canonical, r#"{"a":[1,2],"b":{"c":3}}"#);
    }

    /// Spec vector: integer-valued floats must be rejected (not coerced).
    /// Matrix canonical JSON does not permit any float, even `1.0`.
    #[test]
    fn spec_vector_integer_valued_float_rejected() {
        let json: Value = serde_json::from_str(r#"{"a": 1.0}"#).unwrap();
        let result = canonical_json(&json);
        assert!(result.is_err(), "1.0 must be rejected as a float");
    }

    /// Spec vector: the maximum permitted integer `2^53 - 1 = 9007199254740991`.
    #[test]
    fn spec_vector_max_integer_accepted() {
        let json = serde_json::json!({"max": 9007199254740991_i64});
        let canonical = canonical_json(&json).unwrap();
        assert_eq!(canonical, r#"{"max":9007199254740991}"#);
    }

    /// Spec vector: the minimum permitted integer `-(2^53) + 1 = -9007199254740991`.
    #[test]
    fn spec_vector_min_integer_accepted() {
        let json = serde_json::json!({"min": -9007199254740991_i64});
        let canonical = canonical_json(&json).unwrap();
        assert_eq!(canonical, r#"{"min":-9007199254740991}"#);
    }

    /// Spec vector: `2^53` is out of range and must be rejected.
    #[test]
    fn spec_vector_above_max_integer_rejected() {
        let json = serde_json::json!({"over": 9007199254740992_i64});
        let result = canonical_json(&json);
        assert!(result.is_err(), "2^53 must be rejected as out of range");
    }

    /// Spec vector: `-(2^53)` is out of range and must be rejected.
    #[test]
    fn spec_vector_below_min_integer_rejected() {
        let json = serde_json::json!({"under": -9007199254740992_i64});
        let result = canonical_json(&json);
        assert!(result.is_err(), "-(2^53) must be rejected as out of range");
    }

    /// Spec vector: control characters (U+0000 to U+001F) must be escaped
    /// as `\u00XX`.
    #[test]
    fn spec_vector_control_chars_escaped() {
        let json = serde_json::json!({"key": "a\u{0000}b\u{001f}c"});
        let canonical = canonical_json(&json).unwrap();
        assert_eq!(canonical, r#"{"key":"a\u0000b\u001fc"}"#);
    }

    /// Spec vector: U+2028 (LINE SEPARATOR) must be escaped as `\u2028`.
    #[test]
    fn spec_vector_line_separator_escaped() {
        let json = serde_json::json!({"key": "a\u{2028}b"});
        let canonical = canonical_json(&json).unwrap();
        assert_eq!(canonical, r#"{"key":"a\u2028b"}"#);
    }

    /// Spec vector: U+2029 (PARAGRAPH SEPARATOR) must be escaped as `\u2029`.
    #[test]
    fn spec_vector_paragraph_separator_escaped() {
        let json = serde_json::json!({"key": "a\u{2029}b"});
        let canonical = canonical_json(&json).unwrap();
        assert_eq!(canonical, r#"{"key":"a\u2029b"}"#);
    }

    /// Spec vector: U+FFFD (REPLACEMENT CHARACTER) must be escaped as `\ufffd`.
    #[test]
    fn spec_vector_replacement_char_escaped() {
        let json = serde_json::json!({"key": "a\u{fffd}b"});
        let canonical = canonical_json(&json).unwrap();
        assert_eq!(canonical, r#"{"key":"a\ufffdb"}"#);
    }

    /// Spec vector: `signatures` and `unsigned` fields must be removable
    /// before computing the canonical form for signing.
    #[test]
    fn spec_vector_remove_signatures_and_unsigned() {
        let mut json = serde_json::json!({
            "content": {"body": "Hello"},
            "signatures": {"@user:server": {"ed25519:1": "sig"}},
            "unsigned": {"age_ts": 12345}
        });
        remove_signatures_and_unsigned(&mut json);
        let canonical = canonical_json(&json).unwrap();
        assert_eq!(canonical, r#"{"content":{"body":"Hello"}}"#);
    }

    /// Spec vector: deeply nested structure with mixed types.
    #[test]
    fn spec_vector_deep_nesting_mixed_types() {
        let json = serde_json::json!({
            "a": {"b": {"c": [1, "two", null, true, false]}},
            "d": 42
        });
        let canonical = canonical_json(&json).unwrap();
        assert_eq!(canonical, r#"{"a":{"b":{"c":[1,"two",null,true,false]}},"d":42}"#);
    }

    /// Spec vector: negative integers are preserved.
    #[test]
    fn spec_vector_negative_integer() {
        let json = serde_json::json!({"value": -42});
        let canonical = canonical_json(&json).unwrap();
        assert_eq!(canonical, r#"{"value":-42}"#);
    }

    /// Spec vector: zero is a valid integer.
    #[test]
    fn spec_vector_zero() {
        let json = serde_json::json!({"value": 0});
        let canonical = canonical_json(&json).unwrap();
        assert_eq!(canonical, r#"{"value":0}"#);
    }

    /// Spec vector: Unicode characters above U+FFFF are preserved (not escaped).
    /// Only U+2028, U+2029, U+FFFD and control chars are escaped.
    #[test]
    fn spec_vector_high_unicode_preserved() {
        let json = serde_json::json!({"emoji": "🎉"});
        let canonical = canonical_json(&json).unwrap();
        assert_eq!(canonical, r#"{"emoji":"🎉"}"#);
    }

    /// Spec vector: backslash and quote must be escaped.
    #[test]
    fn spec_vector_backslash_and_quote_escaped() {
        let json = serde_json::json!({"path": "C:\\Users\\test", "quote": "say \"hi\""});
        let canonical = canonical_json(&json).unwrap();
        assert_eq!(canonical, r#"{"path":"C:\\Users\\test","quote":"say \"hi\""}"#);
    }

    /// Spec vector: the canonical form of a signed event body must be
    /// deterministic regardless of input key order. This is the core
    /// invariant that makes event signatures verifiable across implementations.
    #[test]
    fn spec_vector_deterministic_across_key_orders() {
        let json_a = serde_json::json!({
            "content": {"body": "Hello"},
            "origin": "example.com",
            "sender": "@alice:example.com"
        });
        let json_b = serde_json::json!({
            "origin": "example.com",
            "sender": "@alice:example.com",
            "content": {"body": "Hello"}
        });
        let canonical_a = canonical_json(&json_a).unwrap();
        let canonical_b = canonical_json(&json_b).unwrap();
        assert_eq!(canonical_a, canonical_b, "canonical form must be deterministic");
    }
}
