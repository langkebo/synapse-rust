//! 公共房间目录的 `filter` 行为 —— C-S `POST /_matrix/client/v3/publicRooms`（D-108 / D-109 / C67）
//! 与它的共享解析器（`synapse-web/src/routes/public_rooms_filter.rs`）。
//!
//! 背景：Client-Server API 用 **POST** 的 `filter.generic_search_term` 承载房间目录搜索
//! （"A string to search for in the room metadata, e.g. name, topic, canonical alias etc."，
//! 见 MSC2197 §Motivation），而 C67 之前本仓把这个字段**静默忽略**（`let _filter = ...`）
//! ⇒ 客户端搜索拿到的是**未过滤的第一页**。
//!
//! **federation 侧**（`POST /_matrix/federation/v1/publicRooms`，**D-110** / C83）的用例在
//! `api_federation_tests.rs`：那个端点挂在**受签名保护**的 `protected` 路由组里，用例需要
//! `X-Matrix` 签名，因而复用该文件的 `signed_federation_request` 与联邦 app 构造器。

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;

async fn setup_test_app_with_pool() -> Option<(axum::Router, Arc<sqlx::PgPool>, Arc<synapse_rust::cache::CacheManager>)>
{
    super::setup_fresh_test_app_with_pool().await
}

/// 搜索房间目录必须**真的过滤**，且 `total_room_count_estimate` 必须是**过滤后**的匹配数。
///
/// 四处断言各自对应一类"过滤了但只过滤一半"的坏实现：
/// ① chunk 里不得出现不匹配的房间（谓词没生效）；
/// ② 命中的房间必须在 chunk 里（过滤过狠 / 谓词写错字段）；
/// ③ `total_room_count_estimate` 必须等于**同一谓词**下的匹配总数（拿未过滤的 `count_public_rooms`
///    充数就会红 —— 这是最容易被漏掉的一处，"chunk 过滤了、计数没过滤"）；
/// ④ 搜索路径**不得**发 `next_batch`：搜索谓词 + `ORDER BY name` 与 keyset 游标
///    （`created_ts, room_id`）不同构，发游标会让客户端续传跳进**未过滤**的列表。
#[tokio::test]
async fn test_public_rooms_post_filter_generic_search_term_filters_the_directory() {
    let Some((app, pool, _cache)) = setup_test_app_with_pool().await else {
        super::skip_or_fail_without_db();
        return;
    };

    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let term = format!("zzpub{suffix}");
    let matching = format!("!pub_match_{suffix}:localhost");
    let other = format!("!pub_other_{suffix}:localhost");
    // 命中房间的 name 含 term；反例房间是完全无关的公开房间（若 filter 被忽略，它必然出现在 chunk 里）
    for (room_id, name) in
        [(&matching, format!("{term} matched directory room")), (&other, "unrelated public room".to_string())]
    {
        sqlx::query(
            "INSERT INTO rooms (room_id, creator, is_public, room_version, created_ts, name) \
             VALUES ($1, '@pub:localhost', TRUE, '10', 1700000000000, $2)",
        )
        .bind(room_id)
        .bind(&name)
        .execute(&*pool)
        .await
        .expect("insert fixture room");
    }

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/publicRooms")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"limit": 50, "filter": {"generic_search_term": term}}).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "POST /publicRooms 必须成功");
    let body = axum::body::to_bytes(response.into_body(), 256 * 1024).await.unwrap();
    let payload: Value = serde_json::from_slice(&body).expect("publicRooms 必须返回 JSON");

    let chunk = payload["chunk"].as_array().expect("响应的 chunk 必须是数组");
    let ids: Vec<&str> = chunk.iter().filter_map(|room| room["room_id"].as_str()).collect();

    // ② 命中项必须返回
    assert!(ids.contains(&matching.as_str()), "搜索必须返回命中的房间：{ids:?}");
    // ① 不匹配项不得返回（这一条在 C67 之前必然失败：filter 被忽略，未过滤的第一页里就有它）
    assert!(!ids.contains(&other.as_str()), "搜索不得返回不匹配的房间（filter 未被忽略的证据）：{ids:?}");
    // 逐条复核 chunk：每个房间都必须真的含搜索词（name / topic / canonical_alias 任一面）
    let needle = term.to_lowercase();
    for room in chunk {
        let hit = ["name", "topic", "canonical_alias"]
            .iter()
            .any(|field| room[*field].as_str().is_some_and(|value| value.to_lowercase().contains(&needle)));
        assert!(hit, "chunk 里出现了不匹配搜索词的房间：{room}");
    }

    // ③ 计数必须用同一谓词（这里直接问库要"匹配总数"，与响应比对）
    let expected_total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM rooms r WHERE r.is_public = TRUE \
         AND (LOWER(r.name) LIKE $1 OR LOWER(r.topic) LIKE $1 OR LOWER(r.canonical_alias) LIKE $1)",
    )
    .bind(format!("%{needle}%"))
    .fetch_one(&*pool)
    .await
    .expect("count matching rooms");
    assert_eq!(
        payload["total_room_count_estimate"].as_i64(),
        Some(expected_total),
        "total_room_count_estimate 必须是**过滤后**的匹配总数：{payload}"
    );

    // ④ 搜索路径不得发游标
    assert!(
        payload.get("next_batch").is_none_or(Value::is_null),
        "搜索路径不得返回 next_batch（游标与搜索谓词不同构）：{payload}"
    );
}

