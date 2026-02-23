#[cfg(test)]
mod e2e_tests {
    use reqwest::Client;
    use serde::{Deserialize, Serialize};
    use serde_json::json;
    use std::time::Duration;
    use tokio::runtime::Runtime;

    const BASE_URL: &str = "http://localhost:8000";

    fn should_run_e2e() -> bool {
        std::env::var("E2E_RUN").ok().as_deref() == Some("1")
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
        request_id: String,
    }

    #[derive(Debug, Serialize, Deserialize)]
    #[allow(dead_code)]
    struct GetFriendsResponse {
        data: Option<FriendsData>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    #[allow(dead_code)]
    struct FriendsData {
        friends: Vec<FriendData>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    #[allow(dead_code)]
    struct FriendData {
        user_id: String,
        dm_room_id: Option<String>,
    }

    async fn register_user(username: &str, password: &str) -> RegisterResponse {
        let client = Client::new();
        let response = client
            .post(format!("{}/_matrix/client/r0/register", BASE_URL))
            .json(&json!({
                "username": username,
                "password": password,
                "auth": {"type": "m.login.dummy"}
            }))
            .send()
            .await
            .expect("Failed to register user");

        response
            .json()
            .await
            .expect("Failed to parse register response")
    }

    async fn login_user(username: &str, password: &str) -> LoginResponse {
        let client = Client::new();
        let response = client
            .post(format!("{}/_matrix/client/r0/login", BASE_URL))
            .json(&json!({
                "type": "m.login.password",
                "user": username,
                "password": password
            }))
            .send()
            .await
            .expect("Failed to login user");

        response
            .json()
            .await
            .expect("Failed to parse login response")
    }

    async fn create_room(access_token: &str, name: Option<&str>) -> CreateRoomResponse {
        let client = Client::new();
        let mut body = json!({});
        if let Some(room_name) = name {
            body["name"] = json!(room_name);
        }

        let response = client
            .post(format!("{}/_matrix/client/r0/createRoom", BASE_URL))
            .header("Authorization", format!("Bearer {}", access_token))
            .json(&body)
            .send()
            .await
            .expect("Failed to create room");

        response
            .json()
            .await
            .expect("Failed to parse create room response")
    }

