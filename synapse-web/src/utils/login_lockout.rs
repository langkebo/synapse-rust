//! 登录失败锁定（D8 + SEC-05）—— 单一实现，单一真相源。
//!
//! ## 为什么单独成模块（2026-10-10）
//!
//! 这段逻辑原先长在 `routes/auth_compat.rs` 里，并带着三个**硬编码常量**
//! （`LOGIN_MAX_ATTEMPTS = 5` / `LOGIN_MAX_IP_ATTEMPTS = 30` /
//! `LOGIN_LOCKOUT_TTL_SECS = 900`），与 `homeserver.yaml` 的
//! `security.login_failure_lockout_threshold` /
//! `security.login_lockout_max_ip_attempts` /
//! `security.login_lockout_duration_seconds` **并存**：
//!
//! * 服务层 `synapse-services/src/auth/login.rs` 用**配置值**；
//! * 路由层用**硬编码**，配置改了完全不生效。
//!
//! 实测证据：运行配置写 `login_lockout_duration_seconds: 300`，
//! 而 `POST /login` 返回 `retry_after_ms: 900000`（=900s，代码默认值）。
//!
//! 抽出来还有一个直接收益：`routes/auth_compat.rs` 与本文件都受
//! `scripts/ci/check_file_size_ratchet.py` 约束（受保护文件**只许缩小**），
//! 把跨切的锁定关注点从路由文件里挪走同时让两边都合规。
//!
//! ## 两套阈值（SEC-05）
//!
//! * `login_fail:{ip}:{username}` —— 按 (IP, 用户名) 对，挡单账号爆破；
//! * `login_ip_fail:{ip}` —— 按 IP 聚合，挡「同一出口轮换用户名」的喷洒式爆破。
//!
//! ## 受信出口豁免
//!
//! `security.login_lockout_ip_allowlist` 中的 IP/CIDR **既不拒绝也不计数** ——
//! 因为按 IP 聚合的锁定会把同一出口（CI runner、NAT 办公网）的**所有**账号
//! 一起拒掉。豁免只作用于锁定，密码校验与限流照常生效，不构成认证绕过后门。

use std::net::IpAddr;

use synapse_common::ApiError;

/// 总开关关闭时只告警一次。
///
/// 用 `static` 而不是每次请求都 `warn!`：关锁定期间登录是热点路径，
/// 逐请求打日志会淹没真正的告警。但也不能静默——带着 `false` 上线
/// 意味着暴力破解防护为零，必须在日志里留下痕迹。
fn warn_lockout_disabled_once() {
    static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        tracing::warn!(
            "Login lockout is DISABLED (security.login_lockout_enabled = false). \
             Brute-force protection is OFF — intended for test/debug deployments only; \
             set it back to true before going to production."
        );
    }
}

/// 登录锁定是否启用（配置默认 true）。
fn lockout_enabled(config: &synapse_common::config::Config) -> bool {
    config.security.login_lockout_enabled
}

/// Does `ip` match any entry of `allowlist`?
///
/// Entries may be a bare address (`127.0.0.1`, `::1`) or a CIDR block
/// (`10.0.0.0/8`, `fd00::/8`). Blank entries are ignored, so a trailing comma in
/// a config list is harmless.
///
/// Single implementation on purpose: `routes::admin::register`（管理员注册 IP 策略）
/// 与登录锁定豁免都要回答"这个 IP 可信吗"。两份实现 = 改了 A 忘了 B 的风险。
pub(crate) fn ip_in_allowlist(ip: IpAddr, allowlist: &[String]) -> bool {
    allowlist.iter().any(|entry| {
        let candidate = entry.trim();
        if candidate.is_empty() {
            return false;
        }
        candidate
            .parse::<ipnetwork::IpNetwork>()
            .map(|network| network.contains(ip))
            .or_else(|_| candidate.parse::<IpAddr>().map(|allowed| allowed == ip))
            .unwrap_or(false)
    })
}

