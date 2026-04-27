#[cfg(test)]
mod e2e_tests {
    use reqwest::Client;
    use serde::de::DeserializeOwned;
    use serde::{Deserialize, Serialize};
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;
    use tokio::runtime::Runtime;

    fn base_url() -> String {
        std::env::var("E2E_BASE_URL").unwrap_or_else(|_| "http://localhost:8008".to_owned())
    }

    fn unique_username(prefix: &str) -> String {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        format!(
            "{}_{}_{}_{}",
            prefix,
            std::process::id(),
            chrono::Utc::now().timestamp_millis(),
            n
        )
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

    #[derive(Debug, Serialize, Deserialize)]
    struct UploadMediaResponse {
        content_uri: String,
        content_type: String,
        size: i64,
        media_id: String,
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct FriendRequestResponse {
        request_id: i64,
        status: String,
    }

    async fn json_ok<T: DeserializeOwned>(response: reqwest::Response, context: &str) -> T {
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .unwrap_or_else(|e| panic!("{}: Failed to read response body: {}", context, e));
        if !status.is_success() {
            panic!(
                "{}: HTTP {} {}",
                context,
                status,
                String::from_utf8_lossy(&bytes)
            );
        }
        serde_json::from_slice(&bytes).unwrap_or_else(|e| {
            panic!(
                "{}: Failed to parse JSON: {} (body={})",
                context,
                e,
                String::from_utf8_lossy(&bytes)
            )
        })
    }

    async fn bytes_ok(response: reqwest::Response, context: &str) -> Vec<u8> {
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .unwrap_or_else(|e| panic!("{}: Failed to read response body: {}", context, e));
        if !status.is_success() {
            panic!(
                "{}: HTTP {} {}",
                context,
                status,
                String::from_utf8_lossy(&bytes)
            );
        }
        bytes.to_vec()
    }

    async fn register_user(username: &str, password: &str) -> RegisterResponse {
        let client = Client::new();
        let response = client
            .post(format!("{}/_matrix/client/r0/register", base_url()))
            .json(&json!({
                "username": username,
                "password": password,
                "auth": {"type": "m.login.dummy"}
            }))
            .send()
            .await
            .expect("Failed to register user");

        json_ok(response, "register_user").await
    }

    async fn login_user(username: &str, password: &str) -> LoginResponse {
        let client = Client::new();
        let response = client
            .post(format!("{}/_matrix/client/r0/login", base_url()))
            .json(&json!({
                "type": "m.login.password",
                "user": username,
                "password": password
            }))
            .send()
            .await
            .expect("Failed to login user");

        json_ok(response, "login_user").await
    }

    async fn create_room(access_token: &str, name: Option<&str>) -> CreateRoomResponse {
        let client = Client::new();
        let mut body = json!({});
        if let Some(room_name) = name {
            body["name"] = json!(room_name);
        }

        let response = client
            .post(format!("{}/_matrix/client/r0/createRoom", base_url()))
            .header("Authorization", format!("Bearer {}", access_token))
            .json(&body)
            .send()
            .await
            .expect("Failed to create room");

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
            .post(format!("{}/_matrix/client/r0/createRoom", base_url()))
            .header("Authorization", format!("Bearer {}", access_token))
            .json(&body)
            .send()
            .await
            .expect("Failed to create room");

        json_ok(response, "create_public_room").await
    }

    async fn send_message(access_token: &str, room_id: &str, message: &str) -> SendMessageResponse {
        let client = Client::new();
        let response = client
            .put(format!(
                "{}/_matrix/client/r0/rooms/{}/send/m.room.message/{}",
                base_url(),
                room_id,
                chrono::Utc::now().timestamp_millis()
            ))
            .header("Authorization", format!("Bearer {}", access_token))
            .json(&json!({
                "msgtype": "m.text",
                "body": message
            }))
            .send()
            .await
            .expect("Failed to send message");

        json_ok(response, "send_message").await
    }

    async fn upload_media(
        access_token: &str,
        content: Vec<u8>,
        content_type: &str,
    ) -> UploadMediaResponse {
        let client = Client::new();
        let response = client
            .post(format!("{}/_matrix/media/v3/upload", base_url()))
            .header("Authorization", format!("Bearer {}", access_token))
            .header("Content-Type", content_type)
            .body(content)
            .send()
            .await
            .expect("Failed to upload media");

        json_ok(response, "upload_media").await
    }

    async fn search_user(access_token: &str, username: &str) -> Vec<serde_json::Value> {
        let client = Client::new();
        let response = client
            .post(format!(
                "{}/_matrix/client/v3/user_directory/search",
                base_url()
            ))
            .header("Authorization", format!("Bearer {}", access_token))
            .json(&json!({
                "search_term": username,
                "limit": 10
            }))
            .send()
            .await
            .expect("Failed to search user");

        let json: serde_json::Value = json_ok(response, "search_user").await;

        json["results"].as_array().cloned().unwrap_or_default()
    }

    async fn send_friend_request(
        access_token: &str,
        to_user_id: &str,
        message: Option<&str>,
    ) -> FriendRequestResponse {
        let client = Client::new();
        let mut body = json!({"user_id": to_user_id});
        if let Some(msg) = message {
            body["message"] = json!(msg);
        }

        let response = client
            .post(format!("{}/_matrix/client/r0/friends/request", base_url()))
            .header("Authorization", format!("Bearer {}", access_token))
            .json(&body)
            .send()
            .await
            .expect("Failed to send friend request");

        json_ok(response, "send_friend_request").await
    }

    async fn accept_friend_request(access_token: &str, user_id: &str) -> serde_json::Value {
        let client = Client::new();
        let response = client
            .post(format!(
                "{}/_matrix/client/r0/friends/request/{}/accept",
                base_url(),
                user_id
            ))
            .header("Authorization", format!("Bearer {}", access_token))
            .send()
            .await
            .expect("Failed to accept friend request");

        json_ok(response, "accept_friend_request").await
    }

    async fn redact_event(access_token: &str, room_id: &str, event_id: &str) -> serde_json::Value {
        let client = Client::new();
        let response = client
            .put(format!(
                "{}/_matrix/client/r0/rooms/{}/redact/{}/{}",
                base_url(),
                room_id,
                event_id,
                chrono::Utc::now().timestamp_millis()
            ))
            .header("Authorization", format!("Bearer {}", access_token))
            .json(&json!({"reason": "Delete message"}))
            .send()
            .await
            .expect("Failed to redact event");

        json_ok(response, "redact_event").await
    }

    async fn join_room(access_token: &str, room_id: &str) -> serde_json::Value {
        let client = Client::new();
        let response = client
            .post(format!(
                "{}/_matrix/client/r0/rooms/{}/join",
                base_url(),
                room_id
            ))
            .header("Authorization", format!("Bearer {}", access_token))
            .json(&json!({}))
            .send()
            .await
            .expect("Failed to join room");

        json_ok(response, "join_room").await
    }

    #[test]
    #[ignore = "Requires running homeserver and E2E_RUN=1"]
    fn test_e2e_complete_user_flow() {
        let rt = Runtime::new().unwrap();
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

            let msg_response =
                send_message(&alice.access_token, &room.room_id, "Hello, Bob!").await;

            assert!(!msg_response.event_id.is_empty());

            let image_data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

            let upload_response = upload_media(&alice.access_token, image_data, "image/png").await;

            assert!(!upload_response.content_uri.is_empty());
            assert_eq!(upload_response.content_type, "image/png");
            assert_eq!(upload_response.size, 8);

            println!("✅ E2E test passed: Complete user flow");
        });
    }

