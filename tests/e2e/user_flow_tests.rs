#[cfg(test)]
mod e2e_tests {
    use reqwest::Client;
    use serde::de::DeserializeOwned;
    use serde::{Deserialize, Serialize};
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;
    use synapse_common::current_timestamp_millis;
    use tokio::runtime::Runtime;

    fn base_url() -> String {
        std::env::var("E2E_BASE_URL").unwrap_or_else(|_| "http://localhost:8008".to_owned())
    }

    fn unique_username(prefix: &str) -> String {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("{}_{}_{}_{}", prefix, std::process::id(), current_timestamp_millis(), n)
    }

    fn require_e2e_enabled() -> bool {
        if std::env::var("E2E_RUN").ok().as_deref() == Some("1") {
            return true;
        }

        eprintln!(
            "Skipping ignored E2E test because E2E_RUN is not set to 1. Use E2E_RUN=1 cargo test --test e2e -- --ignored --nocapture"
        );
        false
    }

    fn test_runtime() -> Runtime {
        Runtime::new().unwrap_or_else(|e| panic!("failed to create Tokio runtime for E2E test: {e}"))
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct RegisterResponse {
        user_id: String,
        access_token: String,
        device_id: String,
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct LoginResponse {
        user_id: String,
        access_token: String,
        device_id: String,
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct CreateRoomResponse {
        room_id: String,
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct SendMessageResponse {
        event_id: String,
    }

    /// `POST /_matrix/media/v3/upload` 的响应体。
    ///
    /// 按 Matrix 规范它**只有** `content_uri`（本仓额外返回一个非规范的 `media_id`，
    /// 这里刻意不建模 —— 测试不应依赖非规范字段）。
    /// `content_type` / `size` 也**不在**响应里：这两字段此前被写进本结构体并被
    /// `test_e2e_complete_user_flow` 断言为 `"image/png"` / `8`，所以那两条断言
    /// 从来不可能成立；而因为整个目标没有任何入口执行（见
    /// `.github/workflows/e2e-http.yml`），这个缺陷一直没被触发。
    /// 现在改为「上传后用 `download_media` 取回字节逐字节比对」——
    /// 断言的是行为，而不是回读响应里的自述。
    #[derive(Debug, Serialize, Deserialize)]
    struct UploadMediaResponse {
        content_uri: String,
    }

    /// 好友请求的响应体。
    ///
    /// ⚠️ `request_id` 是**字符串**，不是数字：`synapse-web/src/routes/friend_room.rs`
    /// 里 `request_id_val.to_string()` 之后才放进 `json!`（DB 里是 i64，出网即转字符串，
    /// 避免 JS 客户端的 2^53 精度问题）。这里原写 `i64`，serde 直接报
    /// `invalid type: string "1", expected i64` —— 又一条"从没执行过所以一直没发现"。
    #[derive(Debug, Serialize, Deserialize)]
    struct FriendRequestResponse {
        request_id: String,
        status: String,
    }

    async fn json_ok<T: DeserializeOwned>(response: reqwest::Response, context: &str) -> T {
        let status = response.status();
        let bytes = response.bytes().await.unwrap_or_else(|e| panic!("{context}: Failed to read response body: {e}"));
        if !status.is_success() {
            panic!("{}: HTTP {} {}", context, status, String::from_utf8_lossy(&bytes));
        }
        serde_json::from_slice(&bytes).unwrap_or_else(|e| {
            panic!("{}: Failed to parse JSON: {} (body={})", context, e, String::from_utf8_lossy(&bytes))
        })
    }

    async fn bytes_ok(response: reqwest::Response, context: &str) -> Vec<u8> {
        let status = response.status();
        let bytes = response.bytes().await.unwrap_or_else(|e| panic!("{context}: Failed to read response body: {e}"));
        if !status.is_success() {
            panic!("{}: HTTP {} {}", context, status, String::from_utf8_lossy(&bytes));
        }
        bytes.to_vec()
    }

    async fn register_user(username: &str, password: &str) -> RegisterResponse {
        let client = Client::new();
        let response = client
            .post(format!("{}/_matrix/client/v3/register", base_url()))
            .json(&json!({
                "username": username,
                "password": password,
                "auth": {"type": "m.login.dummy"}
            }))
            .send()
            .await
            .unwrap_or_else(|e| panic!("register_user: failed to register user: {e}"));

        json_ok(response, "register_user").await
    }

    async fn login_user(username: &str, password: &str) -> LoginResponse {
        let client = Client::new();
        let response = client
            .post(format!("{}/_matrix/client/v3/login", base_url()))
            .json(&json!({
                "type": "m.login.password",
                "user": username,
                "password": password
            }))
            .send()
            .await
            .unwrap_or_else(|e| panic!("login_user: failed to login user: {e}"));

        json_ok(response, "login_user").await
    }

    async fn create_room(access_token: &str, name: Option<&str>) -> CreateRoomResponse {
        let client = Client::new();
        let mut body = json!({});
        if let Some(room_name) = name {
            body["name"] = json!(room_name);
        }

        let response = client
            .post(format!("{}/_matrix/client/v3/createRoom", base_url()))
            .header("Authorization", format!("Bearer {access_token}"))
            .json(&body)
            .send()
            .await
            .unwrap_or_else(|e| panic!("create_room: failed to create room: {e}"));

        json_ok(response, "create_room").await
    }

    async fn create_public_room(access_token: &str, name: Option<&str>) -> CreateRoomResponse {
        let client = Client::new();
        let mut body = json!({
            "preset": "public_chat",
            "visibility": "public"
        });
        if let Some(room_name) = name {
            body["name"] = json!(room_name);
        }

        let response = client
            .post(format!("{}/_matrix/client/v3/createRoom", base_url()))
            .header("Authorization", format!("Bearer {access_token}"))
            .json(&body)
            .send()
            .await
            .unwrap_or_else(|e| panic!("create_public_room: failed to create room: {e}"));

        json_ok(response, "create_public_room").await
    }

    async fn send_message(access_token: &str, room_id: &str, message: &str) -> SendMessageResponse {
        let client = Client::new();
        let response = client
            .put(format!(
                "{}/_matrix/client/v3/rooms/{}/send/m.room.message/{}",
                base_url(),
                room_id,
                current_timestamp_millis()
            ))
            .header("Authorization", format!("Bearer {access_token}"))
            .json(&json!({
                "msgtype": "m.text",
                "body": message
            }))
            .send()
            .await
            .unwrap_or_else(|e| panic!("send_message: failed to send message: {e}"));

        json_ok(response, "send_message").await
    }

    async fn upload_media(access_token: &str, content: Vec<u8>, content_type: &str) -> UploadMediaResponse {
        let client = Client::new();
        let response = client
            .post(format!("{}/_matrix/media/v3/upload", base_url()))
            .header("Authorization", format!("Bearer {access_token}"))
            .header("Content-Type", content_type)
            .body(content)
            .send()
            .await
            .unwrap_or_else(|e| panic!("upload_media: failed to upload media: {e}"));

        json_ok(response, "upload_media").await
    }

    /// 按 `mxc://server/media_id` 下载媒体并返回原始字节。
    ///
    /// 上传响应对 `content_type` / `size` 什么都不说（规范里只有 `content_uri`），
    /// 所以「上传成功没有」只能用**取回来是不是同一串字节**来回答。
    async fn download_media(access_token: &str, content_uri: &str) -> Vec<u8> {
        let path = content_uri
            .strip_prefix("mxc://")
            .unwrap_or_else(|| panic!("content_uri 必须以 mxc:// 开头，实际 {content_uri}"));
        let response = Client::new()
            .get(format!("{}/_matrix/media/v3/download/{path}", base_url()))
            .header("Authorization", format!("Bearer {access_token}"))
            .send()
            .await
            .unwrap_or_else(|e| panic!("download_media: failed to download media: {e}"));

        bytes_ok(response, "download_media").await
    }

    async fn search_user(access_token: &str, username: &str) -> Vec<serde_json::Value> {
        let client = Client::new();
        let response = client
            .post(format!("{}/_matrix/client/v3/user_directory/search", base_url()))
            .header("Authorization", format!("Bearer {access_token}"))
            .json(&json!({
                "search_term": username,
                "limit": 10
            }))
            .send()
            .await
            .unwrap_or_else(|e| panic!("search_user: failed to search user: {e}"));

        let json: serde_json::Value = json_ok(response, "search_user").await;

        json["results"].as_array().cloned().unwrap_or_default()
    }

    async fn send_friend_request(access_token: &str, to_user_id: &str, message: Option<&str>) -> FriendRequestResponse {
        let client = Client::new();
        let mut body = json!({"user_id": to_user_id});
        if let Some(msg) = message {
            body["message"] = json!(msg);
        }

        let response = client
            .post(format!("{}/_matrix/vendor/v1/friends/request", base_url()))
            .header("Authorization", format!("Bearer {access_token}"))
            .json(&body)
            .send()
            .await
            .unwrap_or_else(|e| panic!("send_friend_request: failed to send friend request: {e}"));

        json_ok(response, "send_friend_request").await
    }

    async fn accept_friend_request(access_token: &str, user_id: &str) -> serde_json::Value {
        let client = Client::new();
        let response = client
            .post(format!("{}/_matrix/vendor/v1/friends/request/{}/accept", base_url(), user_id))
            .header("Authorization", format!("Bearer {access_token}"))
            .send()
            .await
            .unwrap_or_else(|e| panic!("accept_friend_request: failed to accept friend request: {e}"));

        json_ok(response, "accept_friend_request").await
    }

    async fn redact_event(access_token: &str, room_id: &str, event_id: &str) -> serde_json::Value {
        let client = Client::new();
        let response = client
            .put(format!(
                "{}/_matrix/client/v3/rooms/{}/redact/{}/{}",
                base_url(),
                room_id,
                event_id,
                current_timestamp_millis()
            ))
            .header("Authorization", format!("Bearer {access_token}"))
            .json(&json!({"reason": "Delete message"}))
            .send()
            .await
            .unwrap_or_else(|e| panic!("redact_event: failed to redact event: {e}"));

        json_ok(response, "redact_event").await
    }

    async fn join_room(access_token: &str, room_id: &str) -> serde_json::Value {
        let client = Client::new();
        let response = client
            .post(format!("{}/_matrix/client/v3/rooms/{}/join", base_url(), room_id))
            .header("Authorization", format!("Bearer {access_token}"))
            .json(&json!({}))
            .send()
            .await
            .unwrap_or_else(|e| panic!("join_room: failed to join room: {e}"));

        json_ok(response, "join_room").await
    }

    #[test]
    #[ignore = "Requires running homeserver and E2E_RUN=1"]
    fn test_e2e_complete_user_flow() {
        let rt = test_runtime();
        rt.block_on(async {
            if !require_e2e_enabled() {
                return;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;

            let alice_username = unique_username("alice_e2e");
            let bob_username = unique_username("bob_e2e");
            let alice = register_user(&alice_username, "Password123!").await;
            let bob = register_user(&bob_username, "Password456!").await;

            assert!(!alice.user_id.is_empty());
            assert!(!bob.user_id.is_empty());
            assert!(!alice.access_token.is_empty());
            assert!(!bob.access_token.is_empty());

            let room = create_room(&alice.access_token, Some("Test Room")).await;

            assert!(!room.room_id.is_empty());

            let msg_response = send_message(&alice.access_token, &room.room_id, "Hello, Bob!").await;

            assert!(!msg_response.event_id.is_empty());

            let image_data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

            let upload_response = upload_media(&alice.access_token, image_data.clone(), "image/png").await;

            assert!(!upload_response.content_uri.is_empty());
            // 上传响应对 content_type/size 不作承诺（规范只有 content_uri），
            // 真正的验收是「取回来的是同一串字节」。
            let downloaded = download_media(&alice.access_token, &upload_response.content_uri).await;
            assert_eq!(downloaded, image_data, "上传后下载的字节必须与上传内容一致");

            println!("✅ E2E test passed: Complete user flow");
        });
    }

    #[test]
    #[ignore = "Requires running homeserver and E2E_RUN=1"]
    fn test_e2e_friend_flow() {
        let rt = test_runtime();
        rt.block_on(async {
            if !require_e2e_enabled() {
                return;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;

            let alice_username = unique_username("alice_friend");
            let bob_username = unique_username("bob_friend");
            let alice = register_user(&alice_username, "Password123!").await;
            let bob = register_user(&bob_username, "Password456!").await;

            let search_results = search_user(&alice.access_token, &bob_username).await;

            assert!(
                search_results.iter().any(|r| r["user_id"].as_str() == Some(bob.user_id.as_str())),
                "Should find bob in search"
            );

            let friend_request =
                send_friend_request(&alice.access_token, &bob.user_id, Some("Let's be friends!")).await;

            // `request_id` 在线上是十进制字符串（见 `FriendRequestResponse` 的注释），
            // 断言它可解析为正数，而不是断言类型。
            assert!(
                friend_request.request_id.parse::<i64>().is_ok_and(|id| id > 0),
                "request_id 必须是正整数的十进制字符串，实际 {:?}",
                friend_request.request_id
            );
            assert_eq!(friend_request.status, "pending");

            let accept_response = accept_friend_request(&bob.access_token, &alice.user_id).await;

            assert_eq!(accept_response["status"], "accepted");

            println!("✅ E2E test passed: Friend flow");
        });
    }

    #[test]
    #[ignore = "Requires running homeserver and E2E_RUN=1"]
    fn test_e2e_private_chat_flow() {
        let rt = test_runtime();
        rt.block_on(async {
            if !require_e2e_enabled() {
                return;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;

            let alice_username = unique_username("alice_chat");
            let bob_username = unique_username("bob_chat");
            let alice = register_user(&alice_username, "Password123!").await;
            let bob = register_user(&bob_username, "Password456!").await;

            let search_results = search_user(&alice.access_token, &bob_username).await;
            assert!(
                search_results.iter().any(|r| r["user_id"].as_str() == Some(bob.user_id.as_str())),
                "Should find bob in search"
            );

            // Send friend request
            let _friend_request = send_friend_request(&alice.access_token, &bob.user_id, None).await;
            let accept_response = accept_friend_request(&bob.access_token, &alice.user_id).await;
            let dm_room_id = accept_response["room_id"]
                .as_str()
                .map_or_else(|| panic!("room_id should be string, response={accept_response}"), str::to_owned);
            assert!(!dm_room_id.is_empty(), "DM room ID should not be empty");

            join_room(&alice.access_token, &dm_room_id).await;

            let msg_response = send_message(&alice.access_token, &dm_room_id, "Private message from Alice").await;

            assert!(!msg_response.event_id.is_empty(), "Event ID should not be empty");

            // Redact (delete) the message
            let redact_response = redact_event(&alice.access_token, &dm_room_id, &msg_response.event_id).await;
            assert!(
                redact_response["event_id"].as_str().is_some_and(|v| !v.is_empty()),
                "Redaction should return event_id"
            );

            println!("✅ E2E test passed: Private chat flow (using Matrix rooms)");
        });
    }

    #[test]
    #[ignore = "Requires running homeserver and E2E_RUN=1"]
    fn test_e2e_media_upload_and_retrieve() {
        let rt = test_runtime();
        rt.block_on(async {
            if !require_e2e_enabled() {
                return;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;

            let alice_username = unique_username("alice_media");
            let alice = register_user(&alice_username, "Password123!").await;

            let image_data =
                vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52];

            let upload_response = upload_media(&alice.access_token, image_data.clone(), "image/png").await;

            assert!(!upload_response.content_uri.is_empty());
            // 同上：响应体不承诺 content_type/size，用下载字节证明内容真的存下来了。
            let downloaded_content = download_media(&alice.access_token, &upload_response.content_uri).await;

            assert_eq!(downloaded_content, image_data);

            println!("✅ E2E test passed: Media upload and retrieve");
        });
    }

    #[test]
    #[ignore = "Requires running homeserver and E2E_RUN=1"]
    fn test_e2e_multi_user_room() {
        let rt = test_runtime();
        rt.block_on(async {
            if !require_e2e_enabled() {
                return;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;

            let alice_username = unique_username("alice_multi");
            let bob_username = unique_username("bob_multi");
            let charlie_username = unique_username("charlie_multi");
            let alice = register_user(&alice_username, "Password123!").await;
            let bob = register_user(&bob_username, "Password456!").await;
            let charlie = register_user(&charlie_username, "Password789!").await;

            // 房间名必须唯一：公开房间会据此派生 alias，重名时服务端按规范回
            // `409 M_ROOM_IN_USE`（实测：第二次跑本用例就撞上，因为第一次留下的
            // "Group Chat" 还在库里）。用户已经用 `unique_username` 去重了，
            // 房间名同理 —— 否则这条用例只能在新库上跑一次。
            let room_name = format!("Group Chat {}", unique_username("room"));
            let room = create_public_room(&alice.access_token, Some(&room_name)).await;

            join_room(&bob.access_token, &room.room_id).await;
            join_room(&charlie.access_token, &room.room_id).await;

            let msg1 = send_message(&alice.access_token, &room.room_id, "Hello everyone!").await;
            let msg2 = send_message(&bob.access_token, &room.room_id, "Hi Alice!").await;
            let msg3 = send_message(&charlie.access_token, &room.room_id, "Hello all!").await;

            assert!(!msg1.event_id.is_empty());
            assert!(!msg2.event_id.is_empty());
            assert!(!msg3.event_id.is_empty());

            assert_ne!(msg1.event_id, msg2.event_id);
            assert_ne!(msg2.event_id, msg3.event_id);
            assert_ne!(msg1.event_id, msg3.event_id);

            println!("✅ E2E test passed: Multi-user room");
        });
    }

    #[test]
    #[ignore = "Requires running homeserver and E2E_RUN=1"]
    fn test_e2e_user_login_logout() {
        let rt = test_runtime();
        rt.block_on(async {
            if !require_e2e_enabled() {
                return;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;

            let alice_username = unique_username("alice_auth");
            let alice = register_user(&alice_username, "Password123!").await;

            let login_response = login_user(&alice_username, "Password123!").await;

            assert_eq!(login_response.user_id, alice.user_id);
            assert!(!login_response.access_token.is_empty());

            assert_ne!(login_response.access_token, alice.access_token);

            println!("✅ E2E test passed: User login/logout");
        });
    }

    #[test]
    #[ignore = "Requires running homeserver and E2E_RUN=1"]
    fn test_e2e_multiple_rooms() {
        let rt = test_runtime();
        rt.block_on(async {
            if !require_e2e_enabled() {
                return;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;

            let alice_username = unique_username("alice_rooms");
            let alice = register_user(&alice_username, "Password123!").await;

            let room1 = create_room(&alice.access_token, Some("Room 1")).await;
            let room2 = create_room(&alice.access_token, Some("Room 2")).await;
            let room3 = create_room(&alice.access_token, Some("Room 3")).await;

            assert!(!room1.room_id.is_empty());
            assert!(!room2.room_id.is_empty());
            assert!(!room3.room_id.is_empty());

            assert_ne!(room1.room_id, room2.room_id);
            assert_ne!(room2.room_id, room3.room_id);
            assert_ne!(room1.room_id, room3.room_id);

            send_message(&alice.access_token, &room1.room_id, "Message in Room 1").await;
            send_message(&alice.access_token, &room2.room_id, "Message in Room 2").await;
            send_message(&alice.access_token, &room3.room_id, "Message in Room 3").await;

            println!("✅ E2E test passed: Multiple rooms");
        });
    }
}
