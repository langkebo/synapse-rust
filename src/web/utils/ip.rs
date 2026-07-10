use axum::http::HeaderMap;
use std::net::{IpAddr, SocketAddr};

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
                .and_then(|s| s.split(',').next())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
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
            if original.starts_with('\"') && original.ends_with('\"') {
                original = &original[1..original.len() - 1];
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
        let headers = header_map_with( "for=192.0.2.60;proto=http;by=203.0.113.43");
        let priority = vec!["forwarded".to_string()];
        let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5)), 12345);
        let trusted: Vec<String> = vec!["10.0.0.0/8".to_string()];

        let ip = extract_client_ip(&headers, &priority, Some(peer), &trusted).unwrap();
        assert_eq!(ip, "192.0.2.60");
    }

    #[test]
    fn forwarded_header_untrusted() {
        let headers = header_map_with( "for=192.0.2.60;proto=http;by=203.0.113.43");
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
}
