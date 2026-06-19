use serde_json::Value;

/// Canonical JSON serialization per Matrix specification.
///
/// Key differences from `serde_json::to_string`:
/// - Object keys are sorted lexicographically
/// - No whitespace between tokens
/// - Strings are escaped per Matrix canonical JSON rules (U+2028, U+2029, U+FFFD)
/// - Numbers must be integers in the range `[-(2^53)+1, 2^53-1]`; non-integer
///   floats and out-of-range integers are rejected. Integer-valued floats
///   (e.g. `1.0`) are converted to their integer form for compatibility with
///   upstream Synapse canonicaljson behavior.
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
/// - Integer-valued floats (e.g. `1.0`) are converted to their integer form,
///   matching upstream Synapse canonicaljson behavior.
/// - Non-integer floats (e.g. `1.5`), non-finite floats, and out-of-range
///   integers are rejected.
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
        if f.fract() != 0.0 {
            return Err(CanonicalJsonError::FloatNotAllowed(f));
        }
        // Integer-valued float: convert to integer (matches Synapse canonicaljson).
        let i = f as i64;
        if !(MIN_CANONICAL_INT..=MAX_CANONICAL_INT).contains(&i) {
            return Err(CanonicalJsonError::IntegerOutOfRange(i as i128));
        }
        return Ok(i.to_string());
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
    /// A non-integer or non-finite float was encountered. Matrix canonical JSON
    /// only permits integers (integer-valued floats are auto-converted).
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
    fn test_canonical_number_integer_valued_float_converted() {
        // Integer-valued floats are converted to integers (matches Synapse canonicaljson).
        let json: Value = serde_json::from_str(r#"{"val": 1.0}"#).unwrap();
        let canonical = canonical_json(&json).unwrap();
        assert_eq!(canonical, r#"{"val":1}"#);
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
}
