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
}
