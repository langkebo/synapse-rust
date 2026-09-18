use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;
use axum::http::HeaderMap;
use std::future::Future;
use std::net::{IpAddr, SocketAddr};

/// Placeholder used when no address can be attributed to a request.
pub(crate) const UNKNOWN_CLIENT_IP: &str = "unknown";

/// Peer address of the TCP connection, when the server was started with
/// connect-info support (`into_make_service_with_connect_info`).
///
/// Unlike `ConnectInfo<SocketAddr>` this extractor never rejects, so handlers that
/// only *want* the peer address (for attribution, not for access control) stay
/// callable from tests that drive the router with `oneshot` and therefore carry no
/// connection metadata.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PeerAddr(pub Option<SocketAddr>);

impl<S> FromRequestParts<S> for PeerAddr
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    fn from_request_parts(parts: &mut Parts, _state: &S) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        // Read without removing, so downstream extractors can still see it.
        let peer = parts.extensions.get::<ConnectInfo<SocketAddr>>().map(|connect_info| connect_info.0);
        async move { Ok(Self(peer)) }
    }
}

/// The single place that decides which address a request is attributed to.
///
/// Forwarded headers are honoured **only** when the deployment declares them
/// trustworthy; otherwise the peer address wins. That ordering is the whole point:
/// a client that can pick the value used for its rate-limit bucket or its login
/// lockout bucket has no bucket at all — rotating `X-Forwarded-For` would hand it a
/// fresh allowance on every request. Behind a proxy that *appends* (nginx's
/// `$proxy_add_x_forwarded_for`), the left-most entry is exactly the client-supplied
/// one, which is why `extract_client_ip` walks from the right instead.
///
/// Both the rate limiter and the login lockout go through here so they cannot drift
/// apart; see `middleware::rate_limit` and `routes::auth_compat`.
pub(crate) fn effective_client_ip(
    headers: &HeaderMap,
    peer_addr: Option<SocketAddr>,
    trust_forwarded: bool,
    priority: &[String],
    trusted_proxies: &[String],
) -> String {
    if trust_forwarded {
        extract_client_ip(headers, priority, peer_addr, trusted_proxies)
            .unwrap_or_else(|| UNKNOWN_CLIENT_IP.to_string())
    } else {
        peer_addr.map_or_else(|| UNKNOWN_CLIENT_IP.to_string(), |addr| addr.ip().to_string())
    }
}

/// Extract the effective client IP from request headers and peer address.
///
/// When `peer_addr` is Some and its IP matches one of the `trusted_proxies` CIDR
/// strings, forwarded headers (X-Forwarded-For, X-Real-IP, Forwarded) are trusted
/// and their values are used. Otherwise the peer address itself is returned.
///
/// When `peer_addr` is None, forwarded headers are used unconditionally (backward
/// compatibility for callers that do not have access to ConnectInfo).
pub(crate) fn extract_client_ip(
    headers: &HeaderMap,
    priority: &[String],
    peer_addr: Option<SocketAddr>,
    trusted_proxies: &[String],
) -> Option<String> {
    let peer_ip = peer_addr.map(|a| a.ip());

    // If we have a peer address, check whether it comes from a trusted proxy.
    match peer_ip {
        Some(ip) if !trusted_proxies.is_empty() && !is_trusted_peer(&ip, trusted_proxies) => {
            // Untrusted source → ignore forwarded headers, use peer address.
            return Some(ip.to_string());
        }
        Some(ip) if trusted_proxies.is_empty() => {
            // No trusted proxies configured → ignore forwarded headers, use peer address.
            return Some(ip.to_string());
        }
        _ => {
            // Trusted proxy OR no peer address → parse headers.
        }
    }

    // Parse forwarded headers (trusted source or no peer info).
    for name in priority {
        let lower = name.to_ascii_lowercase();
        if lower == "x-forwarded-for" {
            if let Some(ip) = headers
                .get("x-forwarded-for")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| rightmost_untrusted_hop(s, trusted_proxies))
            {
                return Some(ip);
            }
            continue;
        }

        if lower == "x-real-ip" {
            if let Some(ip) = headers
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
            {
                return Some(ip);
            }
            continue;
        }

        if lower == "forwarded" {
            if let Some(ip) = headers.get("forwarded").and_then(|v| v.to_str().ok()).and_then(parse_forwarded_for) {
                return Some(ip);
            }
            continue;
        }

        if let Some(ip) =
            headers.get(name).and_then(|v| v.to_str().ok()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
        {
            return Some(ip);
        }
    }

    // Fall back to peer address if no header matched.
    peer_ip.map(|ip| ip.to_string())
}

