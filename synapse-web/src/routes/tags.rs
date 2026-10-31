use crate::routes::context::RoomContext;
use crate::routes::extractors::MatrixPath;
use axum::{
    extract::State,
    routing::{delete, get, put},
    Json, Router,
};
use serde::Deserialize;

use crate::routes::extractors::UserId;
use crate::routes::response_helpers::empty_json;
use crate::routes::AppState;
use crate::routes::AuthenticatedUser;
use crate::routes::MatrixJson;
use synapse_common::ApiError;

/// The `TagContent` struct.
#[derive(Debug, Deserialize)]
pub struct TagContent {
    /// The `order` field.
    pub order: Option<f64>,
}

/// 房间标签在账户数据里的类型（Matrix 规范：tag 就是房间账号数据 `m.tag`）。
///
/// `/user/{uid}/rooms/{rid}/tags` 这几个端点在规范里**只是 `m.tag` 账户数据的便捷视图**，
/// 不是独立的一份数据。本文件此前把它们写进独立的 `room_tags` 表，于是出现两个后果
/// （2026-10-25 由 SDK 侧实测发现，见 `artifacts/sdk-audit-and-plan-2026-10-20.md` 的 C-6）：
///
///   1. `PUT …/tags/m.favourite` 成功之后，`GET …/account_data/m.tag` 仍回 **404**；
///   2. 更严重：`/sync` 的房间账号数据来自 `room_account_data` 表，所以**标签永远不到客户端** ——
///      客户端（Element 等）读 tag 读的是 sync 里的 `m.tag`，等于标签功能对外完全失效。
const TAGS_ACCOUNT_DATA_TYPE: &str = "m.tag";

/// 读一份标签映射：**优先 `m.tag` 账户数据**，缺了再回退到 `room_tags` 表。
///
/// 回退是为了兼容 2026-10-25 之前只写 `room_tags` 的历史数据（那些行仍然要能读出来）。
async fn read_tag_map(
    ctx: &RoomContext,
    user_id: &str,
    room_id: &str,
) -> Result<serde_json::Map<String, serde_json::Value>, ApiError> {
    if let Some(content) =
        ctx.account_data_service.get_room_account_data(user_id, room_id, TAGS_ACCOUNT_DATA_TYPE).await?
    {
        if let Some(tags) = content.get("tags").and_then(|v| v.as_object()) {
            return Ok(tags.clone());
        }
    }

    let legacy = ctx.room_service.state().get_tags(user_id, room_id).await?;
    Ok(legacy.into_iter().map(|t| (t.tag, serde_json::json!({ "order": t.order.unwrap_or(0.0) }))).collect())
}

/// 把标签映射写回 `m.tag` 账户数据，并**同步维护 `room_tags` 表**。
///
/// 双写不是"两个真相"：读路径以 `m.tag` 为准（`read_tag_map` 先读它），
/// `room_tags` 只是给 `GET /user/{uid}/tags`（跨房间总表）留的索引 ——
/// 那张表按 user 维度查询，比扫全量账户数据便宜。
///
/// 空映射 ⇒ 删掉这条账户数据（与上游行为一致：没有标签时不该留一条 `{"tags":{}}`）。
async fn write_tag_map(
    ctx: &RoomContext,
    user_id: &str,
    room_id: &str,
    tags: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), ApiError> {
    if tags.is_empty() {
        ctx.account_data_service.delete_room_account_data(user_id, room_id, TAGS_ACCOUNT_DATA_TYPE).await?;
        return Ok(());
    }

    let content = serde_json::json!({ "tags": tags });
    ctx.account_data_service.set_room_account_data(user_id, room_id, TAGS_ACCOUNT_DATA_TYPE, &content).await?;
    Ok(())
}

fn create_tags_compat_router() -> Router<AppState> {
    Router::new()
        .route("/user/{user_id}/tags", get(get_global_tags))
        .route("/user/{user_id}/rooms/{room_id}/tags", get(get_tags))
        .route("/user/{user_id}/rooms/{room_id}/tags/{tag}", put(put_tag))
        .route("/user/{user_id}/rooms/{room_id}/tags/{tag}", delete(delete_tag))
}

/// See [`create_tags_router`].
pub fn create_tags_router(state: AppState) -> Router<AppState> {
    let compat_router = create_tags_compat_router();

    Router::new().nest("/_matrix/client/v3", compat_router).with_state(state)
}

