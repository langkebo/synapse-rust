//! m.friends.list sharding 路由逻辑。
//!
//! 详细方案见 `artifacts/W5-friends-list-sharding-design-2026-08-29.md`。
//!
//! 拆分已在 W5 正式实施：好友列表按 friend_id 的 localpart 首字母拆分到
//! 28 个 `m.friends.list` state event（shard），以规避 PG btree 单行 2704
//! 字节上限（idx_events_sync_covering INCLUDE content）。路由原语的唯一
//! 事实来源在 `synapse_common::friend_shard`，本模块仅做 re-export 并保持
//! 与 storage 层一致的接口，避免双方各自实现导致漂移。

// 路由原语来自 synapse-common（storage 与 services 共享，单一事实来源）。
pub use synapse_common::friend_shard::{shard_for_user_id, shard_to_state_key, sort_letter_for};

/// 列出所有合法 shard 字符（含空字符串兼容老 state_key=""）。
///
/// 返回 28 元素 vec：`["", "A", "B", ..., "Z", "#"]`，
/// 即 1 个 legacy 通道 + A-Z（26）+ `#` 兜底。
pub const ALL_SHARDS: &[&str] = &[
    "", "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S", "T", "U", "V",
    "W", "X", "Y", "Z", "#",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shard_for_user_id_alphabetic_localpart() {
        assert_eq!(shard_for_user_id("@alice:test"), 'A');
        assert_eq!(shard_for_user_id("@bob:test"), 'B');
        assert_eq!(shard_for_user_id("@Zoe:test"), 'Z');
        assert_eq!(shard_for_user_id("@a:test"), 'A');
    }

    #[test]
    fn shard_for_user_id_lowercase_normalized_to_upper() {
        // sort_letter_for 内部 to_ascii_uppercase，保证大小写不敏感
        assert_eq!(shard_for_user_id("@alice:test"), 'A');
        assert_eq!(shard_for_user_id("@ALICE:test"), 'A');
    }

    #[test]
    fn shard_for_user_id_non_ascii_routes_to_hash() {
        assert_eq!(shard_for_user_id("@张三:test"), '#');
        assert_eq!(shard_for_user_id("@中文:test"), '#');
    }

    #[test]
    fn shard_for_user_id_digit_routes_to_hash() {
        assert_eq!(shard_for_user_id("@123user:test"), '#');
        assert_eq!(shard_for_user_id("@9lives:test"), '#');
    }

    #[test]
    fn shard_for_user_id_special_chars_route_to_hash() {
        assert_eq!(shard_for_user_id("@_test:test"), '#');
        assert_eq!(shard_for_user_id("@.dot:test"), '#');
    }

    #[test]
    fn shard_for_user_id_no_at_prefix() {
        // 边界：测试场景可能传裸 localpart
        assert_eq!(shard_for_user_id("alice"), 'A');
    }

    #[test]
    fn shard_for_user_id_no_colon_suffix() {
        // 边界：测试场景可能传无 server 的 friend_id
        assert_eq!(shard_for_user_id("@bob"), 'B');
    }

    #[test]
    fn shard_for_user_id_empty_string_falls_back_to_hash() {
        // 边界：空字符串走 sort_letter_for 的 fallback
        assert_eq!(shard_for_user_id(""), '#');
        assert_eq!(shard_for_user_id("@"), '#');
    }

    #[test]
    fn shard_for_user_id_localpart_only_whitespace() {
        // 边界：localpart 全空白 → sort_letter_for 返 '#'
        assert_eq!(shard_for_user_id("@   :test"), '#');
    }

    #[test]
    fn all_shards_has_28_entries() {
        // 28 = "" (legacy 兼容) + A-Z (26) + "#" (1) = 28
        assert_eq!(ALL_SHARDS.len(), 28);
    }

    #[test]
    fn all_shards_covers_full_alphabet() {
        // 验证 ALL_SHARDS 包含 A-Z 全字母 + "" + "#"
        for letter in 'A'..='Z' {
            let letter_str = letter.to_string();
            assert!(ALL_SHARDS.contains(&letter_str.as_str()), "missing {letter}");
        }
        assert!(ALL_SHARDS.contains(&""));
        assert!(ALL_SHARDS.contains(&"#"));
    }

    /// 均匀性 sanity check：模拟 1000 好友（按字母均匀分布），
    /// 验证 shard 分布大致均匀（最大 shard 不超过平均的 1.5x）。
    /// 实际分布应接近 zipf（少量字母占大头），但这里只看上界。
    #[test]
    fn shard_distribution_is_reasonably_uniform_for_alphabet_input() {
        let mut counts: std::collections::HashMap<char, usize> = std::collections::HashMap::new();
        // 1000 user 按 26 字母轮转 — 每个 user 的 localpart 必须是单个字母开头
        for i in 0..1000 {
            // 直接以字母当 localpart（无前缀），最简单
            let ch = (b'A' + (i % 26) as u8) as char;
            let user_id = format!("@{ch}:test");
            *counts.entry(shard_for_user_id(&user_id)).or_insert(0) += 1;
        }
        let total: usize = counts.values().sum();
        let avg = total / 26;
        let max = *counts.values().max().unwrap();
        // 1000 / 26 = 38.46，max 应该是 39（26 个字母均分）
        assert!(max <= avg + 1, "max {max} > avg+1 {avg} — distribution not uniform");
    }
}