// ---------------------------------------------------------------------------
// D8: Login failure lockout (Redis-backed, fail-open)
// ---------------------------------------------------------------------------
//
// ⚠️ 所有阈值与时长都来自 `config.security.*`，**不得**再引入本地常量。
//
// 2026-10-10 修复：此前这里有三个硬编码常量
//   `LOGIN_MAX_ATTEMPTS = 5` / `LOGIN_MAX_IP_ATTEMPTS = 30` /
//   `LOGIN_LOCKOUT_TTL_SECS = 900`
// 与 `homeserver.yaml` 的
//   `security.login_failure_lockout_threshold: 5` /
//   `security.login_lockout_duration_seconds: 300`
// **同时存在**，于是同一语义有两个真相源：
//   - 服务层 `auth/login.rs` 用配置值（300s）；
//   - 本层用硬编码（900s），配置改了**完全不生效**。
// 实测证据：`curl POST /login` 返回 `retry_after_ms: 900000`，而运行配置写的是 300。
// 另外「按 IP 聚合 30 次 ⇒ 锁 900 秒」会把同出口的所有账号一起拒掉 ——
// CI / NAT 办公网因此整体登录失败（API 全量测试被降级为匿名探测却仍报满分，
// 见 scripts/api_test/run_api_tests.py 的 auth-coverage 门禁）。
//
// 现在：阈值取 `login_failure_lockout_threshold`（按 (ip,username) 对）
//       与 `login_lockout_max_ip_attempts`（按 IP 聚合），
//       时长取 `login_lockout_duration_seconds`，
//       并支持 `login_lockout_ip_allowlist` 豁免受信出口。

/// 把配置里的「秒」换算成 `retry_after_ms`，饱和运算避免溢出。
fn lockout_retry_after_ms(config: &synapse_common::config::Config) -> u64 {
    config.security.login_lockout_duration_seconds.saturating_mul(1000)
}

/// 渐进退避：第 `level` 档（从 1 起）的 `retry_after_ms`。
///
/// `base * 2^(level-1)`，并以 `login_lockout_max_duration_seconds` 封顶。
/// 档位随「已锁定状态下的重试次数」增长，因此持续爆破的代价指数上升；
/// 封顶则保证误伤不会变成实质性的长期拒绝服务。
fn backoff_retry_after_ms(config: &synapse_common::config::Config, level: u32) -> u64 {
    let base_seconds = config.security.login_lockout_duration_seconds;
    // 上限小于基数时取两者较大值：否则「第 1 档」会被压到低于配置的锁定窗口，
    // 让锁定看起来完全不生效。
    let cap = config.security.login_lockout_max_duration_seconds.max(base_seconds);
    let shift = level.saturating_sub(1).min(16);
    lockout_retry_after_ms(config).saturating_mul(1u64 << shift).min(cap.saturating_mul(1000))
}

/// 由「连续失败次数」推出退避档位：**达到阈值仍是第 1 档**（base），
/// 此后每在锁定状态下多试一次升一档。
///
/// 计数器是 `u64`（Redis `INCR`），阈值是配置里的 `u32`；档位本身不需要超过
/// `u32`（`backoff_retry_after_ms` 另外做了 16 档封顶），故在此饱和收敛。
fn backoff_level(failures: u64, threshold: u32) -> u32 {
    let failures = u32::try_from(failures).unwrap_or(u32::MAX);
    failures.saturating_sub(threshold).max(1)
}

/// 受信出口豁免：命中则**既不拒绝也不计数**。
///
/// 只豁免「锁定」；密码校验与限流照常，因此不会成为绕过认证的后门。
fn lockout_exempt(config: &synapse_common::config::Config, ip: &str) -> bool {
    if config.security.login_lockout_ip_allowlist.is_empty() {
        return false;
    }
    ip.parse::<std::net::IpAddr>()
        .map(|addr| ip_in_allowlist(addr, &config.security.login_lockout_ip_allowlist))
        .unwrap_or(false)
}