/// 发一次 `POST /publicRooms`，返回 `(状态码, JSON)`。`uri` 由调用方给出（C-S 或 federation）。
async fn post_public_rooms_at(app: &axum::Router, uri: &str, body: Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 256 * 1024).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

/// 发一次 C-S 的 `POST /_matrix/client/v3/publicRooms`。
///
/// federation 侧的同名端点（**D-110**）在 **受签名保护** 的 `protected` 路由组里，用例需要
/// `X-Matrix` 签名 + `federation.allow_ingress = true`，因此那些用例在 `api_federation_tests.rs`
/// 里（复用该文件的 `signed_federation_request` 与联邦 app 构造器）—— 本文件只覆盖 C-S 侧。
async fn post_public_rooms(app: &axum::Router, body: Value) -> (StatusCode, Value) {
    post_public_rooms_at(app, "/_matrix/client/v3/publicRooms", body).await
}

/// 插入一个公开房间 + 它的 `room_summaries` 行（`room_type = None` 即"普通房间"）。
async fn insert_directory_room(pool: &sqlx::PgPool, room_id: &str, name: &str, room_type: Option<&str>) {
    sqlx::query(
        "INSERT INTO rooms (room_id, creator, is_public, room_version, created_ts, name) \
         VALUES ($1, '@pub:localhost', TRUE, '10', 1700000000000, $2)",
    )
    .bind(room_id)
    .bind(name)
    .execute(pool)
    .await
    .expect("insert fixture room");
    sqlx::query(
        "INSERT INTO room_summaries (room_id, room_type, is_space, updated_ts, created_ts) \
         VALUES ($1, $2, $3, 1700000000000, 1700000000000)",
    )
    .bind(room_id)
    .bind(room_type)
    .bind(room_type == Some("m.space"))
    .execute(pool)
    .await
    .expect("insert fixture room summary");
}

