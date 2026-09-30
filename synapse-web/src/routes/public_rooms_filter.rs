//! `/publicRooms` 的 `filter` 解析 —— Client-Server 与 federation 两个调用点**共用一份实现**。
//!
//! 为什么要单独成模块：两个面读的是同一个 `PublicRoomsFilter`，语义必须一致。分开写必然漂移 ——
//! federation 侧正是因为另写了一遍（其实是"干脆没写"）而长期**静默忽略** `filter`，登记为 §7.1 的
//! **D-110**；2026-09-30 裁定 ①：接线而非拒绝，并把解析器收敛到这一份实现。
//!
//! 调用点：
//! * C-S：`directory_reporting::query_public_rooms`（`POST /_matrix/client/v3/publicRooms`，D-108/D-109）；
//! * federation：`federation::events::post_public_rooms`（`POST /_matrix/federation/v1/publicRooms`，D-110）。

use serde_json::Value;
use synapse_common::ApiError;

/// `/publicRooms` 的 `filter` 解析结果（HTTP 层 DTO；storage 的值对象在 `synapse-storage` 里，
/// web 层不需要也不应该依赖它 —— 分层是 route → service → storage）。
#[derive(Debug, Default)]
pub(crate) struct PublicRoomsFilter {
    /// `filter.generic_search_term`（trim 后非空才有值）。
    pub(crate) search_term: Option<String>,
    /// `filter.room_types` 里的**非 null** 类型；`None` = 不做类型过滤。
    pub(crate) room_types: Option<Vec<String>>,
    /// `filter.room_types` 里是否含 `null`（⇒ 也包含普通房间）。
    pub(crate) include_room_type_null: bool,
}

/// 解析 `POST /publicRooms` 的 `filter`（D-108/D-109 接线；D-110 起 federation 侧复用同一份；
/// D-111 修正 `include_all_networks` / `third_party_instance_id` 的**层级**）。
///
/// **三条硬规则**：
/// 1. **接线**：`generic_search_term` 与 `room_types` 都真正生效（列表与搜索两条路径同一套过滤）；
/// 2. **不再静默**：本仓不支持的字段（`include_all_networks = true`、`third_party_instance_id`）
///    以及任何形状非法的 filter（不是对象 / `room_types` 不是数组 / 条目既非字符串也非 null /
///    `generic_search_term` 不是字符串）一律 **400 `M_INVALID_PARAM`**，而不是"当没看见"。
///    （`include_all_networks = false`/缺省是规范默认值，合法，接受。）
/// 3. **层级必须对（D-111）**：`include_all_networks` / `third_party_instance_id` 属于
///    `RoomNetwork`，在规范里是**请求体顶层**字段（ruma：`Request` 上
///    `#[serde(flatten)] pub room_network: RoomNetwork`；C-S `v3` 与 federation `v1` 都一样），
///    **不在 `filter` 里**（`ruma_common::directory::Filter` 只有 `generic_search_term` +
///    `room_types`）。C68 当初把它们当成 `filter.*` 读 ⇒ 规范形状的
///    `{"include_all_networks": true}` 反而被**静默忽略**。现在两个层级都显式判定：
///    顶层按不支持拒绝，`filter` 里出现则按形状非法拒绝（不再"读了但读错地方"）。
pub(crate) fn parse_public_rooms_filter(body: &Value) -> Result<PublicRoomsFilter, ApiError> {
    // ① RoomNetwork（请求体顶层）—— 本仓不支持，显式拒绝而不是静默忽略。
    if body.get("include_all_networks").and_then(Value::as_bool) == Some(true) {
        return Err(ApiError::invalid_input("include_all_networks is not supported"));
    }
    if body.get("third_party_instance_id").is_some_and(|value| !value.is_null()) {
        return Err(ApiError::invalid_input("third_party_instance_id is not supported"));
    }

    let Some(filter) = body.get("filter").filter(|value| !value.is_null()) else {
        return Ok(PublicRoomsFilter::default());
    };
    let Some(filter) = filter.as_object() else {
        return Err(ApiError::invalid_input("filter must be an object"));
    };

    // ② 同一组键出现在 `filter` 里 ⇒ 形状非法（它们不是 `Filter` 的字段，见 D-111）。
    for misplaced in ["include_all_networks", "third_party_instance_id"] {
        if filter.contains_key(misplaced) {
            return Err(ApiError::invalid_input(format!(
                "filter must not contain {misplaced} (it is a top-level request field, not part of filter)"
            )));
        }
    }

    let search_term = match filter.get("generic_search_term") {
        None | Some(Value::Null) => None,
        Some(Value::String(term)) if term.trim().is_empty() => None,
        Some(Value::String(term)) => Some(term.trim().to_string()),
        Some(_) => return Err(ApiError::invalid_input("filter.generic_search_term must be a string")),
    };

    let mut parsed = PublicRoomsFilter { search_term, ..PublicRoomsFilter::default() };
    match filter.get("room_types") {
        None | Some(Value::Null) => {}
        Some(Value::Array(entries)) => {
            let mut names = Vec::new();
            for entry in entries {
                match entry {
                    Value::Null => parsed.include_room_type_null = true,
                    Value::String(name) => names.push(name.clone()),
                    _ => return Err(ApiError::invalid_input("filter.room_types entries must be strings or null")),
                }
            }
            parsed.room_types = Some(names);
        }
        Some(_) => return Err(ApiError::invalid_input("filter.room_types must be an array")),
    }

    Ok(parsed)
}

