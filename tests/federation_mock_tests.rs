mod mock_federation_server;

use hyper::StatusCode;
use serde_json::json;
use tokio::net::TcpListener;

use crate::mock_federation_server::MockFederationServer;

/// 辅助函数：启动 Mock Federation Server
async fn start_mock_server(server_name: &str, port: u16) -> MockFederationServer {
    let mock_server = MockFederationServer::new(server_name, port);
    let router = mock_server.create_router();

    // 在后台启动服务器
    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&addr).await.unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    // 等待服务器启动
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    mock_server
}

/// 测试 1: 服务器发现与密钥交换
#[tokio::test]
async fn test_federation_server_discovery_and_keys() {
    let mock_server = start_mock_server("remote.test", 9001).await;

    // 1. 查询远程服务器版本
    let version_response = reqwest::get("http://127.0.0.1:9001/_matrix/federation/v1/version")
        .await
        .unwrap();

    assert_eq!(version_response.status(), StatusCode::OK);
    let version_json: serde_json::Value = version_response.json().await.unwrap();
    assert_eq!(version_json["server"]["name"], "Mock Synapse");

    // 2. 获取远程服务器密钥
    let keys_response = reqwest::get("http://127.0.0.1:9001/_matrix/key/v2/server")
        .await
        .unwrap();

    assert_eq!(keys_response.status(), StatusCode::OK);
    let keys_json: serde_json::Value = keys_response.json().await.unwrap();
    assert_eq!(keys_json["server_name"], "remote.test");
    assert!(keys_json["verify_keys"].is_object());
    assert!(keys_json["verify_keys"]["ed25519:mock_key"].is_object());

    mock_server.clear();
}

/// 测试 2: 跨服务器房间邀请
#[tokio::test]
async fn test_federation_room_invite() {
    let mock_server = start_mock_server("remote.test", 9002).await;

    let room_id = "!test_room:localhost";
    let event_id = "$invite_event_123";
    let remote_user_id = "@bob:remote.test";

    // 模拟发送邀请到远程服务器
    let invite_event = json!({
        "type": "m.room.member",
        "room_id": room_id,
        "sender": "@alice:localhost",
        "state_key": remote_user_id,
        "content": {
            "membership": "invite"
        },
        "origin": "localhost",
        "origin_server_ts": chrono::Utc::now().timestamp_millis()
    });

    let client = reqwest::Client::new();
    let response = client
        .put(format!(
            "http://127.0.0.1:9002/_matrix/federation/v1/invite/{}/{}",
            urlencoding::encode(room_id),
            urlencoding::encode(event_id)
        ))
        .json(&invite_event)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // 验证 mock server 收到邀请
    assert!(mock_server.received_invite(room_id, remote_user_id));

    mock_server.clear();
}

/// 测试 3: 跨服务器加入房间
#[tokio::test]
async fn test_federation_room_join() {
    let mock_server = start_mock_server("remote.test", 9003).await;

    let room_id = "!test_room:localhost";
    let user_id = "@bob:remote.test";

    // 1. 请求 make_join
    let make_join_response = reqwest::get(format!(
        "http://127.0.0.1:9003/_matrix/federation/v1/make_join/{}/{}",
        urlencoding::encode(room_id),
        urlencoding::encode(user_id)
    ))
    .await
    .unwrap();

    assert_eq!(make_join_response.status(), StatusCode::OK);
    let make_join_json: serde_json::Value = make_join_response.json().await.unwrap();
    assert_eq!(make_join_json["event"]["type"], "m.room.member");
    assert_eq!(make_join_json["event"]["room_id"], room_id);
    assert_eq!(make_join_json["event"]["sender"], user_id);

    // 2. 发送 send_join
    let join_event = make_join_json["event"].clone();
    let event_id = "$join_event_123";

    let client = reqwest::Client::new();
    let send_join_response = client
        .put(format!(
            "http://127.0.0.1:9003/_matrix/federation/v1/send_join/{}/{}",
            urlencoding::encode(room_id),
            urlencoding::encode(event_id)
        ))
        .json(&join_event)
        .send()
        .await
        .unwrap();

    assert_eq!(send_join_response.status(), StatusCode::OK);

    // 验证 mock server 收到加入事件
    assert_eq!(mock_server.received_events_count(), 1);

    mock_server.clear();
}

