//! `POST /_matrix/client/v3/publicRooms` 的 `filter` 行为（D-108 / C67）。
//!
//! 背景：Client-Server API 用 **POST** 的 `filter.generic_search_term` 承载房间目录搜索
//! （"A string to search for in the room metadata, e.g. name, topic, canonical alias etc."，
//! 见 MSC2197 §Motivation），而 C67 之前本仓把这个字段**静默忽略**（`let _filter = ...`）
//! ⇒ 客户端搜索拿到的是**未过滤的第一页**。本文件是那条能力接线的端到端证据。

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
            .any(|field| room[*field].as_str().map(|value| value.to_lowercase().contains(&needle)).unwrap_or(false));
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
