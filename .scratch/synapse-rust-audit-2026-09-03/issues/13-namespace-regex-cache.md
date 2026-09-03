# 13: application_service namespace 正则缓存（P2-4）

**What to build:** `synapse-services/src/application_service/models.rs:276` 的 `any()` 闭包内 `Regex::new(pattern)` 每次都重新编译。预编译所有 namespace pattern 为 `static Lazy<Regex>` 并缓存。

**Blocked by:** None

**Status:** ✅ done（cargo build ✅，app_service 45/45 integration tests ✅）

- [x] 找出 `models.rs` 中所有 namespace 相关的 `Regex::new(...)` 调用（line 276，`namespace_matches`）
- [x] 用 `OnceLock<Mutex<BTreeMap<&str, &'static Regex>>` + `Box::leak` 静态缓存
- [x] 更新闭包引用为 `cached_regex(pattern).is_some_and(|regex| regex.is_match(candidate))`
- [x] cargo build --locked 通过
- [x] 相关单元测试通过（app_service 45/45 ✅）

## 修改文件

| 文件 | 改动 |
|------|------|
| `synapse-services/src/application_service/models.rs` | 新增 `cached_regex()`，把 `namespace_matches` 中的 `Regex::new(pattern)` 替换为静态缓存版本 |
