use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub requests_per_second: u32,
    pub burst_size: u32,
    pub per_user: bool,
    pub per_ip: bool,
    pub window_seconds: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_second: 10,
            burst_size: 100,
            per_user: true,
            per_ip: true,
            window_seconds: 60,
        }
    }
}

#[derive(Debug, Clone)]
struct RateLimitEntry {
    tokens: u32,
    last_refill: Instant,
    blocked_until: Option<Instant>,
    request_count: u64,
}

impl RateLimitEntry {
    fn new(burst_size: u32) -> Self {
        Self {
            tokens: burst_size,
            last_refill: Instant::now(),
            blocked_until: None,
            request_count: 0,
        }
    }

    fn refill(&mut self, rate: u32, burst: u32) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill);
        let tokens_to_add = (elapsed.as_secs_f64() * rate as f64) as u32;
        
        if tokens_to_add > 0 {
            self.tokens = (self.tokens + tokens_to_add).min(burst);
            self.last_refill = now;
        }
    }

    fn try_consume(&mut self, tokens: u32) -> bool {
        if self.tokens >= tokens {
            self.tokens -= tokens;
            self.request_count += 1;
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitInfo {
    pub limit: u32,
    pub remaining: u32,
    pub reset_seconds: u64,
    pub retry_after: Option<u64>,
}

pub struct RateLimiter {
    config: RateLimitConfig,
    user_entries: Arc<RwLock<HashMap<String, RateLimitEntry>>>,
    ip_entries: Arc<RwLock<HashMap<String, RateLimitEntry>>>,
    endpoint_entries: Arc<RwLock<HashMap<String, RateLimitEntry>>>,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            user_entries: Arc::new(RwLock::new(HashMap::new())),
            ip_entries: Arc::new(RwLock::new(HashMap::new())),
            endpoint_entries: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn check_rate_limit(
        &self,
        user_id: Option<&str>,
        ip: &str,
        endpoint: &str,
    ) -> Result<RateLimitInfo, RateLimitInfo> {
        let config = &self.config;
        
        if config.per_user {
            if let Some(uid) = user_id {
                let key = format!("user:{}", uid);
                self.check_entry(&key, config).await?;
            }
        }

        if config.per_ip {
            let key = format!("ip:{}", ip);
            self.check_entry(&key, config).await?;
        }

        let endpoint_config = self.get_endpoint_config(endpoint);
        let key = format!("endpoint:{}", endpoint);
        self.check_entry(&key, &endpoint_config).await
    }

    async fn check_entry(
        &self,
        key: &str,
        config: &RateLimitConfig,
    ) -> Result<RateLimitInfo, RateLimitInfo> {
        let entries = if key.starts_with("user:") {
            self.user_entries.clone()
        } else if key.starts_with("ip:") {
            self.ip_entries.clone()
        } else {
            self.endpoint_entries.clone()
        };

        let mut entries = entries.write().await;
        let entry = entries
            .entry(key.to_string())
            .or_insert_with(|| RateLimitEntry::new(config.burst_size));

        entry.refill(config.requests_per_second, config.burst_size);

        if let Some(blocked_until) = entry.blocked_until {
            if Instant::now() < blocked_until {
                let retry_after = blocked_until.duration_since(Instant::now()).as_secs();
                return Err(RateLimitInfo {
                    limit: config.burst_size,
                    remaining: 0,
                    reset_seconds: config.window_seconds,
                    retry_after: Some(retry_after),
                });
            } else {
                entry.blocked_until = None;
            }
        }

        if entry.try_consume(1) {
            Ok(RateLimitInfo {
                limit: config.burst_size,
                remaining: entry.tokens,
                reset_seconds: config.window_seconds,
                retry_after: None,
            })
        } else {
            let blocked_until = Instant::now() + Duration::from_secs(config.window_seconds);
            entry.blocked_until = Some(blocked_until);
            
            Err(RateLimitInfo {
                limit: config.burst_size,
                remaining: 0,
                reset_seconds: config.window_seconds,
                retry_after: Some(config.window_seconds),
            })
        }
    }

    fn get_endpoint_config(&self, endpoint: &str) -> RateLimitConfig {
        if endpoint.contains("/login") || endpoint.contains("/register") {
            RateLimitConfig {
                requests_per_second: 1,
                burst_size: 5,
                window_seconds: 300,
                ..Default::default()
            }
        } else if endpoint.contains("/sync") {
            RateLimitConfig {
                requests_per_second: 5,
                burst_size: 50,
                window_seconds: 60,
                ..Default::default()
            }
        } else if endpoint.contains("/send") {
            RateLimitConfig {
                requests_per_second: 20,
                burst_size: 200,
                window_seconds: 60,
                ..Default::default()
            }
        } else {
            self.config.clone()
        }
    }

    pub async fn cleanup_expired(&self) {
        let mut user_entries = self.user_entries.write().await;
        let mut ip_entries = self.ip_entries.write().await;
        let mut endpoint_entries = self.endpoint_entries.write().await;

        let now = Instant::now();
        let expire_threshold = Duration::from_secs(self.config.window_seconds * 2);

        user_entries.retain(|_, entry| {
            now.duration_since(entry.last_refill) < expire_threshold
        });
        ip_entries.retain(|_, entry| {
            now.duration_since(entry.last_refill) < expire_threshold
        });
        endpoint_entries.retain(|_, entry| {
            now.duration_since(entry.last_refill) < expire_threshold
        });
    }

    pub async fn get_stats(&self) -> RateLimitStats {
        let user_entries = self.user_entries.read().await;
        let ip_entries = self.ip_entries.read().await;
        let endpoint_entries = self.endpoint_entries.read().await;

        RateLimitStats {
            active_users: user_entries.len(),
            active_ips: ip_entries.len(),
            active_endpoints: endpoint_entries.len(),
            total_requests: user_entries.values().map(|e| e.request_count).sum::<u64>()
                + ip_entries.values().map(|e| e.request_count).sum::<u64>()
                + endpoint_entries.values().map(|e| e.request_count).sum::<u64>(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitStats {
    pub active_users: usize,
    pub active_ips: usize,
    pub active_endpoints: usize,
    pub total_requests: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter_allows_within_limit() {
        let config = RateLimitConfig {
            requests_per_second: 10,
            burst_size: 5,
            ..Default::default()
        };
        let limiter = RateLimiter::new(config);

        for _ in 0..5 {
            let result = limiter.check_rate_limit(Some("@user:test.com"), "127.0.0.1", "/test").await;
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_rate_limiter_blocks_over_limit() {
        let config = RateLimitConfig {
            requests_per_second: 1,
            burst_size: 2,
            ..Default::default()
        };
        let limiter = RateLimiter::new(config);

        limiter.check_rate_limit(Some("@user:test.com"), "127.0.0.1", "/test").await.ok();
        limiter.check_rate_limit(Some("@user:test.com"), "127.0.0.1", "/test").await.ok();
        
        let result = limiter.check_rate_limit(Some("@user:test.com"), "127.0.0.1", "/test").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_rate_limit_info() {
        let config = RateLimitConfig {
            requests_per_second: 10,
            burst_size: 100,
            ..Default::default()
        };
        let limiter = RateLimiter::new(config);

        let info = limiter.check_rate_limit(Some("@user:test.com"), "127.0.0.1", "/test").await.unwrap();
        assert_eq!(info.limit, 100);
        assert!(info.remaining < 100);
        assert!(info.retry_after.is_none());
    }

    #[tokio::test]
    async fn test_endpoint_specific_limits() {
        let config = RateLimitConfig::default();
        let limiter = RateLimiter::new(config);

        let login_config = limiter.get_endpoint_config("/login");
        assert_eq!(login_config.burst_size, 5);

        let sync_config = limiter.get_endpoint_config("/sync");
        assert_eq!(sync_config.burst_size, 50);

        let send_config = limiter.get_endpoint_config("/rooms/!test/send");
        assert_eq!(send_config.burst_size, 200);
    }

    #[tokio::test]
    async fn test_rate_limit_stats() {
        let config = RateLimitConfig::default();
        let limiter = RateLimiter::new(config);

        limiter.check_rate_limit(Some("@user1:test.com"), "127.0.0.1", "/test").await.ok();
        limiter.check_rate_limit(Some("@user2:test.com"), "127.0.0.2", "/test").await.ok();

        let stats = limiter.get_stats().await;
        assert!(stats.active_users >= 2);
        assert!(stats.total_requests >= 2);
    }
}
