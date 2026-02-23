#[cfg(test)]
mod regex_cache_integration_tests {
    use synapse_rust::regex_cache::RegexCache;

    #[test]
    fn test_regex_cache_multiple_patterns() {
        let cache = RegexCache::new();

        let patterns = [r"\d+",
            r"[a-z]+",
            r"\w+",
            r"\d{4}-\d{2}-\d{2}",
            r"[A-Z][a-z]+"];

        let test_strings = ["123", "abc", "test_123", "2023-10-27", "Hello"];

        for (pattern, test_str) in patterns.iter().zip(test_strings.iter()) {
            let regex = cache.get_or_create(pattern).unwrap();
            assert!(regex.is_match(test_str));
        }

        assert_eq!(cache.len(), patterns.len());
    }

    #[test]
    fn test_regex_cache_is_match() {
        let cache = RegexCache::new();

        assert!(cache.is_match(r"\d+", "123").unwrap());
        assert!(!cache.is_match(r"\d+", "abc").unwrap());
        assert!(cache.is_match(r"[a-z]+", "hello").unwrap());
        assert!(!cache.is_match(r"[a-z]+", "HELLO").unwrap());
        assert!(cache.is_match(r"\w+", "test_123").unwrap());
    }

    #[test]
    fn test_regex_cache_clear() {
        let cache = RegexCache::new();

        cache.get_or_create(r"\d+").unwrap();
        cache.get_or_create(r"[a-z]+").unwrap();
        assert_eq!(cache.len(), 2);

        cache.clear();
        assert_eq!(cache.len(), 0);

        cache.get_or_create(r"\d+").unwrap();
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_regex_cache_clone() {
        let cache1 = RegexCache::new();
        cache1.get_or_create(r"\d+").unwrap();
        cache1.get_or_create(r"[a-z]+").unwrap();

        let cache2 = cache1.clone();
        assert_eq!(cache2.len(), 2);

        assert!(cache2.is_match(r"\d+", "123").unwrap());
    }

    #[test]
    fn test_regex_cache_email_validation() {
        let cache = RegexCache::new();
        let email_pattern = r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$";

        assert!(cache.is_match(email_pattern, "test@example.com").unwrap());
        assert!(cache
            .is_match(email_pattern, "user.name@domain.co.uk")
            .unwrap());
        assert!(!cache.is_match(email_pattern, "invalid-email").unwrap());
        assert!(!cache.is_match(email_pattern, "@missing-local.com").unwrap());
        assert!(!cache.is_match(email_pattern, "missing@domain").unwrap());
    }

    #[test]
    fn test_regex_cache_url_patterns() {
        let cache = RegexCache::new();
        let url_pattern = r"^(https?|ftp)://[^\s/$.?#].[^\s]*$";

        assert!(cache.is_match(url_pattern, "http://example.com").unwrap());
        assert!(cache
            .is_match(url_pattern, "https://www.example.com/path")
            .unwrap());
        assert!(!cache.is_match(url_pattern, "not-a-url").unwrap());
    }

    #[test]
    fn test_regex_cache_ipv4_patterns() {
        let cache = RegexCache::new();
        let ipv4_pattern = r"^(?:(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.){3}(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)$";

        assert!(cache.is_match(ipv4_pattern, "192.168.1.1").unwrap());
        assert!(cache.is_match(ipv4_pattern, "255.255.255.255").unwrap());
        assert!(cache.is_match(ipv4_pattern, "0.0.0.0").unwrap());
        assert!(!cache.is_match(ipv4_pattern, "256.1.1.1").unwrap());
        assert!(!cache.is_match(ipv4_pattern, "192.168.1").unwrap());
    }
}
