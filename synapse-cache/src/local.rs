use crate::error::CacheConfig;
use moka::sync::Cache;
use std::collections::HashMap;
use std::sync::Arc;
use synapse_common::claims::Claims;

/// In-process moka-based cache with per-key TTL support and isolated hot namespaces.
///
/// Hot traffic domains (presence, sliding-sync, device-keys, room-state) are routed to
/// independent moka instances so a flood in one domain cannot evict the others' entries.
#[derive(Clone, Debug)]
pub struct LocalCache {
    pub(crate) cache: Cache<String, String>,
    /// D-1: per-key 过期截止时间。moka 0.12 `sync::Cache` 没有 per-entry TTL API，
    /// 用旁路 deadline 表实现「L1 与 L2 Redis 相同的 per-key TTL」。
    /// 用 `RwLock` 而非 `Mutex`：`get_raw` 只读判断，只需 `read()` 锁，
    /// 避免将所有 worker 线程的缓存读串行化在一把排他锁上。
    deadlines: Arc<parking_lot::RwLock<HashMap<String, std::time::Instant>>>,
    /// D-2: 独立命名空间缓存，防止高流量域（如 presence）驱逐
    /// 安全关键数据（如 device_keys、token）。每个命名空间有独立
    /// 的 moka 容量和 deadline 表。
    pub(crate) namespaces: Arc<HashMap<&'static str, NamespaceCache>>,
}

/// D-2: 独立命名空间缓存实例，拥有独立的 moka Cache 和 deadline 表。
#[derive(Clone, Debug)]
pub struct NamespaceCache {
    pub(crate) cache: Cache<String, String>,
    pub(crate) deadlines: Arc<parking_lot::RwLock<HashMap<String, std::time::Instant>>>,
}

/// D-2: 将缓存键路由到对应的命名空间。
/// 返回 None 表示使用通用缓存实例。
pub fn route_key(key: &str) -> Option<&'static str> {
    // presence 数据：高写入频率、短 TTL（60s），不应驱逐其他数据
    if key.starts_with("user:") && key.ends_with(":presence") {
        return Some("presence");
    }
    // sliding_sync 去重/扩展数据：高写入量、中等 TTL
    if key.starts_with("sliding_sync:") {
        return Some("sliding_sync");
    }
    // device_keys：安全关键数据，不应被 presence 洪水驱逐
    if key.starts_with("device_keys_bulk:") {
        return Some("device_keys");
    }
    // room_state：中等写入量、中等 TTL
    if key.starts_with("room_state:") {
        return Some("room_state");
    }
    None
}

/// B2: 命名空间 → 其成员键共享的前导段。`presence` 用 `user:` 探测，
/// 因为这是它唯一与其他域共有的前导（还需要 `:presence` 后缀才能精确路由，
/// 这里只求「不漏」的保守上界）。
const ROUTED_SEGMENTS: [(&str, &str); 4] = [
    ("presence", "user:"),
    ("sliding_sync", "sliding_sync:"),
    ("device_keys", "device_keys_bulk:"),
    ("room_state", "room_state:"),
];

impl LocalCache {
    /// B2: 可能含有以 `prefix` 开头的键的命名空间列表。
    ///
    /// 一个命名空间的键必然以它的前导段开头，因此只有当 `prefix` 与该段
    /// 互为前缀（`prefix.starts_with(seg)` 或 `seg.starts_with(prefix)`）时才可能命中，
    /// 否则两者在某个字符处发散，任何键都不可能同时匹配。
    pub(crate) fn namespaces_for_prefix(prefix: &str) -> Vec<&'static str> {
        ROUTED_SEGMENTS
            .iter()
            .filter(|(_, seg)| prefix.starts_with(seg) || seg.starts_with(prefix))
            .map(|(ns, _)| *ns)
            .collect()
    }

    /// B2: 通用（非命名空间）缓存是否可能含有以 `prefix` 开头的键。
    ///
    /// 当 `prefix` 已覆盖某个纯前缀路由段时，任何以此开头的键都会被路由到
    /// 对应命名空间，通用缓存不可能命中。`presence` 不在此列：它的路由还要求
    /// `:presence` 后缀，以 `user:` 开头的键仍可能落入通用缓存。
    pub(crate) fn generic_may_match_prefix(prefix: &str) -> bool {
        !(prefix.starts_with("sliding_sync:")
            || prefix.starts_with("device_keys_bulk:")
            || prefix.starts_with("room_state:"))
    }
}

