//! 增强的内容净化模块 - 使用 ammonia 防止 XSS 攻击

use ammonia::{Builder, UrlRelative};
use once_cell::sync::Lazy;
use std::collections::HashSet;

/// Matrix 消息的默认净化器配置
static DEFAULT_SANITIZER: Lazy<Builder<'static>> = Lazy::new(|| {
    let mut builder = Builder::default();

    // 允许的标签（Matrix 富文本格式）
    let allowed_tags: HashSet<&str> = [
        "p", "br", "span", "div",
        "strong", "em", "u", "s", "del",
        "code", "pre", "blockquote",
        "ul", "ol", "li",
        "h1", "h2", "h3", "h4", "h5", "h6",
        "a", "img",
        "table", "thead", "tbody", "tr", "th", "td",
    ].iter().copied().collect();

    builder.tags(allowed_tags);

    // 允许的属性
    let mut allowed_attrs: HashSet<&str> = HashSet::new();
    allowed_attrs.insert("href");
    allowed_attrs.insert("src");
    allowed_attrs.insert("alt");
    allowed_attrs.insert("title");
    allowed_attrs.insert("class");

    builder.generic_attributes(allowed_attrs);

    // URL 协议白名单
    builder.url_relative(UrlRelative::Deny);
    builder.link_rel(Some("noopener noreferrer"));

    // 允许的 URL 协议
    let allowed_protocols: HashSet<&str> = [
        "http", "https", "mailto", "matrix",
    ].iter().copied().collect();

    builder.url_schemes(allowed_protocols);

    builder
});

/// 严格的净化器配置（用于用户名、房间名等）
static STRICT_SANITIZER: Lazy<Builder<'static>> = Lazy::new(|| {
    let mut builder = Builder::default();

    // 只允许纯文本，移除所有 HTML
    builder.tags(HashSet::new());
    builder.generic_attributes(HashSet::new());

    builder
});

/// 内容净化器配置
#[derive(Debug, Clone, Copy)]
pub enum SanitizerMode {
    /// 默认模式 - 允许 Matrix 富文本格式
    Default,
    /// 严格模式 - 只允许纯文本
    Strict,
    /// 自定义模式
    Custom,
}

/// 增强的内容净化器
pub struct ContentSanitizer {
    mode: SanitizerMode,
    max_length: usize,
}

impl Default for ContentSanitizer {
    fn default() -> Self {
        Self {
            mode: SanitizerMode::Default,
            max_length: 10_000,
        }
    }
}

impl ContentSanitizer {
    /// 创建新的净化器
    pub fn new(mode: SanitizerMode, max_length: usize) -> Self {
        Self { mode, max_length }
    }

    /// 创建严格模式净化器
    pub fn strict() -> Self {
        Self {
            mode: SanitizerMode::Strict,
            max_length: 1_000,
        }
    }

    /// 净化 HTML 内容
    pub fn sanitize(&self, input: &str) -> String {
        // 长度限制
        let input = if input.len() > self.max_length {
            &input[..self.max_length]
        } else {
            input
        };

        // 根据模式选择净化器
        match self.mode {
            SanitizerMode::Default => {
                DEFAULT_SANITIZER.clean(input).to_string()
            }
            SanitizerMode::Strict => {
                STRICT_SANITIZER.clean(input).to_string()
            }
            SanitizerMode::Custom => {
                // 自定义配置可以在这里实现
                DEFAULT_SANITIZER.clean(input).to_string()
            }
        }
    }

    /// 净化纯文本（移除所有 HTML）
    pub fn sanitize_plain_text(&self, input: &str) -> String {
        let input = if input.len() > self.max_length {
            &input[..self.max_length]
        } else {
            input
        };

        STRICT_SANITIZER.clean(input).to_string()
    }

    /// 检查内容是否包含 HTML
    pub fn contains_html(&self, input: &str) -> bool {
        let cleaned = STRICT_SANITIZER.clean(input).to_string();
        cleaned.len() != input.len() || cleaned != input
    }
}

