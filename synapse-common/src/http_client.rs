//! F-1/E-1: 共享 reqwest HTTP client 工厂。
//!
//! 修复审计 F-1（多处 `Client::new()` 无超时、builder 失败静默退化）与
//! E-1（每次请求新建 client、无连接池复用）：
//! - [`default_client`] 返回进程级共享 client（`OnceLock` 缓存，带连接池），
//!   connect_timeout 10s、总 timeout 30s、统一 User-Agent。
//! - [`client_with_timeout`] / [`no_redirect_client_with_timeout`] 按
//!   (timeout, redirect 策略) 缓存，builder 失败时 `tracing::warn!` 并
//!   回退到 [`default_client`]，不再静默退化为无超时 client。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const USER_AGENT: &str = "synapse-rust";
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
/// 有界防护：TIMEOUT_CLIENTS 缓存的自定义超时 client 数量上限。超时值理论上
/// 来自有限的常量集，但为防止调用方传入不可控超时导致无界累积，超过阈值即清空重建。
const MAX_CACHED_CLIENTS: usize = 128;

static DEFAULT_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
/// 按 (timeout_ms, no_redirect) 缓存的自定义超时 client，保证连接池复用。
#[allow(clippy::type_complexity)]
static TIMEOUT_CLIENTS: OnceLock<Mutex<HashMap<(u64, bool), reqwest::Client>>> = OnceLock::new();

fn build_client(timeout: Duration, no_redirect: bool) -> Result<reqwest::Client, reqwest::Error> {
    let mut builder = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(DEFAULT_CONNECT_TIMEOUT.min(timeout))
        .timeout(timeout)
        .pool_max_idle_per_host(20)
        .pool_idle_timeout(Duration::from_secs(90))
        .tcp_keepalive(Duration::from_secs(60));
    if no_redirect {
        // 联邦密钥抓取的 SSRF 防护依赖禁止重定向（E-1 保留原行为）。
        builder = builder.redirect(reqwest::redirect::Policy::none());
    }
    builder.build()
}

/// 进程级共享 HTTP client（F-1）：带连接池与默认超时，所有无特殊配置需求的
/// 调用方应复用此实例，而不是 `reqwest::Client::new()`。
pub fn default_client() -> reqwest::Client {
    DEFAULT_CLIENT
        .get_or_init(|| {
            build_client(DEFAULT_TIMEOUT, false).unwrap_or_else(|e| {
                // builder 失败（如 TLS 后端初始化失败）时绝不静默：记录 warn，
                // 回退到无配置的 Client（保留 reqwest 内部默认行为）。
                tracing::warn!(error = %e, "F-1: failed to build shared HTTP client, falling back to reqwest::Client::new()");
                reqwest::Client::new()
            })
        })
        .clone()
}

fn cached_client(timeout: Duration, no_redirect: bool) -> reqwest::Client {
    let key = (timeout.as_millis() as u64, no_redirect);
    let cache = TIMEOUT_CLIENTS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().unwrap_or_else(|p| p.into_inner());
    // 有界防护：超过上限且是新 key 时清空重建（连接池可重新建立），避免无界累积。
    if guard.len() >= MAX_CACHED_CLIENTS && !guard.contains_key(&key) {
        tracing::warn!(cached_clients = guard.len(), "TIMEOUT_CLIENTS cache reached capacity, resetting");
        guard.clear();
    }
    guard
        .entry(key)
        .or_insert_with(|| {
            build_client(timeout, no_redirect).unwrap_or_else(|e| {
                // F-1: builder 失败时记录 warn 并回退共享默认 client，不再静默
                // 退化为无超时的 `Client::new()`。
                tracing::warn!(
                    error = %e,
                    timeout_ms = key.0,
                    no_redirect = no_redirect,
                    "F-1: failed to build HTTP client with custom timeout, falling back to shared default client"
                );
                default_client()
            })
        })
        .clone()
}

/// 带自定义总超时的共享 client（F-1）；connect_timeout 取 10s 与 timeout 的较小值。
pub fn client_with_timeout(timeout: Duration) -> reqwest::Client {
    cached_client(timeout, false)
}

