use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LivekitConfig {
    pub api_key: String,
    pub api_secret: String,
    pub host: String,
    pub ws_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LivekitRoom {
    pub sid: String,
    pub name: String,
    pub empty_timeout: u32,
    pub max_participants: u32,
    pub creation_time: i64,
    pub turn_password: String,
    pub enabled_codecs: Vec<LivekitCodec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LivekitCodec {
    pub mime_type: String,
    pub fmtp_line: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LivekitParticipant {
    pub sid: String,
    pub identity: String,
    pub state: String,
    pub tracks: Vec<LivekitTrack>,
    pub metadata: Option<String>,
    pub joined_at: i64,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LivekitTrack {
    pub sid: String,
    pub name: String,
    pub kind: String,
    pub source: String,
    pub muted: bool,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRoomRequest {
    pub name: String,
    pub empty_timeout: Option<u32>,
    pub max_participants: Option<u32>,
    pub node_id: Option<String>,
    pub metadata: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRoomResponse {
    pub room: LivekitRoom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinRoomRequest {
    pub room: String,
    pub identity: String,
    pub name: Option<String>,
    pub metadata: Option<String>,
    pub can_publish: Option<bool>,
    pub can_subscribe: Option<bool>,
    pub can_publish_data: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinRoomResponse {
    pub access_token: String,
    pub room: LivekitRoom,
    pub participant: LivekitParticipant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomParticipant {
    pub identity: String,
    pub state: String,
    pub tracks: Vec<TrackInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackInfo {
    pub sid: String,
    pub name: String,
    pub kind: String,
    pub source: String,
    pub muted: bool,
}

#[derive(Clone)]
pub struct LivekitClient {
    config: LivekitConfig,
    http_client: reqwest::Client,
}

impl LivekitClient {
    pub fn new(config: LivekitConfig) -> Self {
        Self {
            config,
            http_client: reqwest::Client::new(),
        }
    }

    pub async fn create_room(
        &self,
        request: CreateRoomRequest,
    ) -> Result<LivekitRoom, LivekitError> {
        let url = format!("{}/twirp/livekit.RoomService/CreateRoom", self.config.host);

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", self.create_auth_header())
            .json(&request)
            .send()
            .await
            .map_err(|e| LivekitError::Network(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(LivekitError::Api(error_text));
        }

        let room: LivekitRoom = response
            .json()
            .await
            .map_err(|e| LivekitError::Parse(e.to_string()))?;

        Ok(room)
    }

    pub async fn delete_room(&self, room_name: &str) -> Result<(), LivekitError> {
        let url = format!("{}/twirp/livekit.RoomService/DeleteRoom", self.config.host);

        let request = serde_json::json!({
            "room": room_name
        });

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", self.create_auth_header())
            .json(&request)
            .send()
            .await
            .map_err(|e| LivekitError::Network(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(LivekitError::Api(error_text));
        }

        Ok(())
    }

    pub async fn list_rooms(&self) -> Result<Vec<LivekitRoom>, LivekitError> {
        let url = format!("{}/twirp/livekit.RoomService/ListRooms", self.config.host);

        let request = serde_json::json!({});

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", self.create_auth_header())
            .json(&request)
            .send()
            .await
            .map_err(|e| LivekitError::Network(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(LivekitError::Api(error_text));
        }

        #[derive(Deserialize)]
        struct ListRoomsResponse {
            rooms: Vec<LivekitRoom>,
        }

        let result: ListRoomsResponse = response
            .json()
            .await
            .map_err(|e| LivekitError::Parse(e.to_string()))?;

        Ok(result.rooms)
    }

    pub async fn list_participants(
        &self,
        room_name: &str,
    ) -> Result<Vec<LivekitParticipant>, LivekitError> {
        let url = format!(
            "{}/twirp/livekit.RoomService/ListParticipants",
            self.config.host
        );

        let request = serde_json::json!({
            "room": room_name
        });

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", self.create_auth_header())
            .json(&request)
            .send()
            .await
            .map_err(|e| LivekitError::Network(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(LivekitError::Api(error_text));
        }

        #[derive(Deserialize)]
        struct ListParticipantsResponse {
            participants: Vec<LivekitParticipant>,
        }

        let result: ListParticipantsResponse = response
            .json()
            .await
            .map_err(|e| LivekitError::Parse(e.to_string()))?;

        Ok(result.participants)
    }

    pub async fn remove_participant(
        &self,
        room_name: &str,
        identity: &str,
    ) -> Result<(), LivekitError> {
        let url = format!(
            "{}/twirp/livekit.RoomService/RemoveParticipant",
            self.config.host
        );

        let request = serde_json::json!({
            "room": room_name,
            "identity": identity
        });

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", self.create_auth_header())
            .json(&request)
            .send()
            .await
            .map_err(|e| LivekitError::Network(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(LivekitError::Api(error_text));
        }

        Ok(())
    }

    pub async fn mute_published_track(
        &self,
        room_name: &str,
        identity: &str,
        track_sid: &str,
        muted: bool,
    ) -> Result<(), LivekitError> {
        let url = format!(
            "{}/twirp/livekit.RoomService/MutePublishedTrack",
            self.config.host
        );

        let request = serde_json::json!({
            "room": room_name,
            "identity": identity,
            "track_sid": track_sid,
            "muted": muted
        });

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", self.create_auth_header())
            .json(&request)
            .send()
            .await
            .map_err(|e| LivekitError::Network(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(LivekitError::Api(error_text));
        }

        Ok(())
    }

    pub fn create_access_token(
        &self,
        room_name: &str,
        identity: &str,
        name: Option<&str>,
        metadata: Option<&str>,
        can_publish: bool,
        can_subscribe: bool,
        can_publish_data: bool,
    ) -> Result<String, LivekitError> {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        use base64::{Engine as _, engine::general_purpose::STANDARD};

        let now = chrono::Utc::now().timestamp();
        let exp = now + 3600;

        let video_grant = serde_json::json!({
            "room": room_name,
            "roomJoin": true,
            "roomCreate": false,
            "roomAdmin": false,
            "roomRecord": false,
            "canPublish": can_publish,
            "canSubscribe": can_subscribe,
            "canPublishData": can_publish_data,
        });

        let claims = serde_json::json!({
            "iss": self.config.api_key,
            "nbf": now - 60,
            "exp": exp,
            "sub": identity,
            "name": name,
            "metadata": metadata,
            "video": video_grant,
        });

        let header = serde_json::json!({
            "alg": "HS256",
            "typ": "JWT"
        });

        let header_b64 = STANDARD.encode(serde_json::to_string(&header).unwrap());
        let claims_b64 = STANDARD.encode(serde_json::to_string(&claims).unwrap());

        let message = format!("{}.{}", header_b64, claims_b64);

        let mut mac = Hmac::<Sha256>::new_from_slice(self.config.api_secret.as_bytes())
            .map_err(|e| LivekitError::Token(e.to_string()))?;
        mac.update(message.as_bytes());
        let signature = mac.finalize();
        let signature_b64 = STANDARD.encode(signature.into_bytes());

        Ok(format!("{}.{}", message, signature_b64))
    }

    fn create_auth_header(&self) -> String {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        use base64::{Engine as _, engine::general_purpose::STANDARD};

        let now = chrono::Utc::now().timestamp();

        let claims = serde_json::json!({
            "iss": self.config.api_key,
            "nbf": now - 60,
            "exp": now + 60,
        });

        let header = serde_json::json!({
            "alg": "HS256",
            "typ": "JWT"
        });

        let header_b64 = STANDARD.encode(serde_json::to_string(&header).unwrap());
        let claims_b64 = STANDARD.encode(serde_json::to_string(&claims).unwrap());

        let message = format!("{}.{}", header_b64, claims_b64);

        let mut mac = Hmac::<Sha256>::new_from_slice(self.config.api_secret.as_bytes())
            .expect("HMAC can take key of any size");
        mac.update(message.as_bytes());
        let signature = mac.finalize();
        let signature_b64 = STANDARD.encode(signature.into_bytes());

        format!("Bearer {}.{}", message, signature_b64)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LivekitError {
    #[error("Network error: {0}")]
    Network(String),
    #[error("API error: {0}")]
    Api(String),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Token error: {0}")]
    Token(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_livekit_config() {
        let config = LivekitConfig {
            api_key: "test_key".to_string(),
            api_secret: "test_secret".to_string(),
            host: "https://livekit.example.com".to_string(),
            ws_url: Some("wss://livekit.example.com".to_string()),
        };

        assert_eq!(config.api_key, "test_key");
        assert!(config.ws_url.is_some());
    }

    #[test]
    fn test_create_room_request() {
        let request = CreateRoomRequest {
            name: "test_room".to_string(),
            empty_timeout: Some(300),
            max_participants: Some(10),
            node_id: None,
            metadata: Some("test metadata".to_string()),
        };

        assert_eq!(request.name, "test_room");
        assert_eq!(request.max_participants, Some(10));
    }

    #[test]
    fn test_join_room_request() {
        let request = JoinRoomRequest {
            room: "room123".to_string(),
            identity: "@alice:example.com".to_string(),
            name: Some("Alice".to_string()),
            metadata: None,
            can_publish: Some(true),
            can_subscribe: Some(true),
            can_publish_data: Some(true),
        };

        assert_eq!(request.room, "room123");
        assert_eq!(request.identity, "@alice:example.com");
    }

    #[test]
    fn test_livekit_room() {
        let room = LivekitRoom {
            sid: "RM_abc123".to_string(),
            name: "test_room".to_string(),
            empty_timeout: 300,
            max_participants: 100,
            creation_time: 1234567890,
            turn_password: "secret".to_string(),
            enabled_codecs: vec![LivekitCodec {
                mime_type: "video/VP8".to_string(),
                fmtp_line: None,
            }],
        };

        assert_eq!(room.sid, "RM_abc123");
        assert_eq!(room.enabled_codecs.len(), 1);
    }

    #[test]
    fn test_livekit_participant() {
        let participant = LivekitParticipant {
            sid: "PA_xyz".to_string(),
            identity: "@bob:example.com".to_string(),
            state: "ACTIVE".to_string(),
            tracks: vec![],
            metadata: Some("user metadata".to_string()),
            joined_at: 1234567890,
            name: Some("Bob".to_string()),
        };

        assert_eq!(participant.identity, "@bob:example.com");
        assert_eq!(participant.state, "ACTIVE");
    }

    #[test]
    fn test_livekit_track() {
        let track = LivekitTrack {
            sid: "TR_video".to_string(),
            name: "camera".to_string(),
            kind: "video".to_string(),
            source: "camera".to_string(),
            muted: false,
            width: Some(1920),
            height: Some(1080),
        };

        assert_eq!(track.kind, "video");
        assert_eq!(track.width, Some(1920));
        assert!(!track.muted);
    }

    #[test]
    fn test_create_access_token() {
        let config = LivekitConfig {
            api_key: "test_key".to_string(),
            api_secret: "test_secret_key_that_is_long_enough".to_string(),
            host: "https://livekit.example.com".to_string(),
            ws_url: None,
        };

        let client = LivekitClient::new(config);

        let token = client.create_access_token(
            "test_room",
            "@alice:example.com",
            Some("Alice"),
            None,
            true,
            true,
            true,
        );

        assert!(token.is_ok());
        let token_str = token.unwrap();
        assert!(token_str.contains('.'));
    }

    #[test]
    fn test_livekit_error() {
        let error = LivekitError::Network("connection failed".to_string());
        assert!(error.to_string().contains("Network"));

        let error = LivekitError::Api("not found".to_string());
        assert!(error.to_string().contains("API"));
    }

    #[test]
    fn test_track_info() {
        let track = TrackInfo {
            sid: "TR_audio".to_string(),
            name: "microphone".to_string(),
            kind: "audio".to_string(),
            source: "microphone".to_string(),
            muted: true,
        };

        assert_eq!(track.kind, "audio");
        assert!(track.muted);
    }
}