#[cfg(test)]
mod filter_tests {
    use super::parse_public_rooms_filter;
    use serde_json::json;

    /// D-111：`RoomNetwork` 在**顶层** —— `true` 显式拒绝，`false`/缺省接受。
    #[test]
    fn room_network_is_read_at_the_top_level_not_inside_filter() {
        assert!(parse_public_rooms_filter(&json!({"include_all_networks": true})).is_err());
        assert!(parse_public_rooms_filter(&json!({"third_party_instance_id": "irc"})).is_err());
        assert!(parse_public_rooms_filter(&json!({"include_all_networks": false})).is_ok());
        // 错层级：`filter` 里出现这两个键属形状非法（`Filter` 没有这些字段）
        assert!(parse_public_rooms_filter(&json!({"filter": {"include_all_networks": true}})).is_err());
        assert!(parse_public_rooms_filter(&json!({"filter": {"include_all_networks": false}})).is_err());
        assert!(parse_public_rooms_filter(&json!({"filter": {"third_party_instance_id": null}})).is_err());
    }

    /// 搜索词 trim 后为空视为"没有搜索词"；形状非法（非字符串）报错。
    #[test]
    fn search_term_is_trimmed_and_must_be_a_string() {
        let filter = parse_public_rooms_filter(&json!({"filter": {"generic_search_term": "  hi  "}})).unwrap();
        assert_eq!(filter.search_term.as_deref(), Some("hi"));
        let filter = parse_public_rooms_filter(&json!({"filter": {"generic_search_term": "   "}})).unwrap();
        assert_eq!(filter.search_term, None);
        assert!(parse_public_rooms_filter(&json!({"filter": {"generic_search_term": 42}})).is_err());
        assert!(parse_public_rooms_filter(&json!({"filter": "nope"})).is_err());
    }

    /// `room_types` 含 `null` 表示"也要普通房间"；条目非法报错。
    #[test]
    fn room_types_separates_explicit_types_from_the_null_entry() {
        let filter = parse_public_rooms_filter(&json!({"filter": {"room_types": ["m.space", null]}})).unwrap();
        assert_eq!(filter.room_types.as_deref(), Some(["m.space".to_string()].as_slice()));
        assert!(filter.include_room_type_null);
        let filter = parse_public_rooms_filter(&json!({"filter": {"room_types": []}})).unwrap();
        assert_eq!(filter.room_types.as_deref(), Some([].as_slice()));
        assert!(!filter.include_room_type_null);
        assert!(parse_public_rooms_filter(&json!({"filter": {"room_types": [42]}})).is_err());
        assert!(parse_public_rooms_filter(&json!({"filter": {"room_types": "m.space"}})).is_err());
    }
}
