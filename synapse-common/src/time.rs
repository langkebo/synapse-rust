//! Timestamp helpers (current ms / UTC, age calculation, pagination/stream token codec).

use chrono::{DateTime, Utc};

/// Currents the timestamp.
pub fn current_timestamp_millis() -> i64 {
    Utc::now().timestamp_millis()
}

/// Currents the timestamp.
pub fn current_timestamp_utc() -> DateTime<Utc> {
    Utc::now()
}

/// Calculates the age.
pub fn calculate_age(timestamp: i64) -> i64 {
    current_timestamp_millis().saturating_sub(timestamp)
}

/// Generates the stream.
pub fn generate_stream_token_from_ts(timestamp: Option<i64>) -> String {
    format!("t{}", timestamp.unwrap_or_else(current_timestamp_millis))
}

/// Parses stream token.
pub fn parse_stream_token(token: &str) -> Option<i64> {
    token.strip_prefix('t').and_then(|s| s.parse().ok())
}

/// Generate a composite `/messages` pagination token: `t{ts}` or
/// `t{ts}_{stream_ordering}`.
///
/// The `stream_ordering` suffix disambiguates events that share the same
/// `origin_server_ts` millisecond, so paginating across a page boundary no
/// longer skips same-millisecond events (ISSUE-06). Tokens without the
/// suffix remain valid and keep their legacy strict-timestamp semantics.
pub fn generate_pagination_token(ts: i64, stream_ordering: Option<i64>) -> String {
    match stream_ordering {
        Some(stream) => format!("t{ts}_{stream}"),
        None => format!("t{ts}"),
    }
}

/// Parse a `/messages` pagination token into `(origin_server_ts,
/// Option<stream_ordering>)`. Accepts both the composite `t{ts}_{stream}`
/// form and the legacy `t{ts}` form.
pub fn parse_pagination_token(token: &str) -> Option<(i64, Option<i64>)> {
    let rest = token.strip_prefix('t')?;
    match rest.split_once('_') {
        Some((ts_part, stream_part)) => {
            let ts = ts_part.parse().ok()?;
            let stream = stream_part.parse().ok()?;
            Some((ts, Some(stream)))
        }
        None => rest.parse().ok().map(|ts| (ts, None)),
    }
}

/// Returns true if expired.
pub fn is_expired(expires_at: Option<i64>) -> bool {
    expires_at.is_some_and(|exp| exp < current_timestamp_millis())
}

/// Calculates the ttl.
pub fn calculate_ttl(expires_at: Option<i64>) -> Option<i64> {
    expires_at.map(|exp| {
        let now = current_timestamp_millis();
        (exp - now).max(0)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pagination_token_composite() {
        assert_eq!(parse_pagination_token("t12345_678"), Some((12345, Some(678))));
    }

    #[test]
    fn test_parse_pagination_token_legacy() {
        assert_eq!(parse_pagination_token("t12345"), Some((12345, None)));
        assert_eq!(parse_pagination_token("12345"), None);
        assert_eq!(parse_pagination_token("invalid"), None);
        assert_eq!(parse_pagination_token(""), None);
        assert_eq!(parse_pagination_token("t12345_"), None);
        assert_eq!(parse_pagination_token("t12345_abc"), None);
    }

    #[test]
    fn test_generate_pagination_token() {
        assert_eq!(generate_pagination_token(12345, Some(678)), "t12345_678");
        assert_eq!(generate_pagination_token(12345, None), "t12345");
    }

    #[test]
    fn test_pagination_token_roundtrip() {
        let token = generate_pagination_token(9876543210, Some(42));
        assert_eq!(parse_pagination_token(&token), Some((9876543210, Some(42))));

        let legacy = generate_pagination_token(9876543210, None);
        assert_eq!(parse_pagination_token(&legacy), Some((9876543210, None)));
    }

    #[test]
    fn test_current_timestamp_millis() {
        let ts = current_timestamp_millis();
        assert!(ts > 0);
    }

    #[test]
    fn test_calculate_age() {
        let now = current_timestamp_millis();
        let age = calculate_age(now - 1000);
        assert!(age >= 1000);
    }

    #[test]
    fn test_generate_stream_token_from_ts() {
        let token = generate_stream_token_from_ts(Some(12345));
        assert_eq!(token, "t12345");

        let token = generate_stream_token_from_ts(None);
        assert!(token.starts_with('t'));
    }

    #[test]
    fn test_parse_stream_token() {
        assert_eq!(parse_stream_token("t12345"), Some(12345));
        assert_eq!(parse_stream_token("invalid"), None);
    }

    #[test]
    fn test_is_expired() {
        assert!(is_expired(Some(current_timestamp_millis() - 1000)));
        assert!(!is_expired(Some(current_timestamp_millis() + 10000)));
        assert!(!is_expired(None));
    }

    #[test]
    fn test_calculate_ttl() {
        let ttl = calculate_ttl(Some(current_timestamp_millis() + 5000));
        assert!(ttl.unwrap() > 4000);

        assert!(calculate_ttl(None).is_none());
    }

    #[test]
    fn test_calculate_ttl_expired() {
        let ttl = calculate_ttl(Some(current_timestamp_millis() - 10000));
        assert_eq!(ttl, Some(0));
    }

    #[test]
    fn test_parse_stream_token_edge_cases() {
        assert_eq!(parse_stream_token(""), None);
        assert_eq!(parse_stream_token("t"), None);
        assert_eq!(parse_stream_token("tabc"), None);
        assert_eq!(parse_stream_token("t-123"), Some(-123));
    }

    #[test]
    fn test_generate_stream_token_from_ts_none() {
        let token = generate_stream_token_from_ts(None);
        assert!(token.starts_with('t'));
        assert!(token.len() > 1);
    }

    #[test]
    fn test_current_timestamp_millis_monotonic() {
        let t1 = current_timestamp_millis();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let t2 = current_timestamp_millis();
        assert!(t2 >= t1, "timestamps must be monotonic");
    }

    #[test]
    fn test_calculate_age_near_zero() {
        let now = current_timestamp_millis();
        let age = calculate_age(now);
        // Age for the current timestamp should be very small (0 or 1 ms).
        assert!(age <= 1, "age for now should be near zero, got {age}");
    }

    #[test]
    fn test_stream_token_roundtrip() {
        let ts = 9876543210;
        let token = generate_stream_token_from_ts(Some(ts));
        let parsed = parse_stream_token(&token);
        assert_eq!(parsed, Some(ts));
    }

    #[test]
    fn test_is_expired_exactly_now() {
        let now = current_timestamp_millis();
        // `is_expired` uses `<`, so a timestamp equal to now is NOT expired.
        assert!(!is_expired(Some(now)), "exactly-now should not be expired");
    }
}