/// Check if the user is locked out due to too many failed login attempts.
///
/// Failure behavior when Redis is unavailable depends on
/// `config.security.login_lockout_fail_open_on_redis_error`:
/// - `false` (default): return 503 Service Unavailable to block all login
///   attempts while Redis is down. This closes the brute-force window that
///   would otherwise be open during a Redis outage.
/// - `true`: skip the lockout check, login proceeds (only acceptable when
///   Redis is embedded in-process with no external dependency).
pub(crate) async fn check_login_lockout(
    cache: &synapse_cache::CacheManager,
    config: &synapse_common::config::Config,
    ip: &str,
    username: &str,
) -> Result<(), ApiError> {
    // 测试/调试阶段的总开关：关闭时既不判定也不计数（见 `lockout_enabled`）。
    if !lockout_enabled(config) {
        warn_lockout_disabled_once();
        return Ok(());
    }

    if lockout_exempt(config, ip) {
        tracing::debug!(
            ip = %ip,
            username = %username,
            "Login lockout exempt: IP is on security.login_lockout_ip_allowlist"
        );
        return Ok(());
    }

    if !cache.is_redis_enabled() {
        if config.security.login_lockout_fail_open_on_redis_error {
            tracing::warn!(
                ip = %ip,
                username = %username,
                "Login lockout check skipped: Redis unavailable (fail_open_on_redis_error=true)"
            );
            return Ok(());
        }
        tracing::error!(
            ip = %ip,
            username = %username,
            "Login refused: Redis unavailable and fail_open_on_redis_error=false"
        );
        return Err(ApiError::service_unavailable(
            "Login temporarily unavailable: account lockout backend offline".to_string(),
        ));
    }
    let per_pair_threshold = config.security.login_failure_lockout_threshold;
    let key = format!("login_fail:{ip}:{username}");
    // 计数器窗口 = 配置的锁定时长，且**只在该键创建时安装**（见
    // `CacheManager::increment_counter`）：窗口不随重试顺延，否则攻击者只要在
    // 到期前再试一次就能把受害者的账号永久锁死。
    let window_secs = config.security.login_lockout_duration_seconds;
    match cache.get::<u32>(&key).await {
        Ok(Some(count)) if count >= per_pair_threshold => {
            // 已锁定状态下的每一次重试都推高档位：持续爆破的 retry_after 指数增长
            // （封顶 `login_lockout_max_duration_seconds`），而窗口本身不延长。
            let failures = cache.increment_counter(&key, window_secs).await.max(u64::from(count));
            let retry_after_ms = backoff_retry_after_ms(config, backoff_level(failures, per_pair_threshold));
            tracing::warn!(
                ip = %ip,
                username = %username,
                count = failures,
                threshold = per_pair_threshold,
                retry_after_ms,
                "Login locked out due to too many failures"
            );
            return Err(ApiError::rate_limited_with_retry(retry_after_ms));
        }
        _ => {}
    }

    // SEC-05: per-IP aggregate cap, independent of the username so a single
    // address cannot rotate names to dodge the per-pair bucket.
    let ip_threshold = config.security.login_lockout_max_ip_attempts;
    let ip_key = format!("login_ip_fail:{ip}");
    match cache.get::<u32>(&ip_key).await {
        Ok(Some(count)) if count >= ip_threshold => {
            let failures = cache.increment_counter(&ip_key, window_secs).await.max(u64::from(count));
            let retry_after_ms = backoff_retry_after_ms(config, backoff_level(failures, ip_threshold));
            tracing::warn!(
                ip = %ip,
                count = failures,
                threshold = ip_threshold,
                retry_after_ms,
                "Login locked out: too many failures from this IP"
            );
            Err(ApiError::rate_limited_with_retry(retry_after_ms))
        }
        _ => Ok(()),
    }
}

