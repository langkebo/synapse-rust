//! 内容净化模块 - 防止 XSS 攻击
//! 
//! 提供 Matrix 事件内容的净化功能，移除危险的 HTML 和 JavaScript

use regex::Regex;
use once_cell::sync::Lazy;

/// 危险的 HTML 标签模式
static DANGEROUS_TAGS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)<(script|iframe|object|embed|form|input|button|meta|link|style)[^>]*>").unwrap()
});

/// 危险的事件处理器模式
static DANGEROUS_EVENTS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)on\w+\s*=").unwrap()
});

/// JavaScript URL 模式
static JAVASCRIPT_URLS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)javascript\s*:").unwrap()
});

/// 数据 URL 模式（可能包含恶意内容）
static DATA_URLS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)data\s*:").unwrap()
});

/// 内容净化器配置
#[derive(Debug, Clone)]
pub struct SanitizerConfig {
    /// 是否允许 img 标签
    pub allow_images: bool,
    /// 是否允许链接
    pub allow_links: bool,
    /// 最大内容长度
    pub max_length: usize,
    /// 是否净化 HTML
    pub sanitize_html: bool,
}

impl Default for SanitizerConfig {
    fn default() -> Self {
        Self {
            allow_images: true,
            allow_links: true,
            max_length: 10_000,
            sanitize_html: true,
        }
    }
}

/// Matrix 事件内容净化器
pub struct ContentSanitizer {
    config: SanitizerConfig,
}

impl Default for ContentSanitizer {
    fn default() -> Self {
        Self::new(SanitizerConfig::default())
    }
}

impl ContentSanitizer {
    pub fn new(config: SanitizerConfig) -> Self {
        Self { config }
    }

    /// 净化用户输入文本
    pub fn sanitize_text(&self, text: &str) -> String {
        if text.len() > self.config.max_length {
            return text[..self.config.max_length].to_string();
        }

        if !self.config.sanitize_html {
            return text.to_string();
        }

        // 移除危险的 HTML 标签
        let mut result = DANGEROUS_TAGS.replace_all(text, "").to_string();
        
        // 移除事件处理器
        result = DANGEROUS_EVENTS.replace_all(&result, "").to_string();
        
        // 移除 JavaScript URL
        result = JAVASCRIPT_URLS.replace_all(&result, "blocked:").to_string();
        
        // 移除 data URL
        result = DATA_URLS.replace_all(&result, "blocked:").to_string();
        
        result
    }

    /// 净化 Matrix 事件内容
    pub fn sanitize_event_content(&self, content: &str) -> String {
        self.sanitize_text(content)
    }

    /// 检查内容是否包含危险模式
    pub fn contains_dangerous_content(&self, text: &str) -> bool {
        DANGEROUS_TAGS.is_match(text)
            || DANGEROUS_EVENTS.is_match(text)
            || JAVASCRIPT_URLS.is_match(text)
    }
}

/// 创建默认的内容净化器
pub fn create_sanitizer() -> ContentSanitizer {
    ContentSanitizer::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_script_tag() {
        let sanitizer = ContentSanitizer::default();
        let input = "<script>alert('xss')</script>Hello";
        let output = sanitizer.sanitize_text(input);
        assert!(!output.contains("<script>"));
    }

    #[test]
    fn test_sanitize_event_handler() {
        let sanitizer = ContentSanitizer::default();
        let input = "<img onerror=\"alert(1)\" src=x>";
        let output = sanitizer.sanitize_text(input);
        assert!(!output.contains("onerror"));
    }

    #[test]
    fn test_sanitize_javascript_url() {
        let sanitizer = ContentSanitizer::default();
        let input = "<a href=\"javascript:alert(1)\">click</a>";
        let output = sanitizer.sanitize_text(input);
        assert!(!output.contains("javascript:"));
    }

    #[test]
    fn test_safe_content_unchanged() {
        let sanitizer = ContentSanitizer::default();
        let input = "Hello, this is a safe message!";
        let output = sanitizer.sanitize_text(input);
        assert_eq!(output, input);
    }

    #[test]
    fn test_contains_dangerous_content() {
        let sanitizer = ContentSanitizer::default();
        assert!(sanitizer.contains_dangerous_content("<script>"));
        assert!(sanitizer.contains_dangerous_content("<img onerror=\"x\">"));
        assert!(!sanitizer.contains_dangerous_content("safe text"));
    }
}