/// 禁止重定向的共享 client（E-1：联邦密钥抓取按 timeout 复用，不再每次新建）。
pub fn no_redirect_client_with_timeout(timeout: Duration) -> reqwest::Client {
    cached_client(timeout, true)
}

/// S2 修复（SSRF DNS rebinding）：构造将 URL 主机**钉扎到已验证 IP 集合**的
/// HTTP client。`ips` 必须来自 `security::check_url_and_resolve` 的返回值，
/// 保证"连接时使用的地址 == 黑名单校验时的地址"。
///
/// 钉扎是 per-host 的，故本函数不复用进程级 client 缓存；其调用方
/// （联邦密钥抓取、URL 预览）请求频率低且有上层缓存，新建 client 的开销可接受。
pub fn pinned_client_for_url(
    url: &str,
    ips: &[std::net::IpAddr],
    timeout: Duration,
    no_redirect: bool,
) -> Result<reqwest::Client, String> {
    use std::net::SocketAddr;

    let parsed = url::Url::parse(url).map_err(|e| format!("Invalid URL: {e}"))?;
    let host = parsed.host_str().ok_or_else(|| format!("URL has no host: {url}"))?;
    if ips.is_empty() {
        return Err(format!("No verified IPs to pin for host {host}"));
    }
    let port = parsed.port_or_known_default().unwrap_or(443);
    let socket_addrs: Vec<SocketAddr> = ips.iter().map(|ip| SocketAddr::new(*ip, port)).collect();

    let mut builder = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(DEFAULT_CONNECT_TIMEOUT.min(timeout))
        .timeout(timeout)
        .pool_max_idle_per_host(20)
        .pool_idle_timeout(Duration::from_secs(90))
        .tcp_keepalive(Duration::from_secs(60))
        .resolve_to_addrs(host, &socket_addrs);
    if no_redirect {
        // 与 no_redirect_client_with_timeout 一致：SSRF 防护依赖禁止重定向。
        builder = builder.redirect(reqwest::redirect::Policy::none());
    }
    builder.build().map_err(|e| format!("Failed to build pinned client: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_client_constructs() {
        let client = default_client();
        // reqwest::Client 不暴露配置读回，能构造且不 panic 即满足基本契约。
        let _ = format!("{:?}", client);
    }

    #[test]
    fn default_client_is_shared_singleton() {
        let a = default_client();
        let b = default_client();
        // OnceLock 语义：两次调用返回同一内部实例的克隆（共享连接池）。
        assert!(DEFAULT_CLIENT.get().is_some());
        // reqwest::Client 克隆共享同一 Arc；通过 Debug 输出一致做语义等价校验。
        assert_eq!(format!("{:?}", a), format!("{:?}", b));
    }

    #[test]
    fn client_with_timeout_constructs_and_caches() {
        let a = client_with_timeout(Duration::from_secs(5));
        let b = client_with_timeout(Duration::from_secs(5));
        assert_eq!(format!("{:?}", a), format!("{:?}", b));
        let cache = TIMEOUT_CLIENTS.get().expect("cache initialized");
        assert!(cache.lock().expect("lock").contains_key(&(5000, false)));
    }

    #[test]
    fn no_redirect_client_cached_separately() {
        let _ = no_redirect_client_with_timeout(Duration::from_millis(1234));
        let cache = TIMEOUT_CLIENTS.get().expect("cache initialized");
        assert!(cache.lock().expect("lock").contains_key(&(1234, true)));
    }

    // ------------------------------------------------------------------
    // S2 修复（SSRF DNS rebinding）：钉扎 client 必须把 URL 主机解析
    // 锁定到调用方已验证的 IP 集合。
    // ------------------------------------------------------------------

    #[test]
    fn pinned_client_for_url_constructs_with_verified_ips() {
        let ips = vec!["8.8.8.8".parse::<std::net::IpAddr>().unwrap()];
        let client = pinned_client_for_url(
            "https://keyserver.example.com/_matrix/key/v2/server",
            &ips,
            Duration::from_secs(10),
            true,
        )
        .expect("pinned client must construct");
        let _ = format!("{:?}", client);
    }

    #[test]
    fn pinned_client_for_url_rejects_empty_ip_list() {
        let result = pinned_client_for_url("https://example.com/", &[], Duration::from_secs(10), false);
        assert!(result.is_err(), "空 IP 列表必须拒绝（无已验证地址可钉扎）");
    }

    #[test]
    fn pinned_client_for_url_rejects_invalid_url() {
        let ips = vec!["8.8.8.8".parse::<std::net::IpAddr>().unwrap()];
        assert!(pinned_client_for_url("not-a-url", &ips, Duration::from_secs(10), false).is_err());
    }

    // ── S2 扩展测试：钉扎 client 边界场景 ──────────────────────────

    #[test]
    fn pinned_client_for_url_constructs_with_multiple_ips() {
        // 多 IP 钉扎：DNS 返回多个 A/AAAA 记录时，全部纳入钉扎集合。
        let ips = vec!["8.8.8.8".parse::<std::net::IpAddr>().unwrap(), "8.8.4.4".parse::<std::net::IpAddr>().unwrap()];
        let client = pinned_client_for_url(
            "https://keyserver.example.com/_matrix/key/v2/server",
            &ips,
            Duration::from_secs(10),
            true,
        )
        .expect("pinned client with multiple IPs must construct");
        let _ = format!("{:?}", client);
    }

    #[test]
    fn pinned_client_for_url_constructs_with_ipv6() {
        // IPv6 钉扎：验证 IPv6 地址可作为钉扎目标。
        let ips = vec!["2001:4860:4860::8888".parse::<std::net::IpAddr>().unwrap()];
        let client = pinned_client_for_url(
            "https://keyserver.example.com/_matrix/key/v2/server",
            &ips,
            Duration::from_secs(10),
            false,
        )
        .expect("pinned client with IPv6 must construct");
        let _ = format!("{:?}", client);
    }

    #[test]
    fn pinned_client_for_url_rejects_url_without_host() {
        // 无 host 的 URL（如 file:///path）必须被拒绝。
        let ips = vec!["8.8.8.8".parse::<std::net::IpAddr>().unwrap()];
        assert!(pinned_client_for_url("file:///etc/passwd", &ips, Duration::from_secs(10), false).is_err());
    }

    #[test]
    fn pinned_client_for_url_with_custom_port() {
        // 非默认端口 URL：钉扎 client 应使用 URL 中的端口。
        let ips = vec!["8.8.8.8".parse::<std::net::IpAddr>().unwrap()];
        let client = pinned_client_for_url(
            "https://keyserver.example.com:8448/_matrix/key/v2/server",
            &ips,
            Duration::from_secs(15),
            true,
        )
        .expect("pinned client with custom port must construct");
        let _ = format!("{:?}", client);
    }

    #[test]
    fn pinned_client_for_url_no_redirect_flag_respected() {
        // 验证 no_redirect=true 与 no_redirect=false 都能正常构造，
        // 确保 SSRF 防护的重定向策略可配置。
        let ips = vec!["8.8.8.8".parse::<std::net::IpAddr>().unwrap()];
        let _client_nr = pinned_client_for_url("https://example.com/", &ips, Duration::from_secs(10), true)
            .expect("no_redirect=true must construct");
        let _client_r = pinned_client_for_url("https://example.com/", &ips, Duration::from_secs(10), false)
            .expect("no_redirect=false must construct");
    }

    #[test]
    fn pinned_client_for_url_http_scheme() {
        // HTTP scheme + 端口 80：allow_http_key_fetch=true 场景下的钉扎。
        let ips = vec!["93.184.216.34".parse::<std::net::IpAddr>().unwrap()];
        let client = pinned_client_for_url(
            "http://keyserver.example.com/_matrix/key/v2/server",
            &ips,
            Duration::from_secs(10),
            true,
        )
        .expect("pinned client with HTTP scheme must construct");
        let _ = format!("{:?}", client);
    }
}