/// Record a failed login attempt. When
/// `config.security.login_lockout_fail_open_on_redis_error` is false (default),
/// log an error; otherwise silently drop (counter is gone anyway since Redis is down).
pub(crate) async fn record_login_failure(
    cache: &synapse_cache::CacheManager,
    config: &synapse_common::config::Config,
    ip: &str,
    username: &str,
) {
    // 总开关关闭时不计数：否则关掉锁定后计数器仍在写，重新打开时
    // 整个测试期间累积的失败会立刻把用户锁死。
    if !lockout_enabled(config) {
        return;
    }

    // 受信出口既不拒绝也不计数：否则 CI/NAT 出口会在多轮测试中把 IP 桶刷爆，
    // 连带把**同出口的正常用户**锁死 900 秒。
    if lockout_exempt(config, ip) {
        tracing::debug!(
            ip = %ip,
            username = %username,
            "Login failure not counted: IP is on security.login_lockout_ip_allowlist"
        );
        return;
    }

    if !cache.is_redis_enabled() {
        if !config.security.login_lockout_fail_open_on_redis_error {
            tracing::error!(
                ip = %ip,
                username = %username,
                "Cannot record login failure: Redis unavailable and fail_open_on_redis_error=false"
            );
        }
        return;
    }

    // 计数器 TTL 与锁定时长同源，避免「锁定还没到期、计数器先过期」导致锁定
    // 提前失效（或反之，残留计数把下次登录立刻锁死）。
    //
    // 关键：TTL 只在**窗口开启时**安装一次（`increment_counter` 的固定窗口语义）。
    // 此前这里是 `get` + `set(ttl)`，每次都顺延窗口；配合 L1 忽略 per-write TTL
    // 的缺陷（builder 默认 2 小时），实际效果是「失败一次就锁 2 小时且可被无限续期」。
    let ttl = config.security.login_lockout_duration_seconds;

    let key = format!("login_fail:{ip}:{username}");
    let count = cache.increment_counter(&key, ttl).await;
    if count == u64::from(config.security.login_failure_lockout_threshold) {
        tracing::warn!(
            ip = %ip,
            username = %username,
            count,
            "Login failure threshold reached; account is now locked out"
        );
    }

    // SEC-05: bump the per-IP aggregate bucket on every failure (any username),
    // so rotating names still accumulates against the address.
    let ip_key = format!("login_ip_fail:{ip}");
    let ip_count = cache.increment_counter(&ip_key, ttl).await;
    if ip_count == u64::from(config.security.login_lockout_max_ip_attempts) {
        tracing::warn!(ip = %ip, count = ip_count, "Per-IP login failure threshold reached");
    }
}

/// Clear login failure counter on successful login.
///
/// SEC-05: only the per-(IP, username) bucket is cleared. The per-IP aggregate
/// (`login_ip_fail:{ip}`) is left to decay via TTL — otherwise one valid account
/// would let an attacker also spraying other names reset the address-level cap.
pub(crate) async fn clear_login_failures(cache: &synapse_cache::CacheManager, ip: &str, username: &str) {
    if !cache.is_redis_enabled() {
        return;
    }
    let key = format!("login_fail:{ip}:{username}");
    cache.delete(&key).await;
}

// ---------------------------------------------------------------------------
// P1 login-lockout hardening — tests
// ---------------------------------------------------------------------------
//
// Background: `docs/audit/archive/P1_security_2026-09-10.md`.
//
// The audit initially suspected that `check_login_lockout`'s `_ => Ok(())` arm
// let a Redis outage disable the brute-force lock. Direct measurement showed
// otherwise: `CacheManager::get()` **never** surfaces backend failures (it logs
// an L1 miss and returns `Ok(None)`), so that arm is unreachable and the local
// (L1) tier is what keeps the lock alive. These tests pin that real behaviour —
// and the `CacheManager::set` TTL defect it exposed, which is guarded separately
// in `synapse-cache`.
#[cfg(test)]
mod lockout_degradation_tests {
    use super::*;
    use deadpool_redis::{Config as RedisPoolConfig, Runtime as RedisRuntime};

