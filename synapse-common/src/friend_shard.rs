//! m.friends.list sharding 路由原语（共享）。
//!
//! 好友列表按 friend_id 的 localpart 首字母拆分到多个 `m.friends.list`
//! state event（shard），以规避 PG btree 单行 2704 字节上限。本模块是
//! storage 层（读路径按 shard 路由）与 services 层（写路径按 shard 路由）
//! 的单一事实来源，避免双方各自实现导致漂移。
//!
//! 拆分规模见 `ALL_SHARDS`：1 个 legacy 通道（`""`）+ 26 字母（A-Z）+ `#`
//! 兜底，共 28 个 shard。

/// 根据 friend_id 计算其所属 shard 的首字母标识（大写）。
///
/// 规则：剥掉 `@localpart:server` 头尾，取 localpart 首字符的 sort_letter。
/// 与好友列表 `sort_by=alphabet` 排序一致，保证同一好友始终落在同一 shard。
pub fn shard_for_user_id(friend_id: &str) -> char {
    let localpart = friend_id.strip_prefix('@').and_then(|s| s.split(':').next()).unwrap_or(friend_id);
    sort_letter_for(localpart).chars().next().unwrap_or('#')
}

/// 将 shard 字符序列化为 `m.friends.list` 的 state_key。
///
/// `#` 保留为字面量；`A`..`Z` 字母大写不变。
pub fn shard_to_state_key(shard: char) -> String {
    shard.to_string()
}

/// 计算某字符串的首字母排序键。
///
/// 跳过前导空白后的首字符：ASCII 字母 → 大写；其余（非 ASCII、数字、
/// 符号、空串）→ `#` 兜底。
pub fn sort_letter_for(value: &str) -> String {
    value.chars().find(|ch| !ch.is_whitespace()).map_or_else(
        || "#".to_string(),
        |ch| {
            if ch.is_ascii_alphabetic() {
                ch.to_ascii_uppercase().to_string()
            } else {
                "#".to_string()
            }
        },
    )
}

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
        assert_eq!(shard_for_user_id("alice"), 'A');
    }

    #[test]
    fn shard_for_user_id_no_colon_suffix() {
        assert_eq!(shard_for_user_id("@bob"), 'B');
    }

    #[test]
    fn shard_for_user_id_empty_string_falls_back_to_hash() {
        assert_eq!(shard_for_user_id(""), '#');
        assert_eq!(shard_for_user_id("@"), '#');
    }

    #[test]
    fn shard_for_user_id_localpart_only_whitespace() {
        assert_eq!(shard_for_user_id("@   :test"), '#');
    }

    #[test]
    fn sort_letter_for_covers_ascii_and_fallbacks() {
        assert_eq!(sort_letter_for("Alice"), "A");
        assert_eq!(sort_letter_for("bob"), "B");
        assert_eq!(sort_letter_for("Zoe"), "Z");
        assert_eq!(sort_letter_for("  Alice"), "A");
        assert_eq!(sort_letter_for("\tBob"), "B");
        assert_eq!(sort_letter_for("123User"), "#");
        assert_eq!(sort_letter_for("@user"), "#");
        assert_eq!(sort_letter_for("_test"), "#");
        assert_eq!(sort_letter_for(""), "#");
        assert_eq!(sort_letter_for("   "), "#");
        assert_eq!(sort_letter_for("中文"), "#");
    }
}
