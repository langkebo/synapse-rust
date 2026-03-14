use chrono::Utc;

pub fn current_timestamp_millis() -> i64 {
    Utc::now().timestamp_millis()
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
}
