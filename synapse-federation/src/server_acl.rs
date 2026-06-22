//! Server ACL enforcement for federation (m.room.server_acl)
//!
//! Parses the `m.room.server_acl` state event content and provides matching
//! logic to determine whether a remote server is allowed to interact with a
//! room per the room's ACL policy.

use serde::{Deserialize, Serialize};

/// Content of an `m.room.server_acl` state event.
///
/// Per the Matrix specification, the ACL rules are:
/// - `allow`: list of glob patterns matching server names that are allowed.
///   An empty list means no servers are allowed (except those in `deny` takes
///   precedence). `["*"]` means all servers are allowed (subject to `deny`).
/// - `deny`: list of glob patterns matching server names that are denied.
///   `deny` takes precedence over `allow`.
/// - `allow_ip_literals`: whether IP literal server names are allowed. When
///   `false`, any server name that is an IP address (IPv4 or IPv6) or ends
///   with a port number is denied.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServerAclContent {
    /// Glob patterns of server names that are allowed.
    #[serde(default)]
    pub allow: Vec<String>,

    /// Glob patterns of server names that are denied (takes precedence over allow).
    #[serde(default)]
    pub deny: Vec<String>,

    /// Whether IP literal server names are allowed. Defaults to `true` per spec.
    #[serde(default = "default_allow_ip_literals")]
    pub allow_ip_literals: bool,
}

fn default_allow_ip_literals() -> bool {
    true
}

impl ServerAclContent {
    /// Parse the ACL content from a `serde_json::Value` (typically the `content`
    /// field of an `m.room.server_acl` state event).
    pub fn from_value(value: &serde_json::Value) -> Option<Self> {
        serde_json::from_value(value.clone()).ok()
    }

    /// Check whether a server name is allowed by this ACL policy.
    ///
    /// The check follows the Matrix specification:
    /// 1. If `allow_ip_literals` is `false` and the server name is an IP literal,
    ///    deny.
    /// 2. If the server name matches any pattern in `deny`, deny.
    /// 3. If the server name matches any pattern in `allow`, allow.
    /// 4. Otherwise, deny.
    pub fn is_server_allowed(&self, server_name: &str) -> bool {
        // Step 1: Check IP literals
        if !self.allow_ip_literals && is_ip_literal(server_name) {
            return false;
        }

        // Step 2: Check deny list (takes precedence)
        for pattern in &self.deny {
            if glob_match(pattern, server_name) {
                return false;
            }
        }

        // Step 3: Check allow list
        for pattern in &self.allow {
            if glob_match(pattern, server_name) {
                return true;
            }
        }

        // Step 4: Default deny if no allow pattern matched
        false
    }
}

/// Check if a server name is an IP literal (IPv4, IPv6, or has a port).
///
/// Per Synapse behavior, this checks:
/// - IPv4 addresses (e.g., `192.168.1.1`)
/// - IPv6 addresses (e.g., `[::1]`)
/// - Server names with ports (e.g., `example.com:443`)
fn is_ip_literal(server_name: &str) -> bool {
    // Check for IPv6 in brackets
    if server_name.starts_with('[') {
        return true;
    }

    // Check for port suffix (server:port)
    if server_name.rsplit_once(':').is_some() {
        // Could be a port — check if the part after ':' is numeric
        if let Some((_, port_str)) = server_name.rsplit_once(':') {
            if port_str.parse::<u16>().is_ok() {
                return true;
            }
        }
    }

    // Check for IPv4 address (4 dot-separated octets)
    let parts: Vec<&str> = server_name.split('.').collect();
    if parts.len() == 4 && parts.iter().all(|p| p.parse::<u8>().is_ok()) {
        return true;
    }

    false
}

/// Simple glob matching supporting `*` as a wildcard for any sequence of
/// characters (including empty). This matches the Matrix specification's
/// server ACL glob syntax.
///
/// Examples:
/// - `*` matches everything
/// - `*.example.com` matches `foo.example.com` but not `example.com`
/// - `example.com` matches only `example.com`
fn glob_match(pattern: &str, text: &str) -> bool {
    // Fast path: single `*` matches everything
    if pattern == "*" {
        return true;
    }

    let pattern_bytes = pattern.as_bytes();
    let text_bytes = text.as_bytes();
    glob_match_impl(pattern_bytes, text_bytes)
}

