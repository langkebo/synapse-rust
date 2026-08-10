//! Centralized constants for the synapse-rust application
//!
//! This module contains all configuration constants to avoid magic numbers
//! and make the codebase more maintainable.

use std::time::Duration;

// ============================================================================
// Database & Cache Constants
// ============================================================================

/// Default TTL for cache entries in seconds
pub const DEFAULT_CACHE_TTL_SECONDS: u64 = 3600;

/// Default TTL for user profile cache entries in seconds
pub const USER_PROFILE_CACHE_TTL: u64 = 3600;

// ============================================================================
// Pagination & Limits
// ============================================================================

/// Maximum number of items allowed in paginated requests
pub const MAX_PAGINATION_LIMIT: i64 = 1000;

/// Default number of items per page
pub const DEFAULT_PAGE_SIZE: i64 = 20;

/// Minimum pagination limit
pub const MIN_PAGINATION_LIMIT: i64 = 1;

// ============================================================================
// Rate Limiting Constants
// ============================================================================

/// Max nonce requests per IP per hour
pub const ADMIN_REGISTER_NONCE_RATE_LIMIT: u32 = 3;

/// Max registration attempts per IP per hour
pub const ADMIN_REGISTER_RATE_LIMIT: u32 = 2;

/// Token bucket rate limit capacity
pub const TOKEN_BUCKET_CAPACITY: u32 = 10;

// ============================================================================
// Time Durations
// ============================================================================

/// Session token lifetime (30 minutes)
pub const SESSION_MAX_LIFETIME_SECS: u64 = 1800;

/// Session idle timeout (10 minutes)
pub const SESSION_IDLE_TIMEOUT_SECS: u64 = 600;

/// Burn-after-read delay before event redaction (30 seconds)
pub const BURN_AFTER_READ_DELAY_SECS: u64 = 30;

// ============================================================================
// Validation Constants
// ============================================================================

/// Minimum username length
pub const MIN_USERNAME_LENGTH: usize = 1;

/// Maximum username length
pub const MAX_USERNAME_LENGTH: usize = 255;

/// Maximum password length
pub const MAX_PASSWORD_LENGTH: usize = 128;

/// Minimum password length
pub const MIN_PASSWORD_LENGTH: usize = 8;

/// Maximum display name length
pub const MAX_DISPLAY_NAME_LENGTH: usize = 256;

/// Maximum reason/message length
pub const MAX_REASON_LENGTH: usize = 512;

/// Maximum message content length
pub const MAX_MESSAGE_LENGTH: usize = 65536;

/// Maximum device ID length
pub const MAX_DEVICE_ID_LENGTH: usize = 255;

/// Maximum room alias length
pub const MAX_ROOM_ALIAS_LENGTH: usize = 255;

// ============================================================================
// File Size Limits
// ============================================================================

/// Maximum size for voice data files (10MB)
pub const MAX_VOICE_DATA_SIZE: usize = 10 * 1024 * 1024;

// ============================================================================
// Room Defaults
// ============================================================================

/// Default room join rule
pub const DEFAULT_JOIN_RULE: &str = "invite";

/// Default room history visibility
pub const DEFAULT_HISTORY_VISIBILITY: &str = "joined";

/// Default room guest access
pub const DEFAULT_GUEST_ACCESS: &str = "forbidden";

// ============================================================================
// Timestamp Validation Window
// ============================================================================

/// Acceptable timestamp window in seconds (1 year in each direction)
pub const TIMESTAMP_WINDOW_SECONDS: i64 = 5 * 60;

// ============================================================================
// Token Expiry
// ============================================================================

/// Default access token expiry in seconds (1 hour)
pub const DEFAULT_ACCESS_TOKEN_EXPIRY_SECS: i64 = 3600;

/// Default refresh token expiry in seconds (7 days)
pub const DEFAULT_REFRESH_TOKEN_EXPIRY_SECS: i64 = 604800;

// ============================================================================
// Database Connection Pool
// ============================================================================

/// Default maximum database connections
pub const DEFAULT_DB_MAX_CONNECTIONS: u32 = 5;

/// Database connection acquire timeout in seconds
pub const DB_ACQUIRE_TIMEOUT_SECS: u64 = 10;

// ============================================================================
// Helper Functions
// ============================================================================

/// Convert seconds to Duration
pub const fn secs(duration_secs: u64) -> Duration {
    Duration::from_secs(duration_secs)
}

/// Convert milliseconds to Duration
pub const fn millis(duration_ms: u64) -> Duration {
    Duration::from_millis(duration_ms)
}

/// 同步请求服务端外层超时相对客户端 `timeout` 参数的宽余量（毫秒）。
///
/// 客户端长轮询 `timeout` 到期后会自行重试；服务端必须略晚于客户端超时，
/// 否则客户端永远收不到「超时空响应」而只能看到连接被截断。
pub const SYNC_SERVER_TIMEOUT_BUFFER_MS: u64 = 15_000;

/// S13/N6: 同步链路统一的服务端外层超时 = 客户端 timeout + 固定宽余量。
///
/// `/sync`（handlers/sync.rs）与 `/rooms/{id}/sync`（room_sync_with_timeout）
/// 必须使用同一公式；此前后者硬编码 60s，客户端 timeout=120s 会被提前截断。
/// 饱和加法防止恶意/异常的超大 timeout 溢出。
pub const fn sync_server_timeout(client_timeout_ms: u64) -> Duration {
    Duration::from_millis(client_timeout_ms.saturating_add(SYNC_SERVER_TIMEOUT_BUFFER_MS))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_duration_constants() {
        assert_eq!(secs(30), Duration::from_secs(30));
        assert_eq!(millis(500), Duration::from_millis(500));
    }

    #[test]
    fn test_sync_server_timeout_adds_buffer() {
        // 客户端 timeout=30s → 服务端 45s
        assert_eq!(sync_server_timeout(30_000), Duration::from_secs(45));
        // 客户端 timeout=120s 不再被 60s 硬编码截断
        assert_eq!(sync_server_timeout(120_000), Duration::from_secs(135));
        // 客户端 timeout=0（短轮询）仍有宽余量
        assert_eq!(sync_server_timeout(0), Duration::from_secs(15));
    }

    #[test]
    fn test_sync_server_timeout_saturates_on_huge_input() {
        // u64::MAX 毫秒本就超出 Duration 可表示范围，饱和加法保证不 panic、不溢出回绕
        let d = sync_server_timeout(u64::MAX - 1);
        assert!(d >= Duration::from_millis(u64::MAX - 1));
    }

    #[test]
    fn test_limit_values() {
        assert_eq!(MAX_PAGINATION_LIMIT, 1000);
        assert_eq!(DEFAULT_PAGE_SIZE, 20);
        assert_eq!(MIN_PAGINATION_LIMIT, 1);
    }

    #[test]
    fn test_validation_limits() {
        assert_eq!(MIN_USERNAME_LENGTH, 1);
        assert_eq!(MAX_USERNAME_LENGTH, 255);
        assert_eq!(MAX_PASSWORD_LENGTH, 128);
        assert_eq!(MIN_PASSWORD_LENGTH, 8);
    }
}
