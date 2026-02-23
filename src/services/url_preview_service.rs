use crate::cache::CacheManager;
use crate::common::config::{UrlPreviewConfig, UrlBlacklistRule};
use crate::common::error::ApiError;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, warn};
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UrlPreview {
    pub url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub image_url: Option<String>,
    pub image_width: Option<u32>,
    pub image_height: Option<u32>,
    pub image_type: Option<String>,
    pub site_name: Option<String>,
    pub og_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedPreview {
    pub preview: UrlPreview,
    pub cached_at: i64,
    pub expires_at: i64,
}

pub struct UrlPreviewService {
    config: Arc<UrlPreviewConfig>,
    http_client: reqwest::Client,
    cache: Arc<CacheManager>,
}

impl UrlPreviewService {
    pub fn new(config: Arc<UrlPreviewConfig>, cache: Arc<CacheManager>) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout))
            .user_agent(&config.user_agent)
            .redirect(reqwest::redirect::Policy::limited(config.max_redirects as usize))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self { config, http_client, cache }
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    pub async fn get_preview(&self, url: &str) -> Result<UrlPreview, ApiError> {
        if !self.is_enabled() {
            return Err(ApiError::bad_request("URL preview is disabled"));
        }

        let cache_key = format!("url_preview:{}", url);
        if let Some(cached) = self.get_cached_preview(&cache_key).await {
            debug!("Returning cached preview for {}", url);
            return Ok(cached.preview);
        }

        self.validate_url(url)?;

        let preview = self.fetch_and_parse(url).await?;

        self.cache_preview(&cache_key, &preview).await;

        Ok(preview)
    }

    fn validate_url(&self, url_str: &str) -> Result<(), ApiError> {
        let url = Url::parse(url_str)
            .map_err(|e| ApiError::bad_request(format!("Invalid URL: {}", e)))?;

        if url.scheme() != "https" && url.scheme() != "http" {
            return Err(ApiError::bad_request("Only HTTP/HTTPS URLs are allowed"));
        }

        if self.is_url_blacklisted(&url) {
            return Err(ApiError::bad_request("URL is blacklisted"));
        }

        if let Some(host) = url.host_str() {
            if self.is_ip_blacklisted(host) {
                return Err(ApiError::bad_request("IP address is blacklisted"));
            }
        }

        Ok(())
    }

    fn is_url_blacklisted(&self, url: &Url) -> bool {
        for rule in &self.config.url_blacklist {
            if self.matches_blacklist_rule(url, rule) {
                return true;
            }
        }
        false
    }

    fn matches_blacklist_rule(&self, url: &Url, rule: &UrlBlacklistRule) -> bool {
        if let Some(ref domain) = rule.domain {
            if let Some(host) = url.host_str() {
                if host == domain || host.ends_with(&format!(".{}", domain)) {
                    return true;
                }
            }
        }

        if let Some(ref pattern) = rule.regex {
            if let Ok(re) = Regex::new(pattern) {
                if re.is_match(url.as_str()) {
                    return true;
                }
            }
        }

        false
    }

    fn is_ip_blacklisted(&self, host: &str) -> bool {
        if self.config.ip_range_whitelist.iter().any(|range| {
            Self::ip_matches_range(host, range)
        }) {
            return false;
        }

        self.config.ip_range_blacklist.iter().any(|range| {
            Self::ip_matches_range(host, range)
        })
    }

    fn ip_matches_range(host: &str, range: &str) -> bool {
        if host == range {
            return true;
        }
        
        if range.contains('/') {
            if let Ok(addr) = host.parse::<std::net::IpAddr>() {
                if let Ok(network) = range.parse::<ipnetwork::IpNetwork>() {
                    return network.contains(addr);
                }
            }
        }
        
        false
    }

    async fn fetch_and_parse(&self, url: &str) -> Result<UrlPreview, ApiError> {
        let response = self.http_client
            .get(url)
            .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
            .send()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to fetch URL: {}", e)))?;

        if !response.status().is_success() {
            return Err(ApiError::bad_request(format!("HTTP error: {}", response.status())));
        }

        let content_type = response.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if !content_type.starts_with("text/html") {
            return Err(ApiError::bad_request("Content is not HTML"));
        }

        let body = response.text().await
            .map_err(|e| ApiError::internal(format!("Failed to read response: {}", e)))?;

        if body.len() > self.config.max_spider_size_bytes() {
            return Err(ApiError::bad_request("Content too large"));
        }

        Ok(self.parse_html(url, &body))
    }

    fn parse_html(&self, url: &str, html: &str) -> UrlPreview {
        let mut preview = UrlPreview {
            url: url.to_string(),
            title: None,
            description: None,
            image_url: None,
            image_width: None,
            image_height: None,
            image_type: None,
            site_name: None,
            og_type: None,
        };

        preview.title = self.extract_meta_content(html, "og:title")
            .or_else(|| self.extract_title(html));

        preview.description = self.extract_meta_content(html, "og:description")
            .or_else(|| self.extract_meta_content(html, "description"))
            .or_else(|| self.extract_meta_name(html, "description"));

        preview.image_url = self.extract_meta_content(html, "og:image")
            .map(|img| self.resolve_url(url, &img));

        preview.site_name = self.extract_meta_content(html, "og:site_name");
        preview.og_type = self.extract_meta_content(html, "og:type");

        preview
    }

    fn extract_meta_content(&self, html: &str, property: &str) -> Option<String> {
        let pattern = format!(
            r#"<meta\s+(?:property|name)=["']{}["']\s+content=["']([^"']+)["']"#,
            regex::escape(property)
        );
        
        if let Ok(re) = Regex::new(&pattern) {
            if let Some(caps) = re.captures(html) {
                return Some(caps[1].to_string());
            }
        }

        let pattern2 = format!(
            r#"<meta\s+content=["']([^"']+)["']\s+(?:property|name)=["']{}["']"#,
            regex::escape(property)
        );
        
        if let Ok(re) = Regex::new(&pattern2) {
            if let Some(caps) = re.captures(html) {
                return Some(caps[1].to_string());
            }
        }

        None
    }

    fn extract_meta_name(&self, html: &str, name: &str) -> Option<String> {
        let pattern = format!(
            r#"<meta\s+name=["']{}["']\s+content=["']([^"']+)["']"#,
            regex::escape(name)
        );
        
        if let Ok(re) = Regex::new(&pattern) {
            if let Some(caps) = re.captures(html) {
                return Some(caps[1].to_string());
            }
        }

        None
    }

    fn extract_title(&self, html: &str) -> Option<String> {
        let pattern = r#"<title[^>]*>([^<]+)</title>"#;
        
        if let Ok(re) = Regex::new(pattern) {
            if let Some(caps) = re.captures(html) {
                let title = caps[1].trim();
                if !title.is_empty() {
                    return Some(title.to_string());
                }
            }
        }

        None
    }

    fn resolve_url(&self, base: &str, relative: &str) -> String {
        if relative.starts_with("http://") || relative.starts_with("https://") {
            return relative.to_string();
        }

        if let Ok(base_url) = Url::parse(base) {
            if let Ok(resolved) = base_url.join(relative) {
                return resolved.to_string();
            }
        }

        relative.to_string()
    }

    async fn get_cached_preview(&self, cache_key: &str) -> Option<CachedPreview> {
        let cached = self.cache.get::<CachedPreview>(cache_key).await.ok()??;
        
        let now = chrono::Utc::now().timestamp();
        if cached.expires_at > now {
            Some(cached)
        } else {
            None
        }
    }

    async fn cache_preview(&self, cache_key: &str, preview: &UrlPreview) {
        let now = chrono::Utc::now().timestamp();
        let cached = CachedPreview {
            preview: preview.clone(),
            cached_at: now,
            expires_at: now + self.config.cache_duration as i64,
        };

        if let Err(e) = self.cache.set(cache_key, &cached, self.config.cache_duration).await {
            warn!("Failed to cache URL preview: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::CacheConfig;

    fn default_ip_blacklist() -> Vec<String> {
        vec![
            "127.0.0.0/8".to_string(),
            "10.0.0.0/8".to_string(),
            "172.16.0.0/12".to_string(),
            "192.168.0.0/16".to_string(),
        ]
    }

    fn create_test_config() -> UrlPreviewConfig {
        UrlPreviewConfig {
            enabled: true,
            ip_range_blacklist: default_ip_blacklist(),
            ip_range_whitelist: Vec::new(),
            url_blacklist: vec![
                UrlBlacklistRule {
                    domain: Some("blocked.example.com".to_string()),
                    regex: None,
                },
                UrlBlacklistRule {
                    domain: None,
                    regex: Some(r"^https://private\..*".to_string()),
                },
            ],
            spider_enabled: true,
            oembed_enabled: false,
            max_spider_size: "10M".to_string(),
            cache_duration: 86400,
            user_agent: "TestAgent/1.0".to_string(),
            timeout: 10,
            max_redirects: 5,
        }
    }

    fn create_test_service() -> UrlPreviewService {
        let config = Arc::new(create_test_config());
        let cache = Arc::new(CacheManager::new(CacheConfig::default()));
        UrlPreviewService::new(config, cache)
    }

    #[test]
    fn test_url_preview_enabled() {
        let config = create_test_config();
        assert!(config.enabled);
    }

    #[test]
    fn test_url_preview_disabled() {
        let config = UrlPreviewConfig::default();
        assert!(!config.enabled);
    }

    #[test]
    fn test_validate_url_valid() {
        let service = create_test_service();
        let result = service.validate_url("https://example.com/page");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_url_invalid_scheme() {
        let service = create_test_service();
        let result = service.validate_url("ftp://example.com/file");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_url_blacklisted_domain() {
        let service = create_test_service();
        let result = service.validate_url("https://blocked.example.com/page");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_url_blacklisted_regex() {
        let service = create_test_service();
        let result = service.validate_url("https://private.example.com/page");
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_title() {
        let service = create_test_service();
        let html = r#"<html><head><title>Test Page Title</title></head><body></body></html>"#;
        let title = service.extract_title(html);
        assert_eq!(title, Some("Test Page Title".to_string()));
    }

    #[test]
    fn test_extract_meta_content() {
        let service = create_test_service();
        let html = r#"<html><head><meta property="og:title" content="Open Graph Title"></head></html>"#;
        let title = service.extract_meta_content(html, "og:title");
        assert_eq!(title, Some("Open Graph Title".to_string()));
    }

    #[test]
    fn test_parse_html() {
        let service = create_test_service();
        let html = r#"
            <html>
            <head>
                <title>Page Title</title>
                <meta property="og:title" content="OG Title">
                <meta property="og:description" content="OG Description">
                <meta property="og:image" content="/image.jpg">
                <meta property="og:site_name" content="Example Site">
            </head>
            <body></body>
            </html>
        "#;
        
        let preview = service.parse_html("https://example.com/page", html);
        
        assert_eq!(preview.title, Some("OG Title".to_string()));
        assert_eq!(preview.description, Some("OG Description".to_string()));
        assert_eq!(preview.image_url, Some("https://example.com/image.jpg".to_string()));
        assert_eq!(preview.site_name, Some("Example Site".to_string()));
    }

    #[test]
    fn test_resolve_url_absolute() {
        let service = create_test_service();
        let result = service.resolve_url("https://example.com/page", "https://other.com/image.jpg");
        assert_eq!(result, "https://other.com/image.jpg");
    }

    #[test]
    fn test_resolve_url_relative() {
        let service = create_test_service();
        let result = service.resolve_url("https://example.com/page", "/image.jpg");
        assert_eq!(result, "https://example.com/image.jpg");
    }

    #[test]
    fn test_max_spider_size_bytes() {
        let config = create_test_config();
        assert_eq!(config.max_spider_size_bytes(), 10 * 1024 * 1024);
    }

    #[test]
    fn test_ip_blacklist() {
        let service = create_test_service();
        
        assert!(service.is_ip_blacklisted("127.0.0.1"));
        assert!(service.is_ip_blacklisted("10.0.0.1"));
        assert!(service.is_ip_blacklisted("192.168.1.1"));
        assert!(!service.is_ip_blacklisted("8.8.8.8"));
        assert!(!service.is_ip_blacklisted("example.com"));
    }
}