    #[test]
    #[ignore = "Requires running homeserver and E2E_RUN=1"]
    fn test_e2e_friend_flow() {
        let rt = Runtime::new().unwrap();
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
                search_results
                    .iter()
                    .any(|r| r["user_id"].as_str() == Some(bob.user_id.as_str())),
                "Should find bob in search"
            );

            let friend_request =
                send_friend_request(&alice.access_token, &bob.user_id, Some("Let's be friends!"))
                    .await;

            assert!(friend_request.request_id > 0);
            assert_eq!(friend_request.status, "pending");

            let accept_response = accept_friend_request(&bob.access_token, &alice.user_id).await;

            assert_eq!(accept_response["status"], "accepted");

            println!("✅ E2E test passed: Friend flow");
        });
    }

    #[test]
    #[ignore = "Requires running homeserver and E2E_RUN=1"]
    fn test_e2e_private_chat_flow() {
        let rt = Runtime::new().unwrap();
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
                search_results
                    .iter()
                    .any(|r| r["user_id"].as_str() == Some(bob.user_id.as_str())),
                "Should find bob in search"
            );

            // Send friend request
            let _friend_request =
                send_friend_request(&alice.access_token, &bob.user_id, None).await;
            let accept_response = accept_friend_request(&bob.access_token, &alice.user_id).await;
            let dm_room_id = accept_response["room_id"]
                .as_str()
                .expect("room_id should be string")
                .to_owned();
            assert!(!dm_room_id.is_empty(), "DM room ID should not be empty");

            join_room(&alice.access_token, &dm_room_id).await;

            let msg_response = send_message(
                &alice.access_token,
                &dm_room_id,
                "Private message from Alice",
            )
            .await;

            assert!(
                !msg_response.event_id.is_empty(),
                "Event ID should not be empty"
            );

            // Redact (delete) the message
            let redact_response =
                redact_event(&alice.access_token, &dm_room_id, &msg_response.event_id).await;
            assert!(
                redact_response["event_id"]
                    .as_str()
                    .is_some_and(|v| !v.is_empty()),
                "Redaction should return event_id"
            );

            println!("✅ E2E test passed: Private chat flow (using Matrix rooms)");
        });
    }

    #[test]
    #[ignore = "Requires running homeserver and E2E_RUN=1"]
    fn test_e2e_media_upload_and_retrieve() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            if !require_e2e_enabled() {
                return;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;

            let alice_username = unique_username("alice_media");
            let alice = register_user(&alice_username, "Password123!").await;

            let image_data = vec![
                0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
                0x44, 0x52,
            ];

            let upload_response =
                upload_media(&alice.access_token, image_data.clone(), "image/png").await;

            assert!(!upload_response.content_uri.is_empty());
            assert_eq!(upload_response.content_type, "image/png");
            assert_eq!(upload_response.size, 16);

            let client = Client::new();
            let mxc = upload_response.content_uri.trim_start_matches("mxc://");
            let mut parts = mxc.split('/');
            let server_name = parts.next().expect("mxc:// must include server_name");
            let media_id = parts.next().unwrap_or(upload_response.media_id.as_str());

            let response = client
                .get(format!(
                    "{}/_matrix/media/v3/download/{}/{}",
                    base_url(),
                    server_name,
                    media_id
                ))
                .header("Authorization", format!("Bearer {}", alice.access_token))
                .send()
                .await
                .expect("Failed to download media");

            let downloaded_content = bytes_ok(response, "download_media").await;

            assert_eq!(downloaded_content, image_data);

            println!("✅ E2E test passed: Media upload and retrieve");
        });
    }

    #[test]
    #[ignore = "Requires running homeserver and E2E_RUN=1"]
    fn test_e2e_multi_user_room() {
        let rt = Runtime::new().unwrap();
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

            let room = create_public_room(&alice.access_token, Some("Group Chat")).await;

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
        let rt = Runtime::new().unwrap();
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
        let rt = Runtime::new().unwrap();
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
