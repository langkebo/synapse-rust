# 02: 修登录失败锁定在 Redis 故障时的 fail-open（P1-1）

**What to build:** 修改 `src/web/routes/auth_compat.rs` 中的 `check_login_lockout()` 和 `record_login_failure()`，在 Redis 不可用时读取配置 `fail_open_on_login_lockout`：
- `true`（默认）→ 保持现状，放行登录请求并记录 warn 日志
- `false` → 当 Redis 不可用时返回 `503 Service Unavailable`，强制拒绝登录

**Blocked by:** 01（需先添加配置字段）

**Status:** ✅ done (commit pending)

- [x] `check_login_lockout()` 在 Redis 不可用时读取配置，决定是 `Ok(())` 还是 `Err(ApiError::service_unavailable(...))`
- [x] `record_login_failure()` 同样处理（fail_open=false 时记录 error 并拒绝）
- [x] 添加 `tracing::warn!` 或 `tracing::error!` 日志，说明 Redis 不可用状态
- [x] cargo build --locked 通过
- [x] `api_auth_routes_tests` 6/6 PASS

**验证结果**（2026-09-03）：
- `cargo build --locked` ✅（2m58s）
- `cargo test --test integration api_auth_routes_tests` ✅ 6 passed; 0 failed
- 新增 `ApiError::service_unavailable()` 构造器和 `ApiErrorKind::ServiceUnavailable` 枚举（503 状态码）