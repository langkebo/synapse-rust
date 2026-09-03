# 16: csrf_secret 默认值非空校验（P3-4）

**What to build:** `synapse-common/src/config/security.rs:60-62` 的 `default_csrf_secret()` 返回值若为空字符串会导致 CSRF 失效。在 `Config::validate()` 或 `SecurityConfig::validate()` 中强制要求 csrf_secret 非空。

**Blocked by:** None

**Status:** ✅ done

- [x] 找到 `Config::validate()` 中 csrf_secret 校验逻辑
- [x] 加非空检查 + 清晰的错误消息
- [x] cargo build --locked 通过（1m45s，无 warning）
- [x] config::validation 单元测试通过（11/11，含 2 个新测试）

**实现要点**：
- 在 `synapse-common/src/config/validation.rs` 的 `Config::validate()` 中，`security.secret` 检查后立即加 csrf_secret 非空校验
- 错误消息明确：空 csrf_secret → 拒绝启动 + 说明"删除字段用 auto-gen 或手动填随机字符串"
- 两个新测试：`validate_rejects_empty_csrf_secret`、`validate_accepts_non_empty_csrf_secret`
- 注：`security.rs` 中的 `default_csrf_secret()` 用 `rand::rng().fill_bytes()` 自动生成（无需 `std::env::set_var`），生产环境无需手动配置