/// `filter.room_types`（D-109）：`["m.space"]` 只要空间、`[null]` 只要普通房间、两者都给则都要。
/// **列表路径与搜索路径都要应用**（这正是 D-108 那类"只接一半"的错误最容易发生的地方）。
#[tokio::test]
async fn test_public_rooms_post_filter_room_types_selects_normal_and_space_rooms() {
    let Some((app, pool, _cache)) = setup_test_app_with_pool().await else {
        super::skip_or_fail_without_db();
        return;
    };

    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let prefix = format!("zzroomtype{suffix}");
    let normal = format!("!rt_normal_{suffix}:localhost");
    let space = format!("!rt_space_{suffix}:localhost");
    insert_directory_room(&pool, &normal, &format!("{prefix} normal"), None).await;
    insert_directory_room(&pool, &space, &format!("{prefix} space"), Some("m.space")).await;

    // 期望总数与实现无关地由测试自己算（同一谓词的独立表述）
    let count_normal: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM rooms r LEFT JOIN room_summaries rs ON rs.room_id = r.room_id \
         WHERE r.is_public = TRUE AND rs.room_type IS NULL",
    )
    .fetch_one(&*pool)
    .await
    .unwrap();
    let count_space: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM rooms r LEFT JOIN room_summaries rs ON rs.room_id = r.room_id \
         WHERE r.is_public = TRUE AND rs.room_type = ANY(ARRAY['m.space'])",
    )
    .fetch_one(&*pool)
    .await
    .unwrap();

    // 局部类型别名只为避开 clippy::type_complexity（本仓 clippy 以 `-D warnings` 阻断）
    type FilterCase<'a> = (&'a str, Value, Vec<&'a str>, Vec<&'a str>, i64);
    let cases: [FilterCase<'_>; 3] = [
        (
            "只列空间（搜索路径）",
            json!({"limit": 50, "filter": {"room_types": ["m.space"], "generic_search_term": prefix}}),
            vec![space.as_str()],
            vec![normal.as_str()],
            count_space,
        ),
        (
            "只列普通房间（搜索路径）",
            json!({"limit": 50, "filter": {"room_types": [null], "generic_search_term": prefix}}),
            vec![normal.as_str()],
            vec![space.as_str()],
            count_normal,
        ),
        (
            "空间 + 普通房间（列表路径，无搜索词）",
            json!({"limit": 50, "filter": {"room_types": ["m.space", null]}}),
            vec![normal.as_str(), space.as_str()],
            vec![],
            count_normal + count_space,
        ),
    ];

    for (label, body, expected_present, expected_absent, expected_total) in cases {
        let (status, payload) = post_public_rooms(&app, body).await;
        assert_eq!(status, StatusCode::OK, "{label}: POST /publicRooms 必须成功");
        let ids: Vec<&str> = payload["chunk"]
            .as_array()
            .expect("chunk 必须是数组")
            .iter()
            .filter_map(|room| room["room_id"].as_str())
            .collect();
        for id in &expected_present {
            assert!(ids.contains(id), "{label}: 必须包含 {id}：{ids:?}");
        }
        for id in &expected_absent {
            assert!(!ids.contains(id), "{label}: 不得包含 {id}：{ids:?}");
        }
        assert_eq!(
            payload["total_room_count_estimate"].as_i64(),
            Some(expected_total),
            "{label}: total_room_count_estimate 必须是**同一谓词**下的总数：{payload}"
        );
    }
}

/// **显式拒绝**：本仓不支持的 filter 字段与形状非法的 filter 一律 400 `M_INVALID_PARAM`，
/// 不再"当没看见"（D-109 的处置 ②）；层级按规范（**D-111**）：`include_all_networks` /
/// `third_party_instance_id` 属于 `RoomNetwork`，是**请求体顶层**字段，放进 `filter` 里属形状非法。
/// `include_all_networks: false`（顶层，规范默认值）必须接受。
#[tokio::test]
async fn test_public_rooms_post_filter_rejects_unsupported_and_malformed_fields() {
    let Some((app, _pool, _cache)) = setup_test_app_with_pool().await else {
        super::skip_or_fail_without_db();
        return;
    };

    for (label, body) in [
        ("include_all_networks=true（顶层）", json!({"include_all_networks": true})),
        ("third_party_instance_id（顶层）", json!({"third_party_instance_id": "irc"})),
        ("include_all_networks 错放进 filter", json!({"filter": {"include_all_networks": true}})),
        ("third_party_instance_id 错放进 filter", json!({"filter": {"third_party_instance_id": "irc"}})),
        ("filter 不是对象", json!({"filter": "nope"})),
        ("room_types 不是数组", json!({"filter": {"room_types": "m.space"}})),
        ("room_types 条目非法", json!({"filter": {"room_types": [42]}})),
        ("generic_search_term 不是字符串", json!({"filter": {"generic_search_term": 42}})),
    ] {
        let (status, payload) = post_public_rooms(&app, body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{label}: 必须 400");
        assert_eq!(payload["errcode"].as_str(), Some("M_INVALID_PARAM"), "{label}: 必须是 M_INVALID_PARAM：{payload}");
    }

    // `include_all_networks: false` 合法（规范默认值，且在**顶层**）⇒ 200
    let (status, _) = post_public_rooms(&app, json!({"limit": 5, "include_all_networks": false})).await;
    assert_eq!(status, StatusCode::OK, "include_all_networks=false 必须被接受");
}