impl NamespaceCache {
    fn new(max_capacity: u64, ttl_secs: u64) -> Self {
        let deadlines = Arc::new(parking_lot::RwLock::new(HashMap::new()));
        let deadlines_for_listener = Arc::clone(&deadlines);
        let cache = Cache::builder()
            .max_capacity(max_capacity)
            .time_to_live(std::time::Duration::from_secs(ttl_secs))
            .eviction_listener(move |key: Arc<String>, _value, cause| {
                drop_deadline_unless_replaced(&deadlines_for_listener, key.as_str(), &cause);
            })
            .build();
        Self { cache, deadlines }
    }
}

impl LocalCache {
    /// Constructs a new cache from the given configuration.
    pub fn new(config: &CacheConfig) -> Self {
        let deadlines = Arc::new(parking_lot::RwLock::new(HashMap::new()));
        let deadlines_for_listener = Arc::clone(&deadlines);
        let cache = Cache::builder()
            .max_capacity(config.max_capacity)
            .time_to_live(std::time::Duration::from_secs(config.time_to_live))
            // 容量/过期驱逐时同步清理 deadline，避免旁路表泄漏
            .eviction_listener(move |key: Arc<String>, _value, cause| {
                drop_deadline_unless_replaced(&deadlines_for_listener, key.as_str(), &cause);
            })
            .build();

        // D-2: 为高流量域创建独立 moka 实例，防止跨域驱逐
        let mut namespaces = HashMap::new();
        // presence: 高写入频率（每个用户每 60s 更新），短 TTL，容量 20K
        namespaces.insert("presence", NamespaceCache::new(20_000, 120));
        // sliding_sync: 去重键/扩展数据，中高写入量，容量 10K
        namespaces.insert("sliding_sync", NamespaceCache::new(10_000, 7200));
        // device_keys: 安全关键数据，不应被洪泛驱逐，容量 10K
        namespaces.insert("device_keys", NamespaceCache::new(10_000, 600));
        // room_state: 中等写入量，容量 20K
        namespaces.insert("room_state", NamespaceCache::new(20_000, 1200));

        Self { cache, deadlines, namespaces: Arc::new(namespaces) }
    }

    /// Looks up the cached claims associated with `token`.
    pub fn get(&self, token: &str) -> Option<Claims> {
        self.cache.get(token).and_then(|s| serde_json::from_str(&s).ok())
    }

    /// Serialises `claims` to JSON and stores it under `token`.
    pub fn set(&self, token: &str, claims: &Claims) {
        match serde_json::to_string(claims) {
            Ok(s) => {
                self.deadlines.write().remove(token);
                self.cache.insert(token.to_string(), s);
            }
            Err(e) => {
                tracing::error!(target: "cache", "Failed to serialize claims: {}", e);
            }
        }
    }

    /// Stores an arbitrary string value under `key` using the default TTL from the cache builder.
    pub fn set_raw(&self, key: &str, value: &str) {
        // D-2: 路由到独立命名空间缓存（如有）
        if let Some(ns_name) = route_key(key) {
            if let Some(ns) = self.namespaces.get(ns_name) {
                ns.deadlines.write().remove(key);
                ns.cache.insert(key.to_string(), value.to_string());
                return;
            }
        }
        // 无 per-key TTL 的普通写入：清除旧 deadline，回落到 builder 级 TTL
        self.deadlines.write().remove(key);
        self.cache.insert(key.to_string(), value.to_string());
    }

    /// D-1: 带独立 TTL 的写入。此前所有条目共用 builder 级 TTL，
    /// 调用方传入的 ttl 只作用于 L2 Redis，L1 与 L2 过期时间不一致。
    pub fn set_raw_with_ttl(&self, key: &str, value: &str, ttl: std::time::Duration) {
        // D-2: 路由到独立命名空间缓存（如有）
        if let Some(ns_name) = route_key(key) {
            if let Some(ns) = self.namespaces.get(ns_name) {
                ns.deadlines.write().insert(key.to_string(), std::time::Instant::now() + ttl);
                ns.cache.insert(key.to_string(), value.to_string());
                return;
            }
        }
        self.deadlines.write().insert(key.to_string(), std::time::Instant::now() + ttl);
        self.cache.insert(key.to_string(), value.to_string());
    }

