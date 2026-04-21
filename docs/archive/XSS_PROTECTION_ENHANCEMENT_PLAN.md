# XSS 防护加强方案

**日期：** 2026-04-15  
**优先级：** P0  
**状态：** 待实施

---

## 当前状态分析

### 现有实现（src/common/sanitizer.rs）

**优点：**
- 有基础的 XSS 防护机制
- 使用正则表达式过滤危险标签和事件处理器
- 有配置选项和测试用例

**缺点：**
- 使用正则表达式容易被绕过
- 无法处理 HTML 实体编码绕过
- 无法处理嵌套标签和复杂 HTML 结构
- 缺少白名单机制

### 已知的正则表达式绕过方式

1. **HTML 实体编码：**
   ```html
   <img src=x onerror="&#97;&#108;&#101;&#114;&#116;(1)">
   ```

2. **大小写混合：**
   ```html
   <ScRiPt>alert(1)</sCrIpT>
   ```

3. **空格和换行符：**
   ```html
   <img src=x
   onerror
   =
   "alert(1)">
   ```

4. **注释绕过：**
   ```html
   <img src=x o<!---->nerror="alert(1)">
   ```

---

## 改进方案：使用 ammonia 库

### 为什么选择 ammonia

1. **专业的 HTML 净化库**
   - 基于 html5ever 解析器，符合 HTML5 规范
   - 使用白名单机制，默认安全
   - 被广泛使用，经过充分测试

2. **功能强大**
   - 正确处理 HTML 实体编码
   - 支持自定义白名单
   - 自动清理危险属性
   - 处理嵌套和复杂结构

3. **性能优良**
   - 使用 Rust 编写，性能优秀
   - 零拷贝解析
   - 适合高并发场景

---

## 实施步骤

### 步骤 1：添加依赖

**修改 Cargo.toml：**
```toml
[dependencies]
ammonia = "4.1"
```

### 步骤 2：创建新的净化器实现

**创建 src/common/sanitizer_v2.rs：**

```rust
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
    
    builder.allowed_schemes(allowed_protocols);
    
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
        let cleaned = STRICT_SANITIZER.clean(input);
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
}
```

### 步骤 3：迁移现有代码

**保持向后兼容：**

1. 保留 `src/common/sanitizer.rs` 作为 v1
2. 新代码使用 `sanitizer_v2.rs`
3. 逐步迁移现有调用

**迁移计划：**
```rust
// 旧代码
use crate::common::sanitizer::ContentSanitizer;

// 新代码
use crate::common::sanitizer_v2::ContentSanitizer;
```

### 步骤 4：应用场景

#### 1. 消息内容净化
```rust
// src/services/room_service.rs
let sanitizer = ContentSanitizer::default();
let clean_body = sanitizer.sanitize(&message.body);
```

#### 2. 用户名净化
```rust
// src/services/user_service.rs
let sanitizer = ContentSanitizer::strict();
let clean_displayname = sanitizer.sanitize_plain_text(&displayname);
```

#### 3. 房间名净化
```rust
// src/services/room_service.rs
let sanitizer = ContentSanitizer::strict();
let clean_room_name = sanitizer.sanitize_plain_text(&room_name);
```

#### 4. 房间主题净化
```rust
// src/services/room_service.rs
let sanitizer = ContentSanitizer::default();
let clean_topic = sanitizer.sanitize(&topic);
```

---

## 测试计划

### 单元测试

**测试用例：**
1. 基本 XSS 攻击向量
2. HTML 实体编码绕过
3. 嵌套标签
4. 允许的格式保留
5. 严格模式测试
6. 长度限制测试

### 集成测试

**创建 tests/integration/xss_protection_tests.rs：**
```rust
#[tokio::test]
async fn test_message_xss_protection() {
    // 发送包含 XSS 的消息
    // 验证存储和返回的内容已净化
}

#[tokio::test]
async fn test_displayname_xss_protection() {
    // 设置包含 HTML 的显示名
    // 验证被净化为纯文本
}

#[tokio::test]
async fn test_room_topic_xss_protection() {
    // 设置包含 XSS 的房间主题
    // 验证被正确净化
}
```

### 性能测试

**基准测试：**
```rust
#[bench]
fn bench_sanitize_simple(b: &mut Bencher) {
    let sanitizer = ContentSanitizer::default();
    let input = "<p>Simple text</p>";
    b.iter(|| sanitizer.sanitize(input));
}

#[bench]
fn bench_sanitize_complex(b: &mut Bencher) {
    let sanitizer = ContentSanitizer::default();
    let input = include_str!("complex_html.html");
    b.iter(|| sanitizer.sanitize(input));
}
```

---

## 安全考虑

### 1. 内容安全策略（CSP）

**添加 CSP 头部：**
```rust
// src/web/middleware.rs
pub fn add_security_headers() -> impl Fn(Response) -> Response {
    move |mut response| {
        response.headers_mut().insert(
            "Content-Security-Policy",
            "default-src 'self'; script-src 'none'; object-src 'none';"
                .parse()
                .unwrap(),
        );
        response
    }
}
```

### 2. X-Content-Type-Options

```rust
response.headers_mut().insert(
    "X-Content-Type-Options",
    "nosniff".parse().unwrap(),
);
```

### 3. X-Frame-Options

```rust
response.headers_mut().insert(
    "X-Frame-Options",
    "DENY".parse().unwrap(),
);
```

---

## 部署计划

### 阶段 1：准备（1 天）
- [ ] 添加 ammonia 依赖
- [ ] 创建 sanitizer_v2.rs
- [ ] 编写单元测试

### 阶段 2：迁移（2-3 天）
- [ ] 迁移消息内容净化
- [ ] 迁移用户名净化
- [ ] 迁移房间信息净化
- [ ] 添加集成测试

### 阶段 3：验证（1 天）
- [ ] 运行所有测试
- [ ] 性能基准测试
- [ ] 安全审计

### 阶段 4：部署（1 天）
- [ ] 代码审查
- [ ] 合并到主分支
- [ ] 部署到测试环境
- [ ] 监控和验证

---

## 回滚计划

如果发现问题：
1. 保留 sanitizer.rs 作为备份
2. 可以快速切换回旧实现
3. 使用特性开关控制

```rust
#[cfg(feature = "new-sanitizer")]
use crate::common::sanitizer_v2 as sanitizer;

#[cfg(not(feature = "new-sanitizer"))]
use crate::common::sanitizer;
```

---

## 验收标准

- [ ] 所有已知 XSS 攻击向量被阻止
- [ ] HTML 实体编码绕过被阻止
- [ ] 允许的 Matrix 富文本格式正常工作
- [ ] 性能影响 < 10%
- [ ] 所有测试通过
- [ ] 代码覆盖率 > 90%

---

## 参考资料

- [ammonia 文档](https://docs.rs/ammonia/)
- [OWASP XSS 防护备忘单](https://cheatsheetseries.owasp.org/cheatsheets/Cross_Site_Scripting_Prevention_Cheat_Sheet.html)
- [Matrix 富文本格式规范](https://spec.matrix.org/v1.8/client-server-api/#mroommessage-msgtypes)