    async fn send_message(access_token: &str, room_id: &str, message: &str) -> SendMessageResponse {
        let client = Client::new();
        let response = client
            .put(format!(
                "{}/_matrix/client/r0/rooms/{}/send/m.room.message/{}",
                BASE_URL,
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

        response
            .json()
            .await
            .expect("Failed to parse send message response")
    }

    async fn upload_media(
        access_token: &str,
        content: Vec<u8>,
        content_type: &str,
    ) -> UploadMediaResponse {
        let client = Client::new();
        let response = client
            .post(format!("{}/_matrix/media/v3/upload", BASE_URL))
            .header("Authorization", format!("Bearer {}", access_token))
            .header("Content-Type", content_type)
            .body(content)
            .send()
            .await
            .expect("Failed to upload media");

        response
            .json()
            .await
            .expect("Failed to parse upload media response")
    }

    async fn search_user(access_token: &str, username: &str) -> Vec<serde_json::Value> {
        let client = Client::new();
        let response = client
            .get(format!(
                "{}/_matrix/client/r0/user_directory/search/users",
                BASE_URL
            ))
            .header("Authorization", format!("Bearer {}", access_token))
            .query(&[("search_term", username)])
            .send()
            .await
            .expect("Failed to search user");

        let json: serde_json::Value = response
            .json()
            .await
            .expect("Failed to parse search response");

        json["results"].as_array().cloned().unwrap_or_default()
    }

    async fn send_friend_request(
        access_token: &str,
        to_user_id: &str,
        message: Option<&str>,
    ) -> FriendRequestResponse {
        let client = Client::new();
        let mut body = json!({"to_user_id": to_user_id});
        if let Some(msg) = message {
            body["message"] = json!(msg);
        }

        let response = client
            .post(format!("{}/_matrix/client/r0/friends/request", BASE_URL))
            .header("Authorization", format!("Bearer {}", access_token))
            .json(&body)
            .send()
            .await
            .expect("Failed to send friend request");

        response
            .json()
            .await
            .expect("Failed to parse friend request response")
    }

    async fn accept_friend_request(access_token: &str, request_id: &str) -> serde_json::Value {
        let client = Client::new();
        let response = client
            .post(format!(
                "{}/_matrix/client/r0/friends/requests/{}/accept",
                BASE_URL, request_id
            ))
            .header("Authorization", format!("Bearer {}", access_token))
            .send()
            .await
            .expect("Failed to accept friend request");

        response
            .json()
            .await
            .expect("Failed to parse accept friend request response")
    }

    async fn get_dm_room(access_token: &str, other_user_id: &str) -> Option<String> {
        let client = Client::new();
        let response = client
            .get(format!("{}/_matrix/client/v1/friends", BASE_URL))
            .header("Authorization", format!("Bearer {}", access_token))
            .send()
            .await
            .expect("Failed to get friends");

        let friends_response: GetFriendsResponse = response
            .json()
            .await
            .expect("Failed to parse friends response");

        friends_response.data?.friends.iter().find(|f| f.user_id == other_user_id).and_then(|f| f.dm_room_id.clone())
    }

    async fn send_message_to_room(
        access_token: &str,
        room_id: &str,
        message: &str,
    ) -> SendMessageResponse {
        let client = Client::new();
        let response = client
            .post(format!(
                "{}/_matrix/client/r0/rooms/{}/send/m.room.message",
                BASE_URL, room_id
            ))
            .header("Authorization", format!("Bearer {}", access_token))
            .json(&json!({
                "msgtype": "m.text",
                "body": message
            }))
            .send()
            .await
            .expect("Failed to send message");

        response
            .json()
            .await
            .expect("Failed to parse send message response")
    }

    async fn redact_event(
        access_token: &str,
        room_id: &str,
        event_id: &str,
    ) -> serde_json::Value {
        let client = Client::new();
        let response = client
            .put(format!(
                "{}/_matrix/client/r0/rooms/{}/redact/{}",
                BASE_URL, room_id, event_id
            ))
            .header("Authorization", format!("Bearer {}", access_token))
            .json(&json!({"reason": "Delete message"}))
            .send()
            .await
            .expect("Failed to redact event");

        response
            .json()
            .await
            .expect("Failed to parse redact response")
    }

    #[test]
    fn test_e2e_complete_user_flow() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            if !should_run_e2e() {
                return;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;

            let alice = register_user("alice_e2e", "password123").await;
            let bob = register_user("bob_e2e", "password456").await;

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
    fn test_e2e_friend_flow() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            if !should_run_e2e() {
                return;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;

            let alice = register_user("alice_friend", "password123").await;
            let bob = register_user("bob_friend", "password456").await;

            let search_results = search_user(&alice.access_token, "bob_friend").await;

            assert!(!search_results.is_empty(), "Should find bob in search");

            let bob_user_id = &search_results[0]["user_id"]
                .as_str()
                .expect("User ID should be string");

            let friend_request =
                send_friend_request(&alice.access_token, bob_user_id, Some("Let's be friends!"))
                    .await;

            assert!(!friend_request.request_id.is_empty());

            let accept_response =
                accept_friend_request(&bob.access_token, &friend_request.request_id).await;

            assert_eq!(accept_response["status"], "accepted");

            println!("✅ E2E test passed: Friend flow");
        });
    }

    #[test]
    fn test_e2e_private_chat_flow() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            if !should_run_e2e() {
                return;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;

            let alice = register_user("alice_chat", "password123").await;
            let bob = register_user("bob_chat", "password456").await;

            let search_results = search_user(&alice.access_token, "bob_chat").await;
            let bob_user_id = &search_results[0]["user_id"]
                .as_str()
                .expect("User ID should be string");

            // Send friend request
            let friend_request = send_friend_request(&alice.access_token, bob_user_id, None).await;
            accept_friend_request(&bob.access_token, &friend_request.request_id).await;

            // Get the DM room that was automatically created when friend request was accepted
            let dm_room_id = get_dm_room(&alice.access_token, bob_user_id)
                .await
                .expect("DM room should be created after accepting friend request");

            assert!(!dm_room_id.is_empty(), "DM room ID should not be empty");

            // Send a message to the DM room using standard Matrix API
            let msg_response = send_message_to_room(
                &alice.access_token,
                &dm_room_id,
                "Private message from Alice",
            )
            .await;

            assert!(!msg_response.event_id.is_empty(), "Event ID should not be empty");

            // Redact (delete) the message
            let redact_response = redact_event(&alice.access_token, &dm_room_id, &msg_response.event_id).await;
            assert_eq!(redact_response["event_id"], msg_response.event_id, "Redaction should return same event ID");

            println!("✅ E2E test passed: Private chat flow (using Matrix rooms)");
        });
    }

    #[test]
    fn test_e2e_media_upload_and_retrieve() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            if !should_run_e2e() {
                return;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;

            let alice = register_user("alice_media", "password123").await;

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
            let media_filename = upload_response.content_uri.split('/').next_back().unwrap();

            let response = client
                .get(format!(
                    "{}/_matrix/media/v3/download/{}",
                    BASE_URL, media_filename
                ))
                .header("Authorization", format!("Bearer {}", alice.access_token))
                .send()
                .await
                .expect("Failed to download media");

            let downloaded_content = response
                .bytes()
                .await
                .expect("Failed to read media content");

            assert_eq!(downloaded_content.to_vec(), image_data);

            println!("✅ E2E test passed: Media upload and retrieve");
        });
    }

    #[test]
    fn test_e2e_multi_user_room() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            if !should_run_e2e() {
                return;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;

            let alice = register_user("alice_multi", "password123").await;
            let bob = register_user("bob_multi", "password456").await;
            let charlie = register_user("charlie_multi", "password789").await;

            let room = create_room(&alice.access_token, Some("Group Chat")).await;

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
    fn test_e2e_user_login_logout() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            if !should_run_e2e() {
                return;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;

            let alice = register_user("alice_auth", "password123").await;

            let login_response = login_user("alice_auth", "password123").await;

            assert_eq!(login_response.user_id, alice.user_id);
            assert!(!login_response.access_token.is_empty());

            assert_ne!(login_response.access_token, alice.access_token);

            println!("✅ E2E test passed: User login/logout");
        });
    }

    #[test]
    fn test_e2e_multiple_rooms() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            if !should_run_e2e() {
                return;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;

            let alice = register_user("alice_rooms", "password123").await;

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
