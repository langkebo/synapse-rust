use std::time::Duration;

pub struct CacheKeyBuilder;

impl CacheKeyBuilder {
    pub fn user_profile(user_id: &str) -> String {
        format!("user:{}:profile", user_id)
    }

    pub fn user_presence(user_id: &str) -> String {
        format!("user:{}:presence", user_id)
    }

    pub fn user_devices(user_id: &str) -> String {
        format!("user:{}:devices", user_id)
    }

    pub fn room_info(room_id: &str) -> String {
        format!("room:{}:info", room_id)
    }

    pub fn room_members(room_id: &str) -> String {
        format!("room:{}:members", room_id)
    }

    pub fn room_state(room_id: &str) -> String {
        format!("room:{}:state", room_id)
    }

    pub fn room_events(room_id: &str) -> String {
        format!("room:{}:events", room_id)
    }

    pub fn room_messages(room_id: &str) -> String {
        format!("room:{}:messages", room_id)
    }

    pub fn token(token: &str) -> String {
        format!("token:{}", token)
    }

    pub fn public_rooms() -> String {
        "public_rooms".to_string()
    }

    pub fn user_rooms(user_id: &str) -> String {
        format!("user:{}:rooms", user_id)
    }

    pub fn rate_limit(user_id: &str, endpoint: &str) -> String {
        format!("ratelimit:{}:{}", user_id, endpoint)
    }

    pub fn ip_rate_limit(ip: &str, endpoint: &str) -> String {
        format!("ratelimit:ip:{}:{}", ip, endpoint)
    }

    pub fn user_not_found(user_id: &str) -> String {
        format!("user:{}:not_found", user_id)
    }

    pub fn room_not_found(room_id: &str) -> String {
        format!("room:{}:not_found", room_id)
    }

    // Negative cache keys - cache "not found" results to prevent repeated lookups
    pub fn user_not_found_v2(user_id: &str) -> String {
        format!("user:{}:nf:v2", user_id)
    }

    pub fn room_not_found_v2(room_id: &str) -> String {
        format!("room:{}:nf:v2", room_id)
    }

    pub fn event_not_found(event_id: &str) -> String {
        format!("event:{}:not_found", event_id)
    }

    // Batch keys for multi-fetch operations
    pub fn room_batch(room_ids: &[String]) -> String {
        let mut ids = room_ids.to_vec();
        ids.sort();
        format!("room:batch:{}:info", ids.join(","))
    }

    pub fn user_batch(user_ids: &[String]) -> String {
        let mut ids = user_ids.to_vec();
        ids.sort();
        format!("user:batch:{}:profile", ids.join(","))
    }
}

pub struct CacheTtl;

impl CacheTtl {
    pub fn user_profile() -> Duration {
        Duration::from_secs(3600) // 1 hour - profiles rarely change
    }

    pub fn user_presence() -> Duration {
        Duration::from_secs(60) // 1 min - balance freshness and hit rate
    }

    pub fn user_devices() -> Duration {
        Duration::from_secs(1800) // 30 min - devices are stable
    }

    pub fn room_info() -> Duration {
        Duration::from_secs(1800) // 30 min - room metadata is stable
    }

    pub fn room_members() -> Duration {
        Duration::from_secs(900) // 15 min - membership changes are rare
    }

    pub fn room_state() -> Duration {
        Duration::from_secs(1200) // 20 min - room state is relatively stable
    }

    pub fn room_events() -> Duration {
        Duration::from_secs(900) // 15 min - events rarely change once created
    }

    pub fn room_messages() -> Duration {
        Duration::from_secs(900) // 15 min - messages are immutable
    }

    pub fn token() -> Duration {
        Duration::from_secs(86400) // 24 hours
    }

    pub fn public_rooms() -> Duration {
        Duration::from_secs(900) // 15 min - public room list is stable
    }

    pub fn user_rooms() -> Duration {
        Duration::from_secs(600) // 10 min - user's room list changes occasionally
    }

    pub fn rate_limit() -> Duration {
        Duration::from_secs(60)
    }

    pub fn not_found() -> Duration {
        Duration::from_secs(300) // 5 min - prevent rapid re-fetching of missing data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_key_user_profile() {
        let key = CacheKeyBuilder::user_profile("@user:example.com");
        assert_eq!(key, "user:@user:example.com:profile");
    }

    #[test]
    fn test_cache_key_user_presence() {
        let key = CacheKeyBuilder::user_presence("@user:example.com");
        assert_eq!(key, "user:@user:example.com:presence");
    }

    #[test]
    fn test_cache_key_room_info() {
        let key = CacheKeyBuilder::room_info("!room:example.com");
        assert_eq!(key, "room:!room:example.com:info");
    }

    #[test]
    fn test_cache_key_room_members() {
        let key = CacheKeyBuilder::room_members("!room:example.com");
        assert_eq!(key, "room:!room:example.com:members");
    }

    #[test]
    fn test_cache_key_token() {
        let key = CacheKeyBuilder::token("abc123");
        assert_eq!(key, "token:abc123");
    }

    #[test]
    fn test_cache_key_public_rooms() {
        let key = CacheKeyBuilder::public_rooms();
        assert_eq!(key, "public_rooms");
    }

    #[test]
    fn test_cache_key_rate_limit() {
        let key = CacheKeyBuilder::rate_limit("@user:example.com", "/login");
        assert_eq!(key, "ratelimit:@user:example.com:/login");
    }

    #[test]
    fn test_cache_key_ip_rate_limit() {
        let key = CacheKeyBuilder::ip_rate_limit("192.168.1.1", "/login");
        assert_eq!(key, "ratelimit:ip:192.168.1.1:/login");
    }

    #[test]
    fn test_cache_key_user_not_found() {
        let key = CacheKeyBuilder::user_not_found("@user:example.com");
        assert_eq!(key, "user:@user:example.com:not_found");
    }

    #[test]
    fn test_cache_key_room_not_found() {
        let key = CacheKeyBuilder::room_not_found("!room:example.com");
        assert_eq!(key, "room:!room:example.com:not_found");
    }

    #[test]
    fn test_cache_ttl_user_profile() {
        let ttl = CacheTtl::user_profile();
        assert_eq!(ttl, Duration::from_secs(3600));
    }

    #[test]
    fn test_cache_ttl_user_presence() {
        let ttl = CacheTtl::user_presence();
        assert_eq!(ttl, Duration::from_secs(60));
    }

    #[test]
    fn test_cache_ttl_room_info() {
        let ttl = CacheTtl::room_info();
        assert_eq!(ttl, Duration::from_secs(1800));
    }

    #[test]
    fn test_cache_ttl_token() {
        let ttl = CacheTtl::token();
        assert_eq!(ttl, Duration::from_secs(86400));
    }

    #[test]
    fn test_cache_ttl_not_found() {
        let ttl = CacheTtl::not_found();
        assert_eq!(ttl, Duration::from_secs(300));
    }
}
