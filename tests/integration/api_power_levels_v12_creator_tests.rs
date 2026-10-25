#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! MSC4289（room v12）**创建侧一致性**：`m.room.power_levels.users` 不得点名创建者。
//!
//! 背景（C-7）：`create_room` 的初始 `m.room.power_levels` 曾无条件写
//! `users: { creator: 100 }`，而这条写入走 `write_creation_event`（逐字落库、**不经过**
//! `verify_power_levels_change`），于是服务端自己产出的事件就违反了它自己执行的
//! MSC4289 rule 10.4。后果是**客户端读-改-写死锁**：
//!
//! 1. `GET /state/m.room.power_levels` 原样返回 content（含创建者）；
//! 2. SDK 的 `StateLevelManager.setPowerLevel()` 拿它当底稿，加一条 `users[peer] = 50`；
//! 3. `PUT /state/m.room.power_levels` 把创建者一起送回 ⇒ 403
//!    `power_levels.users must not name a room creator (MSC4289 rule 10.4)`。
//!
//! `create_room` 只能建 v12+（`LifecycleService::create_room` 拒绝更低版本），所以这不是
//! "个别房间"，而是**每个房间**。修法是创建侧剥离创建者（创建者权力无上限，删掉不损失任何
//! 权限）：与上游 Synapse 的 `_copy_and_remove_creators_from_pl_users_map`、
//! Dendrite #509（合并 `power_level_content_override` **之后**剥离）同一条路子。
//!
//! 三条用例分别钉住三扇门：默认 PL、`power_level_content_override`、以及最要命的
//! "客户端读-改-写"。

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

const STATE_PL: &str = "m.room.power_levels";

fn bearer(builder: axum::http::request::Builder, token: Option<&str>) -> axum::http::request::Builder {
    match token {
        Some(token) => builder.header("Authorization", format!("Bearer {token}")),
        None => builder,
    }
}

fn get(uri: &str, token: Option<&str>) -> Request<Body> {
    bearer(Request::builder().method("GET").uri(uri), token).body(Body::empty()).unwrap()
}

fn post_json(uri: &str, token: Option<&str>, body: &Value) -> Request<Body> {
    let builder = Request::builder().method("POST").uri(uri).header("Content-Type", "application/json");
    bearer(builder, token).body(Body::from(body.to_string())).unwrap()
}

fn put_json(uri: &str, token: &str, body: &Value) -> Request<Body> {
    let builder = Request::builder().method("PUT").uri(uri).header("Content-Type", "application/json");
    bearer(builder, Some(token)).body(Body::from(body.to_string())).unwrap()
}

