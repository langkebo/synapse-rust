use regex::Regex;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

pub struct RegexCache {
    cache: Arc<RwLock<HashMap<String, Regex>>>,
}

impl RegexCache {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn get_or_create(&self, pattern: &str) -> Result<Regex, regex::Error> {
        {
            let cache = self.cache.read();
            if let Some(regex) = cache.get(pattern) {
                return Ok(regex.clone());
            }
        }

        let regex = Regex::new(pattern)?;
        let mut cache = self.cache.write();
        cache.insert(pattern.to_string(), regex.clone());
        Ok(regex)
    }

    pub fn is_match(&self, pattern: &str, text: &str) -> Result<bool, regex::Error> {
        let regex = self.get_or_create(pattern)?;
        Ok(regex.is_match(text))
    }

    pub fn clear(&self) {
        let mut cache = self.cache.write();
        cache.clear();
    }

    pub fn len(&self) -> usize {
        let cache = self.cache.read();
        cache.len()
    }

    pub fn is_empty(&self) -> bool {
        let cache = self.cache.read();
        cache.is_empty()
    }
}

impl Default for RegexCache {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for RegexCache {
    fn clone(&self) -> Self {
        Self {
            cache: Arc::clone(&self.cache),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_regex_cache_creation() {
        let cache = RegexCache::new();
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_regex_cache_get_or_create() {
        let cache = RegexCache::new();
        let regex1 = cache.get_or_create(r"\d+").unwrap();
        let regex2 = cache.get_or_create(r"\d+").unwrap();
        assert_eq!(cache.len(), 1);
        assert!(regex1.is_match("123"));
        assert!(regex2.is_match("456"));
    }

    #[test]
    fn test_regex_cache_is_match() {
        let cache = RegexCache::new();
        assert!(cache.is_match(r"\d+", "123").unwrap());
        assert!(!cache.is_match(r"\d+", "abc").unwrap());
        assert!(cache.is_match(r"[a-z]+", "abc").unwrap());
    }

    #[test]
    fn test_regex_cache_multiple_patterns() {
        let cache = RegexCache::new();
        cache.get_or_create(r"\d+").unwrap();
        cache.get_or_create(r"[a-z]+").unwrap();
        cache.get_or_create(r"\w+").unwrap();
        assert_eq!(cache.len(), 3);
    }

    #[test]
    fn test_regex_cache_clear() {
        let cache = RegexCache::new();
        cache.get_or_create(r"\d+").unwrap();
        cache.get_or_create(r"[a-z]+").unwrap();
        assert_eq!(cache.len(), 2);
        cache.clear();
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_regex_cache_clone() {
        let cache1 = RegexCache::new();
        cache1.get_or_create(r"\d+").unwrap();
        let cache2 = cache1.clone();
        assert_eq!(cache2.len(), 1);
        assert!(cache2.is_match(r"\d+", "123").unwrap());
    }

    #[test]
    fn test_regex_cache_invalid_pattern() {
        let cache = RegexCache::new();
        let result = cache.get_or_create(r"[invalid");
        assert!(result.is_err());
    }
}
