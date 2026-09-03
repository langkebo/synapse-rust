# 12: saml_service 正则表达式 Lazy 缓存（P2-3）

**What to build:** `synapse-services/src/saml_service.rs:660-1001` 中 `extract_attribute_values()` 与 `extract_audiences()` 每次调用都 `Regex::new(pattern)` 重新编译。用 `once_cell::sync::Lazy<Regex>` 缓存编译后的正则，模块级 `static` 暴露。

**Blocked by:** None

**Status:** ✅ done（cargo build ✅，auth 78/78 integration tests ✅）

- [x] 找出 `saml_service.rs` 中所有 `Regex::new(...)` 调用（8 个静态 pattern + 1 个 runtime attribute）
- [x] 用 `cached_regex!` 宏 + `static OnceLock<Regex>` 缓存 8 个静态 pattern
- [x] `extract_attribute_values` 的 attribute 动态 pattern 用 `attribute_value_regex()`（Mutex<HashMap>）按 attribute 缓存
- [x] `extract_element_by_id` 的 pattern 依赖 runtime `element_id`，不做缓存（每次不同）
- [x] cargo build --locked 通过
- [x] 集成测试通过（auth 78/78 ✅）

## 修改文件

| 文件 | 改动 |
|------|------|
| `synapse-services/src/saml_service.rs` | 8 个静态 regex 用 `cached_regex!` 宏静态缓存；动态 attribute regex 用 `OnceLock<Mutex<HashMap>>` 缓存 |