async fn call(app: &axum::Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(super::with_local_connect_info(request)).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

/// 注册用户，返回 `(access_token, user_id)`。
async fn register(app: &axum::Router, username: &str) -> (String, String) {
    let (status, value) = call(
        app,
        post_json(
            "/_matrix/client/v3/register",
            None,
            &json!({
                "username": username,
                "password": "Password123!",
                "auth": { "type": "m.login.dummy" }
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "注册必须 200：{value}");
    (
        value["access_token"].as_str().expect("access_token").to_string(),
        value["user_id"].as_str().expect("user_id").to_string(),
    )
}

async fn create_room(app: &axum::Router, token: &str, extra: Value) -> String {
    let mut body = json!({ "name": "v12 power levels" });
    if let Some(object) = extra.as_object() {
        for (key, value) in object {
            body[key] = value.clone();
        }
    }
    let (status, value) = call(app, post_json("/_matrix/client/v3/createRoom", Some(token), &body)).await;
    assert_eq!(status, StatusCode::OK, "createRoom 必须 200：{value}");
    value["room_id"].as_str().expect("room_id").to_string()
}

/// 读当前 `m.room.power_levels` 的 content（客户端读-改-写的"读"这一步）。
async fn current_power_levels(app: &axum::Router, token: &str, room_id: &str) -> Value {
    let uri = format!("/_matrix/client/v3/rooms/{room_id}/state/{STATE_PL}");
    let (status, value) = call(app, get(&uri, Some(token))).await;
    assert_eq!(status, StatusCode::OK, "读 power_levels 必须 200：{value}");
    value
}

/// 写 `m.room.power_levels`（读-改-写的"写"这一步），返回状态码与响应体。
async fn put_power_levels(app: &axum::Router, token: &str, room_id: &str, content: &Value) -> (StatusCode, Value) {
    let uri = format!("/_matrix/client/v3/rooms/{room_id}/state/{STATE_PL}");
    call(app, put_json(&uri, token, content)).await
}

/// 建房后初始 `power_levels` **不得**点名创建者，且 v12 的 `m.room.tombstone` 默认是 150。
///
/// 150 不是随便挑的：它必须高于 `state_default`(50) 与管理员级(100)，否则一个普通
/// PL-100 管理员就能 tombstone 房间、在替换房间里**变成创建者**——正是 v12 要堵的提权。
#[tokio::test]
async fn initial_power_levels_do_not_name_the_creator() {
    let Some(ctx) = super::TestContext::new().await else {
        super::skip_or_fail_without_db();
        return;
    };
    let app = &ctx.app;
    let (token, user_id) = register(app, "v12_pl_initial").await;
    let room_id = create_room(app, &token, json!({})).await;

    let content = current_power_levels(app, &token, &room_id).await;
    let names_creator = content["users"].as_object().is_some_and(|users| users.contains_key(&user_id));
    assert!(
        !names_creator,
        "初始 power_levels.users 不得点名创建者（MSC4289 rule 10.4）—— 否则客户端把它读出来再写回去就被 403：{content}"
    );

    assert_eq!(content["state_default"], json!(50), "state_default 应是 50：{content}");
    assert_eq!(
        content["events"]["m.room.tombstone"],
        json!(150),
        "v12 的 m.room.tombstone 默认必须是 150（高于 state_default 与管理员的 100），\
         否则普通管理员可以升级房间并取得新房间的创建者身份：{content}"
    );
}

/// C-7 的原始场景：**读-改-写**。`setPowerLevel(room, peer, 50)` 在自建房间上必须可用。
#[tokio::test]
async fn read_modify_write_power_levels_succeeds_in_a_self_created_room() {
    let Some(ctx) = super::TestContext::new().await else {
        super::skip_or_fail_without_db();
        return;
    };
    let app = &ctx.app;
    let (owner_token, owner_id) = register(app, "v12_pl_owner").await;
    let (peer_token, peer_id) = register(app, "v12_pl_peer").await;
    let room_id = create_room(app, &owner_token, json!({})).await;

    // 对端入房，走真实邀请 + 入房路径。
    let invite_uri = format!("/_matrix/client/v3/rooms/{room_id}/invite");
    let (status, value) = call(app, post_json(&invite_uri, Some(&owner_token), &json!({ "user_id": peer_id }))).await;
    assert_eq!(status, StatusCode::OK, "邀请必须 200：{value}");
    let join_uri = format!("/_matrix/client/v3/rooms/{room_id}/join");
    let (status, value) = call(app, post_json(&join_uri, Some(&peer_token), &json!({}))).await;
    assert_eq!(status, StatusCode::OK, "入房必须 200：{value}");

    // ① 读
    let mut content = current_power_levels(app, &owner_token, &room_id).await;
    // ② 改：把 peer 提到 50（SDK 的 setPowerLevel 就是这么干的）
    content["users"][peer_id.as_str()] = json!(50);
    // ③ 写
    let (status, value) = put_power_levels(app, &owner_token, &room_id, &content).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "客户端读-改-写 power_levels 必须成功；被拒说明创建侧把创建者写进了 users（C-7）：{value}"
    );

    let after = current_power_levels(app, &owner_token, &room_id).await;
    assert_eq!(after["users"][peer_id.as_str()], json!(50), "peer 必须拿到 50：{after}");
    assert!(
        !after["users"].as_object().is_some_and(|users| users.contains_key(&owner_id)),
        "创建者不该出现在 users 里（他本来就无上限，写数字只会让后续写入被拒）：{after}"
    );
}

/// Element Call 场景：`power_level_content_override.users` 里点名创建者时，
/// 服务端应当**剥离**而不是让 createRoom 失败（上游 Dendrite #509 的做法）。
#[tokio::test]
async fn power_level_content_override_naming_the_creator_is_stripped() {
    let Some(ctx) = super::TestContext::new().await else {
        super::skip_or_fail_without_db();
        return;
    };
    let app = &ctx.app;
    let (owner_token, owner_id) = register(app, "v12_pl_override_owner").await;
    let (_, peer_id) = register(app, "v12_pl_override_peer").await;

    let mut users = serde_json::Map::new();
    users.insert(owner_id.clone(), json!(100));
    users.insert(peer_id.clone(), json!(50));

    let room_id = create_room(
        app,
        &owner_token,
        json!({
            "power_level_content_override": {
                "users": users,
                "events": { "m.room.topic": 60 }
            }
        }),
    )
    .await;

    let content = current_power_levels(app, &owner_token, &room_id).await;
    let users = content["users"].as_object().unwrap_or_else(|| panic!("users 必须是对象：{content}"));
    assert!(!users.contains_key(&owner_id), "override 里的创建者条目必须被剥离：{content}");
    assert_eq!(
        users.get(&peer_id),
        Some(&json!(50)),
        "override 里的**非创建者**条目必须保留（不能因为剥离而丢掉整个 users）：{content}"
    );
    assert_eq!(content["events"]["m.room.topic"], json!(60), "override 的其它键（events）必须照旧生效：{content}");
}

/// 第三扇门：`initial_state` 也可以塞一条 `m.room.power_levels`（只禁 create/member/tombstone），
/// 它同样逐字落库、同样会带进创建者 ⇒ 必须走同一条剥离。
#[tokio::test]
async fn initial_state_power_levels_does_not_name_the_creator() {
    let Some(ctx) = super::TestContext::new().await else {
        super::skip_or_fail_without_db();
        return;
    };
    let app = &ctx.app;
    let (owner_token, owner_id) = register(app, "v12_pl_initial_state_owner").await;
    let (_, peer_id) = register(app, "v12_pl_initial_state_peer").await;

    let mut users = serde_json::Map::new();
    users.insert(owner_id.clone(), json!(100));
    users.insert(peer_id.clone(), json!(50));

    let room_id = create_room(
        app,
        &owner_token,
        json!({
            "initial_state": [{
                "type": STATE_PL,
                "state_key": "",
                "content": { "users": users, "state_default": 50 }
            }]
        }),
    )
    .await;

    let content = current_power_levels(app, &owner_token, &room_id).await;
    let users = content["users"].as_object().unwrap_or_else(|| panic!("users 必须是对象：{content}"));
    assert!(!users.contains_key(&owner_id), "initial_state 里的创建者条目必须被剥离：{content}");
    assert_eq!(users.get(&peer_id), Some(&json!(50)), "非创建者条目必须保留：{content}");
    assert_eq!(content["state_default"], json!(50), "initial_state 的其它键必须照旧生效：{content}");
}