async fn get_global_tags(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    MatrixPath(user_id): MatrixPath<UserId>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user_id = user_id.as_str();
    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    // 以 `m.tag` 账户数据为准（与 `read_tag_map` 同一语义）：先按 data_type 跨房扫一遍，
    // 再对「没有 `m.tag` 行」的房间回退 `room_tags` 历史索引。
    //
    // 不能只读 `room_tags`：通过通用 `PUT /user/{uid}/rooms/{rid}/account_data/m.tag`
    // 写入的标签不会经过本文件的双写，只存在于 `room_account_data` 表里。
    let records = ctx.account_data_service.list_room_account_data_by_type(user_id, TAGS_ACCOUNT_DATA_TYPE).await?;

    let mut rooms_map: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
    // 只要该房间有合法的 `m.tag` 内容，就以它为准（即使 `tags` 为空也不回退历史数据）。
    let mut covered_rooms: std::collections::HashSet<String> = std::collections::HashSet::new();

    for record in records {
        let Some(tags) = record.content.get("tags").and_then(|v| v.as_object()) else {
            // 内容缺 `tags` 键 ⇒ 视为不存在，允许下面的 legacy 回退。
            continue;
        };
        covered_rooms.insert(record.room_id.clone());
        if !tags.is_empty() {
            rooms_map.insert(record.room_id, serde_json::Value::Object(tags.clone()));
        }
    }

    let legacy_tags: Vec<synapse_services::room::RoomTag> = ctx.room_service.state().get_all_tags(user_id).await?;
    for tag in legacy_tags {
        if covered_rooms.contains(&tag.room_id) {
            continue;
        }

        let room_tags: &mut serde_json::Value =
            rooms_map.entry(tag.room_id.clone()).or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));

        if let Some(room_tags_map) = room_tags.as_object_mut() {
            room_tags_map.insert(
                tag.tag,
                serde_json::json!({
                    "order": tag.order.unwrap_or(0.0)
                }),
            );
        }
    }

    Ok(Json(serde_json::json!({
        "tags": rooms_map
    })))
}

async fn get_tags(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    MatrixPath((user_id, room_id)): MatrixPath<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    // 以 `m.tag` 账户数据为准（见 `read_tag_map` 的注释）。
    let tags_map = read_tag_map(&ctx, &user_id, &room_id).await?;

    Ok(Json(serde_json::json!({
        "tags": tags_map
    })))
}

async fn put_tag(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    MatrixPath((user_id, room_id, tag)): MatrixPath<(String, String, String)>,
    MatrixJson(content): MatrixJson<TagContent>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    // 1) 写入规范位置（`m.tag` 账户数据）—— 这一步同时让 `/sync` 能把它带给客户端
    let mut tags = read_tag_map(&ctx, &user_id, &room_id).await?;
    tags.insert(tag.clone(), serde_json::json!({ "order": content.order.unwrap_or(0.0) }));
    write_tag_map(&ctx, &user_id, &room_id, &tags).await?;

    // 2) 同步维护 `room_tags`（`GET /user/{uid}/tags` 的跨房索引，见 `write_tag_map`）
    ctx.room_service.state().add_tag(&user_id, &room_id, &tag, content.order).await?;

    Ok(empty_json())
}

async fn delete_tag(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    MatrixPath((user_id, room_id, tag)): MatrixPath<(String, String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if auth_user.user_id != user_id {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let mut tags = read_tag_map(&ctx, &user_id, &room_id).await?;
    tags.remove(&tag);
    write_tag_map(&ctx, &user_id, &room_id, &tags).await?;

    ctx.room_service.state().remove_tag(&user_id, &room_id, &tag).await?;

    Ok(empty_json())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_tags_routes_structure() {
        let compat_routes = [
            "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/tags",
            "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/tags",
            "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/tags/{tag}",
            "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/tags/{tag}",
        ];

        assert!(compat_routes.iter().all(|route| route.starts_with("/_matrix/client/")));
    }

    #[test]
    fn test_tags_compat_router_contains_shared_paths() {
        let shared_paths = ["/user/{user_id}/rooms/{room_id}/tags", "/user/{user_id}/rooms/{room_id}/tags/{tag}"];

        assert_eq!(shared_paths.len(), 2);
        assert!(shared_paths.iter().all(|path| path.starts_with("/user/")));
    }

    #[test]
    fn test_tags_router_keeps_scope_limited_to_v3() {
        let supported_paths = [
            "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/tags",
            "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/tags/{tag}",
        ];
        let unsupported_v1_paths = [
            "/_matrix/client/v1/user/{user_id}/rooms/{room_id}/tags",
            "/_matrix/client/v1/user/{user_id}/rooms/{room_id}/tags/{tag}",
        ];

        assert!(supported_paths.iter().all(|path| !path.starts_with("/_matrix/client/v1/")));
        assert!(unsupported_v1_paths.iter().all(|path| path.starts_with("/_matrix/client/v1/")));
    }
}
