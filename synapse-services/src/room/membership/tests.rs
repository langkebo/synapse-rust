#[cfg(test)]
mod tests {
    use crate::room::membership::service::MembershipService;

    // ── server_name_from_id ────────────────────────────────────────────

    #[test]
    fn server_name_from_id_extracts_server() {
        assert_eq!(MembershipService::server_name_from_id("@user:example.com"), Some("example.com"));
        assert_eq!(MembershipService::server_name_from_id("!room:matrix.org"), Some("matrix.org"));
    }

    #[test]
    fn server_name_from_id_no_colon_returns_none() {
        assert_eq!(MembershipService::server_name_from_id("@user"), None);
        assert_eq!(MembershipService::server_name_from_id("plainstring"), None);
    }

    #[test]
    fn server_name_from_id_multiple_colons_uses_last() {
        assert_eq!(MembershipService::server_name_from_id("@user:sub.example.com:443"), Some("443"));
    }

    // ── is_remote_id ───────────────────────────────────────────────────

    #[test]
    fn is_remote_id_different_server_is_remote() {
        assert!(MembershipService::is_remote_id("@user:other.com", "example.com"));
        assert!(MembershipService::is_remote_id("!room:remote.org", "example.com"));
    }

    #[test]
    fn is_remote_id_same_server_is_local() {
        assert!(!MembershipService::is_remote_id("@user:example.com", "example.com"));
        assert!(!MembershipService::is_remote_id("!room:example.com", "example.com"));
    }

    #[test]
    fn is_remote_id_no_server_is_local() {
        assert!(!MembershipService::is_remote_id("@user", "example.com"));
    }
}
