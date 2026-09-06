use std::time::Duration;

/// Centralised factory for constructing canonical Redis key strings.
///
/// Using a typed builder instead of raw strings prevents key collisions
/// and makes cache invalidation auditable (grep the module for the key).
pub struct CacheKeyBuilder;

impl CacheKeyBuilder {
    /// Builds the cache key for a user's profile data.
    pub fn user_profile(user_id: &str) -> String {
        format!("user:{user_id}:profile")
    }

    /// Builds the cache key for a user's presence state.
    pub fn user_presence(user_id: &str) -> String {
        format!("user:{user_id}:presence")
    }

    /// Builds the cache key for a user's device list.
    pub fn user_devices(user_id: &str) -> String {
        format!("user:{user_id}:devices")
    }

    /// Builds the cache key for a room's metadata (name, topic, avatar, etc.).
    pub fn room_info(room_id: &str) -> String {
        format!("room:{room_id}:info")
    }

    /// Builds the cache key for a room's member list.
    pub fn room_members(room_id: &str) -> String {
        format!("room:{room_id}:members")
    }

    /// Builds the cache key for a room's current state (state events).
    pub fn room_state(room_id: &str) -> String {
        format!("room:{room_id}:state")
    }

    /// Builds the cache key for a room's recent events.
    pub fn room_events(room_id: &str) -> String {
        format!("room:{room_id}:events")
    }

    /// Builds the cache key for a room's message batch.
    pub fn room_messages(room_id: &str) -> String {
        format!("room:{room_id}:messages")
    }

    /// Builds the cache key for an access/refresh token entry.
    pub fn token(token: &str) -> String {
        format!("token:{token}")
    }

    /// Builds the cache key for the global public room list.
    pub fn public_rooms() -> String {
        "public_rooms".to_string()
    }

    /// Builds the cache key for a user's joined-room list.
    pub fn user_rooms(user_id: &str) -> String {
        format!("user:{user_id}:rooms")
    }

    /// Builds the cache key for per-user, per-endpoint rate-limit state.
    pub fn rate_limit(user_id: &str, endpoint: &str) -> String {
        format!("ratelimit:{user_id}:{endpoint}")
    }

    /// Builds the cache key for per-IP, per-endpoint rate-limit state.
    pub fn ip_rate_limit(ip: &str, endpoint: &str) -> String {
        format!("ratelimit:ip:{ip}:{endpoint}")
    }

    /// Cache key for per-origin federation rate limiting.
    pub fn federation_origin_rate_limit(origin: &str, endpoint: &str) -> String {
        format!("ratelimit:fed:{origin}:{endpoint}")
    }

    /// Negative-cache key: records that a user lookup returned "not found".
    /// Caches the negative result so repeated lookups hit cache instead of DB.
    pub fn user_not_found(user_id: &str) -> String {
        format!("user:{user_id}:not_found")
    }

    /// Negative-cache key for a room that does not exist.
    pub fn room_not_found(room_id: &str) -> String {
        format!("room:{room_id}:not_found")
    }

    // Negative cache keys - cache "not found" results to prevent repeated lookups
    /// V2 negative-cache key for a user that does not exist.
    pub fn user_not_found_v2(user_id: &str) -> String {
        format!("user:{user_id}:nf:v2")
    }

    /// V2 negative-cache key for a room that does not exist.
    pub fn room_not_found_v2(room_id: &str) -> String {
        format!("room:{room_id}:nf:v2")
    }

    /// Negative-cache key for an event that does not exist.
    pub fn event_not_found(event_id: &str) -> String {
        format!("event:{event_id}:not_found")
    }

    // Batch keys for multi-fetch operations
    /// Builds a cache key for a batch of rooms' info, stable regardless of
    /// the order of `room_ids` (keys are sorted before hashing).
    pub fn room_batch(room_ids: &[String]) -> String {
        let mut ids = room_ids.to_vec();
        ids.sort();
        format!("room:batch:{}:info", ids.join(","))
    }

    /// Builds a cache key for a batch of users' profiles, stable regardless of
    /// the order of `user_ids` (keys are sorted before hashing).
    pub fn user_batch(user_ids: &[String]) -> String {
        let mut ids = user_ids.to_vec();
        ids.sort();
        format!("user:batch:{}:profile", ids.join(","))
    }
}

pub struct CacheTtl;

impl CacheTtl {
    /// Returns the TTL for user profile cache entries.
    pub fn user_profile() -> Duration {
        Duration::from_secs(3600) // 1 hour - profiles rarely change
    }

    /// Returns the TTL for user presence cache entries.
    pub fn user_presence() -> Duration {
        Duration::from_secs(60) // 1 min - balance freshness and hit rate
    }

    /// Returns the TTL for user device-list cache entries.
    pub fn user_devices() -> Duration {
        Duration::from_secs(1800) // 30 min - devices are stable
    }

    /// Returns the TTL for room-metadata cache entries.
    pub fn room_info() -> Duration {
        Duration::from_secs(1800) // 30 min - room metadata is stable
    }

    /// Returns the TTL for room-member-list cache entries.
    pub fn room_members() -> Duration {
        Duration::from_secs(900) // 15 min - membership changes are rare
    }