    /// Build a `CacheManager` whose Redis backend is unreachable.
    ///
    /// Port 1 is reserved (tcpmux) and never listening, so connect attempts fail
    /// immediately instead of hanging.
    fn cache_with_unreachable_redis() -> synapse_cache::CacheManager {
        const BROKEN_URL: &str = "redis://127.0.0.1:1";
        let pool = RedisPoolConfig::from_url(BROKEN_URL)
            .create_pool(Some(RedisRuntime::Tokio1))
            .expect("failed to build Redis pool for degraded-lockout test");
        synapse_cache::CacheManager::with_redis_pool_and_url(pool, &synapse_cache::CacheConfig::default(), BROKEN_URL)
    }

    /// Build a `Config` whose lockout policy is fail-closed.
    fn fail_closed_config() -> synapse_common::config::Config {
        let mut config = synapse_common::config::Config::default();
        config.security.login_lockout_fail_open_on_redis_error = false;
        config
    }

    /// Regression guard（2026-10-10）：锁定时长必须**取自配置**，而不是本地常量。
    ///
    /// 修复前 `auth_compat.rs` 硬编码 `LOGIN_LOCKOUT_TTL_SECS = 900`，
    /// 与 `homeserver.yaml` 的 `login_lockout_duration_seconds`（dev 栈为 300）
    /// 并存 —— 改配置完全不影响行为。实测 `POST /login` 返回
    /// `retry_after_ms: 900000`，而运行配置写的是 300。
    /// 若有人再把时长写死回常量，本用例会红。
    #[test]
    fn lockout_retry_after_follows_config_not_hardcoded_constant() {
        let mut config = synapse_common::config::Config::default();

        config.security.login_lockout_duration_seconds = 300;
        assert_eq!(
            lockout_retry_after_ms(&config),
            300_000,
            "配置写 300 秒时 retry_after 必须是 300000ms（曾错误地恒为 900000）"
        );

        config.security.login_lockout_duration_seconds = 45;
        assert_eq!(lockout_retry_after_ms(&config), 45_000, "配置变更必须立刻反映到 retry_after");

        // 饱和运算：极大配置值不得 panic 或回绕成小数字。
        config.security.login_lockout_duration_seconds = u64::MAX;
        assert_eq!(lockout_retry_after_ms(&config), u64::MAX, "饱和而非回绕");
    }

    /// Regression guard（2026-10-10）：受信出口 IP 必须免于登录锁定。
    ///
    /// 动机：按 IP 聚合的锁定（默认 30 次 / 900 秒）会把同一出口的**所有**用户名
    /// 一起拒掉。实测 CI 出口被锁后，`scripts/api_test/run_api_tests.py` 拿不到
    /// user/admin token，静默降级为匿名探测却仍报「100/100 优秀」。
    /// 豁免只作用于锁定：这里断言「失败多次也不锁」且「计数器不增长」。
    #[tokio::test]
    async fn allowlisted_ip_is_exempt_from_login_lockout() {
        let cache = cache_with_unreachable_redis();
        let mut config = fail_closed_config();
        let excused = "203.0.113.77";
        config.security.login_lockout_ip_allowlist = vec![excused.to_string()];

        // 远超两个阈值（per-pair 5 / per-IP 30）的失败次数
        let threshold = config.security.login_lockout_max_ip_attempts;
        for _ in 0..(threshold * 2) {
            record_login_failure(&cache, &config, excused, "ci-runner").await;
        }

        assert!(
            check_login_lockout(&cache, &config, excused, "ci-runner").await.is_ok(),
            "已在 allowlist 上的出口 IP 不得被锁定"
        );
        assert!(
            check_login_lockout(&cache, &config, excused, "another-user-same-ip").await.is_ok(),
            "豁免按 IP 生效，同出口换用户名同样不得被锁定"
        );

        // 反向对照：同配置下未列入名单的 IP 仍必须被锁 —— 证明豁免不是全局关闸。
        let unlisted = "198.51.100.77";
        for _ in 0..config.security.login_failure_lockout_threshold {
            record_login_failure(&cache, &config, unlisted, "unlisted").await;
        }
        assert!(
            check_login_lockout(&cache, &config, unlisted, "unlisted").await.is_err(),
            "未列入 allowlist 的 IP 必须照常被锁定"
        );
    }

