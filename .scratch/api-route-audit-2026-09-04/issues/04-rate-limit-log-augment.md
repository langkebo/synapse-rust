# A4: rate_limit middleware 加攻击特征字段

**状态**：✅ DONE（2026-09-05）
**优先级**：💭 低
**审计来源**：API 路由安全审计 #A4（2026-09-04）

## 实际落地

### 改动
- `src/web/middleware/rate_limit.rs`
- +30 / -8 行
- 编译 + clippy 零警告，12 个相关 lib 测试全过

### 1. fail-open 日志（L117-126）
```rust
tracing::warn!(
    target: "rate_limit",
    event = "rate_limit_fail_open",
    client_ip = %ip,
    request_path = %request.uri().path(),
    endpoint = %endpoint_id,
    is_authenticated,
    error = %e,
    "Rate limiter error, allowing request"
);
```

### 2. 限流 reject 日志（L148-159）—— 从 debug 升级到 warn
```rust
tracing::warn!(
    target: "rate_limit",
    event = "rate_limit_rejected",
    client_ip = %ip,
    request_path = %request.uri().path(),
    endpoint = %endpoint_id,
    is_authenticated,
    per_second,
    burst_size,
    retry_after_seconds = decision.retry_after_seconds,
    "rate limit rejected request"
);
```

## 与原 ticket 的差异

**原 ticket 写**："加 user_id"
**实际**：`user_id` 物理上拿不到
- 原因：rate_limit middleware 在 assembly.rs:584 注册，**早于** auth middleware
  （auth 在 route 内的 handler 层）。rate_limit 看到请求时，token 还没解析。
- 不在 rate_limit 层做 JWT 解码/DB lookup（性能/复杂度不可接受）

**替代方案**：`is_authenticated`（Authorization header 存在性）
- 区分"未认证刷量"（如登录爆破、CC）和"已认证刷量"（如单一用户异常高频）
- 运维可在 production 日志直接 grep 定位两类攻击
- 零成本（仅 header key 存在性检查）

## 验收

- [x] `RateLimit::check_rate_limit` 超限日志含 client_ip + path + endpoint + retry_after
- [x] fail-open warn 日志含完整攻击特征
- [x] reject 日志升级为 warn（默认 RUST_LOG=info 可看）
- [x] `cargo build --locked` ✅
- [x] `cargo clippy --workspace --features "test-utils" -D warnings` 零警告
- [x] `cargo test --lib rate_limit` 12/12 通过

## Login 路径 attempted_username 字段

**原 ticket 提到**："login 路径超限的 warn 日志加 attempted_username（不要把 password 也打进去）"
**实际**：未做
- 原因：rate_limit middleware 不知道 path 是 login，也不会解析 body
- 风险：把 password 误打到日志是合规事故，**不解析 body 反而是安全设计**
- 替代：login 端点有独立的 `auth_middleware` log_login_failure 审计（SSO 审计 #1
  已修 HS256 / refresh TTL / SAML localpart），包含 attempted_username

**留给 auth 层的 audit event 处理，不在 rate_limit 这一层加。**