    /// Returns the TTL for room-state cache entries.
    pub fn room_state() -> Duration {
        Duration::from_secs(1200) // 20 min - room state is relatively stable
    }

    /// Returns the TTL for room-event cache entries.
    pub fn room_events() -> Duration {
        Duration::from_secs(900) // 15 min - events rarely change once created
    }

    /// Returns the TTL for room-message-batch cache entries.
    pub fn room_messages() -> Duration {
        Duration::from_secs(900) // 15 min - messages are immutable
    }

    /// Returns the TTL for access/refresh token cache entries.
    ///
    /// Kept short to respect revocation (logout, admin deactivation).
    pub fn token() -> Duration {
        Duration::from_secs(300) // 5 min - must be short to respect revocation
    }

    /// Returns the TTL for the public-room-list cache entry.
    pub fn public_rooms() -> Duration {
        Duration::from_secs(900) // 15 min - public room list is stable
    }

    /// Returns the TTL for a user's joined-room-list cache entry.
    pub fn user_rooms() -> Duration {
        Duration::from_secs(600) // 10 min - user's room list changes occasionally
    }

    /// Returns the TTL for rate-limit state cache entries.
    pub fn rate_limit() -> Duration {
        Duration::from_secs(60)
    }

    /// Returns the TTL for negative-cache entries ("not found" results).
    ///
    /// Caches that an entity does not exist so repeated lookups hit cache
    /// instead of the database.
    pub fn not_found() -> Duration {
        Duration::from_secs(300) // 5 min - prevent rapid re-fetching of missing data
    }
}

#[cfg(test)]
#[allow(missing_docs)]
mod tests {
    use super::*;

    #[test]
    #[allow(missing_docs)]
    fn test_cache_key_user_profile() {
        let key = CacheKeyBuilder::user_profile("@user:example.com");
        assert_eq!(key, "user:@user:example.com:profile");
    }

    #[test]
    #[allow(missing_docs)]
    fn test_cache_key_user_presence() {
        let key = CacheKeyBuilder::user_presence("@user:example.com");
        assert_eq!(key, "user:@user:example.com:presence");
    }

    #[test]
    #[allow(missing_docs)]
    fn test_cache_key_room_info() {
        let key = CacheKeyBuilder::room_info("!room:example.com");
        assert_eq!(key, "room:!room:example.com:info");
    }

    #[test]
    #[allow(missing_docs)]
    fn test_cache_key_room_members() {
        let key = CacheKeyBuilder::room_members("!room:example.com");
        assert_eq!(key, "room:!room:example.com:members");
    }

    #[test]
    #[allow(missing_docs)]
    fn test_cache_key_token() {
        let key = CacheKeyBuilder::token("abc123");
        assert_eq!(key, "token:abc123");
    }

    #[test]
    #[allow(missing_docs)]
    fn test_cache_key_public_rooms() {
        let key = CacheKeyBuilder::public_rooms();
        assert_eq!(key, "public_rooms");
    }

    #[test]
    #[allow(missing_docs)]
    fn test_cache_key_rate_limit() {
        let key = CacheKeyBuilder::rate_limit("@user:example.com", "/login");
        assert_eq!(key, "ratelimit:@user:example.com:/login");
    }

    #[test]
    #[allow(missing_docs)]
    fn test_cache_key_ip_rate_limit() {
        let key = CacheKeyBuilder::ip_rate_limit("192.168.1.1", "/login");
        assert_eq!(key, "ratelimit:ip:192.168.1.1:/login");
    }

    #[test]
    #[allow(missing_docs)]
    fn test_cache_key_user_not_found() {
        let key = CacheKeyBuilder::user_not_found("@user:example.com");
        assert_eq!(key, "user:@user:example.com:not_found");
    }

    #[test]
    #[allow(missing_docs)]
    fn test_cache_key_room_not_found() {
        let key = CacheKeyBuilder::room_not_found("!room:example.com");
        assert_eq!(key, "room:!room:example.com:not_found");
    }

    #[test]
    #[allow(missing_docs)]
    fn test_cache_ttl_user_profile() {
        let ttl = CacheTtl::user_profile();
        assert_eq!(ttl, Duration::from_secs(3600));
    }

    #[test]
    #[allow(missing_docs)]
    fn test_cache_ttl_user_presence() {
        let ttl = CacheTtl::user_presence();
        assert_eq!(ttl, Duration::from_secs(60));
    }

    #[test]
    #[allow(missing_docs)]
    fn test_cache_ttl_room_info() {
        let ttl = CacheTtl::room_info();
        assert_eq!(ttl, Duration::from_secs(1800));
    }

    #[test]
    #[allow(missing_docs)]
    fn test_cache_ttl_token() {
        let ttl = CacheTtl::token();
        assert_eq!(ttl, Duration::from_secs(300)); // 5 min - must be short to respect revocation
    }

    #[test]
    #[allow(missing_docs)]
    fn test_cache_ttl_not_found() {
        let ttl = CacheTtl::not_found();
        assert_eq!(ttl, Duration::from_secs(300));
    }
}