    /// Increments a decimal counter, installing `ttl` **only when the key is
    /// created**, and returns the new value.
    ///
    /// Why "only when created" matters: a failure counter whose TTL is refreshed
    /// on every increment is a *sliding* window. For login lockout that turns
    /// into an unbounded lock — an attacker who keeps re-attempting just before
    /// expiry holds the victim's account (or the whole NAT egress IP) locked for
    /// as long as it keeps trying, even though the configured duration is 300 s.
    /// Here the window starts at the first failure and is never extended, so the
    /// lock always lifts on its own.
    ///
    /// The count is stored as a decimal string so it shares the same
    /// namespace/deadline machinery as every other L1 entry.
    pub fn increment_counter(&self, key: &str, ttl: std::time::Duration) -> u64 {
        let now = std::time::Instant::now();
        if let Some(ns_name) = route_key(key) {
            if let Some(ns) = self.namespaces.get(ns_name) {
                return increment_with_fixed_window(&ns.cache, &ns.deadlines, key, ttl, now);
            }
        }
        increment_with_fixed_window(&self.cache, &self.deadlines, key, ttl, now)
    }

    /// Retrieves the raw string value stored under `key`, returning `None` if absent or expired.
    pub fn get_raw(&self, key: &str) -> Option<String> {
        // D-2: 路由到独立命名空间缓存（如有）
        if let Some(ns_name) = route_key(key) {
            if let Some(ns) = self.namespaces.get(ns_name) {
                // D-1: per-key 过期判定
                let deadline = ns.deadlines.read().get(key).copied();
                if let Some(deadline) = deadline {
                    if std::time::Instant::now() >= deadline {
                        ns.cache.remove(key);
                        ns.deadlines.write().remove(key);
                        return None;
                    }
                }
                return ns.cache.get(key);
            }
        }
        // D-1: per-key 过期判定（moka 自身只认 builder 级 TTL）
        let deadline = self.deadlines.read().get(key).copied();
        if let Some(deadline) = deadline {
            if std::time::Instant::now() >= deadline {
                self.cache.remove(key);
                self.deadlines.write().remove(key);
                return None;
            }
        }
        self.cache.get(key)
    }

    /// Removes the value stored under `token` from whichever cache instance is routing-target.
    pub fn remove(&self, token: &str) {
        // D-2: 路由到独立命名空间缓存（如有）
        if let Some(ns_name) = route_key(token) {
            if let Some(ns) = self.namespaces.get(ns_name) {
                ns.deadlines.write().remove(token);
                ns.cache.remove(token);
                return;
            }
        }
        self.deadlines.write().remove(token);
        self.cache.remove(token);
    }
}

/// 驱逐回调的 deadline 清理：**跳过 `Replaced`**。
///
/// `RemovalCause::Replaced` 表示"条目没被移除，只是值被覆盖"——moka 同样会回调。
/// 在此删掉 deadline 会让 per-key TTL 静默失效：后续读回落回 builder 级 TTL
/// （`CacheConfig::time_to_live`，默认 **7200 秒**）。
///
/// 这正是"用户名/IP 永久锁"的真正成因：登录锁定第一轮写入 300 秒的 deadline，
/// 第二次失败覆盖计数时回调把 deadline 删了 ⇒ L1 认为该键永不过期，
/// 于是 300 秒的锁定在进程内实际持续 2 小时（且每次失败都重新开始）。
fn drop_deadline_unless_replaced(
    deadlines: &parking_lot::RwLock<HashMap<String, std::time::Instant>>,
    key: &str,
    cause: &moka::notification::RemovalCause,
) {
    if !matches!(cause, moka::notification::RemovalCause::Replaced) {
        deadlines.write().remove(key);
    }
}

/// Fixed-window counter body shared by the generic cache and every namespace.
///
/// A live deadline means the current window is still open: add one and keep the
/// existing deadline. No (or expired) deadline means a new window starts now.
fn increment_with_fixed_window(
    cache: &Cache<String, String>,
    deadlines: &parking_lot::RwLock<HashMap<String, std::time::Instant>>,
    key: &str,
    ttl: std::time::Duration,
    now: std::time::Instant,
) -> u64 {
    // The read guard must be dropped before taking the write guard below.
    let window_open = deadlines.read().get(key).is_some_and(|deadline| *deadline > now);
    let current: u64 = if window_open { cache.get(key).and_then(|value| value.parse().ok()).unwrap_or(0) } else { 0 };
    let next = current.saturating_add(1);
    if !window_open {
        deadlines.write().insert(key.to_string(), now + ttl);
    }
    cache.insert(key.to_string(), next.to_string());
    next
}
