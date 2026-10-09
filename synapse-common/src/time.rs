//! Timestamp helpers (current ms / UTC, age calculation, pagination/stream token codec).

use chrono::{DateTime, Utc};
use std::sync::atomic::{AtomicI64, Ordering};

/// Currents the timestamp.
pub fn current_timestamp_millis() -> i64 {
    Utc::now().timestamp_millis()
}

/// 上一次由 [`current_timestamp_millis_monotonic`] 发出的毫秒值（进程内）。
static LAST_MONOTONIC_MS: AtomicI64 = AtomicI64::new(0);

/// **严格递增**的毫秒时间戳（进程内）。
///
/// 与 [`current_timestamp_millis`] 的唯一差别：同一毫秒内多次调用**不会**返回同一个值
/// （必要时在前一次的基础上 +1 ms）。
///
/// 为什么需要它（C88）：v12 房间的 `room_id` 由 create 事件的 reference hash 决定
/// （MSC4291），而 `origin_server_ts` 参与该哈希；`build_create_event_content` 对
/// "同 body 的请求"产生**完全相同**的内容 ⇒ 同一毫秒内并发的 createRoom（负载测试的常态）
/// 会派生出**同一个** room_id，撞 `rooms` 主键。用严格递增的时钟给每个 create 事件一个
/// 不同的 `origin_server_ts`，这种派生就天然唯一 —— 不需要给 create 事件塞非标字段，
/// 也不改协议。跨进程（多 worker）不保证唯一：那一层由 `rooms` 插入的
/// `ON CONFLICT (room_id) DO NOTHING` 检出并转成显式的 409（同一批次 C88）。
pub fn current_timestamp_millis_monotonic() -> i64 {
    let now = current_timestamp_millis();
    // `fetch_update` 的闭包恒返回 `Some(..)` ⇒ 返回 Ok(旧值)；新值 = max(now, 旧值 + 1)。
    LAST_MONOTONIC_MS
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |last| Some(if now > last { now } else { last + 1 }))
        .map(|previous| if now > previous { now } else { previous + 1 })
        .unwrap_or(now)
}

/// Currents the timestamp.
pub fn current_timestamp_utc() -> DateTime<Utc> {
    Utc::now()
}

/// Calculates the age.
pub fn calculate_age(timestamp: i64) -> i64 {
    current_timestamp_millis().saturating_sub(timestamp)
}

/// Parses stream token.
pub fn parse_stream_token(token: &str) -> Option<i64> {
    token.strip_prefix('t').and_then(|s| s.parse().ok())
}

/// Generate a composite `/messages` pagination token: `t{ts}` or
/// `t{ts}_{stream_ordering}`.
///
/// The `stream_ordering` suffix disambiguates events that share the same
/// `origin_server_ts` millisecond, so paginating across a page boundary no
/// longer skips same-millisecond events (ISSUE-06). Tokens without the
/// suffix remain valid and keep their legacy strict-timestamp semantics.
pub fn generate_pagination_token(ts: i64, stream_ordering: Option<i64>) -> String {
    match stream_ordering {
        Some(stream) => format!("t{ts}_{stream}"),
        None => format!("t{ts}"),
    }
}

/// Parse a `/messages` pagination token into `(origin_server_ts,
/// Option<stream_ordering>)`. Accepts both the composite `t{ts}_{stream}`
/// form and the legacy `t{ts}` form.
pub fn parse_pagination_token(token: &str) -> Option<(i64, Option<i64>)> {
    let rest = token.strip_prefix('t')?;
    match rest.split_once('_') {
        Some((ts_part, stream_part)) => {
            let ts = ts_part.parse().ok()?;
            let stream = stream_part.parse().ok()?;
            Some((ts, Some(stream)))
        }
        None => rest.parse().ok().map(|ts| (ts, None)),
    }
}

/// Returns true if expired.
pub fn is_expired(expires_at: Option<i64>) -> bool {
    expires_at.is_some_and(|exp| exp < current_timestamp_millis())
}