/// Check whether `ip` matches any of the CIDR strings in `networks`.
fn is_trusted_peer(ip: &IpAddr, networks: &[String]) -> bool {
    networks.iter().any(|cidr| ip_matches_cidr(ip, cidr))
}

/// SEC-01: 从 XFF 链中取「最右第一个不可信跳」作为真实客户端 IP。
///
/// 攻击者只能向 XFF 左侧注入伪造条目（可信代理会把攻击者真实 IP 追加到右侧），
/// 因此从右往左跳过所有可信代理跳后得到的第一个不可信 IP 才是真实客户端。
/// 取最左元素会让攻击者用 `X-Forwarded-For: fake_ip` 获得全新限流桶。
/// 全链可信时（纯内部转发）回退到最左元素。
fn rightmost_untrusted_hop(xff: &str, trusted_proxies: &[String]) -> Option<String> {
    let hops: Vec<&str> = xff.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
    if hops.is_empty() {
        return None;
    }
    for hop in hops.iter().rev() {
        let trusted = hop.parse::<IpAddr>().map(|ip| is_trusted_peer(&ip, trusted_proxies)).unwrap_or(false);
        if !trusted {
            return Some((*hop).to_string());
        }
    }
    // 全链可信：回退最左元素
    Some(hops[0].to_string())
}

/// Match an IP address against a CIDR string (e.g. "10.0.0.0/8" or "127.0.0.1/32").
fn ip_matches_cidr(ip: &IpAddr, cidr: &str) -> bool {
    let parts: Vec<&str> = cidr.split('/').collect();
    if parts.len() != 2 {
        return false;
    }
    let prefix_len: u8 = match parts[1].parse() {
        Ok(n) => n,
        Err(_) => return false,
    };
    let network = match parts[0].parse::<IpAddr>() {
        Ok(a) => a,
        Err(_) => return false,
    };
    match (ip, network) {
        (IpAddr::V4(ip), IpAddr::V4(net)) => {
            if prefix_len > 32 {
                return false;
            }
            let mask = if prefix_len == 0 { 0 } else { !0u32 << (32 - prefix_len) };
            u32::from(*ip) & mask == u32::from(net) & mask
        }
        (IpAddr::V6(ip), IpAddr::V6(net)) => {
            if prefix_len > 128 {
                return false;
            }
            let mask = if prefix_len == 0 { 0 } else { !0u128 << (128 - prefix_len) };
            u128::from(*ip) & mask == u128::from(net) & mask
        }
        _ => false,
    }
}