    /// CIDR 形式的豁免项（例如 `10.0.0.0/8`）必须生效，而不是只支持裸 IP。
    #[tokio::test]
    async fn allowlist_accepts_cidr_blocks() {
        let cache = cache_with_unreachable_redis();
        let mut config = fail_closed_config();
        config.security.login_lockout_ip_allowlist = vec!["10.0.0.0/8".to_string(), "::1".to_string()];

        let threshold = config.security.login_lockout_max_ip_attempts;
        for _ in 0..(threshold * 2) {
            record_login_failure(&cache, &config, "10.42.7.9", "cidr-victim").await;
        }
        assert!(
            check_login_lockout(&cache, &config, "10.42.7.9", "cidr-victim").await.is_ok(),
            "落在 10.0.0.0/8 内的地址必须被豁免"
        );
        assert!(
            check_login_lockout(&cache, &config, "192.0.2.1", "outside-cidr").await.is_ok(),
            "CIDR 外的地址未被计数，本就不该被锁（对照组）"
        );
    }

    /// Regression guard: the lockout must keep working while Redis is down.
    ///
    /// It survives on the L1 tier rather than through the `_ => Ok(())` arm that
    /// the audit originally flagged. If a refactor ever routes the counter away
    /// from L1 without adding an equivalent fallback, this fails.
    #[tokio::test]
    async fn lockout_engages_while_redis_is_down() {
        let cache = cache_with_unreachable_redis();
        let config = fail_closed_config();
        let (ip, username) = ("203.0.113.20", "local-tier-victim");

        assert!(
            check_login_lockout(&cache, &config, ip, username).await.is_ok(),
            "a fresh identifier must not be locked out"
        );

        for _ in 0..config.security.login_failure_lockout_threshold {
            record_login_failure(&cache, &config, ip, username).await;
        }

        let _err = check_login_lockout(&cache, &config, ip, username)
            .await
            .expect_err("threshold reached ⇒ lock must engage even without Redis");
    }

    /// The counter is scoped per `(ip, username)`: one victim's failures must not
    /// lock out unrelated identifiers.
    #[tokio::test]
    async fn lockout_counter_is_scoped_per_ip_and_username() {
        let cache = cache_with_unreachable_redis();
        let config = fail_closed_config();

        for _ in 0..config.security.login_failure_lockout_threshold {
            record_login_failure(&cache, &config, "203.0.113.8", "scoped-victim").await;
        }

        assert!(
            check_login_lockout(&cache, &config, "203.0.113.8", "scoped-victim").await.is_err(),
            "the targeted identifier must be locked"
        );
        assert!(
            check_login_lockout(&cache, &config, "203.0.113.8", "other-user").await.is_ok(),
            "a different username from the same IP must not inherit the lock"
        );
        assert!(
            check_login_lockout(&cache, &config, "198.51.100.9", "scoped-victim").await.is_ok(),
            "the same username from a different IP must not inherit the lock"
        );
    }