/// Calculates the ttl.
pub fn calculate_ttl(expires_at: Option<i64>) -> Option<i64> {
    expires_at.map(|exp| {
        let now = current_timestamp_millis();
        (exp - now).max(0)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pagination_token_composite() {
        assert_eq!(parse_pagination_token("t12345_678"), Some((12345, Some(678))));
    }

    #[test]
    fn test_parse_pagination_token_legacy() {
        assert_eq!(parse_pagination_token("t12345"), Some((12345, None)));
        assert_eq!(parse_pagination_token("12345"), None);
        assert_eq!(parse_pagination_token("invalid"), None);
        assert_eq!(parse_pagination_token(""), None);
        assert_eq!(parse_pagination_token("t12345_"), None);
        assert_eq!(parse_pagination_token("t12345_abc"), None);
    }

    #[test]
    fn test_generate_pagination_token() {
        assert_eq!(generate_pagination_token(12345, Some(678)), "t12345_678");
        assert_eq!(generate_pagination_token(12345, None), "t12345");
    }

    #[test]
    fn test_pagination_token_roundtrip() {
        let token = generate_pagination_token(9876543210, Some(42));
        assert_eq!(parse_pagination_token(&token), Some((9876543210, Some(42))));

        let legacy = generate_pagination_token(9876543210, None);
        assert_eq!(parse_pagination_token(&legacy), Some((9876543210, None)));
    }

    #[test]
    fn test_current_timestamp_millis() {
        let ts = current_timestamp_millis();
        assert!(ts > 0);
    }

    #[test]
    fn test_calculate_age() {
        let now = current_timestamp_millis();
        let age = calculate_age(now - 1000);
        assert!(age >= 1000);
    }

    #[test]
    fn test_parse_stream_token() {
        assert_eq!(parse_stream_token("t12345"), Some(12345));
        assert_eq!(parse_stream_token("invalid"), None);
    }

    #[test]
    fn test_is_expired() {
        assert!(is_expired(Some(current_timestamp_millis() - 1000)));
        assert!(!is_expired(Some(current_timestamp_millis() + 10000)));
        assert!(!is_expired(None));
    }

    #[test]
    fn test_calculate_ttl() {
        let ttl = calculate_ttl(Some(current_timestamp_millis() + 5000));
        assert!(ttl.unwrap() > 4000);

        assert!(calculate_ttl(None).is_none());
    }

    #[test]
    fn test_calculate_ttl_expired() {
        let ttl = calculate_ttl(Some(current_timestamp_millis() - 10000));
        assert_eq!(ttl, Some(0));
    }

    #[test]
    fn test_parse_stream_token_edge_cases() {
        assert_eq!(parse_stream_token(""), None);
        assert_eq!(parse_stream_token("t"), None);
        assert_eq!(parse_stream_token("tabc"), None);
        assert_eq!(parse_stream_token("t-123"), Some(-123));
    }

    #[test]
    fn test_current_timestamp_millis_monotonic() {
        let t1 = current_timestamp_millis();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let t2 = current_timestamp_millis();
        assert!(t2 >= t1, "timestamps must be monotonic");
    }

    /// C88：严格递增 —— 同一毫秒内的连续调用必须拿到**不同**的值（房间 id 由含
    /// `origin_server_ts` 的 create 事件哈希派生，同值 ⇒ 同 id ⇒ 主键冲突）。
    #[test]
    fn test_current_timestamp_millis_monotonic_is_strictly_increasing() {
        let mut previous = current_timestamp_millis_monotonic();
        for _ in 0..1000 {
            let next = current_timestamp_millis_monotonic();
            assert!(next > previous, "monotonic clock must strictly increase: {previous} → {next}");
            previous = next;
        }
    }

    /// 不回退：即使墙钟被向后拨（或与上次调用同毫秒），也只会 +1 ms 前进。
    #[test]
    fn test_monotonic_clock_never_goes_backwards() {
        let first = current_timestamp_millis_monotonic();
        let second = current_timestamp_millis_monotonic();
        let third = current_timestamp_millis_monotonic();
        assert!(first < second && second < third, "{first} < {second} < {third}");
        assert!(second - first <= 1, "同一毫秒内的相邻两次只应相差 1ms（实测 {}）", second - first);
    }

    #[test]
    fn test_calculate_age_near_zero() {
        let now = current_timestamp_millis();
        let age = calculate_age(now);
        // The tolerance is the scheduling gap between the two wall-clock reads —
        // `now` here and the `current_timestamp_millis()` inside `calculate_age`
        // — not any logic under test. That gap is unbounded: measured `got 6`
        // under `--test-threads 4` with a 1 ms tolerance, and a pre-empted test
        // thread can make it far larger. 50 ms keeps the assertion meaningful
        // (it would still catch a broken sign or unit conversion) while staying
        // off the scheduler's critical path.
        assert!(age <= 50, "age for now should be near zero, got {age}");
    }

    #[test]
    fn test_is_expired_exactly_now() {
        let now = current_timestamp_millis();
        // `is_expired` uses `<`, so a timestamp equal to now is NOT expired.
        assert!(!is_expired(Some(now)), "exactly-now should not be expired");
    }
}