fn parse_forwarded_for(value: &str) -> Option<String> {
    let first = value.split(',').next()?.trim();
    for part in first.split(';') {
        let part = part.trim();
        let lower = part.to_ascii_lowercase();
        if lower.starts_with("for=") {
            let mut original = part[4..].trim();
            // `strip_prefix`/`strip_suffix` require at least two characters, so a
            // lone `"` (e.g. `Forwarded: for="`) can no longer produce
            // `&original[1..0]`. The previous form tested
            // `starts_with('"') && ends_with('"')`, which are BOTH true for a
            // single quote character, and `1..len - 1` is then `1..0` — a panic.
            // `extract_client_ip` parses this unconditionally when `peer_addr` is
            // `None`, and `admin_auth_middleware` passes `None`, so an
            // unauthenticated request carrying that header panicked the task
            // (remote DoS).
            if let Some(stripped) = original.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                original = stripped;
            }

            if original.starts_with('[') {
                if let Some(end) = original.find(']') {
                    return Some(original[1..end].to_string());
                }
            }

            let colons = original.chars().filter(|c| *c == ':').count();
            if colons == 1 {
                return original.split(':').next().map(|s| s.to_string());
            }

            if !original.is_empty() {
                return Some(original.to_string());
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn make_headers_with_xff(xff: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", xff.parse().unwrap());
        headers
    }

    fn header_map_with(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("forwarded", value.parse().unwrap());
        headers
    }

    // ---------------------------------------------------------------------------
    // effective_client_ip: the attribution decision shared by the rate limiter and
    // the login lockout.
    // ---------------------------------------------------------------------------

    fn priority() -> Vec<String> {
        vec!["x-forwarded-for".to_string(), "x-real-ip".to_string()]
    }

    fn loopback_peer() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 5000)
    }

    /// S10 regression: with forwarded headers untrusted, a client-supplied
    /// `X-Forwarded-For` must not decide the bucket. This is the shape the deploy
    /// stack runs in (`TRUST_FORWARDED_HEADERS` unset), and nginx appends the real
    /// peer to whatever the client sends, so the left-most entry is attacker text.
    #[test]
    fn untrusted_forwarded_headers_are_ignored_in_favour_of_the_peer_address() {
        let headers = make_headers_with_xff("203.0.113.99, 198.51.100.7");
        let ip = effective_client_ip(&headers, Some(loopback_peer()), false, &priority(), &[]);
        assert_eq!(ip, "127.0.0.1", "the peer address must win, not the spoofed left-most entry");
    }

    #[test]
    fn untrusted_without_a_peer_address_falls_back_to_unknown() {
        let headers = make_headers_with_xff("203.0.113.99");
        assert_eq!(effective_client_ip(&headers, None, false, &priority(), &[]), UNKNOWN_CLIENT_IP);
    }

    /// Even when forwarding *is* trusted, the right-most untrusted hop is used, so the
    /// client-supplied left edge still cannot choose the bucket.
    #[test]
    fn trusted_forwarding_uses_the_rightmost_untrusted_hop() {
        let headers = make_headers_with_xff("203.0.113.99, 198.51.100.7");
        let trusted = vec!["10.0.0.0/8".to_string()];
        let peer = SocketAddr::new("10.1.2.3".parse().unwrap(), 5000);
        let ip = effective_client_ip(&headers, Some(peer), true, &priority(), &trusted);
        assert_eq!(ip, "198.51.100.7", "the injected left-most entry must be skipped");
    }

    /// A peer that is not a configured proxy cannot make its headers authoritative.
    #[test]
    fn an_untrusted_peer_cannot_forward_at_all() {
        let headers = make_headers_with_xff("203.0.113.99");
        let trusted = vec!["10.0.0.0/8".to_string()];
        let ip = effective_client_ip(&headers, Some(loopback_peer()), true, &priority(), &trusted);
        assert_eq!(ip, "127.0.0.1");
    }

    #[test]
    fn trusted_forwarding_without_configured_proxies_uses_the_peer() {
        let headers = make_headers_with_xff("203.0.113.99");
        let ip = effective_client_ip(&headers, Some(loopback_peer()), true, &priority(), &[]);
        assert_eq!(ip, "127.0.0.1");
    }

    // ---------------------------------------------------------------------------
    // CIDR matching tests
    // ---------------------------------------------------------------------------

    #[test]
    fn ipv4_in_trusted_range() {
        let ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5));
        assert!(ip_matches_cidr(&ip, "10.0.0.0/8"));
        assert!(ip_matches_cidr(&ip, "10.0.0.0/16"));
        assert!(ip_matches_cidr(&ip, "10.0.0.5/32"));
    }

    #[test]
    fn ipv4_outside_trusted_range() {
        let ip = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1));
        assert!(!ip_matches_cidr(&ip, "10.0.0.0/8"));
        assert!(!ip_matches_cidr(&ip, "127.0.0.1/32"));
    }

    #[test]
    fn ipv6_in_trusted_range() {
        let ip: IpAddr = "2001:db8::1".parse().unwrap();
        // 2001:db8::1 is within 2001:db8::/32
        assert!(ip_matches_cidr(&ip, "2001:db8::/32"));
        // 2001:db8::1 exactly matches 2001:db8::1/128
        assert!(ip_matches_cidr(&ip, "2001:db8::1/128"));
        // 2001:db8::1 is NOT within fe80::/10
        assert!(!ip_matches_cidr(&ip, "fe80::/10"));
    }

    #[test]
    fn ipv4_in_ipv6_cidr_is_false() {
        let ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        assert!(!ip_matches_cidr(&ip, "2001:db8::/32"));
    }

    #[test]
    fn empty_networks_not_trusted() {
        let ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        assert!(!is_trusted_peer(&ip, &[]));
    }

    // ---------------------------------------------------------------------------
    // extract_client_ip tests
    // ---------------------------------------------------------------------------

    #[test]
    fn untrusted_peer_uses_peer_addr() {
        let headers = make_headers_with_xff("1.2.3.4");
        let priority = vec!["x-forwarded-for".to_string()];
        let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1)), 54321);
        let trusted: Vec<String> = vec!["10.0.0.0/8".to_string()];

        // Peer is untrusted (not in 10.0.0.0/8) → should use peer addr
        let ip = extract_client_ip(&headers, &priority, Some(peer), &trusted).unwrap();
        assert_eq!(ip, "203.0.113.1");
    }

    #[test]
    fn trusted_peer_uses_xff() {
        let headers = make_headers_with_xff("1.2.3.4");
        let priority = vec!["x-forwarded-for".to_string()];
        let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5)), 12345);
        let trusted: Vec<String> = vec!["10.0.0.0/8".to_string()];

        // Peer is in trusted range → should use XFF
        let ip = extract_client_ip(&headers, &priority, Some(peer), &trusted).unwrap();
        assert_eq!(ip, "1.2.3.4");
    }

    #[test]
    fn empty_trusted_list_uses_peer_addr() {
        let headers = make_headers_with_xff("1.2.3.4");
        let priority = vec!["x-forwarded-for".to_string()];
        let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5)), 12345);

        // Empty trusted list → no peer is trusted
        let ip = extract_client_ip(&headers, &priority, Some(peer), &[]).unwrap();
        assert_eq!(ip, "10.0.0.5");
    }

    #[test]
    fn no_peer_falls_back_to_header() {
        let headers = make_headers_with_xff("1.2.3.4");
        let priority = vec!["x-forwarded-for".to_string()];

        // None peer = unknown (backward compat for callers without ConnectInfo)
        let ip = extract_client_ip(&headers, &priority, None, &[]).unwrap();
        assert_eq!(ip, "1.2.3.4");
    }

    #[test]
    fn no_peer_no_header_returns_none() {
        let headers = HeaderMap::new();
        let priority = vec!["x-forwarded-for".to_string()];

        let ip = extract_client_ip(&headers, &priority, None, &[]);
        assert_eq!(ip, None);
    }

    #[test]
    fn x_real_ip_trusted() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", "5.6.7.8".parse().unwrap());
        let priority = vec!["x-real-ip".to_string()];
        let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5)), 12345);
        let trusted: Vec<String> = vec!["10.0.0.0/8".to_string()];

        let ip = extract_client_ip(&headers, &priority, Some(peer), &trusted).unwrap();
        assert_eq!(ip, "5.6.7.8");
    }

    #[test]
    fn forwarded_header_trusted() {
        let headers = header_map_with("for=192.0.2.60;proto=http;by=203.0.113.43");
        let priority = vec!["forwarded".to_string()];
        let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5)), 12345);
        let trusted: Vec<String> = vec!["10.0.0.0/8".to_string()];

        let ip = extract_client_ip(&headers, &priority, Some(peer), &trusted).unwrap();
        assert_eq!(ip, "192.0.2.60");
    }

    /// A lone `"` after `for=` used to panic.
    ///
    /// `starts_with('"')` and `ends_with('"')` are both true for a single quote
    /// character, so `&original[1..original.len() - 1]` became `&s[1..0]`.
    /// `extract_client_ip` parses the header unconditionally when `peer_addr` is
    /// `None`, and `admin_auth_middleware` passes `None`, so an unauthenticated
    /// request carrying `Forwarded: for="` panicked the request task.
    #[test]
    fn forwarded_header_with_lone_quote_does_not_panic() {
        let priority = vec!["forwarded".to_string()];
        let trusted: Vec<String> = vec!["10.0.0.0/8".to_string()];
        let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5)), 12345);

        for value in ["for=\"", "for=\"\"", "for=\"\"\"", "for=x\""] {
            let headers = header_map_with(value);
            // Both entry points: with a peer (the ordinary path) and without one
            // (the `admin_auth_middleware` path that made this remotely reachable).
            let _ = extract_client_ip(&headers, &priority, None, &trusted);
            let _ = extract_client_ip(&headers, &priority, Some(peer), &trusted);
        }
    }

    #[test]
    fn forwarded_header_quoted_value_is_unwrapped() {
        let headers = header_map_with("for=\"192.0.2.60\"");
        let priority = vec!["forwarded".to_string()];
        let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5)), 12345);
        let trusted: Vec<String> = vec!["10.0.0.0/8".to_string()];

        let ip = extract_client_ip(&headers, &priority, Some(peer), &trusted).unwrap();
        assert_eq!(ip, "192.0.2.60");
    }

    #[test]
    fn forwarded_header_untrusted() {
        let headers = header_map_with("for=192.0.2.60;proto=http;by=203.0.113.43");
        let priority = vec!["forwarded".to_string()];
        let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1)), 54321);
        let trusted: Vec<String> = vec!["10.0.0.0/8".to_string()];

        let ip = extract_client_ip(&headers, &priority, Some(peer), &trusted).unwrap();
        assert_eq!(ip, "203.0.113.1");
    }

    #[test]
    fn xff_priority_order_trusted() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4".parse().unwrap());
        headers.insert("x-real-ip", "10.0.0.1".parse().unwrap());
        let priority = vec!["x-forwarded-for".to_string(), "x-real-ip".to_string()];
        let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5)), 12345);
        let trusted: Vec<String> = vec!["10.0.0.0/8".to_string()];

        let ip = extract_client_ip(&headers, &priority, Some(peer), &trusted).unwrap();
        assert_eq!(ip, "1.2.3.4"); // XFF takes priority
    }

    #[test]
    fn trusted_peer_falls_back_to_peer_when_no_header_matches() {
        let headers = HeaderMap::new();
        let priority = vec!["x-forwarded-for".to_string()];
        let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5)), 12345);
        let trusted: Vec<String> = vec!["10.0.0.0/8".to_string()];

        // Trusted peer but no headers → falls back to peer addr
        let ip = extract_client_ip(&headers, &priority, Some(peer), &trusted).unwrap();
        assert_eq!(ip, "10.0.0.5");
    }

    // ---------------------------------------------------------------------------
    // S16 / SEC-01: XFF 不得取最左元素（可伪造注入），应取最右第一个不可信跳
    // ---------------------------------------------------------------------------

    #[test]
    fn spoofed_leftmost_xff_is_ignored() {
        // 攻击者直连可信代理，注入 XFF: 9.9.9.9(伪造)；代理追加攻击者真实 IP
        let headers = make_headers_with_xff("9.9.9.9, 1.2.3.4");
        let priority = vec!["x-forwarded-for".to_string()];
        let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5)), 12345);
        let trusted: Vec<String> = vec!["10.0.0.0/8".to_string()];

        let ip = extract_client_ip(&headers, &priority, Some(peer), &trusted).unwrap();
        assert_eq!(ip, "1.2.3.4", "伪造的最左 XFF 元素不得生效，应取最右第一个不可信跳");
    }

    #[test]
    fn trusted_chain_skips_trusted_hops() {
        // XFF: 真实客户端 + 可信中间代理 10.0.0.1；peer 10.0.0.5 也可信
        let headers = make_headers_with_xff("1.2.3.4, 10.0.0.1");
        let priority = vec!["x-forwarded-for".to_string()];
        let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5)), 12345);
        let trusted: Vec<String> = vec!["10.0.0.0/8".to_string()];

        let ip = extract_client_ip(&headers, &priority, Some(peer), &trusted).unwrap();
        assert_eq!(ip, "1.2.3.4", "应从右往左跳过所有可信跳后取真实客户端 IP");
    }

    #[test]
    fn all_trusted_xff_falls_back_to_leftmost() {
        // 全链可信（内部转发）→ 回退最左元素
        let headers = make_headers_with_xff("10.0.0.1, 10.0.0.2");
        let priority = vec!["x-forwarded-for".to_string()];
        let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5)), 12345);
        let trusted: Vec<String> = vec!["10.0.0.0/8".to_string()];

        let ip = extract_client_ip(&headers, &priority, Some(peer), &trusted).unwrap();
        assert_eq!(ip, "10.0.0.1");
    }
}