/// 测试 4: 跨服务器消息同步
#[tokio::test]
async fn test_federation_message_sync() {
    let mock_server = start_mock_server("remote.test", 9004).await;

    let room_id = "!test_room:localhost";
    let txn_id = "txn_123";

    // 模拟发送事务（包含消息事件）
    let transaction = json!({
        "origin": "localhost",
        "origin_server_ts": chrono::Utc::now().timestamp_millis(),
        "pdus": [
            {
                "type": "m.room.message",
                "room_id": room_id,
                "sender": "@alice:localhost",
                "content": {
                    "msgtype": "m.text",
                    "body": "Hello from local server"
                },
                "origin": "localhost",
                "origin_server_ts": chrono::Utc::now().timestamp_millis(),
                "event_id": "$msg_event_123"
            }
        ]
    });

    let client = reqwest::Client::new();
    let response = client
        .put(format!(
            "http://127.0.0.1:9004/_matrix/federation/v1/send/{}",
            txn_id
        ))
        .json(&transaction)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // 验证 mock server 收到消息事件
    let events = mock_server.get_room_events(room_id);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["type"], "m.room.message");
    assert_eq!(events[0]["content"]["body"], "Hello from local server");

    mock_server.clear();
}

/// 测试 5: 房间状态查询
#[tokio::test]
async fn test_federation_state_query() {
    let mock_server = start_mock_server("remote.test", 9005).await;

    let room_id = "!test_room:localhost";

    // 查询房间状态
    let response = reqwest::get(format!(
        "http://127.0.0.1:9005/_matrix/federation/v1/state/{}",
        urlencoding::encode(room_id)
    ))
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let state_json: serde_json::Value = response.json().await.unwrap();
    assert!(state_json["auth_chain"].is_array());
    assert!(state_json["pdus"].is_array());

    mock_server.clear();
}

/// 测试 6: 多个事件的批量同步
#[tokio::test]
async fn test_federation_batch_events() {
    let mock_server = start_mock_server("remote.test", 9006).await;

    let room_id = "!test_room:localhost";
    let txn_id = "txn_batch_123";

    // 发送包含多个事件的事务
    let transaction = json!({
        "origin": "localhost",
        "origin_server_ts": chrono::Utc::now().timestamp_millis(),
        "pdus": [
            {
                "type": "m.room.message",
                "room_id": room_id,
                "sender": "@alice:localhost",
                "content": {"msgtype": "m.text", "body": "Message 1"},
                "event_id": "$msg1"
            },
            {
                "type": "m.room.message",
                "room_id": room_id,
                "sender": "@alice:localhost",
                "content": {"msgtype": "m.text", "body": "Message 2"},
                "event_id": "$msg2"
            },
            {
                "type": "m.room.message",
                "room_id": room_id,
                "sender": "@alice:localhost",
                "content": {"msgtype": "m.text", "body": "Message 3"},
                "event_id": "$msg3"
            }
        ]
    });

    let client = reqwest::Client::new();
    let response = client
        .put(format!(
            "http://127.0.0.1:9006/_matrix/federation/v1/send/{}",
            txn_id
        ))
        .json(&transaction)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // 验证所有事件都被接收
    let events = mock_server.get_room_events(room_id);
    assert_eq!(events.len(), 3);
    assert_eq!(events[0]["content"]["body"], "Message 1");
    assert_eq!(events[1]["content"]["body"], "Message 2");
    assert_eq!(events[2]["content"]["body"], "Message 3");

    mock_server.clear();
}

/// 测试 7: 错误处理 - 不存在的端点
#[tokio::test]
async fn test_federation_nonexistent_endpoint() {
    let _mock_server = start_mock_server("remote.test", 9007).await;

    // 请求不存在的端点
    let response = reqwest::get("http://127.0.0.1:9007/_matrix/federation/v1/nonexistent")
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// 测试 8: 清空功能验证
#[tokio::test]
async fn test_federation_mock_server_clear() {
    let mock_server = start_mock_server("remote.test", 9008).await;

    let room_id = "!test_room:localhost";
    let txn_id = "txn_clear_test";

    // 发送一些事件
    let transaction = json!({
        "origin": "localhost",
        "origin_server_ts": chrono::Utc::now().timestamp_millis(),
        "pdus": [
            {
                "type": "m.room.message",
                "room_id": room_id,
                "sender": "@alice:localhost",
                "content": {"msgtype": "m.text", "body": "Test message"},
                "event_id": "$test_msg"
            }
        ]
    });

    let client = reqwest::Client::new();
    client
        .put(format!(
            "http://127.0.0.1:9008/_matrix/federation/v1/send/{}",
            txn_id
        ))
        .json(&transaction)
        .send()
        .await
        .unwrap();

    assert_eq!(mock_server.received_events_count(), 1);

    // 清空
    mock_server.clear();
    assert_eq!(mock_server.received_events_count(), 0);
}
