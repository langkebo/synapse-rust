use chrono::{DateTime, Utc};

pub fn current_timestamp_millis() -> i64 {
    Utc::now().timestamp_millis()
}

pub fn current_timestamp_utc() -> DateTime<Utc> {
    Utc::now()
}

pub fn calculate_age(timestamp: i64) -> i64 {
    current_timestamp_millis().saturating_sub(timestamp)
}

pub fn generate_stream_token_from_ts(timestamp: Option<i64>) -> String {
    format!("t{}", timestamp.unwrap_or_else(current_timestamp_millis))
}

pub fn parse_stream_token(token: &str) -> Option<i64> {
    token.strip_prefix('t').and_then(|s| s.parse().ok())
}

pub fn is_expired(expires_at: Option<i64>) -> bool {
    expires_at.is_some_and(|exp| exp < current_timestamp_millis())
}

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