    /// When Redis is simply not configured at all, the pre-existing
    /// `login_lockout_fail_open_on_redis_error` semantics stay in force — this is
    /// the one path that flag actually governs.
    #[tokio::test]
    async fn lockout_is_skipped_when_redis_is_unconfigured() {
        let cache = synapse_cache::CacheManager::new(&synapse_cache::CacheConfig::default());
        assert!(!cache.is_redis_enabled(), "in-memory cache must report Redis as disabled");

        let mut lenient = synapse_common::config::Config::default();
        lenient.security.login_lockout_fail_open_on_redis_error = true;

        for _ in 0..(lenient.security.login_failure_lockout_threshold * 3) {
            record_login_failure(&cache, &lenient, "203.0.113.10", "no-redis").await;
        }
        assert!(
            check_login_lockout(&cache, &lenient, "203.0.113.10", "no-redis").await.is_ok(),
            "fail_open_on_redis_error=true keeps the documented allow-through behaviour"
        );

        let mut strict = synapse_common::config::Config::default();
        strict.security.login_lockout_fail_open_on_redis_error = false;
        assert!(
            check_login_lockout(&cache, &strict, "203.0.113.11", "no-redis-strict").await.is_err(),
            "fail_open_on_redis_error=false must refuse login while the backend is unavailable"
        );
    }

    /// P2: 连续失败越多退避越长，且必然封顶在 `login_lockout_max_duration_seconds`。
    ///
    /// 需求来自联调报告的 I-03：固定 300 秒的等长锁定既拦不住持续爆破（每 5 分钟
    /// 送 5 次免费尝试），又会对误操作的用户造成同等伤害。指数退避让持续爆破的
    /// 代价快速上升，封顶则避免退避本身变成拒绝服务。
    #[tokio::test]
    async fn lockout_backoff_escalates_then_caps() {
        let cache = cache_with_unreachable_redis();
        let mut config = fail_closed_config();
        config.security.login_failure_lockout_threshold = 2;
        config.security.login_lockout_duration_seconds = 1;
        config.security.login_lockout_max_duration_seconds = 4;

        let (ip, username) = ("203.0.113.30", "escalate-victim");
        for _ in 0..config.security.login_failure_lockout_threshold {
            record_login_failure(&cache, &config, ip, username).await;
        }

        let mut observed = Vec::new();
        for _ in 0..5 {
            let err = check_login_lockout(&cache, &config, ip, username)
                .await
                .expect_err("threshold reached ⇒ still locked while the window is open");
            observed.push(err.retry_after_ms().expect("429 must carry retry_after_ms"));
        }

        assert_eq!(observed[0], 1_000, "第 1 档必须等于配置的锁定窗口（1s）：{observed:?}");
        assert!(observed[1] > observed[0], "已锁定状态下的重试必须升档：{observed:?}");
        assert_eq!(*observed.last().unwrap(), 4_000, "必须封顶在 max_duration=4s：{observed:?}");
        assert!(observed.windows(2).all(|window| window[1] >= window[0]), "退避必须单调不减：{observed:?}");
    }

    /// P2：窗口起点固定在**首次失败**，已锁定状态下的重试只升档、不顺延窗口。
    ///
    /// 这是「永久锁」的真正成因：计数器 TTL 每次失败都顺延（滑窗），攻击者只要在
    /// 到期前再试一次，受害者就永远出不来；叠加 L1 忽略 per-write TTL 的老缺陷
    /// （builder 默认 2 小时）就更明显。固定窗口保证锁定一定自然到期。
    #[tokio::test]
    async fn lockout_window_expires_even_under_continued_attempts() {
        let cache = cache_with_unreachable_redis();
        let mut config = fail_closed_config();
        config.security.login_failure_lockout_threshold = 1;
        config.security.login_lockout_duration_seconds = 1;
        config.security.login_lockout_max_duration_seconds = 4;

        let (ip, username) = ("203.0.113.31", "window-victim");
        record_login_failure(&cache, &config, ip, username).await;
        assert!(check_login_lockout(&cache, &config, ip, username).await.is_err(), "首次失败即达阈值，必须锁定");

        // 攻击者在窗口内持续重试——每次都会升档，但都不得顺延窗口。
        for _ in 0..4 {
            let _ = check_login_lockout(&cache, &config, ip, username).await;
        }

        tokio::time::sleep(std::time::Duration::from_millis(1_100)).await;

        assert!(
            check_login_lockout(&cache, &config, ip, username).await.is_ok(),
            "窗口从首次失败起算，持续重试不得把它无限顺延"
        );
    }
}