/// Recursive glob matching implementation.
fn glob_match_impl(pattern: &[u8], text: &[u8]) -> bool {
    let mut pi = 0;
    let mut ti = 0;
    let mut star_pi: Option<usize> = None;
    let mut star_ti = 0;

    while ti < text.len() {
        if pi < pattern.len() && (pattern[pi] == text[ti] || pattern[pi] == b'?') {
            pi += 1;
            ti += 1;
        } else if pi < pattern.len() && pattern[pi] == b'*' {
            star_pi = Some(pi);
            star_ti = ti;
            pi += 1;
        } else if let Some(sp) = star_pi {
            pi = sp + 1;
            star_ti += 1;
            ti = star_ti;
        } else {
            return false;
        }
    }

    // Consume trailing `*` in pattern
    while pi < pattern.len() && pattern[pi] == b'*' {
        pi += 1;
    }

    pi == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_default_acl_allows_all() {
        // When no m.room.server_acl event exists, all servers are allowed.
        // This is represented by the absence of an ACL, not by a default struct.
        // But if we do have a struct with allow=["*"], it should allow all.
        let acl = ServerAclContent {
            allow: vec!["*".to_string()],
            deny: vec![],
            allow_ip_literals: true,
        };
        assert!(acl.is_server_allowed("example.com"));
        assert!(acl.is_server_allowed("evil.com"));
        assert!(acl.is_server_allowed("192.168.1.1"));
    }

    #[test]
    fn test_deny_takes_precedence_over_allow() {
        let acl = ServerAclContent {
            allow: vec!["*".to_string()],
            deny: vec!["*.evil.com".to_string()],
            allow_ip_literals: true,
        };
        assert!(acl.is_server_allowed("example.com"));
        assert!(!acl.is_server_allowed("hackers.evil.com"));
        // *.evil.com does not match evil.com itself (no subdomain prefix)
        assert!(acl.is_server_allowed("evil.com"));
    }

    #[test]
    fn test_allow_specific_servers() {
        let acl = ServerAclContent {
            allow: vec!["*.example.com".to_string(), "matrix.org".to_string()],
            deny: vec![],
            allow_ip_literals: true,
        };
        assert!(acl.is_server_allowed("foo.example.com"));
        assert!(acl.is_server_allowed("matrix.org"));
        assert!(!acl.is_server_allowed("evil.com"));
        assert!(!acl.is_server_allowed("example.com")); // doesn't match *.example.com
    }

    #[test]
    fn test_deny_ip_literals() {
        let acl = ServerAclContent {
            allow: vec!["*".to_string()],
            deny: vec![],
            allow_ip_literals: false,
        };
        assert!(acl.is_server_allowed("example.com"));
        assert!(!acl.is_server_allowed("192.168.1.1"));
        assert!(!acl.is_server_allowed("[::1]"));
        assert!(!acl.is_server_allowed("example.com:443"));
    }

    #[test]
    fn test_empty_allow_denies_all() {
        let acl = ServerAclContent {
            allow: vec![],
            deny: vec![],
            allow_ip_literals: true,
        };
        assert!(!acl.is_server_allowed("example.com"));
        assert!(!acl.is_server_allowed("any.server"));
    }

    #[test]
    fn test_from_value_parses_correctly() {
        let content = json!({
            "allow": ["*.example.com", "*"],
            "deny": ["*.evil.com"],
            "allow_ip_literals": false
        });
        let acl = ServerAclContent::from_value(&content).expect("should parse");
        assert_eq!(acl.allow, vec!["*.example.com", "*"]);
        assert_eq!(acl.deny, vec!["*.evil.com"]);
        assert!(!acl.allow_ip_literals);
    }

    #[test]
    fn test_from_value_defaults_allow_ip_literals_to_true() {
        let content = json!({
            "allow": ["*"],
            "deny": []
        });
        let acl = ServerAclContent::from_value(&content).expect("should parse");
        assert!(acl.allow_ip_literals);
    }

    #[test]
    fn test_glob_match_wildcard() {
        assert!(glob_match("*", "anything"));
        assert!(glob_match("*.com", "example.com"));
        assert!(!glob_match("*.com", "example.org"));
        assert!(glob_match("example.com", "example.com"));
        assert!(!glob_match("example.com", "evil.com"));
    }

    #[test]
    fn test_is_ip_literal() {
        assert!(is_ip_literal("192.168.1.1"));
        assert!(is_ip_literal("10.0.0.1"));
        assert!(is_ip_literal("[::1]"));
        assert!(is_ip_literal("[2001:db8::1]"));
        assert!(is_ip_literal("example.com:443"));
        assert!(!is_ip_literal("example.com"));
        assert!(!is_ip_literal("sub.example.com"));
    }
}