/// 创建默认净化器
pub fn create_sanitizer() -> ContentSanitizer {
    ContentSanitizer::default()
}

/// 创建严格净化器
pub fn create_strict_sanitizer() -> ContentSanitizer {
    ContentSanitizer::strict()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_script_tag() {
        let sanitizer = ContentSanitizer::default();
        let input = "<script>alert('xss')</script>Hello";
        let output = sanitizer.sanitize(input);
        assert!(!output.contains("<script>"));
        assert!(output.contains("Hello"));
    }

    #[test]
    fn test_sanitize_event_handler() {
        let sanitizer = ContentSanitizer::default();
        let input = "<img onerror=\"alert(1)\" src=\"x\">";
        let output = sanitizer.sanitize(input);
        assert!(!output.contains("onerror"));
    }

    #[test]
    fn test_sanitize_javascript_url() {
        let sanitizer = ContentSanitizer::default();
        let input = "<a href=\"javascript:alert(1)\">click</a>";
        let output = sanitizer.sanitize(input);
        assert!(!output.contains("javascript:"));
    }

    #[test]
    fn test_html_entity_bypass() {
        let sanitizer = ContentSanitizer::default();
        let input = "<img src=x onerror=\"&#97;&#108;&#101;&#114;&#116;(1)\">";
        let output = sanitizer.sanitize(input);
        assert!(!output.contains("onerror"));
    }

    #[test]
    fn test_nested_tags() {
        let sanitizer = ContentSanitizer::default();
        let input = "<div><script>alert(1)</script></div>";
        let output = sanitizer.sanitize(input);
        assert!(!output.contains("<script>"));
    }

    #[test]
    fn test_allowed_formatting() {
        let sanitizer = ContentSanitizer::default();
        let input = "<strong>Bold</strong> <em>Italic</em>";
        let output = sanitizer.sanitize(input);
        assert!(output.contains("<strong>"));
        assert!(output.contains("<em>"));
    }

    #[test]
    fn test_strict_mode_removes_all_html() {
        let sanitizer = ContentSanitizer::strict();
        let input = "<strong>Bold</strong> text";
        let output = sanitizer.sanitize(input);
        assert!(!output.contains("<strong>"));
        assert!(output.contains("Bold"));
        assert!(output.contains("text"));
    }

    #[test]
    fn test_data_url_blocked() {
        let sanitizer = ContentSanitizer::default();
        let input = "<img src=\"data:text/html,<script>alert(1)</script>\">";
        let output = sanitizer.sanitize(input);
        assert!(!output.contains("data:"));
    }

    #[test]
    fn test_safe_links_preserved() {
        let sanitizer = ContentSanitizer::default();
        let input = "<a href=\"https://example.com\">Link</a>";
        let output = sanitizer.sanitize(input);
        assert!(output.contains("https://example.com"));
        assert!(output.contains("noopener noreferrer"));
    }

    #[test]
    fn test_length_limit() {
        let sanitizer = ContentSanitizer::new(SanitizerMode::Default, 10);
        let input = "This is a very long string that exceeds the limit";
        let output = sanitizer.sanitize(input);
        assert_eq!(output.len(), 10);
    }

    #[test]
    fn test_case_insensitive_bypass() {
        let sanitizer = ContentSanitizer::default();
        let input = "<ScRiPt>alert(1)</sCrIpT>";
        let output = sanitizer.sanitize(input);
        assert!(!output.contains("ScRiPt"));
        assert!(!output.contains("alert"));
    }

    #[test]
    fn test_comment_bypass() {
        let sanitizer = ContentSanitizer::default();
        let input = "<img src=x o<!---->nerror=\"alert(1)\">";
        let output = sanitizer.sanitize(input);
        // ammonia removes the dangerous onerror attribute regardless of comments
        assert!(!output.contains("onerror"));
    }
}
